# 008 — Pre-RAID-10 poweredge disk + CI baseline (future-reference snapshot)

> **ANNOTATION 2026-09-04 (v3) — NVMe VALIDATED; the comparison is TWO-PART
> (BEFORE → +NVMe). The RAID-5-only intermediate is SKIPPED, and the RAID-10
> rebuild this note originally anticipated was DROPPED (2026-09-02).** One
> WD_BLACK SN8100 4 TB NVMe is installed and the CI work-volume filesystem
> (`/dev/mapper/nvmea-ci--workvols` → `/var/lib/rancher/k3s/storage`) already
> lives on it. The +NVMe `fio` capture below is decisive — random write **3,389
> → 546k IOPS (~161×)** — so per maintainer direction (2026-09-04, "if it works
> we skip the raid-only testing") the larger-RAID-5-only state is NOT measured: a
> bigger RAID-5 cannot matter once the write-hot churn sits on NVMe. The measured
> delta is therefore two states, not three:
>
> 1. **BEFORE — the original RAID-5 spinning array.** Frozen (Layer 1 `fio` +
>    Layer 2 self-hosted CI window). Nothing to re-run.
> 2. **+ NVMe — current state.** Layer 1 captured 2026-09-04 (see "The AFTER"
>    below). **SINGLE card**; a second NVMe is coming, so this is the one-card
>    number and a two-card capture may refine it. The Layer-2 CI build-time AFTER
>    still accrues as self-hosted runs land on the NVMe-backed pool.
>
> Hardware sequencing is authoritative in the livespec repo's
> `plan/poweredge-raid-array-maintenance/research/nvme-add-tmpfs-tiering-and-clean-raid5-rebuild-plan.md`
> (epic `livespec-g52yrb`); the self-hosted switch-back rode the
> ci-runner-pod-lifecycle-reliability track and is DONE (`CI_RUNNER_LABELS =
> ["livespec-console-beads-k3s"]`). Drive geometry — this note earlier flagged a
> 7-drive-RAID-5 vs 5-drive-RAID-5 + 2-drive-NVMe-JBOD discrepancy — is reconciled
> in `livespec-g52yrb`, not here; it does not affect the two-part delta.

Captured **2026-09-02T02:56Z**, at maintainer request, immediately BEFORE the
`poweredge-xubuntu` CI host's disk array is rebuilt from **RAID5 → RAID10** with
new drives. This note is the frozen "before" the post-RAID-10 rebuild is measured
against (charter `research/001` requirement 1: every improvement proven by a
before→after delta). It has two layers:

1. **Direct disk benchmark (`fio`)** — the *primary* measure of the array itself.
   Unlike the Honeycomb build data, this number CANNOT be recaptured after the
   drives change, which is why it was taken now.
2. **Downstream CI build-time baseline** (Honeycomb) — the effect the disk has on
   real console CI jobs, captured from the self-hosted era.

## The hardware being replaced

| Property | Value (pre-RAID-10) |
|---|---|
| Host | `poweredge-xubuntu` — `100.78.140.72`, 72 cores, k3s/ARC self-hosted CI runner |
| Controller | Dell **PERC H730P Mini** (hardware RAID) |
| Current array | **RAID5**, virtual drive `0/0`, Optimal, 1.745 TB |
| Media | Spinning disks (`ROTA=1`) |
| Controller cache | `RWBD` — ReadAhead + **WriteBack** + Direct |
| Benchmarked filesystem | `/dev/sda5` (669 GiB, ext4 `rw,noatime`) |
| What lives on `sda5` | `/var/cache/ci-runner`, `/var/lib/rancher/k3s/storage`, `/var/lib/rancher/k3s/agent/containerd`, and the k3s `local-volume` PVCs the CI runner pods mount |

`sda5` is the exact partition the CI runner build volumes and k3s PVs sit on, so
it is the surface RAID 10 is meant to improve. The whole of `sda` is one PERC
virtual drive today, so the benchmark characterizes the array CI actually hits.

## Layer 1 — Direct disk benchmark (`fio`, the primary before-number)

Method: `fio` 3.41, file-based (`--size=4G`, O_DIRECT `--direct=1`,
`--ioengine=libaio`, `--runtime=30 --time_based --ramp_time=3`) run against
`/var/lib/rancher/k3s/storage/.fio-preraid10/fiotest`, then removed. Run while
CI was on GitHub-hosted runners (host idle of CI, load ~0.6), so no CI
contention skewed it.

| Test | IOPS | Bandwidth | avg clat | p99 clat |
|---|---|---|---|---|
| **seq write** 1M, qd16 | 215 | 216 MiB/s (226 MB/s) | 74 ms | 197 ms |
| **seq read** 1M, qd16 | 690 | 691 MiB/s (724 MB/s) | 23 ms | 108 ms |
| **rand write** 4k, qd32×4 | **3,389** | 13.3 MiB/s (13.9 MB/s) | 37.7 ms | 127 ms |
| **rand read** 4k, qd32×4 | **15,494** | 60.4 MiB/s (63.3 MB/s) | 8.3 ms | 135 ms |

**Reading:** a textbook RAID5-on-spinning-disk profile — fair sequential
throughput, but **random write is the floor: 3.4k IOPS / 13 MiB/s**, dragged down
by RAID5's read-modify-write parity penalty. A cold `cargo` build is thousands of
small `target/` writes — i.e. random-write-bound — so this is the number most
likely to gate cold-build wall time, and the one RAID 10 (mirror+stripe, no
parity) should improve most. Random read is already healthy (15.5k IOPS), helped
by the controller ReadAhead cache.

### Reproduce this exact benchmark at each later disk state

Run this once for state 2 (7-drive RAID-5) and again for state 3 (+ NVMe),
against the filesystem CI actually builds on in that state (the NVMe tier once
it carries the write-hot churn). Keep every fio flag identical to Layer 1 so the
rows compare.

```bash
ssh poweredge-xubuntu
WORK=/var/lib/rancher/k3s/storage/.fio-diskstate; sudo mkdir -p "$WORK"
for spec in "seqwrite_1M --rw=write --bs=1M --iodepth=16 --numjobs=1" \
            "seqread_1M --rw=read --bs=1M --iodepth=16 --numjobs=1" \
            "randwrite_4k --rw=randwrite --bs=4k --iodepth=32 --numjobs=4" \
            "randread_4k --rw=randread --bs=4k --iodepth=32 --numjobs=4"; do
  set -- $spec; name=$1; shift
  sudo fio --name="$name" --directory="$WORK" --filename=fiotest --size=4G \
    --direct=1 --ioengine=libaio --group_reporting --runtime=30 --time_based \
    --ramp_time=3 "$@"
done
sudo rm -rf "$WORK"
```

Compare IOPS + bandwidth + p99 clat per row. Confirm the array geometry matches
the state being captured (`perccli64 /c0/vall show` for the 7-drive RAID-5; the
NVMe device/tier for state 3) per `livespec-g52yrb` before trusting the numbers,
and run it while CI is idle (or on hosted runners) so contention does not
confound the delta.

## The AFTER — +NVMe capture (current state, single card), 2026-09-04

Captured **2026-09-04T17:36–17:39Z** from the optimize-console-builds plan
session at maintainer signal, same `fio` 3.41 methodology as Layer 1, against the
NVMe-backed CI work-volume fs (`/dev/mapper/nvmea-ci--workvols`, ext4, on
`nvme0n1` = **WD_BLACK SN8100 4 TB**, `ROTA=0`). Host was NOT idle (loadavg
~7→14, vs the BEFORE's ~0.6); the NVMe has so much headroom the concurrent CI
load did not cap it, so the delta is if anything conservative.

| Test | BEFORE (RAID-5 HDD) | +NVMe (1 card) | Gain |
|---|---|---|---|
| seq write 1M, qd16 | 215 IOPS / 216 MiB/s | 3,019 IOPS / 3,020 MiB/s | ~14× |
| seq read 1M, qd16 | 690 IOPS / 691 MiB/s | 3,218 IOPS / 3,219 MiB/s | ~4.7× |
| **rand write** 4k, qd32×4 | 3,389 IOPS / 13.3 MiB/s | **546k IOPS / 2,133 MiB/s** | **~161×** |
| rand read 4k, qd32×4 | 15,494 IOPS / 60.4 MiB/s | 442k IOPS / 1,728 MiB/s | ~28× |

p99 completion latency collapsed with it: **random write 127 ms → ~0.49 ms**,
random read 135 ms → ~2.1 ms. The random-write floor Layer 1 named as the likely
cold-build gate (RAID-5's read-modify-write parity penalty) is gone — a cold
`cargo` build's thousands of small `target/` writes are no longer disk-bound on
this host.

Caveats: (1) **single** NVMe card; a second is coming — re-capture for the
two-card number if the fs geometry changes. (2) seq numbers are single-job
(`numjobs=1`, to match BEFORE) so they understate the drive's ceiling; the random
figures are the build-relevant ones. (3) This is the Layer-1 (raw-device) AFTER;
the Layer-2 CI build-time AFTER is captured directly below.

### Layer 2 AFTER — CI build-time on the NVMe self-hosted pool, 2026-09-06

Honeycomb `github-ci`, `repo = thewoolleyman/livespec-console-beads-fabro`,
`ci.runner.kind = self-hosted` (the runner-kind attribute `bzs6` added, so this
is a real filter, not a date guess), window **2026-09-03T23:25Z → 2026-09-06T00:19Z**
= the self-hosted era since the switch-back onto NVMe. **n = 20 runs per job.**
Per-job wall time P50, against the Layer-2 BEFORE table below (RAID-5, P50):
https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/94dxd82kfDd

| Job | BEFORE P50 s | AFTER P50 s | Δ |
|---|---|---|---|
| check-fuzz | 398 | 316 | −21 % |
| check-e2e-tmux | 258 | 125 | −52 % |
| check-nextest | 256 | 64 | **−75 %** |
| check-coverage | 243 | 87 | −64 % |
| check-deps | 188 | 37 | **−80 %** |
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

Compile/test **phase split** (the `Phase:` step spans, n = 21), P50 s, vs
`research/007`: https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/jBLMJTSTsMp

| Phase span | BEFORE | AFTER | Δ |
|---|---|---|---|
| build.check-nextest.compile | 66 | 20 | −70 % |
| build.check-clippy.compile | 42 | 26 | −38 % |
| build.check-fuzz.compile | 78 | 75 | −4 % |
| build.check-nextest.test | 13 | 14 | flat |
| build.check-fuzz.fuzz | ~180 (ratified floor) | 191 | out of scope |

**Reading.** Most jobs lost 70–80 % of their wall time. The diagnostic this note
was written to test is confirmed: the ~177 s **IO-contention gap** on
`check-nextest` (256 s wall vs ~79 s compute) collapsed to **~30 s** (64 s wall vs
~34 s compute) — the shared-array contention was the gap, and NVMe removed it,
exactly as `research/009` predicted.

**Attribution.** This window carries THREE changes at once, not one. (1) NVMe.
(2) The `zzfntv` `CARGO_BUILD_JOBS=12` raise (#935) on the `check-nextest` /
`check-fuzz` compile phases. (3) **sccache, live and hitting**: dev-tooling's
ci-runner-cache-tiers populator is the one writer, and the hook-pod `postStart`
wires `rustc-wrapper = /opt/ci-runner/bin/sccache` (+ `incremental = false`,
`READ_ONLY` redis) into every Rust job — measured over this same window on
`cache.job-summary`: 128 jobs enabled, **4,371 hits / 3,000 misses, avg per-job
hit ratio 0.45 (~59 % aggregate)**, 0 errors
(https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/8Vh2voqvSCF).
So every Rust compile delta here — `check-nextest` compile 66 → 20 s,
`check-clippy` compile 42 → 26 s, `check-deps`, `check-plan-no-tombstone` — is
NVMe **plus** sccache hits (plus the jobs raise on the two raised phases). An
earlier draft of this paragraph called the un-raised Rust jobs a "pure-NVMe
signal"; that was wrong, because they get sccache hits too. The clean
**pure-NVMe isolators are the jobs that compile no Rust at all**:
`check-shell-quality` (python) 136 → 32 s (−76 %) and `check-doctor-static`
(python) 98 → 46 s (−53 %) — disk and contention relief only, no cache, no
parallelism change. The telling contrast inside the raised pair still holds:
`check-fuzz` compile moved only −4 % while `check-nextest` compile moved −70 %;
the ASAN-instrumented fuzz compile is neither IO- nor parallelism-bound and is
also the phase sccache reaches least. `check-fuzz` wall (−21 %) is dominated by
the ratified ~180 s fuzz-run floor.

Both layers now compose into the Phase-3 report (`uocos3`).

## Layer 2 — Downstream CI build-time baseline (self-hosted era)

Honeycomb `github-ci` dataset (team `thewoolleyweb`, env `livespec`),
`repo = thewoolleyman/livespec-console-beads-fabro`. Self-hosted window
**2026-08-24 → 2026-08-31** (78 runs/job) — the last clean poweredge era before
CI moved to hosted runners (see Layer 3). Per-job wall time, P50/P95/MAX seconds:
https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/ik5tJctCdM6

| Job | P50 s | P95 s | MAX s |
|---|---|---|---|
| check-fuzz | 398 | 481 | 528 |
| check-e2e-tmux | 258 | 408 | 474 |
| check-nextest | 256 | 421 | 449 |
| check-coverage | 243 | 370 | 395 |
| check-deps | 188 | 335 | 394 |
| check-clippy | 184 | 321 | 334 |
| check-arch | 151 | 247 | 321 |
| check-behavior-coverage | 149 | 345 | 371 |
| check-completeness | 147 | 313 | 354 |
| check-baseline | 143 | 270 | 326 |
| check-shell-quality | 136 | 265 | 338 |
| check-plan-no-tombstone | 134 | 307 | 356 |
| check-mutants | 124 | 231 | 275 |
| check-format | 123 | 284 | 358 |
| check-plugin-resolution | 121 | 330 | 353 |
| check-doctor-static | 98 | 206 | 296 |

`ci.job.queue_ms` did not exist in this window (first written 2026-09-01 16:30Z),
so queue is unmeasured here; treat these wall times as execution+overhead on the
shared self-hosted host.

**The disk-relevant gap:** the compile-vs-test phase split (`research/007`) shows
`check-nextest` is ~66 s compile + 13 s test ≈ 79 s of actual cargo work, yet its
wall time is 256 s. Much of that ~177 s gap is contention/IO on the shared
single-host self-hosted lane (many job slots, one disk array) — precisely what
"more pods/volumes + RAID 10" targets. RAID 10's random-write gain should show up
here as reduced cold-compile wall time once CI is switched back to self-hosted.

## Layer 3 — Context: the hosted-runner window and the switch-back

At the Layer-1 BEFORE capture (2026-09-02) `CI_RUNNER_LABELS` was **absent**, so
`ci.yml` fell back to `["ubuntu-latest"]` and CI ran on GitHub-hosted runners —
the poweredge disk was not in their path, so **hosted-runner CI times are
irrelevant to the disk comparison**. That hosted window ran 2026-09-02T08:02Z →
2026-09-03T13:23Z (the hold). The **switch-back is done**: `CI_RUNNER_LABELS =
["livespec-console-beads-k3s"]`, first self-hosted master run
2026-09-03T23:25Z (run 33817563429), on the NVMe pool — which is the AFTER window
"Layer 2 AFTER" measures. Both transitions are visible as `check-nextest` regime
changes
(https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/cVFmpuTGFhQ),
and since `bzs6` every `ci.job.*` span carries `ci.runner.kind`, so the windows
are filterable rather than date-guessed. The three self-hosted master runs
immediately before the hold (2026-09-02 05:xx) all **failed** `check-e2e-tmux` on
RAID-5; the self-hosted runs since the switch-back are green on NVMe — the same
lane, disk changed.

## Telemetry gap found while capturing this

The `github-ci` dataset has **no runner-type attribute** (no `runner.name`,
labels, or self-hosted/hosted flag). Self-hosted vs hosted runs can only be
separated by time, which is fragile. `export-ci-telemetry.sh` should emit the
GitHub jobs API `runner_name` / `labels` (and a derived `ci.runner.kind` =
self-hosted|hosted) so before/after windows are filterable, not date-guessed.
Filed as a follow-up (see the plan epic timeline).

## Factory host hp-xubuntu — SSD disk baseline (companion; NOT a RAID change)

Captured 2026-09-02 at maintainer request, same `fio` methodology as Layer 1.
Unlike poweredge, **hp-xubuntu has no RAID and is already all-SSD**, so this is a
factory-host baseline for Phase-2 comparison, not a pre-RAID capture — hp is not
part of the RAID-10 upgrade.

| Property | Value |
|---|---|
| Host | `hp-xubuntu` — `100.68.193.50`, 16 cores, the fabro dispatch/factory host |
| Storage | `sda` = Crucial **CT2000BX500SSD1** (2 TB SATA SSD, `ROTA=0`); no RAID controller, no mdraid |
| Benchmarked filesystem | `/dev/sda3` (1.4 TB ext4, mounted `/data`) — the same fs `/var/lib/docker` lives on, i.e. where the fabro sandbox containers build |

Method: `fio` 3.41 (installed for this capture), file-based, O_DIRECT, 4 GiB,
30 s/test, against `/data/.fio-hpbaseline/fiotest`, then removed. Host idle
(load ~0.6).

| Test | IOPS | Bandwidth | p99 clat |
|---|---|---|---|
| seq write 1M, qd16 | 484 | 485 MiB/s (508 MB/s) | 42 ms |
| seq read 1M, qd16 | 527 | 528 MiB/s (553 MB/s) | 45 ms |
| **rand write** 4k, qd32×4 | **5,584** | 21.8 MiB/s (22.9 MB/s) | 81 ms |
| rand read 4k, qd32×4 | **30,544** | 119 MiB/s (125 MB/s) | 8.3 ms |

**Reading vs poweredge (RAID5 spinning):** hp's SSD roughly doubles random read
(30.5k vs 15.5k IOPS) and beats random write (5.6k vs 3.4k IOPS) and seq write,
while poweredge's striped RAID5 wins seq read (691 vs 528 MiB/s). hp's random
write (5.6k IOPS / 22 MiB/s) is still modest for an SSD — the BX500 is a budget
DRAM-less SATA drive — so the factory's cold cargo build (random-write-heavy on
`target/`) is somewhat disk-bound here too, though less than poweredge. The
factory's larger cold-build tax is CPU (16 cores) + full re-fetch/recompile of a
thrown-away sandbox, not disk; disk is a secondary lever for the factory tier.

## Snapshot summary (the frozen "before")

- **Array:** PERC H730P, RAID5, spinning, WriteBack — random-write floor **3,389
  IOPS / 13.3 MiB/s**; seq **216 write / 691 read MiB/s**.
- **CI wall time (self-hosted):** critical jobs 184–398 s P50; `check-nextest`
  256 s wall vs ~79 s compute.
- **Two-state delta (per the 2026-09-04 v3 top annotation):** state (1) BEFORE
  is the frozen capture above; state (2) +NVMe Layer-1 `fio` is captured in "The
  AFTER" (random write 3,389 → 546k IOPS, ~161×; single card). The RAID-5-only
  intermediate was skipped — NVMe validated directly. Still owed: the Layer-2 CI
  build-time AFTER (a Honeycomb window of self-hosted runs on the NVMe pool) and,
  if the second card changes the fs geometry, a two-card Layer-1 re-capture.
