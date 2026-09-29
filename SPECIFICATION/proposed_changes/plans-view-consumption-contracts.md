---
topic: plans-view-consumption-contracts
author: claude-fable-5-1
created_at: 2026-09-29T07:38:00Z
---

## Proposal: Declare what the Plans view consumes -- the `context` and `list-plans` reads, the plan-record fields, counts as aggregation, "upstream dependency", and the CLI in the "epic" ban

### Target specification files

- SPECIFICATION/spec.md
- SPECIFICATION/contracts.md
- SPECIFICATION/scenarios.md

### Summary

v053 ratified the Plans view but left its consumption undeclared. This
proposal adds the two missing adapter reads (`context`, `list-plans`) to
§"Initial Adapters" and the source-contracts diagram; names the plan-record
fields the console consumes from `list-work-items` and binds their absence to
the existing leave-un-built rule; distinguishes a COUNT over consumed child
records (permitted) from a SCALAR substituted from another field (forbidden);
defines "upstream dependency" from the consumed `blocked` lane and the
`upstream-dep:` label; rewrites two sentences from the orchestrator's voice
into consumption voice; names CLI usage, help and report strings inside the
"epic" ban; splits the 37-line Plans paragraph at its sentence boundaries with
no wording change; re-wraps the Roster entry; and adds the roster's
`done`-exclusion scenario. Every item is a post-step doctor finding on v053
(objective and subjective phases, 2026-09-29) or the ratification reviewer's
one untaken note.

### Motivation

The v053 post-step doctor ran both LLM-driven phases over the v052→v053
delta. Objective findings: the drill-in and research-directory clauses
require `context` and `list-plans` reads that no adapter clause introduces
(contracts.md §"Initial Adapters" and the source-contracts diagram still list
only `list-work-items --json` and `needs-attention --json`); the roster's
field vocabulary (`plan_slug`, `next_action`, `last_session`, `rank`,
`parent`) appears only in Scenario 33 and a Terminology aside, never in a
clause that declares it consumed; "children's counts per lane" and "count of
its children that are blocked on an upstream dependency" sit in the same
sentence as "MUST render as explicitly absent rather than be computed", so an
implementer cannot tell permitted aggregation from forbidden computation;
"upstream dependency" is undefined; and two sentences assert upstream
obligations (what `list-work-items` returns; the beads storage type and
`plan_slug`) in the console's voice. Subjective findings: shipped CLI strings
print `<epic-id>` and the "epic" ban does not say whether CLI text is bound;
the Plans paragraph carries six rules in one block; the Roster entry is
un-wrapped. The ratification reviewer's untaken note: no scenario excludes a
`done` plan from the roster although v053 defines "open plan" by it.

The code-side contradictions the same doctor pass surfaced (the `<epic-id>`
strings and the `plans <id>` CLI page as a second listing) are dispositioned
as a work-item under `livespec-console-beads-fabro-adf4nu`, not here; this
proposal only makes the clause name the surfaces it binds.

### Proposed Changes

#### 1. `SPECIFICATION/contracts.md`, §"Initial Adapters"

After the Work-items adapter clause (the one that names `list-work-items
--json` and its `lane` / `lane_reason`), insert:

    **Plan reads.** Two further orchestrator reads serve the `Plans` view
    and nothing else. The `context <plan_slug | work-item-id> --json`
    envelope is read ON DEMAND for the selected roster row and carries the
    record, its comments (handoff and scope entries), its children, its
    dependency edges, its typed next action, and its research-directory
    entry; the console renders the envelope as the drill-in and MUST NOT
    re-derive any part of it from another read. The `list-plans --json`
    surface is a pure directory enumeration -- it consults no ledger -- and
    supplies exactly one attribute per plan: whether a `plan/<slug>/`
    research directory exists. Both reads are owned by
    `livespec-orchestrator-beads-fabro` and consumed verbatim under the
    control-plane clause of `spec.md`.

    **Plan-record fields.** The roster's plan identity (`plan_slug`), its
    typed next action (`next_action` with `kind`, `ref`, `text`), the
    session that last wrote it (`last_session`), its rank, and each child's
    parent reference (`parent`) are fields of the orchestrator's published
    `list-work-items --json` shape, consumed verbatim. Where the
    orchestrator has not yet published one of them, the console MUST leave
    that cell un-built -- rendered as explicitly absent -- rather than
    derive it from comments, timestamps, or any other field, and the gap
    MUST be tracked as an upstream dependency.

In the source-contracts `mermaid` diagram of the same section, add two nodes
beside the existing `list-work-items --json` node: `context --json (on
demand, per selected plan)` and `list-plans --json (research-dir attribute)`.

