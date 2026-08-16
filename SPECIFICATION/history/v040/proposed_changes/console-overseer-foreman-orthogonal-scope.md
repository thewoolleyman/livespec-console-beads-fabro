---
topic: console-overseer-foreman-orthogonal-scope
author: claude-sonnet-5
created_at: 2026-08-16T06:19:27Z
---

## Proposal: console-does-not-own-overseer-foreman

### Target specification files

- SPECIFICATION/spec.md

### Summary

Adds livespec-overseer's foreman/overseer driving layer to the console's explicit does-not-own list in the Scope Boundary section, recording that it is an orthogonal, independent mechanism for driving factory work rather than a plane the console observes, composes, or exposes settings for.

### Motivation

A session investigated whether the console TUI needed new UI to surface foreman/overseer autonomy config levers (for example livespec-overseer's foreman_valve_disposition, which governs whether the foreman may convene a consensus panel on blocked sessions). The maintainer clarified that foreman/overseer and the console are two separate, orthogonal approaches to driving factory work, not a layered relationship where one configures or gates the other. This proposal records that clarification as a durable directive so a future session does not re-investigate the same question.

### Proposed Changes

In `SPECIFICATION/spec.md`, `## Scope Boundary`, the console's `does not own` list MUST gain one additional bullet naming `livespec-overseer`'s foreman/overseer driving layer, including its own configuration levers such as `foreman_valve_disposition`. Immediately after the `does not own` list, add one sentence stating: the overseer/foreman layer is an orthogonal, independent mechanism for driving factory work that operates alongside the console rather than through it; the console MUST NOT be treated as a control surface for, and SHOULD NOT gain UI to observe or edit, foreman/overseer configuration or state. This is a documentation-only clarification of an existing boundary; it introduces no new console behavior, event, command, or projection.
