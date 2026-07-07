---
topic: needs-attention-snapshot-port-and-diff-events
author: claude-opus-4-8
created_at: 2026-07-06T22:40:25Z
spec_commitments:
  impl_followups:
    - id_hint: cn1-needs-attention-snapshot-port-diff-events
      description: |
        Implement the console needs-attention snapshot-source port (reads the product `needs-attention --json` surface owned by the orchestrator plugin), the diff-at-ingest adapter, and the attention_item.appeared/.changed/.resolved canonical events (keyed by the stable id; idempotent -- an unchanged id emits nothing), re-sourcing the needs-attention projection from the diffed attention_item.* stream instead of the current lane-derived work_item snapshots. Land the Scenario 12 acceptance/integration test and replace its TODO entry in tests/heading-coverage.json when the slice lands.
---

## Proposal: Add the needs-attention snapshot-source port, diff-at-ingest adapter, and attention_item.* events (CN1)

### Target specification files

- SPECIFICATION/spec.md
- SPECIFICATION/contracts.md
- SPECIFICATION/scenarios.md
- tests/heading-coverage.json

### Summary

Add the console contract surface that SP2 (v015) explicitly deferred as a separate downstream implementation slice: the product `needs-attention` snapshot-source port, the diff-at-ingest adapter, and the `attention_item.appeared` / `.changed` / `.resolved` canonical events (keyed by each item's stable id, idempotent -- an unchanged id emits nothing). It also folds in SP2 review follow-up 1 by adding the omitted open `plan/<topic>` threads primitive to the needs-attention composed-set definitions, and registers a new Scenario 12 (modeled on Scenario 4's snapshot-without-transition-history pattern) with its heading-coverage co-edit. Without this split, CN1's code would introduce a new source/port, a new canonical event family, and a new behavioral journey that the committed spec does not carry -- straddling the governed SPECIFICATION.

### Motivation

A read-only straddle verification (subagent cn1-straddle) found that v015 carries only the needs-attention CONCEPT (bounded context + terminology + the generic Adapter Contract + Scenario 4's work-items snapshot pattern) but NONE of CN1's contract surface: contracts.md §'Initial Adapters' enumerates exactly five sources and five canonical event families (fabro./dispatch./work_item./spec./pr.) with no needs-attention source/port and no attention_item.* family; there is no diff-at-ingest scenario; and the console code still projects attention narrowly from work_item lane snapshots. SP2's own proposal AND the design record both labeled the snapshot port + diff adapter + attention_item.* events 'a separate downstream implementation slice' ('Recorded, NOT filed here') -- deferred spec surface, not pre-covered code. This proposal files that surface so CN1 is a pure code slice. Design record (read in full): repo thewoolleyman/livespec, plan/needs-attention/research/design.md §'Statelessness and the console event-sourcing boundary' (the port + diff-at-ingest + attention_item.* events keyed by a stable id), §'The attention_item schema' (the id/kind/urgency/summary/source_ref/handoff shape and the stable-id diff key), and §'Read primitives needs-attention composes' (the plan/<topic> threads primitive folded in here as SP2 review follow-up 1).

### Proposed Changes

All quoted current text is verbatim from the live console spec files (current
head v015). Each target is marked `[NEW]` (new contract surface CN1 requires),
`[FOLD-IN]` (SP2 review follow-up 1 -- the omitted `plan/<topic>` primitive),
`[DRIFT-SWEEP]` (an unamended statement re-cast so it no longer contradicts the
new port contract), or `[CO-EDIT]` (the mechanical heading-coverage map).

This is one atomic proposed change (one `## Proposal:` section, one per-file
revise decision under this topic). It adds the console contract surface that
SP2 (v015) explicitly deferred as "a separate downstream implementation slice"
(the snapshot port + diff adapter + `attention_item.*` events), so CN1's code
does not straddle the governed spec.

#### spec.md

`[FOLD-IN]` `[NEW]` **§"Bounded Contexts"**, the `needs-attention` bullet --
replace:

> - **needs-attention** -- the console's consumption of the product
>   `needs-attention` surface: a point-in-time inbox of everything actionable
>   about the repo, sourced from the `needs-attention` snapshot the console
>   ingests. It is not derived from a single work-item lane alone -- the
>   snapshot composes impl-side ready work and the human valves (work-item lane,
>   lane reason, admission policy, acceptance policy), spec-side actions (revise
>   / propose-change / critique / prune-history), and repository hygiene.
>   Source-health/telemetry is an observability concern (deferred), not part of
>   this inbox.

with:

> - **needs-attention** -- the console's consumption of the product
>   `needs-attention` surface: a point-in-time inbox of everything actionable
>   about the repo, sourced from the `needs-attention` snapshot the console
>   ingests. It is not derived from a single work-item lane alone -- the
>   snapshot composes impl-side ready work and the human valves (work-item lane,
>   lane reason, admission policy, acceptance policy), spec-side actions (revise
>   / propose-change / critique / prune-history), open `plan/<topic>` threads,
>   and repository hygiene. The console consumes this snapshot through a
>   dedicated snapshot-source port and diffs it at ingest, emitting
>   `attention_item.appeared` / `.changed` / `.resolved` events keyed by each
>   item's stable id (the wire form lives in `contracts.md` -> Initial
>   Adapters). Source-health/telemetry is an observability concern (deferred),
>   not part of this inbox.

