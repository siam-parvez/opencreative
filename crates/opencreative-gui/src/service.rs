//! Desktop integration on Linux: the background service (a systemd user
//! unit), the udev rule for USB access, and the app-menu entry.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub type Result<T> = std::result::Result<T, String>;

const UNIT_NAME: &str = "opencreative";
const UNIT_TEMPLATE: &str = include_str!("../../../packaging/linux/opencreative.service");
const UNIT_EXEC_PLACEHOLDER: &str = "%h/.cargo/bin/opencreative";

const UDEV_RULES: &str = include_str!("../../../packaging/linux/60-opencreative.rules");
const UDEV_RULE_PATH: &str = "/etc/udev/rules.d/60-opencreative.rules";
/// Where distribution packages put the rule instead.
const UDEV_PACKAGED_PATH: &str = "/usr/lib/udev/rules.d/60-opencreative.rules";

const DESKTOP_TEMPLATE: &str = include_str!("../../../packaging/linux/opencreative.desktop");

/// What systemd says about the background service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceState {
    pub installed: bool,
    pub enabled: bool,
    pub active: bool,
}

pub fn query() -> ServiceState {
    let succeeds = |verb| systemctl(&[verb, "--quiet", UNIT_NAME]).is_ok();
    ServiceState {
        installed: unit_path().is_some_and(|p| p.exists()),
        enabled: succeeds("is-enabled"),
        active: succeeds("is-active"),
    }
}

/// Installs the unit if needed, then enables and starts it.
pub fn enable() -> Result<()> {
    install_unit()?;
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", "--now", UNIT_NAME])
}

pub fn disable() -> Result<()> {
    systemctl(&["disable", "--now", UNIT_NAME])
}

fn systemctl(args: &[&str]) -> Result<()> {
    let out = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .map_err(|e| format!("running systemctl: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

fn unit_path() -> Option<PathBuf> {
    Some(
        dirs::config_dir()?
            .join("systemd/user")
            .join(format!("{UNIT_NAME}.service")),
    )
}

/// Writes the unit pointing at the installed `opencreative` command. An
/// existing unit (e.g. one the user edited) is left alone.
fn install_unit() -> Result<()> {
    let path = unit_path().ok_or("no config directory")?;
    if path.exists() {
        return Ok(());
    }
    let cli = cli_binary().ok_or(
        "couldn't find the `opencreative` command; install it with \
         `cargo install --path crates/opencreative-cli`",
    )?;
    let unit = UNIT_TEMPLATE.replace(UNIT_EXEC_PLACEHOLDER, &cli.display().to_string());
    write(&path, unit.as_bytes())
}

/// The `opencreative` CLI: next to this app, else on `PATH`.
fn cli_binary() -> Option<PathBuf> {
    let beside = env::current_exe().ok()?.with_file_name("opencreative");
    if beside.is_file() {
        return Some(beside);
    }
    env::split_paths(&env::var_os("PATH")?)
        .map(|dir| dir.join("opencreative"))
        .find(|p| p.is_file())
}

pub fn udev_rule_installed() -> bool {
    Path::new(UDEV_RULE_PATH).exists() || Path::new(UDEV_PACKAGED_PATH).exists()
}

/// Shell that writes the rule and reloads udev. Needs root.
fn udev_script() -> String {
    let lines: Vec<String> = UDEV_RULES
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        // Rule lines contain double quotes but never single quotes.
        .map(|l| format!("'{l}'"))
        .collect();
    format!(
        "printf '%s\\n' {} > {UDEV_RULE_PATH} && udevadm control --reload && udevadm trigger",
        lines.join(" ")
    )
}

/// The same as a command the user can paste into a terminal.
pub fn udev_install_command() -> String {
    format!("sudo sh -c \"{}\"", udev_script().replace('"', "\\\""))
}

/// Installs the rule, asking for the admin password through polkit.
/// Blocks until the user answers the prompt.
pub fn install_udev_rule() -> Result<()> {
    let out = Command::new("pkexec")
        .args(["sh", "-c", &udev_script()])
        .output()
        .map_err(|e| format!("running pkexec: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        Err(if err.is_empty() {
            "cancelled".to_owned()
        } else {
            err
        })
    }
}

fn desktop_entry_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("applications/opencreative.desktop"))
}

fn icon_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("icons/hicolor/256x256/apps/opencreative.png"))
}

pub fn desktop_entry_installed() -> bool {
    desktop_entry_path().is_some_and(|p| p.exists())
}

/// Adds OpenCreative to the desktop's app menu, launching this binary.
pub fn install_desktop_entry() -> Result<()> {
    let exe = env::current_exe().map_err(|e| e.to_string())?;
    let entry = DESKTOP_TEMPLATE.replace(
        "Exec=opencreative-gui",
        &format!("Exec=\"{}\"", exe.display()),
    );
    let icon = icon_path().ok_or("no data directory")?;
    if let Some(dir) = icon.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    opencreative_render::icon::icon(256)
        .save_png(&icon)
        .map_err(|e| format!("{}: {e}", icon.display()))?;
    write(
        &desktop_entry_path().ok_or("no data directory")?,
        entry.as_bytes(),
    )
}

pub fn remove_desktop_entry() -> Result<()> {
    for path in [desktop_entry_path(), icon_path()].into_iter().flatten() {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
    }
    Ok(())
}

fn write(path: &Path, contents: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::write(path, contents).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_template_has_the_exec_placeholder() {
        assert!(UNIT_TEMPLATE.contains(&format!("ExecStart={UNIT_EXEC_PLACEHOLDER} daemon")));
    }

    #[test]
    fn desktop_template_has_the_exec_line() {
        assert!(DESKTOP_TEMPLATE.contains("\nExec=opencreative-gui\n"));
    }

    #[test]
    fn udev_script_writes_only_rule_lines() {
        let script = udev_script();
        assert!(script.contains("ATTRS{idVendor}==\"3633\""));
        assert!(!script.contains("# "), "comments are dropped");
        assert!(script.contains(UDEV_RULE_PATH));
    }
}
