# Region-coverage gap — measured 2026-08-21

The thread's slice `-txtzn5`(a) was framed as a one-line `justfile` flip
plus a spec-reconciliation rider. **It is not.** This note records the
measurement that falsifies that framing, so the regroom cuts the real
work.

## Method

```
cargo llvm-cov --workspace --all-features --lib --json
```

Exactly the instrumentation `just check-coverage` (`justfile:272`)
already runs — the same scope the ratified clause binds (`--lib`, every
workspace library, no per-crate carve-outs). Only the reporting knob
differs: the gate reads `lines`, this reads `regions`.

## Result

| metric         | covered | total  | pct     |
|----------------|---------|--------|---------|
| lines          | 26,988  | 26,988 | 100.00% |
| functions      | 2,487   | 2,487  | 100.00% |
| **regions**    | 40,858  | 41,751 | **97.86%** |
| instantiations | 3,168   | 3,477  | 91.11%  |

**893 regions are uncovered.** Adding `--fail-under-regions 100` today
fails the gate by 893, not by zero.

Uncovered regions by file:

| uncovered | file |
|-----------|------|
| 494 | `crates/console-cli/src/lib.rs` |
| 152 | `crates/console-eventstore/src/lib.rs` |
|  86 | `crates/console-tui/src/lib.rs` |
|  86 | `crates/console-application/src/source_adapters.rs` |
|  51 | `crates/console-application/src/lib.rs` |
|   8 | `crates/console-fork-drift-check/src/lib.rs` |
|   7 | `crates/console-application/src/action_registry.rs` |
|   6 | `crates/console-spec-check/src/lib.rs` |
|   3 | `crates/console-completeness-check/src/lib.rs` |

## What the uncovered regions ARE

Classified by the source line each uncovered region entry sits on, split
by whether the line falls inside the crate's own trailing `#[cfg(test)]`
module:

| count | scope | kind |
|-------|-------|------|
| 543 | in-crate test module | `?` error propagation |
| 167 | in-crate test module | `assert!` / `.expect()` failure branch |
| 152 | production | `?` error propagation |
|  12 | production | other |
|   4 | either | `match` / `matches!` arm |

Two classes follow:

1. **166 production regions (19%)** are almost entirely the implicit
   `Err(e) => return Err(From::from(e))` arm of a `?`. Examples:
   `console-eventstore/src/lib.rs:485` (`Connection::open(path)?`),
   `console-application/src/source_adapters.rs:176`
   (`checkpoints.load_checkpoint(&adapter_id)?`),
   `console-cli/src/lib.rs:234` (`store.command_count()?`). Every one is
   a failure path no test exercises.
2. **713 regions (81%) are inside the crates' own `#[cfg(test)]`
   modules** — the never-taken branches of test-helper `?` and of
   `assert!` / `.expect()` in *passing* tests. `--lib` instruments the
   unit-test modules compiled into each library target, so they count.

## The ruling: class 2 is a refactoring backlog, not a limit

This note first concluded that class 2 was a natural limit — that a
green suite cannot cover its own passing assertion's failure branch, so
`--fail-under-regions 100` was unreachable and the spec's
no-exclusions clause at `SPECIFICATION/non-functional-requirements.md:122-127`
had to give. **Maintainer ruling, 2026-08-21: it does not.** If it is
code we own, there is no reason it cannot be made reachable by
refactoring; code that is genuinely unreachable is un-executable and
therefore deletable, which is the whole point of the clause. The target
stands as ratified. Do not narrow the measured scope and do not withdraw
the target. Where a region truly resists, delete the code rather than
excluding it from measurement, and report the specific construct.

**The ruling is empirically correct, and the earlier conclusion was
wrong.** Measured on `console-fork-drift-check`, whose 8 uncovered
regions were 3 production and 5 test-module `assert!` sites:

| variant | uncovered regions |
|---------|-------------------|
| as-is (5 `assert!` sites) | 8 |
| two `assert!` sites compressed into one `assert_eq!` tuple compare | 7 |
| all 5 routed through one shared `check` helper + one `#[should_panic]` test | **3** |

Every test-module region went to zero; the 3 that remain are the
production gaps. All 32 tests still pass. The mechanism is that region
coverage is **per source site**: a failure branch is covered if any run
takes it. So the pattern is

```rust
#[track_caller]
fn check(condition: bool, context: &str) {
    if !condition {
        panic!("check failed: {context}");
    }
}

#[test]
#[should_panic(expected = "check failed")]
fn check_reports_failure() {
    check(false, "deliberate");
}
```

Funnelling N assertion sites through one helper leaves exactly one
failure branch in the crate, and a `#[should_panic]` test *takes* it.
The same reduction applies to the 543 test-helper `?` arms: consolidate
the fallible test-support operations behind shared helpers, then give
each consolidated site one deliberate error-case test.

**Nothing resists so far.** The one construct that could have been
structurally unfailable — a `write!(...)?` whose sink cannot error —
occurs exactly once (`console-tui/src/lib.rs:373`) and writes to a real
`io::Write`, so a failing-writer double covers it. If a genuinely
un-executable region does turn up, the ruling's instruction is to delete
the code, not to exclude it, and to report the construct here.

## Consequence for the regroom

Slice (a) as written is one slice covering separable work:

- **(a1) Close the 166 production region gaps.** Factory-safe
  implementation work, and the piece that most directly raises test
  adequacy. Tractable but not small: seams like
  `&mut dyn CommandAppendStore` (`console-cli/src/lib.rs:224`) accept a
  failing double directly, while the many `&mut SqliteEventStore` call
  sites (e.g. `:415`, `:482`, `:590`) need either a real failing SQLite
  (closed / read-only / corrupt handle) or a new trait seam — a refactor
  the spec's own "redesign signal" language endorses.
- **(a2) Work the 713 in-crate test regions down by refactoring**, using
  the shared-check-helper pattern proved above. Mechanical and highly
  repetitive, but per-crate independent, so it slices cleanly:
  `console-cli` (426), `console-eventstore` (96), `console-tui` (84),
  `console-application/source_adapters.rs` (66),
  `console-application/lib.rs` (29), `action_registry.rs` (7),
  `console-fork-drift-check` (5). Report any construct that resists.
- **(a3) Flip the gate + land the spec-reconciliation rider.** Blocked on
  (a1) and (a2). The rider flips only the ":112-119" sentence's
  "NOT yet a present gate" tail; the no-exclusions clause is untouched
  and stays as ratified.

The line gate's own analogue — `tests/fixtures/coverage-unnameable-disposition.json`
(ledger item `livespec-console-beads-fabro-3yx`), a capped disposition
for misses no listing surface can name — is deliberately NOT the model
here. That disposition exists because llvm-cov counts line misses it
cannot name; these region misses are all nameable, and the ruling is to
fix them rather than disposition them.

## Sequencing consequence

The low-water-mark constraint (note 001, Sequencing #2) still applies to
(a3), but it is not the binding constraint on when (a) can start. As of
2026-08-21 the repo IS at a low-water mark — 2 open PRs (#404, the
release-please PR, and #317) — and the flip still cannot land, because
(a1) and (a2) are unfinished. The window is not the blocker; the 893
regions are.

Slices (b) CI merge-gate fuzz and (c) CI mutation are independent of the
region gate and of each other, and can proceed in parallel with (a1).
