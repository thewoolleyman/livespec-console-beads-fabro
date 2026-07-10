# Autonomous-mode MVP — console operator surface plan

**Repo:** `thewoolleyman/livespec-console-beads-fabro` · **Thread:**
`plan/autonomous-mode/` · **Role:** the Control-Plane operator surface for the
autonomous-mode MVP — the TUI toggle, the command surface, the `.livespec.jsonc`
persistence and audit, and the observe/reflect loop. The GUI is OUT of scope.

> **Coordinated by** `livespec/plan/autonomous-mode/design.md` (the overall plan).
> **Depends on** the arming/audit contract published by
> `livespec-orchestrator-beads-fabro/plan/autonomous-mode/design.md`.

---

## 1. Goal (console's half)

From the console **TUI**, a human operator can (a) drive individual work-items
through the human valves manually, and (b) flip a per-repo **full autonomous mode**
that enables the orchestrator plane's own auto-resolution engine and then
**observes, records, and reflects** every auto-resolution — surfacing only
truly-unresolvable decisions back as in-TUI needs-attention items. The console
never owns a gate decision; it enables the owning plane and reflects the outcome.

## 2. Current state (2026-07-10 survey)

**Spec (current version v016) is a COMPLETE normative definition of this MVP**, with
zero pending proposals:
- `SPECIFICATION/spec.md` §"Full Autonomous Mode" — per-repo, default-disabled,
  dangerous, revocable; the console resolves (via an LLM stand-in) the operator
  decisions it would prompt for and issues commands through each plane's published
  surface; the engine that makes the orchestrator's own decisions is owned by the
  orchestrator plane — the console only enables, observes, reflects.
- `SPECIFICATION/contracts.md` §"Command Handling" — the five `work_item.*` commands
  map 1:1 onto the orchestrator `orchestrate run` action ids: `approve:<id>`,
  `accept:<id>`, `reject:<id>:{rework,regroom}`, `set-admission:<id>:{auto,manual}`,
  `set-acceptance:<id>:{ai-only,human-only,ai-then-human}` — issued ONLY through that
  surface, never writing the ledger directly.
- `SPECIFICATION/contracts.md` §"Autonomous Mode" — preference persisted per-repo
  under the console's namespaced `.livespec.jsonc` block
  (`"autonomous_mode": { "enabled": false }`); absent block = disabled;
  `config.autonomous_mode_set` rejects `enabled:true` without `confirmed:true`, and
  on acceptance persists to `.livespec.jsonc` AND appends the audit event; events
  `config.autonomous_mode.enabled/.disabled`; commands
  `factory.autonomous_mode_enable/disable_requested` with honesty
  `factory.autonomous_mode.not_wired`.
- `SPECIFICATION/contracts.md` §"TUI Contract" — five views (needs-attention, Spec,
  Lanes, Events, Repos); default needs-attention; the autonomous-mode toggle with a
  "dangerous / use with caution" label, a type-to-confirm modal to enable, and a
  header mode indicator.
- `SPECIFICATION/constraints.md` §"Autonomous-Mode Safety"; Scenarios 9-12.

**Implementation contains essentially none of it** (file:line from the survey):
- `crates/console-domain/src/lib.rs:310-313` — `CommandType` has only
  `FactoryDrainRequested`. None of the five `work_item.*` commands,
  `config.autonomous_mode_set`, or the two `factory.autonomous_mode_*_requested`
  commands exist.
- `grep -rin autonomous crates/` = 0 hits.
- `crates/console-application/src/lib.rs:95-106,146-184` — `TuiView` =
  {Attention, Spec, Lanes, Events, Repos}; `TuiOverlay` = {None, Search,
  CommandPalette, CommandModal}; `OperatorAction` = {OpenFabroAttach,
  CopyFabroAttach}. No autonomous toggle, confirm modal, "dangerous" label, or header
  mode indicator.
