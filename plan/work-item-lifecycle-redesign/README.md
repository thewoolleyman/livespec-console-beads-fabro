# Plan thread — work-item-lifecycle-redesign

The console's realization of livespec's **work-item lifecycle state
machine**: lane/view redesign, `list-work-items` ingestion, attention as a
pure derivation, and a rebuild-from-ledger conformance test.

This is a **stateful, re-entered planning thread** (like `groom`), not a
one-shot capture. It is **design/planning only** — no Rust changes yet.

## Status anchor (ledger)

- **Console-tenant epic:** `livespec-console-beads-fabro-vqh36l`
  (type `epic`, tenant `livespec-console-beads-fabro`). Intake DoR verdict:
  `needs-regroom` (expected — an epic is multiple "dones"; it is groomed into
  dispatchable slices only after E-1..E-4 are decided).
- **Parent fleet epic (cross-repo, prose link only):**
  `livespec-35s3zo` (livespec **core** tenant), thread
  `/data/projects/livespec/plan/work-item-state-machine/`. Not a typed
  dependency — this repo's schema exposes only a same-tenant `depends_on`
  (no cross-repo dependency kind), so the cross-tenant id lives in prose, not
  in `depends_on`. See [research/boundary.md](research/boundary.md).
- Driven by the livespec **core** design session (`overseer-design4`). When a
  genuine question or a core-contract concern arises, it is printed to the
  pane in plain prose for the core session to relay — **no AskUserQuestion
  pickers** (hard to drive over tmux).

## Why this thread exists (one line)

livespec is adopting **one** deterministic work-item lifecycle (7 stored
states + 2 human-delegable valves + a first-class fractional `rank`). Core
owns the **contract**; this thread owns the **console's consumer-side
realization** of it, captured and owned where the work lives.

## Research notes (durable capture)

1. [research/boundary.md](research/boundary.md) — the load-bearing
   core-owns-the-contract / console-owns-the-how boundary, and what must NOT
   be copied into this repo's `SPECIFICATION/`.
2. [research/locked-core-contract.md](research/locked-core-contract.md) —
   the pinned, already-locked core contract the console MUST honor (states,
   `lane_of`, the `lane`/`lane_reason` emission, the console hard
   constraints, post-merge acceptance).
3. [research/console-recon.md](research/console-recon.md) — current console
   state (crate layout + the six findings): the `bd ready --json` source and
   `Beads*` cluster, the four other sources, the absence of any lane/board
   concept, snooze/ack plumbing, near-zero-primary-state, and the missing
   rebuild conformance test.
4. [research/e-decomposition.md](research/e-decomposition.md) — the E-1..E-4
   decision decomposition with a leading recommendation for each, and the
   walk order.

## Planned E walk order

**E-1 → E-2 → E-3 → E-4** (the natural dependency order):

- **E-1 (work-item source & ingestion)** is foundational — every downstream
  view, attention rule, and conformance assertion consumes the new
  `lane`/`lane_reason` + renamed backend-neutral vocabulary E-1 introduces.
- **E-2 (lane/view rendering)** consumes E-1's ingested 7-lane model.
- **E-3 (attention redefinition + snooze/ack deletion)** consumes E-1's
  state/lane data and is informed by E-2's view model.
- **E-4 (rebuild-from-ledger / zero-primary-state conformance)** is the
  capstone test of the whole pipeline; resolved last, but its invariants
  (zero primary lifecycle state; the two residues — dead `projections`
  table, in-place `commands.status`) **constrain E-1 and E-3 throughout**,
  so they are kept in view from the start.

## Current state of the thread

**Design COMPLETE; autonomous implementation rollout UNDERWAY** (L1a =
orchestrator v0.3.0 released). All four decisions (E-1..E-4) are resolved in
[research/decision-log.md](research/decision-log.md); implementation now lands
slice by slice via worktree → PR → rebase-merge (see the decision-log's
"Implementation rollout" section). **E-1 (source & ingestion) and E-2 (hybrid
lane TUI view, both slices) are implemented & merged**; **E-3
(attention-as-derivation + snooze/ack deletion) is next**, then E-4
(rebuild-from-ledger conformance test). The [handoff](handoff.md) is the
resumable entry point.

Working model used: parts **forced or clearly implied** by the locked core
contract proceeded without a stop (E-1 — source & ingestion; E-3 forced parts —
attention as pure derivation + snooze/ack deletion); genuine design decisions
**stopped and were surfaced as plain text** for the maintainer (E-2 —
lane/view rendering; E-4 — conformance scope + the two residues).
