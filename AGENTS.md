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
  - Topic-scoped durable knowledge lives under `.ai/`, loaded on demand via
    these references:
    - [`.ai/spec-check-and-ci-discipline.md`](.ai/spec-check-and-ci-discipline.md)
      — why a "spec-only" change can break Rust CI (the `console-spec-check`
      spec-ground-truth coupling), reading CI logs (incl. the empty
      `gh run view --log-failed` gotcha), and verifying the CI'd commit before
      trusting a local test run.
    - [`.ai/fleet-repo-naming.md`](.ai/fleet-repo-naming.md)
      — never use bare "beads-fabro" (two sibling repos end in it); the
      repo ↔ tenant ↔ ID-prefix map, and targeting repos by full
      `/data/projects/<full-name>` path for destructive / tenant ops.
    - [`.ai/livespec-plugin-currency.md`](.ai/livespec-plugin-currency.md)
      — keeping the livespec plugins current: per-project pins go stale,
      `claude plugin update <name>@<marketplace> --scope project` (why the
      `name@marketplace` form is required) vs. host-wide `codex plugin
      marketplace upgrade`, "latest" = the `vX.Y.Z` tag, and the beads
      self-heal landing in orchestrator ≥ 0.4.0 (and how that qualifies the
      "Access denied ⇒ outside the wrapper" rule below).
- **Handoffs: update the living plan-thread handoff; NEVER print one inline.**
  Session handoffs live at `plan/<topic>/handoff.md` (one durable thread per
  topic; resume via `/livespec-orchestrator-beads-fabro:plan <topic>`); UPDATE
  it in place and print its PATH. Completed threads archive to
  `plan/archive/<topic>/`; legacy prompt handoffs live in `archive/prompts/`.
  Do not print a handoff body in the chat, and do not proliferate new handoff
  files.

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

    /data/projects/1password-env-wrapper/with-livespec-env.sh -- bd list -n 0

which injects the bare `BEADS_DOLT_PASSWORD` for that one command. **The `-n 0` is
not optional decoration:** `bd list` silently truncates at 50 rows, `--json`
included, with nothing in the output saying the set was cut — so a read without it
under-reports the ledger and can make an existing item look absent. The same holds
for the orchestrator `capture-work-item` / `list-work-items` / `next` skills: run
the `bd`/python commands they drive under the wrapper. An **"Access denied" / "no
beads database found" failure almost always means you ran OUTSIDE the wrapper**
(the password is absent), not that a secret is missing — re-run under the wrapper.
Never hand-hunt the secret or reach around the seam with raw `mysql` / `dolt` /
`sudo`, and never rely on a `!`-prefixed one-off in a Claude prompt (it does not
persist env into later tool-call shells). A `CALL DOLT_BACKUP … command denied`
warning is correct-by-design (tenant users lack SUPER) — ignore it.

**The wrapper execs in a CLEAN environment — set variables INSIDE it, not in
front of it.** Anything exported ahead of `with-livespec-env.sh` is dropped
before the wrapped program runs, and this is not limited to `LIVESPEC_*`:
sentinel variables of any name vanish. The failure is SILENT — the program
runs, exits 0, and behaves as though you never set the variable.

    # WRONG — silently dropped, no error, exit 0
    LIVESPEC_CONSOLE_REPO_PATH=/data/projects/foo \
      with-livespec-env.sh -- livespec-console-beads-fabro serve

    # RIGHT — set it inside the wrapper's environment
    with-livespec-env.sh -- env \
      LIVESPEC_CONSOLE_REPO_PATH=/data/projects/foo \
      livespec-console-beads-fabro serve

The wrapper also supplies only a minimal system `PATH`
(`/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin`), so anything
in `~/.local/bin` (e.g. `fabro`) is NOT on `PATH` inside it — pass an absolute
path for such programs. Found during the B8 release acceptance (2026-07-21),
where a console pointed at another repo appeared to ignore its env override;
`docs/installing.md` carries the user-facing form of the same rule.

