"""Shared fixtures: load the hyphen-named `dev-tooling/*.py` scripts as modules."""

from __future__ import annotations

import importlib.util
import sys
from collections.abc import Callable
from pathlib import Path
from types import ModuleType

import pytest

DEV_TOOLING = Path(__file__).resolve().parents[2] / "dev-tooling"

# The scripts import their sibling helper modules by bare name, exactly as they
# do when run as `python dev-tooling/<script>.py`.
if str(DEV_TOOLING) not in sys.path:
    sys.path.insert(0, str(DEV_TOOLING))


@pytest.fixture
def load_script() -> Callable[[str], ModuleType]:
    def load(filename: str) -> ModuleType:
        path = DEV_TOOLING / filename
        spec = importlib.util.spec_from_file_location(path.stem.replace("-", "_"), path)
        assert spec is not None
        assert spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    return load
