> # ✅ CLOSED — epic complete (exit gate met 2026-07-01)
>
> This planning thread is **CLOSED**; the work-item-lifecycle epic's **exit
> gate is MET** and this snapshot is archived (`plan/archive/`). Everything
> below the banner is preserved verbatim as the **historical record** — it is
> no longer the live next-action.
>
> **What closed the epic:**
> - The console **E-walk is COMPLETE** — E-3a / E-3b / E-4 all merged
>   (factory-implemented).
> - The maintainer's **session-9 hold condition** ("factory self-publishes
>   with zero native-auth bridge, fully clean-green dispatch") was
>   **satisfied** by the clean-green proof dispatch: item `zgd` → **PR #74**,
>   authored **and** merged by `app/livespec-pr-bot` via the factory's own
>   **GitHub App installation token** (**zero native-auth bridge**), post-merge
>   janitor **GREEN**, `zgd` → **done**, master CI **green**.
> - The anchor epic **`livespec-35s3zo`** + all per-repo **L2 migration
>   epics** + this console epic **`livespec-console-beads-fabro-vqh36l`** are
>   now **CLOSED**.
> - The local **overseer skill is KEPT and UPDATED** — deletion deferred to
>   the future console operator-cockpit milestone (**decision 47**); **NOT
>   deleted**.
>
> The token-gate / standing-config follow-ons noted below were non-blocking
> for the E-walk and did not gate this close.

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

- **E-3a (ingestion: admission/acceptance policies) — IMPLEMENTED & MERGED**
  (PR #67). Fabro run `01KWBYVJ4NNSACS4MT183VEATH`.
- **E-3b (attention inbox as pure lane derivation + snooze/ack deletion) —
  IMPLEMENTED & MERGED** (PR #69, + v011 spec backfill). Fabro run
  `01KWC5E015XM3DAPE1VDCQG8TR`.
- **E-4 (rebuild-from-ledger / zero-primary-state conformance test; drop dead
  `projections` table) — IMPLEMENTED & MERGED** (PR #70). Fabro run
  `01KWCBR7CTJ9R59S891AHAB2RH`.

**Next action: the E-walk (E-1 … E-4) is COMPLETE — all slices implemented &
merged.** The redesign epic `livespec-console-beads-fabro-vqh36l` is ready to
**close/groom** (its E-1..E-4 scope is delivered). What remains are non-blocking
follow-ons, none of which gate the E-walk:

1. **Token gate (orchestrator tenant work-items).** The factory's in-sandbox
   `gh pr create` is blocked — the projected `LIVESPEC_FAMILY_GITHUB_TOKEN` lacks
   `Pull requests: write` on this repo, so E-3a/E-3b/E-4 were published via the
   **native-auth bridge** (a human's `gh` auth). Resolve via `bd-ib-p2e`
   (stopgap: grant the PAT PR-write on all targeted family repos) and/or
   `bd-ib-gsl` (durable: GitHub App installation token + parameterize the
   entrypoint token source for adopters).
2. **Standing-config cache refresh.** The console's normal `orchestrate run` uses
   the enabled-plugin **cache** (still pre-`LIVESPEC_CORE_PLUGIN_ROOT`-fix). For
   it to carry the v0.3.2 fix, the console scope needs `claude plugin update` →
   v0.3.2 + a session restart (maintainer / console-session step). The drive used
   the **source** dispatcher, which already carries the fix.

**Reusable per-slice factory recipe (worked out 2026-06-30; for future drives),
under `with-livespec-env.sh` from the console repo root:** (1) `bd update <id>
--status ready`; (2) `bd update <id> --add-label admission:auto` (the valve
admits ONLY `admission_policy == "auto"`); (3) `export
PATH="$HOME/.fabro/bin:$PATH"` + `export GH_TOKEN="$LIVESPEC_FAMILY_GITHUB_TOKEN"`
+ `python3 <orchestrator-source>/.claude-plugin/scripts/bin/orchestrate.py run
--repo <console> --action impl:<id> --json` (prereqs: `fabro` 0.254.0 +
`fabro server start` on `127.0.0.1:32276`); (4) impl+janitor run token-free —
publish the pushed `feat/<id>` branch via native `gh` auth → PR → CI →
rebase-merge → close `<id>` `done`/`resolution:completed`. If a run fails before
pushing, recover its implement diff via `fabro dump <run-id> -o <dir>` →
`stages/002-implement@1/diff.patch`. A SPECIFICATION edit needs a `docs(spec):
backfill vNNN` commit (doctor-static self-heal) in the same PR.

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
