# Slice B — build plan, from an aborted first attempt

Written 2026-08-02 after taking slice B far enough to MEASURE the work and then
backing it out cleanly rather than leaving a half-surgeried tree. Branch
`feat/slice-b-action-invoker` exists, based on slice A at `e396a32`, and is
CLEAN — no partial work is sitting on it.

## What the attempt established

**Base it on slice A, not master.** B's roster test
(`the_invoker_reaches_the_hotkeyless_scope_override_on_a_refused_ready_item`)
exercises A's `set-workflow-scope-override` entry, and the invoker is the only
surface that can reach a `hotkey: None` action at all.

**`git cherry-pick -n 66b4fd2` applies with exactly TWO conflicts, both small,
both resolving to HEAD.** Do not re-derive these:

| file | conflict | resolution |
|---|---|---|
| `action_registry.rs` (~`:501`) | slice A's `let keyed` restructure vs 66b4fd2's `assert_eq!(…, spec.hotkey.is_none())` with the message dropped | **keep HEAD.** A's version keeps the `"{}", spec.id` diagnostic AND is rustfmt-stable; 66b4fd2 solved the same pincer by discarding the diagnostic |
| `lib.rs` (~`:6270`) | test-module import list | **keep HEAD.** 66b4fd2's side adds `project_action_failures` and `work_item_failure_event` — those are slice **C** symbols and must not enter B |

That second conflict is a useful signal in itself: the import diff names C's
symbols explicitly, which is independent confirmation that C is separable.

**The real work is removing C, and it is 54 sites.** `66b4fd2` contains BOTH B
and C. Added lines matching `refusal`: **54**. After cherry-picking onto slice A
they land as 43 in `console-application/src/lib.rs`, 7 in `console-tui/src/lib.rs`,
5 in `console-cli/src/lib.rs`.

## The removal list (slice C — must NOT be in B)

Production:
- `OrchestratorActionOutcome::Failed { refusal }` and the `ActionFailure` type
- `project_action_failures`, `work_item_failure_event`
- the `error_json` refusal capture in `console-cli`
- the record-modal `Last action: … refused — …` render (`console-tui`, the
  `Last action:` site)

Tests (from `research/slice-split-test-partition.md`):
`project_action_failures`, `work_item_failure_event`,
`dispatcher_action_port_failure_without_stdout_carries_no_refusal`,
`a_failed_action_outcome_threads_its_refusal_into_the_failure_event`,
`a_refused_valve_persists_its_refusal_into_the_failure_event`,
`the_model_exposes_the_projected_failure_and_the_drilled_hint_derives`,
`action_failure_display_line_covers_every_refusal_shape`,
`action_failures_project_the_latest_failure_and_clear_on_recovery`,
`the_record_modal_renders_the_latest_refusal_for_its_item`, and the C half of
`the_new_variants_carry_their_derived_debug_forms`.

**B does not need any of it** — measured earlier and recorded in plan 01's
handoff: the invoker overlay references `refusal` NOWHERE, and the refusal
renders in the record modal, a different surface sharing no rendering path.

## Order of operations

1. `git cherry-pick -n 66b4fd2`, resolve the two conflicts to HEAD as above.
2. Remove the C production symbols, then the C tests, then re-run
   `cargo test --workspace --all-features --lib` until it compiles clean. Expect
   the compiler to find stragglers — that is the cheap direction.
3. Check the docs hunk: `66b4fd2` edits `docs/detailed-usage.md` for BOTH the
   invoker prose AND overlay hint rows. Keep the invoker half; C's record-modal
   line goes with C.
4. **The palette claim is a known doc change**: `detailed-usage.md:386-388` says
   the palette accepts "exactly two commands". B makes it three (`actions`).
   That sentence MUST change in B, and the 2026-08-02 doc-custody audit flagged
   it forward precisely so it is not "corrected" early.
5. Full gates. **B is expected to hit the `-3yx` phantom**; if it shows the
   IDENTICAL unnameable signature it passes under the recorded disposition
   (`tests/fixtures/coverage-unnameable-disposition.json`). **A NAMEABLE miss is
   ordinary work and the disposition does not cover it** — read the misses before
   assuming, exactly as slice A's three-step measurement did.
6. If B's unnameable count EXCEEDS the allowance of 1, the gate fails by design.
   Do NOT raise the number; take it to the maintainer with the measurement.

## Why this was banked instead of finished

The removal is 54 sites across three crates, followed by full gates and a PR.
Started with too little context left to finish it safely, and a partially-removed
slice C would compile-fail in ways a successor would have to reverse-engineer.
The conflict resolutions above are the expensive part of what was learned; the
surgery itself is mechanical once they are known.
