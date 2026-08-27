//! SSH Manager module.
//!
//! Parses `~/.ssh/config` to build a list of known hosts.  Supports
//! navigating the list and running a quick connectivity test via
//! `ssh -o BatchMode=yes -o ConnectTimeout=3 <host> true`.
//!
//! **Security note**: The `IdentityFile` path is stored as a plain string;
//! the key file itself is never read or exposed.

use std::process::Command;
use std::sync::mpsc;

/// A host stanza parsed from `~/.ssh/config`.
#[derive(Debug, Clone)]
pub struct SshHost {
    /// The `Host` keyword value (the alias).
    pub alias: String,
    /// The `HostName` directive (falls back to alias if absent).
    pub hostname: String,
    /// The `User` directive (falls back to the current OS user).
    pub user: String,
    /// The `Port` directive (defaults to 22).
    pub port: u16,
    /// Path from `IdentityFile` — the key file is **never** read.
    pub identity_file: Option<String>,
    /// Last-used timestamp sourced from SQLite (human-readable string).
    pub last_used: Option<String>,
    /// `None` = untested, `Some(true)` = reachable, `Some(false)` = unreachable.
    pub reachable: Option<bool>,
    /// Group name sourced from a preceding `# group: <name>` comment.
    pub group: Option<String>,
}

/// A single past SSH session, sourced from SQLite.
#[derive(Debug, Clone)]
pub struct SshHistoryEntry {
    pub alias: String,
    pub connected_at: String,
    pub duration_secs: i64,
    pub exit_code: Option<i32>,
}

/// An in-memory record of a session opened during this run (tab strip).
#[derive(Debug, Clone)]
pub struct SshTab {
    pub alias: String,
    pub started_at: String,
    pub duration_secs: Option<i64>,
    pub exit_code: Option<i32>,
}

/// Outcome of a background connectivity test, delivered through
/// [`SshManager`]'s internal channel and drained by [`SshManager::tick_test`].
pub struct SshTestResult {
    pub alias: String,
    pub reachable: bool,
    /// Last non-empty line of `ssh`'s stderr (or the spawn error), empty on
    /// success. Used to give the user a real reason instead of a bare ✗.
    pub detail: String,
}

/// A live local-port-forward tunnel (`ssh -N -L ...`).
pub struct SshTunnel {
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    pub alias: String,
    pub child: std::process::Child,
}

/// Which field of the connect/edit form currently has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SshConnField {
    Host,
    Username,
    Password,
    Port,
}

impl SshConnField {
    pub fn next(self) -> Self {
        match self {
            SshConnField::Host => SshConnField::Username,
            SshConnField::Username => SshConnField::Password,
            SshConnField::Password => SshConnField::Port,
            SshConnField::Port => SshConnField::Host,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            SshConnField::Host => SshConnField::Port,
            SshConnField::Username => SshConnField::Host,
            SshConnField::Password => SshConnField::Username,
            SshConnField::Port => SshConnField::Password,
        }
    }
}

/// State for the "Add/Edit Connection" form (`AppMode::SshConnectForm`) — a
/// 4-field bar (Host/Username/Password/Port). The `Password` field is
/// intentionally never read when actually connecting or persisted to
/// `~/.ssh/config` — `ssh` has no non-interactive password flag, so it
/// always prompts for the password itself; the field exists only so the
/// form visually matches what the user asked for (TAB cycles through all
/// four), and is harmlessly discarded.
pub struct SshConnForm {
    pub host: String,
    pub username: String,
    pub password: String,
    pub port: String,
    pub focus: SshConnField,
    /// `Some(existing alias)` when editing a host already in
    /// `~/.ssh/config` (Enter saves directly, no connection attempt);
    /// `None` when adding a brand-new ad-hoc connection (Enter connects
    /// first, then asks whether to save it).
    pub editing_alias: Option<String>,
}

impl SshConnForm {
    pub fn new_blank() -> Self {
        Self {
            host: String::new(),
            username: String::new(),
            password: String::new(),
            port: String::new(),
            focus: SshConnField::Host,
            editing_alias: None,
        }
    }

