---
topic: control-plane-realization
author: claude-opus-4-8
created_at: 2026-06-25T16:38:44Z
---

## Proposal: Control-Plane realization framing

### Target specification files

- SPECIFICATION/spec.md

### Summary

In SPECIFICATION/spec.md §'Scope Boundary', add a 'Control-Plane realization' paragraph stating that this console is the reference realization of livespec's Control Plane (the operator-cockpit role livespec core defines in its workflow-planes architecture and elaborates as non-normative 'Control-Plane console guidance' in its non-functional requirements), and extend the existing Scope Boundary mermaid diagram to make the three livespec planes explicit — the console labelled CONTROL PLANE, livespec core grouped under a SPEC PLANE subgraph, and the orchestrator + Fabro grouped under an ORCHESTRATOR PLANE subgraph (GitHub stays a host source). The console-side half of livespec increment 4 (the cross-repo console control-plane contract).

### Motivation

Increment 4 of livespec epic livespec-zs22 (console side). Core landed a NON-normative '### Control-Plane console guidance' section in its non-functional-requirements.md (cut v142) plus 'The Control-Plane role' in its spec.md (increment 1); this is the companion that ties THIS console to that contract — it states explicitly that the console realizes the Control-Plane role, and aligns the Scope-Boundary diagram to the three-plane model. The owns / does-not-own lists already express the boundary; this names the role they realize and the dependency direction (the console is not a required dependency; not a Driver). Source design: livespec research/planning-workflow-gap/planning-lane-design.md §'Increment sequence' (item 4).

### Proposed Changes

Two edits in SPECIFICATION/spec.md §'Scope Boundary'. (1) After the 'The console may invoke existing CLIs or APIs ... source of truth for their own domains.' paragraph and before the mermaid diagram, insert a new bold-led paragraph 'Control-Plane realization.' establishing that this console is the reference realization of livespec's Control Plane — referencing livespec core's workflow-planes architecture (spec.md) and its non-normative Control-Plane console guidance (non-functional-requirements.md) descriptively (no cross-spec section citation, matching the console spec's existing reference style) — and restating the observe / compose / coordinate / never-own triad as the concrete expression of the owns/does-not-own lists, that the console is the Control Plane / operator cockpit and NOT a Driver, and that it is NOT a required dependency (the spec lifecycle and orchestrator skills stay independently drivable without it). (2) Extend the existing Scope Boundary mermaid diagram: relabel the Console subgraph 'CONTROL PLANE: livespec-console-beads-fabro', wrap the LiveSpec node in a 'SPEC PLANE' subgraph, wrap the Orchestrator and Fabro nodes in an 'ORCHESTRATOR PLANE' subgraph, and leave GitHub as a host source outside the planes; all observe/command edges are unchanged. No headings change (so no heading-coverage impact); diagram authored in the console spec's existing mermaid style (\n label breaks, no classDef).
