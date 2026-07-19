# Command-queue semantics — exactly-once consumption in the console command spine

**Epic anchor:** `livespec-console-beads-fabro-irdwyb`

**Supersedes:** `plan/archive/impl-dispatch/SUPERSEDED-BY.md` (split 2026-07-19), which
carries the routing table showing how these items landed here. Do NOT resume the
archived `handoff.md` beside it.

## Charter

Give the console's command queue correct single-consumer (exactly-once) semantics.
Everything downstream — every present and future command handler — flows through this
consumption path, so it is fixed once, here, before the surface widens.

## Read first

1. This file.
2. `crates/console-cli/src/lib.rs` — effect sink :337-345, `handle_pending_factory_commands`
   :1128, `handle_pending_work_item_commands` :1165, `handle_pending_config_commands`
   :1233, `finalize_pending_command` :1431, `distinguish_repeatable_command` :1519-1529.
3. `crates/console-eventstore/src/lib.rs` — commands table :52+, status-update SQL
   :677-678.
4. `SPECIFICATION/contracts.md` §"Command Handling" (:394) — the numbered handler list
   and the `flowchart LR` at :465-484.
5. `SPECIFICATION/non-functional-requirements.md` §"Behavioral Coverage" (:210-220) —
   the clause→scenario→test chain rule the contract rider below depends on.
6. `AGENTS.md` — credential wrapper, mutation protocol.

## Status is read live, never stored here

This handoff stores NO queue and NO per-item status (the no-shadow-ledger rule):

```
/livespec-orchestrator-beads-fabro:list-work-items --json
/livespec-orchestrator-beads-fabro:next --json
```

## The work

### Step 1 (preferred, not blocking) — merge PR #316, which closes `-ble`

Read its live state first — `gh pr view 316` — rather than trusting a status written
here. As of the split it was green and awaiting maintainer review.

It touches exactly one file, `crates/console-cli/src/lib.rs`: one production hunk at
:1506-1524 (the tail of `command_append_from_tui_effect`, `distinguish_repeatable_command`
at :1519-1529, plus a NEW `is_repeatable_command` fn) and ~276 new test lines.

**It is NOT the same region `-ipwtll` edits, and a conflict is not guaranteed.** #316
sits on the APPEND path; `-ipwtll` changes the CONSUME path — `handle_pending_*_commands`
(:1128, :1165, :1233) and `finalize_pending_command` (:1431). Different functions, ≥75
lines apart.

Merge it first anyway: same file, and it closes `-ble`, so sequencing keeps the rebase
trivial. But do not treat that ordering as load-bearing — if the maintainer is slow to
review, `-ipwtll` can proceed and rebase.

On merge, `-ble` closes. No further filing is needed for it.

### Step 2 — `-ipwtll`: the command queue has no single-consumer semantics

Verified GENUINE. `handle_pending_*_commands` (called from the effect sink at :338-344)
carry no claim or lease semantics, so every console client executes every pending
command. Two consoles open against one store double-execute.

Fix direction: an atomic claim on the `commands` table (claim → execute → finalize),
plus stale-`executing` recovery so a crashed consumer does not strand a row forever.

Item sits at `pending-approval` — it needs the admission valve, not more analysis; its
acceptance is already autonomously verifiable.

**Recommended rider:** `contracts.md` §"Command Handling" shows a one-handler sequence
but never states exactly-once consumption or an `executing` status. Since Behavioral
Coverage requires every normative behavior to chain clause→scenario→test, ask the
maintainer whether the new semantics get a one-paragraph contract amendment riding with
the impl, or whether the existing sequence diagram is deemed to imply it.

## Explicitly PARKED — `-8aw` is not in this thread

`-8aw` (the four non-valve initial commands: factory pause/resume, dispatch, spec
doctor) stays `backlog` and unclaimed. Reasons:

- Its four commands ARE still ratified in current `contracts.md:412-415` — the item's
  "per v017" citation is stale (spec is now v032) but its substance stands. Correct the
  citation during grooming; do not act on it.
- It is explicitly ungroomed ("regroom separately before building"; "to be groomed into
  ready slices when a plan step claims them").
- It reaches across `console-domain`, `console-application`, `console-cli` and
  `console-tui`, so building it before the operator-surface contract settles would
  double the surface later needing retrofit — and its four commands are themselves
  operator verbs that the operator-surface redesign may enumerate.

**Do not build `-8aw` until both this thread's claim semantics and the
operator-surface spec amendment have landed.** `backlog` is the correct parking state:
the ranker keys on STATUS, so a backlog item is inert by construction and needs no
artificial blocking.

## Sequencing

- PR #316 → `-ipwtll` → (later, elsewhere) `-8aw`. The first arrow is preference, not
  necessity — different functions in one file (see Step 1).
- Parallel-safe against every other thread. This thread solely owns
  `crates/console-eventstore/src/lib.rs`'s `commands` table; the event-identity thread
  only reads the `events` index — different concern, no collision.
- One shared-file caveat, ALSO recorded in `plan/operator-surface-redesign/`: that
  thread must retire a test at `crates/console-cli/src/lib.rs:2312`. It is in the test-module tail, far from :1519 —
  trivial rebase either way, but retire it AFTER #316 merges.

## Gates

- Maintainer review + merge of PR #316.
- Admission valve on `-ipwtll` (already at `pending-approval`).
- Maintainer ruling on the contract-rider question above.

## Dispatch

Ready items go **factory-side** — the Dispatcher drains `ready`, or run
`/livespec-orchestrator-beads-fabro:drive --action impl:<id>`. Do NOT implement inline
in a planning session.
