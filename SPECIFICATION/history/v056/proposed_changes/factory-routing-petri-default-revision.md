---
proposal: factory-routing-petri-default.md
decision: accept
revised_at: 2026-10-22T22:12:58Z
author_human: Chad Woolley <thewoolleyman@gmail.com>
author_llm: claude-opus-5-5
---

## Decision and Rationale

Accepted as proposed. Coordinator ruling R668 (2026-10-22), maintainer-directed ("get the fabro upgrade done and running on gmktec"), cuts the default dispatch factory over from `legacy` (hp, fork-era engine) to the Petri-era `petri` factory, as the orchestrator did in PR #3461 (bd-ib-iud52v). The v054 clause pinned `legacy` by name, so the cutover needs this revision. The registry-backed, no-machine rule is unchanged: the `petri` entry declares `"registry": true` and carries no server and no docker_host. `legacy` stays declared for recorded pins and rollback. No H2 changes, and the clause count is unchanged. Independent opus review: NO BLOCKERS. Its non-blocking notes (update the committed-table shape test, the stale .livespec.jsonc comments and the console-spec-check comment; confirm the petri entry's values name no machine) are handled in the same change.

## Resulting Changes

- constraints.md
- scenarios.md

## Ratification Review

ratification_review: auto-spawn
reviewer_model: opus
reviewer_identity: opus
separate_reviewer: True
read_only: True
reviewed_at: 2026-10-22T22:12:56Z
verdict: NO BLOCKERS
proposal_stem: factory-routing-petri-default
content_digest: 0cc490c60459f73fad6a583b20f610760715eb76ee41be5ffbafe028557411dc
