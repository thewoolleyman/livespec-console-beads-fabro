# 007 — Cold BEFORE baselines (charter requirement 1)

Captured 2026-08-31 from the Honeycomb `github-ci` dataset (team `thewoolleyweb`,
environment `livespec`) via the Honeycomb MCP `run_query` tool, all scoped to
`repo = thewoolleyman/livespec-console-beads-fabro`. These are the BEFORE numbers
every Phase-2 optimization must beat; each Phase-2 after-measurement re-runs the
same query and cites the delta (charter `research/001` requirement 1).

Anchors the work-item `livespec-console-beads-fabro-fhdzka`. The CI leg is
COMPLETE below; the local and factory legs are documented as gaps with the
concrete unblock each needs.

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

## Local — GAP (emitter fail-soft-skips without the ingest key)

Query `build.env = local AND repo = <this repo>`, 14d: **0 rows**. The local
emitter (`emit-local-build-telemetry.sh`, `iqulbh`) is fail-soft and SKIPS when
`HONEYCOMB_BUILD_INGEST_KEY` is absent — and a normal `just check` run (outside
the family 1Password env wrapper) has no key, so nothing emits. Observed live
this session: `build-telemetry(local): no ingest key — skipping.`

Unblock (fhdzka local leg, host action): run `just check` with the build ingest
key exported into the emitter's env (e.g. under the family wrapper). A WARM local
data point is available immediately; a COLD local baseline additionally needs a
`target/` wipe first. Lower priority — the charter's local story is eviction, not
speedup (`research/001`, `research/004`).

## Factory — GAP (emitter shipped; waits on a routine image rollout)

Query `build.env = factory`, 14d: **0 rows** — no live factory spans YET, but the
emitter is now IMPLEMENTED and MERGED: `livespec-console-beads-fabro-2er6nc` /
`livespec-dev-tooling-bfvbsw` shipped as **livespec-dev-tooling PR #1658**
(merge `e0708150`). A `cargo` shim baked onto the fabro-sandbox `python-rust`
image PATH runs real cargo unchanged, then best-effort emits one
`build.env=factory` span per phase (compile/test/fuzz/fetch) to the host OTel
receiver, routed to `github-ci` by `service.name` (see
`telemetry-attribute-scheme.md`, Factory-routing correction). Non-fatal.

Remaining is a ROUTINE image rollout, NOT a code or console-edit task:
1. The master push already published the immutable image tag
   `python-rust-agent-sha-e070815` (image workflow green).
2. dev-tooling release **v1.37.0** is staged (release PR #1659, open) and carries
   the shim.
3. When v1.37.0 releases, the shared `bump-pin` release fan-out **automatically**
   rewrites the console's `.fabro/workflows/implement-work-item/workflow.toml`
   `python-rust-agent-vX.Y.Z` pin in lockstep (see that file's PIN SURFACE NOTE)
   — this is NOT a manual console edit, and a hand-written sha pin would fight the
   fan-out's semver format.
4. The next console fabro dispatch on the shimmed image then emits live
   `build.env=factory` spans; capture the cold baseline with
   `build.env=factory AND repo=<this repo>`, grouped by `build.phase` / `name`.

Inference until then (`research/003`): the factory builds fully cold every run
(2.3–6.9 GB `target/` thrown away with the container), and the on-host pre-push
`just check` was measured at 251–273 s on 4 vCPUs (2026-07-30).

## Status

- CI cold baseline: **captured and cited** (both queries above).
- Local baseline: blocked on a keyed local run (host).
- Factory baseline: emitter shipped (#1658); waits on the v1.37.0 image rollout
  (auto-pin-bump) + one real dispatch.

fhdzka stays open until all three legs are recorded; this note is the anchor and
is updated in place as the local and factory legs land.
