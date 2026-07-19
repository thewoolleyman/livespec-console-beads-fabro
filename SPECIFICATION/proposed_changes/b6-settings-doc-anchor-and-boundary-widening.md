---
topic: b6-settings-doc-anchor-and-boundary-widening
author: claude-opus-4-8
created_at: 2026-07-19T08:00:16Z
---

## Proposal: Pin the settings-doc anchor and admit documentation-surface contracts at the Boundary

### Target specification files

- SPECIFICATION/contracts.md
- SPECIFICATION/non-functional-requirements.md
- SPECIFICATION/scenarios.md
- tests/heading-coverage.json

### Summary

Corrects three defects the v029 User Documentation Contract shipped. (1) The mechanical settings-completeness gate has NO spec-anchored path: clause 2 declares sub-document identity an implementation detail while clause 3 anchors the gate to 'the detailed-usage sub-document', a referent the contract never pins to a filename. Clause 2 is re-worded to NAME the four sub-documents, and clause 3 to name `docs/detailed-usage.md` outright. (2) The Boundary section of non-functional-requirements.md scopes contracts.md to operator-facing WIRE contracts, which does not admit a documentation-tree layout — so the v029 clauses sit in contracts.md contradicting ratified spec text. The Boundary enumeration is WIDENED to admit documentation-surface contracts. (3) Two contracts.md lines use a section-sign-plus-quotes cross-reference idiom foreign to the file, which otherwise carries exactly one section sign (line 340) citing an EXTERNAL file; both are re-spelled in house-style prose. Scenario 22's cases 1, 3, and 4 are updated in lockstep with the pinned clauses. The change is NET-ZERO on clause counts (15/76/22/52 = 165 before and after), so the console-spec-check ground truth is UNCHANGED; four clause re-links are owed because four clause texts change.

### Motivation

The v029 revision was ratified from the propose-change's FIRST commit, before that proposal's independent review fixes landed on its branch — the merge and the revise happened within roughly fifteen minutes of each other, and the review-fix commit arrived just after. Two independent reviews (a Fable adversarial pass and the doctor LLM-driven post-step phase) had each found the same defects, and the maintainer had separately chosen the Boundary-widening resolution; none of that reached ratified text. The first defect is the load-bearing one and it defeats the v029 change's own stated purpose. That purpose was to give the mechanical settings-doc completeness gate a specified surface to read; `crates/console-completeness-check/src/main.rs` needs exactly ONE path constant, and the ratified contract declines to supply it — clause 2 says which sub-document carries which subject is an implementation detail and that each subject need only be 'covered somewhere in the tree', while clause 3 points the gate at 'the detailed-usage sub-document'. Merge detailed usage into another sub-document and clause 2 is still satisfied while clause 3's referent evaporates. The superseded v024 anchor named a concrete file (`README.md`); this contract must do the same. The second defect is a placement contradiction: the Boundary section enumerates the operator-facing files and assigns contracts.md the WIRE contracts (event/command envelopes, persistence schemas, adapter and TUI contracts). A repository documentation-tree layout is none of those. Keeping the clauses in contracts.md is the right call for cohesion — they sit beside the Settings-surface completeness machinery they re-anchor, and contracts.md already hosts the TUI Contract — but that placement must be RULE-BACKED rather than silently contradicting the enumeration, so the enumeration is widened to admit documentation-surface contracts. The third defect is cosmetic house-style. All three corrections are confined to re-wording ratified clauses; no clause is added or removed, and no new behavior is introduced. In-flight-survey note: no `spec/*` remote branches and no open spec-touching pull requests exist.

### Proposed Changes

NOTATION. A block introduced as VERBATIM is delimited by surrounding double quotes that are FRAMING ONLY and are stripped when the text lands in the spec file. Every character between them is literal; no backslash-escape convention is in play and no backslash may land in the spec.

All gap-ids below were computed against master `b4f1b8f` with a reimplementation of `extract_rules` / `derive_gap_id` validated by reproducing the committed ground truth (15/76/22/52 = 165). The revise step MUST still derive over the FINAL landed text, which is the authority; a mismatch signals the text drifted from this proposal.

--- FIX 1: SPECIFICATION/contracts.md, Settings-surface completeness section ---
REPLACE the section's first paragraph, which today reads VERBATIM (SEVEN physical lines, contracts.md:561-567):

"Every key the orchestrator declares as API-configurable MUST appear, in
lockstep, in three places: a row under the console's Settings surface, the
TUI's inline / context help for that row, and the console's settings doc
(the detailed-usage sub-document of the `docs/` user-documentation tree; see
§"User Documentation Contract"). A mechanical completeness check MUST fail
when a declared key is missing from the Settings surface or from the settings
doc."

with VERBATIM (SIX physical lines):

"Every key the orchestrator declares as API-configurable MUST appear, in
lockstep, in three places: a row under the console's Settings surface, the
TUI's inline / context help for that row, and the console's settings doc
(`docs/detailed-usage.md`, per the User Documentation Contract section). A
mechanical completeness check MUST fail when a declared key is missing from
the Settings surface or from the settings doc."

This names the gate's read path concretely and drops the foreign section-sign idiom. Net-zero on clauses: `MUST` appears on exactly two physical lines before and after. The first line is preserved BYTE-FOR-BYTE, so gap-qjcrfd64 is stable. The MUST-fail line's text changes: gap-umnfoimk -> gap-z3xisytt (expected).

--- FIX 2: SPECIFICATION/contracts.md, User Documentation Contract clause 2 ---
REPLACE the section's SECOND clause, one physical line, which today reads VERBATIM:

