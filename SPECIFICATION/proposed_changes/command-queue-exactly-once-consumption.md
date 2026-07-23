---
topic: command-queue-exactly-once-consumption
author: claude-opus-4-8
created_at: 2026-07-23T02:31:57Z
---

## Proposal: Exactly-once command consumption via an atomic executing claim

### Target specification files

- SPECIFICATION/contracts.md
- SPECIFICATION/scenarios.md

### Summary

Add a normative single-consumer subsection to contracts.md §"Command Handling": every pending command MUST be executed by exactly one consumer via an atomic pending -> executing claim, finalized to a terminal status, with stale-claim recovery so a crashed consumer strands nothing. Extend the section's flowchart with the claim race and the recovery loop, and add scenarios.md Scenario 24 covering the two-console race and stale-claim recovery, so the new semantics enter the clause -> scenario -> test coverage chain.

### Motivation

Work-item livespec-console-beads-fabro-ipwtll fixes a verified defect: the pending-command handlers carry no claim or lease semantics, so every console client executes every pending command — two consoles open against one store double-execute valves, drains, and policy edits. contracts.md §"Command Handling" shows a one-handler sequence but never states consumption cardinality and has no executing status, and its flowchart traces one command through one handler, silent on how many consumers exist. Because non-functional-requirements.md §"Behavioral Coverage" chains forward from clauses (clause -> scenario -> test), semantics that are never written as a normative clause are guarded by nothing — the implementation would ship a new behavior and a new executing status that the mechanical coverage gate structurally cannot see. The maintainer ruled on 2026-07-23 that the semantics get a contract amendment riding with the implementation, rather than deeming the existing sequence diagram to imply them.

### Proposed Changes

**1. `SPECIFICATION/contracts.md` §"Command Handling" — add a "Single-consumer consumption" subsection** (after the initial-commands mapping prose, immediately before the `flowchart LR` diagram):

> The persisted command queue is a SINGLE-CONSUMER queue: every pending command MUST be executed by exactly one console client — that command's CONSUMER — no matter how many clients open the same store. Before running a command's handler, the consumer MUST atomically claim the command by transitioning its status `pending` -> `executing`; a client whose claim takes no effect — another consumer already owns the row — MUST NOT execute that command. On completion the owning consumer MUST finalize the claimed command from `executing` to a terminal status (`completed`, `failed`, `rejected`, or `not_wired` — the honesty rule's not-wired terminal outcome remains a legal finalization of a claimed command).
>
> A command stranded at `executing` by a consumer that crashed before finalizing MUST NOT stay stranded forever: once the claim is recognizably stale, the command MUST be re-offered for consumption (or driven to a terminal status by reconciliation). Staleness recognition MUST be conservative — a claim MUST NOT be treated as stale while its owning consumer is still executing the command, so recovery never steals a live claim and re-introduces the double-execution this section forbids; the concrete staleness mechanism (lease expiry, heartbeat, liveness probe) is the implementation's choice, the no-live-steal invariant is not. Stale-claim recovery governs the CONSUMPTION gap — claim taken, no finalization; handler rule 5's reconciliation/backfill continues to govern the SIDE-EFFECT gap — external side effect performed, outcome append missing. The two recovery paths compose; neither replaces the other.

**2. Same section — extend the `flowchart LR`** so the diagram shows the claim ahead of policy validation and the stale-claim recovery loop:

```diff
 flowchart LR
   Requested["command requested"]
+  Claim["atomic claim: pending -> executing"]
+  Lost["another consumer owns it -- do nothing"]
+  Stale["stale executing claim (crashed consumer)"]
   Policy["context policy validation"]
   Rejected["command rejected event"]
   Accepted["command accepted event"]
   SideEffect["invoke port"]
   Succeeded["success event"]
   Failed["failure event"]
   Reconcile["reconciliation observes external result"]
 
-  Requested --> Policy
+  Requested --> Claim
+  Claim -->|"won"| Policy
+  Claim -->|"lost"| Lost
+  Claim -.->|"crash before finalize"| Stale
+  Stale -.->|"recovery re-offers"| Requested
   Policy -->|"invalid"| Rejected
   Policy -->|"valid"| Accepted
   Accepted --> SideEffect
   SideEffect -->|"ok"| Succeeded
   SideEffect -->|"expected failure"| Failed
   SideEffect -->|"crash gap"| Reconcile --> Succeeded
```

**3. `SPECIFICATION/scenarios.md` — append `## Scenario 24 -- Two consoles consume one command queue exactly once`**, following the house scenario shape (H2, then a gherkin block):

```gherkin
Feature: Exactly-once command consumption
  As an operator
  I want each enqueued command to execute exactly once no matter how many consoles I have open
  So that a second console never double-fires valves, drains, or policy edits

Scenario: Two consoles race for one pending command; exactly one executes it
  Given one console event store holding a pending command
  And two console clients consuming pending commands against that store
  When both clients run a consumption pass over the same pending command
  Then exactly one client wins the atomic claim, moving the command from pending to executing
  And only the winning client executes the handler and appends outcome events
  And the winning client finalizes the command from executing to a terminal status
  And the losing client executes nothing and appends nothing for that command

Scenario: A crashed consumer's executing claim is recovered
  Given a command left at executing by a consumer that crashed before finalizing
  When stale-claim recovery recognizes the claim as stale
  Then the command is re-offered for consumption or driven to a terminal status by reconciliation
  And no command stays stranded at executing forever
```

**4. Coverage chain (rides with the implementation).** Per `non-functional-requirements.md` §"Behavioral Coverage", the accepting revision MUST land atomically with (a) `tests/heading-coverage.json` entries linking each new clause's derived gap-id to Scenario 24 and (b) the top-of-pyramid test delivered by work-item `livespec-console-beads-fabro-ipwtll` (the single-consumer claim implementation). `tests/heading-coverage.json` sits outside the spec target, so it is named here rather than in the target-files list. The revise pass MUST NOT accept this proposal into a revision that ships without those registry entries and that test, or the behavioral-coverage gate breaks.