    pub fn from_host(host: &SshHost) -> Self {
        Self {
            host: host.hostname.clone(),
            username: host.user.clone(),
            password: String::new(),
            port: if host.port == 22 {
                String::new()
            } else {
                host.port.to_string()
            },
            focus: SshConnField::Host,
            editing_alias: Some(host.alias.clone()),
        }
    }

    pub fn focused_field_mut(&mut self) -> &mut String {
        match self.focus {
            SshConnField::Host => &mut self.host,
            SshConnField::Username => &mut self.username,
            SshConnField::Password => &mut self.password,
            SshConnField::Port => &mut self.port,
        }
    }

    /// Parsed port, defaulting to 22 when blank or unparsable.
    pub fn port_or_default(&self) -> u16 {
        self.port.trim().parse().unwrap_or(22)
    }
}

/// State for the SSH Manager panel.
pub struct SshManager {
    pub hosts: Vec<SshHost>,
    pub selected: usize,
    pub status_msg: String,
    /// `true` while a connectivity test is running.
    pub testing: bool,
    /// Past sessions loaded from SQLite.
    pub history: Vec<SshHistoryEntry>,
    /// `true` while the history popup is visible.
    pub history_open: bool,
    pub history_selected: usize,
    /// Sessions opened during this run (tab strip), most-recent last.
    pub tabs: Vec<SshTab>,
    pub tabs_selected: usize,
    /// Live local-port-forward tunnels.
    pub tunnels: Vec<SshTunnel>,
    /// `true` while the tunnels popup is visible.
    pub tunnels_open: bool,
    pub tunnels_selected: usize,
    /// Group names currently collapsed in the host list.
    pub collapsed_groups: std::collections::HashSet<String>,
    /// `Some` while the Add/Edit Connection form (`AppMode::SshConnectForm`)
    /// is open.
    pub conn_form: Option<SshConnForm>,
    /// Background-test channel — kept alive for the manager's whole
    /// lifetime so `test_connectivity` never blocks the UI thread.
    test_tx: mpsc::Sender<SshTestResult>,
    test_rx: mpsc::Receiver<SshTestResult>,
}

/// Cap on the number of in-memory session tabs retained by
/// [`SshManager::record_session_inmem`].
const MAX_TABS: usize = 20;

impl SshManager {
    /// Create a new manager and immediately load `~/.ssh/config`.
    pub fn new() -> Self {
        let hosts = load_ssh_config();
        let (test_tx, test_rx) = mpsc::channel();
        Self {
            hosts,
            selected: 0,
            status_msg: String::new(),
            testing: false,
            history: Vec::new(),
            history_open: false,
            history_selected: 0,
            tabs: Vec::new(),
            tabs_selected: 0,
            tunnels: Vec::new(),
            tunnels_open: false,
            tunnels_selected: 0,
            collapsed_groups: std::collections::HashSet::new(),
            conn_form: None,
            test_tx,
            test_rx,
        }
    }

    /// Record a session that just ended, keeping at most the last
    /// [`MAX_TABS`] entries. Selects the newly pushed tab.
    pub fn record_session_inmem(
        &mut self,
        alias: &str,
        started_at: String,
        duration_secs: i64,
        exit_code: Option<i32>,
    ) {
        self.tabs.push(SshTab {
            alias: alias.to_string(),
            started_at,
            duration_secs: Some(duration_secs),
            exit_code,
        });
        if self.tabs.len() > MAX_TABS {
            let overflow = self.tabs.len() - MAX_TABS;
            self.tabs.drain(0..overflow);
        }
        self.tabs_selected = self.tabs.len() - 1;
    }

    /// Replace the loaded history list and reset the selection.
    pub fn set_history(&mut self, entries: Vec<SshHistoryEntry>) {
        self.history = entries;
        self.history_selected = 0;
    }

    /// Toggle whether `group` is collapsed in the host list.
    pub fn toggle_group_collapse(&mut self, group: &str) {
        if self.collapsed_groups.contains(group) {
            self.collapsed_groups.remove(group);
        } else {
            self.collapsed_groups.insert(group.to_string());
        }
    }

