---
proposal: design-record-archived-path.md
decision: accept
revised_at: 2026-07-20T04:12:27Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Independent Fable review returned NO BLOCKERS for this proposal. Verified: the replace-target exists verbatim exactly once as a whole line at spec.md:340; the destination plan/archive/autonomous-mode/handoff.md exists with the cited content byte-identical to the pre-archive original; the affected line carries no whole-word MUST/SHOULD so console-spec-check's hardcoded ground truth (spec.md=15, total 166) is unperturbed and gap-ids are unaffected; no heading changes so no tests/heading-coverage.json co-edit is owed; and this is one of exactly three live-spec citations of that path fleet-wide, the other two repointed in the sibling orchestrator repo in the same window. The payload asserted the target unique and asserted the post-state carries zero old-path and exactly one new-path reference.

## Resulting Changes

- spec.md
