---
topic: out-of-band-edit-2026-06-29t21-53-36z
author: livespec-doctor
created_at: 2026-06-29T21:53:36Z
---

## Proposal: out-of-band-edit-2026-06-29t21-53-36z

doctor detected drift between HEAD-active spec content and the
HEAD-history-vN snapshot; this auto-backfill records the active
state as the new canonical version.

### Proposed Changes

```diff
--- history/vN/contracts.md
+++ active/contracts.md
@@ -379,12 +379,21 @@
 
 - Attention
 - Spec
-- Ready
-- Factory
-- Manual
-- Done
+- Lanes
 - Events
 - Repos
+
+The `Lanes` view is the work-item consumer: it renders the seven lifecycle
+lanes (`backlog`, `pending-approval`, `ready`, `active`, `acceptance`,
+`blocked`, `done`) projected from the orchestrator's emitted `lane` /
+`lane_reason` — the console consumes that lane assignment and never re-derives
+it (the lane vocabulary is owned by livespec core, referenced here, not
+re-decided). It is a hybrid sub-view: a lane-overview home listing all seven
+lanes with their counts and a preview of each lane's top rank-ordered items,
+with drill-in to a single lane's full rank-ordered list. The `Lanes` view
+subsumes the earlier ad-hoc `Ready` / `Factory` / `Manual` / `Done` groupings,
+which the lane model makes redundant. `Spec`, `Events`, and `Repos` remain as
+orthogonal, non-lane views.
 
 The default view MUST be Attention. Navigation SHOULD use arrow-driven
 selection lists, detail panes, command modals, `/` search, and a command
@@ -395,8 +404,8 @@
 flowchart TB
   subgraph Screen["TUI default screen"]
     Header["Header: fleet, mode, ingestion, Fabro summary"]
-    Left["Left navigation\nAttention / Spec / Ready / Factory / Manual / Done / Events / Repos"]
-    Center["Center list\narrow-selected work cards or attention items"]
+    Left["Left navigation\nAttention / Spec / Lanes / Events / Repos"]
+    Center["Center list\nlane overview, arrow-selected work cards, or attention items"]
     Right["Right detail pane\nsource refs, timeline, next actions"]
     Footer["Footer\nshortcuts, command status, live-update health"]
   end
```
