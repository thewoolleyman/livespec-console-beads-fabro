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
  2026-07-30, once on an item whose PR had already MERGED.

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

## Ledger

Tracks `-htp`, `-8aw`, `-3lxx7t`, `-9ts`. Blocked by `-dvv` (01). Blocks `-9nb` (04).
