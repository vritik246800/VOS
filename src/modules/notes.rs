//! Notes module (Phase 6.1): markdown notes in a configurable folder,
//! indexed in `app.db`. The module is mostly *glue*: it scans the notes
//! directory, extracts a title/tags from each note's frontmatter and offers a
//! list + live markdown preview. Editing reuses the existing editor; preview
//! reuses `ui/md_panel::parse_markdown`.

use ratatui::widgets::ListState;
use std::path::{Path, PathBuf};

/// One indexed note (filesystem-backed).
#[derive(Debug, Clone)]
pub struct NoteEntry {
    pub path: PathBuf,
    pub title: String,
    pub tags: Vec<String>,
    /// Last-modified time as unix seconds (used for newest-first sorting).
    pub modified: u64,
}

pub struct NotesManager {
    /// Notes directory (default `~/Notes`, created on demand).
    pub dir: PathBuf,
    pub notes: Vec<NoteEntry>,
    pub list_state: ListState,
    /// Raw lines of the currently selected note (cached for preview render).
    pub preview: Vec<String>,
    pub preview_scroll: usize,
    /// True while a "new note" title dialog is open (so the generic dialog
    /// handler knows to route Enter to `App::create_note`).
    pub creating: bool,
}

impl NotesManager {
    pub fn new(dir: PathBuf) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            dir,
            notes: Vec::new(),
            list_state,
            preview: Vec::new(),
            preview_scroll: 0,
            creating: false,
        }
    }

    /// Create the notes directory if it does not exist yet (on-demand).
    pub fn ensure_dir(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)
    }

    /// Scan the notes directory for markdown files and rebuild the index.
    /// Newest-modified notes come first. Refreshes the preview afterwards.
    pub fn scan(&mut self) {
        let mut notes = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !is_markdown(&path) {
                    continue;
                }
                let content = std::fs::read_to_string(&path).unwrap_or_default();
                let (title, tags) = parse_note_meta(&content, &path);
                let modified = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                notes.push(NoteEntry {
                    path,
                    title,
                    tags,
                    modified,
                });
            }
        }
        notes.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.title.cmp(&b.title)));
        self.notes = notes;
        let sel = if self.notes.is_empty() {
            None
        } else {
            Some(
                self.list_state
                    .selected()
                    .unwrap_or(0)
                    .min(self.notes.len() - 1),
            )
        };
        self.list_state.select(sel);
        self.refresh_preview();
    }

    pub fn selected(&self) -> Option<&NoteEntry> {
        self.list_state.selected().and_then(|i| self.notes.get(i))
    }

    pub fn move_up(&mut self) {
        if self.notes.is_empty() {
            return;
        }
        let i = self.list_state.selected().unwrap_or(0);
        self.list_state.select(Some(i.saturating_sub(1)));
        self.refresh_preview();
    }

    pub fn move_down(&mut self) {
        if self.notes.is_empty() {
            return;
        }
        let i = self.list_state.selected().unwrap_or(0);
        self.list_state
            .select(Some((i + 1).min(self.notes.len() - 1)));
        self.refresh_preview();
    }

    pub fn scroll_up(&mut self) {
        self.preview_scroll = self.preview_scroll.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.preview_scroll = self
            .preview_scroll
            .saturating_add(1)
            .min(self.preview.len().saturating_sub(1));
    }

    /// Re-read the selected note's content into the preview cache.
    pub fn refresh_preview(&mut self) {
        self.preview_scroll = 0;
        self.preview = match self.selected() {
            Some(n) => std::fs::read_to_string(&n.path)
                .unwrap_or_default()
                .lines()
                .map(|l| l.to_string())
                .collect(),
            None => Vec::new(),
        };
    }

    pub fn preview_content(&self) -> String {
        self.preview.join("\n")
    }

    /// Create a new note from a title; writes `<slug>.md` with minimal
    /// frontmatter, rescans, selects it and returns its path.
    pub fn create_note(&mut self, title: &str) -> std::io::Result<PathBuf> {
        self.ensure_dir()?;
        let title = title.trim();
        let title = if title.is_empty() { "Untitled" } else { title };
        let mut slug = slugify(title);
        if slug.is_empty() {
            slug = "untitled".to_string();
        }
        let mut path = self.dir.join(format!("{slug}.md"));
        let mut n = 1;
        while path.exists() {
            path = self.dir.join(format!("{slug}-{n}.md"));
            n += 1;
        }
        let body = format!("---\ntitle: {title}\ntags: []\n---\n\n# {title}\n\n");
        std::fs::write(&path, body)?;
        self.scan();
        // Select the freshly created note.
        if let Some(idx) = self.notes.iter().position(|nt| nt.path == path) {
            self.list_state.select(Some(idx));
            self.refresh_preview();
        }
        Ok(path)
    }

    /// First embedded image reference (`![alt](path)`) in the selected note,
    /// resolved relative to the notes directory.
    pub fn first_image(&self) -> Option<PathBuf> {
        for line in &self.preview {
            if let Some(p) = extract_image_path(line) {
                let pb = PathBuf::from(&p);
                return Some(if pb.is_absolute() {
                    pb
                } else {
                    self.dir.join(pb)
                });
            }
        }
        None
    }
}

