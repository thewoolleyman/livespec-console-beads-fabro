# The menu-primary decision, and what it discharges

**Recorded 2026-08-02.** Cross-reference note, not a design document.

## The decision

Maintainer, verbatim:

> "I would rather every action be available via MENUS and DIALOGS as the first-class,
> required, primary navigation UX mechanism. And hotkeys are only provided IN ADDITION
> to first-class menu operation AS A POWER-USER CONVENIENCE."

> "ideally some mechanical, generic test to prove that EVERY action can be driven via
> MENUs, and hotkeys are ONLY ADDITIONAL."

> "I'm already decided on menu primary - definitely want this, it's a better UX for me,
> and for agents."

## What it discharges

`plan/operator-surface-redesign/`'s ENTRY GATE is maintainer brainstorm participation —
an absolute gate, not a step. Its standing rule is "no impl items until ratification".

**This decision satisfies that gate for the operator-navigation question.** The
maintainer has participated and decided, so the surface question that thread existed to
open is answered: navigation is menu-primary, hotkeys are additional, and the action set
is registry-derived.

**DISPOSITION DECIDED 2026-08-02: ABSORB AND ARCHIVE.** The maintainer ruled that the
thread is absorbed into the 01–04 arc and archived, with custody of its open design
questions transferred and NAMED BEFORE archival. It now lives at
`plan/archive/operator-surface-redesign/`. The earlier instruction here — "do not edit
that thread, do not archive it" — was correct while the question was open and is
superseded.

### What was absorbed, and by whom — the custody map

Verified against the ledger 2026-08-02 rather than read off the thread's prose: the
epic's children are wired by `parent-child` edges stored on each CHILD, so
`bd dep list` on the epic shows nothing. Five children, four of them design inputs.

| item | state at transfer | inherits | why |
|---|---|---|---|
| `-zweohm` — lane items expose no state-appropriate next action | backlog | **02** (`-et3`) | Presentation of exactly the valid verb set IS the menu. The availability predicate it needs already shipped in 01 (registry predicates + the `-0uw` fix). |
| `-vc7lmq` — detail pane should offer only state-valid commands | backlog | **02** (`-et3`) | Same subject as `-zweohm`, same surface. |
| `-l4p3ce` — no paradigm for handing off to an LLM driver session | backlog | **02** (`-et3`) | The transport SHIPPED as the `h` driver-handoff overlay (`-cxu4eu`). What remains is menu-driven invocation of heavyweight verbs. See also the dead OSC 52 copy path recorded in plan 01's resume block. |
| `-ipi` — migrate attention render to the `attention_item.*` stream | backlog (P3) | **03** (`-1df`) | The stream carries `handoff.command`, the truthful replacement for the fabricated attach line. That is outcome surfacing. **This is the one no other plan naturally absorbs, which is exactly why it is named.** |
| `-qwjfsw` — the bogus attach defect | **CLOSED** | — | Nothing owed. |

The **cross-repo verb-vocabulary dependency** — `contracts.md`'s clause that the
per-state vocabulary is owned by `livespec-orchestrator-beads-fabro` and "not yet
consumed here" — transfers to **01**, which now consumes it mechanically: the parity
fixture `tests/fixtures/drive-human-action-surface.json` pins the orchestrator's
published human action surface bidirectionally, and the reverse arm is
mutation-demonstrated red. Two upstream divergences and the reject-guard divergence are
recorded there rather than left as prose.

The thread's two research records — `l4p3ce-handoff-transport.md` and
`verb-vocabulary-propose-change-draft.md` — travel with the archive and are cited from
here so they remain reachable from the live arc.

### OPEN, AND IT NEEDS A PROPOSE-CHANGE: the archive left a dangling spec citation

**`SPECIFICATION/contracts.md`'s TUI driver-handoff clause cites the transport research
at its PRE-ARCHIVE path** — `plan/operator-surface-redesign/research/l4p3ce-handoff-
transport.md`, which no longer exists. The move created this; it is real rot, and it is
NOT fixed.

**It cannot be fixed by editing the file, and that was demonstrated rather than
assumed.** A direct one-line path correction was attempted in this PR and the pre-push
gate refused it, twice over:

- `doctor-out-of-band-edits` — `out-of-band edits detected at HEAD against history/v037:
  contracts.md`. The spec tree is gated; direct edits are exactly what it forbids.
- `check-behavior-coverage` — `clause not linked to a scenario [gap-vvl5pllp]`. Clauses
  are content-linked to their scenarios, so changing the text broke the link.

The doctor also auto-materialized a synthetic `SPECIFICATION/history/v038/` snapshot
(`out-of-band-edit-2026-08-02t06-54-17z`) capturing the edit. Both the edit and that
snapshot were reverted; the spec tree in this PR is byte-identical to master.

**The reasoning that led there, recorded because the inference was WRONG and the shape
of the error is reusable:** the same file already cites
`plan/archive/work-item-state-machine/`, which looked like precedent for updating a
citation on archival. It is not — that citation reached the spec through a proper
`revise` pass, not a direct edit. **An existing state is evidence of what is allowed to
BE, not of how it is allowed to GET there.**

The fix belongs in a `/livespec:propose-change` → `/livespec:revise` pass. **FILED
2026-08-02** as `SPECIFICATION/proposed_changes/archived-plan-thread-citations.md`.

**CORRECTION 2026-08-02 — the sentence that stood here was WRONG.** It said contracts.md
"also cites `plan/needs-attention/`, which does not exist either". That citation is
explicitly qualified `repo thewoolleyman/livespec`, so it is a CROSS-REPO reference, not
a path in this repository, and calling it a local dangling reference was a
misreading — I matched the path text without reading its repo qualifier.

What is actually true, verified against the livespec marketplace checkout: the thread was
archived UPSTREAM, and the file now sits at
`plan/archive/needs-attention/research/design.md`. So it IS stale, by the same
archived-thread mechanism, one repo over. Both citations are covered by the filed
propose-change — but as two instances of one mechanism, not as two local danglers.

Worth keeping rather than silently fixing, because the error has a shape:
**a grep for a path matched, and the qualifier that changed its meaning sat just before
the match.** The archived predecessor thread recorded the mirror image of this —
"an absence never announces itself in a grep for the wrong token".

## Why this note exists rather than a link

`plan/console-happy-path-mvp/` recorded a named pattern the hard way: **a correction
that reached one document and not its twin.** A decision that lives only in a supervisor
brief is a decision that has to be re-derived by whoever asks next. The verbatim quotes
are duplicated into `SPECIFICATION/proposed_changes/menu-primary-operator-ux.md` and
into plan 01's handoff deliberately, so no single deletion loses the constraint.

## The related premise correction — DECIDED 2026-08-02

The same session that produced this decision also established, by measurement, that
**groomed slices are Dispatcher-admitted** (`admission_policy=auto`), so
`plan/console-happy-path-mvp/`'s Mission text — "slices admitted at the approve valve" —
describes behaviour the system does not have.

**The maintainer AMENDED plan 04's mission to system reality on 2026-08-02:**

> groom → ready (Dispatcher-admitted) → menu-driven dispatch → acceptance → accept

with the approve valve proven SEPARATELY on manual-admission items. The visible
assumption flag has been REMOVED from plan 04's handoff. The declined alternative was a
spec change making groomed slices manual-admission — recorded so it is not re-proposed
as though unconsidered.
