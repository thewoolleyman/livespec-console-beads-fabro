---
topic: persistence-seam-single-permission
author: claude-opus-4-8
created_at: 2026-07-11T06:00:00Z
---

## Proposal: Drop the console's own autonomous-mode persistence in favor of the orchestrator's single permission key

### Target specification files

- SPECIFICATION/contracts.md
- SPECIFICATION/spec.md
- SPECIFICATION/scenarios.md

### Summary

Resolve the persistence-model seam (console plan
`plan/console-autonomous-mode/design.md`, the "Resolve the persistence-model seam"
bullet; overall plan `thewoolleyman/livespec` `plan/autonomous-mode/design.md`
§6.1) now that the orchestrator's O1 arming contract has FROZEN. The console
STOPS persisting its own per-repo autonomous-mode preference. The SINGLE
persistent permission becomes the orchestrator plane's published key
`livespec-orchestrator-beads-fabro.dispatcher.autonomous_mode` in the repo's
`.livespec.jsonc`. Three sites are re-cast: (1) contracts.md §"Autonomous Mode"
DROPS the console's own persisted `"livespec-console-beads-fabro": {
"autonomous_mode": { "enabled": false } }` block and DERIVES the current
per-repo mode by reading the orchestrator's key; (2) the
`config.autonomous_mode_set` handler is RE-TARGETED to effect an enable/disable
by writing the orchestrator's key THROUGH the orchestrator's published command
surface (its `factory.autonomous_mode_enable_requested` /
`factory.autonomous_mode_disable_requested` command) plus the audit event,
rather than persisting a console-owned block -- the confirm-guard and the
`config.autonomous_mode.enabled` / `.disabled` audit events are UNCHANGED; and
(3) spec.md §"Full Autonomous Mode" re-casts its "the mode preference is
persisted per-repo" sentence to the single-permission model (the console
derives/reflects the orchestrator's key, not a console-owned preference). A
scenarios.md drift-sweep re-scopes Scenario 9's `.livespec.jsonc`-persist
mermaid node and Gherkin step so no scenario still asserts the console persists
its own preference.

### Motivation

The orchestrator's O1 arming contract is now FROZEN and it settles the seam
directly. Repo `thewoolleyman/livespec-orchestrator-beads-fabro`,
`SPECIFICATION/contracts.md` §"Arming full autonomous mode" declares the key
`livespec-orchestrator-beads-fabro.dispatcher.autonomous_mode` to be "the SINGLE
persistent record of the operator's intent to allow unattended autonomous runs
for this repo, and it is this plugin's PUBLISHED arming surface: the
Control-Plane console arms and disarms autonomous mode by setting this key (the
console's `factory.autonomous_mode_enable_requested` /
`factory.autonomous_mode_disable_requested` commands map to writing it)." The
same section is explicit about the console's obligation: "Any duplicate
persistent autonomous-mode preference in the same `.livespec.jsonc` is redundant
with this key and is dropped or defined as derived from it; reconciling the
console's own block is the console contract's concern."

The console spec today carries exactly that redundant duplicate: contracts.md
§"Autonomous Mode" persists a second boolean under
`"livespec-console-beads-fabro": { "autonomous_mode": { "enabled": false } }`,
and the `config.autonomous_mode_set` handler writes `"enabled"` back to that
console-owned block. With the orchestrator's key frozen as the single
permission, two persistent booleans would coexist and could disagree. This
change performs the console-side reconciliation the orchestrator's contract
assigns to the console: drop the console-owned block, derive the mode from the
orchestrator's key, and re-target the handler to write the orchestrator's key
through the published command surface.

Design record (read in full): repo `thewoolleyman/livespec-console-beads-fabro`,
`plan/console-autonomous-mode/design.md`, the C1 "Resolve the persistence-model seam
(overall plan §6.1)" bullet -- "the console's
`factory.autonomous_mode_enable/disable_requested` commands set the
ORCHESTRATOR's key, and the console's own namespaced `autonomous_mode` block is
dropped or redefined as derived -- whichever O1's frozen contract says, amend
contracts.md §Autonomous Mode to match"; and repo
`thewoolleyman/livespec-orchestrator-beads-fabro`, `SPECIFICATION/contracts.md`
§"Arming full autonomous mode" (the frozen O1 arming contract quoted above).

This is one atomic proposed change (one `## Proposal:` section, one per-file
revise decision under this topic). It DROPS/REWORDS normative MUST clauses, so
it triggers the console clause-coverage co-edit declared in
"### Resulting-files co-edit" below; those two files are NOT edited in this
propose-change PR (doing so now would break `console-spec-check`, since the live
clause is still present until revise) -- they are declared here and applied by
the driver at revise. This change alters no `## ` heading.

