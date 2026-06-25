---
topic: impl-drift-reconciliation
author: claude-opus-4-8
created_at: 2026-06-25T16:46:50Z
spec_commitments:
  impl_followups:
    - id_hint: coverage-region-gate
      description: |
        Add `--fail-under-regions 100` to the `check-coverage` recipe (cargo llvm-cov --workspace --lib) and backfill region coverage to 100% across every workspace lib target so the region gate passes, realizing the spec's stated 100%-region target. Until this lands, only line coverage is gated.
---

## Proposal: Coverage gate: state line coverage as what gates today; 100% region is a tracked target, not the present gate

### Target specification files

- SPECIFICATION/non-functional-requirements.md

### Summary

The Quality Gate inner-loop coverage bullet mandates `cargo llvm-cov --workspace --lib --fail-under-lines 100 --fail-under-regions 100` (100% line AND region) and asserts parenthetically that region coverage 'is the mature falsifiable knob and is what gates today.' The implemented `check-coverage` recipe runs only `cargo llvm-cov --workspace --all-features --lib --fail-under-lines 100` -- line coverage alone; region is not gated. The present-tense claim is false against the implementation. Reconcile the spec to impl reality: 100% line coverage is what gates today; 100% region is the stated next target carried as a forward impl obligation, not a present gate.

### Motivation

Impl->spec drift. The spec asserts a present-tense fact ('region coverage ... is what gates today') that the justfile contradicts -- an auditor inspecting the gate finds only `--fail-under-lines 100`. Per the refinement methodology, reconcile toward the implementation's actual, coherent behavior rather than doubling down on an ungated requirement; the region gate then becomes an explicit tracked obligation instead of a silent inaccuracy.

### Proposed Changes

In the inner-loop coverage bullet, change the present-tense framing so it matches the implemented recipe: the gate enforces 100% LINE coverage today (`cargo llvm-cov --workspace --lib --fail-under-lines 100`) over every `--lib` target with no per-crate carve-outs, and 100% REGION coverage is the stated next target (adding `--fail-under-regions 100`) tracked as the `coverage-region-gate` impl obligation -- NOT described as already gating. Keep the design-forcing-function framing and the no-exclusions rule unchanged. Revise the parenthetical from 'region coverage is the mature falsifiable knob and is what gates today' to something like 'region coverage is the mature falsifiable knob and is the target the gate is moving to; `--branch` coverage remains unstable in cargo llvm-cov, so line coverage is what gates today and region is added next.'

## Proposal: Coverage gate: name console-cli among the enumerated covered library targets

### Target specification files

- SPECIFICATION/non-functional-requirements.md

### Summary

The coverage rule binds 'every workspace library (--lib) target ... with no per-crate carve-outs -- console-domain, console-application, console-eventstore, console-tui, and any future library crate.' The workspace today has a fifth current lib target, console-cli (crates/console-cli ships both src/lib.rs and src/main.rs), which the `--workspace --lib` gate already covers but which the enumerated set omits. The clause 'and any future library crate' does not cover console-cli, which exists now -- so the enumerated present set understates today's covered surface.

### Motivation

Impl->spec drift. The enumerated current covered-crate set is stale: console-cli is a present `--lib` target swept by the existing gate but not named, so the list misrepresents the workspace's current covered surface.

### Proposed Changes

In the coverage bullet, add `console-cli` to the enumerated lib targets so the set reads 'console-domain, console-application, console-eventstore, console-tui, console-cli, and any future library crate.' The binding rule stays 'every --lib target, no carve-outs'; the enumeration simply tracks the present members. Keep the note that the binary entry point (main.rs) is the only uncovered shim -- console-cli's main.rs is exactly such a shim while its lib.rs is covered.

## Proposal: Initial Adapters diagram: label the Beads canonical stream beads.* to match emitted event names

### Target specification files

- SPECIFICATION/contracts.md

### Summary

The Initial Adapters mermaid diagram labels the Beads adapter's canonical output stream `work_item.* events`, but the implementation emits the Beads work-item event as `beads.work_item_snapshot_observed` (prefix `beads.`, per console-domain EventType::BeadsWorkItemSnapshotObserved.contract_name). The diagram's other four stream labels (fabro.*, dispatch.*, spec.*, pr.*) each match their emitted event-name prefix; only the Beads label diverges. There is no `work_item.*`-prefixed event in the domain vocabulary.

### Motivation

Impl->spec drift. The diagram's illustrative stream label for Beads (`work_item.*`) does not match the emitted event-name prefix (`beads.`), unlike every other adapter whose label matches its prefix; the mismatch could mislead a reader about the canonical Beads event vocabulary.

### Proposed Changes

In the Initial Adapters mermaid diagram, change the Beads canonical stream node from `WorkEvents["work_item.* events"]` to `WorkEvents["beads.* events"]` (keeping the `Beads --> BA --> WorkEvents` edges), so the label matches the implemented `beads.work_item_snapshot_observed` event prefix and is consistent with the other source-prefixed stream labels.
