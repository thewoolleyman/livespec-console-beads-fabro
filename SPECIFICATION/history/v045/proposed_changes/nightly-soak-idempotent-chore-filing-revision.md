---
proposal: nightly-soak-idempotent-chore-filing.md
decision: accept
revised_at: 2026-09-01T01:41:09Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Accept the nightly-soak idempotent-chore-filing proposal (maintainer-directed). Augments the Nightly clause so filing is idempotent: derive a stable finding signature (fuzz: reproducer/backtrace hash; mutant: (file,line,mutation-operator)), persist it, and skip filing when an open chore for that signature already exists. Records the accepted staleness trade-off (open chore may be stale; fine because the soak is expensive and a fix re-runs it manually). All must-stay-unchanged parts intact. New clauses linked to Contributor Scenario C in tests/heading-coverage.json (co-edited); check-behavior-coverage green. Independent read-only opus reviewer: NO BLOCKERS over the exact bytes.

## Resulting Changes

- non-functional-requirements.md

## Ratification Review

ratification_review: auto-spawn
reviewer_model: opus
reviewer_identity: opus
separate_reviewer: True
read_only: True
reviewed_at: 2026-09-01T01:39:08Z
verdict: NO BLOCKERS
proposal_stem: nightly-soak-idempotent-chore-filing
content_digest: a04ad69adebe7f71c92360911d77e305530eb23ff11b7d7ce8e29feeee13f66f