### Proposed Changes

All quoted current text is verbatim from the live console spec files (head
`2737cdb`).

#### contracts.md

`[DROP]` + `[DERIVE]` **§"Autonomous Mode"**, the opening persistence clause,
its `jsonc` block, and the read clause -- replace:

> The console's full-autonomous-mode preference (see `spec.md` -> Full
> Autonomous Mode) MUST be persisted per-repo under the console's namespaced
> block in that repo's `.livespec.jsonc`:
>
> ```jsonc
> "livespec-console-beads-fabro": {
>   "autonomous_mode": { "enabled": false }
> }
> ```
>
> The console's Configuration context MUST read this setting for each
> registered repo; an absent block or key MUST be treated as disabled.

with:

> The console's full-autonomous-mode preference (see `spec.md` -> Full
> Autonomous Mode) is NOT persisted by the console. The single persistent
> record of the operator's intent to allow unattended autonomous runs for a
> repo is the orchestrator plane's permission key
> `livespec-orchestrator-beads-fabro.dispatcher.autonomous_mode` in that repo's
> `.livespec.jsonc` -- the orchestrator's PUBLISHED arming surface (repo
> `thewoolleyman/livespec-orchestrator-beads-fabro`,
> `SPECIFICATION/contracts.md` -> Arming full autonomous mode). The console
> does not persist a second, console-owned autonomous-mode preference; any such
> duplicate would be redundant with that key.
>
> The console's Configuration context MUST derive the current per-repo mode by
> reading that orchestrator key; an absent key MUST be treated as disabled.

`[RETARGET]` **§"Autonomous Mode"**, the `config.autonomous_mode_set`
acceptance sentence (the confirm-guard sentence immediately before it is
UNCHANGED) -- replace:

> On acceptance the
>   handler MUST persist `"enabled": <bool>` back to that repo's
>   `.livespec.jsonc` AND append the matching audit event, so file state and
>   event log never disagree.

with:

> On acceptance the
>   handler MUST effect the change by writing the orchestrator's
>   `livespec-orchestrator-beads-fabro.dispatcher.autonomous_mode` key through
>   the orchestrator's published command surface -- issuing the
>   `factory.autonomous_mode_enable_requested` /
>   `factory.autonomous_mode_disable_requested` command below -- AND append the
>   matching audit event, rather than persisting a console-owned
>   `autonomous_mode` block, so the orchestrator key and the console's audit
>   log never disagree.

#### spec.md

`[DERIVE]` **§"Full Autonomous Mode"**, the persistence sentence (the
preceding `disabling it MUST return every decidable item to human routing.`
clause and the trailing `constraints.md` reference are UNCHANGED) -- replace:

> The mode preference is
> persisted per-repo and audited per `contracts.md` -> Autonomous Mode; its
> operator-observable safety constraints live in `constraints.md` ->
> Autonomous-Mode Safety.

with:

> The single persistent permission is the orchestrator plane's
> `dispatcher.autonomous_mode` key; the console does not persist its own
> preference -- it derives and reflects that key, sets it through the
> orchestrator's published command surface, and audits each change, per
> `contracts.md` -> Autonomous Mode. Its operator-observable safety constraints
> live in `constraints.md` -> Autonomous-Mode Safety.

#### scenarios.md

`[DRIFT-SWEEP]` **§"Scenario 9 -- Enabling full autonomous mode is guarded and
audited"**. The scenario's flow still shows the console writing `enabled=true`
to `.livespec.jsonc`, which contradicts the drop. Three targeted re-scopes; the
actual persistence is already carried by the existing "issues
`factory.autonomous_mode_enable_requested` to the orchestrator through its
published command surface" step (unchanged), which now IS the write of the
orchestrator's key.

(1) The mermaid `Persist` node -- replace:

> ```
>   Command["config.autonomous_mode_set confirmed=true"]
>   Persist["Write enabled=true to .livespec.jsonc"]
>   Audit["config.autonomous_mode.enabled event"]
> ```

with:

> ```
>   Command["config.autonomous_mode_set confirmed=true"]
>   Audit["config.autonomous_mode.enabled event"]
> ```

