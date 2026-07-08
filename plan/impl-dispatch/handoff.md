# Console impl-dispatch — thread handoff

**Status:** ACTIVE — the former factory-dispatch blocker is obsolete for this
thread. The two dependency-root slices have landed and the ready lane is empty;
the next action is to move the downstream backlog chain forward.

**Refreshed:** 2026-07-08.

## Read first

Open only this file, then derive live status from the Beads ledger with:

```bash
/data/projects/1password-env-wrapper/with-livespec-env.sh -- codex exec livespec-orchestrator-beads-fabro:list-work-items --json
/data/projects/1password-env-wrapper/with-livespec-env.sh -- codex exec livespec-orchestrator-beads-fabro:next --json
```

Do not trust stored checklist state in this handoff; the ledger is authoritative.

## What this thread is

Groom and dispatch the console's behavioral-coverage implementation chain: the
Scenario 6 / Scenario 7 prerequisite realizations, the operator-facing and
NFR contributor-facing clause-to-scenario backfills, and the final
`console-spec-check` fail-mode flip.

## Ledger-derived state at refresh

The 2026-07-08 ledger read showed:

- `livespec-console-beads-fabro-idgql3` — done; Scenario 7 realized.
- `livespec-console-beads-fabro-qvrwag` — done; Scenario 6 realized.
- `livespec-console-beads-fabro-cvqcnx` — backlog; operator-facing
  clause-to-scenario backfill.
- `livespec-console-beads-fabro-cc3nlr` — backlog; NFR contributor
  clause-to-scenario backfill.
- `livespec-console-beads-fabro-77t6mk` — backlog; flip behavioral-coverage
  checker to fail mode.
- `livespec-console-beads-fabro-rrr4i4` — backlog; keystone epic that closes
  after the fail-mode flip.

`next --json` returned an empty `candidates[]` envelope, because no downstream
item has been promoted to `ready` after the root slices completed.

## Dependency chain

```text
idgql3 (Scenario 7) ┐
qvrwag (Scenario 6) ┴─► cvqcnx (operator backfill, 30 clauses) ─► cc3nlr (NFR backfill, 52 clauses) ─► 77t6mk (fail-mode flip; closes rrr4i4)
```

## Correction from the parked handoff

The previous handoff parked this thread behind a GitHub-App-auth plan thread and
said to retry a canary dispatch of `idgql3`. That reference is stale:

- No tracked blocking plan thread exists in this repo.
- `idgql3` and `qvrwag` are already `done`.
- The ready lane is empty, so dispatching cannot proceed until the next backlog
  item is promoted.

Do not recreate the old GitHub-App-auth blocker here and do not mint ad-hoc
tokens. This thread's current work is the behavioral-coverage chain above.

## Resume / next action

Open this handoff only:

```bash
sed -n '1,220p' plan/impl-dispatch/handoff.md
```

Then:

1. Re-run the two read-first ledger commands above.
2. If `cvqcnx` is still `backlog` and its dependencies are still `done`,
   promote `cvqcnx` as the next factory-safe slice with `admission:auto` and
   `acceptance:ai-then-human`. If the fresh read shows it is too large or
   non-convergent, run the orchestrator `groom` operation against
   `livespec-console-beads-fabro-cvqcnx` instead and get maintainer approval
   before filing replacement slices.
3. After `cvqcnx` is `ready`, dispatch it through the factory path; do not use
   autonomous drain until the single-item dispatch succeeds.
4. Continue the chain in order: `cc3nlr`, then `77t6mk`, then close `rrr4i4`
   only after fail mode is green in CI.

## Pointers

- Beads tenant: `livespec-console-beads-fabro` on Dolt TCP `127.0.0.1:3307`.
- Always run Beads/orchestrator ledger commands under
  `/data/projects/1password-env-wrapper/with-livespec-env.sh`.
- Repo changes still use the required worktree -> PR -> merge -> cleanup path.
