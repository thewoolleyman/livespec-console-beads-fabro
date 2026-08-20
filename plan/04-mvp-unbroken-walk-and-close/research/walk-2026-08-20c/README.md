# Milestone 1 walk, third attempt, 2026-08-20 — COMPLETE

**One unbroken pass, single item, real stack, one sitting, NO HOTKEYS AND NO
PALETTE.** Every action was invoked from a menu. This is the walk the charter
asks for, with one out-of-band READ named in §5.

Graded by the foreman as **milestone 1 discharged**, on the pre-committed
description below rather than on any self-assessment by the session that
performed it.

Item: `livespec-console-beads-fabro-5deyqc`. Captured live in order against the
real binary on the live tenant, `tmux capture-pane -p` (plain, no `-e`),
2026-08-20T18:33:11Z → 19:05:07Z. Nothing reconstructed afterwards.

## 1. The prior attempts this one stands on

- `../walk-2026-08-20/` — **refused**: no menu dispatched. Nothing in
  `ACTION_REGISTRY` drained, and `:drain` was deliberately not used.
- `../walk-2026-08-20b/` — **partial pass**: dispatch worked, but the human accept
  leg was never presented because the acceptance policy was armed *after* the
  dispatch and queued behind it.

Both blockers were fixed by plan 02 between attempts, and both fixes were
re-measured here rather than taken on report.

## 2. The pass, step by step

**Step 1 — `et3.6` verified live before relying on it.**
`03-hotkeyfree-entry-preserves-selection.txt`: with the item selected in the
drilled ready lane, `Left` opened the bar **in place** and the per-item actions
were **available** — `Move status [s]`, `Set acceptance [n]`, and the three
overrides, none carrying `(unavailable here)`. The selection survives hotkey-free
entry, so the `v` intervention walk 2 had to name is gone.

**Step 2 — armed the human gate FIRST, from the menu.**
`04-legB-acceptance-confirm.txt` captures leg B: `Target:
livespec-console-beads-fabro-5deyqc` and `Policy/mode: ai-then-human`, read back
**before** `Enter`. Command
`cmd_work_item_set_acceptance_requested_...5deyqc_ai-then-human_81` **completed
within seconds** — nothing was in flight. This is the ordering walk 2 got wrong.

**Step 3 — dispatched from the menu.** `06-factory-node.txt` →
`07-dispatch-committed.txt`. `Factory > Dispatch ready work`, reached by `Left`
then `Right`, no hotkey. The journal recorded
`loop-pick {budget: 1, picked: ["livespec-console-beads-fabro-5deyqc"]}` at
18:35:03Z — one keystroke, one item, and **the item that had just been armed**.

**Step 4 — monitored.** The drain ran ~28 minutes holding
`tmp/fabro-dispatch-livespec-console-beads-fabro-5deyqc.lock`, then reached
`completed`.

**Step 5 — the gate HELD.** `08-parked-at-acceptance.txt`: ledger `acceptance` at
19:03:15Z, console `acceptance (1)` / `active (0)`.

**Step 6 — accepted at the human valve, from the menu.**
`10-legA-acceptance-offered.txt` is leg A for this step: in the acceptance lane
the menu offered exactly the two lane exits — `Accept work-item [c]` and `Reject
work-item [r]` — while `Approve` and every policy dial rendered
`(unavailable here)`. `11-legB-accept-confirm.txt` is leg B: `Target:
livespec-console-beads-fabro-5deyqc` read back before `Enter`.

**Step 7 — leg C.** `cmd_work_item_accept_requested_...5deyqc` = `completed`;
ledger `closed` at 19:05:02Z. **Closed by the human accept, not autonomously.**

## 3. Milestone acceptance 2, satisfied in the same pass

All three legs were captured live at both valves, never reconstructed:

| leg | where |
| --- | --- |
| A — offered-actions state before invocation | `03-…`, `10-legA-acceptance-offered.txt` |
| B — confirmation's exact target read back before committing | `04-legB-acceptance-confirm.txt`, `11-legB-accept-confirm.txt` |
| C — ledger check after | command spine + `bd` reads quoted in §2 |

The leg-A derivation designed in `../milestone-2-evidence-design.md` works as
specified: the textual `(unavailable here)` markers survive a plain capture, so
the offered set is the unmarked subset, with no out-of-band query.

## 4. The controlled comparison this walk produced

Same command, same surface, differing only in whether a dispatch was running:

| condition | outcome |
| --- | --- |
| armed while a drain was `executing` (walk 2) | `pending` for 6+ minutes; gate lost; item auto-closed |
| armed with nothing in flight (walk 3) | `completed` in seconds; gate held; item parked for the human |

This is independent corroboration of `livespec-console-beads-fabro-5deyqc`'s own
diagnosis, and it cost nothing — it fell out of running the corrected sequence.

Worth recording: the item walked to completion here **is** `5deyqc`, the
lost-human-gate defect that walk 2 discovered.

## 5. The one named non-menu step

A **read-only ranking consult** — `next --limit 1 --json`, output banked as
`ranking-consult.json` — to learn which item the drain would pick, so that item
could be armed. It invoked nothing and mutated nothing.

It is named rather than hidden because **it was necessary**, and the reason it
was necessary is a finding: the console displayed the top pick **seventh** in the
ready lane. Display order is not pick order, so an operator using only the
primary surface cannot know what its own Dispatch button will do. That is the
per-item dispatch obligation (R8, standing on `-et3` with `-8aw`) in operator
terms, now measured twice — here and in walk 2.

A successor following this record without the consult would arm the wrong item
and lose the gate exactly as walk 2 did.

## 6. How this pass is described, verbatim

Pre-committed before the walk ran, and adopted as the record:

> Clean on the menus-only criterion — every action invoked from a menu, no
> hotkeys, no palette — with one out-of-band read named.

The session that performed the walk declined to grade whether that counts as
"clean" full stop; the foreman decided it does and discharged milestone 1, with
reasoning recorded on the foreman timeline and reported for maintainer objection.

## 7. Side observation, not chased

The header carried `sources: 2 unavailable (dispatcher, livespec)` throughout,
without affecting the run. `livespec` is the known absent-binary case; the
`dispatcher` one is unexplained and was deliberately not investigated here.

## File index

| file | what it captures |
| --- | --- |
| `00-timestamp.txt` / `12-timestamp.txt` | walk start and end, UTC |
| `00-resting.txt` | resting frame |
| `01-ready-drilled.txt` / `02-target-selected.txt` | target selected; note it renders **seventh** |
| `03-hotkeyfree-entry-preserves-selection.txt` | `et3.6` measurement — per-item actions available after `Left` |
| `04-legB-acceptance-confirm.txt` / `05-acceptance-armed.txt` | gate armed first |
| `06-factory-node.txt` / `07-dispatch-committed.txt` | menu dispatch |
| `08-parked-at-acceptance.txt` | the gate held |
| `09-acceptance-drilled.txt` / `10-legA-acceptance-offered.txt` | acceptance lane; exactly two exits offered |
| `11-legB-accept-confirm.txt` / `12-accepted.txt` | the human accept |
| `ranking-consult.json` | the one named out-of-band read |
