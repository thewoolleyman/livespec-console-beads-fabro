---
topic: plans-as-a-first-class-console-concept
author: claude-fable-5-1
created_at: 2026-09-29T08:30:00Z
---

## Proposal: Plans as a first-class console concept -- define "plan" once, name the consumed surfaces, and add the Plans view with a scoped-Lanes cross-link

### Target specification files

- SPECIFICATION/spec.md
- SPECIFICATION/contracts.md
- SPECIFICATION/scenarios.md

### Summary

Define **plan** and **roster** once, in the console's own vocabulary; extend the
control-plane clause's enumeration of consumed orchestrator surfaces with
`context` and `list-plans`; add `Plans` to the required TUI views as a PEER of
`Lanes` -- a roster of open plans with a drill-in, and one cross-link that opens
`Lanes` scoped to the selected plan -- and give `Lanes` a plan column that is
blank for plan-less items. Bind the vocabulary ruling that the console renders
the word "plan" and never the storage word "epic". Add Scenario 33 to carry the
new clauses. This is the shape-1+ decision recorded as a scope event on plan
epic `livespec-console-beads-fabro-pzbdbo` on 2026-09-29 (design item
`livespec-console-beads-fabro-adf4nu`; analysis in
`plan/retire-overseer-and-redesign-control-plane-around-console/research/012-plans-as-a-first-class-console-concept.md`).

### Motivation

The v046 control-plane clause (spec.md, "Control-plane surface") promises "the
roster of plans and their work state" in three undefined words, routed to an
enumeration of orchestrator surfaces that names neither the drill-in surface
(`context`, orchestrator v097) nor the plan-directory enumerator
(`list-plans`), so the clause's own "leave it un-built" escape hatch reads as
satisfied and the shipped TUI has no plan surface at all. Scenario 29 renders
"roster" without binding it to anything. The stronger obligation sits on the
producer's side -- orchestrator v095 ("Plan identity"): the plan slug is "the
human-readable handle that listings, tooling, and the Control-Plane surface
anchor to" -- and was never mirrored here. A three-word clause routed to the
wrong enumeration is precisely how the requirement got lost; this proposal
defines the term where the console reads it.

Two rulings from 2026-09-29 shape the text. **One term:** "plan" is the domain
word everywhere a human reads; "epic" is a beads storage type that survives
only at the orchestrator's realization boundary, so the console never renders
it. **Plan-less work-items are the ordinary case:** orchestrator v095 forbids
duplicating `plan_slug` onto a non-epic ("its plan is its parent chain"); it
does not require a parent to exist, no doctor rule or hygiene fact flags a
parentless item, and 18 of this tenant's 31 open leaf items had no parent plan
on 2026-09-29. A plan is therefore an OPTIONAL tier above the work-item, and the
chosen shape keeps `Lanes` the complete home of every item rather than routing
the majority of the work through a pseudo-plan. The peer-view shape is a strict
subset of the container shape (Plans over Lanes) and can grow into it if
dogfood shows the plan tier should be the entry point; the reverse would be a
rewrite.

The console consumes; it never derives. What the surfaces publish today was
verified against the installed orchestrator package on 2026-09-29, and the
roster is only PARTLY served. `list-work-items --json` emits `title`, `type`,
`status`, `lane`, `rank`, `parent`, and `labels` for every record, plan records
included, so per-lane child counts and the blocked-upstream count are
renderable from consumed rows; but its projection serialises a typed
work-item that carries no `metadata`, so a plan's `plan_slug`, `next_action`,
and `last_session` are dropped before they reach the wire
(`store.py:_record_to_work_item` cherry-picks four metadata keys; the `WorkItem`
type has no field for the plan keys). The `context` envelope publishes the
slug and the typed `next_action`, but reads `last_session` nowhere.
`list-plans` emits bare directory names and consults no ledger, which is all
the research-directory attribute needs. So three roster fields -- `plan_slug`,
`next_action`, `last_session` on the listing, and `last_session` on the
envelope -- are orchestrator gather facts the console must wait for: filed
upstream with a console proxy that the implementation child depends on, never
computed here from comments or timestamps. Until they land, those cells render
as explicitly absent, which the clause below requires and Scenario 33 pins.

