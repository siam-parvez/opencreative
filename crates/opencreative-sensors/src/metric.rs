//! Values a screen layout can show, with their labels and formatting.

use serde::{Deserialize, Serialize};

use crate::Snapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    CpuTemp,
    CpuLoad,
    CpuFreq,
    CpuPower,
    GpuTemp,
    GpuLoad,
    GpuFreq,
    GpuPower,
    RamUsage,
    RamUsed,
    /// The whole computer's power draw (an estimate; see `Snapshot`).
    SystemPower,
    /// Claude's 5-hour limit, used percent.
    #[serde(rename = "claude_5h")]
    ClaudeSession,
    /// Claude's weekly limit, used percent.
    #[serde(rename = "claude_week")]
    ClaudeWeekly,
}

/// A formatted value: `value` is `None` when the sensor isn't available.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub value: Option<String>,
    pub unit: &'static str,
}

impl Metric {
    pub const ALL: [Metric; 13] = [
        Metric::CpuTemp,
        Metric::CpuLoad,
        Metric::CpuFreq,
        Metric::CpuPower,
        Metric::GpuTemp,
        Metric::GpuLoad,
        Metric::GpuFreq,
        Metric::GpuPower,
        Metric::RamUsage,
        Metric::RamUsed,
        Metric::SystemPower,
        Metric::ClaudeSession,
        Metric::ClaudeWeekly,
    ];

    /// Whether the reading comes from the optional Claude integration.
    pub fn is_claude(self) -> bool {
        matches!(self, Metric::ClaudeSession | Metric::ClaudeWeekly)
    }

    /// Name used in config files and on the command line, e.g. `cpu_temp`.
    pub fn key(self) -> &'static str {
        match self {
            Metric::CpuTemp => "cpu_temp",
            Metric::CpuLoad => "cpu_load",
            Metric::CpuFreq => "cpu_freq",
            Metric::CpuPower => "cpu_power",
            Metric::GpuTemp => "gpu_temp",
            Metric::GpuLoad => "gpu_load",
            Metric::GpuFreq => "gpu_freq",
            Metric::GpuPower => "gpu_power",
            Metric::RamUsage => "ram_usage",
            Metric::RamUsed => "ram_used",
            Metric::SystemPower => "system_power",
            Metric::ClaudeSession => "claude_5h",
            Metric::ClaudeWeekly => "claude_week",
        }
    }

    /// Short label for the screen, e.g. `CPU TEMP`.
    pub fn label(self) -> &'static str {
        match self {
            Metric::CpuTemp => "CPU TEMP",
            Metric::CpuLoad => "CPU LOAD",
            Metric::CpuFreq => "CPU FREQ",
            Metric::CpuPower => "CPU POWER",
            Metric::GpuTemp => "GPU TEMP",
            Metric::GpuLoad => "GPU LOAD",
            Metric::GpuFreq => "GPU FREQ",
            Metric::GpuPower => "GPU POWER",
            Metric::RamUsage => "RAM USAGE",
            Metric::RamUsed => "RAM USED",
            Metric::SystemPower => "PC POWER",
            Metric::ClaudeSession => "5 HOURS",
            Metric::ClaudeWeekly => "WEEKLY",
        }
    }

    /// Human name for menus, e.g. `CPU temperature`.
    pub fn name(self) -> &'static str {
        match self {
            Metric::CpuTemp => "CPU temperature",
            Metric::CpuLoad => "CPU load",
            Metric::CpuFreq => "CPU frequency",
            Metric::CpuPower => "CPU power",
            Metric::GpuTemp => "GPU temperature",
            Metric::GpuLoad => "GPU load",
            Metric::GpuFreq => "GPU frequency",
            Metric::GpuPower => "GPU power",
            Metric::RamUsage => "RAM usage",
            Metric::RamUsed => "RAM used",
            Metric::SystemPower => "PC power (estimate)",
            Metric::ClaudeSession => "Claude 5-hour limit",
            Metric::ClaudeWeekly => "Claude weekly limit",
        }
    }

    /// A second line under the label, e.g. `RESETS 4H 23M` for Claude limits.
    pub fn detail(self, s: &Snapshot) -> Option<String> {
        let window = match self {
            Metric::ClaudeSession => s.claude?.session?,
            Metric::ClaudeWeekly => s.claude?.weekly?,
            _ => return None,
        };
        let secs = window.resets_in_secs?;
        Some(format!("RESETS {}", countdown(secs).to_uppercase()))
    }

    pub fn read(self, s: &Snapshot) -> Reading {
        let r = |value: Option<String>, unit| Reading { value, unit };
        let one = |v: f32| format!("{v:.0}");
        match self {
            Metric::CpuTemp => r(s.cpu_temp_c.map(one), "°C"),
            Metric::CpuLoad => r(Some(one(s.cpu_load_pct)), "%"),
            Metric::CpuFreq => r(ghz(s.cpu_mhz as f32), "GHz"),
            Metric::CpuPower => r(s.cpu_power_w.map(watts), "W"),
            Metric::GpuTemp => r(s.gpu.temp_c.map(one), "°C"),
            Metric::GpuLoad => r(s.gpu.load_pct.map(one), "%"),
            Metric::GpuFreq => r(s.gpu.mhz.and_then(|m| ghz(m as f32)), "GHz"),
            Metric::GpuPower => r(s.gpu.power_w.map(watts), "W"),
            Metric::RamUsage => r(Some(one(s.mem_used_pct)), "%"),
            Metric::RamUsed => r(
                (s.mem_total_bytes > 0).then(|| format!("{:.1}", s.mem_used_bytes as f64 / GIB)),
                "GB",
            ),
            Metric::SystemPower => r(s.system_power_w.map(watts), "W"),
            Metric::ClaudeSession => r(s.claude.and_then(|c| c.session).map(|w| one(w.pct)), "%"),
            Metric::ClaudeWeekly => r(s.claude.and_then(|c| c.weekly).map(|w| one(w.pct)), "%"),
        }
    }
}

