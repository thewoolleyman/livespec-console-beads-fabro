# Spec-refinement handoff: critique → revise cycles (console)

Goal: refine and harden the `livespec-console-beads-fabro` `SPECIFICATION/`
through repeated `/livespec:critique` + `/livespec:revise` cycles. The track is
**spec-focused**: a routine cycle changes only `SPECIFICATION/`. Code or config
changes are NOT routine here — make one only when the maintainer explicitly
directs it (e.g. a fail-closed CI placeholder), and even then through the
worktree → PR discipline below; flag it, never make it silently.

> While the **Worktree Discipline Pack** (openbrain ob-0x5) is not yet
> distributed to this repo, the temporary
> `prompts/worktree-discipline-sidecar.md` hand-installs the worktree + beads
> discipline summarized below and supersedes ad-hoc rules until the pack lands
> (then it is archived).

## Operating discipline (MUST — read before running any cycle)

This track previously went off the rails: it edited directly on the primary
checkout, let cut versions (`v002`–`v004`) pile up uncommitted, ran `bd`
unwrapped, and wired a fail-closed gate into the local inner loop. Do not repeat
those. Per `AGENTS.md` §"Mutation protocol" and §"Beads secret convention":

- **Worktree, never the primary.** Every mutation happens in an isolated
  worktree under `~/.worktrees/livespec-console-beads-fabro/<branch>` created
  from `master`. NEVER edit or commit on the primary checkout — the
  commit-refuse hook enforces this; do not work around it, never `--no-verify`.
- **Land each checkpoint; don't accumulate.** Commit and land each cycle's
  result (the new `history/vNNN/` + working spec) via worktree → PR → merge
  before starting the next, so cuts never pile up uncommitted on the primary. A
  docs/spec changeset uses a `docs(...)` / `chore(...)` subject and is exempt
  from Red-Green-Replay.
- **Wrapped beads only.** Any `bd` / work-item filing — e.g.
  `/livespec-orchestrator-beads-fabro:capture-work-item` for a propose-change's
  `spec_commitments.impl_followups` — runs under the fleet wrapper
  (`LIVESPEC_BD_PATH=/usr/local/bin/bd /data/projects/1password-env-wrapper/with-livespec-env.sh bd …`),
  or launch the whole session under the wrapper. "Access denied" means the call
  was unwrapped, not a server fault.
- **Don't fake green; the gate lands with its checker.** The
  clause→scenario→test behavioral-coverage gate is a hard `fail`-mode check that
  attaches to the real Rust checker (`scenario-test-rust-checker`, a
  release-blocking work-item) and runs in `just check` + CI the moment it lands.
  It is NOT enforced by a fail-closed CI placeholder in the interim — a
  placeholder that hard-fails CI for a not-yet-built checker deadlocks the merge
  gate (it blocks every merge), so `v005` removed it. Never neuter a real gate to
  get green; the only legitimate green is building the checker.

## Methodology: ground reconciliations in impl reality

Before — or alongside — the critique→revise cycles, run
`/livespec-orchestrator-beads-fabro:capture-spec-drift` to detect impl→spec
drift and reconcile the spec toward what the code actually does. A spec-internal
critique can otherwise "fix" an inconsistency in a direction the implementation
never took: this happened once — the event-envelope D1 reconciliation was landed
away from the impl's scalar schema, then corrected in `v004` only after a drift
pass caught it. Establish impl reality first so reconciliations point the right
way.

## How to run a cycle

1. Run `/livespec:critique` against the main spec target (`SPECIFICATION/`),
   feeding the steering text below as the critique focus. Critique files a
   `<author>-critique.md` proposed-change under
   `SPECIFICATION/proposed_changes/` containing findings (ambiguities,
   contradictions, untestable language, missing/over-constrained rules).
2. Review the findings, then run `/livespec:revise` to accept / modify /
   reject each, cutting the next `history/vNNN/`.
3. Repeat until a critique pass surfaces no material findings.

