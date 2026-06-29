# E-1..E-4 — the console design decomposition

Item "E" in the core thread, decomposed into four decisions. Each is
resolved **one at a time, leading with a recommendation**. These
recommendations are starting positions for the resolution conversation, not
final calls — and nothing here is resolved yet (the thread is paused awaiting
core go-ahead).

Every decision is constrained by [the locked core
contract](locked-core-contract.md) and the [boundary](boundary.md): the
console **consumes** the emitted `lane` / `lane_reason` and never re-derives a
lane.

## E-1 — work-item source & ingestion model

**What:** switch the one work-item source from `bd ready --json` →
the orchestrator's **`list-work-items --json`** (the full list of ALL lanes,
not just ready); parse it as a **real JSON array** → observed events carrying
the emitted `lane` / `lane_reason` + the new fields; **rename the whole
`Beads*` cluster** to backend-neutral work-item vocabulary; **delete** the
3-way `match status_text` re-derivation.

**Open sub-choice (flagged by core) — event granularity:**
- **One observed event per item** *(recommended)* — widens today's per-item
  snapshot model; finer rebuild; less churn; aligns with one-row-per-item
  ledger reads.
- One list-snapshot event per poll — coarser; couples all items into a single
  event; harder to diff incrementally.

**Why it is first:** every downstream decision consumes the data model and
vocabulary E-1 establishes.

## E-2 — lane/view rendering

**What:** how the 7 lanes render in the ratatui TUI, and how the Attention
view relates to the lanes.

**Shape options:**
- A **7-lane board** (columns/panes per lane).
- **Tab-per-lane** with real per-item lists (evolves the existing `TuiView`
  tabs from count-summaries to real lists).
- A **hybrid** (a board plus a focused per-lane list).

**Recommendation (starting position):** lead with the hybrid — a board for
at-a-glance lane balance plus a drill-in per-lane list — but treat this as
genuinely open; it depends on E-1's ingested model and on operator
ergonomics. Attention is **derived** (see E-3), so it is a lens over the same
lane data, not a separate stored list.

## E-3 — attention inbox redefinition + snooze/ack deletion

**What:** redefine the inbox from the 3 event-type triggers
(`FabroHumanGateObserved | LivespecReviseRequired |
DispatcherNeedsRegroomObserved`) to a **state/lane-derived** rule — which
states demand a human (e.g. `pending-approval` under manual admission;
`acceptance` under `ai-then-human`; `blocked:needs-human`) — and **delete**
the snooze/ack plumbing across the 5 layers (per
[recon finding 4](console-recon.md)).

**Recommendation (starting position):** define `requires_attention` purely as
a function of `(lane, lane_reason, admission_policy, acceptance_policy)` so
the inbox is a pure derivation with zero console-local dismissal state;
remove `Acknowledge`/`Snooze` commands, actions, menus, and TUI affordances
wholesale. "Not now" becomes `defer`/re-rank in the ledger (commanded through
the orchestrator), never a console dismissal.

## E-4 — zero-primary-state / rebuild-from-ledger conformance test

**What:** a net-new conformance test asserting: wipe the store, re-backfill
from the ledger, projections are **identical** (rebuild determinism); plus a
**structural** assertion that **no work-item lifecycle state is persisted as
primary**.

**The two residues to decide (per [recon finding 5](console-recon.md)):**
- The **dead `projections` table** — drop it.
- **`commands.status` mutated in place** (`:568`) — either make it derived
  (event-sourced) or explicitly accept it as **non-lifecycle operator-command
  state** that the structural assertion exempts.

**Recommendation (starting position):** drop the dead `projections` table;
classify `commands.status` as operator-command state (not work-item lifecycle
state) and exempt it from the structural assertion, documenting the carve-out
so the "zero primary lifecycle state" claim stays precise. Resolve E-4 last,
but hold its invariants in view while deciding E-1 and E-3 — they constrain
the ingestion and attention models.

## Walk order

**E-1 → E-2 → E-3 → E-4.** E-1 is foundational; E-2 and E-3 consume its
model; E-4 is the capstone conformance test whose invariants constrain the
earlier decisions throughout.
