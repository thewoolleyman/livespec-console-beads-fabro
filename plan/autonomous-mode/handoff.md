# Autonomous-mode MVP — console plan handoff

**Status:** Step 0 PASSED (2026-07-10, independent Fable validation, NO-BLOCKERS)
and this plan REVISED per its findings (full verdict:
`livespec/plan/autonomous-mode/research/step0-fable-verdict.md`). C1 may start
once the overall plan's Fable certification is recorded. First-drafted
2026-07-10 from a repo survey.

**Repo:** `thewoolleyman/livespec-console-beads-fabro` · **Role:** the Control-Plane
operator TUI surface for autonomous mode (GUI out of scope). Driven from the
delegate session `console-autonomous-mode`.

## Read first
1. This file, then `design.md` here.
2. The overall plan: `livespec/plan/autonomous-mode/design.md`.
3. The orchestrator plan (for the arming contract C3 depends on):
   `livespec-orchestrator-beads-fabro/plan/autonomous-mode/design.md`.
Then derive live status from the ledger (see Pointers) — the ledger is authoritative.

## The one-line state
Spec (v016) fully defines the MVP; implementation has almost none of it
(`CommandType` = only `FactoryDrainRequested`; zero `autonomous` code; no
`.livespec.jsonc`/Configuration reading; TUI has generic modal/palette only). The
lane/valve foundation (archived `work-item-lifecycle-redesign`) is done.

## Steps (design.md §4)
- **C1** execute the Step-0 spec fixes and ratify ONE revision (design.md §4 has
  full text): fix the two confirmed citation drifts ("`orchestrate run`" →
  `drive`; lane vocabulary is orchestrator-owned, not core-owned); re-scope
  Scenario 10 + the blanket resolve-MUST to the delegation model (the engine
  owns gate resolution; the console enables/observes/reflects); resolve the
  `_set` naming; refresh `rt4`'s pointer. The persistence-seam amendment
  additionally waits for the orchestrator's frozen arming contract (I1). Route
  via propose-change → independent Fable review → revise.
- **C2** command foundation: five `work_item.*` valve/policy commands + handlers +
  a port onto the orchestrator's published `drive` action surface; fold `pke3y3`
  (split the four non-valve commands into their own item); extend the read-side
  `AcceptancePolicy` enum; Scenario-11 test.
- **C3** autonomous feature: `config.autonomous_mode_set` + `.livespec.jsonc`
  persistence + audit events + `factory.autonomous_mode_*_requested` + TUI
  toggle/confirm-modal/dangerous-label/header indicator + Scenario-10 loop; fold `rt4`.

Gates: C1 → C2 → C3; C3 also needs the orchestrator arming contract frozen (overall I1).

## NOT this repo's work
The LLM gate-resolution engine is orchestrator item `bd-ib-82a`. The console only
enables/observes/reflects and surfaces the truly-unresolvable (incl. the irreducible
human touchpoints).

## Ledger items
`rt4` (operator surface → C3), `pke3y3` (valve commands → C2, regroom first), `ipi`
(attention-stream migration → C1/C3), `mb64bv` (backlog-bounce rename, active, land
early). `plan/impl-dispatch/` is complete/unrelated — archive separately.

## Next action
Step 0 passed NO-BLOCKERS and this plan carries its findings. Once the overall
plan's Fable certification is recorded
(`livespec/plan/autonomous-mode/handoff.md` names it), start C1: file the
citation fixes + Scenario-10 re-scope as one proposed change (persistence-seam
amendment following I1), independent Fable review, then revise.

## Pointers
- Ledger read: `bd list --json` from inside this repo (its `.beads/config.yaml` →
  database `livespec-console-beads-fabro`). The shared
  `list_work_items.py` cache path can mis-resolve to the wrong tenant — prefer
  `bd list`.
- Discipline: worktree → PR → merge → cleanup; `mise exec -- git …`; never
  `--no-verify`; Rust product changes use Red-Green-Replay; spec H2 changes co-edit
  `tests/heading-coverage.json`; plan docs are `docs(plan):`.
