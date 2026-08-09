use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use std::path::Path;

pub const LUDUSAVI_URL: &str = "https://github.com/mtkennerly/ludusavi";

fn dialog(title: &str, text: &str, level: MessageLevel) -> MessageDialog {
    MessageDialog::new().set_title(title).set_description(text).set_level(level)
}

/// One-time prompt the first time an unrecognized executable is launched.
/// Returns the confirmed game name, or None to skip syncing this session.
pub fn confirm_game(exe: &Path, guess: Option<&str>) -> Confirmation {
    let shown = exe
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| exe.display().to_string());

    match guess {
        Some(name) => {
            let yes = dialog(
                "Nimbus - is this the right game?",
                &format!(
                    "Nimbus doesn't recognize this game yet:\n{shown}\n\n\
                     Best guess: {name}\n\n\
                     Sync saves for it under that name? Nimbus will remember your \
                     answer and won't ask again.",
                ),
                MessageLevel::Warning,
            )
            .set_buttons(MessageButtons::YesNo)
            .show();

            if yes == MessageDialogResult::Yes {
                Confirmation::Named(name.to_string())
            } else {
                Confirmation::Skip
            }
        }
        None => {
            let stop_asking = dialog(
                "Nimbus - unrecognized game",
                &format!(
                    "Nimbus couldn't work out which game this is:\n{shown}\n\n\
                     It will start normally, just without save syncing.\n\n\
                     Stop asking about this program?",
                ),
                MessageLevel::Warning,
            )
            .set_buttons(MessageButtons::YesNo)
            .show();

            if stop_asking == MessageDialogResult::Yes {
                Confirmation::Ignore
            } else {
                Confirmation::Skip
            }
        }
    }
}

pub enum Confirmation {
    Named(String),
    /// Skip syncing this time, but ask again next launch.
    Skip,
    /// Never ask about this executable again.
    Ignore,
}

/// Rough check for "this path is on this machine, not a share". Deliberately
/// conservative - only used to soften a hint, never to block anything.
pub fn looks_local(path: &str) -> bool {
    let p = path.replace('\\', "/");
    if p.starts_with("//") {
        return false; // UNC share
    }
    if cfg!(windows) {
        p.to_uppercase().starts_with("C:/")
    } else {
        p.starts_with("/home/") || p.starts_with("/root/")
    }
}

pub fn open(url: &str) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn()?;
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unc_share_is_not_local() {
        assert!(!looks_local("//nas/gamesaves"));
        assert!(!looks_local(r"\\nas\gamesaves"));
    }

    #[cfg(windows)]
    #[test]
    fn c_drive_is_local() {
        assert!(looks_local(r"C:\Users\urina\ludusavi-backup"));
        assert!(looks_local("C:/Users/urina/ludusavi-backup"));
    }

    #[cfg(windows)]
    #[test]
    fn other_drive_letter_is_not_flagged_local() {
        // A mapped network drive (e.g. Z:) looks like a plain drive letter to
        // us - we can't tell it's remote without asking the OS, so this is a
        // known blind spot, not a bug: we only warn on the common "still
        // pointed at the C: default" case.
        assert!(!looks_local(r"Z:\gamesaves"));
    }
}
