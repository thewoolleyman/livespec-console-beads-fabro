# Agent instructions

This repo is a new LiveSpec-family peer for the Beads/Fabro operator
console. It is in seed state: the authoritative design is the live
specification under `SPECIFICATION/`.

## Repository scope

`livespec-console-beads-fabro` is a separate product from:

- `livespec` core, which owns the spec lifecycle and `/livespec:*`
  contract.
- `livespec-orchestrator-beads-fabro`, which owns Beads work-items,
  Dispatcher, and Fabro dispatch mechanics.
- `fabro`, which owns workflow execution, run state, human gates, logs,
  and sandbox UI.

This repo owns the operator console: event ingestion, canonical events,
commands, projections, TUI/GUI presentation, and human-attention routing.

## Beads secret convention

Use the current family convention. The 1Password Environment wrapper at
`/data/projects/1password-env-wrapper/with-livespec-env.sh` exports one
bare `BEADS_DOLT_PASSWORD`. There is no per-tenant
`BEADS_DOLT_PASSWORD_<tenant>` suffix and no per-tenant-to-bare mapping.
Isolation is by per-tenant SQL user and DB-scoped grant.

Secrets are probe-only: check byte counts, never echo values.

## Mutation protocol

Until this repo has its own finalized commit hooks, follow the family
discipline manually:

- Prefer worktree -> PR -> merge for tracked changes once a remote exists.
- Do not commit directly on a primary checkout when normal family tooling is
  available.
- Rust product changes should follow Red-Green-Replay once the commit hook is
  installed.
- Keep the specification cohesive; do not import orchestrator-only concerns
  except through explicit contracts.

