use anyhow::Result;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v", "mpeg", "mpg", "3gp",
];

const MIN_BUFFER_FRAMES: usize = 8;
const MAX_BUFFER_FRAMES: usize = 30;
const RENDER_CACHE_SIZE: usize = 16;
const SPEEDS: [f64; 7] = [0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0];
const NORMAL_SPEED: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoControl {
    Play,
    Stop,
    Seek(i32),
    Quieter,
    Louder,
    Mute,
    Slower,
    Faster,
    Fullscreen,
}

#[derive(Clone)]
pub struct RawFrame {
    pub pts: f64,
    pub pixels: Vec<u8>,
}

pub type FrameBuffer = Arc<Mutex<VecDeque<RawFrame>>>;

#[derive(Clone, Copy, Debug)]
struct VideoInfo {
    duration: f64,
    fps: f64,
    has_audio: bool,
}

pub struct VideoPlayer {
    pub path: PathBuf,
    pub is_playing: bool,
    pub is_loading: bool,
    pub current_frame: usize,
    pub fps: f64,
    pub width: u16,
    pub height: u16,
    pub cell_px: (u16, u16),
    pub frame_lines: Vec<String>,
    pub current_raw_frame: Option<Vec<u8>>,
    pub last_tick: Instant,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub error: Option<String>,
    pub title: String,
    pub volume: u8,
    pub muted: bool,
    pub has_audio: bool,
    pub frame_buffer: FrameBuffer,
    pub render_cache: VecDeque<(f64, Vec<String>)>,
    pub buffer_len: usize,
    pub scrub: Option<f64>,
    pub frame_version: u64,

    speed: usize,
    resume_after_buffer: bool,
    sound: Option<Child>,
    prefetch_running: Arc<AtomicBool>,
    prefetch_gen: Arc<AtomicU64>,
}

impl VideoPlayer {
    pub fn new(
        path: PathBuf,
        render_width: u16,
        render_height: u16,
        cell_px: (u16, u16),
    ) -> Result<Self> {
        let title = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("video")
            .to_owned();
        let (info, error) = probe_video(&path);
        let mut player = Self {
            path,
            is_playing: false,
            is_loading: error.is_none(),
            current_frame: 0,
            fps: info.fps,
            width: render_width.max(1),
            height: render_height.max(1),
            cell_px,
            frame_lines: Vec::new(),
            current_raw_frame: None,
            last_tick: Instant::now(),
            position_secs: 0.0,
            duration_secs: info.duration,
            error,
            title,
            volume: 100,
            muted: false,
            has_audio: info.has_audio,
            frame_buffer: Arc::new(Mutex::new(VecDeque::new())),
            render_cache: VecDeque::new(),
            buffer_len: 0,
            scrub: None,
            frame_version: 0,
            speed: NORMAL_SPEED,
            resume_after_buffer: true,
            sound: None,
            prefetch_running: Arc::new(AtomicBool::new(false)),
            prefetch_gen: Arc::new(AtomicU64::new(0)),
        };
        if player.error.is_none() {
            player.start_pipe_reader(0.0);
        }
        Ok(player)
    }

    pub fn toggle_playback(&mut self) {
        if self.is_playing || (self.is_loading && self.resume_after_buffer) {
            self.pause();
        } else {
            self.play();
        }
    }

    pub fn play(&mut self) {
        if self.error.is_some() {
            return;
        }
        if self.ended() {
            self.position_secs = 0.0;
            self.current_frame = 0;
            self.current_raw_frame = None;
        }
        self.resume_after_buffer = true;
        self.is_loading = true;
        self.last_tick = Instant::now();
        if !self.prefetch_running.load(Ordering::SeqCst) {
            self.start_pipe_reader(self.position_secs);
        }
    }

    pub fn pause(&mut self) {
        self.is_playing = false;
        self.is_loading = false;
        self.resume_after_buffer = false;
        self.stop_sound();
    }

    pub fn stop(&mut self) {
        self.prefetch_gen.fetch_add(1, Ordering::SeqCst);
        self.is_playing = false;
        self.is_loading = false;
        self.resume_after_buffer = false;
        self.position_secs = 0.0;
        self.current_frame = 0;
        self.scrub = None;
        self.frame_lines.clear();
        self.current_raw_frame = None;
        self.last_tick = Instant::now();
        self.clear_buffer();
        self.render_cache.clear();
        self.buffer_len = 0;
        self.stop_sound();
        self.frame_version = self.frame_version.wrapping_add(1);
    }