    /// Open a local port-forward tunnel via `ssh -N -L <local>:<remote_host>:<remote_port> <alias>`.
    pub fn create_tunnel(
        &mut self,
        local_port: u16,
        remote_host: String,
        remote_port: u16,
        alias: String,
    ) -> anyhow::Result<()> {
        let child = Command::new("ssh")
            .args([
                "-N",
                "-L",
                &format!("{local_port}:{remote_host}:{remote_port}"),
                &alias,
            ])
            .spawn()?;

        self.tunnels.push(SshTunnel {
            local_port,
            remote_host,
            remote_port,
            alias,
            child,
        });
        Ok(())
    }

    /// Reap exited tunnel processes, dropping them from `self.tunnels`.
    /// Errors from `try_wait` are treated as "still alive" — never panics.
    pub fn tick_tunnels(&mut self) {
        self.tunnels
            .retain_mut(|t| !matches!(t.child.try_wait(), Ok(Some(_))));
    }

    /// Kill and remove the tunnel at `idx`, if present. Ignores kill errors
    /// (an already-dead process is fine).
    pub fn kill_tunnel(&mut self, idx: usize) {
        if idx < self.tunnels.len() {
            let mut t = self.tunnels.remove(idx);
            let _ = t.child.kill();
            self.tunnels_selected = self
                .tunnels_selected
                .min(self.tunnels.len().saturating_sub(1));
        }
    }

    /// Move selection one row up.
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Move selection one row down.
    pub fn move_down(&mut self) {
        if !self.hosts.is_empty() && self.selected + 1 < self.hosts.len() {
            self.selected += 1;
        }
    }

    /// Re-parse `~/.ssh/config` and reset selection.
    pub fn reload(&mut self) {
        self.hosts = load_ssh_config();
        self.selected = 0;
        self.status_msg = format!("Loaded {} host(s)", self.hosts.len());
    }

    /// Start a background connectivity test on host `idx` and return
    /// immediately — never blocks the UI thread. A no-op while a test is
    /// already running (avoids overlapping threads racing to update state).
    ///
    /// Uses `ssh -o BatchMode=yes -o ConnectTimeout=3 <alias> true`.
    /// `BatchMode` prevents interactive password prompts; the 3-second
    /// timeout bounds the background thread's lifetime. `output()` (not
    /// `status()`) is used so stderr can be surfaced as a real failure
    /// reason instead of a bare ✗ — see [`SshManager::tick_test`].
    pub fn test_connectivity(&mut self, idx: usize) {
        if self.testing {
            return;
        }
        let Some(host) = self.hosts.get(idx) else {
            return;
        };
        let alias = host.alias.clone();

        self.testing = true;
        self.status_msg = format!("Testing {alias}…");

        let tx = self.test_tx.clone();
        std::thread::spawn(move || {
            let output = Command::new("ssh")
                .args([
                    "-o",
                    "BatchMode=yes",
                    "-o",
                    "ConnectTimeout=3",
                    &alias,
                    "true",
                ])
                .output();

            let (reachable, detail) = match output {
                Ok(o) if o.status.success() => (true, String::new()),
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    let line = stderr
                        .lines()
                        .rev()
                        .map(str::trim)
                        .find(|l| !l.is_empty())
                        .unwrap_or("")
                        .to_string();
                    (false, line)
                }
                Err(e) => (false, e.to_string()),
            };

            let _ = tx.send(SshTestResult {
                alias,
                reachable,
                detail,
            });
        });
    }

    /// Drain the background-test channel, applying at most all pending
    /// results to `hosts`/`status_msg`. Returns the last result observed (if
    /// any) so the caller can decide whether to surface a failure prominently
    /// (e.g. as a dialog) — never panics on a transport error.
    pub fn tick_test(&mut self) -> Option<SshTestResult> {
        let mut last = None;
        while let Ok(result) = self.test_rx.try_recv() {
            self.testing = false;
            if let Some(h) = self.hosts.iter_mut().find(|h| h.alias == result.alias) {
                h.reachable = Some(result.reachable);
            }
            self.status_msg = if result.reachable {
                format!("{}: reachable", result.alias)
            } else {
                format!("{}: unreachable", result.alias)
            };
            last = Some(result);
        }
        last
    }
}

impl Default for SshManager {
    fn default() -> Self {
        Self::new()
    }
}

// ── Parser ────────────────────────────────────────────────────────────────────

