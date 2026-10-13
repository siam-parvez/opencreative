//! Desktop integration on Linux: the background service (a systemd user
//! unit), the udev rule for USB access, and the app-menu entry.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub type Result<T> = std::result::Result<T, String>;

const UNIT_NAME: &str = "opencreative";
/// The unit a distribution package installs.
const PACKAGED_UNIT: &str = "/usr/lib/systemd/user/opencreative.service";
const UNIT_TEMPLATE: &str = include_str!("../../../packaging/linux/opencreative.service");
const UNIT_EXEC_PLACEHOLDER: &str = "%h/.cargo/bin/opencreative";

/// A udev rule we can install with the admin password.
struct Rule {
    text: &'static str,
    path: &'static str,
    /// Where distribution packages put the rule instead.
    packaged: &'static str,
    /// Re-applies the rule to devices that are already there.
    trigger: &'static str,
}

/// USB access to the cooler.
const USB_RULE: Rule = Rule {
    text: include_str!("../../../packaging/linux/60-opencreative.rules"),
    path: "/etc/udev/rules.d/60-opencreative.rules",
    packaged: "/usr/lib/udev/rules.d/60-opencreative.rules",
    trigger: "udevadm trigger",
};

/// Readable CPU energy counters, for power readings. Optional.
const POWER_RULE: Rule = Rule {
    text: include_str!("../../../packaging/linux/61-opencreative-power.rules"),
    path: "/etc/udev/rules.d/61-opencreative-power.rules",
    packaged: "/usr/lib/udev/rules.d/61-opencreative-power.rules",
    // Settle, so the counters are readable when the switch flips.
    trigger: "udevadm trigger --subsystem-match=powercap --action=add && udevadm settle",
};

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
        installed: unit_path().is_some_and(|p| p.exists()) || Path::new(PACKAGED_UNIT).exists(),
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

/// Restarts the service if it's running (e.g. an older version after an
/// upgrade), without waiting for it.
pub fn restart() -> Result<()> {
    systemctl(&["daemon-reload"])?;
    systemctl(&["try-restart", "--no-block", UNIT_NAME])
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
    // A packaged install ships its own unit, removed with the package.
    if path.exists() || (packaged() && Path::new(PACKAGED_UNIT).exists()) {
        return Ok(());
    }
    let cli = cli_binary().ok_or(
        "couldn't find the `opencreative` command; install it with \
         `cargo install --path crates/opencreative-cli`",
    )?;
    let unit = UNIT_TEMPLATE.replace(UNIT_EXEC_PLACEHOLDER, &cli.display().to_string());
    write(&path, unit.as_bytes())
}

/// Installed by a distribution package (binary under `/usr`, not
/// `/usr/local`): the package manager handles the menu entry and removal.
pub fn packaged() -> bool {
    env::var_os("APPIMAGE").is_none()
        && env::current_exe().is_ok_and(|p| p.starts_with("/usr") && !p.starts_with("/usr/local"))
}

/// The `opencreative` CLI: the AppImage (its launcher runs the CLI when
/// given arguments), else next to this app, else on `PATH`.
fn cli_binary() -> Option<PathBuf> {
    if let Some(image) = env::var_os("APPIMAGE") {
        return Some(image.into());
    }
    let beside = env::current_exe().ok()?.with_file_name("opencreative");
    if beside.is_file() {
        return Some(beside);
    }
    env::split_paths(&env::var_os("PATH")?)
        .map(|dir| dir.join("opencreative"))
        .find(|p| p.is_file())
}

pub fn udev_rule_installed() -> bool {
    USB_RULE.installed()
}

/// The USB rule's install script as a command to paste into a terminal.
pub fn udev_install_command() -> String {
    format!("sudo sh -c \"{}\"", USB_RULE.script().replace('"', "\\\""))
}

/// Installs the USB rule, asking for the admin password through polkit.
/// Blocks until the user answers the prompt.
pub fn install_udev_rule() -> Result<()> {
    as_root(&USB_RULE.script())
}

pub fn power_rule_installed() -> bool {
    POWER_RULE.installed()
}

/// Installs or removes the power rule (asks for the admin password).
/// Removing it also makes the counters root-only again right away.
pub fn set_power_rule(on: bool) -> Result<()> {
    if on {
        as_root(&POWER_RULE.script())
    } else {
        as_root(&format!(
            "rm -f {} && udevadm control --reload && chmod 0400 /sys/class/powercap/intel-rapl:*/energy_uj",
            POWER_RULE.path
        ))
    }
}

impl Rule {
    fn installed(&self) -> bool {
        Path::new(self.path).exists() || Path::new(self.packaged).exists()
    }

    /// Shell that writes the rule and applies it. Needs root.
    fn script(&self) -> String {
        let lines: Vec<String> = self
            .text
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            // Rule lines contain double quotes but never single quotes.
            .map(|l| format!("'{l}'"))
            .collect();
        format!(
            "printf '%s\\n' {} > {} && udevadm control --reload && {}",
            lines.join(" "),
            self.path,
            self.trigger
        )
    }
}

/// Uninstalls a download install with the CLI, removing the settings too
/// when `purge`. Blocks until done (and any password prompt is answered).
pub fn uninstall(purge: bool) -> Result<()> {
    let cli = cli_binary().ok_or("couldn't find the `opencreative` command")?;
    let mut cmd = Command::new(cli);
    cmd.args(["uninstall", "--yes"]);
    if purge {
        cmd.arg("--purge");
    }
    let out = cmd
        .output()
        .map_err(|e| format!("running uninstall: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

/// The official yt-dlp release (a single file that runs with Python 3).
const YTDLP_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp";

/// Downloads yt-dlp into OpenCreative's tools folder, where video playback
/// looks for it. Distribution packages of yt-dlp are often too old for
/// YouTube, so the app offers the current release.
pub fn download_ytdlp() -> Result<()> {
    use std::io::Read as _;
    use std::os::unix::fs::PermissionsExt as _;
    let dir = opencreative_render::video::tools_dir().ok_or("no data directory")?;
    let mut response = ureq::get(YTDLP_URL)
        .call()
        .map_err(|e| format!("downloading yt-dlp: {e}"))?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(64 << 20)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("downloading yt-dlp: {e}"))?;
    // A Python zip app starts with a shebang line.
    if !bytes.starts_with(b"#!") {
        return Err("the download wasn't yt-dlp".to_owned());
    }
    let path = dir.join("yt-dlp");
    write(&path, &bytes)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())
}

/// Runs a shell script as root through polkit's password prompt.
fn as_root(script: &str) -> Result<()> {
    let out = Command::new("pkexec")
        .args(["sh", "-c", script])
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
    fn udev_scripts_write_only_rule_lines() {
        let script = USB_RULE.script();
        assert!(script.contains("ATTRS{idVendor}==\"3633\""));
        assert!(!script.contains("# "), "comments are dropped");
        assert!(script.contains(USB_RULE.path));
        let power = POWER_RULE.script();
        assert!(power.contains("chmod 0444 /sys%p/energy_uj"));
        assert!(power.ends_with("--subsystem-match=powercap --action=add && udevadm settle"));
    }
}
