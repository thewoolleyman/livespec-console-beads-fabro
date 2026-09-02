#!/usr/bin/env bash
# local-cache-evict.sh — bounded, AGE-based eviction of the local Rust build caches.
#
# Plan optimize-console-builds, Phase 2 local (livespec-console-beads-fabro-uybgug).
# Charter rule (plan/optimize-console-builds/research/001, requirement 2): every
# cache tier is bounded by AGE or STALENESS, never by a size cap, and eviction
# never deletes still-useful (recently used) cache. This script is that policy
# for the developer host. Nothing here is triggered by a size threshold; the one
# knob is --days N (default 14): anything unused for longer than N days goes.
#
# Four tiers, in this order:
#   1. Orphaned worktrees — `worktree-lib.sh reap` removes worktrees whose branch
#      is merged into origin/master or gone (staleness by merge state), which
#      also frees their ~4 GB target/ dirs. Unmerged worktrees are never touched.
#   2. cargo target/ dirs — `cargo sweep --time N` over the primary checkout and
#      every LIVE worktree removes artifacts not used for N days; then
#      `cargo sweep --installed` removes artifacts built by toolchains that are
#      no longer installed. Both key on cargo's own fingerprints, so a warm
#      build after the pass stays warm.
#   3. cargo registry — crate archives (~/.cargo/registry/cache) that NO live
#      Cargo.lock ON THE HOST (this repo, fuzz/, every worktree, every project
#      under LOCAL_CACHE_EVICT_LOCK_ROOTS — the registry is shared) references
#      AND whose last access is older than N days, plus their extracted src/
#      twins. A locked dependency is live even when unread for weeks (a warm
#      build never touches it), so lockfile membership is the liveness signal
#      and age is the staleness signal; both must hold. The root fs mounts with
#      relatime, so atime is usable at day granularity. cargo re-downloads
#      anything it needs again.
#   4. rustup toolchains — toolchains that are neither the repo pin
#      (rust-toolchain.toml channel), the fuzz `nightly` channel, nor the rustup
#      default (each matched exactly, as <channel> or <channel>-<host triple>;
#      a dated nightly is NOT protected by `nightly`), AND whose libstd rlib was
#      last read (i.e. something was compiled with it) more than N days ago.
#
# Default is a DRY RUN that prints what would go. Pass --execute to delete.
# Sizes (du) of every tier are printed before and after so a pass is measurable.
set -euo pipefail

usage() {
    cat <<'USAGE'
usage: scripts/local-cache-evict.sh [--execute] [--days N] [--repo PATH]
  --execute     actually delete; without it, report only (dry run)
  --days N      age threshold in days (default 14); never a size cap
  --repo PATH   primary checkout to sweep (default: this script's repo root)
USAGE
}

execute=0
days=14
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
while [ $# -gt 0 ]; do
    case "$1" in
        --execute) execute=1 ;;
        --days) shift; days="${1:?--days needs a value}" ;;
        --repo) shift; repo_root="${1:?--repo needs a path}" ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
    shift
done
case "$days" in
    ''|*[!0-9]*) echo "--days must be a non-negative integer, got '$days'" >&2; exit 2 ;;
esac

cargo_home="${CARGO_HOME:-$HOME/.cargo}"
rustup_home="${RUSTUP_HOME:-$HOME/.rustup}"
worktree_root="$HOME/.worktrees/$(basename "$repo_root")"
mode_label="DRY RUN"
[ "$execute" = 1 ] && mode_label="EXECUTE"

log() { printf '[local-cache-evict] %s\n' "$*"; }

size_of() {
    if [ -e "$1" ]; then
        /usr/bin/du -sh "$1" 2>/dev/null | cut -f1
    else
        echo "-"
    fi
}

