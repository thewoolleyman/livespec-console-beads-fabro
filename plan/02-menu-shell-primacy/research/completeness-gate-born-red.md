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
