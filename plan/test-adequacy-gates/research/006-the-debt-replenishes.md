# The region debt replenishes as fast as it is paid down

Measured 2026-08-21, immediately after `-txtzn5.1` (a0) landed. This is
the first end-to-end measurement of the plan actually working, and it
found the plan's largest problem.

## a0 did exactly what was predicted

`-txtzn5.1` merged as PR #770 (`669c1048`). The factory implemented the
pattern correctly — a non-generic
`#[track_caller] fn check(condition: bool, context: &str)` plus one
`#[should_panic]` test, no generic helper — and
`console-fork-drift-check` went from **8 uncovered regions to 3**,
matching the probe in note 003 exactly. The three that remain are its
production `?` arms, which belong to `-txtzn5.4`.

The mechanism works. The slice worked. Then the workspace number went up.

## The workspace number

| | total regions | uncovered | uncovered rate |
|---|---|---|---|
| session start (`593e428`) | 41,751 | 893 | 2.139% |
| after a0 (`669c1048`) | 43,861 | 940 | 2.143% |

**893 → 940**, up 47, on a day when this thread removed 5. Thirteen
commits landed in `crates/` during the same window — roughly 3,700 lines
across `console-application`, `console-cli`, `console-tui`,
`console-eventstore` and others, none of them from this thread.

The percentage is the part worth reading. It did not move: 2.139% before,
2.143% after. New code landed at **97.54% region coverage** — within
noise of the 97.86% the codebase already sat at. The base grew 5.1% and
the debt grew with it, proportionally.

## Why this matters more than the absolute number

The a1 and a2 slices between them close 879 sites. That is sized against
a target that moves. On this one day the repo added 52 uncovered regions
while a0 removed 5; at anything like that ratio the slices finish and the
gate still fails.

This is not an argument that the work is pointless — the 940 are real
unexercised failure paths and closing them is the point of the thread.
It is an argument about **order**. `-txtzn5.11` is currently sequenced
last, after every a1 and a2 slice, on the reasoning that the gate cannot
flip until the count reaches zero. But the count cannot reach zero while
new code lands at 97.5%, so sequencing the gate last means it never
arrives.

What actually forces convergence is a gate that stops NEW uncovered
regions, applied while the existing ones are worked down — not after.
Note the line gate already demonstrates this shape: lines sit at 100%
*because* `--fail-under-lines 100` has been enforced all along, not
because someone once cleaned them up.

## What this does NOT establish

One window, one day, on a fleet that was unusually busy — four other
tracks were merging into this repo. It is a measurement, not a rate.
Re-measure before treating 52/day as a planning constant.

It also does not, by itself, decide what to do. An intermediate
`--fail-under-regions <current>` ratchet would stop the bleeding, but it
asserts a threshold the specification does not state, which makes it a
ratification question rather than a conformance fix — and the
`-txtzn5.11` gate flip is already human-gated on the low-water-mark
window and its `propose-change` rider. The finding belongs to the
maintainer's sequencing decision; it is recorded here and on `-txtzn5.11`
so that decision is made with the number in front of it.

## Reproducing

```
cargo llvm-cov --workspace --all-features --lib --json --output-path /tmp/cov.json
python3 -c "import json;t=json.load(open('/tmp/cov.json'))['data'][0]['totals']['regions'];print(t['covered'],'/',t['count'],'uncovered',t['count']-t['covered'])"
```

`just check-coverage` remains green throughout: the line gate passes with
its one dispositioned unnameable miss (allowance 1, tracked by
`livespec-console-beads-fabro-3yx`). Nothing here is a regression in a
gate that exists today.
