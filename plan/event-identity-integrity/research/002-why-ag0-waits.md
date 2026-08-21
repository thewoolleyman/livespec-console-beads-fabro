# Why `-ag0` is `ready` and deliberately undispatched

Written 2026-08-21, resuming this thread. Companion to
[`001-identity-derivation-live-state.md`](001-identity-derivation-live-state.md),
which carries the defect itself. This note carries only the routing question:
why the sole open carrier is admitted but parked.

## The ruling

`-ag0`'s fix re-keys every stored event/version identity, forcing a one-time
re-observation of every work-item and attention item in every deployed console
store. On 2026-08-21 the maintainer ruled: **admit and implement it now, pin the
merge to a window they clear.** In their words as offered and chosen — the churn
is deferred to their call, the work is not.

They also ruled the fix scope: fix `stable_version` ITSELF with netstring
length-prefixing, reusing the unused `length_prefixed` helper, rather than
patching the three call sites individually. That knowingly widens the re-key to
the four label/id-only call sites, and was accepted because the storm is one
event either way.

## The ruling's first half is not executable

Dispatch and merge are one act in this factory. The mechanism, the three levers
that do not hold it, and the reason you cannot simply go off-factory instead are
recorded once, repo-wide, in
[`.ai/factory-dispatch-and-merge-coupling.md`](../../../.ai/factory-dispatch-and-merge-coupling.md).
That file is the durable copy; this note does not duplicate it.

The consequence for this thread: any route that gets `-ag0` implemented before
the window either merges it (factory) or needs a maintainer authorization nobody
has given (in-session). Both were considered and both were withdrawn on this
thread's ledger timeline.

## What is true now

- `-ag0` is `ready`, unassigned, undispatched. **Readiness does not authorize
  dispatch here** — that warning is on the item and on the epic description,
  because `ready` normally does mean "drain me".
- It drains through the factory normally once the maintainer clears the window,
  and the churn lands inside the window they chose.
- The independent way to lift the gate rather than wait for it is `bd-ib-vlhp`
  (P1) in the `livespec-orchestrator-beads-fabro` tenant. It is not a child of
  this epic and does not gate this thread's archive.

## Process note worth keeping

Two routes were decided and then withdrawn on this thread in one session: an
off-factory implement (withdrawn — it collides with the `implement` operation's
Step 0, which reserves that path to explicit maintainer direction), and a
`do-not-merge` label pin (withdrawn — it cannot reach a factory-bot PR). Both
withdrawals are on the ledger timeline with their reasoning.

The generalizable lesson is the cheap check that would have caught both: before
designing around a gate, read the gate. The auto-merge arming is four lines of
`pr.md`, and the routing default is Step 0 of the `implement` prose. Reading
either first would have skipped a full round of design.
