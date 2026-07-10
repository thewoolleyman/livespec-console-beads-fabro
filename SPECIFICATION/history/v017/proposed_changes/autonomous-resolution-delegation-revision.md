---
proposal: autonomous-resolution-delegation.md
decision: accept
revised_at: 2026-07-10T13:28:17Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Driver ratified ACCEPT after independent read-only Fable review returned NOTHING-BLOCKING. Re-scopes Full Autonomous Mode, Scenario 10, and the Autonomous-Mode Safety audit constraint to the delegation model (the owning plane's engine resolves; the console enables + observes + reflects), explicitly killing the double-resolution race by deferring the console-side resolver. Persistence seam (design 6.1) remains deferred to I1. No H2 heading change.

## Resulting Changes

- spec.md
- scenarios.md
- constraints.md
