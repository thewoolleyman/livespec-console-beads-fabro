#!/usr/bin/env python3
"""Line- AND region-coverage gate over the MERGED, cross-instantiation view. No allowance.

Two enforcement points over one instrumented llvm-cov run, both requiring ZERO:

  * LINE coverage — 0 uncovered lines in the merged per-line view: a source line
    is covered iff ANY instantiation of ANY function spanning it executes it.
    See `_merged_line_uncovered`.
  * REGION coverage — the `coverage-region-gate` obligation ratified in
    SPECIFICATION/non-functional-requirements.md v044: 0 uncovered regions in the
    merged reachable-region view (a region is covered iff its max execution
    count across instantiations is non-zero). See `_merged_region_uncovered`.

There is no allowance, no disposition fixture, and no exclusion. Maintainer
ruling 2026-09-10: "There is no such thing as uncoverable lines of code. If a
line of code cannot be executed by tests, then it cannot be executed by the
application. It has no use existing."

WHY NOT llvm-cov's OWN LINE SUMMARY
-----------------------------------
llvm-cov's file line summary is not a per-line set operation. It is a SUM OVER
INSTANTIATION GROUPS (functions sharing a source start location) of each group's
own `(NumLines, Covered)` pair, and a group merges its instantiations with
`CoverageInfo::merge`, taking the SCALAR MAXIMUM of each field INDEPENDENTLY.
Instantiations covering 16 and 15 of a function's 17 lines report 17/16 — one
"missed" line — while their union covers all 17. That residue names no source
line, which is why no listing surface (`--show-missing-lines`, `--text`, lcov)
ever showed one (ledger item livespec-console-beads-fabro-3yx). Measuring the
per-line union directly removes the artifact from the METRIC, so a real
uncovered line can no longer hide inside a tolerance for a fake one. The raw
summary figure is still printed, for comparison only.

Fail-closed: a parse failure, or an export carrying no attributable lines, fails.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

# llvm's CounterMappingRegion::SkippedRegion. Skipped regions carry no counter
# and are excluded from line and region accounting alike.
_SKIPPED_REGION = 2

# `<path>: 12, 34, 56` under the listing's "Uncovered Lines:" header.
_MISSING_LINE = re.compile(r"^(?P<path>[^:]+):\s*(?P<lines>\d+(?:\s*,\s*\d+)*)\s*$")


def _fail(message: str) -> int:
    print(f"coverage-gate: FAIL — {message}", file=sys.stderr)
    return 1


def _load(export_path: Path) -> dict | None:
    try:
        return json.loads(export_path.read_text())
    except (OSError, ValueError) as error:
        print(f"coverage-gate: cannot read llvm-cov export: {error}", file=sys.stderr)
        return None


def _summary_missed(doc: dict) -> int:
    """Missed lines by llvm-cov's own scalar-merged summary. Reported, never gated."""
    missed = 0
    for data in doc.get("data", []):
        lines = (data.get("totals") or {}).get("lines")
        if lines is not None:
            missed += int(lines["count"]) - int(lines["covered"])
    return missed


def _attributed_regions(doc: dict):
    """Yield `(filename, region)` for every counted region in a file llvm-cov attributes.

    Only files present in `data["files"]` count. A `#[cfg(test)]` module compiled
    into a SEPARATE file (e.g. `src/tests.rs`) appears in `functions[]` but NOT in
    `files[]`, and llvm-cov excludes it from its totals; counting it would gate on
    test-helper code llvm-cov does not (measured: 50 phantom region misses).
    """
    for data in doc.get("data", []):
        attributed = {entry["filename"] for entry in data.get("files", [])}
        for record in data.get("functions", []):
            filenames = record["filenames"]
            for region in record["regions"]:
                file_id, kind = region[5], region[7]
                if kind == _SKIPPED_REGION or file_id >= len(filenames):
                    continue
                if filenames[file_id] in attributed:
                    yield filenames[file_id], region


def _merged_line_uncovered(doc: dict) -> tuple[int, list[str]]:
    """`(lines counted, uncovered rows)` in the merged per-line view.

    Per instantiation, a line's count is the max over the regions spanning it —
    the same per-function accounting llvm-cov's summary uses. Across
    instantiations and across groups, the line takes the max again: covered iff
    anything that spans it ran.
    """
    line_max: dict[tuple[str, int], int] = {}
    for filename, (start_line, _sc, end_line, _ec, count, *_rest) in _attributed_regions(doc):
        for line in range(start_line, end_line + 1):
            key = (filename, line)
            line_max[key] = max(line_max.get(key, 0), count)
    uncovered = [
        f"  {filename}:{line} — 0 executions in the merged per-line view"
        for (filename, line), count in sorted(line_max.items())
        if count == 0
    ]
    return len(line_max), uncovered


