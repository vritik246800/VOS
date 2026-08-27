#![allow(dead_code)]
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum TermMsg {
    Output(String),
    Error(String),
    Done(i32),
}

pub struct TerminalPane {
    pub input: String,
    pub output: Vec<String>,
    pub history: Vec<String>,
    pub history_idx: Option<usize>,
    pub cwd: PathBuf,
    pub running: bool,
    pub tx: mpsc::UnboundedSender<TermMsg>,
    pub rx: mpsc::UnboundedReceiver<TermMsg>,
}

impl TerminalPane {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            input: String::new(),
            output: vec!["VOS Terminal — type a command".to_string()],
            history: Vec::new(),
            history_idx: None,
            cwd: std::env::current_dir().unwrap_or_default(),
            running: false,
            tx,
            rx,
        }
    }

    pub fn tick(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                TermMsg::Output(line) => self.output.push(line),
                TermMsg::Error(line) => self.output.push(format!("✗ {line}")),
                TermMsg::Done(_) => self.running = false,
            }
            if self.output.len() > 2000 {
                self.output.drain(..500);
            }
        }
    }

    pub fn execute(&mut self) {
        let cmd = self.input.trim().to_string();
        if cmd.is_empty() {
            return;
        }
        self.history.push(cmd.clone());
        self.history_idx = None;
        self.output.push(format!("$ {cmd}"));
        self.input.clear();

        if cmd.starts_with("cd") {
            let target = cmd.trim_start_matches("cd").trim();
            let new_dir = if target.is_empty() {
                dirs_path()
            } else if target.starts_with('/') {
                PathBuf::from(target)
            } else {
                self.cwd.join(target)
            };
            match std::fs::metadata(&new_dir) {
                Ok(m) if m.is_dir() => {
                    self.cwd = new_dir.canonicalize().unwrap_or(new_dir);
                    self.output.push(format!("→ {}", self.cwd.display()));
                }
                _ => self.output.push(format!("cd: {target}: no such directory")),
            }
            return;
        }

        self.running = true;
        let tx = self.tx.clone();
        let cwd = self.cwd.clone();

        tokio::spawn(async move {
            let result = Command::new("sh")
                .arg("-c")
                .arg(&cmd)
                .current_dir(&cwd)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn();

            let mut child = match result {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(TermMsg::Error(e.to_string()));
                    let _ = tx.send(TermMsg::Done(1));
                    return;
                }
            };

            let stdout = child.stdout.take().unwrap();
            let stderr = child.stderr.take().unwrap();
            let tx2 = tx.clone();

            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let _ = tx2.send(TermMsg::Error(line));
                }
            });

            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx.send(TermMsg::Output(line));
            }

            let status = child.wait().await.ok();
            let code = status.and_then(|s| s.code()).unwrap_or(0);
            let _ = tx.send(TermMsg::Done(code));
        });
    }

    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let idx = match self.history_idx {
            None => self.history.len() - 1,
            Some(i) if i > 0 => i - 1,
            Some(i) => i,
        };
        self.history_idx = Some(idx);
        self.input = self.history[idx].clone();
    }

    pub fn history_down(&mut self) {
        match self.history_idx {
            None => {}
            Some(i) if i + 1 < self.history.len() => {
                self.history_idx = Some(i + 1);
                self.input = self.history[i + 1].clone();
            }
            Some(_) => {
                self.history_idx = None;
                self.input.clear();
            }
        }
    }
}

impl Default for TerminalPane {
    fn default() -> Self {
        Self::new()
    }
}

fn dirs_path() -> PathBuf {
    std::env::var("HOME").map(PathBuf::from).unwrap_or_default()
}

/// Suspend the TUI (raw mode + alternate screen + mouse capture), run an
/// interactive external process to completion, then restore the TUI.
///
/// Used for real SSH sessions (and reusable for an external editor / `man`
/// in the future). Mouse capture is toggled too — without it, mouse escape
/// sequences leak into the child process's terminal.
///
/// `kbd_enhanced` must mirror the flag `main.rs` used when it originally
/// pushed `PushKeyboardEnhancementFlags` at startup. That protocol is only
/// ever popped once, at full program exit — if left active while a *child*
/// process (ssh, the remote shell, …) owns the terminal, arrow keys and
/// control sequences arrive as raw, un-interpreted escape codes (`^[[A`,
/// `^[[B`, …) instead of being parsed normally, corrupting the session.
/// This pops it before suspending and re-pushes it after resuming.
pub fn run_external_interactive(
    cmd: &str,
    args: &[String],
    kbd_enhanced: bool,
) -> anyhow::Result<std::process::ExitStatus> {
    use crossterm::event::{
        DisableMouseCapture, EnableMouseCapture, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    };
    use crossterm::terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    };

    if kbd_enhanced {
        crossterm::execute!(std::io::stdout(), PopKeyboardEnhancementFlags)?;
    }
    disable_raw_mode()?;
    crossterm::execute!(std::io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;

    let result = std::process::Command::new(cmd).args(args).status();

    enable_raw_mode()?;
    crossterm::execute!(std::io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    if kbd_enhanced {
        crossterm::execute!(
            std::io::stdout(),
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
    }

    Ok(result?)
}
