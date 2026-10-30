---
proposal: ci-run-supersession.md
decision: modify
revised_at: 2026-10-30T21:04:59Z
author_human: Chad Woolley <thewoolleyman@gmail.com>
author_llm: claude-opus-5-5
---

## Decision and Rationale

Accepted with a placement modification under delegated decision mode (maintainer direction 2026-10-30, recorded on livespec-dev-tooling-dfixhe). Work-item livespec-console-beads-fabro-s3kwxt already landed the asymmetric concurrency block and its header comment in .github/workflows/ci.yml; this records the requirement so a rewrite of the workflow cannot silently drop it. The clause text, both flowchart nodes and both Contributor Scenario C gherkin scenarios land verbatim. No design record is contradicted, and the proposal is not drift-origin: it was filed alongside the implementation, which it describes and does not correct. The pinned console-spec-check clause count for non-functional-requirements.md moves from 102 to 107. Independent sonnet ratification review: NO BLOCKERS.

## Modifications

Placement only. The proposal asked for the clause to be inserted directly after the Merge gate bullet list and before the Nightly paragraph. The Code-neutral documentation lane block, which was ratified after the proposal was written, now sits between those two anchors, so both conditions cannot hold at once. The clause is inserted directly after the Merge gate bullet list, and it still precedes the Nightly paragraph. Its five new gap ids (gap-rwxvnwjz, gap-x2o7ji3f, gap-xhgntr67, gap-4dg5pqrv, gap-hmzz26a2) are bound to the existing Contributor Scenario C coverage row. The row's reason states that the ci.yml concurrency block realizes them and that no listed test yet asserts that block.

## Resulting Changes

- non-functional-requirements.md
- ../tests/heading-coverage.json

## Ratification Review

ratification_review: auto-spawn
reviewer_model: sonnet
reviewer_identity: sonnet
separate_reviewer: True
read_only: True
reviewed_at: 2026-10-30T21:03:57Z
verdict: NO BLOCKERS
proposal_stem: ci-run-supersession
content_digest: 9eba922397487362c2aefc38d8e636ea4db330eea47e38898c0b2a7422fe6b03
