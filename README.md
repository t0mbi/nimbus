# nimbus

Self-hosted "cloud saves" for PC games. Keeps your saves in sync across multiple machines using a folder *you* control — a NAS share, an external drive, whatever — with no Steam Cloud, no subscription, and no per-game setup.

Under the hood, Nimbus runs on [Ludusavi](https://github.com/mtkennerly/ludusavi)'s engine, which already knows where thousands of games keep their save files and how to move them between machines (even across Windows/Linux). But Ludusavi is a tool you invoke by hand, with its own separate settings window. Nimbus is the single program you actually run — set it up once, and it works quietly in the background from then on, the same way Steam Cloud does, just pointed at your own folder instead of Valve's servers.

## How it works

Nimbus's normal mode is a background daemon (`nimbus --tray`) that lives in your system tray:

- **Watches** every game Ludusavi finds local save data for. The moment a save file changes, it pushes that game to your sync folder.
- **Polls** the sync folder periodically. If another PC has pushed something newer for a game, it pulls it down automatically.

No Steam Launch Options, no shortcut editing, nothing to configure per game. It doesn't matter how you launch anything — Steam, a desktop icon, whatever — because syncing isn't tied to a launch event at all, just to the save files actually changing.

The one honest limitation: if you close a game on one PC and start it on another within the same short poll window (tens of seconds), the second PC could still be a beat behind. Real Steam Cloud has this same caveat (it tells you to wait a moment before switching machines) — this isn't worse, just not instantaneous.

For Steam-owned games specifically, there's also an optional launch-triggered mode (`nimbus %command%` in Launch Options) that guarantees a pull happens immediately before that specific launch, if you want the extra certainty on top of the background daemon. It's not required, and doesn't work reliably for non-Steam shortcuts (Steam only expands `%command%` for games it actually owns) — see [Advanced: launch-triggered mode](#advanced-launch-triggered-mode).

## Requirements

- [Ludusavi](https://github.com/mtkennerly/ludusavi) — the save-discovery engine Nimbus drives. Either on your `PATH`, or just drop `ludusavi.exe` (or `ludusavi` on Linux) in the same folder as `nimbus.exe` — Nimbus checks next to itself first, so no PATH setup is required for this part.
- A sync destination reachable as a normal filesystem path — a mounted network share, an external drive.

## Setup

Double-click `nimbus.exe` (or run `run.bat` in this folder). The first time it runs, it opens a welcome screen:

1. **Pick your sync folder** — Browse, or type a UNC path.
2. **Enable background sync** — one click. Starts running immediately, and again automatically at every login from then on.
3. *(Optional, collapsed by default)* the Launch Options string and Add-to-PATH button, for the advanced launch-triggered mode described above.

After that, Nimbus opens straight to its normal window with two tabs — **Settings** (sync folder, format/retention, the background-sync toggle, a **Show welcome screen again** button) and **Games** (everything Ludusavi finds locally, with manual **Push**/**Pull** per game for whenever you want to sync on demand). Repeat setup on your other machines, all pointed at the same shared folder.

The tray icon's right-click menu has **Sync now** (an immediate full check across every game), **Pause/Resume syncing**, **Open Nimbus** (the settings window), and **Quit**.

If you already had Ludusavi configured before installing Nimbus, the welcome screen's sync-folder field is pre-filled from that as a starting point.

## Game identification

The background daemon doesn't need to identify a "launching game" at all — it just watches whatever local save data Ludusavi already finds, so there's nothing to configure here for the primary sync path.

Game identification only matters for the *optional* launch-triggered mode:

- **Steam** — Steam sets a `SteamAppId` environment variable on everything it launches, including through a `%command%` wrapper. Nimbus inherits it and hands off to Ludusavi's own Steam lookup. Zero configuration.
- **Anything else** — the first time Nimbus sees an unrecognized executable, it guesses a game name from the folder/exe name (stripping scene-release noise like `-FLT`, `.GOG`, `_REPACK`) via `ludusavi find --fuzzy`, then shows a one-time Yes/No confirmation, remembered from then on.

If Nimbus can't identify the game, or hasn't been given a sync folder yet, **it still launches the game**, just without that extra guaranteed pull. A failed lookup should never stop you from playing.

## Protecting your saves

Conflicts are handled last-write-wins by timestamp, which fits the actual use case here — one person moving between their own machines, not several people editing at once.

Nothing is silently destroyed: backups are stored as timestamped zips (the default format), and old versions stick around up to the retention count you set (5 by default), so an overwrite you didn't want is recoverable. Browse and restore old versions directly:

```bash
ludusavi backups --path "/mnt/nas/gamesaves"
```

Because Ludusavi stores saves in a relocatable form rather than mirroring raw paths, a save backed up on Windows restores correctly on Linux (and under Proton), where the actual save directory and username are different.

## Config

Everything Nimbus needs is in its own config, at `%APPDATA%\nimbus\config.json` (Windows) / `~/.config/nimbus/config.json` (Linux):

```json
{
  "sync_path": "//TOWER/Daniel/Game Saves",
  "format": "zip",
  "full_limit": 5,
  "ludusavi_path": null,
  "exe_names": { "C:/Games/HollowKnight/hollow_knight.exe": "Hollow Knight" },
  "ignored_exes": []
}
```

`ludusavi_path` only needs setting if `ludusavi` isn't on `PATH` and no bundled copy is found next to Nimbus. Activity is logged to `nimbus.log` in the same folder — useful since the release build has no console window. The daemon also keeps a small `daemon_state.json` there, tracking the last remote timestamp it's accounted for per game (so a restart doesn't immediately re-pull everything, and its own pushes don't loop back as pulls).

For testing, `NIMBUS_GAME_NAME` overrides identification for a single launch-triggered run without touching the saved config.

## Commands

| Command | What it does |
| --- | --- |
| `nimbus` | Open the settings window |
| `nimbus --tray` | Run the background sync daemon (tray icon, no window) — usually started automatically, see Settings |
| `nimbus <command> [args...]` | Advanced launch-triggered mode: restore, run the command (blocking), back up |
| `nimbus --version` | Print version |
| `nimbus --help` | Print help |

## Advanced: launch-triggered mode

For a game you actually own on Steam, put this in its Launch Options:

```
nimbus %command%
```

(or the full path to `nimbus.exe`, until it's on your `PATH` — the Settings tab's Add-to-PATH button handles that). Steam expands `%command%` into the real launch command, so Nimbus ends up wrapping it: restore, launch, wait for exit, back up.

This does **not** work for non-Steam shortcuts — Steam only performs `%command%` substitution for games it actually owns. For a shortcut, Launch Options are just appended as plain arguments to whatever's in the Target field, so `nimbus %command%` there does nothing useful. This is exactly why the background daemon is the primary mechanism: it doesn't care how, or through what launcher, a game actually starts.

## Building

```bash
cargo build --release
```

Produces a single self-contained binary (~3 MB) at `target/release/nimbus`. Builds on Windows and Linux. `run.bat` builds it for you if it's missing, and mirrors a root-folder `ludusavi.exe` alongside the built binary.

## Roadmap

- A [Decky Loader](https://github.com/SteamDeckHomebrew/decky-loader) plugin for Steam Deck / gamescope-session handhelds, where the primary interface is Game Mode rather than a desktop with a system tray — same watch-and-poll design, implemented against Decky's Python backend instead of a native tray icon (in progress, see `deck/`)
- `--infer heroic` / `--infer lutris` passthrough for the launch-triggered mode, for launchers other than Steam that set an equivalent env var

Deliberately out of scope: sync over the open internet, and any launcher-specific integration beyond reading an env var.

## License

MIT
