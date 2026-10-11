//! Hardware sensor readings for the cooler screen and the app's dashboard.
//!
//! Backed by `sysinfo` so the same code works on Linux, Windows and macOS,
//! plus vendor sources where `sysinfo` has nothing (GPU details, CPU power).
//! Anything unavailable is reported as `None` rather than guessed.

mod gpu;
mod metric;
mod power;

use std::time::Instant;

use sysinfo::{
    Components, CpuRefreshKind, DiskRefreshKind, Disks, MemoryRefreshKind, Networks, RefreshKind,
    System,
};

pub use gpu::Gpu;
pub use metric::{Metric, Reading};

/// One reading of everything the screen and dashboard may display.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub cpu_name: String,
    pub cpu_temp_c: Option<f32>,
    pub cpu_load_pct: f32,
    pub cpu_mhz: u64,
    /// Package power. Needs read access to RAPL counters on Linux.
    pub cpu_power_w: Option<f32>,
    pub gpu: Gpu,
    pub mem_used_pct: f32,
    pub mem_used_bytes: u64,
    pub mem_total_bytes: u64,
    pub disks: Vec<Disk>,
    pub disk_read_bps: f64,
    pub disk_write_bps: f64,
    pub net_rx_bps: f64,
    pub net_tx_bps: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Disk {
    /// Device name, e.g. `nvme0n1p2`.
    pub name: String,
    pub mount_point: String,
    pub used_bytes: u64,
    pub total_bytes: u64,
}

/// Sensor labels that identify the CPU package temperature, most specific first.
const CPU_LABELS: &[&str] = &["tctl", "package id", "tdie", "k10temp", "coretemp", "cpu"];
const GPU_LABELS: &[&str] = &["amdgpu", "nouveau", "nvidia", "gpu", "radeon"];

pub struct Sensors {
    system: System,
    components: Components,
    disks: Disks,
    networks: Networks,
    gpu: gpu::Probe,
    power: power::Rapl,
    last_read: Instant,
}

impl Sensors {
    pub fn new() -> Self {
        let system = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::nothing().with_cpu_usage().with_frequency())
                .with_memory(MemoryRefreshKind::nothing().with_ram()),
        );
        Self {
            system,
            components: Components::new_with_refreshed_list(),
            disks: Disks::new_with_refreshed_list_specifics(
                DiskRefreshKind::nothing().with_storage().with_io_usage(),
            ),
            networks: Networks::new_with_refreshed_list(),
            gpu: gpu::Probe::new(),
            power: power::Rapl::new(),
            last_read: Instant::now(),
        }
    }

    /// Refreshes all sensors. Load and rates are measured since the previous
    /// call, so the first reading is only meaningful after about a second.
    pub fn read(&mut self) -> Snapshot {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_read).as_secs_f64().max(1e-3);
        self.last_read = now;

        self.system
            .refresh_cpu_specifics(CpuRefreshKind::nothing().with_cpu_usage().with_frequency());
        self.system.refresh_memory();
        self.components.refresh(false);
        self.disks.refresh(true);
        self.networks.refresh(true);

        let total = self.system.total_memory();
        let used = self.system.used_memory();
        let mut gpu = self.gpu.read();
        if gpu.temp_c.is_none() {
            gpu.temp_c = self.find_temp(GPU_LABELS);
        }

        let (read, written) = self
            .real_disks()
            .map(|d| d.usage())
            .fold((0, 0), |(r, w), u| (r + u.read_bytes, w + u.written_bytes));
        let (rx, tx) = self
            .networks
            .list()
            .iter()
            .filter(|(name, _)| *name != "lo")
            .fold((0, 0), |(r, t), (_, n)| {
                (r + n.received(), t + n.transmitted())
            });

        Snapshot {
            cpu_name: self
                .system
                .cpus()
                .first()
                .map(|c| c.brand().trim().to_owned())
                .unwrap_or_default(),
            cpu_temp_c: self.find_temp(CPU_LABELS),
            cpu_load_pct: self.system.global_cpu_usage(),
            cpu_mhz: self
                .system
                .cpus()
                .iter()
                .map(|c| c.frequency())
                .max()
                .unwrap_or(0),
            cpu_power_w: self.power.read(),
            gpu,
            mem_used_pct: if total == 0 {
                0.0
            } else {
                used as f32 / total as f32 * 100.0
            },
            mem_used_bytes: used,
            mem_total_bytes: total,
            disks: self.disk_list(),
            disk_read_bps: read as f64 / elapsed,
            disk_write_bps: written as f64 / elapsed,
            net_rx_bps: rx as f64 / elapsed,
            net_tx_bps: tx as f64 / elapsed,
        }
    }

    /// Block devices worth showing: no loop devices (snaps), FUSE or other
    /// virtual filesystems that mirror a real disk.
    fn real_disks(&self) -> impl Iterator<Item = &sysinfo::Disk> {
        const VIRTUAL: &[&str] = &["fuse", "overlay", "squashfs", "tmpfs", "devtmpfs"];
        self.disks.list().iter().filter(|d| {
            let name = d.name().to_string_lossy();
            let fs = d.file_system().to_string_lossy().to_lowercase();
            d.total_space() > 0
                && !name.is_empty()
                && !name.contains("loop")
                && !VIRTUAL.iter().any(|v| fs.starts_with(v))
        })
    }

    /// One entry per device (a partition mounted twice is listed once).
    fn disk_list(&self) -> Vec<Disk> {
        let mut out: Vec<Disk> = Vec::new();
        for d in self.real_disks() {
            let full = d.name().to_string_lossy();
            let name = full.rsplit('/').next().unwrap_or(&full).to_owned();
            if out.iter().any(|seen| seen.name == name) {
                continue;
            }
            out.push(Disk {
                name,
                mount_point: d.mount_point().display().to_string(),
                used_bytes: d.total_space().saturating_sub(d.available_space()),
                total_bytes: d.total_space(),
            });
        }
        out
    }

    /// Returns the first readable temperature whose label matches a keyword,
    /// trying keywords in priority order.
    fn find_temp(&self, keywords: &[&str]) -> Option<f32> {
        keywords.iter().find_map(|kw| {
            self.components.iter().find_map(|c| {
                let label = c.label().to_lowercase();
                if label.contains(kw) {
                    c.temperature().filter(|t| t.is_finite() && *t > 0.0)
                } else {
                    None
                }
            })
        })
    }

    /// Every temperature sensor, for diagnostics.
    pub fn all_temperatures(&self) -> Vec<(String, Option<f32>)> {
        self.components
            .iter()
            .map(|c| (c.label().to_owned(), c.temperature()))
            .collect()
    }
}

impl Default for Sensors {
    fn default() -> Self {
        Self::new()
    }
}