/// Time left in a short form: `5d 22h`, `4h 23m` or `5m`.
pub fn countdown(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, secs / 3600 % 24, secs / 60 % 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

impl std::str::FromStr for Metric {
    type Err = String;

    /// Accepts `cpu_temp` or `cpu-temp`.
    fn from_str(s: &str) -> Result<Self, String> {
        let key = s.replace('-', "_");
        Metric::ALL
            .into_iter()
            .find(|m| m.key() == key)
            .ok_or_else(|| {
                let names: Vec<_> = Metric::ALL.iter().map(|m| m.key()).collect();
                format!(
                    "unknown metric `{s}`; expected one of: {}",
                    names.join(", ")
                )
            })
    }
}

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

fn ghz(mhz: f32) -> Option<String> {
    (mhz > 0.0).then(|| format!("{:.1}", mhz / 1000.0))
}

/// Whole watts from 100 W up, one decimal below.
fn watts(w: f32) -> String {
    if w >= 100.0 {
        format!("{w:.0}")
    } else {
        format!("{w:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_values_and_missing_sensors() {
        let s = Snapshot {
            cpu_temp_c: Some(61.4),
            cpu_mhz: 5520,
            cpu_power_w: Some(59.94),
            mem_used_bytes: 21_045_000_000,
            mem_total_bytes: 33_000_000_000,
            ..Snapshot::default()
        };
        assert_eq!(Metric::CpuTemp.read(&s).value.as_deref(), Some("61"));
        assert_eq!(Metric::CpuFreq.read(&s).value.as_deref(), Some("5.5"));
        assert_eq!(Metric::CpuPower.read(&s).value.as_deref(), Some("59.9"));
        assert_eq!(Metric::RamUsed.read(&s).value.as_deref(), Some("19.6"));
        assert_eq!(Metric::GpuTemp.read(&s).value, None);
        assert_eq!(Metric::GpuFreq.read(&s).value, None);
    }

    #[test]
    fn claude_limits_show_time_to_reset() {
        let window = |pct, resets_in_secs| {
            Some(crate::ClaudeWindow {
                pct,
                resets_in_secs,
            })
        };
        let s = Snapshot {
            claude: Some(crate::ClaudeUsage {
                session: window(8.0, Some(15794)),
                weekly: window(58.0, None),
            }),
            ..Snapshot::default()
        };
        assert_eq!(Metric::ClaudeSession.read(&s).value.as_deref(), Some("8"));
        assert_eq!(
            Metric::ClaudeSession.detail(&s).as_deref(),
            Some("RESETS 4H 23M")
        );
        assert_eq!(Metric::ClaudeWeekly.detail(&s), None);
        assert_eq!(Metric::CpuTemp.detail(&s), None);
        assert_eq!(countdown(513794), "5d 22h");
        assert_eq!(countdown(300), "5m");
    }

    #[test]
    fn keys_round_trip_and_match_serde() {
        for m in Metric::ALL {
            assert_eq!(m.key().parse::<Metric>(), Ok(m));
            assert_eq!(
                serde_json::to_string(&m).unwrap(),
                format!("\"{}\"", m.key())
            );
        }
        assert_eq!("cpu-temp".parse::<Metric>(), Ok(Metric::CpuTemp));
        assert!("bogus".parse::<Metric>().is_err());
    }

    #[test]
    fn serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&Metric::CpuTemp).unwrap(),
            "\"cpu_temp\""
        );
    }
}
