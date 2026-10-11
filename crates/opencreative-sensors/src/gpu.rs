//! GPU readings. `sysinfo` has none, so each vendor is read its own way:
//! NVIDIA through NVML (loaded at runtime, only if the driver is installed),
//! AMD and Intel through Linux sysfs.

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Gpu {
    pub name: Option<String>,
    pub temp_c: Option<f32>,
    pub load_pct: Option<f32>,
    pub mhz: Option<u32>,
    pub power_w: Option<f32>,
    pub vram_used_bytes: Option<u64>,
    pub vram_total_bytes: Option<u64>,
}

pub struct Probe {
    nvml: Option<nvml_wrapper::Nvml>,
}

impl Probe {
    pub fn new() -> Self {
        // Fails quietly on machines without the NVIDIA driver.
        Self {
            nvml: nvml_wrapper::Nvml::init().ok(),
        }
    }

    pub fn read(&self) -> Gpu {
        if let Some(gpu) = self.nvml.as_ref().and_then(read_nvml) {
            return gpu;
        }
        #[cfg(target_os = "linux")]
        if let Some(gpu) = sysfs::read() {
            return gpu;
        }
        Gpu::default()
    }
}

fn read_nvml(nvml: &nvml_wrapper::Nvml) -> Option<Gpu> {
    use nvml_wrapper::enum_wrappers::device::{Clock, TemperatureSensor};

    let dev = nvml.device_by_index(0).ok()?;
    let memory = dev.memory_info().ok();
    Some(Gpu {
        name: dev.name().ok(),
        temp_c: dev
            .temperature(TemperatureSensor::Gpu)
            .ok()
            .map(|t| t as f32),
        load_pct: dev.utilization_rates().ok().map(|u| u.gpu as f32),
        mhz: dev.clock_info(Clock::Graphics).ok(),
        power_w: dev.power_usage().ok().map(|mw| mw as f32 / 1000.0),
        vram_used_bytes: memory.as_ref().map(|m| m.used),
        vram_total_bytes: memory.map(|m| m.total),
    })
}

#[cfg(target_os = "linux")]
mod sysfs {
    use std::fs;
    use std::path::Path;

    use super::Gpu;

    const AMD: &str = "0x1002";
    const INTEL: &str = "0x8086";

    fn read_u64(path: &Path) -> Option<u64> {
        fs::read_to_string(path).ok()?.trim().parse().ok()
    }

    /// The first AMD or Intel GPU under /sys/class/drm.
    pub fn read() -> Option<Gpu> {
        let mut cards: Vec<_> = fs::read_dir("/sys/class/drm")
            .ok()?
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("card") && !n.contains('-'))
            })
            .collect();
        cards.sort();
        cards.iter().find_map(|card| {
            let device = card.join("device");
            let vendor = fs::read_to_string(device.join("vendor")).ok()?;
            match vendor.trim() {
                AMD => Some(amd(&device)),
                INTEL => Some(intel(card)),
                _ => None,
            }
        })
    }

    fn amd(device: &Path) -> Gpu {
        Gpu {
            load_pct: read_u64(&device.join("gpu_busy_percent")).map(|v| v as f32),
            mhz: active_sclk(device),
            vram_used_bytes: read_u64(&device.join("mem_info_vram_used")),
            vram_total_bytes: read_u64(&device.join("mem_info_vram_total")),
            power_w: hwmon_watts(device),
            ..Gpu::default()
        }
    }

    /// Board power from the card's hwmon, in watts (sysfs has microwatts).
    fn hwmon_watts(device: &Path) -> Option<f32> {
        fs::read_dir(device.join("hwmon"))
            .ok()?
            .flatten()
            .find_map(|e| {
                ["power1_average", "power1_input"]
                    .iter()
                    .find_map(|f| read_u64(&e.path().join(f)))
            })
            .map(|uw| uw as f32 / 1e6)
    }

    /// The active core clock: the line marked `*` in `pp_dpm_sclk`.
    fn active_sclk(device: &Path) -> Option<u32> {
        let table = fs::read_to_string(device.join("pp_dpm_sclk")).ok()?;
        let line = table.lines().find(|l| l.trim_end().ends_with('*'))?;
        let mhz = line.split_whitespace().nth(1)?.trim_end_matches("Mhz");
        mhz.parse().ok()
    }

    fn intel(card: &Path) -> Gpu {
        let mhz = ["gt_act_freq_mhz", "gt_cur_freq_mhz"]
            .iter()
            .find_map(|f| read_u64(&card.join(f)))
            .map(|v| v as u32);
        Gpu {
            mhz,
            ..Gpu::default()
        }
    }
}
