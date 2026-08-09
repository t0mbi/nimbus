use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const DEFAULT_FORMAT: &str = "zip";
pub const DEFAULT_FULL_LIMIT: u8 = 5;

/// A `Command` for `bin` with a console window suppressed on Windows.
/// Ludusavi is a console-subsystem executable; spawned silently from a
/// windowed Nimbus with no console of its own to attach to, Windows would
/// otherwise pop open a new console window for it on every single call.
pub fn ludusavi_command(bin: &Path) -> Command {
    let mut cmd = Command::new(bin);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Nimbus is the single settings surface the user ever touches - this struct
/// is the one source of truth for where saves go and how they're kept.
/// Ludusavi's own config file is never written to; instead every ludusavi
/// invocation gets `--path`/`--format`/`--full-limit` passed explicitly.
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Config {
    pub sync_path: Option<PathBuf>,
    pub format: Option<String>,
    pub full_limit: Option<u8>,
    /// Override if `ludusavi` isn't on PATH and no bundled copy is found.
    pub ludusavi_path: Option<PathBuf>,
    /// exe path -> confirmed Ludusavi game name, for launches with no
    /// identifying env var. Populated by the one-time confirmation prompt.
    #[serde(default)]
    pub exe_names: HashMap<String, String>,
    /// Executables the user declined to identify. Kept so we don't re-prompt
    /// on every single launch of something that isn't a game.
    #[serde(default)]
    pub ignored_exes: Vec<String>,
}

pub fn config_dir() -> io::Result<PathBuf> {
    let base = dirs::config_dir().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "could not determine OS config directory")
    })?;
    Ok(base.join("nimbus"))
}

fn config_file() -> io::Result<PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

impl Config {
    pub fn load() -> io::Result<Config> {
        let path = config_file()?;
        match fs::read_to_string(&path) {
            Ok(data) => serde_json::from_str(&data)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e),
        }
    }

    pub fn save(&self) -> io::Result<()> {
        let dir = config_dir()?;
        fs::create_dir_all(&dir)?;
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(config_file()?, data)
    }

    pub fn name_for_exe(&self, exe: &Path) -> Option<&str> {
        self.exe_names.get(&key(exe)).map(|s| s.as_str())
    }

    pub fn is_ignored(&self, exe: &Path) -> bool {
        self.ignored_exes.contains(&key(exe))
    }

    pub fn remember(&mut self, exe: &Path, name: &str) {
        self.exe_names.insert(key(exe), name.to_string());
    }

    pub fn ignore(&mut self, exe: &Path) {
        let k = key(exe);
        if !self.ignored_exes.contains(&k) {
            self.ignored_exes.push(k);
        }
    }

    pub fn format(&self) -> &str {
        self.format.as_deref().unwrap_or(DEFAULT_FORMAT)
    }

    pub fn full_limit(&self) -> u8 {
        self.full_limit.unwrap_or(DEFAULT_FULL_LIMIT)
    }

    /// Resolution order: explicit config override, then a `ludusavi[.exe]`
    /// sitting next to nimbus's own executable (so keeping both binaries in
    /// one folder just works with nothing to configure), then whatever
    /// `PATH` resolves - see [`bundled_ludusavi`].
    pub fn ludusavi_bin(&self) -> PathBuf {
        if let Some(path) = &self.ludusavi_path {
            return path.clone();
        }
        bundled_ludusavi().unwrap_or_else(|| PathBuf::from("ludusavi"))
    }

    /// One-time convenience for a first run: if nimbus has no sync path of
    /// its own yet, borrow whatever Ludusavi already has configured (from a
    /// prior standalone install) as a starting point, rather than making the
    /// user re-type a path they already set once. Never overwrites a value
    /// nimbus already has.
    pub fn inherit_from_ludusavi_if_unset(&mut self) {
        if self.sync_path.is_some() {
            return;
        }
        let bin = self.ludusavi_bin();
        let Some(settings) = ludusavi_backup_settings(&bin) else { return };
        self.sync_path = settings.path.map(PathBuf::from);
        if self.format.is_none() {
            self.format = settings.format;
        }
        if self.full_limit.is_none() {
            self.full_limit = settings.full_limit;
        }
    }
}

fn bundled_ludusavi() -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let candidate = dir.join(format!("ludusavi{}", std::env::consts::EXE_SUFFIX));
    candidate.is_file().then_some(candidate)
}

fn key(exe: &Path) -> String {
    exe.to_string_lossy().to_string()
}

/// The exact string to paste into a game's Launch Options. Quoted because the
/// install path usually contains spaces.
pub fn launch_options_string() -> String {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "nimbus".into());
    format!("\"{exe}\" %command%")
}

