---
topic: work-item-record-drill-in
author: claude-opus-4-8
created_at: 2026-07-19T13:20:00Z
---

## Proposal: The operator can read a work-item's full standardized record inside the console

### Target specification files

- SPECIFICATION/contracts.md
- SPECIFICATION/scenarios.md
- tests/heading-coverage.json

### Summary

Adds the missing rule that the console must let an operator read a selected
work-item's FULL standardized record — its title and description above all —
without leaving the cockpit, and tightens the Status-line contract so a hint may
not advertise a key the key does not perform in that context. One clause is
ADDED (the record surface, `gap-lu5ergzl`); one clause is AMENDED (the
Status-line hints clause, whose gap id moves `gap-2hiwqz3g` -> `gap-iicnbdqd`
because its text changed); the `Lanes` view prose gains the second drill level.
Scenario 23 is added with six cases and registered against the render-level
proof in `crates/console-tui/src/lib.rs`. Net clause count: +1.

### Motivation

The console shipped a `Lanes` view whose drilled-in lane rows render only id /
repo / rank / status (+ lane reason). Nothing anywhere in the console showed a
work-item's title, let alone its description — so an operator looking at
`livespec-console-beads-fabro-vc7lmq` could not tell what the item WAS without
leaving the console for `bd` or the ledger. That is a hole in the cockpit's
whole premise: the console exists so the operator does not have to leave it.

The gap ran deeper than rendering. `parse_orchestrator_observation` deserialized
only seven fields (`id`, `lane`, `lane_reason`, `rank`, `status`, and the two
policies) and serde silently discarded everything else, even though
`list-work-items --json` already puts the entire standardized record on the wire.
The wire format and its read-back likewise carried no descriptive half, so the
data was thrown away three layers before the renderer. No spec clause required
otherwise, which is why the hole was invisible to every existing check.

The Status-line amendment is the second half of the same defect. The `Lanes`
hint read `enter drill` in BOTH sub-views, but inside a drilled-in lane `Enter`
was explicitly inert — the hint named an action the key did not take, and an
operator pressing it could not tell a broken key from a mis-documented one. The
existing clause required hints to be "appropriate to the currently-focused
pane"; it did not forbid naming a non-existent action, and a sub-view is not a
pane. The added sentence closes exactly that gap, and is what makes the
regression impossible to reintroduce silently.

Deliberately OUT of scope, per the filed work-item: which ACTIONS the record
surface offers is the state-valid-verbs design (tracked separately). This
proposal governs the DATA drill-in and the honest hint only, with the surface
built so an action list can slot in later.

### Proposed Changes

FIX 1 — SPECIFICATION/contracts.md, `Lanes` view prose. The sentence describing
the hybrid sub-view gains the second drill level, so the view's own description
matches the surface the new clause requires: "with drill-in to a single lane's
full rank-ordered list and, from there, a second drill-in to the selected
work-item's full standardized record."

FIX 2 — SPECIFICATION/contracts.md, TUI Contract. The Status-line hints clause
gains one sentence forbidding a dishonest hint: a key inert in the current
context MUST NOT be listed, and a key whose action differs between two contexts
(INCLUDING between a view's sub-views) MUST be described by the action it
actually performs there. The clause's closing summary gains the word "honest".

FIX 3 — SPECIFICATION/contracts.md, TUI Contract. A new clause is ADDED after
the Status-line clause requiring that the operator can read a selected
work-item's full standardized record without leaving the console: reachable from
the drilled-in lane list; every field of the standardized shape rendered (title,
description, type, status, lane, rank, origin, gap id, assignee, dependencies,
capture time, resolution, reason, audit trail, superseding item, spec commitment
hint); an unemitted field rendered as explicitly ABSENT rather than omitted, so
an unset field is distinguishable from an undisplayed one; the description
carried as emitted rather than reformatted; the surface scrolling when the
record is taller than the viewport. The clause also pins the consumption
direction that the rest of this spec already establishes for lane state: the
standardized shape is OWNED by `livespec-orchestrator-beads-fabro` and consumed
verbatim, so the console MUST NOT re-derive or reformat a field, and MUST NOT
drop a work-item from the board because an unrecognized descriptive field is
absent or unparseable. Key binding, modal geometry, and field ordering stay
implementation details.

FIX 4 — SPECIFICATION/scenarios.md. Scenario 23 is added with six cases: Enter
opens the selected item's record; every standardized field is readable; an
unemitted field reads as absent while the item still lists in its lane; a record
taller than the viewport scrolls to its end and clamps there; Esc closes back to
the drilled-in lane rather than the lane overview; and the Status line names the
action the key actually performs in each sub-view.

FIX 5 — tests/heading-coverage.json. Scenario 19's clause link is re-pointed
from `gap-2hiwqz3g` to `gap-iicnbdqd` (same clause, amended text) and its reason
records that the added honesty rule is proven by the two reducer-level hint
tests. A new entry registers Scenario 23 against
`crates/console-tui/src/lib.rs::work_item_detail_modal_renders_every_standardized_record_field`,
with the sibling render, keymap, reducer, and adapter tests named in its reason.

### Verification

`just check` green: format, strict clippy (pedantic + nursery, `unwrap`/`expect`/
`panic` denied), workspace tests, 100% line coverage, cargo-deny, cargo-machete,
architecture rules, behavioral coverage (0 unlinked, 0 untested), settings
completeness, baseline worktree discipline, and doctor static checks.
