---
proposal: operator-status-move-surface.md
decision: accept
revised_at: 2026-07-17T08:11:07Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Independent console-spec-review returned NO-BLOCKERS on the amended proposal (round 2): both drift-sweep blockers closed (A.3 TUI-Contract + B.2 Scenario-10 command-count line), all four replace-targets verbatim+unique, design-record faithful to orchestrator v039 move/resolve-blocked guards. Brings the spec into lockstep with PR #248's shipped W7 surface (selection, s picker, resolve_blocked) and adds the guarded broad move. console-spec-check clean (0 unlinked / 0 untested); the two re-derived Command-Handling + TUI-Contract command-count clauses rebind to new Scenario 17 (the move/selection surface's tests), the two override clauses stay on Scenario 10; clause-count ground truth unchanged at 15/57/22/52 = 146.

## Resulting Changes

- contracts.md
- scenarios.md
- ../tests/heading-coverage.json
