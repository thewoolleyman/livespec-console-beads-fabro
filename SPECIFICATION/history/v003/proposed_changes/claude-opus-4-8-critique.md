---
topic: claude-opus-4-8-critique
author: claude-opus-4-8
created_at: 2026-06-25T06:22:34Z
---

## Proposal: Coverage gate: which crates are in the 100% line+region set is ambiguous

### Target specification files

- SPECIFICATION/non-functional-requirements.md

### Summary

The v002 Quality Gate gates coverage at 100% line AND region 'over the bottom-of-pyramid library targets (console-domain, console-application, console-eventstore, and every other --lib target).' Naming three crates but then adding 'and every other --lib target' is ambiguous: the strict gate is scoped either to the named logic crates or to ALL workspace lib targets -- which also includes console-tui (UI rendering) and console-eventstore (SQLite I/O). 'Bottom-of-pyramid' implies pure logic, yet 'every other --lib target' sweeps in UI and infrastructure crates whose 100% region coverage is not a pure-logic forcing-function.

### Motivation

The crate scope of the strict coverage gate is ambiguous and internally inconsistent: 'bottom-of-pyramid library targets' (implying pure logic) contradicts 'and every other --lib target' (all libs, including the TUI and the SQLite event store), so a reader cannot tell which crates must hit 100% region.

### Proposed Changes

State the exact crate set the 100% line+region gate binds. If it is all workspace lib targets (matching today's `--workspace --lib` gate), drop or redefine the 'bottom-of-pyramid' framing; if it is the logic crates only, name them exhaustively and state the separate (lower or no) coverage expectation for console-tui and console-eventstore. Resolve explicitly whether 100% region (not just line) is intended for the SQLite event store and the TUI.

## Proposal: Behavioral coverage: the warn-to-fail graduation condition is undefined

### Target specification files

- SPECIFICATION/non-functional-requirements.md

### Summary

The Behavioral Coverage rule says the clause -> scenario -> test check 'MUST support a warn -> fail severity lever; reaching and holding fail mode ... is a release-blocking, highest-priority obligation.' It states no concrete condition for when the lever MUST flip from warn to fail, and 'release-blocking' is undefined for a project that declares no release milestones -- the same unfalsifiable shape the fuzz/mutation 'where practical' fix just removed elsewhere in the same gate.

### Motivation

The warn-to-fail graduation condition is undefined, leaving the fail-mode obligation unfalsifiable: it is unclear at what point running the check in warn mode becomes a violation, reintroducing the very vagueness the quality-gate revision otherwise eliminated.

### Proposed Changes

State the concrete condition that flips the behavioral-coverage check from warn to fail (for example: fail mode MUST be active once the initial clause/scenario/test backlog is linked, and no later than a named milestone or date), so the obligation is falsifiable rather than aspirational.

## Proposal: Behavioral coverage: 'corresponding test' binding mechanism is unspecified

### Target specification files

- SPECIFICATION/non-functional-requirements.md
- SPECIFICATION/scenarios.md

### Summary

The Behavioral Coverage rule requires that 'every scenario MUST have a corresponding top-of-pyramid acceptance/integration test' but does not define how a scenario is bound to its test -- a naming convention, a tag, or an explicit link registry. It cites porting livespec's tests/heading-coverage.json registry as the implementation, but the normative rule itself leaves the binding mechanism unspecified.

### Motivation

'Corresponding test' is undefined: without a stated binding mechanism it is unclear how the mechanical check decides a scenario is or is not covered, so the rule's enforceability is ambiguous at the spec level even though the impl note names a registry.

### Proposed Changes

Specify the binding mechanism in the rule itself -- e.g., a scenario is identified by its scenarios.md H2 heading and bound to its test via an explicit link registry (the ported heading-coverage.json) or a stated test-name/tag convention -- so 'every scenario has a corresponding test' is mechanically decidable.

## Proposal: Nightly finding: the opened work-item's type, priority, and store are unspecified

### Target specification files

- SPECIFICATION/non-functional-requirements.md

### Summary

The nightly quality-gate rule says a finding 'MUST instead open a tracked work-item against the work-items store' but does not specify the work-item's type (chore vs bug), its priority, or its readiness, nor name the store beyond 'the work-items store.'

### Motivation

The nightly-finding work-item's attributes are undefined, leaving it unclear what kind of tracked item is opened and how it is prioritized -- an under-specification of a contributor-relevant behavior the rule otherwise mandates.

### Proposed Changes

State the opened work-item's type and priority (e.g., a high-priority chore filed ready for pickup) and name the store (the Beads tenant accessed per the family secret convention), so the nightly-to-tracking behavior is fully pinned.
