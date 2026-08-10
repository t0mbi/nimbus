"""
Nimbus for Steam Deck / gamescope handhelds.

Same design as the Windows tray daemon, ported to Decky's Python backend
instead of a native tray icon - deliberately *not* hooking Steam's app
lifecycle events (SteamClient.GameSessions / AppLifetimeNotifications).
Real-world plugins that rely on that for non-Steam shortcuts (see
NonSteamLaunchersDecky's "game watcher") needed heuristic fallbacks -
overlay-state pattern matching, grace periods, termination buffers - to
make it reliable at all, which is the same category of pain Nimbus hit on
Windows trying to hook Steam Launch Options for shortcuts. A plain polling
loop sidesteps that class of bug entirely: it doesn't care how or whether
a "launch" was ever detected, only whether local mtimes or the remote
share have actually changed.

Unlike the Windows daemon (which gets instant OS-level file-change events),
this polls on a timer and compares actual timestamps every cycle - simpler,
no extra dependency to vendor, and correct either way; the only cost is
polling-interval latency rather than near-instant reaction, which is a fine
trade for a handheld given how much longer a play session usually is than
this loop's interval.
"""

import asyncio
import json
import os
import time
from datetime import datetime

import decky

SETTINGS_FILE = "settings.json"
STATE_FILE = "daemon_state.json"

POLL_INTERVAL_SECONDS = 20
# Filesystem/clock granularity guard, same purpose as the Windows daemon's
# MTIME_TOLERANCE_SECS - avoids flip-flopping on timestamps that are "equal
# enough" rather than a genuine change on either side.
TOLERANCE_SECONDS = 3.0

DEFAULT_FORMAT = "zip"
DEFAULT_FULL_LIMIT = 5


def _settings_path() -> str:
    return os.path.join(decky.DECKY_PLUGIN_SETTINGS_DIR, SETTINGS_FILE)


def _state_path() -> str:
    return os.path.join(decky.DECKY_PLUGIN_SETTINGS_DIR, STATE_FILE)


def _load_json(path: str, default):
    try:
        with open(path, "r") as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError, OSError):
        return default


def _save_json(path: str, data) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        json.dump(data, f, indent=2)


def _ludusavi_bin() -> str:
    """Bundled copy next to the plugin first, then whatever's on PATH -
    mirrors the Windows build's `config::ludusavi_bin` resolution order."""
    bundled = os.path.join(decky.DECKY_PLUGIN_DIR, "bin", "ludusavi")
    if os.path.isfile(bundled) and os.access(bundled, os.X_OK):
        return bundled
    return "ludusavi"


