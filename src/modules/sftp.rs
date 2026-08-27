//! SFTP transfer panel module.
//!
//! Two-pane (local/remote) file transfer over `sftp`. All remote operations
//! (listing, get, put) are shelled out to the system `sftp` binary in batch
//! mode and run on background threads so the UI thread never blocks; results
//! are delivered back through an `mpsc` channel and drained in `tick()`.
//!
//! Mirrors the background-scan pattern used by `crate::modules::disk`.

use crate::fs::explorer::FileExplorer;
use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;

/// Which pane currently has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SftpFocus {
    Local,
    Remote,
}

/// One row in the remote pane's listing.
#[derive(Debug, Clone)]
pub struct SftpEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// Messages delivered from background threads back to the UI.
enum SftpMsg {
    /// A remote directory listing completed.
    Listed(Result<Vec<SftpEntry>, String>),
    /// A download (`get`) completed.
    Downloaded(Result<(), String>),
    /// An upload (`put`) completed.
    Uploaded(Result<(), String>),
}

pub struct SftpPanel {
    pub local: FileExplorer,
    pub alias: String,
    /// Remote working directory; starts at the server's default ("." == home).
    pub remote_path: String,
    pub remote_entries: Vec<SftpEntry>,
    pub remote_selected: usize,
    pub focus: SftpFocus,
    pub status_msg: String,
    pub loading: bool,
    /// Sending/receiving ends of the background-operation channel. Created
    /// once and kept alive for the panel's whole lifetime so multiple
    /// background threads (listing, download, upload) can be in flight
    /// concurrently — each spawned thread holds its own clone of `tx`, and
    /// `tick()` simply drains whatever has arrived on `rx`.
    tx: mpsc::Sender<SftpMsg>,
    rx: mpsc::Receiver<SftpMsg>,
}

impl SftpPanel {
    /// Build a new panel. Does **not** trigger a remote listing — call
    /// [`SftpPanel::refresh_remote`] separately so construction never blocks.
    pub fn new(alias: String, local_start: PathBuf) -> Result<Self> {
        let (tx, rx) = mpsc::channel::<SftpMsg>();
        Ok(Self {
            local: FileExplorer::new(local_start)?,
            alias,
            remote_path: ".".to_string(),
            remote_entries: Vec::new(),
            remote_selected: 0,
            focus: SftpFocus::Local,
            status_msg: String::new(),
            loading: false,
            tx,
            rx,
        })
    }

    /// Spawn a background thread to (re-)list `remote_path` on the server.
    pub fn refresh_remote(&mut self) {
        let alias = self.alias.clone();
        let path = self.remote_path.clone();
        let tx = self.tx.clone();
        self.loading = true;

        std::thread::spawn(move || {
            let result = list_remote(&alias, &path).map_err(|e| e.to_string());
            let _ = tx.send(SftpMsg::Listed(result));
        });
    }

