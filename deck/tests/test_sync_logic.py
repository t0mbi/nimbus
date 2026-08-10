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


async def run():
    # Scenario 1: no remote backup exists yet -> push
    p = fresh_plugin()
    local_file = make_local_file()
    with patch.object(main, "_run_ludusavi", new=AsyncMock()) as mock_run:
        async def fake_run(args):
            if args[0] == "backups":
                return 0, json.dumps({"games": {}}), ""
            if args[0] == "backup":
                return 0, "", ""
            return 1, "", "unexpected"
        mock_run.side_effect = fake_run
        outcome = await p._sync_game({"name": "Game A", "paths": [local_file]}, "/mnt/nas/saves")
    check("no remote backup yet -> push", outcome, "pushed")

    # Scenario 2: local newer than remote -> push
    p = fresh_plugin()
    local_file = make_local_file()
    old_remote = "2020-01-01T00:00:00.000000000Z"
    with patch.object(main, "_run_ludusavi", new=AsyncMock()) as mock_run:
        async def fake_run(args):
            if args[0] == "backups":
                return 0, json.dumps({"games": {"Game B": {"backups": [{"when": old_remote}]}}}), ""
            if args[0] == "backup":
                return 0, "", ""
            return 1, "", "unexpected"
        mock_run.side_effect = fake_run
        outcome = await p._sync_game({"name": "Game B", "paths": [local_file]}, "/mnt/nas/saves")
    check("local newer than remote -> push", outcome, "pushed")

    # Scenario 3: remote newer than local, not yet accounted for -> pull
    p = fresh_plugin()
    local_file = make_local_file(mtime_offset_seconds=-1000)  # old local file
    future_remote = "2099-01-01T00:00:00.000000000Z"
    with patch.object(main, "_run_ludusavi", new=AsyncMock()) as mock_run:
        async def fake_run(args):
            if args[0] == "backups":
                return 0, json.dumps({"games": {"Game C": {"backups": [{"when": future_remote}]}}}), ""
            if args[0] == "restore":
                return 0, "", ""
            return 1, "", "unexpected"
        mock_run.side_effect = fake_run
        outcome = await p._sync_game({"name": "Game C", "paths": [local_file]}, "/mnt/nas/saves")
    check("remote newer, unaccounted -> pull", outcome, "pulled")
    check("state marker updated after pull", p.state["last_synced_remote"].get("Game C"), future_remote)

    # Scenario 4: remote newer than local, but ALREADY accounted for -> skip
    # (this is what stops a daemon from re-pulling its own earlier pull, or
    # another instance's already-seen push, forever)
    p = fresh_plugin()
    local_file = make_local_file(mtime_offset_seconds=-1000)
    p.state["last_synced_remote"]["Game D"] = future_remote
    with patch.object(main, "_run_ludusavi", new=AsyncMock()) as mock_run:
        async def fake_run(args):
            if args[0] == "backups":
                return 0, json.dumps({"games": {"Game D": {"backups": [{"when": future_remote}]}}}), ""
            return 1, "", "should not push or pull in this scenario"
        mock_run.side_effect = fake_run
        outcome = await p._sync_game({"name": "Game D", "paths": [local_file]}, "/mnt/nas/saves")
    check("remote newer but already accounted for -> skip (no ping-pong)", outcome, "skipped")

    # Scenario 5: no local save data at all -> skip without even calling ludusavi
    p = fresh_plugin()
    with patch.object(main, "_run_ludusavi", new=AsyncMock()) as mock_run:
        mock_run.side_effect = AssertionError("should never be called for a game with no local files")
        outcome = await p._sync_game({"name": "Game E", "paths": ["/does/not/exist"]}, "/mnt/nas/saves")
    check("no local save data -> skip without calling ludusavi", outcome, "skipped")

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
