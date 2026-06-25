---
topic: impl-drift-reconciliation
author: claude-opus-4-8
created_at: 2026-06-25T21:54:24Z
---

## Proposal: Name cargo machete and the dual test run in the Quality Gate inner loop

### Target specification files

- non-functional-requirements.md

### Summary

The Quality Gate inner-loop enumeration omits two checks the justfile and CI run on every push/PR. `just check-deps` runs `cargo machete` (unused-dependency audit) alongside `cargo deny`, and `just check` runs `cargo test` (`check-test`) alongside `cargo nextest` (`check-nextest`). Reconcile the enumeration to the justfile so the spec names what actually gates the inner loop.

### Motivation

Impl→spec drift reconciliation grounded in the justfile and CI. Falsifiable: `justfile` `check-deps` runs `cargo deny check` then `cargo machete`; `check` iterates both `check-test` (`cargo test --workspace --all-features`) and `check-nextest` (`cargo nextest run --workspace --all-features`); `.github/workflows/ci.yml` installs `cargo-deny,cargo-machete` for the `check-deps` matrix target. The spec currently names only `cargo nextest` and only `cargo deny`, so an operator-contributor reading the gate would not know `cargo machete` (a distinct unused-dependency tool, not covered by `cargo deny`) and the doctest-covering `cargo test` run also gate.

### Proposed Changes

In `non-functional-requirements.md` §Contracts → Quality Gate, edit two inner-loop (`just check`) bullets.

Replace:
    - tests with a modern Rust test runner (`cargo nextest`)
with:
    - tests run on both the standard runner (`cargo test`, which also exercises doctests `cargo nextest` does not) and a modern Rust test runner (`cargo nextest`)

Replace:
    - dependency audit/deny checks (`cargo deny`)
with:
    - dependency-audit checks: `cargo deny` (advisories, licenses, bans, sources) and `cargo machete` (unused-dependency detection)

Keep the existing `just check` mermaid diagram coherent: the `Audit["audit / deny"]` node already reads generically and needs no change, but if it is touched, it MUST stay consistent with the `cargo deny` + `cargo machete` pair named above. No operator-facing file changes; this is a contributor-facing enumeration reconciliation, so no `scenarios.md` scenario is introduced (the `## Scenarios` section in `non-functional-requirements.md` stays empty).

## Proposal: Reconcile the arch-check operator-help redirect entry in Product Shape

### Target specification files

- spec.md

### Summary

spec.md §Product Shape states architecture checks are "NOT an operator subcommand," but the console binary dispatches an `arch-check` arm and lists `arch-check` in its operator `help` output. The arm performs no architecture check — it prints a redirect to the contributor gate (`just check-arch`). Reconcile the statement to acknowledge the redirect-only entry while preserving the boundary that the console runs no architecture check for operators.

### Motivation

Impl→spec drift reconciliation grounded in `console-cli`. Falsifiable: `crates/console-cli/src/lib.rs` `help_text()` lists `arch-check` under "Commands:", and `run_static` has a `Some("arch-check")` arm returning "run `just check-arch` for architecture enforcement". So an `arch-check` token IS dispatched and IS surfaced to operators; the literal "NOT an operator subcommand" is inaccurate against the redirect arm. The capability boundary (the console performs no architecture check) still holds and the reconciliation preserves it.

### Proposed Changes

In `spec.md` §Product Shape, replace the paragraph:

    Architecture checks are NOT an operator subcommand: they are a
    contributor quality-gate concern owned by
    `non-functional-requirements.md` -> Architecture Tests and realized as
    the separate `console-arch-check` binary.

with:

    Architecture checks are NOT an operator capability: the console
    performs no architecture check for an operator. They are a
    contributor quality-gate concern owned by
    `non-functional-requirements.md` -> Architecture Tests and realized as
    the separate `console-arch-check` binary. The console binary surfaces
    an `arch-check` token only as a discoverability redirect — it appears
    in `help` output and its handler prints a pointer to the contributor
    gate (`just check-arch`) instead of running any check.

This clarifies an existing boundary against observable CLI behavior; it introduces no new operator capability and therefore no new `scenarios.md` scenario (the unchanged invariant remains: the console runs no architecture check for operators).