    pub fn reset_to_start(&mut self) {
        self.stop();
        self.is_loading = true;
        self.resume_after_buffer = false;
        self.start_pipe_reader(0.0);
    }

    pub fn needs_tick(&self) -> bool {
        self.is_playing || self.is_loading
    }

    pub fn tick(&mut self) -> Result<bool> {
        self.buffer_len = self.frame_buffer.lock().map(|buf| buf.len()).unwrap_or(0);

        if self.is_loading {
            let needed = if self.resume_after_buffer {
                MIN_BUFFER_FRAMES
            } else {
                1
            };
            let reader_finished = !self.prefetch_running.load(Ordering::SeqCst);
            let ready = self.buffer_len >= needed || (reader_finished && self.buffer_len > 0);
            if ready {
                self.is_loading = false;
                self.advance_frame()?;
                self.is_playing = self.resume_after_buffer;
                self.last_tick = Instant::now();
                if self.is_playing {
                    self.start_sound();
                }
                return Ok(true);
            }
            if reader_finished && self.buffer_len == 0 {
                self.is_loading = false;
                self.resume_after_buffer = false;
                if self.current_raw_frame.is_none() && self.duration_secs == 0.0 {
                    self.error = Some("ffmpeg could not read this video".to_owned());
                }
            }
            return Ok(false);
        }

        if !self.is_playing {
            return Ok(false);
        }

        let elapsed = self.last_tick.elapsed().as_secs_f64();
        let frame_duration = 1.0 / (self.fps.max(1.0) * self.speed());
        if elapsed < frame_duration {
            return Ok(false);
        }

        self.last_tick = Instant::now();
        self.position_secs += elapsed * self.speed();
        self.advance_frame()?;
        self.current_frame += 1;
        if self.duration_secs > 0.0 && self.position_secs >= self.duration_secs {
            self.position_secs = self.duration_secs;
            self.is_playing = false;
            self.resume_after_buffer = false;
            self.stop_sound();
        }
        Ok(true)
    }

    pub fn seek_forward(&mut self, secs: f64) {
        self.seek_to_seconds(self.position_secs + secs);
    }

    pub fn seek_backward(&mut self, secs: f64) {
        self.seek_to_seconds(self.position_secs - secs);
    }

    pub fn seek_to_seconds(&mut self, seconds: f64) {
        self.position_secs = seconds.max(0.0);
        if self.duration_secs > 0.0 {
            self.position_secs = self.position_secs.min(self.duration_secs);
        }
        self.scrub = None;
        self.rebuffer(self.position_secs);
    }

    pub fn scrub_to(&mut self, fraction: f64) {
        self.scrub = Some(fraction.clamp(0.0, 1.0));
    }

    pub fn seek_to(&mut self, fraction: f64) {
        let position = self.duration_secs * fraction.clamp(0.0, 1.0);
        self.seek_to_seconds(position);
    }

    pub fn resize_if_needed(&mut self, new_w: u16, new_h: u16) {
        if new_w == 0 || new_h == 0 {
            return;
        }
        if (self.width as i32 - new_w as i32).abs() > 2
            || (self.height as i32 - new_h as i32).abs() > 2
        {
            self.width = new_w;
            self.height = new_h;
            self.current_raw_frame = None;
            self.frame_lines.clear();
            self.frame_version = self.frame_version.wrapping_add(1);
            self.rebuffer(self.position_secs);
        }
    }

    pub fn speed(&self) -> f64 {
        SPEEDS[self.speed]
    }

    pub fn change_speed(&mut self, faster: bool) {
        self.speed = if faster {
            (self.speed + 1).min(SPEEDS.len() - 1)
        } else {
            self.speed.saturating_sub(1)
        };
        self.rebuffer(self.position_secs);
    }

