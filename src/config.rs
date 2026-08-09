use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const DEFAULT_FULL_LIMIT: u8 = 5;

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Config {
    /// Where ludusavi reads/writes backups - the self-hosted "cloud" destination
    /// (e.g. a mounted Unraid share). Passed as `ludusavi wrap --path`.
    pub remote_root: Option<PathBuf>,
    /// How many historical zip versions ludusavi retains per game. This is our
    /// "never silently destroy data" guarantee - see DEFAULT_FULL_LIMIT.
    pub full_limit: Option<u8>,
    /// Override if `ludusavi` isn't on PATH.
    pub ludusavi_path: Option<PathBuf>,
    /// exe path (as seen on this machine) -> confirmed ludusavi game name, for
    /// launchers that don't expose an identifying env var (SteamAppId, etc).
    #[serde(default)]
    pub exe_names: HashMap<String, String>,
}

fn config_dir() -> io::Result<PathBuf> {
    let base = dirs::config_dir().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "could not determine OS config directory")
    })?;
    Ok(base.join("savewrap"))
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
        let path = config_file()?;
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(path, data)
    }

    pub fn name_for_exe(&self, exe_path: &Path) -> Option<&str> {
        let key = exe_path.to_string_lossy().to_string();
        self.exe_names.get(&key).map(|s| s.as_str())
    }

    pub fn ludusavi_bin(&self) -> PathBuf {
        self.ludusavi_path.clone().unwrap_or_else(|| PathBuf::from("ludusavi"))
    }
}
