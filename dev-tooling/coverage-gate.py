#!/usr/bin/env python3
"""Line- AND region-coverage gate with ONE narrowly-recorded, now-EXPLAINED disposition.

Two enforcement points over one instrumented llvm-cov run:

  * LINE coverage — replaces a bare `--fail-under-lines 100` with the same
    requirement PLUS an explicit accounting of a signature that flag cannot
    express (below).
  * REGION coverage — the `coverage-region-gate` obligation ratified in
    SPECIFICATION/non-functional-requirements.md v044: 0 uncovered regions in the
    MERGED, cross-instantiation reachable-region view (a region is covered iff its
    max execution count across instantiations is non-zero), NOT the raw
    `--workspace --lib` summary region count. The summary over-reports uncovered
    regions by an llvm-cov cross-object merge artifact — the same
    `CoverageInfo::merge` scalar-max arithmetic explained below for lines — so a
    bare `--fail-under-regions 100` would fail on a residue that corresponds to no
    uncovered source region. See `_merged_region_uncovered` / `_region_gate`. No
    coverage exclusions are permitted: the merged view is a MEASUREMENT correction,
    not an exemption.

The line requirement, unchanged:

The requirement is unchanged for every line llvm-cov can attribute:

    nameable missed lines  ->  MUST be zero.   Any one of them fails the gate.
    unnameable missed      ->  must be EXPLAINED by the instantiation-group model
                               below, capped by the recorded disposition fixture,
                               and REPORTED LOUDLY on every run.

WHY THE SUMMARY CAN COUNT A LINE NO LISTING SURFACE HAS
-------------------------------------------------------
llvm-cov's file line summary is NOT a per-line set operation over the merged
coverage view every listing surface renders. It is a SUM OVER INSTANTIATION
GROUPS (functions sharing a source start location) of that group's own
`(NumLines, Covered)` pair, where a group merges its instantiations with
`CoverageInfo::merge`, taking the SCALAR MAXIMUM of each field INDEPENDENTLY.

Two consequences, both measured in this workspace and both reproduced exactly by
the model in `_attribute` below:

  * A line belonging to two groups (a closure and its enclosing function) is
    counted TWICE by the sum but once by any listing.
  * A group whose instantiations cover DIFFERENT subsets of its lines reports
    `covered = max(covered_i)` rather than `|union of covered lines|`. With
    instantiations at (12 lines, 11 covered) and (12 lines, 10 covered) the
    group reports 12/11 — one missed line — while the union covers all twelve.

That residue is an arithmetic artifact. It corresponds to no source line, which
is exactly why no listing surface can name one, and it cannot be covered by a
test. What the gate CAN require is that every such line be attributed to the
group that produced it, and that is now enforced: an unexplained excess fails.

This is deliberately fail-closed: a parse failure, a missing fixture, an empty
reason, an unnameable count above the allowance, or an unnameable count the
model cannot account for all fail. See
`tests/fixtures/coverage-unnameable-disposition.json` and ledger item
`livespec-console-beads-fabro-3yx`.
"""

from __future__ import annotations

import json
import re
import sys
from collections import defaultdict
from pathlib import Path

# llvm's CounterMappingRegion::SkippedRegion. Skipped regions carry no counter
# and are excluded from a function's own line accounting.
_SKIPPED_REGION = 2

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


def _group_line_stats(record: dict, file_index: int) -> tuple[int, int]:
    """One instantiation's own (lines, covered), the way llvm-cov counts them."""
    counts: dict[int, int] = {}
    for start_line, _sc, end_line, _ec, count, file_id, _expanded, kind in record["regions"]:
        if file_id != file_index or kind == _SKIPPED_REGION:
            continue
        for line in range(start_line, end_line + 1):
            counts[line] = max(counts.get(line, 0), count)
    return len(counts), sum(1 for count in counts.values() if count)