Run against the fixed core: set
`LIVESPEC_CORE_PLUGIN_ROOT=/data/projects/livespec/.claude-plugin` so the
lifecycle uses livespec master (both `livespec-kfjd` and `livespec-lly4` are
landed there); the doctor gate is then fully green. `livespec-lly4` is landed
but NOT yet released to the installed plugin, so a DEFAULT run (installed
plugin) still reports one informational `doctor-template-files-present` failure
for `diagrams/example.{plantuml,svg}` — that exit-3 is non-blocking and the
snapshot still cuts. Once livespec cuts a release + `claude plugin marketplace
upgrade`, the default path is green too.

## Steering text for `/livespec:critique`

Focus this critique on the following areas. For each, surface ambiguities,
contradictions, untestable language, and over- or under-constraint as
findings; do NOT propose implementation.

(The five areas below were the cycle-1 critique scope and were processed into
`v002`; cycles 2+ address findings derived from those edits and from impl→spec
drift, and the latest cut as of this handoff is `v005`. Read the live
`SPECIFICATION/history/` for current state and re-derive steering for that state
rather than re-running these five verbatim.)

1. **Adapter architecture.** (`spec.md` §Architecture / §Bounded Contexts;
   `non-functional-requirements.md` §Constraints → Domain-Driven Design +
   Architecture Tests.) The spec mandates separate `console-adapter-*` crates
   that "MUST NOT depend on each other," yet the same boundary could be
   honored by a single adapters module behind per-source ports. Surface
   whether the per-crate split is an intended hard requirement or an
   over-constraint; the rule must be unambiguous and match the intended design.

2. **Architecture-test enforcement strength.**
   (`non-functional-requirements.md` §Constraints → Architecture Tests.)
   "cargo metadata MAY enforce crate graph rules" and "Source-level checks MAY
   use Rust syntax parsing" are permissive. Surface whether a crate-graph
   dependency check is required vs optional, and whether each enforced rule is
   stated falsifiably (a text scan satisfies the letter today).

3. **Quality-gate thresholds.** (`non-functional-requirements.md` §Contracts →
   Quality Gate.) "coverage with a declared threshold" declares no number;
   "fuzz tests … where practical" and "mutation testing where practical" are
   unfalsifiable. Surface that the spec should state the concrete coverage
   threshold and the criteria under which fuzz and mutation graduate from
   smoke checks to hard gates.

4. **Initial-adapter fidelity and honest incompleteness.** (`spec.md` §Product
   Shape / §Architecture; `contracts.md` §Adapter Contract / §Initial Adapters
   / §Command Handling.) The spec does not resolve whether first-milestone
   adapters may be minimal/simulated vs must perform real source I/O, nor what
   "honest incompleteness" requires — e.g., a stub MUST signal "not observed"
   rather than fabricate success (today a simulated drain port reports success
   without acting). Surface this as a missing rule; it is the spec-side
   counterpart to the simulated-boundary implementation gap tracked separately.

5. **Post-split coherence.** Now that `constraints.md` was reduced and
   `non-functional-requirements.md` added, surface any behavior left in prose
   without a `scenarios.md` scenario, any rule duplicated or contradictory
   across `constraints.md` ↔ `non-functional-requirements.md`, and any
   `spec.md` architecture text inconsistent with the NFR layering rules.

## Out of scope for this track (impl work-items, NOT spec)

- Wiring real source/factory I/O (replacing `ScriptedSource` +
  `SimulatedFactoryDrainPort`) — behavior already specified; an impl
  realization gap, not a spec change.
- Implementing the richer arch-check, the fuzz corpus, the mutation gate, and
  coverage tooling — impl realizations of the (refined) NFRs above.
- Repo doc hygiene (retiring `research/tui-first-milestone-bootstrap-plan.md`,
  the root `README.md` pointer, the `AGENTS.md` "seed state" language) —
  one-off corrections of non-normative docs.

Code/config changes are out of routine scope. When the maintainer explicitly
directs one (e.g. the fail-closed CI placeholder), it is in scope for that
change only and still follows the worktree → PR discipline above — flag it,
never make it silently.
