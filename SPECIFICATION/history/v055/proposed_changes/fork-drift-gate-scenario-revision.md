---
proposal: fork-drift-gate-scenario.md
decision: accept
revised_at: 2026-10-22T16:28:43Z
author_human: Chad Woolley <thewoolleyman@gmail.com>
author_llm: claude-opus-5-5
---

## Decision and Rationale

Accepted as proposed. The console spec had no heading governing check-fork-drift, so work-item livespec-console-beads-fabro-lavbfp (the root-cause fix for three stop-the-line reds in three days: R316, R498, R620) failed its launch-time Definition-of-Done preflight for lack of a governing reference (coordinator R631, remedy R202). The new Workflow-Fork Drift Gate clauses state the intended behaviour (verdict from the pinned release, newer install is a currency warning with exit zero, unrecorded divergence fails naming the file, pins advance only by a committed re-sync) and Contributor Scenario K carries them. Contributor-only gate, so non-functional-requirements.md under Contracts beside Cross-Repo Action-Surface Parity. Independent opus review: NO BLOCKERS; its non-blocking notes (no Gherkin for the pin-advance clause; the fixture records the release as the release tag's commit hash; older-install cases) are left to lavbfp's implementation and a later revision.

## Resulting Changes

- non-functional-requirements.md

## Ratification Review

ratification_review: auto-spawn
reviewer_model: opus
reviewer_identity: opus
separate_reviewer: True
read_only: True
reviewed_at: 2026-10-22T16:28:42Z
verdict: NO BLOCKERS
proposal_stem: fork-drift-gate-scenario
content_digest: 500bf302d58e8acbad5090428c1489d64aa6720b4811d4dacc9352c41a644f60
