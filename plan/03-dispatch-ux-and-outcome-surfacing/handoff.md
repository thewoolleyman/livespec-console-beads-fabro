# 03-dispatch-ux-and-outcome-surfacing — charter

**Epic anchor:** `livespec-console-beads-fabro-1df` — status is READ from the ledger.
**Blocked by:** `livespec-console-beads-fabro-dvv` (plan 01) — a LEDGER EDGE.
Opened 2026-08-02.

## Mission

**Dispatching stops being a freeze, and failures stop being silent.**

Four pieces, each with measured evidence already on the ledger:

- **`-htp` — drain OFF the UI thread.** The drain runs INLINE in effect handling. On
  2026-07-30 the cockpit sat frozen for **2h16m** on one drain and was frozen again on
  the next. **This is the worst live usability defect on record for this console.**
  Anchor re-measured 2026-08-02 against master `69ea9d4`:
  `crates/console-application/src/lib.rs:3561`, `let port_outcome =
  port.drain_ready_queue(&request)?;` — a plain synchronous call, no spawn, no channel.
  The parked thread's `:3363` is ~198 lines STALE; re-measure again when this plan opens,
  because that file is the hottest region in the repo.
  **The misleading negative, so it is not rediscovered:** the only `thread::spawn` in
  `console-cli` is `main.rs:207` (`poller_loop`), which is the SOURCE POLLER, not the
  drain. A grep for `thread::spawn` returns exactly one hit and invites the wrong
  conclusion that the drain is already off-thread.
- **Per-item dispatch (`-8aw`, currently PARKED).** Dispatch ONE item, menu-driven.
  Parked when the queue-level palette drain was judged sufficient for an MVP; menu
  primacy plus 01's registry changes that calculus, so re-open it here rather than
  inheriting the parked reasoning.
- **Background outcome / refusal surfacing.** The dispatcher journal is **ALREADY
  INGESTED** by the console's event store — measured: stored events rose 1566 → 1579
  across a drain whose refusal rendered nowhere. So this is a PRESENTATION gap, not a
  plumbing one. Completes `-ectqye` (01 does its action-invocation half).
- **Stranded-active visibility (`-3lxx7t` class).** A run goes terminal while its ledger
  row stays `active`, and nothing distinguishes that from a live run. Measured twice on
  2026-07-30.
  **CORRECTION 2026-08-02 — the second of those two was NOT this defect.** This bullet
  said "once on an item whose PR had already MERGED". That item was `-6hbfq6`, and its
  stale `active` row had a different cause: post-merge bookkeeping, not the claim/launch
  race. Its run had finished and merged; `reconcile-merged` could not complete because the
  post-merge janitor checks out THE MERGE SHA
  (`_dispatcher_engine_janitor.py:171`, `ref = merged.merge_sha`) and the host-coupled
  `check-fork-drift` can never agree with committed pins after a plugin bump. That is
  `-3r6`, and `-6hbfq6` is now reconciled and CLOSED. **`-3lxx7t`'s window is BEFORE
  execution (claimed, not yet launched); `-6hbfq6`'s was AFTER it finished.** Same
  symptom — an `active` row with nothing running — different defect, different fix.
  So this plan inherits ONE measured instance, not two: three ledger rows `active`
  against ONE in-flight run, 2026-07-30T11:52–11:54Z.
  **The presentation goal is unchanged and arguably strengthened by the correction:** if
  the surface distinguished "executing" from "claimed" from "finished but unreconciled",
  both causes would have been visible instead of indistinguishable.
