# Campaign lifecycle #6 — `qeqax3` — the first PER-ITEM MENU DISPATCH

**COMPLETE AND ACCEPTED AT THE HUMAN VALVE.** Dispatched 2026-08-21T07:32:34Z,
parked at acceptance 08:00:10Z with verdict PASS, accepted from the menu
08:01:24Z, ledger `closed` 08:01:27Z -- BY THE HUMAN ACCEPT. It sat at the gate
and moved only on the keypress.

The first campaign walk driven on a console carrying BOTH `fb9ed6d` (control
commands off the drain lane) and `46b8cce` (per-item factory dispatch wired) —
the two fixes lifecycles #5 and plan 02 respectively produced.

## What is new about this walk

Every previous campaign lifecycle dispatched through the **ranked drain**, which
picks by rank and therefore forced a named non-menu step: a `next --limit 1
--json` ranking consult, so the operator could know which item the drain would
take and arm that one. This walk needed none of it. The operator chose the item,
and the factory dispatched that item.

    id           dispatch-selected-item  ("Dispatch selected item")
    menu_path    ["Factory", "Dispatch"]
    hotkeys      &[]                     -- keyless, menu-only by construction
    modal        Target: livespec-console-beads-fabro-qeqax3
                 Uses Dispatcher loop --budget 1 --parallel 1 --item
    loop-pick    picked ["livespec-console-beads-fabro-qeqax3"]

## Sequence, menus-only

1. Ready lane drilled; `qeqax3` selected and confirmed BY ID.
2. **Hotkey-free menu entry** (`Left` from the resting left edge) — selection
   preserved, per-item verbs AVAILABLE. Leg A captured
   (`09-legA-qeqax3.txt`): on a ready item the offered set is `Move status`,
   `Set merge-on-review cap`, `Set review-fix cap`, `Set acceptance`,
   `Set acceptance-rework cap`; six rows `(unavailable here)`.
3. **Arm first**: `Set acceptance` → `ai-then-human`, leg B read back
   `Target: …qeqax3` before Enter (`10-legB-arm-qeqax3.txt`). Committed
   07:31:04Z.
4. **Selection re-verified by ID** — and it had MOVED. See `-x6lj` below.
5. `Factory > Dispatch > Dispatch selected item`, leg B read back
   `Target: …qeqax3` before Enter (`15-legB-dispatch-readback.txt`). Committed
   07:32:34Z; header went to `factory: dispatch item in flight`.
6. `fabro-run` exit 0 at 07:45:55Z (~13 min); PR #751 merged `86c1499`
   ("fix: stamp command requests per append"); post-merge janitor green.
7. **PARKED** 08:00:10Z, `acceptance_verdict: PASS`, `advisory: false`.
8. `esc` to the lane list, `Down` x2 to `acceptance (1)`, `Enter` to drill;
   target confirmed BY ID (`20-acceptance-drilled.txt`).
9. **Leg A** on the acceptance lane (`21-legA-acceptance-menu.txt`): exactly
   `Move status`, `Accept work-item`, `Reject work-item` available, 8 rows
   `(unavailable here)` -- identical to lifecycle #5, a second independent
   confirmation that the lane offers THREE verbs and not two.
10. **Leg B**: `Accept work-item` / `Target: livespec-console-beads-fabro-qeqax3`
    read back before Enter (`22-legB-accept-confirm.txt`), agreeing with the
    lane row.
11. `Enter` committed 08:01:24Z. Ledger `closed` 08:01:27Z. Acceptance lane went
    empty (`24-post-accept.txt`).

**No hotkey and no palette was used for any action, and there is NO named
non-menu step anywhere in this lifecycle** -- the first campaign walk of which
that is true end to end.

## `fb9ed6d` CONFIRMED on the live surface

The arm in step 3 was committed **while plan 02's `et3.8` drain was 35 minutes
into execution and still running**. `qeqax3` carried no `acceptance:*` label
before it (surveyed minutes earlier, `-- NONE --`) and carried
`acceptance:ai-then-human` seconds after it.

