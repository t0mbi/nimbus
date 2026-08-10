//! The background sync daemon (`nimbus --tray`): watches every known game's
//! local save files for changes and pushes on write, and periodically checks
//! whether the remote side has something newer to pull. No launch hook of any
//! kind - this is what lets Nimbus be "set up once, never touched again."

use crate::config::Config;
use crate::log;
use crate::ludusavi_ctl::{self, GameStatus};
use crate::toast;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_secs(45);
const DEBOUNCE: Duration = Duration::from_secs(3);

/// Per-game "last remote timestamp we've already accounted for," persisted
/// so a restart doesn't immediately re-pull everything, and so a push we
/// just made doesn't look like a new remote change worth pulling back down.
#[derive(Serialize, Deserialize, Default)]
struct DaemonState {
    #[serde(default)]
    last_synced_remote: HashMap<String, String>,
}

fn state_file() -> std::io::Result<PathBuf> {
    Ok(crate::config::config_dir()?.join("daemon_state.json"))
}

impl DaemonState {
    fn load() -> Self {
        state_file()
            .ok()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        let Ok(path) = state_file() else { return };
        if let Ok(data) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, data);
        }
    }
}

/// Shared handle the tray menu uses to talk to the running daemon, without
/// needing to know its internals (state persistence, ludusavi plumbing).
#[derive(Clone)]
pub struct DaemonHandle {
    config: Config,
    state: Arc<Mutex<DaemonState>>,
    paused: Arc<AtomicBool>,
}

impl DaemonHandle {
    pub fn sync_now(&self) {
        sync_all_now(&self.config, &self.state);
    }

    pub fn toggle_pause(&self) -> bool {
        let now_paused = !self.paused.load(Ordering::Relaxed);
        self.paused.store(now_paused, Ordering::Relaxed);
        log::line(&format!("daemon: {}", if now_paused { "paused" } else { "resumed" }));
        now_paused
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }
}

pub fn run() {
    let config = Config::load().unwrap_or_default();
    if config.sync_path.is_none() {
        log::line("daemon: no sync folder configured - exiting. Open Nimbus and set one first.");
        return;
    }

    let bin = config.ludusavi_bin();
    let paused = Arc::new(AtomicBool::new(false));
    let state = Arc::new(Mutex::new(DaemonState::load()));

    let games = match ludusavi_ctl::list_games(&bin) {
        Ok(g) => g,
        Err(e) => {
            log::line(&format!("daemon: couldn't list games at startup: {e}"));
            Vec::new()
        }
    };
    log::line(&format!("daemon: watching {} game(s)", games.len()));

    let handle = DaemonHandle { config: config.clone(), state: Arc::clone(&state), paused: Arc::clone(&paused) };

    let _watcher = start_watcher(&games, config.clone(), Arc::clone(&paused), Arc::clone(&state));
    start_poll_thread(games, config, Arc::clone(&paused), Arc::clone(&state));

    crate::tray::run(handle);
}

/// One file can (in principle) belong to more than one game entry, so route
/// by exact file path -> list of game names, not a 1:1 map.
///
/// Deliberately watches exact files, not their parent directories. Some
/// games' manifest entries point at a single config file sitting directly in
/// a broad shared folder (e.g. `%APPDATA%\Roaming\SomeConfig.ini`, with no
/// game-specific subfolder at all) - watching that file's *parent* would mean
/// watching the entire Roaming folder, which is one of the busiest
/// directories on the whole system and would misattribute unrelated apps'
/// writes as "this game changed." Watching exact files avoids that, at the
/// cost of not noticing a brand-new save file that didn't exist yet (a
/// restart picks up new files via a fresh `list_games` scan).
fn files_by_game(games: &[GameStatus]) -> HashMap<PathBuf, Vec<String>> {
    let mut map: HashMap<PathBuf, Vec<String>> = HashMap::new();
    for game in games {
        for path in &game.paths {
            let names = map.entry(path.clone()).or_default();
            if !names.contains(&game.name) {
                names.push(game.name.clone());
            }
        }
    }
    map
}

fn start_watcher(
    games: &[GameStatus],
    config: Config,
    paused: Arc<AtomicBool>,
    state: Arc<Mutex<DaemonState>>,
) -> Option<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>> {
    let by_file = files_by_game(games);
    if by_file.is_empty() {
        return None;
    }

    let (tx, rx) = channel::<DebounceEventResult>();
    let mut debouncer = match new_debouncer(DEBOUNCE, tx) {
        Ok(d) => d,
        Err(e) => {
            log::line(&format!("daemon: couldn't start file watcher: {e}"));
            return None;
        }
    };

    let mut watched = 0;
    for file in by_file.keys() {
        if file.exists() {
            if debouncer.watcher().watch(file, notify::RecursiveMode::NonRecursive).is_ok() {
                watched += 1;
            }
        }
    }
    log::line(&format!("daemon: watching {watched} file{}", if watched == 1 { "" } else { "s" }));

    std::thread::spawn(move || {
        for result in rx {
            let Ok(events) = result else { continue };
            if paused.load(Ordering::Relaxed) {
                continue;
            }

            let mut changed_games: Vec<String> = Vec::new();
            for event in &events {
                if let Some(names) = by_file.get(&event.path) {
                    for name in names {
                        if !changed_games.contains(name) {
                            changed_games.push(name.clone());
                        }
                    }
                }
            }

            for game in changed_games {
                push_game(&config, &game, &state);
            }
        }
    });

    Some(debouncer)
}

