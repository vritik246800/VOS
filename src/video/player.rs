use anyhow::Result;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[allow(dead_code)]
pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v", "mpeg", "mpg", "3gp",
];

/// Minimum frames in buffer before playback starts (avoids stutter on first play/seek).
const MIN_BUFFER_FRAMES: usize = 15;
/// Maximum frames kept in buffer (pipe reader pauses above this).
const MAX_BUFFER_FRAMES: usize = 90;
/// Rendered ANSI strings kept in the render cache (half-block mode only).
const RENDER_CACHE_SIZE: usize = 16;

// ──────────────────────────────────────────────────────────────────────────────
// Pre-decoded frame buffer (shared with pipe reader thread)
// ──────────────────────────────────────────────────────────────────────────────

/// A raw RGB24 frame decoded by the background pipe reader.
#[derive(Clone)]
pub struct RawFrame {
    #[allow(dead_code)]
    pub pts: f64,
    /// Raw RGB24 pixel data (width × height_px × 3 bytes).
    pub pixels: Vec<u8>,
}

/// Shared ring buffer of pre-decoded frames, protected by a Mutex.
pub type FrameBuffer = Arc<Mutex<VecDeque<RawFrame>>>;

// ──────────────────────────────────────────────────────────────────────────────
// VideoPlayer
// ──────────────────────────────────────────────────────────────────────────────

pub struct VideoPlayer {
    pub path: PathBuf,
    pub is_playing: bool,
    /// True while waiting for the buffer to fill before (re)starting playback.
    pub is_loading: bool,
    pub current_frame: usize,
    pub fps: f64,
    pub width: u16,
    pub height: u16,
    /// Pixel dimensions of one terminal cell: (cell_w_px, cell_h_px).
    /// `(1, 2)` = half-block mode; real cell size = Kitty native-res decode.
    pub cell_px: (u16, u16),
    pub frame_lines: Vec<String>,
    pub current_raw_frame: Option<Vec<u8>>,
    pub last_tick: Instant,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub error: Option<String>,
    pub title: String,

    pub frame_buffer: FrameBuffer,
    pub render_cache: VecDeque<(f64, Vec<String>)>,
    /// Snapshot of the buffer length, updated each tick (avoids locking in render).
    pub buffer_len: usize,

    prefetch_running: Arc<AtomicBool>,
    /// Monotonic generation counter. Incrementing it signals the current pipe
    /// reader thread to exit so a new one can be started cleanly.
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
            .and_then(|s| s.to_str())
            .unwrap_or("video")
            .to_string();

        let (duration_secs, fps) = probe_video(&path);
        let frame_buffer: FrameBuffer = Arc::new(Mutex::new(VecDeque::new()));

        let mut player = Self {
            path,
            is_playing: false,
            is_loading: false,
            current_frame: 0,
            fps,
            width: render_width,
            height: render_height,
            cell_px,
            frame_lines: Vec::new(),
            current_raw_frame: None,
            last_tick: Instant::now(),
            position_secs: 0.0,
            duration_secs,
            error: None,
            title,
            frame_buffer,
            render_cache: VecDeque::new(),
            buffer_len: 0,
            prefetch_running: Arc::new(AtomicBool::new(false)),
            prefetch_gen: Arc::new(AtomicU64::new(0)),
        };

