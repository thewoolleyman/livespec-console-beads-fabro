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
