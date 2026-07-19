# SUPERSEDED 2026-07-19 — this thread was split

Do not resume this thread. Do not extend `handoff.md`. It is retained for history and
for the per-item evidence anchors that had not yet been relocated onto their items.

## Why it was retired

The maintainer's assessment was "coupled, non-cohesive, and off track". That has a
mechanical diagnosis, not merely a stylistic one:

1. **It violated the no-shadow-ledger rule.** The orchestrator's `plan` contract states
   that status is derived, never stored, and that a handoff "never embeds a parallel
   `[ ]`/`[x]` work queue that shadows the ledger". This thread's `## THE QUEUE` section
   was exactly such a stored snapshot — which is why the file itself admitted its
   "item inventory goes stale within the hour". It was structurally guaranteed to.

2. **It had no epic anchor and no single topic.** A plan thread binds to one ledger
   epic. This one bound to nothing and accreted the work-products of five different
   execution vehicles: design-tier problem statements, freeform defects, CI gotchas, a
   cross-tenant P1 writeup, and release-ordering facts owned by another thread.

3. **It was a dispatch-queue frame holding items the ranker can never surface.** It held
   three SPEC-CHANGE-TIER, NEEDS-BRAINSTORM problem statements while `next.py` keys on
   STATUS and never returns `backlog`. The result was a permanently zero-ready queue
   that read as paralysis. This is the retired-`0ak` "wrong vehicle" failure recurring
   at thread granularity.

## Where its content went

| Content | Destination |
|---|---|
| `-ag0`, `-25rvmd` event-identity findings | `plan/event-identity-integrity/` |
| `-ipwtll`, `-ble`/PR #316, `-8aw` parking | `plan/command-queue-semantics/` |
| `-zweohm`, `-l4p3ce`, `-vc7lmq` redesign, `-ipi` | `plan/operator-surface-redesign/` |
| `-txtzn5`, `-topr34` | `plan/test-adequacy-gates/` |
| `-mvu22t`, `-mcj`, `-nxsfih` slice 3 | `plan/repo-invariant-guards/` |
| B8 release ordering, backfill staleness, Scenario 9 re-scope, B7 | `plan/cockpit-ux-docs-release/` (via a follow-up PR after #301 merges) |
| Pin-train paragraph | Already owned by `livespec:plan/fleet-pin-propagation/` — was duplication here |
| `bd-ib-lmi5` writeup | Already fully recorded on the item in the ORCHESTRATOR tenant — was duplication here |
| `-nxsfih` slice-3 design | Already a comment on the item — was duplication here |
| Per-item sweep verdicts + anchors | Relocated onto each item as a comment during grooming |
| `gh` 2.46.0 gotchas, wrapper requirement, mutation protocol | Already in `AGENTS.md` — was duplication here |
| `just check` excludes `check-e2e-tmux` | Already in `justfile:43-53` — was duplication here |
| `bd list` 50-row truncation | `livespec/.ai/beads-gaps-workarounds.md` |
| 1Password quota batching discipline; check-delivered-before-dispatch; cite-evidence-per-AC-clause; blocked-marker honesty | `livespec/.ai/agent-disciplines.md` |
| Never hard-code the plugin version in a plan | `.ai/livespec-plugin-currency.md` |
| Queue snapshots, closed-item lists, session-conduct narration | Deleted — transient by nature |

`live-adversarial-review-prompt.md` is entirely stale: its behavioral-coverage chain
(`idgql3→qvrwag→cvqcnx→cc3nlr→77t6mk→rrr4i4`) is long closed and none of those ids are
open. If a standing adversarial-verification lane is wanted, charter it deliberately as
a new thread with a fresh prompt rather than inheriting this one by inertia.

## One thing worth preserving from it

Its adversarial function had real value — in one pass it found seven phantom records, a
P1 bug, a missing guard, and a false alarm it raised and then withdrew. The successor
threads inherit the findings but not the standing lane. **That lane is a deliberate
decision for the maintainer, not something to resume by default.**
