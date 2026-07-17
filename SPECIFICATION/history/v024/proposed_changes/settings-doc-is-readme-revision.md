---
proposal: settings-doc-is-readme.md
decision: accept
revised_at: 2026-07-17T09:18:09Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

The ratified W5 decision is that the console README IS the settings doc; there is no docs/ dir. Correct the three stale docs/settings.md references (the Settings-surface completeness MUST clause + Scenario 14's mermaid node and gherkin Given) to README.md so the spec names the real doc the W6 completeness check reads. Same-clause reword: net zero new clauses; the completeness MUST clause's gap-id re-derives gap-yfezurch -> gap-3dyfo5pk (verified by the extractor), rebound in Scenario 14's clauses[]; the tests.rs clause-count ledger stays 15/57/22/52=146 (console-spec-check tests pass unchanged).

## Resulting Changes

- contracts.md
- scenarios.md
- ../tests/heading-coverage.json
