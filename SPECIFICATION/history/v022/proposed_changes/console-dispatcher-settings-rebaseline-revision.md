---
proposal: console-dispatcher-settings-rebaseline.md
decision: accept
revised_at: 2026-07-15T05:23:37Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Accept as-filed after three rounds of independent Fable review returned NO-BLOCKERS (maintainer-delegated go). Retires the console's Full Autonomous Mode contract surface and re-baselines the console onto the six orchestrator-owned dispatcher.* policy settings ratified at the orchestrator's spec v034: the console commands and observes them and holds no setting state of its own. Verified: all replacement targets matched verbatim; console-spec-check reports 0 unlinked / 0 untested; full just check exits 0; clause-count ground truth refreshed to 15/57/22/52 = 146.

## Resulting Changes

- spec.md
- contracts.md
- constraints.md
- scenarios.md
- ../tests/heading-coverage.json
- ../crates/console-spec-check/src/tests.rs