`[NEW]` **§"Architecture"**, the hexagonal-boundary mermaid -- two coordinated
edits so the illustrative adapter set stays in step with the authoritative
Initial-Adapters list. First, in the `Outer adapters` subgraph, replace:

> ```
>     GithubAdapter["GitHub adapter"]
>     SqliteAdapter["SQLite event-store adapter"]
> ```

with:

> ```
>     GithubAdapter["GitHub adapter"]
>     NeedsAttentionAdapter["needs-attention adapter"]
>     SqliteAdapter["SQLite event-store adapter"]
> ```

Second, in the edge list, replace:

> ```
>   GithubAdapter --> Ports
>   SqliteAdapter --> Ports
> ```

with:

> ```
>   GithubAdapter --> Ports
>   NeedsAttentionAdapter --> Ports
>   SqliteAdapter --> Ports
> ```

`[DRIFT-SWEEP]` **§"Architecture"**, the earlier `source systems -> pull adapters`
mermaid (the overall event-flow diagram) -- add the needs-attention source and
adapter so the adapter enumeration stays consistent with the Initial-Adapters
list. First, in the `Source systems` subgraph, replace:

> ```
>     FR["Fabro API / ps / SSE"]
>     GH["GitHub API"]
>   end
> ```

with:

> ```
>     FR["Fabro API / ps / SSE"]
>     GH["GitHub API"]
>     NA["Orchestrator needs-attention CLI\n(needs-attention --json)"]
>   end
> ```

Second, in the `Pull adapters` subgraph, replace:

> ```
>     FRA["Fabro adapter"]
>     GHA["GitHub adapter"]
>   end
> ```

with:

> ```
>     FRA["Fabro adapter"]
>     GHA["GitHub adapter"]
>     NAA["needs-attention adapter"]
>   end
> ```

Third, in the edge list, replace:

> ```
>   GH --> GHA --> Log
>   Log --> Proj --> Frontends
> ```

with:

> ```
>   GH --> GHA --> Log
>   NA --> NAA --> Log
>   Log --> Proj --> Frontends
> ```

`[FOLD-IN]` **§"Terminology"**, the `needs-attention item` entry -- replace:

> **needs-attention item** -- A projection entry requiring human review or
> action, sourced from the product `needs-attention` snapshot the console
> consumes -- not derived from a single work-item lane alone. The snapshot
> composes impl-side ready work and the human valves (pending approval with
> manual admission, acceptance with ai-then-human review, blocked with a
> needs-human lane reason), spec-side actions (revise / propose-change /
> critique / prune-history), and repository hygiene. Source-health/telemetry
> findings are an observability concern (deferred), not needs-attention items.

with:

