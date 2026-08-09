// No console window on release Windows builds - launch-time output goes to
// the log file instead (see `log.rs`), which is the only thing that makes
// sense for a process Steam starts silently.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod gameid;
mod gui;
mod log;
mod ludusavi_ctl;
mod pathset;
mod ui;
mod wrap;

use std::env;
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        None => {
            // Double-clicked, or run bare: open the settings window.
            gui::run();
            ExitCode::SUCCESS
        }
        Some("--version" | "-V") => {
            println!("nimbus {VERSION}");
            ExitCode::SUCCESS
        }
        Some("--help" | "-h") => {
            print_help();
            ExitCode::SUCCESS
        }
        Some(_) => wrap::run(&args[1..]),
    }
}

fn print_help() {
    println!(
        r#"nimbus {VERSION} - self-hosted cloud saves, built on ludusavi

    nimbus                     Open settings (sync folder, games, Launch Options)
    nimbus <command> [args]    Restore, run command (blocking), back up.
                               This is what goes in Launch Options:
                                   nimbus %command%
    nimbus --version
    nimbus --help
"#
    );
}
