//! Log Viewer module — live tail, level filtering, text search.
//! No ratatui UI in this file; all rendering is in `ui/log_panel.rs`.

use std::collections::VecDeque;
use std::ops::Range;
use std::path::PathBuf;

use tokio::sync::mpsc;

// ── Public types ──────────────────────────────────────────────────────────────

/// Source of log lines fed into the viewer.
///
/// `Journalctl` and `Command(Vec<String>)` are intentionally public and
/// forward-declared for future Docker-logs / Linux-service integration
/// (§4.3 of PLAN.md), even though not all variants have UI entry-points yet.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum LogSource {
    /// Tail a file on disk.
    File(PathBuf),
    /// Stream `journalctl -f` (Linux-only at runtime; see open()).
    Journalctl,
    /// Read from the VOS SQLite `app_logs` table (snapshot, not streaming).
    AppDb,
    /// Run an arbitrary command and stream its stdout — e.g. `docker logs -f`.
    Command(Vec<String>),
}

/// Parsed severity level extracted from a log line.
#[derive(Debug, Clone, PartialEq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Unknown,
}

/// A single log line with its parsed metadata.
pub struct LogLine {
    /// Raw text of the line.
    pub raw: String,
    /// Detected severity.
    pub level: LogLevel,
    /// Byte range of the timestamp substring within `raw`, if one was found.
    pub ts_range: Option<Range<usize>>,
}

const MAX_LINES: usize = 10_000;

/// State for the Log Viewer mode.
pub struct LogViewer {
    pub source: LogSource,
    /// Circular buffer of parsed log lines.
    pub lines: VecDeque<LogLine>,
    /// First visible line index (within the filtered view).
    pub scroll: usize,
    /// When true, scroll tracks the tail automatically.
    pub follow: bool,
    /// If set, only lines at or above this level are shown.
    pub level_filter: Option<LogLevel>,
    /// Case-insensitive substring filter applied on top of `level_filter`.
    pub text_filter: String,
    /// Receives raw line strings from the background task.
    pub rx: Option<mpsc::UnboundedReceiver<String>>,
    /// Holds the child process so it is killed when the viewer closes.
    pub _child: Option<tokio::process::Child>,
    /// Set to true when the filter dialog is being used for text_filter.
    pub awaiting_filter_input: bool,
}

impl LogViewer {
    /// Create a new, empty LogViewer for the given source.
    /// Call `open()` (async) to actually start streaming.
    pub fn new(source: LogSource) -> Self {
        Self {
            source,
            lines: VecDeque::new(),
            scroll: 0,
            follow: true,
            level_filter: None,
            text_filter: String::new(),
            rx: None,
            _child: None,
            awaiting_filter_input: false,
        }
    }

    /// Start the background I/O task for the current `source`.
    ///
    /// On macOS, `Journalctl` is a no-op (returns an error string via a
    /// sentinel line) — the caller should push a notification.
    pub fn open(&mut self) -> Option<String> {
        match &self.source {
            LogSource::File(path) => {
                self.open_file(path.clone());
                None
            }
            LogSource::Journalctl => self.open_journalctl(),
            LogSource::AppDb => {
                // Snapshot: read from DB synchronously in the caller and push
                // lines directly. No background task needed.
                None
            }
            LogSource::Command(args) => {
                if args.is_empty() {
                    return Some("LogSource::Command: no arguments".to_string());
                }
                self.open_command(args.clone());
                None
            }
        }
    }

    /// Close the background task cleanly: drop rx and kill the child.
    pub fn close(&mut self) {
        self.rx = None;
        if let Some(mut child) = self._child.take() {
            let _ = child.start_kill();
        }
    }

    // ── Private: File tail ────────────────────────────────────────────────────

