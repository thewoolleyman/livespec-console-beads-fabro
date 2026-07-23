---
proposal: command-queue-exactly-once-consumption.md
decision: accept
revised_at: 2026-07-23T22:38:10Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Accept as ALREADY APPLIED: PR #399 (merge 2665cad) applied this proposal verbatim to contracts.md (single-consumer subsection + claim-race flowchart in the Command Handling section) and scenarios.md (Scenario 24), riding with the -ipwtll implementation per the maintainer's amend-riding-with-impl ruling, and linked the new clauses' gap-ids in tests/heading-coverage.json to Scenario 24 and the top-of-pyramid race/recovery tests. The v035 out-of-band-edit cut (4ef9ebc) recorded the direct application; this revision formally dispositions the pending proposal. No further spec-file edits are required — the live tree already carries the accepted content, and the behavioral-coverage gate is clean over it.

## Resulting Changes

(none)
