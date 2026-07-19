---
proposal: user-docs-tree.md
decision: accept
revised_at: 2026-07-19T07:34:40Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Accept the user-docs-tree proposal (B6): the console's user-facing documentation lives under a `docs/` tree at the repository root -- `docs/README.md` an overview plus a relative-path table of contents only, with the substantive documentation in linked sub-documents covering installation (including the download-install path and use from another repository), a general overview and quick start, the environment variables / CLI options / sub-commands, and detailed usage carrying a section per TUI pane -- and the top-level `README.md` carries no user-facing documentation of its own beyond a project overview and a pointer into that tree. Contributor-facing build and development material is unconstrained and MAY remain in the README. This RELOCATES the settings-doc anchor: the settings doc the §"Settings-surface completeness" check reads MUST now be the detailed-usage sub-document, superseding the v024 settings-doc-is-the-README decision, which held only while the console had no `docs/` tree. Adds a contracts.md §"User Documentation Contract" section (three clauses, each one unwrapped physical line, +3 normative clauses), re-anchors the §"Settings-surface completeness" first paragraph (re-wording the `MUST fail` line without changing the clause count), amends Scenario 14's two settings-doc references, appends Scenario 22, and co-edits tests/heading-coverage.json (the new Scenario 22 entry binding the three derived clauses gap-xdj5g2rk / gap-fyxmwbti / gap-qwlrri47, plus the Scenario 14 re-link of gap-3dyfo5pk to the re-derived gap-umnfoimk and its location-agnostic `reason` prose) and the console-spec-check ground-truth bump (contracts.md 73->76, total 162->165). Authoring the `docs/` tree itself, repointing console-completeness-check, and the Scenario 22 structural test are the separate B6 impl deliverable; Scenario 22's `test` stays TODO. Independent Fable review returned NO-BLOCKERS.

## Resulting Changes

- contracts.md
- scenarios.md
- ../tests/heading-coverage.json
- ../crates/console-spec-check/src/tests.rs
