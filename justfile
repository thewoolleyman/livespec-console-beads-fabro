# justfile — livespec-console-beads-fabro task runner.

# Worktree-discipline pack recipe fragments — OPTIONAL imports (`import?`, NOT
# plain `import`). The two `.just` fragments are gitignored + installed (written
# by `just install-worktree-pack`, never tracked-committed), so they are ABSENT
# in a fresh clone until `just bootstrap` runs. A plain `import` of a missing
# file makes `just` fail to parse the ENTIRE justfile — which would brick `just
# bootstrap` on a fresh clone. The optional `import?` silently no-ops while a
# fragment is absent (its recipes simply aren't available until the installer
# materializes it) and resolves once installed. `worktree.just` adds the
# worktree-lifecycle recipes; `branch-protection.just` adds the server-side
# GitHub branch-protection ruleset recipes (protect-default-branch /
# check-branch-protection) — the server-enforced backstop for the structural
# commit-refuse hook installed by `just bootstrap`.
import? 'dev-tooling/worktree.just'
import? 'dev-tooling/branch-protection.just'

default:
    @just --list

# Prefer this over typing the raw hyphenated binary path (which splits on
# copy-paste). It builds the release binary and launches the interactive TUI
# under the family credential wrapper (injecting the bare BEADS_DOLT_PASSWORD).
# Extra args pass through after `serve` (e.g. `just tui --preview` prints the
# one-shot text summary); `just serve` is an alias for the same recipe.
# Build + launch the interactive operator TUI (the primary launch path).
tui *ARGS:
    cargo build --release --package livespec-console-beads-fabro
    /usr/local/bin/with-livespec-env.sh -- "{{justfile_directory()}}/target/release/livespec-console-beads-fabro" serve {{ARGS}}

alias serve := tui

# Build the standalone release binary for distribution. This is the artifact
# `release-binary.yml` uploads to each GitHub Release (linux
# x86_64-unknown-linux-gnu baseline). SQLite is compiled in via rusqlite's
# `bundled` feature, so the output is a single self-contained executable at
# `target/release/livespec-console-beads-fabro` — no local Rust build or
# system SQLite required by the end user.
build-release:
    cargo build --release --package livespec-console-beads-fabro

# Real-TUI end-to-end gate — the TOP tier of the console test pyramid. Builds the
# RELEASE binary and drives the SHIPPED interactive TUI through a real tmux pane
# (send-keys -> capture-pane -> assert on the rendered screen AND on store side
# effects). This is the FIRST automated coverage of the `run_interactive_tui`
# raw-mode/render path, which every other test compiles out via
# `#[cfg(all(not(test), not(coverage)))]`. Hermetic: the harness points the six
# backing CLIs at fast stubs and isolates the event store, so it needs NO beads
# backend and NO credential wrapper — only `tmux` (which the CI image must
# provide). The E2E test is `#[ignore]`d so the default check-test/check-nextest
# matrix stays green and tmux-free; this target runs it explicitly via
# `--ignored`, pointing the harness at the freshly built release binary through
# LIVESPEC_CONSOLE_E2E_BIN. Prerequisite for the tmux E2E test of every cockpit
# behavior (B1-B8) and the backfill.
check-e2e-tmux:
    cargo build --release --package livespec-console-beads-fabro
    LIVESPEC_CONSOLE_E2E_BIN="{{justfile_directory()}}/target/release/livespec-console-beads-fabro" \
      cargo test --package livespec-console-beads-fabro --test tmux_tui_e2e -- --ignored

# First-touch setup — a THIN delegator to the shipped LOCAL first-touch
# reconcile verb (`livespec_dev_tooling.fleet.local_reconcile`), the
# generalized successor to this recipe's former inline steps (livespec-zs22.8
# M5), PLUS the member-specific worktree-pack tail the verb does not cover.
# Reuse-first: NO copied logic — the verb walks the LOCAL obligation partition
# (`contract.LOCAL_OBLIGATION_ROWS`): mise trust/install, uv sync, the
# structural commit-refuse hooks (subsuming `lefthook install`), the advisory
# `refs/notes/*` refspec, the worktree-root mise-trust entry, the beads
# tenant-dir hardening (resolving the primary via `git rev-parse
# --git-common-dir`, so no `primary_path` precompute is needed), the
# beads-runtime detect-and-guide probes, and project-scoped Claude/Codex plugin
# registration via THIS repo's own `ensure-plugins` / `ensure-codex-plugins`
# recipes. The TAIL below installs the worktree-discipline pack (worktree-lib.sh
# + branch-protection.sh + the `.just` recipe fragments) and keeps the tracked
# worktree-hydrate.sh executable — neither is a verb obligation row, so both
# MUST survive the rewire. The verb's uv-sync row precedes the tail's `uv run`.
bootstrap:
    uv run python -m livespec_dev_tooling.fleet.local_reconcile
    just install-worktree-pack
    chmod +x dev-tooling/worktree-hydrate.sh

