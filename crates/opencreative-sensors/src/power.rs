//! Power from RAPL energy counters (Linux, Intel and AMD): the CPU package,
//! Intel's integrated GPU (`uncore`), memory (`dram`) and, on some machines,
//! the whole platform (`psys`).
//!
//! The counters are root-only by default since CVE-2020-8694. OpenCreative's
//! optional power rule (`61-opencreative-power.rules`) makes them readable;
//! without it every reading is `None`.

/// Average watts per RAPL domain since the previous read.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Watts {
    pub package: Option<f32>,
    pub uncore: Option<f32>,
    pub dram: Option<f32>,
    pub psys: Option<f32>,
}

#[cfg(target_os = "linux")]
pub use linux::Rapl;

#[cfg(not(target_os = "linux"))]
pub struct Rapl;

#[cfg(not(target_os = "linux"))]
impl Rapl {
    pub fn new() -> Self {
        Self
    }

    pub fn read(&mut self) -> Watts {
        Watts::default()
    }

    pub fn readable(&self) -> bool {
        false
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    use super::Watts;

    const ROOT: &str = "/sys/class/powercap";

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Domain {
        Package,
        Uncore,
        Dram,
        Psys,
    }

    struct Zone {
        domain: Domain,
        energy: PathBuf,
        max_uj: u64,
        last: Option<(u64, Instant)>,
    }

    pub struct Rapl {
        zones: Vec<Zone>,
        /// Whether any counter could be read when last tried.
        readable: bool,
    }

    impl Rapl {
        pub fn new() -> Self {
            Self::at(Path::new(ROOT))
        }

        fn at(root: &Path) -> Self {
            let mut zones = Vec::new();
            let mut dirs: Vec<PathBuf> = fs::read_dir(root)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("intel-rapl:"))
                })
                .collect();
            dirs.sort();
            for dir in dirs {
                let Ok(name) = fs::read_to_string(dir.join("name")) else {
                    continue;
                };
                let domain = match name.trim() {
                    n if n.starts_with("package") => Domain::Package,
                    "uncore" => Domain::Uncore,
                    "dram" => Domain::Dram,
                    "psys" => Domain::Psys,
                    _ => continue,
                };
                // Only the first package; a second socket is rare on desktops.
                if zones.iter().any(|z: &Zone| z.domain == domain) {
                    continue;
                }
                let max_uj = fs::read_to_string(dir.join("max_energy_range_uj"))
                    .ok()
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(u64::MAX);
                zones.push(Zone {
                    domain,
                    energy: dir.join("energy_uj"),
                    max_uj,
                    last: None,
                });
            }
            let readable = zones.iter().any(|z| fs::read_to_string(&z.energy).is_ok());
            Self { zones, readable }
        }

        /// Whether the counters exist and this user may read them.
        pub fn readable(&self) -> bool {
            self.readable
        }

        pub fn read(&mut self) -> Watts {
            self.read_at(Instant::now())
        }

        /// [`Rapl::read`] as of `now` (tests pass their own clock).
        fn read_at(&mut self, now: Instant) -> Watts {
            let mut out = Watts::default();
            let mut any = false;
            for zone in &mut self.zones {
                let Some(uj) = fs::read_to_string(&zone.energy)
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                else {
                    continue;
                };
                any = true;
                let previous = zone.last.replace((uj, now));
                let watts = previous.and_then(|(before, then)| {
                    // The counter wraps at max_energy_range_uj.
                    let delta = if uj >= before {
                        uj - before
                    } else {
                        zone.max_uj - before + uj
                    };
                    let secs = now.duration_since(then).as_secs_f64();
                    (secs > 0.0).then(|| (delta as f64 / 1e6 / secs) as f32)
                });
                let slot = match zone.domain {
                    Domain::Package => &mut out.package,
                    Domain::Uncore => &mut out.uncore,
                    Domain::Dram => &mut out.dram,
                    Domain::Psys => &mut out.psys,
                };
                *slot = watts;
            }
            self.readable = any;
            out
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn zone(root: &Path, dir: &str, name: &str, uj: u64) {
            let dir = root.join(dir);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("name"), format!("{name}\n")).unwrap();
            fs::write(dir.join("energy_uj"), format!("{uj}\n")).unwrap();
            fs::write(dir.join("max_energy_range_uj"), "1000000000\n").unwrap();
        }

        #[test]
        fn finds_domains_and_measures_watts() {
            let root = std::env::temp_dir().join(format!("oc-rapl-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            zone(&root, "intel-rapl:0", "package-0", 1_000);
            zone(&root, "intel-rapl:0:0", "core", 0);
            zone(&root, "intel-rapl:0:1", "uncore", 5_000);
            zone(&root, "intel-rapl:1", "psys", 999_999_000);
            let mut rapl = Rapl::at(&root);
            assert!(rapl.readable());
            assert_eq!(rapl.zones.len(), 3, "core is skipped");
            // The first read has nothing to compare against.
            let start = Instant::now();
            assert_eq!(rapl.read_at(start), Watts::default());
            zone(&root, "intel-rapl:0", "package-0", 41_001_000);
            // The platform counter wrapped.
            zone(&root, "intel-rapl:1", "psys", 59_000_000);
            // Exactly one second later, however slow the machine is.
            let w = rapl.read_at(start + std::time::Duration::from_secs(1));
            let near = |a: Option<f32>, b: f32| (a.unwrap() - b).abs() < 1.0;
            assert!(near(w.package, 41.0), "{w:?}");
            assert!(near(w.psys, 59.0), "{w:?}");
            assert!(near(w.uncore, 0.0), "{w:?}");
            assert_eq!(w.dram, None);
            fs::remove_dir_all(&root).unwrap();
        }

        #[test]
        fn missing_counters_are_unreadable() {
            let rapl = Rapl::at(Path::new("/nonexistent"));
            assert!(!rapl.readable());
        }
    }
}
