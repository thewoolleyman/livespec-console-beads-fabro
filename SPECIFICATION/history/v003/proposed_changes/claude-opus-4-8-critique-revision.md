---
proposal: claude-opus-4-8-critique.md
decision: modify
revised_at: 2026-06-25T08:58:23Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Accepted the four cycle-2 findings, each precisifying a rule that v002 introduced -- coverage crate-scope, the behavioral-coverage warn-to-fail graduation, the scenario-to-test binding mechanism, and the nightly work-item attributes. The maintainer's overriding criterion was that full enforcement must NOT slip through the cracks, which drove the fail-mode + fail-closed design: there is no advisory limbo to get stuck in, and the checker's own absence is a build failure. The fail-closed placeholder is already wired into just check and CI.

## Modifications

(A) Coverage: the 100% line+region gate binds EVERY workspace --lib target with no per-crate carve-outs (console-domain/application/eventstore/tui and any future lib); a UI or I/O branch that resists coverage is a redesign signal, not a carve-out. (B) Behavioral coverage runs in fail mode with NO warn lever and is fail-closed: until the Rust checker exists, just check + CI include a placeholder that fails, so the checker's absence is itself a build failure -- full enforcement cannot silently never arrive. (C) Binding mechanism specified: a scenario is its scenarios.md H2 heading, bound to clause and test via the tests/heading-coverage.json registry. (D) A nightly finding opens a high-priority chore, filed ready, in the Beads tenant livespec-console-beads-fabro.

## Resulting Changes

- non-functional-requirements.md
