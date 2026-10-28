"""Throwaway-repository fixtures for the documentation-only CI classifier acceptance."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from collections.abc import Callable
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CLASSIFIER = ROOT / ".github/scripts/classify-ci-changes.sh"


def run(
    *args: str, cwd: Path, check: bool = True, env: dict[str, str] | None = None
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args,
        cwd=cwd,
        check=check,
        env=env,
        text=True,
        capture_output=True,
    )


class Repo:
    def __init__(self) -> None:
        self.root = Path(tempfile.mkdtemp(prefix="docs-only-ci-"))
        self.extra_roots: list[Path] = []
        run("git", "init", "-q", "-b", "master", cwd=self.root)
        run("git", "config", "user.name", "CI Test", cwd=self.root)
        run("git", "config", "user.email", "ci@example.invalid", cwd=self.root)
        self.write("CHANGELOG.md", "before\n")
        self.write(".ai/note.md", "before\n")
        self.write("plan/topic/handoff.md", "before\n")
        self.write("docs/doc-custody.md", "before\n")
        self.write("docs/factory-confirmations.md", "before\n")
        self.write("README.md", "before\n")
        self.write("docs/cli-options.md", "before\n")
        self.write("SPECIFICATION/spec.md", "before\n")
        self.write("src/lib.rs", "before\n")
        self.write(".github/workflows/ci.yml", "before\n")
        self.write(".github/scripts/classify-ci-changes.sh", "before\n")
        self.commit("base")
        self.base = self.sha()

    def close(self) -> None:
        shutil.rmtree(self.root)
        for extra_root in self.extra_roots:
            shutil.rmtree(extra_root)

    def write(self, relative: str, content: str) -> Path:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
        return path

    def commit(self, message: str) -> None:
        run("git", "add", "-A", cwd=self.root)
        run("git", "commit", "-q", "-m", message, cwd=self.root)

    def sha(self) -> str:
        return run("git", "rev-parse", "HEAD", cwd=self.root).stdout.strip()

    def add_submodule(self, relative: str) -> None:
        source = Path(tempfile.mkdtemp(prefix="docs-only-ci-submodule-"))
        self.extra_roots.append(source)
        run("git", "init", "-q", "-b", "master", cwd=source)
        run("git", "config", "user.name", "CI Test", cwd=source)
        run("git", "config", "user.email", "ci@example.invalid", cwd=source)
        (source / "README.md").write_text("submodule\n", encoding="utf-8")
        run("git", "add", "README.md", cwd=source)
        run("git", "commit", "-q", "-m", "base", cwd=source)
        run(
            "git",
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            str(source),
            relative,
            cwd=self.root,
        )

    def classify(self) -> subprocess.CompletedProcess[str]:
        return run(
            "bash",
            str(CLASSIFIER),
            self.base,
            self.sha(),
            cwd=self.root,
            check=False,
        )


def classified(change: Callable[[Repo], object]) -> tuple[int, str, str]:
    repo = Repo()
    try:
        change(repo)
        repo.commit("change")
        result = repo.classify()
        return result.returncode, result.stdout.strip(), result.stderr.strip()
    finally:
        repo.close()


def assert_classification(name: str, expected: str, change: Callable[[Repo], object]) -> None:
    code, actual, stderr = classified(change)
    assert code == 0, f"{name}: classifier exited {code}: {stderr}"
    assert actual == expected, f"{name}: expected {expected}, got {actual}"
