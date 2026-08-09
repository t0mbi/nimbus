use crate::config::Config;
use crate::gameid::{self, Identity};
use crate::log;
use crate::ui::{self, Confirmation};
use std::path::Path;
use std::process::{Command, ExitCode};

/// The launch-triggered path: restore, run the game and block, back up.
/// Invoked as `nimbus <command...>` from a launcher's `%command%`.
pub fn run(cmd: &[String]) -> ExitCode {
    let Some(exe) = cmd.first() else {
        log::line("no command given");
        return ExitCode::FAILURE;
    };

    let mut config = Config::load().unwrap_or_default();
    // Covers the case where the sync folder was set up in Ludusavi's own GUI
    // before Nimbus's settings window was ever opened - see
    // Config::inherit_from_ludusavi_if_unset.
    config.inherit_from_ludusavi_if_unset();

    let exe_path = Path::new(exe);
    let identity = resolve_with_fallback(&mut config, exe_path);

    match (&config.sync_path, identity) {
        (Some(_), Some(identity)) => via_ludusavi(&config, &identity, cmd),
        (None, _) => {
            log::line("no sync folder configured (open nimbus to set one) - launching without sync");
            run_raw(exe, &cmd[1..])
        }
        (_, None) => {
            log::line(&format!("no identity for '{exe}' - launching without sync"));
            run_raw(exe, &cmd[1..])
        }
    }
}

/// Wraps [`gameid::resolve`] with the one-time confirmation prompt for
/// executables it can't place on its own - env var and saved-mapping lookups
/// are silent; this is the only path that can pop a dialog.
fn resolve_with_fallback(config: &mut Config, exe: &Path) -> Option<Identity> {
    if let Some(identity) = gameid::resolve(exe, config) {
        return Some(identity);
    }

    if config.is_ignored(exe) {
        return None;
    }

    let bin = config.ludusavi_bin();
    let guess = gameid::guess_name(&bin, exe);

    match ui::confirm_game(exe, guess.as_deref()) {
        Confirmation::Named(name) => {
            config.remember(exe, &name);
            if let Err(e) = config.save() {
                log::line(&format!("failed to save game mapping: {e}"));
            }
            Some(Identity::Named(name))
        }
        Confirmation::Ignore => {
            config.ignore(exe);
            if let Err(e) = config.save() {
                log::line(&format!("failed to save ignore mapping: {e}"));
            }
            None
        }
        Confirmation::Skip => None,
    }
}

fn via_ludusavi(config: &Config, identity: &Identity, cmd: &[String]) -> ExitCode {
    // sync_path is checked by the caller before this is reached.
    let sync_path = config.sync_path.as_ref().expect("sync_path checked by caller");

    let mut args: Vec<String> = vec![
        "wrap".into(),
        "--path".into(),
        sync_path.to_string_lossy().into_owned(),
        "--format".into(),
        config.format().to_string(),
        "--full-limit".into(),
        config.full_limit().to_string(),
        "--force".into(),
    ];

    match identity {
        Identity::Infer(launcher) => {
            args.push("--infer".into());
            args.push((*launcher).into());
        }
        Identity::Named(name) => {
            args.push("--name".into());
            args.push(name.clone());
        }
    }

    args.push("--".into());
    args.extend(cmd.iter().cloned());

    let bin = config.ludusavi_bin();
    log::line(&format!("{} {}", bin.display(), args.join(" ")));

    match Command::new(&bin).args(&args).status() {
        Ok(status) => {
            log::line(&format!("session finished (ludusavi exit {:?})", status.code()));
            // `ludusavi wrap` reports its own exit code, not the game's. Fine
            // here: launcher "still running" detection is PID-based.
            ExitCode::from(status.code().unwrap_or(0) as u8)
        }
        Err(e) => {
            log::line(&format!(
                "could not run ludusavi at '{}' ({e}) - launching without sync",
                bin.display()
            ));
            run_raw(&cmd[0], &cmd[1..])
        }
    }
}

/// Launch the game untouched. Used whenever syncing can't proceed - a failed
/// lookup must never stop someone from playing.
fn run_raw(exe: &str, args: &[String]) -> ExitCode {
    match Command::new(exe).args(args).status() {
        Ok(status) => ExitCode::from(status.code().unwrap_or(0) as u8),
        Err(e) => {
            log::line(&format!("failed to launch '{exe}': {e}"));
            ExitCode::FAILURE
        }
    }
}
