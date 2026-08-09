use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Nimbus deliberately stores almost nothing. Where saves go, how many
/// versions are kept, and what format they're in all live in Ludusavi's own
/// config, editable in its GUI - nimbus doesn't duplicate or override any of
/// it. All that's kept here is what Ludusavi has no concept of: which game a
/// given executable belongs to.
#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Config {
    /// Override if `ludusavi` isn't on PATH.
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

    pub fn ludusavi_bin(&self) -> PathBuf {
        self.ludusavi_path.clone().unwrap_or_else(|| PathBuf::from("ludusavi"))
    }
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

/// Version string if the configured Ludusavi actually runs, else None.
pub fn probe_ludusavi(bin: &Path) -> Option<String> {
    let out = Command::new(bin).arg("--version").output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Reads `backup.path` out of `ludusavi config show` so the setup window can
/// show where saves are actually going. Best-effort: nimbus doesn't own this
/// setting and shouldn't fail if the format shifts.
pub fn ludusavi_backup_path(bin: &Path) -> Option<String> {
    let out = Command::new(bin).args(["config", "show"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    parse_backup_path(&String::from_utf8_lossy(&out.stdout))
}

/// Pulls `backup: / path: "..."` out of `ludusavi config show`'s YAML.
/// Deliberately a dumb top-level-key scanner rather than a real YAML parse -
/// this is a best-effort read of a config file nimbus doesn't own, and
/// shouldn't gain a parser dependency just to stay resilient to reordering.
fn parse_backup_path(text: &str) -> Option<String> {
    let mut in_backup = false;
    for line in text.lines() {
        if !line.starts_with([' ', '\t']) {
            in_backup = line.trim_end() == "backup:";
            continue;
        }
        if in_backup {
            let t = line.trim();
            if let Some(v) = t.strip_prefix("path:") {
                return Some(v.trim().trim_matches('"').to_string());
            }
        }
    }
    None
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
    full: 1
    differential: 0
  format:
    chosen: simple
restore:
  path: "C:/Users/urina/ludusavi-backup"
"#;

    #[test]
    fn finds_backup_path_under_its_own_key() {
        assert_eq!(
            parse_backup_path(SAMPLE),
            Some("C:/Users/urina/ludusavi-backup".to_string())
        );
    }

    #[test]
    fn does_not_match_restores_path() {
        // `restore.path` comes after `backup.path` in real output and must
        // not be picked up if backup's were somehow missing.
        let restore_only = r#"---
restore:
  path: "C:/should/not/match"
"#;
        assert_eq!(parse_backup_path(restore_only), None);
    }

    #[test]
    fn missing_backup_key_yields_none() {
        assert_eq!(parse_backup_path("---\nlanguage: en-US\n"), None);
    }
}