    pub fn change_volume(&mut self, louder: bool) {
        self.volume = if louder {
            self.volume.saturating_add(10).min(100)
        } else {
            self.volume.saturating_sub(10)
        };
        self.muted = false;
        self.start_sound();
    }

    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        self.start_sound();
    }

    pub fn status(&self) -> String {
        let volume = if self.muted || !self.has_audio {
            "Muted".to_owned()
        } else {
            format!("Vol {}%", self.volume)
        };
        format!("{volume}  {}x", self.speed())
    }

    pub fn display_position(&self) -> f64 {
        self.scrub
            .map(|fraction| self.duration_secs * fraction.clamp(0.0, 1.0))
            .unwrap_or(self.position_secs)
    }

    pub fn progress_ratio(&self) -> f32 {
        if self.duration_secs <= 0.0 {
            return 0.0;
        }
        ((self.display_position() / self.duration_secs) as f32).clamp(0.0, 1.0)
    }

    pub fn buffer_fill_pct(&self) -> u8 {
        ((self.buffer_len * 100) / MIN_BUFFER_FRAMES).min(100) as u8
    }

    pub fn ended(&self) -> bool {
        self.duration_secs > 0.0
            && self.position_secs >= self.duration_secs
            && !self.is_playing
            && !self.is_loading
    }

    pub fn buttons(&self) -> Vec<(VideoControl, &'static str)> {
        let play = if self.ended() {
            "Replay"
        } else if self.is_playing || (self.is_loading && self.resume_after_buffer) {
            "Pause"
        } else {
            "Play"
        };
        vec![
            (VideoControl::Play, play),
            (VideoControl::Stop, "Stop"),
            (VideoControl::Seek(-5), "-5s"),
            (VideoControl::Seek(5), "+5s"),
            (VideoControl::Quieter, "Vol-"),
            (VideoControl::Louder, "Vol+"),
            (
                VideoControl::Mute,
                if self.muted { "Unmute" } else { "Mute" },
            ),
            (VideoControl::Slower, "Slower"),
            (VideoControl::Faster, "Faster"),
            (VideoControl::Fullscreen, "Full screen"),
        ]
    }

    pub fn button_at(&self, column: u16) -> Option<VideoControl> {
        let mut left = 1;
        for (control, label) in self.buttons() {
            let right = left + label.chars().count() as u16 + 2;
            if (left..right).contains(&column) {
                return Some(control);
            }
            left = right + 1;
        }
        None
    }

    pub fn progress_layout(&self, width: u16) -> (u16, u16) {
        let label = format_time(self.duration_secs).chars().count() as u16 + 2;
        (label, width.saturating_sub(label * 2))
    }

    pub fn fraction_at(&self, width: u16, column: u16) -> Option<f64> {
        let (start, length) = self.progress_layout(width);
        (length > 0 && (start..start + length).contains(&column))
            .then(|| f64::from(column - start) / f64::from(length.saturating_sub(1).max(1)))
    }

    fn rebuffer(&mut self, from_pts: f64) {
        let resume = self.is_playing || (self.is_loading && self.resume_after_buffer);
        self.last_tick = Instant::now();
        self.clear_buffer();
        self.is_playing = false;
        self.is_loading = true;
        self.resume_after_buffer = resume;
        self.stop_sound();
        self.start_pipe_reader(from_pts);
    }

    fn clear_buffer(&mut self) {
        if let Ok(mut buf) = self.frame_buffer.lock() {
            buf.clear();
        }
    }

    fn advance_frame(&mut self) -> Result<()> {
        let pts = self.position_secs;
        let is_halfblock = self.cell_px == (1, 2);

        if is_halfblock {
            for (cached_pts, cached_lines) in &self.render_cache {
                if (cached_pts - pts).abs() < 0.5 / self.fps.max(1.0) {
                    self.frame_lines = cached_lines.clone();
                    self.error = None;
                    return Ok(());
                }
            }
        }

        let raw_frame = self
            .frame_buffer
            .lock()
            .ok()
            .and_then(|mut buf| buf.pop_front());
        if let Some(frame) = raw_frame {
            self.current_raw_frame = Some(frame.pixels.clone());
            if is_halfblock {
                let lines = render_frame_to_lines(&frame.pixels, self.width, self.height * 2);
                self.render_cache.push_back((pts, lines.clone()));
                while self.render_cache.len() > RENDER_CACHE_SIZE {
                    self.render_cache.pop_front();
                }
                self.frame_lines = lines;
            }
            self.error = None;
            self.frame_version = self.frame_version.wrapping_add(1);
            return Ok(());
        }

        if !self.prefetch_running.load(Ordering::SeqCst)
            && self.duration_secs > 0.0
            && self.position_secs + 1.0 / self.fps.max(1.0) >= self.duration_secs
        {
            self.position_secs = self.duration_secs;
            self.is_playing = false;
            self.is_loading = false;
            self.resume_after_buffer = false;
            self.stop_sound();
            return Ok(());
        }

        self.is_playing = false;
        self.is_loading = true;
        self.resume_after_buffer = true;
        self.stop_sound();
        if !self.prefetch_running.load(Ordering::SeqCst) {
            self.start_pipe_reader(self.position_secs);
        }
        Ok(())
    }

    fn start_sound(&mut self) {
        self.stop_sound();
        if !self.is_playing || !self.has_audio || self.muted || self.volume == 0 || self.ended() {
            return;
        }
        let mut args = vec![
            "-nodisp".to_owned(),
            "-vn".to_owned(),
            "-sn".to_owned(),
            "-autoexit".to_owned(),
            "-loglevel".to_owned(),
            "quiet".to_owned(),
            "-ss".to_owned(),
            format!("{:.3}", self.position_secs),
            "-volume".to_owned(),
            self.volume.to_string(),
        ];
        if self.speed != NORMAL_SPEED {
            args.extend(["-af".to_owned(), format!("atempo={}", self.speed())]);
        }
        args.push(self.path.to_string_lossy().into_owned());
        self.sound = Command::new("ffplay")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok();
    }

    fn stop_sound(&mut self) {
        if let Some(mut child) = self.sound.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn start_pipe_reader(&mut self, from_pts: f64) {
        let pipe_gen = self.prefetch_gen.fetch_add(1, Ordering::SeqCst) + 1;
        self.prefetch_running.store(true, Ordering::SeqCst);

        let path = self.path.clone();
        let width = self.width.max(1);
        let height = self.height.max(1);
        let cell_px = self.cell_px;
        let fps = self.fps;
        let frame_buffer = Arc::clone(&self.frame_buffer);
        let running_flag = Arc::clone(&self.prefetch_running);
        let gen_counter = Arc::clone(&self.prefetch_gen);

        std::thread::spawn(move || {
            let decode_w = (width as u32 * cell_px.0 as u32).max(1);
            let decode_h = (height as u32 * cell_px.1 as u32).max(1);
            let Some(frame_size) = (decode_w as usize)
                .checked_mul(decode_h as usize)
                .and_then(|size| size.checked_mul(3))
            else {
                if gen_counter.load(Ordering::SeqCst) == pipe_gen {
                    running_flag.store(false, Ordering::SeqCst);
                }
                return;
            };
            let frame_duration = 1.0 / fps.max(1.0);
            let filter = format!(
                "scale={decode_w}:{decode_h}:force_original_aspect_ratio=decrease,pad={decode_w}:{decode_h}:(ow-iw)/2:(oh-ih)/2:black"
            );

            let mut child = match Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-nostdin",
                    "-ss",
                    &format!("{from_pts:.3}"),
                    "-i",
                    &path.to_string_lossy(),
                    "-an",
                    "-vf",
                    &filter,
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "rgb24",
                    "pipe:1",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(child) => child,
                Err(_) => {
                    if gen_counter.load(Ordering::SeqCst) == pipe_gen {
                        running_flag.store(false, Ordering::SeqCst);
                    }
                    return;
                }
            };

            let mut stdout = match child.stdout.take() {
                Some(stdout) => stdout,
                None => {
                    if gen_counter.load(Ordering::SeqCst) == pipe_gen {
                        running_flag.store(false, Ordering::SeqCst);
                    }
                    return;
                }
            };
            let mut frame_idx = 0usize;
            let mut raw = vec![0u8; frame_size];
            loop {
                if gen_counter.load(Ordering::SeqCst) != pipe_gen {
                    let _ = child.kill();
                    break;
                }
                if frame_buffer
                    .lock()
                    .map(|buf| buf.len())
                    .unwrap_or(MAX_BUFFER_FRAMES)
                    >= MAX_BUFFER_FRAMES
                {
                    std::thread::sleep(std::time::Duration::from_millis(8));
                    continue;
                }

                use std::io::Read;
                match stdout.read_exact(&mut raw) {
                    Ok(()) => {
                        if gen_counter.load(Ordering::SeqCst) != pipe_gen {
                            let _ = child.kill();
                            break;
                        }
                        let pts = from_pts + frame_idx as f64 * frame_duration;
                        if let Ok(mut buffer) = frame_buffer.lock() {
                            buffer.push_back(RawFrame {
                                pts,
                                pixels: raw.clone(),
                            });
                        }
                        frame_idx += 1;
                    }
                    Err(_) => break,
                }
            }
            if gen_counter.load(Ordering::SeqCst) == pipe_gen {
                running_flag.store(false, Ordering::SeqCst);
            }
        });
    }
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        self.prefetch_gen.fetch_add(1, Ordering::SeqCst);
        self.stop_sound();
    }
}