fn push_game(config: &Config, game: &str, state: &Arc<Mutex<DaemonState>>) {
    let bin = config.ludusavi_bin();
    let Some(sync_path) = &config.sync_path else { return };

    log::line(&format!("daemon: local change detected for {game}, pushing"));
    match ludusavi_ctl::backup(&bin, sync_path, config.format(), config.full_limit(), game) {
        Ok(()) => {
            toast::pushed(game);
            if let Ok(Some(ts)) = ludusavi_ctl::latest_remote_backup(&bin, sync_path, game) {
                let mut s = state.lock().unwrap();
                s.last_synced_remote.insert(game.to_string(), ts);
                s.save();
            }
        }
        Err(e) => {
            log::line(&format!("daemon: push failed for {game}: {e}"));
            toast::backup_failed(game, &e);
        }
    }
}

fn start_poll_thread(games: Vec<GameStatus>, config: Config, paused: Arc<AtomicBool>, state: Arc<Mutex<DaemonState>>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(POLL_INTERVAL);
        if paused.load(Ordering::Relaxed) {
            continue;
        }
        poll_all(&games, &config, &state);
    });
}

/// One `ludusavi backups --api` call covering every game, not one call per
/// game - see `ludusavi_ctl::latest_remote_backups_all`'s doc comment.
/// Measured: ~850ms either way on this machine, so doing it per-game here
/// would turn a 45s poll interval into 71 games x ~850ms = over a minute of
/// actual work per cycle, longer than the interval itself.
fn poll_all(games: &[GameStatus], config: &Config, state: &Arc<Mutex<DaemonState>>) {
    let bin = config.ludusavi_bin();
    let Some(sync_path) = &config.sync_path else { return };

    let remote = match ludusavi_ctl::latest_remote_backups_all(&bin, sync_path) {
        Ok(map) => map,
        Err(e) => {
            log::line(&format!("daemon: couldn't check remote backups: {e}"));
            return;
        }
    };

    for game in games {
        let Some(remote_ts) = remote.get(&game.name) else { continue };
        let local_epoch = ludusavi_ctl::latest_local_mtime_epoch(&game.paths);
        poll_one(&bin, sync_path, &game.name, remote_ts, local_epoch, state);
    }
}

fn poll_one(
    bin: &Path,
    sync_path: &Path,
    name: &str,
    remote_ts: &str,
    local_epoch: Option<f64>,
    state: &Arc<Mutex<DaemonState>>,
) {
    let already_known = {
        let s = state.lock().unwrap();
        s.last_synced_remote.get(name).cloned()
    };

    if already_known.as_deref() == Some(remote_ts) {
        return; // nothing new since we last accounted for this
    }

    // The actual bug this guards against: on a daemon's first-ever run
    // against a share that already has backups from prior manual Ludusavi
    // use, "different from what I've recorded" is true for every game
    // (nothing recorded yet) - without this check, that meant pulling
    // everything unconditionally, regardless of whether the local copy was
    // actually more current. Only pull when the remote genuinely is newer.
    if !ludusavi_ctl::remote_is_newer(remote_ts, local_epoch) {
        // Still worth remembering we've seen this remote state, so a
        // stale-but-unchanged remote doesn't get re-evaluated every cycle.
        let mut s = state.lock().unwrap();
        s.last_synced_remote.insert(name.to_string(), remote_ts.to_string());
        s.save();
        return;
    }

    log::line(&format!("daemon: remote has a newer save for {name}, pulling"));
    match ludusavi_ctl::restore(bin, sync_path, name) {
        Ok(()) => {
            toast::pulled(name);
            let mut s = state.lock().unwrap();
            s.last_synced_remote.insert(name.to_string(), remote_ts.to_string());
            s.save();
        }
        Err(e) => {
            log::line(&format!("daemon: pull failed for {name}: {e}"));
            toast::restore_failed(name, &e);
        }
    }
}

/// Full manual sync pass for every known game, either direction as needed -
/// what the tray's "Sync now" triggers.
fn sync_all_now(config: &Config, state: &Arc<Mutex<DaemonState>>) {
    let bin = config.ludusavi_bin();
    let games = match ludusavi_ctl::list_games(&bin) {
        Ok(g) => g,
        Err(e) => {
            log::line(&format!("daemon: sync-now couldn't list games: {e}"));
            toast::info("Nimbus", &format!("Couldn't list games: {e}"));
            return;
        }
    };
    poll_all(&games, config, state);
    toast::info("Nimbus", "Sync check complete.");
}
