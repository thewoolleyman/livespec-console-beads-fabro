# Decision log — work-item-lifecycle-redesign

Resolved E-decisions, newest-relevant first. Each entry **supersedes** the
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
