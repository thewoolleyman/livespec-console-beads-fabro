---
topic: b1-source-availability-honesty
author: claude-opus-4-8
created_at: 2026-07-18T06:11:21Z
spec_commitments:
  impl_followups:
    - id_hint: b1-source-availability-honesty
      description: |
        Fix the console source-availability honesty across six root causes (diagnosed live against the release binary + the live event store):
        1. fabro: resolve the `fabro` program to an absolute path or honor an env override (LIVESPEC_CONSOLE_FABRO_PROGRAM) so a host-present fabro off the credential wrapper's scrubbed PATH is still found (the wrapper resets PATH to /usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin, which omits ~/.local/bin where fabro lives) — same spirit as the existing `.py`->python3 normalization.
        2. livespec: stop clobbering the livespec program with the orchestrator plugin-bin's `next.py` (impl-next, which emits `{"candidates":...}` with no `action`/`next` field); point the livespec adapter at the real spec-side `livespec next --json` surface, and confirm that surface is observable headlessly under the credential wrapper (it may need its own driver wrapper).
        3. dispatcher: resolve the dispatch journal against the SELECTED-REPO absolute path rather than the console process cwd (`tmp/dispatcher-journal.jsonl` is relative today), and treat an absent-but-expected journal (a factory that has not yet dispatched) as observed-idle, not unavailable.
        4. orchestrator/github normalizers: distinguish reached-but-empty (an empty work-item array, zero open PRs) from genuinely unreachable — a successful-but-empty observation must be observed-idle, not a not-observed finding (`parse_orchestrator_observation` "no work-items observed" Err and `parse_github_observation` "no pull request observed" Err are the current misclassifications).
        5. sticky projection: make `unavailable_sources` (crates/console-application/src/lib.rs) latest-per-source — a source counts unavailable only while its MOST RECENT poll was not-observed — or emit a clearing/superseding event when a source is next observed (the not-observed finding's `source_event_id` is cycle-less today, so it is written once and never cleared, permanently branding any transient failure).
        6. persist the not-observed reason in payload_json (it is dropped as `{}` today in `normalized_payload_json`) so the header/detail can name WHY a source is unavailable, not merely THAT it is.
---

## Proposal: Source-availability honesty: reachable-but-empty vs unreachable, with a reason, latest-per-source

### Target specification files

- SPECIFICATION/scenarios.md
- SPECIFICATION/contracts.md
- tests/heading-coverage.json

### Summary

The console header brands a backing source "unavailable" in three wrong-to-conflate cases, and the layered diagnosis (root-caused live against the release binary + the live event store) shows why every source but the console's own appears down under a normal launch. This proposal refines Scenario 13 (cockpit-blind vs factory-idle) with four availability-honesty scenarios and tightens the Adapter Contract honesty rule so that (a) a SUCCESSFUL observation of an empty source is observed-idle, never a not-observed finding; (b) a not-observed finding carries a durably-persisted human-readable reason; and (c) the header's unavailability tally reflects the LATEST poll outcome per source, so a recovered source clears rather than being branded forever. It adds the four Scenario: blocks UNDER the existing `## Scenario 13` H2 (inside its gherkin fence — no new H2), an Adapter Contract clause, a declared impl commitment (b1-source-availability-honesty), and a tests/heading-coverage.json co-edit (performed at revise time) refreshing the existing Scenario 13 coverage entry. This is the B1 spec of the console `plan/cockpit-ux-docs-release` program. It is a spec-only proposal file; it awaits an independent read-only Fable review, then ratification via /livespec:revise. Do not merge before that review.

### Motivation

Maintainer-reported symptom: under a normal launch the cockpit header shows "sources: N unavailable" for ALL sources except the console's own — e.g. `dispatcher, fabro, livespec` (and at times orchestrator/github). Read-only root-cause (serve --preview under the credential wrapper from the orchestrator tenant cwd, plus direct probe reproduction and a dump of the live console event store) found it is NOT one bug but a per-source layer AND a compounding projection layer:

LAYER 1 (each source genuinely fails to be observed each cycle): fabro's bare `fabro ps --json` exits 127 because the credential wrapper's env -i PATH omits ~/.local/bin where fabro lives; the "livespec" adapter is wired to the ORCHESTRATOR's impl-`next.py` whose `{"candidates":...}` shape has no `action`/`next` field the normalizer needs (verified: exit 0 but Err); the dispatcher reads a RELATIVE `tmp/dispatcher-journal.jsonl` that is absent until a dispatch runs; and an empty ledger / zero open PRs make the orchestrator and github normalizers return Err ("no work-items observed" / "no pull request observed") — collapsing "reached it, it's empty" into "couldn't reach it."

LAYER 2 (the header projection is sticky/monotonic): `unavailable_sources` scans the FULL event log and marks a source unavailable if it EVER emitted a not-observed finding, with no recency check and no clearing by a later observed snapshot; the finding's `source_event_id` is cycle-less (`{source}:{repo}:not_observed`), so it is written once, deduped, and never superseded. Proven live: `livespec` carries BOTH an observed snapshot AND a standing not-observed finding, yet still counts unavailable. So any transient failure becomes a permanent brand, which is why orchestrator/github show "at times."

Both layers violate the whole point of Scenario 13: telling a cockpit-blind screen (sources genuinely unreachable) from an idle factory (reachable, nothing to report). Capturing the corrected invariant spec-first with refined scenarios, a tightened Adapter Contract clause, and a declared impl commitment before implementation, per the livespec workflow.

### Proposed Changes

--- CHANGE 1: SPECIFICATION/contracts.md, §"Adapter Contract" ---
ADD the following clause to the "Adapter rules:" bullet list, inserted immediately AFTER the existing final bullet -- the one that begins "If an adapter does not actually perform real source I/O (a minimal or simulated first-milestone adapter per `spec.md` -> Initial-adapter fidelity), ..." and ENDS "... and MUST NOT emit an event asserting an observed source fact it did not observe." (the last bullet of the "Adapter rules:" list) -- and BEFORE the blank line preceding the first ```mermaid fence of that section. Verbatim text to add (an introductory line plus three new bullets):

The adapter honesty rule tightens to separate a reachable-but-empty source from a genuinely unreachable one:

- A SUCCESSFUL observation of an EMPTY source is NOT an unavailability. When the adapter reaches its source and the source simply holds nothing to report -- an empty work-item ledger, zero open pull requests, or an absent-but-expected dispatch journal -- the adapter MUST treat it as observed-and-idle and MUST NOT emit a not-observed finding for it. A not-observed finding is reserved for GENUINE unreachability: an unresolvable program, a non-zero command exit, an unreadable or absent required file, or an uninterpretable payload, or the preceding bullet's simulated / unimplemented (no real source I/O) case. This is the cockpit-blind-vs-idle distinction (`scenarios.md` Scenario 13): an idle factory MUST NEVER be counted or named as an unavailable source.
- A not-observed finding MUST carry a human-readable reason, and that reason MUST be durably persisted with the finding so the operator can see WHY a source is unavailable, not merely THAT it is.
- The header's source-availability tally MUST reflect the LATEST poll outcome per source: a source counts as unavailable only while its MOST RECENT poll was not-observed. A source that is observed on a later cycle MUST clear from the tally, so a transient failure is never a permanent brand.

--- CHANGE 2: SPECIFICATION/scenarios.md, §"Scenario 13" ---
INSERT the following four `Scenario:` blocks INSIDE the existing Scenario 13 gherkin fenced block, under the SAME `Feature: Source-unavailability is legible in the header`. Insert them immediately AFTER the last line of the existing "A healthy cycle shows no phantom unavailability count" scenario (the line "  Then the header shows no source-unavailability indicator") and BEFORE the closing ``` fence of that gherkin block. This introduces NO new `## ` heading (the blocks live under the existing `## Scenario 13 -- Operator distinguishes cockpit-blind from factory-idle` H2) and needs no new mermaid diagram. Precede the first inserted scenario with one blank line so it is separated from the phantom-count scenario. Verbatim blocks to add:

Scenario: A normally-launched console against a real tenant shows its reachable sources available
  Given a console launched under the credential wrapper against a real tenant with a non-empty ledger
  And every backing-source binary the console invokes is resolvable and every backing file it reads is present
  When the source poll cycle runs and the operator screen is rendered
  Then each reachable source is counted as available and appears in no unavailability tally
  And the header carries no source-unavailability indicator for a source that was successfully observed

Scenario: A reachable-but-empty source is idle, not unavailable
  Given a backing source the console reaches successfully but which currently holds nothing to report
  And that emptiness is an empty work-item ledger, zero open pull requests, or a factory that has not yet written a dispatch journal
  When the source poll cycle runs
  Then the source is treated as observed-and-idle
  And it is not counted or named among the unavailable sources
  And an idle factory is never dressed as a cockpit-blind screen

Scenario: A recovered source clears from the unavailability tally on its next observation
  Given a source that degraded to a not-observed finding on an earlier cycle
  When a later cycle observes that source successfully
  Then the header no longer counts or names that source as unavailable
  And the unavailability tally reflects the latest poll outcome per source rather than any historical failure

Scenario: Only a genuinely unreachable source is counted, and it is named with a reason
  Given a backing source that cannot be reached this cycle because its binary is unresolvable, its command exits non-zero, or its required file is absent or unreadable
  When the operator screen is rendered
  Then the header counts that source among the unavailable and names it
  And the not-observed finding carries a human-readable reason
  And that reason is durably recorded so the operator can see why the source is unavailable

--- CHANGE 3: tests/heading-coverage.json (co-edit performed at REVISE time, described here) ---
This proposal adds NO new `## ` heading: the four new scenarios live under the existing `## Scenario 13` H2 (inside its gherkin fence) and the new Adapter Contract clause lives under the existing `## Adapter Contract` H2. So NO new coverage entry is required and the mechanical `check-heading-coverage` guard is not triggered by an H2-set change. At revise/accept time, perform a FIDELITY REFRESH of the EXISTING `Scenario 13 -- Operator distinguishes cockpit-blind from factory-idle` entry in `tests/heading-coverage.json`: update its `reason` so it also describes the four added availability-honesty assertions (a reachable source is available; a reached-but-empty source is observed-idle, not unavailable; a recovered source clears from the tally on its next observation; a genuinely-unreachable source is named with a durably-persisted reason), and bind the new §"Adapter Contract" honesty clause (empty-is-not-unavailable / persisted-reason / latest-per-source) to Scenario 13's coverage entry. This propose-change lists `tests/heading-coverage.json` in target_spec_files so the revise co-edit is not forgotten, mirroring the co-edit discipline in the console repo.
