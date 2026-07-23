---
topic: out-of-band-edit-2026-07-23t04-43-13z
author: livespec-doctor
created_at: 2026-07-23T04:43:13Z
---

## Proposal: out-of-band-edit-2026-07-23t04-43-13z

doctor detected drift between HEAD-active spec content and the
HEAD-history-vN snapshot; this auto-backfill records the active
state as the new canonical version.

### Proposed Changes

```diff
--- history/vN/contracts.md
+++ active/contracts.md
@@ -462,9 +462,29 @@
 16): there is no local-dismiss command; "not now" is defer/re-rank via the
 orchestrator.
 
+The persisted command queue is a SINGLE-CONSUMER queue: every pending command
+MUST be executed by exactly one console client -- that command's CONSUMER -- no
+matter how many clients open the same store. Before running a command's handler,
+the consumer MUST atomically claim the command by transitioning its status
+`pending` -> `executing`; a client whose claim takes no effect -- another
+consumer already owns the row -- MUST NOT execute that command. On completion
+the owning consumer MUST finalize the claimed command from `executing` to a
+terminal status (`completed`, `failed`, `rejected`, or `not_wired` -- the
+honesty rule's not-wired terminal outcome remains a legal finalization of a
+claimed command). A command stranded at `executing` by a consumer that crashed
+before finalizing MUST NOT stay stranded forever: once the claim is
+recognizably stale, the command MUST be re-offered for consumption or driven to
+a terminal status by reconciliation. Staleness recognition MUST be
+conservative -- a claim MUST NOT be treated as stale while its owning consumer
+is still executing the command, so recovery never steals a live claim and
+re-introduces the double-execution this section forbids.
+
 ```mermaid
 flowchart LR
   Requested["command requested"]
+  Claim["atomic claim: pending -> executing"]
+  Lost["another consumer owns it -- do nothing"]
+  Stale["stale executing claim (crashed consumer)"]
   Policy["context policy validation"]
   Rejected["command rejected event"]
   Accepted["command accepted event"]
@@ -473,7 +493,11 @@
   Failed["failure event"]
   Reconcile["reconciliation observes external result"]
 
-  Requested --> Policy
+  Requested --> Claim
+  Claim -->|"won"| Policy
+  Claim -->|"lost"| Lost
+  Claim -.->|"crash before finalize"| Stale
+  Stale -.->|"recovery"| Requested
   Policy -->|"invalid"| Rejected
   Policy -->|"valid"| Accepted
   Accepted --> SideEffect
--- history/vN/scenarios.md
+++ active/scenarios.md
@@ -939,3 +939,27 @@
   Then no per-item key is advertised
   And the same holds inside a drilled-in lane that is empty
 ```
+
+## Scenario 24 -- Two consoles consume one command queue exactly once
+
+```gherkin
+Feature: Exactly-once command consumption
+  As an operator
+  I want each enqueued command to execute exactly once no matter how many consoles I have open
+  So that a second console never double-fires valves, drains, or policy edits
+
+Scenario: Two consoles race for one pending command; exactly one executes it
+  Given one console event store holding a pending command
+  And two console clients consuming pending commands against that store
+  When both clients run a consumption pass over the same pending command
+  Then exactly one client wins the atomic claim, moving the command from pending to executing
+  And only the winning client executes the handler and appends outcome events
+  And the winning client finalizes the command from executing to a terminal status
+  And the losing client executes nothing and appends nothing for that command
+
+Scenario: A crashed consumer's executing claim is recovered
+  Given a command left at executing by a consumer that crashed before finalizing
+  When stale-claim recovery recognizes the claim as stale
+  Then the command is re-offered for consumption or driven to a terminal status by reconciliation
+  And no command stays stranded at executing forever
+```
```
