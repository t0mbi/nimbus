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

rem Nimbus looks for ludusavi.exe next to its own executable before falling
rem back to PATH. If you dropped a copy in this folder (next to run.bat),
rem mirror it alongside the built binary so that lookup finds it.
if exist "%~dp0ludusavi.exe" if not exist "%~dp0target\release\ludusavi.exe" (
    copy /y "%~dp0ludusavi.exe" "%~dp0target\release\ludusavi.exe" >nul
)

start "" "%NIMBUS_EXE%"
