---
topic: coverage-region-gate-merged-view-reformulation
author: claude-opus-4-8
created_at: 2026-08-30T05:46:56Z
---

## Proposal: Coverage-region gate asserts the merged reachable-region view, not the raw --workspace summary

### Target specification files

- SPECIFICATION/non-functional-requirements.md

### Summary

Reformulate the not-yet-present coverage-region-gate obligation so that, when it lands, it asserts 100% of REACHABLE regions -- zero regions uncovered in the merged, cross-instantiation region view -- rather than gating on the raw `cargo llvm-cov --workspace --lib` summary region count, which over-reports uncovered regions by an llvm-cov cross-object merge artifact. The 100% line gate (`--fail-under-lines 100`) is unchanged; the no-exclusions clause stands; no region, arm, or line is exempted.

### Motivation

The clause states the region target as adding `--fail-under-regions 100` to the ratified `cargo llvm-cov --workspace --lib` command. Measured on master 3f67dd1 after every locatable region across the test-adequacy children (-txtzn5.14/.15/.17/.18/.19) is closed: crates/console-eventstore/src/lib.rs reports 3122/3124 regions (99.936%) with 234/234 = 100.00% FUNCTION coverage, yet `--fail-under-regions 100` reads that summary count and so would FAIL at a 2-region floor. A scalar-max merge over `data[0].functions[].regions` (group by source span, take the max execution count across instantiations) locates ZERO uncovered regions in the file, and `llvm-cov show` over all workspace objects with `-show-instantiations=false` renders no `^0` for those 2. The cause is structural: 146 source regions in that file DISAGREE across instantiations (uncovered in some object, covered in others) because a NON-generic item -- a getter, an open-retry helper, a query_row closure -- is compiled into several crates' `--lib` test binaries under `--workspace`, exercised in its home object and linked-but-unused in the others; llvm-cov's cross-object summary arithmetic nets 2 of the 146 as `uncovered`. These 2 are a MEASUREMENT ARTIFACT of merging coverage across multiple test binaries, not reachable source and not de-genericizable (there is no generic with an uncalled arm to collapse). Gating on the raw summary would therefore permanently fail the gate for a reason unrelated to test adequacy, while asserting the merged reachable-region view keeps the gate's 100%-of-reachable-code intent exactly and lets it flip. This is expressly NOT a coverage exclusion: the no-exclusions clause forbids exempting individual regions/arms/lines and mandates restructuring genuinely-uncoverable code; here nothing is exempted -- the reformulation corrects the metric so the gate asserts true reachable-region coverage. A per-crate measurement was considered and rejected: it would abandon `--workspace` cross-crate integration coverage that the line gate relies on. Three independent derivations (recorded on -txtzn5.14 and in plan/test-adequacy-gates/research/007) agree the residue is an llvm-cov cross-object merge artifact with no locatable source position.

### Proposed Changes

In `non-functional-requirements.md`, in the `just check` coverage bullet (the paragraph beginning "coverage gated at 100% line today ... with 100% region coverage as the stated next target -- adding `--fail-under-regions 100` -- tracked as the `coverage-region-gate` impl obligation, NOT yet a present gate"), REPLACE the description of the region target so it reads (in intent):

- Keep "coverage gated at 100% line today (`cargo llvm-cov --workspace --lib --fail-under-lines 100`)" unchanged.
- Restate the region target as: 100% REGION coverage of every REACHABLE region -- defined as zero regions uncovered in the MERGED, cross-instantiation region view -- remains the stated next target, tracked as the `coverage-region-gate` impl obligation, NOT yet a present gate. The gate MUST assert the merged reachable-region view rather than the raw `cargo llvm-cov --workspace --lib` summary region count, because that summary over-reports uncovered regions by an llvm-cov cross-object merge artifact: under `--workspace` a single non-generic library item is compiled into several crates' `--lib` test binaries and counted uncovered in the objects that link but do not call it, even when every reachable region is exercised. The merged reachable-region view is the number reported by the scalar-max merge over `--json` `data[0].functions[].regions` (a region is covered iff its maximum execution count across instantiations is non-zero), equivalently by `llvm-cov show` across all workspace objects with `-show-instantiations=false` showing no `^0`.
- Add one sentence making the boundary explicit: this measurement correction is NOT a coverage exclusion -- no region, arm, or line is exempted, and the **No coverage exclusions are permitted** clause stands in full; genuinely-uncoverable code MUST still be eliminated by restructuring, never annotated away.
- Leave unchanged: the `--lib`-binds-every-workspace-library requirement with no per-crate carve-outs; the `main.rs`-only-shim statement; and the `--branch`-is-unstable / line-is-the-falsifiable-knob parenthetical.

The corresponding standalone checker/justfile realization of the flipped gate (the `coverage-region-gate` impl obligation) asserts the merged reachable-region view per the above and is the impl half tracked in the beads ledger (livespec-console-beads-fabro-txtzn5.11); it is not itself spec text.
