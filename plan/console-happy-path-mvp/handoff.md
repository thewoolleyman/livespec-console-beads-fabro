# console-happy-path-mvp — handoff

**Epic anchor:** `livespec-console-beads-fabro-b3k5hi` — status is READ from
the ledger (`list-work-items` / `next`), never stored here.
Opened 2026-07-20 (session `exploratory-test-tui`).

## Mission

Make the console usable as an MVP operator cockpit: an **existing filed
backlog work-item** is taken — every keystroke in the TUI — through

> groom (via LLM-driver handoff) → slices admitted at the approve valve →
> ready → dispatched (palette drain) → active/monitored → acceptance →
> accept → done.

Impl-side lanes only. **Out of scope:** spec-side lifecycle actions in the
walked path (propose-change etc.), autonomous mode (retired for good —
dispatcher drains by default), and multi-repo coverage (B7's two-repo doc
acceptance stays with `plan/cockpit-ux-docs-release/`).

This requirement predates this thread and was never delivered because it
fractured across three re-scopes and ended custody-less — the full trace,
with citations, is `research/why-it-never-happened.md`. This thread is the
missing **delivery/integration owner**.

## Read-first chain

1. `plan/console-happy-path-mvp/research/why-it-never-happened.md` — why
   every predecessor stopped short; the fracture map.
2. `plan/console-happy-path-mvp/research/happy-path-gap-analysis.md` —
   leg-by-leg live-verified status of the happy path, the binding
   constraints (locked core contract), and the custody map.
3. `plan/operator-surface-redesign/handoff.md` — the design thread this one
   consumes: maintainer-brainstorm entry gate, "no impl items until
   ratification", cross-repo verb-vocabulary sequencing.
4. `plan/archive/work-item-lifecycle-redesign/research/locked-core-contract.md`
   — the invariants every slice must obey (zero Beads knowledge; commands
   only through the orchestrator surface; lane consumed never re-derived;
   attention as pure derivation; no console→driver dependency).

## Status composition (no shadow queue)

Compose live status from the `list-work-items` operation. The epic's edge
set IS the tracked set:

- **blocks** (critical-path gate): `-6msemd` (operator-surface-redesign
  design ratification).
- **tracks** (collected pieces, custody unchanged): `-zweohm` (groom /
  state-valid verbs), `-l4p3ce` (LLM handoff MVP), `-vc7lmq`
  (valid-commands detail), `-qwjfsw` (bogus attach), `-7rcps4` (modal
  paging), `-276inb` (attention record modal).
- **parent-child**: `-sreeqc` (lane rows show no title).

Deliberately NOT tied: `-irdwyb`/`-ipwtll` (exactly-once command spine —
multi-client hardening, parallel, not needed for a single-operator MVP);
`-6hbfq6` (help-overlay navigation — nice-to-have, off the happy path);
`-8aw` (per-item dispatch commands — the queue-level palette drain
suffices for MVP; stays PARKED per `plan/command-queue-semantics/`).

## The track

**Stage 0 — truthfulness/usability, no design gate.** `-7rcps4`, `-276inb`,
`-sreeqc` sit DoR-passed at `pending-approval`; `-qwjfsw` (custody
`-6msemd`, but factory-safe as scoped) needs admission from `backlog`.
Operator admits at the TUI valve (`p`); implementation is **factory-side**:
the Dispatcher drains admitted `ready` items (or `drive` with
`impl:<id>`) — never in-session.

**Stage 1 — the minimal-verb brainstorm (critical path).** Satisfy
`plan/operator-surface-redesign/`'s maintainer entry gate with a
happy-path-minimal agenda: (a) groom-verb exposure on `backlog` /
regroom-flagged items; (b) the `-l4p3ce` handoff MVP (prompt written to a
tmp file; short copy-paste-safe driver command; full-width render + Copy);
(c) state-valid verb filtering for exactly the happy-path lanes. Anything
beyond that minimal subset stays in that thread's own backlog. Output: that
thread's ratified spec-amendment set — authored there, not here.

**Stage 2 — impl slices.** Filed only AFTER Stage-1 ratification (that
thread's hard rule), under whichever epic the brainstorm rules
custodially correct, and dispatched via the factory path (Dispatcher
drain / `drive` `impl:<id>`).

**Stage 3 — validation.** The MVP acceptance: a single-repo, key-by-key
happy-path walk of a dummy work-item through the REAL TUI in tmux —
operationally a one-repo dry run of cockpit deliverable B7. Coordinate
with `plan/cockpit-ux-docs-release/` (B7 keeps the walkthrough DOC and
two-repo acceptance; this thread only needs the walk to succeed once).
When the walk passes, this epic closes; Stage-2/two-repo remain cockpit's.

## Next action

Open this handoff. Then: in the console TUI, admit the Stage-0 items at
the approve valve (`p`): `-7rcps4`, `-276inb`, `-sreeqc`, and route
`-qwjfsw` to admission; let the Dispatcher drain them (factory path — do
NOT implement in-session). In parallel, schedule the Stage-1 maintainer
brainstorm by resuming the `plan` operation on `operator-surface-redesign`
with this handoff's Stage-1 agenda as the brainstorm input.
