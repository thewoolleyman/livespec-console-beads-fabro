# justfile — livespec-console-beads-fabro task runner.

# Pinned livespec-dev-tooling release (latest RELEASE, per the fleet
# dogfooding-pin posture). Single source for the `uv run --with` clause that
# BOTH the commit-refuse-hook installer and its verifier reuse from the wheel —
# so this repo runs the SAME shared machinery the rest of the fleet does, with
# no per-repo copy of the hook body. The release fan-out's bump-pin automation
# targets pyproject [tool.uv.sources] pins; this repo has no pyproject, so this
# pin is hand-bumped (tracked under the fleet-manifest registration follow-up
# livespec-zs22.7.8 / the Pin-freshness concern).
dev_tooling_pin := "livespec-dev-tooling @ git+https://github.com/thewoolleyman/livespec-dev-tooling.git@v0.19.0"

default:
    @just --list

bootstrap:
    #!/usr/bin/env bash
    set -euo pipefail
    primary_path="$(git worktree list --porcelain | awk 'NR == 1 { print $2 }')"
    # Install the canonical STRUCTURAL commit-refuse hook (armed on install; no
    # livespec.primaryPath arming step to forget, so no fail-open window — the
    # root cause of this repo's three direct primary-master commits). Delegated
    # to the shared install-commit-refuse-hooks recipe — the single Installer
    # slot of the baseline Worktree-discipline concern, reused from the
    # livespec-dev-tooling wheel (no per-repo copy of the hook body).
    just install-commit-refuse-hooks
    # Harden the beads tenant-pointer dir to owner-only on first-touch.
    [ -d "${primary_path}/.beads" ] && chmod 700 "${primary_path}/.beads" || true
    # Idempotent worktree-root + mise-trust setup. Every git worktree in
    # the fleet lives under a single per-user root, ~/.worktrees/<repo>/
    # <branch> (per livespec/SPECIFICATION/non-functional-requirements.md
    # §"Worktree root and mise trust"). Registering that root as one of
    # mise's trusted_config_paths makes each freshly created worktree's
    # .mise.toml auto-trusted, so the first `mise exec` inside it never
    # stops on the "config not trusted" prompt — the failure that
    # otherwise wastes a tool round-trip on every new worktree. The grep
    # guard keeps the global ~/.config/mise/config.toml entry single on
    # repeated bootstraps; the value is the absolute $HOME-rooted path so
    # it resolves identically from any invocation site.
    mkdir -p "${HOME}/.worktrees"
    if ! mise settings get trusted_config_paths 2>/dev/null | grep -qF "${HOME}/.worktrees"; then
        mise settings add trusted_config_paths "${HOME}/.worktrees"
    fi
    just ensure-plugins
    just ensure-codex-plugins

# Installer slot of the baseline Worktree-discipline conformance concern
# (livespec non-functional-requirements §"Conformance Pattern"). Installs the
# canonical STRUCTURAL commit-refuse hook to the primary checkout's shared
# .git/hooks/{pre-commit,pre-push,commit-msg}, REUSED from the pinned
# livespec-dev-tooling wheel (`python -m
# livespec_dev_tooling.install_commit_refuse_hooks`) — the single
# canonical-body source, so this repo carries no copy of the hook body to
# drift. The body refuses STRUCTURALLY (exit 1 when `git rev-parse --git-dir`
# == `git rev-parse --git-common-dir`, i.e. a primary checkout; a secondary
# worktree's git-dir differs) UNLESS `git config livespec.sandboxExempt` is
# true, and delegates to mise-managed lefthook everywhere else. Armed on
# install — no livespec.primaryPath step, so no fail-open window. Idempotent;
# worktree-safe (targets the primary's shared hooks dir via git-common-dir).
install-commit-refuse-hooks:
    uv run --no-project --with "{{dev_tooling_pin}}" python -m livespec_dev_tooling.install_commit_refuse_hooks

