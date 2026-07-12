---
topic: out-of-band-edit-2026-07-12t12-17-29z
author: livespec-doctor
created_at: 2026-07-12T12:17:29Z
---

## Proposal: out-of-band-edit-2026-07-12t12-17-29z

doctor detected drift between HEAD-active spec content and the
HEAD-history-vN snapshot; this auto-backfill records the active
state as the new canonical version.

### Proposed Changes

```diff
--- history/vN/contracts.md
+++ active/contracts.md
@@ -530,6 +530,13 @@
 unavailability count, so a true-empty screen is never dressed as a false
 alarm.
 
+The TUI MUST let the operator drive each of the five human-valve and
+policy-edit commands -- approve, accept, reject, set-admission, and
+set-acceptance -- against the selected work-item, each routed through the
+shared orchestrator action port rather than any direct ledger write, and a
+destructive reject gated behind an explicit confirmation step before the
+command is submitted.
+
 ```mermaid
 flowchart TB
   subgraph Screen["TUI default screen"]
```
