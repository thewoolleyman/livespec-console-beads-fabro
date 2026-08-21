---
topic: per-item-factory-dispatch-launcher-argv
author: claude-opus-5
created_at: 2026-08-21T01:19:47Z
---

## Proposal: Per-item factory-dispatch launcher argv

### Target specification files

- SPECIFICATION/contracts.md
- SPECIFICATION/scenarios.md

### Summary

Define the concrete Dispatcher argv the console's `factory.dispatch_item_requested` path must invoke for a one-item dispatch, as a per-item analogue of the existing Factory-drain launcher argv contract. Names `dispatch --repo <path> --item <id>` and rejects `loop --item` as the alternative, states explicitly that the chosen form is an operator override which does not enforce the work-in-progress cap, requires the console to surface that bypass to the operator, and carries the drain path's no-policy-arming obligation across unchanged. Adds a companion scenario.

### Motivation

The specification confirms the `factory.dispatch_item_requested` command but defines no concrete Dispatcher argv for one-item dispatch, while the drain path has carried its launcher argv contract since it was specified. The console's per-item port therefore cannot be wired against anything: it declines to fabricate an argv it was never told. The argv is not missing or undesigned upstream -- the orchestrator's Dispatcher exposes it publicly, requires the item id, and it is in live fleet use -- so the console is honest but under-informed. Two forms exist and differ in admission and cap behaviour, so the specification must choose one rather than leaving an adapter to pick silently; picking silently in the adapter is the class of defect this proposal exists to close.

### Proposed Changes

Add a new `### Per-item factory-dispatch launcher argv` section to
`SPECIFICATION/contracts.md`, immediately after the existing
`### Factory-drain launcher argv` section, and add a companion `## Scenario` to
`SPECIFICATION/scenarios.md` mirroring the shape of Scenario 16.

**The argv.** The console's `factory.dispatch_item_requested` path MUST invoke
the Dispatcher's `dispatch` subcommand with the selected work-item id:

    dispatch --repo <repository-path> --item <work-item-id>

This is the same launcher shape the drain path already uses -- a program plus
base arguments, with the per-invocation argument appended -- differing only in
the subcommand and in carrying `--item` instead of `--budget`/`--parallel`.

**Why `dispatch --item` and NOT `loop --item`.** Both exist in the Dispatcher's
argument parser and they are NOT interchangeable, so this contract MUST name
one. `dispatch` takes exactly one required `--item` and drives that item as an
operator override. `loop` takes a repeatable `--item` list alongside
`--budget`/`--parallel` and narrows the ranked drain to the named set,
inheriting ranked-queue admission and the work-in-progress cap. The console's
per-item verb means "drive THIS item now", which is the `dispatch` semantic; a
drain narrowed to one item would additionally inherit ranking behaviour whose
observable effect on a single-element set would itself have to be specified.
The Dispatcher's own documented split states the same distinction. The console
MUST therefore use `dispatch`, and MUST NOT substitute `loop --item` for it.

**Work-in-progress cap disclosure, which this contract MUST NOT leave silent.**
`dispatch --item` is an operator override and does NOT enforce the
work-in-progress cap that the ranked drain enforces. The specification MUST
state that consequence explicitly rather than leaving it to be discovered from
the Dispatcher's implementation: a per-item dispatch MAY start a run that the
cap would have refused. Because the console's charter is a truthful operator
surface, the console MUST ALSO surface that the per-item verb bypasses the cap
at the point of invocation, so an operator cannot exercise a cap-bypassing
capability believing it to be cap-governed. Silence on this point would ship an
unspecified capability, which is the defect this clause exists to prevent.

**Policy arming.** The per-item path inherits the drain path's obligation
unchanged: the Dispatcher reads the orchestrator-owned `dispatcher.*` settings
for itself, so the console MUST pass NO per-run policy-arming argument on the
per-item invocation either. Passing one would send an unrecognized argument and
the run would fail.