def _attribute(export_path: Path) -> tuple[int, list[str]] | None:
    """Reproduce llvm-cov's summed line totals and name what makes them differ.

    Returns `(modelled_missed, rows)`, where `rows` describes every instantiation
    group whose scalar-merged accounting reports a missed line. Returns None when
    the model does NOT reproduce llvm-cov's own totals exactly — an explicit
    "I cannot attribute this" that the caller must treat as fail-closed.
    """
    try:
        doc = json.loads(export_path.read_text())
    except (OSError, ValueError) as error:
        print(f"coverage-gate: cannot read llvm-cov export: {error}", file=sys.stderr)
        return None

    modelled_missed = 0
    rows: list[str] = []
    for data in doc.get("data", []):
        totals = (data.get("totals") or {}).get("lines")
        if totals is None:
            return None

        by_file: dict[str, list[dict]] = defaultdict(list)
        for record in data.get("functions", []):
            for filename in record["filenames"]:
                by_file[filename].append(record)

        model_count = model_covered = 0
        for entry in data.get("files", []):
            filename = entry["filename"]
            groups: dict[tuple[int, int], list[tuple[dict, int]]] = defaultdict(list)
            for record in by_file.get(filename, []):
                file_index = record["filenames"].index(filename)
                starts = [(r[0], r[1]) for r in record["regions"] if r[5] == file_index]
                if starts:
                    groups[min(starts)].append((record, file_index))

            for (line, column), instantiations in groups.items():
                stats = [
                    (*_group_line_stats(record, file_index), record["name"])
                    for record, file_index in instantiations
                ]
                # llvm's CoverageInfo::merge — independent scalar maxima.
                group_lines = max(s[0] for s in stats)
                group_covered = max(s[1] for s in stats)
                model_count += group_lines
                model_covered += group_covered
                if group_lines == group_covered:
                    continue
                rows.append(
                    f"  {filename}:{line}:{column} — instantiation group reports "
                    f"{group_lines} line(s), {group_covered} covered "
                    f"({group_lines - group_covered} missed)"
                )
                for lines, covered, name in stats:
                    rows.append(f"      instantiation: {lines} line(s), {covered} covered — {name}")

        if (model_count, model_covered) != (int(totals["count"]), int(totals["covered"])):
            print(
                "coverage-gate: the instantiation-group model reproduces "
                f"{model_count}/{model_covered} lines but llvm-cov reports "
                f"{totals['count']}/{totals['covered']}",
                file=sys.stderr,
            )
            return None
        modelled_missed += model_count - model_covered

    return modelled_missed, rows


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


def _merged_region_uncovered(export_path: Path) -> list[str] | None:
    """Regions uncovered in the MERGED, cross-instantiation reachable-region view.

    Per SPECIFICATION/non-functional-requirements.md (ratified v044): the region
    target is the MERGED reachable-region view, NOT the raw `--workspace --lib`
    summary region count. A source region is COVERED iff its MAXIMUM execution
    count across all instantiations is non-zero. The merged view over-rides the
    summary's per-instantiation-group scalar-max SUM, which over-reports uncovered
    regions by an llvm-cov cross-object merge artifact — the same
    `CoverageInfo::merge` arithmetic the line gate above accounts for, applied to
    regions: a non-generic library item compiled into several crates' `--lib` test
    binaries is counted uncovered in the objects that link but never call it, even
    when every reachable region is exercised.

    Only regions in files llvm-cov itself ATTRIBUTES to that file — the files
    present in `data["files"]` — are considered. A `#[cfg(test)]` module compiled
    into a SEPARATE file (e.g. `src/tests.rs`) appears in `functions[]` but NOT in
    `files[]`, and llvm-cov excludes it from the region totals; counting it would
    gate on test-helper branches llvm-cov does not, and would not reconcile with the
    summary (measured: including it invented 50 phantom misses the summary of 2
    cannot contain).

    Returns the list of genuinely-uncovered source regions (empty when the merged
    view is clean), or None on a parse failure (fail-closed).
    """
    try:
        doc = json.loads(export_path.read_text())
    except (OSError, ValueError) as error:
        print(f"coverage-gate: cannot read llvm-cov export: {error}", file=sys.stderr)
        return None

    uncovered: list[str] = []
    for data in doc.get("data", []):
        attributed = {entry["filename"] for entry in data.get("files", [])}
        region_max: dict[tuple[str, int, int, int, int], int] = {}
        for record in data.get("functions", []):
            filenames = record["filenames"]
            for start_line, start_col, end_line, end_col, count, file_id, _exp, kind in record[
                "regions"
            ]:
                if kind == _SKIPPED_REGION or file_id >= len(filenames):
                    continue
                filename = filenames[file_id]
                if filename not in attributed:
                    continue
                key = (filename, start_line, start_col, end_line, end_col)
                region_max[key] = max(region_max.get(key, 0), count)
        for (filename, start_line, start_col, end_line, end_col), count in sorted(
            region_max.items()
        ):
            if count == 0:
                uncovered.append(
                    f"  {filename}:{start_line}:{start_col}-{end_line}:{end_col} "
                    "— 0 executions in the merged reachable-region view"
                )
    return uncovered


