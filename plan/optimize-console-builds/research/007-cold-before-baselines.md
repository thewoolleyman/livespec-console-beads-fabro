# 007 — Cold BEFORE baselines (charter requirement 1)

Captured 2026-08-31 from the Honeycomb `github-ci` dataset (team `thewoolleyweb`,
environment `livespec`) via the Honeycomb MCP `run_query` tool, all scoped to
`repo = thewoolleyman/livespec-console-beads-fabro`. These are the BEFORE numbers
every Phase-2 optimization must beat; each Phase-2 after-measurement re-runs the
same query and cites the delta (charter `research/001` requirement 1).

Anchors the work-item `livespec-console-beads-fabro-fhdzka`. All three legs are
now recorded below: CI and factory as captured Honeycomb baselines, local as an
accepted on-demand emitter (maintainer decision 2026-09-01).

The poweredge CI **disk** baseline captured just before the RAID5→RAID10 rebuild
lives in a dedicated note, `research/008-pre-raid10-poweredge-disk-baseline.md`.

## CI — cold every run (COMPLETE)

CI on the self-hosted ARC pool builds fully cold every run (no cargo cache;
`research/002`, `research/005`), so the current numbers ARE the cold baseline.

### Per-job wall time (7-day window, P50 / P95 / MAX seconds)

Query: `ci.job.name exists AND repo = <this repo>`, breakdown `ci.job.name`,
`P50/P95/MAX(duration_ms)`, 7d. 91 runs per job.
Honeycomb: https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/6Nix6SZkJy

| Job | P50 s | P95 s | MAX s |
|---|---|---|---|
| check-fuzz | 403 | 528 | 681 |
| check-nextest | 266 | 429 | 449 |
| check-e2e-tmux | 256 | 408 | 474 |
| check-coverage | 244 | 376 | 395 |
| check-deps | 188 | 341 | 401 |
| check-clippy | 185 | 331 | 371 |
| check-arch | 157 | 300 | 401 |
| check-behavior-coverage | 150 | 329 | 371 |
| check-baseline | 148 | 308 | 344 |
| check-completeness | 147 | 313 | 354 |
| check-plan-no-tombstone | 140 | 314 | 356 |
| check-shell-quality | 136 | 264 | 338 |
| check-format | 127 | 321 | 358 |
| check-mutants | 125 | 261 | 333 |
| check-plugin-resolution | 123 | 325 | 353 |
| check-doctor-static | 105 | 207 | 299 |
| ci-green (aggregator) | 2 | 4 | 4 |
| **all jobs (TOTAL)** | **170** | **394** | **681** |

### Compile vs test/fuzz phase split (24h window, P50 / MAX seconds)

