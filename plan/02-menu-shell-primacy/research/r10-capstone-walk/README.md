# R10 capstone walk — `ekb5vq` — the menus-only lifecycle segment

Walked 2026-08-20T23:35:34Z onward against a RELEASE binary built from master
`1cbddf1`, run under the credential wrapper against this repo's LIVE tenant.

**Master advanced to `e073d76` DURING this walk** (plan 04's merged PR #735,
`chore: add charter defect gate`). The walked binary was built from `1cbddf1` and
was NOT rebuilt mid-walk. Disclosed rather than glossed: the delta is a
dev-tooling gate addition with no console behaviour in it, so it does not affect
anything measured here — but the binary's provenance is `1cbddf1`, not the
`e073d76` a reader would infer from the repo state when this lands.
Captures are `tmux capture-pane -p` — plain, no `-e` — taken in the order shown
and never reconstructed.

The claim was PRE-COMMITTED in `00-precommitted-claim.md` BEFORE the first frame
was captured. Read that file first; this record is judged against it, not against
wording chosen after the fact.

Pane 200x50, deliberately. See "The width trap" below — at the width tmux
actually defaulted to, this walk could not have read its own outcome.

## VERDICT: the pre-committed claim is MET

Judged against `00-precommitted-claim.md`, unchanged since before the first
frame. Every action was invoked from a menu row. NO hotkey was used at any point.
NO command palette. EXACTLY ONE out-of-band step — the ranked-pick consult — and
it is named, banked, and justified below. The dispatch outcome was observed IN
the cockpit at a width where the fitter does not evict it.

## What was walked

One lifecycle segment on `livespec-console-beads-fabro-ekb5vq`, which ran all the
way through the human accept:

| time (UTC) | step |
| --- | --- |
| 23:35:34 | walk start; `-et3.2` moved `backlog -> active` by hand |
| 23:35 | ranked-pick consult — THE one out-of-band step — names `ekb5vq` |
| 23:40 | target selected, identity + UNARMED baseline verified in the detail pane |
| 23:41 | armed `ai-then-human` from `Work item > Policy dials > Set acceptance` |
| 23:41-23:42 | the re-sort, and the 84 seconds of misdirected verification (below) |
| 23:42:19 | dispatched from `Factory > Dispatch ready work`; header `factory: drain in flight` |
| 23:42:24 | `loop-pick picked: ["...-ekb5vq"]` — exactly the armed item |
| 00:04:27 | parked at acceptance, `acceptance_verdict: PASS`, policy `ai-then-human` |
| 00:06:09 | **human accept taken from `Work item > Lifecycle > Accept work-item`** |
| 00:06:12 | ledger `closed` |

**The human gate is real, and this walk measures it rather than assuming it.**
The item sat parked at acceptance for 1m42s after a PASS verdict and moved only
when a human pressed Enter. A PASS verdict alone did not close it.

Note this walk is a SEGMENT that happens to run through an accept. It is NOT a
full unbroken lifecycle in one sitting — plan 04's milestone 1 is that strictly
stronger claim, and nothing here may be reported as a full-pass proof.

Every action was invoked from a menu row. No hotkeys. No command palette.
EXACTLY ONE out-of-band step was used, and it is named: the ranked-pick consult
(`02-ranking-consult.json`).

## The named out-of-band step, and why it is not laziness

`next --limit 6 --json` at 23:35Z returned, in rank order:
`ekb5vq, erb2ud, iofvz2, qeqax3, wnlcnj, zbnnlv` — all `rank a0`, 12 ready total.

The consult is the workaround for filed defect `-4s1h`, and this walk shows
exactly why it is needed. In the lane, `ekb5vq` displayed **SEVENTH**, carrying
`rank ~` and with the ranking REASON (`ranked ready item (rank a0, origin
freeform)`) rendered where its title belongs — the `-v8un` stale-projection
symptom. An operator reading the lane would have predicted `erb2ud`, which
displayed first. The drain picked `ekb5vq`. Display order is not pick order, and
the lane does not merely fail to show the pick order — it actively displays the
true top pick at position 7 with a sentinel rank that sorts it last.

`13-journal-loop-pick.txt`: `loop-pick {budget: 1, picked:
["livespec-console-beads-fabro-ekb5vq"]}`. The drain picked exactly the armed
item; arm-first held.

