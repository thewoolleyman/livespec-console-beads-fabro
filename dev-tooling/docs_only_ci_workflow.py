"""Workflow-wiring and lane-verifier acceptance for the documentation-only CI lane."""

from __future__ import annotations

import os

from docs_only_ci_repo import ROOT, run

WORKFLOW = ROOT / ".github/workflows/ci.yml"


def workflow_job(text: str, job_id: str) -> str:
    marker = f"  {job_id}:\n"
    start = text.find(marker)
    assert start >= 0, f"workflow has no {job_id} job"
    remainder = text[start + len(marker) :]
    boundaries = [
        offset
        for offset in (
            remainder.find(f"\n{line}\n")
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

    full_gate_condition = "!cancelled() && needs.classify-changes.outputs.docs_only != 'true'"
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
