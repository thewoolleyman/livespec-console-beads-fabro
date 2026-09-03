# 008 — Pre-RAID-10 poweredge disk + CI baseline (future-reference snapshot)

> **ANNOTATION 2026-09-04 — the RAID-10 rebuild this note anticipates was
> DROPPED (maintainer-decided 2026-09-02).** The array stays RAID-5 (rebuilt
> clean across 5 drives) and the write-hot CI churn moves to a dedicated
> two-drive NVMe JBOD tier instead; the kine datastore moves to tmpfs. See the
> livespec repo's `plan/poweredge-raid-array-maintenance/research/
> nvme-add-tmpfs-tiering-and-clean-raid5-rebuild-plan.md` (epic
> `livespec-g52yrb`). **This baseline stays fully valid** — read "pre-RAID-10"
> as "pre-rebuild/pre-NVMe": it remains the frozen BEFORE for the same
> before→after delta, whatever the AFTER's storage shape is.

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

### Reproduce this exact benchmark AFTER the RAID-10 rebuild

```bash
ssh poweredge-xubuntu
WORK=/var/lib/rancher/k3s/storage/.fio-postraid10; sudo mkdir -p "$WORK"
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

Compare IOPS + bandwidth + p99 clat per row. Confirm `perccli64 /c0/vall show`
reports `RAID10` before trusting the "after" numbers, and run it while CI is
idle (or on hosted runners) so contention does not confound the delta.

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

## Layer 3 — Context: CI is currently on GitHub-hosted runners

At capture time `CI_RUNNER_LABELS` is **absent** from the repo, so `ci.yml`'s
`runs-on` falls back to `["ubuntu-latest"]` — CI runs on GitHub-hosted runners,
NOT poweredge. A previous worker made this switch; the maintainer will switch
back to self-hosted after the RAID-10 rebuild + a pods/volumes increase, so
future numbers reflect the real array under real concurrency. **Hosted-runner CI
times are irrelevant to the RAID-10 comparison** — the poweredge disk is not in
their path. The self-hosted → hosted transition is visible as a `check-nextest`
regime change around 2026-09-01
(https://ui.honeycomb.io/thewoolleyweb/environments/livespec/datasets/github-ci/result/cVFmpuTGFhQ).

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
- **After RAID-10 + volume bump + switch-back:** rerun Layer-1 `fio` and Layer-2
  Honeycomb queries; cite both deltas here.
