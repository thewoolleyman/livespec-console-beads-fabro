# Campaign lifecycle #2 — `4nrwmp` — COMPLETE

**Complete.** The drain finished during session wind-down, so the human accept
leg was taken before this session stopped. Every action was invoked from a menu,
no hotkeys, no palette; the one non-menu step is the ranked-pick consult named
below.

Item: `livespec-console-beads-fabro-4nrwmp` — the R8 carrier (per-item dispatch).
Walked 2026-08-20T21:50:55Z onward. `tmux capture-pane -p` (plain, no `-e`),
in order, never reconstructed.

## Why this item was chosen

The foreman's instruction named `et3.2`. That was not executable and I said so
rather than forcing it: `et3.2` is `backlog` (the dispatch verb is gated on
`ready_work_item_count > 0`), and it is a carrier for a **walk someone performs**,
not factory work — dispatching it would ask the factory to build a walk. It is
also plan 02's shared capstone, which deferral D1 keeps separate from plan 04's
milestone. The foreman accepted all three points and corrected the instruction.

`4nrwmp` was taken instead because it was the ranking's top pick **and** because
R8 landing upgrades campaign walks #3–#6 from ranked-drain roulette to targeted
dispatch — making the rest of the campaign cheaper, and making plan 02's R10
capstone clean by construction.

## The legs performed

1. **Arm-first, from the menu** (`02-legB-arm.txt`). Hotkey-free `Left` entry,
   selection intact. Leg B read back `Set acceptance work-item` /
   `Target: livespec-console-beads-fabro-4nrwmp` /
   `Policy/mode: ai-then-human`. Command
   `cmd_work_item_set_acceptance_requested_...4nrwmp_ai-then-human_86`
   **completed immediately** — nothing was in flight.
2. **Dispatched from the menu** (`03-dispatched.txt`). `Factory > Dispatch ready
   work`, reached by `Left` then `Right`, no hotkey. Journal
   `loop-pick {budget: 1, picked: ["livespec-console-beads-fabro-4nrwmp"]}` at
   21:52:29Z — the armed item.
3. **Ran ~40 minutes** to `completed`, then **parked at acceptance**
   (`04-lanes.txt`: `acceptance (1)`; ledger `acceptance` at 22:32:39Z). The gate
   held, because it was armed before the dispatch.
4. **Accepted at the human valve, from the menu.** Leg A first
   (`06-legA-acceptance.txt`): the acceptance lane offered `Accept work-item [c]`
   and `Reject work-item [r]`, with `Approve` and the policy dials
   `(unavailable here)`. Then leg B (`07-legB-accept.txt`): `Accept work-item` /
   `Target: livespec-console-beads-fabro-4nrwmp`, read back before `Enter`.
5. **Leg C**: `cmd_work_item_accept_requested_...4nrwmp` = `completed`; ledger
   `closed` at 22:35:20Z — **by the human accept, not autonomously.**

## What leg B caught, on its first outing after the labels moved

The policy-dial rows have been **renamed** — `et3.5`'s label-lossiness fix has
landed, so the three identical `Set override` rows are gone and row 8 now reads
`Set review-fix cap [f]`. My row-index assumption from the walk-2026-08-20c walk
was therefore stale, and I briefly believed I had opened the wrong valve.

The modal's read-back settled it: the header said `Set acceptance work-item` with
the right target and policy, so the action was confirmed against **what the
surface displayed**, not against my own counting. That is exactly the failure mode
milestone acceptance 2's leg B exists to catch, and it caught it on the first walk
after the labels moved. It also doubles as live confirmation that `et3.5` shipped.

## The named non-menu step

Same as every campaign walk before per-item dispatch exists: a **read-only ranking
consult** to learn which item the drain would pick, so that item could be armed.
Per the coordinator ruling, ranked-drain walks count toward the six provided each
walk's evidence names the ranked pick explicitly as a non-menu step. This is that
naming.

## Cadence constraint this walk confirmed

The campaign is **serial by necessity**. Arming during a live drain leaves the
command `pending` behind it on the serial command worker and the human gate is
lost — measured in `../walk-2026-08-20b/`. `5deyqc` closed that as a warn-only
honesty fix; the structural constraint is `livespec-console-beads-fabro-iofvz2`.
So six lifecycles is six drains end-to-end at ~28 minutes each, not six in
parallel.

## File index

| file | what it captures |
| --- | --- |
| `00-timestamp.txt` | walk start, UTC |
| `00-resting.txt` | resting frame, bar visible |
| `01-ready-drilled.txt` | ready lane; target rendered **seventh** again |
| `02-legB-arm.txt` | leg B — the read-back that caught the moved labels |
| `03-dispatched.txt` | menu dispatch committed, `drain in flight` |
| `04-lanes.txt` | parked at acceptance |
| `05-acceptance-drilled.txt` | the item selected in the acceptance lane |
| `06-legA-acceptance.txt` | leg A — exactly the two lane exits offered |
| `07-legB-accept.txt` | leg B — target read back before commit |
| `08-accepted.txt` / `08-timestamp.txt` | after the human accept |
