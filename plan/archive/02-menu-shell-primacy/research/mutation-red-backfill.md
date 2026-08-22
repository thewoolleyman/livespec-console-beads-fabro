# Mutation-red backfill for PR #730 and PR #734

**Ledger anchor:** `livespec-console-beads-fabro-oy37gg`

Plan 02's second completeness review (evidence-id
`et3-completeness-review-2-2026-08-21-master-d18c2a7`, comment 37 on `-et3`)
judged the mutation-red evidence trail the weakest part of the plan and named two
merged PRs carrying no mutation line at all: **#730** and **#734**. `-et3.7`'s
acceptance had explicitly demanded the demonstration for #734.

This note banks the missing evidence as **commands and exit codes**, not
attestations. Every exit code below was read **unpiped** (`cmd > file 2>&1; $?`),
because a pipeline reports the exit status of its last stage — the first baseline
run in this exercise reported `0` from `tail`, not from `cargo test`, and that
would have been a false record.

Run on `master` at `defcadc`, in a clean dedicated worktree.

## PR #730 — `-et3.5`, merge `cd71cfc`

**What it strengthened.** It disambiguated three cap-setter labels that all read
`"Set override"`, and added a registry-wide uniqueness gate in
`registry_entries_are_complete_unique_and_off_the_system_keys`:

```rust
let label_is_unique = labels.insert(spec.label, spec.id).is_none();
assert!(label_is_unique, "{}", spec.label);
```

**The mutation.** Revert exactly one of the three disambiguated labels to a
duplicate — `set-review-fix-cap`'s label back to `"Set merge-on-review cap"`.
This is the precise regression the gate was added to catch.

```
mutation: crates/console-application/src/action_registry.rs
          id "set-review-fix-cap": label "Set review-fix cap" -> "Set merge-on-review cap"
command:  cargo test --quiet --package console-application --lib action_registry
```

| run | exit | result |
|---|---:|---|
| baseline | `0` | 16 passed; 0 failed |
| **mutated** | **`101`** | **14 passed; 2 failed** |
| restored | `0` | 16 passed; 0 failed |

Two independent gates caught it:

- `registry_entries_are_complete_unique_and_off_the_system_keys`, panicking at
  `action_registry.rs:1075` with the duplicated label as the message:
  `Set merge-on-review cap`
- `operator_reference_mentions_every_registered_action_once`, panicking at
  `action_registry.rs:1277` with `set-merge-on-review-cap` — the generated-docs
  lockstep gate, which the PR body claimed stays in lockstep and demonstrably does.

## PR #734 — `-et3.7`, merge `1cbddf1`

**What it strengthened.** It replaced hand-maintained detail actions with a
derivation from `ACTION_REGISTRY` (`attention_detail_actions`, filtering on
staging and availability) and added
`command_modal_opens_for_registry_derived_attention_actions`.

**The mutation.** Narrow the staging filter to `DriverHandoff` only, dropping the
`Valve(_)` arm, so the derived set no longer contains the valve actions.

```
mutation: crates/console-application/src/lib.rs, attention_detail_actions
          matches!(spec.staging, Valve(_) | DriverHandoff)  ->  matches!(spec.staging, DriverHandoff)
command:  cargo test --quiet --package console-application --lib
```

| run | exit | result |
|---|---:|---|
| baseline | `0` | 382 passed; 0 failed |
| **mutated** | **`101`** | **378 passed; 4 failed** |
| restored | `0` | 382 passed; 0 failed |

Four gates caught it, including the one `-et3.7`'s acceptance demanded:

- `command_modal_opens_for_registry_derived_attention_actions` — `lib.rs:16075`
- `tui_command_modal_clamps_to_available_actions` — `lib.rs:9588`
- `tui_interaction_moves_attention_selection_with_arrows` — `lib.rs:10640`
- `tui_model_shows_lane_derived_attention_detail` — `lib.rs:10640`

with the failing assertion showing the derived set collapsing to empty:

```
assertion `left == right` failed
  left: Some([])
 right: Some([Registered("approve"), Registered("reject"), Registered("set-admission"),
              Registered("set-merge-on-review-cap"), Registered("set-review-fix-cap"),
              Registered("set-acceptance"), Registered("set-acceptance-rework-cap")])
```

## Two discarded mutations, and why they are recorded

`-oy37gg` says a mutation that does not go red is a finding rather than a
failure. Two candidates for #734 did not go red, and **neither is a gate defect**
— recording them matters because both would have been easy to misreport as one.

**1. Dropping `&& (spec.availability)(&ctx)` — did not compile.** Removing the
only use of `ctx` makes it an unused variable, and `-D warnings` turns that into
`error: could not compile`. Exit was `101`, identical to a test failure. A
mutation that fails to compile demonstrates nothing about the test suite: it
proves the compiler works. **An exit code alone cannot tell the two apart** — the
output has to be read.

**2. Dropping the `| DriverHandoff` arm — compiled, ran, and all 382 tests
passed.** This looks exactly like a gate gap and is not one. It is an
**equivalent mutant**: `ACTION_REGISTRY` contains exactly one `DriverHandoff`
entry (`driver-handoff`, `action_registry.rs:250`) whose availability is

```rust
|ctx| ctx.has_driver_handoff && matches!(ctx.surface, ActionSurface::LaneDrill)
```

while `attention_detail_actions` always builds its context with
`ActionSurface::Attention`. The arm is therefore **unreachable from this call
site**, and removing it cannot change behaviour. No test can catch a change that
changes nothing, so the suite is not at fault.

That unreachable arm is a small real finding in its own right — filed separately
as `-6zoq`.

> **Correction, 2026-08-22.** This section originally went on to claim the arm
> "predicts a justified survivor" for the `-txtzn5.10` mutation gate. **That was
> wrong, and measuring it is what showed it.** cargo-mutants does not generate a
> match-arm deletion here at all — the complete mutant set for
> `attention_detail_actions` is three (two return-value replacements and one
> `&&`→`||`), of which one is caught and two are unviable. Zero survive, so
> there is nothing to allow-list.
>
> The error was assuming the tool would perform the same equivalent mutation I
> had performed by hand. It does not: cargo-mutants mutates return values,
> binary operators and struct fields — not individual alternatives inside a
> `matches!` pattern. **Proving a change is behaviour-preserving says nothing
> about whether the tool will ever make it.** The dead-code finding stands; the
> mutation-gate justification for prioritising it does not.

**The general lesson.** Before reporting a surviving mutant as a test-adequacy
gap, prove the mutation is behaviour-changing at the call site under test. The
control here was cheap — read the one `DriverHandoff` entry's availability
predicate — and it turned a false "gate defect" into an accurate "dead code plus
a future allow-list entry".