## THE FINDING: `-x6lj` re-aims VERIFICATION, not just actions

This is the walk's most important measurement and it is a strict extension of
what `-x6lj` currently says.

`-x6lj` is filed as: selection is anchored to a row index, so a re-sort silently
re-aims the next ACTION at a different item. Interim guidance on the item is
"verify the target in the detail pane immediately before every commit". I
followed that guidance exactly, and the defect still bit — on the READ-BACK.

In order, measured:

1. Selected row 7 (`ekb5vq`), opened its detail, confirmed identity and that it
   was UNARMED: `acceptance_policy  — (not emitted; console assumes
   ai-then-human)` (`05-target-detail-preverify.txt`).
2. Armed from the menu. The valve read back `Target:
   livespec-console-beads-fabro-ekb5vq` / `Policy/mode: ai-then-human`
   (`07-set-acceptance-form.txt`), and I confirmed (`08-arm-committed.txt`).
3. Polled the detail pane for the arm to appear. **Twelve times over 84 seconds
   it read `— (not emitted; console assumes ai-then-human)`.** I was one step
   from recording that the arm had been silently dropped — `-zbnnlv` is exactly
   that defect, which made the false conclusion entirely plausible.
4. That conclusion was WRONG. The projection had refreshed and re-sorted:
   `ekb5vq` moved row 7 -> row 1 and gained `rank a0` and its real title. The
   cursor stayed on row 7, which was now `-4s1h`. Every one of those twelve
   "unarmed" reads was `-4s1h`'s detail pane (`09-resort-reaimed-selection.txt`).
5. Re-selected `ekb5vq` at row 1: `acceptance_policy  ai-then-human`
   (`10-arm-confirmed-ekb5vq.txt`). **The arm had landed on the first attempt.**

Why this matters more than the filed framing: "verify before every commit" is
necessary but NOT SUFFICIENT. I did verify before the commit, and the commit was
correct. The defect struck afterwards, because "open the detail of the selected
row" silently changes WHICH ITEM YOU ARE READING. A walk can verify correctly,
commit correctly, then confirm against the wrong item — and both reads look
entirely normal. Had I trusted my own careful verification, this record would
carry a fabricated `-zbnnlv` sighting against a command that worked perfectly.

Guidance that follows, stronger than what is on the item today: after ANY command
that refreshes the projection, RE-ESTABLISH THE SELECTION BY READING THE ROW'S
ID before trusting a detail pane. Never assume the cursor still points where it
was left.

## The width trap — why this walk ran at 200 columns

`tmux new-session -x 200 -y 50` DID NOT HOLD: the pane came up **105x52**, and at
that width the header fitter (`-et3.11`) dropped both the `fleet:` and
`factory:` segments. `factory:` is the ONLY in-cockpit surface carrying the
dispatch outcome, so a walk at the default width literally cannot observe what it
claims to observe, and would have had to reach outside the cockpit to finish —
spending an out-of-band step the acceptance does not grant.

Caught on a throwaway smoke run against an ISOLATED event store before the real
walk, not during it. Fix: `window-size manual` plus an explicit `resize-window`,
and verifying the width is now a step of the runbook. This is `-et3.11`'s
practical cost, measured: the defect does not merely hide a signal, it can
invalidate a walk that does not know to defend against it.

## Availability is honest — a suspicion raised and RETRACTED

On the Attention view, `Set acceptance` rendered available while its sibling
policy dials read `(unavailable here)`. I suspected an availability defect and
checked `per_item_verb_is_state_valid` against all eight markers before writing
anything down: the selected attention row was `5zjk5b`, an ACTIVE item, and every
marker matches the active lane exactly. Correct behaviour. Recorded here because
a retracted suspicion is cheaper for the next walker than a re-investigation, and
because this plan has spent its life distinguishing measured behaviour from
inferred behaviour.

On the ready lane the same menu offered `Move status`, `Set merge-on-review cap`,
`Set review-fix cap`, `Set acceptance`, `Set acceptance-rework cap` as available
and everything else `(unavailable here)` (`06-hotkeyfree-menu-entry.txt`) —
again exactly the ready-lane row of the table.

## `-et3.10` corroborated in passing