/// Parse `~/.ssh/config` and return one [`SshHost`] per non-wildcard stanza.
///
/// Parsing rules:
/// - A line starting with `Host` (case-insensitive) begins a new stanza.
/// - `Host *` is skipped (wildcard / global defaults).
/// - Subsequent lines that are indented (or plain key-value lines within a
///   stanza) are inspected for `HostName`, `User`, `Port`, `IdentityFile`.
/// - If the file does not exist the function returns an empty `Vec`.
pub fn load_ssh_config() -> Vec<SshHost> {
    let content = match std::fs::read_to_string(config_path()) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    parse_ssh_config(&content)
}

/// Path to the user's `~/.ssh/config`.
fn config_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::PathBuf::from(home).join(".ssh/config")
}

/// Read the current `~/.ssh/config` content (empty string if missing).
pub fn read_ssh_config() -> String {
    std::fs::read_to_string(config_path()).unwrap_or_default()
}

/// Write `content` to `~/.ssh/config`, backing up the previous version to
/// `data/ssh_config.bak` first — same safety pattern as the Cron Editor's
/// `data/crontab.bak`. Editing the user's real SSH config is a real-world,
/// hard-to-reverse action, so it's never written without a backup.
pub fn write_ssh_config(content: &str) -> anyhow::Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if path.exists() {
        let backup = std::path::Path::new("data/ssh_config.bak");
        if let Some(parent) = backup.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&path, backup)?;
    }
    std::fs::write(&path, content)?;
    Ok(())
}

/// Append a new `Host` stanza for `alias`. `hostname`/`user` are omitted
/// when empty (or, for `hostname`, when identical to `alias` — redundant);
/// `port` is omitted when `0` or `22` (the default). Pure — operates on the
/// in-memory config text so it's unit-testable without touching the
/// filesystem; the caller persists the result via [`write_ssh_config`].
pub fn append_host_block(
    content: &str,
    alias: &str,
    hostname: &str,
    user: &str,
    port: u16,
) -> String {
    let mut out = content.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&format!("Host {alias}\n"));
    if !hostname.is_empty() && hostname != alias {
        out.push_str(&format!("    HostName {hostname}\n"));
    }
    if !user.is_empty() {
        out.push_str(&format!("    User {user}\n"));
    }
    if port != 0 && port != 22 {
        out.push_str(&format!("    Port {port}\n"));
    }
    out
}

/// Locate the `Host <alias>` stanza: returns `(start, end)` line indices
/// (into `content.lines()`) — `start` is the `Host` line itself, `end` is
/// the first line of the next stanza (or `lines().len()` at EOF). `None` if
/// `alias` isn't present.
fn find_host_block(lines: &[&str], alias: &str) -> Option<(usize, usize)> {
    let mut start = None;
    let mut end = lines.len();
    for (i, raw) in lines.iter().enumerate() {
        let line = raw.trim();
        let mut parts = line.splitn(2, |c: char| c.is_whitespace());
        let key = parts.next().unwrap_or("").to_lowercase();
        let value = parts.next().unwrap_or("").trim();
        if key == "host" && value == alias {
            start = Some(i);
        } else if start.is_some() && key == "host" {
            end = i;
            break;
        }
    }
    start.map(|s| (s, end))
}

/// Remove the `Host <alias>` stanza (and its immediately preceding
/// `# group: …` comment line, if any — it belongs to this stanza per our
/// own parsing convention) from `content`. A no-op (returns `content`
/// unchanged) if `alias` isn't found.
pub fn remove_host_block(content: &str, alias: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let Some((mut start, end)) = find_host_block(&lines, alias) else {
        return content.to_string();
    };
    if start > 0 {
        let prev = lines[start - 1].trim();
        if let Some(rest) = prev.strip_prefix('#') {
            if rest.trim().to_lowercase().starts_with("group:") {
                start -= 1;
            }
        }
    }
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    out.extend_from_slice(&lines[..start]);
    out.extend_from_slice(&lines[end..]);
    let mut result = out.join("\n");
    if content.ends_with('\n') && !result.is_empty() {
        result.push('\n');
    }
    result
}

