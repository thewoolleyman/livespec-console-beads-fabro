# Test-adequacy gates — region coverage, fuzz, mutation

**Epic anchor:** `livespec-console-beads-fabro-4jb3kl`

**Supersedes:** `plan/archive/impl-dispatch/SUPERSEDED-BY.md` (split 2026-07-19), which
carries the routing table showing how these items landed here. Do NOT resume the
archived `handoff.md` beside it.

## Charter

Make the Quality Gate actually measure test ADEQUACY — region coverage, fuzzing,
mutation — at merge time and nightly. One tool family, one set of targets, one corpus;
the merge-gate and the nightly soak are two schedules of the same system.

This thread deliberately excludes commit-protocol and repo-invariant guards. Those are
a different knowledge domain and live in `plan/repo-invariant-guards/`. Grouping all
"things that fail builds" into one thread is vehicle-grouping — the accretion disease
that produced the superseded thread.

## Read first

1. This file.
2. `SPECIFICATION/non-functional-requirements.md` §"Quality Gate" (heading :97; the merge-gate and nightly clauses at :141-179) — the fuzz
   and mutation jobs are ratified MUSTs; the nightly clause is there too. Read the LIVE
   file: the clause was ratified at v004 but REFRAMED at v007, and the spec is now v032.
   v004's original text claimed "100% line AND 100% region"; v007 reframed it, and the
   sentence slice (a) must flip is the CURRENT v032 one at :112-116 (its
   load-bearing tail, "NOT yet a present gate", is at :116) ("100% line gates
   today; 100% region is the stated next target"). v004 is history and unflippable.
3. `justfile:195` (`--fail-under-lines 100` today) and the `check-fuzz-smoke` /
   `check-mutants-smoke` seeds at :285-291.
4. `.github/workflows/ci.yml` — the `ci-green` aggregation at :248.
5. `AGENTS.md` — mutation protocol, `gh` 2.46.0 gotchas.

## Status is read live, never stored here

```
/livespec-orchestrator-beads-fabro:list-work-items --json
/livespec-orchestrator-beads-fabro:next --json
```

## The work

### `-txtzn5` — region-coverage gate + CI merge-gate fuzz and mutation jobs

Labelled `needs-regroom`; it is an epic of three distinct jobs. Verified GENUINE:
`justfile:195` gates `--fail-under-lines 100`, NOT `--fail-under-regions 100`, and no
fuzz or mutants job exists in CI.

Three slices:
- **(a)** `cargo llvm-cov --fail-under-regions 100` in `check-coverage`. This realizes
  the ratified `coverage-region-gate` spec commitment from v007 — which reframed the
  NFR to "100% line gates today; 100% region is the stated next target". Landing it
  carries a tiny spec-reconciliation rider flipping that sentence. (Hint mismatch on
  record: the epic carries `spec_commitment_hint quality-gate-ci-jobs`; the v007
  follow-up id_hint is `coverage-region-gate`. Same obligation — do not file twice.)
- **(b)** CI merge-gate fuzz job: ≥60s/target on event-envelope, adapter-normalization
  and source-payload; committed regression corpus; fail on any new crash.
- **(c)** CI mutation job: `cargo mutants --in-diff` over `console-domain` +
  `console-application`, `--test-tool nextest`, justified-survivor allow-list.

### `-topr34` — nightly fuzz + mutation soak vs master, opening chores via CI beads access

Labelled `needs-regroom`. Verified GENUINE: no nightly workflow exists in
`.github/workflows/` at all.

Self-declared MIXED autonomy — regroom into two dep-linked slices:
1. **Host/ops (human, maintainer-only):** wire `BEADS_DOLT_PASSWORD` into CI per the
   Beads/Fabro Family Secret Convention. No agent can provision a repo secret.
2. **Factory-safe:** the nightly job + chore-opening. `depends_on` slice 1.

## Sequencing

1. `-txtzn5` before `-topr34` — the nightly soak reuses the merge-gate fuzz/mutants
   infrastructure. Building the nightly first means building it twice.
2. **`-txtzn5`(a) is a repo-global gate change.** The moment the region gate lands,
   every in-flight PR across every thread becomes subject to it. Land it at a
   **low-water mark of open PRs**, or accept topping up in-flight branches. This
   constraint binds all other threads regardless of file layout — it is the one
   sequencing fact in this thread that other sessions need to know.
3. Shares `justfile` and `.github/workflows/ci.yml` with `plan/repo-invariant-guards/`.
   The line-adjacent hazard is the `targets=(...)` array at `justfile:154-167` (that
   thread may append guard targets) versus `check-coverage` at `:195` (edited here).

   **Tie-break, agreed in both handoffs: THIS thread owns `justfile` and `ci.yml` for
   the duration of the region-gate work, and `repo-invariant-guards` rebases onto it.**
   Rationale: the region-coverage flip retroactively binds every open PR including
   theirs, so it needs the low-water mark and must not be made to wait.
4. Parallel-safe against event-identity, command-queue, and operator-surface — no shared
   files.

## Gates

1. **Regroom approval on both items.** `groom` is drafting-only; the maintainer OWNS the
   cut and the acceptance. Neither item can move without it.
2. Maintainer admits each resulting slice. WHICH VERB depends on where the slice lands, which the item's effective
   `admission_policy` decides (`non-functional-requirements.md:170-173`) — do not assume.
   If it lands at `pending-approval`, `approve` is the verb (`contracts.md:442` defines it
   as exactly that transition). If it lands at `backlog`, `approve` does NOT apply and the
   route is `move:<id>:ready`; note the orchestrator also refuses `pending-approval` as a
   `move` target (`:450-451`), so there is no route INTO the valve from `backlog`. Read the
   slice's actual status before asking the maintainer for a verb.
3. Maintainer provisions the CI beads credential for `-topr34` slice 1 — a hard
   host/ops gate.
4. The `-txtzn5`(a) spec-reconciliation rider passes independent review + `/livespec:revise`
   when it lands.
5. Maintainer chooses the low-water-mark window for the region-gate flip.

## Keep this invariant

`just check` does NOT run `check-e2e-tmux` — it is absent from the `targets=(...)` array
at `justfile:154-167`, so ordinary gate runs never spawn tmux. (The related `#[ignore]`
note at `justfile:43-53` explains why the nextest matrix stays tmux-free; the array is
the load-bearing part.) **Keep it that way.** Do not let a new coverage or soak target
pull tmux into the default matrix.

## Dispatch

Ready slices go **factory-side** — the Dispatcher drains `ready`, or run
`/livespec-orchestrator-beads-fabro:drive --action impl:<id>`.
