---
topic: settings-doc-is-readme
author: claude-opus-4-8
created_at: 2026-07-17T10:45:00Z
---

## Proposal: The console settings doc is the README, not `docs/settings.md`

### Target specification files

- SPECIFICATION/contracts.md
- SPECIFICATION/scenarios.md
- tests/heading-coverage.json
- crates/console-spec-check/src/tests.rs

### Summary

The ratified W5 decision is that **the console's README IS the settings doc** — there is no `docs/` directory in this repo, and the README's "Dispatcher settings" section is the operator-facing settings documentation. Three live spec sites still name the never-created `docs/settings.md`: the "Settings-surface completeness" MUST clause in `contracts.md`, and the mermaid node + a gherkin `Given` line in Scenario 14. This proposal corrects all three to `README.md`, so the spec names the real doc the shipped `check-completeness` gate reads.

### Motivation

W5 (`livespec-console-beads-fabro-2ctzhm`, §3) explicitly decided: "There is NO `docs/` directory in this repo. `README.md` is the ONLY user doc, so it IS the settings doc." PR #248 shipped the README "Dispatcher settings" section as that doc, and the W6 completeness check (`console-completeness-check`) asserts every orchestrator-declared key reaches the README's settings section. The spec's stale `docs/settings.md` path is a naming drift against that ratified decision and the shipped code; nothing ever created `docs/settings.md`. Correcting the path keeps the contract, the scenario, and the check in agreement on the one real settings doc.

### Proposed Changes

Each REPLACE-target below was verified to occur VERBATIM, exactly once, in the live file on branch `console-w6-completeness` (base `9a76de0`) by exact substring match. A drift sweep confirms these three are the COMPLETE set of `docs/settings.md` references in the live operator-facing spec.

#### A. `SPECIFICATION/contracts.md`

**A.1 -- AMEND the "Settings-surface completeness" MUST clause** to name the README. (This is a `MUST` clause; the `docs/settings.md` token sits inside it, so its gap-id may re-derive — see section C.)

REPLACE:

```text
TUI's inline / context help for that row, and the console's settings doc
(`docs/settings.md`). A mechanical completeness check MUST fail when a
declared key is missing from the Settings surface or from the settings doc.
```

WITH:

```text
TUI's inline / context help for that row, and the console's settings doc
(the repo `README.md`). A mechanical completeness check MUST fail when a
declared key is missing from the Settings surface or from the settings doc.
```

#### B. `SPECIFICATION/scenarios.md`

**B.1 -- AMEND Scenario 14's mermaid doc node.**

REPLACE:

```text
  Doc["docs/settings.md"]
```

WITH:

```text
  Doc["README.md"]
```

**B.2 -- AMEND Scenario 14's gherkin `Given` naming the doc.**

REPLACE:

```text
  Given the orchestrator declares a dispatcher key that `docs/settings.md` does not document
```

WITH:

```text
  Given the orchestrator declares a dispatcher key that `README.md` does not document
```

#### C. `tests/heading-coverage.json` and `crates/console-spec-check/src/tests.rs` (CO-EDIT -- REQUIRED, atomic with the accept)

At revise, once the amended clause text is final, reconcile the registry MECHANICALLY:

**C.1 -- REBIND the completeness MUST clause if its gap-id re-derives.** The `docs/settings.md` -> `README.md` reword is a same-clause edit inside the "Settings-surface completeness" MUST clause; the gherkin/mermaid edits in B carry NO gap-id. Run the clause extractor over the final `contracts.md`; if the completeness clause's gap-id changed, update its entry in `tests/heading-coverage.json` (currently bound to Scenario 14) to the new gap-id, same scenario. Net new clauses: ZERO.

**C.2 -- SET the clause-count ledger to the ACTUAL extractor count** in `crates/console-spec-check/src/tests.rs`: a same-clause reword nets zero new MUST-clauses, so the running total + per-file breakdown SHOULD stay unchanged — VERIFY with the extractor and pin exactly what it reports, not an assumption.

Scenario 14's `test` is already registered (the shipped `console-completeness-check` test) by the W6 code child on this branch; this proposal does not change it.
