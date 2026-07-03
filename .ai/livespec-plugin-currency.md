# Keeping the livespec plugins current (console)

Durable, learned agent knowledge for `livespec-console-beads-fabro`,
loaded on demand from `AGENTS.md`. Captured after a session where
`/livespec-orchestrator-beads-fabro:next` failed with a raw MySQL
`Access denied` — the cause was a **stale, pre-self-heal orchestrator
build pinned for this project**, not a missing secret.

## The plugins are pinned PER PROJECT and go stale silently

Claude Code pins each plugin to a specific build per project scope (see
`~/.claude/plugins/installed_plugins.json`). A clone can sit on a months-old
pin while the marketplace has moved several releases ahead. Symptoms are
confusing because the *code* looks fine — only the pinned plugin build is old.

## Updating — Claude Code (per project)

    claude plugin update <name>@<marketplace> --scope project   # run from the project root

- **Always use the fully-qualified `name@marketplace` form.** Two installed
  plugins are both literally named `livespec`: `livespec@livespec` (spec-side
  core) and `livespec@livespec-driver-claude` (the Claude harness driver). A
  bare `livespec` is ambiguous. The impl plugin is
  `livespec-orchestrator-beads-fabro@livespec-orchestrator-beads-fabro`.
- Update pins to the marketplace **HEAD commit** (a git SHA); the released
  version is the `version` in that commit's `.claude-plugin/plugin.json`.
  **"Latest released" = the newest `vX.Y.Z` git tag** on the plugin repo
  (`git ls-remote --tags`), not a semver in the marketplace manifest.
- `claude plugin update` prints **"Restart to apply changes."** — the running
  session keeps the OLD build (and OLD hooks) until Claude Code restarts. This
  is why an updated hook or self-heal does not take effect mid-session.

## Updating — Codex (host-wide, not per project)

Codex plugin enablement is global in `~/.codex/config.toml` (see the
"Codex dogfooding" section of `AGENTS.md`). Refresh with:

    codex plugin marketplace upgrade <name>     # then `codex plugin list` to verify

The Codex driver is a **separate plugin** — `livespec@livespec-driver-codex`
(repo `livespec-driver-codex`, `.codex-plugin/` layout). It is NOT the same as
`livespec-driver-claude` and is NOT touchable by `claude plugin`.

## The beads self-heal lives in orchestrator ≥ 0.4.0

The orchestrator's `scripts/bin/_bootstrap.py` runs a credential chokepoint
(`_self_heal_credentials()` + `scripts/_vendor/livespec_runtime/credentials.py`)
that, when `BEADS_DOLT_PASSWORD` is absent, **re-execs the process through the
`credential_wrapper` declared in `.livespec.jsonc`** (`with-livespec-env.sh --`)
so a bare invocation self-authenticates. Builds **before 0.4.0 lack
`credentials.py`** and fail deep in the beads backend with `Access denied`
instead.

Consequence for the "Beads runtime prerequisites" guidance in `AGENTS.md`
("Access denied ⇒ you ran OUTSIDE the wrapper"): that is fully true only on
pre-self-heal builds. On orchestrator **≥ 0.4.0 the plugin skills
self-authenticate**, so a persistent `Access denied` there points at a genuinely
missing/rotated secret or a wrapper misconfig — not merely "run under the wrapper."
