//! Man Page Viewer module — load, parse and search Unix manual pages.
//! No ratatui UI in this file; all rendering is in `ui/man_panel.rs`.

use anyhow::Result;

// ── Public types ──────────────────────────────────────────────────────────────

/// A single parsed line from a man page, composed of styled spans.
#[derive(Debug, Clone)]
pub struct ManLine {
    pub spans: Vec<ManSpan>,
}

/// A run of text with a single style.
#[derive(Debug, Clone)]
pub struct ManSpan {
    pub text: String,
    pub style: ManStyle,
}

/// Text decoration derived from backspace-overstrike conventions.
#[derive(Debug, Clone, PartialEq)]
pub enum ManStyle {
    Normal,
    /// `c\bc` — same char repeated after backspace → bold.
    Bold,
    /// `_\bc` — underscore followed by backspace + char → underline.
    Underline,
    /// Section headers (all-caps lines); rendered slightly dimmer.
    Dim,
}

// ── Main state struct ─────────────────────────────────────────────────────────

pub struct ManViewer {
    /// Parsed content of the currently-loaded page.
    pub lines: Vec<ManLine>,
    /// First visible line (0-based).
    pub scroll: usize,
    /// Name of the page that is currently loaded (e.g. `"ls"`).
    pub page_name: String,
    /// Active search query.
    pub search_query: String,
    /// Indices into `lines` that contain `search_query`.
    pub search_matches: Vec<usize>,
    /// Which element of `search_matches` is currently highlighted.
    pub search_idx: usize,
    /// True while a background load is in progress (reserved for future async use).
    pub loading: bool,
    /// Populated when the last `load_page` call failed.
    pub error: Option<String>,
}

impl ManViewer {
    /// Create an empty viewer — no page loaded, no error.
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            scroll: 0,
            page_name: String::new(),
            search_query: String::new(),
            search_matches: Vec::new(),
            search_idx: 0,
            loading: false,
            error: None,
        }
    }

    // ── Page loading ──────────────────────────────────────────────────────────

    /// Load and parse `page`.  On success, `self.lines` is populated and any
    /// previous error is cleared.  On failure (man returns non-zero), the
    /// stderr is stored in `self.error` and the function still returns `Ok(())`.
    pub fn load_page(&mut self, page: &str) -> Result<()> {
        self.lines.clear();
        self.scroll = 0;
        self.search_query.clear();
        self.search_matches.clear();
        self.search_idx = 0;
        self.error = None;
        self.loading = false;

        let output = std::process::Command::new("man")
            .args(["-P", "cat", page])
            .env("MANPAGER", "cat")
            .env("MANWIDTH", "80")
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let msg = if !stderr.is_empty() { stderr } else { stdout };
            self.error = Some(if msg.is_empty() {
                format!("No manual entry for '{page}'")
            } else {
                msg
            });
            return Ok(());
        }

        let raw = String::from_utf8_lossy(&output.stdout);
        self.lines = raw.lines().map(parse_man_line).collect();
        self.page_name = page.to_string();
        Ok(())
    }

    // ── Scroll helpers ────────────────────────────────────────────────────────

    pub fn scroll_up(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    pub fn scroll_down(&mut self, n: usize) {
        let max = self.lines.len().saturating_sub(1);
        self.scroll = (self.scroll + n).min(max);
    }

    pub fn scroll_page_up(&mut self, height: u16) {
        self.scroll_up(height as usize);
    }

    pub fn scroll_page_down(&mut self, height: u16) {
        self.scroll_down(height as usize);
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll = 0;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll = self.lines.len().saturating_sub(1);
    }

    // ── Search ────────────────────────────────────────────────────────────────

    /// Find all lines that contain `q` (case-insensitive) and populate
    /// `search_matches`.  Scrolls to the first match if any.
    pub fn search(&mut self, q: &str) {
        self.search_query = q.to_string();
        self.search_matches.clear();
        self.search_idx = 0;

        if q.is_empty() {
            return;
        }

        let q_lower = q.to_lowercase();
        for (i, line) in self.lines.iter().enumerate() {
            let text: String = line.spans.iter().map(|s| s.text.as_str()).collect();
            if text.to_lowercase().contains(&q_lower) {
                self.search_matches.push(i);
            }
        }

        if let Some(&first) = self.search_matches.first() {
            self.scroll = first;
        }
    }

    /// Advance to the next search match, wrapping around.
    pub fn next_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.search_idx = (self.search_idx + 1) % self.search_matches.len();
        self.scroll = self.search_matches[self.search_idx];
    }

    /// Go back to the previous search match, wrapping around.
    pub fn prev_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        if self.search_idx == 0 {
            self.search_idx = self.search_matches.len() - 1;
        } else {
            self.search_idx -= 1;
        }
        self.scroll = self.search_matches[self.search_idx];
    }
}

// ── Apropos helper ────────────────────────────────────────────────────────────

/// Run `man -k <query>` and return a list of page names (without section).
///
/// Each output line looks like:  `ls (1)               - list directory contents`
/// We return just the names, e.g. `["ls"]`.
pub fn man_apropos(query: &str) -> Vec<String> {
    let output = match std::process::Command::new("man")
        .args(["-k", query])
        .output()
    {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            // Expected format: `name (section) - description`
            let name = line.split_whitespace().next()?;
            Some(name.to_string())
        })
        .collect()
}