pub fn render_frame_to_lines(rgb_data: &[u8], width: u16, height_px: u16) -> Vec<String> {
    let w = width as usize;
    let h = height_px as usize;
    let mut lines = Vec::with_capacity(h / 2);
    for row in 0..(h / 2) {
        let mut line = String::new();
        for col in 0..w {
            let upper_idx = (row * 2 * w + col) * 3;
            let lower_idx = ((row * 2 + 1) * w + col) * 3;
            if upper_idx + 2 < rgb_data.len() && lower_idx + 2 < rgb_data.len() {
                let ur = rgb_data[upper_idx];
                let ug = rgb_data[upper_idx + 1];
                let ub = rgb_data[upper_idx + 2];
                let lr = rgb_data[lower_idx];
                let lg = rgb_data[lower_idx + 1];
                let lb = rgb_data[lower_idx + 2];
                line.push_str(&format!(
                    "\x1b[38;2;{ur};{ug};{ub}m\x1b[48;2;{lr};{lg};{lb}m▀\x1b[0m"
                ));
            }
        }
        lines.push(line);
    }
    lines
}

fn probe_video(path: &Path) -> (VideoInfo, Option<String>) {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,r_frame_rate,avg_frame_rate:format=duration",
            "-of",
            "json",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    match output {
        Ok(output) if output.status.success() => {
            match parse_probe(&String::from_utf8_lossy(&output.stdout)) {
                Some(info) => (info, None),
                None => (
                    VideoInfo {
                        duration: 0.0,
                        fps: 24.0,
                        has_audio: false,
                    },
                    Some("ffprobe could not read this video".to_owned()),
                ),
            }
        }
        Ok(_) => (
            VideoInfo {
                duration: 0.0,
                fps: 24.0,
                has_audio: false,
            },
            Some("ffprobe could not read this video".to_owned()),
        ),
        Err(_) => (
            VideoInfo {
                duration: 0.0,
                fps: 24.0,
                has_audio: false,
            },
            Some("ffprobe was not found; install ffmpeg".to_owned()),
        ),
    }
}

