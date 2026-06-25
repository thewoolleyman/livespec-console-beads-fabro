---
proposal: impl-drift-reconciliation.md
decision: accept
revised_at: 2026-06-25T21:56:33Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Both proposals are impl->spec drift reconciliations grounded in falsifiable code/config facts, not new behavior. (1) The Quality Gate inner loop genuinely runs `cargo machete` (in check-deps, alongside cargo deny) and `cargo test` (check-test, alongside cargo nextest) on every push/PR per the justfile and CI matrix, so the spec enumeration should name them. (2) The console's `help_text()` lists `arch-check` and its run_static arm redirects to `just check-arch`, so the literal 'NOT an operator subcommand' is inaccurate; the reconciliation preserves the boundary (the console performs no architecture check) while matching observable CLI behavior. Neither edit introduces load-bearing operator behavior, so no new scenarios.md scenario is required (NFR edit is contributor-facing; spec.md edit clarifies an existing boundary without changing the invariant).

## Resulting Changes

- non-functional-requirements.md
- spec.md
