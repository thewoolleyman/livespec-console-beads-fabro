# Agent instructions

This repo is a LiveSpec-family peer for the Beads/Fabro operator console.
The authoritative design is the live specification under `SPECIFICATION/`,
which now carries a revision history (`SPECIFICATION/history/v001/`). A Rust
workspace under `crates/` implements the console against that spec; ongoing
implementation work is tracked in the Beads ledger, not in this file.

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

## Agent interaction (maintainer working style)

- **Decisions → AskUserQuestion with a recommendation.** When a choice is
  genuinely the maintainer's to make, present it via the AskUserQuestion tool
  with 2–4 concrete options and a clearly-marked **"(Recommended)"** first
  option — never as a freeform prose question. Put load-bearing framing inside
  the question / option text.
- **Don't stop to ask what you should just do.** Execute the agreed plan and the
  obvious next steps yourself; reserve questions for genuinely maintainer-owned
  choices you cannot resolve from the request, the code, or sensible defaults —
  and even then lead with a recommendation.
- **Durable agent memory lives in-repo.** Persist durable agent guidance and
  learned preferences in this file (or a file it references), NOT in ephemeral
  per-session agent memory. The repo's hook that blocks `~/.claude` memory writes
  is a signal to capture the memory HERE, not to drop it.
- **Handoffs: update the living handoff file; NEVER print one inline.** When
  handing off to a future session, UPDATE the existing handoff prompt under
  `prompts/` (the single living handoff — the one path the next session runs)
  in place and print its PATH. Do not print a handoff prompt's body in the chat,
  and do not proliferate new handoff files.

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

The Codex TUI picker displays skills by short name with the plugin as context.
In `/skills` → `List skills` (or the `@` picker), search the operation name,
for example `orchestrate`; the row renders as
`orchestrate (livespec-orchestrator-beads-fabro)` with kind `Skill`. The
colon-qualified form `livespec-orchestrator-beads-fabro:orchestrate` is still
valid for prompt / `codex exec` name selection and model-visible skill
references, but it is not the picker row operators should expect.

## Beads runtime prerequisites

This repo's work-item store is a per-repo beads/Dolt TENANT
(`livespec-console-beads-fabro`) on the shared family dolt-server — NOT JSONL
files. Installing the plugin does NOT provision the backend; a clone connects to
its tenant only when ALL of the following are present:

- **`bd` CLI, pinned (v1.0.5)**, at an absolute path (`/usr/local/bin/bd`, NEVER
  the mise shim), with `LIVESPEC_BD_PATH` pointing at it (the impl-beads wrappers
  shell out to `$LIVESPEC_BD_PATH`).
- **A running Dolt `sql-server`** reachable over **TCP `127.0.0.1:3307`**. Family
  tenants force TCP (not the unix socket); `.beads/config.yaml` carries `dolt.*`
  host/port keys with NO `socket` key.
- **The tenant password** in env as a single **bare `BEADS_DOLT_PASSWORD`** —
  injected by THIS project's configured env wrapper. This is a FAMILY tenant: it
  shares the one family password via the family 1Password Environment wrapper
  `with-livespec-env.sh` (canonical copy at
  `/data/projects/1password-env-wrapper/with-livespec-env.sh`). `bd` consumes the
  bare var — there is NO per-tenant `BEADS_DOLT_PASSWORD_<tenant>` variable and
  NO per-tenant→bare mapping. Real isolation comes from the per-tenant SQL user +
  DB-scoped grant, not from password distinctness. Secrets are probe-only —
  `printenv NAME | wc -c`, never echo values — and NEVER committed to
  `.livespec.jsonc` or `.beads/`.
- **The `.beads/` pointer files**: `config.yaml` (committed; the `dolt.*` server
  keys) and `metadata.json` (gitignored, regenerable). NEVER run `bd init` inside
  a primary checkout or worktree — it auto-commits and clobbers `.beads/`.

**Run beads commands from the target repo root.** Per-command `bd` resolves its
connection from the current directory's `.beads/config.yaml` (auto-discovery), so
run from this repo's root, or `bd` silently operates on the wrong tenant.

**Wrap every `bd`-touching command under the env wrapper** — there is no "session
launched under the wrapper"; the wrapper applies per command. The canonical
invocation is `with-livespec-env.sh -- <command>`, e.g.

    /data/projects/1password-env-wrapper/with-livespec-env.sh -- bd list

which injects the bare `BEADS_DOLT_PASSWORD` for that one command. The same holds
for the orchestrator `capture-work-item` / `list-work-items` / `next` skills: run
the `bd`/python commands they drive under the wrapper. An **"Access denied" / "no
beads database found" failure almost always means you ran OUTSIDE the wrapper**
(the password is absent), not that a secret is missing — re-run under the wrapper.
Never hand-hunt the secret or reach around the seam with raw `mysql` / `dolt` /
`sudo`, and never rely on a `!`-prefixed one-off in a Claude prompt (it does not
persist env into later tool-call shells). A `CALL DOLT_BACKUP … command denied`
warning is correct-by-design (tenant users lack SUPER) — ignore it.

## Repository mutation protocol

Every repo change uses a worktree → PR → merge → cleanup path. Treat leaving
dirty state, committing on the primary checkout, or asking the user whether to
commit as failures of the workflow, not as acceptable stopping points. The
canonical STRUCTURAL commit-refuse hook (installed by `just
install-commit-refuse-hooks`, which `just bootstrap` delegates to, as
`pre-commit`/`pre-push`/`commit-msg`) refuses any commit or push at the primary
checkout and delegates to lefthook everywhere else. It is armed on install — it
refuses STRUCTURALLY whenever `git rev-parse --git-dir` equals `git rev-parse
--git-common-dir` (a real primary checkout; a secondary worktree's git-dir
differs), so there is no `livespec.primaryPath` arming step to forget and no
fail-open window. The hook body is reused byte-for-byte from the
livespec-dev-tooling wheel (no per-repo copy); the matching fail-closed verifier
runs in `just check`.

1. Confirm the primary checkout before editing (a primary checkout's git-dir
   equals its git-common-dir; a secondary worktree's differs — the structural
   test the hook itself uses):

   ```bash
   git rev-parse --git-dir --git-common-dir
   git status --short --branch
   ```

2. If the change will modify tracked files, create a dedicated worktree from the
   primary checkout's `master` and do all edits there. Every worktree lives under
   the per-user root `~/.worktrees/livespec-console-beads-fabro/<branch>` — NEVER
   as a peer of the clones under `/data/projects`:

   ```bash
   mise exec -- git worktree add -b <branch> "$HOME/.worktrees/livespec-console-beads-fabro/<branch>" master
   ```

3. Use `mise exec -- git commit ...` and `mise exec -- git push ...` so the
   mise-managed lefthook hooks actually run. Never pass `--no-verify`; if a hook
   fails, fix the cause or halt with the failure.
4. Open a PR, wait for required checks, and merge through the PR using the repo's
   rebase-merge discipline.
5. After merge, refresh the primary checkout to `origin/master`, remove the
   feature worktree, delete the local branch, and verify the primary checkout is
   clean on `master`. Do not leave orphaned worktrees.

Rust product changes follow Red-Green-Replay (enforced by the commit-msg hook
once the Rust checker lands); docs / spec / config changes use `docs(...)` /
`chore(...)` subjects and are exempt. Keep the specification cohesive; do not
import orchestrator-only concerns except through explicit contracts.
