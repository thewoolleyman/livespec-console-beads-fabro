# justfile — livespec-console-beads-fabro task runner.

default:
    @just --list

bootstrap:
    #!/usr/bin/env bash
    set -euo pipefail
    primary_path="$(git worktree list --porcelain | awk 'NR == 1 { print $2 }')"
    git_common_dir="$(git rev-parse --git-common-dir)"
    git config --file "${git_common_dir}/config" livespec.primaryPath "${primary_path}"
    mkdir -p "${git_common_dir}/hooks"
    cp dev-tooling/git-hook-wrapper.sh "${git_common_dir}/hooks/pre-commit"
    cp dev-tooling/git-hook-wrapper.sh "${git_common_dir}/hooks/pre-push"
    cp dev-tooling/git-hook-wrapper.sh "${git_common_dir}/hooks/commit-msg"
    chmod +x "${git_common_dir}/hooks/pre-commit" "${git_common_dir}/hooks/pre-push" "${git_common_dir}/hooks/commit-msg"
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
