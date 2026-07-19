# Repo-invariant guards — mechanical checks for invariants nothing currently enforces

**Epic anchor:** `livespec-console-beads-fabro-thu6gp`

**Supersedes:** `plan/archive/impl-dispatch/SUPERSEDED-BY.md` (split 2026-07-19), which
carries the routing table showing how these items landed here. Do NOT resume the
archived `handoff.md` beside it.

## Charter

Three invariants this repo relies on but does not mechanically enforce. Each is a small,
self-contained guard whose design is already written down. None is a live violation
today — every one is a LATENT gap that would let a regression through silently.

Scope note: "invariant" here covers both structural invariants of the repo (the
zero-Beads-knowledge rule, the toolchain pin) AND commit protocol (`-mvu22t`'s
red-green-replay hook). The common mechanism is a mechanical guard asserting a named
rule that nothing currently checks.

Distinct from `plan/test-adequacy-gates/`: that thread measures whether tests are
adequate; this one asserts named rules. `-mcj` is the clearest test of the boundary —
under vehicle-grouping it would land there because it touches CI, but its mechanism is
a pin-alignment assertion, so it belongs here.

## Read first

1. This file.
2. `crates/console-arch-check/src/main.rs` — `run_checks` :63-71 (exactly three
   families today: crate-graph, crate-sources, tmux-socket-scoping), vacuity-guard
   pattern :234-241.
3. `justfile` — the `targets=(...)` array (~:151-167) that new guard targets append to;
   contended with `plan/test-adequacy-gates/` (see §Sequencing).
4. `lefthook.yml` — must GAIN a `commit-msg` section for `-mvu22t`; it has none today.
5. `/data/projects/livespec-dev-tooling/livespec_dev_tooling/checks/red_green_replay.py`
   (plus `_trailers`/`_modes`) — the actual port source for `-mvu22t`, in a SIBLING repo.
6. `crates/console-cli/src/backing_cli.rs` — the closed accessor set :57-93.
7. `SPECIFICATION/non-functional-requirements.md` — the zero-Beads-knowledge rule at
   `:368-369`, and the SEPARATE falsifiability requirement at `:376-377` ("each enforced
   rule MUST be stated and checked falsifiably"). They are not one range: `:365-366` is
   the source-adapter rule and `:367` is the adjacent "UI does not call
   Beads/Fabro/LiveSpec/GitHub directly" rule — which is plausibly how the superseded
   `:366-368` anchor arose in the first place.
8. `rust-toolchain.toml`, `Cargo.toml:21`, `.github/workflows/ci.yml:93-94`,
   `.fabro/workflows/implement-work-item/workflow.toml:106,117-120`.
9. `AGENTS.md` — mutation protocol.

## Status is read live, never stored here

```
/livespec-orchestrator-beads-fabro:list-work-items --json
```

## Nothing here is agent-dispatchable — every first act is the maintainer's

All three children sit at `backlog` (read live to confirm), so the Dispatcher and `next`
return nothing. **There is no agent work in this thread today.**

Get the route right — `backlog` is NOT the admission valve:
- `approve` is defined (`contracts.md:442`) as the `pending-approval -> ready` transition
  ONLY. None of these items is at `pending-approval`, so approve does not apply.
- The orchestrator REFUSES `pending-approval` as a `move` target (`contracts.md:450-451`),
  so there is no route INTO the valve either.
- The actual route out of `backlog` is **`move:<work-item-id>:ready`** — a maintainer act.

Per item, the honest first act:
- `-p4bvrt` — maintainer `move:<id>:ready`, but merge PR #317 first (it reshapes the
  guard this item copies).
- `-mcj` — GROOM first. Its own body says the scope must be WIDENED to four pin copies;
  moving it to `ready` as filed would dispatch an understated item.
- `-mvu22t` — needs staged-rollout sign-off before it is safe to move at all.

## The work

### `-p4bvrt` — arch-check gains a falsifiable zero-Beads-knowledge rule

Filed 2026-07-19 as slice 3 of epic `-nxsfih`, which had never had a work-item of its
own. `-nxsfih` is now CLOSED (all three slices dispositioned). The full design below is
also carried on `-p4bvrt` itself.

`non-functional-requirements.md:368-369` enumerates "no crate invokes `bd` or embeds a
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
  cannot be parsed, FAIL. Copy the `paths.is_empty()` guard (`:234-241` on master,
  `:234-242` after #317) — but read it
  AFTER merging PR #317, which rewrites it (see the gate note below). Copying the
  master shape would produce a guard inconsistent with its sibling.
- **State the honest limit in the check's doc comment.** `from_environment`
  (`backing_cli.rs:139`) calls `apply_program_overrides`, so env vars can swap a backing
  program at runtime. No static check covers that. The guard's honest promise is "the
  compiled-in defaults contain no Beads-native program, and the resolvable set cannot be
  widened without editing a watched file" — NOT "the console can never invoke bd."

Acceptance: red on a seeded `bd` in the default set, red on a seeded `.beads` read, red
when the scan finds no files, green on unmodified master. Paired must-flag/must-not-flag
tests per case.

**RETRIEVING THE DESIGN.** `-p4bvrt` says to read the full design on the `-nxsfih`
2026-07-19 16:24 comment before writing code. `-nxsfih` is CLOSED, so
`list-work-items --json` will NOT surface it. Fetch it explicitly:

```
/data/projects/1password-env-wrapper/with-livespec-env.sh -- \
  bd show livespec-console-beads-fabro-nxsfih
```

(The same design is also carried on `-p4bvrt` itself, so that record alone is sufficient
if you prefer one call.)

**STALE ANCHORS IN THE RECORDS THIS HANDOFF SENDS YOU TO.** Three records — `-p4bvrt`,
the `-nxsfih` design comment, and `-mcj` — cite
`non-functional-requirements.md:366-368` (superseded: the rule is `:368-369`,
falsifiability is `:376-377`) and/or `main.rs:234-240` for the vacuity guard. The guard
is `:234-241` on master and **drifts to `:234-242` after PR #317** (which adds a
`return findings;` line). `-mcj`'s record additionally describes the guard in its
PRE-#317 shape. The designs are sound; only these anchors are stale. Correction comments
are on the records.