### Proposed Changes

#### 1. `SPECIFICATION/spec.md`, §Scope Boundary, the **Control-plane surface** paragraph

Replace the parenthetical enumeration

    (`needs-attention`, `list-work-items`, `next`, the settings and
    valve command surface, and any surface the orchestrator ratifies later)

with

    (`needs-attention`, `list-work-items`, `next`, `context`, `list-plans`,
    the settings and valve command surface, and any surface the
    orchestrator ratifies later)

and, in the same sentence, replace "the roster of plans and their work state"
with "the roster of plans (see Terminology) and their work state". No other
word of the paragraph changes.

#### 2. `SPECIFICATION/spec.md`, §Terminology

Insert two entries directly BEFORE the **needs-attention item** entry:

    **Plan** -- The optional tier above the work-item: one goal with its
    children, its typed next action, and its handoff timeline. A plan is
    the orchestrator's ledger record for it -- realized in the beads
    substrate as a record of the storage type `epic` carrying the
    `plan_slug` metadata the orchestrator's plan-identity contract requires
    -- and the console renders it under the word "plan" only; a
    `plan/<slug>/` research directory is an optional, write-once attribute
    the orchestrator's `list-plans` surface reports. A plan is a PARENT; a
    lane is a STATUS; a work-item may have both, either, or neither. A
    work-item with no parent plan is the ordinary case, not a finding.

    **Roster** -- The list of open plans the orchestrator's `list-work-items`
    surface returns, rank-ordered, each with the work state the surface
    publishes for it. The roster is rendered, never derived: the console
    does not enumerate plans from any raw ledger read.

#### 3. `SPECIFICATION/contracts.md`, §TUI Contract

(a) In the `Required TUI views` list, insert `- Plans` directly after
`- Lanes`.

(b) Directly AFTER the paragraph that begins "The `Lanes` view is the work-item
consumer" and ends "`Spec`, `Events`, and `Repos` remain as orthogonal,
non-lane views.", insert:

    The `Plans` view is the plan consumer and a PEER of `Lanes`: a plan is a
    vertical slice (one goal, items in every lane) and a lane a horizontal one
    (one lifecycle state, items from every plan), and `Lanes` remains the
    complete home of every work-item whether or not it has a plan. The
    `Plans` view MUST render the roster -- one row per open plan the
    orchestrator's `list-work-items` surface returns, rank-ordered -- and each
    row MUST show, from the consumed record alone, the plan's title, its
    typed next action (kind and text), the session that last wrote it and how
    long ago, its children's counts per lane, and the count of its children
    that are blocked on an upstream dependency; a field the orchestrator did
    not emit MUST render as explicitly absent rather than be computed or
    omitted. Selecting a roster row MUST render that plan's drill-in in the
    detail pane from the orchestrator's `context` envelope -- the record, its
    handoff and scope timeline, its children, its dependency edges, and its
    typed next action -- together with whether a research directory exists as
    `list-plans` reports it. From a roster row the operator MUST be able to
    open the `Lanes` view scoped to that plan's children, with the plan's slug
    visible in the pane title, and MUST be able to clear the scope and return
    to the roster; the scoped `Lanes` offers every per-item verb the unscoped
    one offers, under the same availability predicates. Every `Lanes` row
    MUST carry a plan column naming the parent plan's slug when the consumed
    record names a parent, and that column MUST be blank -- carrying no flag,
    marker, or finding -- when it does not. The console MUST NOT render the
    word "epic" in any human-facing string: a consumed record whose type is
    the storage type `epic` is rendered as a plan, and its type field as
    `plan`. The console MUST NOT offer a rendering of a plan outside the
    `Plans` view and the per-item record surface, so the roster and its
    drill-in are the one specified answer to "what is this plan waiting on,
    and what did the last session leave me". The specific key bindings,
    column widths, and pane geometry are an implementation detail.

(c) In the `mermaid` flowchart of the TUI default screen, change the `Left`
node text from

    needs-attention / Spec / Lanes / Events / Repos / Settings

to

    needs-attention / Spec / Lanes / Plans / Events / Repos / Settings

#### 4. `SPECIFICATION/scenarios.md`

