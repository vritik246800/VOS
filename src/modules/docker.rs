//! Docker Panel module.
//!
//! Drives the `docker` CLI to list containers and images.
//! Uses tab-delimited `--format` strings (no serde_json dependency).
//! On hosts where Docker is absent or the daemon is not running,
//! `available` is set to `false` and the UI shows a graceful fallback.

use std::process::Command;

// ── Data types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub ports: String,
    pub created: String,
}

#[derive(Debug, Clone)]
pub struct DockerImage {
    pub id: String,
    pub repository: String,
    pub tag: String,
    pub size: String,
    pub created: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DockerTab {
    Containers,
    Images,
    Volumes,
}

// ── Main panel state ──────────────────────────────────────────────────────────

pub struct DockerPanel {
    /// `false` when docker binary is absent or the daemon is not responding.
    pub available: bool,
    pub tab: DockerTab,
    pub containers: Vec<Container>,
    pub images: Vec<DockerImage>,
    pub selected: usize,
    /// Whether to show all containers (`docker ps -a`).
    pub show_all: bool,
    pub status_msg: String,
    pub error: Option<String>,
}

impl DockerPanel {
    /// Create a new panel.  Detects whether `docker` is in PATH; if so,
    /// verifies the daemon is running and performs an initial refresh.
    pub fn new() -> Self {
        let binary_found = which_docker();
        let available = binary_found && is_daemon_running();

        let mut panel = Self {
            available,
            tab: DockerTab::Containers,
            containers: Vec::new(),
            images: Vec::new(),
            selected: 0,
            show_all: true,
            status_msg: String::new(),
            error: None,
        };

        if available {
            panel.refresh_containers();
        }

        panel
    }

    // ── Refresh helpers ───────────────────────────────────────────────────────

    /// Re-run `docker ps` (with `-a` when `show_all`) and repopulate
    /// `self.containers`.  Clears `self.error` on success.
    pub fn refresh_containers(&mut self) {
        let format = "{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.Status}}\t{{.Ports}}\t{{.CreatedAt}}";
        let mut cmd = Command::new("docker");
        cmd.args(["ps", "--format", format]);
        if self.show_all {
            cmd.arg("-a");
        }

        match cmd.output() {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout);
                self.containers = parse_containers(&text);
                self.error = None;
                self.status_msg = format!("{} containers", self.containers.len());
                // Clamp selection.
                if self.selected >= self.containers.len() {
                    self.selected = self.containers.len().saturating_sub(1);
                }
            }
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                self.error = Some(if err.is_empty() {
                    "docker ps failed".to_string()
                } else {
                    err
                });
            }
            Err(e) => {
                self.error = Some(e.to_string());
            }
        }
    }

    /// Re-run `docker images` and repopulate `self.images`.
    pub fn refresh_images(&mut self) {
        let format = "{{.ID}}\t{{.Repository}}\t{{.Tag}}\t{{.Size}}\t{{.CreatedAt}}";
        match Command::new("docker")
            .args(["images", "--format", format])
            .output()
        {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout);
                self.images = parse_images(&text);
                self.error = None;
                self.status_msg = format!("{} images", self.images.len());
                if self.selected >= self.images.len() {
                    self.selected = self.images.len().saturating_sub(1);
                }
            }
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                self.error = Some(if err.is_empty() {
                    "docker images failed".to_string()
                } else {
                    err
                });
            }
            Err(e) => {
                self.error = Some(e.to_string());
            }
        }
    }

    /// Refresh whichever tab is currently active.
    pub fn refresh(&mut self) {
        match self.tab {
            DockerTab::Containers => self.refresh_containers(),
            DockerTab::Images => self.refresh_images(),
            DockerTab::Volumes => {} // not implemented yet
        }
    }

    // ── Actions ───────────────────────────────────────────────────────────────

    /// Run `docker <action> <id>` (e.g. start/stop/restart/rm).
    /// Returns the trimmed stdout on success, or an `anyhow::Error` on failure.
    pub fn run_action(&mut self, action: &str, id: &str) -> anyhow::Result<String> {
        let out = Command::new("docker")
            .arg(action)
            .arg(id)
            .output()
            .map_err(|e| anyhow::anyhow!("failed to spawn docker: {e}"))?;

        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            self.status_msg = format!("docker {action} {id}: OK");
            self.error = None;
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            let msg = if stderr.is_empty() {
                format!("docker {action} {id} failed (exit {:?})", out.status.code())
            } else {
                stderr
            };
            self.error = Some(msg.clone());
            Err(anyhow::anyhow!("{msg}"))
        }
    }

    // ── Navigation ────────────────────────────────────────────────────────────

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let max = match self.tab {
            DockerTab::Containers => self.containers.len(),
            DockerTab::Images => self.images.len(),
            DockerTab::Volumes => 0,
        };
        if max > 0 && self.selected + 1 < max {
            self.selected += 1;
        }
    }

    /// Cycle to the next tab and reset selection.
    pub fn next_tab(&mut self) {
        self.tab = match self.tab {
            DockerTab::Containers => DockerTab::Images,
            DockerTab::Images => DockerTab::Volumes,
            DockerTab::Volumes => DockerTab::Containers,
        };
        self.selected = 0;
        self.refresh();
    }

    // ── Convenience getters ───────────────────────────────────────────────────

    pub fn selected_container_id(&self) -> Option<&str> {
        self.containers.get(self.selected).map(|c| c.id.as_str())
    }

    pub fn selected_image_id(&self) -> Option<&str> {
        self.images.get(self.selected).map(|i| i.id.as_str())
    }
}

