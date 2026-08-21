---
proposal: per-item-factory-dispatch-launcher-argv.md
decision: modify
revised_at: 2026-08-21T03:03:34Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-5
---

## Decision and Rationale

MODIFIED rather than accepted, on independent-review evidence that refuted the proposal's own core argument. The proposal chose `dispatch --item` over `loop --item` and justified it on the ground that `loop`'s behaviour over a single-element set 'would be a further thing to specify'. That is false: the orchestrator's ratified SPECIFICATION/contracts.md section 'Dispatcher loop invocation surface' fully specifies the repeatable `--item` narrowing, and its own per-item `drive impl:<id>` action already takes the bounded-loop path. Three further facts, each verified against the sibling spec AND its source by independent reviewers: the orchestrator publishes NO governed grammar for `dispatch`, so binding to it would pin the console to a surface that may change without any contract being violated; `--item` NARROWS and never bypasses eligibility, so the proposal's cap-bypass disclosure described a property of the wrong surface; and the presence of `--item` is the orchestrator's marker that a human hand-picked the dispatch, with a fail-closed cost gate keyed on it. Ratifying the proposal as written would have specified a route around that gate as though it were a feature. The ratified text therefore binds the governed bounded-loop surface instead.

## Modifications

1. ARGV CHANGED from `dispatch --repo <path> --item <id>` to the governed `loop --repo <path> --budget 1 --parallel 1 --item <id>`, with an explicit MUST NOT bind `dispatch`, and repo-qualified citations to the orchestrator's 'Dispatcher loop invocation surface' and 'The skill surface'.
2. WIP-CAP BYPASS DISCLOSURE REMOVED and replaced by its opposite: a per-item dispatch NARROWS the ranked selection and MUST NOT bypass it; a named ineligible item is not dispatched exactly as if unnamed; the console MUST surface that no-dispatch outcome truthfully; the per-repo WIP cap governs the per-item path as it governs a drain. The removed disclosure was true only of the ungoverned surface the console no longer binds.
3. NEW CLAUSE: passing `--item` is load-bearing beyond selection because the orchestrator's fail-closed cost gate keys on its presence, so the console MUST express per-item dispatch by passing `--item` and MUST NOT emulate it by other means.
4. Scenario 28 rewritten to match: governed bounded-loop invocation, the named-ineligible no-dispatch path, and the unconditional-not-wired non-conformance case.
5. Proposal part 2 (a not-wired stub is not conformant, and an unperformable verb MUST NOT render as available) carried through unchanged.

## Resulting Changes

- contracts.md
- scenarios.md

## Ratification Review

ratification_review: auto-spawn
reviewer_model: fable
reviewer_identity: fable
separate_reviewer: True
read_only: True
reviewed_at: 2026-08-21T03:03:04Z
verdict: NO BLOCKERS
proposal_stem: per-item-factory-dispatch-launcher-argv
content_digest: 2b91594940be72015783c323dcbc501181cd7621b0eb5c7871fed7fe091ff0cc
