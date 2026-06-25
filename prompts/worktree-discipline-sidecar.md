# SIDECAR (temporary): worktree discipline + clean checkpoint for the console repo

> **Status: temporary.** This sidecar hand-installs the family worktree
> discipline and rescues an uncommitted state until the **Worktree Discipline
> Pack** lands and distributes the discipline into this repo by default
> (tracked as openbrain **ob-0x5**; design + runnable prompt live in
> `livespec` core at `prompts/worktree-discipline-pack-epic.md` /
> `prompts/worktree-discipline-pack-prompt.md`). When that pack reaches this
> repo via `copier update`, its packaged rules SUPERSEDE the manual rules
> below — at that point, archive this prompt.

You are operating in `/data/projects/livespec-console-beads-fabro`. It has
gotten into a screwed-up state: a full spec-refinement cycle (history `v002`,
`v003`, `v004` plus the working spec and a CI placeholder) is sitting
**uncommitted on the primary checkout's `master`**, and the Beads work-items
the cycle surfaced were never filed because of a beads-auth misstep. **Top
priority: get this repo to a clean, committed checkpoint ASAP** so a
fleet-wide change cannot collide with or blow up on the uncommitted working
copy. Do the work in a worktree, the way this repo should have been operated
all along.

---

## 0. Adopt worktree discipline NOW (the rule this repo was missing)

This repo already carries the enforcement — `dev-tooling/git-hook-wrapper.sh`
is installed (via `just bootstrap`) as `pre-commit`/`pre-push`/`commit-msg`
and **refuses any commit or push whose top-level is the primary checkout**
(`livespec.primaryPath` == `/data/projects/livespec-console-beads-fabro`),
delegating to lefthook everywhere else. The gap was discipline, not
machinery: prior sessions edited directly on the primary checkout and let
changes pile up uncommitted.

From here on, **every mutation happens in an isolated worktree**; never edit
or commit on the primary checkout. Do not work around the refuse hook; never
pass `--no-verify`.

---

## 1. Rescue the uncommitted state into a worktree, commit, land

**The exact uncommitted payload** (`origin/master` == `HEAD`, so these are
purely working-tree — a clean transfer):

- Modified (tracked): `.github/workflows/ci.yml`, `justfile`,
  `SPECIFICATION/contracts.md`, `SPECIFICATION/non-functional-requirements.md`,
  `SPECIFICATION/scenarios.md`, `SPECIFICATION/spec.md`
- Untracked: `SPECIFICATION/history/v002/`, `…/v003/`, `…/v004/` (the cut
  snapshots + their `proposed_changes/`)
- Untracked: `prompts/worktree-discipline-sidecar.md` (THIS file — include it
  in the rescue commit) and any AGENTS.md edits you make in step 3.

This is a **docs / spec / config changeset — no product `.rs`** — so it uses a
`docs(...)` / `chore(...)` subject and is **exempt from Red-Green-Replay**.

Steps:

1. From the primary checkout, stash everything including untracked
   (stashes are shared across worktrees via the common `.git`):

   ```bash
   cd /data/projects/livespec-console-beads-fabro
   mise exec -- git stash push -u -m console-checkpoint-rescue
   git status --short --branch   # expect: clean, on master == origin/master
   ```

2. Create the feature worktree from `master` under the per-user root:

   ```bash
   mise exec -- git -C /data/projects/livespec-console-beads-fabro worktree add \
     -b docs/spec-checkpoint-v002-v004 \
     "$HOME/.worktrees/livespec-console-beads-fabro/docs-spec-checkpoint-v002-v004" master
   ```

3. Pop the stash inside the worktree, then make your AGENTS.md edits (step 3
   below) there:

   ```bash
   cd "$HOME/.worktrees/livespec-console-beads-fabro/docs-spec-checkpoint-v002-v004"
   mise exec -- git stash pop
   ```

   Fallback if `stash pop` is awkward across worktrees: instead of stashing,
   `mise exec -- git -C <primary> diff > /tmp/console-rescue.patch`, `git apply`
   it in the worktree, and `cp -r` the untracked `SPECIFICATION/history/v00{2,3,4}`
   dirs + this prompt across. Either way the primary must end clean on `master`.

4. Stage and commit in the worktree (one checkpoint commit is fine):

   ```bash
   mise exec -- git add -A
   mise exec -- git commit -m "docs(spec): persist v002-v004 spec-refinement checkpoint + fail-closed behavior-coverage placeholder"
   mise exec -- git push -u origin docs/spec-checkpoint-v002-v004
   ```

5. Open a PR and land it via the repo's rebase-merge discipline. **Expect CI
   to be RED** — see §2; that red is by design, not a reason to hold the
   checkpoint. (If a `hold`/`do-not-merge` gate exists, this is exactly the
   kind of intentional-red PR a human should wave through.)

