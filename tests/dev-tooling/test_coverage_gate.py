"""The llvm-cov coverage gate over synthetic exports and missing-line listings."""

from __future__ import annotations

import json
from collections.abc import Callable
from pathlib import Path
from types import ModuleType

import pytest

SKIPPED = 2
CODE = 0


def region(
    start_line: int, end_line: int, count: int, *, file_id: int = 0, kind: int = CODE
) -> list[int]:
    return [start_line, 1, end_line, 2, count, file_id, 0, kind]


def export(
    *regions: list[int],
    files: tuple[str, ...] = ("src/a.rs",),
    filenames: tuple[str, ...] = ("src/a.rs",),
    totals: dict[str, object] | None = None,
) -> dict[str, object]:
    return {
        "data": [
            {
                "files": [{"filename": name} for name in files],
                "functions": [{"filenames": list(filenames), "regions": list(regions)}],
                "totals": {"lines": {"count": 4, "covered": 3}} if totals is None else totals,
            }
        ]
    }


@pytest.fixture
def gate(load_script: Callable[[str], ModuleType]) -> ModuleType:
    return load_script("coverage-gate.py")


def run_gate(
    gate: ModuleType,
    tmp_path: Path,
    document: object,
    listing: str = "",
) -> int:
    export_path = tmp_path / "export.json"
    export_path.write_text(json.dumps(document), encoding="utf-8")
    listing_path = tmp_path / "missing.txt"
    listing_path.write_text(listing, encoding="utf-8")
    return gate.main(["coverage-gate.py", str(export_path), str(listing_path)])


def test_passes_when_every_line_and_region_is_covered(
    gate: ModuleType, tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    assert run_gate(gate, tmp_path, export(region(1, 2, 3))) == 0

    out = capsys.readouterr().out
    assert "0 of 2 lines uncovered" in out
    assert "0 uncovered regions" in out


def test_a_wrong_argument_count_is_a_usage_failure(
    gate: ModuleType, capsys: pytest.CaptureFixture[str]
) -> None:
    assert gate.main(["coverage-gate.py"]) == 1
    assert "usage: coverage-gate.py" in capsys.readouterr().err


def test_an_unreadable_export_fails_closed(
    gate: ModuleType, tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    listing = tmp_path / "missing.txt"
    listing.write_text("", encoding="utf-8")

    assert gate.main(["coverage-gate.py", str(tmp_path / "absent.json"), str(listing)]) == 1
    assert "could not read the llvm-cov export" in capsys.readouterr().err


def test_a_malformed_export_fails_closed(
    gate: ModuleType, tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    export_path = tmp_path / "export.json"
    export_path.write_text("{not json", encoding="utf-8")
    listing = tmp_path / "missing.txt"
    listing.write_text("", encoding="utf-8")

    assert gate.main(["coverage-gate.py", str(export_path), str(listing)]) == 1
    assert "cannot read llvm-cov export" in capsys.readouterr().err


def test_an_unreadable_listing_fails_closed(
    gate: ModuleType, tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    export_path = tmp_path / "export.json"
    export_path.write_text(json.dumps(export(region(1, 1, 1))), encoding="utf-8")

    assert gate.main(["coverage-gate.py", str(export_path), str(tmp_path / "absent.txt")]) == 1
    err = capsys.readouterr().err
    assert "cannot read missing-lines listing" in err
    assert "could not parse the missing-lines listing" in err


def test_lines_named_by_the_listing_fail_the_gate(
    gate: ModuleType, tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    listing = "Filename Regions\nUncovered Lines:\n: src/a.rs: 3, 4\nnoise without a match\n"

    assert run_gate(gate, tmp_path, export(region(1, 2, 3)), listing) == 1
    err = capsys.readouterr().err
    assert "src/a.rs: 2 line(s)" in err
    assert "2 uncovered line(s) named by the listing" in err


def test_a_listing_without_the_uncovered_header_names_nothing(
    gate: ModuleType, tmp_path: Path
) -> None:
    assert run_gate(gate, tmp_path, export(region(1, 2, 3)), "all covered\n") == 0


def test_an_export_with_no_attributed_lines_fails_closed(
    gate: ModuleType, tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    assert run_gate(gate, tmp_path, export()) == 1
    assert "attributes no lines to any file" in capsys.readouterr().err


def test_a_line_no_instantiation_executes_fails_the_gate(
    gate: ModuleType, tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    assert run_gate(gate, tmp_path, export(region(1, 1, 0))) == 1
    err = capsys.readouterr().err
    assert "src/a.rs:1 — 0 executions in the merged per-line view" in err
    assert "1 uncovered line(s) in the merged per-line view" in err


def test_a_line_covered_by_any_instantiation_counts_as_covered(
    gate: ModuleType, tmp_path: Path
) -> None:
    assert run_gate(gate, tmp_path, export(region(1, 1, 0), region(1, 1, 5))) == 0


def test_a_region_no_instantiation_executes_fails_the_gate(
    gate: ModuleType, tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    covered_line_dead_region = export(region(1, 3, 4), [2, 5, 2, 9, 0, 0, 0, CODE])

    assert run_gate(gate, tmp_path, covered_line_dead_region) == 1
    captured = capsys.readouterr()
    assert "0 uncovered regions" not in captured.out
    assert "src/a.rs:2:5-2:9 — 0 executions in the merged reachable-region view" in captured.err
    assert "1 uncovered region(s)" in captured.err


def test_skipped_and_unattributed_regions_are_ignored(gate: ModuleType, tmp_path: Path) -> None:
    document = export(
        region(1, 2, 1),
        region(5, 6, 0, kind=SKIPPED),
        region(7, 8, 0, file_id=1),
        region(9, 10, 0, file_id=2),
        files=("src/a.rs",),
        filenames=("src/a.rs", "src/tests.rs"),
    )

    assert run_gate(gate, tmp_path, document) == 0


def test_the_summary_figure_is_reported_for_comparison_only(
    gate: ModuleType, tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    document = export(region(1, 1, 1), totals={"lines": {"count": 10, "covered": 7}})

    assert run_gate(gate, tmp_path, document) == 0
    assert "summary counts 3" in capsys.readouterr().out


def test_a_summary_without_line_totals_counts_nothing_missed(
    gate: ModuleType, tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    assert run_gate(gate, tmp_path, export(region(1, 1, 1), totals={})) == 0
    assert "summary counts 0" in capsys.readouterr().out


def test_an_export_without_data_is_not_attributable(gate: ModuleType, tmp_path: Path) -> None:
    assert run_gate(gate, tmp_path, {}) == 1
