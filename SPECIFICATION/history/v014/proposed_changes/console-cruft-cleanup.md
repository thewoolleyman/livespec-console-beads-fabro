---
topic: console-cruft-cleanup
author: claude-fable-5
created_at: 2026-07-04T05:41:09Z
---

Disposition note: accepting all four proposals disposes as one per-file
decision (proposal topic `console-cruft-cleanup`); selective disposition
uses the revise modify disposition (or a pre-split of this file), which
is how the "independently ratifiable" property of these proposals is
exercised.

## Proposal: Retire the Beads adapter — the orchestrator CLI is the console's only work-item source

### Target specification files

- SPECIFICATION/spec.md
- SPECIFICATION/contracts.md
- SPECIFICATION/scenarios.md
- SPECIFICATION/non-functional-requirements.md

### Summary

Replace the spec'd Beads adapter (raw `bd` reads emitting snapshot/ready/closed/needs-regroom/manual-routing events) with a Work-items adapter over the orchestrator CLI (`list-work-items --json`), consuming the orchestrator-computed `lane`/`lane_reason` verbatim, and ratify the product-wide zero-Beads-knowledge invariant. This aligns the adapter contract TO the already-ratified Lanes-view rule (contracts.md §"TUI Contract": the console "consumes that lane assignment and never re-derives it") and to the shipped Rust code, which is already clean.

### Motivation

