---
proposal: nfr-contributor-scenarios.md
decision: accept
revised_at: 2026-07-01T06:43:49Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Accepted as-filed. The proposal is correctly routed spec-side and unblocks the release-blocking behavioral-coverage checker (console-spec-check): it replaces the NFR §Scenarios placeholder with contributor-facing Gherkin so the gate can reach fail mode. Placement respects all three authoring splits — contributor-observable behavior binds to the NFR §Scenarios H2s (not scenarios.md), the content is console-specific and stays in the console repo, and each behavior is a Gherkin scenario. Nine theme H2s (Contributor Scenario A–I) were authored that together cover all 52 NFR contributor-facing normative clauses, verified against the spec_clauses extraction rule (clause count unchanged at 52; every clause-bearing section maps to a live H2). Per the proposal's own division, the tests/heading-coverage.json clause->scenario links and the top-of-pyramid tests remain the impl-side B-nfr slice, so this spec change touches only non-functional-requirements.md.

## Resulting Changes

- non-functional-requirements.md