    fn open_file(&mut self, path: PathBuf) {
        let (tx, rx) = mpsc::unbounded_channel::<String>();
        self.rx = Some(rx);

        tokio::spawn(async move {
            use std::io::SeekFrom;
            use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};
            use tokio::time::{Duration, sleep};

            let file = match tokio::fs::File::open(&path).await {
                Ok(f) => f,
                Err(e) => {
                    let _ = tx.send(format!("[ERROR] Cannot open {:?}: {e}", path));
                    return;
                }
            };

            let mut reader = BufReader::new(file);

            // Seek to approx last 1000 lines by reading all and keeping the tail.
            // For small files this is fine; for large files we seek to end-256KB.
            let metadata = tokio::fs::metadata(&path).await;
            if let Ok(meta) = metadata {
                let size = meta.len();
                if size > 256 * 1024 {
                    let _ = reader.seek(SeekFrom::End(-(256 * 1024))).await;
                    // Discard the partial first line.
                    let mut _discard = String::new();
                    let _ = reader.read_line(&mut _discard).await;
                }
            }

            // Drain the initial content.
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break,
                    Ok(_) => {
                        let l = line.trim_end_matches(['\n', '\r']).to_string();
                        if tx.send(l).is_err() {
                            return;
                        }
                    }
                    Err(_) => break,
                }
            }

            // Tail: poll every 250ms for new content.
            loop {
                sleep(Duration::from_millis(250)).await;
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break,
                        Ok(_) => {
                            let l = line.trim_end_matches(['\n', '\r']).to_string();
                            if tx.send(l).is_err() {
                                return;
                            }
                        }
                        Err(_) => return,
                    }
                }
            }
        });
    }

    // ── Private: journalctl ───────────────────────────────────────────────────

    fn open_journalctl(&mut self) -> Option<String> {
        #[cfg(not(target_os = "linux"))]
        {
            return Some("journalctl is unavailable on this system (Linux only)".to_string());
        }

        #[cfg(target_os = "linux")]
        {
            // Runtime check: is journalctl in PATH?
            if std::process::Command::new("which")
                .arg("journalctl")
                .output()
                .map(|o| !o.status.success())
                .unwrap_or(true)
            {
                return Some("journalctl not found in PATH".to_string());
            }

            use tokio::io::{AsyncBufReadExt, BufReader};
            use tokio::process::Command;

            let (tx, rx) = mpsc::unbounded_channel::<String>();
            self.rx = Some(rx);

            let result = Command::new("journalctl")
                .args(["-f", "-n", "500", "--no-pager"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn();

            match result {
                Err(e) => {
                    return Some(format!("Failed to spawn journalctl: {e}"));
                }
                Ok(mut child) => {
                    let stdout = child.stdout.take().unwrap();
                    self._child = Some(child);
                    tokio::spawn(async move {
                        let mut lines = BufReader::new(stdout).lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            if tx.send(line).is_err() {
                                break;
                            }
                        }
                    });
                }
            }
            None
        }
    }

    // ── Private: generic command ──────────────────────────────────────────────

    fn open_command(&mut self, args: Vec<String>) {
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio::process::Command;

        let (tx, rx) = mpsc::unbounded_channel::<String>();
        self.rx = Some(rx);

        let program = args[0].clone();
        let rest: Vec<String> = args[1..].to_vec();

        let result = Command::new(&program)
            .args(&rest)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();

        match result {
            Err(e) => {
                let _ = tx.send(format!("[ERROR] Cannot run {program}: {e}"));
            }
            Ok(mut child) => {
                let stdout = child.stdout.take().unwrap();
                let stderr = child.stderr.take();
                self._child = Some(child);

                let tx2 = tx.clone();
                tokio::spawn(async move {
                    let mut lines = BufReader::new(stdout).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        if tx2.send(line).is_err() {
                            break;
                        }
                    }
                });

                if let Some(se) = stderr {
                    tokio::spawn(async move {
                        let mut lines = BufReader::new(se).lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            if tx.send(line).is_err() {
                                break;
                            }
                        }
                    });
                }
            }
        }
    }

    // ── Tick: drain channel ───────────────────────────────────────────────────

    /// Called on each app tick when the LogViewer is active.
    /// Drains all available lines from the channel, parses them, and appends
    /// them to the circular buffer. If `follow` is set, advances the scroll.
    pub fn tick(&mut self) {
        if self.rx.is_none() {
            return;
        }

        let mut added = false;
        loop {
            match self.rx.as_mut().unwrap().try_recv() {
                Ok(raw) => {
                    let line = parse_line(raw);
                    self.lines.push_back(line);
                    if self.lines.len() > MAX_LINES {
                        self.lines.pop_front();
                        // Keep scroll in bounds.
                        if self.scroll > 0 {
                            self.scroll = self.scroll.saturating_sub(1);
                        }
                    }
                    added = true;
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    self.rx = None;
                    break;
                }
            }
        }

        if added && self.follow {
            let visible = self.apply_filters();
            self.scroll = visible.len().saturating_sub(1);
        }
    }

    /// Return the subset of lines passing both level_filter and text_filter.
    /// Returns indices into `self.lines` so the UI can use them.
    pub fn apply_filters(&self) -> Vec<usize> {
        let text_lower = self.text_filter.to_lowercase();
        self.lines
            .iter()
            .enumerate()
            .filter(|(_, line)| {
                // Level filter
                if let Some(ref min_level) = self.level_filter {
                    if !level_passes(min_level, &line.level) {
                        return false;
                    }
                }
                // Text filter
                if !text_lower.is_empty() {
                    if !line.raw.to_lowercase().contains(&text_lower) {
                        return false;
                    }
                }
                true
            })
            .map(|(i, _)| i)
            .collect()
    }

    // ── Scroll helpers ────────────────────────────────────────────────────────

    pub fn scroll_up(&mut self) {
        self.follow = false;
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.follow = false;
        let visible_len = self.apply_filters().len();
        if visible_len > 0 && self.scroll + 1 < visible_len {
            self.scroll += 1;
        }
    }

    pub fn page_up(&mut self, n: usize) {
        self.follow = false;
        self.scroll = self.scroll.saturating_sub(n);
    }

    pub fn page_down(&mut self, n: usize) {
        self.follow = false;
        let visible_len = self.apply_filters().len();
        if visible_len > 0 {
            self.scroll = (self.scroll + n).min(visible_len.saturating_sub(1));
        }
    }

    pub fn jump_to_end(&mut self) {
        self.follow = true;
        let visible_len = self.apply_filters().len();
        self.scroll = visible_len.saturating_sub(1);
    }

    /// Cycle level filter: None → Error → Warn → Info → None.
    pub fn cycle_level_filter(&mut self, level: LogLevel) {
        if self.level_filter.as_ref() == Some(&level) {
            self.level_filter = None;
        } else {
            self.level_filter = Some(level);
        }
        // Re-clamp scroll to new filtered length.
        let visible_len = self.apply_filters().len();
        if visible_len > 0 {
            self.scroll = self.scroll.min(visible_len.saturating_sub(1));
        } else {
            self.scroll = 0;
        }
    }
}

