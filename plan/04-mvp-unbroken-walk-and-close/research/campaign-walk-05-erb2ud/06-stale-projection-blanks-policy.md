# The stale ready-lane projection blanks `acceptance_policy` too

Measured 2026-08-21T00:18Z during campaign lifecycle #5, before arming.

## What the console showed

`erb2ud`, selected in the drilled ready lane, detail pane:

    rank                 ~
    admission_policy     — (not emitted; console assumes manual)
    acceptance_policy    — (not emitted; console assumes ai-then-human)

## What the ledger says

    erb2ud  | ledger metadata rank = a0 | status ready
    iofvz2  | ledger metadata rank = a0 | status ready

`iofvz2` renders `rank a0` in the same lane at the same moment. `erb2ud`
renders `rank ~`. Both carry `rank: a0` in ledger metadata. So the console
disagrees with ground truth on a field the ledger demonstrably holds — `-v8un`
confirmed against the ledger rather than inferred from a re-sort.

## Why this matters more than a wrong rank

**`rank` and `acceptance_policy` degrade TOGETHER.** During walk #3 at 22:52Z
this same item rendered `rank a0` AND `acceptance_policy ai-then-human`
immediately after being armed. It now renders `rank ~` AND
`acceptance_policy — (not emitted)`. The row is being served from a feed that
carries neither field, and the pane presents the absence as a *policy reading*
rather than as missing data.

The acceptance policy is not stored on the bead at all — `erb2ud`'s ledger
metadata holds only `acceptance_criteria` and `rank`, and its `updated_at` is
`2026-08-20T16:09:20Z`, hours BEFORE walk #3 armed it. The policy lives in the
orchestrator's own state, reached through the read surface that is currently
degraded for this row. So the console cannot presently tell an operator whether
this item is armed, and it says `— (not emitted; console assumes ai-then-human)`
instead of saying it does not know.

## This falsifies a technique plan 04 recommended

Walk #3 recommended, and plan 02 adopted for its R10 capstone, using
`acceptance_policy` reading `ai-then-human` against an unarmed control row as a
menus-only arm confirmation. That recommendation is **only sound for a row whose
projection is fresh**. The failure direction is the dangerous one:

- Reading `ai-then-human` is still trustworthy — it is positive evidence.
- Reading `— (not emitted)` is NOT evidence that the item is unarmed. It is
  equally consistent with an armed item whose row is stale.

Walk #3's own "unarmed control two rows away" was `iofvz2` reading
`— (not emitted)`, which this measurement shows could have been a stale row
rather than an unarmed item. The control was weaker than claimed.

**Corrected guidance:** treat `acceptance_policy` as a one-way confirmation.
`ai-then-human` confirms armed. Anything else confirms nothing, and the row's
`rank` is the tell — a `~` rank means the row is being served stale and every
other field on it is suspect.

## Disposition

Not filed as a new item. This is `-v8un`'s mechanism, and `-v8un` is plan 02's,
already filed and already re-scoped to "ready-lane rows serve STALE projection
values until an unrelated command forces a refresh". What is new here is the
BLAST RADIUS — that the stale set includes `acceptance_policy`, the field this
campaign leans on — and the co-degradation with `rank` that gives an operator a
tell. Reported to plan 02 for `-v8un`, since a fix that restores rank without
restoring policy would look correct and leave the campaign's confirmation
technique broken.

## Addendum, measured after re-arming: the item's OWN command did not refresh it

`-v8un` is currently scoped as *ready-lane rows serve stale projection values
until an unrelated command forces a refresh*. This walk measured something the
"unrelated" qualifier does not cover.

    00:23:42.356Z  cmd_work_item_set_acceptance_requested_...erb2ud_ai-then-human_97  completed
    00:24:36Z      erb2ud detail pane:  rank ~
                                        acceptance_policy — (not emitted; console assumes ai-then-human)

A `set-acceptance` command **against that exact item** completed, and 54 seconds
later the console's own detail pane for that item still showed the stale row and
still could not report the policy it had just set. No re-sort occurred either;
the row stayed at position 6 with the ranking reason as its title.

Contrast walk #3, same command, same item: at 22:50:44Z the arm completed and
the pane read `acceptance_policy ai-then-human` shortly after, with the lane
re-sorting. So the refresh is **not guaranteed and not tied to issuing a
command** — related or unrelated. Whatever drives it is something else, and
"until a command forces a refresh" overstates how reliably an operator can
provoke one.

Practical consequence for a walk: the command spine is the authority on whether
an arm landed. The console is not, and waiting for it to agree can hang a walk
indefinitely on a row that never refreshes.
