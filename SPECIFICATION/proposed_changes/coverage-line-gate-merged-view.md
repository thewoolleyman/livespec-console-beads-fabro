---
topic: coverage-line-gate-merged-view
author: claude-opus-5
created_at: 2026-09-10T15:55:00Z
---

## Proposal: Line coverage is 100% in the merged per-line view, with no allowance; the region gate is a present gate

### Target specification files

- SPECIFICATION/non-functional-requirements.md

### Summary

Replace the Quality Gate coverage bullet's line clause, which names the literal command `cargo llvm-cov --workspace --lib --fail-under-lines 100`, with the metric the gate actually enforces: 100% line coverage in the **merged, cross-instantiation per-line view** (a source line is covered iff any instantiation of any function spanning it executes it), with **no allowance and no disposition fixture**. The same bullet still calls the region target "the stated next target ... NOT yet a present gate"; the region gate has been enforced by `dev-tooling/coverage-gate.py` since v044, so that wording is corrected to present tense. The revision that lands this MUST re-check the pinned clause counts in `crates/console-spec-check/src/tests.rs` (`extract_rules_matches_real_spec_ground_truth`) per `.ai/spec-check-and-ci-discipline.md`, since the bullet's MUST sentences change.

### Motivation

Maintainer ruling 2026-09-10, verbatim: "There is no such thing as uncoverable lines of code. If a line of code cannot be executed by tests, then it cannot be executed by the application. It has no use existing. Every line of code is covered by correct and good architecture." Until now the line gate carried an allowance of one "unnameable" missed line (`tests/fixtures/coverage-unnameable-disposition.json`, item livespec-console-beads-fabro-3yx), because llvm-cov's own line summary merges a function's instantiations by INDEPENDENT SCALAR MAXIMA of `(NumLines, Covered)` rather than a per-line union: instantiations covering 16 and 15 of a function's 17 lines report one "missed" line while their union covers all 17. A bare `--fail-under-lines 100` therefore fails a fully covered workspace, and an allowance tolerating that residue could equally hide a real miss (on 2026-09-10 a second residue appeared and the allowance refused mx9u.3). Measuring the per-line union removes the artifact from the METRIC instead of tolerating it, which is exactly the move v044 already ratified for regions. The implementation (the per-line-union line gate, the allowance and fixture deleted) lands in the same change as this proposal.

### Proposed Changes

In `SPECIFICATION/non-functional-requirements.md` §Quality Gate, replace the opening of the coverage bullet, from "- coverage gated at **100% line** today" through "NOT yet a present gate.", with:

    - coverage gated at **100% line** and **100% region** coverage of
      every **reachable** line and region, both measured in the
      **merged, cross-instantiation view** and both requiring ZERO
      uncovered, with no allowance, tolerance, or disposition record. A
      source line is covered iff any instantiation of any function
      spanning it executes it. The line metric MUST NOT be llvm-cov's own
      line summary (`--fail-under-lines`), because that summary merges a
      function's instantiations by independent scalar maxima of its
      line and covered counts rather than by a per-line union, and so
      reports missed lines that correspond to no source line. The region
      gate is enforced today, as the `coverage-region-gate` obligation.

Leave the remainder of the bullet, beginning "That region target is defined against the merged reachable-region view", unchanged.
