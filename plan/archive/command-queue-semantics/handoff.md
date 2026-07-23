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
2. `crates/console-cli/src/lib.rs` — `handle_pending_factory_commands` :1131,
   `handle_pending_work_item_commands` :1168, `handle_pending_config_commands` :1236,
   `finalize_pending_command` :1492, `is_repeatable_command` :1602,
   `distinguish_repeatable_command` :1620 (anchors refreshed 2026-07-23 on master).
3. `crates/console-eventstore/src/lib.rs` — commands table :52+, status-update SQL
   :678. Terminal command statuses are `completed` / `failed` / `rejected` /
   `not_wired` — NOT "succeeded"; the flowchart's "success event" node misleads.
4. `SPECIFICATION/contracts.md` §"Command Handling" (:394) — the numbered handler list
   and the `flowchart LR` at :465+.
5. `SPECIFICATION/non-functional-requirements.md` §"Behavioral Coverage" (:210-220) —
   the clause→scenario→test chain rule the contract rider below depends on.
6. `SPECIFICATION/proposed_changes/command-queue-exactly-once-consumption.md` — the
   FILED contract rider (pending; see Step 2).
7. `AGENTS.md` — credential wrapper, mutation protocol.

## Status is read live, never stored here

This handoff stores no work queue. Where it does name a status inline it is CONTEXT for
a rationale, not a value to trust — every one may have changed since it was written:

```
/livespec-orchestrator-beads-fabro:list-work-items --json
/livespec-orchestrator-beads-fabro:next --json
```

CAUTION (learned 2026-07-23): "read live" means the credential-wrapped ledger CLIs
above, run so they see current state — a `list-work-items` read through a stale,
behind-origin primary checkout reported `-ipwtll` at `pending-approval` when the live
ledger had it at `ready`. When the primary checkout is behind or dirty, trust `drive`'s
own source-state errors over a local listing.

## All agent work is DONE (2026-07-23, second pass); four maintainer acts remain

1. Review/merge PR #399 — the `-ipwtll` implementation (see Step 2's dispatch
   record). Green, CLEAN, no auto-merge armed.
2. Review/merge PR #316 — closes `-ble`.
   **Order is free: a combined-tree certification (`git merge-tree` of the two
   heads, then lib tests + coverage gate on the merged tree) passed 130/130 with
   the gate clean — the branches compose with ZERO conflicts in either order.**
3. Next `/livespec:revise` — disposition the pending rider proposal, which #399
   APPLIES verbatim (accept-as-applied; see Step 2).
4. `fabro attach 01KY6HC0CJ` → answer `[A]` — release the parked factory
   container (non-gating, no rush).

(Read status live; do not trust this paragraph.)

The numbered steps below are the ORDER OF EVENTS, not a to-do list for the reader.

### Step 1 (preferred, not blocking) — merge PR #316, which closes `-ble`

Read its live state first — `gh pr view 316` — rather than trusting a status written
here. As of the split it was green and awaiting maintainer review.

It touches exactly one file, `crates/console-cli/src/lib.rs`: one production hunk at
:1506-1523 (the tail of `command_append_from_tui_effect`, `distinguish_repeatable_command`
at :1519-1530, plus a NEW `is_repeatable_command` fn) and three test hunks (two are zero-net; ~254 net-new
lines; the `+3381,276` in the diff header is the new-side span, not an added-line count).

**It is NOT the same region `-ipwtll` edits, and a conflict is not guaranteed.** #316
sits on the APPEND path; `-ipwtll` changes the CONSUME path — `handle_pending_*_commands`
(:1131, :1168, :1236) and `finalize_pending_command` (:1492). Different functions.

**Rebased 2026-07-23.** The predicted-trivial rebase turned SEMANTIC: master's
`4241fc3` (maintainer-authored, Jul 20) independently grew its own
`is_repeatable_command` (move + factory drain, with the payload-less `PersistCommand`
arm now calling `distinguish_repeatable_command` too), while #316 widened the
payload-carrying set and claimed drain "never reaches this function" — no longer true.
Resolved as the UNION: all ten `CommandType`s minus the two once-per-item valves
(approve/accept), master's drain semantics and tests preserved verbatim. Head is
`2e1fb83`; all checks green post-rebase; only maintainer review is missing.

