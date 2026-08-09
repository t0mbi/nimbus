@echo off
rem Opens Nimbus setup (Launch Options string, sync status). Double-click, or
rem run from a terminal. The actual %command% wrapping happens via Steam's
rem own Launch Options field, not this file - see README.md.

set NIMBUS_EXE=%~dp0target\release\nimbus.exe

if not exist "%NIMBUS_EXE%" (
    echo Building nimbus...
    where cargo >nul 2>nul
    if errorlevel 1 (
        echo cargo not found - install Rust from https://rustup.rs, then try again.
        pause
        exit /b 1
    )
    cargo build --release --manifest-path "%~dp0Cargo.toml"
    if errorlevel 1 (
        pause
        exit /b 1
    )
)

start "" "%NIMBUS_EXE%"
