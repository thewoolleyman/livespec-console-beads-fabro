# The menu-completeness gate, BORN RED — banked 2026-08-03

Per the standing rule (slice A's parity gate precedent): bank the red output BEFORE
making it green, because a gate never observed failing is not evidence.

Run before ANY binding was added, `cargo test --test menu_completeness`, **RC=101 read
unpiped**:

    Operator-reachable behaviours with NO menu path and NO argued exclusion:
      - KeyCode::Char('c')
      - KeyCode::Char('/')
      - KeyCode::Char(':')
      - KeyCode::Char('?')
      - KeyCode::Char('q')
      - KeyCode::F
      - KeyCode::Media
      - KeyCode::Modifier
    Registry currently holds 11 entries.

**The first five are the real finding and exactly what the ruling predicted**: `Ctrl-C`
quit, `/` search, `:` palette, `?` help, `q` quit are operator actions with no menu path.
A registry-row-quantified gate could never have named them — it would have quantified over
its own input and passed. This is the difference between a verifier and a tautology,
measured.

**The last three are a PARSER DEFECT, not a finding — do not "fix" them by adding
carve-out entries.** `KeyCode::F(_)`, `Media(_)` and `Modifier(_)` are inert arms already
mapped to `None`. They surface only because `handled_key_arms` and `inert_arms` tokenize
differently: the handled scanner keeps `Char(...)` whole but truncates `F(_)` to `F`,
while the inert scanner keeps the parens. **Fix: give both one shared tokenizer.** Adding
them to the carve-out would paper over the bug AND weaken the fixture, since they are not
behaviours at all.

## State at wind-down

Branch `feat/menu-completeness-gate`, **COMMITTED LOCAL-ONLY, NOT PUSHED** — the gate is
red by design, so pre-push (which runs the full suite) correctly refuses it, and
`--no-verify` is never an option here. This is the same posture milestone 2 took.

## Remaining, in order

1. Unify the two tokenizers so `F`/`Media`/`Modifier` drop out as inert. Then the red
   names exactly the five real keys.
2. Register those five with `menu_path`s. This introduces the FIRST top-level nodes beyond
   `Work item` — plausibly `View > Search`, `View > Command palette`, `Help`, `File > Quit`
   — i.e. the first real menu BAR (the ≥2-top-level-node design basis, approved).
   `Ctrl-C` needs a decision on whether a modifier chord is expressible as a registry
   hotkey (`hotkey: Option<char>` cannot carry the modifier today) — it may need
   `hotkey: None` with the chord kept as an accelerator-only annotation.
3. Re-run: green. Then MUTATION-DEMONSTRATE by deleting one carve-out entry and one
   registration, exit codes unpiped, tree restored.
4. Only then push, as one PR carrying this banked red.

---

## STEP 1 DONE — rebased and tokenizers unified, 2026-08-19

**Rebased first, and the red survived unchanged.** The branch sat unpushed for 16 days;
`feat/menu-completeness-gate` was rebased from its old parent `2924915` onto master
`4748daf` and the gate re-run BEFORE any edit, `cargo test --test menu_completeness`,
**RC=101 read unpiped** — byte-identical to the 2026-08-03 banking, all eight names. That
re-banking matters: a red banked against a 16-day-old tree proves nothing about today's
key handler, and the whole point of banking is that the evidence is trustworthy.

**Then step 1: one shared tokenizer.** `handled_key_arms` and `inert_arms` both now call a
single `key_token`, which takes the identifier chars and then, if the next character is
`(`, everything through the matching `)`. Re-run, **RC=101 read unpiped**:

    Operator-reachable behaviours with NO menu path and NO argued exclusion:
      - KeyCode::Char('c')
      - KeyCode::Char('/')
      - KeyCode::Char(':')
      - KeyCode::Char('?')
      - KeyCode::Char('q')
    Registry currently holds 11 entries.

**The red now names exactly the five real keys and nothing else**, which is what step 1
was for. `F(_)`, `Media(_)` and `Modifier(_)` drop out as inert, without a single
carve-out entry — the fixture is unchanged and therefore un-weakened. `check-format` and
`check-clippy` both RC=0.

## STEP 2 IS NOT MECHANICAL — a schema decision surfaced, 2026-08-19

Step 2 as written ("register those five with `menu_path`s") cannot be done as a data edit.
Measured on the current tree:

1. **`hotkey: Some(k)` is refused for four of the five.** `action_registry.rs:536` asserts
   no registry hotkey is one of `/ : ? q space`. The assertion is right — the handler
   matches those arms BEFORE the registry lookup, so a registry claim on them would be a
   lie.
2. **`hotkey: None` erases the Status-band hint.** `action_registry.rs:531` asserts
   `hint_token.is_empty() == hotkey.is_none()`. So a `hotkey: None` registration for `q`
   MUST carry an empty hint token, and the live band's `q quit` would have to come from
   somewhere else.
3. **`Ctrl-C` is not expressible either way.** `hotkey: Option<char>` cannot carry a
   modifier.
4. **Registering without rewiring creates a SECOND ENCODING.** `/`, `:`, `?` and `q` are
   matched at `console-tui/src/lib.rs:571-574`, ahead of the
   `KeyCode::Char(value) => action_for_hotkey(value)` arm at `:576`. A registry row for
   `/` that the handler never consults is exactly the parallel-encoding defect this arc
   exists to retire — the gate would go green while the defect got worse.

**The root cause is one conflated field.** `hotkey` currently means BOTH "the key that
dispatches this action" and "the key we print next to it". Those are different concerns,
and every one of the four problems above is that conflation showing:  a display-only
accelerator (`Ctrl-C`, `q`) has no business claiming the dispatch slot, and the assertion
at :536 exists precisely because the dispatch meaning must stay honest.

Separating them — keeping `hotkey` as DISPATCH and adding an accelerator for DISPLAY — is
also exactly what the charter's "hotkeys displayed beside menu items as accelerators"
requirement needs, so the split is owed by the mission regardless. This is a change to the
registry schema, which is this plan's spine, so it goes to the maintainer rather than being
picked in-session.

## STEP 2 DONE — chords, and the gate goes GREEN, 2026-08-19

**Maintainer ruling: widen to a chord type.** `hotkey: Option<char>` is replaced by
`hotkeys: &'static [KeyChord]`, where `KeyChord { ctrl: bool, key: char }`.

**Why a SLICE and not a single chord** — this was not in the ruling and is worth stating,
because it is a forced consequence rather than a preference. `q` and `Ctrl-C` are the SAME
action. Modelling them as two registry entries would render `Quit` twice in every generated
menu, which is the thing menu generation exists to prevent. One entry, two accelerators.

**The chord type paid for itself immediately, in a way the `char` field could not have.**
`c` accepts a work-item; `Ctrl-C` quits. As bare chars those are the same key — so the old
field could not have carried `Ctrl-C` even if the modifier problem were solved some other
way. `action_for_chord(KeyChord::plain('c')) == accept` and
`action_for_chord(KeyChord::ctrl('c')) == quit` are now both asserted.

**The four handler arms are DELETED, not mirrored.** `KeyCode::Char('/')`,
`Char(':')`, `Char('?')` and `Char('q')` at `console-tui/src/lib.rs` are gone, along with
`slash_input`, `colon_input`, `question_input` and `q_input`; the generic
`KeyCode::Char(value)` arm resolves all four through `action_for_chord`. The `Ctrl-C`
pre-match is likewise gone, replaced by `control_chord_input`, which resolves any
Control-held chord through the registry and is honoured regardless of overlay so `Ctrl-C`
still quits from inside the search field.

**That deletion is the whole point.** Registering the five while leaving the arms in place
would have turned the gate green over a MIRROR — the key-to-action mapping would still
have lived in the handler, with the registry merely restating it. The gate would have
reported success while the defect got worse.

**One behavioural equivalence had to be checked rather than assumed**, because
`question_input` looked like it did something special. Its modal arm returned `None` for
`Help`/`WorkItemDetail`/`ActionInvoker`/`DriverHandoff`, and `text_input` returns `None`
for those same overlays (it yields `Some` only for `Search` and `CommandPalette`). So
`question_input(overlay)` was already exactly `if overlay == None { OpenHelp } else {
text_input('?', overlay) }` — the elaborate match was documentation, not different
behaviour. The generic arm reproduces it precisely.

**Global actions need no selection**, so `ActionStaging::Global(GlobalAction)` is answered
before an `ActionContext` is demanded, in both the key path and the invoker roster. The
invoker previously `zip`ped the spec with `selected_action_context()`; left alone, that
would have made search, the palette, help and quit inert on an empty lane — in the one
surface whose entire purpose is that every registered action is reachable.

**The first top-level menu nodes beyond `Work item` now exist**: `View > Search`,
`View > Command palette`, `Help > Keys and actions`, `File > Quit`. The menu bar is real
rather than a single-node degenerate case, which is the ≥2-node design basis the ruling
predicted.

**Deliberately NOT done here:** the Status band still hard-codes `? help | q quit` at
`console-application/src/lib.rs:1862+`. The four globals carry their `hint_token`s, but
`available_hint_tokens` filters `ActionStaging::Global` out, so the per-item hint row and
the six `docs_*_lockstep` gates are untouched. Re-pointing the band at those tokens is the
Status-band slice's job; doing it here would have merged two slices and put the docs gates
in play for no gain.

Results: `cargo test --test menu_completeness` **RC=0, both tests green**. Full workspace
suite: **42 test targets, 0 failures**. One fixture change was needed —
`tests/fixtures/drive-human-action-surface.json` records the four globals under
`console_local_actions` with reasons, since they have no orchestrator `drive` verb and
never could.

## STEP 3 — MUTATION DEMONSTRATION, 2026-08-19

Four mutations, each applied to a green tree, exit codes read UNPIPED (`cargo`
run bare, `$?` captured immediately, output redirected to a file rather than
piped), tree restored after each and re-verified green.

| # | mutation | completeness gate | other gates |
|---|---|---|---|
| — | baseline, unmutated | **RC=0** | registry invariants RC=0 |
| A | delete the `Tab` carve-out entry | **RC=101**, names `KeyCode::Tab` | — |
| B | delete `menu_path` from the `approve` entry | not applicable | registry invariants **RC=101** at `:698`, naming `approve` |
| C | add `KeyCode::Char('X') => Some(Quit)` to the handler | **RC=101**, names `KeyCode::Char('X')` | — |
| D | delete the whole `quit` registry entry | **RC=0 — GREEN** | registry invariants **RC=101**; console-tui **RC=101** |
| — | restored | **RC=0** | registry invariants RC=0 |

Mutation A proves the fixture arm fails when an argued exclusion is withdrawn.
Mutation C proves the FAIL-CLOSED property, which is the one the gate exists for:
a key arm added tomorrow that is neither registered nor argued is a red build.

**MUTATION D IS THE FINDING, and it corrects an assumption in this plan's own
milestone text.** The milestone says to mutation-demonstrate by "deleting one
carve-out entry and one registration". Deleting a registration NO LONGER reddens
this gate, and that is a direct consequence of doing step 2 honestly rather than
a defect.

The gate's population is the key handler's literal arms. Before the rewire, `/`,
`:`, `?` and `q` WERE literal arms, so un-registering one put it back in the
population and turned the gate red. After the rewire they are resolved by the
generic `KeyCode::Char(value)` arm, which the gate special-cases as the registry
dispatch arm — so deleting the `quit` entry makes `q` a plain literal character
and the gate sees nothing to complain about.

**That is sound, but only because two invariants COMPOSE, and neither is
sufficient alone.** The gate covers the handler side: nothing outside the
registry is operator-reachable without an argued exclusion. `action_registry.rs`
covers the registry side: `assert!(!spec.menu_path.is_empty())` means everything
INSIDE the registry carries a menu path by construction. Together they give the
milestone's property. Measured, mutation D was caught by
`hotkey_and_id_lookups_round_trip_every_entry` and
`only_global_staged_actions_answer_the_global_chord_lookup` (both added in step
2), and by `keymap_maps_quit_and_ignores_unhandled_keys` and
`the_invoker_quits_from_the_quit_row` in console-tui.

**Do not "fix" this by re-adding literal arms so the gate can see them** — that
would reinstate exactly the second encoding step 2 deleted. Record it instead, so
nobody reads a green completeness gate as proof that a registration still exists.

## STEP 4 — coverage, and what it forced, 2026-08-19

The first full `just check` failed `check-coverage` with 8 nameable uncovered
lines, ALL of them this slice's new code. Four were genuinely untested reachable
paths; four were structurally DEAD arms. The dead ones were removed by
restructuring rather than covered by contrived tests:

- `global_input` was split into `global_interaction` (returning
  `Option<TuiInteraction>`, `None` for quit) plus a thin wrapper, so the invoker
  can branch on interaction-or-quit without a `TuiTerminalInput::Confirm` arm
  that nothing can reach.
- `global_action_for_chord` moved into `action_registry`, so
  `control_chord_input` no longer carries a `Valve | DriverHandoff => None` arm
  that is unreachable in the TUI (no per-item verb has a Control chord). In the
  registry that same arm IS reachable and is now tested: plain `p` resolves to
  the approve VALVE, so the global lookup declines it.
- `staged_without_selection` became one shared stager for both the key path and
  the invoker, so the `StagedAction::Global` arm is reached from both instead of
  being dead in one of them.

The four genuinely-untested paths got real tests, each covering behaviour worth
pinning on its own: the invoker reaching a global action with NOTHING selected
and an EMPTY event set, the invoker returning the Quit effect from the quit row,
and Control chords on a non-character key and on an unbound character both being
inert rather than swallowed.

**A miss worth recording about method, not code.** The registry test module's
import line was patched by exact-string replacement AFTER `cargo fmt` had already
rewrapped it, so the replacement silently matched nothing. `cargo test -p
console-tui` still passed, because it does not build console-application's TEST
target — the break was invisible until the mutation run's own BASELINE came back
RC=101 on an unmutated tree. Banking the baseline is what caught it: had the
mutation script only run the mutated case, its RC=101 would have read as a
successful demonstration when it was really a compile error.
