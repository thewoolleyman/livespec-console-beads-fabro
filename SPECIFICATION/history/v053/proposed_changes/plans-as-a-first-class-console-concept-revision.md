---
proposal: plans-as-a-first-class-console-concept.md
decision: modify
revised_at: 2026-09-29T07:32:23Z
author_human: Chad Woolley <thewoolleyman@gmail.com>
author_llm: claude-fable-5-1
---

## Decision and Rationale

Accepted with the modifications above. The proposal lands the shape 1+ decision the maintainer took on 2026-09-29 (scope event on plan epic livespec-console-beads-fabro-pzbdbo) after the plan-less correction: a peer Plans view with a rank-ordered roster, a `context` drill-in, a scoped-Lanes cross-link, and a Lanes plan column that is blank for plan-less items. It defines 'plan' and 'roster' once in spec.md Terminology under the one-term ruling (the console renders 'plan', never the storage word 'epic'), names `context` and `list-plans` in the control-plane enumeration so the v046 roster clause is no longer routed to an enumeration that omits its own surfaces, and carries every new MUST in Scenario 33. The ten new contracts.md clauses are bound to Scenario 33 in tests/heading-coverage.json with a reasoned pending registration owned by livespec-console-beads-fabro-adf4nu, and the pinned clause counts move to 286 (22/160/22/82) in the same change. The verified wire gap (list-work-items drops plan_slug / next_action / last_session; context lacks last_session) is recorded in the proposal and held upstream as orchestrator bd-ib-wi5b behind console proxy livespec-console-beads-fabro-pzbdbo.40; the spec text requires those cells to render as explicitly absent until it lands, per the existing leave-un-built rule. The maintainer reviewed and merged the proposal file itself (PR #1229) before this revise pass.

## Modifications

Three blockers from the independent ratification review, plus two of its notes, applied to the resulting bytes; the proposal file itself is archived unchanged. (1) The clause 'MUST NOT offer a rendering of a plan outside the Plans view and the per-item record surface' contradicted the ratified needs-attention composition (open plan threads are attention rows), the new Lanes plan column, and Scenario 33's own never-render-epic scene; it is narrowed to 'MUST NOT offer a second plan listing or plan drill-in outside the Plans view', with every other surface that names a plan doing so without a listing or drill-in of its own, and pinned by the new scenario 'The Plans view is the only plan listing and drill-in'. (2) The positive half of the Lanes plan column had no scenario and named a field (the parent's slug) no consumed surface publishes today; it now names the parent by slug when the consumed records publish it and otherwise by the parent's id exactly as the record names it, pinned by the new scenario 'A planned work-item names its plan in Lanes'. (3) Dependency edges in the drill-in had no scenario; the drill-in scenario's Given and Then now carry two edges. Notes taken: 'from the consumed records alone' (plural, since child counts span records), and 'open plan' defined as a plan whose consumed lane is not `done`, in the contracts.md clause and the spec.md Roster entry. Clause count unchanged at ten new contracts.md clauses; six gap ids repointed in tests/heading-coverage.json.

## Resulting Changes

- contracts.md
- scenarios.md
- spec.md

## Ratification Review

ratification_review: auto-spawn
reviewer_model: opus
reviewer_identity: opus
separate_reviewer: True
read_only: True
reviewed_at: 2026-09-29T07:32:06Z
verdict: NO BLOCKERS
proposal_stem: plans-as-a-first-class-console-concept
content_digest: e128b878c3d796ee4dc723f03b452b54cc7608f2a4db5d5f098a70734bdb3394
