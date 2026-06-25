---
proposal: impl-drift-reconciliation.md
decision: accept
revised_at: 2026-06-25T09:58:57Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Accepted the four impl->spec drift findings from the capture-spec-drift survey. #1 realigns contracts.md's event envelope and events table to the implemented scalar schema (scalar correlation_id / causation_id / aggregate_id; no subject columns, no correlation object), correcting the prior D1 reconciliation that had drifted toward an idealized envelope the implementation never adopted. #2 enumerates factory.drain.not_wired, the honest not-wired outcome the impl actually emits. #3 removes arch-check from spec.md's operator subcommand surface (it is a contributor binary owned by the NFR Architecture Tests). #4 generalizes Scenario 2's drain command to the configurable drain program invoked through the drain port, matching the implemented DispatcherFactoryDrainPort.

## Resulting Changes

- contracts.md
- spec.md
- scenarios.md
