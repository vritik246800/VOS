use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

#[derive(Debug, Clone)]
pub struct PaletteItem {
    pub label: String,
    pub description: String,
    pub action: PaletteAction,
}

#[derive(Debug, Clone)]
pub enum PaletteAction {
    OpenModule(String),
    RunCommand(String),
    #[allow(dead_code)]
    OpenFile(String),
    SetTheme(String),
}

pub struct CommandPalette {
    pub query: String,
    pub items: Vec<PaletteItem>,
    pub filtered: Vec<(i64, usize)>,
    pub list_state: ListState,
    /// Number of static (built-in) items; everything past this is dynamic
    /// (e.g. note entries injected at runtime).
    base_len: usize,
    matcher: SkimMatcherV2,
}

impl CommandPalette {
    pub fn new() -> Self {
        let items = default_items();
        let len = items.len();
        let mut cp = Self {
            query: String::new(),
            items,
            filtered: (0..len).map(|i| (0, i)).collect(),
            list_state: ListState::default(),
            base_len: len,
            matcher: SkimMatcherV2::default(),
        };
        if !cp.filtered.is_empty() {
            cp.list_state.select(Some(0));
        }
        cp
    }

    /// Replace the dynamic note items (kept after the static `base_len` items).
    /// Each note becomes a fuzzy-searchable `note: <title>` entry that opens the
    /// file when selected.
    pub fn set_note_items(&mut self, notes: Vec<(String, String)>) {
        self.items.truncate(self.base_len);
        for (title, path) in notes {
            self.items.push(PaletteItem {
                label: format!("note: {title}"),
                description: path.clone(),
                action: PaletteAction::OpenFile(path),
            });
        }
    }

    pub fn reset(&mut self) {
        self.query.clear();
        self.refresh_filter();
    }

    pub fn push_char(&mut self, c: char) {
        self.query.push(c);
        self.refresh_filter();
    }

    pub fn pop_char(&mut self) {
        self.query.pop();
        self.refresh_filter();
    }

    fn refresh_filter(&mut self) {
        if self.query.is_empty() {
            self.filtered = self.items.iter().enumerate().map(|(i, _)| (0, i)).collect();
        } else {
            let mut matched: Vec<(i64, usize)> = self
                .items
                .iter()
                .enumerate()
                .filter_map(|(i, item)| {
                    self.matcher
                        .fuzzy_match(&item.label, &self.query)
                        .map(|score| (score, i))
                })
                .collect();
            matched.sort_by(|a, b| b.0.cmp(&a.0));
            self.filtered = matched;
        }
        self.list_state.select(if self.filtered.is_empty() {
            None
        } else {
            Some(0)
        });
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
            if i + 1 < self.filtered.len() {
                self.list_state.select(Some(i + 1));
            }
        }
    }

    pub fn selected_action(&self) -> Option<&PaletteAction> {
        let idx = self.list_state.selected()?;
        let (_, item_idx) = self.filtered.get(idx)?;
        self.items.get(*item_idx).map(|it| &it.action)
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let w = (area.width * 2 / 3)
            .max(50)
            .min(area.width.saturating_sub(4));
        let h = (self.filtered.len() as u16 + 4)
            .min(20)
            .max(6)
            .min(area.height.saturating_sub(4));
        let x = (area.width.saturating_sub(w)) / 2;
        let y = area.height / 6;
        let popup = Rect::new(x, y, w, h);

        f.render_widget(Clear, popup);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(popup);

        let input = Paragraph::new(format!(" {} ", self.query))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Command Palette (Ctrl+P) ")
                    .border_style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
            )
            .style(Style::default().fg(Color::White));
        f.render_widget(input, chunks[0]);

        if self.filtered.is_empty() {
            let msg = Paragraph::new(" No results.").block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
            f.render_widget(msg, chunks[1]);
            return;
        }

        let items: Vec<ListItem> = self
            .filtered
            .iter()
            .filter_map(|(_, idx)| {
                self.items.get(*idx).map(|item| {
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!(" {:<30}", item.label),
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("  {}", item.description),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]))
                })
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");
        f.render_stateful_widget(list, chunks[1], &mut self.list_state);
    }
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

