---
proposal: hint-honesty-scope-and-whole-record.md
decision: accept
revised_at: 2026-07-19T11:48:03Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Accept. Corrects two defects the v031 record drill-in shipped, both found by independent adversarial review of the implementing branch BEFORE merge -- and both are the same failure v031 set out to fix, committed by the fix itself. (1) The v031 hint-honesty sentence ('a key that is inert there MUST NOT be listed') was violated at ship time by the very hints v031 edited: the lane OVERVIEW advertised `s move-status`, `p/c/r approve/accept/reject`, and `m/n set-admission/acceptance`, all six of which act only on a selected work-item while the overview's selection is a LANE, so all six did nothing; and an empty drilled-in lane advertised the newly-added `enter item` where Enter opens nothing. Scenario 23's case asserting 'no hint advertises a key that is inert in that context' passed anyway, because its test only checked the `enter` strings -- a test proving less than the sentence it was registered against. The rule is KEPT and the implementation fixed (hints are now selection-aware); what changes is that the clause draws its line where the console can hold it, and says outright that per-item-STATE suppression (hiding a status move a `done` item cannot be driven through) is NOT promised, because it depends on the valid-verb vocabulary owned by livespec-orchestrator-beads-fabro that this console does not yet consume. Stating the exclusion beats leaving a spec sentence the implementation quietly fails. (2) The v031 record clause says the surface MUST render every field of the standardized shape, but five fields the orchestrator emits on EVERY record (acceptance_criteria, notes, supersedes, blocked_reason, factory_safety) were neither parsed nor rendered, and three more (lane_reason, admission_policy, acceptance_policy) were available but unrendered. This was live data loss, not a hypothetical: the tenant's own livespec-console-beads-fabro-vfd carries a long non-null `notes` the modal hid with no placeholder -- the exact 'undisplayed masquerading as unset' the clause forbids. No clause text changes for this half; the implementation is brought up to the clause, the five fields join the record digest so edits to them reach the operator, and the descriptive half is now read through TOTAL helpers so no field shape can drop a work-item from the board. Clause counts are UNCHANGED at 15/77/22/52 = 166, so console-spec-check ground truth is untouched; one re-link is owed because the Status-line clause text changes again (gap-iicnbdqd -> gap-7heyl2dr, bound in exactly one entry, derived over the final landed text). RATIFICATION PROVENANCE, as for v031: the maintainer delegated the human acceptance leg for work-item mwzrby to the agent, with two independent LLM reviews (Fable adversarial, Codex) standing in for the human review. No human read this text before ratification. `just check` is green end to end, including 100% line coverage.

## Resulting Changes

- contracts.md
- scenarios.md
