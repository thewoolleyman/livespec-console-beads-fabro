"""Regression coverage for the check aggregate's purity inventory."""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
CHECK_TARGETS = (
    "check-format",
    "check-clippy",
    "check-nextest",
    "check-coverage",
    "check-deps",
    "check-arch",
    "check-behavior-coverage",
    "check-heading-coverage-debt-register",
    "check-spec-fenced-block-staleness",
    "check-completeness",
    "check-spec-governance-default-block",
    "check-charters",
    "check-baseline",
    "check-shell-quality",
    "check-no-workflow-edits",
    "check-plan-no-tombstone",
    "check-plan-record-conformance",
    "check-plan-anchor-declared",
    "check-plugin-resolution",
    "check-doctor-static",
    "check-e2e-tmux",
    "check-real-store-smoke",
    "check-ci-parity",
    "check-docs-only-ci",
    "check-fork-drift",
    "check-red-green-replay",
)


def test_every_check_aggregate_member_declares_its_purity() -> None:
    """No aggregate member may silently become a world-reading default."""
    completed = subprocess.run(
        [
            "uv",
            "run",
            "python",
            "-m",
            "livespec_dev_tooling.green_token",
            "undeclared",
            *CHECK_TARGETS,
        ],
        capture_output=True,
        check=False,
        cwd=REPOSITORY_ROOT,
        text=True,
    )

    assert completed.returncode == 0, completed.stdout + completed.stderr


def test_check_aggregate_refuses_an_undeclared_member(*, tmp_path: Path) -> None:
    """The aggregate stops before it can issue a partial green result."""
    (tmp_path / "justfile").write_text(
        (REPOSITORY_ROOT / "justfile").read_text(encoding="utf-8"), encoding="utf-8"
    )
    inventory = (REPOSITORY_ROOT / "check-targets.txt").read_text(encoding="utf-8")
    (tmp_path / "check-targets.txt").write_text(
        inventory.replace("check-format  # purity: world-reading", "check-format", 1),
        encoding="utf-8",
    )
    shim = tmp_path / "bin" / "uv"
    shim.parent.mkdir()
    shim.write_text(
        f"#!/bin/sh\n"
        "test \"$1\" = run && shift\n"
        "test \"$1\" = python && shift\n"
        f"exec {sys.executable} \"$@\"\n",
        encoding="utf-8",
    )
    shim.chmod(0o755)

    completed = subprocess.run(
        ["just", "check"],
        capture_output=True,
        check=False,
        cwd=tmp_path,
        env={**os.environ, "PATH": f"{shim.parent}:{os.environ['PATH']}"},
        text=True,
    )

    assert completed.returncode != 0
    assert "check-format" in completed.stdout
    assert "check aggregate member carries no purity declaration" in completed.stderr