6. After merge, refresh the primary and clean up:

   ```bash
   mise exec -- git -C /data/projects/livespec-console-beads-fabro pull --ff-only origin master
   mise exec -- git -C /data/projects/livespec-console-beads-fabro worktree remove \
     "$HOME/.worktrees/livespec-console-beads-fabro/docs-spec-checkpoint-v002-v004"
   mise exec -- git -C /data/projects/livespec-console-beads-fabro branch -d docs/spec-checkpoint-v002-v004
   git -C /data/projects/livespec-console-beads-fabro status --short --branch   # clean on master
   ```

---

## 2. The green/red tension — do NOT fake green

`just check-behavior-coverage` is a **deliberate fail-closed `exit 1`
placeholder** (`justfile`), mandated by
`SPECIFICATION/non-functional-requirements.md` §"Behavioral Coverage": the
build MUST stay red until the clause→scenario→test Rust checker
(`scenario-test-rust-checker`) lands. "It cannot slip."

So "green" here does **not** mean neutering that gate — that would be a
bypass, which this family forbids ("fix the gate, not the bypass"). The
correct ASAP stopping point is **clean + committed + the intentional red
preserved**. The hazard the maintainer is racing against is the *uncommitted
working copy*, and step 1 removes it.

- Persist the placeholder as-is. The CI red is the spec-mandated state.
- The only legitimate path to true green is implementing
  `scenario-test-rust-checker` — a real impl work-item, **out of scope** for
  this ASAP checkpoint. File it (§4), don't attempt it here.
- Make the by-design red explicit in the PR description so a later fleet-wide
  operation does not mistake it for breakage.

---

## 3. Fix the beads-auth discipline (the wrapper is NOT broken)

Verified against the live system: the fleet wrapper works and this tenant
authenticates fine. The "Access denied" was an **invocation** failure — `bd`
was run without the wrapper, so no password reached the `bd` process. Fix the
documented discipline, then file the work-items.

**The verified working invocation** (run from anywhere; `bd` reads
`.beads/config.yaml`):

```bash
LIVESPEC_BD_PATH=/usr/local/bin/bd \
  /data/projects/1password-env-wrapper/with-livespec-env.sh bd list
```

- `Access denied for user 'livespec-console-beads-fabro'@'%'` ⇒ the call was
  not wrapped (no `BEADS_DOLT_PASSWORD` in the `bd` process env). Not a server
  or wrapper fault.
- `CALL DOLT_BACKUP … command denied to user 'livespec-console-beads-fabro'@'%'`
  is a benign, **correct-by-design** warning (tenant users intentionally lack
  SUPER; backups run via a dedicated user). Ignore it.
- A `!`-prefixed one-off in the Claude prompt does **not** persist env into
  later tool-call shells, so it cannot hand the password to a subsequent skill
  invocation. Either prefix each `bd`-touching command as above, or launch the
  whole session under the wrapper so every subprocess inherits the env.

**Tighten `AGENTS.md` so this is the documented contract** (do these edits in
the worktree from §1; they ride along in the same PR — docs, no Red-Green).

Replace the current `## Mutation protocol` section with:

```markdown
## Mutation protocol

This repo's commit-refuse hook (`dev-tooling/git-hook-wrapper.sh`, installed by
`just bootstrap` as `pre-commit`/`pre-push`/`commit-msg`) refuses any commit or
push at the primary checkout and delegates to lefthook everywhere else. The
hook is the enforcement; these are the rules it enforces:

- Every mutation happens in an isolated worktree under
  `~/.worktrees/livespec-console-beads-fabro/<branch>`, created from the primary
  checkout's `master`:
  `mise exec -- git worktree add -b <branch> "$HOME/.worktrees/livespec-console-beads-fabro/<branch>" master`.
- NEVER edit or commit on the primary checkout
  (`/data/projects/livespec-console-beads-fabro`). The hook refuses it; do not
  work around it, and never pass `--no-verify`.
- Use `mise exec -- git commit/push` so the lefthook gates run. If a hook
  fails, fix the cause or halt — do not bypass it.
- Land via PR -> merge (rebase-merge). After merge, refresh the primary to
  `origin/master`, remove the feature worktree, delete the branch, and verify
  the primary is clean on `master`. Do not leave orphaned worktrees.
- Rust product changes follow Red-Green-Replay (the commit-msg hook enforces
  it); docs / spec / config changes use `docs(...)` / `chore(...)` and are
  exempt.
- Keep the specification cohesive; do not import orchestrator-only concerns
  except through explicit contracts.
```

Replace the current `## Beads secret convention` section with:

