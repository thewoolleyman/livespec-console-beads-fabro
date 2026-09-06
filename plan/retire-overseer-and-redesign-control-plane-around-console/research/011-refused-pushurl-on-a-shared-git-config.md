# 011 — A probe's refusing `pushurl` on a shared `.git/config` refused every worktree push (2026-09-06)

## What happened

Between 15:58Z and 16:21Z the orchestrator repo
`/data/projects/livespec-orchestrator-beads-fabro` refused every push from
every worktree with:

    fatal: unable to access 'https://invalid.invalid/refused.git/': Could not resolve host: invalid.invalid

Two pushes from this thread failed (the plan threads
`arm-plan-record-conformance` and `control-plane-accounts-and-dispatch-policy`,
each opened for the maintainer on request), and two other orchestrator
sessions spent 16:15–16:21Z investigating the same symptom. Nothing on disk
explained the URL: no plugin, pack, hook, binary, wrapper or config file
contains it.

## Root cause, with the commands

Session `2c5ebc20` (the maintainer's `fabro-fork-control-plane-gaps` plan,
probing `bd-ib-js4t57`: "hook-refused pre-clone push must fail staging
loudly") needed the engine's pre-run push to be REFUSED and simulated it by
pointing the origin's push URL at the RFC 2606 reserved host:

    15:58:47Z  git config remote.origin.pushurl https://invalid.invalid/refused.git   (in worktree probe-js4t57)
    16:02:31Z  git config --unset remote.origin.pushurl; …push a probe branch…; git config remote.origin.pushurl https://invalid.invalid/refused.git
    16:21:48Z  git config --unset remote.origin.pushurl

`git config` without `--worktree` writes the SHARED `.git/config` of the
primary checkout, which every linked worktree reads. The probe's measurement
comment on `bd-ib-js4t57` records the method as intended and reads as
correct; its author did not see the blast radius because nothing reported
it — the failure presented, everywhere else, as a DNS error on a host nobody
recognised.

The `merge origin/master: Fast-forward` reflog entry at 16:18:25Z on the
primary and the `.git/config` rewrites of the other three primaries at
16:22–16:30Z were red herrings: the former is an ordinary refresh, the latter
are `git branch -D` removing `[branch]` sections after PR merges.

## Why it was hard to see

- The symptom is a DNS failure on a made-up host, printed before any hook
  runs, so lefthook's summary never appears and nothing names a config key.
- The rewrite is toggled by a session, so it is absent whenever anyone looks
  a few minutes later; two of my three probes found it unset.
- Worktrees share the primary's config, so a probe scoped "to a worktree" in
  the author's mind was fleet-wide in effect.

## Prevention

1. **Mechanical** — `livespec-dev-tooling-xlb5`: `check-remote-standard`, the
   FIRST pre-push step in the worktree pack (before the long aggregate) and a
   fleet-conformance admin row: `remote.origin.url` equals the canonical
   `https://github.com/<owner>/<repo>.git`, no `remote.origin.pushurl`, no
   `url.*.insteadOf` / `pushInsteadOf` rewrite; fails closed naming the key
   and the one-line remedy. A stray guard then costs one second on the next
   push instead of a multi-session investigation.
2. **Discipline** (this repo's AGENTS.md, and handed to the fleet doc): a
   probe that needs a refusing remote uses a throwaway clone or a
   per-invocation override — `git -c remote.origin.pushurl=https://invalid.invalid/refused.git push …`
   or `GIT_CONFIG_COUNT`/`GIT_CONFIG_KEY_0`/`GIT_CONFIG_VALUE_0` for one
   process — never `git config` on a shared remote. A persistent per-worktree
   override requires `extensions.worktreeConfig` and `git config --worktree`.
3. **Recognition**: `Could not resolve host: invalid.invalid` on a push means
   "someone set a refusing `pushurl` on this repo's shared config"; check
   `git config --show-origin --get-all remote.origin.pushurl` before anything
   else.

Recorded on `bd-ib-js4t57` (blast radius) and on the console epic's ordered
command list (the dev-tooling item to drive).
