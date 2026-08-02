# 02-menu-shell-primacy — charter

**Epic anchor:** `livespec-console-beads-fabro-et3` — status is READ from the ledger.
**Blocked by:** `livespec-console-beads-fabro-dvv` (plan 01) — a LEDGER EDGE, not prose.
Opened 2026-08-02.

## Mission

Menus become the **PRIMARY** navigation mechanism, per the maintainer's ratified-pending
decision (quoted verbatim in plan 01's handoff and in
`SPECIFICATION/proposed_changes/menu-primary-operator-ux.md` — do not paraphrase it).

- Menu bar + submenus **GENERATED from the registry taxonomy** 01 shipped. Not
  hand-authored; a hand-authored menu tree is a second encoding and reintroduces the
  defect class `-mbohw3` exists to kill.
- Every menu item opens the **same forms 01 built**. No parallel dialog layer.
- Hotkeys displayed **beside menu items as accelerators**, which is what makes their
  "additional" status visible rather than merely asserted.
- Status band **re-pointed** at the menu system.
- Help modal and the operator docs' key/action reference **GENERATED from the registry**.
  This closes the `-2ckgiy` doc-rot class outright: a verb that ships undocumented
  becomes impossible when the docs are derived.

## Why this handoff is thin, deliberately

This is a **CHARTER**, not a full handoff: mission, scope, milestone acceptance
(including the dogfood leg), dependencies, and the ledger items it owns. Detail is added
when this plan OPENS.

**That unevenness is the anti-yak-shave mechanism, not laziness.** Writing 40 pages of
design for a milestone two steps away is exactly the rabbitholing this numbering exists
to prevent, and it would be written against a registry that does not exist yet — so it
would be wrong as well as premature. Fill this in when you open it, from what 01
actually shipped.

## In scope / out of scope

**In:** menu widget and layout, accelerator display, Status-band re-point, Help-modal
generation, docs generation, re-pointing the six `docs_*_lockstep` gates at generated
output.

**Out:** anything touching dispatch behaviour (03 owns it — the drain still freezes the
cockpit during this plan, and that is expected, not a regression). Out: the walk itself
(04). Out: growing the 01 invoker — it becomes a completeness surface behind menus.

## Milestone acceptance

1. The maintainer's mechanical test: a **generic E2E traversal proving EVERY registered
   action is drivable via a menu path** on the hermetic fixture. Generic, not a
   hand-listed set — a hand-listed set is the same second-encoding defect wearing a test
   costume.
2. **Hotkeys provably additional**: a test build with every hotkey binding DISABLED
   leaves every action reachable. That is the strongest available form of "only
   additional", and it is cheap once menus are generated.
3. The six `docs_*_lockstep` gates re-pointed and green against generated output.
4. Every new gate **MUTATION-DEMONSTRATED RED**, exit codes read UNPIPED, tree restored.

## Dogfood leg

**One full lifecycle segment driven MENUS-ONLY at the real TUI.** Not hermetic. Record
which segment, and record any hotkey used as a FAILURE of the menus-only claim rather
than a convenience.

## Ledger

Tracks `-2ckgiy`. Blocked by `-dvv` (01). Blocks `-9nb` (04).
