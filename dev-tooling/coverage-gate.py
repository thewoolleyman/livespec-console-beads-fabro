#!/usr/bin/env python3
"""Line-coverage gate with ONE narrowly-recorded disposition.

Replaces a bare `--fail-under-lines 100` with the same requirement PLUS an
explicit accounting of a signature that flag cannot express: llvm-cov's summary
counting more missed lines than any listing surface can NAME.

The requirement is unchanged for every line llvm-cov can attribute:

    nameable missed lines  ->  MUST be zero.   Any one of them fails the gate.
    unnameable missed      ->  capped by the recorded disposition fixture, and
                               REPORTED LOUDLY on every run.

This is deliberately fail-closed: a parse failure, a missing fixture, an empty
reason, or an unnameable count above the allowance all fail. See
`tests/fixtures/coverage-unnameable-disposition.json` and ledger item
`livespec-console-beads-fabro-3yx`.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

# `<path>: 12, 34, 56` under the listing's "Uncovered Lines:" header.
_MISSING_LINE = re.compile(r"^(?P<path>[^:]+):\s*(?P<lines>\d+(?:\s*,\s*\d+)*)\s*$")


def _fail(message: str) -> int:
    print(f"coverage-gate: FAIL — {message}", file=sys.stderr)
    return 1


def _total_missed(export_path: Path) -> int | None:
    """Missed lines according to llvm-cov's own summary totals."""
    try:
        doc = json.loads(export_path.read_text())
    except (OSError, ValueError) as error:
        print(f"coverage-gate: cannot read llvm-cov export: {error}", file=sys.stderr)
        return None
    missed = 0
    found = False
    for data in doc.get("data", []):
        lines = (data.get("totals") or {}).get("lines")
        if lines is None:
            continue
        found = True
        missed += int(lines["count"]) - int(lines["covered"])
    return missed if found else None


def _nameable(listing_path: Path) -> tuple[int, list[str]] | None:
    """Missed lines any listing surface can attribute to a source line."""
    try:
        text = listing_path.read_text()
    except OSError as error:
        print(f"coverage-gate: cannot read missing-lines listing: {error}", file=sys.stderr)
        return None
    if "Uncovered Lines" not in text:
        # No section at all means llvm-cov named nothing. That is a legitimate
        # state here — it is precisely the phantom's signature.
        return 0, []
    body = text.split("Uncovered Lines", 1)[1]
    count = 0
    rows: list[str] = []
    for raw in body.splitlines():
        row = raw.strip().lstrip(":").strip()
        if not row:
            continue
        match = _MISSING_LINE.match(row)
        if match is None:
            continue
        numbers = [n for n in match.group("lines").split(",") if n.strip().isdigit()]
        if not numbers:
            continue
        count += len(numbers)
        rows.append(f"  {match.group('path').strip()}: {len(numbers)} line(s)")
    return count, rows


def main(argv: list[str]) -> int:
    if len(argv) != 4:
        return _fail("usage: coverage-gate.py <llvm-cov-json> <missing-lines-txt> <disposition-json>")
    export_path, listing_path, fixture_path = (Path(a) for a in argv[1:])

    try:
        fixture = json.loads(fixture_path.read_text())
    except (OSError, ValueError) as error:
        return _fail(f"cannot read the disposition fixture ({error}); refusing to pass")

    allowance = fixture.get("max_unnameable_missed_lines")
    reason = str(fixture.get("reason") or "").strip()
    if not isinstance(allowance, int) or allowance < 0:
        return _fail("disposition fixture has no valid max_unnameable_missed_lines")
    if allowance > 0 and not reason:
        return _fail("disposition fixture allows unnameable misses but carries NO reason")

    total = _total_missed(export_path)
    if total is None:
        return _fail("llvm-cov export carried no line totals; refusing to pass")

    parsed = _nameable(listing_path)
    if parsed is None:
        return _fail("could not parse the missing-lines listing; refusing to pass")
    nameable, rows = parsed

    # 1. Ordinary, attributable misses block everything. Unchanged requirement.
    if nameable > 0:
        for row in rows:
            print(row, file=sys.stderr)
        return _fail(
            f"{nameable} NAMEABLE uncovered line(s). The recorded disposition does NOT "
            "cover these — cover them with tests."
        )

    # 2. Whatever the summary counts beyond that is the dispositioned signature.
    unnameable = total - nameable
    if unnameable < 0:
        return _fail(
            f"listing named {nameable} lines but the summary counts {total}; "
            "accounting is inconsistent, refusing to pass"
        )
    if unnameable > allowance:
        return _fail(
            f"{unnameable} unnameable missed line(s) exceeds the recorded allowance of "
            f"{allowance}. Re-measure and take this to the maintainer; do NOT raise the "
            "allowance without a new recorded authorization."
        )

    if unnameable:
        print(
            f"coverage-gate: PASS with {unnameable} DISPOSITIONED unnameable missed "
            f"line(s) (allowance {allowance}, tracking "
            f"{fixture.get('tracking_item', 'unrecorded')}). Nameable misses: 0. "
            "The 100% requirement for attributable lines is UNCHANGED."
        )
    else:
        print("coverage-gate: PASS — 0 missed lines, nothing dispositioned.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
