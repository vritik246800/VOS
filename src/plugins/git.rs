#![allow(dead_code)]
use anyhow::Result;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command as Proc, Stdio};
use std::sync::mpsc;

use super::Plugin;

const LILAC: Color = Color::Rgb(180, 100, 255);

// ── Card navigation ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum GitCard {
    BranchHeader,
    Log,
    Status,
    Shortcuts,
    BranchGraph,
}

impl GitCard {
    pub fn next(&self) -> GitCard {
        match self {
            GitCard::BranchHeader => GitCard::Log,
            GitCard::Log => GitCard::Status,
            GitCard::Status => GitCard::Shortcuts,
            GitCard::Shortcuts => GitCard::BranchGraph,
            GitCard::BranchGraph => GitCard::BranchHeader,
        }
    }
    pub fn prev(&self) -> GitCard {
        match self {
            GitCard::BranchHeader => GitCard::BranchGraph,
            GitCard::Log => GitCard::BranchHeader,
            GitCard::Status => GitCard::Log,
            GitCard::Shortcuts => GitCard::Status,
            GitCard::BranchGraph => GitCard::Shortcuts,
        }
    }
}

// ── Data types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum GitView {
    RepoList,
    RepoDetail,
    Log,
    Branches,
    Workspace,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GitStatus {
    Modified,
    Staged,
    Untracked,
    Deleted,
    Renamed,
    Clean,
    Other,
}

impl GitStatus {
    pub fn symbol(&self) -> &str {
        match self {
            GitStatus::Modified => "M",
            GitStatus::Staged => "S",
            GitStatus::Untracked => "?",
            GitStatus::Deleted => "D",
            GitStatus::Renamed => "R",
            GitStatus::Clean => "·",
            GitStatus::Other => "~",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            GitStatus::Modified => Color::Yellow,
            GitStatus::Staged => Color::Green,
            GitStatus::Untracked => Color::Cyan,
            GitStatus::Deleted => Color::Red,
            GitStatus::Renamed => Color::Magenta,
            GitStatus::Clean => Color::DarkGray,
            GitStatus::Other => Color::Gray,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GitEntry {
    pub status: GitStatus,
    pub path: String,
}

#[derive(Clone)]
pub struct RepoSummary {
    pub path: PathBuf,
    pub name: String,
    pub branch: String,
    pub entries: Vec<GitEntry>,
}

impl RepoSummary {
    pub fn status_label(&self) -> String {
        if self.entries.is_empty() {
            return "clean".to_string();
        }
        let mut parts = Vec::new();
        let m = self
            .entries
            .iter()
            .filter(|e| e.status == GitStatus::Modified)
            .count();
        let s = self
            .entries
            .iter()
            .filter(|e| e.status == GitStatus::Staged)
            .count();
        let u = self
            .entries
            .iter()
            .filter(|e| e.status == GitStatus::Untracked)
            .count();
        let d = self
            .entries
            .iter()
            .filter(|e| e.status == GitStatus::Deleted)
            .count();
        if m > 0 {
            parts.push(format!("{m}M"));
        }
        if s > 0 {
            parts.push(format!("{s}S"));
        }
        if u > 0 {
            parts.push(format!("{u}?"));
        }
        if d > 0 {
            parts.push(format!("{d}D"));
        }
        parts.join(" ")
    }
}

// ── Git operations ────────────────────────────────────────────────────────────

pub fn git_root(path: &Path) -> Option<PathBuf> {
    let output = Proc::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(path)
        .output()
        .ok()?;
    if output.status.success() {
        Some(PathBuf::from(
            String::from_utf8_lossy(&output.stdout).trim(),
        ))
    } else {
        None
    }
}

pub fn is_git_repo(path: &Path) -> bool {
    Proc::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(path)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn git_status(path: &Path) -> Result<Vec<GitEntry>> {
    let output = Proc::new("git")
        .args(["status", "--porcelain"])
        .current_dir(path)
        .output()?;

    let text = String::from_utf8_lossy(&output.stdout);
    let entries = text
        .lines()
        .filter(|l| l.len() >= 3)
        .map(|line| {
            let x = line.chars().nth(0).unwrap_or(' ');
            let y = line.chars().nth(1).unwrap_or(' ');
            let file_path = line[3..].to_string();
            let status = match (x, y) {
                ('?', '?') => GitStatus::Untracked,
                ('A', _) => GitStatus::Staged,
                ('M', ' ') => GitStatus::Staged,
                (_, 'M') => GitStatus::Modified,
                ('D', _) | (_, 'D') => GitStatus::Deleted,
                ('R', _) | (_, 'R') => GitStatus::Renamed,
                _ => GitStatus::Other,
            };
            GitEntry {
                status,
                path: file_path,
            }
        })
        .collect();
    Ok(entries)
}

pub fn git_branch(path: &Path) -> Option<String> {
    Proc::new("git")
        .args(["branch", "--show-current"])
        .current_dir(path)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn git_add_all(path: &Path) -> Result<String> {
    let output = Proc::new("git")
        .args(["add", "."])
        .current_dir(path)
        .output()?;
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        Ok("git add . — everything staged.".to_string())
    } else {
        Ok(format!("[err] {stderr}"))
    }
}

pub fn git_add(path: &Path, file: &str) -> Result<String> {
    let output = Proc::new("git")
        .args(["add", file])
        .current_dir(path)
        .output()?;
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !stderr.trim().is_empty() {
        Ok(stderr.trim().to_string())
    } else {
        Ok(format!("Adicionado: {file}"))
    }
}

pub fn git_commit(path: &Path, msg: &str) -> Result<String> {
    let output = Proc::new("git")
        .args(["commit", "-m", msg])
        .current_dir(path)
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Ok(if !stdout.is_empty() { stdout } else { stderr })
}

pub fn git_pull(path: &Path) -> Result<String> {
    let output = Proc::new("git").args(["pull"]).current_dir(path).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Ok(if !stdout.is_empty() { stdout } else { stderr })
}

pub fn git_push(path: &Path) -> Result<String> {
    let output = Proc::new("git").args(["push"]).current_dir(path).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Ok(if !stdout.is_empty() { stdout } else { stderr })
}

pub fn git_checkout(path: &Path, branch: &str) -> Result<String> {
    let output = Proc::new("git")
        .args(["checkout", branch])
        .current_dir(path)
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Ok(if !stdout.is_empty() { stdout } else { stderr })
}

pub fn create_gitignore(path: &Path) -> Result<String> {
    let dest = path.join(".gitignore");
    if dest.exists() {
        return Ok(".gitignore already exists.".to_string());
    }
    let content = "\
# Rust
/target/
Cargo.lock

# OS
.DS_Store
._*
.Spotlight-V100
.Trashes
Thumbs.db

# Editor
.idea/
.vscode/
*.swp
*.swo
*~

# Env
.env
.env.local
.env*.local
";
    std::fs::write(&dest, content)?;
    Ok(".gitignore created successfully.".to_string())
}

pub fn git_all_files(path: &Path) -> Result<Vec<GitEntry>> {
    let changed = git_status(path)?;
    let changed_paths: std::collections::HashSet<String> =
        changed.iter().map(|e| e.path.clone()).collect();

    let output = Proc::new("git")
        .args(["ls-files"])
        .current_dir(path)
        .output()?;

    let mut result = changed;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if !line.is_empty() && !changed_paths.contains(line) {
            result.push(GitEntry {
                status: GitStatus::Clean,
                path: line.to_string(),
            });
        }
    }
    Ok(result)
}

pub fn git_ahead_behind(path: &Path) -> (usize, usize) {
    let output = Proc::new("git")
        .args(["rev-list", "--count", "--left-right", "@{upstream}...HEAD"])
        .current_dir(path)
        .output();
    if let Ok(out) = output {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            let parts: Vec<&str> = text.trim().split('\t').collect();
            if parts.len() == 2 {
                let behind = parts[0].parse().unwrap_or(0);
                let ahead = parts[1].parse().unwrap_or(0);
                return (ahead, behind);
            }
        }
    }
    (0, 0)
}

pub fn git_log(path: &Path) -> Result<Vec<String>> {
    let output = Proc::new("git")
        .args(["log", "--oneline", "--graph", "--decorate", "--all", "-80"])
        .current_dir(path)
        .output()?;
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.lines().map(|l| l.to_string()).collect())
}

pub fn git_branches(path: &Path) -> Result<Vec<String>> {
    let output = Proc::new("git")
        .args(["branch", "-a"])
        .current_dir(path)
        .output()?;
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.lines().map(|l| l.to_string()).collect())
}

