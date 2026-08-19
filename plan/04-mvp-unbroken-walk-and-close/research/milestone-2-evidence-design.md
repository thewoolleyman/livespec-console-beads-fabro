# Milestone 2's evidence capture — design, measured 2026-08-20 at master `5c4fc3b`

Milestone acceptance 2 of this plan's charter reads:

> Evidence captured LIVE: the offered-actions state BEFORE each invocation, the
> confirmation's exact target read back before committing, and a ledger check
> after. **Never reconstruct a capture afterwards.**

This note designs that capture. It became designable only when plan 02's
rendering slice landed — "the offered actions" is not observable until a surface
offers them — and it is designed against the **permanent bar** per the
maintainer's ruling (a) of 2026-08-19, not against the overlay that ships today.
Section 4 records a defect found while doing this design that must be resolved
before the capture can mean anything.

## 1. What each of the three legs binds to, in code

The charter's three legs are not equally easy, and the reason is that they read
from three different places.

**Leg A — the offered-actions state before invocation.** Two candidate sources
exist today and they do not agree:

- `action_registry::available_hint_tokens(ctx)`
  (`crates/console-application/src/action_registry.rs:650`) — the availability-
  filtered set, but only for actions that HAVE a key and are not `Global`. Its
  own comment says menu/invoker-only actions have no key to hint and render in
  menus and the invoker roster instead.
- `action_registry::menu_tree()` (`:552`) — every action, grouped by
  `menu_path`, with NO availability filtering (see §4).

So "the offered-actions state" has to be *defined* before it can be captured.
This note defines it as **the availability-filtered set for the current
`ActionContext`**, because that is what the charter's word "offered" means to an
operator: what they may actually do to this item, here, now. A capture of the
unfiltered roster would be a capture of the registry, not of the offered state,
and it would be identical on every item — which is exactly the kind of evidence
that cannot fail and therefore proves nothing.

**Leg B — the confirmation's exact target, read back before committing.** This
one is already well-anchored, and deliberately so. `render_valve_confirm` is
passed `model.selected_work_item_id()`
(`crates/console-tui/src/lib.rs:1412-1424`), with a comment recording the reason:
the modal's consent target MUST read from the SAME source `Enter` dispatches on,
never from `detail()` alone, because in a drilled-in lane the dispatch acts on the
lane item and reading the Attention detail would show a different or blank target
and let the operator confirm against the wrong work-item. That makes the rendered
target a trustworthy read-back rather than a decorative label, so leg B's capture
is simply the modal's rendered text, taken before `Enter`.

**Leg C — the ledger check after.** Independent of the TUI: read the item's
status and the emitted action through `bd`, after the command settles.

## 2. The capture procedure

Per invocation, in this order, with nothing reconstructed afterwards:

1. **Before opening anything**, capture the pane. With the permanent bar this is
   a single `capture-pane` — the bar's contents are visible without being
   summoned, which is precisely why ruling (a) makes this capture cheaper and
   more honest than the overlay does (§3).
2. **Capture the offered set** for the selected item as defined in §1 — the
   availability-filtered set for the current `ActionContext`.
3. **Invoke through the menu**, and capture the confirm modal's rendered target
   text BEFORE pressing `Enter` (leg B).
4. **Press `Enter`**, then capture the pane again.
5. **Ledger check** (leg C): the item's status and the emitted action.

Each step's artifact is the terminal capture itself, timestamped, not a
transcription of it. The charter's "never reconstruct a capture afterwards" is
the whole point: a capture written from memory after the fact cannot distinguish
what the surface showed from what the operator believed it showed, and this
plan's entire value is that distinction.

## 3. Why the permanent bar changes this capture, not just its ergonomics

Against the shipped overlay, step 1 is not one capture but a sequence: press `v`,
capture, and the act of capturing has changed the screen. The offered-actions
state "before invocation" is then only observable in a state the operator had to
enter deliberately, which means the evidence cannot distinguish "these were the
offered actions" from "these were the offered actions once I opened the menu".

Against a permanent bar the bar's contents are part of the resting screen, so
step 1 is a capture of the state the operator was actually in. That is a
different claim, and a stronger one.

This is also why this plan's milestone 1 walk waits for the bar rather than being
performed through the overlay — recorded in `opening-measurement.md` §2b and not
repeated here.

## 4. A DEFECT that must be resolved before this capture can mean anything

Found while designing leg A, measured at `5c4fc3b`, and not currently tracked by
any ledger item (checked: `-k0w`, `-zbnnlv`, `-rha8`, `-yrs5`, `-zweohm`).

**The menu renders every action with no availability marking, and invoking an
unavailable one is a silent no-op.**

- `render_menu_overlay` (`crates/console-tui/src/lib.rs:2116-2177`) builds its
  rows from `menu_tree()` and formats each as
  `"{marker}   {} [{accelerator}]"`. There is no availability consultation
  anywhere in it. Contrast the ActionInvoker, which appends
  `"  (unavailable here)"` for exactly this reason (`:2086-2090`, asserted by
  `render_action_invoker_lists_every_action_with_availability_markers` at
  `:4935`).
- `menu_confirm_step` (`:492`) stages through
  `staged_without_selection(model, spec)`, and when that returns `None` — the
  unavailable case — it returns
  `TuiRuntimeStep::new(state.clone(), TuiRuntimeEffect::Render)`. No message, no
  marker, no event. The keypress is swallowed.

So an operator can open the menu on a `backlog` item, select `Accept work-item`,
press `Enter`, and observe nothing at all, with no indication that the action was
inapplicable rather than broken.

**Why this blocks milestone 2 specifically.** Leg A is a capture of "the offered
actions". If the menu offers all sixteen regardless of state, then the menu's
rendered state is not the offered state, and a capture of it is the same bytes on
every item in the walk. Evidence that cannot vary cannot corroborate anything.
Either the menu must mark availability, or leg A must capture a source other than
the menu — and capturing a source the operator cannot see would undercut the
menus-only claim the walk exists to make.

**Why it is not `-k0w`, which is the near-miss.** `-k0w` is scoped (maintainer
scope-cut, 2026-07-26) to the transient operator-feedback slot for command
OUTCOMES — failure and silent success of commands that were dispatched, with
`-ectqye` supplying the store-side half. The defect here is upstream of that: no
command is ever dispatched, so there is no outcome to surface. `-k0w`'s slot may
well be the right rendering vehicle for the refusal message once it exists, but
the missing availability marking in the menu is a distinct defect and belongs to
menu primacy.

**Disposition.** Not filed unilaterally from this thread: the menu surface is
plan 02's live slice, and filing into another plan's active design space risks a
duplicate or a competing cut of work already ruled. Recorded here with evidence
and reported to the foreman for plan 02 to take as a new item, an amendment to
`-rha8` (R4), or part of `-yrs5` (the permanent bar). It also bears on R4's own
premise: "every action reachable by menu with all hotkey bindings disabled" can
be satisfied while the menu remains unusable as a primary surface, because
reachable and applicable are different properties.

## 5. What this note does NOT settle

- The exact capture tooling for the walk (tmux `capture-pane` versus the e2e
  harness's fixture) — deliberately deferred until the bar exists, since the
  bar's layout determines what a single capture contains.
- Whether the walk is one sitting or needs a rehearsal pass. The charter says one
  sitting and D1 protects that; no rehearsal is planned.
