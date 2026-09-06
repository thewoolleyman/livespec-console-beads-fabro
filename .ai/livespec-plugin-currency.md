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

### 2026-09-06: the gate stopped REFUSING and started WARNING, and the symptom now points at the wrong repo

The incident above is the easy version: dispatch refused, exit code 3, and
the message named the cause. **That is no longer what a stale root looks
like.** The current gate surfaces staleness and lets the dispatch through:

    WARNING: dispatcher plugin build 93c061eb0a89 lags latest release
    85e277fa81aa; dispatch proceeds because ambient staleness is
    surfaced, not enforced.

The dispatch then creates a real fabro run which dies in about seven
seconds at prepare step 1, and THIS is the only error you are likely to
read:

    Setup command failed (exit code 127):
      set -- {{ inputs.prepare_toolchain_mise }}; test $# -eq 0 || "$@"
    /bin/bash: line 1: {{: command not found

**That symptom looks like a `.fabro` fork defect and it is not one.** The
`{{ inputs.* }}` substitution for `[[run.prepare.steps]]` is performed
HOST-SIDE by the dispatcher plugin (`_dispatcher_overlay.py`'s
`_substitute_input_tokens`, landed for orchestrator `bd-ib-8atx`, released
in v0.124.3). A build predating that fix cannot render the payload it is
about to dispatch, so the literal token reaches bash.

Measured 2026-09-06 on this same plan thread: the misdirection cost an
investigation through `dolt-server`'s constraints, the committed `.fabro`
fork, two closed upstream items, and the `git log` of `workflow.toml` —
and came within one step of filing a spurious upstream defect and of
reverting commit `7667599` (which correctly restored the projections)
back to literal prepare steps. Reverting it would have re-applied the very
workaround that proxy `pzbdbo.2` / upstream `bd-ib-ott6` had just retired,
i.e. the ⛔ never-work-around rule, tripped by a misread symptom.

**The one-command discriminator.** Before blaming the fork, ask whether the
build you are RUNNING can render tokens at all:

```bash
P=/home/ubuntu/.claude/plugins/cache/livespec-orchestrator-beads-fabro/livespec-orchestrator-beads-fabro
for b in <running-build-sha> <installed-build-sha>; do
  printf '%-14s ' "$b"
  grep -rq "_substitute_input_tokens" "$P/$b/scripts/" && echo "HAS fix" || echo "PREDATES fix"
done
```

If the running build says `PREDATES fix` and the installed one says
`HAS fix`, the fork is innocent: you are the 2026-08-21 trap wearing a
different mask. Dispatch from the INSTALLED root (resolve it at dispatch
time, per the section below) and the run proceeds.

Two further notes from that incident. The session had been told: its own
SessionStart hook advanced the install to a newer build while the session
kept the root it resolved at start, exactly as described above — so the
warning was accurate and ignorable-looking. And the recurrence happened
about four hours AFTER orchestrator `bd-ib-h3mm` closed, having added this
very guard; the guard fires correctly and still costs a dispatch, because
warning is not refusing. That gap is filed upstream as `bd-ib-ebd0`,
asking that a build which cannot render the payload refuse (or re-exec
through the installed root) rather than proceed.

#### ⛔ Do NOT record that error as a comment on the work item

This is the sting in the tail, and it is the more expensive half.

`goal-minijinja-preflight` scans an item's title, description, acceptance,
notes, lessons **and every ledger comment** for a MiniJinja opening
delimiter, and REFUSES the dispatch — before `render_goal`'s escape, which
exists to neutralize exactly that string, ever runs. `bd` has no comment
edit or delete (`bd comments` offers only `add`). So a comment quoting the
delimiter makes the item **permanently undispatchable**.

The failure above puts a raw opener in the error text, because the
unexpanded token IS the error. Quoting it on the item is the correct
diagnostic instinct and it is fatal:

1. stale root dispatches → run dies at prepare step 1, error contains a raw
   opener;
2. the diagnostician records that error on the item, verbatim, as evidence;
3. the item can never be dispatched again.

**A transient, self-healing condition becomes a permanent one through the
act of diagnosing it.** Measured 2026-09-06: `-4jb3kl.3` was poisoned this
way about ninety minutes after the stale-root failure it documented, by the
same session, with neither guard being wrong on its own terms.

So when a dispatch dies on a templating symptom:

- Put the forensics on the **plan epic's timeline**, which is not a dispatch
  source, or in a file like this one. A `.md` in the repo is safe; a ledger
  comment on a dispatchable item is not.
- If you must reference the delimiter in ledger prose, describe it — "a
  double-brace opener" — rather than writing it.
- If an item is ALREADY poisoned, the only remedy is to supersede it:
  create a successor with identical title, description and acceptance and
  **zero** comments, close the original as superseded, and re-point any
  `depends_on` edges. That is the precedent from
  `livespec-dev-tooling-npsqeu` and it transferred cleanly here.

Upstream: `bd-ib-it2d` (the refusal reaches an escapable string the escape
never sees). Note it applies to ORCHESTRATOR items too — reporting this
defect by quoting the delimiter on its own tracking item would poison that
item.

What to do when you see exit code 3 with this message:

1. Re-run `mise exec -- just ensure-plugins` — it updates the PROJECT
   pin. It does NOT retarget the running session, which is why the
   remedy text alone is not sufficient.
2. Invoke the new build's path explicitly for the dispatch, e.g.
   `python3 ~/.claude/plugins/cache/livespec-orchestrator-beads-fabro/livespec-orchestrator-beads-fabro/<new-sha>/scripts/bin/drive.py …`,
   or restart Claude Code. Restarting is cleaner if you have other
   plugin-driven work to do; the explicit path is enough for one
   dispatch.

   **Better: resolve the pin at DISPATCH time instead of holding the
   kickoff path.** Suggested by the `repo-invariant-guards` thread,
   verified here 2026-08-21. It needs no host mutation and no tracked
   file change, and it survives the next marketplace move on its own:

   ```python
   import json, pathlib
   KEY = "livespec-orchestrator-beads-fabro@livespec-orchestrator-beads-fabro"
   rows = json.loads((pathlib.Path.home() / ".claude/plugins/installed_plugins.json")
                     .read_text())["plugins"][KEY]
   me = "/data/projects/livespec-console-beads-fabro"
   root = next(r["installPath"] for r in rows if r.get("projectPath") == me)
   # -> .../cache/.../<current-sha>; use f"{root}/scripts/bin/drive.py"
   ```

   **The `projectPath` filter is load-bearing — do not take `rows[0]`.**
   That list is per-project and UNORDERED. Measured on this host the
   same day: 16 rows across SIX distinct builds, with row 0 belonging to
   `/data/projects/livespec-runtime` on `4157cf17b852` while this repo's
   row was the current `ea71503fcf13`. Taking `[0]` would have selected
   another project's build — and an older one, which the staleness gate
   then refuses for a reason that has nothing to do with your repo. The
   `justfile`'s `check-doctor-static` recipe carries the same warning
   about the same file for the livespec core plugin; it is the same
   trap.
3. **Check `just check-fork-drift` after the pin moves**, before
   assuming the bump was free. That check compares this repo's 8 pinned
   fork files against the INSTALLED plugin build, so a pin move can
   redden `just check` for every session in the repo. On the
   `15a4ae9aff88` -> `5dcbc6829ff9` move it stayed green, but that is a
   fact to verify, not to assume.

### Better: resolve the pin AT DISPATCH TIME and the class disappears

The three steps above handle one incident. Any script that dispatches more
than once should not hold a plugin path at all — it should read the PIN each
time it dispatches:

```bash
P=$(python3 -c "
import json
d = json.load(open('/home/ubuntu/.claude/plugins/installed_plugins.json'))
for e in d['plugins']['livespec-orchestrator-beads-fabro@livespec-orchestrator-beads-fabro']:
    if e.get('projectPath') == '/data/projects/livespec-console-beads-fabro':
        print(e['installPath']); break
")
[ -d "$P" ] || { echo "FATAL: pinned plugin path not found: $P" >&2; exit 1; }
```

This needs no host mutation and no tracked-file write, and it survives the
NEXT move as well as this one — which matters, because the marketplace moved
THREE times on 2026-08-21 (`15a4ae9aff88` -> `5dcbc6829ff9` -> `ea71503fcf13`)
and a session that reacted to each move by hand paid for it three times. A
loop that read the pin per dispatch was refused once, before the fix, and never
again.

Note the pin and the executing build are DIFFERENT FACTS. `claude plugin
update ... --scope project` reporting "already at the latest version" says the
PIN is current; it says nothing about what a running session is executing.
Confirm the build you are actually running, from the live process rather than
from the pin:

```bash
ps -eo cmd | grep -o "livespec-orchestrator-beads-fabro/[0-9a-f]\{12\}" | sort -u
```

**The staleness refusals cannot be attributed from the journal.** They carry
`work_item_id` null because they are LOOP-level rows rather than item rows —
the same shape the reflection-row note in
`.ai/factory-dispatch-and-merge-coupling.md` describes. Selecting on the field
returns nothing for them, and a substring grep for an item id matches rows that
item never produced. Two sessions on 2026-08-21 attributed a refusal to the
wrong dispatch from co-occurrence alone. Which build the live chain executes,
and how long it has been alive, is the instrument that settles it: a gate
refusal exits in about twenty seconds, so a chain alive for minutes was not
refused.

Unrelated to ledger item `livespec-console-beads-fabro-3ej` ("livespec
pin bumps cannot land here"), which concerns the `livespec` CORE pin in
`.livespec.jsonc` frozen at v0.26.0 — same family, different pin,
different failure mode.

### The pin FILE goes stale too — refresh it before you resolve it

Added 2026-08-21 after the dispatch-time recipe above failed on its author.
`installed_plugins.json` is not a live view of the marketplace: it only changes
when `claude plugin update` / `just ensure-plugins` actually runs. So on a
fast-moving day, resolving the pin at dispatch time faithfully returns a build
that is *itself* already behind, and the gate refuses exactly as before.

Measured: the orchestrator release moved FIVE times in one working day —
`15a4ae9aff88` → `5dcbc6829ff9` → `ea71503fcf13` → `f51534d61621` →
`392b3fa90f86`. A dispatch that resolved the pin correctly, by `projectPath`,
got `f51534d61621` and was refused against `392b3fa90f86`. The resolution was
right; the file was stale.

**The complete recipe is two steps, in this order:**

1. `mise exec -- just ensure-plugins` — refreshes the pin file. It prints
   "Restart to apply changes", which is true for THIS session's own skills and
   hooks but irrelevant to a subprocess you are about to launch by path.
2. Resolve the pin from `installed_plugins.json`, filtered by `projectPath`,
   and invoke that build's `scripts/bin/drive.py`.

Step 1 without step 2 leaves your session on its kickoff build. Step 2 without
step 1 faithfully resolves a stale pin. Both failures produce the same exit-3
message, which is why it is worth knowing they are different faults.

If you want to know whether a refusal is coming before you spend a dispatch,
probe what the gate probes:

    git ls-remote https://github.com/thewoolleyman/livespec-orchestrator-beads-fabro.git refs/heads/release

and compare that SHA to the build you are about to execute. Cheap, and it
distinguishes "my pin is stale" from "my session is stale" without burning a
run. Re-check `just check-fork-drift` after any pin move — it stayed green
across all five moves today, but that is a fact to verify rather than assume.
