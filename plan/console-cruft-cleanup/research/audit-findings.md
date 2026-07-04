# Audit findings — console-cruft-cleanup

Comprehensive audit of `livespec-console-beads-fabro` (repo
`thewoolleyman/livespec-console-beads-fabro`) for obsolete or wrong
content, run 2026-07-04 against spec revision **v013**. Maintainer
direction (verbatim, recorded by the overseer): "queue the effort to
completely clean up all obsolete or wrong cruft from the console repo."

Notation: each finding carries a **class** (spec cruft → proposal;
code cruft → work-item; doc cruft → docs-only PR) and a **disposition**
(where it was routed). "Design of record" below always means repo
`thewoolleyman/livespec`,
`plan/archive/work-item-state-machine/research/02-design.md` and
`03-decision-log.md` (decisions cited by number).

## Why this audit exists

The console spec was authored before the work-item-state-machine design
session (2026-06-27) and was only PARTIALLY re-derived against it: the
v0xx lifecycle retrofit fixed the Lanes view (`contracts.md` §"TUI
Contract") and the Rust ingestion path, but left the adapter contract,
the Grooming bounded context, the command vocabulary, and several
scenarios carrying retired vocabulary. The orchestrator repo just spent
a full track (`lifecycle-front-end-retrofit`) repairing the same
failure mode — retired vocabulary surviving in ratified text until an
agent faithfully amplified it into wrong decisions.

## Sweep coverage

- `SPECIFICATION/spec.md`, `contracts.md`, `scenarios.md`,
  `constraints.md`, `non-functional-requirements.md`, `README.md`
  (live; `history/` excluded as frozen) — read in full.
- `README.md`, `AGENTS.md`, `.ai/*.md`, `.livespec.jsonc`, `justfile`,
  `lefthook.yml`, `plan/impl-dispatch/` (the one live plan thread) —
  read/grepped in full.
- All code: `crates/**`, `tests/`, `fuzz/`, `dev-tooling/`,
  `.github/workflows/`, Cargo manifests — swept by a dedicated
  read-only search pass (very thorough; retired-vocabulary term list
  from the design of record).
- Overseer addendum (`tmp/overseer/addendum-command-surface-findings.md`,
  untracked) — command-surface anchors folded in below.

## Spec cruft (→ one proposal set: `SPECIFICATION/proposed_changes/console-cruft-cleanup.md`)

1. **The Beads adapter is a retired read path.**
   `contracts.md` §"Initial Adapters": "**Beads adapter** -- reads Beads
   work-item state through the `bd` CLI and emits
   snapshot/ready/closed/needs-regroom/manual-routing events" — collides
   with locked decisions 15/16 (the console consumes the orchestrator's
   emitted `lane`/`lane_reason` from `list-work-items --json` and holds
   zero Beads knowledge; a raw-`bd` read path would force console-side
   lane recomputation, the shadow-state failure the design killed). The
   same retired source appears in `spec.md` (Purpose sources list;
   both architecture mermaids: `BD["Beads tenant via bd"]`,
   `BeadsAdapter`; Terminology §"Adapter") and in `contracts.md`'s
   source-contract mermaid (`bd list / show / ready`, `beads.*` events),
   and `scenarios.md` Scenarios 1/2/4 name Beads as a polled source.
   NOTE the inversion: the Rust code is ALREADY clean (see
   anti-findings) — here the ratified spec lags the shipped code.

2. **`needs-regroom` vocabulary.** There is no needs-regroom state or
   label — a non-convergence bounce goes to `backlog` (decision 32
   superseding 4; orchestrator `contracts.md` §"Grooming and slice-size
   calibration": "there is no separate needs-regroom state"). Live
   occurrences: `spec.md` §"Bounded Contexts" (Grooming defined around
   "needs-regroom routing" + its mermaid node), `scenarios.md`
   Scenario 1 (mermaid "Dispatcher needs-regroom bounce"; gherkin
   "bounced to needs-regroom"), `contracts.md` Initial commands
   (`grooming.regroom_requested`).

3. **The command vocabulary is missing the human valves entirely.**
   `contracts.md` §"Command Handling" enumerates nine initial commands —
   none of them approve / accept / reject, the two human valves the
   Control-Plane role exists to command, and no policy-edit commands.
   The orchestrator's published surface for these is the `orchestrate
   run` action ids (`approve:<id>`, `accept:<id>`,
   `reject:<id>:rework|regroom` — ratified; `set-admission:<id>`,
   `set-acceptance:<id>` — PENDING in repo
   `thewoolleyman/livespec-orchestrator-beads-fabro`,
   `SPECIFICATION/proposed_changes/approval-is-the-pending-approval-to-ready-transition.md`).
   Also `spec.md` Purpose still frames the operator question with the
   retired markers "Which work is manual or host-only and must not
   enter Fabro?" (the `admission_policy` field replaced the
   `host-only`/`human-gated` text markers). TUI mechanics (command
   modals, palette, type-to-confirm) already exist in the spec, so this
   is vocabulary + mapping only.

4. **NFR nightly-chore clause uses retired filing vocabulary.**
   `non-functional-requirements.md` §"Quality Gate" (nightly): "MUST
   instead open a high-priority chore work-item, **filed ready for
   pickup**" (+ the mermaid "finding -> ready chore work-item" and
   Contributor Scenario C restatement). Under the locked model nothing
   is "filed ready" (approval IS the `pending-approval → ready`
   transition, decision 26/32) and `priority` is dead (`rank` is the
   sole order, decisions 11–12).

## Code cruft (→ work-items, dependency-linked behind ratification)

5. **The `needs-regroom` dispatcher-journal vocabulary is hard-coded.**
   `crates/console-application/src/source_adapters.rs:521-533`
   (`DispatcherJournalKind::NeedsRegroom`, label `"needs-regroom"`),
   `crates/console-domain/src/lib.rs:113,134,155`
   (`DispatcherNeedsRegroomObserved`, wire name
   `"dispatch.needs_regroom_observed"`),
   `crates/console-application/src/lib.rs:1102,1489` (match arm +
   display label "Dispatcher needs-regroom"),
   plus store tests (`console-eventstore/src/lib.rs:757,784`), CLI seed
   wiring (`console-cli/src/lib.rs:421`), fuzz target
   (`fuzz/fuzz_targets/event_envelope.rs:19`), and ~15 test sites.
   Rename to backlog-bounce vocabulary once the spec proposal ratifies;
   the observation cache is rebuildable, so the wire-name change is a
   wipe + re-backfill, never an upcaster (design of record §8).

6. **Command-side implementation is one variant deep** —
   `crates/console-domain/src/lib.rs:225-244` has exactly
   `CommandType::FactoryDrainRequested`. Not itself cruft (epic
   `livespec-console-beads-fabro-pke3y3` already tracks the unbuilt
   commands), but it means the vocabulary fix is spec-level-cheap NOW:
   nothing implements `grooming.regroom_requested`, so deleting it
   from the spec orphans no code, and the valve-command implementation
   slice is recorded as ratification-triggered impl impact in the
   proposal (not pre-filed), mirroring the orchestrator's pending
   proposal's own pattern.

## Doc cruft (→ direct docs-only PR)

7. **`README.md`** calls the architecture check "the current
   text-based architecture check" and lists replacing it as a known
   follow-up — stale: `console-arch-check` is AST-based (`syn` v2,
   `cargo metadata`) per NFR §"Architecture Tests" and the shipped
   crate. The gate list also omits the behavioral-coverage check that
   `just check` runs today (`check-behavior-coverage`). And the bullet
   "manual / host-only work that must not enter the factory" carries
   the retired markers (see finding 3).

8. **`plan/impl-dispatch/handoff.md:39`** uses the fleet-banned
   acronym "DoR" ("intake DoR is applied at capture time") — must read
   "Definition-of-Ready". (The handoff's `backlog → ready` promotion
   narrative at lines 32–40 is a frozen historical record of what was
   done pre-restoration and stays; only the banned acronym is a live
   defect. Its raw `add_labels=[...]` mechanism is exactly the
   consent-bypassing edit the PENDING orchestrator policy-edit action
   ids eliminate — no live doc recommends it going forward.)

## Anti-findings (verified correct — do NOT "fix")

- `contracts.md` §"TUI Contract" Lanes view: consumes the orchestrator's
  emitted `lane`/`lane_reason`, "never re-derives" — ALREADY matches
  the design of record; it is the pattern the adapter section is being
  aligned TO.
- Rust code work-item ingestion is clean: the only work-item source
  command is `list-work-items --json`
  (`source_adapters.rs:1348-1350,1631,1662,1693`); lane/lane_reason are
  consumed verbatim; the eventstore even asserts lane/status exist only
  in payloads, never as columns. Zero `bd` invocations in code.
- No snooze/acknowledge operator actions survive in live code or live
  spec (killed per decision 16; the attention lens offers no local
  dismiss — `scenarios.md` Scenario 5 states it correctly).
- No legacy work-item status vocabulary (`open`/`in_progress`/
  `deferred`) in code; the `Lane` enum is exactly the 7-state set.
  (GitHub PR `open`/`closed` in `source_adapters.rs` is PR state, a
  different domain — not cruft.)
- `priority` appears only as Cargo lint-precedence config; `rank` is
  the sole work-item order everywhere.
- `spec.md` §"Bounded Contexts" Attention definition and the Attention
  gating code (`lib.rs:1211-1231`) match the design (pending-approval +
  manual admission; acceptance + ai-then-human; blocked + needs-human).
- `AGENTS.md`, `.ai/*.md`, `justfile`, `.livespec.jsonc`,
  `lefthook.yml`, CI workflows: no retired vocabulary, no banned
  acronym. Live references to archived threads are correctly labelled
  historical.
- The console tenant ledger is on the 7-state model; one legacy
  `open`-status item remains (`livespec-console-beads-fabro-rt4`,
  "Implement full autonomous mode (operator surface)") — a ledger
  remediation leftover, surfaced to the maintainer in the handoff
  rather than silently rewritten by this track.

## Dispositions

- Findings 1–4 → `SPECIFICATION/proposed_changes/console-cruft-cleanup.md`
  (four `## Proposal` sections, one per finding; maintainer ratifies
  via `/livespec:revise` — RATIFICATION IS THE OPEN GATE).
- Finding 5 → work-item, dependency-linked behind the ratification-gate
  work-item (ids in `../handoff.md`, which derives live status from the
  ledger).
- Findings 7–8 → docs-only PR (no gate).
