# Decision log — work-item-lifecycle-redesign

Resolved E-decisions, in walk order (E-1 → E-4). Each entry **supersedes** the
corresponding recommendation in [e-decomposition.md](e-decomposition.md). All
decisions honor [the locked core contract](locked-core-contract.md) and [the
boundary](boundary.md); design/planning only — no Rust changes are made by
recording a decision here.

Legend: **Forced** = dictated by the locked core contract (not ours to
re-decide). **Chosen** = a console-local call we made (recommend + proceed).
**Impl-detail** = deferred to the implementation phase, not a blocking design
decision.

---

## E-1 — work-item source & ingestion model — RESOLVED 2026-06-29

**Decision:** Replace the single work-item source with the orchestrator's
`list-work-items --json`, parsed as a real JSON array; consume the emitted
`lane`/`lane_reason` (+ new fields) directly; rename the `Beads*` cluster to
backend-neutral vocabulary; delete the lane re-derivation. Ingestion emits
**one observed event per item** (not one list-snapshot per poll).

### What is Forced (by the locked core contract)

1. **Source switch (Forced — core decisions 40 + 16).** Drop the direct
   `bd ready --json` reach-around (`console-cli/src/lib.rs:359`). The console's
   ONLY external work-item interface is the **orchestrator CLI**'s
   `list-work-items --json`, which returns **ALL lanes** (not just ready) — the
   per-lane board (E-2) and the lane-derived attention rule (E-3) both require
   the full set. `bd`/Dolt/"beads" knowledge is removed from the console
   (zero-Beads-knowledge constraint).
2. **Real JSON-array parse (Forced).** Replace `parse_beads_observation`'s
   substring hack (`first_json_string` grabbing the *first* id+status,
   `source_adapters.rs:1139`) with a proper deserialization (serde_json) over
   the full array of work-item objects.
3. **Consume `lane`/`lane_reason`; NEVER re-derive (Forced — the drift hazard
   the redesign retires).** Delete the 3-way
   `match status_text → BeadsWorkItemStatus::{Ready,Closed,NeedsRegroom}`
   (`source_adapters.rs:1143`). Store the emitted `lane` (one of the 7) and
   `lane_reason` (`dependency`/`needs-human`/`infra-external`/null) verbatim.
4. **Carry the new fields (Forced).** The observation widens from
   `{repo, work_item_id, status, source_version}` to also carry the fields
   downstream needs: `lane`, `lane_reason`, the 7-state `status`, `rank`,
   `admission_policy`, `acceptance_policy`, stored `blocked_reason`,
   `assignee`. These auto-emit from `list-work-items --json` via `asdict`.

### What is Chosen (console-local)

5. **Rename the `Beads*` cluster → backend-neutral work-item vocabulary
   (Chosen).** The type names themselves are Beads references and violate the
   zero-Beads-knowledge constraint. Concretely (final names settled at impl
   time): `BeadsWorkItemSnapshot` → `WorkItemObservation`;
   `parse_beads_observation` → `parse_work_item_observations`;
   `BeadsWorkItemStatus` → **deleted** (subsumed by consuming `lane`). No
   identifier in the console retains "beads"/"bd"/"dolt".