- **`-9ts` — the drain discards the requested budget.** Anchor re-measured 2026-08-02
  against master `69ea9d4`: `crates/console-application/src/lib.rs:1992` binds
  `_request: &FactoryDrainRequest` with a leading underscore (the compiler-visible
  statement that the caller's budget is deliberately unread) and `:1995-1997` pushes the
  `OPERATOR_DRAIN_BUDGET` constant (`= 50`, `:1975`) unconditionally. The parked thread's
  `:1849,1869` are ~130 lines stale. Its sibling at `:1917` DOES thread `request` without
  the underscore, so the surrounding code already has the shape the fix wants.

## Why this handoff is thin, deliberately

This is a **CHARTER**, not a full handoff: mission, scope, milestone acceptance
(including the dogfood leg), dependencies, and the ledger items it owns. Detail is added
when this plan OPENS.

**That unevenness is the anti-yak-shave mechanism, not laziness.** Writing 40 pages of
design for a milestone two steps away is exactly the rabbitholing this numbering exists
to prevent, and it would be written against a registry that does not exist yet — so it
would be wrong as well as premature. Fill this in when you open it, from what 01
actually shipped.

## In scope / out of scope

**In:** threading the drain, per-item dispatch, journal-derived outcome/refusal
surfacing, stranded-active detection and display.

**Out:** the menu system itself (02). **Out: re-plumbing stderr through `SourceProbe`** —
`-ectqye`'s recorded technical guidance is that the diagnostic already lives in drive's
captured `--json` stdout. Out: fixing the orchestrator-side factory-safety matcher
(`-w7d` names it; it is another repo's surface).

## Milestone acceptance

1. A drain leaves the cockpit RESPONSIVE throughout — demonstrated, not asserted.
2. A refused dispatch surfaces its refusal IN THE UI. Use the three refusal paths already
   recorded as the test corpus: `dispatcher-staleness-refused`, human-valve
   `invalid-source-state`, and `host-only-refused`.
3. A stranded `active` row is distinguishable from a live one.
4. Every gate **MUTATION-DEMONSTRATED RED**.

## Dogfood leg

**Dispatch a real item from a menu.** The cockpit stays responsive; the outcome —
success OR refusal — appears in the UI without leaving the cockpit.

## Sequencing note

Decoupled from 02 by design: neither needs the other's output. It runs AFTER 02 under
the one-live-execution-thread rule, not because of a dependency.

## Inherited custody — ACCEPTED 2026-08-02

From `plan/archive/operator-surface-redesign/`, absorbed and archived on the
maintainer's ruling. **This section IS the acceptance**, per the standing rule that
"another plan owns it" is not a handoff until the successor confirms it.

| item | what it asks for | status at transfer |
|---|---|---|
| `-ipi` | migrate the attention render path from `WorkItemSnapshot(Observed)` to the `attention_item.*` stream | backlog, P3 |

**Why this lands on 03.** The `attention_item.*` stream carries `handoff.command` — the
truthful replacement for the fabricated `fabro attach` line. That makes the migration
an OUTCOME-SURFACING change, which is this plan's subject, not a menu change.

**This is the one item no other plan naturally absorbed**, which is precisely why the
archival named it rather than letting it fall between 02 and 04. If this plan later
judges it out of scope, it must be re-homed EXPLICITLY, not dropped.

**Known constraint, carried:** the migration is blocked on reconciling with ratified
Scenario 5, so a propose-change MUST precede the code. That is why it sat in a design
thread rather than a delivery one, and the constraint survives the transfer.

**Cross-tenant bookkeeping: NOTHING IS OWED.** `livespec-yes5` is CLOSED
(maintainer-directed wind-down 2026-07-08) and its close reason explicitly records that
prose-linked carry-overs "PERSIST as standalone backlog items in their own tenants (NOT
lost)", naming `-ipi`. There is no open epic to report back to.

## Implementation mode — RULED 2026-08-02

**Implement IN-SESSION**: worktree → PR → **full gates** → rebase-merge. That is the
default for every slice in this plan.

**Factory dispatch is the exception, not the fallback**: use it only for well-bounded,
sandbox-safe slices, and **record the choice per slice** with the reason. A slice that
touches host-coupled surfaces, plugin resolution, or anything under `.github/workflows/`
is not sandbox-safe — see the known live hazards in
`plan/01-action-registry-and-invoker/handoff.md`.

Recording the mode here rather than leaving it as session convention, because the two
routes have different evidence obligations and a successor cannot infer which was used
from the merge alone.

## Ledger

Tracks `-htp`, `-8aw`, `-3lxx7t`, `-9ts`. Blocked by `-dvv` (01). Blocks `-9nb` (04).
