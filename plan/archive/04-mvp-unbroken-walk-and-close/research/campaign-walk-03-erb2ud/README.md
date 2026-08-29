# Campaign lifecycle #3 — `5zjk5b` — COMPLETE, and the walk that falsified per-item dispatch

Walked 2026-08-20T22:48:09Z onward on master `ebfbfef`. `tmux capture-pane -p`
(plain, no `-e`), in order, never reconstructed. Menus-only: every action was
invoked from a menu, no hotkeys, no palette.

**The whole walk ran on one binary, deliberately.** Master moved to `1cbddf1`
mid-walk when plan 02's `et3.7` merged (its post-merge janitor pulled the primary
checkout under this session), and `et3.7` changes what the detail pane offers —
the same detail pane this walk uses for its arm confirmation. The console was NOT
relaunched: the running process holds the `ebfbfef` build for the walk's whole
length, including the human accept, so every frame in this directory came from
one binary. Plan 02 built its own release binary into an isolated target dir
rather than `target/release/`, specifically so this record could not be swapped
under it. A detail-pane verb list that differs from these captures on a later
walk is `et3.7`, not a regression.

**The directory is named for `erb2ud` and the lifecycle ran on `5zjk5b`.** That
is not an error, and the reason is the finding: this walk set out to be the first
CLEAN campaign lifecycle using the new per-item dispatch verb on `erb2ud`, and
discovered that the verb does not dispatch. The lifecycle was then re-run on the
drain path, which picks by rank, and the rank picked `5zjk5b`.

## What this walk was supposed to be, and why it could not be

The predecessor's handoff (timeline entry 9, plus its addendum) recorded that
`dispatch-selected-item` had landed on master — keyless, menu-only, availability
gated on `Lane::Ready` + `ActionSurface::LaneDrill` — and ruled that walk #3
would therefore need **no ranking consult and no named non-menu step**. That
ruling rested on plan 02's read of `action_registry.rs`, which was accurate.

A registry entry proves the row renders. It proves nothing about whether invoking
it does anything.

## Finding: `Dispatch selected item` is a not-wired stub, presented as available

Walked on `erb2ud`: armed, selected, entered the menu hotkey-free, invoked
`Factory > Dispatch > Dispatch selected item`. It committed with **no
confirmation modal and no read-back**, and dispatched nothing. Sixty seconds
later the item was still `ready`, no dispatcher process existed for this repo,
and the ledger had not moved.

Root cause, read from source (`crates/console-application/src/lib.rs:2447-2456`):
`DispatcherFactoryDispatchItemPort::dispatch_item` returns
`Ok(FactoryDispatchItemPortOutcome::not_wired())` **unconditionally**. Its own
doc comment says why: the specification confirms the
`factory.dispatch_item_requested` COMMAND contract but defines no stable concrete
Dispatcher argv for one-item dispatch, so the port "reports `not_wired` instead
of fabricating success or silently falling back to a fleet drain". R8's carrier
landed the registry entry, the command type, the event vocabulary and the port
TRAIT — everything except a working port.

Command spine and event stream (`09-`, `10-`):

    command  cmd_factory_dispatch_item_requested_..._erb2ud_90   status=not_wired
    2892  factory.drain_requested          stream ...erb2ud  stream_seq 0
    2893  command.accepted                 stream ...erb2ud  stream_seq 1
    2894  factory.dispatch_item.not_wired  stream ...erb2ud  stream_seq 2

Two further defects fall out of that six-line window, and they are not the same
bug as each other or as the stub:

- The request-time event for a PER-ITEM dispatch is written as
  `factory.drain_requested`, on one work-item's own stream. The console records a
  fleet drain that never happened.
- `stream_seq` collides. The set-acceptance command wrote seq 1,2,3 on that same
  stream (`2887`-`2889`); the dispatch command then wrote seq 0,1,2.

**Why the operator sees none of this.** The console *does* know. At 160 columns
the header reads `... | factory: dispatch item not wired | ...` (`08-`). At the
103 columns this walk ran at, the header fitter drops that segment — and
`view: Lanes` — to fit, while keeping the static `repo:` and `attention:`
segments. The one transient, decision-relevant segment is the first thing
sacrificed. Measured both ways rather than assumed: the same live TUI, resized.

### Disposition, agreed with plan 02 rather than filed unilaterally