fn parse_probe(json: &str) -> Option<VideoInfo> {
    let value = serde_json::from_str::<serde_json::Value>(json).ok()?;
    let streams = value["streams"].as_array()?;
    let video = streams
        .iter()
        .find(|stream| stream["codec_type"] == "video")?;
    let fps = ["avg_frame_rate", "r_frame_rate"]
        .iter()
        .find_map(|key| parse_rate(video[*key].as_str()?))
        .unwrap_or(24.0);
    let duration = value["format"]["duration"]
        .as_str()
        .and_then(|duration| duration.parse::<f64>().ok())
        .filter(|duration| duration.is_finite() && *duration >= 0.0)
        .unwrap_or(0.0);
    Some(VideoInfo {
        duration,
        fps: if fps.is_finite() && fps > 0.0 {
            fps
        } else {
            24.0
        },
        has_audio: streams.iter().any(|stream| stream["codec_type"] == "audio"),
    })
}

fn parse_rate(rate: &str) -> Option<f64> {
    if let Some((numerator, denominator)) = rate.split_once('/') {
        let numerator = numerator.parse::<f64>().ok()?;
        let denominator = denominator.parse::<f64>().ok()?;
        (denominator != 0.0).then_some(numerator / denominator)
    } else {
        rate.parse().ok()
    }
}

