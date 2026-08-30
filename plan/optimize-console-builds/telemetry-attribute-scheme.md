# Build-telemetry attribute scheme

Shared scheme for build-time OTLP spans emitted from CI, factory, and local
environments. Every emitter calls `.github/scripts/emit-build-telemetry.sh`
with the required attributes below; optional attributes are added when the
corresponding capability exists (e.g. cache tiers in Phase 2+).

Defined as part of `optimize-console-builds` Phase 1; grounded in the gap
analysis at `research/005-honeycomb-telemetry-gap.md` and the charter at
`research/001-charter-and-measurement-plan.md`.

## Required attributes

| Attribute | Type | Description |
|---|---|---|
| `build.env` | string | Execution environment: `local` \| `factory` \| `ci` |
| `build.phase` | string | Build phase: `fetch` \| `compile` \| `test` \| `fuzz` \| `link` |
| `repo` | string | `owner/name`, e.g. `thewoolleyman/livespec-console-beads-fabro` |
| `git.commit.sha` | string | Full 40-character commit SHA |
| `toolchain.version` | string | Rust toolchain version from `rust-toolchain.toml`, e.g. `1.92.0` |

## Optional attributes (populate when capability exists)

| Attribute | Type | Description |
|---|---|---|
| `build.cache.tier` | string | Cache tier in use: `none` \| `registry` \| `target` |
| `build.cache.hit` | bool | `true` when the tier was warm; `false` when cold |

## Allowed `build.env` values

- `local` — maintainer/agent session on the `vps` dev host
- `factory` — fabro dispatch sandbox run on `hp-xubuntu`
- `ci` — GitHub Actions self-hosted ARC runner on `poweredge-xubuntu`

## Allowed `build.phase` values

- `fetch` — dependency fetch / crate registry download (e.g. `cargo fetch`)
- `compile` — Rust compilation (`cargo build`, including proc-macro expansion)
- `test` — test execution (`cargo nextest run`, `cargo test`)
- `fuzz` — fuzzing run (`cargo fuzz`; ASAN build + libFuzzer execution)
- `link` — link phase when measurable separately (e.g. via `cargo build --timings`)

## Dataset routing decision

All three environments emit to the **`github-ci`** Honeycomb dataset. The
`build.env` attribute discriminates in queries, so one query covers all
environments and before/after deltas need no cross-dataset joins.

| Environment | Dataset | Ingest key variable |
|---|---|---|
| CI | `github-ci` | `HONEYCOMB_BUILD_INGEST_KEY` ← `HONEYCOMB_GITHUB_CI_INGEST_KEY_LIVESPEC` (existing repo secret) |
| Factory | `github-ci` | `HONEYCOMB_BUILD_INGEST_KEY` — family env wrapper injection |
| Local | `github-ci` | `HONEYCOMB_BUILD_INGEST_KEY` — family env wrapper injection |

The factory's existing `prepare.*` spans (in `fabro-sandbox`) are unchanged;
only the new build/check phase spans route here.

The `github-ci` choice over a dedicated `build-times` dataset: the existing
dataset already receives `repo` and `git.commit.sha` from the CI `ci.run` spans,
and cross-linking the build phase spans to their CI run context is the most
direct path to before/after queries over the same traces. A single dataset also
avoids divergent retention policies.

## Failure contract per environment

- **CI** — fail-hard: `emit-build-telemetry.sh` exits non-zero on ingest
  failure, reddening the job. Matches the closed-loop property of
  `export-ci-telemetry.sh`.
- **Factory** — fail-hard: emission failures surface immediately in the run log.
- **Local** — fail-soft: set `BUILD_EMIT_FAIL_SOFT=1` so network absence or
  ingest errors do not abort `just check`. This is the OPPOSITE of the CI
  property, and deliberately so (local builds must never block on telemetry).

## Emission helper

`.github/scripts/emit-build-telemetry.sh` — accepts the scheme attributes as
environment variables and POSTs a single OTLP/HTTP-JSON span to Honeycomb.
See the script header for the full interface contract.

Optional trace correlation: supply `BUILD_TRACE_ID` (32 hex chars) to place the
span in a specific trace, and `BUILD_PARENT_SPAN_ID` (16 hex chars) to nest it
under a parent span. When absent, both are generated randomly; each invocation
is an independent trace.
