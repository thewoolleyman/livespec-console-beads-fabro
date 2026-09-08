# 012 — Improvement ledger, and the observability holes it exposes

`research/011` is the approved Phase 3 report (charter `research/001`
requirement 3). **This note is a different audit**, asked for by the maintainer
on 2026-09-08: one row per thing this plan improved, with the raw numbers AND
the percentages, and — wherever an AFTER cannot be read from telemetry — an
explicit **HOLE** with a work item filed under this plan to fill it.

The rule this note applies, as given:

- A missing **BEFORE** is acceptable. The row is left blank and says so.
- A missing **AFTER** is **not** acceptable. It means charter requirement 1
  ("every optimization proven by a measured Honeycomb before→after") is unmet
  for that row, whatever the change did in practice. Each one is filed as a
  child of this plan's epic and listed in §6.

## Method

- **Source:** Honeycomb team `thewoolleyweb`, environment `livespec`, dataset
  `github-ci` (CI `ci.job.*` / `build.check-*` spans, factory
  `build.cargo-*` spans, local `build.local.*` spans) and `fabro-sandbox`
  (`prepare.*` spans). Every AFTER cell names the query it came from.
- **BEFORE** is `research/007` throughout — the charter's cold baseline
  (7-day window, 91 runs per job). `research/008`'s Layer-2 table is a
  different window and reads a few percent apart; where `research/011` cited
  it, this note re-states the delta against 007 so all rows share one baseline.
- **Percentages** are (AFTER − BEFORE) / BEFORE at the stated percentile.
- **Ratified floors are excluded** from improvement claims: the 3×60 s fuzz run
  in `check-fuzz` is not reducible and is reported for context only.
- The poweredge disk swap (RAID-5 → dual NVMe) happened mid-plan and is **not**
  one of this plan's levers. It is reported separately in §1.4 because it
  dominates the CI numbers, and the plan's own CI levers are attributed against
  the post-NVMe substrate per `research/009`.

---

## 1. CI — the poweredge k3s self-hosted pool

### 1.1 Per-job wall time, P50 seconds

BEFORE: `research/007`, 91 runs per job. AFTER: the self-hosted NVMe window
2026-09-03T23:25Z → 2026-09-06T00:19Z, n = 20 runs per job
([94dxd82kfDd](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/94dxd82kfDd)).

| Job | BEFORE s | AFTER s | Δ s | Δ % |
|---|---|---|---|---|
| check-fuzz | 403 | 316 | −87 | −21.6 % |
| check-nextest | 266 | 64 | −202 | **−75.9 %** |
| check-e2e-tmux | 256 | 125 | −131 | −51.2 % |
| check-coverage | 244 | 87 | −157 | −64.3 % |
| check-deps | 188 | 37 | −151 | **−80.3 %** |
| check-clippy | 185 | 52 | −133 | −71.9 % |
| check-arch | 157 | 52 | −105 | −66.9 % |
| check-behavior-coverage | 150 | 35 | −115 | −76.7 % |
| check-baseline | 148 | 34 | −114 | −77.0 % |
| check-completeness | 147 | 44 | −103 | −70.1 % |
| check-plan-no-tombstone | 140 | 31 | −109 | −77.9 % |
| check-shell-quality | 136 | 32 | −104 | −76.5 % |
| check-format | 127 | 28 | −99 | −78.0 % |
| check-mutants | 125 | 67 | −58 | −46.4 % |
| check-plugin-resolution | 123 | 29 | −94 | −76.4 % |
| check-doctor-static | 105 | 46 | −59 | −56.2 % |
| **all jobs** | **170** | **55.8** | **−114.2** | **−67.2 %** |

No job regressed.

### 1.2 The same jobs re-read today, over a much larger sample

2026-09-05T00Z → 2026-09-08T05Z, self-hosted, **n = 211 runs per job**
([oqWHXpG1xiL](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/oqWHXpG1xiL)). This window includes `ydlant`'s seeded ASAN
tree, which landed after the window in §1.1.

| Job | BEFORE s | AFTER s (n=211) | Δ s | Δ % |
|---|---|---|---|---|
| check-fuzz | 403 | 223 | −180 | −44.7 % |
| check-e2e-tmux | 256 | 135 | −121 | −47.3 % |
| check-coverage | 244 | 85 | −159 | −65.2 % |
| check-nextest | 266 | 68 | −198 | −74.4 % |
| check-mutants | 125 | 68 | −57 | −45.6 % |
| check-clippy | 185 | 58 | −127 | −68.6 % |

