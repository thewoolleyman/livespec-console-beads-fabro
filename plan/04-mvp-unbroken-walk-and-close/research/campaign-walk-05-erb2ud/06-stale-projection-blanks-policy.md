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

## Correction, 2026-08-21T04:5xZ: the policy IS on the bead, as a LABEL

The section above says the acceptance policy "is not stored on the bead at all —
`erb2ud`'s ledger metadata holds only `acceptance_criteria` and `rank`". **That is
wrong.** It was concluded from reading the record's `metadata` and top-level
fields and never looking at `labels`, which were sitting in the same JSON.

`erb2ud` at closure carried:

    labels: ["acceptance:ai-then-human", "admission:manual",
             "blocked-reason:needs-human", "origin:freeform"]

And the unarmed items in the same lane carry no `acceptance:*` label at all:

    iofvz2  ready  labels=['origin:freeform']
    qeqax3  ready  labels=['origin:freeform']
    zbnnlv  ready  labels=['intake:triaged', 'origin:freeform']

### This makes the finding STRONGER, not weaker

The speculative half of the original write-up — that the policy lives in
orchestrator state reached through a read surface that was degraded for this row
— is unnecessary and unsupported. The simpler and better-evidenced statement:

**The ledger plainly carried `acceptance:ai-then-human` on `erb2ud`, and the
console still rendered `acceptance_policy — (not emitted; console assumes
ai-then-human)`.** That is the same shape as the `rank: a0` versus `rank ~`
disagreement, against a second field, from the same stale row. The console failed
to read a value that was present, and printed a default in its place.

### And it partly un-does this walk's retraction of walk #3's control

This record retracted walk #3's "unarmed control two rows away" as possibly
stale rather than genuinely unarmed. On the label evidence that particular
control was in fact SOUND: `iofvz2` carries no `acceptance:*` label and was not
armed. The methodological point stands unchanged and is the part worth keeping —
a blank pane is not evidence of an unset value, so the technique is still
ONE-WAY — but the specific accusation against walk #3's control was harsher than
the evidence warranted, and correcting in that direction matters as much as
correcting in the other.

### The generalisation this keeps proving

Three times in this walk a conclusion rested on an absence: a stale pane read as
"unarmed", a local-pool query read as "run gone", a stale ref read as "foreign
commit". This is the fourth, and it is the same shape turned inward — a field
absent from the part of the record I chose to read, taken as absent from the
record. Before concluding something is absent, establish that you would have
seen it if it were there.
