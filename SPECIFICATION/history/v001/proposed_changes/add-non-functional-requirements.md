---
topic: add-non-functional-requirements
author: claude-opus-4-8
created_at: 2026-06-24T20:02:30Z
---

## Proposal: Add non-functional-requirements.md

### Target specification files

- SPECIFICATION/non-functional-requirements.md
- SPECIFICATION/README.md

### Summary

Add a new non-functional-requirements.md modeled on the real livespec NFR doc, separating contributor-facing requirements from the operator-facing spec files, and update SPECIFICATION/README.md to list it.

### Motivation

The seed never created the template-declared non-functional-requirements.md; constraints.md was doing double duty for both operator-observable and contributor-only rules.

### Proposed Changes

A new `non-functional-requirements.md` MUST be added to the spec tree, modeled on the real livespec NFR document. It MUST be read alongside `spec.md`, `contracts.md`, `constraints.md`, and `scenarios.md`, and MUST enumerate contributor-facing non-functional requirements that are NOT observable at the operator-facing TUI/CLI/API surface. It MUST use a `## Boundary` preamble plus four top-level sections mirroring the functional files: `## Spec` (Red-Green-Replay commit discipline), `## Contracts` (the `just check` quality-gate aggregate and the Beads/Fabro family secret convention), `## Constraints` (the implementation language and `#![forbid(unsafe_code)]`, Railway-Oriented Programming, Domain-Driven Design layering, and the Architecture Tests rules), and `## Scenarios` (empty initially). The `## Boundary` section MUST carry the litmus that constraints a console operator could observe stay in `constraints.md` while constraints binding only contributors live here. `SPECIFICATION/README.md` MUST be updated to list `non-functional-requirements.md` and SHOULD redescribe `constraints.md` as operator-observable constraints.

## Proposal: Split contributor-facing constraints out of constraints.md

### Target specification files

- SPECIFICATION/constraints.md

### Summary

Reduce constraints.md to only operator-observable constraints, moving all contributor-facing requirements into non-functional-requirements.md.

### Motivation

Apply the user-observability litmus so each requirement lives in exactly one place, matching the real livespec functional/non-functional split.

### Proposed Changes

`constraints.md` MUST retain only operator-observable constraints -- the single-binary multi-mode runtime shape and the Event-Sourcing Safety guarantees -- and MUST move all contributor-facing requirements to `non-functional-requirements.md`: the `product code MUST be Rust` rule and `#![forbid(unsafe_code)]`, Railway-Oriented Programming, Domain-Driven Design layering, the Architecture Tests, the Quality Gate, the Beads/Fabro Family Secret Convention, and Red-Green-Replay. The file preamble MUST point readers to `non-functional-requirements.md` for the migrated contributor-facing rules.
