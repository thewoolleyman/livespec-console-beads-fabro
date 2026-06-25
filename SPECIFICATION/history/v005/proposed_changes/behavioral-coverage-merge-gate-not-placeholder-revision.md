---
proposal: behavioral-coverage-merge-gate-not-placeholder.md
decision: accept
revised_at: 2026-06-25T15:23:25Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Accepted: drop the fail-closed CI placeholder mandate (it deadlocked the merge gate, blocking all merges). The clause->scenario->test requirement stays as a hard fail-mode gate that attaches to the real checker and lands with it, tracked as the release-blocking scenario-test-rust-checker work-item. Getting CI green for a not-yet-built checker requires enforcement via that tracked obligation, not a blocking placeholder.

## Resulting Changes

- non-functional-requirements.md
