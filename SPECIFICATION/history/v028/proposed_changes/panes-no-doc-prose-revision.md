---
proposal: panes-no-doc-prose.md
decision: accept
revised_at: 2026-07-19T03:46:22Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Accept the panes-no-doc-prose proposal (B5): the console TUI's pane bodies render operational content only -- the live data and state an operator acts on -- and MUST NOT carry baked-in explanatory or documentation prose beyond the operational help surfaces the contract separately requires (the Status-line hints, the modal Help overlay, and the Settings per-row inline help). Adds a contracts.md TUI-Contract clause (one physical line, +1 normative clause), a new Scenario 21 in scenarios.md, the tests/heading-coverage.json Scenario 21 entry linking the derived clause gap-xiziukv6, and the console-spec-check ground-truth bump (contracts.md 72->73, total 161->162). Independent Fable review returned NO-BLOCKERS.

## Resulting Changes

- contracts.md
- scenarios.md
- ../tests/heading-coverage.json
- ../crates/console-spec-check/src/tests.rs
