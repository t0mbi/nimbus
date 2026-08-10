# nimbus

Self-hosted "cloud saves" for PC games. Keeps your saves in sync across multiple machines using a folder *you* control — a NAS share, an external drive, whatever — with no Steam Cloud, no subscription, and no per-game setup.

Under the hood, Nimbus runs on [Ludusavi](https://github.com/mtkennerly/ludusavi)'s engine, which already knows where thousands of games keep their save files and how to move them between machines (even across Windows/Linux). But Ludusavi is a tool you invoke by hand, with its own separate settings window. Nimbus is the single program you actually run: **its own window** for setup, games, and manual sync, plus the auto-trigger that fires on every launch. You never need to open anything called "Ludusavi."

## How it works

You put this in a game's **Launch Options**:

```
nimbus %command%
```

(or the full path to `nimbus.exe`, until it's on your `PATH` — see [Setup](#setup)). Your launcher expands `%command%` into the real command that starts the game, so nimbus ends up wrapping it. From there:

1. **Restore** — pulls the latest save for this game down from your sync folder.
2. **Launch** — starts the real game and waits for it to exit. (Blocking matters: Steam considers a game "running" until the whole wrapped process tree exits.)
3. **Back up** — pushes any changed saves back up.
4. Exits.

There's no background daemon and nothing running between sessions. Sync happens at the only two moments it needs to: right before you play, and right after you stop.

## Requirements

- [Ludusavi](https://github.com/mtkennerly/ludusavi) — the save-discovery engine Nimbus drives. Either on your `PATH`, or just drop `ludusavi.exe` in the same folder as `nimbus.exe` (or `target/release/nimbus.exe` if building from source) — Nimbus checks next to itself first, so no PATH setup is required for this part.
- A sync destination reachable as a normal filesystem path — a mounted network share, an external drive.

## Setup

Double-click `nimbus.exe` (or run `run.bat` in this folder).

The **first time** it runs, it opens a welcome screen: a short explanation of how it works, the sync folder picker, the Add-to-PATH button, and the Launch Options line to paste into Steam, all in one place. After that, it opens straight to Nimbus's normal window:

- **Settings tab** — confirms Ludusavi is found, lets you pick the sync folder (Browse, or type a UNC path), format (zip, keeps history — recommended) and how many versions to retain, and gives you the exact Launch Options line with a Copy button. An **Add Nimbus to PATH** button lets Launch Options just say `nimbus %command%` instead of a full path (restart Steam afterward for it to take effect). A **Show welcome screen again** button at the bottom re-opens the first-run screen any time.
- **Games tab** — lists everything Ludusavi finds local save data for, with manual **Push** (back up now) / **Pull** (restore now) per game, for whenever you want to sync without actually launching something.

Paste the Launch Options line into each game's Steam **Properties → Launch Options**. Repeat on your other machines, all pointed at the same shared folder.

If you already had Ludusavi configured before installing Nimbus, the welcome screen is skipped automatically on first launch (your existing sync folder is inherited as a starting point) — use **Show welcome screen again** if you want to see it anyway.

## Game identification

Nimbus needs to know *which* game it's wrapping, and it should never make you look up an ID.

- **Steam** — Steam sets a `SteamAppId` environment variable on everything it launches, including through a `%command%` wrapper. Nimbus inherits it and hands off to Ludusavi's own Steam lookup. Zero configuration.
- **Anything else** (a raw shortcut, a launcher Steam doesn't wrap) — the first time Nimbus sees an executable it doesn't recognize, it guesses a game name from the folder/exe name (stripping scene-release noise like `-FLT`, `.GOG`, `_REPACK`) via `ludusavi find --fuzzy`, then shows a one-time Yes/No confirmation. Your answer is remembered — the exe→name mapping is saved, so you're never asked twice for the same game. Saying no to a guess it can't place at all offers "stop asking about this."

If Nimbus can't identify the game, or hasn't been given a sync folder yet, **it still launches the game**, just without syncing. A failed lookup should never stop you from playing.

## Protecting your saves

Conflicts are handled last-write-wins by timestamp, which fits the actual use case here — one person moving between their own machines, not several people editing at once.

Nothing is silently destroyed: backups are stored as timestamped zips (the default format), and old versions stick around up to the retention count you set (5 by default), so an overwrite you didn't want is recoverable. Browse and restore old versions directly:

```bash
ludusavi backups --path "/mnt/nas/gamesaves"
```

Because Ludusavi stores saves in a relocatable form rather than mirroring raw paths, a save backed up on Windows restores correctly on Linux (and under Proton), where the actual save directory and username are different.

## Config

Everything Nimbus needs is in its own config, at `%APPDATA%\nimbus\config.json` (Windows) / `~/.config/nimbus/config.json` (Linux) — Ludusavi's own config file is never read for settings (only, once, to pre-fill the sync folder if you'd already set one up before installing Nimbus) or written to at all:

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

`ludusavi_path` only needs setting if `ludusavi` isn't on `PATH` and no bundled copy is found next to Nimbus. Launch-time activity is logged to `nimbus.log` in the same folder — useful since the release build has no console window.

For testing, `NIMBUS_GAME_NAME` overrides identification for a single run without touching the saved config.

## Commands

| Command | What it does |
| --- | --- |
| `nimbus` | Open the settings window |
| `nimbus <command> [args...]` | Restore, run the command (blocking), back up |
| `nimbus --version` | Print version |
| `nimbus --help` | Print help |

## Building

```bash
cargo build --release
```

Produces a single self-contained binary (~3 MB) at `target/release/nimbus`. Builds on Windows and Linux. `run.bat` builds it for you if it's missing, and mirrors a root-folder `ludusavi.exe` alongside the built binary.

## Roadmap

- `--infer heroic` / `--infer lutris` passthrough, which Ludusavi already supports, for launchers other than Steam that set an equivalent env var
- Optional background service that checks for newer remote saves before you launch anything — a convenience, not a correctness fix, since the launch-time restore already handles that

Deliberately out of scope: sync over the open internet, and any launcher-specific integration beyond reading an env var.

## License

MIT
