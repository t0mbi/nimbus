use crate::config::{self, Config};
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use std::path::Path;

const LUDUSAVI_URL: &str = "https://github.com/mtkennerly/ludusavi";

fn dialog(title: &str, text: &str, level: MessageLevel) -> MessageDialog {
    MessageDialog::new().set_title(title).set_description(text).set_level(level)
}

fn copy(text: &str) -> bool {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text.to_string()))
        .is_ok()
}

/// Shown when nimbus is run with no arguments - i.e. double-clicked.
pub fn setup() {
    let config = Config::load().unwrap_or_default();
    let bin = config.ludusavi_bin();

    let Some(version) = config::probe_ludusavi(&bin) else {
        let choice = dialog(
            "Ludusavi not found",
            &format!(
                "Nimbus needs Ludusavi installed to find and move your save files, \
                 and couldn't run it.\n\n\
                 Looked for: {}\n\n\
                 Install Ludusavi, then run Nimbus again.",
                bin.display()
            ),
            MessageLevel::Error,
        )
        .set_buttons(MessageButtons::OkCancelCustom(
            "Open download page".into(),
            "Close".into(),
        ))
        .show();

        if choice == MessageDialogResult::Custom("Open download page".into()) {
            let _ = open(LUDUSAVI_URL);
        }
        return;
    };

    let launch_options = config::launch_options_string();
    let copied = copy(&launch_options);

    let destination = config::ludusavi_backup_path(&bin);
    let destination_line = match &destination {
        Some(p) => format!("Saves currently sync to:\n{p}"),
        None => "Save destination: could not read it from Ludusavi.".into(),
    };

    let warning = match &destination {
        Some(p) if looks_local(p) => {
            "\n\nHeads up: that looks like a folder on this PC, not a network \
             share. Saves won't reach your other machines until you point it at \
             a shared folder."
        }
        _ => "",
    };

    let body = format!(
        "Found {version}.\n\n\
         {destination_line}{warning}\n\n\
         ── To sync a game ──\n\
         In Steam, right-click the game → Properties → General → Launch Options, \
         and paste this in:\n\n\
         {launch_options}\n\n\
         {}\n\n\
         Repeat on each PC, with every machine pointed at the same shared folder.\n\n\
         Change where saves go, how many versions are kept, or which folders are \
         scanned in Ludusavi's own settings.",
        if copied {
            "(already copied to your clipboard)"
        } else {
            "(copy the line above)"
        }
    );

    let open_ludusavi = dialog("Nimbus setup", &body, MessageLevel::Info)
        .set_buttons(MessageButtons::OkCancelCustom(
            "Open Ludusavi settings".into(),
            "Done".into(),
        ))
        .show();

    if open_ludusavi == MessageDialogResult::Custom("Open Ludusavi settings".into()) {
        // Detached: Ludusavi's GUI should outlive this process.
        let _ = std::process::Command::new(&bin).arg("gui").spawn();
    }
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
fn looks_local(path: &str) -> bool {
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

fn open(url: &str) -> std::io::Result<()> {
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
