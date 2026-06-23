# TUI first milestone handoff

## Objective

Drive `livespec-console-beads-fabro` to the first milestone: a Rust TUI
version that satisfies the live specification, then use the repo's own
Beads/Fabro factory as soon as the bootstrap makes that possible.

## Required Startup Checks

Run these before making changes:

```bash
git -C /data/projects/livespec-console-beads-fabro status --short --branch
git -C /data/projects/livespec-console-beads-fabro worktree list
git -C /data/projects/livespec-console-beads-fabro config --get livespec.primaryPath || true
```

Read:

- `AGENTS.md`
- `SPECIFICATION/spec.md`
- `SPECIFICATION/contracts.md`
- `SPECIFICATION/constraints.md`
- `SPECIFICATION/scenarios.md`
- `research/tui-first-milestone-bootstrap-plan.md`

Probe Beads through the family wrapper without printing secrets:

```bash
/data/projects/1password-env-wrapper/with-livespec-env.sh bash -lc 'printenv BEADS_DOLT_PASSWORD | wc -c; bd --version'
/data/projects/1password-env-wrapper/with-livespec-env.sh bd list
```

Run the orchestrator plan read-only:

```bash
python3 "$PLUGIN_ROOT/scripts/bin/orchestrate.py" plan --repo /data/projects/livespec-console-beads-fabro --json
```

Resolve `PLUGIN_ROOT` as described in the
`livespec-orchestrator-beads-fabro:orchestrate` skill before running
that command.

## Current Status

As of 2026-06-23 after the first planning PR:

- The repo has only seed docs/spec plus `.livespec.jsonc`.
- No Rust workspace exists yet.
- No `justfile`, `lefthook.yml`, CI, or Rust toolchain pin exists yet.
- No `.beads/` pointer files exist yet.
- `bd list` fails with `no beads database found`.
- `orchestrate plan` reports no spec actions and cannot compute impl
  actions until Beads is provisioned.
- The Codex plugins for livespec core, `livespec-driver-codex`, and
  `livespec-orchestrator-beads-fabro` are installed host-wide.

The `bootstrap-rust-infra` branch merged as `3c82407`. The repo now has
the first Rust workspace, `just bootstrap`, `just check`, CI, Claude
project settings, a pinned Rust toolchain, and committed non-secret
`.beads/` pointer files.

The Beads/Dolt tenant is provisioned server-side and local
`.beads/metadata.json` has been regenerated as a gitignored file.
`bd list` works under `/data/projects/1password-env-wrapper/with-livespec-env.sh`.
The ledger currently has:

- `livespec-console-beads-fabro-rt3g3t` — blocked planning epic:
  "Deliver the Rust TUI first milestone".
- `livespec-console-beads-fabro-i6n4rm` — implementation merged via
  PR #6; Beads close update is pending a retry because the
  1Password Environment wrapper returned `rate limit exceeded`.
- `livespec-console-beads-fabro-ysvmwh` — active:
  "Implement the SQLite event store skeleton"; current branch
  `event-store-ysvmwh` adds `console-eventstore`.
- `livespec-console-beads-fabro-y45jhj` — ready:
  "Replace the TUI preview with a testable TUI model".
- `livespec-console-beads-fabro-gyxlib` — ready:
  "Add first source adapter ports and LiveSpec/Beads snapshots".

`orchestrate plan` must be run under the livespec environment wrapper so
the Beads password is present. The quality-gate slice has already
raised the default gate to format, strict Clippy, cargo test,
cargo-nextest, 100% library line coverage, cargo-deny, cargo-machete,
and the repo architecture check. Fuzz and mutation discovery smoke
targets are explicit non-default checks.

The event-store branch adds a SQLite WAL-backed `console-eventstore`
crate with the required `events`, `commands`, `checkpoints`, and
`projections` tables; idempotent event append by stable source event
identity; and durable reads ordered by global sequence.

Known warning: `bd update` / `bd list --ready` currently prints an
auto-backup warning for `backup_export` permission denial under the
tenant user. Tenant onboarding did register the `s3` backup remote as
the backup user; investigate whether this is a Beads auto-backup config
expectation or a harmless client-side self-heal attempt before treating
it as a product blocker.

## Constraints

- Product code must be Rust.
- Use worktree -> PR -> merge -> cleanup for tracked changes.
- Do not edit primary checkouts directly.
- Do not print secret values.
- Keep research rationale in `research/`; keep active session
  instructions in `prompts/`.
- Do not use `prompts/` as a shadow ledger. Once work is concrete, file
  Beads work-items and reference their IDs here.
- Use the live spec lifecycle for any requirement change.
- Use the Beads/Fabro Dispatcher for implementation slices only after
  Beads is working and `just check` is reliable.

## Next Actions

1. Land the planning/handoff artifacts.
2. Bootstrap family infrastructure in a secondary worktree. Done in
   `3c82407`.
3. Provision the Beads tenant and verify the pointer files. Done; use
   the livespec env wrapper for all `bd` calls.
4. File a Beads epic plus small, dependency-aware slices for the TUI
   milestone. Done; see IDs above.
5. Dispatch the first ready factory slice after operator action
   selection. Recommended: `impl:livespec-console-beads-fabro-i6n4rm`.
6. Continue filing/dispatching slices until the first TUI milestone is
   complete.

## Verification

The planning/handoff PR is complete when:

- `research/tui-first-milestone-bootstrap-plan.md` exists.
- `prompts/tui-first-milestone-handoff.md` exists.
- The primary checkout remains clean.

The bootstrap-to-factory phase is complete when:

- `just bootstrap` exists and succeeds.
- `just check` exists and succeeds.
- Beads works in the repo.
- `orchestrate plan --repo /data/projects/livespec-console-beads-fabro`
  returns concrete implementation actions.

The first product milestone is complete when:

- All specification scenarios pass through automated tests or documented
  acceptance checks.
- `just check` passes locally and in CI.
- The TUI can drive the attention inbox and factory drain workflow
  against real or explicitly simulated source data, without bypassing
  the event store, command store, projections, or ports.

## Refresh Rule

Refresh this prompt before context compaction, before ending a partially
complete session, and whenever the active Beads work-item IDs or
bootstrap status changes.

Archive or remove this prompt after the first TUI milestone is merged
and the durable status is represented by spec history, commits, and
closed Beads items.