// ── Level ordering for filter (show levels >= min) ────────────────────────────

fn level_passes(min: &LogLevel, line_level: &LogLevel) -> bool {
    level_rank(line_level) >= level_rank(min)
}

fn level_rank(l: &LogLevel) -> u8 {
    match l {
        LogLevel::Unknown => 0,
        LogLevel::Trace => 1,
        LogLevel::Debug => 2,
        LogLevel::Info => 3,
        LogLevel::Warn => 4,
        LogLevel::Error => 5,
    }
}

// ── Line parser ───────────────────────────────────────────────────────────────

fn parse_line(raw: String) -> LogLine {
    let level = detect_level(&raw);
    let ts_range = detect_timestamp(&raw);
    LogLine {
        raw,
        level,
        ts_range,
    }
}

/// Case-insensitive search for ERROR/WARN/INFO/DEBUG/TRACE in the line.
/// Returns the most severe level found.
pub fn detect_level(line: &str) -> LogLevel {
    let upper = line.to_uppercase();
    if upper.contains("ERROR") || upper.contains("CRITICAL") || upper.contains("FATAL") {
        LogLevel::Error
    } else if upper.contains("WARN") {
        LogLevel::Warn
    } else if upper.contains("INFO") {
        LogLevel::Info
    } else if upper.contains("DEBUG") {
        LogLevel::Debug
    } else if upper.contains("TRACE") {
        LogLevel::Trace
    } else {
        LogLevel::Unknown
    }
}

