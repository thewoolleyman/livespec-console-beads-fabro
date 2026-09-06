---
proposal: nightly-soak-ssh-ingress-filing.md
decision: accept
revised_at: 2026-09-06T12:40:58Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-5
---

## Decision and Rationale

Accepted after five independent review rounds, each of which found a real defect. The amendment aligns the ratified nightly chore-filing clause with the on-tailnet SSH write ingress dolt-server actually ships: filing routes through the ingress rather than the orchestrator's capture surface, the finding identity is rendered as a bounded DNS-label-like fingerprint that is the dedup key, CI holds no work-items database credential and authenticates with an SSH key alone, and the nightly must run on the self-hosted, on-tailnet runner pool or fail loudly. Rounds 1-2: a partial amendment would have ratified two mutually exclusive MUSTs, so the primary filing sentence, the Family Secret Convention sentence and Contributor Scenario E were all amended together. Round 3: the 'top of the rank order' obligation was hollow, and round 4 confirmed that RELOCATING it to the `pending-approval -> ready` transition did not fix it, because an ingress-filed item cannot reach that state -- `bd create` has no status flag (verified: `bd create --dry-run --json` returns "status": "open"), the orchestrator's lifecycle states are custom beads statuses reachable only by a two-step path, and dolt-server's single-verb constraint forecloses step two for CI. The obligation is therefore REMOVED, not relocated, and the resulting gap -- nothing adopts or ranks `ci-soak-finding` items -- is tracked upstream as livespec-orchestrator-beads-fabro/bd-ib-3nlq with console BLOCKED-ON proxy livespec-console-beads-fabro-4jb3kl.1, rather than papered over with a MUST no actor performs. Round 4 also found a substrate-false credential claim asserting that the ingress host obtains the family BEADS_DOLT_PASSWORD 'through the same convention' -- restated in four places including a mermaid EDGE, the fenced-block blind spot. That claim contradicted dolt-server's ratified constraints.md, which requires the `ci-writer` wrapper profile to load ONLY a systemd-creds-sealed credential and MUST NOT load any family credential. The convention is now scoped to processes in this repository and explicitly declines to assert how the ingress host credentials its own `bd`. Round 4's dedup observation was also acted on: the unqualified suppression MUST is now stated as observational rather than atomic, naming the benign list-then-create race dolt-server ratifies as accepted. Both reductions in normative force are recorded as reductions, not disguised as clarifications. tests/heading-coverage.json and crates/console-spec-check/src/tests.rs are co-edited atomically: 11 clause lines added, 3 removed, non-functional-requirements.md 59 -> 67, total 245 -> 253, 3 orphaned gap_ids deleted, 0 unlinked clauses, 0 newly dangling ids, and 26 console-spec-check tests passing. Round 5 re-derived every one of those counts independently by reimplementing the clause extractor, and swept all 24 fenced blocks by hand plus the whole live spec tree, finding no surviving reference to the retired phrases.

## Resulting Changes

- non-functional-requirements.md

## Ratification Review

ratification_review: auto-spawn
reviewer_model: opus
reviewer_identity: opus
separate_reviewer: True
read_only: True
reviewed_at: 2026-09-06T11:01:26Z
verdict: NO BLOCKERS
proposal_stem: nightly-soak-ssh-ingress-filing
content_digest: ec90e7083f07eab279b0cd1d1747532650736ab773a0f88a390fab0c2c4e7540
