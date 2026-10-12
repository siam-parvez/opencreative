//! "Report a problem" and "Request support for your cooler": GitHub issue
//! forms opened in the browser with the technical details already filled
//! in, so people only describe what they saw.
//!
//! Nothing is sent by the app itself; the details go into the link, and the
//! user sees them on GitHub before submitting.

use std::fmt::Write as _;
use std::fs;

use crate::{App, Link, service};

const NEW_ISSUE: &str = "https://github.com/siam-parvez/opencreative/issues/new";

/// The problem form, with this system's details filled in.
pub fn problem_url(app: &App) -> String {
    format!(
        "{NEW_ISSUE}?template=problem.yml&system={}",
        encode(&system_report(app))
    )
}

/// The new-cooler form, with the DeepCool USB devices found filled in.
pub fn device_url(app: &App) -> String {
    let mut details = String::new();
    let found = deepcool_usb_devices();
    if found.is_empty() {
        details.push_str("No DeepCool USB device (vendor 3633) found.\n");
    } else {
        details.push_str("DeepCool USB devices:\n");
        for device in found {
            let _ = writeln!(details, "  {device}");
        }
    }
    details.push('\n');
    details.push_str(&system_report(app));
    format!(
        "{NEW_ISSUE}?template=new-device.yml&usb={}",
        encode(&details)
    )
}

/// Version, system, install and what the service is doing.
pub fn system_report(app: &App) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "OpenCreative {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(out, "System: {}", os_name());
    if let Ok(kernel) = fs::read_to_string("/proc/sys/kernel/osrelease") {
        let _ = writeln!(out, "Kernel: {}", kernel.trim());
    }
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "unknown".into());
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".into());
    let _ = writeln!(out, "Desktop: {desktop} ({session})");
    let install = if std::env::var_os("APPIMAGE").is_some() {
        "AppImage"
    } else if service::packaged() {
        "package (.deb/.rpm)"
    } else {
        "other (tarball or source)"
    };
    let _ = writeln!(out, "Installed as: {install}");
    let link = match &app.link {
        Link::External => "background service".to_owned(),
        Link::Embedded => "inside the app window".to_owned(),
        Link::Connecting | Link::WaitingForService(_) => "connecting".to_owned(),
        Link::Unavailable(e) => format!("unavailable: {e}"),
    };
    let _ = writeln!(out, "Running: {link}");
    let _ = writeln!(
        out,
        "Cooler: {}",
        app.device.as_deref().unwrap_or("none connected")
    );
    let _ = writeln!(
        out,
        "Showing: {}{}",
        scene_name(&app.applied.scene),
        if app.zen { " (Zen mode)" } else { "" }
    );
    out
}

fn os_name() -> String {
    fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                .map(|v| v.trim_matches('"').to_owned())
        })
        .unwrap_or_else(|| "Linux".to_owned())
}

fn scene_name(scene: &opencreative_config::SceneConfig) -> &'static str {
    use opencreative_config::SceneConfig as S;
    match scene {
        S::Monitor { .. } => "gauges",
        S::Panels { .. } => "panels",
        S::Image { .. } | S::Media { .. } => "media",
        S::Solid { .. } => "color",
        S::Music { .. } => "music bars",
    }
}

/// DeepCool's USB devices (vendor 3633), as `vid:pid name`, from sysfs.
fn deepcool_usb_devices() -> Vec<String> {
    let read = |dir: &std::path::Path, file: &str| {
        fs::read_to_string(dir.join(file))
            .map(|s| s.trim().to_owned())
            .unwrap_or_default()
    };
    let mut found: Vec<String> = fs::read_dir("/sys/bus/usb/devices")
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|dir| read(dir, "idVendor") == "3633")
        .map(|dir| {
            let name = [read(&dir, "manufacturer"), read(&dir, "product")]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            format!("3633:{} {name}", read(&dir, "idProduct"))
                .trim()
                .to_owned()
        })
        .collect();
    found.sort();
    found.dedup();
    found
}

/// Percent-encodes `text` for a URL query value.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => {
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_query_values() {
        assert_eq!(encode("a b&c=d\n"), "a%20b%26c%3Dd%0A");
        assert_eq!(encode("°"), "%C2%B0");
        assert_eq!(encode("v0.4.2_x-y~"), "v0.4.2_x-y~");
    }
}
