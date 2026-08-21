# Charter and sequencing — test-adequacy gates

Durable reasoning for the `test-adequacy-gates` plan thread. Status,
next actions, and handoffs are NOT here: they are ledger comments on the
plan epic `livespec-console-beads-fabro-4jb3kl`, read via the plan
operation's timeline. This file migrates the durable half of the retired
`plan/test-adequacy-gates/handoff.md`.

**Supersedes:** `plan/archive/impl-dispatch/SUPERSEDED-BY.md` (split
2026-07-19), which carries the routing table showing how these items
landed here. Do NOT resume the archived `handoff.md` beside it.

## Charter

Make the Quality Gate actually measure test ADEQUACY — region coverage,
fuzzing, mutation — at merge time and nightly. One tool family, one set
of targets, one corpus; the merge-gate and the nightly soak are two
schedules of the same system.

This thread deliberately excludes commit-protocol and repo-invariant
guards. Those are a different knowledge domain and live in
`plan/repo-invariant-guards/`. Grouping all "things that fail builds"
into one thread is vehicle-grouping — the accretion disease that
produced the superseded thread.

## Read first

1. This file, then
   `research/002-region-coverage-gap-measurement.md` — the measured size
   of the region-gate slice, which contradicts the original one-line-flip
   framing.
2. `SPECIFICATION/non-functional-requirements.md` §"Quality Gate"
   (heading :97). The fuzz and mutation jobs are ratified MUSTs. The
   region sentence slice (a) must flip is the CURRENT one at :112-119,
   with its load-bearing tail "NOT yet a present gate" at :115-116
   ("100% line gates today; 100% region is the stated next target").
   Read the LIVE file: the clause was ratified at v004 and REFRAMED at
   v007; v004's "100% line AND 100% region" text is history and
   unflippable.
3. The no-exclusions clause at :120-132 — load-bearing for slice (a),
   see research note 002.
4. `justfile:272` (`check-coverage`, gating `--fail-under-lines 100` via
   `dev-tooling/coverage-gate.py`) and the `check-fuzz-smoke` /
   `check-mutants-smoke` seeds at :481-491.
5. `.github/workflows/ci.yml` — the `ci-green` aggregation.
6. `AGENTS.md` — mutation protocol, `gh pr checks --json` polling.

## The work

### `-txtzn5` — region-coverage gate + CI merge-gate fuzz and mutation jobs

Labelled `needs-regroom`; it is an epic of distinct jobs. Verified
GENUINE on 2026-08-21: `justfile:272` gates `--fail-under-lines 100`,
NOT `--fail-under-regions 100`, and no fuzz or mutants job exists in
`.github/workflows/`.

- **(a)** `cargo llvm-cov --fail-under-regions 100` in `check-coverage`.
  This realizes the ratified `coverage-region-gate` spec commitment from
  v007. Landing it carries a spec-reconciliation rider flipping the
  :112-119 sentence. **The 2026-08-21 measurement (note 002) shows this
  is not a one-line flip: 893 regions are uncovered today**, and the
  maintainer ruling recorded there cuts (a) into a1 (close the 166
  production gaps), a2 (refactor the 713 in-crate test regions down
  via the shared-check-helper pattern), and a3 (flip the gate +
  rider). (Hint
  mismatch on record: the epic carries `spec_commitment_hint
  quality-gate-ci-jobs`; the v007 follow-up id_hint is
  `coverage-region-gate`. Same obligation — do not file twice.)
- **(b)** CI merge-gate fuzz job: >=60s/target on event-envelope,
  adapter-normalization and source-payload; committed regression corpus;
  fail on any new crash.
- **(c)** CI mutation job: `cargo mutants --in-diff` over
  `console-domain` + `console-application`, `--test-tool nextest`,
  justified-survivor allow-list.

### `-topr34` — nightly fuzz + mutation soak vs master, opening chores via CI beads access

Labelled `needs-regroom`. Verified GENUINE on 2026-08-21: no nightly
workflow exists in `.github/workflows/` at all (the only `schedule:`
trigger in the tree is `pin-freshness.yml:22`).

Self-declared MIXED autonomy — regroom into two dep-linked slices:

1. **Host/ops (human, maintainer-only):** wire `BEADS_DOLT_PASSWORD`
   into CI per the Beads/Fabro Family Secret Convention. No agent can
   provision a repo secret.
2. **Factory-safe:** the nightly job + chore-opening. `depends_on`
   slice 1.

## Sequencing

1. `-txtzn5` before `-topr34` — the nightly soak reuses the merge-gate
   fuzz/mutants infrastructure. Building the nightly first means
   building it twice.
2. **`-txtzn5`(a) is a repo-global gate change.** The moment the region
   gate lands, every in-flight PR across every thread becomes subject to
   it. Land it at a **low-water mark of open PRs**, or accept topping up
   in-flight branches. This constraint binds all other threads
   regardless of file layout — it is the one sequencing fact in this
   thread that other sessions need to know.
3. Shares `justfile` and `.github/workflows/ci.yml` with
   `plan/repo-invariant-guards/`. The line-adjacent hazard is the
   worktree-pack / guard-target region of the `justfile` versus
   `check-coverage` at `:272` (edited here).

   **Tie-break, agreed in both handoffs: THIS thread owns `justfile` and
   `ci.yml` for the duration of the region-gate work, and
   `repo-invariant-guards` rebases onto it.** Rationale: the
   region-coverage flip retroactively binds every open PR including
   theirs, so it needs the low-water mark and must not be made to wait.
4. Parallel-safe against event-identity, command-queue, and
   operator-surface — no shared files.
5. Ledger item `livespec-console-beads-fabro-wnlcnj` (ready) also edits
   `ci.yml` (dropping the redundant `check-test` matrix job). It is not
   owned by this thread; sequence it before the fuzz/mutants job lands
   or expect a rebase.

## Gates

1. **Regroom approval on both items.** `groom` is drafting-only; the
   maintainer OWNS the cut and the acceptance. Neither item can move
   without it.
2. Maintainer admits each resulting slice. WHICH VERB depends on where
   the slice lands, which the item's effective `admission_policy`
   decides (`non-functional-requirements.md` §"Admission") — do not
   assume. If it lands at `pending-approval`, `approve` is the verb. If
   it lands at `backlog`, `approve` does NOT apply and the route is
   `move:<id>:ready`; the orchestrator also refuses `pending-approval`
   as a `move` target, so there is no route INTO the valve from
   `backlog`. Read the slice's actual status before asking the
   maintainer for a verb.
3. Maintainer provisions the CI beads credential for `-topr34` slice 1 —
   a hard host/ops gate.
4. The `-txtzn5`(a) spec-reconciliation rider passes independent review
   + `/livespec:revise` when it lands.
5. Maintainer chooses the low-water-mark window for the region-gate
   flip.

## Keep this invariant

`just check` does NOT run `check-e2e-tmux` — it is absent from the
`targets=(...)` array the `check` aggregate walks, so ordinary gate runs
never spawn tmux. (The related `#[ignore]` note near the top of the
`justfile` explains why the nextest matrix stays tmux-free; the array is
the load-bearing part.) **Keep it that way.** Do not let a new coverage
or soak target pull tmux into the default matrix.

## Dispatch

Ready slices go **factory-side** — the Dispatcher drains `ready`, or run
`/livespec-orchestrator-beads-fabro:drive --action impl:<id>`.
