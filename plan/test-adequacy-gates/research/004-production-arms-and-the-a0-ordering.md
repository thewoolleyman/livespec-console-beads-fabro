# Production `?` arms are reachable — and a1 cannot precede a0

Measured 2026-08-21, after note 003. Two claims underpinned the a1
slices (`-txtzn5.2`, `.3`, `.4`, the 166 production gap sites), and
neither had been measured:

1. that a production `?` `Err` arm sitting behind a **concrete**
   `SqliteEventStore` — 122 of the 166 sites — can be reached at all
   without inventing a new trait seam; and
2. implicitly, that closing them is independent of the a2-* work, which
   is why they were filed as separate slices.

Claim 1 holds. **Claim 2 does not**, and it changes the dependency
graph.

## Claim 1: the arms are reachable, two techniques, no new seam

Measured on `console-eventstore` with
`cargo llvm-cov --package console-eventstore --all-features --lib`:

- **Make the real call fail.** `SqliteEventStore::open(&path)` against a
  path that is a *directory* returns `Err`, covering
  `Connection::open(path)?` (`console-eventstore/src/lib.rs:509`).
- **Sabotage the schema in place.** Open in memory, `drop table events`
  through the store's own connection, then `append_event(...)`. The
  transaction's `?` arm is entered and covered.

Neither needs a trait seam, a mock, or a redesign. The store's
`connection` is reachable from the crate's own test module, so the
"failing SQLite (closed / read-only / corrupt handle)" route recorded on
`-txtzn5.2` is real and cheaper than the trait-seam alternative also
recorded there.

## Claim 2: closing a production arm ADDS an assertion region

The measurement, step by step:

| step | uncovered regions |
|------|-------------------|
| baseline | 166 |
| + two tests covering the two production `?` arms, asserting with `assert!` | **166 — unchanged** |
| same two tests, asserting through a monomorphic `check` helper | **164 — the two arms, cleanly** |

The middle row is the finding. Each new test covered its production
arm (−1) and introduced its own `assert!` failure branch (+1). Net zero.
**a1 work done without a0's pattern in place moves the gate metric not
at all** — it trades a production uncovered region for a test-module
uncovered region, one for one, while looking in every other respect like
progress. 31/31 `console-eventstore` tests pass in the final row.

## Consequence for the ledger

`-txtzn5.2`, `.3` and `.4` must depend on `-txtzn5.1`, exactly as
`.5`–`.8` already do. They were filed without that edge on the
assumption that production work was independent of the pattern work. It
is not: every test written to close a production arm is itself a
test-module assertion site.

The slices stay separate — the work is genuinely different, and a0 is
small — but the ordering is now forced. a0 first, then a1 and a2 per
crate in either order.

## Still nothing that resists

Both production techniques reach their target on the first attempt, and
the residual in every variant above is the known assertion class that
note 002's pattern closes. The 2026-08-21 ruling stands unqualified.
