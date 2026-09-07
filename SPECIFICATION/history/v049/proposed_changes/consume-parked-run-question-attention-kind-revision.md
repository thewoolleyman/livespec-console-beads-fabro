---
proposal: consume-parked-run-question-attention-kind.md
decision: accept
revised_at: 2026-09-07T12:57:30Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-fable-5-1
---

## Decision and Rationale

ACCEPTED on the maintainer's explicit authorization (2026-09-07: "I authorize the revise, get it done"), under delegated decision mode with the independent auto-spawned ratification review returning NO BLOCKERS for the exact final bytes. The amendment is the console-owned consume of orchestrator b3, which landed on 2026-09-06 as two slices: b3.S1 (bd-ib-aqith2, orchestrator PR #2168) enriches the resolve-blocked valve item for a work-item at blocked/needs-human with the terminated run's own account, and b3.S2 (bd-ib-uuohty, orchestrator PR #2200) lets drive --action resolve-blocked carry an operator --answer that the orchestrator poison-preflights and writes as the livespec-human-answer ledger comment before the transition. Three contracts.md edits land verbatim from the proposal: the account is rendered verbatim on the inbox row and in the detail and is never composed, augmented, or re-derived by the console (no factory reads, no run reads, no event scanning); the resolve-blocked dialog offers an optional free-text answer that rides the persisted work_item.resolve_blocked_requested command as the action's --answer argument, with the orchestrator owning the comment write and the console surfacing a refusal verbatim; and the action-id mapping clause records the optional answer payload field and the appended --answer argument. Scenario 32 binds all nine new MUST clauses (gap-lijw44ol, gap-py5j3745, gap-auzn5k4s, gap-rnchi3mj, gap-addrybqf, gap-qmy2qi5l, gap-kpwru6qi, gap-yuvnoiwj, gap-dovqe6ig) with four Gherkin scenarios. No new attention kind, no attach or resume route, and the never-work-around-upstream rule is preserved: the console consumes the projection and invokes the published action surface only. Co-edits, atomic and mandatory: tests/heading-coverage.json registers Scenario 32 as a Test-tier TODO carrying the nine gap ids (0 unlinked clauses, 0 untested scenarios; 14 pending registrations, all reasoned), and crates/console-spec-check/src/tests.rs pins contracts.md 142 -> 150 and the total 253 -> 261 (26 console-spec-check tests passing). The impl follow-up named in the proposal front matter, render-needs-human-account-and-answer-valve, is filed as a console work-item under livespec-console-beads-fabro-pzbdbo after this ratification and flips the TODO to landed tests.

## Resulting Changes

- contracts.md
- scenarios.md

## Ratification Review

ratification_review: auto-spawn
reviewer_model: opus
reviewer_identity: opus
separate_reviewer: True
read_only: True
reviewed_at: 2026-09-07T12:56:36Z
verdict: NO BLOCKERS
proposal_stem: consume-parked-run-question-attention-kind
content_digest: 533f862458bc2785230fce9f3139fe83356e63c94d942c5cfad361ee67525ca6
