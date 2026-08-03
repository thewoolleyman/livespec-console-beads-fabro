# Stage-1 verb-vocabulary brainstorm — decisions and verification

The `plan/operator-surface-redesign/` entry gate is maintainer brainstorm
participation. This note is the durable record of that brainstorm's
decided points — taken 2026-07-21 through 2026-07-25, maintainer
answering (directly, then relayed via the supervisor session) — so the
decisions survive the conversation that produced them. The output ROUTES
as an orchestrator-side propose-change first: the per-state valid-verb
vocabulary is owned by `livespec-orchestrator-beads-fabro`
(console `SPECIFICATION/contracts.md`, per-item verb-suppression clause)
and — verified 2026-07-25 against that repo's SPECIFICATION — has not
been authored there yet. The console side is presentation, consumed after
ratification. Authored in `plan/operator-surface-redesign/` custody;
recorded here because this thread drove the brainstorm.

## Baseline

Draft "table B" (adopted 2026-07-21 as the working baseline): per-lane
offered/suppressed verb sets for the happy-path lanes only, generalizing
from the one state-aware verb that exists (`s` /
`status_move_targets(lane)`, `console-application/src/lib.rs:477-493`).
Everything beyond the happy-path-minimal subset stays in the redesign
thread's own backlog.

## Decided points

1. **Groom exposure — uniform on all `backlog` items, `backlog` only.**
   The ratified seven-state lifecycle has NO regroom label or status
   (orchestrator `regroom.py`: decomposition targets are ordinary
   `backlog` items; `require_backlog_target` hard-refuses other lanes).
   Draft B's "groom on blocked-when-needs-regroom" was WRONG and is
   dropped — a rejected-as-regroom item routes `move→backlog` first. No
   groom-worthiness filter: every backlog item is a legal target; the
   operator judges.

2. **Single admission door.** `approve` is the only exit from
   `pending-approval` toward `ready` (it journals `human-valve-approve`;
   the `s` move `pending-approval→ready` performed the same transition
   unjournaled). `s` from `pending-approval` keeps only `backlog`
   (withdraw) and `blocked` (park). The `s` `→active` target drops too
   (point 3).

3. **Two journaled doors into `active`; bare moves suppressed
   everywhere.** `active` means "being worked, with a reference to what
   is working it". Doors: **factory dispatch** (Dispatcher drain /
   `drive impl:<id>`, fabro run ref) and **driver dispatch** — the
   `-l4p3ce` handoff verb firing on a `ready` item journals a
   driver-dispatch with actor + driver-session reference; the driver
   session parks its result at `acceptance`, where the same accept valve
   applies. Maintainer chose to DESIGN THE DRIVER DOOR NOW (not reserve
   it). Open sub-decision: whether it fires on any `ready` item or only
   factory-unsafe ones. Bare `s` `move→active` is suppressed from every
   lane. Note groom needs no door: a groomed item stays `backlog` through
   the drafting conversation.

4. **Reject at the valves only.** `reject` (rework/regroom) is valid at
   `pending-approval` and `acceptance` only. Active-abort — which needs
   run-cancellation semantics to be honest (today's fire-anywhere `r` on
   an `active` item moves the lane while the run keeps going) — is PARKED
   to the redesign thread. Scoping note per the cross-repo constraint:
   this (like every point here) is vocabulary pending orchestrator-side
   ratification; the console meanwhile must not present `r` outside the
   two valve lanes.

5. **Dials get the upstream-window rule.** A policy dial is valid only
   while the decision it governs is still ahead: `m` set-admission on
   `backlog`/`pending-approval`; `g` merge-on-review-cap and `f`
   review-fix-cap through `ready`; `n` set-acceptance and `k`
   acceptance-rework-cap through `active`; none on `done`.
   **Verified 2026-07-25 against the dispatcher**: the supervisor's
   caveat that g/f might need to extend "through active" is WRONG — the
   dispatcher loop snapshots both review caps into the run's parameters
   at dispatch time (`_dispatcher_loop.py:125-128`); a dial change on an
   `active` item never reaches the in-flight run. A rework bounce
   re-reads them at the next dispatch, from a dial-valid lane.

6. **Acceptance-lane `s` targets — `backlog` + `blocked` only** (decided
   2026-07-25, relayed via supervisor). Code offered
   `acceptance → backlog/ready/active/blocked/done` (`lib.rs:483-489`),
   contradicting the walkthrough's ship-guard prose. `done` drops (accept
   is the only ship door), `active` drops (dispatch-only), and `ready`
   drops too — reject(rework) is the only journaled re-queue. `s` from
   `acceptance` means deliberate de-scope (`backlog`) or park
   (`blocked`). The walkthrough's ship-guard prose becomes true rather
   than aspirational; `docs/detailed-usage.md`'s move table changes with
   the impl.

7. **Driver-door firing scope — factory-unsafe items only** (decided
   2026-07-25, relayed via supervisor; predicate verified at source
   2026-07-25). The driver door fires on exactly the items the
   Dispatcher's **host-only refusal at dispatch admission** already
   refuses to sandbox: `_dispatcher_admission.py:82-86` runs every
   selection candidate through `host_only_refusal`;
   `_dispatcher_host_only.py` defines `is_host_only_item` as
   `factory_safety is not None`, and its refusal text says "Host-route it
   to a host sub-agent instead; the item remains open for that route" —
   the orchestrator already anticipates this exact route. No
   dispatcher/driver race by construction. (Naming correction: this gate
   is the dispatch-admission host-only refusal, NOT the "janitor gate" —
   the janitor is the post-merge check family.) Any-ready firing needs a
   claim mechanism and defers to the redesign thread.

## `-ectqye` routing (decided 2026-07-25: reconcile with `-k0w` first)

The maintainer rejected both amendment cuts as filed and directed
reconciliation with `livespec-console-beads-fabro-k0w` (filed 2026-07-20,
a day before `-ectqye`, P2 `backlog`, from the factory-drain path) before
any split. Verified 2026-07-25: `-k0w` indeed covers both halves — no
operator surface for failed commands AND `error_json` empty at the store
— plus silent SUCCESS, and carries the code analysis (`footer_hint` is a
`const fn` with no message slot; surfacing outcomes is a multi-layer
feature). The honest seam between the two records is **store-side vs
UI-side, not drain-path vs valve-path** — both paths share the same two
broken layers, so a drain/valve seam would duplicate the shared
mechanism across both items. Custody proposal handed to the supervisor;
nothing filed until it returns.

## Also verified during the brainstorm

- The five dispatch-leg P1s (`-6ma`, `-8i9`, `-m36`, `-htp`, `-9ts`) and
  the 2026-07-21 strand are recorded in the handoff's § "Status
  composition" snapshot — they bound what the walk can currently prove,
  not what the vocabulary should say.
- `-ipwtll` (single-consumer command spine) closed 2026-07-23; the
  single-operator MVP still assumes one live console client.
