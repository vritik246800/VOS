use ratatui::widgets::ListState;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;

/// Messages sent from the background scan thread.
pub enum ScanMsg {
    #[allow(dead_code)]
    Entry {
        path: PathBuf,
        size: u64,
        is_dir: bool,
    },
    Done,
}

/// One visible row in the disk manager list.
#[derive(Debug, Clone)]
pub struct DiskEntry {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub is_dir: bool,
}

pub struct DiskManager {
    /// Root of the current scan.
    pub root: PathBuf,
    /// Directory currently being viewed (may be a sub-directory of root).
    pub current: PathBuf,
    /// Visible entries for the current level, sorted by size desc.
    pub entries: Vec<DiskEntry>,
    /// Whether a background scan is in progress.
    pub scanning: bool,
    /// Total size of the current directory.
    pub total: u64,
    /// Number of entries received during scan.
    pub scan_count: usize,
    /// Spinner frame index.
    pub spinner_tick: usize,
    /// List selection state.
    pub list_state: ListState,
    /// Raw accumulated sizes per path (populated during scan).
    sizes: HashMap<PathBuf, u64>,
    /// Channel for receiving scan messages.
    rx: Option<mpsc::Receiver<ScanMsg>>,
}

impl DiskManager {
    pub fn new() -> Self {
        let cwd = std::env::current_dir().unwrap_or_default();
        Self {
            root: cwd.clone(),
            current: cwd,
            entries: Vec::new(),
            scanning: false,
            total: 0,
            scan_count: 0,
            spinner_tick: 0,
            list_state: ListState::default(),
            sizes: HashMap::new(),
            rx: None,
        }
    }

    /// Start a background scan rooted at `path`.
    pub fn start_scan(&mut self, path: PathBuf) {
        self.root = path.clone();
        self.current = path.clone();
        self.entries.clear();
        self.sizes.clear();
        self.total = 0;
        self.scan_count = 0;
        self.scanning = true;
        self.list_state = ListState::default();

        let (tx, rx) = mpsc::channel::<ScanMsg>();
        self.rx = Some(rx);

        std::thread::spawn(move || {
            scan_dir_recursive(&path, &tx);
            let _ = tx.send(ScanMsg::Done);
        });
    }

    /// Drain pending messages from the scan thread.
    /// Returns true if entries changed (redraw needed).
    pub fn tick(&mut self) -> bool {
        if !self.scanning {
            return false;
        }

        self.spinner_tick = self.spinner_tick.wrapping_add(1);

        let Some(rx) = &self.rx else { return false };
        let mut changed = false;

        // Drain without blocking — collect up to 1000 messages per tick.
        for _ in 0..1000 {
            match rx.try_recv() {
                Ok(ScanMsg::Entry {
                    path,
                    size,
                    is_dir: _,
                }) => {
                    self.scan_count += 1;
                    changed = true;
                    // Accumulate the size to the direct child of `current` that
                    // contains this path (or the path itself if it IS a direct child).
                    if let Some(child_key) = direct_child_of(&self.current, &path) {
                        *self.sizes.entry(child_key).or_insert(0) += size;
                    }
                    // Always track the absolute size per path for sub-dir navigation.
                    *self.sizes.entry(path).or_insert(0) += size;
                }
                Ok(ScanMsg::Done) => {
                    self.scanning = false;
                    self.rx = None;
                    self.rebuild_entries();
                    return true;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.scanning = false;
                    self.rx = None;
                    self.rebuild_entries();
                    return true;
                }
            }
        }

        if changed {
            self.rebuild_entries();
        }
        changed
    }

    /// Rebuild `entries` from `sizes` for `current` directory.
    fn rebuild_entries(&mut self) {
        // Collect direct children of `current` that are in `sizes`.
        let mut entries: Vec<DiskEntry> = Vec::new();

        for (path, &size) in &self.sizes {
            // Only include direct children of `current`.
            if path.parent() == Some(self.current.as_path()) {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string());
                entries.push(DiskEntry {
                    name,
                    path: path.clone(),
                    size,
                    is_dir: path.is_dir(),
                });
            }
        }

        entries.sort_by_key(|e| std::cmp::Reverse(e.size));
        self.total = entries.iter().map(|e| e.size).sum();
        self.entries = entries;

