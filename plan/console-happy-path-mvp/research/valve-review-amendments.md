# Fable pre-dispatch reviews — the three items approved at the valve 2026-07-23

Four independent Fable reviewers audited the full `pending-approval` set
against master `ab6e567`, each verifying the filed diagnosis at the cited
file:lines (all confirmed; several citations had drifted and are corrected in
the blocks below). Verdicts: **APPROVE** for `-6hbfq6`, `-ipwtll`, `-u3w3er`;
**FLAG** for `-ectqye`.

**Status: acted on, 2026-07-23.** Each APPROVE block below was attached to its
work-item as a `bd` comment, and the approve valve was then fired on all three
(`pending-approval → ready`). They are now dispatchable; the factory should
read the item comments (or this file) before implementing.

**`-ectqye` was NOT approved and is left untouched** for its filing session
(the console-happy-path-mvp real-stack walk). Its FLAG: the Done criterion
assumes an "existing status surface" for command outcomes that does not exist —
the operator-visible half needs a NEW surface (or a store-side/TUI split), and
an automated factory given the current text could plausibly stuff error text
into the contractually-constrained hint line (contracts.md:653). Blockers:
populate `acceptance_criteria`; name the surface concretely or split the item;
add guidance that the diagnostic lives in the already-captured stdout (drive
runs `--json`; do NOT re-plumb stderr through `SourceProbe`).

The per-item amendment blocks follow, as reviewed.


---

## `-6hbfq6` — Help overlay focus-based navigation  → APPROVE (high)

**Attached to the item as a `bd` comment — pre-dispatch conditions:**

1. **Citation corrections** (the record's line refs drifted; one names the
   wrong crate):
   - Hint string `"up/down section | PgUp/PgDn scroll | esc close help"` lives
     at `crates/console-application/src/lib.rs` (~line 1501), NOT
     `crates/console-tui/src/lib.rs:1359`.
   - `HelpScrollDown`/`HelpScrollUp` at `console-application/src/lib.rs:719-725`
     (record said 669-675); `help_scroll` at `:5425` (record said 5062);
     `PageUp`/`PageDown` binding at `console-tui/src/lib.rs:518-519` (record
     said 511-512).

2. **Coordinated doc edit (mechanically enforced).** Changing the hint string
   requires co-editing `docs/detailed-usage.md:278` (hint-table row) and the
   prose key table at `docs/detailed-usage.md:386-387` in the SAME change —
   `docs_status_hint_lockstep` reddens otherwise. This one self-enforces in
   `cargo test`.

3. **Coordinated tmux-e2e edit (NOT caught by `cargo test`).**
   `tmux_tui_e2e_modal_help_scenario_18` (~`crates/console-cli/tests/tmux_tui_e2e.rs:248-260`)
   EXPLICITLY asserts left/right are inert inside Help. The new behavior makes
   left/right switch focus, so those assertions MUST change or
   `just check-e2e-tmux` reddens silently later (it is `#[ignore]`, invisible
   to a default test run).

4. **Update the Help overlay's own prose** that hardcodes old bindings
   (`console-tui/src/lib.rs:1681, 1725`) — covered by the item's
   "pane-specific help text must be updated" clause, enumerated so it is not
   missed.

5. **Design note.** Adopt the `WorkItemDetail` render-measured-rows pattern
   (renderer feeds measured height back into state, e.g. `work_item_detail_page_rows`)
   for the page-scroll clamp, rather than the current unbounded
   `saturating_add` + renderer-side clamp — the acceptance's "clamp at both
   edges" for PAGE scroll implies this convergence.

**No spec change needed:** `contracts.md:653` declares hint strings/bindings an
implementation detail; Scenario 18's constraints (right-pane text scrolls
up/down only, Esc-only close, 3-char border) all remain satisfied.

---

## `-ipwtll` — command-queue single-consumer  → APPROVE (high)

**Attached to the item as a `bd` comment — pre-dispatch conditions:**

1. **DECIDE the stale-`executing` recovery policy** (the one genuine
   underspecification). The atomic claim converts a crash-mid-execution from
   "silently re-executed" into "wedged forever", so recovery is in-scope, not a
   second change. Choose and state in the item:
   - **auto-requeue after a timeout** — reintroduces at-least-once for a
     slow-but-alive consumer; or
   - **mark-failed + needs-attention** — conservative.
   Content-stable outcome-event ids are the backstop either way.

2. **Cover two implementation details:**
   - the direct-drive path `record_drive_command` (`crates/console-cli/src/lib.rs:1377`)
     admits AND finalizes its own command in one flow — it must claim too, or
     be explicitly exempted;
   - `finalize_pending_command`'s status update should become conditional on
     `status='executing'` for symmetry-hardening.

3. **No schema migration** (`status text` has no CHECK constraint; `'executing'`
   is just a new value). WAL + `busy_timeout 5000` make the single-statement
   claim sound. Citation drift: `finalize_pending_command` is now ~`:1492`
   (record said 1451).

4. **Parking is still correct** — multi-client hardening, off the
   single-operator MVP critical path. Related item `-ble` (admission keys) is
   correctly non-blocking.

---

## `-u3w3er` — approve/accept retryability after failure  → APPROVE (high)

**Attached to the item as a `bd` comment — pre-dispatch conditions:**

1. **Prefer FAILURE-AWARE discrimination, not blanket repeatability.**
   Discriminate the idempotency key only when the existing static-key row is
   terminal-`failed`. Blanket repeatability would let a double keypress fire the
   valve twice (the second bounces off the orchestrator as `invalid-source-state`,
   producing a noise `failed` row). The failure-aware variant preserves the
   DOCUMENTED double-keypress absorption AND enables retry.

2. **MUST update the opposing regression test and doc comment** — they encode
   the exact behavior being changed; leaving them stale is worse than the bug:
   - test `distinguish_repeatable_command_distinguishes_drain_and_leaves_the_valves_alone`
     (`crates/console-cli/src/lib.rs:3689-3743`);
   - design-intent comments at `crates/console-cli/src/lib.rs:1578-1601`
     ("approve … and accept … are idempotent by design").

3. **Prefer the persist-time discrimination route** (mirroring move/drain's
   sequence-fold) over changing key CONSTRUCTION — fewer surfaces. Construction-
   site tests (`console-application/src/lib.rs:8640, 8776, 8792, 8844`) only bind
   if you change construction.

4. **Leave `reflect_autonomous_decision` untouched** (`console-cli/src/lib.rs:1355`)
   — it depends on `Duplicate` for idempotency and is a different command type;
   a valve-scoped change is safe.

5. **Add acceptance criteria** (currently null): (a) approve via a port stubbed
   to fail leaves the row at `failed`; (b) a second approve yields
   `CommandAppendStatus::Inserted` with an attempt-discriminated key, not
   `Duplicate`; (c) the new row dispatches and succeeds against a now-succeeding
   port; (d) an exact re-persist at the same sequence still dedupes (replay
   safety).

6. **Consider a follow-up item** for reject / set-admission / set-acceptance —
   same static-key trap; the item hedges but does not commit.

---

## Sibling coupling worth sequencing (for the happy-path session)

`-u3w3er` (retry is dead after a failed valve) and `-ectqye` (the failure is
invisible) sit at the SAME operator-feedback locus. Fixing one without the
other leaves a half-broken valve UX: surface the error but the retry still
no-ops, or make retry work but the operator never sees why the first attempt
failed. Recommend sequencing them together even though `-ectqye` is FLAGGED.