pub fn git_diff_file(path: &Path, file: &str, staged: bool) -> Result<String> {
    let mut args = vec!["diff"];
    if staged {
        args.push("--cached");
    }
    args.push("--");
    args.push(file);
    let output = Proc::new("git").args(&args).current_dir(path).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if stdout.is_empty() && !stderr.is_empty() {
        Ok(stderr)
    } else {
        Ok(stdout)
    }
}

pub fn git_unstage(path: &Path, file: &str) -> Result<String> {
    let output = Proc::new("git")
        .args(["restore", "--staged", file])
        .current_dir(path)
        .output()?;
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        Ok(format!("Unstaged: {file}"))
    } else {
        Ok(format!("[err] {stderr}"))
    }
}

pub fn git_merge(path: &Path, branch: &str) -> Result<String> {
    let output = Proc::new("git")
        .args(["merge", branch])
        .current_dir(path)
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        Ok(if !stdout.is_empty() { stdout } else { stderr })
    } else {
        Ok(format!("[merge failed] {stderr}"))
    }
}

// ── Repo discovery ────────────────────────────────────────────────────────────

const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    ".git",
    ".cache",
    "Library",
    "Applications",
    ".npm",
    ".cargo",
    "vendor",
    ".Trash",
];

pub fn discover_git_repos(root: &Path, max_depth: usize) -> Vec<RepoSummary> {
    let mut repos = Vec::new();
    find_repos(root, 0, max_depth, &mut repos);
    repos.sort_by(|a, b| a.name.cmp(&b.name));
    repos
}

fn find_repos(dir: &Path, depth: usize, max_depth: usize, repos: &mut Vec<RepoSummary>) {
    if depth > max_depth {
        return;
    }
    if dir.join(".git").is_dir() {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| dir.display().to_string());
        let branch = git_branch(dir).unwrap_or_else(|| "HEAD".to_string());
        let entries = git_status(dir).unwrap_or_default();
        repos.push(RepoSummary {
            path: dir.to_path_buf(),
            name,
            branch,
            entries,
        });
        return;
    }
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
            continue;
        }
        find_repos(&path, depth + 1, max_depth, repos);
    }
}

// ── GitPlugin ─────────────────────────────────────────────────────────────────

pub struct GitPlugin {
    // ── Legacy fields (used by side-panel render) ─────────────────────────────
    pub view: GitView,
    pub repos: Vec<RepoSummary>,
    pub repo_list_state: ListState,
    pub entries: Vec<GitEntry>,
    pub all_entries: Vec<GitEntry>,
    pub list_state: ListState,
    pub branch: String,
    pub repo_path: PathBuf,
    pub search_root: PathBuf,
    pub ahead: usize,
    pub behind: usize,
    pub commit_msg: Option<String>,
    pub push_output: Option<Vec<String>>,
    pub push_rx: Option<mpsc::Receiver<String>>,
    pub push_running: bool,
    pub log_lines: Vec<String>,
    pub log_scroll: usize,
    pub branches: Vec<String>,
    pub branch_list_state: ListState,

    // ── New panel fields ──────────────────────────────────────────────────────
    pub active_card: GitCard,
    pub branch_dropdown_open: bool,
    pub branch_dropdown_state: ListState,
    pub shortcuts_scroll: usize,
    pub branch_graph_scroll: usize,

    // ── Workspace view fields ─────────────────────────────────────────────────
    pub workspace_pane: u8, // 0=status, 1=log, 2=diff
    pub diff_content: String,
    pub diff_scroll: usize,
    pub staged_files: Vec<(String, String)>, // (status_char, filename)
    pub unstaged_files: Vec<(String, String)>, // (status_char, filename)
    pub untracked_files: Vec<String>,
    pub workspace_staged_idx: usize,
    pub workspace_unstaged_idx: usize,
    pub workspace_section: u8, // 0=staged, 1=unstaged, 2=untracked
    pub workspace_log_idx: usize,
}

impl GitPlugin {
    pub fn new(initial_path: PathBuf) -> Self {
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/"));
        Self {
            view: GitView::RepoList,
            repos: Vec::new(),
            repo_list_state: ListState::default(),
            entries: Vec::new(),
            all_entries: Vec::new(),
            list_state: ListState::default(),
            branch: String::new(),
            repo_path: initial_path,
            search_root: home,
            ahead: 0,
            behind: 0,
            commit_msg: None,
            push_output: None,
            push_rx: None,
            push_running: false,
            log_lines: Vec::new(),
            log_scroll: 0,
            branches: Vec::new(),
            branch_list_state: ListState::default(),
            active_card: GitCard::BranchHeader,
            branch_dropdown_open: false,
            branch_dropdown_state: ListState::default(),
            shortcuts_scroll: 0,
            branch_graph_scroll: 0,
            workspace_pane: 0,
            diff_content: String::new(),
            diff_scroll: 0,
            staged_files: Vec::new(),
            unstaged_files: Vec::new(),
            untracked_files: Vec::new(),
            workspace_staged_idx: 0,
            workspace_unstaged_idx: 0,
            workspace_section: 1, // default: unstaged
            workspace_log_idx: 0,
        }
    }

    // ── Panel data loading ────────────────────────────────────────────────────

    pub fn load_panel_data(&mut self) {
        let path = self.repo_path.clone();
        self.branch = git_branch(&path).unwrap_or_else(|| "HEAD".to_string());
        self.entries = git_status(&path).unwrap_or_default();
        self.all_entries = git_all_files(&path).unwrap_or_default();
        let (ahead, behind) = git_ahead_behind(&path);
        self.ahead = ahead;
        self.behind = behind;
        self.log_lines = git_log(&path).unwrap_or_default();
        self.log_scroll = 0;
        self.branches = git_branches(&path).unwrap_or_default();
        self.branch_graph_scroll = 0;
        self.active_card = GitCard::BranchHeader;
        if !self.all_entries.is_empty() && self.list_state.selected().is_none() {
            self.list_state.select(Some(0));
        }
    }

    pub fn refresh_panel_data(&mut self) {
        let path = self.repo_path.clone();
        self.branch = git_branch(&path).unwrap_or_else(|| "HEAD".to_string());
        self.entries = git_status(&path).unwrap_or_default();
        self.all_entries = git_all_files(&path).unwrap_or_default();
        let (ahead, behind) = git_ahead_behind(&path);
        self.ahead = ahead;
        self.behind = behind;
        self.log_lines = git_log(&path).unwrap_or_default();
        self.branches = git_branches(&path).unwrap_or_default();
        if !self.all_entries.is_empty() && self.list_state.selected().is_none() {
            self.list_state.select(Some(0));
        }
    }

    // ── Workspace refresh ─────────────────────────────────────────────────────

