# 001 — Charter and measurement plan

Plan thread: `optimize-console-builds`. Created 2026-08-30 from live research on all
three build environments (see notes 002–005) plus the mechanism/prior-art survey
(note 006). This note is the charter: what the plan must deliver, the three hard
maintainer requirements it is bound by, the phase sequencing, and the eviction
principles every cache tier must satisfy.

## Goal

Optimize BUILD TIMES for the console (Rust workspace) across all three
environments it is built in, with cache growth bounded by cleanup/eviction that
never deletes still-useful cache:

| Env | Host | Role | Cores / RAM | Build story today |
|---|---|---|---|---|
| Local dev | `vps` (vmi3006760, 100.89.189.118) | maintainer + agent sessions | 18 / 94 GiB | persistent `target/` per checkout+worktree; warm no-op build ~17 s; no eviction of anything |
| Factory | `hp-xubuntu` (100.68.193.50) | fabro dispatch/factory | 16 / 30 GiB | fresh Docker sandbox per run, **fully cold** build every run (registry download + full workspace compile thrown away with the container) |
| CI | `poweredge-xubuntu` (100.78.140.72) | k3s/ARC self-hosted runner | 72 / 188 GiB | **fully cold** Rust build every run; warm cache is uv-only |

## Hard requirements (maintainer-set, binding)

1. **Honeycomb measurement, BEFORE and AFTER, proving every improvement.**
   Proper build-time analytics in Honeycomb are a precondition: where they do
   not exist (they largely do not — note 005), CREATING them is the plan's FIRST
   phase, before any fix. Every optimization this plan ships must show a
   measured before→after delta in Honeycomb, not a hand-timed anecdote.
2. **Bounded growth, safe eviction.** Every cache tier the plan adds must ship
   WITH a bounded, age/staleness-based eviction policy that retains live/useful
   cache and evicts only old/stale — never a blunt size cap that drops hot
   cache. See "Eviction principles" below.
3. **Final report + human approval gate before archive.** The plan's final
   state is a report of overall improvements in ALL affected areas, in BOTH raw
   times AND percentages (per environment, per optimization, before→after, from
   Honeycomb data). A human APPROVES those results before the plan is archived.
   This is an EXPLICIT archive gate for this plan, in addition to the plan
   operation's built-in child-disposition and independent-completeness-review
   gates. Do not archive without the recorded approval.

## Phases

**Phase 1 — telemetry first (the measurement substrate).**
Stand up build-time telemetry in Honeycomb for all three environments, then
capture the cold BEFORE baselines. Today only CI has anything, and only
partially (note 005): per-JOB spans on push-to-master runs only — no PR runs,
no build-vs-test phase split inside a job, and nothing at all from local or
factory builds. Phase 1 delivers, at minimum:

- CI: extend `.github/scripts/export-ci-telemetry.sh` (or add step-level
  emission) so PR runs are covered and the compile phase is separable from the
  test/fuzz phase for the critical-path jobs (`check-nextest`, `check-fuzz`,
  `check-coverage`, `check-clippy`).
- Factory: emit spans for the sandbox build/check phases. The `fabro-sandbox`
  dataset already receives `prepare.*` spans, so a span-emission seam exists —
  extend it to the cargo build/test phases of the run.
- Local: instrument the common entry points (`just check` / the pre-push
  aggregate, or a thin cargo-timing exporter) so local build durations land in
  Honeycomb with a `host`/`env` attribute.
- A shared attribute scheme so before/after queries are trivial:
  `build.env` (local|factory|ci), `build.phase`, `repo`, `git.commit.sha`,
  cache-state markers (e.g. `build.cache.tier`, hit/miss) once tiers exist.
- BEFORE baselines recorded as Honeycomb query results (raw seconds), cited in
  this plan's notes.

**Phase 2 — per-environment optimizations, each measured.**
Mechanism options and tradeoffs are laid out in note 006; the worker session +
maintainer pick per environment. Ordering guidance from the research: CI's
cheapest lever is job-scoped compile parallelism; CI's real build is extending
the tier-1 warm cache (cargo registry + warmed target generations); the
factory's cheapest levers are a shared registry mount and/or sccache on
`/data`; local needs eviction more than speedup. Each landed optimization gets
its own after-measurement against the Phase 1 baseline before the next is
judged.

**Phase 3 — final measured report + approval + archive.**
Compose the raw+percentage before/after table for every affected area from
Honeycomb data, present to the maintainer, obtain explicit approval, then run
the plan's normal archive gates.

## Eviction principles (requirement 2, expanded)

- **Generation model preferred.** The ratified uv warm-cache model (populate a
  new hardlink-seeded generation → atomic symlink flip → prune all but newest
  N) is the house pattern: age-based, bounded, never touches the live
  generation, one trusted writer. New tiers should reuse it where the shape
  fits (note 006 §a/§b).
- **Age/staleness, not size.** Eviction keys on generation age or last-use
  staleness. A size threshold may exist only as a monitoring alarm, not as the
  eviction trigger. Where a tool ships only an LRU size cap (sccache), the plan
  must either front it with an age-based janitor or record an explicit,
  reasoned acceptance that LRU meets the requirement's intent (never deletes
  hot cache) for that tier — decided in the work-item, not silently.
- **Every tier ships WITH its eviction.** No cache lands in a PR without its
  bound. "Add cache now, prune later" is not a valid slice.
- **Existing unbounded growth is in scope.** Local `target/` dirs (12 G primary
  + ~4 G per worktree, nothing evicts orphaned worktree targets) and the
  factory's exited-container layers (bounded at 24–48 h today, but each carries
  a thrown-away 2–7 GB cold build) are the current offenders; the CI ARC
  work-volume's misleading dead `requests.storage: 5Gi` gets corrected as
  cleanup.

## Scope notes

- Ratified floors are not build time: the 180 s fuzzing floor (3×60 s) in
  `check-fuzz` is NOT reducible and is excluded from improvement targets.
- This plan SUBSUMES `livespec-console-beads-fabro-txtzn5.16` (cache the fuzz
  build; `ready`, autonomy HOST-ONLY) — reference it, do not duplicate it.
- Cross-repo touchpoints: warm-cache implementation lives in
  `livespec-dev-tooling` (`ci-runner/k3s/phase2/warm-cache/`), factory host
  services in `fabro-hosts`; changes there follow those repos' processes.
