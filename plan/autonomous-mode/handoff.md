# Autonomous-mode MVP — console plan handoff

**Status:** DRAFT — awaiting the overall Step-0 multi-model (Fable) validation pass
before implementation. First-drafted 2026-07-10 from a repo survey.

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
- **C1** spec currency + reconcile the three seams (persistence model, division of
  resolution, vocab drift) + the `_set` naming + refresh `rt4`'s version pointer.
  Route real changes via propose-change → Fable review → revise.
- **C2** command foundation: five `work_item.*` valve/policy commands + handlers +
  orchestrator `orchestrate run` port; fold `pke3y3`; Scenario-11 test.
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
After overall Step 0 passes NO-BLOCKERS, start C1: diff the borrowed vocab vs
current core/orchestrator and reconcile the seams; file any spec change via
propose-change with an independent Fable review before revise.

## Pointers
- Ledger read: `bd list --json` from inside this repo (its `.beads/config.yaml` →
  database `livespec-console-beads-fabro`). The shared
  `list_work_items.py` cache path can mis-resolve to the wrong tenant — prefer
  `bd list`.
- Discipline: worktree → PR → merge → cleanup; `mise exec -- git …`; never
  `--no-verify`; Rust product changes use Red-Green-Replay; spec H2 changes co-edit
  `tests/heading-coverage.json`; plan docs are `docs(plan):`.
