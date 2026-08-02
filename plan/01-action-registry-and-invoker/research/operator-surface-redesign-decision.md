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

**That thread's own disposition — absorb into this arc, or archive — is NOT this plan's
call and NOT the worker's.** The supervisor is raising it with the maintainer. Until it
returns, do not edit that thread, do not archive it, and do not file impl items under
its epic.

## Why this note exists rather than a link

`plan/console-happy-path-mvp/` recorded a named pattern the hard way: **a correction
that reached one document and not its twin.** A decision that lives only in a supervisor
brief is a decision that has to be re-derived by whoever asks next. The verbatim quotes
are duplicated into `SPECIFICATION/proposed_changes/menu-primary-operator-ux.md` and
into plan 01's handoff deliberately, so no single deletion loses the constraint.

## The related premise correction, which is NOT decided

The same session that produced this decision also established, by measurement, that
**groomed slices are Dispatcher-admitted** (`admission_policy=auto`), so
`plan/console-happy-path-mvp/`'s Mission text — "slices admitted at the approve valve" —
describes behaviour the system does not have. Plan 04 carries the amended walk as a
VISIBLE ASSUMPTION pending maintainer confirmation. It is not settled, and nothing in
this arc should treat it as settled.
