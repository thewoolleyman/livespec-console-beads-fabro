---
topic: nightly-soak-idempotent-chore-filing
author: claude-opus-4-8
created_at: 2026-08-31T10:24:25Z
---

## Proposal: Nightly soak files chores idempotently — skip when an open chore for the finding already exists

### Target specification files

- non-functional-requirements.md
- scenarios.md

### Summary

Add an idempotency requirement to the nightly-soak clause: a nightly finding MUST NOT create a duplicate chore when an open chore for the same finding already exists. The nightly computes a stable finding signature, and files a chore only when no non-closed chore carrying that signature is already open. The pre-existing open chore MAY be stale relative to the latest run, and that is explicitly acceptable.

### Motivation

The ratified nightly clause requires a finding (a new fuzz crash, or a new surviving mutant not on the allow-list) to file a chore work-item rather than fail master, through the orchestrator's capture surface. That capture path is freeform (origin: freeform, gap_id: null) and has NO built-in idempotency — unlike the gap-tied capture-impl-gaps surface, which is idempotent by gap_id. As specified today, an unfixed finding is therefore re-filed on EVERY nightly run, accumulating duplicate chores for days or weeks until it is fixed (fuzz: reproducer committed to fuzz/regressions/) or allow-listed (mutant: #[mutants::skip] / mutants.toml). The natural dampening those two mechanisms provide covers only the AFTER-fixed state, not the found-but-not-yet-fixed window, which is exactly when duplicates pile up. The clause must require idempotent filing keyed on a stable finding signature. The trade-off this accepts is deliberate and recorded: a pre-existing open chore may be stale — its details can lag, and it does not re-surface newly-appeared findings masked behind it — but that is fine, because the soak jobs are expensive and any fix will re-run the soak MANUALLY, which re-discovers the current finding set (including new ones). The nightly's obligation is that each distinct finding is TRACKED at least once, not that every open chore is kept continuously current.

### Proposed Changes

In non-functional-requirements.md, AUGMENT the existing nightly clause (the paragraph beginning 'Nightly -- scheduled run against the canonical branch ... A nightly finding ... MUST instead file a chore work-item at the top of the rank order ...') so the filing is IDEMPOTENT:

- Add that the nightly MUST derive a STABLE finding signature for each finding — for a fuzz crash, a hash of the reproducing input (or the crash backtrace); for a surviving mutant, the (source file, line, mutation-operator) identity — stable across runs for the same underlying defect, and MUST persist that signature on the filed work-item (a label or structured field) so an existence check is cheap.
- Add that BEFORE filing, the nightly MUST check for an existing NON-CLOSED chore carrying that signature in the livespec-console-beads-fabro tenant, and MUST NOT file a duplicate when one already exists — it files only when no open chore for that signature is present.
- Add the explicit accepted trade-off: the pre-existing open chore MAY be stale (its recorded detail can lag, and it does not re-surface findings masked behind it); this is acceptable BY DESIGN because the soak is expensive and a fix re-runs the soak manually to surface the current finding set. The nightly's obligation is that each distinct finding is tracked at least once, not that every open chore stays current.
- Keep unchanged: the MUST-NOT-fail-canonical-branch requirement, the top-of-rank-order filing, routing through the orchestrator capture surface (intake Definition-of-Ready + admission_policy; never filed directly into ready), and the Family-Secret-Convention CI-credential requirement.

BECAUSE this introduces load-bearing behavior (an observable MUST NOT — no duplicate chore for an already-open finding), it also requires a linked scenario per the Behavior => Gherkin split. In scenarios.md, ADD a Given/When/Then scenario under the Quality Gate coverage, for example: 'Given the nightly soak found a defect on a prior run and an open chore carrying that finding's signature already exists in the tenant, When tonight's soak re-discovers the same defect (unfixed), Then the nightly files NO new chore and the single existing open chore remains the tracking item' — plus a companion scene that a DISTINCT new finding (no open chore for its signature) DOES file one. Register the new scenario in tests/heading-coverage.json linking the augmented clause to the scenario (and its owning test tier), co-edited atomically so console-spec-check's clause->scenario gate stays green. This is a contributor-facing quality-gate concern, so non-functional-requirements.md + scenarios.md are the correct trees (not spec.md/contracts.md).
