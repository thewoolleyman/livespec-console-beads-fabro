# The A/B/C split is a RE-PARTITION, not a cherry-pick — measured

Written before building slice A, because the obvious shortcut is wrong and it would fail
late (at pre-push, after the work) rather than early.

## The shortcut that does not work

`e3d82eb` looks like slice A already exists as a commit: it carries the parity gate, the
fixture, and the whole `set-workflow-scope-override` binding. Cherry-picking it onto
master looks like a one-command slice.

**It would be coverage-red with REAL misses** — not the phantom, ordinary ones.
Measured on the rebased branch:

| commit | new `#[test]` fns | lines mentioning scope-override | production churn |
|---|---|---|---|
| `e3d82eb` (A) | **3** — exactly the three parity arms | 42 | `lib.rs` +79, `action_registry.rs` +77, `console-cli` +33, `console-domain` +14 |
| `66b4fd2` (B) | **20** | 32 | the invoker |

Slice A's commit adds **zero** inline test functions to `console-application/src/lib.rs`
against 79 added production lines there. Its code is exercised by tests that live in the
INVOKER commit — which is not an accident: `set-workflow-scope-override` ships
`hotkey: None`, so before the invoker existed there was no operator path to it, and the
tests that reach it were written alongside the surface that reaches it.

**So the split must move tests with their subject.** That is what "each slice
self-contained WITH its tests" meant, and this table is the reason it was not a
formality.

## The partition

Classified by subject, from the test names and their bodies. Verify each against its
body before moving it — this is a reading, not a proof.

**To slice A — the scope-override valve and its spine:**

- `workflow_scope_override_valve_maps_onto_its_action_id_and_payload`
- `workflow_scope_handler_maps_the_payload_onto_the_action_id`
- `workflow_scope_handler_rejects_an_empty_work_item_id_without_invoking_the_port`
- `workflow_scope_handler_rejects_bad_payloads_without_invoking_the_port`
- `the_workflow_scope_override_rides_the_spine_to_its_action_id`
- `every_registered_action_stages_from_some_admitting_context` (registry-wide; A adds a
  registry entry, so this must be green in A)
- `the_erroring_port_inherits_the_honest_not_wired_read_default` and
  `dispatcher_action_port_is_not_wired_when_unavailable` — port-default coverage on the
  cli test port; check the bodies, they may straddle A and C

**To slice B — the invoker:**

- `the_palette_actions_query_opens_the_invoker_and_the_roster_moves_and_clamps`
- `the_palette_actions_command_opens_the_invoker_and_enter_stages_the_selection`
- `render_action_invoker_lists_every_action_with_availability_markers`
- `the_invoker_stages_the_driver_handoff_from_its_row`
- `the_invoker_reaches_the_hotkeyless_scope_override_on_a_refused_ready_item` — B-side
  even though it exercises A's action; it is a test OF the invoker's reach

**To slice C — the `-ectqye` refusal half:**

- `project_action_failures`
- `work_item_failure_event`
- `dispatcher_action_port_failure_without_stdout_carries_no_refusal`
- `a_failed_action_outcome_threads_its_refusal_into_the_failure_event`
- `a_refused_valve_persists_its_refusal_into_the_failure_event`
- `the_model_exposes_the_projected_failure_and_the_drilled_hint_derives`
- `action_failure_display_line_covers_every_refusal_shape`
- `action_failures_project_the_latest_failure_and_clear_on_recovery`
- `the_record_modal_renders_the_latest_refusal_for_its_item`
- `the_new_variants_carry_their_derived_debug_forms` — derive coverage for variants added
  across A and C; may need splitting

## What this predicts about the phantom

If slice A comes up coverage-red, **read the misses before assuming the phantom
followed it.** A's expected failure mode is ordinary uncovered lines from tests still
sitting in B. The phantom is distinguishable by its signature: a summary count that no
listing surface can name. Ordinary misses are named by
`cargo llvm-cov report --show-missing-lines` in one shot.

Recording the distinction here because conflating the two would corrupt `-3yx`'s
experiment, which is the whole reason the split is worth running.
