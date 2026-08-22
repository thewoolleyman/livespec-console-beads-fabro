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
# Extra args pass through after `serve` (e.g. `just tui -- --preview` prints the
# one-shot text summary); `just serve` is an alias for the same recipe.
# Build + launch the interactive operator TUI (the primary launch path).
# Ambient LIVESPEC_CONSOLE_* overrides are re-injected INSIDE the credential
# wrapper because the wrapper runs with a clean environment.
# errexit is deliberately omitted; the build is guarded before argv pass-through.
[positional-arguments]
tui *ARGS:
    #!/usr/bin/env bash
    set -uo pipefail
    cargo build --release --package livespec-console-beads-fabro || exit $?
    env_args=()
    while IFS= read -r name; do
      env_args+=("$name=${!name}")
    done < <(compgen -e LIVESPEC_CONSOLE_ | sort)
    /usr/local/bin/with-livespec-env.sh -- env "${env_args[@]}" ./target/release/livespec-console-beads-fabro serve "$@"

alias serve := tui

# Build the standalone release binary for distribution. This is the artifact
# `release-binary.yml` uploads to each GitHub Release (linux
# x86_64-unknown-linux-gnu baseline). SQLite is compiled in via rusqlite's
# `bundled` feature, so the output is a single self-contained executable at
# `target/release/livespec-console-beads-fabro` — no local Rust build or
# system SQLite required by the end user.
build-release:
    cargo build --release --package livespec-console-beads-fabro

# Regenerate the committed operator key/action reference from ACTION_REGISTRY.
# errexit is deliberately omitted; each filesystem/build step is guarded directly.
generate-key-action-reference:
    #!/usr/bin/env bash
    set -uo pipefail
    mkdir -p docs/reference || exit $?
    cargo run --quiet --package livespec-console-beads-fabro -- docs key-action-reference > docs/reference/key-action-reference.md || exit $?

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
#
# The trailing guard defends against a silent pass: `cargo test -- --ignored`
# exits 0 even when it matches ZERO tests (e.g. if the `#[ignore]` attribute is
# ever dropped, the run reports "0 passed ... N filtered out" and still greens).
# So we require the summary to report at least one passing test, failing the gate
# if the E2E suite silently ran nothing.
# errexit is deliberately omitted so captured test output is emitted on failure.
check-e2e-tmux:
    #!/usr/bin/env bash
    set -uo pipefail
    cargo build --release --package livespec-console-beads-fabro || exit $?
    # Respect CARGO_TARGET_DIR (the self-hosted CI runner redirects it to a shared
    # cache, e.g. /opt/ci-cache/target); when it is absent, keep the fallback
    # absolute so the shebang recipe finds the release binary from any invocation
    # directory without reintroducing just interpolation.
    target_dir="${CARGO_TARGET_DIR:-$(pwd)/target}"
    output="$(LIVESPEC_CONSOLE_E2E_BIN="$target_dir/release/livespec-console-beads-fabro" \
      cargo test --package livespec-console-beads-fabro --test tmux_tui_e2e -- --ignored 2>&1)"
    status=$?
    echo "$output"
    if [ "$status" -ne 0 ]; then exit "$status"; fi
    if ! grep -qE '[1-9][0-9]* passed' <<<"$output"; then
      echo "ERROR: the tmux E2E suite ran ZERO tests (0 passed) — did the #[ignore] get dropped, or the test rename?" >&2
      exit 1
    fi

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
# errexit is deliberately omitted; each first-touch command is guarded directly.
bootstrap:
    #!/usr/bin/env bash
    set -uo pipefail
    uv run python -m livespec_dev_tooling.fleet.local_reconcile || exit $?
    just install-worktree-pack || exit $?
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

# errexit is deliberately omitted; the optional codex probe can skip cleanly.
ensure-codex-plugins:
    #!/usr/bin/env bash
    set -uo pipefail
    if ! command -v codex >/dev/null 2>&1; then
        echo "codex CLI not found; skipping host-wide Codex plugin install." >&2
        exit 0
    fi
    codex plugin marketplace add thewoolleyman/livespec --ref release || exit $?
    codex plugin marketplace add thewoolleyman/livespec-driver-codex --ref release || exit $?
    codex plugin marketplace add thewoolleyman/livespec-orchestrator-beads-fabro --ref release || exit $?
    codex plugin marketplace upgrade livespec || exit $?
    codex plugin marketplace upgrade livespec-driver-codex || exit $?
    codex plugin marketplace upgrade livespec-orchestrator-beads-fabro || exit $?
    codex plugin add livespec@livespec || exit $?
    codex plugin add livespec@livespec-driver-codex || exit $?
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

