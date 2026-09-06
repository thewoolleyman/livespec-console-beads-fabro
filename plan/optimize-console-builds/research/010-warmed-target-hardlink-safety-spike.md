# 010 — Warmed `target/` generations: hardlink safety, relocation freshness, copy cost (z2siyn spike, host-local half)

The `z2siyn` spike exists to burn down two risks before `ydlant` invests in
warmed `target/` generations (`research/006` §b): **path identity** (a generation
built anywhere but the in-pod path silently never hits) and **copy cost /
hardlink safety** (a multi-GB tree copied per job may eat the win, and cargo's
in-place fingerprint rewrites may corrupt a hardlink-shared generation). This
note is the half of that spike that needs **no pool**: it is a cargo-behaviour
question, answered on the vps in an isolated scratch clone. The pool half —
the ASAN fuzz tree's real size and copy time on the NVMe work volume, at the
in-pod path, with a Fresh confirmation inside a job — is deferred to a window
the maintainer confirms is free (console CI moved to GitHub-hosted
2026-09-06T00:44Z for pool maintenance; `research/008` Layer 3).

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

## Go / no-go (acceptance bullet 4)

**GO — for the ASAN fuzz tree, with the hybrid copy.** `research/009` re-bounded
the levers: sccache already takes `check-nextest` compile to 20 s P50, so a Fresh
dev/test tree has ≤ 20 s/job of headroom and is **conditional**; the ASAN fuzz
compile (75 s P50, the phase sccache reaches least) is where a warmed tree still
wins, and it is already `ydlant`'s first tree. Hardlink the artifacts, byte-copy
the fingerprints, build at the in-pod source path, key as above.

**Still owed (pool half, needs a maintainer-confirmed free window):** the ASAN
tree's size and `cp -a`/`cp -al` times on the NVMe work volume; a build at
`/__w/livespec-console-beads-fabro/livespec-console-beads-fabro` and a Fresh
confirmation from inside a real job; and the resulting `build.check-fuzz.compile`
against the current 75 s sccache-hit compile.

## Caveats

vps disk, not the pool's NVMe (ratios robust, absolutes indicative). Dev profile,
not the ASAN profile (larger tree, same cargo mechanics). One host, one run of
each measurement. The first experiment's "original still Fresh" sanity check was
confounded by reverting the probe source (that revert, not corruption, caused the
one recompile); the second run isolated the artifact question cleanly.
