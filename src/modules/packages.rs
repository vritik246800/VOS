use anyhow::{Context, Result};
use std::process::Command;

// ── Package data ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Pkg {
    pub name: String,
    pub version: String,
    pub description: String,
    pub installed: bool,
}

// ── Backend trait ─────────────────────────────────────────────────────────────

pub trait PkgBackend: Send {
    fn name(&self) -> &'static str;
    fn list_installed(&self) -> Result<Vec<Pkg>>;
    fn search(&self, q: &str) -> Result<Vec<Pkg>>;
    fn outdated(&self) -> Result<Vec<Pkg>>;
    fn install_cmd(&self, name: &str) -> Vec<String>;
    fn remove_cmd(&self, name: &str) -> Vec<String>;
}

// ── Brew backend (macOS) ──────────────────────────────────────────────────────

pub struct BrewBackend;

impl BrewBackend {
    /// Parse `brew list --versions` output.
    /// Each line: `<name> <version> [<version2> …]`
    fn parse_list(output: &str) -> Vec<Pkg> {
        output
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                let mut parts = l.split_whitespace();
                let name = parts.next().unwrap_or("").to_string();
                let version = parts.next().unwrap_or("").to_string();
                Pkg {
                    name,
                    version,
                    description: String::new(),
                    installed: true,
                }
            })
            .collect()
    }

    /// Parse `brew search <q>` output.
    /// Lines may include section headers like `==> Formulae` — skip those.
    fn parse_search(output: &str) -> Vec<Pkg> {
        output
            .lines()
            .filter(|l| {
                let t = l.trim();
                !t.is_empty() && !t.starts_with("==>")
            })
            .map(|l| Pkg {
                name: l.trim().to_string(),
                version: String::new(),
                description: String::new(),
                installed: false,
            })
            .collect()
    }

    /// Parse `brew outdated` output.
    /// Each line: `<name> (<current_version>) < <latest_version>`
    fn parse_outdated(output: &str) -> Vec<Pkg> {
        output
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                // typical: `curl (7.88.1) < 8.4.0`
                let mut parts = l.splitn(2, ' ');
                let name = parts.next().unwrap_or("").to_string();
                let rest = parts.next().unwrap_or("");
                // extract the latest version after " < "
                let version = rest.split(" < ").nth(1).unwrap_or(rest).trim().to_string();
                Pkg {
                    name,
                    version,
                    description: String::new(),
                    installed: true,
                }
            })
            .collect()
    }
}

impl PkgBackend for BrewBackend {
    fn name(&self) -> &'static str {
        "brew"
    }

    fn list_installed(&self) -> Result<Vec<Pkg>> {
        let out = Command::new("brew")
            .args(["list", "--versions"])
            .output()
            .context("Failed to run brew list")?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        Ok(Self::parse_list(&stdout))
    }

    fn search(&self, q: &str) -> Result<Vec<Pkg>> {
        let out = Command::new("brew")
            .args(["search", q])
            .output()
            .context("Failed to run brew search")?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        Ok(Self::parse_search(&stdout))
    }

    fn outdated(&self) -> Result<Vec<Pkg>> {
        let out = Command::new("brew")
            .arg("outdated")
            .output()
            .context("Failed to run brew outdated")?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        Ok(Self::parse_outdated(&stdout))
    }

    fn install_cmd(&self, name: &str) -> Vec<String> {
        vec!["brew".into(), "install".into(), name.to_string()]
    }

    fn remove_cmd(&self, name: &str) -> Vec<String> {
        vec!["brew".into(), "uninstall".into(), name.to_string()]
    }
}

// ── Apt backend (Linux) ───────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
pub struct AptBackend;

#[cfg(target_os = "linux")]
impl AptBackend {
    fn parse_list(output: &str) -> Vec<Pkg> {
        // `dpkg-query -W -f='${Package}\t${Version}\t${binary:Summary}\n'`
        output
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                let mut cols = l.splitn(3, '\t');
                let name = cols.next().unwrap_or("").to_string();
                let version = cols.next().unwrap_or("").to_string();
                let description = cols.next().unwrap_or("").to_string();
                Pkg {
                    name,
                    version,
                    description,
                    installed: true,
                }
            })
            .collect()
    }

    fn parse_search(output: &str) -> Vec<Pkg> {
        // `apt-cache search <q>` → `<name> - <description>`
        output
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                let mut parts = l.splitn(2, " - ");
                let name = parts.next().unwrap_or("").trim().to_string();
                let description = parts.next().unwrap_or("").trim().to_string();
                Pkg {
                    name,
                    version: String::new(),
                    description,
                    installed: false,
                }
            })
            .collect()
    }
}

