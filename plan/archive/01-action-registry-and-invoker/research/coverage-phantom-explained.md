# The one-line coverage phantom — explained, measured, and attributed

Closes the forensic thread opened in `coverage-phantom-bounded-pass.md` and
tracked as `livespec-console-beads-fabro-3yx`. Measured 2026-08-19 against
master `8b03c12`, in this repo, with the toolchain the gate actually uses
(rustup 1.92.0's bundled `llvm-cov`).

## The answer

llvm-cov's **file line summary is not a per-line set operation.** It is a sum
over *instantiation groups* — functions sharing a source start location — of
each group's own `(NumLines, Covered)` pair, and a group merges its
instantiations with `CoverageInfo::merge`, which takes the **scalar maximum of
each field independently** rather than the union of covered lines.

Every listing surface (`--show-missing-lines`, the `--text` annotated listing,
the lcov `DA` records, the JSON segment array) renders the *merged per-line*
view instead. When the two disagree, the summary wins the count and the listing
wins the naming, and the difference is reported as a missed line that no
listing can name — because it corresponds to no source line at all.

Two distinct ways the sum diverges from the union, both observed here:

1. **Double counting across groups.** A line belonging to two groups — a
   closure and its enclosing function — is counted twice by the sum and once by
   any listing.
2. **Scalar-max merge within a group.** A group whose instantiations cover
   *different subsets* of its lines reports `covered = max(covered_i)`, not
   `|union of covered lines|`.

## The instance in this workspace

    crates/console-application/src/lib.rs:1869  const fn overlay_footer_hint(...)

    instantiation                                     lines  covered
    _RNvCsacvGuOcoAvQ_..._19overlay_footer_hint          12       11
    _RNvCsddCUhtQxyzO_..._19overlay_footer_hint          12       10
    ----------------------------------------------------------------
    group, per llvm's scalar-max merge                   12       11   -> 1 MISSED
    union of covered lines                               12       12   -> 0 missed

The two instantiations are the crate's own test-harness compilation and its
plain-lib compilation linked into other crates' test binaries. They cover
different lines of the same function; the union is complete.

## The prediction on -3yx is REFUTED

`-3yx` predicted the phantom **tracks the NUMBER OF COMPILATIONS** of
`console-application` in the merged profile. Measured directly, by exporting
from explicit object sets against the one profdata:

    objects                      summary lines  summary missed  merged-view missed  PHANTOM
    console_application only              8801              51                  50        1
    + 1 dependent test binary             8801               1                   0        1
    + 2                                   8801               1                   0        1
    + 3                                   8801               1                   0        1
    + 4 (what `just check-coverage` sees) 8801               1                   0        1
    dependent binaries only (1)           3441             716                 695       21
    dependent binaries only (2)           3441             716                 695       21
    all 4 dependents, no own binary       3448             716                 695       21

The phantom is **present with a single object** and **does not move** as objects
are added, so it does not track compilation count. It tracks per-group scalar
accounting, which is why the single-object case still shows 1: there the excess
comes from mechanism (1) above — line 2786, a closure inside
`LaneColumn::finished_unreconciled_count`, uncovered in both the closure's group
and its parent's, counted twice.

This also explains the four mutually inconsistent line totals recorded on -3yx.
Re-measured today the file reports `LF=8801` with only `8689` `DA` records and
**no `DA` record with count 0**: `LF` is the group sum, `DA` is the per-line
view, and the 112-line gap is lines counted by more than one group.

## Reproduced exactly, not inferred

`dev-tooling/coverage-gate.py`'s `_attribute` recomputes llvm's arithmetic from
the JSON export. Against the whole workspace it reproduces llvm's own totals to
the line — `25216/25215`, one missed — and names the single responsible group.
The gate now **refuses to pass an unnameable miss it cannot attribute**, so an
unexplained excess is a new signature that fails rather than passing under the
old bare cap.

## What is NOT claimed, and what cannot be fixed here

The residue **cannot be covered by a test.** It is not a line. Covering the
whole function in *both* compilations would remove this instance, but that means
making another crate's tests exercise `overlay_footer_hint` — a test written to
please a tool's arithmetic, in a crate this plan is scope-guarded out of. Not
done, and not recommended.

The standing maintainer ruling that the coverage gate is correct and is not to
be re-litigated is untouched: this changes attribution only, and in the
stricter direction.
