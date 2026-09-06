#!/usr/bin/env bash
# seed-fuzz-target.sh — put the runner pool's warmed ASAN fuzz tree in place
# before `cargo +nightly fuzz build`, so the console's check-fuzz compile phase
# starts Fresh instead of cold (plan optimize-console-builds,
# livespec-console-beads-fabro-ydlant; sized by that plan's research/010).
#
# The pool side (livespec-dev-tooling ci-runner/k3s/phase2/warm-cache, README
# "Target generations"): the warm-cache populator builds master's
# `cargo +nightly fuzz build` tree at this job's own checkout path
# (/__w/<repo>/<repo> — cargo fingerprints embed the SOURCE path) and publishes
# it as a keyed generation; the local-path provisioner reflink-copies the
# published generation into every fresh work volume at
# <volume>/_warm/target/<repo>/asan-fuzz/{tree,.target-manifest.json,.generation}
# (copy-on-write, so the job owns every inode and no shared cache is writable
# from here). This script is the consumer half. It does exactly two things:
#
#   1. MOVES the seeded tree to fuzz/target (a rename within the volume) when
#      the generation's toolchain key (nightly rustc + cargo-fuzz) matches this
#      job's, and
#   2. RESTORES source mtimes for every tracked file UNCHANGED vs the
#      generation's source commit. A fresh checkout stamps every source with
#      clone time, and cargo's local-crate fingerprints are mtime-based, so
#      without this the console's own fuzz-graph crates (~80-90 % of the ASAN
#      build) would recompile although the dependencies were Fresh. Files the
#      PR changed (`git diff --name-only <source_sha>`) KEEP their fresh mtime
#      and rebuild, together with everything that depends on them — cargo's
#      own invalidation, not this script's.
#
# Every path is fail-soft and the script ALWAYS exits 0: no seed (the hosted
# lane, the kill switch, a populator that has not built this key yet), a
# toolchain mismatch, a failed move or an unreachable generation commit all
# fall back to the cold build the job did before this step existed. It prints
# one `seed-fuzz-target: HIT ...` or `seed-fuzz-target: MISS ...` line so a job
# log answers "did the tree land" at a glance. Correctness never depends on the
# seed: a stale or mismatched artifact is rebuilt by cargo's fingerprints, not
# trusted.
set -uo pipefail

log() { printf 'seed-fuzz-target: %s\n' "$*"; }
miss() { log "MISS ($*)"; exit 0; }

workspace="${GITHUB_WORKSPACE:-$(pwd)}"
cd "${workspace}" || miss "workspace ${workspace} unreachable"
repo="${SEED_REPO_NAME:-$(basename "${workspace}")}"
key="asan-fuzz"
seed="${SEED_FUZZ_TARGET_DIR:-/__w/_warm/target/${repo}/${key}}"
manifest="${seed}/.target-manifest.json"
dest="fuzz/target"

[ -d "${seed}/tree" ] || miss "no seed at ${seed}/tree"
[ -r "${manifest}" ] || miss "seed has no manifest at ${manifest}"

# One top-level string field of the manifest, or empty.
field() {
  python3 - "${manifest}" "$1" <<'PY' 2>/dev/null || true
import json, sys
with open(sys.argv[1]) as f:
    print(json.load(f).get(sys.argv[2]) or "")
PY
}
source_sha="$(field source_sha)"
toolchain="$(field toolchain)"
generation="$(cat "${seed}/.generation" 2>/dev/null || field generation)"
[ -n "${source_sha}" ] && [ -n "${toolchain}" ] || miss "manifest lacks source_sha/toolchain"

# The generation KEY, spelled exactly as the populator spells it.
job_toolchain="${FUZZ_TOOLCHAIN_ID:-}"
if [ -z "${job_toolchain}" ]; then
  rustc_ver="$(rustc +nightly --version 2>/dev/null | awk '{print $2}')"
  fuzz_ver="$(cargo +nightly fuzz --version 2>/dev/null | awk '{print $2}')"
  [ -n "${rustc_ver}" ] && [ -n "${fuzz_ver}" ] || miss "nightly or cargo-fuzz not resolvable in this job"
  job_toolchain="nightly-${rustc_ver}@cargo-fuzz-${fuzz_ver}"
fi
[ "${job_toolchain}" = "${toolchain}" ] \
  || miss "toolchain key differs: seed ${toolchain}, job ${job_toolchain}"

[ -e "${dest}" ] && miss "${dest} already exists in this checkout"
mkdir -p "$(dirname "${dest}")"
t0=$(date +%s%N)
if ! mv "${seed}/tree" "${dest}"; then
  rm -rf "${dest}"
  miss "moving ${seed}/tree to ${dest} failed"
fi
move_ms=$(( ($(date +%s%N) - t0) / 1000000 ))

# Restore mtimes for sources unchanged vs the generation's commit. The
# commit must be readable to diff against it; a shallow checkout fetches it
# by sha at depth 1 (GitHub serves reachable commits by sha). If it cannot be
# reached the tree still saves the sanitized dependencies; only the local
# crates rebuild — reported as a partial hit, not hidden.
restored=""
if git cat-file -e "${source_sha}^{commit}" 2>/dev/null \
   || git fetch --quiet --depth=1 origin "${source_sha}" 2>/dev/null; then
  changed="$(mktemp)"
  if git diff --name-only "${source_sha}" > "${changed}" 2>/dev/null; then
    # Tracked files not in the changed set get an mtime OLDER than every
    # fingerprint the generation carries; changed files keep clone time.
    n_restored=$(git ls-files -z \
      | grep -z -v -x -F -f "${changed}" \
      | tee >(xargs -0 -r touch -h -d '2020-01-01T00:00:00Z' 2>/dev/null) \
      | tr -cd '\0' | wc -c)
    restored="restored mtimes for ${n_restored} unchanged files, $(wc -l < "${changed}") changed vs ${source_sha:0:8}"
  fi
  rm -f "${changed}"
fi
[ -n "${restored}" ] || restored="mtime restore SKIPPED (commit ${source_sha:0:8} unreachable; local crates rebuild)"

log "HIT generation=${generation:-?} toolchain=${toolchain} source_sha=${source_sha:0:8} moved in ${move_ms} ms; ${restored}"
exit 0
