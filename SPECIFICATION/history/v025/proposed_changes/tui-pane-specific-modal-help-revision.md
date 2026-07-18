---
proposal: tui-pane-specific-modal-help.md
decision: accept
revised_at: 2026-07-18T07:49:36Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Ratifying B4: the TUI in-app help becomes a pane-specific modal overlay (invoked by `?`, left-side section menu beside an up/down-only scrollable right pane, 3-character-border near-full-viewport window, auto-focused to the focused pane's section, closable only on Esc with `esc to exit` always shown). Applied verbatim from the Fable-cleared, maintainer-approved propose-change: a new contracts.md TUI Contract clause, a new scenarios.md Scenario 18, and the tests/heading-coverage.json co-edit binding the seven new TUI Contract MUST clauses to Scenario 18 and registering Scenario 18 with a top-of-pyramid pending-test entry.

## Resulting Changes

- contracts.md
- scenarios.md
- ../tests/heading-coverage.json
- ../crates/console-spec-check/src/tests.rs
