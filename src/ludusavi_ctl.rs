use crate::config::ludusavi_command;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Guards the local-vs-remote comparison against flip-flopping on
/// timestamps that are "equal enough" (filesystem mtime resolution,
/// clock skew) rather than a genuine change on either side - same purpose
/// and magnitude as the Python daemon's TOLERANCE_SECONDS.
pub const TOLERANCE_SECONDS: f64 = 3.0;

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

/// Same data as [`latest_remote_backup`], but for every game at `sync_path`
/// in a single call. Measured cost: ludusavi loads/parses its full manifest
/// on almost any real invocation (~850ms on this machine, regardless of
/// subcommand or how many games are asked about - `--version` alone is
/// ~80ms). That means one call per game in a poll loop doesn't scale: 71
/// games x ~850ms is over a minute, longer than the poll interval itself.
/// Checking everything in one call costs the same ~850ms as checking one
/// game, since the manifest load dominates - so the poll loop should always
/// use this, never the single-game version, in a loop over many games.
pub fn latest_remote_backups_all(bin: &Path, sync_path: &Path) -> Result<HashMap<String, String>, String> {
    let out = ludusavi_command(bin)
        .args(["backups", "--api", "--path"])
        .arg(sync_path)
        .output()
        .map_err(|e| format!("couldn't run ludusavi: {e}"))?;

    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }

    let parsed: BackupsOutput =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("unexpected ludusavi output: {e}"))?;

    Ok(parsed
        .games
        .into_iter()
        .filter_map(|(name, g)| g.backups.into_iter().map(|b| b.when).max().map(|when| (name, when)))
        .collect())
}

/// Latest mtime among a game's local save files, as seconds since the Unix
/// epoch, or `None` if none of them exist locally.
pub fn latest_local_mtime_epoch(paths: &[PathBuf]) -> Option<f64> {
    paths
        .iter()
        .filter_map(|p| p.metadata().ok()?.modified().ok())
        .map(|t| t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64())
        .fold(None, |acc, v| Some(acc.map_or(v, |a: f64| a.max(v))))
}

/// True only when `remote` is genuinely newer than `local_epoch` by more
/// than the tolerance window - i.e. safe to pull without risking overwriting
/// something that's actually more current locally. Silently returns `false`
/// (never pull) if `remote` doesn't parse, rather than guessing.
///
/// This exists because of a real incident: the first time the daemon ever
/// ran against a share that already had backups from prior manual Ludusavi
/// use, the poll loop pulled dozens of games it had no reason to touch,
/// because it only compared "is this different from what I've already
/// recorded" (nothing, on a first run) rather than ever checking whether the
/// remote was actually newer than what's on disk. Most of those pulls were
/// harmless (the local files genuinely hadn't changed since that remote
/// snapshot), but the check needed to exist regardless - it's the only thing
/// standing between "remote has some backup" and "overwrite local data that
/// might be more current."
pub fn remote_is_newer(remote: &str, local_epoch: Option<f64>) -> bool {
    let Some(remote_epoch) = parse_rfc3339_to_epoch(remote) else { return false };
    match local_epoch {
        None => true, // nothing local to lose
        Some(local) => remote_epoch > local + TOLERANCE_SECONDS,
    }
}

/// Parses `2026-08-10T02:05:42.123456789Z`-style RFC3339 (what Ludusavi's
/// `when` field uses) into seconds since the Unix epoch. Hand-rolled rather
/// than a datetime dependency, mirroring `log.rs`'s `civil_from_days` - this
/// is its inverse (civil date -> day count) used for parsing instead of
/// formatting.
fn parse_rfc3339_to_epoch(s: &str) -> Option<f64> {
    let s = s.trim().strip_suffix('Z')?;
    let (date, time) = s.split_once('T')?;

    let mut date_parts = date.split('-');
    let y: i64 = date_parts.next()?.parse().ok()?;
    let mo: u32 = date_parts.next()?.parse().ok()?;
    let d: u32 = date_parts.next()?.parse().ok()?;

    let (time, frac) = time.split_once('.').unwrap_or((time, ""));
    let mut time_parts = time.split(':');
    let h: i64 = time_parts.next()?.parse().ok()?;
    let mi: i64 = time_parts.next()?.parse().ok()?;
    let se: i64 = time_parts.next()?.parse().ok()?;

    let frac_secs = if frac.is_empty() {
        0.0
    } else {
        let mut digits = frac.to_string();
        digits.truncate(9);
        while digits.len() < 9 {
            digits.push('0');
        }
        digits.parse::<i64>().ok()? as f64 / 1_000_000_000.0
    };

    let days = days_from_civil(y, mo, d);
    Some((days * 86_400 + h * 3600 + mi * 60 + se) as f64 + frac_secs)
}

/// Howard Hinnant's days-from-civil.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy as u64;
    era * 146_097 + doe as i64 - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_real_ludusavi_timestamp() {
        // Real capture from `ludusavi backups --api`, cross-checked against
        // Python's datetime.fromisoformat for the same string: 1786324081.053222.
        let epoch = parse_rfc3339_to_epoch("2026-08-10T01:08:01.053222200Z").unwrap();
        assert!((epoch - 1786324081.053222).abs() < 0.001);
    }

    #[test]
    fn parses_a_timestamp_with_no_fractional_seconds() {
        let epoch = parse_rfc3339_to_epoch("2026-08-10T01:08:01Z").unwrap();
        assert_eq!(epoch as i64, 1786324081);
    }

    #[test]
    fn later_timestamp_parses_as_later() {
        let a = parse_rfc3339_to_epoch("2026-08-10T01:08:01.053222200Z").unwrap();
        let b = parse_rfc3339_to_epoch("2026-08-10T01:08:02.5Z").unwrap();
        assert!(b > a);
    }

    #[test]
    fn remote_never_wins_against_a_genuinely_newer_local_file() {
        // The actual bug: a remote timestamp from a stale bulk backup must
        // never be judged "newer" than a local file that's more recent.
        let old_remote = "2020-01-01T00:00:00Z";
        let recent_local_epoch = parse_rfc3339_to_epoch("2026-08-10T00:00:00Z").unwrap();
        assert!(!remote_is_newer(old_remote, Some(recent_local_epoch)));
    }

    #[test]
    fn remote_wins_when_genuinely_newer_than_local() {
        let new_remote = "2099-01-01T00:00:00Z";
        let old_local_epoch = parse_rfc3339_to_epoch("2020-01-01T00:00:00Z").unwrap();
        assert!(remote_is_newer(new_remote, Some(old_local_epoch)));
    }

    #[test]
    fn remote_wins_when_there_is_no_local_file_at_all() {
        assert!(remote_is_newer("2020-01-01T00:00:00Z", None));
    }

    #[test]
    fn within_tolerance_is_not_considered_newer() {
        let base = "2026-08-10T00:00:00Z";
        let local_epoch = parse_rfc3339_to_epoch("2026-08-10T00:00:01Z").unwrap(); // 1s later
        assert!(!remote_is_newer(base, Some(local_epoch)));
    }
}
