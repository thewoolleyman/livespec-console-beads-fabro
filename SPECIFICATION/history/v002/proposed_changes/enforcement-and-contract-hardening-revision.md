---
proposal: enforcement-and-contract-hardening.md
decision: accept
revised_at: 2026-06-25T05:03:05Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Accepted as authored. D1 reconciles the event envelope with the events table so the table is a faithful 1:1 projection (adds causation_event_id to the envelope; adds subject_kind/subject_id columns; renames correlation_id -> correlation_json; defines aggregate_id as the derived '<subject.kind>:<subject.id>' routing key). The behavioral-coverage finding pins the clause -> scenario -> test discipline as a mechanical fail-mode rule, ported to a first-class Rust check. Red-Green-Replay is hardened from a conditional ('once the repo hooks exist') to a mechanically-enforced hard MUST. The four declared impl_followups (scenario-test Rust checker, quality-gate CI jobs, nightly soak + beads chore wiring, red-green Rust checker) are filed separately as high-priority impl work-items.

## Resulting Changes

- contracts.md
- non-functional-requirements.md