#[cfg(target_os = "linux")]
impl PkgBackend for AptBackend {
    fn name(&self) -> &'static str {
        "apt"
    }

    fn list_installed(&self) -> Result<Vec<Pkg>> {
        let out = Command::new("dpkg-query")
            .args(["-W", "-f=${Package}\t${Version}\t${binary:Summary}\n"])
            .output()
            .context("Failed to run dpkg-query")?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        Ok(Self::parse_list(&stdout))
    }

    fn search(&self, q: &str) -> Result<Vec<Pkg>> {
        let out = Command::new("apt-cache")
            .args(["search", q])
            .output()
            .context("Failed to run apt-cache search")?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        Ok(Self::parse_search(&stdout))
    }

    fn outdated(&self) -> Result<Vec<Pkg>> {
        // `apt list --upgradable` — lines: `<name>/<repo> <new_ver> <arch> [upgradable from: <old_ver>]`
        let out = Command::new("apt")
            .args(["list", "--upgradable"])
            .output()
            .context("Failed to run apt list --upgradable")?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let pkgs = stdout
            .lines()
            .filter(|l| l.contains("upgradable"))
            .map(|l| {
                let name = l.split('/').next().unwrap_or("").trim().to_string();
                let version = l.split_whitespace().nth(1).unwrap_or("").to_string();
                Pkg {
                    name,
                    version,
                    description: String::new(),
                    installed: true,
                }
            })
            .collect();
        Ok(pkgs)
    }

    fn install_cmd(&self, name: &str) -> Vec<String> {
        vec![
            "sudo".into(),
            "apt".into(),
            "install".into(),
            "-y".into(),
            name.to_string(),
        ]
    }

    fn remove_cmd(&self, name: &str) -> Vec<String> {
        vec![
            "sudo".into(),
            "apt".into(),
            "remove".into(),
            "-y".into(),
            name.to_string(),
        ]
    }
}

// ── Dnf backend (Linux) ───────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
pub struct DnfBackend;

#[cfg(target_os = "linux")]
impl DnfBackend {
    fn parse_list(output: &str) -> Vec<Pkg> {
        // `dnf list installed` → `<name>.<arch>  <version>  <repo>`
        output
            .lines()
            .filter(|l| {
                let t = l.trim();
                !t.is_empty() && !t.starts_with("Installed") && !t.starts_with("Last")
            })
            .map(|l| {
                let mut cols = l.split_whitespace();
                let raw_name = cols.next().unwrap_or("").to_string();
                // Strip `.arch` suffix
                let name = raw_name
                    .rsplitn(2, '.')
                    .last()
                    .unwrap_or(&raw_name)
                    .to_string();
                let version = cols.next().unwrap_or("").to_string();
                Pkg {
                    name,
                    version,
                    description: String::new(),
                    installed: true,
                }
            })
            .collect()
    }

    fn parse_search(output: &str) -> Vec<Pkg> {
        // `dnf search <q>` — pairs of lines: `<name>.<arch> : <desc>`
        output
            .lines()
            .filter(|l| l.contains(" : "))
            .map(|l| {
                let mut parts = l.splitn(2, " : ");
                let raw = parts.next().unwrap_or("").trim();
                let name = raw.rsplitn(2, '.').last().unwrap_or(raw).to_string();
                let description = parts.next().unwrap_or("").trim().to_string();
                Pkg {
                    name,
                    version: String::new(),
                    description,
                    installed: false,
                }
            })
            .collect()
    }
}