# Idempotent: marketplace add / install / update all exit 0 when the target is
# already present / already at latest. The `update` calls after each `install`
# are required for currency — `install` is a no-op when any version is already
# present locally, so without `update` a bumped upstream release never reaches a
# previously-bootstrapped working copy. The SessionStart hook in
# `.claude/settings.json` runs this recipe so each new session's project-scope
# plugins are current; the plugin set mirrors this repo's `.claude/settings.json`
# `enabledPlugins`.
ensure-plugins:
    mise exec -- uv run --no-sync python -m livespec_dev_tooling.fleet.ensure_plugins

ensure-codex-plugins:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v codex >/dev/null 2>&1; then
        echo "codex CLI not found; skipping host-wide Codex plugin install." >&2
        exit 0
    fi
    codex plugin marketplace add thewoolleyman/livespec --ref release
    codex plugin marketplace add thewoolleyman/livespec-driver-codex --ref release
    codex plugin marketplace add thewoolleyman/livespec-orchestrator-beads-fabro --ref release
    codex plugin marketplace upgrade livespec
    codex plugin marketplace upgrade livespec-driver-codex
    codex plugin marketplace upgrade livespec-orchestrator-beads-fabro
    codex plugin add livespec@livespec
    codex plugin add livespec@livespec-driver-codex
    codex plugin add livespec-orchestrator-beads-fabro@livespec-orchestrator-beads-fabro

# Install the canonical livespec commit-refuse hook by REUSING the shared
# livespec-dev-tooling installer module (the SINGLE source of the structural
# hook body; pinned in pyproject.toml). NOT re-implemented in Rust/shell.
# Idempotent; worktree-safe (resolves the primary's shared .git/hooks).
install-commit-refuse-hooks:
    uv run python -m livespec_dev_tooling.install_commit_refuse_hooks

# Install the canonical worktree-discipline PACK (worktree-lib.sh +
# branch-protection.sh + the two `.just` recipe fragments imported above) by
# REUSING the shared livespec-dev-tooling installer module — the SINGLE
# canonical source of all four bodies (pinned in pyproject.toml). NOT a
# repo-vendored copy, so there is ZERO drift-prone pack copy in this repo. This
# is the Installer slot for the pack facet of the Worktree-discipline concern,
# mirroring `install-commit-refuse-hooks` exactly: `bootstrap` delegates to it,
# and CI runs it before `check-baseline` so the verifier VALIDATES the installed
# pack (byte-identical to the package source) rather than skipping it. The
# installer writes the files into `dev-tooling/` and sets the executable bit;
# they are gitignored (installed, not tracked), exactly as the commit-refuse
# hooks are installed into the untracked `.git/hooks/` dir. Idempotent.
install-worktree-pack:
    uv run python -m livespec_dev_tooling.install_worktree_pack

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
        check-behavior-coverage
        check-completeness
        check-baseline
        check-plugin-resolution
        check-doctor-static
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

# Behavioral-coverage gate (clause -> scenario -> test), per
# livespec-console-beads-fabro SPECIFICATION/non-functional-requirements.md
# §"Behavioral Coverage". Ports livespec's spec_clauses gap-id primitive and
# behavior_scenario_link guardrail and adds scenario -> test enforcement over
# the tests/heading-coverage.json link registry. The severity lever
# LIVESPEC_BEHAVIOR_SCENARIO_LINK defaults to `fail` so `just check` and CI block
# on coverage regressions; set it to `warn` only for deliberate local
# report-only runs.
check-behavior-coverage:
    cargo run --quiet --package console-spec-check

# API-to-Settings-to-help-to-doc completeness gate: asserts every key the
# orchestrator declares as API-configurable (its published config-manifest,
# captured at tests/fixtures/orchestrator-config-manifest.json) reaches the
# console's Settings surface, its inline help, and the README settings doc,
# FAILING and naming any key that fell out of lockstep. Consumer-side per the
# No-Circular-Dependency Directive; hermetic (reads the committed capture, no
# live orchestrator). Refresh the capture with `just refresh-config-manifest`.
check-completeness:
    cargo run --quiet --package console-completeness-check

