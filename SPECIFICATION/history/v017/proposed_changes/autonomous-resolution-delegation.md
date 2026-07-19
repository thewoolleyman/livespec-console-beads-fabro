---
topic: autonomous-resolution-delegation
author: claude-opus-4-8
created_at: 2026-07-10T12:24:00Z
---

## Proposal: Re-scope autonomous mode to the delegation model (the owning plane's engine resolves; the console enables + observes + reflects)

### Target specification files

- SPECIFICATION/spec.md
- SPECIFICATION/scenarios.md
- SPECIFICATION/constraints.md

### Summary

Resolve the division-of-resolution seam (`livespec/plan/autonomous-mode/design.md`
§6.2; console plan §4) by re-scoping full autonomous mode to the DELEGATION
model: the ORCHESTRATOR (owning) plane's engine owns ALL gate resolution; the
console's autonomous responsibility is **enable + observe + reflect +
surface-unresolvable**. The console does NOT run its own gate-resolver in this
MVP -- its LLM-stand-in resolution layer is deferred -- which single-sources
resolution to the owning plane and KILLS the double-resolution race explicitly.
Three sites are re-cast: the §"Full Autonomous Mode" blanket resolve-MUST
(spec.md), Scenario 10 (scenarios.md), and the §"Autonomous-Mode Safety" audit
constraint (constraints.md, drift-sweep so no unamended MUST contradicts the
re-scope).

### Motivation

The console spec today CONTRADICTS itself on who resolves a gate. §"Full
Autonomous Mode" opens by requiring the console itself to resolve, via its own
LLM, the operator decisions and issue the resulting commands -- but the same
section closes by saying "the console only enables, observes, and reflects" the
orchestrator's engine, and §"Scope Boundary" forbids the console from owning any
plane's decision semantics. The Step-0 Fable validation (2026-07-10) found this
is not merely wording: Scenario 10's first scene has the CONSOLE "record the
auto-decision as a command", which is UNSATISFIABLE for orchestrator-owned gates
when the orchestrator's engine resolves them upstream and audits them in the
orchestrator journal -- the console has nothing to record as its own command.

The recommended MVP reading (design §6.2) resolves the contradiction toward the
plane boundary: the orchestrator engine (orchestrator item `bd-ib-82a`) owns
gate resolution; the console enables that engine through the plane's published
command surface and reflects each audited auto-resolution. Keeping a
console-side resolver instead would RACE the engine on items resting between
engine runs (double-resolution). This re-scope removes the console-side resolver
for the MVP (deferring that layer), so resolution is single-sourced and the race
cannot occur.

Design record (read in full): repo `thewoolleyman/livespec`,
`plan/autonomous-mode/design.md` §6.2 (division of resolution / avoid
double-resolution) and §3 (the plane boundary); the console plan
`plan/console-autonomous-mode/design.md` §4 (the resolution-division bullet) and §5
(what is NOT console work -- the decision engine is orchestrator item
`bd-ib-82a`).

SCOPE NOTE: this proposal does NOT touch the persistence seam (design §6.1) or
the `factory.autonomous_mode_*`/`config.autonomous_mode_set` arming/persistence
commands -- that amendment gates on the orchestrator's frozen arming contract
(I1) and is deferred. The one plane-arming command referenced below is described
by role only; its name, shape, and persistence target are unchanged here.

No new implementation follow-up beyond the plan's existing C3 step, which builds
the console autonomous feature to this delegated model (enable + observe +
reflect, no console-side resolver).

### Proposed Changes

All quoted current text is verbatim from the live console spec files (head
v016). This is one atomic proposed change (one `## Proposal:` section, one
per-file revise decision under this topic). It changes no `## ` heading, so it
requires no `tests/heading-coverage.json` co-edit.

#### spec.md

`[RE-SCOPE]` **§"Full Autonomous Mode"**, the opening blanket resolve-MUST
paragraph -- replace:

> Full autonomous mode is an operator-facing, per-repo mode that MUST
> default to disabled. When enabled for a repo, the console MUST resolve --
> via an LLM standing in for the operator -- the operator decisions it
> would otherwise prompt a human for, and MUST issue the resulting commands
> through each plane's own published command surface. Only decisions that
> are truly unresolvable by the LLM remain human needs-attention items.

with:

> Full autonomous mode is an operator-facing, per-repo mode that MUST
> default to disabled. When enabled for a repo, the console MUST enable --
> through each owning plane's own published command surface -- that plane's
> autonomous resolution of the operator decisions it would otherwise prompt
> a human for, and then observe, record, and reflect every auto-resolution
> that plane makes. Gate resolution is delegated wholesale to the owning
> plane's engine and is single-sourced there: the console itself MUST NOT
> resolve a decision a plane owns, and its own LLM-stand-in resolution layer
> is deferred for this MVP -- no console-side resolver runs, so no gate is
> ever resolved twice and the console never races a plane's engine on items
> resting between its runs. Only decisions that no plane's engine resolves --
> the truly unresolvable residual -- remain human needs-attention items.

`[DRIFT-SWEEP]` **§"Full Autonomous Mode"**, the truly-unresolvable / audit
paragraph -- replace:

> A decision is **truly unresolvable** when the LLM cannot resolve it with
> sufficient confidence, when it requires information the console cannot
> obtain, or when a policy marks it human-only. Truly unresolvable
> decisions MUST continue to appear as needs-attention items, each carrying its
> source reference and next operator action, exactly as they do outside
> autonomous mode. The console MUST NOT silently drop, indefinitely defer,
> or fabricate a decision it cannot resolve. Every decision the console
> auto-resolves MUST be recorded through the same command-plus-outcome-event
> path as an operator-issued command, so the audit trail is identical
> whether a human or the autonomous LLM made the call.

