---
topic: citation-currency-fixes
author: claude-opus-4-8
created_at: 2026-07-10T12:23:00Z
---

## Proposal: Fix the two confirmed vocabulary-drift citations (orchestrate -> drive; lane vocabulary owner)

### Target specification files

- SPECIFICATION/spec.md
- SPECIFICATION/contracts.md
- SPECIFICATION/scenarios.md

### Summary

Two pure citation-currency corrections the autonomous-mode Step-0 validation
CONFIRMED as drift (`livespec/plan/autonomous-mode/design.md` §6.3 d/e), plus
the drift-sweep of every co-occurring stale citation of the same fact:

- **(d) `orchestrate` / `orchestrate run` -> `drive`.** The console spec cites
  the orchestrator's published action-id surface as `orchestrate` (three sites)
  and `orchestrate run` (one site). The orchestrator RENAMED that surface to
  `drive`: repo `thewoolleyman/livespec-orchestrator-beads-fabro`,
  `SPECIFICATION/contracts.md` §"`drive`" (line 168, `bin/drive.py`, invoked as
  `drive --repo <path> --action <action-id>`) states "The former `orchestrate
  plan` two-`next` composition and the former bare `orchestrate` interactive
  walkthrough are RETIRED". "orchestrate run" appears nowhere in the live
  orchestrator contracts. All four console-spec references are corrected to
  `drive` so no unamended statement is left citing a retired surface.
- **(e) lane vocabulary owner: `livespec core` -> `livespec-orchestrator-beads-fabro`.**
  The console TUI Contract says "the lane vocabulary is owned by livespec core".
  livespec core defines NO lane vocabulary; the seven lifecycle lanes are the
  orchestrator's states, owned by `livespec-orchestrator-beads-fabro`
  (`SPECIFICATION/contracts.md` §"Work-item state semantics", line 1161). The
  console consumes the orchestrator's emitted `lane` / `lane_reason` and
  never re-derives them, so the owner citation is corrected to the orchestrator.

### Motivation

These are the two CONFIRMED-DRIFT items from the autonomous-mode cross-repo
plan's Step-0 Fable validation (2026-07-10, NO-BLOCKERS), verified first-hand
against the live orchestrator spec before quoting here:

- `git show origin/master:SPECIFICATION/contracts.md` in
  `thewoolleyman/livespec-orchestrator-beads-fabro` shows the published surface
  is `drive` (`#### `drive``, line 168; "the published surface the console
  invokes", line 262; "`drive` additionally accepts the five human valve
  actions", line 235) and explicitly RETIRES the bare `orchestrate` walkthrough
  (lines 185-186). The console's five `work_item.*` commands map 1:1 onto
  `drive`'s action-id grammar (`approve:` / `accept:` / `reject:` /
  `set-admission:` / `set-acceptance:`).
- The lane vocabulary (`backlog`, `pending-approval`, `ready`, `active`,
  `acceptance`, `blocked`, `done`) is the orchestrator's Work-item state
  semantics, not core's; core ships no lane names. Design record: repo
  `thewoolleyman/livespec`, `plan/autonomous-mode/design.md` §6.3 (d/e) and the
  console plan `plan/autonomous-mode/design.md` §4.

No implementation follow-up: the console impl already targets the `drive`
surface for its (not-yet-built) valve wiring (plan step C2), and no lane names
change. These are documentation-currency fixes only.

### Proposed Changes

All quoted current text is verbatim from the live console spec files (head
v016). This is one atomic proposed change (one `## Proposal:` section, one
per-file revise decision under this topic). It changes no `## ` heading, so it
requires no `tests/heading-coverage.json` co-edit.

#### spec.md

`[DRIFT]` **§"Bounded Contexts"**, the `Work-item Lifecycle` bullet -- replace:

> - **Work-item Lifecycle** -- the human-valve commands (approve / accept /
>   reject) and the policy-edit commands (set-admission / set-acceptance),
>   issued through the orchestrator's published `orchestrate` action surface;
>   observes the resulting lane transitions.

with:

> - **Work-item Lifecycle** -- the human-valve commands (approve / accept /
>   reject) and the policy-edit commands (set-admission / set-acceptance),
>   issued through the orchestrator's published `drive` action surface;
>   observes the resulting lane transitions.

#### contracts.md

`[DRIFT]` **§"Command Handling"**, the 1:1 mapping sentence -- replace:

> The five `work_item.*` commands are the Work-item Lifecycle context's
> vocabulary. Each maps 1:1 onto the orchestrator's published `orchestrate run`
> action-id surface, and the console MUST issue them ONLY through that surface --

with:

> The five `work_item.*` commands are the Work-item Lifecycle context's
> vocabulary. Each maps 1:1 onto the orchestrator's published `drive`
> action-id surface, and the console MUST issue them ONLY through that surface --

`[DRIFT]` **§"Command Handling"**, the ratified-contract cross-reference --
replace:

> `thewoolleyman/livespec-orchestrator-beads-fabro`, `SPECIFICATION/contracts.md`,
> its Work-item state semantics section and its `orchestrate` action-id surface).

with:

> `thewoolleyman/livespec-orchestrator-beads-fabro`, `SPECIFICATION/contracts.md`,
> its Work-item state semantics section and its `drive` action-id surface).

`[DRIFT]` **§"TUI Contract"**, the lane-vocabulary owner citation -- replace:

> it (the lane vocabulary is owned by livespec core, referenced here, not

with:

> it (the lane vocabulary is owned by `livespec-orchestrator-beads-fabro`, referenced here, not

#### scenarios.md

`[DRIFT]` **§"Scenario 11 -- Human valve and policy-edit commands map onto the
orchestrator surface"**, the ratified-contract cross-reference -- replace:

> `thewoolleyman/livespec-orchestrator-beads-fabro`, `SPECIFICATION/contracts.md`,
> its Work-item state semantics section and its `orchestrate` action-id surface):

with:

> `thewoolleyman/livespec-orchestrator-beads-fabro`, `SPECIFICATION/contracts.md`,
> its Work-item state semantics section and its `drive` action-id surface):