/// Rewrite the `HostName`/`User`/`Port` lines of an existing `Host <alias>`
/// stanza, leaving `IdentityFile` and any other directives untouched. Falls
/// back to [`append_host_block`] if `alias` isn't found (so callers can
/// treat "update" as "upsert" without checking existence first).
pub fn update_host_block(
    content: &str,
    alias: &str,
    hostname: &str,
    user: &str,
    port: u16,
) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let Some((start, end)) = find_host_block(&lines, alias) else {
        return append_host_block(content, alias, hostname, user, port);
    };

    let mut new_block: Vec<String> = vec![lines[start].to_string()];
    let mut wrote_hostname = false;
    let mut wrote_user = false;
    let mut wrote_port = false;

    for raw in &lines[start + 1..end] {
        let line = raw.trim();
        let key = line
            .split(|c: char| c.is_whitespace())
            .next()
            .unwrap_or("")
            .to_lowercase();
        match key.as_str() {
            "hostname" => {
                if !hostname.is_empty() && hostname != alias {
                    new_block.push(format!("    HostName {hostname}"));
                }
                wrote_hostname = true;
            }
            "user" => {
                if !user.is_empty() {
                    new_block.push(format!("    User {user}"));
                }
                wrote_user = true;
            }
            "port" => {
                if port != 0 && port != 22 {
                    new_block.push(format!("    Port {port}"));
                }
                wrote_port = true;
            }
            _ => new_block.push((*raw).to_string()),
        }
    }
    if !wrote_hostname && !hostname.is_empty() && hostname != alias {
        new_block.push(format!("    HostName {hostname}"));
    }
    if !wrote_user && !user.is_empty() {
        new_block.push(format!("    User {user}"));
    }
    if !wrote_port && port != 0 && port != 22 {
        new_block.push(format!("    Port {port}"));
    }

    let mut out: Vec<String> = lines[..start].iter().map(|s| s.to_string()).collect();
    out.extend(new_block);
    out.extend(lines[end..].iter().map(|s| s.to_string()));
    let mut result = out.join("\n");
    if content.ends_with('\n') && !result.is_empty() {
        result.push('\n');
    }
    result
}

/// Pure parser — accepts a config file as a string slice.
/// Exposed so it can be used in unit tests without touching the filesystem.
pub fn parse_ssh_config(content: &str) -> Vec<SshHost> {
    let current_user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "user".to_string());

    let mut hosts: Vec<SshHost> = Vec::new();
    // Pending stanza being built; `None` when we are between stanzas or on
    // a wildcard block.
    let mut current: Option<SshHost> = None;
    // Group name carried from a `# group: <name>` comment to the next
    // `Host` stanza. Cleared (via `.take()`) when consumed by a `Host` line.
    let mut pending_group: Option<String> = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();

        if line.is_empty() {
            continue;
        }

        // Comments: only `# group: <name>` (case-insensitive on "group")
        // has meaning — it sets `pending_group` for the next stanza. Any
        // other comment is ignored.
        if let Some(rest) = line.strip_prefix('#') {
            let comment = rest.trim();
            if comment.len() >= 6 && comment[..6].eq_ignore_ascii_case("group:") {
                let name = comment[6..].trim();
                if !name.is_empty() {
                    pending_group = Some(name.to_string());
                }
            }
            continue;
        }

        // Split on the first run of whitespace.
        let mut parts = line.splitn(2, |c: char| c.is_whitespace());
        let key = parts.next().unwrap_or("").to_lowercase();
        let value = parts.next().unwrap_or("").trim();

        if key == "host" {
            // Commit the previous stanza (if any).
            if let Some(h) = current.take() {
                hosts.push(h);
            }

            // A new stanza begins here — the pending group (if any) belongs
            // to it (or is discarded, for a wildcard block) and must not
            // leak to whatever stanza comes after.
            let group = pending_group.take();

            // Skip wildcard blocks.
            if value == "*" {
                current = None;
                continue;
            }

            current = Some(SshHost {
                alias: value.to_string(),
                hostname: value.to_string(), // default = alias
                user: current_user.clone(),
                port: 22,
                identity_file: None,
                last_used: None,
                reachable: None,
                group,
            });
        } else if let Some(ref mut h) = current {
            match key.as_str() {
                "hostname" => h.hostname = value.to_string(),
                "user" => h.user = value.to_string(),
                "port" => {
                    if let Ok(p) = value.parse::<u16>() {
                        h.port = p;
                    }
                }
                "identityfile" => h.identity_file = Some(value.to_string()),
                _ => {}
            }
        }
    }

    // Commit the last stanza.
    if let Some(h) = current.take() {
        hosts.push(h);
    }

    hosts
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"
# Global defaults
Host *
    ServerAliveInterval 60

