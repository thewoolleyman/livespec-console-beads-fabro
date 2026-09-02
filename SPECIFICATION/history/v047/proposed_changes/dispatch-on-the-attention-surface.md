---
topic: dispatch-on-the-attention-surface
author: claude-fable-5-1
created_at: 2026-09-02T04:33:01Z
---

## Proposal: Per-item verbs act where the operator looks: needs-attention surface parity

### Target specification files

- SPECIFICATION/contracts.md
- SPECIFICATION/scenarios.md

### Summary

Add a TUI-contract clause and a new Scenario 31 requiring that a needs-attention row backed by a known work-item id offers every per-item verb the item's lifecycle state admits (per-item dispatch, move-to-status, driver handoff, human valves) exactly as the drilled-in lane does, with identical availability predicates, dialogs and command envelopes; the hosting view is never an input to availability. Rows with no work-item id offer nothing, unchanged. Scenarios 17, 25 and 27 keep their drilled-in-lane premises and are not amended.

### Motivation

Maintainer ruling 2026-09-02 on plan retire-overseer-and-redesign-control-plane-around-console (epic livespec-console-beads-fabro-pzbdbo, scope event of the same day): cut the thin console-local 'usable drive loop' slice now and hold everything that needs an orchestrator primitive. The evidence is the maintainer's own first TUI dogfooding session of 2026-08-31 (verbatim transcript: https://github.com/thewoolleyman/livespec-console-beads-fabro/blob/master/plan/retire-overseer-and-redesign-control-plane-around-console/research/livespec-console-beads-fabro-dogfooding-session-transcript.jsonl). The item that needed the operator appeared in the needs-attention inbox; 'dispatch' there was presented 'not available', which the maintainer called 'the critical blocker'. The verified explanation was that per-item dispatch, move and driver handoff are all withheld on the inbox surface by construction (the registry's surface split, pinned by a registry test), so the recipe for driving one item was: leave the inbox, re-find the same item in Lanes, drill in, press `s`, confirm, walk to the Ready lane to look, then `v`, Factory, Dispatch, pick, confirm -- because the one verb the factory exists for has no key. The maintainer's verdict on that recipe: 'THat's a horrible UX'. This proposal changes WHERE state-admitted per-item verbs are offered and gives per-item dispatch a key; it does not touch the orchestrator-owned per-state verb vocabulary, the governed launcher argv, or the menu-primacy rule of Scenario 27 (the menu path stays; the key is additional). Groom and consent inside the TUI are explicitly deferred to orchestrator b3/b4 by the scope event. Finding 2 of the dogfooding session, verbatim from the session's own diagnosis: 'You can't act where you look. The item that needs you shows up in Attention, but dispatch (and move, and handoff) are all disabled there.'

### Proposed Changes

In `contracts.md`, directly AFTER the TUI-contract paragraph that begins "The TUI MUST let the operator read a selected work-item's FULL standardized record without leaving the console", insert a new paragraph:

    **Per-item verb surface parity.** A needs-attention row that carries a known work-item id resolves the SAME standardized work-item record that a drilled-in lane selection resolves, so the console MUST offer on that row -- the needs-attention row and the drilled-in lane selection being the two per-item surfaces -- every per-item verb the selected item's lifecycle state admits under the per-state operator verb vocabulary -- the per-item factory dispatch, the move-to-status picker, the driver handoff, and the human valves alike -- exactly as it offers them in the drilled-in lane: the same availability predicates, the same confirmation dialogs, and the same persisted command envelopes. The surface the operator is looking at MUST NOT be the reason a state-admitted verb is withheld. The only inputs to a per-item verb's availability are the consumed work-item record (lane, admission and acceptance policy, factory-safety marker, scope-override flag) and the board facts the verb itself names (the ready-work count for the drain); which view hosts the selection is not one of them. A row that is not backed by a known work-item id (for example a source-unavailability row or a repository-health row) offers no per-item verb, and the honesty rule of the Status-line hints clause above applies unchanged. This clause exists because of a measured failure: on 2026-08-31 the item that needed the operator appeared in the inbox, every action on it was presented unavailable there, and the operator had to leave the inbox, re-find the same item in the Lanes view, and drill in before any verb lit up.

In `scenarios.md`, append a new scenario after Scenario 30:

    ## Scenario 31 -- Per-item verbs are offered wherever the operator sees the work-item

    ```mermaid
    flowchart LR
      Inbox["needs-attention row (known work-item id)"]
      Drill["Drilled-in lane selection"]
      Record["One standardized work-item record"]
      Vocab["Per-state verb vocabulary (orchestrator-owned)"]
      Verbs["Same per-item verbs, same dialogs, same commands"]

      Inbox --> Record
      Drill --> Record
      Record --> Vocab --> Verbs
    ```

    ```gherkin
    Feature: The surface never withholds a state-admitted per-item verb
      As a LiveSpec operator
      I want to act on a work-item where I see it
      So that the inbox that shows me what needs me is also where I can do it

    Scenario: A ready item in the inbox offers per-item dispatch
      Given a needs-attention row backed by a `ready` work-item
      When the operator views the per-item verbs for that row
      Then the per-item dispatch verb is offered
      And confirming it persists the same `factory.dispatch_item_requested` command for that work-item that a drilled-in ready-lane selection persists

    Scenario: A backlog item in the inbox offers move and groom handoff
      Given a needs-attention row backed by a `backlog` work-item
      When the operator views the per-item verbs for that row
      Then the move-to-status picker and the driver handoff are offered
      And the approve and accept verbs are absent because the vocabulary does not admit them at `backlog`

    Scenario: The offered verb set is identical on both per-item surfaces
      Given one work-item selected on the needs-attention surface and the same work-item selected in a drilled-in lane
      When the console evaluates per-item verb availability on each surface
      Then the two offered verb sets are identical
      And the Status-line hints on each surface name the same per-item keys

    Scenario: A row without a work-item id offers no per-item verb
      Given a needs-attention row that carries no known work-item id
      When the operator views the per-item verbs for that row
      Then no per-item verb is offered
      And no per-item key is hinted
    ```