#[cfg(target_os = "linux")]
impl PkgBackend for DnfBackend {
    fn name(&self) -> &'static str {
        "dnf"
    }

    fn list_installed(&self) -> Result<Vec<Pkg>> {
        let out = Command::new("dnf")
            .args(["list", "installed"])
            .output()
            .context("Failed to run dnf list installed")?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        Ok(Self::parse_list(&stdout))
    }

    fn search(&self, q: &str) -> Result<Vec<Pkg>> {
        let out = Command::new("dnf")
            .args(["search", q])
            .output()
            .context("Failed to run dnf search")?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        Ok(Self::parse_search(&stdout))
    }

    fn outdated(&self) -> Result<Vec<Pkg>> {
        let out = Command::new("dnf")
            .args(["check-update"])
            .output()
            .context("Failed to run dnf check-update")?;
        // exit code 100 means updates available — that's fine
        let stdout = String::from_utf8_lossy(&out.stdout);
        let pkgs = stdout
            .lines()
            .filter(|l| {
                let t = l.trim();
                !t.is_empty() && !t.starts_with("Last") && !t.starts_with("Loaded")
            })
            .map(|l| {
                let mut cols = l.split_whitespace();
                let raw = cols.next().unwrap_or("").to_string();
                let name = raw.rsplitn(2, '.').last().unwrap_or(&raw).to_string();
                let version = cols.next().unwrap_or("").to_string();
                Pkg {
                    name,
                    version,
                    description: String::new(),
                    installed: true,
                }
            })
            .collect();
        Ok(pkgs)
    }

    fn install_cmd(&self, name: &str) -> Vec<String> {
        vec![
            "sudo".into(),
            "dnf".into(),
            "install".into(),
            "-y".into(),
            name.to_string(),
        ]
    }

    fn remove_cmd(&self, name: &str) -> Vec<String> {
        vec![
            "sudo".into(),
            "dnf".into(),
            "remove".into(),
            "-y".into(),
            name.to_string(),
        ]
    }
}

// ── Pacman backend (Linux) ────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
pub struct PacmanBackend;

#[cfg(target_os = "linux")]
impl PacmanBackend {
    fn parse_list(output: &str) -> Vec<Pkg> {
        // `pacman -Q` → `<name> <version>`
        output
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                let mut cols = l.split_whitespace();
                let name = cols.next().unwrap_or("").to_string();
                let version = cols.next().unwrap_or("").to_string();
                Pkg {
                    name,
                    version,
                    description: String::new(),
                    installed: true,
                }
            })
            .collect()
    }

    fn parse_search(output: &str) -> Vec<Pkg> {
        // `pacman -Ss <q>` → alternating lines: `<repo>/<name> <version>` then `    <desc>`
        let mut pkgs = Vec::new();
        let mut iter = output.lines();
        while let Some(line) = iter.next() {
            if line.contains('/') && !line.starts_with(' ') {
                let mut parts = line.split_whitespace();
                let full = parts.next().unwrap_or("");
                let name = full.split('/').nth(1).unwrap_or(full).to_string();
                let version = parts.next().unwrap_or("").to_string();
                let description = iter.next().unwrap_or("").trim().to_string();
                pkgs.push(Pkg {
                    name,
                    version,
                    description,
                    installed: false,
                });
            }
        }
        pkgs
    }
}

#[cfg(target_os = "linux")]
impl PkgBackend for PacmanBackend {
    fn name(&self) -> &'static str {
        "pacman"
    }

    fn list_installed(&self) -> Result<Vec<Pkg>> {
        let out = Command::new("pacman")
            .arg("-Q")
            .output()
            .context("Failed to run pacman -Q")?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        Ok(Self::parse_list(&stdout))
    }

    fn search(&self, q: &str) -> Result<Vec<Pkg>> {
        let out = Command::new("pacman")
            .args(["-Ss", q])
            .output()
            .context("Failed to run pacman -Ss")?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        Ok(Self::parse_search(&stdout))
    }

    fn outdated(&self) -> Result<Vec<Pkg>> {
        // `checkupdates` (from pacman-contrib) or fall back to empty list
        let out = Command::new("checkupdates").output();
        match out {
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                let pkgs = stdout
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| {
                        let mut cols = l.split_whitespace();
                        let name = cols.next().unwrap_or("").to_string();
                        // skip old version and arrow
                        let version = cols.nth(2).unwrap_or("").to_string();
                        Pkg {
                            name,
                            version,
                            description: String::new(),
                            installed: true,
                        }
                    })
                    .collect();
                Ok(pkgs)
            }
            Err(_) => Ok(Vec::new()),
        }
    }

    fn install_cmd(&self, name: &str) -> Vec<String> {
        vec![
            "sudo".into(),
            "pacman".into(),
            "-S".into(),
            "--noconfirm".into(),
            name.to_string(),
        ]
    }

    fn remove_cmd(&self, name: &str) -> Vec<String> {
        vec![
            "sudo".into(),
            "pacman".into(),
            "-R".into(),
            "--noconfirm".into(),
            name.to_string(),
        ]
    }
}

