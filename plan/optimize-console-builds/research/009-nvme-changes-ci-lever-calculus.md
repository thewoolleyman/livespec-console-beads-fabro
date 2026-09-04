# 009 — What the +NVMe result changes for the Phase-2 CI levers

A decision note, not a new baseline. The +NVMe disk capture (`research/008`,
2026-09-04) is large enough to change how the still-parked Phase-2 **CI**
levers should be sized, so this records the implication before anyone picks them
up against the wrong (RAID-5) baseline.

## The measured change

`research/008` "The AFTER": on the poweredge CI pool, one NVMe took random write
from **3,389 → 546k IOPS** (~161×) and p99 write latency from **127 ms → ~0.49
ms**. The random-write floor Layer 1 named as the likely cold-build gate — the
RAID-5 read-modify-write parity penalty over thousands of small `target/` writes
— is gone. The CI work-volume fs already lives on the NVMe.

## Why this touches the lever calculus

`research/008` Layer 2 read the self-hosted `check-nextest` wall time as **256 s
wall vs ~79 s compute** (`research/007`: 66 s compile + 13 s test), and
attributed much of the ~177 s gap to **shared-disk IO contention** on the
single-array self-hosted lane. NVMe removes the disk-IO component of that gap. So
the RAID-5-era wall times **overstate the headroom** the IO-sensitive CI levers
can still recover: part of what they were sized to reclaim is now reclaimed by
the hardware.

This does **not** retire the levers — it re-bases them. Each targets more than
disk IO:

- **`wki5zf` (cargo-registry warm generation)** — targets re-*fetch* (network)
  + re-*extract* (disk + CPU). NVMe subsumes the extract-IO cost, so that slice
  of its win shrinks; the **network-fetch avoidance remains**. Re-baseline
  against the post-NVMe Layer-2 AFTER; expect a smaller win than the RAID-5
  estimate, dominated now by fetch, not extract.
- **`z2siyn` / `ydlant` (warmed `target/` generations)** — target **recompile
  CPU** avoidance (the dominant per-`research/007` cost: 66 s compile on
  `check-nextest`, 78 s ASAN on `check-fuzz`) plus the IO of writing `target/`.
  NVMe subsumes only the IO; **the compile-CPU saving is untouched by faster
  disk**. These keep most of their value — but only if the warmed-tree hit
  proves out (`z2siyn`'s hardlink/path-identity spike is still the gate), and
  sized against the post-NVMe numbers.
- **`zzfntv` (job-scoped `CARGO_BUILD_JOBS` raise, merged #935)** — targets
  compile **parallelism** (CPU), never disk. **Unaffected**; its after-
  measurement stands as-is once ≥10 self-hosted NVMe runs exist.

## The tmpfs deferral is now decidable

The 2026-09-02 scope event deferred CI option (f) *tmpfs/RAM target dirs*,
"measurement-gated … using `research/008` disk numbers to decide whether IO
still gates." The numbers now answer it: **IO no longer gates** — 546k random-
write IOPS at ~0.49 ms p99 leaves tmpfs no meaningful build-IO headroom to
recover over NVMe, against a real memory-pressure cost (16 jobs × multi-GB in
RAM). So tmpfs stays deferred and its reconsider-condition is effectively
**closed by the hardware**, not merely postponed.

## Action

The **Layer-2 CI build-time AFTER on the NVMe pool is now a prerequisite** for
sizing `wki5zf` / `z2siyn` / `ydlant` — do not size or prioritize them against
the RAID-5 baseline. Compose that AFTER into the Phase-3 report (`uocos3`)
alongside the disk delta, and re-read this note before scheduling those levers.
Nothing here is a disposition: the levers stay filed; their **justification is
re-based, and their measurement moved onto the NVMe substrate**.
