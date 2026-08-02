---
topic: menu-primary-operator-ux
author: claude-opus-5
created_at: 2026-08-02T02:27:58Z
---

## Proposal: Menus and dialogs are the primary operator navigation mechanism; hotkeys are additional

### Target specification files

- SPECIFICATION/contracts.md
- SPECIFICATION/scenarios.md
- SPECIFICATION/non-functional-requirements.md

### Summary

Ratify menu-and-dialog operation as the FIRST-CLASS, REQUIRED, PRIMARY navigation
mechanism for every operator action in the console, with hotkeys admitted only as an
ADDITIONAL power-user convenience. Today the contract is hotkey-primary: the Status-line
hint clause and the per-item verb vocabulary describe keys, and there is no menu surface
at all.

### Motivation

This is a maintainer decision, recorded verbatim so the constraint is not softened by
paraphrase:

> "I would rather every action be available via MENUS and DIALOGS as the first-class,
> required, primary navigation UX mechanism. And hotkeys are only provided IN ADDITION
> to first-class menu operation AS A POWER-USER CONVENIENCE."

and, on how it must be proven:

> "ideally some mechanical, generic test to prove that EVERY action can be driven via
> MENUs, and hotkeys are ONLY ADDITIONAL."

The decision is not a compromise position reached under constraint; the maintainer
confirmed it independently: "I'm already decided on menu primary - definitely want this,
it's a better UX for me, and for agents."

Three measured defects on the current hotkey-primary surface motivate the structural
half of this proposal, and each is a symptom of the same cause — action availability and
action invocation are encoded per-key rather than once:

- `livespec-console-beads-fabro-0uw`: the Status band advertised `p approve` on a
  `pending-approval` item whose effective admission policy is `auto`, where the valve
  provably cannot fire. The availability predicate keyed on LANE only. Pressing the
  advertised key did nothing, twice.
- `livespec-console-beads-fabro-w7d`: `set-workflow-scope-override:<id>:citation-only`
  is a human valve the orchestrator defines and instructs operators to use, and the
  console binds NO key to it. An operator whose dispatch is refused for factory-safety
  cannot clear it without leaving the cockpit — which defeats the console's purpose.
- `livespec-console-beads-fabro-2ckgiy`: the `h` driver-handoff verb shipped completely
  undocumented, and no existing gate could catch it, because the hint-lockstep gate is
  deliberately one-directional and the completeness arm only requires rows to exist.

A key-indexed surface can only grow these defects: every new action needs a free key, a
hint-table row, a docs row, and a predicate, each maintained independently. A registry
with a menu taxonomy makes the action set enumerable, so availability, presentation and
documentation are derived rather than restated.

### Proposed Changes

Add to contracts.md a new normative clause, "Menu-primary operator navigation":

1. Every operator action the console offers MUST be reachable through the menu system.
   Menus and dialogs are the first-class, required, primary navigation mechanism.
2. Hotkeys MUST be provided only IN ADDITION to menu operation, as a power-user
   convenience. A hotkey MUST NOT be the sole route to any action. Removing every
   hotkey MUST leave every action reachable.
3. The console MUST maintain a single ACTION REGISTRY as the sole source of truth for
   the operator action set. Each entry MUST carry: a stable action id; a human label; a
   parameter schema; an availability predicate; a handler; and a menu path / category
   taxonomy.
4. The availability predicate MUST be able to depend on the full item state required by
   the action — including at least lifecycle lane AND effective admission policy — and
   MUST be the single derivation consumed by BOTH the presentation layer (whether the
   action is offered) and the invocation layer (whether the action fires). A surface
   that offers an action which cannot fire is a contract violation, as is an action that
   fires while not offered.
5. Menus, hotkey bindings, Status-line hints, the in-app Help modal, and the operator
   documentation's key/action reference MUST be DERIVED from the registry, not restated
   beside it. A second, independently-maintained encoding of the action set is a
   contract violation. (This generalizes the existing hint-honesty clause rather than
   replacing it.)
6. When an action is invoked and refused, the console MUST render the refusal to the
   operator. Refusal payloads returned on the orchestrator's `--json` surface are
   structured and actionable — one names the exact command that would unblock the
   operator — and discarding them is a contract violation.

Add to scenarios.md the mechanical coverage the maintainer requires:

- Given the console is built with every hotkey binding disabled, When the operator drives
  the session, Then every registered action remains reachable through a menu path.
- Given the registry, When the generic menu-traversal test runs, Then for EVERY
  registered action there exists a menu path that reaches its dialog.
- Given a registered action whose availability predicate is false for the selected item,
  When the operator opens the menu, Then that action is presented as unavailable and its
  hotkey is inert.
- Given an action is invoked and the orchestrator refuses it, When the refusal returns,
  Then the console renders the refusal detail without the operator leaving the cockpit.

Add to non-functional-requirements.md: the registry MUST be covered by a cross-repo
PARITY check asserting it accounts for the orchestrator's published human action
surface, so an action the orchestrator gains and the console never binds is a red build
rather than a silent gap. The check MUST be able to fail — it is expected to be born red
against the currently-unbound valve actions, and that is its red demonstration.

### Notes for the revise pass

This proposal deliberately does NOT specify the menu widget, key-accelerator display, or
menu-bar layout: those are presentation decisions for the implementing plan
(`plan/02-menu-shell-primacy/`). What is offered for ratification is the PRIMACY rule,
the single-registry rule, the derived-surfaces rule, and the refusal-rendering rule.

Filed by `plan/01-action-registry-and-invoker/`. Ratification is the maintainer's revise
pass; this file does not ratify itself.