Plain-English semantics: the console holds ZERO Beads knowledge; its only work-item interface is the orchestrator CLI, whose `list-work-items --json` emits every item with its computed `lane`/`lane_reason` in one batch read; the console consumes that lane and never re-derives it. Rationale: a raw-`bd` read path would force the console to recompute lanes from Beads-native status text — a stored/re-derived copy that can silently disagree with the single authority (`lane_of`), recreating the shadow-state failure the lifecycle design killed. Design record: repo `thewoolleyman/livespec`, `plan/archive/work-item-state-machine/research/03-decision-log.md` decisions 15 (`lane_of` is one pure function, emitted to the console — consume-don't-recompute) and 16 (console: zero Beads knowledge, hard negative constraint; only interface is the orchestrator CLI), and `02-design.md` §7 (the contract boundary). The console spec was authored BEFORE that design session (2026-06-27); the v0xx lifecycle retrofit fixed the Lanes view and the Rust ingestion path but left the adapter contract carrying the retired read path — here the ratified spec lags the shipped code, and any future implementer following the spec would faithfully re-introduce the reach-around.

### Proposed Changes

All quoted current text is verbatim from the live v013 files.

**contracts.md §"Initial Adapters"** — replace the bullet:

> - **Beads adapter** -- reads Beads work-item state through the `bd` CLI and
>   emits snapshot/ready/closed/needs-regroom/manual-routing events.

with:

> - **Work-items adapter** -- reads work-item state through the orchestrator
>   CLI (`list-work-items --json`, one batch read carrying every item with its
>   orchestrator-computed `lane` / `lane_reason`) and emits work-item snapshot
>   events that carry the emitted lane assignment verbatim. The console holds
>   zero Beads knowledge: this adapter MUST NOT invoke `bd` or parse
>   Beads-native records, and MUST NOT re-derive a lane from `status` or any
>   other field -- lane re-derivation is the shadow-state failure the
>   lifecycle design killed (design record: repo `thewoolleyman/livespec`,
>   `plan/archive/work-item-state-machine/research/03-decision-log.md`,
>   decisions 15/16).

**contracts.md** source-contract mermaid (same section) — rename the nodes: `Beads["bd list / show / ready"]` becomes `WorkItems["orchestrator list-work-items --json"]`; `BA["Beads adapter"]` becomes `WIA["Work-items adapter"]`; `WorkEvents["beads.* events"]` becomes `WorkEvents["work_item.* events"]`; the edge `Beads --> BA --> WorkEvents` becomes `WorkItems --> WIA --> WorkEvents`.

**contracts.md**, the paragraph after the adapter list — extend:

> Adapters MUST call existing stable CLIs/APIs through ports. UI code MUST NOT
> call Fabro, Beads, LiveSpec, Dispatcher, or GitHub directly.

with one added sentence: "Work-item state enters the console ONLY through the orchestrator-CLI port: no console code -- adapter, application, or UI -- invokes `bd` or reads the Beads tenant directly."

**spec.md §"Purpose"** — in "It consumes source facts from LiveSpec, Beads, Dispatcher, Fabro, GitHub, and local repository state", replace "Beads" with "the orchestrator's work-items surface".

**spec.md §"Architecture"** first mermaid — `BD["Beads tenant via bd"]` becomes `WI["Orchestrator work-items CLI\n(list-work-items --json)"]`; `BDA["Beads adapter"]` becomes `WIA["Work-items adapter"]`; the edge `BD --> BDA --> Log` becomes `WI --> WIA --> Log`.

**spec.md §"Architecture"** hexagonal mermaid — `BeadsAdapter["Beads adapter"]` becomes `WorkItemsAdapter["Work-items adapter"]`; the edge `BeadsAdapter --> Ports` becomes `WorkItemsAdapter --> Ports`.

**spec.md §"Terminology"**, the **Adapter** entry — "observes a system such as Fabro, Beads, LiveSpec, Dispatcher, or GitHub" becomes "observes a system such as Fabro, the orchestrator's work-items surface, LiveSpec, Dispatcher, or GitHub".

**scenarios.md Scenario 1** gherkin — "So that I do not have to poll LiveSpec, Beads, Dispatcher, Fabro, and GitHub separately" becomes "So that I do not have to poll LiveSpec, the orchestrator's work-items surface, Dispatcher, Fabro, and GitHub separately".

**scenarios.md Scenario 2** gherkin — "So that ready Beads work can enter Dispatcher/Fabro without manual command assembly" becomes "So that ready work-items can enter Dispatcher/Fabro without manual command assembly".

**scenarios.md Scenario 4** (H2 title unchanged — no heading-coverage impact) — mermaid `Source["Beads current state"]` becomes `Source["Work-items current state (orchestrator CLI)"]`; gherkin scenario line "Scenario: Beads current-state snapshot lacks transition history" becomes "Scenario: Work-items current-state snapshot lacks transition history"; "Given the Beads adapter can observe current work-item state" becomes "Given the Work-items adapter can observe current work-item state through the orchestrator CLI".

**non-functional-requirements.md §"Domain-Driven Design"** — after the "UI crates MUST talk only to projections and command APIs..." bullet, add:

> - No console crate may invoke the `bd` CLI or parse Beads-native records:
>   work-item state enters only through the orchestrator-CLI port
>   (`list-work-items --json`), carrying the orchestrator-computed `lane` /
>   `lane_reason` verbatim.

**non-functional-requirements.md §"Architecture Tests"** — in "Concretely, the checks MUST enforce at least:", insert the bullet "- no crate invokes `bd` or embeds a Beads-native read path (the zero-Beads-knowledge rule), and" immediately AFTER the existing bullet "- UI does not call Beads/Fabro/LiveSpec/GitHub directly, and" and BEFORE "- product crates do not use `unwrap`/`expect` outside allowed scopes, and". The new bullet keeps its trailing ", and" (a non-final list position); the final bullet "- all use cases return typed `Result`." is unchanged.

**non-functional-requirements.md §"Contributor Scenario H"** gherkin, second scenario — after "And UI crates talk only to projections and command APIs, never directly to Beads, Fabro, LiveSpec, Dispatcher, or GitHub", add the line "And no crate invokes bd or parses Beads-native records; work-item state enters only through the orchestrator-CLI port" (co-edited per the behavior => scenario split; the H2 heading is unchanged).

Implementation impact (recorded; no work-item filed in this pass): the Rust ingestion path already complies (the only work-item source command is `list-work-items --json`; lanes consumed verbatim; zero `bd` invocations) — the shipped delta is confined to the arch-check gaining the zero-Beads rule, authorized for filing at ratification.

## Proposal: Retire needs-regroom vocabulary — a non-convergence bounce goes to backlog

### Target specification files

- SPECIFICATION/spec.md
- SPECIFICATION/scenarios.md

### Summary

Remove the retired `needs-regroom` vocabulary from the Grooming bounded context and Scenario 1: the lifecycle has no needs-regroom state or label — a Dispatcher non-convergence bounce transitions the item back to `backlog` for re-decomposition.

### Motivation

Plain-English semantics: when a dispatched item will not converge, the Dispatcher bounces it to `backlog` (the `bounce` transition, `active → backlog`) and surfaces it — escalate-don't-drop; grooming then re-decomposes it. There is NO needs-regroom state, label, or lane anywhere in the 7-state machine (`backlog · pending-approval · ready · active · acceptance · blocked · done`). Rationale: spec text that names a nonexistent state gets faithfully amplified by implementing agents into wrong event vocabularies and wrong routing decisions — exactly what happened here: the console code hard-codes a `dispatch.needs_regroom_observed` event type from this spec text (its rename is a dependency-linked work-item behind this ratification). Design record: repo `thewoolleyman/livespec`, `plan/archive/work-item-state-machine/research/03-decision-log.md` decision 32 (supersedes decision 4; the locked transition table row `bounce`: active → backlog, "non-convergence needs re-groom") and decisions 22-32 (the seven stored states); repo `thewoolleyman/livespec-orchestrator-beads-fabro`, `SPECIFICATION/contracts.md` §"Grooming and slice-size calibration" ("the lifecycle `backlog` bounce disposition (there is no separate needs-regroom state)").

### Proposed Changes

All quoted current text is verbatim from the live v013 files.

**spec.md §"Bounded Contexts"** — replace the bullet:

> - **Grooming** -- needs-regroom routing, slice proposal/approval events,
>   factory/manual/spec routing.

with:

> - **Grooming** -- backlog-bounce observation (a Dispatcher non-convergence
>   bounce lands the item back in `backlog` for re-decomposition; there is no
>   needs-regroom state or label), slice proposal/approval events,
>   factory/manual/spec routing.

**spec.md §"Bounded Contexts"** mermaid — `Grooming["Grooming\nneeds-regroom + slicing"]` becomes `Grooming["Grooming\nbacklog bounce + slicing"]`.

**scenarios.md Scenario 1** mermaid — `Dispatcher["Dispatcher needs-regroom bounce"]` becomes `Dispatcher["Dispatcher backlog bounce"]`.

**scenarios.md Scenario 1** gherkin — "And the Dispatcher adapter observes a non-converging item bounced to needs-regroom" becomes "And the Dispatcher adapter observes a non-converging item bounced to `backlog` for re-grooming".

(The `grooming.regroom_requested` command deletion is the command-surface change and lives in the "Command vocabulary" proposal beside this one; the two are independently ratifiable but share this motivation.)

Implementation impact (recorded; the slice is already filed dependency-linked behind this ratification): rename the console's dispatcher-journal vocabulary — `DispatcherJournalKind::NeedsRegroom` (label `needs-regroom`), `EventType::DispatcherNeedsRegroomObserved` (wire name `dispatch.needs_regroom_observed`, display label "Dispatcher needs-regroom") and their store/CLI/fuzz/test wiring — to backlog-bounce vocabulary. The observation log is a rebuildable cache, so the wire-name change is handled by wipe + re-backfill, never an upcaster (design record `02-design.md` §8).

## Proposal: Command vocabulary — add the human-valve and policy-edit commands; retire grooming.regroom_requested and the manual/host-only markers

### Target specification files

- SPECIFICATION/contracts.md
- SPECIFICATION/spec.md
- SPECIFICATION/scenarios.md

### Summary

Grow the initial-commands vocabulary with the five work-item lifecycle commands the Control-Plane role exists to issue — approve, accept, reject (rework|regroom), set-admission, set-acceptance — each mapping 1:1 onto the orchestrator's published `orchestrate run` action-id surface; delete `grooming.regroom_requested` (no regroom state exists to request); add the Work-item Lifecycle bounded context that owns the new vocabulary; and replace the retired `manual / host-only` markers in spec.md's Purpose with the admission-policy framing. TUI mechanics (command modals, palette, type-to-confirm) already exist in the ratified spec — this adds vocabulary and mapping only, no new UI machinery.

### Motivation

Plain-English semantics: the two human valves of the lifecycle — approve (a human authorizes a `pending-approval` item onward) and accept/reject (a human confirms or rejects a merged, live `acceptance` item) — are operator decisions, and the console is the operator cockpit; yet the spec'd command vocabulary has no way to issue them, and no way to edit an item's `admission_policy`/`acceptance_policy` (today policies are display-only everywhere in the console, and the only historical edit mechanism was a consent-bypassing raw `bd update --add-label` write). Rationale: decision 16 fixes the seam — the console commands each plane only through that plane's published surface and never writes the ledger directly; the orchestrator's published surface for exactly these acts is the `orchestrate run` action ids, and decision 17 folds `orchestrate` into the console, making this mapping the console's core operator loop. Killed alternatives stay killed: snooze/acknowledge are NOT reintroduced — "not now" remains defer/re-rank via the orchestrator. Design record: repo `thewoolleyman/livespec`, `plan/archive/work-item-state-machine/research/03-decision-log.md` decisions 16 and 17, and the two-valve model in `02-design.md` §4. Ratified action-id surface: repo `thewoolleyman/livespec-orchestrator-beads-fabro`, `SPECIFICATION/contracts.md` §"`orchestrate`" → "Human valve actions" (`approve:<id>`, `accept:<id>`, `reject:<id>:rework|regroom` — "the published surface the console invokes for the two human valves"). PENDING anchor (treat as pending, not ratified): that same repo's `SPECIFICATION/proposed_changes/approval-is-the-pending-approval-to-ready-transition.md` restores approve as the `pending-approval → ready` transition, makes the admission valve purely mechanical, and adds the `set-admission:<id>:auto|manual` / `set-acceptance:<id>:<policy>` policy-edit action ids under the no-surprise-transitions rule (a policy edit never moves an item between states). Where this proposal states approve/policy-edit semantics it cites that proposal as the pending anchor rather than asserting it as ratified; if the pending proposal is rejected upstream, the mapping paragraph's semantics sentences follow whatever replaces it.

### Proposed Changes

All quoted current text is verbatim from the live v013 files.

**contracts.md §"Command Handling"** — replace the "Initial commands:" list:

> - `factory.drain_requested`
> - `factory.dispatch_item_requested`
> - `factory.pause_requested`
> - `factory.resume_requested`
> - `spec.doctor_requested`
> - `grooming.regroom_requested`
> - `config.autonomous_mode_set`
> - `factory.autonomous_mode_enable_requested`
> - `factory.autonomous_mode_disable_requested`

with:

> - `factory.drain_requested`
> - `factory.dispatch_item_requested`
> - `factory.pause_requested`
> - `factory.resume_requested`
> - `spec.doctor_requested`
> - `work_item.approve_requested`
> - `work_item.accept_requested`
> - `work_item.reject_requested`
> - `work_item.set_admission_requested`
> - `work_item.set_acceptance_requested`
> - `config.autonomous_mode_set`
> - `factory.autonomous_mode_enable_requested`
> - `factory.autonomous_mode_disable_requested`

(`grooming.regroom_requested` is deleted: there is no needs-regroom state to request — a non-convergence bounce is Dispatcher-owned machinery landing the item in `backlog`, and the human regroom disposition on a live change is `work_item.reject_requested` with mode `regroom`. Nothing implements the deleted command — the shipped domain enum carries only `factory.drain_requested` — so no code is orphaned.)

Follow the list with this paragraph: "The five `work_item.*` commands are the Work-item Lifecycle context's vocabulary. Each maps 1:1 onto the orchestrator's published `orchestrate run` action-id surface, and the console MUST issue them ONLY through that surface — it never writes the ledger directly: `work_item.approve_requested` → `approve:<work-item-id>`; `work_item.accept_requested` → `accept:<work-item-id>`; `work_item.reject_requested` (payload `mode ∈ {rework, regroom}`) → `reject:<work-item-id>:rework|regroom`; `work_item.set_admission_requested` (payload `policy ∈ {auto, manual}`) → `set-admission:<work-item-id>:<policy>`; `work_item.set_acceptance_requested` (payload `policy ∈ {ai-only, human-only, ai-then-human}`) → `set-acceptance:<work-item-id>:<policy>`. Approve semantics and the two policy-edit action ids follow the PENDING orchestrator proposal (topic `approval-is-the-pending-approval-to-ready-transition`, repo `thewoolleyman/livespec-orchestrator-beads-fabro`): approve is the human approval act — the `pending-approval → ready` transition — and a policy edit never moves an item between states (the no-surprise-transitions rule). This mapping cites that proposal BY TOPIC as its pending anchor — no `proposed_changes/` path lands in ratified text, because that path dangles once the proposal ratifies and archives into `history/`; upon its ratification the reference becomes that repo's ratified `SPECIFICATION/contracts.md` sections (§"Work-item state semantics" and the `orchestrate` action-id surface), and updating it is part of the already-planned post-ratification alignment. The honesty rule of this section applies unchanged: a simulated or unimplemented orchestrator port MUST surface a not-observed / not_wired outcome and MUST NOT fabricate success. Snooze/acknowledge remain killed (design record decision 16): there is no local-dismiss command; \"not now\" is defer/re-rank via the orchestrator."

**spec.md §"Bounded Contexts"** — after the **Grooming** bullet, add:

> - **Work-item Lifecycle** -- the human-valve commands (approve / accept /
>   reject) and the policy-edit commands (set-admission / set-acceptance),
>   issued through the orchestrator's published `orchestrate` action surface;
>   observes the resulting lane transitions.

**spec.md §"Bounded Contexts"** mermaid — the diagram mirrors the bullet list 1:1, so the new context needs a matching node and edge. After the node line `Attention["Attention\nlane-derived inbox"]`, add the node `WorkItemLifecycle["Work-item Lifecycle\nvalves + policy edits"]`; beside the section's existing context-to-Attention edges, add the edge `WorkItemLifecycle -->|"valve + policy outcome events"| Attention`. Edge reasoning: every operator-facing context in this diagram edges INTO the Attention inbox (`Ingestion`, `Factory`, `Grooming`, `Hygiene` all do), and the valve/policy outcome events are exactly what resolve or update the Attention items the valves act on, so the new context follows the section's existing edge semantics. The label uses this diagram's existing `\n` line-break style and carries no temporal markers. (The `Grooming` node's own label is edited by the "Retire needs-regroom vocabulary" proposal beside this one; the two diagram edits touch different lines and compose in either ratification order.)