with:

> A decision is **truly unresolvable** when the owning plane's engine cannot
> resolve it with sufficient confidence, when it requires information that
> engine cannot obtain, or when a policy marks it human-only. Truly
> unresolvable decisions MUST continue to appear as needs-attention items,
> each carrying its source reference and next operator action, exactly as
> they do outside autonomous mode. The console MUST NOT silently drop,
> indefinitely defer, or fabricate a decision no plane resolved. Every
> command the console itself issues -- the plane-arming command and the
> human-valve commands -- MUST be recorded through the same
> command-plus-outcome-event path as an operator-issued command; and every
> auto-resolution an owning plane's engine makes, audited in that plane's
> own journal, the console MUST observe and reflect through that same path --
> so the audit trail is complete whether a human, the console, or a plane's
> engine made the call.

#### scenarios.md

`[RE-SCOPE]` **§"Scenario 10 -- Autonomous mode resolves the decidable and
escalates the rest"**, the whole mermaid + gherkin body -- replace:

> ```mermaid
> flowchart LR
>   Mode["Repo in autonomous mode"]
>   Decidable["LLM-resolvable decision"]
>   AutoCmd["Auto-issued command + outcome events"]
>   Leave["Item leaves needs-attention inbox"]
>   Unresolvable["Truly unresolvable decision"]
>   NeedsAttention["Stays in needs-attention with source ref + next action"]
>
>   Mode --> Decidable --> AutoCmd --> Leave
>   Mode --> Unresolvable --> NeedsAttention
> ```
>
> ```gherkin
> Feature: Autonomous mode resolves the decidable and escalates the rest
>   As an operator running a repo in autonomous mode
>   I want the console to auto-resolve decisions it can and escalate the rest
>   So that only truly unresolvable decisions still need me
>
> Scenario: A decidable needs-attention item is auto-resolved and recorded
>   Given a repo in autonomous mode
>   And a needs-attention item derived from a decision the LLM can resolve
>   When the console runs autonomously
>   Then it records the auto-decision as a command and its outcome events
>   And the item leaves the needs-attention inbox
>
> Scenario: A truly unresolvable decision still reaches the operator
>   Given a repo in autonomous mode
>   And a decision the LLM cannot resolve with sufficient confidence
>   When the console runs autonomously
>   Then the decision remains a needs-attention item with its source reference and next operator action
>   And the console neither drops nor fabricates the decision
> ```

with:

> ```mermaid
> flowchart LR
>   Mode["Repo in autonomous mode"]
>   Enable["Console enables the owning plane's autonomous resolution"]
>   Decidable["Decision the plane's engine can resolve"]
>   Reflect["Plane engine resolves + audits in its journal; console observes + reflects"]
>   Leave["Item leaves needs-attention inbox"]
>   Unresolvable["Truly unresolvable decision"]
>   NeedsAttention["Stays in needs-attention with source ref + next action"]
>
>   Mode --> Enable --> Decidable --> Reflect --> Leave
>   Mode --> Unresolvable --> NeedsAttention
> ```
>
> ```gherkin
> Feature: Autonomous mode resolves the decidable and escalates the rest
>   As an operator running a repo in autonomous mode
>   I want the owning plane's engine to auto-resolve what it can while the console reflects it, and escalate the rest
>   So that only truly unresolvable decisions still need me
>
> Scenario: A decidable item is resolved by the owning plane's engine and reflected
>   Given a repo in autonomous mode
>   And a needs-attention item derived from a decision the owning plane's engine can resolve
>   When the console runs autonomously
>   Then the console has enabled that plane's autonomous resolution through the plane's published command surface
>   And the plane's engine resolves the decision and audits it in the plane's own journal
>   And the console observes and reflects that auto-resolution through its own event path
>   And the item leaves the needs-attention inbox
>
> Scenario: A truly unresolvable decision still reaches the operator
>   Given a repo in autonomous mode
>   And a decision no plane's engine can resolve with sufficient confidence
>   When the console runs autonomously
>   Then the decision remains a needs-attention item with its source reference and next operator action
>   And the console neither drops nor fabricates the decision
> ```

#### constraints.md

`[DRIFT-SWEEP]` **§"Autonomous-Mode Safety"**, the surface-unresolvable and
auto-resolved-audit bullets -- replace:

> - In autonomous mode the console MUST still surface every truly
>   unresolvable decision as a needs-attention item, and MUST NOT drop, silently
>   defer, or fabricate a decision it cannot resolve.
> - Every auto-resolved decision MUST be recorded through the same
>   command-plus-outcome-event path as an operator-issued command; no side
>   effect MUST occur without an auditable command and outcome.

with:

> - In autonomous mode the console MUST still surface every truly
>   unresolvable decision as a needs-attention item, and MUST NOT drop, silently
>   defer, or fabricate a decision no plane's engine resolved.
> - Every command the console itself issues in autonomous mode -- the
>   plane-arming command and the human-valve commands -- MUST be recorded
>   through the same command-plus-outcome-event path as an operator-issued
>   command, and every auto-resolution an owning plane's engine makes MUST be
>   observed and reflected through that same path; no console side effect MUST
>   occur without an auditable command and outcome.
