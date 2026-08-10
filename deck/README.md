# Nimbus for Steam Deck / gamescope handhelds

A [Decky Loader](https://github.com/SteamDeckHomebrew/decky-loader) plugin: the same self-hosted cloud-save sync as [Nimbus](../README.md) on Windows, for a device whose primary interface is Steam's Game Mode rather than a desktop with a system tray.

## Why this isn't a launch hook

The obvious approach - hook Steam's app-lifecycle events, restore right before launch, back up right after exit - turns out to be unreliable for non-Steam shortcuts specifically. Checked how [`NonSteamLaunchersDecky`](https://github.com/moraroy/NonSteamLaunchersDecky) handles this: `AppLifetimeNotifications` alone wasn't reliable enough, so they layered on overlay-state pattern matching, a 90-second grace period, and a 10-second termination buffer just to make exit detection trustworthy. That's the same category of problem Nimbus hit on Windows trying to use Steam's `%command%` for shortcuts (it doesn't even fire at all there - see the main README).

So this plugin does the same thing the Windows daemon does: **poll, don't hook.** Every 20 seconds, it compares each game's latest local save mtime against the latest remote backup timestamp and pushes or pulls whichever side is behind. No launch detection of any kind, so there's nothing Steam-specific to break.

## Structure

- `main.py` - the Python backend. `_main()` starts a persistent poll loop (`_poll_loop`); `_sync_game` is the actual push-or-pull-or-skip decision per game, timestamp-driven.
- `src/index.tsx` - the Quick Access Menu panel: sync folder field, background-sync toggle, format/retention, a manual Sync Now button.
- `tests/` - standalone tests for the timestamp parsing and sync decision logic, run with plain `python`, no Decky runtime needed (a stub `decky` module stands in for the real one). Not part of the packaged plugin.

## Status

Scaffolded and unit-tested (the pure decision logic, timestamp parsing) from a Windows dev machine - **not yet verified against the actual Decky runtime or real hardware**, since that needs a Linux/gamescope environment this machine doesn't have. Specifically still open:

- Real end-to-end test on an actual Steam Deck / CachyOS handheld
- `ludusavi` needs to be reachable on the device - either on `PATH`, or bundled at `bin/ludusavi` inside the plugin directory (mirroring how the Windows build checks next to itself first)
- The full frontend build (`pnpm run build`) needs `@rollup/rollup-linux-x64-musl`, a Linux-only native binary - can't run on Windows. `tsc --noEmit` type-checks clean on this machine, but that's as far as Windows can verify.
- Packaging via the Decky CLI (which itself needs Docker for plugins with a custom Python backend) hasn't been run yet.

## Building (on Linux)

```bash
pnpm install
pnpm run build
```

Then package with the [Decky CLI](https://wiki.deckbrew.xyz/plugin-dev/getting-started) and sideload onto the device via Decky's developer mode.

## Running the logic tests

No Decky runtime or Linux needed for these - they exercise `main.py` directly with a stub `decky` module:

```bash
cd tests
python test_timestamps.py
python test_sync_logic.py
```
