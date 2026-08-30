# 005 — Honeycomb build-time telemetry: what exists, what's missing

Verified 2026-08-30 against the live Honeycomb workspace (team `thewoolleyweb`,
environment `livespec`) via the Honeycomb MCP tools, and by reading the
exporter source.

## What exists today

**CI (partial).** `.github/workflows/ci.yml` job `export-telemetry` runs
`.github/scripts/export-ci-telemetry.sh` with
`HONEYCOMB_GITHUB_CI_INGEST_KEY_LIVESPEC`, emitting OTLP/HTTP-JSON spans to
dataset **`github-ci`** (service.name), namespace `livespec-family`, endpoint
`api.honeycomb.io/v1/traces`. Confirmed live: the dataset receives one
`ci.run` root span + one `ci.job.<name>` child per completed job, with columns
`ci.run_id`, `ci.job.name`, `ci.job.conclusion`, `ci.conclusion`, `ci.event`,
`git.commit.sha`, `git.branch`, `repo`, `duration_ms`, and (recently)
`ci.job.queue_ms`. The export is a closed loop — it fails the job if Honeycomb
rejects the payload.

Limits, from the script header and wiring (both deliberate at the time):

- **push(master)/merge_group only — PR runs emit NOTHING.** Optimization work
  iterates on PRs, so before/after deltas would be invisible until merge.
- **Job-level only.** No step/phase granularity: a job's compile time is not
  separable from its test/fuzz/floor time, and requirement 1 needs exactly that
  split for `check-fuzz` (~136 s build vs 180 s ratified floor) and
  `check-nextest`/`check-coverage`.
- No cache-state attributes (there is no cache yet) — after tiers land, spans
  need `build.cache.*` markers to prove hit-vs-miss deltas.

**Factory (prepare phases only).** Dataset **`fabro-sandbox`** receives
`prepare.*` spans (`prepare.mise-install`, `prepare.uv-sync`,
`prepare.fetch-unshallow`, `prepare.commit-refuse-install`, …) — so a working
span-emission seam exists in the sandbox prepare path — but **no cargo
build/check/test spans at all.** Datasets `fabro`, `livespec-dispatcher`,
`agent-hooks`, `claude-code`, `metrics` etc. also exist in the environment;
none carry console build durations.

**Local: nothing.** No dataset receives local build/`just check` timings.

## What Phase 1 must add (the measurement substrate)

1. **CI PR coverage**: emit job spans for PR runs too (either extend
   `export-ci-telemetry.sh`'s trigger, or per-job in-workflow emission). Keep
   the closed-loop verification property for the master path.
2. **CI phase split**: for the critical-path jobs, emit child spans (or timing
   attributes) separating compile from test/fuzz — e.g. wrap the cargo build
   step and the run step separately. Candidate: `cargo build --timings` JSON +
   a small exporter, or plain per-step `date +%s%N` bracketing like the
   existing script.
3. **Factory build spans**: extend the existing `prepare.*` emission seam to
   the build/check phases of the run (`build.env=factory`), so each fabro run's
   cold compile is a measured span.
4. **Local emission**: a thin exporter for `just check` / pre-push aggregate
   (and optionally raw `cargo build`) durations, `build.env=local`. Must be
   fail-soft (no network ⇒ no failed build) — the OPPOSITE of the CI
   closed-loop property, and deliberately so.
5. **Shared attribute scheme** across all three: `build.env`
   (local|factory|ci), `build.phase` (fetch|compile|test|fuzz|link…), `repo`,
   `git.commit.sha`, toolchain version, and once caches exist
   `build.cache.tier` + hit/miss — so one query answers "before vs after, per
   env, per phase".
6. **Recorded BEFORE baselines**: after the substrate lands, capture the cold
   baselines per env as Honeycomb query results and cite them in a research
   note; every Phase 2 optimization is judged against them (requirement 1),
   and the Phase 3 final report (raw + %) is composed from the same queries.

## Practical notes

- Ingest key: CI already holds `HONEYCOMB_GITHUB_CI_INGEST_KEY_LIVESPEC` as a
  repo secret; factory/local emission needs a key delivery decision (family
  env wrapper is the local candidate; the sandbox already emits `prepare.*`
  spans so the factory key path exists — find where that emission gets its key
  and reuse it).
- Dataset choice: keep `github-ci` for CI; factory build spans most naturally
  extend `fabro-sandbox`; local either joins a `build-times`-style dataset or
  reuses `github-ci` with `build.env` — worker's call, recorded when made.