This is the controlled converse of walk #2's measurement, which is what made
`iofvz2` (campaign lifecycle #5) a defect in the first place:

    armed while a drain was executing, PRE-fix  (walk 2):  pending 6+ min, gate LOST
    armed while a drain was executing, POST-fix (walk 6):  completed in seconds

The structural fix landed as part of lifecycle #5's own merged PR #747
("fix: keep control commands off drain lane"). The campaign therefore closed the
loop on a defect it discovered: measured it, filed it, dispatched it, accepted
it, and then re-measured the fixed behaviour under the original failing
conditions.

## `updated_at` DOES NOT TRACK LABEL WRITES — a correction

While selecting a target I read `zbnnlv`'s `updated_at` (2026-08-20T01:34:48Z),
saw it predated my arm, and concluded the item had been pre-armed by an earlier
session. **That inference was wrong.** `qeqax3` falsifies it directly: its
`acceptance:ai-then-human` label is provably new (absent in a survey minutes
earlier) while its `updated_at` still reads 2026-08-20T16:30:09Z.

So `updated_at` cannot be used to date a policy arm, and an acceptance label's
presence says nothing about WHEN it was set. The only reliable way to tell a
fresh arm from a pre-existing one is a before/after label survey, or the command
spine. A successor comparing labels across a walk must not reach for
`updated_at`.

## `-x6lj` is now a LIVE hazard, not a conditional one

Full write-up in `12-x6lj-now-live-hazard.md`; capture in
`12-x6lj-resort-caught.txt`. In short: committing the arm re-sorted the ready
lane (`qeqax3` rank `~` → `a0`, moving it to row 1), the cursor stayed pinned at
row index 6, and row 6 was now **`et3.11`, plan 02's work-item**. No arrow key
was pressed.

Walk 5b could only state this consequence conditionally, because the per-item
port was `not_wired()`. It is wired now, so the hazard can fire. And the
`Factory Dispatch` modal's target read-back does **not** protect against it: the
modal faithfully names whatever is selected *now*, so a re-sorted selection
produces a correct-looking confirmation for an item the operator never chose.
Read-back and selection integrity are independent properties; only one is
present.

Re-reading the row's ID before the commit is the only reason this walk
dispatched `qeqax3` rather than `et3.11`.

`-x6lj` is plan 02's item. This is corroborating evidence at higher severity,
recorded here rather than filed again.

## Side observations

- `sources` FLAPS between `1 unavailable (livespec)` and
  `2 unavailable (dispatcher, livespec)` within a single walk -- observed at
  both values here, minutes apart, with no dispatch state change explaining it.
  An earlier draft of this note claimed the dispatcher-source unavailability was
  "gone on this build"; that was a single sample and it is WITHDRAWN. Flapping,
  not fixed. This is consistent with walk #3's own retraction of a candidate
  explanation for the same source, contradicted fifteen minutes later by the
  same walk.
- The header carries a new `mode: tui` field.
- `factory: dispatch item in flight` is a distinct status from the drain's
  `drain in flight`, so the two dispatch paths are distinguishable on the
  primary surface.
- The menu cursor does not skip `(unavailable here)` rows (also noted in
  lifecycle #5).

## File index

| file | what it captures |
| --- | --- |
| `00-timestamp.txt` / `00-resting.txt` | resting frame, 160 cols, no keystroke sent |
| `01-lanes.txt` | Lanes view |
| `02-ready-drilled.txt` | ready lane drilled, 8 items |
| `03-target-selected.txt` | `zbnnlv` selected (initial target, later re-targeted) |
| `04-legA-hotkeyfree-entry.txt` | leg A via `Left`, selection preserved |
| `05-legB-arm.txt` | leg B read-back for the `zbnnlv` arm |
| `06-arm-timestamp.txt` / `06-arm-committed.txt` | `zbnnlv` arm committed 07:29:20Z |
| `07-state-before-retarget.txt` | clean lane state, no modal open |
| `08-target-qeqax3.txt` | `qeqax3` selected, confirmed by ID |
| `09-legA-qeqax3.txt` | **leg A** on `qeqax3` — offered set on a ready item |
| `10-legB-arm-qeqax3.txt` | **leg B** — `Target: …qeqax3` / `ai-then-human` |
| `11-arm-timestamp.txt` / `11-arm-committed.txt` | arm committed 07:31:04Z during a live drain |
| `12-x6lj-resort-caught.txt` / `12-x6lj-now-live-hazard.md` | the re-sort, capture + write-up |
| `13-reselected-qeqax3.txt` | selection restored, re-verified by ID |
| `14-factory-menu.txt` | `Dispatch selected item` renders AVAILABLE |
| `15-dispatch-timestamp.txt` / `15-legB-dispatch-readback.txt` | **leg B** — dispatch modal target read-back |
| `16-dispatch-commit-timestamp.txt` / `16-dispatch-committed.txt` | dispatch committed; `dispatch item in flight` |
| `17-journal-loop-pick.txt` | `loop-pick` took exactly `qeqax3` via the `--item` path |
| `18-park-timestamp.txt` / `18-parked.txt` | `dispatch item completed` |
| `19-lane-list.txt` | `acceptance (1)` |
| `20-acceptance-drilled.txt` | acceptance lane drilled; target confirmed by ID |
| `21-legA-acceptance-menu.txt` | **leg A** — three verbs offered, 8 unavailable |
| `22-legB-accept-confirm.txt` | **leg B** — `Target: …qeqax3` read back before Enter |
| `23-accept-timestamp.txt` / `23-accept-committed.txt` | the human accept committed |
| `24-post-accept.txt` | acceptance lane empty |
| `25-ledger-closed.txt` | ledger `closed` 08:01:27Z |
| `26-final-lanes.txt` | final frame |
| `27-journal-summary.txt` | `loop-pick` + `acceptance-parked` PASS + `done` green |
