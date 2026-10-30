---
proposal: plans-view-consumption-contracts.md
decision: modify
revised_at: 2026-10-30T21:04:59Z
author_human: Chad Woolley <thewoolleyman@gmail.com>
author_llm: claude-opus-5-5
---

## Decision and Rationale

Accepted with modifications under delegated decision mode (maintainer direction 2026-10-30, recorded on livespec-dev-tooling-dfixhe). The following land as proposed: the Plan reads and Plan-record fields clauses, the two diagram nodes, the distinction between counts and scalars in the roster clause, the extension of the epic ban to CLI strings, the rewrite of the Plan terminology entry into consumption voice, the re-wrapped Roster entry, the new Upstream dependency definition, the amended Scenario 33 line, and both new Scenario 33 scenarios. The decision is consistent with the v053 design and with the orchestrator's published context, list-plans and list-work-items contracts. No design record is contradicted, and the proposal is not drift-origin. The pinned contracts.md clause count moves from 160 to 163. Independent sonnet ratification review: NO BLOCKERS.

## Modifications

(1) The five-paragraph split of the TUI Contract Plans paragraph (item 2(c)) is not applied, and neither is the rank-order rewording in 2(a). Item 2(c) was a readability change with no change of wording. console-spec-check derives a gap id from each line that carries MUST, so either change would have re-pointed the gap ids that open gap-tied work-items carry as labels: lndcrv (gap-6ubrnmzb), raq2z5 (gap-3wqidxgd), e8zv (gap-hzzcuwgn), f6adnx (gap-4f4djnpn) and zzs2ig (gap-jdhsm2e6). The kept rank wording matches the Roster entry, which the proposal itself leaves unchanged. The rewording of 2(a) that separates counts from scalars and the 2(b) CLI extension are applied, re-wrapped so that every unchanged MUST line keeps its exact bytes. Only gap-z3bfxje2 moves, to gap-j7vmsvil. (2) The two new adapter paragraphs are indented continuation paragraphs of the Work-items adapter bullet. (3) The spec.md citation uses the orchestrator's exact heading, Plan identity: `plan_slug` and the `associated_work_item_id` anchor, and that heading is allowlisted in .livespec.jsonc external_references. (4) The Scenario 33 scenario that bans the storage word now also names CLI usage, help and report strings, so it stays in step with the extended ban. (5) The three new contracts.md clauses (gap-tmh23rb3, gap-qgyt3zta, gap-jlb3dzlj) are bound to the Scenario 33 coverage row, and the row's reason names the coverage still owed.

## Resulting Changes

- spec.md
- contracts.md
- scenarios.md
- ../tests/heading-coverage.json

## Ratification Review

ratification_review: auto-spawn
reviewer_model: sonnet
reviewer_identity: sonnet
separate_reviewer: True
read_only: True
reviewed_at: 2026-10-30T21:03:57Z
verdict: NO BLOCKERS
proposal_stem: plans-view-consumption-contracts
content_digest: 78ce5d71058ceafa0ef3d60967890b8a409b098922724f603c49e30972f735d6
