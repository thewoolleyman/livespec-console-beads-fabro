---
proposal: console-cruft-cleanup.md
decision: accept
revised_at: 2026-07-04T09:40:41Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Maintainer-ratified 2026-07-04 after independent Fable verification (1 minor blocker found and amended; fresh re-verification NO BLOCKERS). Aligns the console spec onto the ratified work-item state machine: the orchestrator CLI becomes the sole work-item source (zero beads knowledge, consume-don't-recompute per decisions 15/16); needs-regroom vocabulary retired; the operator command vocabulary gains the approve/accept/reject and set-admission/set-acceptance commands mapped 1:1 onto the orchestrate action ids ratified in livespec-orchestrator-beads-fabro v029; nightly chores file through the consented capture surface.

## Resulting Changes

- spec.md
- contracts.md
- scenarios.md
- non-functional-requirements.md