ensure-plugins:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v claude >/dev/null 2>&1; then
        echo "claude CLI not found; skipping project-scoped Claude plugin install." >&2
        exit 0
    fi
    claude plugin marketplace add --scope project thewoolleyman/livespec
    claude plugin marketplace add --scope project thewoolleyman/livespec-driver-claude
    claude plugin marketplace add --scope project thewoolleyman/livespec-orchestrator-beads-fabro
    claude plugin install -s project livespec@livespec
    claude plugin install -s project livespec@livespec-driver-claude
    claude plugin install -s project livespec-orchestrator-beads-fabro@livespec-orchestrator-beads-fabro

ensure-codex-plugins:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v codex >/dev/null 2>&1; then
        echo "codex CLI not found; skipping host-wide Codex plugin install." >&2
        exit 0
    fi
    codex plugin marketplace add thewoolleyman/livespec
    codex plugin marketplace add thewoolleyman/livespec-driver-codex
    codex plugin marketplace add thewoolleyman/livespec-orchestrator-beads-fabro
    codex plugin marketplace upgrade livespec
    codex plugin marketplace upgrade livespec-driver-codex
    codex plugin marketplace upgrade livespec-orchestrator-beads-fabro
    codex plugin add livespec@livespec
    codex plugin add livespec@livespec-driver-codex
    codex plugin add livespec-orchestrator-beads-fabro@livespec-orchestrator-beads-fabro

check:
    #!/usr/bin/env bash
    set -uo pipefail
    targets=(
        check-format
        check-clippy
        check-test
        check-nextest
        check-coverage
        check-deps
        check-arch
        check-primary-checkout-commit-refuse-hook-installed
    )
    failed=()
    for target in "${targets[@]}"; do
        echo "=== just ${target} ==="
        if ! just "${target}"; then
            failed+=("${target}")
        fi
    done
    if [ "${#failed[@]}" -gt 0 ]; then
        echo "FAILED targets: ${failed[*]}" >&2
        exit 1
    fi

check-format:
    cargo fmt --all --check

check-clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

check-test:
    cargo test --workspace --all-features

check-nextest:
    just ensure-rust-quality-tools
    cargo nextest run --workspace --all-features

check-coverage:
    just ensure-rust-quality-tools
    cargo llvm-cov --workspace --all-features --lib --fail-under-lines 100

check-deps:
    just ensure-rust-quality-tools
    cargo deny check
    cargo machete

check-arch:
    cargo run --quiet --package console-arch-check

# Verifier slot of the baseline Worktree-discipline conformance concern.
# Fail-closed shared check REUSED from the pinned livespec-dev-tooling wheel (no
# re-implementation): it reads the primary checkout's .git/hooks/pre-commit +
# pre-push and asserts each carries the canonical commit-refuse fingerprint (the
# `# livespec commit-refuse hook` marker + an `exit 1` branch + a `git rev-parse
# --git-common-dir` (structural) or legacy `--show-toplevel` detection). Exit 4
# on a missing / non-executable / non-canonical hook; exit 0 when both are
# correct (or when not inside a git work tree). The verifier is
# project-agnostic, so the same module that gates the Python fleet repos gates
# this Rust repo unchanged.
check-primary-checkout-commit-refuse-hook-installed:
    uv run --no-project --with "{{dev_tooling_pin}}" python -m livespec_dev_tooling.checks.primary_checkout_commit_refuse_hook_installed

check-fuzz-smoke:
    just ensure-fuzz-tooling
    cargo +nightly fuzz run event_envelope -- -max_total_time=5

check-mutants-smoke:
    just ensure-mutants-tooling
    cargo mutants --workspace --list --package console-domain --package console-application

check-pre-commit:
    just check-format
    just check-clippy
    just check-arch

check-pre-push:
    just check

ensure-rust-quality-tools:
    ./dev-tooling/ensure-rust-quality-tools.sh core

ensure-fuzz-tooling:
    ./dev-tooling/ensure-rust-quality-tools.sh fuzz

ensure-mutants-tooling:
    ./dev-tooling/ensure-rust-quality-tools.sh mutants
