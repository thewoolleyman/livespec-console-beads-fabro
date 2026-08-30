---
proposal: coverage-region-gate-merged-view-reformulation.md
decision: accept
revised_at: 2026-08-30T13:59:24Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Measurement correction: the not-yet-present coverage-region-gate obligation is defined against the merged, cross-instantiation reachable-region view (scalar-max over --json data[0].functions[].regions; equivalently llvm-cov show -show-instantiations=false with no ^0) rather than the raw --workspace --lib summary region count, which over-reports by an llvm-cov cross-object merge artifact. Line gate, --lib no-carve-outs binding, main.rs shim, and the No-coverage-exclusions clause all stand unchanged; nothing is exempted. Reformulated as a definitional clarification (no new normative clause; enforcement stays with impl obligation txtzn5.11) to satisfy console-spec-check clause-scenario linkage. Independent read-only opus reviewer returned NO BLOCKERS over the exact resulting bytes.

## Resulting Changes

- non-functional-requirements.md

## Ratification Review

ratification_review: auto-spawn
reviewer_model: opus
reviewer_identity: opus
separate_reviewer: True
read_only: True
reviewed_at: 2026-08-30T13:57:23Z
verdict: NO BLOCKERS
proposal_stem: coverage-region-gate-merged-view-reformulation
content_digest: a61bd0189c86f54e9dabf8741869242d790d8938570ebce295a5b1bb7ac9570c
