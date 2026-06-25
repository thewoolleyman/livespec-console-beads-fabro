---
topic: claude-opus-4-8-critique
author: claude-opus-4-8
created_at: 2026-06-25T17:04:11Z
---

## Proposal: Control-Plane realization: name the console's invoke/command facet, not only read-only observation

### Target specification files

- SPECIFICATION/spec.md

### Summary

The 'Control-Plane realization' paragraph in spec.md SScope Boundary compresses livespec core's four-facet Control-Plane console guidance (core non-functional-requirements.md S'Control-Plane console guidance': what the console reads / composes / coordinates / never owns) into 'observes every plane read-only ... composes ... coordinates the human ... while never owning any plane's semantics.' It drops core's explicit 'What the console coordinates' facet -- that the console INVOKES every plane's operations on the operator's behalf and issues commands through the owning plane's published surface. As written, 'observes every plane read-only' sits directly above the SAME section's diagram edges (`Commands -->|ports invoke existing systems| Orchestrator/LiveSpec/Fabro`) and reads as if the console never acts on a plane, contradicting the console's own command model: the owns-list includes 'canonical console events and commands', contracts.md SCommand Handling and scenarios.md Scenario 2 have the console issue factory.drain/dispatch/pause/resume + spec.doctor + grooming.regroom commands that invoke planes through their published CLIs/APIs.

### Motivation

Internal inconsistency plus a faithful-realization gap. 'Observes every plane read-only ... coordinates the human' can be read as 'the console never acts on a plane', which contradicts the command->ports->plane edges in the very same diagram and the console's central command model; it also under-states livespec core's authoritative Control-Plane role, which explicitly includes invoking each plane's operations and issuing commands through the owning plane's own published surface (the same one-directional seam discipline). The realization paragraph should name the invoke/command facet so it is internally consistent and a faithful realization of core's contract.

### Proposed Changes

In the 'Control-Plane realization' paragraph, extend the observe/compose/coordinate/never-own triad to include the invoke/command facet from core's 'What the console coordinates', keeping faith with 'never own' and the one-directional published-surface seam. For example, change '...composes the cross-plane operator picture that no single plane can produce alone, and *coordinates* the human -- while *never owning* any plane's semantics...' to '...composes the cross-plane operator picture that no single plane can produce alone, *invokes* each plane's own operations on the operator's behalf -- issuing commands only through that plane's published command surface, never reaching around it -- and *coordinates* the human, while *never owning* any plane's semantics...'. This makes the read-only observation channel and the command-invocation channel both explicit and consistent with the diagram's command edges, the owns-list, and core's four-facet Control-Plane guidance. No heading changes.
