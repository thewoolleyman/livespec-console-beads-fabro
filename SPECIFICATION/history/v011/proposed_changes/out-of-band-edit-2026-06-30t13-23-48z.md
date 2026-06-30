---
topic: out-of-band-edit-2026-06-30t13-23-48z
author: livespec-doctor
created_at: 2026-06-30T13:23:48Z
---

## Proposal: out-of-band-edit-2026-06-30t13-23-48z

doctor detected drift between HEAD-active spec content and the
HEAD-history-vN snapshot; this auto-backfill records the active
state as the new canonical version.

### Proposed Changes

```diff
--- history/vN/contracts.md
+++ active/contracts.md
@@ -346,8 +346,6 @@
 - `factory.pause_requested`
 - `factory.resume_requested`
 - `spec.doctor_requested`
-- `attention.acknowledge_requested`
-- `attention.snooze_requested`
 - `grooming.regroom_requested`
 
 ```mermaid
--- history/vN/scenarios.md
+++ active/scenarios.md
@@ -135,13 +135,9 @@
   List["Attention list"]
   Select["Arrow selection"]
   Detail["Detail pane"]
-  Actions["Action list"]
-  Ack["Acknowledge / snooze"]
-  Fabro["Open or copy Fabro attach command"]
-
-  List --> Select --> Detail --> Actions
-  Actions --> Ack
-  Actions --> Fabro
+  Timeline["Latest timeline"]
+
+  List --> Select --> Detail --> Timeline
 ```
 
 ```gherkin
@@ -150,11 +146,11 @@
   I want arrow-driven views and detail panes
   So that I can drive common orchestration actions before the GUI exists
 
-Scenario: Operator handles a human gate
-  Given a selected Attention item represents a Fabro human gate
+Scenario: Operator inspects a lane-derived attention item
+  Given a selected Attention item is derived from a blocked needs-human work-item lane
   When the operator opens the detail pane
-  Then the TUI shows the repo, work item, Fabro run, latest timeline events, and attach action
-  And the operator can acknowledge, snooze, or open/copy the Fabro attach command
+  Then the TUI shows the repo, work item, and latest timeline events
+  And no local dismiss command is offered from the attention lens
 ```
 
 ## Scenario 6 -- Policy-rejected command produces no side effect
--- history/vN/spec.md
+++ active/spec.md
@@ -273,7 +273,7 @@
   critique, revise-required signals.
 - **Grooming** -- needs-regroom routing, slice proposal/approval events,
   factory/manual/spec routing.
-- **Attention** -- alerts, acknowledgement, snooze, owner/triage state.
+- **Attention** -- a pure inbox derived from work-item lane, lane reason, admission policy, and acceptance policy.
 - **Repository Hygiene** -- janitor checks, stale PR/branch/worktree
   findings, primary checkout health.
 - **Configuration** -- registered repos, source endpoints, notification
@@ -288,14 +288,14 @@
   Factory["Factory\ndrain + dispatch + gates"]
   Spec["Spec Lifecycle\nnext + doctor + revise signals"]
   Grooming["Grooming\nneeds-regroom + slicing"]
-  Attention["Attention\nalerts + ack + snooze"]
+  Attention["Attention\nlane-derived inbox"]
   Hygiene["Repository Hygiene\njanitor + stale state"]
   Config["Configuration\nrepos + endpoints + policy"]
 
   Ingestion -->|"source health events"| Attention
-  Factory -->|"human gate / failure events"| Attention
-  Spec -->|"revise / doctor events"| Attention
-  Grooming -->|"regroom events"| Attention
+  Factory -->|"blocked needs-human lane"| Attention
+  Spec -->|"revise / doctor signals"| Spec
+  Grooming -->|"lane derivation inputs"| Attention
   Hygiene -->|"hygiene findings"| Attention
   Config --> Ingestion
   Config --> Factory
@@ -319,8 +319,8 @@
 attention inbox, work card list, event timeline, or repo health view.
 
 **Attention item** -- A projection entry requiring human review or action,
-such as a Fabro human gate, LiveSpec revise need, doctor failure, host-only
-task, or non-converging factory item.
+derived only from a work item in pending approval with manual admission,
+acceptance with ai-then-human review, or blocked with a needs-human lane reason.
 
 **Factory** -- The Beads/Fabro execution path: ready work-items selected for
 Dispatcher, run in Fabro sandboxes, gated, merged, closed, bounced, or
```