The not-wired port and the header fitter are plan 02's: R8 is its requirement
carrier and the fitter is menu-shell charter. Plan 02 filed
`livespec-console-beads-fabro-et3.10` (the port, spec-change-tier — there is no
one-item Dispatcher argv in `SPECIFICATION/contracts.md`) and
`livespec-console-beads-fabro-et3.11` (the fitter). A drafted plan-04 item
folding the fitter in as a "contributing factor" was **deleted, not trimmed**, so
it cannot resurface and undercut et3.11.

Plan 02 also filed `livespec-console-beads-fabro-v8un` (stale ready-lane
projection) and `livespec-console-beads-fabro-x6lj` (row-index selection
anchoring, p1), both from this walk's measurements — see below.

Plan 04 filed exactly one: **`livespec-console-beads-fabro-1d5f`** — the
`stream_seq` collision, standalone, `czcjh5` cited as related and deliberately
not folded in. The valuable half is the extent, measured after the initial
finding: `fleet:livespec` carries `stream_seq` 1 thirteen times, 2 and 3 twelve
times each, and **every** work-item stream that took two commands shows the same
1,2,3 / 1,2,3 shape — `4nrwmp` included. So it is systemic and pre-existing, and
must not be booked as R8 fallout.

## The coordinator ruling this walk overturned

The standing ruling before this walk was that per-item dispatch was live, that
every walk after `4nrwmp` merged would use it, and that the ranked-drain
allowance was therefore SPENT. The console foreman has since retracted that
ruling in writing, on the ground that it had been accepted on a verification
which only proved the registry ROW renders:

> THE RANKED-DRAIN PATH IS REINSTATED AS THE ONLY DISPATCH PATH, and its expiry
> is re-tied from `4nrwmp` to `-et3.10` landing. Ranked-drain walks continue to
> COUNT toward the six, on the same conditions as before (name the ranked pick
> explicitly as a non-menu step). Do not attempt per-item dispatch for walk #4 or
> later until `et3.10` lands — it will report `not_wired`, which is honest but is
> not a lifecycle.

So walks #4-#6 use `Dispatch ready work` plus a named ranked-pick consult, and
this walk's shape is the campaign's shape until `et3.10` lands.

## The lifecycle that did run: `5zjk5b`

Re-run on `Factory > Dispatch ready work`, whose port is real and proven.

1. **Named non-menu step — the ranked-pick consult** (`11-`). `next --limit 4
   --json` returned `5zjk5b, ekb5vq, erb2ud, iofvz2`, all `rank a0`. The top pick
   was **not** `erb2ud`. Neither plan re-ranked to force it: gaming the shared
   ranking is the one shortcut this campaign has refused twice.
2. **Arm-first, from the menu, with nothing in flight** (`13-`). Leg B read back
   `Set acceptance work-item` / `Target: livespec-console-beads-fabro-5zjk5b` /
   `Policy/mode: ai-then-human`.
3. **Arm confirmed in-surface** (`14-`): the item detail pane shows
   `acceptance_policy    ai-then-human`, against unarmed controls two rows away
   reading `— (not emitted; console assumes ai-then-human)`. This is a better
   confirmation than the command-spine read earlier walks used for the one verb
   whose screen is byte-identical across the drive call (`-tyeonw`), and it needs
   nothing out-of-band.
4. **Dispatched from the menu** at 23:02:17Z (`15-`). Header: `factory: drain in
   flight`. Journal at 23:02:25Z (`16-`): `loop-pick {budget: 1, picked:
   ["livespec-console-beads-fabro-5zjk5b"]}` — exactly the armed item. Arm-first
   held for the third consecutive time.

5. **Ran ~15 minutes.** `fabro-run` exit 0 at 23:17:44Z. PR **#735** created
   23:16:51Z and merged as **`e073d76`** ("chore: add charter defect gate");
   post-merge janitor green.
6. **THE GATE HELD.** `acceptance-parked` at 23:30:54Z, `acceptance_verdict:
   PASS`, `policy: ai-then-human`; `outcome: merged, post-merge janitor green`.
   Console: `acceptance (1)`, `active (0)` (`18-`).
