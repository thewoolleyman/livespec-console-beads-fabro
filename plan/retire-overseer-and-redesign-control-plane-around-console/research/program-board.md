# Program board — the tracks this plan references, and the advice it gives them

This plan owns console work only. Every other track — orchestrator, overseer,
fabro fork, homelab — is REFERENCED here by tenant / slug / ledger id and
ADVISED; it is never tracked, dispatched, or executed from this thread
(maintainer ruling 2026-09-01, point 1 of the resume brief: keep the big
picture readable or drown in context). Consequences:

- **No status column.** Status lives in the ledgers and is read fresh from one
  cached `bd list --status all --json -n 0` per session (homelab's 013 rule:
  status in a file is a shadow ledger). This file changes when the STRUCTURE
  changes — a track is added, closed, or re-homed, or a ruling lands.
- **Children of this epic are console-owned only.** Cross-tenant references use
  the D6 form `plan_ref: <tenant>/<slug>`; the two orchestrator epics filed on
  2026-09-01 carry it in their metadata as the first dogfood of that rule.
- **Anchor:** `associated_work_item_id` → `livespec-console-beads-fabro-pzbdbo`
  (epic, `metadata.plan_slug` = this directory's name), created 2026-09-01.

## Rulings log

### 2026-09-01 — resume brief and the four-question batch

The maintainer's brief on resume: (1) this plan keeps the big picture and only
refers to / advises other tracks; (2) the critical path is (a) fix the
overseer-era problems that remain after the overseer is retired — core
livespec/factory stability — (b) land the orchestrator's new API surfaces and
primitives with the console on hold until they exist, and (c) concurrently,
deep-sweep every existing orchestrator issue for obsolete / consolidate /
update-to-direction. The four rulings that followed:

1. **Anchor now.** The console epic is created (`-pzbdbo`) and the anchor file
   updated from `unassigned` in the same PR as this board.
2. **Fabro-fork survivors live in the orchestrator tenant**, as one epic:
   `livespec-orchestrator-beads-fabro/fabro-fork-control-plane-gaps`
   (`bd-ib-bb41`, six children). Reason: the orchestrator owns the pinned fork
   and the consume path; adopters re-pin.
3. **Console hold = features held, two prerequisites allowed, AND finish
   `optimize-console-builds`** so current and future console work is reliable
   and fast. That track needed a work item to feed the factory for its
   telemetry legs; two small isolated items were reparented under its epic
   (`-1d5f`, `-6zoq`) with the instruction to dispatch, measure, close.
4. **The orchestrator sweep runs in a fresh orchestrator session anchored on a
   full work item:** `livespec-orchestrator-beads-fabro/orchestrator-backlog-sweep-for-console-control-plane`
   (`bd-ib-j81s`, ready). Its verdict summary comes back as ONE comment on
   `-pzbdbo`; this plan records nothing more of it.

## Critical path (as ruled)

**(a) Survivors — hit any dispatch path, regardless of approach.** Named in
the 2026-08-31 brainstorm (turn 1, item 3: "what survives, because it hits any
dispatch path including the console's") and re-verified against the ledgers
on 2026-09-01:

| Survivor | Where | Advice |
|---|---|---|
| RGR ritual under pre-commit gates exceeds the 1800 s implement turn | `runaway-process-containment`, `bd-ib-wcuauj` (five investigation children; live session) | Only the H2 leg (`.2`, what saturates the sandbox during the checkpoint) is on the path; the other three incidents are containment hygiene. |
| `unknown-run` needs-human ref collision; sandbox PID 1 reaps nothing; `AgentAcpTimedOut` reports `stdout: ""`; no per-tool ACP events; unconditional `--allow-empty` checkpoints; `request_permission` → interview questions | `fabro-fork-control-plane-gaps`, `bd-ib-bb41.1`–`.6` (filed 2026-09-01; the first two were tracked nowhere before) | Fork-side fixes, re-pinned through the bundled workflow; the console re-pins (its fork-drift class is already measured — PR #901). `.6` is phase 4 of the charter; `.1`–`.3` are the ones a console-driven loop hits first. |
| Green dispatch report hides a FAILED acceptance verdict | `silent-failure-surfaces`, `bd-ib-cewr.2` (ready) | Hits the console's dispatch-outcome surface directly; keep, and let the sweep confirm. |
| "ACP zero-output hang" | `bd-ib-b5dg` | Label falsified by live measurement 2026-08-31; close or re-cut to `bb41.3`. |
| `--invoker` consumption gap | journal side landed as v073 `bd-ib-vwwlwp` (closed); the CALLER gap remains | Under the redesign the caller that matters is the console → a console phase-1 child of this epic, not an overseer fix. |
| v092 typed integration contract | `bd-ib-vblnq2` — closed; consumed by console PR #901 | Off the list. |
| Empty merged diff graded as delivered | `bd-ib-xmom` — closed | Off the list; its fabro half is `bb41.5`. |

**(b) Orchestrator primitives — console on hold.** Order by what unblocks
what; none of these has a ledger item yet except b4:

- **b1 — D6 contract**: `plan_slug` required and unique per tenant,
  `associated_work_item_id`, the doctor rules, typed `next_action`; one
  orchestrator propose-change plus the one-shot migration. Cheapest item on
  the path; it is the sorting rule for (c) and what this plan's anchor
  dogfoods.
- **b2 — `context` loader + `discuss-work-item`**: replaces the plan operation
  this session runs on; it is what keeps this thread small.
- **b3 — fabro interview questions in `needs-attention` + answer route**: the
  picker-kill; the first thing the console renders.
- **b4 — workflow variants** (`pluggable-factory-workflow-configs`,
  `bd-ib-yqpdrt`): revise / gap-capture with interview consent, then panel /
  review. Its only open child is a version-pin task — it needs a re-scope and
  fresh children before it can be "central".
- **b5** — valve policy on attention items; `accounts` (caam generalized,
  event-driven off rate-limit signals); re-dispatch on `transient_infra`;
  starvation → dispatch cadence (rule prose in overseer `7ranbh`, closed —
  re-home).

**Console hold, exactly:** no new console feature tracks until b1–b3 land.
Allowed: the console spec propose-change (drop the overseer-orthogonality
clause at `SPECIFICATION/spec.md:49-52`; adopt the v093 `needs_human` valve
per the charter's §11 — the existing item `-h7jp`, "render the needs-human
gate as ledger valves", is that consume leg); the `--invoker` pass-through;
and finishing `optimize-console-builds`.

**(c) Sweep — concurrent with (b).** Anchored on `bd-ib-j81s`; sorting rule
keep / re-scope-to-workflow-variant / superseded-by-transport / consolidate /
close; the D6 draft is the tiebreaker; overseer excluded (D5 freezes it
whole).

## Referenced tracks

| Track | Tenant / id | Role here | Advice |
|---|---|---|---|
| `optimize-console-builds` | console `-gqmtwa` | finish (ruling 3) | Dispatch `-1d5f` (Red→Green) and `-6zoq` (SuiteGreen) via `drive impl:<id>`; `-2er6nc` collects the `build.env=factory` spans; `-fhdzka` records AFTER vs BEFORE; close with the numbers. |
| `test-adequacy-gates` / nightly soak | console `-4jb3kl`, `-topr34` | parked | Human-gated on the CI secret (`topr34.1`); leave. |
| needs-human as ledger valves | console `-h7jp` | prerequisite | The §11 consume leg; belongs with the console propose-change. |
| `runaway-process-containment` | orchestrator `bd-ib-wcuauj` | survivor (a) | H2 leg first. |
| `fabro-fork-control-plane-gaps` | orchestrator `bd-ib-bb41` | survivor (a) | See table above; the sweep must not re-file these. |
| `silent-failure-surfaces` | orchestrator `bd-ib-cewr` | survivor (a) | `.2` first. |
| `acp-implement-zero-output-hang` | orchestrator `bd-ib-b5dg` | falsified | Close or re-cut. |
| `pluggable-factory-workflow-configs` | orchestrator `bd-ib-yqpdrt` | b4, central under D3 | Re-scope; fresh children per variant. |
| `acceptance-evidence-admissibility` | orchestrator `bd-ib-vq6z` | keep/defer call | Five backlog children, unstarted; the sweep decides. |
| `orchestrator-backlog-sweep-for-console-control-plane` | orchestrator `bd-ib-j81s` | (c) | Verdict summary → one comment on `-pzbdbo`. |
| overseer freeze | overseer tenant, one scope event (pending) | D5 | Precondition nearly met: `m7qrgp` closed, `7ranbh` closed, `nbzgrk` blocked with no children, `zidpiu` has `.5` (do not start) and `.6`. Capabilities transfer by name: caam → `accounts` (b5); panel → workflow variant (b4); starvation / `transient_infra` rules → dispatcher (b5). |
| `steady-state-loop-hardening` | homelab `hl-eufbpx` | exit gate (D8) | Stays blocked with "homelab moves one real fleet item ready → done under the console-driven loop" as its far gate. The phase-0 finding note must be written by a fresh hand — the seat is marked do-not-restart. |
| fabro upstream | — | constraint | Stable-frozen since v0.254.0; fork-only. |

## Phase-0 actions still open (none is a child of this epic)

- Console propose-change: orthogonality clause + `needs_human` valve (this
  plan files it — console-owned; the one console child admitted before b1).
- Overseer freeze scope event (overseer tenant).
- Homelab finding note (fresh session).
- Idle peer sessions on the host (~40 idle 1–12 days): operator action; a
  budget hazard and picker-stall reservoir until the transport is gone.