"Across its linked sub-documents the `docs/` tree MUST cover installation (including the download-install path and use from a repository other than the console's own), a general overview and quick start, the console's environment variables / CLI options / sub-commands, and detailed usage carrying a section per TUI pane; which sub-document carries which additional heading is an implementation detail, and the contract is that user documentation lives under `docs/`, the top-level `README.md` is a pointer, and every one of those subjects is covered somewhere in the tree."

with VERBATIM (ONE UNWRAPPED PHYSICAL LINE):

"The `docs/` tree MUST carry four sub-documents: `docs/installing.md` covering installation, including the download-install path and running the console against a repository other than its own; `docs/overview-quickstart.md` covering a general overview and quick start; `docs/cli-options.md` covering the console's environment variables, CLI options, and sub-commands; and `docs/detailed-usage.md` covering detailed usage with a section per TUI pane; the tree MAY carry further sub-documents, and what additional headings each one carries is an implementation detail."

The implementation-detail carve-out is now scoped to ADDITIONAL sub-documents and ADDITIONAL headings, so it no longer contradicts clause 3's dependence on a fixed sub-document identity. Net-zero on clauses (one MUST-bearing physical line before and after). gap-fyxmwbti -> gap-hbcwvf5e (expected).

--- FIX 3: SPECIFICATION/contracts.md, User Documentation Contract clause 3 ---
REPLACE the section's THIRD clause, one physical line, which today reads VERBATIM:

"The console's settings doc -- the documentation surface the §"Settings-surface completeness" check reads -- MUST be the detailed-usage sub-document of this `docs/` tree and MUST NOT be the top-level `README.md`; this SUPERSEDES the earlier settings-doc-is-the-README anchor, which held only while the console had no `docs/` tree."

with VERBATIM (ONE UNWRAPPED PHYSICAL LINE):

"The console's settings doc -- the documentation surface the Settings-surface completeness check reads -- MUST be `docs/detailed-usage.md` and MUST NOT be the top-level `README.md`; this supersedes the earlier settings-doc-is-the-README anchor, which held only while the console had no `docs/` tree."

Names the file, and drops the second foreign section-sign reference. Net-zero on clauses. gap-qwlrri47 -> gap-ynr73rzr (expected).

--- FIX 4: SPECIFICATION/non-functional-requirements.md, Boundary section ---
REPLACE the contracts.md entry of the operator-facing-file enumeration, which today reads VERBATIM (TWO physical lines, non-functional-requirements.md:28-29):

"- Operator-facing wire contracts (event/command envelopes, persistence
  schemas, adapter and TUI contracts) MUST stay in `contracts.md`."

with VERBATIM (FOUR physical lines, preserving the two-space continuation indent):

"- Operator-facing wire contracts (event/command envelopes, persistence
  schemas, adapter and TUI contracts) and operator-facing
  documentation-surface contracts (the user-documentation tree and the
  settings doc the completeness check reads) MUST stay in `contracts.md`."

This makes the v029 clauses' placement in contracts.md rule-backed. Net-zero on clauses: `MUST` appears on exactly one physical line before (line 2) and after (line 4), so non-functional-requirements.md stays at 52. The three sibling entries (`spec.md`, `constraints.md`, `scenarios.md`) and the trickiest-boundary paragraph that follows are UNCHANGED. gap-cijpvz66 -> gap-ahcqrwyu (expected).

--- FIX 5: SPECIFICATION/scenarios.md, Scenario 22 ---
Scenario 22's gherkin still mirrors the UNPINNED clauses. Because gherkin lives inside a code fence, `extract_rules` skips it and these edits touch NO clause count or gap-id. Three verbatim replacements:

(a) In case 1, REPLACE the two lines

  Then it carries the project overview and a link to the docs/ tree's index
  And it carries no user-facing documentation sections of its own
  And contributor-facing build and development material may still appear there

with

  Then it carries the project overview and a link to the docs/ tree's index document
  And it carries no user-facing documentation sections of its own
  And contributor-facing build, development, and quality-gate material may still appear there

(b) REPLACE case 3 in full, from its `Scenario:` line through its final `And` line:

  Scenario: The docs tree covers every required subject
    ... Then each of those subjects is covered somewhere in the tree ...

with a case titled "The docs tree carries the four required sub-documents", whose Given is "the docs/ tree's index document", whose Then reads "each of those four subjects is covered by its own linked sub-document", and whose two And lines read "the installation sub-document covers both the download-install path and running the console against a repository other than its own" and "the detailed-usage sub-document carries a section per TUI pane".

(c) In case 4, REPLACE the Then line

  Then it reads the detailed-usage sub-document of the docs/ tree

with

  Then it reads `docs/detailed-usage.md`

--- FIX 6: tests/heading-coverage.json (co-edit performed at REVISE time) ---
FOUR clause re-links, each replacing a stale gap-id in place. No entry is added or removed, and no `test` or `reason` field changes:

  gap-umnfoimk -> gap-z3xisytt   in the Scenario 14 entry
  gap-fyxmwbti -> gap-hbcwvf5e   in the Scenario 22 entry
  gap-qwlrri47 -> gap-ynr73rzr   in the Scenario 22 entry
  gap-cijpvz66 -> gap-ahcqrwyu   in the Contributor Scenario A entry

Each stale id is bound in exactly one entry; every other clause link MUST be left untouched.

NO console-spec-check ground-truth change is owed. Every fix above is net-zero on clause counts, so `crates/console-spec-check/src/tests.rs` keeps `("contracts.md", 76)` / `total, 165` and MUST NOT be edited by this revision.

The impl follow-up declared by the v029 propose-change (`user-docs-tree`) is UNCHANGED and still owed: building the `docs/` tree and repointing `console-completeness-check` at `docs/detailed-usage.md`. That work now has a concretely-named target to point at, which is the purpose of this correction.
