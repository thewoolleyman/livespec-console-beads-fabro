---
topic: attention-item-sourced-tui-render-path
author: claude-fable-5
created_at: 2026-08-25T11:58:01Z
---

## Proposal: Scenario 5 attention inspection re-sourced to the attention_item stream; adapter emissions made exclusive

### Target specification files

- SPECIFICATION/scenarios.md
- SPECIFICATION/contracts.md

### Summary

Scenario 5's needs-attention inspection scenario is re-sourced from a lane-derived item to one projected from the ingested attention_item.* stream, surfacing the item's handoff command as the operator action and joining work-item detail by id against genuinely-ingested work-item snapshots only; the needs-attention adapter contract gains an explicit exclusivity sentence stating the console MUST NOT synthesize work-item lane state from attention items. This completes the v016/CN1 re-sourcing at the render path and removes the last ratified dependency on lane-derived attention shape.

### Motivation

Ratified v016 (CN1) re-sourced the canonical needs-attention inbox projection to the attention_item.* stream, and the ratified adapter contract already maps the needs-attention source to attention_item.* events ONLY, with the console consuming the composed snapshot verbatim. Scenario 5's inspection scenario still sources the inspected item from 'a blocked needs-human work-item lane', which is the one ratified clause keeping the TUI render path lane-derived. In code, that residue is load-bearing the wrong way round: ingest_needs_attention (crates/console-cli/src/lib.rs:1000-1008) calls normalize_impl_attention_ready_snapshot (crates/console-application/src/source_adapters.rs:1431-1468), fabricating work_item.* Ready-lane snapshots with manufactured admission/acceptance policy values from every impl: attention row to feed that lane-derived path — the shadow-state pattern console-sol finding 4 (homelab plan steady-state-loop-hardening, research/009) identified, contradicting the ratified adapter mapping. Work item livespec-console-beads-fabro-cddfxl (plan epic livespec-console-beads-fabro-ddfbcx) removes the synthesis and migrates the render path; this proposal is its spec-first leg, accepted before the lane-derived path is deleted.

### Proposed Changes

In `SPECIFICATION/scenarios.md` §"Scenario 5 -- TUI-first operator workflow", the inspection scenario MUST be replaced as follows (the mermaid flow and the surrounding feature block are unchanged):

```diff
-Scenario: Operator inspects a lane-derived needs-attention item
-  Given a selected needs-attention item is derived from a blocked needs-human work-item lane
-  When the operator opens the detail pane
-  Then the TUI shows the repo, work item, and latest timeline events
-  And no local dismiss command is offered from the needs-attention lens
+Scenario: Operator inspects an attention_item-sourced needs-attention item
+  Given a selected needs-attention item is projected from the ingested attention_item.* stream
+  When the operator opens the detail pane
+  Then the TUI shows the item's summary, kind, urgency, source reference, and handoff command as the operator action
+  And when the row carries a known work-item id the pane joins work-item detail by that id against genuinely-ingested work-item snapshots
+  And a referenced work-item record the console never ingested renders as explicitly absent rather than synthesized
+  And no local dismiss command is offered from the needs-attention lens
```

In `SPECIFICATION/contracts.md`, the needs-attention adapter bullet (§"Initial Adapters") MUST gain the following closing sentences after "the console MUST NOT reach around this port to recompute the inbox.":

```diff
+The adapter's `attention_item.*` emissions are the ONLY events this source
+produces: the console MUST NOT synthesize work-item lane state -- `work_item.*`
+events, lane membership, rank, status, or admission/acceptance policy values --
+from attention items. A work-item referenced by an attention row MUST be
+rendered from genuinely-ingested work-item snapshots, and one the console never
+ingested MUST render as explicitly absent rather than be fabricated.
```

Behavior discipline: the load-bearing behavior is carried by the amended Gherkin scenario plus the new MUST NOT clause together; the console's negative architecture test (an `impl:` attention row creates ONLY `attention_item.*` events) grades against this clause.
