# Console impl-dispatch — thread handoff

**Status:** ACTIVE.

**Refreshed:** 2026-07-10 (post-49a rustdoc sweep).

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

At this refresh, live `next --json` returns **zero ready candidates** — the
dispatch queue holds no ready work. Every remaining item is gated on a
maintainer decision, so autonomous dispatch is paused pending maintainer input.

Most recently closed:

- `livespec-console-beads-fabro-49a` — comprehensive rustdoc sweep. CLOSED via
  PR #130 (merge `0438f4ad7d2fd2669d17896973d9ffac88cfb2f8`). Enabled
  `#![warn(missing_docs)]` across every crate that lacked it (a permanent,
  workspace-wide coverage gate, since `warnings = "deny"`) and documented the
  10 remaining public field gaps in `console-application`. Every console crate
  already carried crate-level `//!` docs and `///` docs on its public API; the
  gate now keeps it that way.

Remaining ledger items and why each is NOT ready:

- `livespec-console-beads-fabro-6tn` — "Add a crate-level doc comment to
  console-eventstore" (status `active`, assignee fabro). Almost certainly
  ALREADY SATISFIED: `console-eventstore/src/lib.rs` already carries a
  crate-level `//!` doc (confirmed during the 49a sweep). Candidate for
  admin closure (`resolved-out-of-band` / `no-longer-applicable`) — maintainer
  call.
- `livespec-console-beads-fabro-6sf` — deliberate LONG-RUN TTL exercise
  (>60-min delay to validate publish-node token re-mint). `active`, assignee
  fabro; run only when intentionally exercising that path.
- `livespec-console-beads-fabro-mb64bv` — needs-regroom→backlog-bounce rename;
  `active` but BLOCKED by ratification gate `iblkzp` (depends_on edge).
- `livespec-console-beads-fabro-txtzn5`, `-mvu22t`, `-pke3y3` — quality-gate CI
  jobs, red-green-replay checker, 7 unimplemented commands: `backlog`, several
  labelled `needs-regroom` (need regrooming before dispatch).
- `livespec-console-beads-fabro-nxsfih` / `rt4` / `fpo` / `ipi` — console-cruft
  cleanup epic, full-autonomous-mode feature, latent u64→SQLite overflow bug,
  TUI attention-stream migration: `backlog`, awaiting admission / regroom /
  ratification.

Do not infer completion from the behavioral-coverage chain or the rustdoc sweep
being closed. The console implementation track remains open; it is simply
maintainer-gated at this refresh.

## Guardrails

- Do not archive this thread merely because a subchain closes. Archive only when
  the console implementation track itself is complete or the maintainer
  explicitly decides to close this plan topic.
- Always run Beads/orchestrator ledger commands from the repo root under
  `/data/projects/1password-env-wrapper/with-livespec-env.sh`.
- Repo changes still use the required worktree -> PR -> merge -> cleanup path.
