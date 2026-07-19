# Event-identity integrity — console event/version derivation

**Epic anchor:** `livespec-console-beads-fabro-czcjh5`

**Supersedes:** `plan/archive/impl-dispatch/SUPERSEDED-BY.md` (split 2026-07-19), which
carries the routing table showing how these items landed here. Do NOT resume the
archived `handoff.md` beside it.
That thread was a dispatch-queue view that accreted five execution vehicles; this thread
inherits only its event-identity findings.

## Charter

Every event/version identity the console derives MUST be injective over its inputs and
fresh per state transition, so a genuine change can never dedupe away against the
eventstore's unique index.

One mechanism, one thread: identity derivation in the adapters → the
`(source, source_event_id)` unique index → the projection fold.

## Read first

1. This file.
2. `crates/console-application/src/source_adapters.rs` — `stable_version` :1865-1876,
   `length_prefixed` :553-555, `source_stream_seq` :2024-2034,
   `attention_item_version` :2547-2561, `not_observed` stable id :1784-1803.
3. `crates/console-eventstore/src/lib.rs` — unique index (statement opens :48), `insert or ignore`
   :486, duplicate short-circuit :744-752.
4. `AGENTS.md` — credential wrapper, mutation protocol, `gh` 2.46.0 gotchas.

## Status is read live, never stored here

This handoff stores NO queue and NO per-item status (the no-shadow-ledger rule). Read
current state with:

```
/livespec-orchestrator-beads-fabro:list-work-items --json
/livespec-orchestrator-beads-fabro:next --json
```

## The work

Two items, both re-keying event identity. They are the same churn class and belong in
one coordinated migration window.

### `-ag0` — `stable_version` delimits with a raw `0x1f`, no escaping or length prefix

Verified GENUINE on master 2026-07-19 at both accused call sites.

`stable_version` folds each part's bytes then an unconditional `0x1f`, so it is
injective only while no part can CONTAIN that byte. A `length_prefixed` helper already
exists (:553-555) but is reached only from `WorkItemDetail::digest` (:501, :547), which
PR #309 / commit `14499d5` fixed. That commit deliberately scoped these out — its
message says "The shared `stable_version` is left alone, so lifecycle hashes do not
churn again."

Two call sites still pass raw wire-arbitrary parts:
- `source_stream_seq` work-item snapshot identity — `item.rank` / `item.status` are
  plain `String` fields deserialized straight from `list-work-items --json`; nothing
  trims or rejects control characters.
- `attention_item_version` — `summary()` and `handoff().command()` are free text.

Collisions — **the MECHANISM is the claim; no hash value is asserted here.** Because
`stable_version` emits `part_bytes ++ 0x1f` per part, a trailing `0x1f` in part N is
indistinguishable from a leading `0x1f` in part N+1, for ANY fixed values of the
surrounding parts. So:
- work-item snapshot: rank `"a\x1f"` + status `"b"` collides with rank `"a"` + status
  `"\x1fb"` (adjacent parts in the `source_stream_seq` vector at :2024-2034).
- attention: summary `"Approve\x1f"` + `source_ref().repo()` `"console"` collides with
  summary `"Approve"` + `source_ref().repo()` `"\x1fconsole"`. NOTE these must be the
  ADJACENT pair — `summary` at index 4 and `source_ref().repo()` at index 5 — not the
  outer `repo` at index 0, which is not adjacent and would not collide.

An earlier revision quoted specific 64-bit values. They were not reproducible from the
inputs shown and have been removed; recompute against the live code if you want a
concrete pair for a test.

Impact: the colliding version lands in `source_event_id`, the unique index
short-circuits the append, and the console shows the pre-edit record indefinitely —
the exact staleness class `14499d5` was written to kill.

Fix direction: netstring-style length prefixing, reusing `length_prefixed`.

### `-25rvmd` — source-availability tally dedupes across a recovery

Stable id `evt:{source}:{repo}:not_observed` plus `insert or ignore` plus an
order-dependent fold with no epoch means a re-down after a recovery dedupes against the
original down event. Current behavior VIOLATES ratified text (Adapter Contract: the
tally MUST reflect the LATEST poll outcome) — this is impl catching up to spec, not a
spec change.

**It needs a maintainer decision on the epoch scheme before it can groom to ready.**
(Read its live status rather than trusting a value written here.)

## Sequencing

1. `-ag0` first, then `-25rvmd`, never interleaved — same file, same churn class.
2. **`-ag0` merges alone in a communicated window.** It re-keys every stored version,
   forcing a one-time re-observation of every work-item and attention item in every
   deployed console store. Land it when no other session is mid-E2E-verification; the
   churn will otherwise read as a regression, especially to anyone watching the
   attention pane.
3. Merge `-25rvmd`'s identity change in the SAME window so operators eat one
   re-observation storm, not two.
4. Parallel-safe against every other thread with one rule: `-25rvmd`'s
   `console-application/src/lib.rs` diff stays confined to the `unavailable_sources`
   fold (~:2434-2464). If its design grows beyond that fold, escalate to
   must-sequence against the operator-surface thread, which owns the rest of that file.

## Explicitly NOT in this thread

`crates/console-spec-check/src/lib.rs:76` `derive_gap_id` joins with the same raw
`0x1f` — same abstract family, different contract. Its doc comment at :73 declares
byte-identity with a counterpart in SIBLING REPOS, checked out alongside this one under
`/data/projects/` — `livespec/dev-tooling/spec_clauses.py:108-119`, plus a third vendored
copy at
`livespec-orchestrator-beads-fabro/.claude-plugin/scripts/_vendor/livespec_spec_clauses.py:108-119`.
Neither path resolves inside this repo; `git ls-files` will not find them.
Changing it re-keys every persisted `gap_id` and severs gap↔work-item associations
fleet-wide. **Livespec core owns that decision; the console can never move first.**
Filed in the livespec tenant as **`livespec-6bndap`** ("accepted risk"
is a legitimate disposition there; the point of filing was that the hazard was recorded
nowhere). Never put it in the same PR as `-ag0`.

## Gates

- Admission valve per item (maintainer approve → `ready`).
- Maintainer decision on the `-25rvmd` epoch scheme before it can groom.
- Maintainer scheduling window for the `-ag0` re-observation churn.
- Normal PR review/merge.

## Dispatch

Ready items are implemented **factory-side** — the Dispatcher drains `ready`, or an
operator runs `/livespec-orchestrator-beads-fabro:drive --action impl:<id>`. Do NOT
hand-code these inline in a planning session.
