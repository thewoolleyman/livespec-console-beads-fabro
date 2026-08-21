# R10 capstone walk — the claim, PRE-COMMITTED before the walk runs

Written BEFORE any frame was captured, per `-et3.2` amended acceptance clause 4,
adopted from plan 04's discipline: a claim written after the walk can be flattered
to fit whatever happened. This wording is fixed now and the walk either meets it
or does not.

## The claim this walk will either earn or fail

> One lifecycle SEGMENT of livespec-console-beads-fabro was driven MENUS-ONLY at
> the real TUI — every action invoked from a menu row, no hotkeys, no command
> palette — and the segment CONTAINED the dispatch step, taken via
> `Factory > Dispatch ready work`. Exactly ONE out-of-band step was used: the
> read-only ranked-pick consult, named here and banked as evidence. The dispatch
> outcome — success OR refusal — was observed IN the cockpit.

## What FAILS this claim, stated in advance so it cannot be renegotiated later

- **Any hotkey.** `-et3.4` (ce9e245) made the closed bar enterable without a
  hotkey and `-et3.6` (99d19e7) made that entry preserve the item selection, so
  there is no remaining excuse. If a hotkey is used, it is recorded as a FAILURE
  of the menus-only claim and filed, not granted as a convenience. This half of
  the acceptance is NOT relaxed.
- **The command palette**, for anything. Same standing.
- **More than one out-of-band step.** The ranked-pick consult is the ONLY one
  permitted, and only because it is the workaround for filed defect `-4s1h`
  (the ready lane's display order is not the drain's pick order, so an operator
  cannot predict which item their own drain will claim). If a second out-of-band
  step proves necessary, the walk does not silently absorb it — it is recorded
  and the claim is reported as NOT met.
- **A dispatch outcome read from outside the cockpit.** The journal and the
  ledger may CORROBORATE, but the outcome must be visible in the TUI itself.
  Reading the journal instead of the header is a different, weaker claim.
- **Per-item dispatch.** `Factory > Dispatch selected item` is a not-wired stub
  presented as available (`-et3.10`). It is not on this walk's path, and if it
  is invoked by accident that is a failed walk, not a data point.

## What this walk does NOT claim

It is a SEGMENT, not a full unbroken lifecycle in one sitting. Plan 04's
milestone 1 is the strictly stronger claim and is deferral D1 on this plan's
timeline. Nothing here may be reported as a full-pass proof.

## Standing hazard the walk must defend against

`-x6lj` (p1): selection is anchored to a ROW INDEX, not a work-item id, so a
re-sort silently re-aims the next action at a different item — and both dispatch
verbs commit with NO confirmation modal and NO read-back, so there is no moment
at which a re-aimed selection becomes visible. Therefore: **verify the target in
the detail pane immediately before every commit**, and capture that frame.
