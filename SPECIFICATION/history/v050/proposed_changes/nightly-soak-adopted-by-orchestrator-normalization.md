---
topic: nightly-soak-adopted-by-orchestrator-normalization
author: claude-sonnet-5
created_at: 2026-09-09T20:22:39Z
---

## Proposal: Nightly-soak filing consumes the orchestrator's adoption+rank primitive instead of asserting a shadow-ledger gap

### Target specification files

- SPECIFICATION/non-functional-requirements.md

### Summary

Retract the shadow-ledger negative in the Nightly quality-gate clause ("nothing adopts a ci-soak-finding item into backlog or ranks it ... a known gap that this specification does NOT close") and replace it with a CONSUME clause: the filed item still lands at beads `open`, outside the lifecycle and unranked AT THE FILING STEP, but the orchestrator's Dispatcher ledger normalization adopts every beads-native `open` row into `backlog` and assigns it a real, non-sentinel rank -- an orchestrator primitive this specification cites and consumes, never asserts. The "top of the rank order" placement obligation stays removed (charter D2: the console never asserts an orchestrator primitive). Contributor Scenario C's mermaid Chore node and gherkin scenario are co-edited to match.

### Motivation

Work-item livespec-console-beads-fabro-4jb3kl.8, HELD behind upstream proxy -4jb3kl.1 (BLOCKED-ON orchestrator bd-ib-3nlq). The proxy closed 2026-09-09: orchestrator bd-ib-3nlq ratified and shipped the ledger-normalization adoption+rank primitive (bd-ib-0026 merged orch PR #2414, released v0.146.0, verified on orchestrator and the console consumer). The v048 nightly-soak-ssh-ingress-filing revision deliberately recorded the missing adoption+rank as a live, tracked gap -- status-in-a-spec that is now false and must be retracted per NO-SHADOW-LEDGER discipline, replaced with a clause that consumes the now-ratified upstream primitive by prose citation rather than reintroducing a console-asserted placement obligation.

### Proposed Changes

In `SPECIFICATION/non-functional-requirements.md` §Quality Gate, in the **Nightly** paragraph:

RETRACT the sentence "This clause deliberately asserts NO transition out of that state. Nothing in the ingress, in this specification, or in the orchestrator's ratified work-item state semantics (repo `thewoolleyman/livespec-orchestrator-beads-fabro`, `SPECIFICATION/contracts.md`, its Work-item state semantics section) adopts a `ci-soak-finding` item into `backlog` or ranks it, and a placement obligation no actor performs would be a false requirement." and the trailing sentence "Adopting such an item into the orchestrator's lifecycle -- and ranking it once adopted -- is a known gap that this specification does NOT close; it is tracked as live work in this repo's work-items ledger rather than asserted here."

REPLACE them with:

    That landing does not rest there. The orchestrator's Dispatcher ledger
    normalization adopts every beads-native `open` row into `backlog` and
    assigns it a real, non-sentinel rank (repo
    `thewoolleyman/livespec-orchestrator-beads-fabro`,
    `SPECIFICATION/contracts.md`, section "Work-item beads-issue mapping",
    sub-bullet "Adoption of a row a non-lifecycle writer left `open`
    assigns it a real rank"). That adoption is an orchestrator primitive:
    it runs console-unaware and label-unaware, treating a
    `ci-soak-finding` item exactly as it would any other non-lifecycle-
    writer `open` row. This specification CONSUMES that adoption and
    asserts no placement, transition, or rank of its own -- it restores
    none of the "top of the rank order" placement obligation this clause
    previously removed, because expressing rank is still something no
    actor in this ingress performs, and an orchestrator primitive stays
    the orchestrator's to assert, never this one's.

Also append ", AT THE FILING STEP" to the preceding sentence's "...OUTSIDE the orchestrator's work-item lifecycle states, and unranked." (now reading "...and unranked -- AT THE FILING STEP."), and append " -- the orchestrator's adoption lands it at `backlog`, never `ready`." to the existing "A nightly finding MUST NOT be filed into `ready`." sentence (both retained MUST sentences keep their original line-wrap byte-for-byte, so no gap-id in tests/heading-coverage.json's Contributor-Scenario-C clause list changes).

In the Contributor Scenario C mermaid flowchart, change the `Chore` node text from `"finding -> chore filed unranked at beads open; never fail master"` to `"finding -> chore filed unranked at beads open (adopted+ranked by orchestrator); never fail master"`.

In the Contributor Scenario C gherkin, replace the scenario's final step `And the item lands unranked at the beads default open status, outside the orchestrator lifecycle states, with no transition out of it asserted here` with:

      And the item lands unranked at the beads default open status,
        outside the orchestrator lifecycle states, at the filing step
      And the orchestrator's own ledger normalization -- not this
        console -- later adopts the item into backlog and assigns it a
        real rank

Revision note (implementation coupling, not spec text): this is a net-zero-count edit to the pinned clause counts in `crates/console-spec-check/src/tests.rs` (`extract_rules_matches_real_spec_ground_truth`, 261 total / 67 for non-functional-requirements.md) -- both retained MUST lines keep their original wrap, so no count or gap-id updates are needed; the revision SHOULD still append a dated commentary paragraph to that test documenting the retraction, per the running per-revision convention.
