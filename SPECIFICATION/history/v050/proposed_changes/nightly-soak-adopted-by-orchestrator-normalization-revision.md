---
proposal: nightly-soak-adopted-by-orchestrator-normalization.md
decision: accept
revised_at: 2026-09-09T20:29:35Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-sonnet-5
---

## Decision and Rationale

Accepted verbatim as drafted. The upstream orchestrator ratified and shipped the Dispatcher ledger-normalization adoption+rank primitive (bd-ib-3nlq closed, orch PR #2414 / v0.146.0, console proxy livespec-console-beads-fabro-4jb3kl.1 released 2026-09-09), which makes the prior shadow-ledger negative in the Nightly quality-gate clause false. This proposal retracts that negative and replaces it with a CONSUME clause citing the ratified orchestrator contracts.md section by prose (never a Python identifier or CLI subcommand), keeps the previously-removed 'top of the rank order' placement obligation removed per charter D2, preserves every narrow filing guarantee, and co-edits the Contributor Scenario C mermaid/gherkin to match. Verified: both retained MUST lines keep byte-identical line-wrap, so the pinned console-spec-check clause counts (261 total / 67 non-functional-requirements.md) and every tests/heading-coverage.json gap_id under Scenario C are unchanged (cargo test -p console-spec-check green). Independently re-verified by a separate read-only opus reviewer against the upstream orchestrator source directly, not just the proposal's summary: NO BLOCKERS.

## Resulting Changes

- non-functional-requirements.md

## Ratification Review

ratification_review: auto-spawn
reviewer_model: opus
reviewer_identity: opus
separate_reviewer: True
read_only: True
reviewed_at: 2026-09-09T20:29:11Z
verdict: NO BLOCKERS
proposal_stem: nightly-soak-adopted-by-orchestrator-normalization
content_digest: 782e85258bf126309b0ee0b217b520c88bfec5dbc02e1784c9e71f7e8a577fe3
