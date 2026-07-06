---
topic: attention-to-needs-attention-rename-and-reconciliation
author: claude-opus-4-8
created_at: 2026-07-06T02:54:10Z
---

This is one atomic proposed change (one `## Proposal:` section, one per-file
revise decision under topic `attention-to-needs-attention-rename-and-reconciliation`).
It does two coupled things that touch the same lines and therefore cannot be
split into independently-ratifiable proposals without conflicting edit targets:
(1) a ubiquitous-language rename of the console concept `Attention` to
`needs-attention`, and (2) reconciliation of the narrow/broad `Attention`
contradiction. It is a SPEC change only: no adapter, port, event, or other code
is specified here (the snapshot port + diff adapter + `attention_item.*` events
are a separate downstream implementation slice).

## Proposal: Rename the `Attention` ubiquitous-language concept to `needs-attention` and reconcile the narrow/broad contradiction

### Target specification files

- SPECIFICATION/spec.md
- SPECIFICATION/contracts.md
- SPECIFICATION/constraints.md
- SPECIFICATION/scenarios.md

### Summary

Rename the console's ubiquitous-language concept `Attention` (the bounded
context and its projection / view / inbox / item / list terms) to
`needs-attention` throughout the spec, aligning the console's language with the
product `needs-attention` surface it consumes; and reconcile the internally
contradictory `Attention` definition -- a narrow *"derived only from a work
item"* definition (`spec.md` §"Terminology" and §"Bounded Contexts") versus a
broad diagram + Scenario 1 that also pull in spec-side revise, source health,
and hygiene. The reconciliation widens the narrow definition to match the broad
picture (`needs-attention` = the product `needs-attention` core: impl-side
work-item signals AND spec-side actions AND repository hygiene all arrive
through the consumed `needs-attention` snapshot), subsumes the separate
"Repository Hygiene → Attention" diagram edge (hygiene now arrives THROUGH
`needs-attention`), and moves the "Ingestion → Attention" source-health edge out
of the inbox (source-health/telemetry belongs to the deferred observability
bounded context, not the attention inbox).

### Motivation

Plain-English semantics: "what needs attention" in this repo is one first-class,
reusable read surface named `needs-attention`, and the console is a CONSUMER of
that surface's point-in-time snapshot, not an independent re-definer of it. The
console spec was authored with an `Attention` bounded context that is internally
contradictory: §"Terminology" and §"Bounded Contexts" define an attention item
as *"derived only from a work item"* (narrow), while the §"Bounded Contexts"
dataflow diagram and Scenario 1 also feed the inbox from source health,
repository hygiene, and spec-side revise (broad). Neither half is a clean,
reusable surface. Renaming to `needs-attention` and widening the definition so
that impl-side work-item signals, spec-side actions, and hygiene all arrive
through the one consumed `needs-attention` snapshot removes the contradiction
and makes the console's language match the surface it observes. Two of the
broad-picture edges are re-attributed rather than kept: hygiene is one of the
primitives the product `needs-attention` composes, so it arrives THROUGH
`needs-attention` and the separate "Repository Hygiene → Attention" edge is
subsumed; source-health/telemetry is an observability concern (a separate,
first-class, DEFERRED Control-Plane bounded context handled by a telemetry
pipeline, never the attention inbox), so the "Ingestion → Attention" edge is
removed from the inbox.

Design record (read in full): repo `thewoolleyman/livespec`,
`plan/needs-attention/research/design.md` -- §"Origin and problem" (the narrow-
vs-broad `Attention` contradiction this proposal reconciles), the rollout entry
for `livespec-console-beads-fabro` (the exact reconciliation: `Attention` = the
product `needs-attention` core; the Repository-Hygiene edge subsumed; the
Ingestion edge belonging to deferred observability), §"Statelessness and the
console event-sourcing boundary" (the console consumes `needs-attention`
snapshots as a source -- the concept honored here; the port/diff/event MECHANICS
are the separate downstream slice, not this proposal), and §"Deferred" (the
deferred observability bounded context that owns source-health/telemetry).

### Proposed Changes

All quoted current text is verbatim from the live spec files (current head
v014). Each target is marked `[RENAME]` (ubiquitous-language rename) or
`[RECONCILIATION]` (widening / edge re-attribution); targets that do both carry
both marks.

#### spec.md

`[RENAME]` **§"Purpose"** -- in the sentence "derives operator projections such
as attention inboxes, cards, timelines, and repository health.", replace:

> attention inboxes, cards, timelines

with:

> needs-attention inboxes, cards, timelines

`[RENAME]` `[RECONCILIATION]` **§"Bounded Contexts"**, the `Attention` bullet --
replace:

> - **Attention** -- a pure inbox derived from work-item lane, lane reason, admission policy, and acceptance policy.

with:

