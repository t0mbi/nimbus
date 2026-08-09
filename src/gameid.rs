use crate::config::Config;
use std::env;
use std::path::Path;

/// How to hand game identity to `ludusavi wrap`.
#[derive(Debug, Clone)]
pub enum Identity {
    /// Let ludusavi resolve the name itself from launcher context (currently
    /// just Steam's `SteamAppId`, which ludusavi reads the same way we do).
    Infer(&'static str),
    /// A ludusavi-manifest-recognized game name, resolved by us.
    Named(String),
}

/// Resolution order:
///   1. `SteamAppId` env var (Steam sets this on any process it launches,
///      including through a `%command%` wrapper) -> defer to ludusavi's own
///      `--infer steam`, which does the actual name lookup.
///   2. `NIMBUS_GAME_NAME` env var - explicit override, used for testing and
///      for anyone manually wrapping a launcher we don't auto-detect yet.
///   3. Previously-confirmed exe_path -> name mapping in config. This is what
///      the (not-yet-built) one-time confirmation prompt will populate for
///      raw shortcuts / unsupported launchers - see
///      [[future: fallback confirmation flow]].
///
/// A future `HYDRA_GAME_ID` var (if Hydra ever injects one) or
/// `--infer heroic|lutris` slot into step 1's position the same way.
pub fn resolve(exe_path: &Path, config: &Config) -> Option<Identity> {
    if env::var_os("SteamAppId").is_some() {
        return Some(Identity::Infer("steam"));
    }

    if let Ok(name) = env::var("NIMBUS_GAME_NAME") {
        return Some(Identity::Named(name));
    }

    config.name_for_exe(exe_path).map(|n| Identity::Named(n.to_string()))
}