// ── Backspace-overstrike parser ───────────────────────────────────────────────

/// Parse a single raw man-page line, interpreting backspace overstrike sequences
/// into styled spans:
///
/// - `c BS c`  → Bold (same char repeated)
/// - `_ BS c`  → Underline
/// - anything else passes through as Normal
fn parse_man_line(raw: &str) -> ManLine {
    let chars: Vec<char> = raw.chars().collect();

    // Build a flat list of (char, ManStyle) then merge adjacent same-style runs.
    let mut styled: Vec<(char, ManStyle)> = Vec::with_capacity(chars.len());
    let mut i = 0usize;

    while i < chars.len() {
        let c = chars[i];
        // Look ahead for BS pattern: chars[i], chars[i+1] == '\x08', chars[i+2]
        if i + 2 < chars.len() && chars[i + 1] == '\x08' {
            let after = chars[i + 2];
            if c == '_' {
                // Underline: `_ BS char`
                styled.push((after, ManStyle::Underline));
                i += 3;
                continue;
            } else if c == after {
                // Bold: `c BS c`
                styled.push((c, ManStyle::Bold));
                i += 3;
                continue;
            }
        }
        // Skip lone backspace control characters
        if c == '\x08' {
            i += 1;
            continue;
        }
        styled.push((c, ManStyle::Normal));
        i += 1;
    }

    // Determine if this looks like a section header (all-caps words).
    let plain: String = styled.iter().map(|(c, _)| *c).collect();
    let trimmed = plain.trim();
    let is_section_header = !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|c| c.is_uppercase() || c.is_ascii_punctuation() || c == ' ')
        && trimmed.chars().any(|c| c.is_alphabetic())
        && trimmed.len() >= 2;

    // Merge adjacent same-style characters into spans.
    let mut spans: Vec<ManSpan> = Vec::new();
    let mut current_text = String::new();
    let mut current_style: Option<ManStyle> = None;

    for (ch, style) in &styled {
        let effective_style = if is_section_header && *style == ManStyle::Normal {
            ManStyle::Dim
        } else {
            style.clone()
        };

        match &current_style {
            None => {
                current_style = Some(effective_style);
                current_text.push(*ch);
            }
            Some(s) if *s == effective_style => {
                current_text.push(*ch);
            }
            Some(_) => {
                spans.push(ManSpan {
                    text: std::mem::take(&mut current_text),
                    style: current_style.take().unwrap(),
                });
                current_style = Some(effective_style);
                current_text.push(*ch);
            }
        }
    }

    if !current_text.is_empty() {
        spans.push(ManSpan {
            text: current_text,
            style: current_style.unwrap_or(ManStyle::Normal),
        });
    }

    // An empty line should still produce a single empty span so the renderer
    // can draw a blank line.
    if spans.is_empty() {
        spans.push(ManSpan {
            text: String::new(),
            style: ManStyle::Normal,
        });
    }

    ManLine { spans }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(line: &ManLine) -> String {
        line.spans.iter().map(|s| s.text.as_str()).collect()
    }

    fn styles(line: &ManLine) -> Vec<ManStyle> {
        line.spans.iter().map(|s| s.style.clone()).collect()
    }

    #[test]
    fn test_plain_line() {
        let line = parse_man_line("hello world");
        assert_eq!(plain(&line), "hello world");
        assert!(line.spans.iter().all(|s| s.style == ManStyle::Normal));
    }

    #[test]
    fn test_bold_overstrike() {
        // "l\x08l" → Bold 'l'
        let raw = "l\x08l";
        let line = parse_man_line(raw);
        assert_eq!(plain(&line), "l");
        assert_eq!(styles(&line), vec![ManStyle::Bold]);
    }

    #[test]
    fn test_underline_overstrike() {
        // "_\x08c" → Underline 'c'
        let raw = "_\x08c";
        let line = parse_man_line(raw);
        assert_eq!(plain(&line), "c");
        assert_eq!(styles(&line), vec![ManStyle::Underline]);
    }

    #[test]
    fn test_mixed_bold_and_normal() {
        // "h\x08he" → Bold 'h' + Normal 'e'
        let raw = "h\x08he";
        let line = parse_man_line(raw);
        assert_eq!(plain(&line), "he");
        assert_eq!(line.spans[0].style, ManStyle::Bold);
        assert_eq!(line.spans[0].text, "h");
        assert_eq!(line.spans[1].text, "e");
    }

    #[test]
    fn test_mixed_underline_and_normal() {
        // "_\x08xfoo" → Underline 'x' + Normal "foo"
        let raw = "_\x08xfoo";
        let line = parse_man_line(raw);
        assert_eq!(plain(&line), "xfoo");
        assert_eq!(line.spans[0].style, ManStyle::Underline);
    }

    #[test]
    fn test_empty_line() {
        let line = parse_man_line("");
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].text, "");
    }

    #[test]
    fn test_section_header() {
        let line = parse_man_line("DESCRIPTION");
        // All-caps → should be Dim
        assert!(line.spans.iter().any(|s| s.style == ManStyle::Dim));
    }

    #[test]
    fn test_repeated_bold_word() {
        // Simulate "ls" in bold: l\x08l s\x08s
        let raw = "l\x08ls\x08s";
        let line = parse_man_line(raw);
        assert_eq!(plain(&line), "ls");
        assert!(line.spans.iter().all(|s| s.style == ManStyle::Bold));
    }
}
