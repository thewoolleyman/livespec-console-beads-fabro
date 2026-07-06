---
proposal: attention-to-needs-attention-rename-and-reconciliation.md
decision: accept
revised_at: 2026-07-06T19:58:22Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Accept the reviewed SP2 proposal (independent adversarial review verdict: NO-BLOCKERS) for the cross-repo needs-attention epic (livespec-bj9x). Rename the ubiquitous-language concept `Attention` to `needs-attention` throughout, keeping the three natural-English `attention` uses. Reconcile the narrow/broad contradiction by widening the narrow "derived only from a work item" definition to the product `needs-attention` core (impl-side work-item signals, spec-side actions, and repository hygiene all arrive through the consumed `needs-attention` snapshot); subsume the Repository-Hygiene inbox edge (hygiene arrives THROUGH needs-attention) and remove the Ingestion source-health edge from the inbox (source-health/telemetry belongs to the deferred observability context). Spec-only: the snapshot port, diff adapter, and attention_item.* events are a separate downstream slice, not filed here.

## Resulting Changes

- spec.md
- contracts.md
- constraints.md
- scenarios.md