Scenario 17, Scenario 25 and Scenario 27 are NOT amended: their drilled-in-lane premises remain true, and Scenario 31 states the parity that extends them to the inbox. Co-edits owed at ratification (per spec.md "Self-application"): register Scenario 31 in `tests/heading-coverage.json` (pending TODO naming the implementing child derived by capture-impl-gaps), and re-pin the console-spec-check ground-truth clause count for `contracts.md`. The implementation MUST retire the registry's drill-only surface split (the `surface_offering_matches_the_documented_surface_split` test and the `ActionSurface` doc comment) rather than special-casing dispatch, because the clause is stated for every state-admitted verb.

## Proposal: Per-item dispatch carries a single-key accelerator

### Target specification files

- SPECIFICATION/contracts.md
- SPECIFICATION/scenarios.md

### Summary

Add a clause to the per-item factory-dispatch launcher section requiring the per-item dispatch verb to carry a single-key accelerator (rendered and inert per the Status-line hint honesty rule, on either per-item surface) in addition to its menu path, and append one sub-scenario to Scenario 28 pinning the key-to-same-dialog behaviour. The key itself stays an implementation detail; Scenario 27's menu primacy is untouched.

### Motivation

Maintainer ruling 2026-09-02 on plan retire-overseer-and-redesign-control-plane-around-console (epic livespec-console-beads-fabro-pzbdbo, scope event of the same day): cut the thin console-local 'usable drive loop' slice now and hold everything that needs an orchestrator primitive. The evidence is the maintainer's own first TUI dogfooding session of 2026-08-31 (verbatim transcript: https://github.com/thewoolleyman/livespec-console-beads-fabro/blob/master/plan/retire-overseer-and-redesign-control-plane-around-console/research/livespec-console-beads-fabro-dogfooding-session-transcript.jsonl). The item that needed the operator appeared in the needs-attention inbox; 'dispatch' there was presented 'not available', which the maintainer called 'the critical blocker'. The verified explanation was that per-item dispatch, move and driver handoff are all withheld on the inbox surface by construction (the registry's surface split, pinned by a registry test), so the recipe for driving one item was: leave the inbox, re-find the same item in Lanes, drill in, press `s`, confirm, walk to the Ready lane to look, then `v`, Factory, Dispatch, pick, confirm -- because the one verb the factory exists for has no key. The maintainer's verdict on that recipe: 'THat's a horrible UX'. This proposal changes WHERE state-admitted per-item verbs are offered and gives per-item dispatch a key; it does not touch the orchestrator-owned per-state verb vocabulary, the governed launcher argv, or the menu-primacy rule of Scenario 27 (the menu path stays; the key is additional). Groom and consent inside the TUI are explicitly deferred to orchestrator b3/b4 by the scope event. Finding 1 of the dogfooding session, verbatim: 'The most important verb has no key. approve/accept/reject get p/c/r, but dispatch -- the thing the factory exists for -- is menu-only (v -> Factory -> Dispatch -> pick -> Enter) or a palette command. That's backwards.'

### Proposed Changes

In `contracts.md`, at the END of the section "### Per-item factory-dispatch launcher argv" (after the paragraph that begins "Once this argv is specified"), append a new paragraph:

    **The per-item dispatch verb MUST carry a single-key accelerator** in addition to its menu path. The key MUST be rendered in the Status-line hints wherever the verb is available and MUST be inert, and its hint absent, wherever the verb is unavailable, per the honesty rule of the Status-line hints clause. The verb the factory exists for MUST NOT be the one per-item verb an operator can reach only through the menu bar or the command palette. The specific key is an implementation detail; the obligation is that a selected `ready` work-item, on either per-item surface (the needs-attention row or the drilled-in lane selection), is dispatched by one keystroke plus the same confirmation dialog the menu path opens. This does not weaken Scenario 27: the menu path remains and the key is the additional accelerator that scenario permits. The obligation was earned on 2026-08-31, when dispatching one hand-picked item from the console required opening the menu bar, walking to Factory, then Dispatch, then choosing the per-item entry, while approve, accept and reject each had a key.

In `scenarios.md`, amend Scenario 28 by appending one sub-scenario at the end of its Gherkin block (the mermaid diagram is unchanged):

    Scenario: The per-item dispatch verb is reachable by one key
      Given a selected `ready` work-item on either per-item surface
      When the operator presses the per-item dispatch key
      Then the same per-item dispatch confirmation the menu path opens is staged for that work-item
      And the Status-line hint names the key while the verb is available
      And the key is inert and its hint absent when the verb is unavailable

Scenario 27 is NOT amended. Implementation note for the derived child, recorded here so it is not re-decided: `d` is unbound in the current registry (bound keys are `/ : ? c f g h k m n p q r s v z`), so it is the natural choice; binding it is the implementation's call, not this clause's.
