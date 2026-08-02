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

## Inherited custody — ACCEPTED 2026-08-02

From `plan/archive/operator-surface-redesign/`, absorbed and archived on the
maintainer's ruling. **This section IS the acceptance** — the discipline that thread and
`plan/console-happy-path-mvp/` both record is that "another plan owns it" is not a
handoff; a handoff is complete only when the successor confirms it. Confirmed here.

| item | what it asks for | status at transfer |
|---|---|---|
| `-zweohm` | lane items expose no state-appropriate next action; each lane/status maps to its actually-valid verbs and the item surface presents exactly that set | backlog |
| `-vc7lmq` | the detail pane should offer only state-valid commands (redesign half; the immediate defect shipped separately as `-qwjfsw`, closed) | backlog |
| `-l4p3ce` | a paradigm for handing off to an LLM driver session. **The transport SHIPPED** as the `h` driver-handoff overlay (`-cxu4eu`); what remains here is menu-driven invocation of heavyweight verbs | backlog |

**Why these land on 02 and not 01:** the availability predicate they depend on already
shipped in 01 (registry predicates, multi-dimensional, consumed by both presentation and
invocation — the `-0uw` fix). What is left is *presentation of exactly the valid set*,
which is the menu. Generating menus from the registry taxonomy is this plan's mission,
so these are not extra scope; they are the same work stated as grievances.

**Carry forward, measured and still true:** `valve_open_input` gates only on "is a
work-item selected", never on `item.lane()` — so `p`/`c`/`r` all fire on a backlog item
where they are meaningless. Generalize from `s` (move-status), which already consults
`status_move_targets(lane)` and returns `None` when a lane has no drivable target.
`RejectMode::Regroom` is a REJECT mode, semantically the opposite of grooming a backlog
item — it is not the groom transport, but it is shipped and the design must account for
it.

**A live hazard for this plan specifically:** the shipped binary never sends the
driver-handoff OSC 52 copy — only the deferred test sink handles `CopyDriverHandoff`,
while the overlay says "copy sent to terminal". Recorded in plan 01's resume block.

## Registry readiness — MEASURED 2026-08-02, before this plan opens

This plan generates menus FROM the registry, so the registry's actual shape is this
plan's input. Measured against master `69ea9d4` and the unmerged slice A, so the
first session does not discover it mid-flight.

**GOOD — `menu_path` shipped day one, exactly as plan 01 requirement 1 demanded.** It is
`&'static [&'static str]` (`action_registry.rs:104`), a REAL two-level taxonomy, not a
flat placeholder: all ten master entries carry one, and a registry test asserts
non-emptiness for every entry. Groups today: `Work item > Hand off`,
`Work item > Lifecycle` (4), `Work item > Policy dials` (5). **No schema migration is
owed for the taxonomy** — the sequencing worked.

**BUT THE HOTKEY FIELD DID NOT GET THE SAME TREATMENT, and this one gates you.** On
master `pub hotkey: char` is MANDATORY (`:100`), so **a menu-only action is not
expressible in the registry at all.** Slice A widens it to `Option<char>` and adds the
first `hotkey: None` entry (`set-workflow-scope-override`). That widening is a schema
migration of exactly the kind requirement 1 existed to prevent — it simply landed one
slice later instead of never. **Practical consequence: until slice A merges, this plan
cannot generate a menu entry for any action that has no key.** Slice A is currently
BLOCKED (see plan 01 — the `-3yx` coverage phantom), so treat that as a real dependency
and not a formality.

**THE SCOPING QUESTION THIS PLAN MUST ANSWER FIRST — "every action" is not yet defined.**
The maintainer's proof obligation is *"some mechanical, generic test to prove that EVERY
action can be driven via MENUs, and hotkeys are ONLY ADDITIONAL."* Measured, the registry
holds TEN entries, ALL under a single top-level node `Work item`, and every one is a
per-item verb. The structural keys — `/` search, `:` palette, `?` help, `q` quit, space,
and Tab/arrow focus movement — are **deliberately excluded**: they are matched ahead of
the registry lookup in `key_event_to_terminal_input`, and `action_registry.rs:475`
actively FORBIDS them as registry hotkeys.

So a registry-generated menu covers per-item verbs and nothing else, and the completeness
test can be written against either of two very different populations:

- **every REGISTERED action** — mechanical, provable today, and arguably what the registry
  is for; but it makes the test true by construction and says nothing about navigation.
- **every operator-reachable BEHAVIOUR** — matches the plain meaning of the maintainer's
  words, and requires either registering the structural keys or defining them out.

**Do not pick this silently.** It determines whether the completeness gate is a real
verifier or a tautology, and this repo's standing rule is that a verifier must be able to
fail. Raise it with the maintainer at open, with this measurement attached.

## Implementation mode — RULED 2026-08-02

**Implement IN-SESSION**: worktree → PR → **full gates** → rebase-merge. That is the
default for every slice in this plan.

**Factory dispatch is the exception, not the fallback**: use it only for well-bounded,
sandbox-safe slices, and **record the choice per slice** with the reason. A slice that
touches host-coupled surfaces, plugin resolution, or anything under `.github/workflows/`
is not sandbox-safe — see the known live hazards in
`plan/01-action-registry-and-invoker/handoff.md`.

Recording the mode here rather than leaving it as session convention, because the two
routes have different evidence obligations and a successor cannot infer which was used
from the merge alone.

## Ledger

Tracks `-2ckgiy`. Blocked by `-dvv` (01). Blocks `-9nb` (04).
