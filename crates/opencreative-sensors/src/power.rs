//! CPU package power from RAPL energy counters (Linux, Intel and AMD).
//!
//! The counters are root-only by default since CVE-2020-8694, so this is
//! usually `None` for normal users; we don't loosen that.

#[cfg(target_os = "linux")]
pub use linux::Rapl;

#[cfg(not(target_os = "linux"))]
pub struct Rapl;

#[cfg(not(target_os = "linux"))]
impl Rapl {
    pub fn new() -> Self {
        Self
    }

    pub fn read(&mut self) -> Option<f32> {
        None
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::fs;
    use std::path::PathBuf;
    use std::time::Instant;

    const ZONE: &str = "/sys/class/powercap/intel-rapl:0";

    pub struct Rapl {
        energy: Option<PathBuf>,
        max_uj: u64,
        last: Option<(u64, Instant)>,
    }

    impl Rapl {
        pub fn new() -> Self {
            let energy = PathBuf::from(ZONE).join("energy_uj");
            let readable = fs::read_to_string(&energy).is_ok();
            let max_uj = fs::read_to_string(PathBuf::from(ZONE).join("max_energy_range_uj"))
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(u64::MAX);
            Self {
                energy: readable.then_some(energy),
                max_uj,
                last: None,
            }
        }

        /// Average watts since the previous call.
        pub fn read(&mut self) -> Option<f32> {
            let uj: u64 = fs::read_to_string(self.energy.as_ref()?)
                .ok()?
                .trim()
                .parse()
                .ok()?;
            let now = Instant::now();
            let previous = self.last.replace((uj, now));
            let (before, then) = previous?;
            // The counter wraps at max_energy_range_uj.
            let delta = if uj >= before {
                uj - before
            } else {
                self.max_uj - before + uj
            };
            let secs = now.duration_since(then).as_secs_f64();
            (secs > 0.0).then(|| (delta as f64 / 1e6 / secs) as f32)
        }
    }
}