def _region_gate(export_path: Path) -> int:
    """Assert 0 uncovered regions in the merged reachable-region view (v044)."""
    uncovered = _merged_region_uncovered(export_path)
    if uncovered is None:
        return _fail("could not compute the merged reachable-region view; refusing to pass")
    if uncovered:
        for row in uncovered:
            print(row, file=sys.stderr)
        return _fail(
            f"{len(uncovered)} uncovered region(s) in the merged reachable-region view. "
            "Every reachable region MUST be exercised: cover it with a test (for a `?` "
            "error arm, drive the failing branch), or restructure genuinely-uncoverable "
            "code. No coverage exclusions are permitted. This is the coverage-region-gate "
            "obligation (SPECIFICATION/non-functional-requirements.md, v044)."
        )
    print(
        "coverage-gate: PASS — 0 uncovered regions in the merged reachable-region view "
        "(every region's max execution count across instantiations is non-zero). Any "
        "residual in the raw --workspace --lib summary is a cross-object merge artifact; "
        "the merged view is the ratified metric (v044)."
    )
    return 0


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
            "cover these — cover them with tests. For a grouped or-pattern arm, "
            "exercise the untaken alternative; never split the arm or relax "
            "clippy `match_same_arms`."
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

    if not unnameable:
        print("coverage-gate: PASS — 0 missed lines, nothing dispositioned.")
        return _region_gate(export_path)

    # 3. An unnameable miss passes only if the model can say WHERE it came from.
    attributed = _attribute(export_path)
    if attributed is None:
        return _fail(
            f"{unnameable} unnameable missed line(s) could NOT be attributed to an "
            "instantiation group. The recorded disposition covers only the explained "
            "scalar-merge artifact; an unexplained excess is a new signature — "
            "re-measure and take it to the maintainer."
        )
    modelled, rows = attributed
    if modelled != unnameable:
        return _fail(
            f"the model accounts for {modelled} missed line(s) but the summary counts "
            f"{unnameable} beyond the listing. Attribution is incomplete; refusing to pass."
        )

    print(
        f"coverage-gate: PASS with {unnameable} DISPOSITIONED unnameable missed "
        f"line(s) (allowance {allowance}, tracking "
        f"{fixture.get('tracking_item', 'unrecorded')}). Nameable misses: 0. "
        "The 100% requirement for attributable lines is UNCHANGED."
    )
    print(
        "coverage-gate: attributed to llvm-cov's scalar-maximum merge over "
        "instantiation groups — these correspond to no uncovered source line:"
    )
    for row in rows:
        print(row)
    return _region_gate(export_path)


if __name__ == "__main__":
    sys.exit(main(sys.argv))
