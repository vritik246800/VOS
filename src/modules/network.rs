#![allow(dead_code)]
use std::collections::VecDeque;
use std::time::Instant;
use sysinfo::Networks;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

// ── Per-interface data ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NetIface {
    pub name: String,
    pub mac: String,
    pub rx_total: u64,
    pub tx_total: u64,
    pub rx_rate: f32, // bytes/s
    pub tx_rate: f32, // bytes/s
    pub rx_history: VecDeque<f32>,
    pub tx_history: VecDeque<f32>,
    pub ips: Vec<String>,
}

impl NetIface {
    fn new(name: String, mac: String, ips: Vec<String>, rx_total: u64, tx_total: u64) -> Self {
        Self {
            name,
            mac,
            rx_total,
            tx_total,
            rx_rate: 0.0,
            tx_rate: 0.0,
            rx_history: VecDeque::with_capacity(60),
            tx_history: VecDeque::with_capacity(60),
            ips,
        }
    }

    fn push_history(deque: &mut VecDeque<f32>, value: f32) {
        if deque.len() >= 60 {
            deque.pop_front();
        }
        deque.push_back(value);
    }
}

// ── IP address detection ──────────────────────────────────────────────────────

/// Parse `ifconfig` output (macOS / BSD) into a map of iface_name → Vec<IPv4>.
fn parse_ifconfig(output: &str) -> std::collections::HashMap<String, Vec<String>> {
    let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    let mut current: Option<String> = None;

    for line in output.lines() {
        // Interface header: starts at column 0, not whitespace
        if !line.starts_with(char::is_whitespace) && line.contains(':') {
            let iface = line.split(':').next().unwrap_or("").trim().to_string();
            if !iface.is_empty() {
                current = Some(iface);
            }
        } else if let Some(ref iface) = current {
            let trimmed = line.trim();
            // inet <ip> netmask ...
            if trimmed.starts_with("inet ") && !trimmed.starts_with("inet6") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if let Some(ip) = parts.get(1) {
                    map.entry(iface.clone()).or_default().push(ip.to_string());
                }
            }
        }
    }
    map
}

/// Parse `ip -o addr` output (Linux) into a map of iface_name → Vec<IPv4>.
fn parse_ip_addr(output: &str) -> std::collections::HashMap<String, Vec<String>> {
    let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

    for line in output.lines() {
        // Format: <index>: <iface>    inet <ip/prefix> ...
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }
        // parts[0] = "1:", parts[1] = "lo", parts[2] = "inet", parts[3] = "127.0.0.1/8"
        if parts[2] != "inet" {
            continue;
        }
        let iface = parts[1].trim_end_matches(':').to_string();
        let ip_cidr = parts[3];
        let ip = ip_cidr.split('/').next().unwrap_or(ip_cidr).to_string();
        map.entry(iface).or_default().push(ip);
    }
    map
}

/// Collect IPs per interface. Tries `ip -o addr` first (Linux), falls back to `ifconfig` (macOS).
fn collect_ips() -> std::collections::HashMap<String, Vec<String>> {
    // Try Linux `ip` first
    if let Ok(out) = std::process::Command::new("ip")
        .args(["-o", "addr"])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            let map = parse_ip_addr(&text);
            if !map.is_empty() {
                return map;
            }
        }
    }
    // Fall back to ifconfig (macOS / BSD)
    if let Ok(out) = std::process::Command::new("ifconfig").output() {
        let text = String::from_utf8_lossy(&out.stdout);
        return parse_ifconfig(&text);
    }
    std::collections::HashMap::new()
}

/// Return the primary non-loopback IPv4 address of this machine.
/// This is a stable public API — reused by the Transfer system (§6.3).
pub fn local_ip() -> Option<String> {
    let ips = collect_ips();
    // Prefer addresses on common LAN interfaces; skip loopback
    let preferred_prefixes = ["en", "eth", "wlan", "wlp", "ens", "enp"];
    for prefix in &preferred_prefixes {
        for (iface, addrs) in &ips {
            if iface.starts_with(prefix) {
                for ip in addrs {
                    if !ip.starts_with("127.") && !ip.starts_with("::1") {
                        return Some(ip.clone());
                    }
                }
            }
        }
    }
    // Fall back: any non-loopback IPv4
    for addrs in ips.values() {
        for ip in addrs {
            if !ip.starts_with("127.") && !ip.starts_with("::1") {
                return Some(ip.clone());
            }
        }
    }
    None
}

