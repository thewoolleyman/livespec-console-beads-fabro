# Fleet repo naming — never use ambiguous shorthand

Three sibling repos, **two of which end in `-beads-fabro`**. Bare
"beads-fabro" is ambiguous and MUST NOT be used.

| Use this name | Repo | Beads tenant | ID prefix |
|---|---|---|---|
| **console** | `livespec-console-beads-fabro` | `livespec-console-beads-fabro` | `livespec-console-beads-fabro-*` |
| **orchestrator-beads-fabro** | `livespec-orchestrator-beads-fabro` | `livespec-orch-beads-fabro` | `bd-ib-*` |
| **orchestrator-git-jsonl** | `livespec-orchestrator-git-jsonl` | `livespec-orchestrator-git-jsonl` | `bd-gj-*` |

**Rule: never write bare "beads-fabro."** Both the console and the
orchestrator repo end in `-beads-fabro`, so it does not identify a repo.
Always disambiguate: `console`, `orchestrator-beads-fabro`,
`orchestrator-git-jsonl`, or the full repo name.

This is a safety rule, not just style. It matters most for
destructive / tenant-scoped operations (beads work-item writes,
gap-capture, `bd delete`): the beads tenant is resolved from the process
**cwd's `.beads/`** (see also `spec-check-and-ci-discipline.md`), and the
console tenant (`livespec-console-beads-fabro`) vs the orchestrator
tenant (`livespec-orch-beads-fabro`) are one careless label away from
each other. Always:

- target a repo by its **full `/data/projects/<full-name>` path** (e.g.
  `env -C /data/projects/livespec-orchestrator-beads-fabro ...`), never a
  bare shorthand, and
- **verify the resulting ID prefix** (`bd-ib-` = orchestrator-beads-fabro,
  `bd-gj-` = orchestrator-git-jsonl, `livespec-console-beads-fabro-` =
  console) before trusting that a write hit the intended tenant.

(This entry exists because bare "beads-fabro" was used as shorthand for
the orchestrator repo during a session that also touched the console —
exactly the setup where a mix-up corrupts the wrong ledger.)
