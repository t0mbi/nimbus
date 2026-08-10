//! Toast notifications - the only visible feedback for the background daemon
//! (no window on screen) and a nice-to-have for the launch-triggered path.
//! Fire-and-forget: a failure to show a notification (no notification daemon
//! present, etc.) must never affect syncing itself, so every call here only
//! ever logs on error, never propagates one.

use crate::log;
use notify_rust::Notification;

/// Windows toasts are attributed to a registered "AppUserModelID," not to
/// whatever process happened to call the API - without one, notify-rust
/// falls back to Toast::POWERSHELL_APP_ID (a real, always-registered identity
/// it borrows so toasts work at all out of the box), which is why
/// notifications showed up under "Windows PowerShell" instead of "Nimbus."
/// `register_app_identity` fixes that for real, once per machine.
const AUMID: &str = "Nimbus.SaveSync";

pub fn pulled(game: &str) {
    show(&format!("{game} updated"), "Pulled a newer save from another PC.");
}

pub fn pushed(game: &str) {
    show(&format!("{game} backed up"), "Local changes pushed to your sync folder.");
}

pub fn restore_failed(game: &str, reason: &str) {
    show(&format!("{game}: restore failed"), reason);
}

pub fn backup_failed(game: &str, reason: &str) {
    show(&format!("{game}: backup failed"), reason);
}

pub fn info(summary: &str, body: &str) {
    show(summary, body);
}

fn show(summary: &str, body: &str) {
    log::line(&format!("notify: {summary} - {body}"));
    let mut n = Notification::new();
    n.summary(summary).body(body).appname("Nimbus");
    #[cfg(windows)]
    n.app_id(AUMID);
    if let Err(e) = n.show() {
        log::line(&format!("notify: failed to show notification: {e}"));
    }
}

/// Registers `AUMID` as a real Windows application identity ("Nimbus," with
/// its own icon) and tells this process to use it, so subsequent toasts stop
/// being attributed to PowerShell. Idempotent and cheap - safe to call every
/// startup rather than trying to detect "already done."
#[cfg(windows)]
pub fn register_app_identity() {
    use windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    if let Err(e) = write_registry_identity() {
        log::line(&format!("notify: couldn't register app identity: {e}"));
    }

    let wide: Vec<u16> = AUMID.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(wide.as_ptr());
    }
}

#[cfg(not(windows))]
pub fn register_app_identity() {}

#[cfg(windows)]
fn write_registry_identity() -> Result<(), String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = format!(r"Software\Classes\AppUserModelId\{AUMID}");
    let (key, _) = hkcu.create_subkey(&path).map_err(|e| e.to_string())?;
    key.set_value("DisplayName", &"Nimbus").map_err(|e| e.to_string())?;

    if let Ok(exe) = std::env::current_exe() {
        // "<path>,0" is the standard "first icon resource in this file"
        // syntax Windows uses for icon references throughout the shell.
        let icon_uri = format!("{},0", exe.display());
        let _ = key.set_value("IconUri", &icon_uri);
    }

    Ok(())
}
