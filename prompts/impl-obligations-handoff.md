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

> **Progress (last session).** Keystone slice **A — `uljbzh`** is **CLOSED**:
> the net-new `console-spec-check` crate ports livespec's `spec_clauses` gap-id
> primitive (parity test vs the orchestrator's vendored Python; real-spec
> ground truth **82** clauses: spec 3 / contracts 20 / constraints 7 / nfr 52)
> + the `behavior_scenario_link` clause→scenario guardrail + adds scenario→test
> enforcement over a net-new `tests/heading-coverage.json`. It is **warn-wired**
> as `check-behavior-coverage` into `just check` + CI via the
> `LIVESPEC_BEHAVIOR_SCENARIO_LINK` lever (default `warn`; reports **82 unlinked
> clauses + 8 untested scenarios**, exits 0 — NO fail-closed placeholder). PR
> #48, rebase merge `edbb06c`; `just check` + all 9 CI jobs green. Closing it
> **unblocked `qvrwag` (S6) and `idgql3` (S7)** — both now ready. (Earlier:
> `gkqyaf` CLOSED, PR #42, `6171984`.) The **SC-nfr** spec change still sits in
> `SPECIFICATION/proposed_changes/nfr-contributor-scenarios.md` awaiting the
> spec-refinement track's `/livespec:revise` — it gates `cc3nlr` (B-nfr). NOTE:
> a **concurrent M3 track** also operates this repo/ledger (`d5c`/`e8y`,
> baseline-conformance `76c9fc2`); those are that track's items, not this
> handoff's. Coordinate; don't clobber its worktrees.

## Status (as of master `edbb06c`, spec `v009`)

This handoff's obligations in the Beads tenant — run `… with-livespec-env.sh --
bd list` to confirm (it also shows the concurrent M3 track's `d5c`/`e8y`), and
**groom every remaining `needs-regroom` epic (`txtzn5`, `topr34`, `pke3y3`)
before implementing** (`rrr4i4` is already groomed + filed):

| Item | Pri | Ready? | What |
|---|---|---|---|
| `rrr4i4` | **P0** | groomed → FILED; **slice A done** | **THE KEYSTONE**, 6 filed slices. **A=`uljbzh` ✅ CLOSED** — `console-spec-check` checker + `tests/heading-coverage.json`, warn-wired (PR #48, `edbb06c`). **Now ready: `qvrwag` (S6), `idgql3` (S7)** → then `cvqcnx` (B-ops, deps S6+S7); `cc3nlr` (B-nfr, gated on SC-nfr landing); `77t6mk` (F = flip lever default to `fail`, closes the epic). SC-nfr routed (PR #46), awaiting `/livespec:revise`. |
| `gkqyaf` | P1 | ✅ **CLOSED** | DONE — `console-arch-check` upgraded to `cargo metadata` crate-graph + Rust AST rules. PR #42, merge `6171984`. |
| `mvu22t` | P1 | **ready** | Red-Green-Replay `commit-msg` enforcement (port livespec's `red_green_replay.py`; canonical source now at `livespec-dev-tooling/livespec_dev_tooling/checks/red_green_replay.py`). The one remaining immediately-dispatchable P1. |
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
- **Red-Green-Replay for Rust product changes.** `rrr4i4`, `mvu22t`,
  `txtzn5`, `pke3y3` are Rust product work and MUST follow RGR (Red commit =
  failing test; Green amend = impl + passing evidence) and pass `just check`
  (fmt, strict clippy, `cargo test` + `cargo nextest`, 100% lib line coverage,
  `cargo deny` + `cargo machete`, arch-check) before landing. Note: until
  `mvu22t` lands, RGR is review-enforced, not hook-enforced — `gkqyaf` landed
  its completed Red→Green as one commit carrying both `TDD-*` trailer sets
  (the spec's "final commit" shape), which is the precedent to follow.
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

## The keystone — `rrr4i4` (groomed → FILED; implement the slices)

The cut below was **approved and FILED**; `rrr4i4` is regroomed out. Filed
slice ids: `uljbzh` (A, **✅ CLOSED** — checker landed, PR #48 `edbb06c`),
`qvrwag` (S6, **now ready**), `idgql3` (S7, **now ready**), `cvqcnx` (B-ops —
deps S6+S7), `cc3nlr` (B-nfr — blocked on SC-nfr landing), `77t6mk` (F =
fail-flip, closes the epic). SC-nfr was routed to the spec-refinement track
(PR #46; `SPECIFICATION/proposed_changes/nfr-contributor-scenarios.md`, awaiting
`/livespec:revise`). **Next action:** implement a now-ready slice —
`/livespec-orchestrator-beads-fabro:implement livespec-console-beads-fabro-qvrwag`
(S6) or `…-idgql3` (S7); both must land before `cvqcnx` (B-ops). The ground
truth + per-slice scope below remain the authoritative reference for each slice.
Slice A's shipped shape is the reference for the others: `console-spec-check`
crate (lib at 100% line coverage + thin `main.rs` shim), the
`tests/heading-coverage.json` `clauses[]` registry, the
`LIVESPEC_BEHAVIOR_SCENARIO_LINK` warn/fail lever, and the audience partition
(operator clauses → `scenarios.md`; NFR clauses → NFR `## Scenarios`).

**Ground truth (measured via the real `spec_clauses.py` over the console
SPECIFICATION):** **82** normative clauses — spec.md 3, contracts.md 20,
constraints.md 7, non-functional-requirements.md **52**. Binding rule (NFR
§Behavioral Coverage): operator-facing clauses (spec/contracts/constraints,
**30**) link to `scenarios.md` H2s (Scenarios 1–8, which exist); NFR's own
contributor-facing clauses (**52**) link to NFR `## Scenarios` H2s. The
console repo has **no** `tests/` or `dev-tooling/checks/` — the checker and
`tests/heading-coverage.json` are net-new. gap-id = `gap-` + first 8 lc
base32 of `sha256(spec_file \x1f heading_path \x1f rule_text)`; the Rust port
MUST be byte-identical (orchestrator `detect-impl-gaps` vendors the same
primitive — add a parity test). Keep the checker a **separate** crate
(`console-spec-check`), not inside `console-arch-check` (now AST-based).

**No fail-closed placeholder exists** (v009 reconciled that away — verified:
`just check` has no `check-behavior-coverage` target). The checker wires into
`just check`/CI **fresh**, with a severity lever (env, default `warn`, like
livespec's `LIVESPEC_BEHAVIOR_SCENARIO_LINK`) so it can land + run during
backfill WITHOUT deadlocking the merge gate; the final slice flips the
default to **`fail`**.

### ⚠ NFR-link blocker (load-bearing, cross-track)
The **52 NFR contributor-facing clauses** must link to NFR `## Scenarios`
H2s, but that section is **empty** ("No contributor-facing scenarios are
pinned yet"). Authoring those scenarios (or changing the binding rule) is a
`SPECIFICATION/` change — **out of this impl track**. So the keystone CANNOT
reach full `fail` mode on the impl side alone; it has a hard dependency on a
spec-refinement deliverable. Two resolutions, both spec changes to route via
`capture-spec-drift` → `/livespec:propose-change` → revise:
- **(RECOMMENDED) Simplify the binding rule** so NFR clauses link to
  `scenarios.md` operator scenarios (as livespec itself does — its
  `behavior_scenario_link.py` resolves ALL clauses against `scenarios.md`),
  dropping the separate NFR `## Scenarios` requirement. Smallest spec surface.
- **Author contributor scenarios** under NFR `## Scenarios` (+ tests). Richer
  spec, more to maintain.

### Drafted cut (factory unless noted; deps = earlier factory slice titles)
- **A** — `console-spec-check` Rust checker + `tests/heading-coverage.json` +
  warn-wiring into `just check`/CI. Ports `spec_clauses` (gap-id parity test)
  + `behavior_scenario_link` + scenario→test enforcement. Deps: none.
- **S6** — Realize Scenario 6 (policy-rejected drain → `command.rejected`, no
  port). `handle_factory_drain_command` (console-application/src/lib.rs:~780)
  invokes the port + emits `CommandAccepted` UNCONDITIONALLY — no policy gate;
  `EventType::CommandRejected` (console-domain/src/lib.rs:~89) is defined but
  NEVER constructed. Add a policy gate before the port; construct
  `command.rejected` with a reason; invoke no port. + Scenario 6 test.
  Deps: A. (Overlaps `pke3y3`.)
- **S7** — Realize Scenario 7 (crash-gap reconciliation reconstructs a missing
  outcome). No command-context reconciliation exists (only the adapter
  safety-window reconcile for Scenario 3). + Scenario 7 test. Deps: A.
- **B-ops** — Backfill the 30 operator-facing clause→scenario links
  (spec/contracts/constraints → Scenarios 1–8) + register tests for Scenarios
  1–5,8. Deps: A, S6, S7.
- **SC-nfr** — *(SPEC CHANGE, routed to propose-change — NOT factory)* resolve
  the NFR-link blocker (recommend binding-rule simplification).
- **B-nfr** — Backfill the 52 NFR clause links (+ tests if scenarios authored).
  Deps: A. **External gate:** SC-nfr must land first (documented, not a ledger
  edge since SC-nfr is routed).
- **F** — Flip the lever default to **`fail`** + close the keystone. Deps:
  B-ops, B-nfr, S6, S7. Acceptance: `just check`/CI green in `fail` mode, 0
  unlinked / 82, every scenario tested.

**Filing status: DONE; A landed.** All six factory slices are filed
(A=`uljbzh` **✅ CLOSED**, S6=`qvrwag`, S7=`idgql3`, B-ops=`cvqcnx`,
B-nfr=`cc3nlr`, F=`77t6mk`) and `rrr4i4` is regroomed out. With A closed,
**`qvrwag` (S6) and `idgql3` (S7) are now ready**; `cvqcnx` (B-ops) stays
blocked until both land. SC-nfr is routed (PR #46); `cc3nlr` (B-nfr) is
**blocked until SC-nfr lands** on master via `/livespec:revise` — do not
implement it before then.

**Implement each slice** via
`/livespec-orchestrator-beads-fabro:implement <slice-id>` — each
Red-Green-Replay, `just check` green, landed worktree → PR → merge. For
gap-tied slices, verify closure by re-running `capture-impl-gaps` in dry-run
(the `implement` skill does this). Close `rrr4i4` when the checker runs in
**`fail` mode** in `just check` + CI and every clause→scenario→test link
resolves.

## Then — the remaining obligations

Use `/livespec-orchestrator-beads-fabro:next` to pick the most-ripe item and
`/livespec-orchestrator-beads-fabro:list-work-items` for state. With slice A
(`uljbzh`) closed, the ripest keystone actions are **`qvrwag` (S6)** and
**`idgql3` (S7)** — both now ready (see the keystone section). `mvu22t` (P1) is
an independent autonomous win (gate-hardening momentum) and can run in parallel:

- **`mvu22t` (ready)** — `/…:implement livespec-console-beads-fabro-mvu22t`.
  First-class in-repo RGR check wired into `commit-msg` + `just check`. Port
  the canonical source `livespec-dev-tooling/livespec_dev_tooling/checks/
  red_green_replay.py` (trailer keys `TDD-Red-Test-File-Checksum:` /
  `TDD-Green-Verified-At:` pair shape, or `TDD-Suite-Green-Captured-At:` for
  behavior-preserving), adapted to Rust product crates (`.rs` under `crates/`,
  excluding `#[cfg(test)]`/docs/chore). ⚠ Blast radius: once landed its
  `commit-msg` hook gates ALL later commits — test it thoroughly first and
  exempt `docs(...)`/`chore(...)`.
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

- All obligations closed in the console ledger (`gkqyaf` ✅ done; remaining:
  `rrr4i4`, `mvu22t`, `txtzn5`, `topr34`, `pke3y3`), or the maintainer-owned
  steps explicitly handed back (the `topr34` CI credential; the keystone's
  `SC-nfr` spec change). No duplicates. The keystone's full `fail`-mode close
  depends on `SC-nfr` landing on the spec side (see the NFR-link blocker).
- `just check` green locally and in CI on `master`; the behavioral-coverage and
  RGR gates run against their **real** checkers (no placeholders).
- Primary clean on `master`, `origin/master` carries each landed change, no
  orphaned worktrees/branches, no red/pending CI.