> - **needs-attention** -- the console's consumption of the product
>   `needs-attention` surface: a point-in-time inbox of everything actionable
>   about the repo, sourced from the `needs-attention` snapshot the console
>   ingests. It is not derived from a single work-item lane alone -- the
>   snapshot composes impl-side ready work and the human valves (work-item lane,
>   lane reason, admission policy, acceptance policy), spec-side actions (revise
>   / propose-change / critique / prune-history), and repository hygiene.
>   Source-health/telemetry is an observability concern (deferred), not part of
>   this inbox.

`[RENAME]` `[RECONCILIATION]` **§"Bounded Contexts"** mermaid -- four
coordinated edits:

1. `[RENAME]` relabel the inbox node. Replace `Attention["Attention\nlane-derived inbox"]`
   with `NeedsAttention["needs-attention\nactionable inbox"]` (the node id
   changes to `NeedsAttention` -- mermaid ids cannot contain the hyphen -- and
   the label loses "lane-derived", which the widened definition contradicts).
2. `[RENAME]` repoint the three kept edges to the renamed node:
   `Factory -->|"blocked needs-human lane"| Attention` becomes
   `Factory -->|"blocked needs-human lane"| NeedsAttention`;
   `Grooming -->|"lane derivation inputs"| Attention` becomes
   `Grooming -->|"lane derivation inputs"| NeedsAttention`;
   `WorkItemLifecycle -->|"valve + policy outcome events"| Attention` becomes
   `WorkItemLifecycle -->|"valve + policy outcome events"| NeedsAttention`.
3. `[RECONCILIATION]` DELETE the edge `Ingestion -->|"source health events"| Attention`
   entirely (source-health/telemetry belongs to the deferred observability
   context, not the inbox).
4. `[RECONCILIATION]` DELETE the edge `Hygiene -->|"hygiene findings"| Attention`
   entirely (subsumed -- hygiene now arrives THROUGH `needs-attention`).

   The `Ingestion` node retains its `Config --> Ingestion` edge; the
   `Repository Hygiene` node (`Hygiene`) remains a listed bounded context but no
   longer carries a direct inbox edge (its findings reach the inbox through the
   consumed `needs-attention` snapshot). Leaving `Hygiene` without a direct
   inbox edge is the intended depiction of the subsumption. The
   `Spec -->|"revise / doctor signals"| Spec` self-edge and the `Config -->`
   edges are unchanged.

`[RENAME]` **§"Full Autonomous Mode"** -- two prose occurrences. Replace "are
truly unresolvable by the LLM remain human Attention items." with "are truly
unresolvable by the LLM remain human needs-attention items."; and replace
"decisions MUST continue to appear as Attention items, each carrying its" with
"decisions MUST continue to appear as needs-attention items, each carrying its".

`[RENAME]` **§"Terminology"**, the `Projection` entry -- replace "such as the
attention inbox, work card list" with "such as the needs-attention inbox, work
card list".

`[RENAME]` `[RECONCILIATION]` **§"Terminology"**, the `Attention item` entry --
replace:

> **Attention item** -- A projection entry requiring human review or action,
> derived only from a work item in pending approval with manual admission,
> acceptance with ai-then-human review, or blocked with a needs-human lane reason.

with:

> **needs-attention item** -- A projection entry requiring human review or
> action, sourced from the product `needs-attention` snapshot the console
> consumes -- not derived from a single work-item lane alone. The snapshot
> composes impl-side ready work and the human valves (pending approval with
> manual admission, acceptance with ai-then-human review, blocked with a
> needs-human lane reason), spec-side actions (revise / propose-change /
> critique / prune-history), and repository hygiene. Source-health/telemetry
> findings are an observability concern (deferred), not needs-attention items.

#### contracts.md

`[RENAME]` **§"TUI Contract"**, the required-views list -- replace the item
"- Attention" (first item of the list "- Attention / - Spec / - Lanes / -
Events / - Repos") with "- needs-attention".

`[RENAME]` **§"TUI Contract"**, the default-view sentence -- replace "The
default view MUST be Attention. Navigation SHOULD use arrow-driven" with "The
default view MUST be needs-attention. Navigation SHOULD use arrow-driven".

`[RENAME]` **§"TUI Contract"** mermaid -- two edits. Replace
`Left["Left navigation\nAttention / Spec / Lanes / Events / Repos"]` with
`Left["Left navigation\nneeds-attention / Spec / Lanes / Events / Repos"]`; and
replace `Center["Center list\nlane overview, arrow-selected work cards, or attention items"]`
with `Center["Center list\nlane overview, arrow-selected work cards, or needs-attention items"]`.

#### constraints.md

`[RENAME]` **§"Autonomous-Mode Safety"** -- replace "surface every truly
unresolvable decision as an Attention item, and MUST NOT drop, silently" with
"surface every truly unresolvable decision as a needs-attention item, and MUST
NOT drop, silently" (article `an` → `a`).

#### scenarios.md

`[RENAME]` **Scenario 1 H2 title** -- replace the heading
"## Scenario 1 -- Operator sees one attention inbox" with
"## Scenario 1 -- Operator sees one needs-attention inbox". (Heading-coverage
impact: none -- see the co-edit analysis below.)

`[RENAME]` **Scenario 1 mermaid** -- replace `Projection["Attention projection"]`
with `Projection["needs-attention projection"]`; and replace
`TUI["Attention view"]` with `TUI["needs-attention view"]`.

`[RENAME]` **Scenario 1 gherkin** -- replace "Feature: Unified attention inbox"
with "Feature: Unified needs-attention inbox"; replace "Scenario: Mixed source
signals appear as attention items" with "Scenario: Mixed source signals appear
as needs-attention items"; replace "Then the Attention view lists all three
items" with "Then the needs-attention view lists all three items". (The line "I
want one place to see work requiring my attention" is deliberately unchanged --
see the retained-usages note.)

