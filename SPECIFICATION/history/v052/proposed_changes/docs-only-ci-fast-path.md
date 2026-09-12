---
topic: docs-only-ci-fast-path
author: gpt-5.6-sol
created_at: 2026-09-10T18:32:29Z
---

## Proposal: Code-neutral documentation changes use a lightweight CI lane

### Target specification files

- SPECIFICATION/non-functional-requirements.md
- SPECIFICATION/scenarios.md

### Summary

Permit a narrowly defined, fail-closed documentation-only CI lane that avoids Rust compilation, end-to-end execution, mutation testing, and fuzzing while preserving the stable ci-green branch-protection context and retaining the full existing quality gate for every change that could affect executable or specified behavior.

### Motivation

Work item livespec-console-beads-fabro-3toz records that a root-documentation-only pull request currently schedules the complete Rust matrix and the three-target ASAN fuzz gate, consuming runner capacity and several minutes without exercising changed behavior. The current specification requires fuzzing on every pull request, so the optimization needs an explicit ratified carve-out rather than an implementation-only path filter. The proposed boundary is deliberately narrower than every Markdown file because Markdown under SPECIFICATION/, .fabro/, and .claude-plugin/ is executable or behavior-defining input.

### Proposed Changes

Amend `SPECIFICATION/non-functional-requirements.md` section `Quality Gate` so the inner-loop and merge-gate obligations distinguish code-bearing changes from code-neutral documentation changes. CI MUST classify a change as code-neutral documentation-only only when every changed path is a root-level Markdown file, a path under `.ai/` or `docs/`, or a Markdown file under `plan/`. Both source and destination paths of a rename MUST satisfy that allow-list. Paths under `SPECIFICATION/`, `.fabro/`, `.claude-plugin/`, `.github/`, source or test trees, manifests, lockfiles, and every unrecognized path MUST select the full gate even when the filename ends in `.md`. An empty diff, an unavailable base or head commit, or any classifier error MUST also select the full gate.

For a code-neutral documentation-only pull request or canonical-branch push, CI MUST NOT schedule the Rust compile/test matrix, the real-TUI end-to-end job, mutation testing, or fuzzing. It MUST still run the lightweight change-classification job, static specification/reference validation, and the plan-tombstone validation, and it MUST report the same required `ci-green` context used by code-bearing changes. `ci-green` MUST treat deliberately skipped code-only jobs as neutral, and MUST fail when classification or any applicable lightweight check fails or is cancelled.

For every change outside the documentation-only allow-list, the existing full inner-loop and merge-gate obligations MUST remain unchanged: the Rust checks run, mutation remains diff-scoped, and every fuzz target runs for at least 60 seconds. The docs-only carve-out MUST NOT reduce the nightly quality gate.

Update the Quality Gate diagrams to show the classifier branching to either the lightweight documentation lane or the existing full gate. In `SPECIFICATION/scenarios.md`, amend the quality-gate contributor scenario so the existing fuzz and mutation scenario is explicitly given a code-bearing pull request, and add a scenario in which a root documentation-only pull request skips all code-only jobs, runs the lightweight validations, reports `ci-green`, and a classifier uncertainty falls back to the full gate. The revision MUST co-edit any required scenario-to-test coverage registry entry.