    /// Drain any pending background-thread messages. Never panics on a
    /// transport error — failures become a status message.
    pub fn tick(&mut self) {
        loop {
            match self.rx.try_recv() {
                Ok(SftpMsg::Listed(Ok(entries))) => {
                    self.remote_entries = entries;
                    self.loading = false;
                    if self.remote_entries.is_empty() {
                        self.remote_selected = 0;
                    } else {
                        self.remote_selected =
                            self.remote_selected.min(self.remote_entries.len() - 1);
                    }
                }
                Ok(SftpMsg::Listed(Err(e))) => {
                    self.loading = false;
                    self.status_msg = format!("sftp ls failed: {e}");
                }
                Ok(SftpMsg::Downloaded(Ok(()))) => {
                    self.status_msg = "Download complete".to_string();
                    let _ = self.local.load_entries();
                }
                Ok(SftpMsg::Downloaded(Err(e))) => {
                    self.status_msg = format!("Download failed: {e}");
                }
                Ok(SftpMsg::Uploaded(Ok(()))) => {
                    self.status_msg = "Upload complete".to_string();
                    self.refresh_remote();
                }
                Ok(SftpMsg::Uploaded(Err(e))) => {
                    self.status_msg = format!("Upload failed: {e}");
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            SftpFocus::Local => SftpFocus::Remote,
            SftpFocus::Remote => SftpFocus::Local,
        };
    }

    pub fn move_up(&mut self) {
        match self.focus {
            SftpFocus::Local => self.local.move_up(),
            SftpFocus::Remote => {
                if self.remote_selected > 0 {
                    self.remote_selected -= 1;
                }
            }
        }
    }

    pub fn move_down(&mut self) {
        match self.focus {
            SftpFocus::Local => self.local.move_down(),
            SftpFocus::Remote => {
                if self.remote_selected + 1 < self.remote_entries.len() {
                    self.remote_selected += 1;
                }
            }
        }
    }

    /// Open the selected directory entry in whichever pane has focus.
    pub fn enter_selected(&mut self) -> Result<()> {
        match self.focus {
            SftpFocus::Local => {
                if let Some(entry) = self.local.selected_entry() {
                    if entry.is_dir {
                        let path = entry.path.clone();
                        self.local.enter_dir(path)?;
                    }
                }
            }
            SftpFocus::Remote => {
                if let Some(entry) = self.remote_entries.get(self.remote_selected) {
                    if entry.is_dir {
                        self.remote_path = join_remote(&self.remote_path, &entry.name);
                        self.remote_selected = 0;
                        self.refresh_remote();
                    }
                }
            }
        }
        Ok(())
    }

    /// Navigate to the parent directory in whichever pane has focus.
    pub fn go_parent(&mut self) {
        match self.focus {
            SftpFocus::Local => {
                let _ = self.local.go_parent();
            }
            SftpFocus::Remote => {
                self.remote_path = parent_remote(&self.remote_path);
                self.remote_selected = 0;
                self.refresh_remote();
            }
        }
    }

    /// Download the selected remote entry into the local pane's current directory.
    pub fn download_selected(&mut self) {
        let Some(entry) = self.remote_entries.get(self.remote_selected) else {
            return;
        };
        if entry.is_dir {
            self.status_msg = "Cannot download a directory".to_string();
            return;
        }

        let alias = self.alias.clone();
        let remote_file = join_remote(&self.remote_path, &entry.name);
        let local_file = self.local.current_dir.join(&entry.name);

        let tx = self.tx.clone();
        self.status_msg = format!("Downloading {}…", entry.name);

        std::thread::spawn(move || {
            let result = get_file(&alias, &remote_file, &local_file).map_err(|e| e.to_string());
            let _ = tx.send(SftpMsg::Downloaded(result));
        });
    }

    /// Upload the selected local entry to the remote pane's current directory.
    pub fn upload_selected(&mut self) {
        let Some(entry) = self.local.selected_entry() else {
            return;
        };
        if entry.is_dir {
            self.status_msg = "Cannot upload a directory".to_string();
            return;
        }

        let alias = self.alias.clone();
        let local_path = entry.path.clone();
        let remote_file = join_remote(&self.remote_path, &entry.name);

        let tx = self.tx.clone();
        self.status_msg = format!("Uploading {}…", entry.name);

        std::thread::spawn(move || {
            let result = put_file(&alias, &local_path, &remote_file).map_err(|e| e.to_string());
            let _ = tx.send(SftpMsg::Uploaded(result));
        });
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }
}

/// Join a remote path segment, handling `.`/`..` reasonably without
/// depending on the remote filesystem's actual layout.
fn join_remote(base: &str, segment: &str) -> String {
    match segment {
        "." => base.to_string(),
        ".." => parent_remote(base),
        _ => {
            if base.is_empty() || base == "." {
                segment.to_string()
            } else if base.ends_with('/') {
                format!("{base}{segment}")
            } else {
                format!("{base}/{segment}")
            }
        }
    }
}

/// Pop the last path segment off `path`, without going above `.`/`/`.
fn parent_remote(path: &str) -> String {
    if path == "." || path == "/" || path.is_empty() {
        return ".".to_string();
    }
    let trimmed = path.trim_end_matches('/');
    match trimmed.rsplit_once('/') {
        Some(("", _)) => "/".to_string(),
        Some((parent, _)) => parent.to_string(),
        None => ".".to_string(),
    }
}

// ── Background sftp operations ──────────────────────────────────────────────

/// Run `ls -la <path>` on the remote host via an `sftp` batch script and
/// parse the resulting listing.
fn list_remote(alias: &str, path: &str) -> Result<Vec<SftpEntry>> {
    let script = format!("ls -la {path}\n");
    let output = run_sftp_batch(alias, &script)?;
    Ok(parse_sftp_ls_output(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

/// Download a remote file to a local path.
fn get_file(alias: &str, remote: &str, local: &Path) -> Result<()> {
    let script = format!("get \"{remote}\" \"{}\"\n", local.display());
    let output = run_sftp_batch(alias, &script)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("sftp get failed: {}", stderr.trim()));
    }
    Ok(())
}

/// Upload a local file to a remote path.
fn put_file(alias: &str, local: &Path, remote: &str) -> Result<()> {
    let script = format!("put \"{}\" \"{remote}\"\n", local.display());
    let output = run_sftp_batch(alias, &script)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("sftp put failed: {}", stderr.trim()));
    }
    Ok(())
}

/// Run the `sftp` CLI in batch mode, feeding `script` over stdin, and return
/// the captured process output.
fn run_sftp_batch(alias: &str, script: &str) -> Result<std::process::Output> {
    use std::io::Write;

    let mut child = Command::new("sftp")
        .args(["-b", "-", "-o", "BatchMode=yes", alias])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(script.as_bytes())?;
    }
    // Drop stdin (closing it) so `sftp` knows the batch script is complete.
    drop(child.stdin.take());

