---
proposal: b1-source-availability-honesty.md
decision: accept
revised_at: 2026-07-18T07:49:36Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Ratifying B1: the console's source-availability honesty is tightened so a reachable-but-empty source is observed-and-idle (never a not-observed finding), a not-observed finding carries a durably-persisted human-readable reason, and the header tally reflects the latest poll outcome per source (a recovered source clears rather than being branded forever). Applied verbatim from the Fable-cleared, maintainer-approved propose-change: a new contracts.md Adapter Contract honesty clause (intro + three bullets), four availability-honesty Scenario blocks inserted inside the existing Scenario 13 gherkin fence (no new H2), and the tests/heading-coverage.json fidelity refresh of the Scenario 13 entry binding the six new Adapter Contract MUST clauses to Scenario 13.

## Resulting Changes

- contracts.md
- scenarios.md
- ../tests/heading-coverage.json
- ../crates/console-spec-check/src/tests.rs