**spec.md §"Purpose"** — replace the operator-question bullet:

> - Which work is manual or host-only and must not enter Fabro?

with:

> - Which work rests at `pending-approval` awaiting my explicit approval
>   (effective `admission_policy: manual` -- the first-class field that
>   replaced the retired `host-only` / `human-gated` markers)?

**scenarios.md** — add a new H2 section at the end:

> ## Scenario 11 -- Human valve and policy-edit commands map onto the orchestrator surface
>
> Pending anchor: the approve and policy-edit scenes below follow the
> PENDING orchestrator proposal (topic
> `approval-is-the-pending-approval-to-ready-transition`, repo
> `thewoolleyman/livespec-orchestrator-beads-fabro`); if that proposal is
> rejected or ratifies in a different form, this scenario reworks
> alongside the command-mapping paragraph in `contracts.md`. Upon its
> ratification the reference becomes that repo's ratified
> `SPECIFICATION/contracts.md` §"Work-item state semantics", updated as
> part of the post-ratification alignment.
>
> ```gherkin
> Feature: Human valve and policy-edit commands
>   As a LiveSpec operator
>   I want to approve, accept, reject, and re-policy work-items from the console
>   So that the two human valves and the policy dials are one keystroke away, with the orchestrator owning the ledger write
>
> Scenario: Approve routes through the orchestrator's published action surface
>   Given a `pending-approval` work-item whose effective admission_policy is manual, shown in Attention
>   When the operator invokes Approve on it
>   Then the console persists a `work_item.approve_requested` command
>   And invokes the orchestrator's published action surface with `approve:<work-item-id>` through its port
>   And appends the outcome events from the orchestrator result
>   And observes the item's lane change on a subsequent work-items poll
>
> Scenario: Reject with mode regroom maps onto the reject action id
>   Given an `acceptance` work-item the operator judges wrongly scoped
>   When the operator invokes Reject with mode regroom
>   Then the console persists a `work_item.reject_requested` command carrying mode regroom
>   And invokes the orchestrator's published action surface with `reject:<work-item-id>:regroom`
>   And never writes the ledger directly
>
> Scenario: A policy edit never moves an item between states
>   Given a work-item whose stored admission_policy is manual
>   When the operator invokes set-admission with policy auto
>   Then the console persists a `work_item.set_admission_requested` command
>   And invokes the orchestrator's published action surface with `set-admission:<work-item-id>:auto`
>   And the item's lifecycle state is unchanged
> ```

