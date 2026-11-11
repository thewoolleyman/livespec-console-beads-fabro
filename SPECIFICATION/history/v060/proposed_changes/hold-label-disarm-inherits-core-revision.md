---
proposal: hold-label-disarm-inherits-core.md
decision: modify
revised_at: 2026-11-11T09:11:06Z
author_human: Chad Woolley <thewoolleyman@gmail.com>
author_llm: claude-opus-5-5
---

## Decision and Rationale

Modified under delegated revise authority (revise_decision_mode: delegated; legacy-coordinator-agent R301 standing grade, the same authority the four sibling members used, --only-topic hold-label-disarm-inherits-core). The clause binds this repository, which carries auto-enable-merge.yml, to carry disarm-on-hold-label.yml byte-identical to livespec's root copy, citing livespec non-functional-requirements.md §"Shared content sync — copier template" (ratified in livespec v240, livespec#3163) rather than restating the behaviour, matching livespec-runtime v034. Byte-identical holds because console's auto-enable-merge.yml mints with client-id:, the same input key as livespec's root copy (verified by the reviewer). Contributor Scenario M is added because this repository's ### Behavioral Coverage links every normative clause to a scenario.

## Modifications

Placement corrected: the proposal said the subsection goes under ## Contracts after ### Delegated Pre-Push Gate, but ### Delegated Pre-Push Gate sits under ## Constraints. The subsection therefore lands under ## Constraints, directly after ### Delegated Pre-Push Gate and before ## Scenarios. The section citation is kept on one line. The proposal's sentence 'The clause links to ## Contributor Scenario M ...' is dropped from the spec text, because this repository records clause-to-scenario linkage in tests/heading-coverage.json (row for Contributor Scenario M, clause gap-ajl74sxc), not in prose. Scenario M is otherwise as proposed.

## Resulting Changes

- non-functional-requirements.md

## Ratification Review

ratification_review: auto-spawn
reviewer_model: sonnet
reviewer_identity: sonnet
separate_reviewer: True
read_only: True
reviewed_at: 2026-11-11T09:10:15Z
verdict: NO BLOCKERS
proposal_stem: hold-label-disarm-inherits-core
content_digest: 2d2a0a28e719d289513a905e4dab8f69cce7eee21d9a3acd7da7445082722d8c