Host web-prod
    HostName 10.0.1.100
    User deploy
    Port 2222
    IdentityFile ~/.ssh/deploy_key

Host db-replica
    HostName db.example.com
    User postgres
    # Port not set — should default to 22

Host bastion
    HostName bastion.example.com
    User admin
    Port 443
    IdentityFile ~/.ssh/id_rsa
"#;

    #[test]
    fn test_parses_three_hosts() {
        let hosts = parse_ssh_config(FIXTURE);
        assert_eq!(hosts.len(), 3, "should parse 3 non-wildcard stanzas");
    }

    #[test]
    fn test_web_prod_fields() {
        let hosts = parse_ssh_config(FIXTURE);
        let h = hosts
            .iter()
            .find(|h| h.alias == "web-prod")
            .expect("web-prod not found");
        assert_eq!(h.hostname, "10.0.1.100");
        assert_eq!(h.user, "deploy");
        assert_eq!(h.port, 2222);
        assert_eq!(h.identity_file.as_deref(), Some("~/.ssh/deploy_key"));
    }

    #[test]
    fn test_db_replica_default_port() {
        let hosts = parse_ssh_config(FIXTURE);
        let h = hosts
            .iter()
            .find(|h| h.alias == "db-replica")
            .expect("db-replica not found");
        assert_eq!(h.hostname, "db.example.com");
        assert_eq!(h.user, "postgres");
        assert_eq!(h.port, 22, "port should default to 22");
        assert!(h.identity_file.is_none());
    }

    #[test]
    fn test_bastion_fields() {
        let hosts = parse_ssh_config(FIXTURE);
        let h = hosts
            .iter()
            .find(|h| h.alias == "bastion")
            .expect("bastion not found");
        assert_eq!(h.hostname, "bastion.example.com");
        assert_eq!(h.user, "admin");
        assert_eq!(h.port, 443);
        assert_eq!(h.identity_file.as_deref(), Some("~/.ssh/id_rsa"));
    }

    #[test]
    fn test_empty_config() {
        let hosts = parse_ssh_config("");
        assert!(hosts.is_empty());
    }

    #[test]
    fn test_wildcard_only() {
        let input = "Host *\n    ServerAliveInterval 60\n";
        let hosts = parse_ssh_config(input);
        assert!(
            hosts.is_empty(),
            "wildcard stanza should not produce a host"
        );
    }

    const GROUP_FIXTURE: &str = r#"
# group: prod
Host web-prod
    HostName 10.0.1.100
    User deploy

Host bastion
    HostName bastion.example.com
    User admin
