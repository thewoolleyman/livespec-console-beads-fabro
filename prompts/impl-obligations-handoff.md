# Impl-obligations handoff: land the tracked quality-gate + product obligations (console)

Goal: land the impl obligations the spec-refinement track kept FILING and
DEFERRING ("impl realizations the gap pass files, not work that track does") —
**keystone-first**: groom + build the P0 behavioral-coverage checker
(`rrr4i4`), then the remaining gate/checker obligations, then the product
commands. This is Rust **product** work (Red-Green-Replay, `just check` green),
the opposite of the spec-refinement track. Make **NO** changes under
`SPECIFICATION/` here.

> This is the single living impl handoff and the ONE path the next session runs
> (`run prompts/impl-obligations-handoff.md`). It supersedes the prior
> impl-cleanup handoff (its items `0u2` / `o1x` / `awj` are all **closed**) and
> follows the spec-refinement track, which **converged at `v009`**
> (`prompts/spec-refinement-critique-handoff.md`).

## Status (as of master `9f891d2`, spec `v009`)

The 6 open obligations in the Beads tenant — run `… with-livespec-env.sh -- bd
list` to confirm, and **groom every `needs-regroom` epic before implementing**:

| Item | Pri | Ready? | What |
|---|---|---|---|
| `rrr4i4` | **P0** | needs-regroom | **THE KEYSTONE.** clause→scenario→test behavioral-coverage Rust checker + `tests/heading-coverage.json` registry, wired as the CI gate (replacing the fail-closed placeholder), + the ~87-clause backfill. |
| `gkqyaf` | P1 | **ready** | `console-arch-check`: text-scan → `cargo metadata` (crate graph) + Rust AST (unwrap/expect ban, adapter isolation, event/command-type placement). |
| `mvu22t` | P1 | **ready** | Red-Green-Replay `commit-msg` enforcement (port livespec's `dev-tooling/checks/red_green_replay.py`). |
| `txtzn5` | P1 | needs-regroom | region-coverage gate (`--fail-under-regions 100`) + CI merge-gate fuzz + CI mutation jobs (3 distinct jobs). |
| `topr34` | P1 | needs-regroom | nightly fuzz+mutation soak; finding → opens a ready chore. **⚠ MIXED autonomy:** the CI beads-credential wiring is **host/ops / maintainer-owned** — flag it, never fake it. |
| `pke3y3` | P2 | needs-regroom | the 7 unimplemented initial commands (incl. the Scenario 6 policy-rejection path). Product feature work. |

## Operating discipline (MUST — read before any item)

Per `AGENTS.md` §"Repository mutation protocol", §"Beads runtime prerequisites":

- **Worktree, never the primary.** Every mutation in an isolated worktree under
  `~/.worktrees/livespec-console-beads-fabro/<branch>` from `master`; land via
  worktree → PR → merge → cleanup. The commit-refuse hook enforces it; never
  `--no-verify`. Refresh the primary to `origin/master` and remove the worktree
  after each merge.
- **Red-Green-Replay for Rust product changes.** `rrr4i4`, `gkqyaf`, `mvu22t`,
  `txtzn5`, `pke3y3` are Rust product work and MUST follow RGR (Red commit =
  failing test; Green amend = impl + passing evidence) and pass `just check`
  (fmt, strict clippy, `cargo test` + `cargo nextest`, 100% lib line coverage,
  `cargo deny` + `cargo machete`, arch-check) before landing. Note: until
  `mvu22t` lands, RGR is review-enforced, not hook-enforced.
- **Don't fake green; gates land with their checkers.** Never neuter a gate to
  pass. The ONLY legitimate green for these obligations is building them. Do not
  add a fail-closed placeholder for a not-yet-built checker (it deadlocks the
  merge gate — see the spec's Behavioral Coverage note); enforcement attaches to
  the real checker.
- **Wrapped beads only.** Every `bd` / work-item call from the repo root under
  `LIVESPEC_BD_PATH=/usr/local/bin/bd /data/projects/1password-env-wrapper/with-livespec-env.sh -- bd <args>`.
  "Access denied" / "no beads database found" means UNWRAPPED, not a fault. The
  `CALL DOLT_BACKUP … command denied` warning is correct-by-design — ignore it.
- **No `SPECIFICATION/` changes in this track.** If implementing exposes a spec
  ambiguity or a needed spec change, STOP and route it to the spec-refinement
  track (`capture-spec-drift` → propose-change → revise), then resume.

## The keystone first — `rrr4i4`

1. **Groom it** (it is `needs-regroom`):
   `/livespec-orchestrator-beads-fabro:groom livespec-console-beads-fabro-rrr4i4`
   The maintainer OWNS the cut. Expect dependency-layered slices along the
   epic's own split: **(a)** the Rust checker + `tests/heading-coverage.json`
   registry + wiring into `just check` and CI (replacing the fail-closed
   `check-behavior-coverage` placeholder), then **(b)** the ~87-clause
   clause→scenario→test backfill in slices.
2. **The (b) backfill is not just linking — some slices BUILD behavior.** The
   cross-reference comment on `rrr4i4` names two currently-**unrealized**
   scenarios its backfill must implement so their top-of-pyramid tests can pass:
   - **Scenario 6** — policy-rejected command produces no side effect.
     `console-application::handle_factory_drain_command` always invokes the port
     and emits `command.accepted`; it never validates context policy and never
     emits `command.rejected` (`EventType::CommandRejected` is defined but never
     constructed). Realize: validate, emit `command.rejected` with reason,
     invoke no port. (Overlaps `pke3y3` command-handler maturity.)
   - **Scenario 7** — command crash-gap recovery reconstructs a missing outcome.
     No command-context reconciliation exists (only the adapter-level
     safety-window reconcile for Scenario 3).
3. **Implement each slice** via
   `/livespec-orchestrator-beads-fabro:implement <slice-id>` — each
   Red-Green-Replay, `just check` green, landed worktree → PR → merge. For
   gap-tied slices, verify closure by re-running `capture-impl-gaps` in dry-run
   (the `implement` skill does this). Close `rrr4i4` when the checker runs in
   **`fail` mode** in `just check` + CI and every clause→scenario→test link
   resolves.

## Then — the remaining obligations

Use `/livespec-orchestrator-beads-fabro:next` to pick the most-ripe item and
`/livespec-orchestrator-beads-fabro:list-work-items` for state. The two **ready**
P1s (`gkqyaf`, `mvu22t`) are immediately dispatchable and MAY be landed in
parallel with / before grooming the keystone if you want early gate-hardening
momentum:

- **`gkqyaf` (ready)** — `/…:implement livespec-console-beads-fabro-gkqyaf`.
  Crate-graph rules from `cargo metadata`; source rules at the Rust AST level
  (distinguish real calls from substrings like `unwrap_or`).
- **`mvu22t` (ready)** — `/…:implement livespec-console-beads-fabro-mvu22t`.
  First-class in-repo RGR check wired into `commit-msg` + `just check`.
- **`txtzn5` (groom first)** — region-coverage gate + CI fuzz job (≥60s/target
  on the three named targets, committed corpus) + CI mutation job
  (`cargo mutants --in-diff`, justified-survivor allow-list). Part (a) is the
  `coverage-region-gate` spec commitment from `v007`.
- **`topr34` (groom first; MIXED)** — nightly soak + chore-opening. The CI
  beads-credential wiring (`BEADS_DOLT_PASSWORD` via the family wrapper) is
  host/ops — surface it to the maintainer; build the factory-safe nightly job +
  chore-opening around it.
- **`pke3y3` (groom first; P2)** — the 7 commands per `contracts.md` Command
  Handling, one slice per command / bounded context.

## Out of scope for this track

- Any `SPECIFICATION/` change (route to the spec-refinement track).
- Neutering or fail-closing any gate to force green.
- Provisioning host/ops CI credentials beyond flagging the `topr34` step.

## Done criteria

- All 6 obligations closed in the console ledger (or the maintainer-owned
  `topr34` credential step explicitly handed back), no duplicates.
- `just check` green locally and in CI on `master`; the behavioral-coverage and
  RGR gates run against their **real** checkers (no placeholders).
- Primary clean on `master`, `origin/master` carries each landed change, no
  orphaned worktrees/branches, no red/pending CI.
