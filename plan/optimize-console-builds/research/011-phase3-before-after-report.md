# 011 — Phase 3 report: build-time before → after, per environment, per optimization

The charter's requirement 3 (`research/001`): a report of overall improvements
in ALL affected areas, in raw seconds AND percent, per environment and per
optimization, from Honeycomb data, which a human APPROVES before the plan is
archived. This note is that report, composed 2026-09-06 by the plan session for
`livespec-console-beads-fabro-uocos3`. Every AFTER row cites the Honeycomb
query it was read from and the work-item whose close recorded it.

**Completeness at composition time.** The CI and local legs are complete. The
factory leg carries the baked-registry AFTER; its **sccache row is PENDING**
(`di6fn5`): the sccache layer shipped on `python-rust-agent-v1.52.2` at
2026-09-06T11:03Z, its AFTER needs ≥ 10 organic console dispatches on that pin,
and at composition there were 0 (Honeycomb `build.env = factory`, since 11:03Z:
0 spans). The maintainer may approve the report as it stands with the sccache
row to be appended by amendment, or hold approval until that row exists; the
approval comment on the plan epic records which.

## Method

- **Source of truth:** Honeycomb team `thewoolleyweb`, environment `livespec`,
  dataset `github-ci`, `repo = thewoolleyman/livespec-console-beads-fabro`.
  CI job spans are `ci.job.*`, CI phase spans `build.check-<job>.<phase>`,
  factory spans `build.cargo-*` (per cargo invocation), local spans
  `build.local.*`.
- **BEFORE** numbers are the cold baselines in `research/007` (CI per-job and
  phase split, factory per-invocation) and `research/008` (the RAID-5-era CI
  per-job table, Layer 2). **AFTER** numbers are the records each Phase-2 child
  wrote when it closed, re-read here from the same query family.
- **Hardware vs optimization deltas are kept separable** (`research/009`): the
  poweredge CI pool moved from a RAID-5 HDD array to two NVMe cards during
  Phase 2. That delta is reported on its own line, and every CI optimization is
  attributed against the post-NVMe window, not the RAID-5 one.
- Percentages are `(AFTER − BEFORE) / BEFORE` at P50 unless the row says
  otherwise. Ratified floors (the 3×60 s fuzz run) are excluded from
  improvement claims.

## CI — poweredge k3s pool (self-hosted, cold every run)

### Substrate change first: the disk (not an optimization of this plan)

`research/008`, `fio` 3.41, same flags before and after:

| Metric | BEFORE (RAID-5 HDD) | AFTER (dual NVMe, XFS work tier) | Δ |
|---|---|---|---|
| random write 4k, qd32×4 | 3,389 IOPS | 485k IOPS | ~143× |
| random read 4k, qd32×4 | 15,494 IOPS | 777k IOPS | ~50× |
| p99 random-write latency | 127 ms | 0.31 ms | −99.8 % |

The pure-NVMe isolators are the jobs that compile no Rust: `check-shell-quality`
136 → 32 s (−76 %) and `check-doctor-static` 98 → 46 s (−53 %). Everything a
Rust job gained in the same window is NVMe **plus** the levers below.

### Per-job wall time, P50 seconds (n = 20 runs per job)

BEFORE = `research/008` Layer-2 table (RAID-5 era). AFTER = self-hosted NVMe
window 2026-09-03T23:25Z → 2026-09-06T00:19Z, which also carries the
`zzfntv` jobs raise, dev-tooling's crates proxy and sccache tier
(https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/94dxd82kfDd).
The check-fuzz row continues below with this plan's two fuzz levers.

| Job | BEFORE | AFTER | Δ |
|---|---|---|---|
| check-fuzz | 398 | 316 | −21 % |
| check-e2e-tmux | 258 | 125 | −52 % |
| check-nextest | 256 | 64 | −75 % |
| check-coverage | 243 | 87 | −64 % |
| check-deps | 188 | 37 | −80 % |
| check-clippy | 184 | 52 | −72 % |
| check-arch | 151 | 52 | −66 % |
| check-behavior-coverage | 149 | 35 | −77 % |
| check-completeness | 147 | 44 | −70 % |
| check-baseline | 143 | 34 | −76 % |
| check-shell-quality | 136 | 32 | −76 % |
| check-plan-no-tombstone | 134 | 31 | −77 % |
| check-mutants | 124 | 67 | −46 % |
| check-format | 123 | 28 | −77 % |
| check-plugin-resolution | 121 | 29 | −76 % |
| check-doctor-static | 98 | 46 | −53 % |

No job regressed (the `zzfntv` ≤ 10 % other-jobs check passed on every row).

### Per-optimization rows (CI)