    Ok(child.wait_with_output()?)
}

/// Split a `ls -l`-style line into its first 8 whitespace-delimited fields
/// (perms, links, owner, group, size, month, day, time/year) plus the
/// remainder of the line (the filename, which may itself contain spaces).
/// Returns `None` if fewer than 8 fields are present. Whitespace runs
/// between the first 8 fields are collapsed; only the leading whitespace
/// before the filename is stripped — internal spaces in the filename are
/// preserved as-is.
fn split_ls_fields(line: &str) -> Option<(Vec<&str>, &str)> {
    let mut fields = Vec::with_capacity(8);
    let mut chars = line.char_indices().peekable();
    let mut pos = 0usize;

    for _ in 0..8 {
        // Skip whitespace.
        while let Some(&(i, c)) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
                pos = i + c.len_utf8();
            } else {
                break;
            }
        }
        let start = pos;
        // Consume non-whitespace.
        while let Some(&(i, c)) = chars.peek() {
            if !c.is_whitespace() {
                chars.next();
                pos = i + c.len_utf8();
            } else {
                break;
            }
        }
        if start == pos {
            return None;
        }
        fields.push(&line[start..pos]);
    }

    // Skip whitespace before the filename.
    while let Some(&(i, c)) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            pos = i + c.len_utf8();
        } else {
            break;
        }
    }

    if pos >= line.len() {
        return None;
    }

    Some((fields, &line[pos..]))
}

