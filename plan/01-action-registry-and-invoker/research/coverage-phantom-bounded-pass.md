# The one-line coverage phantom — the bounded pass, and what it settled

**Ledger:** `livespec-console-beads-fabro-3yx` (P2 bug, `tracks` under epic `-dvv`),
filed 2026-08-02 on the maintainer's authorization. That item carries the falsifiable
prediction and is where the A/B/C split's verdict gets recorded. This file is its
forensic backing.

**Outcome: the pass FAILED to close the blocker.** It did not find the line and
did not make `just check-coverage` green. Recorded anyway, because it converts
"nobody can find it" into a much narrower and more falsifiable claim, and
because it closes off three avenues so a successor does not re-walk them.

Authorized 2026-08-02 by the maintainer as ONE keyed pass with a
stop-regardless discipline: key on regions newly-missed versus master rather
than on blind execution fixes, then report either way. Not re-entered.

## What was measured

Branch `feat/action-invoker`, tip `35a8c2b` (since rebased to `66b4fd2` onto
master `d765aae`; the coverage numbers below were taken pre-rebase, and the
rebase carried only a dev-tooling pin and plan docs, no Rust).

`just check-coverage` reproduces exactly as the resume block recorded it:

    console-application/src/lib.rs   7929 lines   1 missed   99.99%
    TOTAL                           22491 lines   1 missed  100.00%   RC=1

## Five listing surfaces, none of which can name the line

1. **`cargo llvm-cov report --show-missing-lines`** — the tool
   `plan/console-happy-path-mvp/handoff.md` § 0c item 2 names as *the* tool for
   ordinary misses. It prints **no missing-line section at all** while the same
   invocation's summary still counts 1.
2. **JSON segment reconstruction** (this pass). 23,012 segments, **0
   expansions**, **0 zero-count gap segments**. Reconstructing llvm's
   `LineCoverageStats` rule over the segment list yields **zero** uncovered
   lines. This independently reproduces the predecessor's four reconstructions.
3. **Zero-count segments intersected with the diff** (this pass, the keyed
   step). There are 42 zero-count segments in `lib.rs`. **Not one of them lies
   on a line the milestone-2 diff added** — the 591 added lines contain no
   zero-count segment at all. This is the pass's real result: *no missed
   region, segment, or line in `lib.rs` is attributable to the new code.*
4. **`cargo llvm-cov report --text` annotated listing** (this pass). 7,836
   instrumented lines rendered, **0 of them with an execution count of 0**.
5. **lcov export** (predecessor, not re-run here): `LF=7917/7928` against 7,823
   `DA` records, none zero-count.

Note the four mutually inconsistent answers to "how many lines does this file
have": 7,929 (JSON summary), 7,917 (lcov `LF`), 7,836 (annotated listing),
7,823 (lcov `DA`). The disagreement is in llvm's own accounting, not in any
one exporter.

## A NEW lead, deliberately NOT pursued

Recorded so it is not lost, and flagged so nobody mistakes it for a finding.
Pursuing it is re-entry, which the stop-regardless discipline forbids.

The JSON carries **1,151 function records** for `lib.rs`, of which **91 have
`count == 0`** — while the file summary simultaneously reports 723 functions
with **0 missed**. The mangled names carry **two distinct crate
disambiguators** (`CsacvGuOcoAvQ_` and `CsddCUhtQxyzO_`), i.e. two separate
compilations of `console_application` are present in the merged profile — the
expected shape when the crate is built once for its own unit-test binary and
again as a dependency of another crate's tests.

**Hypothesis, explicitly unverified:** per-compilation line accounting merged
inconsistently into the file summary would produce exactly this signature — a
line covered in one compilation and not the other, counted as missed by an
aggregate that the merged segment view (which every listing surface renders)
shows as covered. That would explain why no listing surface can name it. It
would also mean the line is NOT necessarily in the new code at all, which
finding 3 above is consistent with.

If anyone tests this, the falsifiable prediction is: the phantom tracks the
number of distinct compilations of `console-application` in the profile, not
the content of the diff.

## Why the commit-boundary bisect was a dead end

Measuring `ab49fc4` (the parity/`-w7d` commit) alone reports **43** missed
lines, not zero. That is not the phantom — it is that the commit's tests land
in the tip commit, so its code is genuinely unexercised at that boundary.
**Intermediate commits are not required to be coverage-green**, so no bisect
along the existing commit boundary can localize anything. Do not repeat it.

## Why a feature-keyed split IS different, and is the recommendation

The mechanism that makes it work is the one the bisect lacked: **each split PR
carries its own tests and must independently pass the coverage gate.** That is
a constraint the existing commit boundary never imposed.

The natural three-way cut, each self-contained:

| PR | content | `lib.rs` churn |
|---|---|---|
| A | cross-repo parity gate + `set-workflow-scope-override` binding (`-w7d`) | +78 |
| B | the action invoker (`TuiOverlay::ActionInvoker`, palette roster, confirm staging) + docs | ~34 keyword-bearing added lines |
| C | the `-ectqye` action half (refusal capture, `OrchestratorActionOutcome::Failed`, `error_json`, latest-failure projection, record-modal render) | ~69 keyword-bearing added lines |

Both B and C touch `lib.rs` substantially, which is where the phantom lives, so
the split places it in exactly one of three independently-gated PRs — and lands
the other two regardless. If the hypothesis above is right, the phantom may
instead vanish or persist uniformly across all three, which is itself the
cheapest available test of it.

### Separability, measured rather than assumed

The charter states requirement 4 as one clause — a generic invoker that renders
"success or the full refusal payload" — which reads as though B and C are one
indivisible piece. **They are not.** Measured on the branch:

- The refusal is rendered by the **record modal**
  (`crates/console-tui/src/lib.rs:1454`, `Last action: …`), not by the invoker
  overlay.
- `TuiOverlay::ActionInvoker`'s code references `refusal` **nowhere**. The two
  features surface at different places and share no rendering path.

So B and C impose no ordering constraint on each other. Whichever lands second
rebases over the other's `OrchestratorActionOutcome` shape.

### Recommended order: A -> B -> C

**A first.** Smallest (`lib.rs` +78), carries the born-red parity gate, and
delivers the `-w7d` binding.

**B second, and this is the leg-critical one.** `set-workflow-scope-override`
ships with `hotkey: None` — it is the first deliberately menu/invoker-only
action — so **A alone does not make it reachable.** The dogfood leg
(`set-workflow-scope-override:…-ccycuk:citation-only` from the cockpit)
requires **A and B both**, and does NOT require C. A + B is therefore the
minimum that unblocks the milestone's acceptance leg.

**C last.** Independently valuable, on no critical path.

A soft coupling to expect, not a blocker: B's invoker-roster tests may assert
that `set-workflow-scope-override` appears in the roster, so landing B before A
means adjusting those assertions. Landing A first avoids it.