Ratification co-edit requirement: this adds one `## ` H2 to `scenarios.md`, so ratification MUST land the matching `tests/heading-coverage.json` entry via the revise `resulting_files[]` mechanism in the same pass (`test` MAY be `"TODO"` with a non-empty `reason`); the registry currently registers only Scenario 7, so no retitle impact exists.

Implementation impact (recorded; NO work-item is filed in this pass — filing the slice is authorized at ratification, not before): the domain `CommandType` enum grows the five `work_item.*` variants and their handlers/ports; the existing epic `livespec-console-beads-fabro-pke3y3` ("Implement the 7 unimplemented initial commands") shifts scope — `grooming.regroom_requested` leaves the spec unimplemented (nothing to remove) and the five valve/policy commands join the unbuilt set.

## Proposal: Nightly-finding chores are filed top-of-rank through the capture surface, never "filed ready"

### Target specification files

- SPECIFICATION/non-functional-requirements.md

### Summary

Re-derive the nightly-finding clause of the Quality Gate (and its Contributor Scenario C restatement) against the 7-state lifecycle: a nightly fuzz/mutation finding files a chore work-item at the top of the rank order through the orchestrator's capture surface, whose intake routing and effective admission_policy govern its lifecycle state — nothing is "filed ready", and "high-priority" names a removed field.

