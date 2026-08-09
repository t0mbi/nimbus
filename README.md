# nimbus

Self-hosted "cloud saves" for PC games. Keeps your saves in sync across multiple machines using a folder *you* control — a NAS share, an external drive, whatever — with no Steam Cloud, no subscription, and no per-game setup.

Nimbus is a small wrapper around [Ludusavi](https://github.com/mtkennerly/ludusavi), which already knows where thousands of games keep their save files and how to move them between machines. Ludusavi has no idea *when* to run, though — it's a stateless command you invoke by hand. Nimbus supplies the "when": it hooks into the moment you launch a game and the moment you quit.

## How it works

You put this in a game's **Launch Options**:

```
nimbus %command%
```

Your launcher expands `%command%` into the real command that starts the game, so nimbus ends up wrapping it. From there:

1. **Restore** — pulls the latest save for this game down from your sync folder.
2. **Launch** — starts the real game and waits for it to exit. (Blocking matters: Steam considers a game "running" until the whole wrapped process tree exits.)
3. **Back up** — pushes any changed saves back up to your sync folder.
4. Exits.

There's no background daemon and nothing running between sessions. Sync happens at the only two moments it needs to: right before you play, and right after you stop.

## Requirements

- [Ludusavi](https://github.com/mtkennerly/ludusavi) installed and on your `PATH`
- A sync destination reachable as a normal filesystem path — an SMB/NFS share that's already mounted, a mapped drive, an external disk

Syncing over the open internet is out of scope. Nimbus writes to a folder; getting that folder onto your network is your OS's job.

## Setup

Point nimbus at your sync folder once:

```bash
nimbus --set-remote "/mnt/nas/gamesaves"
```

On Windows that might look like `nimbus --set-remote "Z:\gamesaves"` or a UNC path.

Then set the Launch Options for any game you want synced (use the full path to the binary if it isn't on your `PATH`):

```
nimbus %command%
```

That's the whole setup. Repeat the Launch Options step on your other machines, pointing them at the same shared folder.

## Game identification

Nimbus needs to know *which* game it's wrapping, and it should never make you look up an ID.

- **Steam** — Steam sets a `SteamAppId` environment variable on everything it launches, including through a `%command%` wrapper. Nimbus inherits it and hands off to Ludusavi's own Steam lookup. Zero configuration.
- **Anything else** — if there's no recognizable launcher signal, nimbus can't identify the game on its own. For now you can tell it explicitly:

  ```bash
  nimbus --set-name "/path/to/Game.exe" "Exact Ludusavi Game Name"
  ```

  That mapping is saved, so you only do it once per game. A one-time confirmation prompt to replace this manual step is planned — see [Roadmap](#roadmap).

If nimbus can't identify the game, **it still launches it**, just without syncing. A failed lookup should never stop you from playing.

## Protecting your saves

Conflicts are handled last-write-wins by timestamp, which fits the actual use case here — one person moving between their own machines, not several people editing at once.

Nothing is silently destroyed. Nimbus tells Ludusavi to store backups as timestamped zips and keep several historical versions per game (5 by default), so an overwrite you didn't want is recoverable:

```bash
nimbus --set-full-limit 10
```

You can browse and restore old versions with Ludusavi directly:

```bash
ludusavi backups --path "/mnt/nas/gamesaves"
```

Because Ludusavi stores saves in a relocatable form rather than mirroring raw paths, a save backed up on Windows restores correctly on Linux (and under Proton), where the actual save directory and username are different.

## Commands

| Command | What it does |
| --- | --- |
| `nimbus <command> [args...]` | Restore, run the command (blocking), back up |
| `nimbus --set-remote <path>` | Set the sync destination |
| `nimbus --set-name <exe> <name>` | Map an executable to a Ludusavi game name |
| `nimbus --forget-exe <exe>` | Remove a mapping |
| `nimbus --set-full-limit <n>` | Historical versions retained per game (default 5) |
| `nimbus --list` | Show current config |
| `nimbus --version` | Print version |
| `nimbus --help` | Print help |

Config lives at `%APPDATA%\nimbus\config.json` on Windows and `~/.config/nimbus/config.json` on Linux.

For testing, `NIMBUS_GAME_NAME` overrides identification for a single run.

## Building

```bash
cargo build --release
```

Produces a single self-contained binary (~276 KB) at `target/release/nimbus`. Builds on Windows and Linux.

## Roadmap

- One-time confirmation prompt (OS notification) the first time an unrecognized executable is seen, replacing manual `--set-name`
- Support for `--infer heroic` / `--infer lutris`, which Ludusavi already provides
- Optional background service that checks for newer remote saves before you launch anything — a convenience, not a correctness fix, since the launch-time restore already handles that

Deliberately out of scope: sync over the open internet, and any launcher-specific integration.

## License

MIT