        // Clamp selection.
        if self.entries.is_empty() {
            self.list_state.select(None);
        } else {
            let sel = self
                .list_state
                .selected()
                .unwrap_or(0)
                .min(self.entries.len() - 1);
            self.list_state.select(Some(sel));
        }
    }

    /// Enter a sub-directory by index (re-aggregate from existing data).
    pub fn enter(&mut self, idx: usize) {
        if let Some(entry) = self.entries.get(idx) {
            if entry.is_dir {
                self.current = entry.path.clone();
                self.list_state = ListState::default();
                self.rebuild_entries();
                if !self.entries.is_empty() {
                    self.list_state.select(Some(0));
                }
            }
        }
    }

    /// Navigate to parent directory (up to root).
    pub fn up(&mut self) {
        if self.current != self.root {
            if let Some(parent) = self.current.parent() {
                self.current = parent.to_path_buf();
                self.list_state = ListState::default();
                self.rebuild_entries();
                if !self.entries.is_empty() {
                    self.list_state.select(Some(0));
                }
            }
        }
    }

    pub fn move_up(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if i > 0 {
                self.list_state.select(Some(i - 1));
            }
        }
    }

    pub fn move_down(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if i + 1 < self.entries.len() {
                self.list_state.select(Some(i + 1));
            }
        }
    }

    pub fn selected_entry(&self) -> Option<&DiskEntry> {
        let idx = self.list_state.selected()?;
        self.entries.get(idx)
    }
}

impl Default for DiskManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Given a `current` dir and a `path` somewhere beneath it, return the direct child
/// of `current` that is the ancestor of `path`.
fn direct_child_of(current: &std::path::Path, path: &std::path::Path) -> Option<PathBuf> {
    // Strip `current` prefix to get the relative path.
    let rel = path.strip_prefix(current).ok()?;
    // The first component of the relative path is the direct child name.
    let first = rel.components().next()?;
    Some(current.join(first))
}

/// Recursively walk a directory, sending one `ScanMsg::Entry` per file/dir
/// encountered. Does NOT follow symlinks. Ignores permission errors.
fn scan_dir_recursive(dir: &std::path::Path, tx: &mpsc::Sender<ScanMsg>) {
    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };

    for entry_result in read_dir {
        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();

        // Do not follow symlinks — use symlink_metadata to detect them.
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        if meta.file_type().is_symlink() {
            continue;
        }

        if meta.is_dir() {
            // Send the directory itself with size 0 so it shows up in the list
            // even if it is empty.
            let _ = tx.send(ScanMsg::Entry {
                path: path.clone(),
                size: 0,
                is_dir: true,
            });
            scan_dir_recursive(&path, tx);
        } else if meta.is_file() {
            let size = meta.len();
            let _ = tx.send(ScanMsg::Entry {
                path,
                size,
                is_dir: false,
            });
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direct_child_of_immediate() {
        let current = std::path::Path::new("/home/user/docs");
        let path = std::path::Path::new("/home/user/docs/file.txt");
        let child = direct_child_of(current, path).unwrap();
        assert_eq!(child, std::path::PathBuf::from("/home/user/docs/file.txt"));
    }

    #[test]
    fn test_direct_child_of_nested() {
        let current = std::path::Path::new("/home/user/docs");
        let path = std::path::Path::new("/home/user/docs/subdir/deep/file.txt");
        let child = direct_child_of(current, path).unwrap();
        assert_eq!(child, std::path::PathBuf::from("/home/user/docs/subdir"));
    }

    #[test]
    fn test_direct_child_of_unrelated() {
        let current = std::path::Path::new("/home/user/docs");
        let path = std::path::Path::new("/tmp/other/file.txt");
        assert!(direct_child_of(current, path).is_none());
    }

    #[test]
    fn format_size_bytes() {
        use crate::fs::explorer::FileExplorer;
        assert_eq!(FileExplorer::format_size(512), "512 B");
    }

    #[test]
    fn format_size_kb() {
        use crate::fs::explorer::FileExplorer;
        assert_eq!(FileExplorer::format_size(2048), "2.0 KB");
    }

    #[test]
    fn format_size_mb() {
        use crate::fs::explorer::FileExplorer;
        assert_eq!(FileExplorer::format_size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn disk_manager_new_does_not_panic() {
        let dm = DiskManager::new();
        assert!(!dm.scanning);
        assert!(dm.entries.is_empty());
    }

    #[test]
    fn disk_manager_move_empty() {
        let mut dm = DiskManager::new();
        // Should not panic on empty entries.
        dm.move_up();
        dm.move_down();
        assert_eq!(dm.list_state.selected(), None);
    }

    #[test]
    fn disk_manager_enter_invalid_index() {
        let mut dm = DiskManager::new();
        // No entries — should not panic.
        dm.enter(0);
        dm.enter(99);
    }

    #[test]
    fn rebuild_entries_does_not_panic_on_fake_paths() {
        // Fake paths will have no parent match so entries remain empty — no crash.
        let mut dm = DiskManager::new();
        let base = std::path::PathBuf::from("/fake/base");
        dm.current = base.clone();
        dm.sizes.insert(base.join("a.txt"), 1000);
        dm.sizes.insert(base.join("b.txt"), 5000);
        // rebuild_entries calls path.is_dir() — false for fake paths; no crash expected.
        dm.rebuild_entries();
        // Entries may or may not be populated depending on the OS, but no panic.
    }
}