6. **Event granularity: ONE observed event per item (Chosen — per the
   go-ahead's pre-authorized recommendation).** Rationale:
   - Mirrors the per-item `lane`/`lane_reason` emission and the one-row-per-
     item ledger read.
   - Finer rebuild determinism (E-4): each item's observation replays
     independently into its projection.
   - Less churn: a single item's lane change appends one event, not a whole
     re-emitted list-snapshot.
   - Continuity: today's model is already per-item
     (`BeadsWorkItemSnapshot`); this **widens** it rather than swapping in a
     list-snapshot model.
   - Recommended emission discipline: poll reads the full list, the adapter
     emits an event per item **on change** (diff vs last-seen) to avoid
     redundant events.

### Considered & rejected

- **One list-snapshot event per poll** — rejected. It couples all items into a
  single coarse event, makes per-item rebuild a diffing exercise, and re-emits
  unchanged items. Its one genuine advantage (capturing item *absence* in a
  single event) is not needed here — see the absence note below.

### Impl-details flagged (not blocking)

- **Item absence / deletion.** Because `list-work-items --json` emits ALL
  lanes including `done`, a closed item **moves to the `done` lane** rather
  than vanishing — so per-item events cover lifecycle transitions without a
  tombstone mechanism. Genuine hard-deletion (rare) needs an absence/tombstone
  rule; deferred to impl.
- **Orchestrator CLI entrypoint + credentials.** The console shells the
  orchestrator's `list-work-items --json` as an **opaque** work-item provider;
  it stays Beads-ignorant. Credential/env-wrapper injection
  (`BEADS_DOLT_PASSWORD`) and harness reach are the orchestrator's concern,
  reached **transitively** (no direct console→driver dependency). The concrete
  binary/command path is an impl detail.
- **Recon line numbers** (`console-recon.md`) are "as of the brief" and must be
  re-verified when E-1 is implemented.

### Downstream impact

- E-2 consumes the per-item `lane` to render the 7 lanes.
- E-3 keys the attention rule off `(lane, lane_reason, admission_policy,
  acceptance_policy)` from the same observation.
- E-4 asserts that replaying these per-item observation events rebuilds the
  projections identically.

---

## E-2 — lane/view rendering — RESOLVED 2026-06-29 (maintainer decision)

**Decision:** **(C) Hybrid.** A **lane-overview home** showing all 7 lanes with
per-lane counts + the top few `rank`-ordered items each, with **drill-in to a
full-width per-lane item list**. **Attention is a DERIVED LENS** over the lanes
— not a standalone view, not an 8th lane.

### Decided (maintainer)

- **Presentation:** hybrid overview + per-lane drill-in (option C). Chosen over
  a 7-column board (too cramped for a terminal — ~11–17 cols/lane) and over a
  pure tab-per-lane (which loses the at-a-glance cross-lane overview).
- **Attention = derived lens** over the same lane data: the subset of items
  whose state/lane demands a human (the E-3 rule). No stored attention list.

### Consequential nav cleanup (entailed by the decision)

- The pseudo-lane tabs **`Ready` / `Factory` / `Manual` / `Done` collapse into
  the 7 real lanes** (ad-hoc groupings the lane model subsumes).
- **`Spec` / `Events` / `Repos` stay** as orthogonal non-lane views.
- **`Attention` becomes the lens** (a filter over the lanes), replacing the old
  standalone Attention tab as the sole item-listing view.

### Forced vs chosen

- **Forced:** the 7 lanes themselves and consuming the emitted `lane` (E-1).
- **Chosen (maintainer):** the hybrid presentation + Attention-as-lens.

### Impl-details flagged

- Per-lane lists are ordered by the fractional **`rank`**; the overview shows
  counts + top-N per lane (N is an impl/UX tuning detail).
- The **`blocked`** lane renders its `lane_reason`
  (`dependency`/`needs-human`/`infra-external`) as a sub-label; the derived
  `blocked:dependency` overlay (stored `ready` + open dependency) renders in
  `blocked` and auto-clears when the blocker closes.

---

## E-3 — attention inbox redefinition + snooze/ack deletion — RESOLVED 2026-06-29

**Decision:** Redefine the attention inbox as a **pure derivation of work-item
lifecycle state** (lane + policy) and **delete the snooze/ack plumbing** across
all 5 layers. The 3 old event-type triggers are retired — two are subsumed by
the lane derivation and one relocates to the spec-side view.

### Forced (by the locked core contract — decision 16)

1. **Inbox = pure state/lane derivation.** Rewrite `requires_attention()`
   (`console-application/src/lib.rs:1244`) from the 3 event-type triggers to a
   pure function of the ingested work-item observation's
   `(lane, lane_reason, admission_policy, acceptance_policy)`. An item needs a
   human **iff** its lifecycle state requires one:
   - `pending-approval` under **manual admission** → human must admit;
   - `acceptance` under an **ai-then-human** acceptance policy → human must
     confirm/reject the shipped artifact;
   - `blocked` with `lane_reason == needs-human` → human must unblock.
   Items in other lanes, or with auto policies, are not in the lens. No
   commands-table / dismissal-state consultation (the old inbox already never
   consulted the commands table — recon finding 4).
2. **Snooze/ack deleted (all 5 layers).** Remove:
   - `CommandType::{AttentionAcknowledgeRequested, AttentionSnoozeRequested}`
     (`console-domain/src/lib.rs:204`);
   - `OperatorAction::{Acknowledge, Snooze}`
     (`console-application/src/lib.rs:106`);
   - the snooze/ack action-menu entries (`:1280`);
   - snooze/ack handling in `attention_command` (`:1052`);
   - the snooze/ack TUI affordances (`console-tui/src/lib.rs:448,903`).
   Because the inbox is a pure derivation, this is plain plumbing deletion, not
   a projection-filter unwind (there is no filter to unwind).
3. **"Not now" = `defer` (a ledger state) or re-rank (a ledger field).** The
   console's only "not now" is a command to the **orchestrator** to defer or
   re-rank — never a console-local dismissal. (Wiring that defer/re-rank command
   UI is downstream impl work; the forced E-3 change is the deletion + the
   derivation.)

### Accounting for the 3 retired triggers

- **`DispatcherNeedsRegroomObserved`** → **subsumed by the lane derivation**: a
  needs-regroom item surfaces in whatever lane `lane_of` assigns it; if that
  lane demands a human it appears in the lens.
- **`FabroHumanGateObserved`** → **subsumed by the lane derivation**: a fabro
  run blocked on a human gate manifests as the work-item needing a human, which
  `lane_of` reflects as `blocked:needs-human` (or `acceptance`). The console
  reads that lane from `list-work-items` rather than a fabro-specific event.
  *(Assumption to verify at impl: the orchestrator/ledger reflects a fabro human
  gate in the work-item's lane. If it does not, this trigger's replacement needs
  revisiting — surface it then.)*
- **`LivespecReviseRequired`** → **relocated to the spec-side `Spec` view**: it
  is a spec-side signal (`livespec next` → revise/critique), not a work-item
  lifecycle state, so it leaves the work-item attention inbox and is surfaced in
  the `Spec` view (per recon finding 2: `livespec next --json` is spec-side
  enrichment, not a work-item source).

### Chosen (console-local)

- **The attention lens is purely work-item-lifecycle-derived.** Non-work-item
  human signals are surfaced in their own contexts (fabro execution; the `Spec`
  view), keeping the lens a clean pure derivation of the state machine exactly
  as the contract states. *(Alternative the maintainer may prefer: a unified
  "everything needing a human" aggregate lens over both work-item and fabro/spec
  signals. Recorded as a possible follow-on; not built in E-3.)*

### Downstream impact

- E-4's rebuild test asserts the attention lens is reproducible purely by
  replaying the work-item observation events (no attention/dismissal state
  persisted) — reinforcing zero-primary-state.

---

## E-4 — rebuild-from-ledger / zero-primary-state conformance — RESOLVED 2026-06-29 (maintainer ratified)

**Decision:** Add a net-new conformance test asserting **rebuild determinism**
and **no primary work-item lifecycle state** — both scoped to the work-item
projections (lanes + the Attention lens) and **EXCLUDING** the operator
`commands` table. **Drop** the dead `projections` table. **Accept**
`commands.status` as console-local operator-command state via a documented
carve-out (do **not** event-source it). All three recommendations ratified.

### What the test asserts (ratified)

1. **Rebuild determinism.** Snapshot the work-item projections → **wipe** the
   console store → **re-backfill from the ledger** by replaying the per-item
   observation events (E-1's `list-work-items --json` ingestion) → recompute the
   projections → assert they are **identical** to the snapshot. Projections
   (lanes + Attention lens) are a pure function of the ledger.
2. **Structural no-primary-lifecycle-state.** Assert the console store persists
   **no authoritative work-item lifecycle state** — no primary lane/status/
   attention column or table; the only persisted work-item data is the
   observation cache (a cache of the ledger, not primary).
3. **Scope.** Both assertions cover **work-item projections only (lanes +
   Attention lens)** and **EXCLUDE** the console-local operator `commands` table
   (residue B).

### The two residues (ratified)

- **Residue A — dead `projections` table:** **DROP it.** Declared but never
  read/written outside a table-exists test; keeping it muddies the structural
  assertion.
- **Residue B — `commands.status` (in-place mutation, `:568`):** **accept as
  console-local operator-command state** with a **documented carve-out**, and
  **exclude it from the rebuild assertion**. It is outbound-action bookkeeping
  (did the operator's command to the orchestrator apply?) — not a work-item
  lifecycle state and not derivable from the ledger. **Do NOT event-source it.**
  The "zero primary **lifecycle** state" invariant is specifically about
  work-item lifecycle (lanes/attention); the carve-out keeps it precise.

### Impl-details flagged

- Test home is net-new (today only `list_console_events_rebuilds_domain_events`
  at `console-eventstore/src/lib.rs:748` exists, row→domain only). The
  conformance test builds work-item projections from a known ledger fixture,
  snapshots, wipes, re-ingests from the same fixture, rebuilds, asserts equal;
  plus a structural assertion (no primary work-item-lifecycle column; `commands`
  is the documented exception). The structural assertion may be expressible via
  `console-arch-check` / `console-spec-check` or as a store-schema test —
  settled at impl.

### Downstream impact

- Capstone: locks in E-1 (per-item observation events as the rebuild input),
  E-2 (lanes + Attention lens as the projections under test), and E-3
  (Attention lens has no persisted state). **The E walk is complete**; the epic
  is ready to groom into dispatchable slices (maintainer-owned).

---

# Implementation rollout (L1a released — orchestrator v0.3.0)

The design above is locked. Implementation proceeds slice by slice under the
autonomous-rollout authorization (orchestrator **v0.3.0** ships the flat
`lane`/`lane_reason` emission from `list-work-items` — the artifact the console
consumes). Each slice lands via worktree → PR → rebase-merge; the repo enforces
**100% line coverage**.

## E-1 — work-item source & ingestion — IMPLEMENTED 2026-06-29

Realizes the E-1 decision in Rust. Landed:

- **Source switch:** the single work-item source now shells the orchestrator's
  `list-work-items --json` (backend-neutral; `SourceAdapterKind::Orchestrator`,
  adapter id `orchestrator`), replacing the direct `bd ready --json`
  reach-around. The console holds **zero** Beads/Dolt knowledge — a workspace
  grep census proves the only remaining `beads` token is the product/tenant
  name `livespec-console-beads-fabro`.
- **Real JSON-array parse:** `parse_orchestrator_observation` uses `serde_json`
  to deserialize the full array; the `first_json_string` substring hack and the
  3-way `match status_text` re-derivation are gone.
- **Consume the emitted lane:** new `Lane` (7 variants) and `LaneReason` (3
  variants) enums (`serde` kebab-case `Deserialize` + `label()`) are
  deserialized **directly** from the emitted `lane`/`lane_reason` — the console
  never re-derives a lane.
- **Backend-neutral model:** `BeadsWorkItemStatus` deleted; `BeadsWorkItemSnapshot`
  → `WorkItemSnapshot` carrying `lane`/`lane_reason`; `EventType` variant →
  `WorkItemSnapshotObserved` (wire name `work_item.snapshot_observed`); event-id
  prefixes `orchestrator:`. The stored-event cache rename needs no migration
  (E-4: the events table is a rebuildable cache of the ledger).
- **One observed event per item** (the per-item granularity decision).

Verification: full `just check` green (incl. `check-deps` on the new `serde`/
`serde_json` deps, `check-arch`, and `check-coverage` at 100% lines). Tests
cover the array parse + lane/lane_reason consumption, the empty/malformed/
invalid-item error paths, and all `Lane`/`LaneReason` variants.

Deferred to later slices (per the decision-log): carrying `rank`/`status`/
`admission_policy`/`acceptance_policy`/`assignee` into the observation (added
when E-2/E-3 consume them); the concrete orchestrator-CLI entrypoint +
credential threading remains an impl-detail (the console shells
`list-work-items` as an opaque, credential-ambient provider).

## E-2 — hybrid lane TUI view — IMPLEMENTED & MERGED (both slices, 2026-06-29)

E-2 landed in two slices, both merged. **Slice 1 (the data spine)** is PR #62
(master `e7898aa`); **slice 2 (the TUI lane sub-view)** is PR #64 (master
`a696125`).

### E-2a — lane-board data spine — IMPLEMENTED & MERGED (PR #62)

The pure projection the hybrid view will render, with no TUI wiring yet:

- **`rank` + `status` carried** on `WorkItemSnapshot` and the orchestrator
  `WorkItemRecord`. `rank` is the orchestrator's lexicographic fractional key
  (`key_between() -> str`); a missing `rank` defaults to the bottom sentinel
  `"~"`, a missing `status` to `""`. Both join `lane`/`lane_reason` in the
  observation identity hash, so a re-rank or status transition appends a fresh
  observation the board picks up.
- **Payload persistence + reload.** Each snapshot observation persists its
  `payload_json` (lane, lane_reason, rank, status, source_version); the store
  re-attaches it on load via the new `ConsoleEvent::payload_json`, so the
  reduction can read the snapshot an observation captured. Snapshot
  serialization is built as a `serde_json::Value` (infallible `Display`)
  mirroring the typed read shape — no unreachable failure arm, keeping the
  100%-line gate honest.
- **`project_lane_board`** (console-application): a pure in-memory reduction —
  **no persisted projection table** (zero-primary-state, E-4). Latest
  observation per work-item wins; every item lands in its emitted `lane`; the 7
  lanes render in canonical order, each ordered by `(rank, id)`; non-snapshot /
  unparseable payloads are skipped.

Verification: full `just check` green (fmt, clippy `-D warnings`, tests, **100%
line coverage**, `cargo deny` + `machete`, arch-check, behavior-coverage,
baseline, doctor-static); all 10 CI checks green; rebase-merged.

### E-2b — hybrid lane TUI sub-view — IMPLEMENTED & MERGED (PR #64, master `a696125`)

Consumes `project_lane_board` through a reshaped TUI navigation:

- **`TuiView` reshaped** to `{Attention, Spec, Lanes, Events, Repos}`; the ad-hoc
  `Ready/Factory/Manual/Done` pseudo-lane tabs are collapsed into the single
  `Lanes` view (the lane model subsumes them). `Spec/Events/Repos` stay as
  orthogonal non-lane views; `Attention` stays the default (its
  rewrite-as-pure-lens is E-3).
- **`LaneFocus { Overview, Lane }`** drives the hybrid sub-view: a lane-overview
  home (all 7 lanes, counts + a preview of each lane's top rank-ordered items,
  selected lane highlighted) with drill-in to a single lane's full rank-ordered
  list. Arrows move the selected lane in the overview; `Enter` drills in; `Esc`
  returns (closing an open overlay first). Key routing is view/focus-aware, so
  the keymap consumes the screen model.
- **State/model**: `TuiInteractionState` carries `lane_focus` +
  `selected_lane_index` (set via single-field `with_*` helpers so the reducer
  reads one change per arm); `TuiScreenModel` carries the projected `lane_board`,
  `lane_focus`, and the clamped overview selection. View-summary rows are dropped
  for `Attention`/`Lanes`, which render their own projections.
- **Spec**: the console-local `SPECIFICATION/contracts.md` TUI-nav section is
  updated to the reshaped view set + the Lanes sub-view (lane vocabulary
  consumed from core's emitted `lane`/`lane_reason`, never re-derived). The
  direct spec edit was healed by doctor-static's auto-backfill as history `v010`
  (committed alongside).

Verification: full `just check` green (fmt, clippy `-D warnings`, tests, **100%
line coverage**, `cargo deny` + `machete`, arch-check, behavior-coverage,
baseline, doctor-static); all 10 CI checks green; rebase-merged.

## E-3..E-4 — pending implementation

E-3 (attention-as-derivation + snooze/ack deletion) then E-4
(rebuild-from-ledger conformance test).

---

# Side-task — L2 tenant migration (9-tenant lockstep) — DONE 2026-06-29

Separate from the E-2 code rollout: this repo's own beads tenant was
**pre-migration** and needed the fleet's L2 lockstep backfill onto the
work-item-state-machine schema. Migrated via the orchestrator's own v0.3.0
primitives (per `livespec-orchestrator-beads-fabro`
`plan/work-item-state-machine/l2-tenant-migration.md`), under the family env
wrapper:

1. **Registered the 5 custom lifecycle statuses** (`store.register_custom_statuses`
   → `bd config set status.custom
   "backlog,pending-approval,ready:active,active:wip,acceptance:wip"`; idempotent).
2. **Backfilled `rank`** on all 12 live (non-`done`) heads via the
   `rebalance_ranks.legacy_seed` primitive, in `(priority, created_at, id)`
   order → `a0…aB`, written in place through `store.update_work_item_rank`
   (metadata-only; statuses/labels/edges untouched; every head was rank-less, so
   nothing was clobbered). Legacy `open` status VALUES were deliberately NOT
   reclassified (per the runbook's scope boundary).

Verification: the S6 `work_item_state_invariants` doctor check exits **0**
against the live tenant (no rank WARNINGS, no `active⟹assignee` /
`blocked⟹blocked_reason` ERRORS). Formalized as the closed work-item
`livespec-console-beads-fabro-vxq`.