(2) The mermaid flow edge that ends at `Persist` -- replace:

> ```
>   Operator --> Label --> Confirm --> Command --> Persist
> ```

with:

> ```
>   Operator --> Label --> Confirm --> Command
> ```

(3) The Gherkin scenario title and the `.livespec.jsonc`-persist step --
replace:

> Scenario: Enabling autonomous mode is confirmed, persisted, and audited

with:

> Scenario: Enabling autonomous mode is confirmed, armed, and audited

and replace:

>   And the console submits config.autonomous_mode_set with confirmed true
>   And persists enabled true to the repo's .livespec.jsonc
>   And appends a config.autonomous_mode.enabled audit event

with:

>   And the console submits config.autonomous_mode_set with confirmed true
>   And appends a config.autonomous_mode.enabled audit event

### Resulting-files co-edit

This change DROPS/REWORDS normative MUST clauses in contracts.md, so ACCEPTANCE
(at `/livespec:revise` time) MUST carry, in the SAME revise commit, the
lockstep console clause-coverage co-edit below. These two files are NOT edited
in THIS propose-change PR -- editing them now would break `console-spec-check`,
because the live contracts.md clause is still present until revise. They are
DECLARED here and applied by the driver at revise, computed from the ratified
reworded text via the `console-spec-check` `derive_gap_id` primitive (mirroring
the C1 v017 lockstep, revise `8aa5d54`, which refreshed the `console-spec-check`
counts and rebound the `heading-coverage.json` gap-ids in one commit).

**`tests/heading-coverage.json`** -- rebind the changed clauses' gap-ids under
the "Scenario 9 -- Enabling full autonomous mode is guarded and audited" entry:

- **DROP** the clause-gap-id of the REMOVED clause: `gap-dchrh3if` (the deleted
  opening "MUST be persisted per-repo under the console's namespaced block"
  clause, contracts.md).
- **REBIND** (recompute the gap-id string from the ratified reworded line; the
  Scenario-9 binding is unchanged) for each REWORDED clause:
  - `gap-d24kqbpi` -> the reworded "Configuration context MUST derive ... by
    reading that orchestrator key" line.
  - `gap-cu3t3prv` -> the reworded "an absent key MUST be treated as disabled"
    line.
  - `gap-lswx3ste` -> the re-targeted "handler MUST effect the change by
    writing the orchestrator's ... key through the orchestrator's published
    command surface ... AND append the matching audit event" line.
- **UNCHANGED** (do NOT rebind): `gap-dixkqk3i` (the confirm-guard "handler MUST
  reject the command when `enabled` is `true` and `confirmed` is not `true`"
  clause) and every other Scenario-9 clause (the spec.md §"Full Autonomous Mode"
  danger/default/revocable clauses, the constraints.md §"Autonomous-Mode Safety"
  clauses, and the not-wired/honesty clauses). The spec.md persistence
  re-cast changes a NON-clause sentence (no `MUST`/`SHOULD`), so it introduces
  no gap-id change. The scenarios.md drift-sweep touches only fenced
  mermaid/gherkin (not clause-bearing), so it rebinds nothing.

**`crates/console-spec-check/src/tests.rs`** -- refresh the ground-truth
`clause count for {file}` golden tuples. Expected count delta:

- **contracts.md: 37 -> 36 (-1).** Exactly ONE `MUST`/`SHOULD` clause is
  REMOVED (the standalone opening "MUST be persisted per-repo under the
  console's namespaced block" clause). The read clause, the absent-key clause,
  and the handler clause are REWORDED in place (count-neutral); the negative
  "does not persist a console-owned block" guard is folded into the reworded
  DERIVE/RETARGET clauses rather than added as a separate normative line, so it
  adds no counted clause.
- **spec.md: 15 -> 15 (0).** No spec.md `MUST`/`SHOULD` clause is dropped; the
  re-cast sentence carries no rule keyword.
- **constraints.md: 19 -> 19 (0).** No constraints.md clause is changed.

Net: one clause removed (contracts.md), decrement of exactly 1. If the driver's
final reflow of the RETARGET clause splits the positive write-through-surface
`MUST` and its negative guard onto two `MUST`-bearing physical lines, the driver
computes the resulting count and gap-id set from the ratified text accordingly;
the EXPECTED delta assuming the retarget stays one `MUST`-bearing clause is the
-1 above.
