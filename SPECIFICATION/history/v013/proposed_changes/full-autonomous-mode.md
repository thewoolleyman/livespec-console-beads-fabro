---
topic: full-autonomous-mode
author: claude-opus-4-8
created_at: 2026-07-02T23:50:08Z
spec_commitments:
  impl_followups:
    - id_hint: console-full-autonomous-mode
      description: |
        Implement the console's full-autonomous-mode surface in the Rust console: the per-repo .livespec.jsonc autonomous_mode setting read by the Configuration context (default disabled), the config.autonomous_mode_set command with its confirmed-guard rejection, the config.autonomous_mode.enabled/disabled audit events, the factory.autonomous_mode_enable/disable_requested commands issued to the orchestrator through its published command surface (with honest not-wired outcomes when the port is simulated), the auto-resolution path that records every auto-decision as a command plus outcome events while escalating truly-unresolvable decisions to Attention, and the TUI dangerous-labelled type-to-confirm toggle plus header mode indicator — each behavior linked clause->scenario->test.
---

## Proposal: Full autonomous mode (dangerous, per-repo)

### Target specification files

- SPECIFICATION/spec.md
- SPECIFICATION/contracts.md
- SPECIFICATION/constraints.md
- SPECIFICATION/scenarios.md

### Summary

Add full autonomous mode to the console as an operator-facing, per-repo, default-OFF capability: when enabled for a repo the console resolves — via an LLM standing in for the operator — the operator decisions it would otherwise prompt for, issues the resulting commands through each plane's published command surface (including a command that enables the orchestrator plane's own autonomous mode), and escalates only truly-unresolvable decisions to the Attention inbox. The mode is dangerous: it MUST default off, require explicit type-to-confirm, be labelled 'dangerous / use with caution' in every UI, persist per-repo in .livespec.jsonc, and emit a durable audit event on enable. Consistent with the Control-Plane boundary, the console never owns any plane's decision semantics — it only enables and reflects them.

### Motivation

Operator request: a 'full autonomous mode' toggle so an LLM handles all possible human decisions, blocking only on decisions that are truly unresolvable by the LLM; the option must be labelled dangerous / use-with-caution in both the orchestrator API and the console TUI/GUI, and on the console side may be persisted as a .livespec.jsonc setting. SCOPE NOTE: this proposal is filed against the CONSOLE repo (livespec-console-beads-fabro), so it specifies only the console's surface for autonomous mode (config persistence, per-repo default-off toggle, danger guard, audit + command trail, and the command that enables the orchestrator's autonomous mode through its published command surface). The orchestrator-API side — the engine that autonomously makes the orchestrator's own human decisions — is a realization mechanism of the orchestrator plane and, per the propose-change cross-repo placement discipline, belongs in the livespec-orchestrator-beads-fabro repo's SPECIFICATION via a separate /livespec:propose-change run. That out-of-target work is surfaced here rather than mis-filed into the console spec.

### Proposed Changes

