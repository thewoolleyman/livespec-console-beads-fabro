# 004 — Local dev environment: vps (vmi3006760)

Measured live 2026-08-30 on the host itself (tailscale 100.89.189.118).

## Host and space

- **18 cores, 94 GiB RAM** (~45 GiB available at measurement).
- Single partition `/dev/sda1` (ext4) holds everything — `/`, `~/.cargo`, the
  primary checkout, and all worktrees: 678 G total, 332 G used, **346 G free
  (49%)**. The maintainer's "a few gig free" concern does NOT hold on the
  partition that actually carries cargo state — measured directly with
  `df`/`findmnt` on `~/.cargo` and the repo path. Space is comfortable today;
  the requirement is to keep it bounded, not to emergency-shrink.

## Cargo state

| Thing | Size |
|---|---|
| `~/.cargo/registry` | 1.2 G (1442 crate archives in cache) |
| `~/.cargo/git` | 9.2 M |
| `~/.rustup` | 4.9 G |
| primary checkout `target/` | **12 G** |
| worktree targets (`~/.worktrees/livespec-console-beads-fabro/*/target`) | ~4.1 G each; count varies with live worktrees |

## Build times

- Warm near-no-op `cargo build --workspace` (one crate recompiled): **17.3 s**.
- Cold-build and cold-`just check` timings are NOT yet measured locally — a
  cold measurement invalidates the primary 12 G target and costs a full
  rebuild, so it belongs in Phase 1 under telemetry (measure once, record in
  Honeycomb) rather than being repeated ad hoc. The factory's same-workspace
  evidence (note 003) puts full `just check` at 251–273 s on 4 vCPU; local at
  18 cores with `build.jobs = 4` capped will differ — measure, don't infer.
- The committed `.cargo/config.toml` cap (`build.jobs = 4`,
  `RUST_TEST_THREADS = 4`) exists for the shared CI host but ALSO throttles
  every local build on this 18-core host — a candidate local lever (e.g.
  developer-local `CARGO_BUILD_JOBS` guidance or a config override outside the
  committed file), to be measured in Phase 2.

## Eviction today: none

- Nothing evicts `~/.cargo/registry` (grows monotonically; `cargo cache`/
  age-based sweep candidates exist).
- Nothing evicts `target/` dirs. Worktree targets (~4 G each) are reclaimed
  only when `just worktree-land`/`worktree-reap` removes the worktree —
  orphaned worktrees hold theirs indefinitely (the reap recipe exists but is
  manual). The primary's 12 G target accretes stale artifacts across toolchain
  bumps forever; `cargo sweep --time N` style age-based pruning is the obvious
  bounded policy.
- `~/.rustup` keeps old toolchains after pin bumps (4.9 G now) — an age/unused
  based `rustup toolchain uninstall` sweep is in scope as cleanup.

## Telemetry today: none

Local builds emit nothing to Honeycomb (note 005). Phase 1 adds a thin
emission path for `just check` / pre-push aggregate durations with
`build.env=local`.
