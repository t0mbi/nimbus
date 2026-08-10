import sys
import os
import time

sys.path.insert(0, os.path.dirname(__file__))  # for the decky stub
sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))  # for main.py

import main  # noqa: E402

failures = []


def check(label, actual, expected):
    ok = actual == expected
    print(f"{'ok ' if ok else 'FAIL'}  {label}: {actual!r}" + ("" if ok else f" (expected {expected!r})"))
    if not ok:
        failures.append(label)


# Real capture: `2026-08-10T01:08:01.053222200Z` from ludusavi backups --api
epoch = main._parse_rfc3339("2026-08-10T01:08:01.053222200Z")
check(
    "parses a real 9-digit-fraction ludusavi timestamp",
    round(epoch, 3),
    round(1786324081.053222, 3),
)

epoch2 = main._parse_rfc3339("2026-08-10T01:08:01Z")
check("parses a timestamp with no fractional seconds at all", int(epoch2), int(epoch))

epoch3 = main._parse_rfc3339("2026-08-10T01:08:02.5Z")
check("a later timestamp parses as later", epoch3 > epoch, True)

# Sanity check the ordering property the whole sync decision depends on:
# lexicographic string comparison of the raw `when` values must agree with
# the parsed-epoch comparison, since `_latest_remote` picks the max via
# plain string max() on the raw strings elsewhere in the codebase's Windows
# counterpart - confirming that assumption isn't silently wrong in the
# Python port too.
a = "2026-08-10T01:08:01.053222200Z"
b = "2026-08-10T01:08:02.000000000Z"
check(
    "string ordering agrees with parsed-epoch ordering",
    (a < b) == (main._parse_rfc3339(a) < main._parse_rfc3339(b)),
    True,
)

if failures:
    print(f"\n{len(failures)} FAILURE(S): {failures}")
    sys.exit(1)
print(f"\nall checks passed")
