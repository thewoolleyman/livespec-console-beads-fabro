---
topic: attention-record-modal-reachability
author: codex
created_at: 2026-07-21T05:48:00Z
---

## Proposal: Attention rows open the work-item record modal

### Target specification files

- SPECIFICATION/contracts.md
- SPECIFICATION/scenarios.md

### Summary

Extend the work-item record drill-in contract so the same record surface is reachable from the needs-attention view when the selected row carries a known work-item id. The Lanes path already had this surface; this closes the Attention path and states that the legacy command modal is not the fallback when it has no actions.

### Motivation

The needs-attention view is where an operator first encounters many blocked or human-needed work-items. Enter previously opened the older command modal. For orchestrator-sourced items with no attach actions, that modal rendered as an empty bordered box while the Status line advertised runnable action keys. That violated the hint-honesty contract and left the full work-item record unreachable from the highest-attention surface.

### Proposed Changes

--- CHANGE 1: SPECIFICATION/contracts.md, TUI Contract work-item record clause ---

Amend the record drill-in clause so the surface is reachable from both the drilled-in lane list and the needs-attention view when the selected attention row carries a known work-item id. This is an in-place clause rewording, not a new clause; the contracts.md clause count stays at 77 and the total count stays at 166. The clause gap id re-derives from `gap-lu5ergzl` to `gap-rmrpojby`.

--- CHANGE 2: SPECIFICATION/scenarios.md, Scenario 23 ---

Add the needs-attention path to the record-drill-in flowchart, add a scenario for Enter on a needs-attention row opening the same record surface, and broaden the Esc scenario so it returns to the drilled-in lane or needs-attention list it was opened from with selection preserved. Gherkin is fenced, so this changes no normative clause count.

--- CHANGE 3: tests/heading-coverage.json ---

Rebind Scenario 23 from `gap-lu5ergzl` to `gap-rmrpojby` and extend its reason with the reducer/renderer tests covering the Attention reachability and empty command-modal behavior.
