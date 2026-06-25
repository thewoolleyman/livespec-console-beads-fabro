---
proposal: control-plane-realization.md
decision: accept
revised_at: 2026-06-25T16:39:23Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Increment 4 of livespec epic livespec-zs22 (console side). Ties this console to the Control-Plane contract core landed in v142 (its non-functional-requirements.md '### Control-Plane console guidance') and increment 1 (its spec.md 'The Control-Plane role'): adds a 'Control-Plane realization' paragraph to §'Scope Boundary' stating the console is the reference realization of livespec's Control Plane (observe / compose / coordinate / never-own; not a Driver; not a required dependency), and extends the Scope-Boundary diagram to the three-plane framing (console = CONTROL PLANE; livespec core under a SPEC PLANE subgraph; orchestrator + Fabro under an ORCHESTRATOR PLANE subgraph; GitHub a host source). Core referenced descriptively, matching the console spec's existing reference style (no cross-spec section citation). No heading changes. Source design: livespec research/planning-workflow-gap/planning-lane-design.md §'Increment sequence' (item 4).

## Resulting Changes

- spec.md
