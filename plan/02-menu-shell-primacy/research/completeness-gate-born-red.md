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
