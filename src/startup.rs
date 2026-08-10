//! Registers (or removes) Nimbus's background daemon to launch at login.
//! Deliberately not a Windows Service or systemd unit - both need elevated
//! setup, and a plain per-user autostart entry needs none.

use std::path::PathBuf;

#[cfg(windows)]
fn entry_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs\Startup\nimbus-tray.bat"))
}

#[cfg(not(windows))]
fn entry_path() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join(".config/autostart/nimbus-tray.desktop"))
}

pub fn is_enabled() -> bool {
    entry_path().is_some_and(|p| p.exists())
}

#[cfg(windows)]
pub fn enable() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let path = entry_path().ok_or("couldn't determine Startup folder")?;
    let content = format!("@echo off\r\nstart \"\" \"{}\" --tray\r\n", exe.display());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

#[cfg(not(windows))]
pub fn enable() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let path = entry_path().ok_or("couldn't determine autostart directory")?;
    let content = format!(
        "[Desktop Entry]\nType=Application\nName=Nimbus (background sync)\nExec=\"{}\" --tray\nX-GNOME-Autostart-enabled=true\n",
        exe.display()
    );
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

pub fn disable() -> Result<(), String> {
    let Some(path) = entry_path() else { return Ok(()) };
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Launches the daemon right now, detached, so enabling background sync
/// takes effect immediately rather than only at next login.
pub fn launch_now() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new(exe).arg("--tray").spawn().map_err(|e| e.to_string())?;
    Ok(())
}