report_sizes() {
    local label="$1"
    log "== sizes ($label) =="
    printf '  %-8s %s\n' "$(size_of "$repo_root/target")" "$repo_root/target"
    local wt
    for wt in "$worktree_root"/*/; do
        [ -d "$wt/target" ] || continue
        printf '  %-8s %s\n' "$(size_of "$wt/target")" "${wt%/}/target"
    done
    printf '  %-8s %s\n' "$(size_of "$cargo_home/registry")" "$cargo_home/registry"
    printf '  %-8s %s\n' "$(size_of "$rustup_home/toolchains")" "$rustup_home/toolchains"
    /usr/bin/df -h / | tail -1 | awk '{printf "  root fs: %s used of %s, %s free (%s)\n", $3, $2, $4, $5}'
}

ensure_cargo_sweep() {
    if command -v cargo-sweep >/dev/null 2>&1; then
        return
    fi
    log "cargo-sweep missing; installing (cargo install --locked cargo-sweep)"
    cargo install --locked cargo-sweep
}

# ---- 1. orphaned worktrees ----------------------------------------------------
reap_worktrees() {
    local lib="$repo_root/dev-tooling/worktree-lib.sh"
    if [ ! -x "$lib" ]; then
        log "tier 1 worktrees: SKIP — $lib absent (run 'just install-worktree-pack')"
        return
    fi
    log "tier 1 worktrees: reaping merged/gone-branch worktrees ($mode_label)"
    if [ "$execute" = 1 ]; then
        (cd "$repo_root" && "$lib" reap --execute) || log "tier 1 worktrees: reap reported a problem (continuing)"
    else
        (cd "$repo_root" && "$lib" reap) || log "tier 1 worktrees: reap reported a problem (continuing)"
    fi
}

# ---- 2. cargo target dirs -----------------------------------------------------
sweep_target() {
    local dir="$1"
    [ -d "$dir/target" ] || return 0
    local dry=()
    [ "$execute" = 1 ] || dry=(--dry-run)
    log "tier 2 target: cargo sweep --time $days ($mode_label) in $dir"
    (cd "$dir" && cargo sweep "${dry[@]}" --time "$days") || log "tier 2 target: sweep --time failed in $dir (continuing)"
    (cd "$dir" && cargo sweep "${dry[@]}" --installed) || log "tier 2 target: sweep --installed failed in $dir (continuing)"
}

sweep_targets() {
    ensure_cargo_sweep
    sweep_target "$repo_root"
    local wt
    for wt in "$worktree_root"/*/; do
        [ -d "$wt" ] || continue
        sweep_target "${wt%/}"
    done
}

# ---- 3. cargo registry --------------------------------------------------------
live_lock_entries() {
    # name-version for every [[package]] in every live lockfile ON THE HOST.
    # ~/.cargo/registry is shared by every Rust checkout here, so liveness is
    # host-wide: this repo, every worktree, and every project under the lock
    # roots (LOCAL_CACHE_EVICT_LOCK_ROOTS, colon-separated; depth <= 3).
    local roots="${LOCAL_CACHE_EVICT_LOCK_ROOTS:-/data/projects:$HOME/.worktrees}"
    local root
    {
        printf '%s\n' "$repo_root/Cargo.lock" "$repo_root/fuzz/Cargo.lock"
        IFS=: read -r -a root_list <<< "$roots"
        for root in "${root_list[@]}"; do
            [ -d "$root" ] || continue
            find "$root" -maxdepth 3 -name Cargo.lock -type f 2>/dev/null
        done
    } | sort -u | while IFS= read -r lock; do
        [ -f "$lock" ] || continue
        awk '/^name = /{n=$3} /^version = /{v=$3; gsub(/"/,"",n); gsub(/"/,"",v); print n "-" v}' "$lock"
    done | sort -u
}

