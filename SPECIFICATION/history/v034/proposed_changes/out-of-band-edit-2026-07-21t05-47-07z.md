---
topic: out-of-band-edit-2026-07-21t05-47-07z
author: livespec-doctor
created_at: 2026-07-21T05:47:07Z
---

## Proposal: out-of-band-edit-2026-07-21t05-47-07z

doctor detected drift between HEAD-active spec content and the
HEAD-history-vN snapshot; this auto-backfill records the active
state as the new canonical version.

### Proposed Changes

```diff
--- history/vN/contracts.md
+++ active/contracts.md
@@ -652,7 +652,7 @@
 
 The TUI's Status line MUST render context-specific shortcut key hints — the keys that act in the CURRENTLY-focused context — and MUST NOT render a static or empty hint line where actions are available. The hints MUST reflect the currently-focused pane: switching focus to a different pane MUST change the hints to that pane's available actions. The hints MUST also reflect any open modal or overlay: opening a modal or overlay MUST replace the pane's hints with the hints for that modal/overlay, and closing it MUST restore the focused pane's hints. No context in which shortcut actions are available may show an empty hint line. A hint MUST NOT advertise a binding the key does not perform in that context — a hint that names an action the key does not take is worse than no hint, because the operator cannot tell a broken key from a mis-documented one. Concretely: a key whose action differs between two contexts (including between a view's sub-views) MUST be described by the action it actually performs in the current one; and a key that acts only on a selected work-item MUST NOT be listed where no work-item is selected, which includes a lane overview (whose selection is a lane, not an item) and an empty drilled-in lane. Finer-grained per-item suppression — hiding a key that is inert for the SPECIFIC selected item, such as a status move a `done` item cannot be driven through — is NOT required by this clause; it depends on the per-state valid-verb vocabulary, which is owned by `livespec-orchestrator-beads-fabro` and not yet consumed here. The specific hint strings and key bindings are an implementation detail; the contract is that the hint line is non-empty, honest, and appropriate to the currently-focused pane and any open overlay, and changes as focus or overlay changes.
 
-The TUI MUST let the operator read a selected work-item's FULL standardized record without leaving the console. The record surface MUST be reachable from the drilled-in lane list, where an individual work-item is selected. It MUST render every field of the orchestrator's standardized work-item shape — at minimum the title, the description, the type, the status, the lane, the rank, the origin, the gap id, the assignee, the dependencies, the capture time, the resolution, the reason, the audit trail, the superseding item, and the spec commitment hint — and a field the orchestrator did not emit MUST render as explicitly absent rather than be omitted, so the operator can tell an unset field from an undisplayed one. The description MUST be carried as emitted rather than reformatted, and the surface MUST scroll when the record is taller than the viewport, so no part of a long record is unreachable. The standardized work-item shape is owned by `livespec-orchestrator-beads-fabro` and consumed here verbatim: the console MUST NOT re-derive, re-compute, or reformat a record field, and MUST NOT drop a work-item from the board because a descriptive field it does not recognize is absent or unparseable. The specific key binding, modal geometry, and field ordering are an implementation detail; the contract is that every standardized field of the selected work-item is readable inside the console.
+The TUI MUST let the operator read a selected work-item's FULL standardized record without leaving the console. The record surface MUST be reachable from the drilled-in lane list, where an individual work-item is selected, and from the needs-attention view when the selected attention row carries a known work-item id. It MUST render every field of the orchestrator's standardized work-item shape — at minimum the title, the description, the type, the status, the lane, the rank, the origin, the gap id, the assignee, the dependencies, the capture time, the resolution, the reason, the audit trail, the superseding item, and the spec commitment hint — and a field the orchestrator did not emit MUST render as explicitly absent rather than be omitted, so the operator can tell an unset field from an undisplayed one. The description MUST be carried as emitted rather than reformatted, and the surface MUST scroll when the record is taller than the viewport, so no part of a long record is unreachable. The standardized work-item shape is owned by `livespec-orchestrator-beads-fabro` and consumed here verbatim: the console MUST NOT re-derive, re-compute, or reformat a record field, and MUST NOT drop a work-item from the board because a descriptive field it does not recognize is absent or unparseable. The specific key binding, modal geometry, and field ordering are an implementation detail; the contract is that every standardized field of the selected work-item is readable inside the console.
 
 The TUI's top/header pane MUST be focusable within the pane focus cycle: the operator MUST be able to move focus onto it as onto any other pane. While the top/header pane holds focus, it MUST support HORIZONTAL scrolling to reveal content clipped at the current viewport width — content cut off on a narrow viewport MUST become reachable by scrolling the pane left and right while it is focused. When focus moves away from the top/header pane (on blur), the pane MUST return to its default left-justified position rather than remaining mid-scroll. The specific key bindings, scroll step, and column counts are an implementation detail; the contract is that the top/header pane joins the focus cycle, scrolls horizontally to reveal clipped content while focused, and snaps back to its left-justified default on blur.
 
--- history/vN/scenarios.md
+++ active/scenarios.md
@@ -872,11 +872,14 @@
 flowchart LR
   Overview["Lane overview"]
   Lane["Drilled-in lane (item list)"]
+  Attention["Needs-attention row with work-item id"]
   Item["Work-item record (title, description, whole standardized shape)"]
 
   Overview -- "enter: drill into lane" --> Lane
   Lane -- "enter: open selected item" --> Item
+  Attention -- "enter: open selected item" --> Item
   Item -- "esc" --> Lane
+  Item -- "esc" --> Attention
   Lane -- "esc" --> Overview
 ```
 
@@ -892,6 +895,13 @@
   Then the selected work-item's record opens over the lane list
   And the record shows that work-item, not any other
 
+Scenario: Enter on a needs-attention work-item opens the same record surface
+  Given the needs-attention view has a selected row carrying a known work-item id
+  When the operator presses the key the Status line advertises for the item
+  Then the selected work-item's record opens over the needs-attention list
+  And the command modal is not opened unless it has at least one action to offer
+  And the record shows that work-item, not any other
+
 Scenario: The record surface shows every standardized field
   Given a selected work-item whose standardized record has every field populated
   When the operator opens its record
@@ -910,11 +920,12 @@
   Then the end of the description becomes reachable
   And scrolling stops at the last row rather than running past it
 
-Scenario: Esc closes the record back to the lane it was opened from
+Scenario: Esc closes the record back to where it was opened from
   Given an open work-item record
   When the operator presses Esc
   Then the record closes
-  And the operator is back in the drilled-in lane, not the lane overview
+  And the operator is back in the drilled-in lane or needs-attention list it was opened from
+  And the previous selection is preserved
 
 Scenario: The Status line names the action the key actually performs
   Given the Lanes view
```
