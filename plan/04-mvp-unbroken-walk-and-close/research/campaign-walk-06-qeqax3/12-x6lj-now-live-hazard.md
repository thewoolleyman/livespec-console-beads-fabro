# `-x6lj` is no longer a theoretical hazard — captured against WIRED per-item dispatch

**Measured 2026-08-21T07:31Z, campaign lifecycle #6, master `22fce15` + the
console rebuilt from it (so carrying `46b8cce` "fix: wire selected factory
dispatch").**

## What happened

1. `qeqax3` was selected in the drilled ready lane, confirmed BY ID:
   `> livespec-console-beads-fabro-qeqax3  rank ~  [ready]` — it sat at **row 6**.
2. `Set acceptance` → `ai-then-human` was committed from the menu. The command
   completed in seconds.
3. That commit refreshed the projection. `qeqax3`'s rank changed `~` → `a0`, so
   the lane **re-sorted** and `qeqax3` moved to **row 1**.
4. The cursor did not follow it. It stayed pinned at **row index 6**, which now
   held **`livespec-console-beads-fabro-et3.11` — PLAN 02'S WORK-ITEM.**

No arrow key was pressed between steps 2 and 4. The capture is
`12-x6lj-resort-caught.txt`.

## Why this is worse than the walk-5b capture

Walk 5b (`../campaign-walk-05b/09-x6lj-resort-in-walk.md`) recorded the same
mechanism and had to state its consequence in the conditional: *"had
`Dispatch selected item` been wired and used, this walk would have dispatched a
DIFFERENT PLAN'S work-item."* At that time the port returned `not_wired()`
unconditionally, so the hazard could not actually fire.

**It is wired now.** `et3.10` landed as `46b8cce`, and this walk confirmed on the
live surface that `Dispatch selected item` renders available, opens a
`Factory Dispatch` modal, and really dispatches. So the conditional is
discharged: the next operator to hit this re-sort and reach for per-item dispatch
WILL dispatch the wrong item.

## The read-back does not save you

This is the part worth dwelling on. The `Factory Dispatch` modal DOES read back
its target:

    Dispatch selected work-item
    Target: livespec-console-beads-fabro-qeqax3
    Uses Dispatcher loop --budget 1 --parallel 1 --item
    Enter to dispatch | Esc to cancel

A leg-B read-back defends against the command mis-resolving its target. It does
NOT defend against the *selection itself* having silently moved, because the
modal faithfully reads back **whatever is selected now** — which after a re-sort
is the wrong row. The operator sees a correct-looking confirmation naming an item
they never chose. Read-back and selection integrity are independent properties,
and only one of them is present.

## What caught it

Re-reading the ROW'S ID after the arm and before the dispatch — the rule walk #3
wrote after plan 02 nearly filed a false `-zbnnlv` against a command that had
worked. It is the only reason this walk dispatched `qeqax3` and not `et3.11`.

Concretely: after the arm, `Up` × 5 restored the selection, re-verified by ID
(`> livespec-console-beads-fabro-qeqax3  rank a0`) before the Factory menu was
opened.

## Ownership

`-x6lj` is **plan 02's** item (they filed it from plan 04's walk-#3 evidence).
This is corroborating evidence at higher severity, not a new defect, and it is
recorded here rather than filed again. The severity argument that belongs to the
owner: the item is already p1 on a selection-correctness rationale; the wiring of
per-item dispatch converts its worst case from "the operator acts on a stale
detail pane" to "the operator dispatches another plan's work-item through a
confirmation dialog that looks right."