| Optimization | Item | Measure | BEFORE | AFTER | Δ | Query |
|---|---|---|---|---|---|---|
| `CARGO_BUILD_JOBS=12` on the compile phases of check-nextest / check-fuzz | `zzfntv` (#935) | build.check-nextest.compile P50 | 66 s | 20 s | −70 % (NVMe + sccache + raise combined; not separable) | [jBLMJTSTsMp](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/jBLMJTSTsMp) |
| same | `zzfntv` | build.check-fuzz.compile P50 | 78 s | 75 s | −4 % (ASAN compile is neither IO- nor parallelism-bound) | same |
| Cargo registry: crates proxy + sccache tier (dev-tooling; this plan's `wki5zf` superseded and closed as consumer) | `wki5zf` | check-deps job wall P50 | 188 s | 37 s | −80 % | [94dxd82kfDd](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/94dxd82kfDd) |
| Fuzz-capable sandbox image (`python-rust-fuzz`; g++, nightly, cargo-fuzz baked) | `gqmtwa.1` (#974, dev-tooling #1760) | check-fuzz job wall P50, image-only window 07:16–07:55Z, n = 8 | 316 s | 282 s | −34 s (−11 %); step-level: C++ install 13–16 s → 1 s, nightly + cargo-fuzz install 26–32 s → 0 s | [81HePYWZh36](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/81HePYWZh36) |
| Warmed ASAN `target/` generation (populator builds at the in-pod path, provisioner reflink-seeds it, `seed-fuzz-target.sh` restores keyed mtimes) | `ydlant` (#982, dev-tooling #1771/#1776) | build.check-fuzz.compile P50, n = 21 since 07:55Z | 78 s | 4 s | −95 % (P95 5 s, MAX 52 s = designed partial hit on a domain-crate PR) | [hBdGTVBtYWv](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/hBdGTVBtYWv) |
| Fuzz image + warmed tree together | `gqmtwa.1` + `ydlant` | check-fuzz job wall P50 / P95 / MAX, n = 21 | 316 s | 224 / 245 / 265 s | −29 % at P50; the job now sits ~30 s above its ratified 180 s fuzz floor (build.check-fuzz.fuzz P50 190 s) | [xaE3EmeUvD7](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/xaE3EmeUvD7) |
| Drop the per-job `mise trust` steps (pin ≥ v1.45.2 bakes `MISE_TRUSTED_CONFIG_PATHS`) | `gqmtwa.2` (#974) | hygiene; seconds per step, no separate window | — | — | folded into the gqmtwa.1 window | — |
| Warmed dev/test `target/` generation | `z2siyn` spike → `ydlant` bullet 1 | — | — | — | **dropped**, not deferred: `research/009` bounded its headroom at ≤ 20 s/job once sccache took check-nextest compile to 20 s | `research/010` |

End-to-end for the critical path: `check-fuzz`, the longest job on every PR,
went **398 → 224 s at P50 (−44 %)** across the whole plan, of which the disk
accounts for the first 82 s and this plan's two fuzz levers for the next 92 s;
`check-nextest` went **256 → 64 s (−75 %)**.

## Factory — hp-xubuntu fabro sandboxes (cold every run)

BEFORE = `research/007` factory table (v1.37.1 shimmed image, 891 spans,
per-cargo-invocation, so cold full compiles show in P95/MAX, not P50).

| Optimization | Item | Measure | BEFORE | AFTER | Δ | Query |
|---|---|---|---|---|---|---|
| Cargo registry baked into the `python-rust` image (`cargo fetch --locked` of the console lockfile, 298 MB; the host-mount design was impossible — fabro's docker provider has no mount knob) | `qxjdan` (dev-tooling `3u3gm2.1`) | build.cargo-build P95 / MAX, n = 24 since 2026-09-04 | 49.75 / 50.40 s | 36.0 / 47.4 s | −27.6 % / −5.9 % | [4NtJSPMz3xb](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/4NtJSPMz3xb) |
| same | `qxjdan` | build.cargo-test P50 / MAX, n = 3 | 7.82 / 61.67 s | 57.8 / 63.2 s | n too small, invocation mix differs; recorded as-is, no claim | same |
| sccache in the agent image (`sccache-or-rustc` wrapper) + `sccache-redis` on hp (172.17.0.1:6379, 4 GB) | `di6fn5` (dev-tooling #1773 + #1825 → v1.52.2; fabro-hosts #13) | build.cargo-build / -test P50 / P95 / MAX + `build.cache.sccache.hit_ratio`, exit_code = 0 rows, ≥ 10 organic dispatches on `python-rust-agent-v1.52.2+` | 36.0 / 47.4 s (post-registry) | **PENDING** — 0 dispatches on the pin at composition; the first feed dispatch (`ohtig5`) started 2026-09-06T11:56Z | — | to be cited on `di6fn5` at close |

The first run on the sccache layer (v1.49.1, before the wrapper fix) found the
probe defect recorded on `di6fn5` at 10:45Z: the verdict flipped to `unusable`
mid-run and no `build.cache.sccache.*` attribute reached any factory span. The
fix (dev-tooling #1825) is what v1.52.2 carries, so the window restarts from
that pin; nothing from the defective run is counted.

## Local — the vps (persistent `target/`, eviction was the goal)

The charter's local story is eviction, not speedup (`research/001`,
`research/004`); local telemetry is on-demand by maintainer decision
(`research/007`). The seven `build.local.*` spans below were emitted under the
family wrapper on 2026-09-02 (`vhtfpe`), 18-core host under concurrent load
(the ratio is the finding):
https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/fRJyXteJsKg

| Optimization | Item | Measure | BEFORE (jobs = 4, the committed cap) | AFTER (jobs = 16 via gitignored `.mise.local.toml`) | Δ |
|---|---|---|---|---|---|
| Local build parallelism override (documented in README "Local build parallelism") | `vhtfpe` (#941) | cold `cargo build --workspace` | 68.5 s | 34.7 s | −49 % |
| same | `vhtfpe` | warm incremental, `console-domain` touched | 3.3 s | 3.2 s | flat |
| same | `vhtfpe` | warm incremental, `console-cli` touched | 1.7 s | 1.2 s | −29 % |
| Age-based local cache eviction (`just local-cache-evict [--days N]`, default 14) | `uybgug` (#940) | `~/.rustup` on disk | 4.9 G | 3.0 G | −39 % (two toolchains last used 2026-08-13) |
| same | `uybgug` | `~/.cargo/registry` on disk | 1.4 G | 1.1 G | −21 % (383 archives referenced by no lockfile, unread > 14 d) |
| same | `uybgug` | primary + worktree `target/` | 12 G + ~4.1 G each | unchanged | correct: nothing older than 14 d, all hot |
| same | `uybgug` | warm no-op `cargo build --workspace` after the pass | 17.3 s (`research/004`) | 12.4 s | hot cache intact (not claimed as a speedup; different day and load) |

## Eviction policy and disk bound per tier (charter requirement 2)

| Tier | Eviction | Bound observed |
|---|---|---|
| CI warmed `target/` generations (`.warm/target/<repo>/asan-fuzz`) | newest-2 generations per key, pruned by the populator (`KEEP_GENERATIONS`); key = source sha × nightly × cargo-fuzz; key miss → cold, never a mismatched tree | 264 MB per generation; jobs own a reflink copy, so no job can write the shared tier |
| CI crates proxy + sccache tier (dev-tooling ci-runner-cache-tiers) | populator is the one writer; jobs are `READ_ONLY`; age-based generation model per the tier's README | measured 0 errors, 4,371 hits / 3,000 misses over 128 jobs ([8Vh2voqvSCF](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/8Vh2voqvSCF)) |
| Factory baked registry (image tags) | hp `docker-prune.timer`: `image prune -a --filter until=72h`, daily; N = 72 h so a live dispatch's tag stays hot; no size cap | +56.8 MB compressed per image; exited-container layers already bounded 24–48 h |
| Factory sccache (`sccache-redis` on hp) | `allkeys-lru`, 4 GB, no persistence — an explicit, reasoned LRU acceptance recorded in the fabro-hosts service README (every run is cold; the working set is one lockfile's object set, ~1–2 GB, under a cap sized so nothing hot evicts); a cache fault never changes cargo's exit code | 4 GB cap; hp RAM 24 GiB available after start |
| Local (`scripts/local-cache-evict.sh`) | age/staleness only, never size: gone-branch worktrees, `cargo sweep --time N`, registry archives referenced by no lockfile AND unread > N days, unused toolchains by libstd atime | first pass freed ~2.2 G; hot `target/` untouched |

## Enabling work counted, not measured

Phase 1 telemetry (`ewzknf`, `577xhi`, `icmvza`, `2h5kes`, `iqulbh`, `2er6nc`,
`fhdzka`) built the attribute scheme and the spans every row above is read
from; `2dnpq3` and `o36w` fixed the k3s and hosted-runner OTLP endpoints so the
spans arrive. Bugs adopted under this epic during Phase 2 but not build-time
optimizations: `pis7qu` (e2e harness ceilings), `1d5f` (eventstore
`stream_seq`, landed as #992), `s3kwxt` (per-PR CI concurrency group; in
`acceptance`, human-only).

## What the maintainer is asked to approve

1. The CI leg as complete: every job improved; the critical path
   (`check-fuzz`) is within ~30 s of its ratified floor; the dev/test warmed
   tree is dropped by measured headroom, not deferred.
2. The local leg as complete: parallelism override documented (−49 % cold),
   eviction shipped and age-based.
3. The factory leg with the baked-registry AFTER and the sccache row to be
   appended by amendment when ≥ 10 dispatches on v1.52.2+ exist — or held
   until then, at the maintainer's choice.

The approval (or the amendments required) is recorded as a comment on the plan
epic `livespec-console-beads-fabro-gqmtwa`; the plan archives only after that
comment exists (`research/001` requirement 3).