### Motivation

Plain-English semantics: urgency is expressed by `rank` (the sole, fractional ordering key — lexicographically earliest is most urgent) and by the capture position parameter (`top`), not by a `priority` field, which the design removed; and no item can be born `ready`, because approval IS the `pending-approval → ready` transition — the universal gate every item transits (a capture whose effective `admission_policy` is `auto` is approved onward at capture/groom time; `manual` rests at `pending-approval`). Rationale: "filed ready for pickup" would have a CI implementer bypass the approval gate with a direct-to-`ready` write, and "high-priority" would have them reach for a field that no longer exists — both faithful amplifications of pre-redesign vocabulary. Design record: repo `thewoolleyman/livespec`, `plan/archive/work-item-state-machine/research/03-decision-log.md` decisions 11-13 (rank is the sole persisted order; `priority` killed; create position is a required parameter) and 26/32 (approval ≡ `ready` membership; the structural `pending-approval` gate is universal). PENDING anchor for the approve-transition phrasing: repo `thewoolleyman/livespec-orchestrator-beads-fabro`, `SPECIFICATION/proposed_changes/approval-is-the-pending-approval-to-ready-transition.md` (cited as pending, not ratified).

### Proposed Changes

All quoted current text is verbatim from the live v013 file.

**non-functional-requirements.md §"Quality Gate"**, the nightly paragraph — replace:

