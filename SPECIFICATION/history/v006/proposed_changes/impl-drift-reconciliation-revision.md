---
proposal: impl-drift-reconciliation.md
decision: modify
revised_at: 2026-06-25T16:30:51Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Accepted all three impl->spec drift findings, reconciling the spec toward the implementation's actual behavior per the refinement methodology. F1 (coverage gate): the v005 NFR claimed region coverage 'is what gates today' and mandated --fail-under-regions 100, but the check-coverage recipe gates only --fail-under-lines 100; reframed so 100% line coverage is what gates today and 100% region is the stated next target, tracked as the coverage-region-gate impl obligation rather than a silent false claim. F2 (coverage crates): added console-cli, a current --lib target the enumerated set omitted ('any future library crate' did not cover a crate that exists now). F3 (Beads stream label): relabeled the Initial Adapters diagram's Beads canonical stream from work_item.* to beads.* to match the emitted beads.work_item_snapshot_observed prefix and stay consistent with the other source-prefixed stream labels.

## Modifications

F1's parenthetical was finalized (line coverage is the falsifiable knob that gates today; region is the mature next knob the gate is moving to; --branch remains unstable), and the inner-loop mermaid Coverage node was updated from 'coverage 100% line + region (lib)' to 'coverage 100% line (lib); region next' so the diagram no longer re-asserts region-gates-today. F2 and F3 landed as proposed. The region gate itself is deferred to the coverage-region-gate impl follow-up declared in the proposed change's spec_commitments.

## Resulting Changes

- non-functional-requirements.md
- contracts.md
