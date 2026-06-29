# Handoff — work-item-lifecycle-redesign

Single resumable execution-coordination point for this planning thread. A
fresh session that opens ONLY this file and follows the read-first chain can
proceed to the next action without re-deriving anything or consulting chat
history.

## Ledger status anchor (read-only)

- **Console epic:** `livespec-console-beads-fabro-vqh36l` (this tenant).
- **Parent fleet epic (cross-repo, prose link only):** `livespec-35s3zo`
  (livespec core tenant).
- Live status is **composed from the ledger** — never shadowed here. To see
  current state run, under the family env wrapper, the orchestrator
  `list-work-items` / `next` operations against this repo's tenant. This
  handoff embeds **no** `[ ]`/`[x]` work queue.

## Read-first chain (in order)

1. `plan/work-item-lifecycle-redesign/README.md` — thread overview, walk
   order, status.
2. `plan/work-item-lifecycle-redesign/research/locked-core-contract.md` —
   the fixed inputs from livespec core (states, `lane_of`, the
   `lane`/`lane_reason` emission, the console hard constraints, post-merge
   acceptance). These are **referenced, not re-decided**.
3. `plan/work-item-lifecycle-redesign/research/boundary.md` — the
   core-owns-contract / console-owns-how seam and the spec-hygiene rule.
4. `plan/work-item-lifecycle-redesign/research/console-recon.md` — current
   console state (crate layout + the 6 findings).
5. `plan/work-item-lifecycle-redesign/research/e-decomposition.md` — the
   E-1..E-4 decisions, each with a leading recommendation, and the walk
   order.

When a decision-log note exists at
`plan/work-item-lifecycle-redesign/research/decision-log.md`, read it after
the chain above — it carries the resolved E-decisions and supersedes the
recommendations in `e-decomposition.md`.

## Working model (how to proceed)

- **Proceed** (recommend + act, no stop) on parts **forced or clearly
  implied by the locked core contract**.
- **Stop and surface as PLAIN TEXT** (no AskUserQuestion pickers) only on
  **genuine design decisions** — then wait for the maintainer's answer
  relayed by the core session.
- After each E decision: record it in
  `plan/work-item-lifecycle-redesign/research/decision-log.md` and update
  this handoff's **Next action** so the thread stays resumable.
- Repo discipline: all changes via **worktree → PR** (per `AGENTS.md`);
  never `--no-verify`; on a hook failure, halt and report. Do not modify
  other repos or touch branches this thread did not create.

## Next action (exactly one path)

**E-4 — rebuild-from-ledger / zero-primary-state conformance. STOPPED,
awaiting the maintainer's answer.** E-1, E-2, and E-3 are RESOLVED and recorded
in [research/decision-log.md](research/decision-log.md). E-4 is a **genuine
design decision** — what the conformance test asserts (wipe the store +
re-backfill from the ledger → projections identical; plus a structural "no
work-item lifecycle state persisted as primary") and how to resolve the **two
residues** (drop the dead `projections` table; make `commands.status` derived
vs. accept it as non-lifecycle operator-command state) — so it was **surfaced
as plain text** for the maintainer and the thread is paused here.

When the maintainer's E-4 answer is relayed: record it in the decision-log,
then update this "Next action" — the E walk completes and the next step is
grooming the epic `livespec-console-beads-fabro-vqh36l` into dispatchable
console-local implementation slices (via the orchestrator `groom` operation).

This is design/planning only — **no Rust changes**.