**Budget your wrapper calls.** Each one is an `op run` against a 1Password daily
quota that is shared **account-wide across every tenant**, not per-repo — a session
that spends it blocks `git push` and every ledger write fleet-wide, for other
sessions too. Batch: make one `bd list --json -n 0` and parse the cached result
locally as often as needed, and loop multi-item work inside a single wrapper
invocation rather than wrapping each command. Do not narrate into the ledger.
Detail: the `livespec` repo's fleet agent-disciplines reference, §"Ledger-write
economy under a shared secret wrapper".

## Repository mutation protocol

Every repo change uses a worktree → PR → merge → cleanup path. Treat leaving
dirty state, committing on the primary checkout, or asking the user whether to
commit as failures of the workflow, not as acceptable stopping points. The
commit-refuse hook — the canonical STRUCTURAL body REUSED from
livespec-dev-tooling, installed by `just bootstrap` (which delegates to `just
install-commit-refuse-hooks`) as `pre-commit`/`pre-push`/`commit-msg` — refuses
any commit or push at the primary checkout and delegates to lefthook everywhere
else. It detects the primary structurally (refuses when `git rev-parse
--git-dir` equals `git rev-parse --git-common-dir`, unless
`livespec.sandboxExempt` is set), so it is ARMED ON INSTALL with no
`livespec.primaryPath` arming step to miss. This is the `baseline` profile of
livespec's Conformance Pattern (concern #1, Worktree-discipline); `just
check-baseline` is the fail-closed verifier wired into `just check`.

1. Confirm the primary checkout before editing (the primary is where `git
   rev-parse --git-dir` equals `git rev-parse --git-common-dir`):

   ```bash
   git rev-parse --git-dir; git rev-parse --git-common-dir
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

## Post-merge janitor: Rust toolchain on the mise PATH

The factory Dispatcher's post-merge janitor re-runs `mise exec -- just check` in a
fresh detached worktree under a scrubbed, non-interactive PATH. rustup (not mise)
owns the Rust toolchain — pinned by `rust-toolchain.toml` — and installs
`cargo`/`rustc`/`rustfmt`/`clippy` under `~/.cargo/bin`. An interactive shell gets
that directory from rustup's profile snippet, but the janitor's minimal env does
not, so `.mise.toml` exposes it via `[env] _.path = ["~/.cargo/bin"]`. Removing
that entry reintroduces `cargo: not found` (exit 127) in the janitor even when the
PR merged green.

## GitHub CLI: pinned to 2.46.0 — no `--json` on `gh pr checks`

`gh --version` here is **2.46.0**. `gh pr checks` in that release has **no
`--json` flag**: it exits non-zero with `unknown flag: --json` and writes nothing
to stdout. Use the plain tab-separated form (name, status, duration, URL):

```bash
gh pr checks <n> 2>&1 | awk '{print $2}' | sort | uniq -c   # rollup
gh pr checks <n> 2>&1 | grep -v " pass"                     # non-passing only
```

**This bites hardest inside a polling loop, and it fails silently.** A CI monitor
built on `s=$(gh pr checks <n> --json name,bucket 2>/dev/null)` gets an empty `s`
on every iteration, so its "are we done yet" guard never fires and it spins to
timeout reporting "still pending" no matter what CI did. The `2>/dev/null` is
what makes it undiagnosable: it discards the one line that explains the empty
output. Two rules follow, and they generalize past this flag:

- **Never silence stderr on a command a loop depends on.** A hard error and
  "nothing has happened yet" produce identical stdout; only stderr distinguishes
  them. Run the exact command once in the foreground before arming any Monitor.
- **Never gate a poll loop on `gh pr checks`' exit status, and never add a
  `|| echo '[]'` style fallback.** It exits non-zero *while checks are pending*
  (a normal state, not an error), and the fallback swallows real output too. Gate
  on parsed content, and require a non-empty result before evaluating any
  all-done predicate — `jq 'all(...)'` over an empty array is `true`, which turns
  a broken fetch into a false "all green".

Separately, `gh pr edit --body` fails against this repo with a Projects-classic
GraphQL deprecation error (`repository.pullRequest.projectCards`). Update PR
bodies through REST instead:

```bash
gh api -X PATCH repos/<owner>/<repo>/pulls/<n> --input body.json   # {"body": "..."}
```