# Refresh the captured orchestrator config-manifest the completeness gate reads,
# from the LIVE orchestrator drive surface, PIN-STAMPED with the current
# .livespec.jsonc compat.pinned. Run after an orchestrator dispatcher key set
# change (part of the orchestrator pin bump); requires the orchestrator plugin +
# credential wrapper on PATH. DRIVE defaults to the family drive CLI. The
# --refresh mode stamps captured_at_pin so the gate fails until the capture is
# refreshed at the new pin.
refresh-config-manifest DRIVE="livespec-orchestrator-drive":
    {{DRIVE}} --action config-manifest --json | cargo run --quiet --package console-completeness-check -- --refresh

# Baseline worktree-discipline verifier — the `baseline` profile's Verifier,
# REUSED from livespec-dev-tooling (NOT re-implemented). Fail-closed: exit 4
# when the canonical structural commit-refuse hook is absent from the primary's
# shared .git/hooks (run `just install-commit-refuse-hooks` to install it). Per
# livespec/SPECIFICATION/non-functional-requirements.md §"Conformance Pattern"
# concern #1 (Worktree-discipline); the check is layout-independent (consumes no
# [tool.livespec_dev_tooling] role keys).
check-baseline:
    uv run python -m livespec_dev_tooling.checks.primary_checkout_commit_refuse_hook_installed

# Baseline plugin-resolution Verifier — the `baseline` profile's second
# concern (cross-harness plugin-resolution), REUSED from livespec-dev-tooling
# (NOT re-implemented). Reads the optional `.livespec.jsonc` `harnesses`
# declaration; fail-closed on a malformed declaration. Per
# livespec/SPECIFICATION/non-functional-requirements.md §"Conformance Pattern"
# concern #2 (Plugin-resolution).
check-plugin-resolution:
    uv run python -m livespec_dev_tooling.checks.plugin_resolution

# livespec core's doctor STATIC phase (reference-discipline + out-of-band
# invariants) against THIS repo's SPECIFICATION/ tree, wired fleet-wide per
# livespec epic livespec-6jfq. core ships the checker: doctor_static.py is
# self-contained (vendored deps + bare python3), so it runs under plain
# python3 and NEVER `uv run`. Resolve core's plugin root via
# LIVESPEC_CORE_PLUGIN_ROOT (CI sets it to a livespec checkout at this repo's
# .livespec.jsonc compat.pinned tag) → else the installed livespec@livespec
# plugin cache (local dev). The two reference-discipline checks
# (no-cross-spec-reference, no-spec-section-citation-in-code) are pure reads;
# doctor-out-of-band-edits is self-healing — on a drifted tree it writes a
# history backfill into the worktree and fails, and committing that backfill
# heals the track; on a clean tree it never fires.
check-doctor-static:
    #!/usr/bin/env bash
    set -euo pipefail
    core_root="${LIVESPEC_CORE_PLUGIN_ROOT:-}"
    if [ -z "$core_root" ]; then
      # Resolve the CURRENT released core build (== marketplace clone HEAD), NOT
      # installed_plugins.json[...]["livespec@livespec"][0] — that per-project list is
      # unordered and its first row can be a different, stale project on a mixed-build
      # host, which the c1k9 currency gate then correctly blocks (livespec-q2me).
      core_root="$(python3 -c 'import subprocess, pathlib; mk = pathlib.Path.home() / ".claude" / "plugins" / "marketplaces" / "livespec"; head = subprocess.run(["git", "-C", str(mk), "rev-parse", "--short=12", "HEAD"], capture_output=True, text=True).stdout.strip().lower(); cache = pathlib.Path.home() / ".claude" / "plugins" / "cache" / "livespec" / "livespec" / head; print(cache if head and (cache / "scripts" / "bin" / "doctor_static.py").is_file() else "")' 2>/dev/null || true)"
    fi
    if [ -z "$core_root" ] || [ ! -f "$core_root/scripts/bin/doctor_static.py" ]; then
      echo "livespec core not found. Set LIVESPEC_CORE_PLUGIN_ROOT to a livespec checkout's .claude-plugin, or install the livespec@livespec plugin (claude plugin install livespec@livespec)." >&2
      exit 1
    fi
    python3 "$core_root/scripts/bin/doctor_static.py" --project-root .

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
