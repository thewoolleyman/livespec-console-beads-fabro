# delivery-path-speed-and-caching

## Origin

Maintainer directive, 2026-08-17, raised while watching a plan-01
dogfood-leg live invoker walk stall on a slow PR CI run (PR #666 for
livespec-console-beads-fabro-3lxx7t, 15 checks pending for an extended
window). Direct quote: "We need to be diligent about slowness in
factory and ci by aggressively optimizing and caching everything. We
have a hosted CI runner now, we can cache as much as we want."

## Scope

Investigate and catalog slowness across every delivery path this repo's
work actually travels, then identify concrete optimization/caching
opportunities for each:

1. **Local dev loop** -- `just check`, `cargo build`/`cargo test`,
   `mise exec` overhead, per-file coverage, lefthook/pre-commit hook
   cost.
2. **Factory dispatch (Fabro)** -- per-stage latency in the
   implement -> janitor -> review -> pr-view -> merge pipeline observed
   this session (multi-minute-to-tens-of-minutes runs), sandbox image
   provisioning cost, whether the `livespec-fabro-sandbox` docker image
   is rebuilt/pulled fresh per run or cached, npm/cargo dependency
   fetch cost inside the sandbox (the `@zed-industries/codex-acp`
   `--no-install` failure this session is a related but distinct
   defect -- see livespec-console-beads-fabro-9ts's blocked marker).
3. **CI (GitHub Actions / hosted runner)** -- now running on a HOSTED
   runner per the maintainer's note (previously implied ephemeral),
   which changes the caching calculus: persistent caches across runs
   become viable where they weren't before. Candidates to investigate:
   cargo registry/target caching, docker layer caching, dependency
   installs (uv/npm/cargo), test-matrix parallelism, redundant
   full-workspace rebuilds when only one crate changed.

## Evidence already in hand (from this session's live observation)

- PR #666 (livespec-console-beads-fabro-3lxx7t): 15 CI checks queued
  simultaneously, still pending after an extended wait -- worth pulling
  the actual per-check timing breakdown once it completes.
- Two independent PR-stage sandbox infra failures observed same
  session: -9ts (npm package `@zed-industries/codex-acp` not
  pre-cached in sandbox image, `--no-install` refuses to fetch) and
  -2ckgiy (GitHub App installation token hit a rate limit). Both point
  at the same theme: infra/caching gaps outside this repo's own code.
- Multiple `fabro`/dispatcher runs this session took 10-45+ minutes
  from dispatch to PR-open, largely appearing to be sandbox
  provisioning + build time rather than actual work-item complexity.

## Next action

Record a scoping event once requirement carriers and explicit
deferrals are identified (e.g., decide whether sandbox-image-level
fixes belong to this repo's plan or must be escalated/deferred to
whatever owns `livespec-fabro-sandbox`), then route ripe measurement
work into ledger children.