/// Just the exe's own directory, quoted - what an "Add to PATH" step needs,
/// as opposed to the full launch_options_string.
pub fn install_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(|p| p.to_path_buf())
}

/// Version string if the configured Ludusavi actually runs, else None.
pub fn probe_ludusavi(bin: &Path) -> Option<String> {
    let out = ludusavi_command(bin).arg("--version").output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[derive(Default, Debug, PartialEq)]
struct InheritedBackupSettings {
    path: Option<String>,
    format: Option<String>,
    full_limit: Option<u8>,
}

/// Reads `backup.path` / `backup.format.chosen` / `backup.retention.full` out
/// of `ludusavi config show`, for the one-time inherit on first run.
/// Best-effort: if Ludusavi has never been configured, or the format shifts,
/// this just yields nothing and the user picks fresh values instead.
fn ludusavi_backup_settings(bin: &Path) -> Option<InheritedBackupSettings> {
    let out = ludusavi_command(bin).args(["config", "show"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(parse_backup_settings(&String::from_utf8_lossy(&out.stdout)))
}

fn parse_backup_settings(text: &str) -> InheritedBackupSettings {
    let leaves = scan_yaml_leaves(text);
    InheritedBackupSettings {
        path: leaves.get("backup.path").cloned(),
        format: leaves.get("backup.format.chosen").cloned(),
        full_limit: leaves.get("backup.retention.full").and_then(|v| v.parse().ok()),
    }
}

/// A deliberately dumb YAML leaf-scanner: tracks section nesting purely by
/// indentation and returns every `a.b.c -> value` leaf it sees. No lists, no
/// multi-line scalars, no anchors - Ludusavi's own config doesn't need any of
/// that for the handful of keys nimbus cares about, and a real parser
/// dependency isn't worth it for reading a file nimbus doesn't own.
fn scan_yaml_leaves(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut stack: Vec<(usize, String)> = Vec::new();

    for raw in text.lines() {
        let trimmed = raw.trim_start();
        if trimmed.is_empty() || trimmed == "---" || trimmed.starts_with(['#', '-']) {
            continue;
        }
        let Some(colon) = trimmed.find(':') else { continue };
        let indent = raw.len() - trimmed.len();
        let key = trimmed[..colon].trim().to_string();
        let rest = trimmed[colon + 1..].trim();

        while stack.last().is_some_and(|(i, _)| *i >= indent) {
            stack.pop();
        }

        if rest.is_empty() {
            stack.push((indent, key));
        } else {
            let path = stack
                .iter()
                .map(|(_, k)| k.as_str())
                .chain(std::iter::once(key.as_str()))
                .collect::<Vec<_>>()
                .join(".");
            out.insert(path, rest.trim_matches('"').to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed from a real `ludusavi config show` (v0.31.0) capture.
    const SAMPLE: &str = r#"---
runtime:
  threads: ~
release:
  check: true
manifest:
  enable: true
language: en-US
theme: light
roots:
  - store: steam
    path: "C:/Program Files (x86)/Steam"
redirects: []
backup:
  path: "C:/Users/urina/ludusavi-backup"
  ignoredGames: []
  filter:
    excludeStoreScreenshots: false
  retention:
    full: 5
    differential: 0
  format:
    chosen: zip
    zip:
      compression: deflate
restore:
  path: "C:/Users/urina/ludusavi-backup"
"#;

    #[test]
    fn reads_backup_path_format_and_retention() {
        let settings = parse_backup_settings(SAMPLE);
        assert_eq!(settings.path.as_deref(), Some("C:/Users/urina/ludusavi-backup"));
        assert_eq!(settings.format.as_deref(), Some("zip"));
        assert_eq!(settings.full_limit, Some(5));
    }

    #[test]
    fn does_not_confuse_restore_path_with_backup_path() {
        let restore_only = r#"---
restore:
  path: "C:/should/not/match"
"#;
        assert_eq!(parse_backup_settings(restore_only).path, None);
    }

    #[test]
    fn missing_backup_key_yields_defaults() {
        assert_eq!(parse_backup_settings("---\nlanguage: en-US\n"), InheritedBackupSettings::default());
    }

    #[test]
    fn sibling_sections_do_not_leak_into_each_other() {
        // format.zip.compression must not be mistaken for format.chosen, and
        // retention's "full" must not collide with anything above it.
        let leaves = scan_yaml_leaves(SAMPLE);
        assert_eq!(leaves.get("backup.format.zip.compression").map(|s| s.as_str()), Some("deflate"));
        assert_eq!(leaves.get("backup.retention.full").map(|s| s.as_str()), Some("5"));
    }
}
