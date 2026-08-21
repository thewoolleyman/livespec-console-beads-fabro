# Anchor refresh — every cited anchor in this thread re-verified 2026-08-21

**Ledger anchor:** `livespec-console-beads-fabro-thu6gp`

Scope of this note: a from-scratch re-verification of every file anchor and
factual claim this thread's records send a reader to. Nothing here changes the
charter or the design of any child. All three children are still GENUINE and
still LATENT (no live violation) — but the citations rotted over the month
since they were written, and one child's stated premise is now half-obsolete.

Read this note BEFORE the ledger records for `-p4bvrt`, `-mcj`, or `-mvu22t`.
Those records' designs are sound; their anchors are not.

## The three children are all still genuine

Verified against `master` at `593e428`:

- `-p4bvrt` — `run_checks` (`crates/console-arch-check/src/main.rs:62-71`) still
  runs exactly three families: crate-graph, crate-sources, tmux-socket-scoping.
  No zero-Beads-knowledge rule. `grep -rn red_green_replay` over `crates/`
  returns nothing.
- `-mcj` — the pins are still mutually aligned, so there is still nothing to
  repair and still nothing that would catch drift tomorrow.
- `-mvu22t` — `lefthook.yml` still has NO `commit-msg` section at all.

## Corrected anchors

| Cited in the records as | Actually, 2026-08-21 |
| --- | --- |
| `non-functional-requirements.md:368-369` (zero-Beads rule) | `:386-387` |
| `non-functional-requirements.md:376-377` (falsifiability) | `:394` |
| `main.rs:234-241` (vacuity guard, master) | `:236-242` |
| `Cargo.toml:21` (`rust-version`) | `:23` |
| `.github/workflows/ci.yml:94` (Rust pin prose) | `:167-168` |
| `.fabro/workflows/implement-work-item/workflow.toml:106` (pin prose) | `:155-158` |
| `backing_cli.rs:57-93` (closed accessor set) | `:55-95` |
| dev-tooling `checks/fabro_image_pin_lockstep.py` | `livespec_dev_tooling/fabro_image_pin_lockstep.py` (out of `checks/`) |

The vacuity guard on `master` is STILL in its pre-#317 shape — a bare
`return vec![format!(...)]` at `:236-242`. PR #317's rewrite (to
`findings.push(...)` + `return findings`, with `rust_files_for_tmux_scan`
returning a tuple) has not landed, so the "copy the guard AFTER #317" instruction
in `-p4bvrt` still stands unchanged. Only the line numbers moved.

## `-mcj`'s premise is now HALF-OBSOLETE — its scope shrinks

`-mcj` is filed as a MUTUAL DEFERRAL: "each repo's comment defers the check to
the other." That is no longer true. Both halves changed:

- **dev-tooling's side is now correct and explicit.** `docker/fabro-sandbox/
  python-rust/Dockerfile:14-20` no longer punts. It now states that RUST_VERSION
  carries no lockstep obligation there because dev-tooling is not a Rust repo,
  and that per the No-Circular-Dependency Directive any such check "lives on the
  CONSUMER (console) side reading this image ARG, never in dev-tooling." That is
  an assignment, not a deferral, and it agrees with this thread's plan.
- **The console's counter-deferral is GONE.** The `workflow.toml:117-120` comment
  that assigned the check back to dev-tooling no longer exists; it was removed
  incidentally by the sandbox-image work (`de01baa` → `fc43f26` → `bb53f53`).
  What remains at `:155-158` is purely descriptive ("mirroring
  rust-toolchain.toml") and hides nothing.

So the `-mcj` line item "also correct the misleading workflow.toml comment at
:117-120 that hid the gap" is **already satisfied** and should be struck rather
than worked. What survives is the whole of the actual value: no console-side
guard exists, and the four copies still need reconciling or neutralizing.

The warning NOT to fix this in dev-tooling still holds, with a corrected path:
`livespec_dev_tooling/fabro_image_pin_lockstep.py:30` documents RUST_VERSION as
a deliberately un-obligated extra, and that exclusion is locked in by a passing
test at `tests/livespec_dev_tooling/test_fabro_image_pin_lockstep.py:36,74`.

## The four copies of the pin, re-located

1. `rust-toolchain.toml:2` — `channel = "1.92.0"`, AUTHORITATIVE (unmoved)
2. `Cargo.toml:23` — `rust-version = "1.92"`, a real MSRV cargo enforces
3. `.github/workflows/ci.yml:167-168` — prose comment
4. `.fabro/workflows/implement-work-item/workflow.toml:155-158` — prose comment

Image side: `livespec-dev-tooling` Dockerfile `:33` `ARG RUST_VERSION=1.92.0`
and `:54-55` `--component clippy,rustfmt`. All aligned; components identical.

## PR #317 has been green and unmerged for a month

`harden-tmux-check` / PR #317 is the thread's first sequencing gate. As of
2026-08-21 it is still OPEN, last touched 2026-07-19, with **all 14 required
checks passing** and no review decision recorded. It is not blocked on CI; it is
blocked on a maintainer merge.

## The thread's own record-keeping is out of contract

The plan operation now holds handoffs as append-only comments on the plan epic
and research as files under `plan/<slug>/research/`. This thread had neither:
epic `-thu6gp` carried zero comments, and there was no `research/` directory —
everything lived in `plan/repo-invariant-guards/handoff.md`, which the operation
no longer authors. A fresh session resuming from the ledger alone would have got
nothing at all.

`handoff.md` is deliberately left in place, but NOT because anything still
requires it. `just check` surfaced that the check which used to require it has
RETIRED ITSELF:

    {"check_id": "plan_anchor_declared", "disposition": "retired",
     "replacement": "plan_epic_parity",
     "reason": "ratified Planning Lane uses ledger-held plan epic metadata,
                not git anchor files"}

So `check-plan-anchor-declared` passes trivially whether or not a `handoff.md`
exists, and its successor `plan_epic_parity` asserts against the LEDGER epic
instead. `pyproject.toml:52` still sets `plan_lifecycle_anchor = true`; that flag
now only keeps the retired check from self-skipping silently.

Nothing mechanical therefore blocks retiring this thread's `handoff.md` once its
content lives on the epic. It is left for now only so the six sibling live
threads and this one are retired together rather than one-off, and so this
thread's migration is reviewable as content movement alone.