> **needs-attention item** -- A projection entry requiring human review or
> action, sourced from the product `needs-attention` snapshot the console
> consumes -- not derived from a single work-item lane alone. The snapshot
> composes impl-side ready work and the human valves (pending approval with
> manual admission, acceptance with ai-then-human review, blocked with a
> needs-human lane reason), spec-side actions (revise / propose-change /
> critique / prune-history), open `plan/<topic>` threads, and repository
> hygiene. Source-health/telemetry findings are an observability concern
> (deferred), not needs-attention items.

#### contracts.md

`[NEW]` **§"Initial Adapters"**, the adapter list -- add the needs-attention
adapter after the GitHub adapter. Replace:

> - **LiveSpec adapter** -- reads spec-side `next`, doctor output,
>   proposed changes, history, and filesystem/git state.
> - **GitHub adapter** -- reads PR, check, branch, and merge state.

with:

> - **LiveSpec adapter** -- reads spec-side `next`, doctor output,
>   proposed changes, history, and filesystem/git state.
> - **GitHub adapter** -- reads PR, check, branch, and merge state.
> - **needs-attention adapter** -- reads the product `needs-attention` snapshot
>   through the orchestrator CLI (`needs-attention --json`, one point-in-time
>   read of the flat `attention[]` array; each item carries its stable `id`,
>   `kind`, `urgency`, `summary`, `source_ref`, and `handoff`) and DIFFS that
>   snapshot against the last ingested one at ingest, emitting
>   `attention_item.appeared` (an `id` not previously present),
>   `attention_item.changed` (a present `id` whose composed content changed),
>   and `attention_item.resolved` (a previously-present `id` now absent), each
>   keyed by the stable `id`. The diff is idempotent: an unchanged `id` emits
>   nothing. The `needs-attention` surface is stateless / point-in-time (no
>   timestamps, no events, no history) and re-derives none of the primitives it
>   composes (impl-side ready work, the human valves, spec-side actions, open
>   `plan/<topic>` threads, repository hygiene); the console consumes the
>   composed snapshot verbatim, and this diff-at-ingest is what turns the
>   point-in-time snapshots into a durable event stream -- ALL event-sourcing
>   lives in the console. This mirrors the Work-items adapter's
>   snapshot-without-transition-history pattern (`scenarios.md` Scenario 4 and
>   the new Scenario 12; design record: repo `thewoolleyman/livespec`,
>   `plan/needs-attention/research/design.md` §"Statelessness and the console
>   event-sourcing boundary"). The `needs-attention` CLI surface is owned by the
>   orchestrator plugin, not the console; the console MUST NOT reach around this
>   port to recompute the inbox.

`[NEW]` **§"Initial Adapters"**, the source-to-canonical-stream mermaid -- add
the needs-attention source, adapter, and event stream (one new node in each
subgraph plus one edge). Replace:

> ```
>   subgraph SourceContracts["Source contracts"]
>     Fabro["Fabro API / run events"]
>     Dispatcher["Dispatcher journal JSONL"]
>     WorkItems["orchestrator list-work-items --json"]
>     LiveSpec["/livespec next / doctor / files"]
>     GitHub["PR / check / merge API"]
>   end
>
>   subgraph AdapterContracts["Adapter contracts"]
>     FA["Fabro adapter"]
>     DA["Dispatcher adapter"]
>     WIA["Work-items adapter"]
>     LA["LiveSpec adapter"]
>     GA["GitHub adapter"]
>   end
>
>   subgraph Canonical["Canonical console stream"]
>     RunEvents["fabro.* events"]
>     DispatchEvents["dispatch.* events"]
>     WorkEvents["work_item.* events"]
>     SpecEvents["spec.* events"]
>     PrEvents["pr.* events"]
>   end
>
>   Fabro --> FA --> RunEvents
>   Dispatcher --> DA --> DispatchEvents
>   WorkItems --> WIA --> WorkEvents
>   LiveSpec --> LA --> SpecEvents
>   GitHub --> GA --> PrEvents
> ```

with:

> ```
>   subgraph SourceContracts["Source contracts"]
>     Fabro["Fabro API / run events"]
>     Dispatcher["Dispatcher journal JSONL"]
>     WorkItems["orchestrator list-work-items --json"]
>     LiveSpec["/livespec next / doctor / files"]
>     GitHub["PR / check / merge API"]
>     NeedsAttention["orchestrator needs-attention --json"]
>   end
>
>   subgraph AdapterContracts["Adapter contracts"]
>     FA["Fabro adapter"]
>     DA["Dispatcher adapter"]
>     WIA["Work-items adapter"]
>     LA["LiveSpec adapter"]
>     GA["GitHub adapter"]
>     NAA["needs-attention adapter"]
>   end
>
>   subgraph Canonical["Canonical console stream"]
>     RunEvents["fabro.* events"]
>     DispatchEvents["dispatch.* events"]
>     WorkEvents["work_item.* events"]
>     SpecEvents["spec.* events"]
>     PrEvents["pr.* events"]
>     AttnEvents["attention_item.* events"]
>   end
>
>   Fabro --> FA --> RunEvents
>   Dispatcher --> DA --> DispatchEvents
>   WorkItems --> WIA --> WorkEvents
>   LiveSpec --> LA --> SpecEvents
>   GitHub --> GA --> PrEvents
>   NeedsAttention --> NAA --> AttnEvents
> ```

#### scenarios.md

`[DRIFT-SWEEP]` **Scenario 1 -- Operator sees one needs-attention inbox**,
mermaid + gherkin -- Scenario 1 currently depicts the console recomposing the
needs-attention view by projecting its OWN Fabro / LiveSpec / Dispatcher events
("When the console projects the event log"), which contradicts the new contract
that the console consumes the product snapshot through the needs-attention port
and MUST NOT reach around it to recompute the inbox. Re-cast Scenario 1 so the
three example signals are UPSTREAM inputs the product `needs-attention` snapshot
composes, and the console ingests-and-diffs that snapshot into the
`attention_item.*` stream it projects. The Scenario 1 H2 title and the
`Feature:` block are unchanged (no heading-coverage impact).

First, replace the Scenario 1 mermaid:

> ```mermaid
> flowchart LR
>   Fabro["Fabro human gate"]
>   Spec["LiveSpec revise needed"]
>   Dispatcher["Dispatcher backlog bounce"]
>   Events["Canonical events"]
>   Projection["needs-attention projection"]
>   TUI["needs-attention view"]
>
>   Fabro --> Events
>   Spec --> Events
>   Dispatcher --> Events
>   Events --> Projection --> TUI
> ```

with:

> ```mermaid
> flowchart LR
>   Fabro["Fabro human gate"]
>   Spec["LiveSpec revise needed"]
>   Dispatcher["Dispatcher backlog bounce"]
>   Snapshot["product needs-attention snapshot"]
>   Adapter["needs-attention adapter (diff at ingest)"]
>   AttnEvents["attention_item.* events"]
>   Projection["needs-attention projection"]
>   TUI["needs-attention view"]
>
>   Fabro --> Snapshot
>   Spec --> Snapshot
>   Dispatcher --> Snapshot
>   Snapshot --> Adapter --> AttnEvents --> Projection --> TUI
> ```

Second, replace the Scenario 1 gherkin `Scenario:` block (the `Feature:` block
above it is unchanged):

> ```
> Scenario: Mixed source signals appear as needs-attention items
>   Given the Fabro adapter observes a blocked run with a human gate
>   And the LiveSpec adapter observes pending proposed changes requiring revise
>   And the Dispatcher adapter observes a non-converging item bounced to `backlog` for re-grooming
>   When the console projects the event log
>   Then the needs-attention view lists all three items
>   And each item carries a source reference and next operator action
> ```

with:

> ```
> Scenario: Mixed source signals appear as needs-attention items
>   Given the product needs-attention snapshot composes a blocked Fabro run with a human gate, pending proposed changes requiring revise, and a non-converging item bounced to `backlog` for re-grooming
>   When the needs-attention adapter ingests the snapshot and diffs it into attention_item events
>   Then the needs-attention view lists all three items from the attention_item stream
>   And each item carries a source reference and next operator action
> ```

`[NEW]` **Append a new Scenario 12** after Scenario 11 (at end of file),
modeled on Scenario 4's snapshot-without-transition-history pattern:

> ```
>
> ## Scenario 12 -- needs-attention snapshot diffed at ingest into attention_item events
>
> ```mermaid
> flowchart LR
>   Prior["Last ingested needs-attention snapshot"]
>   Poll["Poll product needs-attention --json"]
>   Diff["Diff by stable id"]
>   Appeared["attention_item.appeared (new id)"]
>   Changed["attention_item.changed (content changed)"]
>   Resolved["attention_item.resolved (id now absent)"]
>   Unchanged["unchanged id: emit nothing"]
>
>   Poll --> Diff
>   Prior --> Diff
>   Diff --> Appeared
>   Diff --> Changed
>   Diff --> Resolved
>   Diff --> Unchanged
> ```
>
> ```gherkin
> Feature: needs-attention snapshot diffed at ingest
>   As a console maintainer
>   I want the stateless product needs-attention snapshot turned into durable events at ingest
>   So that the event-sourced console can project appeared, changed, and resolved attention items without the source keeping history
>
> Scenario: The needs-attention adapter diffs a point-in-time snapshot into keyed events
>   Given the needs-attention adapter has a prior ingested snapshot of the product needs-attention surface
>   And the surface is stateless and point-in-time with no transition history
>   When the adapter polls the surface and diffs the new snapshot against the prior one by stable id
>   Then it emits an attention_item.appeared event for each id not present before
>   And an attention_item.changed event for each present id whose composed content changed
>   And an attention_item.resolved event for each previously-present id now absent
>   And emits nothing for an unchanged id
>   And every emitted event is keyed by the item's stable id
> ```
> ```

#### tests/heading-coverage.json

`[CO-EDIT]` Per `spec.md` §"Self-application", the new `## ` H2 scenario is
registered in the link map as a FOURTH entry (after Scenarios 6, 7, and 11;
mirroring the Scenario 11 TODO precedent), so scenario -> test stays in
lockstep. At revise time this file is included in `resulting_files[]`,
appending:

> ```json
>   {
>     "scenario": "Scenario 12 -- needs-attention snapshot diffed at ingest into attention_item events",
>     "scenario_file": "scenarios.md",
>     "test": "TODO",
>     "reason": "Test tier: a top-of-pyramid acceptance/integration test under crates/console-cli/tests/ (the Scenario 7 precedent). Not yet built because the needs-attention snapshot-source port, the diff-at-ingest adapter, and the attention_item.appeared/.changed/.resolved event variants are unbuilt; the test lands with the CN1 implementation slice (spec_commitment_hint cn1-needs-attention-snapshot-port-diff-events). TODO rides through this repo's behavioral-coverage gate: console-spec-check counts any non-empty test string as satisfying scenario->test, and the LIVESPEC_BEHAVIOR_SCENARIO_LINK lever defaults to warn (non-blocking). Replace TODO with the real test path when the slice lands.",
>     "clauses": []
>   }
> ```

### Heading-coverage co-edit analysis

This proposal ADDS one `## ` H2 heading (`scenarios.md` Scenario 12) and removes
or renames none. Per the co-edit discipline, `tests/heading-coverage.json` gains
exactly one entry (above); no existing entry (Scenarios 6, 7, 11) is touched.

### Deliberately-retained non-change -- Scenario 5

Scenario 5's `Given a selected needs-attention item is derived from a blocked
needs-human work-item lane` is intentionally NOT reworded. It remains accurate
after CN1: a blocked-needs-human work-item is one of the human-valve primitives
the product `needs-attention` snapshot composes, so the item's underlying signal
IS a blocked-needs-human lane -- it now merely arrives THROUGH the composed
snapshot (the new Scenario 12) rather than being re-derived by the console. SP2
deliberately retained this phrasing at v015; this proposal keeps that decision.

### Spec-to-impl commitment

The `spec_commitments.impl_followups` front-matter declares the single CN1
impl-side work-item this proposal owes (id_hint
`cn1-needs-attention-snapshot-port-diff-events`), formalizing the deferred
downstream slice SP2 recorded in prose ("Recorded, NOT filed here"). The
work-item is filed in the console's beads store carrying that
`spec_commitment_hint`.