`[RENAME]` **Scenario 5 mermaid** -- replace `List["Attention list"]` with
`List["needs-attention list"]`.

`[RENAME]` **Scenario 5 gherkin** -- replace "Scenario: Operator inspects a
lane-derived attention item" with "Scenario: Operator inspects a lane-derived
needs-attention item"; replace "Given a selected Attention item is derived from
a blocked needs-human work-item lane" with "Given a selected needs-attention
item is derived from a blocked needs-human work-item lane"; replace "And no
local dismiss command is offered from the attention lens" with "And no local
dismiss command is offered from the needs-attention lens".

`[RENAME]` **Scenario 10 mermaid** -- three coordinated edits. Replace
`Leave["Item leaves Attention inbox"]` with
`Leave["Item leaves needs-attention inbox"]`; replace
`Attention["Stays in Attention with source ref + next action"]` with
`NeedsAttention["Stays in needs-attention with source ref + next action"]`
(node id `Attention` → `NeedsAttention`); and replace the edge
`Mode --> Unresolvable --> Attention` with
`Mode --> Unresolvable --> NeedsAttention`.

`[RENAME]` **Scenario 10 gherkin** -- replace "Scenario: A decidable attention
item is auto-resolved and recorded" with "Scenario: A decidable needs-attention
item is auto-resolved and recorded"; replace "And an attention item derived from
a decision the LLM can resolve" with "And a needs-attention item derived from a
decision the LLM can resolve" (article `an` → `a`); replace "And the item leaves
the Attention inbox" with "And the item leaves the needs-attention inbox";
replace "Then the decision remains an Attention item with its source reference
and next operator action" with "Then the decision remains a needs-attention item
with its source reference and next operator action" (article `an` → `a`).

`[RENAME]` **Scenario 11 gherkin** -- replace "Given a `pending-approval`
work-item whose effective admission_policy is manual, shown in Attention" with
"Given a `pending-approval` work-item whose effective admission_policy is
manual, shown in needs-attention".

### Heading-coverage co-edit analysis

This proposal renames one `## ` H2 heading (`scenarios.md` Scenario 1's title).
`tests/heading-coverage.json` registers only Scenarios 6, 7, and 11 -- none of
whose titles contain "attention" -- so this proposal changes no registered
scenario title, adds and removes no scenario, and therefore requires NO
`tests/heading-coverage.json` co-edit. (The mechanical guard is
`console-spec-check` under the `LIVESPEC_BEHAVIOR_SCENARIO_LINK` lever, which
enforces scenario→test only for registered scenarios and defaults to `warn`.)
No other `## ` heading changes anywhere in this proposal.

### Deliberately-retained natural-English "attention" usages

These occurrences use "attention" as ordinary English, not as the proper-noun
concept, and are intentionally NOT renamed (flagged so the omission reads as a
decision, not an oversight):

- `spec.md` §"Purpose": "What needs attention now?" (a plain operator question;
  already phrased as "needs attention").
- `spec.md` §"Scope Boundary": "human-attention routing and notification-ready
  alert semantics" ("human-attention routing" denotes routing to a human, an
  ownership responsibility; renaming to "needs-attention routing" would lose the
  human-routing nuance).
- `scenarios.md` Scenario 1 gherkin: "I want one place to see work requiring my
  attention" (plain English).

### Implementation / downstream note

Recorded, NOT filed here: the console's consumption of `needs-attention` as a
snapshot source -- the snapshot port, the diff-at-ingest adapter, and the
`attention_item.appeared` / `.changed` / `.resolved` events keyed by a stable
id -- is a separate downstream implementation slice (per the design record's
§"Statelessness and the console event-sourcing boundary"), not part of this
spec-only rename-and-reconciliation. This proposal keeps the spec's concept of
`needs-attention` as a consumed snapshot source coherent with that boundary
without specifying its wire mechanics.
