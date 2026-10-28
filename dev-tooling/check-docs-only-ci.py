#!/usr/bin/env python3
"""Acceptance checks for the fail-closed documentation-only CI lane."""

from __future__ import annotations

from docs_only_ci_repo import CLASSIFIER, Repo, assert_classification, run
from docs_only_ci_workflow import assert_workflow_wiring, verify_lane

ALL_SUCCESS = ("success", "success", "success", "success", "success")
ALL_SKIPPED = ("skipped", "skipped", "skipped", "skipped", "skipped")


def check_true_classifications() -> None:
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


def check_false_classifications() -> None:
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


def check_degenerate_ranges() -> None:
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


def check_lane_verifier() -> None:
    assert verify_lane("true", ALL_SKIPPED) == 0
    assert verify_lane("false", ALL_SUCCESS) == 0
    assert verify_lane("true", ("success", "skipped", "skipped", "skipped", "skipped")) != 0
    assert verify_lane("false", ("skipped", "success", "success", "success", "success")) != 0
    for invalid_docs_only in ("", "garbage"):
        assert verify_lane(invalid_docs_only, ALL_SUCCESS) != 0
    for bad_classifier in ("failure", "cancelled", "skipped"):
        assert verify_lane("false", ALL_SUCCESS, classifier=bad_classifier) != 0
    for bad_doctor in ("failure", "cancelled", "skipped"):
        assert verify_lane("false", ALL_SUCCESS, doctor=bad_doctor) != 0
    for index in range(5):
        for bad_result in ("failure", "cancelled", "skipped"):
            results: list[str] = list(ALL_SUCCESS)
            results[index] = bad_result
            assert verify_lane("false", tuple(results)) != 0


def main() -> None:
    assert CLASSIFIER.is_file(), f"missing classifier: {CLASSIFIER}"
    check_true_classifications()
    check_false_classifications()
    check_degenerate_ranges()
    assert_workflow_wiring()
    check_lane_verifier()
    print("check-docs-only-ci: classification and lane wiring are fail-closed")


if __name__ == "__main__":
    main()
