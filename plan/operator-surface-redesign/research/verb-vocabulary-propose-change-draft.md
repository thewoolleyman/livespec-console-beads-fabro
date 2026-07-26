# Per-state valid-verb vocabulary — orchestrator-side propose-change DRAFT

Drafted 2026-07-26. **Status: ready-to-file body, NOT filed.** The
vocabulary is owned by `livespec-orchestrator-beads-fabro` (console
`SPECIFICATION/contracts.md`, per-item verb-suppression clause: "owned by
`livespec-orchestrator-beads-fabro` and not yet consumed here"), so this
files there via `/livespec:propose-change` — a cross-repo write that is
the maintainer's (or a directed session's) move, not this thread's.
Every rule below is a maintainer decision from the Stage-1 brainstorm
(2026-07-21..26), recorded with verification in
`plan/console-happy-path-mvp/research/verb-vocabulary-brainstorm.md`.
The four open `-l4p3ce` transport questions do NOT block this PC — they
are console/driver-side presentation.

## Proposed normative content

### The per-lane operator verb sets (impl-side lanes)

| Lane | Valid operator verbs |
|---|---|
| `backlog` | **groom** (every backlog item, uniformly); move→ready (admission); move→blocked; set-admission; set-acceptance; merge-on-review-cap; review-fix-cap; acceptance-rework-cap |
| `pending-approval` | **approve** (the single door toward `ready`); reject (rework \| regroom); set-admission; move→backlog (withdraw); move→blocked (park); set-acceptance; review caps per the window rule |
| `ready` | move→backlog (withdraw); move→blocked (park); **driver-dispatch** (factory-unsafe items only); set-acceptance; acceptance-rework-cap; merge-on-review-cap; review-fix-cap |
| `active` | observe only — no operator verbs beyond set-acceptance / acceptance-rework-cap per the window rule |
| `acceptance` | **accept** (the single door into `done`); reject (rework \| regroom); move→backlog (de-scope); move→blocked (park) |
| `blocked` | move→ready (unblock); move→backlog (an item needing decomposition routes here first — groom is `backlog`-only) |
| `done` | nothing |

### The door rules (each transition has exactly one journaled owner)

- `ready` is entered by **approve** (from `pending-approval`, journaled
  `human-valve-approve`) or an operator **move** from
  `backlog`/`blocked`. The `s`-move from `pending-approval` to `ready`
  is REMOVED — an unjournaled duplicate of the valve.
- `active` is entered ONLY by a journaled dispatch: **factory dispatch**
  (Dispatcher drain / `drive impl:<id>`, fabro run ref) or
  **driver-dispatch** (below). Bare operator moves into `active` are
  removed from every lane.
- `done` is entered ONLY by **accept**. The `s`-move
  `acceptance → done` is REMOVED (it currently exists in code —
  console `status_move_targets`, `lib.rs:483-489` — contradicting the
  walkthrough's ship-guard prose, which this rule makes true).
- `pending-approval` is never a move target (existing rule, unchanged);
  it is entered only by intake DoR routing.
- reject (rework \| regroom) is valid at the two human valves only —
  `pending-approval` and `acceptance`. Mid-flight abort of an `active`
  run is PARKED (needs run-cancellation semantics; redesign-thread
  backlog).

### The dial window rule

A policy dial is valid only while the decision it governs is still
ahead: set-admission through `pending-approval`;
merge-on-review-cap and review-fix-cap through `ready` (both are
snapshotted into the run at dispatch — `_dispatcher_loop.py:125-128` —
so a dial change on an `active` item never reaches the in-flight run);
set-acceptance and acceptance-rework-cap through `active`; nothing on
`done`.

### New drive surface: `driver-dispatch:<id>`

Valid on `ready` items whose `factory_safety` is non-null — exactly the
set the dispatch-admission host-only refusal already refuses to sandbox
(`_dispatcher_admission.py:82-86`; its refusal text already says
"Host-route it to a host sub-agent instead"). Journals actor + a
driver-session reference and moves `ready → active`; the driver session
parks its result at `acceptance`, where the normal accept valve
applies. No dispatcher/driver race by construction. Groom needs no
door: a groomed item stays `backlog` through the drafting conversation
(`regroom.py`: groom targets are backlog-only; the groom exit is
close-regroomed-out into replacement slices).

## Grounding for the reviewer

`status_move_targets` (console `lib.rs:477-493`) is the one state-aware
verb today and the generalization model; every narrowing above was
verified against master 2026-07-25..26 (see the brainstorm record for
per-point cites). Console-side changes (hint suppression, move-table
narrowing, groom/driver-dispatch presentation, keybindings) follow as
console proposals AFTER this ratifies — they are explicitly out of this
PC's scope.
