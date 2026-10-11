//! Hardware sensor readings shown on the cooler screen.
//!
//! Backed by `sysinfo` so the same code works on Linux, Windows and macOS.
//! Temperature sensor names differ per platform and vendor, so they are matched
//! by keyword; anything unmatched is reported as `None` rather than guessed.

use sysinfo::{Components, CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

/// One reading of everything a screen theme may display.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub cpu_temp_c: Option<f32>,
    pub cpu_load_pct: f32,
    pub cpu_mhz: u64,
    pub gpu_temp_c: Option<f32>,
    pub mem_used_pct: f32,
}

/// Sensor labels that identify the CPU package temperature, most specific first.
const CPU_LABELS: &[&str] = &["tctl", "package id", "tdie", "k10temp", "coretemp", "cpu"];
const GPU_LABELS: &[&str] = &["amdgpu", "nouveau", "nvidia", "gpu", "radeon"];

pub struct Sensors {
    system: System,
    components: Components,
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
        }
    }

    /// Refreshes all sensors. CPU load is measured since the previous call, so
    /// the first reading is only meaningful after `sysinfo::MINIMUM_CPU_UPDATE_INTERVAL`.
    pub fn read(&mut self) -> Snapshot {
        self.system
            .refresh_cpu_specifics(CpuRefreshKind::nothing().with_cpu_usage().with_frequency());
        self.system.refresh_memory();
        self.components.refresh(false);

        let total = self.system.total_memory();
        let mem_used_pct = if total == 0 {
            0.0
        } else {
            self.system.used_memory() as f32 / total as f32 * 100.0
        };

        Snapshot {
            cpu_temp_c: self.find_temp(CPU_LABELS),
            cpu_load_pct: self.system.global_cpu_usage(),
            cpu_mhz: self
                .system
                .cpus()
                .iter()
                .map(|c| c.frequency())
                .max()
                .unwrap_or(0),
            gpu_temp_c: self.find_temp(GPU_LABELS),
            mem_used_pct,
        }
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

    /// Every temperature sensor, for `opencreative sensors` diagnostics.
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
