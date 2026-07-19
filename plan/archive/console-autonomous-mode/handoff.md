# Autonomous-mode MVP — console plan handoff

> **CLOSED + ARCHIVED 2026-07-19.** This thread is finished; nothing below is a
> live instruction. See §"Closing record" at the bottom for what happened to each
> step and where the surviving work went. Everything from `## Read first` down to
> `## Pointers` is preserved AS WRITTEN on 2026-07-10 and is **stale by design** —
> read it as history, not as state.

**Status (historical, 2026-07-10):** the overall plan's fable-review LOOP is OPEN — C1 MUST NOT start
until the loop exits: a FRESH Fable session review finds nothing blocking AND
the MAINTAINER certifies. The AUTHORITATIVE loop state (rounds run, fixes
landed, certification) lives in `livespec/plan/autonomous-mode/handoff.md`;
per-round records accumulate there under `research/` (round 1, 2026-07-10:
Step-0 validation NO-BLOCKERS, then this plan REVISED per its findings —
`research/step0-fable-verdict.md`).
First-drafted 2026-07-10 from a repo survey.

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

Gates: C1 → C2 → C3 — where C2 needs only C1's MAIN ratification (the
citation-drift + Scenario-10 + naming revision); the I1-gated persistence-seam
amendment gates C3, not C2, so C2 runs concurrently with orchestrator O1→O2.
C3 also needs the orchestrator arming contract frozen (overall I1).

## NOT this repo's work
The LLM gate-resolution engine is orchestrator item `bd-ib-82a`. The console only
enables/observes/reflects and surfaces the truly-unresolvable (incl. the irreducible
human touchpoints).

## Ledger items
`rt4` (operator surface → C3), `pke3y3` (valve commands → C2, regroom first), `ipi`
(attention-stream migration → C1/C3), `mb64bv` (backlog-bounce rename, active, land
early). `plan/impl-dispatch/` is complete/unrelated — archive separately.

## Next action
WAIT for the overall plan's fable-review loop to exit — a FRESH Fable session
finds nothing blocking AND the maintainer certifies
(`livespec/plan/autonomous-mode/handoff.md` records the loop state). Only
then start C1: file the citation fixes + Scenario-10 re-scope as one proposed
change (persistence-seam amendment following I1), independent Fable review,
then revise.

## Pointers
- Ledger read: `bd list --json` from inside this repo (its `.beads/config.yaml` →
  database `livespec-console-beads-fabro`). The shared
  `list_work_items.py` cache path can mis-resolve to the wrong tenant — prefer
  `bd list`.
- Discipline: worktree → PR → merge → cleanup; `mise exec -- git …`; never
  `--no-verify`; Rust product changes use Red-Green-Replay; spec H2 changes co-edit
  `tests/heading-coverage.json`; plan docs are `docs(plan):`.

## Closing record (2026-07-19)

The thread closed for TWO independent reasons: its three steps all landed, and
the feature it is named for was retired underneath it.

**Per-step disposition** (derived from the ledger + `SPECIFICATION/` at the time
of closing, not from this document's stale body):

| Step | Disposition |
|---|---|
| C1 — Step-0 spec fixes, one ratified revision | LANDED. Spec advanced v016 → **v028**; the `orchestrate run` citation drift is gone from `SPECIFICATION/`. |
| C2 — command foundation (`pke3y3`) | **closed** on the ledger. |
| C3 — autonomous feature (`rt4`) | **closed** on the ledger. |

**Why the feature itself is gone.** Orchestrator step O2 retired Full autonomous
mode — the dispatcher now DRAINS BY DEFAULT — and the console spec re-baselined
around that. The old autonomous-mode Scenarios 9/10/11 are now dispatcher-
*policy-settings* scenarios, and Scenario 16 is `Factory drain passes the
Dispatcher no policy-arming argument`. There is no `autonomous_mode_set` command,
no `.livespec.jsonc` arming toggle, and no header arming indicator anywhere in
v028 — C3's operator surface was superseded rather than shipped as designed.

**Where the work went.** The console's remaining MVP body moved to
`plan/cockpit-ux-docs-release/` (deliverable #0 + B1–B5 DONE; B6/B7 docs, the B8
release capstone remainder, the real-TUI E2E backfill, and maintainer-gated
Stage-2 remain). Autonomous-mode Stage-2 acceptance is tracked overall at
`livespec/plan/autonomous-mode/handoff.md`.

**Loose ends that outlive this thread** — both continue as plain ledger items, no
thread needed:
- `livespec-console-beads-fabro-ipi` (backlog) — TUI needs-attention render path,
  lane-derived → `attention_item.*` stream.
- `livespec-console-beads-fabro-8aw` (backlog) — the four non-valve initial
  commands, split out of `pke3y3` as this document's §Steps anticipated.

**No epic anchor to close.** This thread predated the Planning-Lane epic-anchor
rule and never had one, so archiving is the directory move alone.

**Two corrections to the §Pointers above, for anyone reading this from history:**
- The ledger read is `with-livespec-env.sh -- bd list --json`. A bare `bd list`
  fails with `Access denied for user 'livespec-console-beads-fabro'` — the tenant
  password only arrives through the wrapper.
- Ledger item `livespec-console-beads-fabro-0tu` ("Remove baked-in explanatory doc
  prose from the TUI panes") was still `backlog` at closing time. **⚠ CORRECTED
  2026-07-19 — an earlier version of this line claimed it was "in fact DONE, B5
  shipped it as Scenario 21 in PR #289". That claim was WRONG and this document
  was the origin of a real error: acting on it, this session closed `0tu` as
  `resolution:completed` with a close_reason asserting its criteria were "met in
  full by B5".** `0tu`'s acceptance criteria are TWO-PART — (a) the prose no
  longer renders, AND (b) "any useful explanation is relocated to `docs/*.md`".
  The cockpit plan deliberately splits them: its B5 section reads "relocate any
  genuinely-useful explanation into the docs tree (**B6**)". PR #289 delivered
  only half (a); half (b) is B6's and was not complete at close time. The error
  was weighing clause (a) and not clause (b). **`0tu` is therefore closed with an
  overstated reason, and its disposition (leave-closed-and-amend vs. reopen) is
  OPEN and owned by the `cockpit-ux-docs-release` track — tracked as
  `livespec-console-beads-fabro-3rdmqu` (`blocked` / `needs-human`), which carries
  the full account.** Do not treat `0tu`'s `resolution:completed` as evidence its
  relocation half shipped.
