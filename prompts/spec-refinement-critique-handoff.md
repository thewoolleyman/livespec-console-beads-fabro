# Spec-refinement handoff: capture-drift + capture-gaps cycles (console)

Goal: keep hardening the `livespec-console-beads-fabro` `SPECIFICATION/` by
reconciling spec ↔ implementation in **two rounds** — running
`/livespec-orchestrator-beads-fabro:capture-spec-drift` (impl→spec: bend the
spec toward what the code actually does) and
`/livespec-orchestrator-beads-fabro:capture-impl-gaps` (spec→impl: file
work-items for what the code still lacks), **two runs of each** — while the
livespec quality-of-life improvements land.

This track is **dual-output by design**:

- `capture-spec-drift` **changes the spec** — each finding becomes a
  `/livespec:propose-change`, processed by `/livespec:revise` into a new
  `history/vNNN/` cut that you land via worktree → PR → merge.
- `capture-impl-gaps` **does NOT change the spec** — it files gap-tied
  **impl work-items** into the Beads tenant (wrapped). No spec cut comes out
  of it.

> This file is the single living spec-refinement handoff: it is updated in
> place each cycle and is the ONE path the next session runs
> (`run prompts/spec-refinement-critique-handoff.md`). The prior
> `/livespec:critique` → `/livespec:revise` sub-track it used to describe has
> **converged at `v008`** (see Status); its detail lives in git history.

## Status (as of master `1711e52`, history `v008`)

- The `/livespec:critique` → `/livespec:revise` track **converged at `v008`**.
  Recent cuts: `v007` reconciled the coverage gate (line gates today; 100%
  region is the tracked `coverage-region-gate` target) and the Beads stream
  label (`work_item.*` → `beads.*`); `v008` added the Control-Plane
  realization's invoke/command facet. The persistence envelope already matches
  the eventstore (`v004`). A final critique pass surfaced no further material
  findings.
- `SPECIFICATION/proposed_changes/` is empty but tracked via its `README.md`,
  so `doctor-static` is **green on a clean checkout** (exit 0).
- **Impl work-items already filed** in the Beads tenant — run
  `… with-livespec-env.sh bd list` to confirm and DO NOT duplicate:
  - `rrr4i4` P0 epic — port the clause→scenario→test behavioral-coverage
    checker to Rust (`scenario-test-rust-checker`).
  - `gkqyaf` P1 — upgrade `console-arch-check` from text scans to
    `cargo metadata` + AST checks.
  - `mvu22t` P1 — Rust Red-Green-Replay commit-msg enforcement.
  - `topr34` P1 — nightly fuzz+mutation soak opening chores via CI beads access.
  - `txtzn5` P1 epic — region-coverage gate (`--fail-under-regions 100`) + CI
    merge-gate fuzz and mutation jobs (part (a) is the `coverage-region-gate`
    spec commitment from `v007`; see its cross-reference comment).
  - `pke3y3` P2 epic — implement the 7 unimplemented initial commands.

## Operating discipline (MUST — read before running any cycle)

Per `AGENTS.md` §"Repository mutation protocol" and §"Beads runtime
prerequisites":

- **Worktree, never the primary.** Every mutation happens in an isolated
  worktree under `~/.worktrees/livespec-console-beads-fabro/<branch>` created
  from `master`. NEVER edit or commit on the primary checkout — the
  commit-refuse hook enforces it; never `--no-verify`. Branch-delete/push from
  the primary is refused too; delete a merged remote branch via
  `gh api -X DELETE repos/<owner>/<repo>/git/refs/heads/<branch>`.
- **Land each checkpoint; don't accumulate.** Commit + land each drift cut
  (new `history/vNNN/` + working spec) via worktree → PR → merge → cleanup
  before starting the next. A docs/spec changeset uses a `docs(...)` /
  `chore(...)` subject and is exempt from Red-Green-Replay. After merge,
  refresh the primary to `origin/master`, remove the worktree, delete the
  branch, verify the primary is clean on `master`.
