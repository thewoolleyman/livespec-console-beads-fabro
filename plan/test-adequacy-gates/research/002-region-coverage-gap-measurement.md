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

Two facts follow, and they point in different directions:

1. **166 production regions (19%)** are almost entirely the implicit
   `Err(e) => return Err(From::from(e))` arm of a `?`. Examples:
   `console-eventstore/src/lib.rs:485` (`Connection::open(path)?`),
   `console-application/src/source_adapters.rs:176`
   (`checkpoints.load_checkpoint(&adapter_id)?`),
   `console-cli/src/lib.rs:234` (`store.command_count()?`). These are
   real, closeable gaps: every one is a failure path no test exercises.
2. **713 regions (81%) are inside the crates' own `#[cfg(test)]`
   modules** — the never-taken branches of test-helper `?` and of
   `assert!` / `.expect()` in *passing* tests. `--lib` instruments the
   unit-test modules compiled into each library target, so they are
   counted.

Fact 2 is the load-bearing one: **a green test suite cannot, even in
principle, cover the failure branch of its own passing assertion.**
`assert!(cond)` whose `cond` always holds leaves its panic region
uncovered by construction; making it covered means making the test fail.
The `justfile:256-269` coverage-pincer comment already records the
line-level form of exactly this trap ("a passing test suite cannot
exercise a failing assert message"). At region granularity it is not an
edge case to work around — it is 167 regions, plus 543 test-helper `?`
arms whose error paths exist only to satisfy the compiler.

## Why this collides with the ratified spec

`SPECIFICATION/non-functional-requirements.md:122-127` is unambiguous:

> **No coverage exclusions are permitted** -- regions the language makes
> uncoverable (macro-generated code, exhaustiveness arms for states the
> type system already makes impossible) MUST be eliminated by
> restructuring, never annotated away

Under that clause the only sanctioned response to an uncoverable region
is restructuring. But 713 of the 893 are in test code, where
"restructuring" would mean deleting assertions or rewriting every test
helper to be infallible — the opposite of test adequacy, which is this
thread's whole charter. And no exclusion (`--ignore-filename-regex`, a
disposition file, an annotation) is permitted to carve them out.

So `--fail-under-regions 100` as currently specified is **not reachable**
by work this thread can legitimately do. The gate cannot simply be
flipped once the production gaps close; it would still be 713 short.

## Consequence for the regroom

Slice (a) as written is one slice covering three separable pieces of
work at three different tiers:

- **(a1) Close the 166 production region gaps.** Genuinely factory-safe
  implementation work, and the piece that actually raises test adequacy.
  Tractable but not small: seams like `&mut dyn CommandAppendStore`
  (`console-cli/src/lib.rs:224`) accept a failing double directly, while
  the many `&mut SqliteEventStore` call sites (e.g. `:415`, `:482`,
  `:590`) need either a real failing SQLite (closed / read-only /
  corrupt handle) or a new trait seam — a refactor the spec's own
  "redesign signal" language endorses.
- **(a2) Decide the measurement scope for the region target.** Whether
  the ratified region goal counts in-crate test modules. This is a
  **spec-tier** question, not an implementation choice, and it gates
  whether (a3) can ever pass.
- **(a3) Flip the gate + land the spec-reconciliation rider.** Blocked on
  (a2) and (a1).

Note that (a2) is not a request to weaken the gate. It is the question
of what the gate MEASURES. The line gate already answers this question
implicitly for its own metric, and the answer is recorded in
`tests/fixtures/coverage-unnameable-disposition.json` (ledger item
`livespec-console-beads-fabro-3yx`) — a reasoned, capped disposition for
misses no listing surface can name. The region target has no equivalent
answer, and the no-exclusions clause forbids inventing one without a
spec revision.

## Sequencing consequence

The low-water-mark constraint (note 001, Sequencing #2) still applies,
but it is no longer the binding constraint on when (a) can start. As of
2026-08-21 the repo is AT a low-water mark — 2 open PRs (#404, the
release-please PR, and #317) — and the flip still cannot land, because
(a1) and (a2) are unfinished. The window is not the blocker; the 893
regions are.
