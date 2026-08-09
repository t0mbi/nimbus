//! Adds Nimbus's own folder to the user's PATH, so Launch Options can just
//! say `nimbus %command%` instead of a full quoted path. Only ever runs on an
//! explicit button click - this edits a persistent, user-visible setting.

use std::path::Path;

/// Computes the new `Path` value to write, or `None` if `dir` is already on
/// it (including via a trailing-slash or case difference on Windows). Pure
/// and side-effect-free on purpose, so the actual registry write in
/// [`add_to_user_path`] can stay a thin, untested wrapper around it - the
/// interesting logic (dedup, separator handling) is what's worth testing,
/// not the registry I/O itself.
fn compute_updated_path(current: &str, dir: &Path) -> Option<String> {
    let dir_str = dir.to_string_lossy().to_string();

    let already_present = current
        .split(';')
        .any(|p| Path::new(p.trim()) == dir || p.trim().eq_ignore_ascii_case(&dir_str));
    if already_present {
        return None;
    }

    Some(if current.trim().is_empty() {
        dir_str
    } else if current.trim_end().ends_with(';') {
        format!("{current}{dir_str}")
    } else {
        format!("{current};{dir_str}")
    })
}

#[cfg(windows)]
pub fn add_to_user_path(dir: &Path) -> Result<(), String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let env = hkcu
        .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
        .map_err(|e| format!("couldn't open registry Environment key: {e}"))?;

    let current: String = env.get_value("Path").unwrap_or_default();

    let Some(updated) = compute_updated_path(&current, dir) else {
        return Ok(());
    };

    env.set_value("Path", &updated).map_err(|e| format!("couldn't write PATH: {e}"))?;

    broadcast_environment_change();
    Ok(())
}

#[cfg(windows)]
fn broadcast_environment_change() {
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    let param: Vec<u16> = "Environment\0".encode_utf16().collect();
    unsafe {
        let mut result: usize = 0;
        SendMessageTimeoutW(
            HWND_BROADCAST as HWND,
            WM_SETTINGCHANGE,
            0 as WPARAM,
            param.as_ptr() as LPARAM,
            SMTO_ABORTIFHUNG,
            2000,
            &mut result as *mut usize as *mut _,
        );
        let _ = null_mut::<()>();
    }
}

#[cfg(not(windows))]
pub fn add_to_user_path(dir: &Path) -> Result<(), String> {
    // No universal "user PATH" registry equivalent on Linux - the closest
    // reliable, no-sudo option is a symlink into ~/.local/bin, which most
    // distros already put on PATH for the desktop session.
    let Some(home) = dirs::home_dir() else {
        return Err("couldn't determine home directory".into());
    };
    let local_bin = home.join(".local/bin");
    std::fs::create_dir_all(&local_bin).map_err(|e| format!("couldn't create {}: {e}", local_bin.display()))?;

    let target = dir.join("nimbus");
    let link = local_bin.join("nimbus");

    if link.exists() {
        return Ok(());
    }

    std::os::unix::fs::symlink(&target, &link)
        .map_err(|e| format!("couldn't symlink into {}: {e}", local_bin.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_to_a_nonempty_path_with_a_semicolon() {
        let dir = Path::new(r"D:\Nimbus\target\release");
        assert_eq!(
            compute_updated_path(r"C:\Windows;C:\Windows\System32", dir),
            Some(r"C:\Windows;C:\Windows\System32;D:\Nimbus\target\release".to_string())
        );
    }

    #[test]
    fn handles_an_empty_path() {
        let dir = Path::new(r"D:\Nimbus\target\release");
        assert_eq!(compute_updated_path("", dir), Some(r"D:\Nimbus\target\release".to_string()));
    }

    #[test]
    fn does_not_double_up_a_trailing_semicolon() {
        let dir = Path::new(r"D:\Nimbus\target\release");
        assert_eq!(
            compute_updated_path(r"C:\Windows;", dir),
            Some(r"C:\Windows;D:\Nimbus\target\release".to_string())
        );
    }

    #[test]
    fn is_a_no_op_when_already_present() {
        let dir = Path::new(r"D:\Nimbus\target\release");
        assert_eq!(compute_updated_path(r"C:\Windows;D:\Nimbus\target\release;C:\tools", dir), None);
    }

    #[test]
    fn dedup_is_case_insensitive_like_windows_path_matching() {
        let dir = Path::new(r"D:\Nimbus\target\release");
        assert_eq!(compute_updated_path(r"d:\nimbus\target\release", dir), None);
    }
}