- No `.livespec.jsonc` / Configuration reading anywhere in `crates/*/src/`; the
  repo's `.livespec.jsonc` carries no `autonomous_mode` block.
- The factory-drain port IS real: `DispatcherFactoryDrainPort` is constructed in both
  live paths (`crates/console-cli/src/main.rs:104` serve, `:130` TUI) and runs a real
  probe; `FactoryDrainPortOutcome::NotWired` is the honest unavailable-dispatcher
  fallback, not a stub. (This is the pattern the autonomous commands reuse for their
  own honesty signal.)

**Foundation already landed** (archived thread `work-item-lifecycle-redesign`,
closed 2026-07-01): the seven stored lifecycle lanes, `lane_of` as the single
authority, the console consumes the orchestrator's emitted `lane`/`lane_reason` and
NEVER re-derives a lane, the two human-delegable valves + fractional `rank`. The
Lanes view and valve commands sit on this.

## 3. The gap list to a TUI-driven autonomous MVP

1. The five `work_item.*` valve/policy commands: `CommandType` variants + handlers +
   an orchestrator action-surface port (the actions already ship on the orchestrator
   `drive` skill — wire to them).
2. `config.autonomous_mode_set` + a Configuration context that reads/writes the
   `.livespec.jsonc` `autonomous_mode` block + the `config.autonomous_mode.enabled/
   .disabled` audit events + the two `factory.autonomous_mode_enable/disable_requested`
   commands (+ `factory.autonomous_mode.not_wired` honesty outcome).
3. The TUI autonomous toggle + type-to-confirm modal (enable only) + "dangerous /
   use with caution" label + header mode indicator.
4. The Scenario-10 auto-resolve-the-decidable / escalate-the-rest loop that issues
   each plane's commands and ENABLES the orchestrator's own engine (the engine is the
   separate orchestrator item `bd-ib-82a`, NOT console work — see §5).
5. Two lifecycle-redesign follow-ups this leans on: `ipi` (migrate the TUI
   needs-attention view from lane-derived to the `attention_item.*` stream,
   Scenario 12) and `mb64bv` (rename the dispatcher-journal `needs-regroom` vocab to
   `backlog-bounce`).

## 4. Steps

