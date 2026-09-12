#!/usr/bin/env python3
"""Acceptance checks for the fail-closed documentation-only CI lane."""

from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
from typing import Callable


ROOT = Path(__file__).resolve().parents[1]
CLASSIFIER = ROOT / ".github/scripts/classify-ci-changes.sh"
WORKFLOW = ROOT / ".github/workflows/ci.yml"


def run(*args: str, cwd: Path, check: bool = True, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
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


def classified(change: Callable[[Repo], None]) -> tuple[int, str, str]:
    repo = Repo()
    try:
        change(repo)
        repo.commit("change")
        result = repo.classify()
        return result.returncode, result.stdout.strip(), result.stderr.strip()
    finally:
        repo.close()


def assert_classification(
    name: str, expected: str, change: Callable[[Repo], None]
) -> None:
    code, actual, stderr = classified(change)
    assert code == 0, f"{name}: classifier exited {code}: {stderr}"
    assert actual == expected, f"{name}: expected {expected}, got {actual}"


def workflow_job(text: str, job_id: str) -> str:
    marker = f"  {job_id}:\n"
    start = text.find(marker)
    assert start >= 0, f"workflow has no {job_id} job"
    remainder = text[start + len(marker) :]
    boundaries = [
        offset
        for offset in (
            remainder.find(f"\n  {line.split(':', 1)[0]}:\n")
            for line in remainder.splitlines()
            if line.startswith("  ") and not line.startswith("    ") and line.endswith(":")
        )
        if offset >= 0
    ]
    end = min(boundaries, default=len(remainder))
    return remainder[:end]


def inline_run_script(block: str) -> str:
    marker = "        run: |\n"
    start = block.find(marker)
    assert start >= 0, "workflow job has no inline run script"
    body: list[str] = []
    for line in block[start + len(marker) :].splitlines():
        if line and not line.startswith("          "):
            break
        body.append(line[10:] if line else "")
    assert body, "workflow inline run script is empty"
    return "\n".join(body)


def assert_workflow_wiring() -> None:
    workflow = WORKFLOW.read_text(encoding="utf-8")
    classifier = workflow_job(workflow, "classify-changes")
    assert "fetch-depth: 0" in classifier
    assert 'git show "${BASE_SHA}:.github/scripts/classify-ci-changes.sh"' in classifier
    assert "docs_only:" in classifier

    full_gate_condition = (
        "!cancelled() && needs.classify-changes.outputs.docs_only != 'true'"
    )
    for job_id in (
        "check",
        "check-e2e-tmux",
        "check-real-store-smoke",
        "check-mutants",
        "check-fuzz",
    ):
        block = workflow_job(workflow, job_id)
        assert "needs: classify-changes" in block, f"{job_id} does not need classifier"
        assert full_gate_condition in block, f"{job_id} is not fail-closed to the full gate"

    lightweight = workflow_job(workflow, "check-doctor-static")
    assert "needs: classify-changes" in lightweight
    assert "if: ${{ !cancelled() }}" in lightweight
    for recipe in (
        "check-doctor-static",
        "check-charters",
        "check-plan-no-tombstone",
        "check-plan-record-conformance",
        "check-plan-anchor-declared",
    ):
        assert f"just {recipe}" in lightweight, f"lightweight lane omits {recipe}"

    aggregate = workflow_job(workflow, "ci-green")
    for job_id in (
        "classify-changes",
        "check",
        "check-doctor-static",
        "check-e2e-tmux",
        "check-real-store-smoke",
        "check-mutants",
        "check-fuzz",
    ):
        assert job_id in aggregate, f"ci-green does not account for {job_id}"
    assert "required_result classify-changes" in aggregate
    assert "required_result check-doctor-static" in aggregate
    for job_id in (
        "check",
        "check-e2e-tmux",
        "check-real-store-smoke",
        "check-mutants",
        "check-fuzz",
    ):
        assert f"required_result {job_id}" in aggregate


def lane_verifier() -> str:
    workflow = WORKFLOW.read_text(encoding="utf-8")
    return inline_run_script(workflow_job(workflow, "ci-green"))


def verify_lane(
    docs_only: str,
    expected: tuple[str, ...],
    *,
    classifier: str = "success",
    doctor: str = "success",
) -> int:
    assert len(expected) == 5
    env = os.environ.copy()
    env.update(
        {
            "CLASSIFIER_RESULT": classifier,
            "DOCS_ONLY": docs_only,
            "CHECK_RESULT": expected[0],
            "DOCTOR_RESULT": doctor,
            "E2E_RESULT": expected[1],
            "REAL_STORE_RESULT": expected[2],
            "MUTANTS_RESULT": expected[3],
            "FUZZ_RESULT": expected[4],
        }
    )
    return run("bash", "-c", lane_verifier(), cwd=ROOT, check=False, env=env).returncode


def main() -> None:
    assert CLASSIFIER.is_file(), f"missing classifier: {CLASSIFIER}"

    for safe_path in (
        "CHANGELOG.md",
        ".ai/note.md",
        "plan/topic/handoff.md",
        "docs/doc-custody.md",
        "docs/factory-confirmations.md",
    ):
        assert_classification(
            f"safe modification {safe_path}",
            "true",
            lambda repo, path=safe_path: repo.write(path, "after\n"),
        )

    assert_classification(
        "nested allowed Markdown addition",
        "true",
        lambda repo: repo.write("plan/topic/research/new.md", "new\n"),
    )
    assert_classification(
        "allowed regular-file deletion",
        "true",
        lambda repo: (repo.root / "docs/doc-custody.md").unlink(),
    )
    assert_classification(
        "allowed rename",
        "true",
        lambda repo: (repo.root / "plan/topic/handoff.md").rename(
            repo.root / "plan/topic/renamed.md"
        ),
    )

    for unsafe_path in (
        "README.md",
        "docs/cli-options.md",
        "SPECIFICATION/spec.md",
        "src/lib.rs",
        ".github/workflows/ci.yml",
        ".github/scripts/classify-ci-changes.sh",
    ):
        assert_classification(
            f"unsafe modification {unsafe_path}",
            "false",
            lambda repo, path=unsafe_path: repo.write(path, "after\n"),
        )

    def mixed(repo: Repo) -> None:
        repo.write("CHANGELOG.md", "after\n")
        repo.write("src/lib.rs", "after\n")

    assert_classification("mixed diff", "false", mixed)
    assert_classification(
        "rename leaves allow-list",
        "false",
        lambda repo: (repo.root / "plan/topic/handoff.md").rename(repo.root / "README-2.md"),
    )
    assert_classification(
        "rename enters allow-list",
        "false",
        lambda repo: (repo.root / "src/lib.rs").rename(repo.root / "plan/lib.md"),
    )

    def symlink(repo: Repo) -> None:
        path = repo.root / ".ai/link.md"
        path.symlink_to("note.md")

    assert_classification("symlink addition", "false", symlink)

    def regular_file_to_symlink(repo: Repo) -> None:
        path = repo.root / ".ai/note.md"
        path.unlink()
        path.symlink_to("../CHANGELOG.md")

    assert_classification("regular file becomes symlink", "false", regular_file_to_symlink)

    assert_classification(
        "submodule addition", "false", lambda repo: repo.add_submodule("plan/submodule.md")
    )

    def executable_mode(repo: Repo) -> None:
        path = repo.root / "plan/topic/handoff.md"
        path.chmod(path.stat().st_mode | 0o100)

    assert_classification("mode change", "false", executable_mode)

    repo = Repo()
    try:
        unchanged = repo.classify()
        assert unchanged.returncode == 0
        assert unchanged.stdout.strip() == "false"
        invalid = run(
            "bash",
            str(CLASSIFIER),
            "0" * 40,
            repo.sha(),
            cwd=repo.root,
            check=False,
        )
        assert invalid.returncode != 0
    finally:
        repo.close()

    assert_workflow_wiring()
    assert verify_lane(
        "true", ("skipped", "skipped", "skipped", "skipped", "skipped")
    ) == 0
    assert verify_lane(
        "false", ("success", "success", "success", "success", "success")
    ) == 0
    assert verify_lane(
        "true", ("success", "skipped", "skipped", "skipped", "skipped")
    ) != 0
    assert verify_lane(
        "false", ("skipped", "success", "success", "success", "success")
    ) != 0
    for invalid_docs_only in ("", "garbage"):
        assert verify_lane(
            invalid_docs_only,
            ("success", "success", "success", "success", "success"),
        ) != 0
    for bad_classifier in ("failure", "cancelled", "skipped"):
        assert verify_lane(
            "false",
            ("success", "success", "success", "success", "success"),
            classifier=bad_classifier,
        ) != 0
    for bad_doctor in ("failure", "cancelled", "skipped"):
        assert verify_lane(
            "false",
            ("success", "success", "success", "success", "success"),
            doctor=bad_doctor,
        ) != 0
    for index in range(5):
        for bad_result in ("failure", "cancelled", "skipped"):
            results = ["success", "success", "success", "success", "success"]
            results[index] = bad_result
            assert verify_lane("false", tuple(results)) != 0
    print("check-docs-only-ci: classification and lane wiring are fail-closed")


if __name__ == "__main__":
    main()