pub fn format_time(seconds: f64) -> String {
    let seconds = seconds.max(0.0).round() as u64;
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> VideoPlayer {
        VideoPlayer {
            path: PathBuf::from("clip.mp4"),
            is_playing: false,
            is_loading: false,
            current_frame: 0,
            fps: 25.0,
            width: 80,
            height: 40,
            cell_px: (1, 2),
            frame_lines: Vec::new(),
            current_raw_frame: None,
            last_tick: Instant::now(),
            position_secs: 0.0,
            duration_secs: 75.5,
            error: None,
            title: "clip".to_owned(),
            volume: 100,
            muted: false,
            has_audio: true,
            frame_buffer: Arc::new(Mutex::new(VecDeque::new())),
            render_cache: VecDeque::new(),
            buffer_len: 0,
            scrub: None,
            frame_version: 0,
            speed: NORMAL_SPEED,
            resume_after_buffer: false,
            sound: None,
            prefetch_running: Arc::new(AtomicBool::new(false)),
            prefetch_gen: Arc::new(AtomicU64::new(0)),
        }
    }

    #[test]
    fn probe_reads_duration_frame_rate_and_audio() {
        let info = parse_probe(
            r#"{"streams":[{"codec_type":"video","avg_frame_rate":"30000/1001"},{"codec_type":"audio"}],"format":{"duration":"75.5"}}"#,
        )
        .unwrap();
        assert_eq!(info.duration, 75.5);
        assert!((info.fps - 29.970).abs() < 0.001);
        assert!(info.has_audio);
        assert!(parse_probe(r#"{"streams":[{"codec_type":"audio"}]}"#).is_none());
    }

    #[test]
    fn buttons_progress_and_scrubbing_match_mouse_columns() {
        let mut video = sample();
        assert_eq!(video.button_at(0), None);
        assert_eq!(video.button_at(1), Some(VideoControl::Play));
        assert_eq!(video.buttons()[0].1, "Play");
        let (start, length) = video.progress_layout(27);
        assert_eq!((start, length), (7, 13));
        assert_eq!(video.fraction_at(27, 6), None);
        assert_eq!(video.fraction_at(27, 7), Some(0.0));
        assert_eq!(video.fraction_at(27, 19), Some(1.0));
        video.scrub_to(0.5);
        assert_eq!(video.display_position(), 37.75);
        assert_eq!(format_time(video.display_position()), "00:38");
        assert_eq!(format_time(3725.0), "1:02:05");
    }

    #[test]
    fn volume_and_mute_are_bounded() {
        let mut video = sample();
        video.change_volume(false);
        assert_eq!(video.volume, 90);
        video.toggle_mute();
        assert!(video.muted);
        video.change_volume(true);
        assert!(!video.muted);
        assert_eq!(video.volume, 100);
        for _ in 0..20 {
            video.change_volume(false);
        }
        assert_eq!(video.volume, 0);
    }

    #[test]
    fn ffmpeg_decodes_and_seeks_a_real_video_when_available() {
        if Command::new("ffmpeg").arg("-version").output().is_err()
            || Command::new("ffprobe").arg("-version").output().is_err()
        {
            return;
        }
        let directory =
            PathBuf::from("target").join(format!("vos-video-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("clip.mp4");
        let created = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=64x36:rate=10:duration=1",
                "-an",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert!(created.status.success());

        let mut player = VideoPlayer::new(path, 32, 16, (1, 2)).unwrap();
        let started = Instant::now();
        while player.current_raw_frame.is_none() && started.elapsed().as_secs() < 5 {
            player.tick().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let frame = player.current_raw_frame.as_ref().unwrap();
        assert_eq!(frame.len(), 32 * 32 * 3);
        assert!(player.duration_secs >= 0.9);
        player.pause();
        player.seek_backward(5.0);
        let started = Instant::now();
        while player.is_loading && started.elapsed().as_secs() < 5 {
            player.tick().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(player.position_secs, 0.0);
        assert!(player.current_raw_frame.is_some());
        assert!(!player.is_playing);
        drop(player);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
