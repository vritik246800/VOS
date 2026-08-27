use anyhow::Result;
use std::path::{Path, PathBuf};

pub struct Buffer {
    pub lines: Vec<String>,
    pub cursor: (usize, usize),
    pub path: Option<PathBuf>,
    pub is_modified: bool,
    pub undo_stack: Vec<BufferSnapshot>,
    pub redo_stack: Vec<BufferSnapshot>,
    pub md_view: bool,
    pub md_scroll: usize,
}

#[derive(Clone)]
pub struct BufferSnapshot {
    pub lines: Vec<String>,
    pub cursor: (usize, usize),
}

impl Buffer {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor: (0, 0),
            path: None,
            is_modified: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            md_view: false,
            md_scroll: 0,
        }
    }

    pub fn load_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let lines: Vec<String> = if content.is_empty() {
            vec![String::new()]
        } else {
            content.lines().map(|l| l.to_string()).collect()
        };
        let is_md = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| matches!(e.to_lowercase().as_str(), "md" | "markdown"))
            .unwrap_or(false);
        Ok(Self {
            lines,
            cursor: (0, 0),
            path: Some(path.to_path_buf()),
            is_modified: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            md_view: is_md,
            md_scroll: 0,
        })
    }

    pub fn is_markdown(&self) -> bool {
        self.path
            .as_ref()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .map(|e| matches!(e.to_lowercase().as_str(), "md" | "markdown"))
            .unwrap_or(false)
    }

    pub fn save_file(&mut self) -> Result<()> {
        if let Some(path) = &self.path {
            let content = self.lines.join("\n");
            std::fs::write(path, content)?;
            self.is_modified = false;
        }
        Ok(())
    }

    fn snapshot(&self) -> BufferSnapshot {
        BufferSnapshot {
            lines: self.lines.clone(),
            cursor: self.cursor,
        }
    }

    fn push_undo(&mut self) {
        let snap = self.snapshot();
        self.undo_stack.push(snap);
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) {
        if let Some(snap) = self.undo_stack.pop() {
            self.redo_stack.push(self.snapshot());
            self.lines = snap.lines;
            self.cursor = snap.cursor;
            self.is_modified = true;
        }
    }

    pub fn redo(&mut self) {
        if let Some(snap) = self.redo_stack.pop() {
            self.undo_stack.push(self.snapshot());
            self.lines = snap.lines;
            self.cursor = snap.cursor;
            self.is_modified = true;
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.push_undo();
        let (row, col) = self.cursor;
        let line = &mut self.lines[row];
        let col = col.min(line.len());
        line.insert(col, c);
        self.cursor.1 = col + 1;
        self.is_modified = true;
    }

    pub fn delete_char_before(&mut self) {
        let (row, col) = self.cursor;
        if col > 0 {
            self.push_undo();
            let line = &mut self.lines[row];
            let col = col.min(line.len());
            line.remove(col - 1);
            self.cursor.1 = col - 1;
            self.is_modified = true;
        } else if row > 0 {
            self.push_undo();
            let line = self.lines.remove(row);
            let prev_len = self.lines[row - 1].len();
            self.lines[row - 1].push_str(&line);
            self.cursor = (row - 1, prev_len);
            self.is_modified = true;
        }
    }

    pub fn insert_newline(&mut self) {
        self.push_undo();
        let (row, col) = self.cursor;
        let col = col.min(self.lines[row].len());
        let rest = self.lines[row].split_off(col);
        self.lines.insert(row + 1, rest);
        self.cursor = (row + 1, 0);
        self.is_modified = true;
    }

    pub fn move_up(&mut self) {
        if self.cursor.0 > 0 {
            self.cursor.0 -= 1;
            self.clamp_col();
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor.0 + 1 < self.lines.len() {
            self.cursor.0 += 1;
            self.clamp_col();
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor.1 > 0 {
            self.cursor.1 -= 1;
        } else if self.cursor.0 > 0 {
            self.cursor.0 -= 1;
            self.cursor.1 = self.lines[self.cursor.0].len();
        }
    }

    pub fn move_right(&mut self) {
        let row_len = self.lines[self.cursor.0].len();
        if self.cursor.1 < row_len {
            self.cursor.1 += 1;
        } else if self.cursor.0 + 1 < self.lines.len() {
            self.cursor.0 += 1;
            self.cursor.1 = 0;
        }
    }

    pub fn move_home(&mut self) {
        self.cursor.1 = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor.1 = self.lines[self.cursor.0].len();
    }

    pub fn page_up(&mut self, lines: usize) {
        self.cursor.0 = self.cursor.0.saturating_sub(lines);
        self.clamp_col();
    }

    pub fn page_down(&mut self, lines: usize) {
        self.cursor.0 = (self.cursor.0 + lines).min(self.lines.len().saturating_sub(1));
        self.clamp_col();
    }

    fn clamp_col(&mut self) {
        let row_len = self.lines[self.cursor.0].len();
        self.cursor.1 = self.cursor.1.min(row_len);
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_redo() {
        let mut b = Buffer::new();
        b.insert_char('a');
        b.insert_char('b');
        assert_eq!(b.lines[0], "ab");
        b.undo();
        assert_eq!(b.lines[0], "a");
        b.redo();
        assert_eq!(b.lines[0], "ab");
    }

    #[test]
    fn home_end() {
        let mut b = Buffer::new();
        b.insert_char('a');
        b.insert_char('b');
        b.move_home();
        assert_eq!(b.cursor.1, 0);
        b.move_end();
        assert_eq!(b.cursor.1, 2);
    }
}