fn default_items() -> Vec<PaletteItem> {
    vec![
        PaletteItem {
            label: "Favorites".into(),
            description: "Bookmarked files & folders".into(),
            action: PaletteAction::OpenModule("Favorites".into()),
        },
        PaletteItem {
            label: "File Manager".into(),
            description: "Browse and manage files".into(),
            action: PaletteAction::OpenModule("FileManager".into()),
        },
        PaletteItem {
            label: "Text Editor".into(),
            description: "Edit files".into(),
            action: PaletteAction::OpenModule("Editor".into()),
        },
        PaletteItem {
            label: "Terminal".into(),
            description: "Integrated async shell".into(),
            action: PaletteAction::OpenModule("Terminal".into()),
        },
        PaletteItem {
            label: "Monitor".into(),
            description: "CPU/RAM/Disk/Net + processes".into(),
            action: PaletteAction::OpenModule("ProcessViewer".into()),
        },
        PaletteItem {
            label: "Git".into(),
            description: "Git panel".into(),
            action: PaletteAction::OpenModule("Git".into()),
        },
        PaletteItem {
            label: "Audio Player".into(),
            description: "Play audio files".into(),
            action: PaletteAction::OpenModule("AudioPlayer".into()),
        },
        PaletteItem {
            label: "Video Player".into(),
            description: "Play video files".into(),
            action: PaletteAction::OpenModule("VideoPlayer".into()),
        },
        PaletteItem {
            label: "Help".into(),
            description: "Keyboard shortcuts and docs".into(),
            action: PaletteAction::OpenModule("Help".into()),
        },
        PaletteItem {
            label: "Theme Switcher".into(),
            description: "Browse and switch themes".into(),
            action: PaletteAction::OpenModule("ThemeSwitcher".into()),
        },
        PaletteItem {
            label: "Theme: Dark".into(),
            description: "Dark theme".into(),
            action: PaletteAction::SetTheme("dark".into()),
        },
        PaletteItem {
            label: "Theme: Light".into(),
            description: "Light theme".into(),
            action: PaletteAction::SetTheme("light".into()),
        },
        PaletteItem {
            label: "Split Horizontal".into(),
            description: "Split panel horizontally".into(),
            action: PaletteAction::RunCommand("split h".into()),
        },
        PaletteItem {
            label: "Split Vertical".into(),
            description: "Split panel vertically".into(),
            action: PaletteAction::RunCommand("split v".into()),
        },
        // Phase 2 modules
        PaletteItem {
            label: "Log Viewer".into(),
            description: "Live log tail with filters".into(),
            action: PaletteAction::OpenModule("LogViewer".into()),
        },
        PaletteItem {
            label: "Service Manager".into(),
            description: "systemd services (Linux) or launchctl (macOS)".into(),
            action: PaletteAction::OpenModule("ServiceManager".into()),
        },
        PaletteItem {
            label: "Network".into(),
            description: "Network interfaces, IPs and ping".into(),
            action: PaletteAction::OpenModule("NetworkPanel".into()),
        },
        PaletteItem {
            label: "Disk Usage".into(),
            description: "Disk usage analyser (ncdu-style)".into(),
            action: PaletteAction::OpenModule("DiskManager".into()),
        },
        PaletteItem {
            label: "Calculator".into(),
            description: "Expression calculator with history".into(),
            action: PaletteAction::OpenModule("Calculator".into()),
        },
        // Phase 6 apps
        PaletteItem {
            label: "Notes".into(),
            description: "Markdown notes with live preview".into(),
            action: PaletteAction::OpenModule("Notes".into()),
        },
        PaletteItem {
            label: "Weather".into(),
            description: "Current conditions + 24h forecast (open-meteo)".into(),
            action: PaletteAction::OpenModule("Weather".into()),
        },
        PaletteItem {
            label: "Calendar".into(),
            description: "Month calendar with per-day tasks".into(),
            action: PaletteAction::OpenModule("Calendar".into()),
        },
        // Phase 4 modules
        PaletteItem {
            label: "Packages".into(),
            description: "Package manager (brew/apt/dnf/pacman)".into(),
            action: PaletteAction::OpenModule("PackageManager".into()),
        },
        PaletteItem {
            label: "SSH".into(),
            description: "SSH manager from ~/.ssh/config".into(),
            action: PaletteAction::OpenModule("SshManager".into()),
        },
        PaletteItem {
            label: "Docker".into(),
            description: "Docker containers, images and volumes".into(),
            action: PaletteAction::OpenModule("DockerPanel".into()),
        },
        PaletteItem {
            label: "Cron".into(),
            description: "Cron job editor".into(),
            action: PaletteAction::OpenModule("CronEditor".into()),
        },
        PaletteItem {
            label: "Man".into(),
            description: "Man page viewer".into(),
            action: PaletteAction::OpenModule("ManViewer".into()),
        },
        PaletteItem {
            label: "Quit".into(),
            description: "Exit VOS".into(),
            action: PaletteAction::RunCommand("quit".into()),
        },
    ]
}
