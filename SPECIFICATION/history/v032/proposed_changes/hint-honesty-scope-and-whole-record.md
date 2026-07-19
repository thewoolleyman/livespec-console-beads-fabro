---
topic: hint-honesty-scope-and-whole-record
author: claude-opus-4-8
created_at: 2026-07-19T14:05:00Z
---

## Proposal: Scope the hint-honesty rule to what the console can enforce, and mean "every field" literally

### Target specification files

- SPECIFICATION/contracts.md
- SPECIFICATION/scenarios.md
- tests/heading-coverage.json

### Summary

Corrects two defects the v031 record drill-in shipped, both found by independent
adversarial review of the implementing branch before merge. (1) The v031
hint-honesty sentence ("a key that is inert there MUST NOT be listed") was
violated at ship time by the very hints v031 edited: the lane OVERVIEW
advertised six per-item keys that are inert there, and an empty drilled-in lane
advertised `enter item` where Enter opens nothing. The sentence is re-worded to
state the rule the console can actually enforce — no per-item key where no
work-item is selected — and to say explicitly that per-item-STATE suppression is
NOT promised, because it depends on a valid-verb vocabulary this console does
not yet consume. The implementation is fixed in lockstep. (2) The v031 record
clause says the surface MUST render every field of the standardized shape, but
five fields the orchestrator emits on every record (`acceptance_criteria`,
`notes`, `supersedes`, `blocked_reason`, `factory_safety`) were neither parsed
nor rendered, and three more (`lane_reason`, `admission_policy`,
`acceptance_policy`) were available but unrendered. No clause text changes for
this half — the implementation is brought up to the clause. Scenario 23 gains a
case for the no-selection hint. Clause count is UNCHANGED at 15/77/22/52 = 166;
one clause re-link is owed because the Status-line clause text changes again
(`gap-iicnbdqd` -> `gap-7heyl2dr`).

### Motivation

Both defects are the same failure the v031 change set out to fix, committed by
the fix itself.

The hint one is the sharper. v031 added a rule forbidding a hint that names an
action a key does not take, motivated by `enter drill` appearing where Enter was
inert. That rule, as written, condemned six other hints on the same screen: the
lane overview's `s move-status`, `p/c/r approve/accept/reject`, and
`m/n set-admission/acceptance` all act only on a selected work-item, and the
overview's selection is a LANE, so all six do nothing. An empty drilled-in lane
was likewise advertising the new `enter item`. Scenario 23's case 6 asserted
"no hint advertises a key that is inert in that context", but its test only
checked the `enter` strings, so the suite passed while the clause was violated —
a test that proved less than the sentence it was registered against.

The rule is kept and the implementation fixed; what changes is that the clause
now draws its line where the console can hold it. Suppressing a key that is
inert for the SPECIFIC selected item (a status move a `done` item cannot be
driven through) needs the per-state valid-verb vocabulary owned by
`livespec-orchestrator-beads-fabro`, which this console does not consume yet;
promising it would put the spec back in the position of asserting something the
implementation does not do. The clause says so outright rather than leaving the
gap for a later reader to discover.

The record one is plainer: "every field" has to mean every field. The tenant's
own data has a non-null `notes` today (a long operator regroom note on
`livespec-console-beads-fabro-vfd`) that the modal was hiding with no
placeholder — the exact "undisplayed masquerading as unset" the v031 clause
forbids. The five missing fields also had to join the record digest, or an edit
to them would never appended a fresh observation even once rendered.

### Proposed Changes

FIX 1 — SPECIFICATION/contracts.md, TUI Contract, Status-line hints clause. The
inert-key sentence is re-worded: the general prohibition and its rationale stay;
the enforceable cases are named (a key whose action differs between contexts
must be described by the action it performs there; a key that acts only on a
selected work-item must not be listed where none is selected, which includes a
lane overview and an empty drilled-in lane); and finer-grained per-item-state
suppression is explicitly declared OUT of the clause, with the reason.

FIX 2 — SPECIFICATION/scenarios.md, Scenario 23. The Status-line case is split
in two: one asserting the drill-in wording per sub-view, and a new one asserting
that no per-item key is advertised where no work-item is selected (lane overview
and empty drilled-in lane).

FIX 3 — tests/heading-coverage.json. The Status-line clause re-links
`gap-iicnbdqd` -> `gap-7heyl2dr` (same clause, amended text) and its reason
records what the added rule now covers and what it deliberately does not.

No change is proposed to the record clause: it already says "every field", and
the implementation is corrected to satisfy it.

### Verification

`just check` green: format, strict clippy, workspace tests, 100% line coverage,
cargo-deny, cargo-machete, architecture rules, behavioral coverage (0 unlinked,
0 untested), settings completeness, baseline, doctor static checks.