// ── NetworkPanel state ────────────────────────────────────────────────────────

pub struct NetworkPanel {
    pub ifaces: Vec<NetIface>,
    pub selected: usize,
    pub ping_host: String,
    pub ping_rx: Option<mpsc::UnboundedReceiver<String>>,
    pub ping_lines: VecDeque<String>,
    pub ping_child: Option<tokio::process::Child>,
    pub last_tick: Instant,
    networks: Networks,
}

impl NetworkPanel {
    pub fn new() -> Self {
        let mut networks = Networks::new_with_refreshed_list();
        networks.refresh(true);
        let ip_map = collect_ips();

        let ifaces: Vec<NetIface> = networks
            .iter()
            .map(|(name, data)| {
                let mac = data.mac_address().to_string();
                let ips = ip_map.get(name).cloned().unwrap_or_default();
                NetIface::new(
                    name.clone(),
                    mac,
                    ips,
                    data.total_received(),
                    data.total_transmitted(),
                )
            })
            .collect();

        let mut panel = Self {
            ifaces,
            selected: 0,
            ping_host: "8.8.8.8".to_string(),
            ping_rx: None,
            ping_lines: VecDeque::with_capacity(32),
            ping_child: None,
            last_tick: Instant::now(),
            networks,
        };
        // Sort interfaces by name for stable ordering
        panel.ifaces.sort_by(|a, b| a.name.cmp(&b.name));
        panel
    }

    pub fn tick(&mut self) {
        let elapsed = self.last_tick.elapsed().as_secs_f32();
        self.last_tick = Instant::now();

        // Refresh sysinfo network data
        self.networks.refresh(true);
        let ip_map = collect_ips();

        // Update iface data (match by name, add/remove)
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (name, data) in self.networks.iter() {
            seen.insert(name.clone());
            let mac = data.mac_address().to_string();
            let ips = ip_map.get(name).cloned().unwrap_or_default();
            let new_rx = data.total_received();
            let new_tx = data.total_transmitted();

            if let Some(iface) = self.ifaces.iter_mut().find(|i| &i.name == name) {
                let delta_rx = new_rx.saturating_sub(iface.rx_total);
                let delta_tx = new_tx.saturating_sub(iface.tx_total);
                iface.rx_rate = if elapsed > 0.0 {
                    delta_rx as f32 / elapsed
                } else {
                    0.0
                };
                iface.tx_rate = if elapsed > 0.0 {
                    delta_tx as f32 / elapsed
                } else {
                    0.0
                };
                iface.rx_total = new_rx;
                iface.tx_total = new_tx;
                iface.mac = mac;
                iface.ips = ips;
                NetIface::push_history(&mut iface.rx_history, iface.rx_rate);
                NetIface::push_history(&mut iface.tx_history, iface.tx_rate);
            } else {
                // New interface appeared
                let iface = NetIface::new(name.clone(), mac, ips, new_rx, new_tx);
                self.ifaces.push(iface);
            }
        }
        // Remove interfaces that disappeared
        self.ifaces.retain(|i| seen.contains(&i.name));
        self.ifaces.sort_by(|a, b| a.name.cmp(&b.name));

        // Clamp selection
        if !self.ifaces.is_empty() && self.selected >= self.ifaces.len() {
            self.selected = self.ifaces.len() - 1;
        }

        // Drain ping channel
        if let Some(rx) = &mut self.ping_rx {
            while let Ok(line) = rx.try_recv() {
                if self.ping_lines.len() >= 32 {
                    self.ping_lines.pop_front();
                }
                self.ping_lines.push_back(line);
            }
        }
    }

    pub fn select_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn select_down(&mut self) {
        if !self.ifaces.is_empty() && self.selected + 1 < self.ifaces.len() {
            self.selected += 1;
        }
    }

    pub fn selected_iface(&self) -> Option<&NetIface> {
        self.ifaces.get(self.selected)
    }