"#;

    #[test]
    fn test_group_comment_assigns_group() {
        let hosts = parse_ssh_config(GROUP_FIXTURE);
        let h = hosts
            .iter()
            .find(|h| h.alias == "web-prod")
            .expect("web-prod not found");
        assert_eq!(h.group.as_deref(), Some("prod"));
    }

    #[test]
    fn test_no_group_comment_leaves_group_none() {
        let hosts = parse_ssh_config(GROUP_FIXTURE);
        let h = hosts
            .iter()
            .find(|h| h.alias == "bastion")
            .expect("bastion not found");
        assert_eq!(h.group, None, "group comment must not leak to next host");
    }

    #[test]
    fn test_move_up_down() {
        let content = "Host a\n    HostName a.example.com\nHost b\n    HostName b.example.com\n";
        let (test_tx, test_rx) = mpsc::channel();
        let mut mgr = SshManager {
            hosts: parse_ssh_config(content),
            selected: 0,
            status_msg: String::new(),
            testing: false,
            history: Vec::new(),
            history_open: false,
            history_selected: 0,
            tabs: Vec::new(),
            tabs_selected: 0,
            tunnels: Vec::new(),
            tunnels_open: false,
            tunnels_selected: 0,
            collapsed_groups: std::collections::HashSet::new(),
            conn_form: None,
            test_tx,
            test_rx,
        };

        assert_eq!(mgr.selected, 0);
        mgr.move_up(); // no-op at top
        assert_eq!(mgr.selected, 0);
        mgr.move_down();
        assert_eq!(mgr.selected, 1);
        mgr.move_down(); // no-op at bottom
        assert_eq!(mgr.selected, 1);
        mgr.move_up();
        assert_eq!(mgr.selected, 0);
    }

    #[test]
    fn test_append_host_block_on_empty_config() {
        let out = append_host_block("", "newbox", "10.0.0.5", "deploy", 2222);
        assert_eq!(
            out,
            "Host newbox\n    HostName 10.0.0.5\n    User deploy\n    Port 2222\n"
        );
    }

    #[test]
    fn test_append_host_block_omits_default_port_and_redundant_hostname() {
        let out = append_host_block("", "example.com", "example.com", "", 22);
        assert_eq!(out, "Host example.com\n");
    }

    #[test]
    fn test_append_host_block_preserves_existing_content() {
        let existing = "Host a\n    HostName a.example.com\n";
        let out = append_host_block(existing, "b", "b.example.com", "bob", 22);
        let hosts = parse_ssh_config(&out);
        assert_eq!(hosts.len(), 2);
        assert!(hosts.iter().any(|h| h.alias == "a"));
        let b = hosts.iter().find(|h| h.alias == "b").unwrap();
        assert_eq!(b.hostname, "b.example.com");
        assert_eq!(b.user, "bob");
    }

    #[test]
    fn test_remove_host_block_removes_only_target() {
        let content = "Host a\n    HostName a.example.com\n\nHost b\n    HostName b.example.com\n\nHost c\n    HostName c.example.com\n";
        let out = remove_host_block(content, "b");
        let hosts = parse_ssh_config(&out);
        assert_eq!(hosts.len(), 2);
        assert!(hosts.iter().all(|h| h.alias != "b"));
        assert!(hosts.iter().any(|h| h.alias == "a"));
        assert!(hosts.iter().any(|h| h.alias == "c"));
    }

    #[test]
    fn test_remove_host_block_drops_preceding_group_comment() {
        let content = "# group: prod\nHost b\n    HostName b.example.com\n\nHost c\n    HostName c.example.com\n";
        let out = remove_host_block(content, "b");
        assert!(!out.contains("group: prod"));
        let hosts = parse_ssh_config(&out);
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].alias, "c");
    }

    #[test]
    fn test_remove_host_block_missing_alias_is_noop() {
        let content = "Host a\n    HostName a.example.com\n";
        let out = remove_host_block(content, "does-not-exist");
        assert_eq!(out, content);
    }

    #[test]
    fn test_update_host_block_rewrites_existing_directives() {
        let content = "Host a\n    HostName old.example.com\n    User olduser\n    IdentityFile ~/.ssh/id_rsa\n";
        let out = update_host_block(content, "a", "new.example.com", "newuser", 2222);
        let hosts = parse_ssh_config(&out);
        let a = hosts.iter().find(|h| h.alias == "a").unwrap();
        assert_eq!(a.hostname, "new.example.com");
        assert_eq!(a.user, "newuser");
        assert_eq!(a.port, 2222);
        // IdentityFile is untouched.
        assert_eq!(a.identity_file.as_deref(), Some("~/.ssh/id_rsa"));
    }

    #[test]
    fn test_update_host_block_only_touches_target_stanza() {
        let content = "Host a\n    HostName a.example.com\n\nHost b\n    HostName b.example.com\n";
        let out = update_host_block(content, "a", "a2.example.com", "", 22);
        let hosts = parse_ssh_config(&out);
        assert_eq!(hosts.len(), 2);
        let a = hosts.iter().find(|h| h.alias == "a").unwrap();
        assert_eq!(a.hostname, "a2.example.com");
        let b = hosts.iter().find(|h| h.alias == "b").unwrap();
        assert_eq!(b.hostname, "b.example.com");
    }

    #[test]
    fn test_update_host_block_falls_back_to_append_when_missing() {
        let content = "Host a\n    HostName a.example.com\n";
        let out = update_host_block(content, "new", "new.example.com", "", 22);
        let hosts = parse_ssh_config(&out);
        assert_eq!(hosts.len(), 2);
        assert!(hosts.iter().any(|h| h.alias == "new"));
    }
}
