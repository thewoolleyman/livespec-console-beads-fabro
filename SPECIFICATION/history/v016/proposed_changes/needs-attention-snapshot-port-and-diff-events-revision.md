---
proposal: needs-attention-snapshot-port-and-diff-events.md
decision: accept
revised_at: 2026-07-07T00:30:46Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Straddle verdict + independent CODEX review (NO-BLOCKERS on the amended proposal). Adds the console contract surface SP2/v015 deferred as a downstream slice: the product needs-attention snapshot-source port, the diff-at-ingest adapter, and the attention_item.appeared/.changed/.resolved events (keyed by stable id, idempotent). Folds in SP2 follow-up 1 (open plan/<topic> threads), re-casts Scenario 1 so the inbox is consumed from the product snapshot rather than recomposed console-side, adds Scenario 12, and co-edits the heading-coverage map. Ratification authorized by the maintainer (2026-07-07).

## Resulting Changes

- spec.md
- contracts.md
- scenarios.md
- ../tests/heading-coverage.json