> A nightly finding (a new
> crash, or a new surviving mutant not on the allow-list) MUST NOT fail
> the canonical branch; it MUST instead open a **high-priority chore
> work-item, filed ready for pickup**, in the project's Beads tenant
> (`livespec-console-beads-fabro`).

with:

> A nightly finding (a new
> crash, or a new surviving mutant not on the allow-list) MUST NOT fail
> the canonical branch; it MUST instead file a chore work-item at the
> **top of the rank order** in the project's work-items ledger (the
> `livespec-console-beads-fabro` tenant), through the orchestrator's
> capture surface, so the intake Definition-of-Ready checklist and the
> item's effective `admission_policy` route its lifecycle state -- an
> item is never filed directly into `ready` (approval IS the
> `pending-approval → ready` transition; pending anchor: the orchestrator
> proposal topic `approval-is-the-pending-approval-to-ready-transition`,
> repo `thewoolleyman/livespec-orchestrator-beads-fabro` -- upon its
> ratification this reference becomes that repo's ratified
> `SPECIFICATION/contracts.md` §"Work-item state semantics", updated as
> part of the post-ratification alignment).

**non-functional-requirements.md §"Contributor Scenario C"** mermaid — `Chore["finding -> ready chore work-item; never fail master"]` becomes `Chore["finding -> top-ranked chore work-item; never fail master"]`.

**non-functional-requirements.md §"Contributor Scenario C"** gherkin, final scenario — replace the lines:

>     And a high-priority chore work-item is filed ready for pickup in the
>       livespec-console-beads-fabro Beads tenant

with:

>     And a chore work-item is filed at the top of the rank order in the
>       livespec-console-beads-fabro tenant through the orchestrator's
>       capture surface

(No H2 heading changes anywhere in this proposal, so no `tests/heading-coverage.json` co-edit is required.)
