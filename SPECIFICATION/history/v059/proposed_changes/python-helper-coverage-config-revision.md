---
proposal: python-helper-coverage-config.md
decision: accept
revised_at: 2026-11-09T23:08:06Z
author_human: Chad Woolley <thewoolleyman@gmail.com>
author_llm: claude-opus-5-5
---

## Decision and Rationale

Accepted unmodified under delegated decision mode. No ratified clause governs the 100% line and branch coverage gate that check-per-file-coverage already holds the Python helper tree under dev-tooling/ to, so the execution mirror livespec-console-beads-fabro-olhsgx (of livespec-dev-tooling-wj45qt, plan dev-host-test-gate-load, epic livespec-dev-tooling-6vuiwe) had no heading to reference. The clause states that gate, and requires that pyproject.toml's coverage configuration govern every pytest coverage run, so a bare `pytest --cov` spelling cannot fail combining statement and branch data after the suite passes. That defect was measured in livespec-dev-tooling on 2026-11-09 and fixed there in livespec-dev-tooling#3895. This is a contributor-only concern, so it lands in non-functional-requirements.md as an inner-loop bullet plus a Contributor Scenario C gherkin scenario. No H2 heading changes. No design record is contradicted, and the proposal is not drift-origin: it states a requirement ahead of the mirror that implements it. The factual premise was verified against the justfile recipe and pyproject.toml. Independent sonnet ratification review: NO BLOCKERS.

## Resulting Changes

- non-functional-requirements.md

## Ratification Review

ratification_review: auto-spawn
reviewer_model: sonnet
reviewer_identity: sonnet
separate_reviewer: True
read_only: True
reviewed_at: 2026-11-09T23:07:06Z
verdict: NO BLOCKERS
proposal_stem: python-helper-coverage-config
content_digest: 2bced6d7ee1c1247614ce6f872356a355e4b5338a5503b69671d25efee4ee5b4
