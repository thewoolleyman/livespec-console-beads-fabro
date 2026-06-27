#!/usr/bin/env bash
# worktree-hydrate.sh — per-ecosystem worktree hydration hook (rust profile).
#
# "Hydrate" means: prepare a freshly-created linked worktree so this repo's
# checks and tooling can run inside it. What that entails is ECOSYSTEM-SPECIFIC
# — there is NO neutral default that fits Python, Rust, and JavaScript — so the
# portable, ecosystem-NEUTRAL core (dev-tooling/worktree-lib.sh, installed from
# the livespec-dev-tooling pack) delegates hydration here. This file is the
# rust-profile specialization for this console repo.
#
# The worktree-lifecycle CORE and the commit-refuse gate stay pure-git and
# ecosystem-neutral; ONLY this hydration script varies by ecosystem.
#
# Resolution: worktree-lib.sh runs, in order, the WORKTREE_HYDRATE_HOOK env
# command if set, else this executable script, else a friendly no-op. Override
# the command without editing this file by exporting
# WORKTREE_HYDRATE_OVERRIDE="<command>" (takes precedence over the default
# below), or replace the whole hook via WORKTREE_HYDRATE_HOOK.
#
# Idempotent and safe to re-run: worktree-lib.sh only invokes this from inside
# a linked worktree, and `./dev-tooling/worktree-lib.sh hydrate` re-runs it.

set -euo pipefail

# This console is a CROSS-ECOSYSTEM repo: a pure-Rust workspace PLUS a thin
# Python/uv side-channel that exists only to REUSE the shared livespec-dev-tooling
# baseline tooling (it runs `uv run python -m livespec_dev_tooling.*` for
# install-commit-refuse-hooks, check-baseline, and check-plugin-resolution). So
# a freshly-created worktree needs BOTH halves materialized:
#
#   * `cargo fetch` — populate the Cargo registry + git deps from the committed
#     Cargo.lock so the Rust build/test/clippy/coverage targets run offline.
#   * `uv sync --all-groups` — create the per-worktree Python `.venv` the shared
#     baseline tooling REQUIRES; without it, the `uv run python -m
#     livespec_dev_tooling.*` recipes fail to import in the worktree.
#
# A Rust worktree still needs a Python `.venv` PURELY for that shared livespec
# baseline tooling — this coupling is the reason the default below runs both.
# Overridable at runtime via WORKTREE_HYDRATE_OVERRIDE.
HYDRATE_CMD="cargo fetch && uv sync --all-groups"
if [ -n "${WORKTREE_HYDRATE_OVERRIDE:-}" ]; then
    HYDRATE_CMD="${WORKTREE_HYDRATE_OVERRIDE}"
fi

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

echo "worktree-hydrate (rust): $HYDRATE_CMD"
eval "${HYDRATE_CMD}"
echo "worktree-hydrate (rust): done."
exit 0
