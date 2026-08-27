//! Service Manager module (systemd).
//!
//! Linux-only: drives `systemctl` to list and control system services.
//! On non-Linux systems (e.g. the macOS dev box) `available()` returns false
//! and the UI shows a "systemd not available on this system" fallback screen.
//!
//! Data is a snapshot (not streamed): the unit list is refreshed on mode-open
//! and on `F5`, never every frame, so the event loop is never blocked for long.

use ratatui::widgets::ListState;
use std::process::Command;

/// One systemd service unit, parsed from `systemctl list-units`.
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceUnit {
    pub name: String,
    pub load: String,
    pub active: String,
    pub sub: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SortBy {
    Name,
    Active,
}

pub struct ServiceManager {
    pub units: Vec<ServiceUnit>,
    pub list_state: ListState,
    pub sort_by: SortBy,
    pub filter: String,
    pub filtered: Vec<usize>,
    /// Last error encountered while talking to systemctl (for the sidebar).
    pub last_error: Option<String>,
}

impl ServiceManager {
    pub fn new() -> Self {
        let mut sm = Self {
            units: Vec::new(),
            list_state: ListState::default(),
            sort_by: SortBy::Name,
            filter: String::new(),
            filtered: Vec::new(),
            last_error: None,
        };
        sm.refresh();
        sm
    }

    /// Whether the systemd backend is usable on this host: compiled for Linux
    /// AND `systemctl` is resolvable in `$PATH`.
    pub fn available() -> bool {
        if !cfg!(target_os = "linux") {
            return false;
        }
        Command::new("systemctl")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Re-run `systemctl list-units` and rebuild the table. No-op-ish on
    /// non-Linux (the command will simply fail and we keep an empty list).
    pub fn refresh(&mut self) {
        if !Self::available() {
            self.units.clear();
            self.filtered.clear();
            return;
        }

        let output = Command::new("systemctl")
            .args([
                "list-units",
                "--type=service",
                "--all",
                "--no-pager",
                "--plain",
                "--no-legend",
            ])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout);
                self.units = parse_units(&text);
                self.last_error = None;
            }
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                self.last_error = Some(if err.is_empty() {
                    "systemctl list-units failed".to_string()
                } else {
                    err
                });
            }
            Err(e) => {
                self.last_error = Some(e.to_string());
            }
        }

        self.sort();
        self.apply_filter();

        if self.list_state.selected().is_none() && !self.filtered.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    /// Run `systemctl <verb> <unit>`. Returns Ok(()) on success, or Err with a
    /// human-readable stderr message on failure (caller turns it into a
    /// Notification; permission-denied hints at sudo). Never elevates.
    pub fn run_action(verb: &str, unit: &str) -> Result<(), String> {
        let output = Command::new("systemctl")
            .arg(verb)
            .arg(unit)
            .output()
            .map_err(|e| e.to_string())?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let lower = stderr.to_lowercase();
            if lower.contains("permission denied")
                || lower.contains("access denied")
                || lower.contains("authentication")
                || lower.contains("interactive authentication required")
            {
                Err(format!(
                    "Permission denied: try `sudo systemctl {verb} {unit}` ({stderr})"
                ))
            } else if stderr.is_empty() {
                Err(format!("systemctl {verb} {unit} failed"))
            } else {
                Err(stderr)
            }
        }
    }

    fn sort(&mut self) {
        match self.sort_by {
            SortBy::Name => self
                .units
                .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
            SortBy::Active => self
                .units
                .sort_by(|a, b| a.active.cmp(&b.active).then(a.name.cmp(&b.name))),
        }
    }

    fn apply_filter(&mut self) {
        let q = self.filter.to_lowercase();
        self.filtered = self
            .units
            .iter()
            .enumerate()
            .filter(|(_, u)| {
                q.is_empty()
                    || u.name.to_lowercase().contains(&q)
                    || u.description.to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect();
    }

    pub fn visible_units(&self) -> Vec<&ServiceUnit> {
        self.filtered
            .iter()
            .filter_map(|&i| self.units.get(i))
            .collect()
    }

    pub fn selected_unit(&self) -> Option<&ServiceUnit> {
        let idx = self.list_state.selected()?;
        let unit_idx = *self.filtered.get(idx)?;
        self.units.get(unit_idx)
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

    pub fn set_sort(&mut self, sort: SortBy) {
        self.sort_by = sort;
        self.sort();
        self.apply_filter();
    }

    pub fn filter_push(&mut self, c: char) {
        self.filter.push(c);
        self.apply_filter();
        self.list_state.select(if self.filtered.is_empty() {
            None
        } else {
            Some(0)
        });
    }

    pub fn filter_pop(&mut self) {
        self.filter.pop();
        self.apply_filter();
    }

    pub fn filter_clear(&mut self) {
        self.filter.clear();
        self.apply_filter();
    }
}

impl Default for ServiceManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse the output of:
/// `systemctl list-units --type=service --all --no-pager --plain --no-legend`
///
/// Columns are whitespace-separated: `UNIT LOAD ACTIVE SUB DESCRIPTION`.
/// The DESCRIPTION column may contain spaces, so we split into at most 5 parts.
/// Lines that don't have at least 4 columns are skipped.
pub fn parse_units(text: &str) -> Vec<ServiceUnit> {
    let mut units = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        // Some systemctl outputs prefix failed units with a bullet "●".
        let line = line.trim_start_matches('●').trim_start();

        let mut parts = line.split_whitespace();
        let name = match parts.next() {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        let load = match parts.next() {
            Some(s) => s.to_string(),
            None => continue,
        };
        let active = match parts.next() {
            Some(s) => s.to_string(),
            None => continue,
        };
        let sub = match parts.next() {
            Some(s) => s.to_string(),
            None => continue,
        };
        // Everything remaining is the (space-containing) description.
        let description = parts.collect::<Vec<_>>().join(" ");

        units.push(ServiceUnit {
            name,
            load,
            active,
            sub,
            description,
        });
    }
    units
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "\
accounts-daemon.service          loaded active   running Accounts Service
apparmor.service                 loaded active   exited  Load AppArmor profiles
bluetooth.service                loaded active   running Bluetooth service
cron.service                     loaded active   running Regular background program processing daemon
dbus.service                     loaded active   running D-Bus System Message Bus
nginx.service                    loaded failed   failed  A high performance web server and a reverse proxy server
ssh.service                      loaded active   running OpenBSD Secure Shell server
systemd-journald.service         loaded active   running Journal Service
unattended-upgrades.service      loaded inactive dead    Unattended Upgrades Shutdown
";

    #[test]
    fn parses_all_rows() {
        let units = parse_units(FIXTURE);
        assert_eq!(units.len(), 9);
    }

    #[test]
    fn parses_columns_correctly() {
        let units = parse_units(FIXTURE);
        let first = &units[0];
        assert_eq!(first.name, "accounts-daemon.service");
        assert_eq!(first.load, "loaded");
        assert_eq!(first.active, "active");
        assert_eq!(first.sub, "running");
        assert_eq!(first.description, "Accounts Service");
    }

    #[test]
    fn description_with_spaces_preserved() {
        let units = parse_units(FIXTURE);
        let cron = units.iter().find(|u| u.name == "cron.service").unwrap();
        assert_eq!(
            cron.description,
            "Regular background program processing daemon"
        );
    }

    #[test]
    fn parses_failed_and_inactive_states() {
        let units = parse_units(FIXTURE);
        let nginx = units.iter().find(|u| u.name == "nginx.service").unwrap();
        assert_eq!(nginx.active, "failed");
        assert_eq!(nginx.sub, "failed");

        let uu = units
            .iter()
            .find(|u| u.name == "unattended-upgrades.service")
            .unwrap();
        assert_eq!(uu.active, "inactive");
        assert_eq!(uu.sub, "dead");
    }

    #[test]
    fn handles_bullet_prefix_on_failed_units() {
        let with_bullet = "● foo.service loaded failed failed Some failed service\n";
        let units = parse_units(with_bullet);
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].name, "foo.service");
        assert_eq!(units[0].active, "failed");
        assert_eq!(units[0].description, "Some failed service");
    }

    #[test]
    fn skips_blank_and_short_lines() {
        let text = "\n  \nfoo.service loaded\n bar.service loaded active running Bar\n";
        let units = parse_units(text);
        // "foo.service loaded" has only 2 cols -> skipped; bar is valid.
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].name, "bar.service");
    }
}