def _merged_region_uncovered(doc: dict) -> list[str]:
    """Regions uncovered in the merged reachable-region view (ratified v044).

    A source region is COVERED iff its MAXIMUM execution count across all
    instantiations is non-zero. This overrides the raw `--workspace --lib`
    summary region count, which over-reports by the same `CoverageInfo::merge`
    arithmetic described in the module docstring: a non-generic library item
    compiled into several crates' `--lib` test binaries is counted uncovered in
    the objects that link but never call it.
    """
    region_max: dict[tuple[str, int, int, int, int], int] = {}
    for filename, (start_line, start_col, end_line, end_col, count, *_rest) in _attributed_regions(
        doc
    ):
        key = (filename, start_line, start_col, end_line, end_col)
        region_max[key] = max(region_max.get(key, 0), count)
    return [
        f"  {filename}:{start_line}:{start_col}-{end_line}:{end_col} "
        "— 0 executions in the merged reachable-region view"
        for (filename, start_line, start_col, end_line, end_col), count in sorted(
            region_max.items()
        )
        if count == 0
    ]


def _nameable(listing_path: Path) -> tuple[int, list[str]] | None:
    """Missed lines the `--show-missing-lines` listing attributes to a source line."""
    try:
        text = listing_path.read_text()
    except OSError as error:
        print(f"coverage-gate: cannot read missing-lines listing: {error}", file=sys.stderr)
        return None
    if "Uncovered Lines" not in text:
        return 0, []
    body = text.split("Uncovered Lines", 1)[1]
    count = 0
    rows: list[str] = []
    for raw in body.splitlines():
        row = raw.strip().lstrip(":").strip()
        match = _MISSING_LINE.match(row) if row else None
        if match is None:
            continue
        numbers = [n for n in match.group("lines").split(",") if n.strip().isdigit()]
        count += len(numbers)
        rows.append(f"  {match.group('path').strip()}: {len(numbers)} line(s)")
    return count, rows


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        return _fail("usage: coverage-gate.py <llvm-cov-json> <missing-lines-txt>")
    export_path, listing_path = (Path(a) for a in argv[1:])

    doc = _load(export_path)
    if doc is None:
        return _fail("could not read the llvm-cov export; refusing to pass")

    parsed = _nameable(listing_path)
    if parsed is None:
        return _fail("could not parse the missing-lines listing; refusing to pass")
    nameable, rows = parsed
    if nameable > 0:
        for row in rows:
            print(row, file=sys.stderr)
        return _fail(
            f"{nameable} uncovered line(s) named by the listing. Cover them with tests. "
            "For a grouped or-pattern arm, exercise the untaken alternative; never split "
            "the arm or relax clippy `match_same_arms`."
        )

    counted, uncovered = _merged_line_uncovered(doc)
    if counted == 0:
        return _fail("llvm-cov export attributes no lines to any file; refusing to pass")
    if uncovered:
        for row in uncovered:
            print(row, file=sys.stderr)
        return _fail(
            f"{len(uncovered)} uncovered line(s) in the merged per-line view. A line no "
            "test executes is a line the application cannot execute either: cover it or "
            "delete it. There is no allowance."
        )
    print(
        f"coverage-gate: PASS — 0 of {counted} lines uncovered in the merged per-line view "
        f"(llvm-cov's scalar-merged summary counts {_summary_missed(doc)}; that figure is "
        "an instantiation-group arithmetic artifact and is not the metric)."
    )

    uncovered_regions = _merged_region_uncovered(doc)
    if uncovered_regions:
        for row in uncovered_regions:
            print(row, file=sys.stderr)
        return _fail(
            f"{len(uncovered_regions)} uncovered region(s) in the merged reachable-region "
            "view. Every reachable region MUST be exercised: cover it with a test (for a "
            "`?` error arm, drive the failing branch), or restructure the code so the "
            "region does not exist. No coverage exclusions are permitted. This is the "
            "coverage-region-gate obligation (SPECIFICATION/non-functional-requirements.md, "
            "v044)."
        )
    print("coverage-gate: PASS — 0 uncovered regions in the merged reachable-region view.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