(a) In Scenario 29's `mermaid` flowchart, change the `Orch` node text from

    needs-attention / list-work-items / next / settings + valves

to

    needs-attention / list-work-items / next / context / list-plans / settings + valves

(b) Append, after Scenario 32 (at the end of the file):

    ## Scenario 33 -- A plan is a first-class console concept: the roster, the drill-in, and the scoped lanes

    ```mermaid
    flowchart LR
      LWI["list-work-items: plan records with plan_slug, next_action, last_session, rank, parent, lane"]
      Roster["Plans view: one roster row per open plan"]
      Ctx["context envelope: record, timeline, children, edges, next action"]
      Detail["Detail pane: the plan's drill-in"]
      Scoped["Lanes scoped to the plan's children"]
      LWI --> Roster --> Detail
      Ctx --> Detail
      Roster --> Scoped
    ```

    ```gherkin
    Feature: The operator sees plans as the tier above work-items, without the plan being mandatory
      As an operator
      I want a roster of open plans with each plan's next action and a drill-in, beside the lanes I already use
      So that I can tell what a plan is waiting on and what the last session left me, while plan-less work stays where it always was

    Scenario: The roster renders one row per open plan from the consumed records
      Given the orchestrator's list-work-items surface returns two open plan records carrying plan_slug, a typed next_action, last_session, and rank
      When the operator opens the Plans view
      Then the roster shows two rows in rank order
      And each row shows the plan's title, its next action kind and text, who last wrote it and how long ago, its children's counts per lane, and how many children are blocked on an upstream dependency
      And no row shows a value the consumed records did not carry

    Scenario: Selecting a plan renders its drill-in from the context envelope
      Given a roster row for a plan whose context envelope carries a handoff entry, a scope event, three children, and a typed next action
      When the operator selects that row
      Then the detail pane shows the record, the handoff and scope entries in order, the three children, and the next action
      And it shows whether a research directory exists as list-plans reports it
      And the console composes no part of the drill-in from a raw ledger read

    Scenario: A roster row opens Lanes scoped to that plan and the scope can be cleared
      Given a selected roster row for the plan "alpha-topic"
      When the operator opens the lanes from that row
      Then the Lanes view shows only that plan's children, with "alpha-topic" in the pane title
      And every per-item verb the unscoped Lanes offers for a selected item is offered under the same availability predicate
      When the operator clears the scope
      Then the roster is shown again and the unscoped Lanes is unchanged

    Scenario: A plan-less work-item is ordinary
      Given a ready bug whose consumed record names no parent
      When the operator views it in Lanes
      Then its plan column is blank
      And no flag, marker, or attention finding is rendered for the absence of a plan

    Scenario: The console never renders the storage word
      Given a consumed record whose type field is the storage type "epic"
      When the console renders it anywhere a person reads -- the roster, the record surface, or a lane row
      Then the word rendered for it is "plan"
      And the string "epic" appears in no pane, hint, title, or message

    Scenario: A roster field the orchestrator does not publish stays un-built
      Given a plan record that carries no last_session value
      When the roster renders that plan's row
      Then the last-session cell renders as explicitly absent
      And the console does not compute a substitute from comments, timestamps, or any other field
    ```

Revision note (implementation coupling, not spec text): the inserted text adds
MUST sentences to `spec.md` and `contracts.md`; the revision that accepts it
MUST update the pinned per-file and total clause counts in
`crates/console-spec-check/src/tests.rs`
(`extract_rules_matches_real_spec_ground_truth`) and bind each new clause's gap
id to "Scenario 33" in `tests/heading-coverage.json` (with a reasoned pending
top-of-pyramid entry until the implementation children land) in the same
change, or `check-spec`, `check-test`, `check-nextest` and `check-coverage`
fail on a markdown-only diff. Implementation follows as children of
`livespec-console-beads-fabro-adf4nu`, the first being the roster row for
`plan_slug` `retire-overseer-and-redesign-control-plane-around-console` itself,
whose rendered-text acceptance criteria are derived from the STATUS RE-PRINT a
restarted session prints by hand today. The CLI's unlisted static HTML page
(`plans <epic-id>`) is removed by the child that lands the roster, per the
"no rendering outside the Plans view" clause.