# Factory-boundary helper: fail if the current branch changes GitHub workflow
# files. Deliberately OUTSIDE the canonical `check` aggregate — the factory
# janitor lane invokes it explicitly, ahead of `check`, so an implementation
# branch never reaches PR publication carrying `.github/workflows/` edits. The
# fleet App's push token is contents-only, so such a branch would otherwise be
# discovered by GitHub's own push rejection instead of being reported here and
# routed to maintainer-side landing.
#
# ADOPTED 2026-07-28 because the orchestrator plugin's `_DEFAULT_JANITOR` is
# `just check-no-workflow-edits install-worktree-pack check`, and this repo
# defined the latter two but not this one — so every post-merge janitor died
# with "Justfile does not contain recipe `check-no-workflow-edits`" and stranded
# the work-item at `active` after its PR had already merged (observed on
# livespec-console-beads-fabro-dm5f7q, PR #466 merged as 77ed854). Body mirrors
# the peer at livespec-dev-tooling `justfile` rather than the orchestrator's
# `workflow_guard.py` form, which resolves a `.claude-plugin/` script this
# consumer repo does not carry.
# errexit is deliberately omitted to report workflow-file violations explicitly.
check-no-workflow-edits:
    #!/usr/bin/env bash
    set -uo pipefail
    base=$(git merge-base HEAD origin/master 2>/dev/null || git merge-base HEAD master) || exit $?
    if git diff --quiet --name-only "$base"...HEAD -- .github/workflows; then
        exit 0
    fi
    echo "ERROR: factory branches must not modify .github/workflows/ files" >&2
    git diff --name-only "$base"...HEAD -- .github/workflows >&2
    exit 1

