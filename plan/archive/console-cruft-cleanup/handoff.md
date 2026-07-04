# Handoff — console-cruft-cleanup

**Epic anchor:** `livespec-console-beads-fabro-nxsfih` (this repo's
tenant). Status is DERIVED from the ledger — run
`/livespec-orchestrator-beads-fabro:list-work-items --json` (under the
family env wrapper, from the repo root) and read the items named below;
this file stores no status and no shadow work queue.

**Thread state:** ARCHIVED — closed out 2026-07-04 (maintainer-verified). The
proposal set was AMENDED 2026-07-04 per an independent read-only verification
(one minor blocker + four advisories), re-verified clean, and RATIFIED
2026-07-04 — all four proposals accepted as one accept-all per-file decision,
cutting `SPECIFICATION/history/v014/` (PR #92, merge commit `df70a7c`). rt4 was
re-statused `open` → `backlog`; the ratification-gate work-item
`livespec-console-beads-fabro-iblkzp` is CLOSED (resolution: completed). Every
maintainer gate below is executed or resolved; no maintainer gate remains open.
The remaining work is normal factory-dispatchable impl tracked in the ledger.
This thread now lives under `plan/archive/console-cruft-cleanup/`.

## What this thread is

The maintainer-directed (2026-07-04) comprehensive cleanup of obsolete
or wrong cruft in `livespec-console-beads-fabro` — the console spec was
authored before the work-item-state-machine design session (2026-06-27)
and was only partially re-derived against it. The audit, its
classifications, and its anti-findings are in
`research/audit-findings.md`.

## Read-first chain

1. `research/audit-findings.md` (this thread) — the full audit record.
2. `SPECIFICATION/history/v014/proposed_changes/console-cruft-cleanup.md`
   (this repo) — the four ratified proposals, with their paired
   `console-cruft-cleanup-revision.md` (decision: accept).
3. The design of record: repo `thewoolleyman/livespec`,
   `plan/archive/work-item-state-machine/research/02-design.md` (§3,
   §7, §8) and `03-decision-log.md` (decisions 3, 15, 16, 17, 22–32).
4. The upstream anchor, now RATIFIED: repo
   `thewoolleyman/livespec-orchestrator-beads-fabro`, ratified as that
   repo's `SPECIFICATION/history/v029/` (topic
   `approval-is-the-pending-approval-to-ready-transition`); the live
   `SPECIFICATION/contracts.md` "Work-item state semantics" section and
   the `orchestrate` action-id surface. This thread's proposals cite it
   in resolved (non-pending) prose form.

## Gates — all executed or resolved

1. **Re-verification + ratification of the proposal set** — EXECUTED
   2026-07-04. Independent re-verification of the amended
   `console-cruft-cleanup.md` returned NO BLOCKERS; `/livespec:revise`
   accepted all four proposals in one accept-all per-file decision,
   cutting `SPECIFICATION/history/v014/`. Post-step doctor static: green
   (19 pass, 2 skip, 0 fail). Co-edits landed in the same PR:
   `tests/heading-coverage.json` gained the Scenario 11 entry
   (`test="TODO"` + a reason naming its test tier — the literal TODO rides
   through this repo's warn-default behavioral-coverage gate), and
   `crates/console-spec-check/src/tests.rs` ground-truth clause counts
   moved in lockstep (contracts.md 32→36; total 116→120). Gate work-item
   `livespec-console-beads-fabro-iblkzp` (the ratification gate) is CLOSED
   (resolution: completed, 2026-07-04) — its acceptance (a new
   `history/vNNN/` recording the four dispositions + `proposed_changes/`
   consumed) is met by v014.
2. **The dependency-linked code rename** — work-item
   `livespec-console-beads-fabro-mb64bv` (`DispatcherJournalKind::NeedsRegroom`
   / `dispatch.needs_regroom_observed` → backlog-bounce vocabulary). Its
   `depends_on` edge → `iblkzp` cleared when `iblkzp` closed (2026-07-04); it
   now rests at `pending-approval`, awaiting the maintainer's explicit approve
   through the operator surface. NOT a maintainer gate.
3. **Upstream pending proposal** — RESOLVED. The orchestrator proposal
   `approval-is-the-pending-approval-to-ready-transition` ratified
   2026-07-04 as that repo's `SPECIFICATION/history/v029/`, so proposal 3
   and proposal 4's pending-anchor hedges were dropped at ratification and
   cite the ratified sections directly. No longer pending.
4. **Legacy `open` ledger item** — DISPOSITIONED.
   `livespec-console-beads-fabro-rt4` ("Implement full autonomous mode
   (operator surface)") was re-statused from the retired legacy `open`
   status to `backlog` (the 7-state lifecycle decomposition state) per the
   maintainer decision recorded by the overseer 2026-07-04, with a
   journaled comment naming the actor (claude-opus-4-8, this session) and
   the decision. The maintainer classified it epic-shaped future work.

## Already merged / no gate

- Docs-only cruft fixes (README arch-check staleness + gate list +
  retired `host-only` marker; the banned "DoR" acronym in
  `plan/impl-dispatch/handoff.md`) — PR #88.
- The ratification — PR #92 (`SPECIFICATION/history/v014/`, merge `df70a7c`).
- The handoff update recording the ratification — PR #93.

## Impl work remaining (ledger-tracked, no maintainer gate)

Authorized-at-ratification slices, now unblocked, that proceed through the
normal factory/admission path (none is a planning-thread gate):

- `mb64bv` — the dispatcher-journal vocabulary rename (needs-regroom →
  backlog-bounce), dependency-linked behind the now-executed gate.
- The arch-check zero-Beads-knowledge rule (NFR Architecture Tests now
  enumerate it; `console-arch-check` gains the check).
- The five `work_item.*` valve/policy commands (domain `CommandType`
  variants + handlers + orchestrator port) recorded in proposal 3's
  impl-impact note; the Scenario 11 top-of-pyramid test lands with this
  slice, replacing the `TODO` in `tests/heading-coverage.json`.

## Close-out (final)

CLOSED OUT 2026-07-04, maintainer-verified. Definition-of-Ready for archive,
all met: v014 present + doctor green; rt4 dispositioned (`open` → `backlog`);
the ratification-gate work-item `iblkzp` CLOSED (resolution: completed); no
open maintainer gate. PR #92 ratified the spec (merge `df70a7c`); PR #93
recorded the ratification in this handoff; this PR moves the thread to
`plan/archive/console-cruft-cleanup/`. The remaining impl slices (`mb64bv`
rename — now `pending-approval`; the arch-check zero-Beads rule; the
valve/policy command slice) are ordinary ledger work, tracked independently of
this archived planning thread.
