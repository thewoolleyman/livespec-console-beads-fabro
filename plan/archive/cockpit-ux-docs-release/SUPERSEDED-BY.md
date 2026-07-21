# ARCHIVED 2026-07-21 — programme delivered, custody transferred

Do not resume this thread. Do not extend `handoff.md`. It is retained for the
reasoning: the four false greens, the doc-rot case studies, and the two
gate-design lessons are the most reusable material this repo has produced, and
none of it is derivable from the code.

## Why it was archived

Its programme finished. Deliverable #0 (the tmux real-TUI E2E harness) and
**B1–B8** all landed and were live-verified. The maintainer approved archival
on 2026-07-21 with an explicit condition: **fold the doc-custody obligation
into `plan/console-happy-path-mvp/` rather than let it lapse.** That condition
is discharged in that thread's own `## Doc custody` section — if that section
is ever deleted, this archive is the only remaining record that the obligation
exists.

The thread was NOT archivable earlier for exactly that reason. An earlier
revision of its handoff twice claimed the docs work was finished, and was
disproved within hours both times. Archiving is correct now only because the
obligation moved somewhere live, not because it ended.

## What was settled at archival

Four decisions the handoff had marked ask-do-not-act, all answered by the
maintainer:

| Decision | Outcome |
|---|---|
| Make the tmux E2E merge-blocking? | **Yes.** `check-e2e-tmux` added to `ci-green.needs`; §"DELIVERABLE #0" annotated as resolved. |
| Release 0.3.0 (PR #265) | **Doc fix committed onto the release branch** (`0cf0d4a`) — the gate fires on the PR, not after it; see the correction banner in §"RELEASE 0.3.0 IS PENDING". |
| Accept `bamsy3`? | **Yes.** `acceptance -> done` via the human valve. |
| Archive this thread? | **Yes**, folding doc custody into `console-happy-path-mvp` (this file). |

## Where its content went

| Content | Destination |
|---|---|
| Doc custody — the recurring audit obligation | `plan/console-happy-path-mvp/handoff.md` § "Doc custody" |
| Scenario 5 / 11 tmux E2E backfill | `plan/console-happy-path-mvp/` (re-homed; it owns those flows and already reuses B7's fixture) |
| Scenario 9 tmux E2E backfill | Standalone work-item — no thread affinity; NOT plan work |
| B1–B8 deliverables | Shipped; `docs/` tree, `SPECIFICATION/` Scenarios 18–22, the tmux harness |
| `-25rvmd`, `-ble` follow-ups | Already live work-items in the ledger |
| Stage-2 (autonomous-mode MVP acceptance) | **DEAD** — mode retired for good; do not resume |
| Ledger reconciliation (5 stale `pending-approval` + 12 red pin-bump PRs) | **STILL UNFILED and still unowned** — see below |
| Verification discipline, doc-rot case studies, gate-design lessons | Retained here; they are why this file is kept |

## What is NOT discharged

**The ledger reconciliation was never this thread's work and is still nobody's.**
Five `pending-approval` records (W3 `-636m46`, W4 `-j3ts23`, W5 `-2ctzhm`,
W6 `-zmunjo`, W7 `-yvikqp.1`) plus parent epic `-yvikqp` are already delivered
and merged — stale records, NOT admission gates, so do not walk them as valves.
Separately, 12 stacked pin-bump PRs are red on `check-completeness` because the
bump automation rewrites `.livespec.jsonc` `compat.pinned` without refreshing
`tests/fixtures/orchestrator-config-manifest.json`. Archiving this thread does
not resolve either; both need an owner.

## The two lessons worth carrying out of here

1. **A gate that pins a VALUE does not pin the CONDITION under which the value
   applies.** Every doc gate stayed green while every description of a
   broadened predicate went stale. Only reading the source finds this.
2. **Reading one code path and generalizing is how a confident, wrong doc gets
   written — and rendering one screen and generalizing is the same error
   wearing better evidence.** For a claim that splits by case, exercise each
   case.

A third, earned at archival: **a checklist written from a gate's doc comment is
not one written from its assertions.** This thread's own release checklist
enumerated the three claims the comment described and missed the second test in
the same file.