`Factory > Dispatch` offers `Dispatch ready work [menu]` and `Dispatch selected
item [menu]`, both keyless and menu-only. `Dispatch selected item` renders with
NO `(unavailable here)` marker (`11-factory-dispatch-menu.txt`) — presented as
available while its port returns `not_wired()` unconditionally. Not invoked: my
pre-committed claim names invoking it as a failed walk, not a data point.

## Side observation, logged not chased

The drain logged `dispatch-claim-abandoned reason=no-outcome-since-ledger-admit`
against `-et3.2` at 23:42:24Z. `-et3.2` is this walk's own R10 carrier, which the
plan's procedure says to move to `active` BY HAND. To the drain's stranded-claim
reconciler, a hand-made `active` with no dispatcher claim looks stranded. It
released a claim that never existed; the status survived as `active`, verified
after the fact. No damage, and not filed — recorded so the next person to
hand-move a non-factory carrier and then drain does not chase an alarming line.

## Leg A in the acceptance lane: THREE verbs, not two

`18-legA-acceptance-verbs.txt`: the acceptance lane offers `Move status`,
`Accept work-item` and `Reject work-item` as available; everything else —
Driver handoff, Approve, all five policy dials, workflow scope override — reads
`(unavailable here)`. Note `Set acceptance` is correctly UNAVAILABLE here: the
acceptance lane is not in its valid set, so the console will not let you re-arm
a policy on an item already parked against it.

This independently confirms the plan-04 session's correction that walk #2's
"exactly Accept and Reject" was imprecise. Recorded because two walks now agree
and the earlier record does not.

## The `sources` question, answered by round trip

The plan-04 session hypothesised from ONE transition that the `dispatcher` source
reads unavailable simply because no dispatch is running, and named the clean test:
whether it returns to 2 once the drain completes. This walk ran the full round
trip on one instance:

- before dispatch: `sources: 2 unavailable (dispatcher, livespec)`
- drain in flight: `sources: 1 unavailable (livespec)`
- drain completed: `sources: 2 unavailable (dispatcher, livespec)`

That is the clean test, and it passes. The `dispatcher` source's unavailability
tracks whether a dispatch is running; it is not evidence of a broken source.
`livespec`'s remains the known absent-binary case. Logged, not chased — but the
arc no longer needs to carry this half as unexplained.

## File index

| file | what it captures |
| --- | --- |
| `00-precommitted-claim.md` | the claim, fixed BEFORE the walk |
| `01-walk-start-timestamp.txt` | walk start, UTC |
| `02-ranking-consult.json` | THE named out-of-band step |
| `03-resting.txt` / `03-resting-timestamp.txt` | resting frame, bar visible, `factory:` legible at 200 cols |
| `04-ready-lane-drilled.txt` | the lane as displayed — `ekb5vq` 7th with `rank ~` and reason-as-title |
| `05-target-detail-preverify.txt` | target identity + UNARMED baseline, same item |
| `06-hotkeyfree-menu-entry.txt` | `Left` opens the bar, selection preserved; ready-lane leg A |
| `07-set-acceptance-form.txt` | valve read-back naming the target |
| `08-arm-committed.txt` / `08-arm-timestamp.txt` | arm committed, no modal, no read-back |
| `09-resort-reaimed-selection.txt` / `09-resort-timestamp.txt` | **the re-sort that re-aimed the cursor** |
| `10-arm-confirmed-ekb5vq.txt` | `acceptance_policy ai-then-human` on the RIGHT item |
| `11-factory-dispatch-menu.txt` | both dispatch verbs, the stub presented as available |
| `12-dispatch-committed.txt` / `12-dispatch-timestamp.txt` | dispatch committed; header `factory: drain in flight` |
| `13-journal-loop-pick.txt` | drain picked exactly the armed item |
| `14-cockpit-responsive-during-drain.txt` | menus still navigable with the drain in flight |
| `15-parked-header.txt` | post-drain header; `sources` back to 2 |
| `16-acceptance-lane-drilled.txt` | the parked item in the acceptance lane |
| `17-accept-preverify-detail.txt` | target verified: lane row and detail header agree |
| `18-legA-acceptance-verbs.txt` | leg A — THREE available verbs |
| `19-legB-accept-readback.txt` | accept valve read-back naming the target |
| `20-accept-committed.txt` / `20-accept-timestamp.txt` | the human accept |
| `21-accepted-lane-cleared.txt` / `21-walk-end-timestamp.txt` | acceptance lane empty; walk end |
