---
proposal: attention-item-sourced-tui-render-path.md
decision: accept
revised_at: 2026-08-25T12:22:30Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-fable-5
---

## Decision and Rationale

Accepts the spec-first leg of livespec-console-beads-fabro-cddfxl (plan epic livespec-console-beads-fabro-ddfbcx): Scenario 5 re-sourced to the ingested attention_item.* stream and the needs-attention adapter's emissions made exclusive. This completes the ratified v016/CN1 re-sourcing direction - the adapter contract already maps this source to attention_item.* events only - so no design record is contradicted; the CN1 design record is the direction being completed. Decided under revise_decision_mode: delegated per the homelab maintainer's standing in-session-revise directive (recorded on homelab/hl-nkuzaz; exercised and recorded on the plan epic), with independent auto-spawn ratification review returning NO BLOCKERS for these exact bytes.

## Resulting Changes

- scenarios.md
- contracts.md

## Ratification Review

ratification_review: auto-spawn
reviewer_model: fable
reviewer_identity: fable
separate_reviewer: True
read_only: True
reviewed_at: 2026-08-25T12:22:08Z
verdict: NO BLOCKERS
proposal_stem: attention-item-sourced-tui-render-path
content_digest: 7e1bc260ad57341429bbbde861dbb098607264697a07b1ac3d15a1c83d44655b
