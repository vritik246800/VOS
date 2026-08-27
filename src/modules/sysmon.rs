use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use sysinfo::{Disks, Networks, System};

/// Sampling period. `tick()` is called once per frame (~16 ms); anything faster
/// than this is dropped — sysinfo needs ≥200 ms between refreshes for the CPU
/// numbers to mean anything, and the network rates below are per second.
const SAMPLE_EVERY: Duration = Duration::from_secs(1);

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct CpuSample {
    pub usage: f32,
}

/// Category of a mounted volume shown in the Storage card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageKind {
    /// System root volume ("/")
    Main,
    /// Removable drive (USB stick, external disk)
    Usb,
    /// Network filesystem (NFS, SMB, AFP, sshfs, …)
    Network,
}

impl StorageKind {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Main => "Main",
            Self::Usb => "USB",
            Self::Network => "Network",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiskInfo {
    pub name: String,
    pub mount: String,
    pub kind: StorageKind,
    pub total_gb: f64,
    pub used_gb: f64,
    pub pct: f64,
}

#[derive(Debug, Clone)]
pub struct NetInfo {
    pub name: String,
    pub rx_kbps: f64,
    pub tx_kbps: f64,
}

/// GPU snapshot (Apple Silicon, via `ioreg`). macOS does not expose per-core
/// GPU utilization, so the card shows the overall device load plus the two
/// engine counters Apple does publish (renderer / tiler).
#[derive(Debug, Clone)]
pub struct GpuInfo {
    /// e.g. "Apple M5"
    pub name: String,
    pub cores: u32,
    /// "Device Utilization %" — the headline number
    pub device_pct: f32,
    pub renderer_pct: f32,
    pub tiler_pct: f32,
    /// "In use system memory" / "Alloc system memory", in MB
    pub mem_used_mb: f64,
    pub mem_alloc_mb: f64,
}

pub struct SystemMonitor {
    sys: System,
    disks: Disks,
    networks: Networks,
    pub cpu_history: Vec<f32>,
    pub ram_history: Vec<f32>,
    pub swap_history: Vec<f32>,
    /// Total download/upload rate across interfaces (KB per tick)
    pub rx_history: Vec<f64>,
    pub tx_history: Vec<f64>,
    pub cpu_cores: Vec<f32>,
    pub total_ram_mb: f64,
    pub used_ram_mb: f64,
    pub total_swap_mb: f64,
    pub used_swap_mb: f64,
    pub disk_info: Vec<DiskInfo>,
    pub net_info: Vec<NetInfo>,
    /// `None` on machines without a queryable GPU (and off macOS).
    pub gpu: Option<GpuInfo>,
    pub gpu_history: Vec<f32>,
    pub history_len: usize,
    /// GPU polls run off-thread: `ioreg` takes ~25 ms, too long to block a
    /// frame. Each tick spawns one query (if none is in flight) and picks up
    /// the previous query's result.
    gpu_tx: mpsc::Sender<Option<GpuInfo>>,
    gpu_rx: mpsc::Receiver<Option<GpuInfo>>,
    gpu_pending: bool,
    /// `None` until the first sample lands.
    last_sample: Option<Instant>,
}

impl SystemMonitor {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        let disks = Disks::new_with_refreshed_list();
        let networks = Networks::new_with_refreshed_list();

        let total_ram_mb = sys.total_memory() as f64 / 1024.0 / 1024.0;
        let used_ram_mb = sys.used_memory() as f64 / 1024.0 / 1024.0;
        let total_swap_mb = sys.total_swap() as f64 / 1024.0 / 1024.0;
        let used_swap_mb = sys.used_swap() as f64 / 1024.0 / 1024.0;

        let (gpu_tx, gpu_rx) = mpsc::channel();
        // First GPU read is synchronous so the card shows up immediately;
        // later reads are pipelined through the channel in `tick()`.
        let gpu = query_gpu();

        let mut m = Self {
            sys,
            disks,
            networks,
            cpu_history: Vec::new(),
            ram_history: Vec::new(),
            swap_history: Vec::new(),
            rx_history: Vec::new(),
            tx_history: Vec::new(),
            cpu_cores: Vec::new(),
            total_ram_mb,
            used_ram_mb,
            total_swap_mb,
            used_swap_mb,
            disk_info: Vec::new(),
            net_info: Vec::new(),
            gpu,
            gpu_history: Vec::new(),
            history_len: 60,
            gpu_tx,
            gpu_rx,
            gpu_pending: false,
            last_sample: None,
        };
        if let Some(g) = &m.gpu {
            m.gpu_history.push(g.device_pct);
        }
        m.tick();
        m
    }

