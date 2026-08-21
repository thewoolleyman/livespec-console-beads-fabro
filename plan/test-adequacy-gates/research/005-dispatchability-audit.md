# Dispatchability audit — the slices were filed undispatchable

Measured 2026-08-21, after notes 003 and 004 had corrected the slices'
*content*. This note is about whether the factory can actually take
them. Three defects, all in the filing rather than the work.

## 1. Every child inherited `needs-regroom`

All 13 slices were created carrying `needs-regroom`, the very label the
regroom existed to clear. `bd create --parent` inherits the parent's
labels unless `--no-inherit-labels` is passed, and the parents still
carried the marker at creation time — it was removed from them at the
end of the same script.

`needs-regroom` is not a passive tag. It is the Dispatcher's
**non-convergence escalation marker**: `.fabro/workflows/implement-work-item/workflow.fabro`
routes a fix-loop-cap exhaustion to a `non_converged` terminal, and the
Dispatcher's `is_non_convergence_outcome` marks the slice
`needs-regroom` on that signal. So thirteen freshly-groomed slices were
wearing "the factory already tried this and could not converge".

It does **not** gate selection — there is no label filter in
`_dispatcher_loop_selection.py` — so this was misleading rather than
blocking. Cleared on all 13.

**Carry forward:** pass `--no-inherit-labels` when filing children under
a `needs-regroom` parent, or clear the parent's marker *before* creating
the children rather than after.

## 2. Three slices declared "factory-safe" that cannot be

`-txtzn5.9` (CI fuzz job), `-txtzn5.10` (CI mutation job) and
`-topr34.2` (nightly soak) all add or edit files under
`.github/workflows/`. All three were filed as "Autonomy: factory-safe".

The Dispatcher refuses to sandbox an item whose declared scope edits
`.github/workflows/` (`_dispatcher_host_only.py`), and the refusal is a
**text heuristic over the item's own description** — an edit verb within
80 characters of the literal prefix. None of the three tripped it: `.9`
and `.10` never mentioned the path at all, and `topr34.2` mentioned it
only in a non-edit sentence.

Dispatching any of them would have produced a sandbox branch carrying
workflow edits, which `check-workflow-boundary` fails and the fleet
App's contents-only push token could not push anyway. A wasted run that
dies at the boundary.

All three descriptions now declare the workflow edit explicitly and are
marked HOST-ONLY; re-tested against the heuristic, all three refuse and
`.1`/`.11` correctly do not.

## 3. Dispatch merges, and `-txtzn5.11` cannot be merged on a whim

`.ai/factory-dispatch-and-merge-coupling.md` is unambiguous: dispatching
a work-item to the factory **merges** it, because
`prompts/pr.md` step 5 arms rebase auto-merge unconditionally from
inside the sandbox. Readiness does not authorize dispatch when a merge
must wait.

`-txtzn5.11` is precisely that case — the region-gate flip is a
repo-global gate change that must land at a chosen low-water mark, and
it also carries a `/livespec:propose-change` rider that is
design-human-gated by construction. Neither `do-not-merge`,
`merge_on_review_cap`, nor `factory_safety` can hold it; the doc
explains why each fails. Recorded on the item: when its seven blockers
close, it stays **ready and undispatched** until the maintainer picks
the window.

## 4. The first real dispatch was refused — a mid-session stale build

With the three filing defects fixed, dispatching `-txtzn5.1` still
failed, exit 3:

    ERROR: dispatcher plugin build is stale; executing build 15a4ae9aff88
    predates latest release 5dcbc6829ff9.

The Dispatcher carries a release-currency gate
(`_dispatcher_staleness_gate.py`) that probes
`git ls-remote <orchestrator repo> refs/heads/release` and refuses
admission when the executing build predates it. Nothing about the
work-item was wrong.

The cause is a **mid-session** staleness, which is the part worth
carrying forward. This session's SessionStart hook ran
`just ensure-plugins` and correctly reported `15a4ae9aff88` as latest.
The marketplace then moved to `5dcbc6829ff9` (v0.62.11) *while the
session was running*. A session resolves its plugin root once, at start,
so it goes on invoking the build it resolved — which the gate now
refuses. Re-running `just ensure-plugins` updated the project pin, but
that does not retarget the running session; the fix is to invoke the new
cache path explicitly, or restart.

`console-fork-drift-check` stayed green across the bump ("compared 8
pinned file(s) against installed plugin 5dcbc6829ff9; fork in lockstep
with its pins"), so the pin move cost nothing repo-wide — worth checking
rather than assuming, since that check is what would have reddened
master for every other session.

Note this is a different mechanism from ledger item
`livespec-console-beads-fabro-3ej` ("livespec pin bumps cannot land
here"), which is about the `livespec` CORE pin in `.livespec.jsonc`
frozen at v0.26.0. Same family, different pin, different failure.

## Also observed: no always-on drain took the ready lane

`-txtzn5.1` sat `ready` and unclaimed for over two hours. It is tempting
to conclude that nothing dispatches in this repo; that would be wrong,
and the correction is worth recording alongside the claim. Later the
same afternoon the host was running Fabro for
`livespec-console-beads-fabro-mcj.2` — this repo's own tenant — while
also running `overseer-54k2za.1` and `overseer-hgq4wi.30` for
`/data/projects/livespec-overseer`.

So dispatch here works and is in active use. What does NOT exist is an
always-on drain that picks up whatever is `ready` in this tenant:
dispatch is per-action (`drive --action impl:<id>`), initiated by a
session or an overseer track that has decided to run that item. A slice
marked `ready` here is not therefore a slice that will start — someone
has to dispatch it, and until then it waits regardless of how ripe the
ledger says it is.
