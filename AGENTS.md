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

## Codex dogfooding (OpenAI Codex CLI/TUI)

This repo's `/livespec:*` and orchestrator surfaces can be dogfooded from
OpenAI Codex CLI/TUI, not just Claude Code. Unlike the Claude path (plugins
enabled PER PROJECT via a committed `.claude/settings.json`), Codex plugin
enablement is **HOST-WIDE**: each registration persists in `~/.codex/config.toml`
and applies to every project on the host. Codex offers no project-scoped plugin
enablement, so there is no committed-settings analogue for the Codex path.

Install the three family plugins host-wide: livespec CORE (the artifact carrier
that ships the spec-side prose and wrappers), the `livespec-driver-codex` Codex
Driver (which supplies the `/livespec:*` operation surface over core's prose),
and the selected orchestrator plugin:

```bash
# livespec CORE (spec-side prose + wrappers; no skills of its own):
codex plugin marketplace add thewoolleyman/livespec
codex plugin add livespec@livespec

# The Codex Driver (supplies the spec-side /livespec:* operation surface):
codex plugin marketplace add thewoolleyman/livespec-driver-codex
codex plugin add livespec@livespec-driver-codex

# The selected orchestrator plugin (ships its own Codex skills):
codex plugin marketplace add thewoolleyman/livespec-orchestrator-beads-fabro
codex plugin add livespec-orchestrator-beads-fabro@livespec-orchestrator-beads-fabro
```

Once installed, Codex operations are driven via `codex exec` and NAME-selected as
`<plugin>:<op>` (for example, `livespec:next`,
`livespec-orchestrator-beads-fabro:list-work-items`) rather than as
`/`-prefixed slash commands. The distributed Drivers resolve their prose at
runtime; no `AGENTS.md` skill-to-prose mapping is required. See
`livespec/SPECIFICATION/contracts.md` §"Plugin distribution" and
`livespec/SPECIFICATION/non-functional-requirements.md` §"Codex dogfooding
contracts" for the authoritative install and resolution contracts.

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