        // Pre-fill the buffer immediately so the first Space press is instant.
        player.start_pipe_reader(0.0);
        Ok(player)
    }

    pub fn play(&mut self) {
        if !self.prefetch_running.load(Ordering::SeqCst) {
            self.start_pipe_reader(self.position_secs);
        }
        // Defer is_playing = true until tick() sees MIN_BUFFER_FRAMES ready.
        self.is_loading = true;
        self.last_tick = Instant::now();
    }

    pub fn pause(&mut self) {
        self.is_playing = false;
        self.is_loading = false;
    }

    pub fn stop(&mut self) {
        self.prefetch_gen.fetch_add(1, Ordering::SeqCst);
        self.is_playing = false;
        self.is_loading = false;
        self.position_secs = 0.0;
        self.current_frame = 0;
        self.frame_lines.clear();
        self.current_raw_frame = None;
        self.last_tick = Instant::now();
        if let Ok(mut buf) = self.frame_buffer.lock() {
            buf.clear();
        }
        self.render_cache.clear();
        self.buffer_len = 0;
    }

    /// Returns true if a new frame was rendered.
    pub fn tick(&mut self) -> Result<bool> {
        self.buffer_len = self.frame_buffer.lock().unwrap().len();

        // Waiting for buffer to fill before (re)starting playback.
        if self.is_loading {
            if self.buffer_len >= MIN_BUFFER_FRAMES {
                self.is_loading = false;
                self.is_playing = true;
                self.last_tick = Instant::now();
            }
            return Ok(false);
        }

        if !self.is_playing {
            return Ok(false);
        }

        let elapsed = self.last_tick.elapsed().as_secs_f64();
        let frame_duration = 1.0 / self.fps.max(1.0);
        if elapsed < frame_duration {
            return Ok(false);
        }

        self.last_tick = Instant::now();
        self.position_secs += elapsed;

        if self.duration_secs > 0.0 && self.position_secs >= self.duration_secs {
            self.position_secs = self.duration_secs;
            self.is_playing = false;
            return Ok(false);
        }

        self.advance_frame()?;
        self.current_frame += 1;
        Ok(true)
    }

    pub fn seek_forward(&mut self, secs: f64) {
        self.position_secs += secs;
        if self.duration_secs > 0.0 {
            self.position_secs = self.position_secs.min(self.duration_secs);
        }
        self.rebuffer(self.position_secs);
    }

    pub fn seek_backward(&mut self, secs: f64) {
        self.position_secs = (self.position_secs - secs).max(0.0);
        self.rebuffer(self.position_secs);
    }

    /// Call from the render function when the display area changes to restart
    /// the pipe reader at the correct decode resolution.
    pub fn resize_if_needed(&mut self, new_w: u16, new_h: u16) {
        if (self.width as i32 - new_w as i32).abs() > 5
            || (self.height as i32 - new_h as i32).abs() > 3
        {
            self.width = new_w;
            self.height = new_h;
            if let Ok(mut buf) = self.frame_buffer.lock() {
                buf.clear();
            }
            self.render_cache.clear();
            self.start_pipe_reader(self.position_secs);
        }
    }

    /// Returns position / duration clamped to 0.0–1.0.
    pub fn progress_ratio(&self) -> f32 {
        if self.duration_secs <= 0.0 {
            return 0.0;
        }
        ((self.position_secs / self.duration_secs) as f32).clamp(0.0, 1.0)
    }

    /// Percentage (0–100) of the minimum pre-roll buffer that is filled.
    pub fn buffer_fill_pct(&self) -> u8 {
        ((self.buffer_len * 100) / MIN_BUFFER_FRAMES.max(1)).min(100) as u8
    }

    // ── Internal helpers ─────────────────────────────────────────────────────

    /// Kill the current pipe reader, flush the buffer, and start a new one
    /// from `from_pts`. Sets `is_loading` so tick() waits for the buffer.
    fn rebuffer(&mut self, from_pts: f64) {
        self.last_tick = Instant::now();
        if let Ok(mut buf) = self.frame_buffer.lock() {
            buf.clear();
        }
        let was_playing = self.is_playing;
        self.is_playing = false;
        if was_playing {
            self.is_loading = true;
        }
        self.start_pipe_reader(from_pts);
    }

    fn advance_frame(&mut self) -> Result<()> {
        let pts = self.position_secs;
        let is_halfblock = self.cell_px == (1, 2);

        // Check render cache (half-block mode only).
        if is_halfblock {
            for (cached_pts, cached_lines) in &self.render_cache {
                if (cached_pts - pts).abs() < 0.5 / self.fps.max(1.0) {
                    self.frame_lines = cached_lines.clone();
                    self.error = None;
                    return Ok(());
                }
            }
        }

        // Pop the next pre-decoded frame from the pipe buffer.
        let raw_frame = self.frame_buffer.lock().unwrap().pop_front();

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
            return Ok(());
        }

        // Buffer underrun during playback — re-buffer from current position.
        self.is_playing = false;
        self.is_loading = true;
        if !self.prefetch_running.load(Ordering::SeqCst) {
            self.start_pipe_reader(self.position_secs);
        }
        Ok(())
    }

    // ── Background pipe reader ───────────────────────────────────────────────

    /// Spawn a background thread that runs one persistent `ffmpeg` process and
    /// pipes all frames (as raw RGB24) starting from `from_pts`.  The thread
    /// fills `frame_buffer` up to `MAX_BUFFER_FRAMES`, pausing when full, and
    /// exits as soon as `prefetch_gen` changes (incremented by callers to
    /// signal shutdown).
    fn start_pipe_reader(&mut self, from_pts: f64) {
        // Bump the generation — the old thread will notice and exit.
        let pipe_gen = self.prefetch_gen.fetch_add(1, Ordering::SeqCst) + 1;
        self.prefetch_running.store(true, Ordering::SeqCst);

        let path = self.path.clone();
        let width = self.width;
        let height = self.height;
        let cell_px = self.cell_px;
        let fps = self.fps;
        let frame_buffer = Arc::clone(&self.frame_buffer);
        let running_flag = Arc::clone(&self.prefetch_running);
        let gen_counter = Arc::clone(&self.prefetch_gen);

        std::thread::spawn(move || {
            let decode_w = width as u32 * cell_px.0 as u32;
            let decode_h = height as u32 * cell_px.1 as u32;
            let frame_size = decode_w as usize * decode_h as usize * 3;
            let frame_duration = 1.0 / fps.max(1.0);

            let mut child = match std::process::Command::new("ffmpeg")
                .args([
                    "-ss",
                    &format!("{from_pts:.3}"),
                    "-i",
                    &path.to_string_lossy().into_owned(),
                    "-vf",
                    &format!("scale={decode_w}:{decode_h}"),
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "rgb24",
                    "pipe:1",
                ])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(c) => c,
                Err(_) => {
                    running_flag.store(false, Ordering::SeqCst);
                    return;
                }
            };

            let mut stdout = match child.stdout.take() {
                Some(s) => s,
                None => {
                    running_flag.store(false, Ordering::SeqCst);
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

                // Pause when the buffer is full to avoid unbounded memory use.
                if frame_buffer.lock().unwrap().len() >= MAX_BUFFER_FRAMES {
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
                        frame_buffer.lock().unwrap().push_back(RawFrame {
                            pts,
                            pixels: raw.clone(),
                        });
                        frame_idx += 1;
                    }
                    Err(_) => break, // EOF or pipe closed
                }
            }

            running_flag.store(false, Ordering::SeqCst);
        });
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Frame rendering helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Render raw RGB24 pixel data as ANSI-coloured half-block strings.
/// `height_px` must be even (= `height_chars * 2`).
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

