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

As of 2026-06-23:

- The repo has only seed docs/spec plus `.livespec.jsonc`.
- No Rust workspace exists yet.
- No `justfile`, `lefthook.yml`, CI, or Rust toolchain pin exists yet.
- No `.beads/` pointer files exist yet.
- `bd list` fails with `no beads database found`.
- `orchestrate plan` reports no spec actions and cannot compute impl
  actions until Beads is provisioned.
- The Codex plugins for livespec core, `livespec-driver-codex`, and
  `livespec-orchestrator-beads-fabro` are installed host-wide.

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
2. Bootstrap family infrastructure in a secondary worktree.
3. Provision the Beads tenant and pointer files.
4. File a Beads epic plus small, dependency-aware slices for the TUI
   milestone.
5. Build the Rust skeleton and quality gates before broad feature work.
6. Switch eligible slices to Dispatcher/Fabro execution.

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
