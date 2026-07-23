# Real-stack walk findings — Stage-0 admission session (2026-07-21)

First session driving the REAL console (`just tui`, live tenant) through the
happy path's admission leg. Ledger outcomes, then the defects the walk
surfaced. Live status is read from `list-work-items` / `next`, never from
here.

## What the walk achieved

- `-276inb` admitted **through the TUI approve valve** (`p` → Enter):
  command `completed`, lane `pending-approval → ready`. The valve works.
- `-sreeqc` admitted via the orchestrator drive surface directly
  (`drive.py --action approve:…`) after its TUI path wedged — see defect 2.
- `-qwjfsw` routed `backlog → ready` through the TUI `s` move valve
  (drilled backlog lane → `s` → `ready` → Enter). Worked first try.
- `-7rcps4` was already `done` before this session; nothing to admit.
- Palette drain (`:` → `drain`) issued; the factory takes it from `ready`.

## Blocking precondition discovered: the autonomous-mode levers

The approve valve refused with `invalid-source-state`:
"approve requires an effective-manual pending-approval item."
`.livespec.jsonc` still carried the maintainer-directed 2026-07-20
autonomous-mode levers (`auto_approve_ready: true`,
`acceptance_mode: "ai-only"`), which make every item's effective admission
policy `auto` — the human valve is definitionally invalid under them.

Reverted 2026-07-21 (auto_approve_ready `false` via the TUI Settings row;
acceptance_mode `ai-then-human` by hand per the file's own edit-by-hand
warning, bd-ib-lmi5). Rationale: autonomous mode is retired for good, and
the happy-path mission admits at the approve valve and ships at the accept
valve. The `.livespec.jsonc` comment block now records the reversal.

Note for the walkthrough doc (Stage 3): the walk has a PRECONDITION the doc
does not state — the approve/accept valves require effective-manual
admission/acceptance policy. Worth a line in
`docs/lifecycle-walkthrough.md`'s "Before you start".

## Defect 1 — valve failure is silent everywhere the operator looks

When the approve failed, the TUI gave NO feedback: the modal closed, the
row stayed `Pending approval`, the header stayed unchanged. The console
store recorded `work_item.action.failed` with EMPTY `error_json` and empty
event metadata — `DispatcherOrchestratorActionPort::run_action` collapses
the child's stderr/stdout into a boolean
(`crates/console-application/src/lib.rs:2040-2048`), so the drive CLI's
actual diagnostic ("approve requires an effective-manual pending-approval
item", `domain_error: invalid-source-state`) is discarded. Diagnosis
required re-running the exact drive.py invocation by hand. An operator at
the keyboard has no path to that.

## Defect 2 — a failed valve command is permanently unretryable

The console `commands` table keys approve/accept on
`<item-id>:work_item.approve_requested` — no attempt discriminator. After
the failed attempt, every later `p` → Enter on `-sreeqc` was silently
swallowed by idempotency: no new command row, no new event triple, no
error. Contrast `move`, whose command ids DO carry an attempt suffix
(`cmd_work_item_move_requested_…_ready_13`) and which retried fine. Once an
approve fails once, the TUI can never admit that item; the only route is
the drive CLI outside the console. Happy-path blocking.

Also observed while diagnosing: every command/event this session carries
the SAME `requested_at`/`occurred_at` (the session-start timestamp) —
`current_requested_at()` is captured once at TUI startup. Made "did the
retry even run" undecidable from timestamps; worth folding into whichever
of the two bugs gets fixed first.

## Environment hazards worth knowing

- A leftover `serve` instance from the retired `console-autonomous-mode`
  tmux session (unattached, 22h old) was still polling. With `-ipwtll`
  open (every client executes queued commands), two live clients risk
  double-execution; killed the stale instance before driving anything.
  Single-operator MVP assumes ONE live client — check `ps` for stray
  `serve` processes before a walk.
- The Attention list reorders under live refresh (new items appeared
  mid-session: `-ogpok4`, a second "ranked ready item" row). Selection
  index persists across focus changes while rows shift beneath it — verify
  the Detail pane id before EVERY valve press. (This is `-sreeqc`'s
  no-title bug biting the operator in real time; its fix directly
  de-risks the walk.)
