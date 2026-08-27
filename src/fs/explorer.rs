use anyhow::Result;
use ratatui::widgets::ListState;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum ClipboardOp {
    Copy,
    Cut,
}

#[derive(Debug, Clone)]
pub struct ClipboardEntry {
    pub path: PathBuf,
    pub op: ClipboardOp,
}

#[derive(Debug, Clone)]
pub struct EntryInfo {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
}

pub struct FileExplorer {
    pub current_dir: PathBuf,
    pub entries: Vec<EntryInfo>,
    pub list_state: ListState,
    pub show_hidden: bool,
}

impl FileExplorer {
    pub fn new(start: PathBuf) -> Result<Self> {
        let mut fe = Self {
            current_dir: start,
            entries: Vec::new(),
            list_state: ListState::default(),
            show_hidden: false,
        };
        fe.load_entries()?;
        Ok(fe)
    }

    pub fn load_entries(&mut self) -> Result<()> {
        let mut entries: Vec<EntryInfo> = std::fs::read_dir(&self.current_dir)?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if !self.show_hidden && name.starts_with('.') {
                    return None;
                }
                let meta = e.metadata().ok()?;
                Some(EntryInfo {
                    name,
                    path: e.path(),
                    is_dir: meta.is_dir(),
                    size: meta.len(),
                })
            })
            .collect();

        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        self.entries = entries;
        if self.list_state.selected().is_none() && !self.entries.is_empty() {
            self.list_state.select(Some(0));
        }
        Ok(())
    }

    pub fn selected_entry(&self) -> Option<&EntryInfo> {
        self.list_state.selected().and_then(|i| self.entries.get(i))
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

    pub fn page_up(&mut self, page: usize) {
        if let Some(i) = self.list_state.selected() {
            self.list_state.select(Some(i.saturating_sub(page)));
        }
    }

    pub fn page_down(&mut self, page: usize) {
        if let Some(i) = self.list_state.selected() {
            let max = self.entries.len().saturating_sub(1);
            self.list_state.select(Some((i + page).min(max)));
        }
    }

    pub fn go_parent(&mut self) -> Result<()> {
        if let Some(parent) = self.current_dir.parent().map(|p| p.to_path_buf()) {
            self.current_dir = parent;
            self.list_state.select(Some(0));
            self.load_entries()?;
        }
        Ok(())
    }

    pub fn enter_dir(&mut self, path: PathBuf) -> Result<()> {
        self.current_dir = path;
        self.list_state.select(Some(0));
        self.load_entries()
    }

    pub fn toggle_hidden(&mut self) -> Result<()> {
        self.show_hidden = !self.show_hidden;
        self.load_entries()
    }

    pub fn has_git(&self) -> bool {
        self.current_dir.join(".git").exists()
    }

    #[allow(dead_code)]
    pub fn format_size(bytes: u64) -> String {
        if bytes < 1024 {
            format!("{bytes} B")
        } else if bytes < 1024 * 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else if bytes < 1024 * 1024 * 1024 {
            format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        }
    }
}

pub fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dst_child = dst.join(entry.file_name());
        if entry.metadata()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_child)?;
        } else {
            std::fs::copy(entry.path(), dst_child)?;
        }
    }
    Ok(())
}
