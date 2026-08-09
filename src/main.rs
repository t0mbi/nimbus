mod config;
mod gameid;

use config::Config;
use gameid::Identity;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_help();
        return ExitCode::FAILURE;
    }

    match args[1].as_str() {
        "--set-remote" => cmd_set_remote(&args[2..]),
        "--set-name" => cmd_set_name(&args[2..]),
        "--set-full-limit" => cmd_set_full_limit(&args[2..]),
        "--forget-exe" => cmd_forget_exe(&args[2..]),
        "--list" => cmd_list(),
        "--version" | "-V" => {
            println!("nimbus {VERSION}");
            ExitCode::SUCCESS
        }
        "--help" | "-h" => {
            print_help();
            ExitCode::SUCCESS
        }
        _ => run_wrapped(&args[1..]),
    }
}

fn print_help() {
    println!(
        r#"nimbus {VERSION} - launch-triggered save sync, built on ludusavi

USAGE:
    nimbus <command> [args...]            Restore, run <command> (blocking), back up.
                                          This is what goes in Launch Options:
                                              nimbus %command%

    nimbus --set-remote <path>            Set the self-hosted sync destination
                                          (e.g. a mounted network share)
    nimbus --set-name <exe-path> <name>   Manually confirm the ludusavi game name
                                          for an exe (for launchers that don't
                                          set SteamAppId)
    nimbus --set-full-limit <n>           How many historical versions ludusavi
                                          retains per game (default 5)
    nimbus --forget-exe <exe-path>        Remove an exe -> name mapping
    nimbus --list                         Show current config
    nimbus --version
    nimbus --help

Requires `ludusavi` (https://github.com/mtkennerly/ludusavi) on PATH.
"#
    );
}

fn cmd_set_remote(args: &[String]) -> ExitCode {
    let Some(path) = args.first() else {
        eprintln!("usage: nimbus --set-remote <path>");
        return ExitCode::FAILURE;
    };
    let mut config = load_config_or_exit();
    config.remote_root = Some(PathBuf::from(path));
    save_config_or_exit(&config);
    println!("remote destination set to {path}");
    ExitCode::SUCCESS
}

fn cmd_set_name(args: &[String]) -> ExitCode {
    if args.len() < 2 {
        eprintln!("usage: nimbus --set-name <exe-path> <ludusavi-game-name>");
        return ExitCode::FAILURE;
    }
    let exe = args[0].clone();
    let name = args[1].clone();
    let mut config = load_config_or_exit();
    config.exe_names.insert(exe.clone(), name.clone());
    save_config_or_exit(&config);
    println!("'{exe}' -> '{name}'");
    ExitCode::SUCCESS
}

fn cmd_set_full_limit(args: &[String]) -> ExitCode {
    let Some(n) = args.first().and_then(|s| s.parse::<u8>().ok()) else {
        eprintln!("usage: nimbus --set-full-limit <1-255>");
        return ExitCode::FAILURE;
    };
    let mut config = load_config_or_exit();
    config.full_limit = Some(n);
    save_config_or_exit(&config);
    println!("full_limit set to {n}");
    ExitCode::SUCCESS
}

fn cmd_forget_exe(args: &[String]) -> ExitCode {
    let Some(exe) = args.first() else {
        eprintln!("usage: nimbus --forget-exe <exe-path>");
        return ExitCode::FAILURE;
    };
    let mut config = load_config_or_exit();
    config.exe_names.remove(exe);
    save_config_or_exit(&config);
    println!("forgot mapping for '{exe}'");
    ExitCode::SUCCESS
}

fn cmd_list() -> ExitCode {
    let config = load_config_or_exit();
    println!("remote_root: {:?}", config.remote_root);
    println!("full_limit: {}", config.full_limit.unwrap_or(config::DEFAULT_FULL_LIMIT));
    println!("ludusavi_path: {:?}", config.ludusavi_bin());
    println!("exe_names:");
    for (exe, name) in &config.exe_names {
        println!("  {exe} -> {name}");
    }
    ExitCode::SUCCESS
}

fn run_wrapped(cmd: &[String]) -> ExitCode {
    let Some(exe) = cmd.first() else {
        eprintln!("nimbus: no command given");
        return ExitCode::FAILURE;
    };

    let config = load_config_or_exit();
    let exe_path = Path::new(exe);
    let identity = gameid::resolve(exe_path, &config);

    match (&config.remote_root, &identity) {
        (Some(remote_root), Some(identity)) => {
            run_via_ludusavi(&config, remote_root, identity, cmd)
        }
        (None, _) => {
            eprintln!("nimbus: no remote configured (run --set-remote) - launching without sync");
            run_raw(exe, &cmd[1..])
        }
        (_, None) => {
            eprintln!(
                "nimbus: game not recognized (no SteamAppId, and no saved mapping for \
                 '{exe}') - launching without sync. Fix with: nimbus --set-name \"{exe}\" \
                 \"<Ludusavi Game Name>\""
            );
            run_raw(exe, &cmd[1..])
        }
    }
}

fn run_via_ludusavi(
    config: &Config,
    remote_root: &Path,
    identity: &Identity,
    cmd: &[String],
) -> ExitCode {
    let full_limit = config.full_limit.unwrap_or(config::DEFAULT_FULL_LIMIT);

    let mut ludu_args: Vec<String> = vec![
        "wrap".into(),
        "--path".into(),
        remote_root.to_string_lossy().into_owned(),
        "--format".into(),
        "zip".into(),
        "--full-limit".into(),
        full_limit.to_string(),
        "--force".into(),
    ];

    match identity {
        Identity::Infer(launcher) => {
            ludu_args.push("--infer".into());
            ludu_args.push((*launcher).into());
        }
        Identity::Named(name) => {
            ludu_args.push("--name".into());
            ludu_args.push(name.clone());
        }
    }

    ludu_args.push("--".into());
    ludu_args.extend(cmd.iter().cloned());

    eprintln!("nimbus: {} {}", config.ludusavi_bin().display(), ludu_args.join(" "));

    match Command::new(config.ludusavi_bin()).args(&ludu_args).status() {
        Ok(status) => {
            // Note: `ludusavi wrap` does not forward the wrapped game's own exit
            // code - it reports its own. Acceptable since launcher "still
            // running" detection is PID-based, not exit-code-based.
            ExitCode::from(status.code().unwrap_or(0) as u8)
        }
        Err(e) => {
            eprintln!(
                "nimbus: failed to launch ludusavi ({e}) - is it installed and on PATH? \
                 Falling back to launching the game without sync."
            );
            run_raw(&cmd[0], &cmd[1..])
        }
    }
}

fn run_raw(exe: &str, args: &[String]) -> ExitCode {
    match Command::new(exe).args(args).status() {
        Ok(status) => ExitCode::from(status.code().unwrap_or(0) as u8),
        Err(e) => {
            eprintln!("nimbus: failed to launch '{exe}': {e}");
            ExitCode::FAILURE
        }
    }
}

fn load_config_or_exit() -> Config {
    match Config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("nimbus: failed to load config: {e}");
            std::process::exit(1);
        }
    }
}

fn save_config_or_exit(config: &Config) {
    if let Err(e) = config.save() {
        eprintln!("nimbus: failed to save config: {e}");
        std::process::exit(1);
    }
}
