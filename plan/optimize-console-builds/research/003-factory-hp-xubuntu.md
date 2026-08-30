# 003 — Factory environment: hp-xubuntu (fabro dispatch host)

All numbers measured live 2026-08-30 over `tailscale ssh cwoolley@hp-xubuntu`
(100.68.193.50). Host docs: `/data/projects/hp-xubuntu-info/`.

## Host

- **16 cores, 30 GiB RAM** (2.4 free, 22 buff/cache; swap 8 G with 5.8 G used —
  RAM headroom is TIGHT, unlike the other two hosts).
- `/dev/sda1` → `/`: 458 G, **408 G free**. `/dev/sda3` → `/data`: 1.4 T,
  **1.3 T free**. Docker/containerd stores are bind mounts from `/data`
  (`/data/docker`, `/data/containerd`), relocated after the 2026-08-22
  disk-full incident. Docker 29 uses the containerd snapshotter; ~99% of the
  store is per-container overlayfs snapshots.

## How the factory builds the console

- fabro 0.254.0 (`~/.fabro/bin/fabro`), server 127.0.0.1:32276,
  `max_concurrent_runs = 15` (`~/.fabro/settings.toml`).
- **One Docker sandbox container per run** (`fabro-run-<ULID>`), image
  `ghcr.io/thewoolleyman/livespec-fabro-sandbox:python-rust-agent-v1.36.0`
  (3.94 GB); the console's `.fabro/workflows/implement-work-item/workflow.toml`
  gives the `livespec-ci` environment **4 CPU / 8 GB** (`preserve = false`).
  Inside the container `nproc` reports 16 (cgroup quota, not cpuset), but the
  repo's `.cargo/config.toml` caps jobs at 4 anyway.
- **Fresh clone per run**: shallow (depth 10) into `/workspace/<repo>`, then a
  prepare step runs `git fetch --unshallow`. Rust 1.92.0 toolchain is baked
  into the image (matches `rust-toolchain.toml`); **no sccache in the image, no
  `CARGO_*` env set** — `CARGO_HOME=/root/.cargo` and `target/` both live in
  the container's writable layer and **die with the container. Zero cross-run
  reuse** of registry, git deps, or compiled artifacts.

## Measured accumulation (2026-08-30)

- `docker system df`: Images 3 = 85.24 GB; **Containers 53 = 79.55 GB, of which
  76.18 GB (95%) reclaimable** — exited-run writable layers.
- Per-run writable layers: 0.5–1.1 GB for non-Rust (orchestrator) runs;
  **2.3–6.9 GB for Rust-building console runs** (e.g.
  `fabro-run-01M16M00BQ7HJ5DKGG3NMY1HKJ` 6.89 GB). Each multi-GB layer is a
  from-scratch registry download + full cold `target/`, thrown away.
- Documented growth rate ~4 GB/hour, ~100 GB/day at current dispatch rate
  (`~/repos/fabro-hosts/services/container-reclaim/hosts/hp-xubuntu.env`).
- fabro's own state is small (`~/.fabro/storage` 652 M).

## Build timings

No plain-text cargo timing logs are accessible (run logs live in fabro's
slatedb object store; container stdout is empty — work happens via
`docker exec`). Best on-host timing evidence is written into the workflow
itself, measured on this host 2026-07-30
(`.fabro/workflows/implement-work-item/workflow.toml` checkpoint section):
**pre-commit hook 91.76 s; pre-push `just check` 251–273 s** (hence
`commit_timeout = "10m"`), each run additionally paying the cold
full-workspace compile implied by the 2–7 GB target dirs. Phase 1 telemetry
(note 005) is what turns this from inference into measurement.

## Existing cleanup/eviction

- `container-reclaim.timer` (hourly, fleet-managed from `fabro-hosts`): exited
  containers > 48 h, unreferenced images > 72 h.
- `docker-prune.timer` (daily, unmanaged hotfix): container prune `until=24h` +
  image prune `until=72h` + builder prune — the effective container horizon.
- `disk-guard.timer` (every 15 min, `/usr/local/sbin/disk-guard.sh`, unmanaged):
  emergency prune when `/` < 40 G free — **still watches `/` although the store
  moved to `/data`** (pre-relocation artifact; correct as cleanup).
- No storage-reclaim (target-dir) timer; **nothing evicts or reuses cargo
  caches because none persist.** Eviction is healthy today (~80–200 GB steady
  state against 1.3 T); the problem is not disk pressure but that **every run
  pays a cold crates.io download + cold full-workspace build on 4 vCPUs.**

## Side observations (logged, not blocking)

- Sandbox image rustc (1.92.0) matches the repo pin today; any pin bump desyncs
  them and triggers per-run rustup downloads — worth a lockstep check.
- `docker inspect` on run containers exposes live credentials in `Config.Env` —
  host-hardening note, out of this plan's scope but recorded.
- Swap already 5.8 G used — RAM-hungry options (tmpfs targets) are off the
  table on THIS host; watch RAM if raising `resources.cpu` for the
  `livespec-ci` environment.

Mechanism options for this host, ranked with tradeoffs: note 006 (factory
section).