The phase-span split landed 2026-08-31 (`icmvza`, PR #903 + the skipped-step fix
#904), so phase data starts today. Query: `build.env = ci AND repo = <this repo>
AND duration_ms > 0`, breakdown `name, build.phase`, `P50/MAX(duration_ms)`, 24h.
Honeycomb: https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/DrAnWUoymgm

| Phase span | phase | P50 s | MAX s |
|---|---|---|---|
| build.check-fuzz.fuzz | fuzz | 188 | 192 |
| build.check-fuzz.compile | compile | 78 | 80 |
| build.check-nextest.compile | compile | 66 | 71 |
| build.check-clippy.compile | compile | 42 | 49 |
| build.check-nextest.test | test | 13 | 45 |

(`check-coverage` is intentionally NOT phase-split — `cargo llvm-cov` cleans and
re-instruments by default, so a warm compile step would double-compile the 100%
gate; its 244 s job-level number above is the coverage baseline. The `duration_ms
> 0` filter excludes the pre-#904 zero-duration artifacts.)

### Reading of the CI baseline

**Compile time dominates the critical jobs, and it is the cold-cache tax.**
- `check-nextest`: 66 s compile vs 13 s test — compile is ~84% of the measured
  phase time. A warm cargo/target cache attacks the 66 s directly.
- `check-fuzz`: 78 s ASAN **compile** is separable from the **188 s fuzz-run
  floor** (3×60 s, ratified, NOT reducible — `research/001` scope note). Only the
  78 s compile is an optimization target.
- `check-clippy`: 42 s, effectively all compile-time lint.

This confirms `research/006`'s CI ordering: the real CI lever is extending the
warm cache (cargo registry + warmed `target/` generations) to cut the cold
compile, not job-level parallelism. Phase-2 targets: the compile rows above.

## Local — on-demand (accepted, documented)

Query `build.env = local AND repo = <this repo>`, 14d: **0 rows**. The local
emitter (`emit-local-build-telemetry.sh`, `iqulbh`) is fail-soft and SKIPS when
`HONEYCOMB_BUILD_INGEST_KEY` is absent — and a normal `just check` run (outside
the family 1Password env wrapper) has no key, so nothing emits. Observed live:
`build-telemetry(local): no ingest key — skipping.`

**Maintainer decision 2026-09-01: leave local telemetry DOCUMENTED / on-demand —
it is not a blocking gap.** The charter's local story is eviction, not speedup
(`research/001`, `research/004`), so a continuous local baseline is not required
for Phase 2. To capture a local data point on demand: run `just check` with the
build ingest key exported into the emitter's env (e.g. under the family wrapper);
a WARM point is immediate, a COLD one needs a `target/` wipe first.

## Factory — cold every run (COMPLETE)

The fabro-sandbox factory (`hp-xubuntu`) builds fully cold every run (the
sandbox `target/` is thrown away with the container), so live spans ARE the cold
baseline. Captured 2026-09-02 from `github-ci` over the v1.37.1 shimmed-image
dispatches (891 `build.cargo-*` spans; the `co3` validation run plus subsequent
factory dispatches). Query: name starts-with `build.cargo-` AND
`repo = <this repo>`, breakdown `name`, P50/P95/MAX(`duration_ms`), 7d.
https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/9x1UKx8qXo2

| cargo span | P50 s | P95 s | MAX s | n |
|---|---|---|---|---|
| build.cargo-llvm-cov | 9.59 | 29.07 | 61.51 | 155 |
| build.cargo-test | 7.82 | 31.86 | 61.67 | 136 |
| build.cargo-nextest | 3.14 | 13.83 | 30.22 | 61 |
| build.cargo-check | 1.67 | 5.36 | 15.24 | 37 |
| build.cargo-clippy | 1.44 | 9.53 | 23.01 | 99 |
| build.cargo-build | 0.13 | 49.75 | 50.40 | 64 |
| build.cargo-run | 0.09 | 4.26 | 59.60 | 338 |
| build.cargo-fetch | 2.00 | 2.00 | 2.00 | 1 |

**Reading — these are per-`cargo`-invocation spans, not per-run wall time.** One
factory run invokes cargo many times; the cheap incremental invocations pull P50
down, while the **cold full compile shows in P95/MAX** (`cargo-build` MAX 50.4 s,
`cargo-test`/`cargo-llvm-cov` MAX ~61 s). A per-RUN factory wall-time comparable
to CI's per-job table would need run-level aggregation; the on-host pre-push
`just check` was measured at 251–273 s on 4 vCPUs (2026-07-30, `research/003`).

**Query the factory with `name starts-with "build.cargo-"`, NOT `build.env =
factory`.** Conformance gap (follow-up, not a data gap): the v1.37.1 shim sets
span name + `repo` + duration but NOT the scheme's `build.env=factory` /
`build.phase` attrs (`build.env exists` on `build.cargo-*` = 0). Filed as a
follow-up; either conform the emitter (map subcmd→`build.phase`, set
`build.env=factory`) or accept name-keyed factory spans and note the deviation.

## Status

- CI cold baseline: **captured and cited** (both queries above).
- Factory cold baseline: **captured and cited** (query above), on the v1.37.1
  shimmed image; `build.cargo-*`-keyed pending the attribute-conformance
  follow-up.
- Local baseline: **accepted on-demand** (maintainer decision); not a blocker.
- poweredge CI **disk** baseline (pre-RAID5→RAID10): captured in `research/008`.

All three telemetry legs are recorded; `fhdzka` and `2er6nc` are closed on this
basis. Phase 2 (per-environment optimizations) is judged against these numbers.
