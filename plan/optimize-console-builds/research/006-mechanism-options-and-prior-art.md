# 006 — Mechanism options per environment, and prior art

Options are laid out WITH tradeoffs and an eviction story each (requirement 2:
bounded, age/staleness-based, never deletes hot cache). The worker session +
maintainer pick per environment; nothing here is pre-decided except the
ordering guidance at the end.

## Prior art (cross-reference, do not duplicate)

- **Cache-tier design record** (livespec repo, archived plan
  `fleet-ci-runner-pool`):
  `/data/projects/livespec/plan/archive/fleet-ci-runner-pool/research/design.md`
  §"Cache tiers, and the volume that holds them" — tier 1 = warm uv+cargo+target
  lowers (only uv shipped), tier 2 = local Actions cache service (deferred, "no
  consumer yet"), tier 3 = Nix. Maintainer directed 2026-08-13 that local
  caching is in scope.
- **Tier-2 design + deferral**:
  `/data/projects/livespec/plan/archive/fleet-ci-runner-pool/research/cache-tier-2-design.md`
  — requires a third-party fork of the runner binary (or patched
  `Runner.Worker.dll`) because `ACTIONS_RESULTS_URL` cannot be overridden;
  standing recommendation: DO NOT BUILD until a consumer exists.
- **Warm-cache implementation** (the uv generation model):
  `/data/projects/livespec-dev-tooling/ci-runner/k3s/phase2/warm-cache/`
  (README, `warm-cache-populate.sh`, `warm-cache-cronjob.yaml`) +
  `../arc/hook-pod-template.yaml`. Model: one trusted writer (CronJob every
  30 min, `concurrencyPolicy: Forbid`) populates a NEW generation dir
  hardlink-seeded from the current one, publishes via one atomic symlink
  rename, prunes all but newest 2; readers get the warm root READ-ONLY and
  postStart-copy onto their work volume before step 1; fail-soft on the read
  side, fail-loud in the populator. README's "Why uv only, not cargo" calls the
  cargo-registry extension "mechanical" (with the `CARGO_HOME`-moves-installed-
  tools caveat) — but that warms only the REGISTRY; the compiled `target/` is
  the harder, higher-value piece.
- **Ledger**: `livespec-console-beads-fabro-txtzn5.16` — "Cache the fuzz build"
  (`ready`, autonomy HOST-ONLY, re-scoped toward host-ops/k3s persistence).
  THIS PLAN SUBSUMES IT: its measured breakdown (316 s job = 180 s ratified
  floor + ~136 s ASAN build; the no-Rust-skip is the WRONG fix) carries over
  into the CI options below. Related closed items: `txtzn5.9` (fuzz gate),
  `txtzn5.10` (mutants job). Archived plan `delivery-path-speed-and-caching`
  (`plan/archive/delivery-path-speed-and-caching/`) is earlier prior art on
  delivery-path speed.
- **Deleted `actions/cache`** (commit d67a2d6, 2026-07-17): its cold-self-hosted
  vs warm-hosted comparison is methodologically broken (conflates cold-vs-warm
  with big-vs-small host; never measured the fuzz build). "Caching buys nothing
  here" is UNPROVEN — Phase 1 re-measures same-host warm-vs-cold.

## CI (poweredge-xubuntu) options

**(a) Extend tier 1 with a cargo-registry generation** (`cargo fetch` into
hardlink-seeded `warm/cargo-generations/<stamp>`, postStart copy, `CARGO_HOME`
pointed at the copy with baked `/root/.cargo/bin` kept on PATH).
Eviction: identical newest-2 age-based generation prune — already ratified,
bounded, hot-safe. Tradeoff: mechanical and safe but small (~10–30 s/job);
downloads are not the dominant cost. Copy cost must stay conditional so
non-Rust fleet jobs don't pay it.

**(b) Additionally warm compiled `target/` generations — the big win, the hard
one.** Populator builds master's workspace per needed profile into per-key
generation dirs; jobs copy (prefer `cp -al` hardlink-copy on the shared
filesystem) and build incrementally on top. Keying: rustc release (1.92.0 pin,
+ nightly for fuzz) × profile × RUSTFLAGS × features × triple — at least four
distinct trees (normal dev/test; llvm-cov `-C instrument-coverage`; mutants
(cargo-mutants copies trees anyway — likely skip); ASAN fuzz, which MUST be its
own tree — sanitized rlibs are ABI-incompatible with normal ones). Invalidation
free-rides on cargo fingerprints (stale entries rebuilt, not corrupting).
Eviction: same newest-2 generation prune; bounded at a few GB × 2 per tree.
Engineering risks to burn down FIRST: (1) **path identity** — cargo
fingerprints embed the workspace path, so the populator must build at the same
in-pod path jobs use (`/__w/<repo>/<repo>`) or hit-rate silently drops to 0;
(2) postStart copy time for multi-GB trees (hardlink-copy mitigates; cargo
rewrites some files in place — fingerprints/depinfo — so `cp -a` vs `cp -al`
safety needs one measured experiment); (3) populator now compiles Rust every
cycle (cheap on 72 cores; can drop to hourly). Ceiling: warms fuzz to its
~200 s floor and plausibly brings the whole run from ~420 s toward
~250–300 s.

**(c) Persistent shared read-write target dir (hostPath/static PV as
`CARGO_TARGET_DIR`).** Rejected-shaped: cargo takes a single coarse lock per
target dir, so 12+ concurrent matrix jobs would serialize; and it breaks the
one-trusted-writer model (PR code writes a cache later jobs read —
cache-poisoning surface the current design explicitly structures away).
Eviction would need a new janitor (`cargo sweep --time N` — age-based, so
compliant, but a new moving part). Keep only as a fallback record.

**(d) sccache, local disk cache on `/var/cache/ci-runner/sccache`**
(`RUSTC_WRAPPER=sccache` via hook template; binary baked into the image;
`CARGO_INCREMENTAL=0` required — fine in CI). Concurrent-safe,
content-addressed, covers all profiles at once (ASAN keyed correctly via
hashed flags; can cover libFuzzer's C++ compile via `CC="sccache cc"`); does
not cache build-script execution or links. **Eviction friction: sccache ships
only an LRU size cap (`SCCACHE_CACHE_SIZE`), not age-based eviction** — LRU
never evicts hot entries (the requirement's spirit) but is a blunt size cap by
mechanism (the requirement's letter). If chosen, the work-item must record the
reasoned acceptance or front it with an age-sweep janitor. Trust: shared
writable cache written by PR code — mitigated but not eliminated by
content-addressing. Best leverage-per-effort on raw compile time; second to
(a)+(b) on architectural fit.

**(e) Tier-2 local Actions cache service.** Keep deferred per the standing
design pass (runner-fork cost; and restore/save tar churn of multi-GB archives
re-adds the exact cost the deleted `actions/cache` measured as net-negative).
Eviction itself would be fine (server-side age-based retention).

**(f) tmpfs/RAM target dirs** (188 GiB RAM). No persistence — orthogonal to
caching; compile here is CPU-bound, and 16 jobs × multi-GB in RAM is a real
memory-pressure risk. Measurement-gated micro-optimization only.

**(g) Job-scoped parallelism raise.** `.cargo/config.toml` caps every build at
`jobs = 4` (16 jobs × 4 ≈ 64 threads on 72 cores). Raising SELECTIVELY for the
2–3 longest jobs (`CARGO_BUILD_JOBS=12` env on `check-fuzz`, `check-nextest`'s
compile phase; runtime `RUST_TEST_THREADS` stays) keeps matrix-wide worst case
≤ ~1.2× cores and directly attacks the ~136 s ASAN build. A blanket raise
(12×12=144 threads) is NOT safe. Zero infrastructure, reversible, measurable
next run — the cheapest first experiment.

## Factory (hp-xubuntu) options

1. **Shared cargo registry via bind mount/volume** (e.g. `/data/cache/cargo`
   mounted into each sandbox; cargo ≥1.68 fine-grained registry locking makes
   concurrent same-version access safe). Kills the per-run crates.io
   download/unpack. Needs a fabro per-env mount knob — verify fabro's docker
   provider supports volume mounts, else it becomes a fabro feature request.
   Eviction: age-based find-mtime/`cargo cache` cron on `/data/cache/cargo`.
   Low risk, moderate gain.
2. **sccache, local disk cache on `/data/cache/sccache`** bind-mounted +
   `RUSTC_WRAPPER` — designed exactly for concurrent ephemeral builders sharing
   one content-addressed cache. Needs sccache baked into the
   `python-rust-agent` image (livespec-dev-tooling Dockerfile) + env vars.
   Incremental-compile loss is irrelevant (builds are cold anyway). Same LRU
   eviction-letter caveat as CI §d. Best gain/risk combined with 1.
3. **Persistent shared `CARGO_TARGET_DIR` on `/data`**: biggest possible gain
   but riskiest — cargo's coarse target-dir lock convoys up to 15 concurrent
   runs; profile/flag churn (coverage, mutants) thrashes it; needs its own
   age-based eviction. Only after 1+2 if compile still dominates.
4. **Pre-warmed image**: bake a fetched registry (or even a pre-built master
   `target/`) into the sandbox image at image-build time — zero runtime
   coupling, refreshed per release, at the cost of image size (already
   3.94 GB). A clean hermetic alternative to mounts, worth costing.
5. **Flanking**: raise `resources.cpu` for the `livespec-ci` env (host has 16
   cores, 15-run cap rarely saturates) — but swap is already 5.8 G used; watch
   RAM. Container reuse / `preserve = true` is NOT promising (deliberate
   hermeticity; existing 24–48 h container eviction already bounds growth).

## Local (vps) options

1. **Eviction first** (the local gap is bounded-growth, not speed):
   age-based `cargo sweep --time N` (or `cargo clean` staleness policy) over
   primary + worktree targets; scheduled `just worktree-reap` for orphaned
   worktrees; `cargo cache`-style registry trim; unused-toolchain rustup sweep.
   All age/staleness-based; sizes in note 004.
2. **Parallelism**: the committed `build.jobs = 4` cap throttles an 18-core
   host; a local (uncommitted) override is a measured candidate.
3. **sccache locally** — cheap to try; interacts with incremental compilation
   (sccache disables it), so measure warm-editing loops before adopting; may
   be a net LOSS locally where incremental is already warm.

## Ordering guidance (recommendation, not a decision)

1. Phase 1 telemetry + baselines everywhere (note 005) — nothing ships before
   it can be measured.
2. CI: (g) job-scoped parallelism (cheapest, reversible) → (a) registry
   generation → (b) warmed target generations (burn down path-identity + copy
   cost first; fuzz tree first since txtzn5.16 is ready and HOST-ONLY, then the
   normal-profile tree that moves `check-nextest`).
3. Factory: registry mount (or pre-warmed image) → sccache; target-dir sharing
   only if still compile-bound.
4. Local: eviction policies + parallelism measurement.
5. Keep tier 2 deferred; skip CI §c; tmpfs measurement-gated.
Each landed change gets its Honeycomb after-measurement before the next is
judged (requirement 1); every tier lands WITH its eviction (requirement 2);
Phase 3 report + human approval precede archive (requirement 3).
