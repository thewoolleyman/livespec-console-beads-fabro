# Campaign lifecycle #5 — `iofvz2` — COMPLETE, ACCEPTED AT THE HUMAN VALVE

**Campaign count: 5 of 6.** One unbroken pass, single item, real stack, menus-only
for every action. Dispatched 06:11:38Z, accepted 06:47:49Z, ledger `closed`
06:47:52Z. The replacement for the parked `erb2ud` walk
(`../campaign-walk-05-erb2ud/`), run after the fleet-wide E2BIG blocker cleared
with orchestrator v0.62.7.

This walk was performed across TWO sessions. The first dispatched it and wound
down with the run in flight; the successor resumed from the plan-epic timeline,
re-measured the inherited state, and took the accept. Everything below the
"resume" line was captured by the successor.

## Outcome

    work-item     livespec-console-beads-fabro-iofvz2
    dispatch-id   120f5f62a7104333a0254d160060a4c0
    fabro run     01M0HF4YPBYSSG3JRPRT0SJHXN
    armed         ai-then-human, command ..._iofvz2_ai-then-human_99 completed
                  06:10:57.053679528Z, with NOTHING in flight
    dispatched    06:11:38Z from Factory > Dispatch ready work (the DRAIN)
    loop-pick     06:11:45Z picked ["livespec-console-beads-fabro-iofvz2"]
                  -- exactly the armed item; arm-first held a FOURTH time
    fabro-run     exit 0 at 06:34:41Z (~23 min)
    PR            #747, merged fb9ed6d "fix: keep control commands off drain lane"
    janitor       green
    PARKED        06:45:37Z, acceptance_verdict PASS, policy ai-then-human,
                  advisory: false
    ACCEPTED      06:47:49Z at the human valve, FROM THE MENU
    ledger closed 06:47:52Z -- BY THE HUMAN ACCEPT

The item sat parked at the gate and moved only on the keypress. It was not
closed autonomously.

## The accept, menus-only

No hotkey and no palette was used for any action. The status band advertised
`c accept` throughout and was deliberately ignored.

1. `esc` to the lane list, `Down` x2 to `acceptance (1)`, `Enter` to drill.
   Navigation only -- the menus-only claim is about how ACTIONS are invoked.
2. Target confirmed BY ID on the lane row before anything was invoked:
   `livespec-console-beads-fabro-iofvz2  rank a0  [acceptance]`.
3. **Hotkey-free menu entry**: `Left` from the resting left edge opened the bar
   IN PLACE with the selection preserved -- `Accept work-item [c]` rendered
   AVAILABLE, no `(unavailable here)`. et3.6 held. This is the intervention walk
   #2 had to name and no longer needs naming.
4. **Leg A** captured before invoking (`15-hotkeyfree-menu-entry.txt`).
5. **Leg B**: the Valve modal read back `Accept work-item` /
   `Target: livespec-console-beads-fabro-iofvz2` BEFORE Enter
   (`16-legB-accept-confirm.txt`). It agreed with the lane row, so the `-x6lj`
   selection hazard was ruled out rather than assumed absent.
6. `Enter` committed. Ledger `closed` three seconds later.

**There is no named non-menu step.** Walk #3 needed a ranking consult to know
which item the drain would pick; this walk inherited an already-armed,
already-dispatched item, so no consult was required.

## Leg A: the acceptance lane offers THREE verbs, not two

Measured, with 8 rows marked `(unavailable here)`:

    Move status [s]
    Accept work-item [c]
    Reject work-item [r]

This independently corroborates walk #3's retraction of walk #2's "the acceptance
lane offers exactly Accept and Reject". It is three verbs. The retraction was
right.

## Two negative findings worth as much as a positive

Both failure modes this README's earlier revision told a successor to watch for
did **NOT** occur:

- **E2BIG at `fabro-run` did not recur.** No dispatch had been re-run from THIS
  tenant on the v0.62.7 build, so this walk was the designated confirming test.
  `fabro-run` returned exit 0. **The fix is confirmed on this tenant.**
- **`is missing a scripts/bin directory` at the pre-push hook did not recur.**
  Corroborated three ways: the run pushed and opened PR #747, `check-e2e-tmux`
  passed in CI, and the post-merge janitor came back green.

## The "Set override" label lossiness is FIXED

Walk `../walk-2026-08-20/` measured three menu rows all reading `Set override`
([g], [f], [k]) -- lossy exactly where the menu is the primary vocabulary. They
now render distinctly:

    Set merge-on-review cap [g]
    Set review-fix cap [f]
    Set acceptance-rework cap [k]

## Minor observation, logged not chased

The menu cursor does **not** skip unavailable rows -- stepping down from
`Driver handoff` lands on `Approve work-item [p]  (unavailable here)` before
reaching `Accept work-item`. Harmless here; noted because a keyboard-driven
primary surface that stops on inert rows costs keystrokes proportional to how
many verbs are gated off.

The header carried `sources: 2 unavailable (dispatcher, livespec)` throughout
without affecting the run, as in walk #3.

## What this walk already proved: `x6lj` caught in the act

`09-x6lj-resort-in-walk.md` is the substantive finding from the first session and
is complete and self-contained. Three different work-items occupied one cursor
position inside ninety seconds with no arrow key pressed, and had
`Dispatch selected item` been wired and used, that walk would have dispatched a
DIFFERENT PLAN'S work-item with no modal and no read-back. First in-walk capture
of the mechanism rather than a reconstruction.

## File index

| file | what it captures |
| --- | --- |
| `00-timestamp.txt` | walk start, UTC |
| `01-ranking-consult.json` | the arming session's named non-menu step; `iofvz2` top of five |
| `02-resting.txt` | resting frame at 160 columns |
| `03-ready-drilled.txt` | ready lane; the top pick renders SEVENTH with `rank ~` |
| `04-target-selected.txt` | target confirmed BY ID before acting |
| `05-legA-ready-lane.txt` | leg A — verbs the ready lane offers |
| `06-legB-arm.txt` | leg B — `Target: …iofvz2` / `ai-then-human` read back |
| `07-post-arm.txt` | frame after the arm committed |
| `08-dispatch-timestamp.txt` / `08-dispatch-committed.txt` | dispatch committed; `drain in flight` |
| `09-x6lj-resort-in-walk.txt` / `09-x6lj-resort-in-walk.md` | the in-walk re-sort, capture + write-up |
| `10-journal-loop-pick.txt` | `loop-pick` took exactly the armed item |
| --- | *— successor session resumes here —* |
| `11-resume-timestamp.txt` / `11-resume-monitoring.txt` | inherited state re-measured; `drain in flight` |
| `12-park-timestamp.txt` / `12-parked.txt` | `drain completed` |
| `13-lane-list.txt` | `acceptance (1)` holding `iofvz2` |
| `14-acceptance-drilled.txt` | acceptance lane drilled; target confirmed by ID |
| `15-hotkeyfree-menu-entry.txt` | **leg A** — hotkey-free entry, selection preserved, offered set |
| `16-legB-accept-confirm.txt` | **leg B** — `Target: …iofvz2` read back before Enter |
| `17-accept-timestamp.txt` / `17-accept-committed.txt` | the human accept committed |
| `18-post-accept-lane.txt` | acceptance lane empty |
| `19-ledger-closed.txt` | ledger `closed` 06:47:52Z |
| `20-final-lanes.txt` | final frame |
| `21-journal-park-and-done.txt` | `acceptance-parked` PASS + `done` green + merge sha |