// ──────────────────────────────────────────────────────────────────────────────
// ffprobe helpers
// ──────────────────────────────────────────────────────────────────────────────

fn probe_video(path: &PathBuf) -> (f64, f64) {
    let out = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_streams",
            &path.to_string_lossy().into_owned(),
        ])
        .output();

    match out {
        Ok(output) => {
            let json = String::from_utf8_lossy(&output.stdout).into_owned();
            let duration = parse_json_float(&json, "duration").unwrap_or(0.0);
            let fps = parse_fps(&json).unwrap_or(24.0);
            (duration, fps)
        }
        Err(_) => (0.0, 24.0),
    }
}

fn parse_json_float(json: &str, key: &str) -> Option<f64> {
    let search = format!("\"{}\":", key);
    let pos = json.find(&search)?;
    let after = json[pos + search.len()..].trim_start();
    let after = after.trim_start_matches('"');
    let end = after.find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')?;
    after[..end].parse().ok()
}

fn parse_fps(json: &str) -> Option<f64> {
    for key in &["r_frame_rate", "avg_frame_rate"] {
        let search = format!("\"{}\":\"", key);
        if let Some(pos) = json.find(&search) {
            let after = &json[pos + search.len()..];
            if let Some(end) = after.find('"') {
                let rate_str = &after[..end];
                if let Some(slash) = rate_str.find('/') {
                    let num: f64 = rate_str[..slash].parse().ok()?;
                    let den: f64 = rate_str[slash + 1..].parse().ok()?;
                    if den != 0.0 {
                        return Some(num / den);
                    }
                } else if let Ok(v) = rate_str.parse::<f64>() {
                    return Some(v);
                }
            }
        }
    }
    None
}
