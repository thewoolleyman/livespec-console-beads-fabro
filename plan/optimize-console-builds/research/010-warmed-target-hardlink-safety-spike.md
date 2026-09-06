# 010 — Warmed `target/` generations: hardlink safety, relocation freshness, copy cost (z2siyn spike, both halves)

The `z2siyn` spike exists to burn down two risks before `ydlant` invests in
warmed `target/` generations (`research/006` §b): **path identity** (a generation
built anywhere but the in-pod path silently never hits) and **copy cost /
hardlink safety** (a multi-GB tree copied per job may eat the win, and cargo's
in-place fingerprint rewrites may corrupt a hardlink-shared generation). The
first half needs **no pool**: a cargo-behaviour question, answered on the vps in
an isolated scratch clone (Results 1–4). The **pool half** (Results 5–8, added
2026-09-06) ran on the idle k3s pool once both NVMe cards were in: the ASAN fuzz
tree's real size and copy cost on the XFS work volume, a build at the in-pod
path, Fresh confirmations against a fresh clone, and a PR-shaped case. Its
finding changes the design the first half recommended — see "Go / no-go".

## Setup

`vmi3006760` (the vps, 18 cores), `git clone --depth 1` of the console into a
scratch dir — **never the primary's live `target/`**. Dev profile,
`CARGO_BUILD_JOBS=16`. Cold `cargo build --workspace`: **26 s**, `target/` =
**752 MB**, **628** files under `debug/.fingerprint/`. (`du`/`ls` on this host are
shimmed to `dust`/`lsd`; sizes here are re-measured with `/usr/bin/du`.)

## Result 1 — copy cost: `cp -a` vs `cp -al`

| method | wall | on disk |
|---|---|---|
| `cp -a` (bytes) | **1,140 ms** | 749 MB (second copy) |
| `cp -al` (hardlinks) | **77 ms** | ~0 (shares blocks) |

Hardlinking is ~15× cheaper and costs no space. Ratios transfer; absolute
times do not — this is vps disk, and at the NVMe pool's ~3 GB/s sequential even a
byte-copy of a multi-GB ASAN tree is a few seconds. The pool half measures it.

## Result 2 — relocating the target dir preserves freshness

Rebuilding **in the hardlink copy** (`CARGO_TARGET_DIR=../target-cp-al`, a
different path from the original) with the source unchanged: **0 `Compiling`, 0 s**
— everything Fresh. Cargo's fingerprints embed the **source** path, not the
target-dir path. Two consequences, both matching the populator's design
(dev-tooling `warm-cache-populate.sh` builds at `/__w/<repo>/<repo>`): the
generation MUST be built at the in-pod **source** path (path identity, risk #1),
and it can then be **copied to any target location** on the job's volume without
losing freshness.

## Result 3 — hardlink safety splits in two (the finding)

A source change was made and the crate recompiled **in the copy**, twice, while
watching the **original** generation's inodes, bytes and mtimes.

**Artifacts (`deps/*.rlib`) — SAFE to hardlink.** Each recompile wrote the rlib as
a **new inode** (original `145370701` → copy `145370600`, then `145371984`); the
original's rlib kept its inode and bytes (`md5 8accb1f3`) both times. rustc/cargo
write artifacts as fresh files, so a consumer's rebuild **cannot** reach the
shared generation's artifacts through a hardlink.

**Fingerprints (`.fingerprint/*`) — NOT safe to hardlink.** They were rewritten
**in place** on the shared inode: **628/628** copy fingerprint files still shared
an inode with the original after the recompile, and the **original's**
`lib-console_upstream_dep_check{,.json}` mtime moved to the copy's recompile time
(`1788657489`). It was harmless *here* only because the bytes were identical —
for local crates cargo's fingerprint hash excludes source content (freshness is
mtime-based via dep-info), so a comment-only change rewrites identical bytes.
A consumer whose build differs **structurally** (an added dependency, a feature,
different `RUSTFLAGS`) would rewrite fingerprint **content** in place and break
the generation's fingerprint↔artifact pairing for every other reader.

