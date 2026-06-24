# Impl-cleanup handoff: drive post-bootstrap work-items to completion (console)

Goal: complete the post-bootstrap implementation cleanup work-items in
`livespec-console-beads-fabro` BEFORE the spec-refinement (critique) track.
Impl only — make NO changes under `SPECIFICATION/` in this track.

## Work-items (console ledger)

- `livespec-console-beads-fabro-0u2` (bug, P1) — make the live factory-drain
  path honest (stop fabricating success).
- `livespec-console-beads-fabro-o1x` (task, P2) — post-bootstrap doc hygiene.
- `livespec-console-beads-fabro-awj` (task, P1, needs-regroom) — wire real
  source + factory I/O behind the existing ports. Large; groom first.

Read each with (run from `/data/projects/livespec-console-beads-fabro`):
`/data/projects/1password-env-wrapper/with-livespec-env.sh -- bd show <id>`

## Discipline (read first)

- Read `AGENTS.md` / `CLAUDE.md` and the `justfile`. The enforced gate is
  `just check` (fmt, strict clippy, tests, nextest, 100% lib coverage,
  cargo-deny, cargo-machete, arch-check).
- Rust product changes (`0u2`, `awj`) MUST follow Red-Green-Replay and MUST
  pass `just check` before landing.
- Commits are REFUSED on the primary checkout
  (`dev-tooling/git-hook-wrapper.sh`); do the work in a worktree
  (e.g. `~/.worktrees/<branch>`) and land via the family worktree → PR → merge
  flow. The doc-only item (`o1x`) is non-product and MAY use the family
  non-product commit exemption.
- This track makes NO `SPECIFICATION/` changes. If you find yourself wanting a
  spec change, STOP and note it for the critique track instead.

## Suggested order

1. **`0u2` (quick, P1).**
   `/livespec-orchestrator-beads-fabro:implement livespec-console-beads-fabro-0u2`
   Make the live drain path report an honest not-wired / no-op outcome instead
   of fabricated success. Red-Green-Replay; `just check` green. Close with
   evidence.
2. **`o1x` (quick, P2).**
   `/livespec-orchestrator-beads-fabro:implement livespec-console-beads-fabro-o1x`
   Retire/refresh `research/tui-first-milestone-bootstrap-plan.md`, fix the root
   `README.md` pointer, and update the `AGENTS.md` "seed state" wording (history/
   v001 now exists). Doc-only. Close with evidence.
3. **`awj` (large, P1).** First groom into ready, dependency-layered slices:
   `/livespec-orchestrator-beads-fabro:groom livespec-console-beads-fabro-awj`
   Then implement each slice (per-adapter: Beads/`bd`, Dispatcher journal,
   Fabro API/ps/SSE, LiveSpec next/doctor/files, GitHub/`gh`, and the real
   Dispatcher-invoking drain) via
   `/livespec-orchestrator-beads-fabro:implement <slice-id>` — each
   Red-Green-Replay with `just check` green. Adapters MUST observe real sources
   or emit honest "not observed" findings; the drain MUST reflect real
   Dispatcher outcomes. Close `awj` when all slices land.

Use `/livespec-orchestrator-beads-fabro:next` to pick the most-ripe item, and
`/livespec-orchestrator-beads-fabro:list-work-items` to check state.

## Done criteria for this track

- `0u2`, `o1x`, and `awj` (with its slices) closed in the console ledger.
- `just check` green locally and in CI on `master`.
- The live serve/tui path no longer uses `ScriptedSource` or
  `SimulatedFactoryDrainPort`.

## Then: hand off to the spec-refinement track

Once these land, proceed to `prompts/spec-refinement-critique-handoff.md` and
run the `/livespec:critique` + `/livespec:revise` cycles.
