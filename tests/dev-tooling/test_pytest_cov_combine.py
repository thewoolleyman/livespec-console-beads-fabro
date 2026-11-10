"""A bare `--cov` pytest run combines under this repository's real pytest and coverage config."""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

PYPROJECT = Path(__file__).resolve().parents[2] / "pyproject.toml"
KEPT_SECTIONS = ("[tool.pytest.ini_options]", "[tool.coverage.")
COMBINE_FAILURE = "Can't combine statement coverage data with branch data"

HELPER_MODULE = """\
def double(value):
    if value < 0:
        return -value * 2
    return value * 2
"""

SUBPROCESS_TEST = """\
import importlib
import subprocess
import sys
from pathlib import Path

HELPER_DIR = Path(__file__).resolve().parents[2] / "dev-tooling"


def test_a_subprocess_with_no_pyproject_in_view_runs_the_helper(tmp_path):
    script = "import sys; sys.path.insert(0, sys.argv[1]); import helper; print(helper.double(2))"
    completed = subprocess.run(
        [sys.executable, "-c", script, str(HELPER_DIR)],
        cwd=tmp_path,
        capture_output=True,
        text=True,
        check=True,
    )
    assert completed.stdout.strip() == "4"


def test_the_helper_branches_are_covered_in_process():
    sys.path.insert(0, str(HELPER_DIR))
    helper = importlib.import_module("helper")

    assert helper.double(-1) == 2
    assert helper.double(2) == 4
"""


def real_config_sections() -> str:
    kept: list[str] = []
    keeping = False
    for line in PYPROJECT.read_text(encoding="utf-8").splitlines():
        if line.startswith("["):
            keeping = line.startswith(KEPT_SECTIONS)
        if keeping:
            kept.append(line)
    return "\n".join(kept) + "\n"


def build_throwaway_project(root: Path) -> None:
    (root / "dev-tooling").mkdir(parents=True)
    (root / "tests" / "dev-tooling").mkdir(parents=True)
    (root / "pyproject.toml").write_text(real_config_sections(), encoding="utf-8")
    (root / "dev-tooling" / "helper.py").write_text(HELPER_MODULE, encoding="utf-8")
    (root / "tests" / "dev-tooling" / "test_helper.py").write_text(
        SUBPROCESS_TEST, encoding="utf-8"
    )


def run_pytest(root: Path, *arguments: str) -> subprocess.CompletedProcess[str]:
    environment = {
        name: value
        for name, value in os.environ.items()
        if not name.startswith(("COV_CORE_", "COVERAGE_", "PYTEST_"))
    }
    return subprocess.run(
        [sys.executable, "-m", "pytest", *arguments],
        cwd=root,
        env=environment,
        capture_output=True,
        text=True,
        check=False,
    )


def test_a_bare_cov_run_ends_with_pytests_own_status_and_a_coverage_report(
    tmp_path: Path,
) -> None:
    build_throwaway_project(tmp_path)
    completed = run_pytest(tmp_path, "--cov=dev-tooling")
    output = completed.stdout + completed.stderr
    assert COMBINE_FAILURE not in output
    assert "INTERNALERROR" not in output
    assert "coverage:" in completed.stdout
    assert "helper.py" in completed.stdout
    assert completed.returncode == 0


def test_a_run_without_cov_collects_and_runs_the_same_tests(tmp_path: Path) -> None:
    build_throwaway_project(tmp_path)
    completed = run_pytest(tmp_path)
    assert "2 passed" in completed.stdout
    assert "coverage:" not in completed.stdout
    assert completed.returncode == 0