**Branch question RESOLVED 2026-07-19 — no coordination needed.** Remote branch
`fix/arch-check-suspect-by-default` touches this crate but is fully landed and stale:
its sole commit `8f3ee6f` is already in master by patch-id (`git cherry` reports `-`)
and its PR #307 is MERGED. It is a deletion candidate, not in-flight work.

**PR #317 IS the real gate, and it REWRITES THE GUARD THIS ITEM TELLS YOU TO COPY.**
Read the diff body, not the hunk labels: git labels a hunk with the *preceding* function
signature, so `@@ -231,15 @@ fn check_crate_sources` is misleading — those changed lines
are inside `check_tmux_socket_scoping` and they ARE the `paths.is_empty()` vacuity guard
at :234-241.

After #317 merges, the pattern changes shape:
- `rust_files_for_tmux_scan` returns `(Vec<PathBuf>, Vec<String>)` instead of
  `Vec<PathBuf>` — it now accumulates its own findings.
- The guard becomes `findings.push(...)` followed by `return findings`, instead of
  `return vec![format!(...)]`.

**So copy the guard as it exists AFTER #317, not as described from master.** `run_checks`
(:63-71) is genuinely untouched and unshifted (#317's first hunk starts at old :231), so
that anchor holds either way.

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
`_trailers`/`_modes`), consumed by `livespec` as an installed package. Note
`livespec/dev-tooling/checks/` still holds 9 other check sources — it is only
`red_green_replay.py` that moved out of it. (An earlier note in this thread and a
comment on the item wrongly said that directory was empty of source; disregard it.)

**The decorative `ready` LABEL was already dropped 2026-07-19** — the item now reads
`LABELS: origin:freeform`. Nothing to do; recorded here only because the label's history
explains why older notes call this item "ready". The ranker keys on STATUS, not labels,
so the label never conferred anything.

**BLAST RADIUS — the reason this needs staged rollout.** Once landed, its commit-msg
hook gates ALL later commits fleet-wide. `lefthook.yml` currently has NO `commit-msg`
section at all — one must be added. Test thoroughly before enabling; exempt
`docs(...)` and `chore(...)`.

Cross-language parity hazard: the Rust port will drift from the Python original as the
trailer grammar evolves. Either pin the ported grammar version or add a parity fixture.

## Sequencing

1. **Merge PR #317 first — it rewrites the vacuity guard this thread copies** (and adds
   ~109 test lines in the same file). `run_checks` itself is untouched and unshifted.
   See the note under `-p4bvrt`; do not work from the master shape of the guard.
2. ~~`-nxsfih` closes only after its slice-3 child exists~~ — DONE 2026-07-19: `-p4bvrt`
   was filed first, then `-nxsfih` closed, in that order.
3. `-mvu22t` last, or behind a deliberate enable flag — it is the only item here that
   changes every future commit.
4. `-mcj` slots anywhere; it is mostly new files plus comment corrections.
5. **Shared files with `plan/test-adequacy-gates/`: BOTH `justfile` AND
   `.github/workflows/ci.yml`** (that thread adds CI jobs; `-mcj` here reconciles the
   pin comment at `ci.yml:94`). The one genuinely line-adjacent hazard is the
   `targets=(...)` array in `justfile` (~:151-167) — that thread edits `check-coverage`
   at :195 while new guards here plausibly append to the array.

   **Tie-break, so neither session waits on the other:** `plan/test-adequacy-gates/`
   OWNS `justfile` and `ci.yml` for the duration of its region-gate work; this thread
   rebases onto it. That ordering is not arbitrary — its region-coverage flip is a
   repo-global gate that retroactively binds every open PR, including this thread's, so
   it wants the low-water mark and should not be made to wait.
6. Parallel-safe against event-identity, command-queue and operator-surface.

## Gates

- Maintainer review + merge of PR #317.
- Maintainer `move:<id>:ready` per item (NOT `approve` — see the dispatchability section;
  these are `backlog`, not `pending-approval`).
- Staged-rollout sign-off for the `-mvu22t` commit-msg hook.


## Dispatch

Ready items go **factory-side** — the Dispatcher drains `ready`, or run
`/livespec-orchestrator-beads-fabro:drive --action impl:<id>`.
