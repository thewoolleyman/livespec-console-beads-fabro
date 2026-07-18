---
proposal: top-pane-focus-hscroll.md
decision: accept
revised_at: 2026-07-18T14:47:32Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Ratify B3: the console TUI's top/header pane joins the pane focus cycle, scrolls horizontally to reveal content clipped at the current viewport width while focused, and snaps back to its left-justified default on blur. The independent Fable-model review returned NO-BLOCKERS and the driver spot-checked the anchors, scenario-max, and ground-truth. Applies the three ADDs verbatim: (CHANGE 1) a new contracts.md TUI-Contract clause (one physical line, five MUSTs, inserted after the B2 Status-line clause and before the eight-lifecycle paragraph); (CHANGE 2) a new Scenario 20 in scenarios.md with its flowchart and four gherkin scenarios; (CHANGE 3) the tests/heading-coverage.json co-edit registering the Scenario 20 top-of-pyramid acceptance entry with the newly-derived TUI-Contract clause gap-lepclyx4 linked (this repo's ratification-time clause-link gate).

## Resulting Changes

- contracts.md
- scenarios.md
- ../tests/heading-coverage.json
