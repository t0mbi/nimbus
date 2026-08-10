use crate::config::ludusavi_command;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct GameStatus {
    pub name: String,
    pub bytes: u64,
    /// Local save file paths for this game - what the background watcher
    /// actually watches for changes.
    pub paths: Vec<PathBuf>,
}

#[derive(Deserialize)]
struct PreviewOutput {
    #[serde(default)]
    games: HashMap<String, PreviewGame>,
}

#[derive(Deserialize)]
struct PreviewGame {
    #[serde(default)]
    files: HashMap<String, PreviewFile>,
}

#[derive(Deserialize, Default)]
struct PreviewFile {
    #[serde(default)]
    bytes: u64,
}

/// Every game Ludusavi finds local save data for, on this machine. This scans
/// the whole library (can take upwards of 20s for a large collection) - the
/// caller is responsible for running it off the UI thread.
pub fn list_games(bin: &Path) -> Result<Vec<GameStatus>, String> {
    let out = ludusavi_command(bin)
        .args(["backup", "--preview", "--api"])
        .output()
        .map_err(|e| format!("couldn't run ludusavi: {e}"))?;

    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }

    let parsed: PreviewOutput =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("unexpected ludusavi output: {e}"))?;

    let mut games: Vec<GameStatus> = parsed
        .games
        .into_iter()
        .map(|(name, g)| {
            let bytes = g.files.values().map(|f| f.bytes).sum();
            let paths = g.files.keys().map(PathBuf::from).collect();
            GameStatus { name, bytes, paths }
        })
        .collect();
    games.sort_by_key(|g| g.name.to_lowercase());
    Ok(games)
}

/// Manual "Push" / the background watcher's push-on-change: backs up one
/// game's current local saves to `sync_path`, using Nimbus's own
/// format/retention settings (Ludusavi's config is never touched).
pub fn backup(bin: &Path, sync_path: &Path, format: &str, full_limit: u8, game: &str) -> Result<(), String> {
    run(
        bin,
        [
            "backup",
            "--api",
            "--force",
            "--path",
        ],
        sync_path,
        Some(("--format", format)),
        Some(full_limit),
        game,
    )
}

/// Manual "Pull" / the background watcher's pull-when-remote-is-newer:
/// restores one game's saves from `sync_path` into its local save location.
pub fn restore(bin: &Path, sync_path: &Path, game: &str) -> Result<(), String> {
    run(bin, ["restore", "--api", "--force", "--path"], sync_path, None, None, game)
}

fn run(
    bin: &Path,
    leading_args: [&str; 4],
    sync_path: &Path,
    format: Option<(&str, &str)>,
    full_limit: Option<u8>,
    game: &str,
) -> Result<(), String> {
    let mut cmd = ludusavi_command(bin);
    cmd.args(leading_args).arg(sync_path);
    if let Some((flag, value)) = format {
        cmd.args([flag, value]);
    }
    if let Some(limit) = full_limit {
        cmd.args(["--full-limit", &limit.to_string()]);
    }
    cmd.arg(game);

    let out = cmd.output().map_err(|e| format!("couldn't run ludusavi: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let msg = if !stderr.trim().is_empty() { stderr } else { stdout };
        Err(msg.trim().to_string())
    }
}

#[derive(Deserialize)]
struct BackupsOutput {
    #[serde(default)]
    games: HashMap<String, BackupsGame>,
}

#[derive(Deserialize)]
struct BackupsGame {
    #[serde(default)]
    backups: Vec<BackupEntry>,
}

#[derive(Deserialize)]
struct BackupEntry {
    /// RFC3339, e.g. "2026-08-10T01:08:01.053222200Z" - compared as plain
    /// strings rather than parsed, since UTC RFC3339 timestamps sort
    /// correctly lexicographically. Not worth a datetime dependency for that.
    when: String,
}

/// The most recent remote backup's timestamp for one game, or `None` if it
/// has no backups there yet. Used by the background poll loop to decide
/// whether the remote side has something newer than what's local.
pub fn latest_remote_backup(bin: &Path, sync_path: &Path, game: &str) -> Result<Option<String>, String> {
    let out = ludusavi_command(bin)
        .args(["backups", "--api", "--path"])
        .arg(sync_path)
        .arg(game)
        .output()
        .map_err(|e| format!("couldn't run ludusavi: {e}"))?;

    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }

    let parsed: BackupsOutput =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("unexpected ludusavi output: {e}"))?;

    Ok(parsed
        .games
        .get(game)
        .and_then(|g| g.backups.iter().map(|b| b.when.clone()).max()))
}
