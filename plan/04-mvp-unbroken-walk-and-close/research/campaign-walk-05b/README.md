# Campaign lifecycle #5 (replacement) — `iofvz2` — IN FLIGHT AT SESSION END

**This walk is INCOMPLETE.** It was dispatched at 06:11:38Z and the session wound
down at ~06:14Z with the run still executing. It has NOT reached the acceptance
valve and has NOT been accepted, so **it does not count yet**. Campaign is
**4 of 6** until a successor takes the human accept.

The replacement for the parked `erb2ud` walk (`../campaign-walk-05-erb2ud/`),
run after the fleet-wide E2BIG blocker cleared with orchestrator v0.62.7.

## State at wind-down — what a successor inherits

    work-item     livespec-console-beads-fabro-iofvz2
    dispatch-id   120f5f62a7104333a0254d160060a4c0
    armed         ai-then-human, command ..._iofvz2_ai-then-human_99 completed
                  06:10:57.053679528Z, with NOTHING in flight
    dispatched    06:11:38Z from Factory > Dispatch ready work (the DRAIN)
    loop-pick     06:11:45Z picked ["livespec-console-beads-fabro-iofvz2"]
                  -- exactly the armed item; arm-first held again
    sizing-warn   description is 1758 chars (> 1500)
    last journal  dispatch-id 06:11:49Z, then silence (NORMAL -- the sandbox emits
                  nothing until it returns; the walk #3 reference run was quiet
                  for ~26 min at this stage)

The TUI was left RUNNING on tmux session `walk5b` under `TMUX_TMPDIR=/tmp/w4`,
built from master `593e428`. Verify it is alive before relying on it; relaunching
with `just tui` is cheap if not.

## What remains, in order

1. Wait for the acceptance park. Watch
   `tmux_tui_e2e`-free journal lines for `iofvz2` in
   `tmp/fabro-dispatch-journal.jsonl`, or the console header.
2. When it parks: drill the acceptance lane, capture leg A, verify the target in
   the detail pane immediately before committing (`-x6lj`), take the human accept
   from the menu, capture leg B and the close.
3. Finish this README with the accept legs and the outcome, then land it.

## Two failure modes to distinguish if it does NOT park

- **Dies at `fabro-run` with `exec /bin/bash: argument list too long`** — that is
  the fleet-wide E2BIG regression the orchestrator fixed in v0.62.7, and its
  recurrence here would be a NEW finding, not a repeat, because no dispatch had
  yet been re-run from THIS tenant on the fixed build. This walk is that
  confirming test.
- **Dies at the pre-push hook with `is missing a scripts/bin directory`** — that
  is the sandbox provisioning fault this plan measured on the `erb2ud` run
  (`../campaign-walk-05-erb2ud/`, finding 2). v0.62.7 addresses E2BIG, NOT this,
  so a recurrence is a REPEAT of plan 04's own finding and belongs on that
  report rather than as a new one.

## What this walk already proved: `x6lj` caught in the act

`09-x6lj-resort-in-walk.md` is the substantive finding and it is complete and
self-contained regardless of how the lifecycle ends. Three different work-items
occupied one cursor position inside ninety seconds with no arrow key pressed,
and had `Dispatch selected item` been wired and used, this walk would have
dispatched a DIFFERENT PLAN'S work-item with no modal and no read-back. First
in-walk capture of the mechanism rather than a reconstruction.

## File index

| file | what it captures |
| --- | --- |
| `00-timestamp.txt` | walk start, UTC |
| `01-ranking-consult.json` | the one named non-menu step; `iofvz2` top of five |
| `02-resting.txt` | resting frame at 160 columns |
| `03-ready-drilled.txt` | ready lane; the top pick renders SEVENTH with `rank ~` |
| `04-target-selected.txt` | target confirmed BY ID before acting |
| `05-legA-ready-lane.txt` | leg A — verbs the ready lane offers |
| `06-legB-arm.txt` | leg B — `Target: …iofvz2` / `ai-then-human` read back |
| `07-post-arm.txt` | frame after the arm committed |
| `08-dispatch-timestamp.txt` / `08-dispatch-committed.txt` | dispatch committed; `drain in flight` |
| `09-x6lj-resort-in-walk.txt` / `09-x6lj-resort-in-walk.md` | the in-walk re-sort, capture + write-up |
| `10-journal-loop-pick.txt` | `loop-pick` took exactly the armed item |
