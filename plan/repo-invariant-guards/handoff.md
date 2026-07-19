# Repo-invariant guards — mechanical checks for invariants nothing currently enforces

**Epic anchor:** `livespec-console-beads-fabro-thu6gp`

**Supersedes:** `plan/archive/impl-dispatch/handoff.md` (split 2026-07-19).

## Charter

Three invariants this repo relies on but does not mechanically enforce. Each is a small,
self-contained guard whose design is already written down. None is a live violation
today — every one is a LATENT gap that would let a regression through silently.

Distinct from `plan/test-adequacy-gates/`: that thread measures whether tests are
adequate; this one asserts named structural invariants.

## Read first

1. This file.
2. `crates/console-arch-check/src/main.rs` — `run_checks` :63-71 (exactly three
   families today: crate-graph, crate-sources, tmux-socket-scoping), vacuity-guard
   pattern :234-240.
3. `crates/console-cli/src/backing_cli.rs` — the closed accessor set :57-93.
4. `SPECIFICATION/non-functional-requirements.md:366-368` — the zero-Beads-knowledge
   rule and the falsifiability requirement.
5. `rust-toolchain.toml`, `Cargo.toml:21`, `.github/workflows/ci.yml:93-94`,
   `.fabro/workflows/implement-work-item/workflow.toml:106,117-120`.
6. `AGENTS.md` — mutation protocol.

## Status is read live, never stored here

```
/livespec-orchestrator-beads-fabro:list-work-items --json
```

## The work

### `-p4bvrt` — arch-check gains a falsifiable zero-Beads-knowledge rule

Filed 2026-07-19 as slice 3 of epic `-nxsfih`, which had never had a work-item of its
own. `-nxsfih` is now CLOSED (all three slices dispositioned). The full design below is
also carried on `-p4bvrt` itself.

`non-functional-requirements.md:366-368` enumerates "no crate invokes `bd` or embeds a
Beads-native read path" as a rule the Architecture Tests MUST enforce, falsifiably.
`run_checks` does not enforce it. The invariant HOLDS today — this is a latent guard
gap, not a live violation — but it is the load-bearing invariant of the whole console
design (work-item-state-machine decision 16: the console has zero beads knowledge; the
orchestrator CLI is its only work-item interface).

**THE TRAP — the obvious implementation is VACUOUS.** A full design is recorded as a
comment on `-nxsfih` (2026-07-19 16:24); read it before writing code. Summary:

- Scanning for `Command::new("bd")` protects nothing. There is exactly ONE process-spawn
  site in the product crates (`console-cli/src/main.rs:321`), and its program is a
  RUNTIME VALUE — there is no literal to match, and grep for a literal `"bd"` returns
  nothing today. Such a check reports green forever, even after `bd` becomes resolvable.
  This is the exact failure `5bddff8` documented when hardening the tmux rule.
- **The right shape asserts the CLOSED ALLOW-LIST, not the absent hazard.** Extract the
  compiled-in default program set from `backing_cli.rs` and fail any program not on an
  explicit permitted list — suspect-by-default, so adding a backing CLI forces a
  deliberate decision.
- Assert no product crate reads a Beads-native store (no `.beads` path construction, no
  Dolt/SQLite handle against a beads database).
- **Refuse to pass vacuously** — if the walk finds no Rust files or `backing_cli.rs`
  cannot be parsed, FAIL. Copy the `paths.is_empty()` guard at :234-240.
- **State the honest limit in the check's doc comment.** `from_environment`
  (`backing_cli.rs:139`) calls `apply_program_overrides`, so env vars can swap a backing
  program at runtime. No static check covers that. The guard's honest promise is "the
  compiled-in defaults contain no Beads-native program, and the resolvable set cannot be
  widened without editing a watched file" — NOT "the console can never invoke bd."

Acceptance: red on a seeded `bd` in the default set, red on a seeded `.beads` read, red
when the scan finds no files, green on unmodified master. Paired must-flag/must-not-flag
tests per case.

**Coordinate:** remote branch `fix/arch-check-suspect-by-default` touches this exact
crate — verify whether it is still live before filing. PR #317 also appends to
`run_checks`; merge it first.