7. **Accepted at the human valve, from the menu.** Leg A (`21-`): the acceptance
   lane offers `Accept work-item [c]`, `Reject work-item [r]` **and**
   `Move status [s]` — **three** available verbs, with Approve, Driver handoff,
   all five policy dials and the workflow scope override all
   `(unavailable here)`. Walk #2's record says "exactly Accept and Reject"; that
   was imprecise and is corrected here. Then the x6lj guard (`20-`): 5zjk5b's
   detail pane read back `status acceptance` / `lane acceptance` /
   `acceptance_policy ai-then-human` immediately before committing. Then leg B
   (`22-`): `Accept work-item` / `Target: livespec-console-beads-fabro-5zjk5b`,
   read back before `Enter`.
8. **Leg C**: `cmd_work_item_accept_requested_...5zjk5b` = `completed` at
   23:32:38.258Z; ledger `closed` at **23:32:40Z** (`24-`). **Closed by the human
   accept, not autonomously** — it sat parked for 1m44s and moved only on the
   keypress. Console settled to `acceptance (0)`, `done (175)`.

### The accept is not instant, and the screen does not say so

After `Enter` the item still rendered in the acceptance lane for about thirty
seconds, while the command spine already had it `completed`. The projection
caught up a cycle later. An operator reading the unchanged lane as a failed
accept would re-press — which is the `-k0w` family (failed operator commands
surface nothing) seen from the success side. Do not re-press; check the spine or
wait a cycle.

### `-qeqax3` corroborated a third time, and harder

`25-command-spine-final.txt` holds all four commands this walk issued across
44 minutes — two on `erb2ud`, two on `5zjk5b`:

    ..._erb2ud_ai-then-human_89   completed  requested_at 2026-08-20T22:48:29.528869692Z
    ..._erb2ud_90                 not_wired  requested_at 2026-08-20T22:48:29.528869692Z
    ..._5zjk5b_ai-then-human_91   completed  requested_at 2026-08-20T22:48:29.528869692Z
    ..._accept_..._5zjk5b         completed  requested_at 2026-08-20T22:48:29.528869692Z

Four commands, two aggregates, 44 minutes apart, ONE `requested_at` identical to
the nanosecond. `updated_at` does discriminate them. Combined with `-1d5f`
(stream_seq is a per-command ordinal), the store has neither a usable timestamp
nor a usable sequence for intra-stream ordering.

### The re-sort that would have aimed the next action at the wrong item

When the arm landed, the lane re-rendered: `5zjk5b` changed from `rank ~` with
the ranking **reason** rendered as its title, to `rank a0` with its real title,
and moved from row 8 to row 1. The cursor stayed on **row 8**, which was now
`4s1h` — and the next `Enter` opened `4s1h`'s detail pane. Selection is anchored
to a row index, not to a work-item id.

An earlier draft of this record called the `rank ~` / reason-as-title symptom a
fixed rendering defect. That was a true observation of a false steady state and
was corrected to plan 02 **before** it could be built on. Plan 02 confirmed the
mechanism from source — `rank_bottom_sentinel()` returns the literal `"~"` for an
item ingested with NO rank and is documented to sort such rows LAST, so the lane
can display the drain's top pick at the BOTTOM of the list; and the snapshot
build assigns `title: Some(item.summary()...)`, which is how the orchestrator
`next` command's own `reason` string ends up rendered as a work-item title — and
then **re-scoped** `livespec-console-beads-fabro-v8un` accordingly: it is now
*ready-lane rows serve STALE projection values until an unrelated command forces
a refresh*, with a correction comment saying the original rendering-defect
framing is wrong and must not be implemented against. The source findings survive
as a description of what the stale state CONTAINS, not as a permanent mis-mapping
to fix at the adapter.

**The re-sort itself is the more dangerous half, and it is now
`livespec-console-beads-fabro-x6lj` (p1, standalone, filed by plan 02).** Every
other defect in this family MISINFORMS the operator; this one MISDIRECTS THE
ACTION. The operator reads the right item, decides correctly, invokes from a
menu, and the console applies the verb to a different item — because selection is
anchored to a row index rather than to a work-item id. Every per-item verb is
exposed: approve, accept, reject, move status, set admission, set acceptance,
driver handoff, dispatch selected item. It composes badly with this walk's other
measurement that **both** dispatch verbs commit with no confirmation modal and no
read-back, which leaves no moment at which a re-aimed selection becomes visible.
Fixing v8un's freshness would not fix it: a legitimate re-sort still moves the row
out from under the cursor. Interim guidance for anyone walking, recorded on the
item: **verify the target in the detail pane immediately before every commit, not
at selection time.**

