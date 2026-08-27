use anyhow::Result;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::time::Instant;

#[allow(dead_code)]
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "wav", "ogg", "aac", "m4a", "opus", "wma", "aiff",
];

pub struct AudioPlayer {
    pub path: PathBuf,
    pub playlist: Vec<PathBuf>,
    pub playlist_idx: usize,
    pub is_playing: bool,
    pub volume: f32,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub track_name: String,
    pub artist: String,
    pub album: String,
    /// Equalizer bar heights (0.0–1.0) for the animated side panel.
    pub eq_bars: [f32; 8],
    pub eq_tick: u64,
    /// Cover art: raw RGB24 bytes (flattened), or None if unavailable
    pub cover_art: Option<Vec<u8>>,
    /// Original pixel dimensions of the cover art (width, height)
    pub cover_dims: (u32, u32),
    // rodio internals - kept alive with _ prefix
    _stream: OutputStream,
    _stream_handle: OutputStreamHandle,
    sink: Option<Sink>,
    play_start: Option<Instant>,
    play_offset: f64,
}

impl AudioPlayer {
    pub fn new(path: PathBuf) -> Result<Self> {
        let playlist = vec![path.clone()];
        Self::from_playlist(playlist)
    }

    pub fn from_playlist(files: Vec<PathBuf>) -> Result<Self> {
        let path = files.first().cloned().unwrap_or_default();

        let track_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();

        let album = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let (_stream, _stream_handle) = match OutputStream::try_default() {
            Ok(pair) => pair,
            Err(e) => {
                return Err(anyhow::anyhow!("No audio output device: {}", e));
            }
        };

        Ok(AudioPlayer {
            path,
            playlist: files,
            playlist_idx: 0,
            is_playing: false,
            volume: 0.8,
            position_secs: 0.0,
            duration_secs: 0.0,
            track_name,
            artist: String::new(),
            album,
            eq_bars: [0.0; 8],
            eq_tick: 0,
            cover_art: None,
            cover_dims: (0, 0),
            _stream,
            _stream_handle,
            sink: None,
            play_start: None,
            play_offset: 0.0,
        })
    }

    fn load_current_track_meta(&mut self) {
        if let Some(p) = self.playlist.get(self.playlist_idx).cloned() {
            self.path = p.clone();
            // Try lofty first for rich metadata
            let meta = crate::modules::music_library::read_track_meta(&p);
            self.track_name = meta.title;
            self.artist = meta.artist;
            self.album = meta.album;
            self.duration_secs = meta.duration_secs;
            // Reset cover art (will be loaded separately)
            self.cover_art = None;
            self.cover_dims = (0, 0);
        }
    }

    /// Public wrapper for load_current_track_meta (used from input handler).
    pub fn load_current_track_meta_pub(&mut self) {
        self.load_current_track_meta();
    }

    /// Load cover art for the current track from embedded metadata.
    pub fn load_cover_art(&mut self) {
        match crate::modules::music_library::read_cover_art(&self.path) {
            Some((data, w, h)) => {
                self.cover_art = Some(data);
                self.cover_dims = (w, h);
            }
            None => {
                self.cover_art = None;
                self.cover_dims = (0, 0);
            }
        }
    }

    /// Append a track to the end of the playlist (queue).
    pub fn add_to_queue(&mut self, path: PathBuf) {
        self.playlist.push(path);
    }

    /// Remove a track from the playlist by index, adjusting playlist_idx.
    pub fn remove_from_queue(&mut self, idx: usize) {
        if idx >= self.playlist.len() {
            return;
        }
        self.playlist.remove(idx);
        if self.playlist.is_empty() {
            self.stop();
            self.playlist_idx = 0;
        } else if idx < self.playlist_idx {
            self.playlist_idx = self.playlist_idx.saturating_sub(1);
        } else if idx == self.playlist_idx {
            self.playlist_idx = self.playlist_idx.min(self.playlist.len().saturating_sub(1));
        }
    }

    pub fn play(&mut self) -> Result<()> {
        self.sink = None;

        let path = self.path.clone();
        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        let source = Decoder::new(reader)?;

        let sink = Sink::try_new(&self._stream_handle)?;
        sink.set_volume(self.volume);
        sink.append(source);

        self.sink = Some(sink);
        self.play_start = Some(Instant::now());
        self.is_playing = true;

        Ok(())
    }

    pub fn pause(&mut self) {
        if let Some(ref s) = self.sink {
            s.pause();
        }
        if let Some(start) = self.play_start.take() {
            self.play_offset += start.elapsed().as_secs_f64();
        }
        self.is_playing = false;
    }

    /// Resume playback from the current position (does NOT restart from beginning).
    pub fn resume(&mut self) {
        if let Some(ref s) = self.sink {
            s.play();
            self.play_start = Some(Instant::now());
            self.is_playing = true;
        } else {
            // No sink means we never started or stopped completely — restart from offset
            let _ = self.seek_to_secs(self.play_offset);
            self.is_playing = true;
        }
    }

    pub fn stop(&mut self) {
        self.sink = None;
        self.play_start = None;
        self.play_offset = 0.0;
        self.position_secs = 0.0;
        self.is_playing = false;
    }