### `-mcj` — no guard binds `rust-toolchain.toml` to the baked image's `RUST_VERSION`

Verified GENUINE and still aligned (both read 1.92.0, components identical), so there is
no drift to repair today — only the absence of anything that would catch drift tomorrow.

The hole is MUTUAL DEFERRAL. `livespec-dev-tooling`'s
`docker/fabro-sandbox/python-rust/Dockerfile:14-20` assigns the check to the console;
the console's `.fabro/workflows/implement-work-item/workflow.toml:117-120` assigns it
back to dev-tooling. Neither implements it.

**The fix is console-side and single-repo**, per the No-Circular-Dependency Directive.
Do NOT "fix" this in dev-tooling: `fabro_image_pin_lockstep.py` deliberately excludes
RUST_VERSION (it parses the ARG at :90-98 then discards it), and that exclusion is
LOCKED IN BY A PASSING TEST (`test_fabro_image_pin_lockstep.py:36,59`).

Grooming must WIDEN the item's scope — there are **four** copies of the pin in this
repo, not the two the item names:
- `rust-toolchain.toml:2` — authoritative
- `Cargo.toml:21` `rust-version = "1.92"` — a real MSRV declaration cargo enforces
- `.github/workflows/ci.yml:94` — prose comment (the unfiled third copy)
- `.fabro/workflows/implement-work-item/workflow.toml:106` — prose comment

Also correct the misleading workflow.toml comment at :117-120 that hid the gap.

Prefer interrogating the actual image (`docker inspect` / `rustc --version`) over
text-reading the Dockerfile: if dev-tooling renames the ARG or reshuffles layers, a
text-reading guard goes silently vacuous. **Fail loudly when the probe finds nothing.**

### `-mvu22t` — Rust Red-Green-Replay commit-msg enforcement

Verified GENUINE: no `red_green_replay` reference exists in `justfile`, hooks, or
`crates/`. The spec is ALREADY ratified — NFR §"Red-Green-Replay" says "Until that check
is wired, this requirement is unmet, not waived." No propose-change needed.

**Stale path in the item body, correct at grooming:** the source to port now lives at
`livespec-dev-tooling/livespec_dev_tooling/checks/red_green_replay.py` (plus
`_trailers`/`_modes`). `livespec/dev-tooling/checks/` holds no source, only
`__pycache__`.

**Drop the decorative `ready` LABEL.** The item sits at `backlog` STATUS, and the ranker
keys on status — the label confers nothing and misleadingly implies an admission that
has not happened. Evidence is on the item's own 2026-07-19 comment.

**BLAST RADIUS — the reason this needs staged rollout.** Once landed, its commit-msg
hook gates ALL later commits fleet-wide. `lefthook.yml` currently has NO `commit-msg`
section at all — one must be added. Test thoroughly before enabling; exempt
`docs(...)` and `chore(...)`.

Cross-language parity hazard: the Rust port will drift from the Python original as the
trailer grammar evolves. Either pin the ported grammar version or add a parity fixture.

## Sequencing

1. **Merge PR #317 first** — it appends to `run_checks` in the same file the
   zero-Beads guard must extend.
2. ~~`-nxsfih` closes only after its slice-3 child exists~~ — DONE 2026-07-19: `-p4bvrt`
   was filed first, then `-nxsfih` closed, in that order.
3. `-mvu22t` last, or behind a deliberate enable flag — it is the only item here that
   changes every future commit.
4. `-mcj` slots anywhere; it is mostly new files plus comment corrections.
5. Shares `justfile` with `plan/test-adequacy-gates/` — shallow, but coordinate.
6. Parallel-safe against event-identity, command-queue and operator-surface.

## Gates

- Maintainer review + merge of PR #317.
- Admission valve per item.
- Staged-rollout sign-off for the `-mvu22t` commit-msg hook.
- Coordination check on `fix/arch-check-suspect-by-default` before filing the child.

## Dispatch

Ready items go **factory-side** — the Dispatcher drains `ready`, or run
`/livespec-orchestrator-beads-fabro:drive --action impl:<id>`.