    /// Refreshes at most once per [`SAMPLE_EVERY`]; cheap no-op in between.
    pub fn tick(&mut self) {
        let secs = match self.last_sample {
            Some(t) if t.elapsed() < SAMPLE_EVERY => return,
            Some(t) => t.elapsed().as_secs_f64(),
            // First sample: the counters were zeroed in `new()`, so there is no
            // elapsed window to divide by yet.
            None => 1.0,
        };
        self.last_sample = Some(Instant::now());

        self.sys.refresh_cpu_all();
        self.sys.refresh_memory();
        self.disks.refresh(true);
        self.networks.refresh(true);

        let cpu = self.sys.global_cpu_usage();
        push_hist(&mut self.cpu_history, cpu, self.history_len);

        self.total_ram_mb = self.sys.total_memory() as f64 / 1024.0 / 1024.0;
        self.used_ram_mb = self.sys.used_memory() as f64 / 1024.0 / 1024.0;
        let ram_pct = if self.total_ram_mb > 0.0 {
            (self.used_ram_mb / self.total_ram_mb * 100.0) as f32
        } else {
            0.0
        };
        push_hist(&mut self.ram_history, ram_pct, self.history_len);

        self.total_swap_mb = self.sys.total_swap() as f64 / 1024.0 / 1024.0;
        self.used_swap_mb = self.sys.used_swap() as f64 / 1024.0 / 1024.0;
        let swap_pct = if self.total_swap_mb > 0.0 {
            (self.used_swap_mb / self.total_swap_mb * 100.0) as f32
        } else {
            0.0
        };
        push_hist(&mut self.swap_history, swap_pct, self.history_len);

        self.cpu_cores = self.sys.cpus().iter().map(|c| c.cpu_usage()).collect();

        self.disk_info = self
            .disks
            .list()
            .iter()
            .filter_map(|d| {
                let kind = storage_kind(d)?;
                let total = d.total_space() as f64 / 1e9;
                let avail = d.available_space() as f64 / 1e9;
                let used = total - avail;
                let pct = if total > 0.0 {
                    used / total * 100.0
                } else {
                    0.0
                };
                let name = d.name().to_string_lossy().to_string();
                let mount = d.mount_point().to_string_lossy().to_string();
                Some(DiskInfo {
                    name: if name.is_empty() { mount.clone() } else { name },
                    mount,
                    kind,
                    total_gb: total,
                    used_gb: used,
                    pct,
                })
            })
            .collect();
        self.disk_info.sort_by_key(|d| match d.kind {
            StorageKind::Main => 0,
            StorageKind::Usb => 1,
            StorageKind::Network => 2,
        });

        self.net_info = self
            .networks
            .list()
            .iter()
            .map(|(name, data)| NetInfo {
                name: name.clone(),
                rx_kbps: data.received() as f64 / 1024.0 / secs,
                tx_kbps: data.transmitted() as f64 / 1024.0 / secs,
            })
            .collect();
        self.net_info.sort_by(|a, b| a.name.cmp(&b.name));

        let (rx, tx) = self
            .net_info
            .iter()
            .filter(|n| !is_virtual_iface(&n.name))
            .fold((0.0, 0.0), |(r, t), n| (r + n.rx_kbps, t + n.tx_kbps));
        push_hist(&mut self.rx_history, rx, self.history_len);
        push_hist(&mut self.tx_history, tx, self.history_len);

        // Kick off the next GPU read and collect the previous one.
        if !self.gpu_pending {
            self.gpu_pending = true;
            let tx = self.gpu_tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(query_gpu());
            });
        }
        if let Ok(new_gpu) = self.gpu_rx.try_recv() {
            self.gpu_pending = false;
            if let Some(info) = new_gpu {
                push_hist(&mut self.gpu_history, info.device_pct, self.history_len);
                self.gpu = Some(info);
            }
        }
    }

    pub fn cpu_pct(&self) -> f32 {
        self.cpu_history.last().copied().unwrap_or(0.0)
    }

    pub fn ram_pct(&self) -> f32 {
        self.ram_history.last().copied().unwrap_or(0.0)
    }

    pub fn swap_pct(&self) -> f32 {
        self.swap_history.last().copied().unwrap_or(0.0)
    }

    pub fn rx_kbps(&self) -> f64 {
        self.rx_history.last().copied().unwrap_or(0.0)
    }

    pub fn tx_kbps(&self) -> f64 {
        self.tx_history.last().copied().unwrap_or(0.0)
    }

    pub fn uptime_str() -> String {
        let secs = System::uptime();
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        let s = secs % 60;
        format!("{h}h {m}m {s}s")
    }
}