# errexit is deliberately omitted so all checks run before failure reporting.
check:
    #!/usr/bin/env bash
    set -uo pipefail
    # Canonical workspace test harness: nextest. It exercises the same Rust test
    # inventory as `cargo test` with better scheduling/reporting, so the local
    # aggregate does not run both harnesses serially. Coverage remains a separate
    # instrumented pass because llvm-cov needs its own profile and profdata.
    targets=(
        check-format
        check-clippy
        check-nextest
        check-coverage
        check-deps
        check-arch
        check-behavior-coverage
        check-completeness
        check-spec-governance-default-block
        check-charters
        check-baseline
        check-shell-quality
        check-plan-no-tombstone
        check-plan-anchor-declared
        check-plugin-resolution
        check-doctor-static
        check-e2e-tmux
        check-ci-parity
        check-fork-drift
        check-red-green-replay
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

# Standalone fallback and legacy CI target. `just check` uses check-nextest as
# the canonical non-coverage harness so local full-suite execution stays at two
# passes: nextest plus llvm-cov.
# errexit is deliberately omitted; the tool installer is guarded directly.
check-nextest:
    #!/usr/bin/env bash
    set -uo pipefail
    just ensure-rust-quality-tools || exit $?
    cargo nextest run --workspace --all-features

# Line coverage. The 100% requirement for ATTRIBUTABLE lines is unchanged; a
# single nameable uncovered line still fails. What `--fail-under-lines 100`
# cannot express is llvm-cov counting misses no listing surface can NAME, so the
# summary and the listing are compared explicitly and that one signature is
# capped by a recorded, reasoned disposition. See
# tests/fixtures/coverage-unnameable-disposition.json and ledger item
# livespec-console-beads-fabro-3yx.
#
# Coverage-pincer reminder: do not satisfy llvm-cov by fighting another gate. If
# a grouped or-pattern arm (`A | B => ...`) is reported uncovered, keep the arm
# grouped and exercise the untaken alternative; splitting identical arms trips
# clippy `match_same_arms`. If rustfmt forces a failure-only assertion message
# onto its own uncovered line, shrink the argument list so the assertion stays on
# one line; a passing test suite cannot exercise a failing assert message. Never
# weaken the coverage gate or relax the formatter/clippy gate to escape this
# family of misses.
# errexit is deliberately omitted so coverage output is printed before failure.
check-coverage:
    #!/usr/bin/env bash
    set -uo pipefail
    just ensure-rust-quality-tools || exit $?
    export_json="$(mktemp)"
    missing_txt="$(mktemp)"
    trap 'rm -f "${export_json}" "${missing_txt}"' EXIT
    # One instrumented run; the listing reuses its profdata.
    cargo llvm-cov --workspace --all-features --lib --json --output-path "${export_json}" || exit 1
    cargo llvm-cov report --show-missing-lines | tee "${missing_txt}" || exit 1
    python3 dev-tooling/coverage-gate.py \
        "${export_json}" "${missing_txt}" tests/fixtures/coverage-unnameable-disposition.json

# errexit is deliberately omitted; dependency checks are guarded directly.
check-deps:
    #!/usr/bin/env bash
    set -uo pipefail
    just ensure-rust-quality-tools || exit $?
    cargo deny check || exit $?
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
# console's Settings surface, its inline help, and the settings doc
# (docs/detailed-usage.md, per the spec's User Documentation Contract),
# FAILING and naming any key that fell out of lockstep. Consumer-side per the
# No-Circular-Dependency Directive; hermetic (reads the committed capture, no
# live orchestrator). Refresh the capture with `just refresh-config-manifest`.
check-completeness:
    cargo run --quiet --package console-completeness-check

check-spec-governance-default-block:
    uv run python dev-tooling/check-spec-governance-default-block.py

# errexit is deliberately omitted so every defect is printed before the gate fails.
check-charters:
    #!/usr/bin/env bash
    set -uo pipefail
    uv run python - <<'PY'
    from pathlib import Path
    import sys

    from returns.pipeline import is_successful

    from livespec_dev_tooling.charters import CHARTER_GLOBS, DETECTORS, charters_in, defects_in


    EXPECTED_CHARTERS = [
        ".ai/supervisor-protocol.md",
        "plan/archive/console-happy-path-mvp/supervisor-handoff.md",
    ]


    root = Path(".")
    result = charters_in(root=root)
    if not is_successful(result):
        failure = result.failure()._inner_value
        print(f"charter scan failed at {failure.path}: {failure.detail}", file=sys.stderr)
        sys.exit(1)

    charters = [path.relative_to(root).as_posix() for path in result.unwrap()._inner_value]
    print(f"charter globs: {', '.join(CHARTER_GLOBS)}")
    print(f"detectors: {len(DETECTORS)}")
    print(f"charters scanned: {len(charters)}")
    for charter in charters:
        print(f"  {charter}")

    if charters != EXPECTED_CHARTERS:
        print(
            "expected exactly these charters: "
            + ", ".join(EXPECTED_CHARTERS),
            file=sys.stderr,
        )
        sys.exit(1)

    defects = []
    for charter in charters:
        text = (root / charter).read_text(encoding="utf-8")
        defects.extend(f"{charter}: {defect}" for defect in defects_in(text=text))

    print(f"defects: {len(defects)}")
    if defects:
        print("\n".join(defects), file=sys.stderr)
        sys.exit(1)
    PY

# Refresh the captured orchestrator config-manifest the completeness gate reads,
# from the LIVE orchestrator drive surface, DIGEST-STAMPED with the declared key
# set. Run after an orchestrator dispatcher key set change; requires the
# orchestrator plugin + credential wrapper on PATH. DRIVE defaults to the family
# drive CLI via the DRIVE environment variable. The --refresh mode stamps
# captured_key_set_digest so the gate fails until a changed key set is refreshed.
# errexit is deliberately omitted; pipefail owns the manifest refresh pipeline.
refresh-config-manifest:
    #!/usr/bin/env bash
    set -uo pipefail
    drive="${DRIVE:-livespec-orchestrator-drive}"
    "$drive" --action config-manifest --json | cargo run --quiet --package console-completeness-check -- --refresh

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
# Plan-lifecycle tombstone ban: a topic must not exist at BOTH
# plan/<topic>/ and plan/archive/<topic>/. Fail-closed with no opt-in
# lever. Wired by hand because this repo carries no canonical
# aggregate-completeness gate, so a new canonical check never arrives on
# its own with a dev-tooling pin bump.
#
# DO NOT spell that gate's `check-`-prefixed slug anywhere in this file
# until the repo actually wires it. The dev-tooling bump-pin action's
# opt-in probe is `grep -q '<that slug>' justfile` over the WHOLE file, so
# a prose mention alone makes the action believe this repo carries the
# gate. It then runs both canonical reconcilers, the justfile one adopts
# every canonical slug into `check:`, and the ci.yml one hard-fails for
# lack of a `strategy.matrix.target:` list carrying the same slug. That is
# what froze this repo's dev-tooling pin at v1.19.0 from 2026-08-04 to
# 2026-08-18 (livespec-dev-tooling-y23f).
check-plan-no-tombstone:
    uv run python -m livespec_dev_tooling.checks.plan_no_tombstone

check-plan-anchor-declared:
    uv run python -m livespec_dev_tooling.checks.plan_anchor_declared

check-plugin-resolution:
    uv run python -m livespec_dev_tooling.checks.plugin_resolution

check-ci-parity:
    cargo run --quiet --package console-ci-parity-check

# Canonical fleet shell-quality verifier: ShellCheck 0.11.0 over tracked shell
# files plus the governed justfile recipe policy. The recipe body is deliberately
# a one-line module invocation so the gate also validates its own wiring.
check-shell-quality:
    uv run python -m livespec_dev_tooling.checks.shell_quality

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
# Fork-drift gate over the committed `.fabro/workflows/implement-work-item/`
# fork. Upstream once fixed the pr-stage publish leg (bd-ib-qq7f, PR #905) and
# our fork silently kept the broken one for three weeks, because NOTHING
# asserted the leg was present. This is that assertion. It pins UPSTREAM digests
# per file rather than asserting byte-equality against an allowlist — six of the
# seven files diverge deliberately, so an allowlist wide enough to tolerate that
# would pass on anything. Re-pin with `just refresh-fork-upstream-pins`.
check-fork-drift:
    cargo run --quiet --package console-fork-drift-check

check-red-green-replay:
    cargo run --quiet --package console-red-green-replay-check

# Re-capture upstream digests for the fork after a conscious review of what
# upstream changed. Needs the orchestrator plugin installed; preserves each
# pin's `reason`.
refresh-fork-upstream-pins:
    cargo run --quiet --package console-fork-drift-check -- --refresh

# errexit is deliberately omitted; core resolution steps are guarded directly.
check-doctor-static:
    #!/usr/bin/env bash
    set -uo pipefail
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

# errexit is deliberately omitted; fuzz tooling install is guarded directly.
check-fuzz-smoke:
    #!/usr/bin/env bash
    set -uo pipefail
    just ensure-fuzz-tooling || exit $?
    cargo +nightly fuzz run event_envelope -- -max_total_time=5

# errexit is deliberately omitted; mutants tooling install is guarded directly.
check-mutants-smoke:
    #!/usr/bin/env bash
    set -uo pipefail
    just ensure-mutants-tooling || exit $?
    cargo mutants --workspace --list --package console-domain --package console-application

# Merge-gate mutation run (livespec-console-beads-fabro-txtzn5.10). Scoped to
# the code a change actually touches via --in-diff, because a FULL sweep of
# these two packages is 1456 mutants and takes at least 46 minutes — measured,
# and that figure is a floor. DELIBERATELY ABSENT from the `just check`
# aggregate: SPECIFICATION/non-functional-requirements.md ratifies that
# `just check` MUST NOT include mutation runs.
#
# Exit 2 from cargo-mutants means a mutant SURVIVED; that must fail the gate.
# Before silencing one, classify it — see .cargo/mutants.toml.
#
# An empty diff is a legitimate no-op here, not a fault.
# errexit is deliberately omitted; the tooling install and the diff are guarded directly.
[positional-arguments]
check-mutants base="origin/master":
    #!/usr/bin/env bash
    set -uo pipefail
    just ensure-mutants-tooling || exit $?
    diff_file="$(mktemp)"
    trap 'rm -f "$diff_file"' EXIT
    if ! git diff "$1...HEAD" > "$diff_file"; then
      echo "check-mutants: cannot diff against $1 — fetch it first" >&2
      exit 1
    fi
    if [ ! -s "$diff_file" ]; then
      echo "check-mutants: empty diff vs $1; no mutants to test"
      exit 0
    fi
    cargo mutants --in-diff "$diff_file" --test-tool nextest \
      --package console-domain --package console-application

# errexit is deliberately omitted; fast hook checks are guarded directly.
check-pre-commit:
    #!/usr/bin/env bash
    set -uo pipefail
    just check-format || exit $?
    just check-clippy || exit $?
    just check-arch

check-pre-push:
    just check

ensure-rust-quality-tools:
    ./dev-tooling/ensure-rust-quality-tools.sh core

ensure-fuzz-tooling:
    ./dev-tooling/ensure-rust-quality-tools.sh fuzz

ensure-mutants-tooling:
    ./dev-tooling/ensure-rust-quality-tools.sh mutants