    /// Spawn `ping -c 5 <host>` with streaming stdout → mpsc.
    /// Kills any previously running ping child.
    pub fn start_ping(&mut self, host: &str) {
        self.kill_ping();
        self.ping_host = host.to_string();
        self.ping_lines.clear();

        let (tx, rx) = mpsc::unbounded_channel::<String>();
        self.ping_rx = Some(rx);

        let host_owned = host.to_string();
        let result = tokio::process::Command::new("ping")
            .args(["-c", "5", &host_owned])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();

        match result {
            Ok(mut child) => {
                let stdout = child.stdout.take();
                let stderr = child.stderr.take();
                let tx2 = tx.clone();

                if let Some(so) = stdout {
                    tokio::spawn(async move {
                        let mut lines = BufReader::new(so).lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            let _ = tx.send(line);
                        }
                    });
                }
                if let Some(se) = stderr {
                    tokio::spawn(async move {
                        let mut lines = BufReader::new(se).lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            let _ = tx2.send(format!("✗ {line}"));
                        }
                    });
                }
                self.ping_child = Some(child);
            }
            Err(e) => {
                // Send the error via the channel so it shows in the UI
                let _ = tx.send(format!("ping failed: {e}"));
            }
        }
    }

    /// Kill running ping child and reset rx.
    pub fn kill_ping(&mut self) {
        if let Some(mut child) = self.ping_child.take() {
            // Best-effort kill; ignore error
            let _ = child.start_kill();
        }
        self.ping_rx = None;
    }

    pub fn is_pinging(&self) -> bool {
        self.ping_child.is_some()
    }
}

impl Default for NetworkPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests for IP parsers ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Fixture: realistic macOS `ifconfig` snippet
    const IFCONFIG_FIXTURE: &str = "\
lo0: flags=8049<UP,LOOPBACK,RUNNING,MULTICAST> mtu 16384
    inet 127.0.0.1 netmask 0xff000000
    inet6 ::1 prefixlen 128
en0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500
    ether a4:c3:f0:11:22:33
    inet 192.168.1.42 netmask 0xffffff00 broadcast 192.168.1.255
utun0: flags=8051<UP,POINTOPOINT,RUNNING,MULTICAST> mtu 1380
    inet6 fe80::1234:5678:90ab:cdef prefixlen 64
";

    #[test]
    fn test_parse_ifconfig_loopback() {
        let map = parse_ifconfig(IFCONFIG_FIXTURE);
        let lo_ips = map.get("lo0").expect("lo0 should be present");
        assert!(lo_ips.contains(&"127.0.0.1".to_string()));
    }

    #[test]
    fn test_parse_ifconfig_lan() {
        let map = parse_ifconfig(IFCONFIG_FIXTURE);
        let en_ips = map.get("en0").expect("en0 should be present");
        assert!(en_ips.contains(&"192.168.1.42".to_string()));
    }

    #[test]
    fn test_parse_ifconfig_no_inet6_as_inet() {
        let map = parse_ifconfig(IFCONFIG_FIXTURE);
        // utun0 has only inet6, should not appear in the map with IPv4
        if let Some(ips) = map.get("utun0") {
            for ip in ips {
                assert!(
                    !ip.contains(':'),
                    "IPv6 address leaked into ifconfig parse: {ip}"
                );
            }
        }
    }

    // Fixture: realistic Linux `ip -o addr` snippet
    const IP_ADDR_FIXTURE: &str = "\
1: lo    inet 127.0.0.1/8 scope host lo\\       valid_lft forever preferred_lft forever
2: eth0    inet 10.0.0.5/24 brd 10.0.0.255 scope global eth0\\       valid_lft forever preferred_lft forever
3: wlan0    inet 192.168.0.77/24 brd 192.168.0.255 scope global wlan0\\       valid_lft forever preferred_lft forever
";

    #[test]
    fn test_parse_ip_addr_loopback() {
        let map = parse_ip_addr(IP_ADDR_FIXTURE);
        let lo = map.get("lo").expect("lo should be present");
        assert!(lo.contains(&"127.0.0.1".to_string()));
    }

    #[test]
    fn test_parse_ip_addr_eth0() {
        let map = parse_ip_addr(IP_ADDR_FIXTURE);
        let eth = map.get("eth0").expect("eth0 should be present");
        assert!(eth.contains(&"10.0.0.5".to_string()));
    }

    #[test]
    fn test_parse_ip_addr_wlan0() {
        let map = parse_ip_addr(IP_ADDR_FIXTURE);
        let wlan = map.get("wlan0").expect("wlan0 should be present");
        assert!(wlan.contains(&"192.168.0.77".to_string()));
    }

    #[test]
    fn test_local_ip_skips_loopback() {
        // local_ip() uses the real system; just ensure it doesn't return 127.x
        if let Some(ip) = local_ip() {
            assert!(
                !ip.starts_with("127."),
                "local_ip() should not return loopback, got: {ip}"
            );
        }
        // If None, that's fine too (e.g., no network in CI).
    }
}