#### 2. `SPECIFICATION/contracts.md`, §"TUI Contract", the `Plans` paragraph

(a) Replace

    `Plans` view MUST render the roster -- one row per open plan (a plan whose
    consumed lane is not `done`) the orchestrator's `list-work-items` surface
    returns, rank-ordered -- and each row MUST show, from the consumed records
    alone, the plan's title, its typed next action (kind and text), the
    session that last wrote it and how long ago, its children's counts per
    lane, and the count of its children that are blocked on an upstream
    dependency; a field the orchestrator did not emit MUST render as
    explicitly absent rather than be computed or omitted.

with

    `Plans` view MUST render the roster -- one row per open plan (a plan whose
    consumed lane is not `done`), in the rank order the orchestrator's
    `list-work-items` surface publishes -- and each row MUST show, from the
    consumed records alone, the plan's title, its typed next action (kind and
    text), the session that last wrote it and how long ago, its children's
    counts per lane, and the count of its children that are blocked on an
    upstream dependency. The two counts are COUNTS OVER THE CONSUMED CHILD
    RECORDS -- each child's published `lane`, and the `upstream dependency`
    marker defined in `spec.md` -- and nothing else; a SCALAR field the
    orchestrator did not emit MUST render as explicitly absent rather than be
    substituted from another field or omitted.

(b) Replace

    The console MUST NOT render the word
    "epic" in any human-facing string: a consumed record whose type is the
    storage type `epic` is rendered as a plan, and its type field as `plan`.

with

    The console MUST NOT render the word "epic" in any human-facing string
    it presents -- in the TUI, and in every CLI usage, help, and report
    string: a consumed record whose type is the storage type `epic` is
    rendered as a plan, and its type field as `plan`.

(c) Split the paragraph, with no wording change beyond (a) and (b), into
five paragraphs at these sentence boundaries: after "...whether or not it
has a plan."; after "...substituted from another field or omitted."; after
"...as `list-plans` reports it."; after "...when the record names no
parent."; the remainder (the two prohibitions and the implementation-detail
sentence).

#### 3. `SPECIFICATION/spec.md`, §Terminology

(a) In the **Plan** entry, replace

    A plan is
    the orchestrator's ledger record for it -- realized in the beads
    substrate as a record of the storage type `epic` carrying the `plan_slug`
    metadata the orchestrator's plan-identity contract requires -- and the
    console renders it under the word "plan" only;

with

    A plan is
    the orchestrator's ledger record for it, identified by the `plan_slug`
    its plan-identity contract (repo
    `thewoolleyman/livespec-orchestrator-beads-fabro`,
    `SPECIFICATION/contracts.md` §"Plan identity") publishes; that contract
    owns the storage realization, and the console renders the record under
    the word "plan" only;

(b) Re-wrap the **Roster** entry at the file's column width, no wording
change.

(c) Insert, directly after the **Roster** entry:

    **Upstream dependency** -- A child work-item that sits in the consumed
    `blocked` lane and carries a consumed label of the form
    `upstream-dep:<tenant>`: the console's proxy for a dependency another
    repository owns. The console recognises it from those two consumed
    facts only and never infers one from a title, a description, or a
    dependency edge.

#### 4. `SPECIFICATION/scenarios.md`, Scenario 33

(a) In "The roster renders one row per open plan from the consumed records",
replace the last line

    And no row shows a value the consumed records did not carry

with

    And every count shown is a count of consumed child records by their published lane or upstream-dependency marker
    And no row shows a scalar value the consumed records did not carry

(b) Append, after "A roster field the orchestrator does not publish stays
un-built":

    Scenario: A finished plan leaves the roster
      Given a plan record whose consumed lane is done
      When the operator opens the Plans view
      Then that plan has no roster row
      And its children remain reachable in Lanes with their plan column filled

    Scenario: The drill-in and research attribute come from their own reads
      Given a selected roster row
      When the detail pane renders
      Then its drill-in is the context envelope for that plan as the orchestrator returned it
      And its research-directory attribute is the list-plans membership as the orchestrator returned it
      And the console derives neither from the list-work-items records

Revision note (implementation coupling, not spec text): the inserted text
adds MUST sentences to `contracts.md` and one to `spec.md`; the revision that
accepts it MUST update the pinned clause counts in
`crates/console-spec-check/src/tests.rs`
(`extract_rules_matches_real_spec_ground_truth`), re-point any Scenario 33
gap ids whose line text changed in `tests/heading-coverage.json`, and bind
the new clauses to Scenario 33 there, in the same change.