This proposal adds **full autonomous mode** to the console as an
operator-facing, per-repo, default-OFF capability. It touches four
operator-facing files; the clause, contract, constraint, and scenario
edits MUST land atomically (a behavior clause with no scenario is
malformed per the console's Behavioral Coverage discipline).

The console is the Control Plane and "never owns any plane's
semantics" (`spec.md` §"Scope Boundary"). Accordingly, this proposal
specifies only the console's **surface** for autonomous mode — the
operator toggle, the danger guard, per-repo persistence, the
audit/command trail, and the command the console issues to *enable* the
orchestrator plane's own autonomous mode through that plane's published
command surface. The engine that actually resolves the orchestrator's
human decisions autonomously is owned by the orchestrator plane
(`livespec-orchestrator-beads-fabro`) and MUST be specified in that
repo, not here.

---

### `SPECIFICATION/spec.md`

**(a) §"Bounded Contexts" — Configuration bullet.** Extend the
Configuration context's owned concerns to include autonomous-mode
policy:

> - **Configuration** -- registered repos, source endpoints,
>   notification policy, adapter enablement, and **autonomous-mode
>   policy**.

**(b) Add a new top-level section `## Full Autonomous Mode`** with the
following normative content:

- Full autonomous mode is an operator-facing, **per-repo** mode that
  MUST default to **disabled**. When enabled for a repo, the console
  MUST resolve — via an LLM standing in for the operator — the operator
  decisions it would otherwise prompt a human for, and MUST issue the
  resulting commands through each plane's own published command surface.
- The console MUST NOT own or re-implement any plane's decision
  semantics. Where a plane owns a decision (for example a Fabro human
  gate), the console MUST enable that plane's own autonomous resolution
  by issuing a command on that plane's published command surface, and
  MUST NOT reach around the plane to fabricate the decision itself.
  This preserves the Scope-Boundary rule that the console issues
  commands only through published command surfaces.
- A decision is **truly unresolvable** when the LLM cannot resolve it
  with sufficient confidence, when it requires information the console
  cannot obtain, or when a policy marks it human-only. Truly
  unresolvable decisions MUST continue to appear as Attention items,
  each carrying its source reference and next operator action, exactly
  as they do outside autonomous mode. The console MUST NOT silently
  drop, indefinitely defer, or fabricate a decision it cannot resolve.
- Every decision the console auto-resolves MUST be recorded through the
  same command-plus-outcome-event path as an operator-issued command,
  so the audit trail is identical whether a human or the autonomous LLM
  made the call.
- Full autonomous mode is **dangerous**. It MUST default off, MUST
  require explicit operator confirmation to enable, MUST be labelled
  "dangerous / use with caution" wherever it is presented in any UI
  surface (TUI and future GUI/API), and MUST be revocable at any time;
  disabling it MUST return every decidable item to human routing.
- **Cross-plane note (non-normative).** The engine that autonomously
  makes the *orchestrator's* own human decisions is owned by the
  orchestrator plane (`livespec-orchestrator-beads-fabro`) and is
  specified in that repo. The console only enables, observes, and
  reflects it; a companion `/livespec:propose-change` SHOULD be filed
  in the orchestrator repo for that engine.

---

### `SPECIFICATION/contracts.md`

**(a) §"Command Handling" — Initial commands.** Add:

- `config.autonomous_mode_set` (context `configuration`) with payload
  `{ "repo": "<repo-id>", "enabled": <bool>, "confirmed": <bool> }`.
  The handler MUST reject the command when `enabled` is `true` and
  `confirmed` is not `true` (guarding against an accidental enable).
  On acceptance the handler MUST persist the per-repo setting (below)
  and MUST append the corresponding audit event.
- `factory.autonomous_mode_enable_requested` and
  `factory.autonomous_mode_disable_requested` (context `factory`) —
  the commands the console issues to the orchestrator plane, through
  that plane's published command surface, to turn the orchestrator's
  own autonomous mode on or off. These MUST obey the existing honesty
  rule: a simulated or unimplemented orchestrator port MUST surface a
  not-wired / not-observed outcome (for example
  `factory.autonomous_mode.not_wired`) and MUST NOT fabricate success.

**(b) New canonical events** (context `configuration`):
`config.autonomous_mode.enabled` and `config.autonomous_mode.disabled`,
each carrying the target `repo`, the requesting actor, and
`occurred_at`. These are the durable audit facts for the mode change.

**(c) `.livespec.jsonc` per-repo setting.** The console's autonomous-mode
preference MUST be persisted per-repo under the console's namespaced
block in that repo's `.livespec.jsonc`:

```jsonc
"livespec-console-beads-fabro": {
  "autonomous_mode": { "enabled": false }
}
```

The console's Configuration context MUST read this setting for each
registered repo; an absent block or key MUST be treated as disabled.
Persisting an enable MUST both write `"enabled": true` back to that
repo's `.livespec.jsonc` **and** emit the `config.autonomous_mode.enabled`
event, so file state and event log never disagree.

**(d) §"TUI Contract".** Add an autonomous-mode affordance. The toggle
MUST render the "dangerous / use with caution" label. Enabling MUST
require an explicit type-to-confirm modal before the console submits a
`config.autonomous_mode_set` command carrying `confirmed: true`;
disabling MUST NOT require confirmation. The header mode indicator
(already "fleet, mode, ingestion, Fabro summary") MUST reflect whether
autonomous mode is active for the selected repo.

---

### `SPECIFICATION/constraints.md`

Add operator-observable safety constraints (new section
`## Autonomous-Mode Safety`):

- Autonomous mode MUST default to disabled for every repo; a repo with
  no autonomous-mode setting MUST be treated as disabled.
- Enabling autonomous mode MUST require explicit operator confirmation
  AND MUST emit a durable `config.autonomous_mode.enabled` audit event;
  it MUST NOT be enabled without both.
- In autonomous mode the console MUST still surface every truly
  unresolvable decision as an Attention item, and MUST NOT drop,
  silently defer, or fabricate a decision it cannot resolve.
- Every auto-resolved decision MUST be recorded through the same
  command-plus-outcome-event path as an operator-issued command; no
  side effect MUST occur without an auditable command and outcome.
- The console MUST NOT reach around a plane to force a decision that
  plane owns; it MUST enable the plane's own autonomous mode through
  that plane's published command surface.

---

### `SPECIFICATION/scenarios.md`

Add two operator scenarios. The new normative clauses above MUST be
linked to these scenario H2s through `tests/heading-coverage.json` per
`non-functional-requirements.md` §"Behavioral Coverage".

```gherkin
Feature: Guarded, audited full autonomous mode
  As a LiveSpec operator
  I want autonomous mode to be off by default, confirmed, and audited
  So that a dangerous mode can never be enabled by accident or silently

  Scenario: Enabling autonomous mode is confirmed, persisted, and audited
    Given a registered repo whose autonomous mode is disabled by default
    When the operator enables autonomous mode from the TUI
    Then the TUI shows a "dangerous / use with caution" label
    And requires an explicit type-to-confirm modal
    And the console submits config.autonomous_mode_set with confirmed true
    And persists "enabled": true to the repo's .livespec.jsonc
    And appends a config.autonomous_mode.enabled audit event
    And issues factory.autonomous_mode_enable_requested to the orchestrator
      through its published command surface

  Scenario: An unconfirmed enable is rejected with no effect
    Given a registered repo whose autonomous mode is disabled
    When a config.autonomous_mode_set with enabled true arrives without confirmed true
    Then the Configuration context rejects the command
    And no setting is written and no audit event is appended
```

```gherkin
Feature: Autonomous mode resolves the decidable and escalates the rest
  As an operator running a repo in autonomous mode
  I want the console to auto-resolve decisions it can and escalate the rest
  So that only truly unresolvable decisions still need me

  Scenario: A decidable attention item is auto-resolved and recorded
    Given a repo in autonomous mode
    And an attention item derived from a decision the LLM can resolve
    When the console runs autonomously
    Then it records the auto-decision as a command and its outcome events
    And the item leaves the Attention inbox

  Scenario: A truly unresolvable decision still reaches the operator
    Given a repo in autonomous mode
    And a decision the LLM cannot resolve with sufficient confidence
    When the console runs autonomously
    Then the decision remains an Attention item with its source reference
      and next operator action
    And the console neither drops nor fabricates the decision
```
