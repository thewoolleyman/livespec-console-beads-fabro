---
proposal: console-control-plane-boundary.md
decision: modify
revised_at: 2026-09-01T11:16:40Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-fable-5
---

## Decision and Rationale

Accepted with three modifications from the independent ratification review (opus, read-only, two passes). The proposal replaces the v040 overseer-orthogonality boundary with a control-plane-surface clause plus a no-resident-decider guard (A), retires the Fabro human-gate observation model for the orchestrator's ratified v093 needs_human terminal and blocked/needs-human ledger valve (B, the spec half of livespec-console-beads-fabro-h7jp), and records run questions as an orchestrator-published attention kind with no console-owned semantics (C, boundary-level only). Design-record note: the departed-from clause is the v040 boundary declaring livespec-overseer orthogonal to the console; the departure is deliberate and maintainer-directed -- decisions D2 and D5 of plan retire-overseer-and-redesign-control-plane-around-console (research/redesign-brainstorm-and-decisions.md, captured 2026-08-31 from the maintainer's own framing), approved for filing on 2026-09-01 (scope 'A + B + boundary-level C') and for ratification the same day. Every new behaviour carries a BCP14 clause and a Gherkin scenario (Scenario 29 and Scenario 30 new; Scenario 1 amended; Scenario 15 untouched); wire-level obligations land in contracts.md, intent in spec.md; the cross-spec citation uses the allowlisted form whose allowlist entry landed with the proposal (PR #911). The 13 new clause gap-ids and both new scenarios are registered in tests/heading-coverage.json (pending TODO with owning items named) in the same change, and the console-spec-check ground-truth counts are re-pinned (spec.md 18->22, contracts.md 133->140, total 232->243).

## Modifications

1. Placement: the new contracts.md section is a standalone '## Needs-human as a ledger valve' H2 inserted immediately before '## User Documentation Contract', not a '###' under '## Dispatcher Policy Settings' as the proposal directed -- needs-human is not a dispatcher policy setting and the heading path would have read as one. 2. Term alignment: the orphaned-factory-runs lane's last field is 'termination route (the orchestrator's name for the remedy it prescribes)' instead of 'remedy command', matching the orchestrator's ratified prose for the same field. 3. Scenario shape: Scenario 15 is left untouched; the needs-human Gherkin lands as a new '## Scenario 30 -- A needs-human terminal reaches the operator as a ledger valve' with three scenarios -- the valve / no-attach / dispatch-metadata scenario extended with the orphaned-runs lane, a Fabro-adapter status-kind scenario for the Initial Adapters clause, and a bundled-fork-terminates-at-needs_human scenario for the fork-conformance clause -- so the two obligations the review found thinly covered have Gherkin steps and Scenario 15's concrete test registration is not asked to vouch for clauses it does not exercise. Everything else lands exactly as proposed (spec.md operator question, does-not-own bullets, Control-plane surface paragraphs, plane diagram, Factory bounded context, Terminology entry; contracts.md Fabro adapter bullet; Scenario 1 wording; Scenario 29).

## Resulting Changes

- spec.md
- contracts.md
- scenarios.md

## Ratification Review

ratification_review: auto-spawn
reviewer_model: opus
reviewer_identity: opus
separate_reviewer: True
read_only: True
reviewed_at: 2026-09-01T11:14:40Z
verdict: NO BLOCKERS
proposal_stem: console-control-plane-boundary
content_digest: ace96c52aa2cd97f5d887469e4b25bc4682e32662a8509c2c9701019191f670a
