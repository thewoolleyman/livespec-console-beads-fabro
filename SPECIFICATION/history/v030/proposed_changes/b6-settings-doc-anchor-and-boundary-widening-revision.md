---
proposal: b6-settings-doc-anchor-and-boundary-widening.md
decision: accept
revised_at: 2026-07-19T08:15:14Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Accept. Corrects three defects the v029 User Documentation Contract shipped because that revision was ratified from the B6 propose-change first commit, minutes before that proposal independent review fixes landed on its branch. (1) The mechanical settings-completeness gate had no spec-anchored path: clause 2 declared sub-document identity an implementation detail while clause 3 anchored the gate to the detailed-usage sub-document, a referent never pinned to a filename, so console-completeness-check had no contract-named constant to read. Clause 2 now NAMES the four sub-documents and scopes its carve-out to ADDITIONAL sub-documents and headings; clause 3 names docs/detailed-usage.md outright, as the superseded v024 anchor named README.md. (2) The non-functional-requirements.md Boundary enumeration scoped contracts.md to operator-facing WIRE contracts, which does not admit a documentation-tree layout, so the v029 clauses sat there contradicting ratified spec text; the enumeration is WIDENED to admit documentation-surface contracts, making the placement rule-backed. This is the resolution the maintainer selected. (3) Two contracts.md lines used a section-sign-plus-quotes cross-reference idiom foreign to that file, which otherwise carries exactly one section sign citing an external file; both are re-spelled in house prose. Scenario 22 cases 1, 3, and 4 are updated in lockstep with the pinned clauses; gherkin is fenced, so those edits shift no clause count or gap-id. NET-ZERO on clause counts: 15/76/22/52 = 165 before and after, independently confirmed, so crates/console-spec-check/src/tests.rs count assertions are UNCHANGED and that file is not in resulting_files. Four clause re-links are performed in tests/heading-coverage.json because four clause texts change: gap-umnfoimk -> gap-z3xisytt (Scenario 14), gap-fyxmwbti -> gap-hbcwvf5e and gap-qwlrri47 -> gap-ynr73rzr (Scenario 22), gap-cijpvz66 -> gap-ahcqrwyu (Contributor Scenario A); each stale id was bound in exactly one entry and each new id was derived over the final text. Independent Fable review recomputed every mechanical claim and returned one blocker (a non-verbatim replacement block), which was fixed before this ratification. The user-docs-tree spec-to-impl follow-up declared by the v029 propose-change is UNCHANGED and still owed.

## Resulting Changes

- contracts.md
- non-functional-requirements.md
- scenarios.md
- ../tests/heading-coverage.json