/// Detect a timestamp at the start of the line.
/// Recognises:
///   - ISO-8601: `2024-01-15T12:34:56` or `2024-01-15 12:34:56`
///   - syslog:   `Jan 15 12:34:56` or `Jan  5 12:34:56`
pub fn detect_timestamp(line: &str) -> Option<Range<usize>> {
    // ISO-8601: YYYY-MM-DDTHH:MM:SS or YYYY-MM-DD HH:MM:SS
    if line.len() >= 19 {
        let candidate = &line[..19];
        if is_iso_timestamp(candidate) {
            return Some(0..19);
        }
    }

    // Syslog: "Mmm DD HH:MM:SS " (15 chars) — e.g. "Jan 15 12:34:56"
    if line.len() >= 15 {
        let candidate = &line[..15];
        if is_syslog_timestamp(candidate) {
            return Some(0..15);
        }
    }

    None
}

fn is_iso_timestamp(s: &str) -> bool {
    // YYYY-MM-DDTHH:MM:SS or YYYY-MM-DD HH:MM:SS
    let b = s.as_bytes();
    if b.len() < 19 {
        return false;
    }
    b[0..4].iter().all(|c| c.is_ascii_digit())
        && b[4] == b'-'
        && b[5..7].iter().all(|c| c.is_ascii_digit())
        && b[7] == b'-'
        && b[8..10].iter().all(|c| c.is_ascii_digit())
        && (b[10] == b'T' || b[10] == b' ')
        && b[11..13].iter().all(|c| c.is_ascii_digit())
        && b[13] == b':'
        && b[14..16].iter().all(|c| c.is_ascii_digit())
        && b[16] == b':'
        && b[17..19].iter().all(|c| c.is_ascii_digit())
}