**Design consequence — hybrid copy.** `cp -al` the artifact directories
(`deps/`, `build/`, `incremental/` if kept) and `cp -a` (bytes) the
`.fingerprint/` tree. The fingerprint tree is 628 small files here — byte-copying
it costs nothing measurable and removes the one real hazard. This is the same
shape the uv tier already uses for the analogous hazard (`UV_LINK_MODE=copy` so a
read-only shared inode is never written through).

## Result 4 — generation key (acceptance bullet 3)

Because local fingerprints exclude source content, the key must carry everything
the fingerprint hash depends on **plus** the source snapshot the artifacts were
built from: **rustc release · profile (dev/test vs the ASAN fuzz profile) ·
`RUSTFLAGS`/`RUSTDOCFLAGS` · feature set · target triple · source ref (master
commit)**. A key miss on any of these must fall back to cold, never to a
mismatched tree.

## Pool half — the ASAN fuzz tree on the real work volume (2026-09-06)

### Setup

A pod on `poweredge-xubuntu` (72 cores, idle pool: every scale set at 0
runners), 2026-09-06T03:14–03:22Z, in the jobs' own image
(`livespec-fabro-sandbox:python-rust-v1.45.0`), with the jobs' own cargo
config written the way the hook template's `postStart` writes it (crates
proxy + `rustc-wrapper = sccache`, `SCCACHE_REDIS_RW_MODE=READ_ONLY`), CI's
setup steps in CI's order (`mise trust` + `mise install`, the g++ apt install,
`just ensure-fuzz-tooling`), and `CARGO_BUILD_JOBS=12` like the job's
"Phase: compile" step. The XFS work-volume tier (`nvmeb-ci--workvols`,
`reflink=1`; `research/008` "dual card") was hostPath-mounted **once** at `/__w`,
so the generation and the checkout sat under one mount — the only place a
hardlink is legal, and where the provisioner's seed runs. Source path
`/__w/livespec-console-beads-fabro/livespec-console-beads-fabro`, master
`6edfb8a0`, four independent runs (the first three found harness bugs of mine;
the numbers below are the fourth, with ranges where the earlier runs also
measured the step).

### Result 5 — the tree, and the cold build it replaces

| Measure | Value |
|---|---|
| cold `cargo +nightly fuzz build` at the in-pod path, 12 jobs | **33–41 s** (4 runs: 40.2, 33.0, 38.4, 35.0; a fifth fresh-clone cold control: 41.2 s) |
| crates compiled | 21 (`Compiling` lines); sccache saw 74 Rust + 26 C++ compile requests |
| sccache hits | **0 %** — the populator never builds the fuzz shape, and `-Zsanitizer=address` keys differ from every dev/test artifact |
| `fuzz/target` | **253 MB** on disk (`/usr/bin/du`) |
| tooling install before the build | g++ via apt 9–11 s; nightly + cargo-fuzz 26–32 s (paid on every CI run: `livespec-dev-tooling-3u3gm2.2`) |

### Result 6 — copy cost on XFS, three methods

| method | wall (4 runs) |
|---|---|
| `cp -a` (bytes) | 47–53 ms |
| `cp -al` (hardlinks) | 18–23 ms |
| `cp -a --reflink=always` (copy-on-write) | 37–47 ms |

All three are noise against a 33 s build — the copy-cost risk is closed for
this tree on this tier. Reflink costs 20–30 ms more than hardlinks and gives the
job **its own inodes** (writes never reach the generation), so the hybrid-copy
design of Result 3 is no longer needed: reflink everything.

### Result 7 — Fresh in a fresh clone (the real job shape), and why mtimes decide it

Each row re-clones master (`--depth 1`) at the in-pod path, copies the
generation onto `fuzz/target`, and builds:

| case | copy | build | `Compiling` |
|---|---|---|---|
| same checkout, no copy (control) | — | 86–94 ms | 0 |
| fresh clone + reflink copy, **no mtime restore** | 51 ms | **27.3–32.5 s** | 3: `console-domain`, `console-application`, the fuzz crate |
| fresh clone + reflink copy + **mtime restore** | 42 ms | **89–101 ms** | **0 — Fresh** |
| fresh clone + hardlink copy + mtime restore | 21 ms | 91–98 ms | 0 — Fresh |
| fresh clone, no seed (cold control) | — | 33.0–41.2 s | 21 |

