# nimbus

Self-hosted "cloud saves" for PC games. Keeps your saves in sync across multiple machines using a folder *you* control — a NAS share, an external drive, whatever — with no Steam Cloud, no subscription, and no per-game setup.

Nimbus is a small wrapper around [Ludusavi](https://github.com/mtkennerly/ludusavi), which already knows where thousands of games keep their save files, how to move them between machines (even across Windows/Linux), and has a proper settings GUI for all of that. Ludusavi has no idea *when* to run, though — it's a command you invoke by hand. Nimbus supplies the "when": it hooks into the moment you launch a game and the moment you quit, and figures out *which* game with no manual ID-hunting.

Nimbus does not have its own settings window and does not duplicate Ludusavi's — where saves go, what format, how many versions are kept, and which folders are scanned are all configured in Ludusavi's GUI (`ludusavi gui`). Nimbus's setup window just points you there and hands you the one string Ludusavi's GUI can't give you: what to paste into Steam.

## How it works

You put this in a game's **Launch Options**:

```
"C:\path\to\nimbus.exe" %command%
```

Your launcher expands `%command%` into the real command that starts the game, so nimbus ends up wrapping it. From there:

1. **Restore** — pulls the latest save for this game down, via `ludusavi wrap`.
2. **Launch** — starts the real game and waits for it to exit. (Blocking matters: Steam considers a game "running" until the whole wrapped process tree exits.)
3. **Back up** — pushes any changed saves back up.
4. Exits.

There's no background daemon and nothing running between sessions. Sync happens at the only two moments it needs to: right before you play, and right after you stop.

## Requirements

- [Ludusavi](https://github.com/mtkennerly/ludusavi) installed and on your `PATH`
- In Ludusavi's own settings (`ludusavi gui`): a backup path pointed at your shared folder (not the local default), format set to `zip`, and a full-backup retention above 1 so old saves aren't silently overwritten

## Setup

Double-click `nimbus.exe` (or run `run.bat` in this folder). It opens a small setup dialog that:

- Confirms Ludusavi is installed and shows its version
- Shows where your saves are currently syncing to (reading Ludusavi's own config), and warns if it still looks like a local folder rather than a network share
- Gives you the exact Launch Options line, already copied to your clipboard
- Has a button to jump straight into Ludusavi's settings

Paste that line into each game's Steam **Properties → Launch Options**. Repeat on your other machines, with everyone's Ludusavi pointed at the same shared folder.

## Game identification

Nimbus needs to know *which* game it's wrapping, and it should never make you look up an ID.

- **Steam** — Steam sets a `SteamAppId` environment variable on everything it launches, including through a `%command%` wrapper. Nimbus inherits it and hands off to Ludusavi's own Steam lookup. Zero configuration.
- **Anything else** (a raw shortcut, a launcher Steam doesn't wrap) — the first time nimbus sees an executable it doesn't recognize, it guesses a game name from the folder/exe name (stripping scene-release noise like `-FLT`, `.GOG`, `_REPACK`) via `ludusavi find --fuzzy`, then shows a one-time Yes/No confirmation. Your answer is remembered — the exe→name mapping is saved, so you're never asked twice for the same game. Saying no to a guess it can't place at all offers "stop asking about this."

If nimbus can't identify the game, or hasn't been given a sync folder yet, **it still launches the game**, just without syncing. A failed lookup should never stop you from playing.

## Protecting your saves

Conflicts are handled last-write-wins by timestamp, which fits the actual use case here — one person moving between their own machines, not several people editing at once.

Nothing is silently destroyed, as long as Ludusavi's own retention is set above the default of 1 (see [Requirements](#requirements)) — with that set, backups are timestamped zips and old versions stick around, so an overwrite you didn't want is recoverable. Browse and restore old versions directly:

```bash
ludusavi backups --path "/mnt/nas/gamesaves"
```

Because Ludusavi stores saves in a relocatable form rather than mirroring raw paths, a save backed up on Windows restores correctly on Linux (and under Proton), where the actual save directory and username are different.

## Config

Nimbus only stores what Ludusavi has no concept of: which game a given executable maps to, and which executables you've told it to stop asking about. At `%APPDATA%\nimbus\config.json` (Windows) / `~/.config/nimbus/config.json` (Linux):

```json
{
  "ludusavi_path": null,
  "exe_names": { "C:/Games/HollowKnight/hollow_knight.exe": "Hollow Knight" },
  "ignored_exes": []
}
```

`ludusavi_path` only needs setting if `ludusavi` isn't on `PATH`. Launch-time activity is logged to `nimbus.log` in the same folder — useful since the release build has no console window.

For testing, `NIMBUS_GAME_NAME` overrides identification for a single run without touching the saved config.

## Commands

| Command | What it does |
| --- | --- |
| `nimbus` | Open the setup dialog |
| `nimbus <command> [args...]` | Restore, run the command (blocking), back up |
| `nimbus --version` | Print version |
| `nimbus --help` | Print help |

## Building

```bash
cargo build --release
```

Produces a single self-contained binary (~340 KB) at `target/release/nimbus`. Builds on Windows and Linux. `run.bat` builds it for you if it's missing.

## Roadmap

- `--infer heroic` / `--infer lutris` passthrough, which Ludusavi already supports, for launchers other than Steam that set an equivalent env var
- Optional background service that checks for newer remote saves before you launch anything — a convenience, not a correctness fix, since the launch-time restore already handles that

Deliberately out of scope: sync over the open internet, any Ludusavi settings UI of our own, and any launcher-specific integration beyond reading an env var.

## License

MIT