- **Wrapped beads only.** Every `bd` / work-item filing — including everything
  `capture-impl-gaps` and `capture-work-item` drive — runs under the fleet
  wrapper, from the repo root:
  `LIVESPEC_BD_PATH=/usr/local/bin/bd /data/projects/1password-env-wrapper/with-livespec-env.sh bd <args>`.
  "Access denied" / "no beads database found" means the call was UNWRAPPED, not
  a server fault. A `CALL DOLT_BACKUP … command denied` warning is
  correct-by-design — ignore it.
- **Don't fake green; gates land with their checkers.** Never neuter a gate to
  get green. The only legitimate green for the behavioral-coverage,
  arch-check-upgrade, RGR, nightly-soak, and region-coverage obligations is
  building them (each is a tracked work-item above). Code/config changes are
  NOT routine here — make one only when the maintainer explicitly directs it,
  and even then via the worktree → PR discipline.
- **Run against the fixed core.** Set
  `LIVESPEC_CORE_PLUGIN_ROOT=/data/projects/livespec/.claude-plugin` so the
  lifecycle uses livespec master (doctor fully green). Author identity:
  `LIVESPEC_AUTHOR_LLM=<model-id>`.
- **Version-collision recovery.** The fleet sometimes lands a concurrent
  `docs(spec)` commit that cuts the same `vNNN` while your PR is in CI (this
  happened once at `v006`). If your merge reports a conflict, confirm the
  collision touches a DISJOINT set of files, then **re-cut your reconciliation
  as the next `vNNN` on the new master** (do not git-rebase the colliding
  `history/` dir). Land fast to shrink the window: merge the moment CI is green.

## Methodology: ground reconciliations in impl reality

Run drift FIRST each round so the gap pass reads a spec already reconciled to
impl reality. Ground every reconciliation by reading the actual code — the
domain `ConsoleEvent` / `EventType`, the `console-eventstore` DDL, the
`console-application` adapters/ports, the `justfile`, `.github/workflows/ci.yml`,
and the workspace lints — and reconcile toward what the impl does, never toward
an idealized shape the code never took. (This is why the event-envelope D1
reconciliation had to be corrected in `v004`, and why the `v007` coverage
finding pointed the spec at the line-only gate the justfile actually enforces.)

## How to run the two rounds

For each round (run drift, then gaps):

1. **`capture-spec-drift` (run N).** Detect impl→spec drift; for each finding,
   file a `/livespec:propose-change` (bundle related findings under one topic,
   e.g. `impl-drift-reconciliation`, as `v004` / `v007` did). Then
   `/livespec:revise` to accept / modify / reject each — dispositions are
   maintainer-owned; lead with a recommendation, and in an autonomous run
   decide and document the rationale. This cuts the next `history/vNNN/`.
   **Land it** before continuing.
2. **`capture-impl-gaps` (run N).** Detect spec→impl gaps; for each NEW gap not
   already covered by the filed work-items above, file a gap-tied work-item via
   the wrapper, with per-gap consent / documented decision. These are impl
   realizations, NOT spec changes — do not edit the spec from this pass. If a
   gap maps to an existing item, add a cross-reference comment instead of a
   duplicate (as was done linking `coverage-region-gate` → `txtzn5`).
3. **Round 2.** Repeat steps 1–2. A second run that surfaces nothing new means
   that stream has **converged** — record that and stop it early rather than
   manufacturing findings.

Stop when both streams' second runs surface no new material findings/gaps, or at
the maintainer's direction.

## Out of scope for this track

- Building the tracked impl obligations (the behavioral-coverage Rust checker,
  the arch-check `cargo metadata`/AST upgrade, RGR enforcement, the nightly
  soak, the region-coverage gate + CI fuzz/mutation jobs, the 7 commands) —
  these are impl realizations the gap pass FILES, not work this track DOES.
- Neutering any gate to force green.
- The `/livespec:critique` → `/livespec:revise` sub-track (converged at `v008`;
  reopen only if drift/gaps or new spec content surface fresh material findings).
- Any code/config change not explicitly directed by the maintainer.

## Close criteria

- Primary clean on `master`, `origin/master` carries each landed drift cut, no
  orphaned worktrees/branches, no red/pending CI.
- New impl gaps filed (wrapped) without duplicating the existing work-items;
  cross-references added where a gap maps to an existing item.
- Both streams' second runs reported (converged, or the residual findings
  landed/filed).
