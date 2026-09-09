# The shared-site pattern, and the one constraint that breaks it

Measured 2026-08-21, after note 002. Note 002 established that the
in-crate `#[cfg(test)]` regions are a refactoring backlog rather than a
natural limit, and proved it on the `assert!` class. It then asserted —
on reasoning, not measurement — that "the same reduction applies to the
543 test-helper `?` arms".

**That assertion was half wrong, and the half that is wrong is the
dominant class.** This note records the measurement, because four
slices (`-txtzn5.5` … `-txtzn5.8`, 713 sites) were cut on the strength
of it.

## The rule

A shared failure site collapses N uncovered branches into one **only if
the helper is monomorphic.** A generic helper is instantiated once per
concrete type argument, and *each monomorphisation carries its own copy
of the match arms* — so N call-site types produce N uncovered `Err`
arms instead of one, plus the unused `Ok` arm in the instantiation that
only ever fails.

## The measurement

`console-eventstore`, converting one test function
(`opened_store_uses_wal_mode_and_creates_required_tables`) from
`-> Result<(), EventStoreError>` + `?` to a `()` test calling a shared
unwrap helper. Two `?` arms are targeted. Measured with
`cargo llvm-cov --package console-eventstore --all-features --lib`:

| variant | uncovered regions | vs baseline |
|---------|-------------------|-------------|
| baseline | 166 | — |
| generic `fn ok<T, E: Debug>(Result<T, E>) -> T` | **167** | **+1 — worse** |
| monomorphic `ok_store` + `ok_string`, one `#[should_panic]` test each | **164** | **−2 — exactly the two `?` arms** |

All 30 `console-eventstore` tests pass in the monomorphic variant.

The generic variant's regression is visible at instantiation level: the
three monomorphisations of `ok` each carry 3 uncovered regions (+9),
which more than pays back the −2. Note the trap — a **segment-level**
view of the generic variant reads −2 and looks like a clean win, and
only the file/`totals` view (the one `--fail-under-regions` actually
gates on) shows +1. Measure the gate's number, not the source-site
count.

This also explains, retroactively, why the note 002 experiment
succeeded: `fn check(condition: bool, context: &str)` is monomorphic by
construction. It was never evidence for the generic case.

## What this means for the a2-* slices

The pattern holds, with a constraint:

- **Assertions** (167 sites): one non-generic `check(condition, context)`
  per crate, covered by one `#[should_panic]` test. Proven in note 002.
- **Test-helper `?`** (543 sites): tests return `()` and call fallible
  operations through **concrete, per-result-type** helpers
  (`ok_store`, `ok_string`, …), each covered by its own
  `#[should_panic]` test. The helper count is the number of distinct
  `Result<T, E>` types in the test module, which is far smaller than the
  site count — but it is NOT one.

Do not write a generic `ok<T, E>`. It reads as the obvious
consolidation, it compiles, the tests pass, the source-site count drops,
and the gate metric gets worse.

## Still nothing that resists

This is a constraint on the refactor's shape, not a region that cannot
be reached. Every site in the measurement above is reachable; the
monomorphic form reaches them. The maintainer ruling of 2026-08-21
stands unqualified: the target holds as ratified, no measured-scope
narrowing, no exclusions.