impl Default for DockerPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ── Helper functions ──────────────────────────────────────────────────────────

/// Returns `true` when the `docker` binary is found in `$PATH`.
fn which_docker() -> bool {
    Command::new("docker")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Returns `true` when the Docker daemon is reachable (`docker info` succeeds).
pub fn is_daemon_running() -> bool {
    Command::new("docker")
        .arg("info")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ── Parsers ───────────────────────────────────────────────────────────────────

/// Parse `docker ps --format "{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.Status}}\t{{.Ports}}\t{{.CreatedAt}}"`.
///
/// Lines with fewer than 4 tab-separated columns are accepted; missing trailing
/// fields default to an empty string.
pub fn parse_containers(text: &str) -> Vec<Container> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.splitn(6, '\t');
        let id = match parts.next() {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        let name = parts.next().unwrap_or("").to_string();
        let image = parts.next().unwrap_or("").to_string();
        let status = parts.next().unwrap_or("").to_string();
        let ports = parts.next().unwrap_or("").to_string();
        let created = parts.next().unwrap_or("").to_string();
        out.push(Container {
            id,
            name,
            image,
            status,
            ports,
            created,
        });
    }
    out
}

/// Parse `docker images --format "{{.ID}}\t{{.Repository}}\t{{.Tag}}\t{{.Size}}\t{{.CreatedAt}}"`.
///
/// Lines with fewer than expected columns are accepted; missing trailing fields
/// default to an empty string.
pub fn parse_images(text: &str) -> Vec<DockerImage> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.splitn(5, '\t');
        let id = match parts.next() {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        let repository = parts.next().unwrap_or("").to_string();
        let tag = parts.next().unwrap_or("").to_string();
        let size = parts.next().unwrap_or("").to_string();
        let created = parts.next().unwrap_or("").to_string();
        out.push(DockerImage {
            id,
            repository,
            tag,
            size,
            created,
        });
    }
    out
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Container parser ──────────────────────────────────────────────────────

    const CONTAINERS_FIXTURE: &str = "\
abc123def456\tmy_web\tnginx:latest\tUp 3 hours\t0.0.0.0:80->80/tcp\t2024-01-10 12:00:00 +0000 UTC\n\
dead000beef0\tmy_db\tpostgres:15\tExited (0) 2 minutes ago\t5432/tcp\t2024-01-09 08:00:00 +0000 UTC\n\
cafe111aaa00\tmy_cache\tredis:7\tUp 5 days\t\t2024-01-05 00:00:00 +0000 UTC\n\
";

    #[test]
    fn parses_all_container_rows() {
        let cs = parse_containers(CONTAINERS_FIXTURE);
        assert_eq!(cs.len(), 3);
    }

    #[test]
    fn parses_container_columns_correctly() {
        let cs = parse_containers(CONTAINERS_FIXTURE);
        let first = &cs[0];
        assert_eq!(first.id, "abc123def456");
        assert_eq!(first.name, "my_web");
        assert_eq!(first.image, "nginx:latest");
        assert_eq!(first.status, "Up 3 hours");
        assert_eq!(first.ports, "0.0.0.0:80->80/tcp");
        assert_eq!(first.created, "2024-01-10 12:00:00 +0000 UTC");
    }

    #[test]
    fn parses_container_with_empty_ports() {
        let cs = parse_containers(CONTAINERS_FIXTURE);
        let cache = &cs[2];
        assert_eq!(cache.name, "my_cache");
        assert_eq!(cache.ports, "");
    }

    #[test]
    fn container_parser_skips_blank_lines() {
        let text = "\n  \nabc\tname\timage\tUp\t\t2024-01-01\n";
        let cs = parse_containers(text);
        assert_eq!(cs.len(), 1);
    }

    #[test]
    fn container_parser_handles_missing_trailing_fields() {
        // Only id + name: the remaining fields default to "".
        let text = "id001\tmy_container\n";
        let cs = parse_containers(text);
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].image, "");
        assert_eq!(cs[0].ports, "");
    }

    // ── Image parser ──────────────────────────────────────────────────────────

    const IMAGES_FIXTURE: &str = "\
sha256:aaa111\tnginx\tlatest\t141MB\t2024-01-08 10:00:00 +0000 UTC\n\
sha256:bbb222\tpostgres\t15\t379MB\t2024-01-07 09:00:00 +0000 UTC\n\
sha256:ccc333\tredis\t7-alpine\t29.9MB\t2024-01-06 08:00:00 +0000 UTC\n\
";

    #[test]
    fn parses_all_image_rows() {
        let imgs = parse_images(IMAGES_FIXTURE);
        assert_eq!(imgs.len(), 3);
    }

    #[test]
    fn parses_image_columns_correctly() {
        let imgs = parse_images(IMAGES_FIXTURE);
        let first = &imgs[0];
        assert_eq!(first.id, "sha256:aaa111");
        assert_eq!(first.repository, "nginx");
        assert_eq!(first.tag, "latest");
        assert_eq!(first.size, "141MB");
        assert_eq!(first.created, "2024-01-08 10:00:00 +0000 UTC");
    }

    #[test]
    fn image_parser_skips_blank_lines() {
        let text = "\n\nsha256:abc\trepo\ttag\t10MB\t2024-01-01\n\n";
        let imgs = parse_images(text);
        assert_eq!(imgs.len(), 1);
    }

    #[test]
    fn image_parser_handles_missing_trailing_fields() {
        let text = "sha256:xyz\trepo\n";
        let imgs = parse_images(text);
        assert_eq!(imgs.len(), 1);
        assert_eq!(imgs[0].tag, "");
        assert_eq!(imgs[0].size, "");
    }

    // ── Tab cycling ───────────────────────────────────────────────────────────

    #[test]
    fn tab_enum_equality() {
        assert_eq!(DockerTab::Containers, DockerTab::Containers);
        assert_ne!(DockerTab::Containers, DockerTab::Images);
        assert_ne!(DockerTab::Images, DockerTab::Volumes);
    }
}
