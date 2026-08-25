---
proposal: console-consumer-tolerance-identity-and-settings-enumeration.md
decision: modify
revised_at: 2026-08-25T16:38:02Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-5
---

## Decision and Rationale

Accepts, with the modifications recorded below, the ONE bundled propose-change of livespec-console-beads-fabro-2fr7hy (plan epic livespec-console-beads-fabro-ddfbcx) -- the spec carrier for charge legs 2, 6, and 7. All three clauses adopt upstream contracts that are ratified AND released, each verified by release tag in plan/homelab-loop-hardening-console/research/002-deferral-keys-fired.md: the consumer-tolerance posture (orchestrator v077, released v0.72.8), the journal-invoker resolution order (orchestrator v073, released v0.72.4), and the drift_capture_merge_threshold API-configurable declaration (orchestrator v078, released v0.72.9). No design record is contradicted: the adapter-honesty narrowing RESOLVES an ambiguity the tree already carried -- 'an uninterpretable payload' never said whether one bad item made the payload uninterpretable -- rather than reversing a decision, and the settings work removes frozen enumerations that already contradicted the binding read-the-declaration clause beside them. The new Gherkin scenarios were placed under the existing Scenario 12 and Scenario 2 headings deliberately: the behavioral-coverage gate binds a registered test per scenario HEADING, so a new heading would be untested until its code leg lands and would break the gate on a spec-only change. Verified before filing: 0 untested scenarios, 20 unlinked clauses all mapped to existing tested headings, ground truth 213 -> 230 (18/133/22/57). Decided under revise_decision_mode: delegated per this repo's armed spec_governance, with independent auto-spawn ratification review returning NO BLOCKERS for these exact bytes only after two rounds of substantive blockers. The reviewer was a separate, read-only agent running the configured fable model, addressed in-session as ratification-reviewer-v043; the evidence records reviewer_identity as `fable` because the revise CLI requires identity and model to match.

## Modifications

Accepted WITH MODIFICATIONS forced by independent ratification review, which returned BLOCKERS twice before passing. The proposal's three clauses are unchanged in substance; what changed is their REACH.

The proposal de-enumerated ONE frozen settings count, in contracts.md's Dispatcher Policy Settings section. Review round 1 established that ratifying only that one would write into the tree the exact contradiction the proposal exists to remove, because three further frozen counts survived elsewhere:
  (a) contracts.md, eight lines above the rewritten clause: '.livespec.jsonc for the six global defaults' -> 'for the global defaults'.
  (b) spec.md Dispatcher Policy Settings: 'governed by six orchestrator-owned dispatcher.* policy settings -- <six keys> (the ratified set: ...)' -> the set is now introduced as illustrative and growth-bearing, and the parenthetical's lead-in changed from 'the ratified set' to 'the owning declaration', because naming the enumeration 'the ratified set' was itself part of the staleness.
  (c) spec.md: 'Five of the six settings admit a per-item override' -> 'A dispatcher setting admits a per-item override unless the orchestrator declares otherwise', keeping the wip_cap prohibition verbatim and adding that the console MUST take which settings admit an override from the orchestrator's published declaration rather than from a count fixed here.
This ADDED SPECIFICATION/spec.md to the change set; it was not among the proposal's target files.

Review round 2 found two more of the same defect class, in forms the round-1 sweep missed because they were not the string 'six':
  (d) scenarios.md Scenario 10: 'Given the five overridable settings are ...' (lowercase number word) and 'And the remaining three settings are served by ...' -> the set is now 'the orchestrator's overridable dispatcher settings, which at the time of writing are ...' and 'every other overridable setting is served by ...'. This one directly contradicted clause (c), which had just been added.
  (e) contracts.md: '...this command therefore serves merge_on_review_cap, review_fix_cap, and acceptance_rework_cap' -- a closed list forming a complete partition with no illustrative marker -> 'serves every other overridable key -- at the time of writing <those three>'.
Round 2 also noted, non-blocking, that 'wip_cap is the standing exception' read as singular while two other keys are non-overridable by declaration; changed to 'a standing exception'.

One further instance was found by this session's own tree-wide sweep for number-words adjacent to setting/key/override/dispatcher, not by the reviewer:
  (f) contracts.md's EIGHTH-command paragraph: 'it fans out to the orchestrator's three per-setting override actions' -> 'to the orchestrator's per-setting override actions'. Same class: a frozen count of the same growing upstream set, which goes false when a fourth overridable cap is ratified.

Deliberately NOT changed, and recorded so a later reader does not read the omission as an oversight: scenarios.md Scenario 10's closing 'the Work-item Lifecycle vocabulary is therefore eight commands, seven of them mapping 1:1' (console-owned and structurally stable -- a new overridable key is absorbed by the generic set_dispatcher_override_requested command, so the command count does not move when the orchestrator's key set grows); contracts.md 'in lockstep, in three places' (structural: row, inline help, settings doc); scenarios.md 'the two human valves'; and contracts.md 'differs between two contexts' (generic phrasing, not an enumeration).

Mechanical note carried for future editors: the behavioral-coverage gate derives each clause id from the hard-wrapped SOURCE LINE, so reflowing a MUST-bearing line silently re-keys that clause and orphans its heading-coverage link. Edit (e) was rewrapped specifically to leave every MUST line untouched, and the unlinked-clause id set was verified byte-identical before and after.

## Resulting Changes

- contracts.md
- scenarios.md
- spec.md

## Ratification Review

ratification_review: auto-spawn
reviewer_model: fable
reviewer_identity: fable
separate_reviewer: True
read_only: True
reviewed_at: 2026-08-25T16:33:08Z
verdict: NO BLOCKERS
proposal_stem: console-consumer-tolerance-identity-and-settings-enumeration
content_digest: 75fd8c74c8da7ad7bf779a38b1af73ede7059846267423edc031b4d18ef85a04
