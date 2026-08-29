# Milestone 1 walk, second attempt, 2026-08-20 — PARTIAL PASS

The first attempt that day (`../walk-2026-08-20/`) was refused before it started:
no menu dispatched. Plan 02 landed a keyless `dispatch-ready` action and a
hotkey-free bar entry, and this attempt ran.

**Outcome: a partial pass.** The item was dispatched from the menu and ran to
completion, and then the human accept leg was never presented — the item closed
autonomously. This is not the clean pass the charter asks for, and it is not a
refusal either. Milestone acceptance 3 anticipates exactly this and requires the
interventions be named, which §4 does.

Captured live in order against the real binary on the live tenant,
`tmux capture-pane -p` (plain, no `-e`), 2026-08-20T15:56:52Z → 16:26:44Z.
Nothing reconstructed afterwards.

## 1. Both release-condition halves: MET

- **`00-resting.txt`** — resting frame, no keystroke sent. The bar now carries a
  fifth node: `Menu:  Work item   Factory   View   Help   File`.
- **`01-bar-entered-no-hotkey.txt`** — `Left` from the resting left edge opened
  the bar (`[Work item]` highlighted) with no registry hotkey.
- **`02-factory-node.txt`** — `Factory > Dispatch > Dispatch ready work [menu]`,
  rendered with no unavailable marker. In source: id `dispatch-ready`,
  `hotkeys: &[]` (genuinely keyless), `menu_path: &["Factory", "Dispatch"]`,
  availability `ctx.ready_work_item_count > 0`.

## 2. What passed

**Dispatch, menu-driven.** `Enter` on `Dispatch ready work` fired a real drain
(`03-dispatch-committed.txt`: header `factory: drain in flight`). The dispatcher
journal recorded `ledger-admit` (assignee fabro), then
`loop-pick {budget: 1, picked: ["livespec-console-beads-fabro-25rvmd"]}`, then
`dispatch-id 76dafe296fd34c1ea37e12bc63c738dd`. **One keystroke, exactly one
item** — `-9ts`'s budget fix holds on the menu path.

**Monitoring.** `04-lanes.txt` shows `active (1); executing 1 claimed 0` — the
executing-vs-claimed distinction (`-3lxx7t`) reading correctly. The cockpit did
not freeze; the header tracked `drain in flight` → `drain completed`
(`11-final-state.txt`). The dispatch process lived ~27 minutes and exited.

**Milestone 2 legs B and C.** `09-legB-acceptance-confirm.txt` captures the Valve
modal reading back `Target: livespec-console-beads-fabro-25rvmd` and
`Policy/mode: ai-then-human` **before** `Enter`. The command spine then recorded
`cmd_work_item_set_acceptance_requested_...25rvmd_ai-then-human_78`, and
`work_item.action.completed {"action_id":"set-acceptance:...:ai-then-human"}`.

## 3. What did not happen: the accept leg

The item never entered the acceptance lane. It went from `active` to `closed`
(ledger `updated_at 2026-08-20T16:24:59Z`), with
`cmd_autonomous_reflect_acceptance_...25rvmd` completed. The human accept valve
was never presented, so it could not be taken. `11-final-state.txt` shows the
active lane empty and `factory: drain completed`.

**Why, and it is not a silent drop.** The set-acceptance command sat `pending`
while the drain sat `executing` — observed repeatedly across ~6 minutes — and
both completed only after the dispatch finished. That is serial-worker
head-of-line blocking, the mundane explanation, **not** `-zbnnlv`'s silent-drop
family. Do not conflate them.

The consequence is the finding: **the policy could not gate the run it was queued
behind.** A human gate requires `Lane::Acceptance` together with
`AiThenHuman`/`HumanOnly` (`crates/console-application/src/lib.rs:6400-6412`);
by the time the policy applied, acceptance had already been decided
autonomously and the item was closed. The operator-facing shape: you arm a human
gate mid-dispatch, the UI shows a confirm dialog naming the exact target and
policy, the command reports completed — and the gate does not apply to the run
you are watching, with nothing saying so. Filed as
`livespec-console-beads-fabro-5deyqc`.

