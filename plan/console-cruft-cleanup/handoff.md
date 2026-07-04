# Handoff — console-cruft-cleanup

**Epic anchor:** `livespec-console-beads-fabro-nxsfih` (this repo's
tenant). Status is DERIVED from the ledger — run
`/livespec-orchestrator-beads-fabro:list-work-items --json` (under the
family env wrapper, from the repo root) and read the items named below;
this file stores no status and no shadow work queue.

**Thread state:** filing complete; every remaining step is behind the
maintainer ratification gate below. A thread with open maintainer gates
is NEVER archived — this thread stays under `plan/` until the gates are
executed.

## What this thread is

The maintainer-directed (2026-07-04) comprehensive cleanup of obsolete
or wrong cruft in `livespec-console-beads-fabro` — the console spec was
authored before the work-item-state-machine design session (2026-06-27)
and was only partially re-derived against it. The audit, its
classifications, and its anti-findings are in
`research/audit-findings.md` (read it first).

## Read-first chain

1. `research/audit-findings.md` (this thread) — the full audit record:
   what was found, what was verified correct, where each finding went.
2. `SPECIFICATION/proposed_changes/console-cruft-cleanup.md` (this
   repo) — the four filed proposals awaiting ratification.
3. The design of record: repo `thewoolleyman/livespec`,
   `plan/archive/work-item-state-machine/research/02-design.md` (§3,
   §7, §8) and `03-decision-log.md` (decisions 3, 15, 16, 17, 22–32).
4. The pending upstream anchor: repo
   `thewoolleyman/livespec-orchestrator-beads-fabro`,
   `SPECIFICATION/proposed_changes/approval-is-the-pending-approval-to-ready-transition.md`
   — proposal 3 of this thread's set cites it as PENDING (approve
   semantics + the `set-admission:`/`set-acceptance:` action ids).

## Open gates (all maintainer-owned)

1. **Ratification of the proposal set** — work-item
   `livespec-console-beads-fabro-iblkzp` (blocked: needs-human, in
   Attention). Run `/livespec:revise` in THIS repo over
   `SPECIFICATION/proposed_changes/console-cruft-cleanup.md` (four
   independently accept/rejectable proposals; recommendation in the
   work-item: accept all four). Co-edit at ratification: proposal 3
   adds one `scenarios.md` H2, so the revise pass lands the matching
   `tests/heading-coverage.json` entry (`test` MAY be `"TODO"` with a
   reason).
2. **The dependency-linked code rename** — work-item
   `livespec-console-beads-fabro-mb64bv` (`pending-approval`,
   `depends_on` → `iblkzp`, so it renders blocked:dependency until the
   gate closes). Factory-dispatchable after ratification; no manual
   step beyond the normal admission path.
3. **Upstream pending proposal** (context, owned by the orchestrator
   repo, not this thread): if
   `approval-is-the-pending-approval-to-ready-transition` ratifies in a
   different form or is rejected, proposal 3's mapping paragraph tracks
   whatever replaces it — revisit wording at ratification time.
4. **Surfaced, not acted on:** one legacy `open`-status ledger item
   remains in this repo's tenant (`livespec-console-beads-fabro-rt4`,
   "Implement full autonomous mode (operator surface)") — a leftover
   from the fleet 7-state remediation, for the maintainer to
   re-status or close; this track did not silently rewrite it.

## Already merged / no gate

- Docs-only cruft fixes (README arch-check staleness + gate list +
  retired `host-only` marker; the banned "DoR" acronym in
  `plan/impl-dispatch/handoff.md`) — PR #88 in this repo.

## Next action (single path)

Execute gate 1: run `/livespec:revise` in this repo and disposition the
four proposals of
`SPECIFICATION/proposed_changes/console-cruft-cleanup.md`. Everything
else (the `mb64bv` rename slice, the arch-check zero-Beads rule, the
valve-command implementation slice recorded in proposal 3's impl-impact
note) unblocks or is filed from that pass.
