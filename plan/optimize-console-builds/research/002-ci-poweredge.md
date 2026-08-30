# 002 — CI environment: poweredge-xubuntu (k3s/ARC self-hosted runner)

All numbers measured live 2026-08-30 over `ssh cwoolley@poweredge-xubuntu`
(tailscale 100.78.140.72) unless noted.

## Host

- **72 cores**, kernel 7.0.0-30-generic, k3s v1.36.2+k3s1, **188 GiB RAM**
  (~127 free at measurement), 7 G swap.
- Cache volume `/dev/sda5` = 658 G, **37 G used, 587 G avail (6%)**, mounted
  `/var/cache/ci-runner` (noatime). k3s local-path PVC storage bind-mounts onto
  it (`/var/lib/rancher/k3s/storage` → `/dev/sda5[/k3s-storage]`); the growth
  from an earlier 14 G reading is `k3s-containerd/` + `k3s-storage/`, not warm
  cache. Space is NOT a constraint on this host.
- `/var/cache/ci-runner/` holds only `k3s-containerd`, `k3s-storage`,
  `lost+found`, `warm`. `warm/` = `src` (212 M repo clones), `uv` symlink,
  `uv-generations` (**1.2 G, exactly 2 generations**). **No cargo registry
  cache and no compiled `target/` cache exist anywhere on the host — every
  Rust CI build is fully cold.** Confirmed twice (2026-08-30).
- Runner concurrency: ARC `containerMode: kubernetes`; console scale set config
  at `livespec-dev-tooling/ci-runner/k3s/phase2/arc/values-livespec-console-beads-fabro.yaml`
  (`minRunners: 0`, `maxRunners: 16`, Kueue nominalQuota 1 with cohort
  borrowing, host-wide churn-slot capacity C=16). ~17 runner pods observed for
  one scale set during the check; 10 scale-set listeners in `arc-systems`.
- Work volume: `kubernetesModeWorkVolumeClaim`, RWO `local-path` PVC per
  runner, ephemeral, discarded with the runner. Its
  `resources.requests.storage: 5Gi` is a **dead value** — the local-path
  provisioner is `mkdir`-only, no quota; active-run space can grow to the full
  volume. The plan should correct/clean up this misleading 5Gi.
- Node timers are all CI-ops (`archive-arc-logs`, `scan-wedged-runners`,
  `ci-runner-heartbeat`, `ci-kueue-webhook-probe`,
  `reapply-node-extended-resource`) — **no cache-cleanup cron exists at node
  level; the only cache eviction on the host is the uv populator's newest-2
  generation prune.**

## Job graph and cold-baseline durations

`.github/workflows/ci.yml` (~522 lines): **16 self-hosted jobs, all fully
parallel** (no inter-job `needs` except the two hosted tail jobs) — a 12-target
`check` matrix (format, clippy, nextest, coverage, deps, arch,
behavior-coverage, completeness, baseline, shell-quality, plan-no-tombstone,
plugin-resolution) in container `ghcr.io/thewoolleyman/livespec-fabro-sandbox:python-rust-v1.36.0`,
plus `check-doctor-static`, `check-e2e-tmux` (release-binary build),
`check-mutants` (diff-scoped), `check-fuzz`. `ci-green` (ubuntu-latest) is the
sole required context; `export-telemetry` (push-only) runs last.
`taiki-e/install-action` provides prebuilt nextest/llvm-cov/deny/machete.

Per-job durations, two recent green master runs (`gh run view --json jobs`,
runs 33295026519 and 33292108349), seconds:

| Job | run …26519 | run …08349 |
|---|---|---|
| check-nextest | **416** | **366** |
| check-fuzz | 371 | 335 |
| check-deps | 326 | 347 |
| check-coverage | 320 | 327 |
| check-e2e-tmux | 307 | 263 |
| check-clippy | 301 | 117 |
| check-completeness | 281 | 142 |
| check-baseline | 270 | 224 |
| check-shell-quality | 265 | 170 |
| check-arch | 148 | 300 |
| check-behavior-coverage | 190 | 228 |
| check-format | 176 | 267 |
| check-plan-no-tombstone | 182 | 147 |
| check-plugin-resolution | 105 | 278 |
| check-doctor-static | 130 | 94 |
| check-mutants | 100 | 149 |

**Critical path = slowest parallel job: `check-nextest` (366–416 s), with
`check-fuzz` (335–371 s) and `check-deps` close behind.** `check-fuzz` = ~136 s
cold ASAN build of 3 fuzz targets + libFuzzer-from-source, plus the ratified
180 s fuzzing floor (3×60 s, NOT reducible) — so a fully-warm `check-fuzz`
bottoms at ~200 s, below `check-nextest`. **Caching that helps only the fuzz
job cannot shorten the run below ~370 s; the win requires warming the workspace
compile shared (under differing profiles) by nextest/coverage/clippy/deps
too.** Large run-to-run variance (clippy 301 vs 117) is contention noise from
16 concurrent jobs at `build.jobs = 4` each on 72 cores.

## The deleted-cache decision is UNPROVEN

`.github/workflows/ci.yml` deleted all `actions/cache` steps on the self-hosted
cutover (commit d67a2d6, 2026-07-17). Its justification (workflow comment,
lines ~178–189) measured *cold-on-this-large-host vs
warm-on-a-2-4-core ubuntu-latest hosted runner* (clippy 37 s cold-here vs 53 s
warm-there; test 35 s vs 48 s) — a broken comparison conflating cold-vs-warm
with big-host-vs-small-host, and it never measured the fuzz ASAN build at all.
Treat "cargo caching buys nothing here" as UNPROVEN; Phase 1 re-measures
warm-vs-cold ON THE SAME HOST.

## `.cargo/config.toml` parallelism cap

The repo caps `build.jobs = 4` and `RUST_TEST_THREADS = 4` so 16 concurrent
matrix jobs (≈64 threads) roughly fill the 72-core host without
oversubscription. A job-scoped raise (e.g. `CARGO_BUILD_JOBS=12` for
`check-fuzz`/`check-nextest` only) is the cheapest experiment on the table —
see note 006 §g for the safety arithmetic; a blanket raise is NOT safe.

## Cache-mount seam

The ARC hook-pod-template
(`livespec-dev-tooling/ci-runner/k3s/phase2/arc/hook-pod-template.yaml`,
injected via `ACTIONS_RUNNER_CONTAINER_HOOK_TEMPLATE`) is the proven seam for
getting cache into job pods: warm root hostPath-mounted READ-ONLY, postStart
copy onto the pod's work volume before step 1, fail-soft. Extending it keeps
the design's trust tiering (one trusted writer; jobs can never write the shared
cache). A read-write hostPath or hand-made static PV would break that tiering —
see note 006 §c/§d for the tradeoff record.
