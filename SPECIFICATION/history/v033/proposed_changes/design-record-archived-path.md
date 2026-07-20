---
topic: design-record-archived-path
author: claude-opus-4-8
created_at: 2026-07-20T04:01:37Z
---

## Proposal: Design-record citation names a path that no longer exists

### Target specification files

- SPECIFICATION/spec.md

### Summary

The `design record:` citation in spec.md names `plan/autonomous-mode/handoff.md` in repo `thewoolleyman/livespec`. That plan thread was ARCHIVED 2026-07-20 to `plan/archive/autonomous-mode/handoff.md`, so the citation now names a path that does not resolve. This repoints it. Path-only: the archive move preserved the cited content byte-for-byte, so only the directory changes.

### Motivation

The livespec-core plan thread `plan/autonomous-mode/` was superseded and archived on 2026-07-20 as part of splitting a 3220-line thread that had become coupled and non-cohesive. This repo's spec.md cites that path as the DESIGN RECORD for the dispatcher policy settings it describes. This matters more than an ordinary broken link: across this fleet the cited design record is the TIEBREAKER over shipped spec text when the two disagree, so a dangling design-record citation silently removes the tiebreaker. Citation fidelity is review-enforced only — no mechanical check catches a dangling design-record path — which is exactly why this is repaired deliberately rather than left to be discovered later. The sibling orchestrator repo carries the same repair for its two citations of the same record, and both land in the same window as the archive move so no period exists in which any of the three dangle.

### Proposed Changes

--- CHANGE 1: SPECIFICATION/spec.md, the dispatcher-policy-settings design-record citation ---
REPLACE the line reading VERBATIM:

record: repo `thewoolleyman/livespec`, `plan/autonomous-mode/handoff.md`).

with VERBATIM:

record: repo `thewoolleyman/livespec`, `plan/archive/autonomous-mode/handoff.md`).

--- CLAUSE-COUNT NOTE for the revise step ---
PATH-ONLY change. The line carries no whole-word MUST or SHOULD, so no
normative clause is added, removed, or reworded, and this repo's
console-spec-check ground-truth rule counts are UNCHANGED. No
tests/heading-coverage.json co-edit is required: no `## ` heading changes and
no clause gap-id changes.