```markdown
## Beads secret convention

This repo's Beads tenant is `livespec-console-beads-fabro` (server-mode Dolt on
`127.0.0.1:3307`, per `.beads/config.yaml`). It is a FLEET tenant: it shares the
single fleet password, injected as one bare `BEADS_DOLT_PASSWORD` by the fleet
1Password Environment wrapper
`/data/projects/1password-env-wrapper/with-livespec-env.sh`. There is no
per-tenant `BEADS_DOLT_PASSWORD_<tenant>` suffix; isolation is by per-tenant SQL
user and DB-scoped grant.

Every `bd`-touching command — the orchestrator `capture-work-item` /
`list-work-items` / `next` skills, or a raw `bd` call — must run in an
environment where (a) the wrapper has exported `BEADS_DOLT_PASSWORD` and (b)
`LIVESPEC_BD_PATH` points at the pinned `bd` binary (`/usr/local/bin/bd`,
v1.0.5). The working invocation:

    LIVESPEC_BD_PATH=/usr/local/bin/bd \
      /data/projects/1password-env-wrapper/with-livespec-env.sh bd <args>

or launch the whole agent session under the wrapper so every subprocess
inherits the env. A `!`-prefixed one-off in the Claude prompt does NOT persist
env into later tool-call shells, so it cannot supply the password to a later
skill invocation — do not rely on it.

A `bd` "Access denied for user 'livespec-console-beads-fabro'@'%'" error means
the password was absent from the `bd` process env (the call was not wrapped),
NOT a server or wrapper fault. A `CALL DOLT_BACKUP … command denied` warning is
correct-by-design (tenant users intentionally lack SUPER) — ignore it.

Secrets are probe-only: check byte counts (`printenv NAME | wc -c`), never echo
values.
```

---

## 4. File the work-items the cycle surfaced

With the wrapped invocation working, file the items the prior session was
blocked on, via `/livespec-orchestrator-beads-fabro:capture-work-item` (run
the session/skill under the wrapper per §3). From the prior handoff:

- The 4 impl work-items (ready / high-priority).
- 2 gap-reinforcements the drift survey surfaced: (a) the `console-arch-check`
  text-scan → `cargo metadata`/AST upgrade; (b) 7-of-8 commands unimplemented.
- `scenario-test-rust-checker` — the impl work-item that, when complete,
  replaces the §2 fail-closed placeholder and turns the build legitimately
  green.

These are ledger operations (the Dolt store), independent of the §1 PR — they
do not need to be in the commit.

---

## 5. Optional: confirm convergence

If time allows after the checkpoint is clean, run a `/livespec:critique` cycle-3
over `v004` (especially the realigned `contracts.md`) to confirm the
spec-refinement loop has converged. This is the lowest-priority item — the
clean checkpoint and the discipline fixes come first.

---

## Close criteria

- Primary checkout clean on `master`, `origin/master` carries the v002–v004
  checkpoint, no orphaned worktrees.
- `AGENTS.md` mutation-protocol + beads-secret sections tightened (no stale
  "until hooks exist" / "once a remote exists" / `!`-wrapper hedges).
- Beads work-items filed via the wrapped invocation; CI red is by design
  (behavior-coverage placeholder), documented as such.
- This sidecar superseded and archived once the Worktree Discipline Pack
  (ob-0x5) reaches this repo.

---

## Holistic flags surfaced for livespec / OpenEPICS (not yet tracked)

Surfaced during this checkpoint; flagged for the family discussion, NOT acted on
here. Route each to the right livespec-core / OpenEPICS epic (some may already be
covered by ob-0x5 — confirm).

1. **Console `AGENTS.md` should inherit the family-universal agent-instruction
   core via the impl-plugin template — not be a hand-maintained divergent copy.**
   The orchestrator's `AGENTS.md` declares itself that core ("shared by every
   family member via the impl-plugin template"); the console's had drifted to a
   hand-rolled, hedged version, which is the ROOT CAUSE of the beads-invocation
   confusion this checkpoint hit. This commit hand-patches the console to the
   canonical text as a stopgap; the systemic fix is re-aligning the console (and
   auditing every templated repo) to the template's current core via `copier
   update`, so the canonical Beads runtime prerequisites + Repository mutation
   protocol cannot silently drift again.

2. **Refinement methodology: ground reconciliations in impl reality BEFORE/with
   critique.** A spec-internal `/livespec:critique` reconciled the event-envelope
   inconsistency (D1) AWAY from the impl's actual scalar schema; it was corrected
   only after a later `capture-spec-drift` pass. livespec's refinement guidance
   should sequence drift-detection before (or alongside) critique so a
   spec-internal pass cannot "fix" something in a direction the code never took.

3. **Fail-closed gate placement as an explicit family rule.** A fail-closed
   placeholder belongs at the CI merge gate, NEVER the local `just check` /
   pre-push aggregate — a fail-closed check in pre-push deadlocks every local push
   (including a repo's own checkpoints). This checkpoint hit exactly that. Worth a
   family-level contract. (Console-local follow-up: reconcile the v004 NFR
   "Behavioral Coverage" wording from "just check and CI" to "CI merge gate" in
   cycle-3.)
