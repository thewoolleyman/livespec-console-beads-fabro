"""The spec_governance default-block verifier wrapper maps results to exit codes."""

from __future__ import annotations

from collections.abc import Callable
from pathlib import Path
from types import ModuleType

import pytest

REPO_ROOT = Path(__file__).resolve().parents[2]


def test_main_passes_on_this_repos_livespec_jsonc(
    load_script: Callable[[str], ModuleType],
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    monkeypatch.chdir(REPO_ROOT)

    assert load_script("check-spec-governance-default-block.py").main() == 0
    assert capsys.readouterr().out.strip().startswith("{")


def test_main_fails_when_the_config_is_absent(
    load_script: Callable[[str], ModuleType],
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    monkeypatch.chdir(tmp_path)

    assert load_script("check-spec-governance-default-block.py").main() == 1
    assert capsys.readouterr().err.strip() != ""
