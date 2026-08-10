// No console window on release Windows builds - launch-time output goes to
// the log file instead (see `log.rs`), which is the only thing that makes
// sense for a process Steam starts silently.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod daemon;
mod gameid;
mod gui;
mod log;
mod ludusavi_ctl;
mod toast;
mod pathset;
mod startup;
mod tray;
mod ui;
mod wrap;

use std::env;
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A windows-subsystem binary starts with no console and invalid stdio
/// handles - by design, so double-clicking or Steam launching it never
/// flashes a window. But that also silently swallows `println!` when someone
/// runs it directly from an existing terminal, which is a real papercut for
/// `--version`/`--help`. This is the standard fix: if a console the process
/// could attach to already exists (i.e. it was actually run from a shell),
/// attach to it and point stdout/stderr at it. If there's no such console
/// (Steam, Explorer double-click), this is a harmless no-op and everything
/// stays silent exactly as before.
#[cfg(windows)]
fn attach_parent_console_if_any() {
    use std::ptr::null;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Console::{
        AttachConsole, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
    };

    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }

        let name: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
        let handle = CreateFileW(
            name.as_ptr(),
            0xC000_0000, // GENERIC_READ | GENERIC_WRITE
            FILE_SHARE_WRITE | FILE_SHARE_READ,
            null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        if handle != INVALID_HANDLE_VALUE {
            SetStdHandle(STD_OUTPUT_HANDLE, handle);
            SetStdHandle(STD_ERROR_HANDLE, handle);
        }
    }
}

#[cfg(not(windows))]
fn attach_parent_console_if_any() {}

fn main() -> ExitCode {
    attach_parent_console_if_any();

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
        Some("--tray") => {
            daemon::run();
            ExitCode::SUCCESS
        }
        Some(_) => wrap::run(&args[1..]),
    }
}

fn print_help() {
    println!(
        r#"nimbus {VERSION} - self-hosted cloud saves, built on ludusavi

    nimbus                     Open settings (sync folder, games, Launch Options)
    nimbus --tray              Run the background sync daemon (tray icon, no window).
                               Usually started automatically at login - see Settings.
    nimbus <command> [args]    Restore, run command (blocking), back up. Optional -
                               for games launched some other way than Steam, the
                               background daemon covers this without any setup.
                               For Steam-owned games, this can still go in Launch
                               Options: nimbus %command%
    nimbus --version
    nimbus --help
"#
    );
}
