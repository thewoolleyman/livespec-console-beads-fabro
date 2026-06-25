---
topic: behavioral-coverage-merge-gate-not-placeholder
author: claude-opus-4-8
created_at: 2026-06-25T15:23:24Z
---

## Proposal: Reframe Behavioral Coverage: drop the fail-closed CI placeholder; the gate lands with the checker

### Target specification files

- SPECIFICATION/non-functional-requirements.md

### Summary

The v004 Behavioral Coverage rule mandated a fail-closed CI placeholder that hard-fails the build until the Rust checker exists. That mechanism deadlocks the merge gate -- it blocks every merge (including the checker's own PR and unrelated work) -- so it is removed. The clause->scenario->test requirement stays; the hard fail-mode gate attaches to the real checker and lands with it, tracked as the scenario-test-rust-checker work-item (a release-blocking obligation).

### Motivation

The fail-closed placeholder blocked all merges to master, contradicting the need to land checkpoints and continue work; CI must be green while the checker is unbuilt, with enforcement carried by a tracked release-blocking work-item rather than a blocking placeholder.

### Proposed Changes

In non-functional-requirements.md S'Behavioral Coverage', replace the 'fail-closed placeholder MUST fail until the checker exists' mandate with: when implemented, the checker runs as a hard fail-mode gate in just check + CI; implementing it and backfilling all clause->scenario->test links is a release-blocking, highest-priority obligation tracked as the scenario-test-rust-checker work-item; until it lands, NO fail-closed CI placeholder is used (it deadlocks the merge gate). Soften the inner-loop quality-gate bullet to 'once its checker lands'.