The mtime restore is `git ls-files -z | xargs -0 touch -h -d 2020-01-01`
(9–10 ms): a fresh checkout stamps every source file with clone time, and for
path crates cargo's fingerprints are mtime-based (Result 3), so **without** it
the generation's dependency artifacts are Fresh but the console's own crates
rebuild — and those three crates are **~80–90 % of the ASAN build** (27–32 of
33–41 s). Path identity (risk #1) is confirmed the other way round too: the
generation built at the in-pod source path was Fresh against a fresh clone at
that path. In production the restore must be keyed on the generation's commit:
restore old mtimes only for files unchanged vs it (`git diff --name-only
<generation-sha>` excluded), so a PR's edits keep fresh mtimes and rebuild.

### Result 8 — PR-shaped: one edited source file

Reflink copy + mtime restore, then one line appended to
`crates/console-domain/src/lib.rs` (a fresh mtime, as a PR edit would carry):
**28.5 s, 3 crates rebuilt** (`console-domain`, `console-application`, fuzz) vs
**41.2 s cold** in the same run. The fuzz graph's local crates are exactly those
three, so the warmed tree's win splits by what a PR touches:

- PR touching neither `console-domain` nor `console-application` (adapters,
  TUI, checks, docs, the majority): compile **33–41 s → ~0.1 s**.
- PR touching the domain or application crate: **41 s → 28.5 s** (the ~12 s of
  dependency compile; the sanitized local crates must rebuild regardless).

Against the Honeycomb `build.check-fuzz.compile` span (**79 s P50, n = 41 since
2026-09-03T23Z**): the span carries ~30 s of nightly + cargo-fuzz install that
`3u3gm2.2` removes independently; of the remaining ~45–50 s of compile (slower
in a job than in this pod — the job container is CPU-limited), the warmed tree
removes all of it for most PRs and about a third for domain/application PRs.

## Go / no-go (acceptance bullet 4)

**GO — for the ASAN fuzz tree, with a reflink seed and the keyed mtime
restore.** `research/009` re-bounded the levers: sccache already takes
`check-nextest` compile to 20 s P50, so a Fresh dev/test tree has ≤ 20 s/job of
headroom and stays **conditional**; the ASAN fuzz compile is the phase sccache
reaches least (0 % hits, Result 5), and it is `ydlant`'s first tree. The design
the pool half fixes:

1. **Build** the generation at the in-pod source path with the job's exact
   command (`cargo +nightly fuzz build`), keyed as in Result 4 (nightly date ×
   fuzz profile × cargo-fuzz `RUSTFLAGS` × features × triple × master commit).
2. **Seed** by `cp -a --reflink=always` onto the XFS work volume at PVC
   provisioning (the provisioner's setup script, beside the uv seed). Copy-on-
   write replaces the hybrid hardlink copy: the job owns every inode, so the
   in-place fingerprint hazard of Result 3 cannot reach the generation, and the
   spec's "a job MUST NOT be able to write any shared cache" holds by
   construction rather than by convention.
3. **Restore mtimes** for files unchanged vs the generation's commit before the
   compile step (~10 ms). Without it the tree saves only the ~12 s of
   dependencies (Result 7).
4. **Key miss → cold**, never a mismatched tree; newest-2 prune per key.

Nothing from this spike is a production change. Implementation is dev-tooling's
(populator, provisioner seed, hook template); the console's only wiring is the
mtime-restore step, and `3u3gm2.2` (baked fuzz toolchain) precedes it because it
is the larger, simpler win on the same job.

## Caveats

Host-local half: vps disk, dev profile, one run of each measurement; the first
experiment's "original still Fresh" check was confounded by reverting the probe
source (that revert, not corruption, caused the one recompile), and the second
run isolated the artifact question. Pool half: four runs of a pod on an idle
node, not a Kueue-admitted job — absolute compile times are faster than a
CPU-limited job container's (the 79 s CI P50 is the number to project from),
while the Fresh/not-Fresh outcomes and copy costs are cargo and filesystem
facts that transfer. The in-image `find` did not enumerate dotfile paths (it
reported 328 files and no `.fingerprint/` for a tree whose freshness travelled
with the copy), so the fingerprint-count and shared-inode re-checks on the ASAN
tree produced nothing; under the reflink design no inode is shared, which is
what makes that re-check unnecessary rather than merely unmeasured.
