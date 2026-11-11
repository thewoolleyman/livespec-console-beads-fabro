"""Regression coverage for the check aggregate's purity inventory."""

from __future__ import annotations

import subprocess
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