async def _run_ludusavi(args: list[str]) -> tuple[int, str, str]:
    proc = await asyncio.create_subprocess_exec(
        _ludusavi_bin(),
        *args,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    stdout, stderr = await proc.communicate()
    code = proc.returncode if proc.returncode is not None else 1
    return code, stdout.decode(errors="replace"), stderr.decode(errors="replace")


def _parse_rfc3339(value: str) -> float:
    """Ludusavi's `when` timestamps look like `2026-08-10T01:08:01.053222200Z`
    - up to 9 fractional digits, which `datetime.fromisoformat` only reliably
    accepts on very recent Python. Normalize by hand rather than assume a
    particular Python version on whatever this plugin ends up running under.
    """
    s = value.strip()
    if s.endswith("Z"):
        s = s[:-1] + "+00:00"
    if "." in s:
        head, rest = s.split(".", 1)
        tz = ""
        frac = rest
        for i, c in enumerate(rest):
            if c in "+-" and i > 0:
                frac, tz = rest[:i], rest[i:]
                break
        frac = (frac + "000000")[:6]
        s = f"{head}.{frac}{tz}"
    return datetime.fromisoformat(s).timestamp()


def _newest(backups: list[dict]):
    """(raw_when_string, epoch_seconds) for the newest entry in a ludusavi
    `backups` list, or (None, None) if it's empty."""
    whens = [b["when"] for b in backups if "when" in b]
    if not whens:
        return None, None
    latest_when = max(whens)
    return latest_when, _parse_rfc3339(latest_when)


class Plugin:
    async def _main(self):
        self.settings = _load_json(_settings_path(), {})
        self.state = _load_json(_state_path(), {"last_synced_remote": {}})
        self.paused = False
        self.last_result = None
        decky.logger.info("Nimbus: daemon starting")
        self.loop = asyncio.get_event_loop()
        self.loop.create_task(self._poll_loop())

    async def _unload(self):
        decky.logger.info("Nimbus: daemon stopping")

    async def _uninstall(self):
        pass

    # ---------------------------------------------------------------
    # Frontend-callable methods (via @decky/api's `callable`)
    # ---------------------------------------------------------------

    async def get_settings(self):
        return {
            "sync_path": self.settings.get("sync_path"),
            "format": self.settings.get("format", DEFAULT_FORMAT),
            "full_limit": self.settings.get("full_limit", DEFAULT_FULL_LIMIT),
            "paused": self.paused,
        }

    async def set_sync_path(self, path: str):
        self.settings["sync_path"] = path.strip()
        _save_json(_settings_path(), self.settings)

    async def set_format(self, fmt: str):
        self.settings["format"] = fmt
        _save_json(_settings_path(), self.settings)

    async def set_full_limit(self, n: int):
        self.settings["full_limit"] = int(n)
        _save_json(_settings_path(), self.settings)

    async def toggle_pause(self):
        self.paused = not self.paused
        decky.logger.info(f"Nimbus: {'paused' if self.paused else 'resumed'}")
        return self.paused

    async def ludusavi_status(self):
        code, out, _ = await _run_ludusavi(["--version"])
        if code == 0:
            return {"found": True, "version": out.strip()}
        return {"found": False, "version": None}

    async def sync_now(self):
        await self._poll_once()
        await decky.emit("nimbus_sync_complete", self.last_result)
        return self.last_result

    # ---------------------------------------------------------------
    # Core loop
    # ---------------------------------------------------------------

    async def _poll_loop(self):
        while True:
            await asyncio.sleep(POLL_INTERVAL_SECONDS)
            if not self.paused:
                await self._poll_once()

    async def _poll_once(self):
        sync_path = self.settings.get("sync_path")
        if not sync_path:
            self.last_result = {"ok": False, "message": "No sync folder configured yet."}
            return

        games = await self._list_games()
        remote_map = await self._latest_remote_all(sync_path)

        pushed, pulled, failed = [], [], []
        for game in games:
            outcome = await self._sync_game(game, sync_path, remote_map)
            if outcome == "pushed":
                pushed.append(game["name"])
            elif outcome == "pulled":
                pulled.append(game["name"])
            elif outcome == "failed":
                failed.append(game["name"])

        self.last_result = {
            "ok": True,
            "pushed": pushed,
            "pulled": pulled,
            "failed": failed,
            "checked": len(games),
        }
        if pushed or pulled or failed:
            decky.logger.info(
                f"Nimbus: sync pass - pushed {pushed}, pulled {pulled}, failed {failed}"
            )

    async def _list_games(self):
        code, out, err = await _run_ludusavi(["backup", "--preview", "--api"])
        if code != 0:
            decky.logger.warning(f"Nimbus: couldn't list games: {err}")
            return []
        try:
            data = json.loads(out)
        except json.JSONDecodeError:
            return []
        games = []
        for name, info in data.get("games", {}).items():
            paths = list(info.get("files", {}).keys())
            if paths:
                games.append({"name": name, "paths": paths})
        return games

    async def _latest_local_mtime(self, paths: list[str]) -> float:
        latest = 0.0
        for p in paths:
            try:
                latest = max(latest, os.path.getmtime(p))
            except OSError:
                pass
        return latest

    async def _latest_remote(self, name: str, sync_path: str):
        """Returns (raw_when_string, epoch_seconds) for one game's newest
        remote backup, or (None, None) if there isn't one yet. Only for the
        one legitimate per-game use left (updating the marker right after
        *this* game was just pushed) - never call this in a loop over many
        games. See `_latest_remote_all`."""
        code, out, err = await _run_ludusavi(["backups", "--api", "--path", sync_path, name])
        if code != 0:
            return None, None
        try:
            data = json.loads(out)
        except json.JSONDecodeError:
            return None, None
        backups = data.get("games", {}).get(name, {}).get("backups", [])
        return _newest(backups)

    async def _latest_remote_all(self, sync_path: str) -> dict:
        """Same data as `_latest_remote`, for every game at `sync_path` in a
        single call - measured cost on the Windows build was ~850ms per
        `ludusavi` invocation almost regardless of subcommand or how much
        data is asked about, because it loads/parses its full manifest on
        startup; `--version` alone was ~80ms, everything else was ~850ms
        whether asking about one game or all of them. A poll loop calling
        this once per game, per cycle, would turn a supposedly-cheap timer
        into `num_games` x ~850ms of real work every cycle - potentially
        longer than the interval itself for a large library. One call
        covering everything costs the same as one call covering one game.
        """
        code, out, err = await _run_ludusavi(["backups", "--api", "--path", sync_path])
        if code != 0:
            decky.logger.warning(f"Nimbus: couldn't check remote backups: {err}")
            return {}
        try:
            data = json.loads(out)
        except json.JSONDecodeError:
            return {}
        result = {}
        for name, info in data.get("games", {}).items():
            when, epoch = _newest(info.get("backups", []))
            if when is not None:
                result[name] = (when, epoch)
        return result

    async def _sync_game(self, game: dict, sync_path: str, remote_map: dict) -> str:
        name = game["name"]
        local_mtime = await self._latest_local_mtime(game["paths"])
        if local_mtime == 0.0:
            return "skipped"

        remote_when, remote_epoch = remote_map.get(name, (None, None))

        if remote_epoch is None:
            return await self._push(name, sync_path)

        if local_mtime > remote_epoch + TOLERANCE_SECONDS:
            return await self._push(name, sync_path)

        if remote_epoch > local_mtime + TOLERANCE_SECONDS:
            if self.state["last_synced_remote"].get(name) == remote_when:
                return "skipped"  # already accounted for this exact remote state
            return await self._pull(name, sync_path, remote_when)

        return "skipped"

    async def _push(self, name: str, sync_path: str) -> str:
        fmt = self.settings.get("format", DEFAULT_FORMAT)
        limit = str(self.settings.get("full_limit", DEFAULT_FULL_LIMIT))
        code, _, err = await _run_ludusavi(
            [
                "backup", "--api", "--force",
                "--path", sync_path,
                "--format", fmt,
                "--full-limit", limit,
                name,
            ]
        )
        if code != 0:
            decky.logger.warning(f"Nimbus: push failed for {name}: {err}")
            await decky.emit("nimbus_game_event", name, "push_failed", err)
            return "failed"

        when_after, _ = await self._latest_remote(name, sync_path)
        if when_after:
            self.state["last_synced_remote"][name] = when_after
            _save_json(_state_path(), self.state)
        decky.logger.info(f"Nimbus: pushed {name}")
        await decky.emit("nimbus_game_event", name, "pushed", None)
        return "pushed"

    async def _pull(self, name: str, sync_path: str, remote_when: str) -> str:
        code, _, err = await _run_ludusavi(
            ["restore", "--api", "--force", "--path", sync_path, name]
        )
        if code != 0:
            decky.logger.warning(f"Nimbus: pull failed for {name}: {err}")
            await decky.emit("nimbus_game_event", name, "pull_failed", err)
            return "failed"

        self.state["last_synced_remote"][name] = remote_when
        _save_json(_state_path(), self.state)
        decky.logger.info(f"Nimbus: pulled {name}")
        await decky.emit("nimbus_game_event", name, "pulled", None)
        return "pulled"
