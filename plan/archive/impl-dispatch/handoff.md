# Console impl-dispatch — archived handoff

**Status:** COMPLETE / ARCHIVED.

**Archived:** 2026-07-10.

## What This Thread Was

This thread groomed and dispatched the console behavioral-coverage chain:
Scenario 6 / Scenario 7 prerequisite realizations, operator-facing and
non-functional-requirements contributor-facing clause-to-scenario backfills, and
the final `console-spec-check` fail-mode flip.

## Final State

The Beads ledger is authoritative, but at archive time the chain was closed:

- `livespec-console-beads-fabro-qvrwag` — closed; Scenario 6 realized.
- `livespec-console-beads-fabro-idgql3` — closed; Scenario 7 realized.
- `livespec-console-beads-fabro-cvqcnx` — closed; operator-facing backfill.
- `livespec-console-beads-fabro-cc3nlr` — closed; NFR contributor backfill.
- `livespec-console-beads-fabro-77t6mk` — closed; fail-mode flip.
- `livespec-console-beads-fabro-rrr4i4` — closed; keystone epic.

Final implementation evidence:

- PR #127 merged by rebase at
  `5990c4ceb8373b1b5f9f96f15ff6bb8e16222e01`.
- `console-spec-check` now defaults to `fail`; explicit
  `LIVESPEC_BEHAVIOR_SCENARIO_LINK=warn` remains the report-only escape hatch.
- Local verification passed: `cargo fmt --all --check`,
  `cargo test -p console-spec-check`, both default and explicit-warn
  `console-spec-check` runs, and `mise exec -- just check`.
- CI run `29055167960` passed every job, including
  `check-behavior-coverage` in default fail mode and `check-coverage`.

## Do Not Resume This Thread

There is no remaining work in `plan/impl-dispatch/`. Future implementation work
should start from the live Beads ranking, not from this archived thread.

At archive time:

```bash
/data/projects/1password-env-wrapper/with-livespec-env.sh -- python3 /home/ubuntu/.codex/plugins/cache/livespec-orchestrator-beads-fabro/livespec-orchestrator-beads-fabro/0.13.9/scripts/bin/next.py --project-root /data/projects/livespec-console-beads-fabro --json
```

returned one candidate:

- `livespec-console-beads-fabro-49a` — comprehensive rustdoc sweep across all
  console crates.

## Pointers

- Beads tenant: `livespec-console-beads-fabro` on Dolt TCP `127.0.0.1:3307`.
- Always run Beads/orchestrator ledger commands under
  `/data/projects/1password-env-wrapper/with-livespec-env.sh`.
- Repo changes still use the required worktree -> PR -> merge -> cleanup path.