## Side observations, logged not chased

- `sources: 2 unavailable (dispatcher, livespec)` — **a candidate explanation was
  raised during this walk and then FALSIFIED by this walk's own next
  measurement; it is recorded here so nobody re-derives it.** The header read
  `2 unavailable (dispatcher, livespec)` before the dispatch and
  `1 unavailable (livespec)` fifteen minutes into the drain, which suggested the
  dispatcher source simply reads unavailable when no dispatch is running. It
  does not: at 23:18Z, with the drain STILL in flight, the header was back to
  `2 unavailable (dispatcher, livespec)`. Both samples are banked in
  `17-sources-during-drain.txt`. So dispatcher-source availability flickers
  DURING a single dispatch, which is a different and more interesting shape than
  either "broken" or "only available while dispatching". Still unexplained, and
  now unexplained with two contradicting samples rather than one. livespec's
  remains the known absent-binary case.
- `sizing-warn` on this run: `description is 2955 chars (> 1500)`.
- `5zjk5b` was refused by the factory-safety gate on 2026-08-17 for declaring a
  `.github/workflows/` edit. Today's `ledger-admit` accepted it, so the item was
  reworded in between.
- The plan record-rate guard fired two warnings for 2026-08-20 and both are
  recorded rather than swallowed: 7 handoff entries by `claude-code-session`
  (threshold 6), and 77 research notes (threshold 6). The handoff count is real
  and reflects the days this arc spent gated on `-et3`. **The research-note count
  is an artifact**: the guard counts working-tree modification times, and this
  session's `git pull` rewrote 78 files today, so every pre-existing note dates
  to 2026-08-20. Do not read 77 as authored-today.

## File index

| file | what it captures |
| --- | --- |
| `00-timestamp.txt` / `00-resting.txt` | walk start, UTC; resting frame with the bar visible |
| `01-ready-drilled.txt` | ready lane drilled, `erb2ud` selected |
| `02-hotkeyfree-menu-entry.txt` | `Left` opens the bar in place, selection preserved (et3.6 holds) |
| `03-legB-arm.txt` / `04-acceptance-armed.txt` | leg B read-back for `erb2ud`; post-confirm frame |
| `05-arm-confirmed-detail.txt` | `acceptance_policy ai-then-human` on `erb2ud` |
| `06-factory-dispatch-menu.txt` | `Dispatch selected item` rendering as AVAILABLE |
| `07-legB-dispatch-item.txt` | the per-item dispatch committing with no modal and no read-back |
| `08-header-at-160-cols.txt` | the header the fitter was hiding: `factory: dispatch item not wired` |
| `09-command-spine.txt` | both `erb2ud` commands; note the identical `requested_at` (`-qeqax3`) |
| `10-event-stream-erb2ud.txt` | the `stream_seq` collision, and `factory.drain_requested` on an item stream |
| `11-ranking-consult.json` | the named non-menu step |
| `12-legA-ready-lane-actions.txt` | leg A — which verbs the ready lane offers |
| `13-legB-arm-5zjk5b.txt` | leg B read-back for `5zjk5b` |
| `14-arm-confirmed-5zjk5b.txt` | `acceptance_policy ai-then-human` on `5zjk5b` |
| `15-dispatch-timestamp.txt` / `15-dispatch-committed.txt` | dispatch committed; `factory: drain in flight` |
| `16-journal-loop-pick.txt` | `loop-pick` picked exactly the armed item |
| `17-sources-during-drain.txt` | the two contradicting `sources:` samples, mid-drain |
| `18-lanes-parked.txt` | parked at acceptance: `acceptance (1)`, `active (0)` |
| `19-acceptance-drilled.txt` | the item selected in the acceptance lane |
| `20-target-verified-pre-commit.txt` | the x6lj guard — target confirmed before commit |
| `21-legA-acceptance-offered.txt` | leg A — three verbs available, not two |
| `22-legB-accept-confirm.txt` | leg B — target read back before `Enter` |
| `23-accept-timestamp.txt` / `23-accepted.txt` | the human accept |
| `24-final-lanes.txt` | `acceptance (0)`, `done (175)` |
| `25-command-spine-final.txt` | all four commands, one `requested_at` |
| `26-journal-full-lifecycle.txt` | dispatch to `outcome`, full stage list |
