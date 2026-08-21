# `x6lj` observed firing DURING a walk — first in-walk capture

Every prior record of `-x6lj` (selection anchored to a row index, not a
work-item id) was a reconstruction after the fact. This is the mechanism caught
in the act, mid-lifecycle, with the cursor never touched between observations.

## The three states, same cursor position, no arrow key pressed

**Before the arm** (`04-target-selected.txt`, 06:10:4xZ) — ready lane, 9 rows,
cursor at row 7:

    1 qeqax3   a0     5 et3.11  ~
    2 wnlcnj   a0     6 et3.8   ~
    3 zbnnlv   a0   > 7 iofvz2  ~   <- SELECTED, the drain's top pick
    4 4s1h     ~      8 x6lj    ~
                      9 zfcp    ~

**After the arm, before the dispatch** (06:11:38Z) — cursor still row 7, now
reading:

    > livespec-console-beads-fabro-et3.8

The arm refreshed `iofvz2`'s projection, it gained `rank a0`, and it sorted UP
into the a0 group — pushing `et3.8` down into row 7 underneath a cursor that had
not moved.

**After the dispatch** (`09-x6lj-resort-in-walk.txt`) — `iofvz2` has left the
ready lane for `active`, 8 rows remain, and row 7 now reads:

    > livespec-console-beads-fabro-x6lj

Three different work-items occupied one cursor position inside about ninety
seconds, none of them because an operator moved.

## The consequence, made concrete

This walk dispatched with `Factory > Dispatch ready work` — the DRAIN, which
picks by rank and ignores the selection — so the dispatch was unaffected and
`loop-pick` took exactly the armed `iofvz2`.

**Had `Dispatch selected item` been wired and used, this walk would have
dispatched `et3.8` — a DIFFERENT PLAN'S work-item — while the operator read
`iofvz2` on screen moments earlier and confirmed it by id.** That is the sharpest
available statement of `x6lj`'s severity, and it is measured rather than argued:
the per-item verb commits with no modal and no read-back (walk #3), so there
would have been no moment at which the substitution became visible.

Note the irony worth recording: the row that ended up under the cursor at the
end is `x6lj` itself.

## What caught it

Reading the ROW'S ID immediately before committing, rather than trusting the
cursor — the interim guidance recorded on `-x6lj` after walk #3, sharpened by
plan 02 after its R10 near-miss. It worked exactly as intended and cost one
capture. Without it this walk would have believed it dispatched `iofvz2` by
selection and been right only by accident, because the drain does not read the
selection at all.
