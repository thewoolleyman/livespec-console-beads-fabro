---
proposal: status-line-context-hints.md
decision: accept
revised_at: 2026-07-18T11:56:28Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Ratify B2: Status-line context-specific shortcut hints. The independent Fable-model review returned NO-BLOCKERS and the driver spot-checked every replace anchor. Applies the three ADDs verbatim: (CHANGE 1) a new TUI-Contract paragraph in contracts.md requiring the Status line to render context-specific, non-empty shortcut hints that change with the focused pane and any open modal/overlay; (CHANGE 2) a new Scenario 19 in scenarios.md with its flowchart and four gherkin scenarios; (CHANGE 3) the tests/heading-coverage.json co-edit registering the Scenario 19 top-of-pyramid acceptance entry.

## Resulting Changes

- contracts.md
- scenarios.md
- ../tests/heading-coverage.json