// ── Backend detection ─────────────────────────────────────────────────────────

fn which(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn detect_backend() -> Option<Box<dyn PkgBackend>> {
    if which("brew") {
        return Some(Box::new(BrewBackend));
    }
    #[cfg(target_os = "linux")]
    {
        if which("apt") {
            return Some(Box::new(AptBackend));
        }
        if which("dnf") {
            return Some(Box::new(DnfBackend));
        }
        if which("pacman") {
            return Some(Box::new(PacmanBackend));
        }
    }
    None
}

// ── Tab enum ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum PkgTab {
    Installed,
    Search,
    Updates,
}

// ── PackageManager state ──────────────────────────────────────────────────────

pub struct PackageManager {
    pub backend: Option<Box<dyn PkgBackend>>,
    pub packages: Vec<Pkg>,
    pub search_results: Vec<Pkg>,
    pub search_query: String,
    pub tab: PkgTab,
    pub selected: usize,
    pub loading: bool,
    pub status_msg: String,
}

impl PackageManager {
    pub fn new() -> Self {
        let backend = detect_backend();
        let backend_name = backend.as_ref().map(|b| b.name()).unwrap_or("none");
        let status_msg = format!("Backend: {backend_name}  Press F5 to load packages");
        Self {
            backend,
            packages: Vec::new(),
            search_results: Vec::new(),
            search_query: String::new(),
            tab: PkgTab::Installed,
            selected: 0,
            loading: false,
            status_msg,
        }
    }

    pub fn load_installed(&mut self) {
        if let Some(ref b) = self.backend {
            self.loading = true;
            match b.list_installed() {
                Ok(pkgs) => {
                    let count = pkgs.len();
                    self.packages = pkgs;
                    self.selected = 0;
                    self.status_msg = format!("{count} packages installed");
                }
                Err(e) => {
                    self.status_msg = format!("Error: {e}");
                }
            }
            self.loading = false;
        }
    }

    pub fn do_search(&mut self) {
        if let Some(ref b) = self.backend {
            let q = self.search_query.clone();
            if q.is_empty() {
                self.search_results.clear();
                self.status_msg = "Search query is empty".into();
                return;
            }
            self.loading = true;
            match b.search(&q) {
                Ok(results) => {
                    let count = results.len();
                    self.search_results = results;
                    self.selected = 0;
                    self.status_msg = format!("{count} results for \"{q}\"");
                }
                Err(e) => {
                    self.status_msg = format!("Search error: {e}");
                }
            }
            self.loading = false;
        }
    }

    pub fn load_updates(&mut self) {
        if let Some(ref b) = self.backend {
            self.loading = true;
            match b.outdated() {
                Ok(pkgs) => {
                    let count = pkgs.len();
                    self.packages = pkgs;
                    self.selected = 0;
                    self.status_msg = if count == 0 {
                        "All packages up to date".into()
                    } else {
                        format!("{count} updates available")
                    };
                }
                Err(e) => {
                    self.status_msg = format!("Error checking updates: {e}");
                }
            }
            self.loading = false;
        }
    }

    /// Returns the currently visible list (depends on active tab).
    pub fn visible_packages(&self) -> &[Pkg] {
        match self.tab {
            PkgTab::Search => &self.search_results,
            _ => &self.packages,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let max = self.visible_packages().len().saturating_sub(1);
        if self.selected < max {
            self.selected += 1;
        }
    }

    pub fn selected_pkg(&self) -> Option<&Pkg> {
        self.visible_packages().get(self.selected)
    }

    /// Cycle to next tab and reset selection.
    pub fn next_tab(&mut self) {
        self.tab = match self.tab {
            PkgTab::Installed => PkgTab::Search,
            PkgTab::Search => PkgTab::Updates,
            PkgTab::Updates => PkgTab::Installed,
        };
        self.selected = 0;
    }

    pub fn backend_name(&self) -> &'static str {
        self.backend.as_ref().map(|b| b.name()).unwrap_or("none")
    }
}

