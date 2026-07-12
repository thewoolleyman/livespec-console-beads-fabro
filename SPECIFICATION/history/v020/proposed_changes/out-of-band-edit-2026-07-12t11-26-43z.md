---
topic: out-of-band-edit-2026-07-12t11-26-43z
author: livespec-doctor
created_at: 2026-07-12T11:26:43Z
---

## Proposal: out-of-band-edit-2026-07-12t11-26-43z

doctor detected drift between HEAD-active spec content and the
HEAD-history-vN snapshot; this auto-backfill records the active
state as the new canonical version.

### Proposed Changes

```diff
--- history/vN/contracts.md
+++ active/contracts.md
@@ -522,6 +522,14 @@
 (fleet, mode, ingestion, Fabro summary) MUST reflect whether autonomous
 mode is active for the selected repo.
 
+The header/status line MUST surface backing-source unavailability for the
+current cycle -- how many and which sources degraded to a not-observed
+finding rather than an observed snapshot -- so an operator can tell a
+cockpit-blind screen (sources unreachable) from an idle factory (nothing
+actionable). When every source is observed the header carries no phantom
+unavailability count, so a true-empty screen is never dressed as a false
+alarm.
+
 ```mermaid
 flowchart TB
   subgraph Screen["TUI default screen"]
--- history/vN/scenarios.md
+++ active/scenarios.md
@@ -389,3 +389,35 @@
   And emits nothing for an unchanged id
   And every emitted event is keyed by the item's stable id
 ```
+
+## Scenario 13 -- Operator distinguishes cockpit-blind from factory-idle
+
+```mermaid
+flowchart LR
+  Poll["Poll each backing source"]
+  NotObserved["source.not_observed_finding_observed"]
+  Observed["observed snapshot"]
+  Header["Header source-health indicator"]
+  Operator["Operator sees which sources are unavailable"]
+
+  Poll --> NotObserved --> Header --> Operator
+  Poll --> Observed
+```
+
+```gherkin
+Feature: Source-unavailability is legible in the header
+  As an operator
+  I want the header to show when backing sources are unavailable
+  So that a cockpit-blind screen is never mistaken for an idle factory
+
+Scenario: Unavailable sources are counted and named in the header
+  Given one or more backing sources degraded to a not-observed finding this cycle
+  When the operator screen is rendered
+  Then the header shows how many sources are unavailable
+  And the header names which sources are unavailable
+
+Scenario: A healthy cycle shows no phantom unavailability count
+  Given every backing source was observed this cycle
+  When the operator screen is rendered
+  Then the header shows no source-unavailability indicator
+```
```
