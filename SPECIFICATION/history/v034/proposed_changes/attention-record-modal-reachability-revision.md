---
proposal: attention-record-modal-reachability.md
decision: accept
revised_at: 2026-07-21T05:49:00Z
author_human: delegated-factory-work-item
author_llm: codex
---

## Decision and Rationale

Accept. This revision extends the v031 work-item record drill-in to the needs-attention surface, where operators most often meet unknown blocked or human-needed items. The record surface remains one modal and one reducer path: it pins the selected work-item id at open time, now using the view-scoped selected work-item rather than only the drilled-in lane item. The command modal is no longer opened or rendered when the selected attention detail has zero actions, so the Status line no longer advertises `enter run` for an empty action set.

The contracts.md work-item-record clause is amended in place and rebinds from `gap-lu5ergzl` to `gap-rmrpojby`; clause counts remain 15/77/22/52 = 166. Scenario 23 is extended with the needs-attention path and Esc selection-preservation wording. `tests/heading-coverage.json` links the new gap id to Scenario 23 and names the reducer/renderer coverage for Attention Enter, empty command-modal suppression, honest hints, and Esc selection preservation.

## Resulting Changes

- contracts.md
- scenarios.md
