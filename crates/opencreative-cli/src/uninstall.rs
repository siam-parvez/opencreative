//! `opencreative uninstall`: removes a per-user install (tarball, AppImage or
//! `cargo install`) and everything the app set up: the background service,
//! the app-menu entry, the udev rules and, with `--purge`, the settings.
//!
//! Package-manager installs are left to the package manager.

use std::env;
use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Result, bail};

const UNIT: &str = "opencreative";
const SYSTEM_FILES: [&str; 2] = [
    "/etc/udev/rules.d/60-opencreative.rules",
    "/etc/udev/rules.d/61-opencreative-power.rules",
];

/// What an uninstall would remove.
#[derive(Debug, Default, PartialEq)]
struct Plan {
    /// Per-user files and folders.
    user: Vec<PathBuf>,
    /// Root-owned files, removed through polkit.
    system: Vec<PathBuf>,
}

pub fn run(purge: bool, yes: bool) -> Result<()> {
    let exe = env::current_exe()?.canonicalize()?;
    if packaged(&exe) {
        bail!(
            "OpenCreative was installed by your package manager. Remove it from your \
             software center, or with `sudo apt remove opencreative` (Debian, Ubuntu) or \
             `sudo dnf remove opencreative` (Fedora)."
        );
    }
    let plan = plan(&exe, env::var_os("APPIMAGE").map(PathBuf::from), purge);
    println!("This removes:");
    for path in plan.user.iter().chain(&plan.system) {
        println!("  {}", path.display());
    }
    println!("  and stops the background service.");
    if !purge {
        println!("Your settings are kept (use --purge to remove them too).");
    }
    if !yes && !confirm()? {
        println!("Nothing removed.");
        return Ok(());
    }

    // Stop the service before deleting the binary it runs.
    let _ = systemctl(&["disable", "--now", UNIT]);
    let mut failed = Vec::new();
    for path in &plan.user {
        let removed = if path.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        };
        match removed {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => failed.push(format!("{}: {e}", path.display())),
        }
    }
    let _ = systemctl(&["daemon-reload"]);
    if !plan.system.is_empty() && !remove_as_root(&plan.system) {
        let files: Vec<_> = plan
            .system
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        failed.push(format!(
            "couldn't remove the udev rules; run: sudo rm {} && sudo udevadm control --reload",
            files.join(" ")
        ));
    }
    if failed.is_empty() {
        println!("OpenCreative is uninstalled.");
        Ok(())
    } else {
        bail!("some files were left behind:\n  {}", failed.join("\n  "))
    }
}

/// Installed under `/usr` (but not `/usr/local`) means a package owns it.
fn packaged(exe: &Path) -> bool {
    exe.starts_with("/usr") && !exe.starts_with("/usr/local")
}

fn plan(exe: &Path, appimage: Option<PathBuf>, purge: bool) -> Plan {
    let mut user = Vec::new();
    if let Some(config) = dirs::config_dir() {
        user.push(config.join("systemd/user").join(format!("{UNIT}.service")));
        if purge {
            user.push(config.join("opencreative"));
        }
    }
    if let Some(data) = dirs::data_dir() {
        user.push(data.join("applications/opencreative.desktop"));
        user.push(data.join("icons/hicolor/256x256/apps/opencreative.png"));
    }
    if let Some(dir) = dirs::runtime_dir().or_else(dirs::cache_dir) {
        user.push(dir.join("opencreative"));
    }
    // The program itself: the AppImage, or the binaries side by side.
    match appimage {
        Some(image) => user.push(image),
        None => {
            if let Some(dir) = exe.parent() {
                for name in ["opencreative", "opencreative-gui"] {
                    let bin = dir.join(name);
                    if bin.exists() {
                        user.push(bin);
                    }
                }
            }
        }
    }
    let system = SYSTEM_FILES
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect();
    Plan { user, system }
}

fn confirm() -> Result<bool> {
    if !io::stdin().is_terminal() {
        bail!("pass --yes to uninstall without a prompt");
    }
    print!("Uninstall OpenCreative? [y/N] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}

fn systemctl(args: &[&str]) -> io::Result<()> {
    Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .map(|_| ())
}

/// Removes root-owned files through polkit's password prompt.
fn remove_as_root(files: &[PathBuf]) -> bool {
    let list: Vec<String> = files.iter().map(|p| format!("'{}'", p.display())).collect();
    let script = format!("rm -f {} && udevadm control --reload", list.join(" "));
    Command::new("pkexec")
        .args(["sh", "-c", &script])
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_installs_are_left_to_the_package_manager() {
        assert!(packaged(Path::new("/usr/bin/opencreative")));
        assert!(!packaged(Path::new("/usr/local/bin/opencreative")));
        assert!(!packaged(Path::new("/home/me/.local/bin/opencreative")));
    }

    #[test]
    fn plans_the_appimage_or_the_binaries_and_keeps_settings_unless_purging() {
        let image = PathBuf::from("/home/me/Apps/OpenCreative.AppImage");
        let p = plan(
            Path::new("/tmp/.mount_x/usr/bin/opencreative"),
            Some(image.clone()),
            false,
        );
        assert!(p.user.contains(&image));
        assert!(!p.user.iter().any(|f| f.starts_with("/tmp/.mount_x")));
        let settings = dirs::config_dir().unwrap().join("opencreative");
        assert!(!p.user.contains(&settings));
        let purged = plan(Path::new("/nonexistent/opencreative"), None, true);
        assert!(purged.user.contains(&settings));
    }
}