**Scenario to add to `SPECIFICATION/scenarios.md`**, following Scenario 16's
Given/When/Then form:

    Feature: Per-item dispatch drives one named work-item
      As a LiveSpec operator
      I want the console's per-item dispatch verb to drive exactly the item I selected
      So that dispatching one item is a direct operator override rather than a drain narrowed to one

    Scenario: The per-item launcher names the selected work-item
      Given a repo with a selected work-item in the ready lane
      When the operator requests a per-item dispatch and the console invokes the Dispatcher through its per-item port
      Then the invocation carries the `dispatch` subcommand with the selected work-item id
      And the invocation carries no per-run policy-arming argument
      And no ranked-drain budget or parallelism argument is passed

    Scenario: The per-item verb discloses that it bypasses the work-in-progress cap
      Given a per-item dispatch is available on the selected work-item
      When the operator is offered the verb
      Then the console surfaces that a per-item dispatch is an operator override which does not enforce the work-in-progress cap

Because this project links spec headings to coverage entries, accepting this
proposal MUST also add the new headings to `tests/heading-coverage.json` in the
same revision; a spec-only edit that omits them will fail the repository's
own completeness gate.

## Proposal: A not-wired stub is not a conformant per-item dispatch

### Target specification files

- SPECIFICATION/contracts.md

### Summary

Add a conformance clause stating that once a per-item launcher argv is specified, a port that unconditionally reports a not-wired outcome does not conform to it, while preserving the existing rule that a genuinely unachievable effect must still be reported honestly rather than fabricated. Also requires that a verb the console cannot perform must not be presented as available.

### Motivation

The per-item dispatch port shipped returning a not-wired outcome for every invocation while its menu entry rendered as available with no unavailability marker, so the verb committed with no confirmation and no read-back and dispatched nothing. The acceptance criterion it was built against explicitly permitted an honest not-wired fallback, and the fallback shipped as the only path -- an acceptance criterion a stub can satisfy is not an acceptance criterion. Specifying the argv alone would not close this: without a conformance clause, an unconditional refusal remains defensible as honest reporting. The distinction between honest failure and a permanent refusal standing in for an implementation must be normative.

### Proposed Changes

Add to the same new `### Per-item factory-dispatch launcher argv` section a
conformance clause closing the gap that the current stub occupies.

**A stub MUST NOT be a conformant implementation of a specified argv.** Once
this contract names a concrete per-item launcher argv, an implementation whose
per-item port unconditionally reports a not-wired outcome MUST NOT be treated as
conforming to it. The port MUST invoke the argv this contract names.

**The honest-outcome rule still applies, and is NOT weakened by the above.** The
existing obligation stands: a port MUST NOT emit a success or outcome event for
an effect it did not achieve, and MUST surface a not-observed / unimplemented
outcome or a typed failure instead. So a per-item dispatch whose Dispatcher
invocation genuinely cannot run -- an absent binary, an unavailable probe --
MUST still report an honest not-wired or failed outcome. What this clause
forbids is the UNCONDITIONAL not-wired path: reporting not-wired for every
invocation, including those the specified argv could have served.

**Why this clause is necessary rather than implied.** The console's per-item
port was shipped returning a not-wired outcome unconditionally, while its menu
entry rendered as available and carried no unavailability marker. The
acceptance criterion it was built against permitted "an honest not_wired
fallback", and the fallback shipped as the only path -- so an operator invoking
the verb saw a row commit with no confirmation and no read-back while nothing
dispatched. An acceptance criterion a stub can satisfy is not an acceptance
criterion. This clause makes the distinction normative: honest reporting of a
genuine failure is required, and a permanently-honest refusal in place of an
implementation is non-conformance.

**Availability MUST match capability.** A per-item dispatch verb the console
cannot actually perform MUST NOT be presented to the operator as available. The
console MUST either offer the verb and invoke the specified argv, or mark it
unavailable; it MUST NOT render an available verb whose only outcome is a
refusal.