impl Default for PackageManager {
    fn default() -> Self {
        Self::new()
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brew_parse_list() {
        let raw = "curl 8.4.0\nzlib 1.3\nfoo 2.0.1_1\n";
        let pkgs = BrewBackend::parse_list(raw);
        assert_eq!(pkgs.len(), 3);
        assert_eq!(pkgs[0].name, "curl");
        assert_eq!(pkgs[0].version, "8.4.0");
        assert!(pkgs[0].installed);
        assert_eq!(pkgs[2].name, "foo");
        assert_eq!(pkgs[2].version, "2.0.1_1");
    }

    #[test]
    fn brew_parse_list_empty() {
        let pkgs = BrewBackend::parse_list("");
        assert!(pkgs.is_empty());
    }

    #[test]
    fn brew_parse_search_skips_headers() {
        let raw = "==> Formulae\ncurl\ngit\n==> Casks\ncurl-openssl\n";
        let pkgs = BrewBackend::parse_search(raw);
        assert_eq!(pkgs.len(), 3);
        assert_eq!(pkgs[0].name, "curl");
        assert_eq!(pkgs[1].name, "git");
        assert_eq!(pkgs[2].name, "curl-openssl");
        assert!(!pkgs[0].installed);
    }

    #[test]
    fn brew_parse_outdated() {
        let raw = "curl (7.88.1) < 8.4.0\ngit (2.41.0) < 2.43.0\n";
        let pkgs = BrewBackend::parse_outdated(raw);
        assert_eq!(pkgs.len(), 2);
        assert_eq!(pkgs[0].name, "curl");
        assert_eq!(pkgs[0].version, "8.4.0");
        assert_eq!(pkgs[1].name, "git");
        assert_eq!(pkgs[1].version, "2.43.0");
    }

    #[test]
    fn brew_parse_outdated_empty() {
        let pkgs = BrewBackend::parse_outdated("");
        assert!(pkgs.is_empty());
    }

    #[test]
    fn brew_install_cmd() {
        let cmds = BrewBackend.install_cmd("ripgrep");
        assert_eq!(cmds, vec!["brew", "install", "ripgrep"]);
    }

    #[test]
    fn brew_remove_cmd() {
        let cmds = BrewBackend.remove_cmd("ripgrep");
        assert_eq!(cmds, vec!["brew", "uninstall", "ripgrep"]);
    }

    #[test]
    fn package_manager_new_detects_backend() {
        let pm = PackageManager::new();
        // On macOS with brew installed, backend should be Some.
        // On systems without any supported pkg manager, it can be None — just check it doesn't panic.
        let _ = pm.backend_name();
    }

    #[test]
    fn package_manager_visible_packages_empty() {
        let pm = PackageManager::new();
        assert!(pm.visible_packages().is_empty());
    }

    #[test]
    fn package_manager_move_bounds() {
        let mut pm = PackageManager::new();
        pm.packages = vec![
            Pkg {
                name: "a".into(),
                version: "1".into(),
                description: "".into(),
                installed: true,
            },
            Pkg {
                name: "b".into(),
                version: "2".into(),
                description: "".into(),
                installed: true,
            },
        ];
        pm.tab = PkgTab::Installed;
        pm.selected = 0;
        pm.move_up(); // should clamp at 0
        assert_eq!(pm.selected, 0);
        pm.move_down();
        assert_eq!(pm.selected, 1);
        pm.move_down(); // should clamp at len-1
        assert_eq!(pm.selected, 1);
    }

    #[test]
    fn package_manager_next_tab_cycles() {
        let mut pm = PackageManager::new();
        assert_eq!(pm.tab, PkgTab::Installed);
        pm.next_tab();
        assert_eq!(pm.tab, PkgTab::Search);
        pm.next_tab();
        assert_eq!(pm.tab, PkgTab::Updates);
        pm.next_tab();
        assert_eq!(pm.tab, PkgTab::Installed);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn apt_parse_list() {
        let raw = "bash\t5.1.16-1\tGNU Bourne Again SHell\ngzip\t1.10-4\tGNU compression\n";
        let pkgs = AptBackend::parse_list(raw);
        assert_eq!(pkgs.len(), 2);
        assert_eq!(pkgs[0].name, "bash");
        assert_eq!(pkgs[0].version, "5.1.16-1");
        assert_eq!(pkgs[0].description, "GNU Bourne Again SHell");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn apt_parse_search() {
        let raw = "curl - command line tool for transferring data with URL syntax\nwget - retrieves files from the web\n";
        let pkgs = AptBackend::parse_search(raw);
        assert_eq!(pkgs.len(), 2);
        assert_eq!(pkgs[0].name, "curl");
        assert_eq!(
            pkgs[0].description,
            "command line tool for transferring data with URL syntax"
        );
    }
}
