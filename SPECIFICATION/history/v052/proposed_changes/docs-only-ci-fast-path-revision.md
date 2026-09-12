---
proposal: docs-only-ci-fast-path.md
decision: modify
revised_at: 2026-09-10T18:51:48Z
author_human: Chad Woolley <thewoolleyman@gmail.com>
author_llm: gpt-5.6-sol
---

## Decision and Rationale

The optimization is warranted, but the proposal is narrowed to paths proven independent of Rust lockstep and completeness tests, regular-file content changes only, exact lightweight recipes, and lane-aware fail-closed aggregation. Contributor behavior remains in the existing non-functional-requirements scenario partition rather than duplicating it in the operator scenario file.

## Modifications

Narrow the allow-list to CHANGELOG.md, Markdown beneath .ai/ and plan/, and docs/doc-custody.md or docs/factory-confirmations.md; reject type, mode, symlink, submodule, unknown, empty, or unreadable comparisons to the full gate; name every lightweight recipe; make ci-green validate the expected skipped/success state for the classifier-selected lane; and keep all Gherkin changes in Contributor Scenario C.

## Resulting Changes

- non-functional-requirements.md

## Ratification Review

ratification_review: auto-spawn
reviewer_model: opus
reviewer_identity: opus
separate_reviewer: True
read_only: True
reviewed_at: 2026-09-10T18:51:22Z
verdict: NO BLOCKERS
proposal_stem: docs-only-ci-fast-path
content_digest: 7ea53f1bf05e92c219710581513424c689919ca124a1126d7ea4257f37ab1ed8
