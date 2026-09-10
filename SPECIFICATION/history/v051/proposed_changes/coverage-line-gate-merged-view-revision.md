---
proposal: coverage-line-gate-merged-view.md
decision: modify
revised_at: 2026-09-10T14:50:23Z
author_human: Chad Woolley <thewoolleyman@gmail.com>
author_llm: claude-opus-5
---

## Decision and Rationale

Accepted with modifications (the proposal's block lands verbatim; three untouched passages that still called the region gate a future step are reworded to match, after the independent review flagged them). It implements the maintainer ruling of 2026-09-10 that no line of code is uncoverable, so the coverage gate carries no allowance. PR #1193 (livespec-console-beads-fabro-jvfvkf) already enforces exactly this on master: dev-tooling/coverage-gate.py measures lines in the merged per-line union view and regions in the merged reachable-region view, both requiring zero, and tests/fixtures/coverage-unnameable-disposition.json is deleted. The prior clause named `--fail-under-lines 100` as the line gate and called the region gate a future target, so the ratified text no longer matched the enforced gate. The resulting file applies the proposal's replacement verbatim, and the bullet's untouched continuation begins a new line. The new MUST NOT moves the pinned console-spec-check clause counts, which are updated in the same change.

## Modifications

The independent ratification review returned BLOCKERS on the verbatim accept. Three passages the proposal did not touch still called the region gate a future step, contradicting the new 'The region gate is enforced today'. Modified to cover them. (1) The same bullet's closing parenthetical no longer says 'line coverage is the falsifiable knob that gates today; region coverage is the mature next knob the gate is moving to'; it now says line and region coverage, both in the merged view, are the falsifiable knobs that gate today. (2) The Quality Gate mermaid node 'coverage 100% line (lib); region next' now reads 'coverage 100% line + region (lib, merged view)'. (3) Contributor Scenario C's diagram node and Gherkin now say '100% line + region coverage' and '100% line and region coverage' instead of line coverage alone. The proposal's own replacement block is applied verbatim. No normative keyword was added beyond the proposal's one MUST NOT.

## Resulting Changes

- non-functional-requirements.md

## Ratification Review

ratification_review: auto-spawn
reviewer_model: opus
reviewer_identity: opus
separate_reviewer: True
read_only: True
reviewed_at: 2026-09-10T14:49:58Z
verdict: NO BLOCKERS
proposal_stem: coverage-line-gate-merged-view
content_digest: 069c5342162d9b0336f92fe08c04a5ff48ac7d714d37dd14c14fd5941cf74418