fn is_syslog_timestamp(s: &str) -> bool {
    // "Mmm DD HH:MM:SS" — month is 3 letters, then space, day (1-2 digits + space), time
    let b = s.as_bytes();
    if b.len() < 15 {
        return false;
    }
    b[0..3].iter().all(|c| c.is_ascii_alphabetic())
        && b[3] == b' '
        && (b[4].is_ascii_digit() || b[4] == b' ')
        && b[5].is_ascii_digit()
        && b[6] == b' '
        && b[7..9].iter().all(|c| c.is_ascii_digit())
        && b[9] == b':'
        && b[10..12].iter().all(|c| c.is_ascii_digit())
        && b[12] == b':'
        && b[13..15].iter().all(|c| c.is_ascii_digit())
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Level detection ───────────────────────────────────────────────────────

    #[test]
    fn test_level_error() {
        assert_eq!(
            detect_level("2024-01-01 12:00:00 ERROR something failed"),
            LogLevel::Error
        );
        assert_eq!(detect_level("[FATAL] out of memory"), LogLevel::Error);
        assert_eq!(detect_level("[CRITICAL] disk full"), LogLevel::Error);
    }

    #[test]
    fn test_level_warn() {
        assert_eq!(detect_level("WARN: connection slow"), LogLevel::Warn);
        assert_eq!(detect_level("[warn] retry 3"), LogLevel::Warn);
    }

    #[test]
    fn test_level_info() {
        assert_eq!(detect_level("[INFO] server started"), LogLevel::Info);
        assert_eq!(detect_level("info: listening on :8080"), LogLevel::Info);
    }

    #[test]
    fn test_level_debug() {
        assert_eq!(detect_level("[DEBUG] entering function"), LogLevel::Debug);
    }

    #[test]
    fn test_level_trace() {
        assert_eq!(detect_level("[TRACE] span entered"), LogLevel::Trace);
    }

    #[test]
    fn test_level_unknown() {
        assert_eq!(detect_level("some ordinary log line"), LogLevel::Unknown);
        assert_eq!(detect_level(""), LogLevel::Unknown);
    }

    #[test]
    fn test_level_priority_error_over_warn() {
        // ERROR takes priority even when WARN appears too.
        assert_eq!(
            detect_level("ERROR: WARN-level threshold exceeded"),
            LogLevel::Error
        );
    }

    // ── Timestamp detection ───────────────────────────────────────────────────

    #[test]
    fn test_ts_iso_t() {
        let line = "2024-01-15T12:34:56 INFO started";
        assert_eq!(detect_timestamp(line), Some(0..19));
    }

    #[test]
    fn test_ts_iso_space() {
        let line = "2024-01-15 12:34:56 something happened";
        assert_eq!(detect_timestamp(line), Some(0..19));
    }

    #[test]
    fn test_ts_syslog() {
        let line = "Jan 15 12:34:56 myhost sshd[1234]: something";
        assert_eq!(detect_timestamp(line), Some(0..15));
    }

    #[test]
    fn test_ts_none() {
        assert_eq!(detect_timestamp("no timestamp here"), None);
        assert_eq!(detect_timestamp(""), None);
    }

    // ── Filter logic ──────────────────────────────────────────────────────────

    fn make_viewer_with_lines(lines: Vec<(&str, LogLevel)>) -> LogViewer {
        let mut v = LogViewer::new(LogSource::AppDb);
        for (raw, level) in lines {
            v.lines.push_back(LogLine {
                raw: raw.to_string(),
                level,
                ts_range: None,
            });
        }
        v
    }

    #[test]
    fn test_filter_level_error_only() {
        let mut v = make_viewer_with_lines(vec![
            ("line error", LogLevel::Error),
            ("line info", LogLevel::Info),
            ("line warn", LogLevel::Warn),
        ]);
        v.level_filter = Some(LogLevel::Error);
        let visible = v.apply_filters();
        assert_eq!(visible.len(), 1);
        assert_eq!(v.lines[visible[0]].level, LogLevel::Error);
    }

    #[test]
    fn test_filter_level_warn_includes_error() {
        let mut v = make_viewer_with_lines(vec![
            ("line error", LogLevel::Error),
            ("line warn", LogLevel::Warn),
            ("line info", LogLevel::Info),
        ]);
        v.level_filter = Some(LogLevel::Warn);
        let visible = v.apply_filters();
        assert_eq!(visible.len(), 2);
    }

    #[test]
    fn test_filter_text() {
        let mut v = make_viewer_with_lines(vec![
            ("connection established", LogLevel::Info),
            ("connection failed", LogLevel::Error),
            ("server started", LogLevel::Info),
        ]);
        v.text_filter = "connection".to_string();
        let visible = v.apply_filters();
        assert_eq!(visible.len(), 2);
    }

    #[test]
    fn test_filter_combined_level_and_text() {
        let mut v = make_viewer_with_lines(vec![
            ("ERROR: connection lost", LogLevel::Error),
            ("INFO: connection ok", LogLevel::Info),
            ("WARN: retrying", LogLevel::Warn),
        ]);
        v.level_filter = Some(LogLevel::Error);
        v.text_filter = "connection".to_string();
        let visible = v.apply_filters();
        assert_eq!(visible.len(), 1);
        assert!(v.lines[visible[0]].raw.contains("ERROR"));
    }

    #[test]
    fn test_filter_none_returns_all() {
        let mut v = make_viewer_with_lines(vec![
            ("a", LogLevel::Error),
            ("b", LogLevel::Info),
            ("c", LogLevel::Unknown),
        ]);
        v.level_filter = None;
        v.text_filter = String::new();
        let visible = v.apply_filters();
        assert_eq!(visible.len(), 3);
    }

    #[test]
    fn test_filter_text_case_insensitive() {
        let mut v = make_viewer_with_lines(vec![
            ("Connection FAILED", LogLevel::Error),
            ("other line", LogLevel::Info),
        ]);
        v.text_filter = "connection".to_string();
        let visible = v.apply_filters();
        assert_eq!(visible.len(), 1);
    }
}
