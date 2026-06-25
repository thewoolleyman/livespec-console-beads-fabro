---
proposal: claude-opus-4-8-critique.md
decision: modify
revised_at: 2026-06-25T17:05:02Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Accepted the critique finding. The 'Control-Plane realization' paragraph (added in v006) compressed livespec core's four-facet Control-Plane console guidance into 'observes read-only / composes / coordinates the human / never owns', dropping core's explicit 'What the console coordinates' facet -- that the console INVOKES each plane's operations and issues commands through the owning plane's published surface. 'Observes every plane read-only' then sat directly above the same Scope-Boundary diagram's Commands->ports->plane edges and the console's own command model, reading as if the console never acts on a plane. Reconciled by naming the invoke/command facet explicitly, restoring fidelity to core's contract and internal consistency with the diagram's command edges, the owns-list, contracts.md Command Handling, and scenarios.md Scenario 2.

## Modifications

Landed the proposed wording: inserted '*invokes* each plane's own operations on the operator's behalf -- issuing commands only through that plane's published command surface, never reaching around it --' before '*coordinates* the human', and kept 'while *never owning* any plane's semantics' plus the one-directional published-surface seam. spec.md only; no heading changes (no heading-coverage impact).

## Resulting Changes

- spec.md