/// Pure parser for `sftp`/`ls -l`-style long-format directory listings.
/// Defensive: malformed or unexpected lines are silently skipped, never
/// causing a panic.
pub fn parse_sftp_ls_output(raw: &str) -> Vec<SftpEntry> {
    let mut entries = Vec::new();

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("sftp>") {
            continue;
        }

        let first_byte = line.as_bytes()[0];
        if first_byte != b'd' && first_byte != b'-' && first_byte != b'l' {
            continue;
        }

        // Fields: perms links owner group size month day time/year name...
        // `split_ls_fields` collapses runs of whitespace between the first 8
        // fields (perms, links, owner, group, size, month, day, time/year)
        // while leaving the filename — everything after — intact, including
        // any internal spaces it may contain.
        let Some((fields, name)) = split_ls_fields(line) else {
            continue;
        };

        let is_dir = first_byte == b'd';
        let size: u64 = fields[4].parse().unwrap_or(0);
        let name = name.trim();

        if name.is_empty() || name == "." || name == ".." {
            continue;
        }

        entries.push(SftpEntry {
            name: name.to_string(),
            is_dir,
            size,
        });
    }

    entries
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "\
sftp> ls -la .
drwxr-xr-x   3 user  group   4096 Jan  1 12:00 .
drwxr-xr-x   3 user  group   4096 Jan  1 12:00 ..
drwxr-xr-x   3 user  group   4096 Jan  1 12:00 some_dir
-rw-r--r--   1 user  group   1234 Feb 14 09:30 file.txt
-rw-r--r--   1 user  group     42 Mar  3 08:00 my file with spaces.txt
";

    #[test]
    fn parses_mixed_files_and_dirs() {
        let entries = parse_sftp_ls_output(FIXTURE);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"some_dir"));
        assert!(names.contains(&"file.txt"));
        let dir = entries.iter().find(|e| e.name == "some_dir").unwrap();
        assert!(dir.is_dir);
        let file = entries.iter().find(|e| e.name == "file.txt").unwrap();
        assert!(!file.is_dir);
        assert_eq!(file.size, 1234);
    }

    #[test]
    fn empty_string_yields_no_entries() {
        let entries = parse_sftp_ls_output("");
        assert!(entries.is_empty());
    }

    #[test]
    fn only_command_echo_yields_no_entries() {
        let input = "sftp> ls -la .\nsftp> \n";
        let entries = parse_sftp_ls_output(input);
        assert!(entries.is_empty());
    }

    #[test]
    fn filename_with_spaces_is_preserved() {
        let entries = parse_sftp_ls_output(FIXTURE);
        let f = entries
            .iter()
            .find(|e| e.name == "my file with spaces.txt")
            .expect("expected filename-with-spaces entry");
        assert_eq!(f.size, 42);
        assert!(!f.is_dir);
    }

    #[test]
    fn dot_and_dotdot_are_skipped() {
        let entries = parse_sftp_ls_output(FIXTURE);
        assert!(!entries.iter().any(|e| e.name == "." || e.name == ".."));
    }

    #[test]
    fn malformed_line_is_skipped_without_dropping_valid_entries() {
        let input = "\
drwxr-xr-x   3 user  group   4096 Jan  1 12:00 good_dir
not a valid listing line at all
-rw-r--r--   1 user  group   1234 Feb 14 09:30 good_file.txt
";
        let entries = parse_sftp_ls_output(input);
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|e| e.name == "good_dir"));
        assert!(entries.iter().any(|e| e.name == "good_file.txt"));
    }

    #[test]
    fn bad_size_field_defaults_to_zero_without_panic() {
        let input = "drwxr-xr-x   3 user  group   notanumber Jan  1 12:00 weird_dir\n";
        let entries = parse_sftp_ls_output(input);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].size, 0);
    }

    #[test]
    fn join_remote_handles_dot_and_dotdot() {
        assert_eq!(join_remote(".", "sub"), "sub");
        assert_eq!(join_remote("a/b", ".."), "a");
        assert_eq!(join_remote("a/b", "."), "a/b");
        assert_eq!(join_remote("a/b", "c"), "a/b/c");
    }

    #[test]
    fn parent_remote_does_not_go_above_root() {
        assert_eq!(parent_remote("."), ".");
        assert_eq!(parent_remote("/"), ".");
        assert_eq!(parent_remote("/home/user"), "/home");
        assert_eq!(parent_remote("a/b/c"), "a/b");
    }
}