// ── Pure helpers (testable) ────────────────────────────────────────────────

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_lowercase().as_str(), "md" | "markdown"))
        .unwrap_or(false)
}

/// Extract (title, tags) from note content. Looks at YAML-ish frontmatter
/// (`title:` / `tags:`), then the first `# heading`, falling back to the
/// filename stem.
pub fn parse_note_meta(content: &str, path: &Path) -> (String, Vec<String>) {
    let mut title: Option<String> = None;
    let mut tags: Vec<String> = Vec::new();

    let mut lines = content.lines();
    if lines.next().map(|l| l.trim()) == Some("---") {
        for line in lines.by_ref() {
            let t = line.trim();
            if t == "---" {
                break;
            }
            if let Some(v) = t.strip_prefix("title:") {
                title = Some(v.trim().trim_matches('"').to_string());
            } else if let Some(v) = t.strip_prefix("tags:") {
                tags = parse_tags(v.trim());
            }
        }
    }

    if title.as_deref().map(str::is_empty).unwrap_or(true) {
        title = content
            .lines()
            .find_map(|l| l.trim().strip_prefix("# ").map(|h| h.trim().to_string()));
    }

    let title = title.filter(|t| !t.is_empty()).unwrap_or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string()
    });
    (title, tags)
}

fn parse_tags(s: &str) -> Vec<String> {
    let inner = s
        .strip_prefix('[')
        .and_then(|x| x.strip_suffix(']'))
        .unwrap_or(s);
    inner
        .split(',')
        .map(|t| t.trim().trim_matches('"').trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// Turn an arbitrary title into a filesystem-safe slug.
pub fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// Extract the path from the first `![alt](path)` markdown image on a line.
fn extract_image_path(line: &str) -> Option<String> {
    let start = line.find("![")?;
    let rest = &line[start..];
    let paren = rest.find("](")?;
    let after = &rest[paren + 2..];
    let end = after.find(')')?;
    // Drop an optional title: ![alt](path "title")
    let path = after[..end].split_whitespace().next().unwrap_or("").trim();
    if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("  My Note!! "), "my-note");
        assert_eq!(slugify("Já chovê? (sim)"), "j-chov-sim");
        assert_eq!(slugify("---"), "");
    }

    #[test]
    fn frontmatter_title_and_tags() {
        let content = "---\ntitle: My Title\ntags: [rust, tui]\n---\n\n# Heading\nbody";
        let (title, tags) = parse_note_meta(content, Path::new("/x/note.md"));
        assert_eq!(title, "My Title");
        assert_eq!(tags, vec!["rust".to_string(), "tui".to_string()]);
    }

    #[test]
    fn falls_back_to_heading_then_filename() {
        let from_heading = "# A Heading\n\nsome text";
        let (t, _) = parse_note_meta(from_heading, Path::new("/x/note.md"));
        assert_eq!(t, "A Heading");

        let plain = "just text, no heading";
        let (t2, _) = parse_note_meta(plain, Path::new("/x/my-file.md"));
        assert_eq!(t2, "my-file");
    }

    #[test]
    fn extract_image() {
        assert_eq!(
            extract_image_path("see ![alt](img/pic.png) here"),
            Some("img/pic.png".to_string())
        );
        assert_eq!(
            extract_image_path("![a](photo.jpg \"t\")"),
            Some("photo.jpg".to_string())
        );
        assert_eq!(extract_image_path("no image here"), None);
    }
}
