---
topic: console-verb-presentation
author: claude-fable-5
created_at: 2026-07-26T17:55:33Z
---

## Proposal: Per-item verb suppression consumes the ratified per-state vocabulary

### Target specification files

- SPECIFICATION/contracts.md
- SPECIFICATION/scenarios.md

### Summary

Upgrade the deferred per-item verb-suppression clause from not-required to REQUIRED, consuming the vocabulary ratified in livespec-orchestrator-beads-fabro SPECIFICATION/contracts.md section 'Per-state operator verb vocabulary' (its v050).

### Motivation

The console's Status-line hint contract explicitly deferred finer-grained per-item suppression because the per-state valid-verb vocabulary was 'owned by livespec-orchestrator-beads-fabro and not yet consumed here'. That vocabulary is now ratified (that repo's history/v050, merged 2026-07-26). The deferral clause's precondition is discharged, and the operator-surface-redesign brainstorm (all points maintainer-decided 2026-07-21..26) directed exactly this consumption. Today p/c/r and the policy dials fire on any selected item regardless of lane, failing at the orchestrator - and per the silent-valve-failure defect, failing invisibly.

### Proposed Changes

In the TUI Status-line hint clause, replace the sentence deferring finer-grained per-item suppression with: per-item verb hints MUST be suppressed, and the corresponding keys MUST be inert, when the selected work-item's lifecycle state does not admit the verb per the per-state operator verb vocabulary owned by livespec-orchestrator-beads-fabro (SPECIFICATION/contracts.md section 'Per-state operator verb vocabulary'). The console MUST consume that vocabulary as data it does not re-derive: approve only at pending-approval; accept and reject only at the two human-valve lanes (reject at pending-approval and acceptance); the policy dials only within their upstream windows (set-admission through pending-approval; merge-on-review-cap and review-fix-cap through ready; set-acceptance and acceptance-rework-cap through active; none on done). The suppression rule MUST NOT assume active has a single entry door: the ratified rule admits journaled dispatch AND the rework returns from acceptance (the reject:rework valve and the Dispatcher's acceptance-auto-rework disposition), so an acceptance-lane item MUST keep its reject hint. A hint that names a verb the vocabulary forbids for the selected item is a contract violation of the existing hint-honesty clause. A new Scenario in scenarios.md MUST cover: Given a done item is selected, When the operator views the Status line, Then no valve, move, or dial hint is offered and the corresponding keys are inert; and Given a backlog item is selected, Then approve and accept hints are absent while groom and move hints are present.

## Proposal: Move-table narrowing to the ratified doors

### Target specification files

- SPECIFICATION/contracts.md
- SPECIFICATION/scenarios.md

### Summary

Narrow the s move-to-status targets to the ratified door rules: remove pending-approval to ready/active, remove acceptance to ready/active/done, and remove active as a move target from every lane.

### Motivation

The ratified vocabulary makes each lifecycle transition the property of exactly one journaled owner: approve is the only door from pending-approval toward ready; accept is the only door into done; active is entered only by journaled dispatch or a journaled rework return from acceptance. The console's current move table (status_move_targets) still offers pending-approval to ready (an unjournaled duplicate of the approve valve), acceptance to done (an unjournaled duplicate of the accept valve, which docs/lifecycle-walkthrough.md's ship-guard prose already contradicts today), and bare moves into active from four lanes. These duplicates were exercised in production: the 2026-07-21 real-stack walk admitted an item through the unjournaled path and the valve journal for it does not exist.

### Proposed Changes

The move valve's offered targets MUST become: backlog to ready or blocked; pending-approval to backlog or blocked; ready to backlog or blocked; active offers no operator move targets; acceptance to backlog or blocked; blocked to backlog or ready; done offers nothing. The move valve MUST NOT offer active as a target from any lane, MUST NOT offer ready from pending-approval, and MUST NOT offer done from acceptance. The ship-guard description in the operator documentation MUST match this table exactly (the docs move-table lockstep gate covers the structural claim). The existing s-valve Scenario MUST be updated to the narrowed table, and a new Scenario MUST cover: Given an item at acceptance is selected in a drilled lane, When the operator opens the move valve, Then the offered targets are exactly backlog and blocked, and done is not offered.

## Proposal: Driver handoff presentation: one lane-appropriate verb, render plus copy, no execution

### Target specification files

- SPECIFICATION/contracts.md
- SPECIFICATION/scenarios.md

### Summary

Introduce the driver-handoff surface: a single lane-appropriate verb that renders a copy-paste-safe driver invocation (groom on backlog items; driver-implement on factory-unsafe ready items), with OSC 52 copy under a normative no-claimed-success wording constraint, and no execution of the driver by the console.

### Motivation

The happy path's first leg (groom via LLM-driver handoff) has no transport: no verb exposes grooming, no copy mechanism exists (the CopyFabroAttach scaffold is dead code), and heavyweight LLM-driven verbs have no route to a driver session. The transport design (plan/operator-surface-redesign/research/l4p3ce-handoff-transport.md) is maintainer-decided 2026-07-26 on all four open questions, and the orchestrator-side driver-dispatch surface is ratified with its narrow scope recorded as load-bearing.

### Proposed Changes

The TUI MUST offer a driver-handoff verb whose meaning is lane-appropriate per the ratified vocabulary: on a backlog item it MUST render the groom invocation for the item id; on a ready item whose factory_safety is non-null (exactly the dispatch-admission host-only-refused set) it MUST render the driver-implement invocation for the item id; in every other state the verb MUST be suppressed. The rendered command MUST be the existing orchestrator operation invocation carrying only the work-item id (no tmp-file prompt leg is built; the tmp-file slot remains reserved and deliberately unbuilt per the design record, which names the CopyFabroAttach scaffold-read-as-feature failure as the reason). The command MUST render in a full-width overlay that preserves copyability, and the console MAY additionally emit the command via OSC 52. The overlay MUST NOT claim the copy succeeded: OSC 52 is fire-and-forget and success is unknowable to the console, so permitted wording describes what the console did (for example 'copy sent to terminal') and wording that asserts an outcome the console cannot observe (for example 'Copied!') is forbidden. The console MUST NOT execute, spawn, monitor, or await the driver session (the no-console-to-driver-dependency contract); the driver session journals its own dispatch through the orchestrator's driver-dispatch surface, and widening driver-implement beyond the host-only-refused set MUST NOT be done without the claim mechanism the orchestrator vocabulary names as its precondition. The specific key binding remains an implementation detail; one key serving both lane-appropriate meanings is the reference shape. Adopter precondition to record beside the contract clause: the rendered invocation resolves only in a driver session whose project has the orchestrator plugin installed (project-scoped only, verified 2026-07-26); a console adopter without that plugin gets a non-resolving handoff the console cannot detect by contract. New Scenarios MUST cover: Given a backlog item is selected, When the operator invokes the driver handoff, Then a full-width overlay renders the groom invocation carrying the item id and the console spawns nothing; Given a ready item with non-null factory_safety is selected, Then the overlay renders the driver-implement invocation; Given a ready item with null factory_safety is selected, Then the driver-handoff verb is suppressed.
