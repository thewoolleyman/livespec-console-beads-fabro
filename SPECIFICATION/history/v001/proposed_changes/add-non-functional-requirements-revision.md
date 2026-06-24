---
proposal: add-non-functional-requirements.md
decision: accept
revised_at: 2026-06-24T20:03:33Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Authored per the agreed full-split mapping: operator-observable constraints (single-binary runtime shape + event-sourcing safety) stay in constraints.md; contributor-facing requirements (Rust+forbid(unsafe_code), Railway-Oriented Programming, DDD layering, Architecture Tests, Quality Gate, secret convention, Red-Green-Replay) move into the new non-functional-requirements.md. README.md updated to list the new file.

## Resulting Changes

- non-functional-requirements.md
- constraints.md
- README.md