### C1 — spec currency + seam reconciliation
The spec is complete, so this step is validation-and-reconcile, not authoring.
- **Diff the borrowed vocabulary against current core/orchestrator** (the "livespec
  moved forward" concern): (a) the seven lane names + the `blocked:dependency`
  overlay; (b) the acceptance-policy enum `{ai-only, human-only, ai-then-human}` and
  reject modes `{rework, regroom}`; (c) the `attention_item.*` schema (item `ipi` /
  Scenario 12) versus core's current `needs-attention` schema. The console cites the
  orchestrator contract by reference (correct), but confirm no drift.
- **Resolve the persistence-model seam** (overall plan §6.1): the console persists
  `autonomous_mode.enabled` (intent); define how that intent reaches the orchestrator,
  which does not persist autonomous mode. This must agree with the orchestrator's O1
  arming contract.
- **Resolve the division-of-resolution question** (overall plan §6.2): pin which
  decisions the console's own LLM-stand-in resolves versus which it delegates
  wholesale to the orchestrator engine. Recommended MVP reading: the orchestrator
  engine owns all gate resolution; the console's autonomous responsibility is
  enable + observe + reflect + surface-unresolvable, keeping the console's LLM layer
  thin or deferred.
- **Resolve the `config.autonomous_mode_set` naming** consistency versus the factory
  `autonomous_mode_enable/disable_requested` pair (the `_set` command verb is correct
  at the command layer; the inconsistency is single-`_set` vs split enable/disable —
  decide one convention).
- Refresh item `rt4`'s stale version pointer (cites v013; spec is v016).
- **Route:** any real change via `/livespec:propose-change` → independent Fable
  review → `/livespec:revise`, co-editing `tests/heading-coverage.json` for any H2
  change. **Gate:** overall Step 0. **Done:** ratified revision or a documented
  "no change needed" with the seams pinned.

### C2 — console command foundation (manual valves)
- Add the five `work_item.*` `CommandType` variants + handlers + a port that issues
  them through the orchestrator's existing published `orchestrate run` surface. Fold
  item `pke3y3` (regroom it against the current valve model first — it predates the
  lifecycle redesign). Land the Scenario-11 test.
- TDD Red-Green-Replay per the repo's Rust ritual; worktree → PR → merge.
- **Gate:** C1. **Done:** merged PR; the TUI can issue each valve manually against a
  real tenant (live evidence, not just tests).

### C3 — console autonomous-mode feature
- Add `config.autonomous_mode_set` + the Configuration context (`.livespec.jsonc`
  read/write) + the `config.autonomous_mode.enabled/.disabled` audit events + the two
  `factory.autonomous_mode_enable/disable_requested` commands (+ `not_wired` honesty).
- Add the TUI toggle, type-to-confirm modal (enable only; disable needs no
  confirmation), "dangerous / use with caution" label, and header mode indicator.
- Implement the Scenario-10 loop scoped per the C1 resolution-division decision.
- Fold item `rt4`. TDD; worktree → PR → merge.
- **Gate:** C1 AND C2 AND the orchestrator arming contract frozen (overall plan I1).
  **Done:** merged PR; the TUI toggle round-trips intent → orchestrator arming
  command → observed/reflected outcome, with `config.autonomous_mode.enabled` audited.

## 5. What is NOT console work (plane boundary)

The **decision engine** that actually LLM-resolves the orchestrator's parked gates
(`blocked_reason: needs-human`, manual admission, the human acceptance leg) is
owned by the Orchestrator Plane and tracked by orchestrator item `bd-ib-82a`
(`livespec-orchestrator-beads-fabro/plan/autonomous-mode/`). The console ENABLES it
via a published command and REFLECTS its audited outcomes; the console MUST NOT
reach around the plane to fabricate a gate decision. Truly-unresolvable decisions —
including the core-spec irreducible human touchpoints (drift acceptance,
spec-change slices, regroom) — surface as in-TUI needs-attention, never guessed.

## 6. Items to fold / supersede
- `rt4` (feature, backlog) — THE console operator-surface item; folded into C3.
- `pke3y3` (task, backlog) — the "7 unimplemented commands" item; regroom to the five
  current valve/policy commands and fold into C2.
- `ipi` (task, backlog) — attention-stream TUI migration (Scenario 12); fold into C1
  (spec confirm) + C3 (the reflect surface consumes `attention_item.*`).
- `mb64bv` (task, active) — `needs-regroom` → `backlog-bounce` vocab rename; a small
  independent cleanup that can land ahead of C2 (removes stale vocab the valves touch).
- `plan/impl-dispatch/` (behavioral-coverage chain) — COMPLETE and unrelated;
  recommend archiving separately, not a dependency here.

## 7. Definition of done (console's contribution to the MVP)
C3 merged and live-exercised: from the TUI, enabling autonomous mode is
dangerous-labelled + type-to-confirm, persists `autonomous_mode.enabled` to
`.livespec.jsonc`, emits `config.autonomous_mode.enabled`, issues the arming command
to the orchestrator, and the header reflects the active mode; disabling round-trips
back and returns decidable items to human routing. Final MVP acceptance is the
overall plan's I2 end-to-end live exercise, which this repo participates in.

## 8. Discipline
Worktree → PR → merge → cleanup from the console primary checkout on `origin/master`;
`mise exec -- git …`; never `--no-verify`. Product Rust changes use the repo's
Red-Green-Replay ritual; plan-doc commits are `docs(plan):`. Any spec H2 change
co-edits `tests/heading-coverage.json`. End on `master`, clean.
