import sys
import os
import asyncio
import json
import tempfile
import time
from unittest.mock import AsyncMock, patch

sys.path.insert(0, os.path.dirname(__file__))
sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

import main  # noqa: E402

failures = []


def check(label, actual, expected):
    ok = actual == expected
    print(f"{'ok ' if ok else 'FAIL'}  {label}: {actual!r}" + ("" if ok else f" (expected {expected!r})"))
    if not ok:
        failures.append(label)


def fresh_plugin():
    p = main.Plugin()
    p.settings = {"sync_path": "/mnt/nas/saves", "format": "zip", "full_limit": 5}
    p.state = {"last_synced_remote": {}}
    return p


def make_local_file(mtime_offset_seconds=0):
    f = tempfile.NamedTemporaryFile(delete=False)
    f.close()
    if mtime_offset_seconds:
        t = time.time() + mtime_offset_seconds
        os.utime(f.name, (t, t))
    return f.name


def remote_entry(when: str):
    """Builds a `(when, epoch)` pair the way `_latest_remote_all` would, for
    tests that pass a pre-fetched remote_map directly to `_sync_game` -
    matching how `_poll_once` actually calls it now (fetch once, use for
    every game), not one `_run_ludusavi` call per game."""
    return (when, main._parse_rfc3339(when))


async def run():
    # Scenario 1: no remote backup exists yet -> push
    p = fresh_plugin()
    local_file = make_local_file()
    with patch.object(main, "_run_ludusavi", new=AsyncMock()) as mock_run:
        async def fake_run(args):
            if args[0] == "backup":
                return 0, "", ""
            return 1, "", "unexpected"
        mock_run.side_effect = fake_run
        outcome = await p._sync_game({"name": "Game A", "paths": [local_file]}, "/mnt/nas/saves", {})
    check("no remote backup yet -> push", outcome, "pushed")

    # Scenario 2: local newer than remote -> push
    p = fresh_plugin()
    local_file = make_local_file()
    old_remote = "2020-01-01T00:00:00.000000000Z"
    with patch.object(main, "_run_ludusavi", new=AsyncMock()) as mock_run:
        async def fake_run(args):
            if args[0] == "backup":
                return 0, "", ""
            return 1, "", "unexpected"
        mock_run.side_effect = fake_run
        outcome = await p._sync_game(
            {"name": "Game B", "paths": [local_file]}, "/mnt/nas/saves", {"Game B": remote_entry(old_remote)}
        )
    check("local newer than remote -> push", outcome, "pushed")

    # Scenario 3: remote newer than local, not yet accounted for -> pull
    p = fresh_plugin()
    local_file = make_local_file(mtime_offset_seconds=-1000)  # old local file
    future_remote = "2099-01-01T00:00:00.000000000Z"
    with patch.object(main, "_run_ludusavi", new=AsyncMock()) as mock_run:
        async def fake_run(args):
            if args[0] == "restore":
                return 0, "", ""
            return 1, "", "unexpected"
        mock_run.side_effect = fake_run
        outcome = await p._sync_game(
            {"name": "Game C", "paths": [local_file]}, "/mnt/nas/saves", {"Game C": remote_entry(future_remote)}
        )
    check("remote newer, unaccounted -> pull", outcome, "pulled")
    check("state marker updated after pull", p.state["last_synced_remote"].get("Game C"), future_remote)

    # Scenario 4: remote newer than local, but ALREADY accounted for -> skip
    # (this is what stops a daemon from re-pulling its own earlier pull, or
    # another instance's already-seen push, forever)
    p = fresh_plugin()
    local_file = make_local_file(mtime_offset_seconds=-1000)
    p.state["last_synced_remote"]["Game D"] = future_remote
    with patch.object(main, "_run_ludusavi", new=AsyncMock()) as mock_run:
        mock_run.side_effect = AssertionError("should not push or pull in this scenario")
        outcome = await p._sync_game(
            {"name": "Game D", "paths": [local_file]}, "/mnt/nas/saves", {"Game D": remote_entry(future_remote)}
        )
    check("remote newer but already accounted for -> skip (no ping-pong)", outcome, "skipped")

    # Scenario 5: no local save data at all -> skip without even calling ludusavi
    p = fresh_plugin()
    with patch.object(main, "_run_ludusavi", new=AsyncMock()) as mock_run:
        mock_run.side_effect = AssertionError("should never be called for a game with no local files")
        outcome = await p._sync_game({"name": "Game E", "paths": ["/does/not/exist"]}, "/mnt/nas/saves", {})
    check("no local save data -> skip without calling ludusavi", outcome, "skipped")

    # Scenario 6: the actual batching fix - _latest_remote_all makes exactly
    # ONE ludusavi call no matter how many games are in the response, and
    # never passes a per-game name argument (that's what "all games in one
    # call" means at the CLI level - see main.py's docstring on this).
    p = fresh_plugin()
    call_count = 0
    with patch.object(main, "_run_ludusavi", new=AsyncMock()) as mock_run:
        async def fake_run(args):
            nonlocal call_count
            call_count += 1
            assert args == ["backups", "--api", "--path", "/mnt/nas/saves"], (
                f"expected exactly the batched 'all games' call with no name arg, got {args}"
            )
            return 0, json.dumps({
                "games": {
                    "Game F": {"backups": [{"when": "2025-01-01T00:00:00.000000000Z"}]},
                    "Game G": {"backups": [{"when": "2025-06-01T00:00:00.000000000Z"}]},
                }
            }), ""
        mock_run.side_effect = fake_run
        remote_map = await p._latest_remote_all("/mnt/nas/saves")
    check("_latest_remote_all makes exactly one call for many games", call_count, 1)
    check("_latest_remote_all returns every game", sorted(remote_map.keys()), ["Game F", "Game G"])

    for f in [local_file]:
        try:
            os.unlink(f)
        except OSError:
            pass


asyncio.run(run())

if failures:
    print(f"\n{len(failures)} FAILURE(S): {failures}")
    sys.exit(1)
print("\nall checks passed")