`check-fuzz` is the row that moved between the two windows (316 → 223 s):
that is `ydlant`. The others sit a few seconds above §1.1 on a busier pool,
which is the same load sensitivity §2 records for the factory.

### 1.3 Compile / test phase spans, P50 seconds

BEFORE: `research/007` phase table. AFTER: [jBLMJTSTsMp](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/jBLMJTSTsMp)
(n = 21) and, for the fuzz compile, the post-seed window
[hBdGTVBtYWv](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/hBdGTVBtYWv) (n = 21).

| Phase span | BEFORE s | AFTER s | Δ s | Δ % |
|---|---|---|---|---|
| build.check-fuzz.compile | 78 | 4 | −74 | **−94.9 %** |
| build.check-nextest.compile | 66 | 20 | −46 | −69.7 % |
| build.check-clippy.compile | 42 | 26 | −16 | −38.1 % |
| build.check-nextest.test | 13 | 14 | +1 | +7.7 % (flat) |
| build.check-fuzz.fuzz | 188 | 190 | +2 | ratified floor, excluded |

### 1.4 Substrate: the disk swap (context, not a plan lever)

`research/008`, `fio` 3.41, identical flags:

| Measure | BEFORE (RAID-5 HDD) | AFTER (dual NVMe, XFS) | Δ % |
|---|---|---|---|
| random write 4k, qd32×4 | 3,389 IOPS | 485,000 IOPS | +14,213 % (~143×) |
| random read 4k, qd32×4 | 15,494 IOPS | 777,000 IOPS | +4,915 % (~50×) |
| sequential write 1M | 216 MiB/s | 3,299 MiB/s | +1,427 % (~15×) |
| p99 write latency | 127 ms | 0.31 ms | −99.8 % |

The clean pure-substrate isolators are the jobs that compile no Rust:
`check-shell-quality` −76.5 % and `check-doctor-static` −56.2 %. Every Rust
job's delta in §1.1 is substrate **plus** the levers in §1.5.

### 1.5 Per-optimization CI rows

