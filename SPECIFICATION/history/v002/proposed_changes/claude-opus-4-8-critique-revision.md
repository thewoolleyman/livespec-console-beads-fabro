---
proposal: claude-opus-4-8-critique.md
decision: modify
revised_at: 2026-06-25T05:03:05Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Accepted the eight critique findings, landing the concrete resolutions chosen with the maintainer. F1 relaxes the adapter split to per-source modules behind ports (matching the implementation, which has no adapter crates), with isolation enforced by the arch-check rather than the compiler. F2 makes the architecture checks required and falsifiable (structured crate-graph via cargo metadata + AST-level source checks, not text scans). F3/F4 set the quality gate concretely (see modifications). F5 adds the adapter/command honesty rule (no fabricated success; simulated/absent I/O must signal not-observed). F6 unifies the duplicated DDD-layering / architecture-test dependency rules into a single source of truth and fixes 'MUST not' -> 'MUST NOT'. F7 adds the missing command-rejection, crash-gap-recovery, and snapshot-corruption-recovery scenarios. F8 settles the single-binary runtime shape as a SHOULD by softening spec.md's declarative phrasing to match constraints.md.

## Modifications

F3/F4 land more concretely than the findings asked. Coverage gates at 100% line AND region (cargo llvm-cov --fail-under-lines 100 --fail-under-regions 100) over the bottom-of-pyramid lib targets (console-domain, console-application, console-eventstore) with NO exclusions -- uncoverable code is redesigned, never annotated away -- as a design forcing-function. Fuzz and mutation are explicitly excluded from `just check` and run on the CI merge-gate (fuzz >=60s/target seeded from a committed regression corpus, fail on any new crash; cargo mutants --in-diff over the logic crates with a justified-survivor allow-list via #[mutants::skip]+comment or mutants.toml, not a blind 100% score) plus a nightly soak that opens a work-item instead of failing the canonical branch. F1 is resolved toward modules-not-crates with port-based isolation as the binding invariant. F8 is resolved toward SHOULD.

## Resulting Changes

- spec.md
- contracts.md
- scenarios.md
- non-functional-requirements.md
