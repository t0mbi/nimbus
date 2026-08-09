use crate::config::{ludusavi_command, Config};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::path::Path;

/// How to hand game identity to `ludusavi wrap`.
#[derive(Debug, Clone)]
pub enum Identity {
    /// Let Ludusavi resolve the name from launcher context (Steam's
    /// `SteamAppId`, which it reads the same way we would).
    Infer(&'static str),
    /// A manifest-recognized game name we resolved ourselves.
    Named(String),
}

/// Resolution order, cheapest and most reliable first:
///   1. `SteamAppId` - Steam sets this on everything it launches, including
///      through a `%command%` wrapper. Zero configuration.
///   2. `NIMBUS_GAME_NAME` - explicit override, for testing or for wrapping a
///      launcher nimbus can't auto-detect.
///   3. A previously-confirmed exe -> name mapping.
///
/// Returns None when the game is unknown; the caller decides whether to prompt.
/// A future `HYDRA_GAME_ID`, or Ludusavi's `--infer heroic|lutris`, slots in
/// alongside step 1.
pub fn resolve(exe: &Path, config: &Config) -> Option<Identity> {
    if env::var_os("SteamAppId").is_some() {
        return Some(Identity::Infer("steam"));
    }
    if let Ok(name) = env::var("NIMBUS_GAME_NAME") {
        return Some(Identity::Named(name));
    }
    config.name_for_exe(exe).map(|n| Identity::Named(n.to_string()))
}

#[derive(Deserialize)]
struct FindOutput {
    #[serde(default)]
    games: HashMap<String, FindMatch>,
}

#[derive(Deserialize)]
struct FindMatch {
    #[serde(default)]
    score: f64,
}

/// Best-guess game name for an unrecognized executable, via `ludusavi find`.
///
/// The executable's own stem is often meaningless (`launcher.exe`, `game.exe`,
/// `bin/x64/shipping.exe`), so the parent directory name is usually the better
/// signal - both are tried, best score wins.
pub fn guess_name(bin: &Path, exe: &Path) -> Option<String> {
    let mut candidates: Vec<String> = Vec::new();

    if let Some(dir) = exe.parent().and_then(|p| p.file_name()) {
        candidates.push(dir.to_string_lossy().to_string());
    }
    if let Some(stem) = exe.file_stem() {
        candidates.push(stem.to_string_lossy().to_string());
    }

    let mut best: Option<(String, f64)> = None;

    for candidate in candidates {
        let cleaned = clean(&candidate);
        if cleaned.is_empty() {
            continue;
        }

        let out = ludusavi_command(bin)
            .args(["find", "--fuzzy", "--normalized", "--api", &cleaned])
            .output()
            .ok()?;

        let Ok(parsed) = serde_json::from_slice::<FindOutput>(&out.stdout) else {
            continue;
        };

        for (name, m) in parsed.games {
            if best.as_ref().map(|(_, s)| m.score > *s).unwrap_or(true) {
                best = Some((name, m.score));
            }
        }
    }

    // Below this, matches are usually noise - better to ask about nothing than
    // to confidently suggest the wrong game.
    best.filter(|(_, score)| *score >= 0.6).map(|(name, _)| name)
}

/// Strips the packaging noise that shows up in directory names so the fuzzy
/// matcher sees something closer to an actual title.
fn clean(raw: &str) -> String {
    let mut s = raw.replace(['_', '.', '-'], " ");
    for junk in [
        "repack", "codex", "plaza", "fitgirl", "dodi", "skidrow", "razor1911", "empress",
        "goldberg", "gog", "elamigos", "rune", "tenoke", "flt", "setup", "win64", "win32",
        "x64", "x86", "bin", "game", "launcher",
    ] {
        let lower = s.to_lowercase();
        if let Some(pos) = lower.find(junk) {
            let before_ok = pos == 0 || !lower.as_bytes()[pos - 1].is_ascii_alphanumeric();
            let after = pos + junk.len();
            let after_ok =
                after >= lower.len() || !lower.as_bytes()[after].is_ascii_alphanumeric();
            if before_ok && after_ok {
                s.replace_range(pos..after, " ");
            }
        }
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_scene_group_and_release_noise() {
        assert_eq!(clean("Hollow_Knight-FLT"), "Hollow Knight");
        assert_eq!(clean("Stardew.Valley.GOG"), "Stardew Valley");
        assert_eq!(clean("Cuphead-CODEX"), "Cuphead");
    }

    #[test]
    fn leaves_titles_that_only_partially_overlap_junk_words() {
        // "gog" is junk, but must not eat part of an unrelated word.
        assert_eq!(clean("Goggles of Doom"), "Goggles of Doom");
    }

    #[test]
    fn collapses_separators_to_single_spaces() {
        assert_eq!(clean("The___Long---Dark..."), "The Long Dark");
    }

    /// Exercises guess_name against the real `ludusavi` binary. Not run by
    /// default (needs ludusavi on PATH and its manifest downloaded) - run
    /// explicitly with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn guesses_real_game_name_from_a_scene_release_folder() {
        let exe = Path::new("D:/Nimbus/testenv/Hollow Knight-FLT/game.exe");
        let guess = guess_name(Path::new("ludusavi"), exe);
        assert_eq!(guess.as_deref(), Some("Hollow Knight"));
    }
}