impl Default for SystemMonitor {
    fn default() -> Self {
        Self::new()
    }
}

fn push_hist<T>(hist: &mut Vec<T>, v: T, max: usize) {
    hist.push(v);
    if hist.len() > max {
        hist.remove(0);
    }
}

/// Reads the GPU via `ioreg` (no sudo needed for IOAccelerator). Always
/// `None` off macOS.
fn query_gpu() -> Option<GpuInfo> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("ioreg")
            .args(["-r", "-d", "1", "-w", "0", "-c", "IOAccelerator"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        parse_gpu(&String::from_utf8_lossy(&out.stdout))
    }
    #[cfg(not(target_os = "macos"))]
    None
}

/// Parses the `IOAccelerator` property dump. Utilization keys live in the
/// `PerformanceStatistics` dictionary as `"Device Utilization %"=36`; the key
/// names are unique in the whole output, so a flat search is enough.
#[cfg(target_os = "macos")]
fn parse_gpu(output: &str) -> Option<GpuInfo> {
    let device_pct = stat_number(output, "Device Utilization %")? as f32;
    Some(GpuInfo {
        name: stat_string(output, "model").unwrap_or_else(|| "GPU".to_string()),
        cores: stat_number(output, "gpu-core-count").unwrap_or(0.0) as u32,
        device_pct,
        renderer_pct: stat_number(output, "Renderer Utilization %").unwrap_or(0.0) as f32,
        tiler_pct: stat_number(output, "Tiler Utilization %").unwrap_or(0.0) as f32,
        mem_used_mb: stat_number(output, "In use system memory").unwrap_or(0.0) / 1024.0 / 1024.0,
        mem_alloc_mb: stat_number(output, "Alloc system memory").unwrap_or(0.0) / 1024.0 / 1024.0,
    })
}

/// `"key"=42` (PerformanceStatistics style) or `"key" = 42` (top level).
#[cfg(target_os = "macos")]
fn stat_number(output: &str, key: &str) -> Option<f64> {
    for pat in [format!("\"{key}\"="), format!("\"{key}\" = ")] {
        if let Some(start) = output.find(&pat) {
            let digits: String = output[start + pat.len()..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(v) = digits.parse() {
                return Some(v);
            }
        }
    }
    None
}

/// `"key" = "value"`.
#[cfg(target_os = "macos")]
fn stat_string(output: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\" = \"");
    let start = output.find(&pat)? + pat.len();
    let end = output[start..].find('"')? + start;
    Some(output[start..end].to_string())
}

/// Loopback / tunnel pseudo-interfaces excluded from the throughput totals.
fn is_virtual_iface(name: &str) -> bool {
    name.starts_with("lo")
        || name.starts_with("utun")
        || name.starts_with("gif")
        || name.starts_with("stf")
        || name.starts_with("anpi")
        || name.starts_with("ap")
        || name.starts_with("bridge")
}

/// Keep only volumes worth showing: the system root, removable (USB)
/// drives and network shares. Other internal volumes are skipped.
fn storage_kind(d: &sysinfo::Disk) -> Option<StorageKind> {
    let fs = d.file_system().to_string_lossy().to_lowercase();
    if fs.contains("nfs")
        || fs.contains("smb")
        || fs.contains("cifs")
        || fs.contains("afp")
        || fs.contains("sshfs")
        || fs.contains("osxfuse")
        || fs.contains("webdav")
    {
        return Some(StorageKind::Network);
    }
    if d.mount_point() == Path::new("/") {
        return Some(StorageKind::Main);
    }
    if d.is_removable() {
        return Some(StorageKind::Usb);
    }
    None
}