## 4. Interventions, named as milestone acceptance 3 requires

1. **`v` was used to open the menu.** The hotkey-free entry path (`Left`) works,
   but it **costs the item selection**, and every per-item action then renders
   `(unavailable here)`. Measured side by side on the same item in the same lane:

   | row | via `v` (hotkey) | via `Left` (hotkey-free) |
   | --- | --- | --- |
   | `Set acceptance [n]` | available | `(unavailable here)` |
   | `Set override [k]` | available | `(unavailable here)` |

   `06-menu-from-lane.txt` and `07-menu-after-leaving-drill.txt` are the
   hotkey-free path; `08-menu-via-hotkey-from-drill.txt` is the `v` path.
   Mechanism: `Left` from a drilled-in lane exits the drill, so
   `selected_work_item_id` is gone by the time the bar opens. So this is **a pass
   with one named intervention, not the clean menus-only pass.**

2. **The walk's own sequencing was wrong.** Acceptance was armed *after*
   dispatch, which is what put it behind the drain. The correct order is
   arm-then-dispatch. It was done this way because the operator cannot know which
   item the drain will pick — see §5.

## 5. Why this walk is not yet repeatable

`Dispatch ready work` drains by the dispatcher's own ranking. The operator cannot
target an item: `25rvmd` was picked while displayed **last** in the ready lane, so
display order is not pick order. Arming a gate in advance is therefore a coin
flip against the ranking, and arming every ready item would pollute eight other
items' policies.

A clean, repeatable, gate-exercising pass needs both of plan 02's pieces:
selection-preserving hotkey-free entry (from §4's finding) and the per-item
dispatch verb (the standing inherited obligation on `-et3`, carried with `-8aw`
as R8). This walk is the first measured proof of why R8 matters. A retry before
both land was considered and **held** — it could not produce the clean pass nor
reliably exercise the gate.

## 6. Defects this walk produced, all filed by the foreman

| item | finding |
| --- | --- |
| `erb2ud` | console reports a false `Dispatcher backlog bounce` for a running dispatch; `BacklogBounce` is the fallback kind for any non-outcome journal entry (`source_adapters.rs:2490-2515`), confirmed by the `terminal_status: None` payload signature |
| `5deyqc` | the lost human gate (§3) |
| `qeqax3` | degenerate command-spine timestamps — the drain, the set-acceptance and the autonomous reflect all carry `requested_at = 2026-08-20T15:56:20.521745397Z`, identical to the nanosecond, so the spine cannot order or age its own commands. None of this README's ordering claims rest on those fields; they rest on live observation. |

## File index

| file | what it captures |
| --- | --- |
| `00-timestamp.txt` / `11-timestamp.txt` | walk start and end, UTC |
| `00-resting.txt` | resting frame, no keystroke — bar with the Factory node |
| `01-bar-entered-no-hotkey.txt` | `Left` opens the bar without a hotkey |
| `02-factory-node.txt` | the keyless `Dispatch ready work` row, available |
| `03-legB-confirm-target.txt` / `03-dispatch-committed.txt` | the frame after `Enter` on dispatch — note there is **no** confirm step for this global action |
| `04-lanes.txt` | `active (1); executing 1 claimed 0` |
| `05-active-drilled.txt` | the dispatched item, drilled in |
| `06-menu-from-lane.txt` | `Left` exits the drill |
| `07-menu-after-leaving-drill.txt` | hotkey-free entry: every per-item row unavailable |
| `08-menu-via-hotkey-from-drill.txt` | `v` entry: the same rows available |
| `09-legB-acceptance-confirm.txt` | leg B — target and policy read back before commit |
| `10-acceptance-committed.txt` | after confirming set-acceptance |
| `11-final-state.txt` | drain completed, active lane empty, item closed without a human gate |
