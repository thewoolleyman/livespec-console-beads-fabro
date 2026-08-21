# Charter, thread boundary, and sequencing

**Ledger anchor:** `livespec-console-beads-fabro-thu6gp`

This note carries the durable REASONING that lived in this thread's retired
`handoff.md`. It deliberately holds no status and no next action: status is
composed live from the ledger, and the next action is the newest handoff comment
on the plan epic. See `001-anchor-refresh-2026-08-21.md` for the verified anchors
— the ones in the older records are stale.

`handoff.md` was retired because `plan_anchor_declared`, the check that required
it, has retired itself in favour of `plan_epic_parity`, on the ratified ground
that plan anchors are ledger-held rather than git-held.

## Charter

Three invariants this repo relies on but does not mechanically enforce. Each is a
small, self-contained guard whose design is already written down. **None is a
live violation** — every one is a LATENT gap that would let a regression through
silently, re-confirmed 2026-08-21.

"Invariant" here covers both structural invariants of the repo (the
zero-Beads-knowledge rule, the toolchain pin) AND commit protocol (`-mvu22t`'s
red-green-replay hook). The common mechanism is a mechanical guard asserting a
named rule that nothing currently checks.

## Boundary against `plan/test-adequacy-gates/`

That thread measures whether tests are ADEQUATE; this one asserts NAMED RULES.

`-mcj` is the clearest test of the boundary: under vehicle-grouping it would land
there because it touches CI, but its mechanism is a pin-alignment assertion, so
it belongs here.

## Read first

1. `001-anchor-refresh-2026-08-21.md` — every anchor below, re-verified.
2. `crates/console-arch-check/src/main.rs` — `run_checks` (three families today),
   and the vacuity-guard pattern `-p4bvrt` copies.
3. `justfile` — the `targets=(...)` array new guard targets append to; contended
   with `plan/test-adequacy-gates/` (see Sequencing).
4. `lefthook.yml` — must GAIN a `commit-msg` section for `-mvu22t`; it has none.
5. `/data/projects/livespec-dev-tooling/livespec_dev_tooling/checks/red_green_replay.py`
   plus its `_red_green_replay_trailers.py` / `_red_green_replay_modes.py`
   siblings — the port source for `-mvu22t`, in a SIBLING repo.
6. `crates/console-cli/src/backing_cli.rs` — the closed accessor set.
7. `SPECIFICATION/non-functional-requirements.md` — the zero-Beads-knowledge rule
   and the SEPARATE falsifiability requirement. They are not one range.
8. `rust-toolchain.toml`, `Cargo.toml`, `.github/workflows/ci.yml`,
   `.fabro/workflows/implement-work-item/workflow.toml`.
9. `AGENTS.md` — mutation protocol.

## Sequencing

1. **PR #317 lands before `-p4bvrt` starts.** It rewrites the vacuity guard that
   item copies; working from the master shape produces a guard inconsistent with
   its sibling. `run_checks` itself is untouched by #317.
2. **`-mvu22t` last, or behind a deliberate enable flag.** It is the only item
   here that changes every future commit.
3. **`-mcj` slots anywhere.** It is mostly new files plus comment corrections.
4. **Shared files with `plan/test-adequacy-gates/`: BOTH `justfile` AND
   `.github/workflows/ci.yml`.** The genuinely line-adjacent hazard is the
   `targets=(...)` array in `justfile` — that thread edits `check-coverage`
   while new guards here plausibly append to the array.

   **Tie-break, so neither session waits on the other:**
   `plan/test-adequacy-gates/` OWNS `justfile` and `ci.yml` for the duration of
   its region-gate work; this thread rebases onto it. That ordering is not
   arbitrary — its region-coverage flip is a repo-global gate that retroactively
   binds every open PR, including this thread's, so it wants the low-water mark
   and should not be made to wait.
5. Parallel-safe against event-identity, command-queue, and operator-surface.

## Gates

- **`-mvu22t` needs staged-rollout sign-off before admission.** Once landed its
  commit-msg hook gates ALL later commits fleet-wide, and `lefthook.yml` has no
  `commit-msg` section at all today. Test thoroughly before enabling; exempt
  `docs(...)` and `chore(...)`.
- Cross-language parity hazard on the same item: the Rust port will drift from
  the Python original as the trailer grammar evolves. Either pin the ported
  grammar version or add a parity fixture.

## Two findings recorded so they are not re-derived

- **The `-p4bvrt` branch question is closed: there is no branch.**
  `fix/arch-check-suspect-by-default` was deleted on the remote when PR #307
  merged. What survives in some checkouts is a stale local remote-tracking ref
  that `git remote prune origin` clears. Its work is in master (`8f3ee6f` and
  #307's squash `5bddff8` share a patch-id). Nothing to coordinate with.
- **`-mvu22t`'s decorative `ready` LABEL was dropped 2026-07-19.** Recorded only
  because the label's history explains why older notes call the item "ready".
  The ranker keys on STATUS, not labels, so the label never conferred anything.
