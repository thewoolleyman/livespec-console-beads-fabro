---
topic: nfr-contributor-scenarios
author: claude-opus-4-8
created_at: 2026-06-26T02:11:10Z
---

## Proposal: Author NFR contributor-facing scenarios for the behavioral-coverage binding rule

### Target specification files

- non-functional-requirements.md

### Summary

`non-functional-requirements.md` §"Behavioral Coverage" binds **every**
normative clause to a Gherkin scenario, and states explicitly that the
document's **own contributor-facing clauses link to a `##` H2 section in
`## Scenarios` below** (operator-facing clauses link to `scenarios.md`).
But the NFR `## Scenarios` section is currently empty — it reads "No
contributor-facing scenarios are pinned yet." Measured against the spec
(via the family `spec_clauses` extractor), `non-functional-requirements.md`
carries **52** of the project's 82 normative clauses, and none of them have
a scenario to link to. Author contributor-facing Gherkin scenarios under
NFR `## Scenarios`, each a `##` H2 the behavioral-coverage checker can
resolve, so every NFR contributor-facing clause can bind to one.

### Motivation

Surfaced by grooming the keystone work-item `rrr4i4` (port the
clause→scenario→test behavioral-coverage checker to Rust). The keystone's
Rust checker enforces the binding rule in `fail` mode; it **cannot reach
`fail` mode** while 52 NFR clauses have no scenario link-targets. Authoring
those targets is a `SPECIFICATION/` change, so it is out of the impl track
and routed here. Falsifiable today: NFR §Scenarios literally states "No
contributor-facing scenarios are pinned yet"; the binding rule in
§"Behavioral Coverage" requires the NFR clauses to link to NFR `## Scenarios`
H2s; the orchestrator's `detect-impl-gaps`/`spec_clauses` primitive counts 52
NFR clauses. Accepting this proposal unblocks the keystone's impl-side
`B-nfr` slice (add the clause→scenario links + register the scenarios' tests)
and the final `fail`-mode flip.

The maintainer chose to **author contributor scenarios** (this proposal)
over the alternative of simplifying the binding rule so NFR clauses link to
`scenarios.md` operator scenarios as livespec core does.

### Proposed Changes

In `non-functional-requirements.md` §"Scenarios" (currently the placeholder
"No contributor-facing scenarios are pinned yet"), author a set of
contributor-facing Gherkin scenarios — one `##` H2 per behavior theme —
covering every NFR contributor-facing normative clause. Each `##` H2 must be
resolvable by the behavioral-coverage checker (the `scenario` target of a
`tests/heading-coverage.json` `clauses[]` link) and have a top-of-pyramid
test registered against it. A suggested theme set, aligned to the
clause-bearing NFR sections (the `## revise` step refines the exact cut and
prose, ensuring full clause coverage):

- **Quality gate enforces the inner and merge loops** — the §Contracts →
  "Quality Gate" clauses (the `just check` inner loop: fmt, strict clippy,
  `cargo test` + `cargo nextest`, 100% lib line coverage, `cargo deny` +
  `cargo machete`, arch-check; and the merge/CI loop).
- **Behavioral coverage links every clause to a tested scenario** — the
  §"Behavioral Coverage" clauses (clause→scenario→test chain; `fail` mode;
  the `tests/heading-coverage.json` binding mechanism; no fail-closed
  placeholder).
- **Red-Green-Replay gates Rust product commits** — the §Spec →
  "Red-Green-Replay" clauses (staged-phase + trailer requirements;
  `commit-msg` + `just check` enforcement; the non-product exemption).
- **Architecture tests enforce the layering invariants** — the §Constraints
  → "Architecture Tests" clauses (structured crate-graph source; Rust AST
  source rules; falsifiable rule statements).
- **Domain-driven boundaries are preserved** — the §Constraints →
  "Domain-Driven Design" layering clauses (bounded-context ownership; domain
  has no infrastructure dependency; adapter isolation; UI talks only to
  projections/command APIs).
- **Railway-oriented error handling** — the §Constraints →
  "Railway-Oriented Programming" clauses (typed `Result` for expected
  failures).
- **Implementation language and the unsafe-code ban** — the §Constraints →
  "Implementation Language" clauses (Rust product code; `#![forbid(unsafe_code)]`).
- **Beads/Fabro family secret convention** — the §"Beads/Fabro Family Secret
  Convention" clauses (single bare `BEADS_DOLT_PASSWORD`; never committed or
  echoed; CI obtains it the same way).
- **Console boundary: observe, compose, coordinate, never own** — the
  §"Boundary" clauses.
- **Spec process and contributor toolchain** — the §"Spec" / §"Contracts"
  contributor-process clauses not covered above.

The `## revise` step authors the actual Gherkin per theme and confirms,
against the `spec_clauses` enumeration, that all 52 NFR contributor-facing
clauses resolve to a live `## Scenarios` H2 (so the keystone checker reaches
`fail` mode with zero unlinked clauses). No operator-facing behavior changes;
`scenarios.md` (Scenarios 1–8) is untouched.