On merge, `-ble` closes. No further filing is needed for it.

### Step 2 — `-ipwtll`: the command queue has no single-consumer semantics

Verified GENUINE. `handle_pending_*_commands` (called from the effect sink at :338-344)
carry no claim or lease semantics, so every console client executes every pending
command. Two consoles open against one store double-execute.

Fix direction: an atomic claim on the `commands` table (claim → execute → finalize),
plus stale-`executing` recovery so a crashed consumer does not strand a row forever.
Terminal statuses are `completed`/`failed`/`rejected`/`not_wired` (NOT "succeeded").

Valve CLEARED 2026-07-23 — item read `ready` from the live ledger; it is
Dispatcher-drainable now.

**Rider RESOLVED and FILED (2026-07-23):** the maintainer ruled "amend, riding with
the impl". The proposal is pending in-tree at
`SPECIFICATION/proposed_changes/command-queue-exactly-once-consumption.md` (landed via
PR #393 + a doctor-driven amendment). It adds the single-consumer subsection to
§"Command Handling" (atomic `pending -> executing` claim, terminal finalize including
`not_wired`, stale-claim recovery with a normative NO-LIVE-STEAL invariant), extends
the section flowchart, and adds `scenarios.md` Scenario 24. BINDING CONSTRAINT the
proposal itself states: the revise pass MUST accept it atomically with (a)
`tests/heading-coverage.json` entries linking the new clauses' gap-ids to Scenario 24
and (b) `-ipwtll`'s top-of-pyramid test — accept earlier and the behavioral-coverage
gate breaks. Practically: run the revise acceptance as part of (or immediately behind)
the `-ipwtll` implementation PR.

**Gate tolerance, verified in `console-spec-check` source (2026-07-23):** the
coverage gate walks LIVE files only — orphan registry entries (gap-ids matching no
live clause) and registry rows for not-yet-live scenarios are silently ignored
(`evaluate` + `missing_tests`, `crates/console-spec-check/src/lib.rs:445-520`). So
impl-first is gate-green; rider-first breaks. One trap: gap-ids hash the clause's
EXACT line text (line-wrap-sensitive), so registry entries must be computed from the
amendment's post-application wrapping — same-PR landing is safest.

**DISPATCHED, FAILED AT PUBLISH, RESCUED — now PR #399 (2026-07-23):** the factory
run (`01KY6HC0CJNYM7V5DV6PZGJ2T4`) implemented everything and went locally green
in-sandbox — including applying the pending rider proposal VERBATIM to
`contracts.md`/`scenarios.md`, writing the `heading-coverage.json` entries, and a
janitor-cut `SPECIFICATION/history/v035/` out-of-band revision record. Its publish
push was rejected by a factory INFRA defect, not code: the engine's pre-clone push of
the source checkout was hook-refused, it silently fell back to a synthetic snapshot
base (exists nowhere on origin), and the disjoint-history push tripped GitHub's
workflows-scope wall on the first `.github/workflows/*` file. Filed as
`bd-ib-pums` (P2) in the orchestrator tenant. The work product was replayed from
`fabro dump` stage artifacts (implement + janitor + review_fix diffs) onto real
master — clean apply, 123/123 tests, coverage gate clean — and published as PR #399
with full provenance. The run container is parked `human_input_required`; answer
`[A]` (abandon) after #399 merges. Consequence for the revise pass: the pending
proposal is now APPLIED by #399 — disposition it accept-as-applied, not re-apply.

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

- Maintainer review + merge of PR #316 — gates `-ble`'s CLOSURE only. It does NOT gate
  `-ipwtll`, which can proceed and rebase (see Step 1).
- ~~Admission valve on `-ipwtll`~~ — CLEARED 2026-07-23 (item `ready`).
- ~~Maintainer ruling on the contract-rider question~~ — RESOLVED 2026-07-23: amend,
  riding with the impl; rider filed and pending in-tree (Step 2).
- NEW: revise-pass accept/reject of the filed rider — maintainer act, exercised
  atomically with the `-ipwtll` impl's coverage entries and test (Step 2's binding
  constraint).

## Dispatch

Ready items go **factory-side** — the Dispatcher drains `ready`, or run
`/livespec-orchestrator-beads-fabro:drive --action impl:<id>`. Do NOT implement inline
in a planning session.
