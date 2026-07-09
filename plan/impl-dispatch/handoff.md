# Console impl-dispatch — thread handoff

**Status:** ACTIVE.

**Refreshed:** 2026-07-10.

## Scope

This is the living implementation-dispatch handoff for the console repo. It is
not complete until the console application is fully running to the maintainer's
standard.

The behavioral-coverage subchain is complete, but that does not complete the
overall application implementation track.

## Read First

Derive live work status from the Beads ledger before acting:

```bash
/data/projects/1password-env-wrapper/with-livespec-env.sh -- bd list --json
/data/projects/1password-env-wrapper/with-livespec-env.sh -- python3 /home/ubuntu/.codex/plugins/cache/livespec-orchestrator-beads-fabro/livespec-orchestrator-beads-fabro/0.13.9/scripts/bin/next.py --project-root /data/projects/livespec-console-beads-fabro --json
```

The ledger is authoritative; this file is only a durable orientation note.

## Completed Subchain

The clause-to-scenario-to-test behavioral-coverage chain is closed:

- `livespec-console-beads-fabro-qvrwag` — closed; Scenario 6 realized.
- `livespec-console-beads-fabro-idgql3` — closed; Scenario 7 realized.
- `livespec-console-beads-fabro-cvqcnx` — closed; operator-facing backfill.
- `livespec-console-beads-fabro-cc3nlr` — closed; NFR contributor backfill.
- `livespec-console-beads-fabro-77t6mk` — closed; fail-mode flip.
- `livespec-console-beads-fabro-rrr4i4` — closed; behavioral-coverage keystone
  epic.

Final evidence for that subchain:

- PR #127 merged by rebase at
  `5990c4ceb8373b1b5f9f96f15ff6bb8e16222e01`.
- `console-spec-check` now defaults to `fail`; explicit
  `LIVESPEC_BEHAVIOR_SCENARIO_LINK=warn` remains the report-only escape hatch.
- Local verification passed: `cargo fmt --all --check`,
  `cargo test -p console-spec-check`, both default and explicit-warn
  `console-spec-check` runs, and `mise exec -- just check`.
- CI run `29055167960` passed every job, including
  `check-behavior-coverage` in default fail mode and `check-coverage`.

## Current Next Work

At this refresh, live `next --json` returned:

- `livespec-console-beads-fabro-49a` — comprehensive rustdoc sweep across all
  console crates.

There are also active/queued items in the ledger that may need cleanup,
acceptance, regrooming, or separate plan threads. Do not infer completion from
the behavioral-coverage chain being closed.

## Guardrails

- Do not archive this thread merely because a subchain closes. Archive only when
  the console implementation track itself is complete or the maintainer
  explicitly decides to close this plan topic.
- Always run Beads/orchestrator ledger commands from the repo root under
  `/data/projects/1password-env-wrapper/with-livespec-env.sh`.
- Repo changes still use the required worktree -> PR -> merge -> cleanup path.