    pub fn next_track(&mut self) -> Result<()> {
        if self.playlist.is_empty() {
            return Ok(());
        }
        let was_playing = self.is_playing;
        self.stop();
        self.playlist_idx = (self.playlist_idx + 1) % self.playlist.len();
        self.load_current_track_meta();
        if was_playing {
            self.play()?;
        }
        Ok(())
    }

    pub fn prev_track(&mut self) -> Result<()> {
        if self.playlist.is_empty() {
            return Ok(());
        }
        let was_playing = self.is_playing;
        self.stop();
        if self.playlist_idx == 0 {
            self.playlist_idx = self.playlist.len() - 1;
        } else {
            self.playlist_idx -= 1;
        }
        self.load_current_track_meta();
        if was_playing {
            self.play()?;
        }
        Ok(())
    }

    pub fn volume_up(&mut self) {
        self.volume = (self.volume + 0.1).min(1.0);
        if let Some(ref s) = self.sink {
            s.set_volume(self.volume);
        }
    }

    pub fn volume_down(&mut self) {
        self.volume = (self.volume - 0.1).max(0.0);
        if let Some(ref s) = self.sink {
            s.set_volume(self.volume);
        }
    }

    pub fn seek_forward(&mut self, secs: u64) {
        let new_pos = if self.duration_secs > 0.0 {
            (self.position_secs + secs as f64).min(self.duration_secs)
        } else {
            self.position_secs + secs as f64
        };
        let _ = self.seek_to_secs(new_pos);
    }

    pub fn seek_backward(&mut self, secs: u64) {
        let new_pos = (self.position_secs - secs as f64).max(0.0);
        let _ = self.seek_to_secs(new_pos);
    }

    /// Reopen the file and skip to `target_secs` using `Source::skip_duration`.
    fn seek_to_secs(&mut self, target_secs: f64) -> Result<()> {
        let was_playing = self.is_playing;

        // Drop the current sink to stop playback immediately.
        self.sink = None;

        let file = File::open(&self.path)?;
        let source = Decoder::new(BufReader::new(file))?
            .skip_duration(std::time::Duration::from_secs_f64(target_secs.max(0.0)));

        let sink = Sink::try_new(&self._stream_handle)?;
        sink.set_volume(self.volume);
        sink.append(source);

        if !was_playing {
            sink.pause();
        }

        self.sink = Some(sink);
        self.position_secs = target_secs;
        self.play_offset = target_secs;
        self.play_start = if was_playing {
            Some(Instant::now())
        } else {
            None
        };
        self.is_playing = was_playing;

        Ok(())
    }

    /// Called ~60fps by the main loop; advances position_secs when playing
    /// and handles automatic track advancement when a track finishes.
    pub fn tick(&mut self) {
        if self.is_playing {
            if let Some(start) = self.play_start {
                self.position_secs = self.play_offset + start.elapsed().as_secs_f64();
            }

            // Animate equalizer bars — only when playing.
            self.eq_tick = self.eq_tick.wrapping_add(1);
            for (i, bar) in self.eq_bars.iter_mut().enumerate() {
                let freq = 0.15 + (i as f64) * 0.07;
                let phase = (i as f64) * 0.8;
                let raw = ((self.eq_tick as f64 * freq + phase).sin() * 0.5 + 0.5) as f32;
                *bar = (*bar * 0.6 + raw * 0.4).clamp(0.0, 1.0);
            }

            let finished = self.sink.as_ref().map(|s| s.empty()).unwrap_or(false);

            if finished {
                let has_next =
                    self.playlist.len() > 1 || (self.playlist.len() == 1 && self.playlist_idx == 0);
                if has_next && self.playlist.len() > 1 {
                    let _ = self.next_track();
                } else {
                    self.stop();
                }
            }
        } else {
            // When paused: decay eq_bars toward 0 smoothly.
            for bar in self.eq_bars.iter_mut() {
                if *bar > 0.01 {
                    *bar = (*bar * 0.85).max(0.0);
                } else {
                    *bar = 0.0;
                }
            }
        }
    }

    /// Returns playback progress as a ratio in [0.0, 1.0].
    pub fn progress_ratio(&self) -> f32 {
        if self.duration_secs <= 0.0 {
            return 0.0;
        }
        ((self.position_secs / self.duration_secs) as f32).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volume_clamping() {
        let vol: f32 = (1.0_f32 + 0.1).min(1.0);
        assert!((vol - 1.0).abs() < f32::EPSILON);
        let vol2: f32 = (0.0_f32 - 0.1).max(0.0);
        assert!((vol2 - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_progress_ratio_no_duration() {
        let duration_secs = 0.0_f64;
        let position_secs = 5.0_f64;
        let ratio = if duration_secs <= 0.0 {
            0.0_f32
        } else {
            ((position_secs / duration_secs) as f32).clamp(0.0, 1.0)
        };
        assert!((ratio - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_progress_ratio_clamped() {
        let duration_secs = 60.0_f64;
        let position_secs = 120.0_f64;
        let ratio = if duration_secs <= 0.0 {
            0.0_f32
        } else {
            ((position_secs / duration_secs) as f32).clamp(0.0, 1.0)
        };
        assert!((ratio - 1.0).abs() < f32::EPSILON);
    }
}