    pub fn refresh_workspace(&mut self) {
        let path = self.repo_path.clone();
        self.branch = git_branch(&path).unwrap_or_else(|| "HEAD".to_string());
        let (ahead, behind) = git_ahead_behind(&path);
        self.ahead = ahead;
        self.behind = behind;
        self.log_lines = git_log(&path).unwrap_or_default();

        // Parse git status --porcelain
        self.staged_files.clear();
        self.unstaged_files.clear();
        self.untracked_files.clear();

        let output = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&path)
            .output();

        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if line.len() < 3 {
                    continue;
                }
                let x = line.chars().nth(0).unwrap_or(' ');
                let y = line.chars().nth(1).unwrap_or(' ');
                let file = line[3..].to_string();

                // Untracked
                if x == '?' && y == '?' {
                    self.untracked_files.push(file);
                    continue;
                }
                // Staged changes (index column)
                if x != ' ' && x != '?' {
                    let status_str = x.to_string();
                    self.staged_files.push((status_str, file.clone()));
                }
                // Unstaged changes (worktree column)
                if y != ' ' && y != '?' {
                    let status_str = y.to_string();
                    self.unstaged_files.push((status_str, file));
                }
            }
        }

        // Clamp indices
        if self.workspace_staged_idx >= self.staged_files.len().max(1) {
            self.workspace_staged_idx = self.staged_files.len().saturating_sub(1);
        }
        if self.workspace_unstaged_idx >= self.unstaged_files.len().max(1) {
            self.workspace_unstaged_idx = self.unstaged_files.len().saturating_sub(1);
        }

        // Load diff for selected file
        self.load_selected_diff();
    }

    pub fn load_selected_diff(&mut self) {
        let path = self.repo_path.clone();
        let (file, staged) = match self.workspace_section {
            0 => {
                let f = self
                    .staged_files
                    .get(self.workspace_staged_idx)
                    .map(|(_, f)| f.clone());
                (f, true)
            }
            _ => {
                let f = self
                    .unstaged_files
                    .get(self.workspace_unstaged_idx)
                    .map(|(_, f)| f.clone())
                    .or_else(|| {
                        let ui = self
                            .workspace_unstaged_idx
                            .saturating_sub(self.unstaged_files.len());
                        self.untracked_files.get(ui).cloned()
                    });
                (f, false)
            }
        };
        if let Some(file) = file {
            self.diff_content =
                git_diff_file(&path, &file, staged).unwrap_or_else(|e| format!("[diff error] {e}"));
            self.diff_scroll = 0;
        } else {
            self.diff_content.clear();
            self.diff_scroll = 0;
        }
    }

    /// Returns the currently selected filename in the status pane (for add/unstage).
    pub fn workspace_selected_file(&self) -> Option<(String, bool)> {
        match self.workspace_section {
            0 => self
                .staged_files
                .get(self.workspace_staged_idx)
                .map(|(_, f)| (f.clone(), true)),
            1 => self
                .unstaged_files
                .get(self.workspace_unstaged_idx)
                .map(|(_, f)| (f.clone(), false)),
            2 => self
                .untracked_files
                .get(self.workspace_unstaged_idx)
                .map(|f| (f.clone(), false)),
            _ => None,
        }
    }

    pub fn workspace_move_up(&mut self) {
        match self.workspace_section {
            0 => {
                if self.workspace_staged_idx > 0 {
                    self.workspace_staged_idx -= 1;
                }
            }
            1 => {
                if self.workspace_unstaged_idx > 0 {
                    self.workspace_unstaged_idx -= 1;
                } else {
                    // Move to staged section
                    self.workspace_section = 0;
                    self.workspace_staged_idx = self.staged_files.len().saturating_sub(1);
                }
            }
            2 => {
                if self.workspace_unstaged_idx > 0 {
                    self.workspace_unstaged_idx -= 1;
                } else {
                    // Move to unstaged section
                    self.workspace_section = 1;
                    self.workspace_unstaged_idx = self.unstaged_files.len().saturating_sub(1);
                }
            }
            _ => {}
        }
    }

    pub fn workspace_move_down(&mut self) {
        match self.workspace_section {
            0 => {
                let max = self.staged_files.len().saturating_sub(1);
                if self.workspace_staged_idx < max {
                    self.workspace_staged_idx += 1;
                } else {
                    // Move to unstaged section
                    self.workspace_section = 1;
                    self.workspace_unstaged_idx = 0;
                }
            }
            1 => {
                let max = self.unstaged_files.len().saturating_sub(1);
                if self.workspace_unstaged_idx < max {
                    self.workspace_unstaged_idx += 1;
                } else {
                    // Move to untracked section
                    self.workspace_section = 2;
                    self.workspace_unstaged_idx = 0;
                }
            }
            2 => {
                let max = self.untracked_files.len().saturating_sub(1);
                if self.workspace_unstaged_idx < max {
                    self.workspace_unstaged_idx += 1;
                }
            }
            _ => {}
        }
    }

    // ── Full-screen panel render ──────────────────────────────────────────────

    pub fn render_panel(&mut self, f: &mut Frame, area: Rect) {
        if area.height < 4 {
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Git ")
                .border_style(Style::default().fg(LILAC));
            f.render_widget(block, area);
            return;
        }

        // Route to Workspace view
        if self.view == GitView::Workspace {
            self.render_workspace(f, area);
            if self.push_output.is_some() {
                self.render_push_popup(f, area);
            }
            return;
        }

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        let top_area = rows[0];
        let body_area = rows[1];

        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(50),
                Constraint::Percentage(25),
            ])
            .split(body_area);

        let right_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(cols[2]);

        self.render_branch_header(f, top_area);
        self.render_log_card(f, cols[0]);
        self.render_status_card_panel(f, cols[1]);
        self.render_shortcuts_card(f, right_rows[0]);
        self.render_branch_graph_card(f, right_rows[1]);

        if self.branch_dropdown_open {
            self.render_branch_dropdown(f, area);
        }
        if self.push_output.is_some() {
            self.render_push_popup(f, area);
        }
    }

    fn card_border(&self, card: GitCard) -> Style {
        if self.active_card == card {
            Style::default().fg(LILAC).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        }
    }

    fn render_branch_header(&self, f: &mut Frame, area: Rect) {
        let border = self.card_border(GitCard::BranchHeader);
        let active = self.active_card == GitCard::BranchHeader;

        let hint = if active {
            "  ·  Enter: switch branch  ·  Tab: next  ·  Esc: close"
        } else {
            ""
        };

        let line = Line::from(vec![
            Span::raw("  "),
            Span::styled("branch  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                self.branch.clone(),
                Style::default().fg(LILAC).add_modifier(Modifier::BOLD),
            ),
            Span::styled(hint, Style::default().fg(Color::DarkGray)),
        ]);

        let widget = Paragraph::new(line).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Git ")
                .border_style(border),
        );
        f.render_widget(widget, area);
    }

    fn render_log_card(&self, f: &mut Frame, area: Rect) {
        let border = self.card_border(GitCard::Log);
        let active = self.active_card == GitCard::Log;
        let hint = if active { " ↑↓: scroll" } else { "" };

        let graph_chars: &[char] = &['*', '|', '/', '\\', '-', '_', ' '];

        let lines: Vec<Line> = if self.log_lines.is_empty() {
            vec![Line::from(Span::styled(
                "  no history",
                Style::default().fg(Color::DarkGray),
            ))]
        } else {
            self.log_lines
                .iter()
                .map(|raw| {
                    let prefix_end = raw
                        .char_indices()
                        .find(|(_, c)| !graph_chars.contains(c))
                        .map(|(i, _)| i)
                        .unwrap_or(raw.len());

                    let prefix = &raw[..prefix_end];
                    let rest = &raw[prefix_end..];

                    let mut spans: Vec<Span> = prefix
                        .chars()
                        .map(|ch| {
                            let color = match ch {
                                '*' => Color::Yellow,
                                '|' => Color::Blue,
                                '/' | '\\' => Color::Cyan,
                                '-' | '_' => Color::Magenta,
                                _ => Color::DarkGray,
                            };
                            Span::styled(ch.to_string(), Style::default().fg(color))
                        })
                        .collect();

                    let hash_end = rest
                        .char_indices()
                        .nth(7)
                        .map(|(i, _)| i)
                        .unwrap_or(rest.len());
                    if hash_end > 0 {
                        spans.push(Span::styled(
                            rest[..hash_end].to_string(),
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        ));
                        let after = &rest[hash_end..];
                        if let Some(open) = after.find('(') {
                            if let Some(rel_close) = after[open..].find(')') {
                                let close = open + rel_close;
                                spans.push(Span::raw(after[..open].to_string()));
                                spans.push(Span::styled(
                                    after[open..=close].to_string(),
                                    Style::default()
                                        .fg(Color::Green)
                                        .add_modifier(Modifier::BOLD),
                                ));
                                if close + 1 < after.len() {
                                    spans.push(Span::raw(after[close + 1..].to_string()));
                                }
                            } else {
                                spans.push(Span::raw(after.to_string()));
                            }
                        } else {
                            spans.push(Span::raw(after.to_string()));
                        }
                    } else {
                        spans.push(Span::raw(rest.to_string()));
                    }

                    Line::from(spans)
                })
                .collect()
        };

        let widget = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" Git Log{hint} "))
                    .border_style(border),
            )
            .scroll((self.log_scroll as u16, 0));
        f.render_widget(widget, area);
    }

    fn render_status_card_panel(&mut self, f: &mut Frame, area: Rect) {
        let border = self.card_border(GitCard::Status);
        let active = self.active_card == GitCard::Status;

        let m = self
            .entries
            .iter()
            .filter(|e| e.status == GitStatus::Modified)
            .count();
        let s = self
            .entries
            .iter()
            .filter(|e| e.status == GitStatus::Staged)
            .count();
        let u = self
            .entries
            .iter()
            .filter(|e| e.status == GitStatus::Untracked)
            .count();
        let d = self
            .entries
            .iter()
            .filter(|e| e.status == GitStatus::Deleted)
            .count();

        let (state_color, state_label) = if self.entries.is_empty() && self.ahead == 0 {
            (Color::Green, "clean")
        } else if self.entries.is_empty() {
            (Color::Cyan, "ready to push")
        } else {
            (Color::Yellow, "has changes")
        };

        let ahead_str = if self.ahead > 0 {
            format!(" ↑{}", self.ahead)
        } else {
            String::new()
        };
        let behind_str = if self.behind > 0 {
            format!(" ↓{}", self.behind)
        } else {
            String::new()
        };
        let hint = if active && self.commit_msg.is_none() {
            " ↑↓: navigate  a:add  c:commit  p:push  P:pull"
        } else {
            ""
        };

        let outer = Block::default()
            .borders(Borders::ALL)
            .title(format!(" Status{hint} "))
            .border_style(border);
        let inner = outer.inner(area);
        f.render_widget(outer, area);

        if inner.height < 3 {
            return;
        }

        let commit_h = if self.commit_msg.is_some() { 3u16 } else { 0 };
        let info_h = 2u16;

        let constraints = if self.commit_msg.is_some() {
            vec![
                Constraint::Length(info_h),
                Constraint::Min(0),
                Constraint::Length(commit_h),
            ]
        } else {
            vec![Constraint::Length(info_h), Constraint::Min(0)]
        };

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        // Info header
        let info_lines = vec![
            Line::from(vec![
                Span::styled(" branch ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    self.branch.clone(),
                    Style::default().fg(LILAC).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{ahead_str}{behind_str}"),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled("  ", Style::default()),
                Span::styled(
                    state_label,
                    Style::default()
                        .fg(state_color)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    format!("M:{m} "),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("S:{s} "),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("?:{u} "),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("D:{d}"),
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
            ]),
        ];
        f.render_widget(Paragraph::new(info_lines), rows[0]);

        // File list
        let list_area = rows[1];
        if self.all_entries.is_empty() {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "  repository clean ✓",
                    Style::default().fg(Color::DarkGray),
                ))),
                list_area,
            );
        } else {
            let items: Vec<ListItem> = self
                .all_entries
                .iter()
                .map(|e| {
                    let sym = e.status.symbol();
                    let col = e.status.color();
                    let path_style = if e.status == GitStatus::Clean {
                        Style::default().fg(Color::DarkGray)
                    } else {
                        Style::default()
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!(" {sym} "),
                            Style::default().fg(col).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(e.path.clone(), path_style),
                    ]))
                })
                .collect();

            let list = List::new(items)
                .highlight_style(
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("▶ ");
            f.render_stateful_widget(list, list_area, &mut self.list_state);
        }

        // Commit input
        if self.commit_msg.is_some() {
            let msg = self.commit_msg.as_deref().unwrap_or("");
            let text = format!("{msg}_");
            let widget = Paragraph::new(text)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Commit message (Enter: confirm · Esc: cancel) ")
                        .border_style(Style::default().fg(Color::Magenta)),
                )
                .style(Style::default().fg(Color::White));
            f.render_widget(widget, rows[2]);
        }
    }

    fn render_shortcuts_card(&self, f: &mut Frame, area: Rect) {
        let border = self.card_border(GitCard::Shortcuts);
        let active = self.active_card == GitCard::Shortcuts;
        let hint = if active { " ↑↓: scroll" } else { "" };

        let all_shortcuts: &[(&str, &str)] = &[
            ("Tab", "navigate cards"),
            ("↑↓", "scroll / navigate"),
            ("Enter", "switch branch / action"),
            ("a", "git add ."),
            ("c", "commit"),
            ("p", "push -u origin"),
            ("P", "pull"),
            ("r", "refresh"),
            ("i", "create .gitignore"),
            ("Ctrl+G", "toggle git panel"),
            ("Esc", "close panel"),
        ];

        let items: Vec<Line> = all_shortcuts
            .iter()
            .skip(self.shortcuts_scroll)
            .map(|(key, desc)| {
                Line::from(vec![
                    Span::styled(
                        format!("  {:<8}", key),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!(" {}", desc), Style::default().fg(Color::White)),
                ])
            })
            .collect();

        let widget = Paragraph::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Git Shortcuts{hint} "))
                .border_style(border),
        );
        f.render_widget(widget, area);
    }

    fn render_branch_graph_card(&self, f: &mut Frame, area: Rect) {
        let border = self.card_border(GitCard::BranchGraph);
        let active = self.active_card == GitCard::BranchGraph;
        let hint = if active { " ↑↓: scroll" } else { "" };

        let outer = Block::default()
            .borders(Borders::ALL)
            .title(format!(" Branches{hint} "))
            .border_style(border);
        let inner = outer.inner(area);
        f.render_widget(outer, area);

        if self.branches.is_empty() {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "  no branches",
                    Style::default().fg(Color::DarkGray),
                ))),
                inner,
            );
            return;
        }

        let locals: Vec<(bool, String)> = self
            .branches
            .iter()
            .filter(|b| {
                let t = b.trim();
                !t.starts_with("remotes/") && !t.contains("->")
            })
            .map(|b| {
                let t = b.trim();
                let is_current = t.starts_with('*');
                let name = if is_current {
                    t[1..].trim().to_string()
                } else {
                    t.to_string()
                };
                (is_current, name)
            })
            .collect();

        let remotes: Vec<String> = self
            .branches
            .iter()
            .filter(|b| {
                let t = b.trim();
                t.starts_with("remotes/") && !t.contains("->")
            })
            .map(|b| {
                let t = b.trim();
                t.strip_prefix("remotes/origin/").unwrap_or(t).to_string()
            })
            .collect();

        let n_local = locals.len();
        let mut all_lines: Vec<Line> = Vec::new();

        for (i, (is_current, name)) in locals.iter().enumerate() {
            let is_last = i == n_local - 1 && remotes.is_empty();
            let connector = if i == 0 {
                "╭─"
            } else if is_last {
                "╰─"
            } else {
                "├─"
            };

            let name_style = if *is_current {
                Style::default().fg(LILAC).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let mut spans = vec![
                Span::styled(connector, Style::default().fg(Color::Blue)),
                Span::styled(format!(" {name}"), name_style),
            ];
            if *is_current {
                spans.push(Span::styled(
                    " ●",
                    Style::default().fg(LILAC).add_modifier(Modifier::BOLD),
                ));
            }
            all_lines.push(Line::from(spans));

            if !is_last {
                all_lines.push(Line::from(Span::styled(
                    "│ ",
                    Style::default().fg(Color::Blue),
                )));
            }
        }

        if !remotes.is_empty() {
            if !locals.is_empty() {
                all_lines.push(Line::from(Span::styled(
                    "│",
                    Style::default().fg(Color::Blue),
                )));
            }
            all_lines.push(Line::from(Span::styled(
                " remotes:",
                Style::default().fg(Color::DarkGray),
            )));
            let n_rem = remotes.len();
            for (i, name) in remotes.iter().enumerate() {
                let is_last = i == n_rem - 1;
                let conn = if i == 0 {
                    "  ╭─"
                } else if is_last {
                    "  ╰─"
                } else {
                    "  ├─"
                };
                all_lines.push(Line::from(vec![
                    Span::styled(conn, Style::default().fg(Color::Cyan)),
                    Span::styled(format!(" {name}"), Style::default().fg(Color::Cyan)),
                ]));
                if !is_last {
                    all_lines.push(Line::from(Span::styled(
                        "  │ ",
                        Style::default().fg(Color::Cyan),
                    )));
                }
            }
        }

        let visible: Vec<Line> = all_lines
            .into_iter()
            .skip(self.branch_graph_scroll)
            .collect();
        f.render_widget(Paragraph::new(visible), inner);
    }

    // ── Workspace 3-pane render ───────────────────────────────────────────────

    fn render_workspace(&mut self, f: &mut Frame, area: Rect) {
        // Footer: 1 line at bottom
        let footer_h = 1u16;
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(footer_h)])
            .split(area);
        let body = rows[0];
        let footer_area = rows[1];

        // Body: left 30%, right 70%
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(body);
        let left_area = cols[0];
        let right_area = cols[1];

        // Right: top ~80% diff, bottom ~20% log
        let log_h = (right_area.height / 5).max(4);
        let diff_h = right_area.height.saturating_sub(log_h);
        let right_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(diff_h), Constraint::Length(log_h)])
            .split(right_area);
        let diff_area = right_rows[0];
        let log_area = right_rows[1];

        // ── Left pane: file status list ───────────────────────────────────────
        let left_focused = self.workspace_pane == 0;
        let left_border = if left_focused {
            Style::default().fg(LILAC).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let mut items: Vec<ListItem> = Vec::new();

        // STAGED section header
        items.push(ListItem::new(Line::from(vec![Span::styled(
            " STAGED",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )])));
        let staged_start = 1usize;
        for (status, file) in &self.staged_files {
            let sym_color = match status.as_str() {
                "M" => Color::Yellow,
                "A" => Color::Green,
                "D" => Color::Red,
                "R" => Color::Magenta,
                _ => Color::Gray,
            };
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    format!("  {} ", status),
                    Style::default().fg(sym_color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(file.clone()),
            ])));
        }
        if self.staged_files.is_empty() {
            items.push(ListItem::new(Line::from(Span::styled(
                "  (none)",
                Style::default().fg(Color::DarkGray),
            ))));
        }

        // UNSTAGED section header
        items.push(ListItem::new(Line::from(vec![Span::styled(
            " UNSTAGED",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )])));
        let unstaged_start = staged_start + self.staged_files.len().max(1) + 1;
        for (status, file) in &self.unstaged_files {
            let sym_color = match status.as_str() {
                "M" => Color::Yellow,
                "D" => Color::Red,
                _ => Color::Gray,
            };
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    format!("  {} ", status),
                    Style::default().fg(sym_color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(file.clone()),
            ])));
        }
        if self.unstaged_files.is_empty() {
            items.push(ListItem::new(Line::from(Span::styled(
                "  (none)",
                Style::default().fg(Color::DarkGray),
            ))));
        }

        // UNTRACKED section header
        items.push(ListItem::new(Line::from(vec![Span::styled(
            " UNTRACKED",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )])));
        let untracked_start = unstaged_start + self.unstaged_files.len().max(1) + 1;
        for file in &self.untracked_files {
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    "  ? ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(file.clone()),
            ])));
        }
        if self.untracked_files.is_empty() {
            items.push(ListItem::new(Line::from(Span::styled(
                "  (none)",
                Style::default().fg(Color::DarkGray),
            ))));
        }

        // Compute the list index for the selected item
        let list_selected_idx: usize = match self.workspace_section {
            0 => {
                if self.staged_files.is_empty() {
                    staged_start
                } else {
                    staged_start + self.workspace_staged_idx
                }
            }
            1 => {
                if self.unstaged_files.is_empty() {
                    unstaged_start
                } else {
                    unstaged_start + self.workspace_unstaged_idx
                }
            }
            _ => {
                if self.untracked_files.is_empty() {
                    untracked_start
                } else {
                    let idx = self
                        .workspace_unstaged_idx
                        .saturating_sub(self.unstaged_files.len());
                    untracked_start + idx
                }
            }
        };

        let mut left_list_state = ListState::default();
        if left_focused {
            left_list_state.select(Some(list_selected_idx));
        }

        let ahead_str = if self.ahead > 0 {
            format!(" ↑{}", self.ahead)
        } else {
            String::new()
        };
        let behind_str = if self.behind > 0 {
            format!(" ↓{}", self.behind)
        } else {
            String::new()
        };
        let title = format!(" {} {}{} ", self.branch, ahead_str, behind_str);

        let file_list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(left_border),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");
        f.render_stateful_widget(file_list, left_area, &mut left_list_state);

        // ── Right-top pane: diff view ─────────────────────────────────────────
        let diff_focused = self.workspace_pane == 2;
        let diff_border = if diff_focused {
            Style::default().fg(LILAC).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let diff_lines: Vec<Line> = if self.diff_content.is_empty() {
            vec![Line::from(Span::styled(
                "  Select a file to view the diff",
                Style::default().fg(Color::DarkGray),
            ))]
        } else {
            self.diff_content
                .lines()
                .map(|raw| {
                    let style = if raw.starts_with('+') && !raw.starts_with("+++") {
                        Style::default().fg(Color::Green)
                    } else if raw.starts_with('-') && !raw.starts_with("---") {
                        Style::default().fg(Color::Red)
                    } else if raw.starts_with("@@") {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default()
                    };
                    Line::from(Span::styled(raw.to_string(), style))
                })
                .collect()
        };

        let diff_widget = Paragraph::new(diff_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Diff ")
                    .border_style(diff_border),
            )
            .scroll((self.diff_scroll as u16, 0))
            .wrap(Wrap { trim: false });
        f.render_widget(diff_widget, diff_area);

        // ── Right-bottom pane: recent log ─────────────────────────────────────
        let log_focused = self.workspace_pane == 1;
        let log_border = if log_focused {
            Style::default().fg(LILAC).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let graph_chars: &[char] = &['*', '|', '/', '\\', '-', '_', ' '];
        let log_lines: Vec<Line> = if self.log_lines.is_empty() {
            vec![Line::from(Span::styled(
                "  no history",
                Style::default().fg(Color::DarkGray),
            ))]
        } else {
            self.log_lines
                .iter()
                .skip(self.workspace_log_idx)
                .map(|raw| {
                    let prefix_end = raw
                        .char_indices()
                        .find(|(_, c)| !graph_chars.contains(c))
                        .map(|(i, _)| i)
                        .unwrap_or(raw.len());
                    let prefix = &raw[..prefix_end];
                    let rest = &raw[prefix_end..];
                    let mut spans: Vec<Span> = prefix
                        .chars()
                        .map(|ch| {
                            let color = match ch {
                                '*' => Color::Yellow,
                                '|' => Color::Blue,
                                '/' | '\\' => Color::Cyan,
                                _ => Color::DarkGray,
                            };
                            Span::styled(ch.to_string(), Style::default().fg(color))
                        })
                        .collect();
                    let hash_end = rest
                        .char_indices()
                        .nth(7)
                        .map(|(i, _)| i)
                        .unwrap_or(rest.len());
                    if hash_end > 0 {
                        spans.push(Span::styled(
                            rest[..hash_end].to_string(),
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        ));
                        spans.push(Span::raw(rest[hash_end..].to_string()));
                    } else {
                        spans.push(Span::raw(rest.to_string()));
                    }
                    Line::from(spans)
                })
                .collect()
        };

        let log_widget = Paragraph::new(log_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Log ")
                .border_style(log_border),
        );
        f.render_widget(log_widget, log_area);

        // ── Footer ────────────────────────────────────────────────────────────
        let footer = Line::from(vec![
            Span::styled(
                " [a]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("dd "),
            Span::styled(
                "[u]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("nstage "),
            Span::styled(
                "[c]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("ommit "),
            Span::styled(
                "[b]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("ranches "),
            Span::styled(
                "[p]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("ull "),
            Span::styled(
                "[P]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("ush "),
            Span::styled(
                "[Tab]",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("panel "),
            Span::styled("[Esc]", Style::default().fg(Color::DarkGray)),
            Span::raw("quit"),
        ]);
        f.render_widget(Paragraph::new(footer), footer_area);
    }

    fn render_branch_dropdown(&mut self, f: &mut Frame, area: Rect) {
        let local_branches: Vec<String> = self
            .branches
            .iter()
            .filter(|b| {
                let t = b.trim();
                !t.starts_with("remotes/") && !t.contains("->")
            })
            .map(|b| {
                let t = b.trim();
                if t.starts_with('*') {
                    t[1..].trim().to_string()
                } else {
                    t.to_string()
                }
            })
            .collect();

        if local_branches.is_empty() {
            return;
        }

        let popup_w = 44u16.min(area.width.saturating_sub(4));
        let popup_h = (local_branches.len() as u16 + 2)
            .min(area.height.saturating_sub(6))
            .max(4);
        let popup_x = area.x + (area.width.saturating_sub(popup_w)) / 2;
        let popup_y = area.y + 4;
        let popup_area = Rect::new(popup_x, popup_y, popup_w, popup_h);

        f.render_widget(Clear, popup_area);

        let items: Vec<ListItem> = local_branches
            .iter()
            .map(|b| {
                let is_current = b == &self.branch;
                let style = if is_current {
                    Style::default().fg(LILAC).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let marker = if is_current { "●" } else { " " };
                ListItem::new(Line::from(vec![
                    Span::styled(format!(" {marker} "), style),
                    Span::styled(b.clone(), style),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Switch Branch  Enter: confirm · Esc: cancel ")
                    .border_style(Style::default().fg(LILAC).add_modifier(Modifier::BOLD)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        f.render_stateful_widget(list, popup_area, &mut self.branch_dropdown_state);
    }

    // ── Legacy panel helpers (side-panel + old views) ─────────────────────────

    pub fn refresh_repos(&mut self) {
        self.repos = discover_git_repos(&self.search_root, 4);
        if !self.repos.is_empty() && self.repo_list_state.selected().is_none() {
            self.repo_list_state.select(Some(0));
        }
    }

    pub fn open_selected_repo(&mut self) {
        if let Some(idx) = self.repo_list_state.selected() {
            if let Some(repo) = self.repos.get(idx) {
                self.repo_path = repo.path.clone();
                self.branch = repo.branch.clone();
                self.entries = repo.entries.clone();
                self.all_entries = git_all_files(&repo.path).unwrap_or_default();
                let (ahead, behind) = git_ahead_behind(&repo.path);
                self.ahead = ahead;
                self.behind = behind;
                self.list_state = ListState::default();
                if !self.all_entries.is_empty() {
                    self.list_state.select(Some(0));
                }
                self.view = GitView::RepoDetail;
            }
        }
    }

    pub fn back_to_list(&mut self) {
        self.commit_msg = None;
        self.view = GitView::RepoList;
    }

    pub fn back_to_detail(&mut self) {
        self.view = GitView::RepoDetail;
    }

    pub fn open_repo_at(&mut self, path: &Path) {
        self.repo_path = path.to_path_buf();
        self.branch = git_branch(path).unwrap_or_else(|| "HEAD".to_string());
        self.entries = git_status(path).unwrap_or_default();
        self.all_entries = git_all_files(path).unwrap_or_default();
        let (ahead, behind) = git_ahead_behind(path);
        self.ahead = ahead;
        self.behind = behind;
        self.list_state = ListState::default();
        if !self.all_entries.is_empty() {
            self.list_state.select(Some(0));
        }
        self.commit_msg = None;
        self.push_output = None;
        self.view = GitView::RepoDetail;
    }

    pub fn open_cwd_repo(&mut self, cwd: &Path) -> bool {
        if let Some(root) = git_root(cwd) {
            self.open_repo_at(&root);
            true
        } else {
            false
        }
    }

    pub fn start_push(&mut self, path: &Path, branch: &str) {
        let (tx, rx) = mpsc::channel::<String>();
        self.push_rx = Some(rx);
        self.push_running = true;
        self.push_output = Some(vec![
            format!("$ git push -u origin {branch}"),
            String::new(),
        ]);

        let path_owned = path.to_path_buf();
        let branch_owned = branch.to_string();

        std::thread::spawn(move || {
            let mut child = match Proc::new("git")
                .args(["push", "-u", "origin", &branch_owned])
                .current_dir(&path_owned)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(format!("[Error starting git push: {e}]"));
                    let _ = tx.send("\x00DONE\x00".to_string());
                    return;
                }
            };

            let tx_err = tx.clone();
            let stderr = child.stderr.take();
            let stderr_handle = std::thread::spawn(move || {
                if let Some(s) = stderr {
                    for line in BufReader::new(s).lines().map_while(Result::ok) {
                        let _ = tx_err.send(line);
                    }
                }
            });

            if let Some(s) = child.stdout.take() {
                for line in BufReader::new(s).lines().map_while(Result::ok) {
                    let _ = tx.send(line);
                }
            }

            let _ = stderr_handle.join();
            let _ = child.wait();
            let _ = tx.send("\x00DONE\x00".to_string());
        });
    }

    pub fn poll_push(&mut self) -> bool {
        if !self.push_running {
            return false;
        }
        let Some(ref rx) = self.push_rx else {
            self.push_running = false;
            return false;
        };

        loop {
            match rx.try_recv() {
                Ok(line) if line == "\x00DONE\x00" => {
                    self.push_rx = None;
                    self.push_running = false;
                    if let Some(ref mut out) = self.push_output {
                        out.push(String::new());
                        out.push("── Done. Press any key to close. ──".to_string());
                    }
                    return false;
                }
                Ok(line) => {
                    if let Some(ref mut out) = self.push_output {
                        out.push(line);
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.push_rx = None;
                    self.push_running = false;
                    return false;
                }
            }
        }
        true
    }

    pub fn open_log(&mut self) {
        self.log_lines = git_log(&self.repo_path).unwrap_or_default();
        self.log_scroll = 0;
        self.view = GitView::Log;
    }

    pub fn open_branches(&mut self) {
        self.branches = git_branches(&self.repo_path).unwrap_or_default();
        self.branch_list_state = ListState::default();
        if !self.branches.is_empty() {
            self.branch_list_state.select(Some(0));
        }
        self.view = GitView::Branches;
    }

    pub fn is_committing(&self) -> bool {
        self.commit_msg.is_some()
    }

    pub fn refresh(&mut self) {
        match self.view {
            GitView::RepoList => self.refresh_repos(),
            GitView::RepoDetail => {
                self.entries = git_status(&self.repo_path).unwrap_or_default();
                self.all_entries = git_all_files(&self.repo_path).unwrap_or_default();
                self.branch = git_branch(&self.repo_path).unwrap_or_else(|| "HEAD".to_string());
                let (ahead, behind) = git_ahead_behind(&self.repo_path);
                self.ahead = ahead;
                self.behind = behind;
                if !self.all_entries.is_empty() && self.list_state.selected().is_none() {
                    self.list_state.select(Some(0));
                }
            }
            GitView::Log => {
                self.log_lines = git_log(&self.repo_path).unwrap_or_default();
                self.log_scroll = 0;
            }
            GitView::Branches => {
                self.branches = git_branches(&self.repo_path).unwrap_or_default();
            }
            GitView::Workspace => {
                self.refresh_workspace();
            }
        }
    }

    pub fn move_up(&mut self) {
        match self.view {
            GitView::RepoList => {
                if let Some(i) = self.repo_list_state.selected() {
                    if i > 0 {
                        self.repo_list_state.select(Some(i - 1));
                    }
                }
            }
            GitView::RepoDetail => {
                if let Some(i) = self.list_state.selected() {
                    if i > 0 {
                        self.list_state.select(Some(i - 1));
                    }
                }
            }
            GitView::Log => {
                if self.log_scroll > 0 {
                    self.log_scroll -= 1;
                }
            }
            GitView::Branches => {
                if let Some(i) = self.branch_list_state.selected() {
                    if i > 0 {
                        self.branch_list_state.select(Some(i - 1));
                    }
                }
            }
            GitView::Workspace => {}
        }
    }

    pub fn move_down(&mut self) {
        match self.view {
            GitView::RepoList => {
                if let Some(i) = self.repo_list_state.selected() {
                    if i + 1 < self.repos.len() {
                        self.repo_list_state.select(Some(i + 1));
                    }
                }
            }
            GitView::RepoDetail => {
                if let Some(i) = self.list_state.selected() {
                    if i + 1 < self.all_entries.len() {
                        self.list_state.select(Some(i + 1));
                    }
                }
            }
            GitView::Log => {
                let max = self.log_lines.len().saturating_sub(1);
                if self.log_scroll < max {
                    self.log_scroll += 1;
                }
            }
            GitView::Branches => {
                if let Some(i) = self.branch_list_state.selected() {
                    if i + 1 < self.branches.len() {
                        self.branch_list_state.select(Some(i + 1));
                    }
                }
            }
            GitView::Workspace => {}
        }
    }

    // ── Legacy render helpers (side-panel) ────────────────────────────────────

    fn render_repo_list(&mut self, f: &mut Frame, area: Rect) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(28)])
            .split(area);

        let hints = vec![
            Line::from(""),
            Line::from(Span::styled(
                " Navigation",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    " [↑↓]   ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("navigate"),
            ]),
            Line::from(vec![
                Span::styled(
                    " [Enter]",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" view detail"),
            ]),
            Line::from(vec![
                Span::styled(
                    " [r]    ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" refresh"),
            ]),
            Line::from(vec![
                Span::styled(
                    " [Esc]  ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" menu"),
            ]),
        ];
        f.render_widget(
            Paragraph::new(hints).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Help ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            cols[1],
        );

        if self.repos.is_empty() {
            f.render_widget(
                Paragraph::new(
                    "\n Searching for git repositories in ~/ ...\n\n Press 'r' to search.",
                )
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Git — Repositories ")
                        .border_style(Style::default().fg(Color::Blue)),
                ),
                cols[0],
            );
            return;
        }

        let title = format!(" Git — {} repositories ", self.repos.len());
        let items: Vec<ListItem> = self
            .repos
            .iter()
            .map(|repo| {
                let label = repo.status_label();
                let col = if label == "clean" {
                    Color::Green
                } else {
                    Color::Yellow
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!(" {:<28}", repo.name),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(" [{:<12}]", repo.branch),
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::styled(format!("  {label}"), Style::default().fg(col)),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(Style::default().fg(Color::Blue)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");
        f.render_stateful_widget(list, cols[0], &mut self.repo_list_state);
    }

    fn render_repo_detail(&mut self, f: &mut Frame, area: Rect) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(32)])
            .split(area);

        let m = self
            .entries
            .iter()
            .filter(|e| e.status == GitStatus::Modified)
            .count();
        let s = self
            .entries
            .iter()
            .filter(|e| e.status == GitStatus::Staged)
            .count();
        let u = self
            .entries
            .iter()
            .filter(|e| e.status == GitStatus::Untracked)
            .count();
        let d = self
            .entries
            .iter()
            .filter(|e| e.status == GitStatus::Deleted)
            .count();

        let (state_color, state_label) = if self.entries.is_empty() && self.ahead == 0 {
            (Color::Green, "clean / synced")
        } else if self.entries.is_empty() && self.ahead > 0 {
            (Color::Cyan, "ready to push")
        } else {
            (Color::Yellow, "has changes")
        };

        let ahead_txt = if self.ahead > 0 {
            format!("↑{}", self.ahead)
        } else {
            "—".to_string()
        };
        let behind_txt = if self.behind > 0 {
            format!("↓{}", self.behind)
        } else {
            "—".to_string()
        };

        let right = vec![
            Line::from(""),
            Line::from(Span::styled(
                " Project Status",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled(" branch   ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    self.branch.clone(),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled(" ahead    ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    ahead_txt,
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  behind  ", Style::default().fg(Color::DarkGray)),
                Span::styled(behind_txt, Style::default().fg(Color::Red)),
            ]),
            Line::from(vec![
                Span::styled(" status   ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    state_label,
                    Style::default()
                        .fg(state_color)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    format!(" M:{m:<3}"),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" S:{s:<3}"),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" ?:{u:<3}"),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" D:{d}"),
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                " Commands",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    " [a]  ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("add ."),
            ]),
            Line::from(vec![
                Span::styled(
                    " [c]  ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("commit"),
            ]),
            Line::from(vec![
                Span::styled(
                    " [p]  ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("push -u"),
            ]),
            Line::from(vec![
                Span::styled(
                    " [l]  ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("log"),
            ]),
            Line::from(vec![
                Span::styled(
                    " [b]  ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("branches"),
            ]),
            Line::from(vec![
                Span::styled(
                    " [r]  ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("refresh"),
            ]),
            Line::from(vec![
                Span::styled(
                    " [i]  ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(".gitignore"),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    " [↑↓] ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("navigate"),
            ]),
            Line::from(vec![
                Span::styled(
                    " [Esc]",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" repo list"),
            ]),
        ];
        f.render_widget(
            Paragraph::new(right).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Status & Help ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            cols[1],
        );

        let left = cols[0];
        if self.commit_msg.is_some() {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(3)])
                .split(left);
            self.render_file_list(f, rows[0]);
            self.render_commit_box(f, rows[1]);
        } else {
            self.render_file_list(f, left);
        }
    }

    fn render_file_list(&mut self, f: &mut Frame, area: Rect) {
        let repo_name = self
            .repo_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        let changed = self.entries.len();
        let total = self.all_entries.len();
        let title = format!(
            " {} [{}]  {changed} changed / {total} total ",
            repo_name, self.branch
        );

        if self.all_entries.is_empty() {
            f.render_widget(
                Paragraph::new("\n Empty repository — no tracked files.").block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(title)
                        .border_style(Style::default().fg(Color::Green)),
                ),
                area,
            );
            return;
        }

        let items: Vec<ListItem> = self
            .all_entries
            .iter()
            .map(|e| {
                let sym = e.status.symbol();
                let col = e.status.color();
                let path_style = if e.status == GitStatus::Clean {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!(" {sym} "),
                        Style::default().fg(col).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(e.path.clone(), path_style),
                ]))
            })
            .collect();

        let border_color = if self.entries.is_empty() {
            Color::Green
        } else {
            Color::Yellow
        };

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(Style::default().fg(border_color)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");
        f.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_log_legacy(&mut self, f: &mut Frame, area: Rect) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(28)])
            .split(area);

        let hints = vec![
            Line::from(""),
            Line::from(Span::styled(
                " Git Log",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    " [↑↓]  ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("scroll"),
            ]),
            Line::from(vec![
                Span::styled(
                    " [r]   ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("refresh"),
            ]),
            Line::from(vec![
                Span::styled(
                    " [Esc] ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("back"),
            ]),
        ];
        f.render_widget(
            Paragraph::new(hints).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Help ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            cols[1],
        );

        let repo_name = self
            .repo_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let title = format!(" Git Log — {} [{}] ", repo_name, self.branch);
        let graph_set: &[char] = &['*', '|', '/', '\\', '-', '_', ' '];

        let lines: Vec<Line> = self
            .log_lines
            .iter()
            .map(|raw| {
                let prefix_end = raw
                    .char_indices()
                    .find(|(_, c)| !graph_set.contains(c))
                    .map(|(i, _)| i)
                    .unwrap_or(raw.len());
                let prefix = &raw[..prefix_end];
                let rest = &raw[prefix_end..];
                let mut spans: Vec<Span> = prefix
                    .chars()
                    .map(|ch| {
                        let color = match ch {
                            '*' => Color::Yellow,
                            '|' => Color::Blue,
                            '/' | '\\' => Color::Cyan,
                            '-' | '_' => Color::Magenta,
                            _ => Color::DarkGray,
                        };
                        Span::styled(ch.to_string(), Style::default().fg(color))
                    })
                    .collect();
                let hash_end = rest
                    .char_indices()
                    .nth(7)
                    .map(|(i, _)| i)
                    .unwrap_or(rest.len());
                if hash_end > 0 {
                    spans.push(Span::styled(
                        rest[..hash_end].to_string(),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ));
                    let after = &rest[hash_end..];
                    if let Some(open) = after.find('(') {
                        if let Some(rel_close) = after[open..].find(')') {
                            let close = open + rel_close;
                            spans.push(Span::raw(after[..open].to_string()));
                            spans.push(Span::styled(
                                after[open..=close].to_string(),
                                Style::default()
                                    .fg(Color::Green)
                                    .add_modifier(Modifier::BOLD),
                            ));
                            if close + 1 < after.len() {
                                spans.push(Span::raw(after[close + 1..].to_string()));
                            }
                        } else {
                            spans.push(Span::raw(after.to_string()));
                        }
                    } else {
                        spans.push(Span::raw(after.to_string()));
                    }
                } else {
                    spans.push(Span::raw(rest.to_string()));
                }
                Line::from(spans)
            })
            .collect();

        let widget = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(Style::default().fg(Color::Blue)),
            )
            .scroll((self.log_scroll as u16, 0));
        f.render_widget(widget, cols[0]);
    }

    fn render_branches_legacy(&mut self, f: &mut Frame, area: Rect) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(28)])
            .split(area);

        let hints = vec![
            Line::from(""),
            Line::from(Span::styled(
                " Branches",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    " [↑↓]  ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("navigate"),
            ]),
            Line::from(vec![
                Span::styled(
                    " [r]   ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("refresh"),
            ]),
            Line::from(vec![
                Span::styled(
                    " [Esc] ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("back"),
            ]),
        ];
        f.render_widget(
            Paragraph::new(hints).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Help ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            cols[1],
        );

        let repo_name = self
            .repo_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let title = format!(" Branches — {} ", repo_name);

        if self.branches.is_empty() {
            f.render_widget(
                Paragraph::new("\n No branches found.").block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(title)
                        .border_style(Style::default().fg(Color::Yellow)),
                ),
                cols[0],
            );
            return;
        }

        let items: Vec<ListItem> = self
            .branches
            .iter()
            .map(|b| {
                let style = if b.starts_with('*') {
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                } else if b.contains("remotes/") {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(Span::styled(format!(" {b}"), style)))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");
        f.render_stateful_widget(list, cols[0], &mut self.branch_list_state);
    }

    fn render_commit_box(&self, f: &mut Frame, area: Rect) {
        let msg = self.commit_msg.as_deref().unwrap_or("");
        let text = format!("{msg}_");
        let widget = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Commit message (Enter confirm · Esc cancel) ")
                    .border_style(Style::default().fg(Color::Magenta)),
            )
            .style(Style::default().fg(Color::White));
        f.render_widget(widget, area);
    }

    fn render_push_popup(&self, f: &mut Frame, area: Rect) {
        let Some(ref lines) = self.push_output else {
            return;
        };

        let popup_w = (area.width * 3 / 4)
            .max(40)
            .min(area.width.saturating_sub(4));
        let content_h = lines.len() as u16 + 2;
        let popup_h = content_h.max(6).min(area.height.saturating_sub(4));
        let popup_x = (area.width.saturating_sub(popup_w)) / 2;
        let popup_y = (area.height.saturating_sub(popup_h)) / 2;
        let popup_area = Rect::new(popup_x, popup_y, popup_w, popup_h);

        f.render_widget(Clear, popup_area);

        let text: Vec<Line> = lines
            .iter()
            .map(|l| {
                let color = if l.contains("error") || l.contains("fatal") || l.contains("rejected")
                {
                    Color::Red
                } else if l.contains("->") || l.contains("..") || l.contains("branch") {
                    Color::Green
                } else if l.starts_with("remote:") {
                    Color::Cyan
                } else {
                    Color::White
                };
                Line::from(Span::styled(format!(" {l}"), Style::default().fg(color)))
            })
            .collect();

        let widget = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" git push — output  (any key to close) ")
                    .border_style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
            )
            .wrap(Wrap { trim: false });
        f.render_widget(widget, popup_area);
    }
}

impl Plugin for GitPlugin {
    fn name(&self) -> &str {
        "Git"
    }

    fn render(&mut self, f: &mut Frame, area: Rect) {
        match self.view {
            GitView::RepoList => self.render_repo_list(f, area),
            GitView::RepoDetail => self.render_repo_detail(f, area),
            GitView::Log => self.render_log_legacy(f, area),
            GitView::Branches => self.render_branches_legacy(f, area),
            GitView::Workspace => self.render_workspace(f, area),
        }
        self.render_push_popup(f, area);
    }
}
