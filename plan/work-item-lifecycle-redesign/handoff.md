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

**AUTONOMOUS IMPLEMENTATION ROLLOUT (design locked; L1a = orchestrator
v0.3.0 released).** The E walk design (E-1..E-4) is complete in
[research/decision-log.md](research/decision-log.md); implementation now
proceeds slice by slice via worktree → PR → rebase-merge. Rollout status is in
the decision-log's "Implementation rollout" section.

- **E-1 (work-item source & ingestion) — IMPLEMENTED & MERGED.** The console
  consumes the orchestrator's `list-work-items --json` flat `lane`/`lane_reason`
  emission; the `bd ready` re-derivation and the entire `Beads*` cluster are
  retired (backend-neutral `Orchestrator`/`WorkItemSnapshot`/`Lane`/`LaneReason`
  vocabulary; one observed event per item).
- **E-2a (lane-board data spine) — IMPLEMENTED & MERGED** (PR #62, master
  `e7898aa`). `rank`+`status` carried on `WorkItemSnapshot`; snapshot
  `payload_json` persisted and re-attached on load via
  `ConsoleEvent::payload_json`; `project_lane_board` reduces
  `WorkItemSnapshotObserved` events into the 7 lanes (latest-per-item wins,
  ordered by `(rank, id)`) — a pure derivation, **no projection table**.
- **E-2b (hybrid lane TUI sub-view) — IMPLEMENTED & MERGED** (PR #64, master
  `a696125`). `TuiView` reshaped to `{Attention, Spec, Lanes, Events, Repos}`
  (the `Ready/Factory/Manual/Done` tabs collapsed into the single `Lanes` view);
  `LaneFocus {Overview, Lane}` drives a hybrid overview-home + per-lane drill-in
  over `project_lane_board`; key routing is view/focus-aware (`Enter` drills in /
  `Esc` returns); `SPECIFICATION/contracts.md` TUI-nav section updated (healed by
  doctor-static auto-backfill as history `v010`).

**Next action: implement E-3 — attention inbox as a pure derivation + snooze/ack
deletion.** Per [research/decision-log.md](research/decision-log.md) §E-3:
rewrite `requires_attention()` from the 3 event-type triggers
(`FabroHumanGateObserved | LivespecReviseRequired |
DispatcherNeedsRegroomObserved`) to a pure function of the work-item
observation's `(lane, lane_reason, admission_policy, acceptance_policy)` — an
item needs a human iff its lifecycle state requires one (`pending-approval`
under manual admission; `acceptance` under `ai-then-human`; `blocked` with
`lane_reason == needs-human`). Delete the snooze/ack plumbing across all 5
layers (`CommandType::{AttentionAcknowledgeRequested,AttentionSnoozeRequested}`,
`OperatorAction::{Acknowledge,Snooze}`, the action-menu entries,
`attention_command` handling, the TUI affordances). Relocate
`LivespecReviseRequired` to the `Spec` view; account for the other two retired
triggers via the lane derivation (verify the assumption that the ledger reflects
a fabro human gate in the work-item's lane — surface if it does not). "Not now"
becomes a `defer`/re-rank command to the orchestrator, never a console-local
dismissal. NOTE: E-2b carries `admission_policy`/`acceptance_policy` only as far
as E-2 needed; E-3 must thread them into the observation if not already present.
Then E-4 (rebuild-from-ledger conformance test).

Discipline: worktree → PR → rebase-merge; `mise exec -- git`; never
`--no-verify`; halt+report on hook failure; the repo enforces **100% line
coverage** (`just check-coverage`) — cover every new line/branch. A direct
`SPECIFICATION/*` edit triggers doctor-static's self-healing history backfill
(a new `history/vNNN/`) — commit that backfill in the same PR to heal the track.

**Side-task done (separate from E-2 code):** this repo's beads tenant L2
lockstep migration (register 5 custom statuses + `rank` backfill `a0…aB` on the
12 live heads via the orchestrator `legacy_seed` primitive) is APPLIED and
verified (S6 doctor exits 0); formalized as closed work-item
`livespec-console-beads-fabro-vxq`. See the decision-log's L2 side-task section.