| # | Optimization | Item | Measure | BEFORE | AFTER | Δ % | Query |
|---|---|---|---|---|---|---|---|
| 1 | `CARGO_BUILD_JOBS=12` on the nextest/fuzz compile phases | `zzfntv` (#935) | build.check-nextest.compile P50 | 66 s | 20 s | −69.7 % | [jBLMJTSTsMp](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/jBLMJTSTsMp) |
| 2 | same, on the ASAN compile | `zzfntv` | build.check-fuzz.compile P50 | 78 s | 75 s | −3.8 % | same |
| 3 | Cargo registry served by the crates proxy (this plan's own generation superseded and closed as consumer) | `wki5zf` | check-deps job wall P50 | 188 s | 37 s | −80.3 % | [94dxd82kfDd](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/94dxd82kfDd) |
| 4 | Fuzz-capable sandbox image (g++, nightly, cargo-fuzz baked) | `gqmtwa.1` (#974) | check-fuzz wall P50, image-only window n=8 | 316 s | 282 s | −10.8 % | [81HePYWZh36](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/81HePYWZh36) |
| 5 | Warmed ASAN `target/` generation (reflink seed + keyed mtime restore) | `ydlant` (#982) | build.check-fuzz.compile P50, n=21 | 78 s | 4 s | −94.9 % | [hBdGTVBtYWv](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/hBdGTVBtYWv) |
| 6 | Rows 4 + 5 together, end to end | `gqmtwa.1`+`ydlant` | check-fuzz wall P50, n=211 | 403 s | 223 s | −44.7 % | [oqWHXpG1xiL](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/oqWHXpG1xiL) |
| 7 | Drop the per-job `mise trust` steps | `gqmtwa.2` (#974) | the mise setup step, mean seconds per job | 3.2 s/job (51 s across 16 jobs) | 2.4 s/job (38 s across 16 jobs) | −25.5 % of that step; **−13 s per run** | **HOLE H7** — not in Honeycomb; read by hand from the forge API, one run each side (34018240842 → 34034390394) |
| 8 | Per-PR concurrency group (cancel superseded runs) | `s3kwxt` (#936) | share of genuinely-superseded PR runs that were cancelled | 1 of 2 (50 %) | 2 of 2 (100 %) | **n = 2 per side — not acceptance-grade** | **HOLE H1** — not in Honeycomb; derived by hand from forge run timestamps |
| 9 | tmux e2e harness readiness + ceilings | `pis7qu` | check-e2e-tmux failure rate | *(blank)* | *(blank)* | **not measured** | **HOLE H1** — job outcome is absent from telemetry, so a flake rate cannot be computed at all |
| 9b | same item, duration side | `pis7qu` | check-e2e-tmux wall P50, n=211 | 256 s | 135 s | −47.3 % | [oqWHXpG1xiL](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/oqWHXpG1xiL) |
| 10 | Warmed dev/test `target/` generation | `z2siyn` → `ydlant` bullet 1 | — | — | — | — | **dropped by measurement**, not deferred: `research/009` bounded the remaining headroom at ≤ 20 s/job once sccache took nextest compile to 20 s |

#### Row 8 deserves its own paragraph: `s3kwxt`'s effect is unproven

This is the first time the per-PR concurrency group has been measured, and the
result does not support its acceptance yet.

- The block is on `master` and correctly shaped: PR runs are keyed by `github.ref`
  with `cancel-in-progress` on, master runs are keyed per-SHA and never cancel
  each other — exactly what the item specified.
- Raw cancellation rates went the *wrong* way: cancelled PR runs were 3 of 74
  (4.1 %) before and 2 of 157 (1.3 %) after.
- The honest metric is narrower. A run is only superseded when a later run on
  the same branch starts **before it finishes**; most same-branch re-runs here
  follow a completed failure, which supersedes nothing. By that definition there
  were **2 superseded runs before (1 cancelled) and 2 after (both cancelled)**.

So the mechanism looks right on every case it has actually faced, and the sample
is two runs per side. The motivating incident (five PRs re-pushed at once on
2026-09-02) has simply not recurred. **This is not evidence the item works; it is
evidence the item has barely been exercised.** Accepting it on this basis is a
judgement call for the maintainer, and it is the reason H1 is filed at P1 — with
job-outcome telemetry, this becomes a standing query instead of a hand count.

---

## 2. Factory — hp-xubuntu fabro sandboxes

Per-`cargo`-invocation spans (one dispatch invokes cargo many times), so cold
full compiles show in P95/MAX rather than P50. BEFORE: `research/007` factory
table (891 spans, v1.37.1 image). AFTER: the low-load window
2026-09-07T08–22Z, 13 dispatches, n = 275, `exit_code = 0`
([7JwDJxt5hnH](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/7JwDJxt5hnH)); the baked-registry column is `qxjdan`'s own
AFTER ([4NtJSPMz3xb](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/4NtJSPMz3xb)).

| Measure | BEFORE | after baked registry (`qxjdan`) | after sccache (`di6fn5`) | Δ % vs BEFORE |
|---|---|---|---|---|
| build.cargo-build P95 | 49.75 s | 36.0 s (−27.6 %) | **8.9 s** | **−82.1 %** |
| build.cargo-build MAX | 50.40 s | 47.4 s (−5.9 %) | 49.9 s | −1.0 % (one cold compile still runs) |
| build.cargo-clippy P95 | 9.53 s | — | 5.4 s | −43.3 % |
| build.cargo-clippy MAX | 23.01 s | — | 5.7 s | −75.2 % |
| build.cargo-llvm-cov P95 | 29.07 s | — | 27.9 s | −4.0 % |
| build.cargo-llvm-cov MAX | 61.51 s | — | 42.5 s | −30.9 % |
| build.cargo-test P50 | 7.82 s | — | 1.8 s | −77.0 % |
| build.cargo-nextest P50 | 3.14 s | — | 12.3 s | **+291.7 % (regression)** |
| sccache hit rate | 0 % (no cache) | 0 % | **84 %** (2,489 hits / 483 misses, 839 spans, 27 dispatches) | — |
| crates.io download per run | every run | none (baked, 298 MB) | none | −100 % |

**The nextest regression is real and explained, not noise:** test-binary linking
dominates that invocation and is not a cacheable compile — nextest's own hit
rate is 21 % (4 hits / 15 misses) against clippy's 98 %
([hNXCW91pebd](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/hNXCW91pebd)). sccache wins where it can hit and is
neutral-to-negative where it cannot.

**Load sensitivity, not a cache effect.** Over the full window to 09-08T05Z
(27 dispatches, n = 616, [cw38qzGhqfJ](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/cw38qzGhqfJ)) every P95 sits ABOVE
the BEFORE column — cargo-build P95 85.9 s, cargo-test 68.8 s — on the same
image with the same 84 % hit rate, during the busiest factory window since
telemetry began ([sJUThPUL9vV](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/sJUThPUL9vV)). Filed upstream as
orchestrator `bd-ib-9zg1` (bound per-host factory concurrency: hp is 16 cores,
each sandbox is capped at 4 CPU, and cargo inside still sees 16).

**Two factory rows have no AFTER at all** and are filed: there is no per-run
wall time (**HOLE H2**) and prepare-phase timing has stopped landing
(**HOLE H3**).

---

## 3. Local — the vps

The charter's local story is eviction, not speedup. All seven data points were
hand-emitted on 2026-09-02 under the family wrapper
([fRJyXteJsKg](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/fRJyXteJsKg)); 18-core host under concurrent load, so the
ratios are the finding and the absolute numbers are pessimistic.

| Measure | Item | BEFORE | AFTER | Δ | Δ % |
|---|---|---|---|---|---|
| cold `cargo build --workspace` | `vhtfpe` (#941) | 68.5 s (jobs=4) | 34.7 s (jobs=16) | −33.8 s | **−49.3 %** |
| warm incremental, `console-cli` touched | `vhtfpe` | 1.7 s | 1.2 s | −0.5 s | −29.4 % |
| warm incremental, `console-domain` touched | `vhtfpe` | 3.3 s | 3.2 s | −0.1 s | −3.0 % |
| warm no-op | `vhtfpe` | — | 0.7 s | — | (no BEFORE) |
| `~/.rustup` on disk | `uybgug` (#940) | 4.9 G | 3.0 G | −1.9 G | −38.8 % |
| `~/.cargo/registry` on disk | `uybgug` | 1.4 G | 1.1 G | −0.3 G | −21.4 % |
| primary + worktree `target/` | `uybgug` | 12 G + ~4.1 G each | unchanged | 0 | 0 % — correct: nothing older than 14 d, all hot |
| warm no-op after an eviction pass | `uybgug` | 17.3 s (`research/004`) | 12.4 s | −4.9 s | −28.3 %, **not claimed** (different day and load; recorded to show the hot cache survived) |

Every local number above is a **single observation**. There is no local
telemetry stream (**HOLE H4**).

---

## 4. Enabling work — delivered, and not a duration row by nature

These built the measurement substrate every table above is read from, so they
have no before→after of their own.

| Item | What it delivered |
|---|---|
| `ewzknf` | the shared build-telemetry attribute scheme + emitter helper |
| `577xhi` | CI PR-run span coverage + the compile-vs-test phase split |
| `icmvza` | the `Phase:` step split on the critical-path jobs (the source of §1.3) |
| `2h5kes` | wired `ci.yml` to activate the PR export + phase spans |
| `iqulbh` | local fail-soft emission (`build.env=local`) |
| `2er6nc` | factory sandbox cargo spans (`build.env=factory`) |
| `fhdzka` | captured and recorded the cold BEFORE baselines |
| `2dnpq3` | fixed the k3s lane's OTLP endpoint so build-phase spans reach Honeycomb at all |
| `o36w` | fixed the hosted-runner endpoint ternary (the pod URL was in the wrong branch) |
| `elt9` | `build.env=factory` / `build.phase` now present on factory spans — verified 2026-09-08 over 1,759 spans and closed |

---

## 5. Eviction bounds (charter requirement 2): asserted vs observed

| Tier | Policy shipped | Bound observed | How it was verified |
|---|---|---|---|
| CI warmed `target/` generations | newest-2 per key, pruned by the populator; key miss → cold, never a mismatched tree | 264 MB per generation | one-off `du` |
| CI crates proxy + sccache tier | populator is the sole writer; jobs `READ_ONLY` | 4,371 hits / 3,000 misses over 128 jobs, 0 errors | one query, 2026-09-06 |
| Factory image store | `docker-prune.timer`: `image prune -a --filter until=72h`, daily | +56.8 MB compressed per image | read the timer once |
| Factory sccache redis | 4 GB `allkeys-lru`, no persistence (explicit LRU acceptance recorded on `di6fn5`) | 910 keys, 977 MB used, 3,133 hits / 979 misses | one `redis-cli` spot check, 2026-09-08 |
| Local caches | age/staleness only, never size: `just local-cache-evict [--days N]`, default 14 | first pass freed ~2.2 G | one-off `du` |

**Every cell in the right-hand column is a spot check.** No tier emits its size
or its oldest-generation age, so a bound breach would be invisible until a disk
filled (**HOLE H5**).

---

## 6. The observability holes, and the items filed

| # | Hole | Why it matters here | Item |
|---|---|---|---|
| H1 | CI job/run **outcome** is not telemetered: `ci.conclusion` is absent from all 7,570 `ci.job.*` spans and empty on 451 of 457 `ci.run` spans ([3uEU3NC5Fgm](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/3uEU3NC5Fgm), [g3juvnRv8vN](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/g3juvnRv8vN)) | `s3kwxt` and `pis7qu` have no Honeycomb AFTER; and no table in this plan can show whether a speed win cost reliability | `livespec-console-beads-fabro-gqmtwa.4` |
| H2 | No factory **per-run wall time** — only per-cargo-invocation spans exist | the factory leg has no counterpart to CI's per-job table; flagged in `research/007` at baseline time and never filled | `livespec-console-beads-fabro-gqmtwa.5` (proxy for orchestrator `bd-ib-cp2x`) |
| H3 | Factory **prepare-phase** spans stopped landing: 8 spans since 2026-09-04T15:00Z against 27+ dispatches ([DRncvSG2uhn](https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/fabro-sandbox/result/DRncvSG2uhn)) | sandbox setup cost is unobservable, so it cannot be optimized or ruled out | `livespec-console-beads-fabro-u6sx` |
| H4 | No **local** telemetry stream: 7 spans ever, all from one sitting | the local leg is one-shot; no trend, and the eviction effect is not tracked over time | `livespec-console-beads-fabro-xa2l` |
| H5 | **Cache size / eviction bounds** are asserted by spot check; no tier emits size or age | charter requirement 2 is verified by hand, once, per tier | `livespec-console-beads-fabro-fse7` |
| H6 | `build.cache.sccache.hit_ratio` ships as **0** on every span (and `backend` as `unknown`) while hits/misses are populated | the acceptance metric of `di6fn5` reads 0 % when the real rate is 84 %; the AFTER had to be derived from the counters | `livespec-console-beads-fabro-opdv` |
| H7 | **Setup-step timing** is not exported — only `Phase:`-named steps become spans | `gqmtwa.2` has no measurement of its own, and the fuzz image's step savings were read from one job's log | `livespec-console-beads-fabro-p5ci` |

---

## 7. Scorecard

Counted by **work item that shipped a change**, not by table row.

| Disposition | Count | Items |
|---|---|---|
| AFTER measured **in Honeycomb** | 8 | `zzfntv`, `wki5zf`, `gqmtwa.1`, `ydlant`, `qxjdan`, `di6fn5`, `vhtfpe`, `pis7qu` (duration half only) |
| AFTER measured, but **outside Honeycomb** (forge API or `du`) | 2 | `gqmtwa.2` (forge step timestamps), `uybgug` (disk `du`) |
| AFTER **not acceptance-grade** | 1 | `s3kwxt` (n = 2 superseded runs per side, by hand) |
| AFTER **absent entirely** | 1 | `pis7qu` flake-rate half |
| Shipped no production change by design | 1 | `z2siyn` (spike; its go/no-go became `ydlant`) |

| Charter requirement | State |
|---|---|
| Req 1 — every optimization proven by a Honeycomb before→after | **met for 8 of 11** shipping items; three read from outside Honeycomb or not at all |
| Req 2 — bounded, age-based eviction on every tier | policies **shipped** on all five tiers; **observation missing on all five** (H5) |
| Req 3 — final report + human approval | met: `research/011`, approved 2026-09-06, amended 2026-09-08 |

| Environment | Coverage |
|---|---|
| CI | ✅ per-job and per-phase, continuous |
| Factory | ⚠️ per-invocation only — no per-run wall (H2), no setup timing (H3) |
| Local | ⚠️ one-shot — 7 spans, one sitting (H4) |

**Effect on the plan's lifecycle.** The seven items in §6 are children of epic
`livespec-console-beads-fabro-gqmtwa`, so the archive gate (no undisposed
children) now holds until they close or are explicitly disposed. That is the
correct consequence rather than an accident: this plan's charter is measurement,
and a lever whose effect cannot be seen is not finished. The alternative —
archiving with the holes recorded and these items re-parented to a successor
plan — is the maintainer's call, not this session's.

**What this note is not.** It does not re-litigate `research/011`, which the
maintainer approved on 2026-09-06 and which stands as the Phase 3 report. Every
number here is either that report's, re-based onto `research/007` so one
baseline is used throughout, or a new reading taken on 2026-09-08 and cited to
its query.