trim_registry() {
    local cache="$cargo_home/registry/cache"
    if [ ! -d "$cache" ]; then
        log "tier 3 registry: SKIP — $cache absent"
        return
    fi
    local live_file
    live_file="$(mktemp)"
    live_lock_entries > "$live_file"
    log "tier 3 registry: archives unreferenced by any live Cargo.lock ($(wc -l < "$live_file") live entries) AND not accessed for > $days days ($mode_label)"
    local count=0 bytes=0 crate src_dir stem reg
    while IFS= read -r -d '' crate; do
        reg="$(basename "$(dirname "$crate")")"
        stem="$(basename "$crate" .crate)"
        if grep -qxF -- "$stem" "$live_file"; then
            continue
        fi
        count=$((count + 1))
        bytes=$((bytes + $(stat -c %s "$crate")))
        src_dir="$cargo_home/registry/src/$reg/$stem"
        if [ "$execute" = 1 ]; then
            rm -f -- "$crate"
            [ -d "$src_dir" ] && rm -rf -- "$src_dir"
        else
            printf '  would remove %s' "$crate"
            [ -d "$src_dir" ] && printf ' (+ src/%s/%s)' "$reg" "$stem"
            printf '\n'
        fi
    done < <(find "$cache" -mindepth 2 -maxdepth 2 -type f -name '*.crate' -atime "+$days" -print0)
    rm -f -- "$live_file"
    log "tier 3 registry: $count crate archive(s), $((bytes / 1024 / 1024)) MiB"
}

# ---- 4. rustup toolchains -----------------------------------------------------
keep_toolchain() {
    # A kept CHANNEL protects exactly its host-triple toolchain (nightly ->
    # nightly-<triple>), never a dated sibling such as nightly-2026-04-14-<triple>.
    local name="$1" keep
    for keep in "${keep_channels[@]}"; do
        if [ "$name" = "$keep" ] || [ "$name" = "$keep-$host_triple" ]; then
            return 0
        fi
    done
    return 1
}

sweep_toolchains() {
    if ! command -v rustup >/dev/null 2>&1; then
        log "tier 4 toolchains: SKIP — rustup not on PATH"
        return
    fi
    keep_channels=()
    host_triple="$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')"
    local pin
    pin="$(sed -n 's/^channel *= *"\(.*\)"/\1/p' "$repo_root/rust-toolchain.toml" 2>/dev/null | head -1)"
    [ -n "$pin" ] && keep_channels+=("$pin")
    keep_channels+=("nightly")   # cargo +nightly fuzz (justfile check-fuzz)
    local default_tc
    default_tc="$(rustup default 2>/dev/null | awk '{print $1}')"
    [ -n "$default_tc" ] && keep_channels+=("$default_tc")
    log "tier 4 toolchains: keep = ${keep_channels[*]}; others unused > $days days ($mode_label)"
    # Last-USE signal is the toolchain's libstd rlib, read only when something is
    # actually compiled/linked with it. NOT bin/rustc: tooling that merely
    # enumerates toolchains (`cargo sweep --installed`, rustup) runs `rustc -vV`
    # and refreshes that atime, which made every toolchain look used today.
    local tc name probe
    for tc in "$rustup_home"/toolchains/*/; do
        name="$(basename "$tc")"
        keep_toolchain "$name" && continue
        probe="$(find "$tc/lib/rustlib/$host_triple/lib" -maxdepth 1 -name 'libstd-*.rlib' -print -quit 2>/dev/null)"
        [ -n "$probe" ] || probe="$tc/bin/rustc"
        [ -e "$probe" ] || continue
        if [ -n "$(find "$probe" -atime "+$days" -print)" ]; then
            if [ "$execute" = 1 ]; then
                log "tier 4 toolchains: uninstalling $name ($(size_of "$tc"), last compile $(stat -c %x "$probe" | cut -d. -f1))"
                rustup toolchain uninstall "$name" || log "tier 4 toolchains: uninstall $name failed (continuing)"
            else
                printf '  would uninstall %s (%s, last compile %s)\n' "$name" "$(size_of "$tc")" "$(stat -c %x "$probe" | cut -d. -f1)"
            fi
        fi
    done
}

log "mode=$mode_label days=$days repo=$repo_root worktrees=$worktree_root"
report_sizes before
reap_worktrees
sweep_toolchains
sweep_targets
trim_registry
report_sizes after
[ "$execute" = 1 ] || log "dry run complete — re-run with --execute to apply"
