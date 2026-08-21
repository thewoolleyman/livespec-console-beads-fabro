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

## A session's plugin root goes stale MID-SESSION, and dispatch refuses

Found 2026-08-21 while dispatching from the `test-adequacy-gates` plan
thread. `drive --action impl:<id>` failed with dispatcher exit code 3:

    ERROR: dispatcher plugin build is stale; executing build 15a4ae9aff88
    predates latest release 5dcbc6829ff9. Run `claude plugin update
    livespec-orchestrator-beads-fabro@livespec-orchestrator-beads-fabro`
    before dispatching.

The Dispatcher has a **release-currency gate**
(`commands/_dispatcher_staleness_gate.py`) that probes
`git ls-remote https://github.com/thewoolleyman/livespec-orchestrator-beads-fabro.git
refs/heads/release` and REFUSES admission when the executing build
predates that SHA. It fires before anything about the work-item is
considered, so the failure says nothing about the item.

**The trap is that the session was current when it started.** Its
SessionStart hook ran `just ensure-plugins`, which correctly reported
`15a4ae9aff88` as the latest version. The marketplace moved to
`5dcbc6829ff9` (v0.62.11) hours later, while the session was still
running. A session resolves its plugin root ONCE, at start, so it keeps
invoking the build it resolved — and that build is now refused. Nothing
warns you; the first symptom is a dispatch that will not admit.

So the "pins go stale silently" section above has a second, shorter
clock: not just *a clone can sit on a months-old pin*, but **a live
session can go stale within hours of its own successful currency check.**

What to do when you see exit code 3 with this message:

1. Re-run `mise exec -- just ensure-plugins` — it updates the PROJECT
   pin. It does NOT retarget the running session, which is why the
   remedy text alone is not sufficient.
2. Invoke the new build's path explicitly for the dispatch, e.g.
   `python3 ~/.claude/plugins/cache/livespec-orchestrator-beads-fabro/livespec-orchestrator-beads-fabro/<new-sha>/scripts/bin/drive.py …`,
   or restart Claude Code. Restarting is cleaner if you have other
   plugin-driven work to do; the explicit path is enough for one
   dispatch.
3. **Check `just check-fork-drift` after the pin moves**, before
   assuming the bump was free. That check compares this repo's 8 pinned
   fork files against the INSTALLED plugin build, so a pin move can
   redden `just check` for every session in the repo. On the
   `15a4ae9aff88` -> `5dcbc6829ff9` move it stayed green, but that is a
   fact to verify, not to assume.

Unrelated to ledger item `livespec-console-beads-fabro-3ej` ("livespec
pin bumps cannot land here"), which concerns the `livespec` CORE pin in
`.livespec.jsonc` frozen at v0.26.0 — same family, different pin,
different failure mode.
