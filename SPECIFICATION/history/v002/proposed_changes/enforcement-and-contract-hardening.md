---
topic: enforcement-and-contract-hardening
author: claude-opus-4-8
created_at: 2026-06-25T05:01:32Z
spec_commitments:
  impl_followups:
    - id_hint: scenario-test-rust-checker
      description: |
        Implement, in this repo, a Rust check enforcing the clause -> scenario -> test linkage (port the discipline of livespec's dev-tooling/checks/behavior_scenario_link.py clause->scenario guardrail, the shared spec_clauses.py gap-id primitive, and the tests/heading-coverage.json link registry). Wire into just check + CI, backfill all clause/scenario/test links, and reach fail mode. Non-negotiable, highest priority.
    - id_hint: quality-gate-ci-jobs
      description: |
        Add region coverage (cargo llvm-cov --fail-under-regions 100) to check-coverage; add a CI merge-gate fuzz job (>=60s per target over event-envelope decode, adapter normalization, source-payload parsing; committed regression corpus; fail on any new crash/timeout/OOM) and a mutation job (cargo mutants --in-diff over console-domain + console-application, --test-tool nextest, bounded --timeout, fail on any surviving mutant not on the justified-survivor allow-list). High priority.
    - id_hint: nightly-soak-beads-chore
      description: |
        Add a nightly fuzz soak + full cargo mutants sweep against the canonical branch; give CI credentialed access to the work-items backend (BEADS_DOLT_PASSWORD via the family secret convention); a nightly finding MUST open a tracked work-item (chore) rather than fail the canonical branch. High priority; pre-marked ready/groomed so the factory picks it up immediately.
    - id_hint: red-green-replay-checker
      description: |
        Implement, in this repo, a Rust Red-Green-Replay commit-message enforcement check (port the discipline of livespec's dev-tooling/checks/red_green_replay.py) and wire it into the commit-msg hook + just check, so staged-phase/trailer violations are rejected. High priority.
---

## Proposal: Reconcile the event envelope with the SQLite events table

### Target specification files

- SPECIFICATION/contracts.md

### Summary

The Event Envelope and the events table disagreed on which fields exist and how they map. The table carried aggregate_id, causation_id, and a scalar correlation_id with no clear envelope source, while the envelope's subject{} and rich correlation{} object had no columns. This makes the events table a faithful 1:1 projection of the envelope.

### Motivation

Doctor LLM-driven objective finding D1: the contracts.md Event Envelope and SQLite events table were internally inconsistent, leaving the envelope-to-column mapping undefined.

### Proposed Changes

Add an optional causation_event_id to the event envelope. In the events table, add subject_kind and subject_id columns, rename correlation_id to correlation_json (storing the correlation object), document causation_id as storing the envelope causation_event_id, and define aggregate_id as the derived routing key "<subject.kind>:<subject.id>". Add an explicit envelope-to-table mapping note and update the ER diagram to match.

## Proposal: Pin the clause to scenario to test behavioral-coverage discipline

### Target specification files

- SPECIFICATION/non-functional-requirements.md
- SPECIFICATION/scenarios.md

### Summary

Add a mechanical, fail-mode rule that every normative MUST/SHOULD clause links to a Gherkin scenario and every scenario has a corresponding top-of-pyramid acceptance/integration test, enforced by a first-class Rust check in this repository.

### Motivation

The clause -> scenario -> test linkage is one of livespec's primary quality-enforcement tools and was entirely absent from the console spec; nothing guaranteed the implementation realizes the specification or that specified behavior does not silently regress.

### Proposed Changes

Add a Behavioral Coverage section to non-functional-requirements.md (## Contracts) requiring the clause -> scenario -> test chain, enforced by a first-class Rust check in this repo that ports livespec's behavior_scenario_link.py clause->scenario guardrail, the shared spec_clauses.py gap-id primitive, and the tests/heading-coverage.json link registry; run in just check + CI; with a warn -> fail severity lever whose fail-mode end state (every clause linked, every scenario tested) is a release-blocking obligation.

## Proposal: Harden Red-Green-Replay from conditional to mechanically enforced

### Target specification files

- SPECIFICATION/non-functional-requirements.md

### Summary

Make the family Red-Green-Replay commit discipline a hard MUST with mechanical enforcement, removing the 'once the repo hooks exist' conditional that left it unenforced.

### Motivation

The discipline was specified conditionally and is currently unenforced in this repo (the commit-msg hook only blocks direct commits to master), unlike the rest of livespec which enforces it via dev-tooling/checks/red_green_replay.py.

### Proposed Changes

Rewrite the Red-Green-Replay subsection to mandate mechanical enforcement via a commit-msg hook and the just check aggregate, implemented as a first-class Rust check in this repo porting livespec's red_green_replay discipline, and state that until the check is wired the requirement is unmet rather than waived.
