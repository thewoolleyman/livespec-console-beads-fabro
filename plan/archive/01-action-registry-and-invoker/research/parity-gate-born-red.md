# The cross-repo parity gate, born red — banked before it went green

Per the plan charter: "This check should be BORN RED against the known-missing
valve actions, and that IS its red demonstration — bank the red output before
making it green."

Run 2026-08-02, worktree `feat/action-invoker` at `64131cd` plus only the
fixture (`tests/fixtures/drive-human-action-surface.json`) and the test
(`crates/console-cli/tests/drive_surface_parity.rs`) — no registry change yet:

```text
---- every_captured_human_action_is_registered_or_deliberately_omitted stdout ----

thread 'every_captured_human_action_is_registered_or_deliberately_omitted' (2110063) panicked at crates/console-cli/tests/drive_surface_parity.rs:66:5:
captured human actions bind to NO registered action (the registry does not account for the orchestrator's published surface):
set-workflow-scope-override -> set-workflow-scope-override
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    every_captured_human_action_is_registered_or_deliberately_omitted

test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p livespec-console-beads-fabro --test drive_surface_parity`
```

The red is the forward arm refusing the shipped-but-unpublished
`set-workflow-scope-override` (the documented remedy a factory-safety refusal
names, `-w7d`), which the console bound only AFTER this run. The reverse arm
(registry -> capture) and the omission-reason arm passed on the same run — a
gate demonstrated red in one direction is evidence about that direction only,
so the reverse arm's own red demonstration is the mutation demo below.

## The reverse arm, mutation-demonstrated red (2026-08-02)

Owed by the paragraph above and discharged here rather than in a PR body, so
the evidence lives with the gate instead of in a forge comment. Run on the
rebased branch (`66b4fd2`, parent `e3d82eb`, onto master `d765aae`).

Mutation: the sole `console_local_actions` entry in
`tests/fixtures/drive-human-action-surface.json` retitled
`driver-handoff` -> `driver-handoff-TYPO`, simulating the capture silently
losing an entry the registry still ships.

```text
---- every_registry_action_is_accounted_for_in_the_capture stdout ----

thread 'every_registry_action_is_accounted_for_in_the_capture' (1915097) panicked at crates/console-cli/tests/drive_surface_parity.rs:139:5:
registry actions the capture never reviewed: ["driver-handoff"]

failures:
    every_registry_action_is_accounted_for_in_the_capture

test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`RC=101`, read UNPIPED. **Only the reverse arm failed** — the forward and
omission-reason arms stayed green on the same run, which is the point: the
three arms are independently sensitive, so a green build is not one arm
carrying the other two. The fixture was restored with `git checkout --` and
verified byte-identical by `sha256sum -c` plus an empty `git status --short`.

Both arms are now demonstrated red in their own direction. The omission-reason
arm is NOT yet mutation-demonstrated; it is the remaining one, and no claim
here covers it.
