# Event-identity derivation — live state of the charter

Re-verified against `origin/master` at `593e428` on 2026-08-21. Supersedes the
thread's retired git `handoff.md`, whose facts had rotted (see "What rotted").

## Charter

Every event/version identity the console derives MUST be injective over its
inputs and fresh per state transition, so a genuine change can never dedupe away
against the eventstore's `(source, source_event_id)` unique index.

One mechanism, one thread: identity derivation in the adapters → the unique
index → the projection fold.

## Read first

1. This note. Handoff entries and status are ledger-held on epic
   `livespec-console-beads-fabro-czcjh5`, never in this directory.
2. `crates/console-application/src/source_adapters.rs` — `stable_version`
   :2271-2282, `length_prefixed` :615-617, `source_stream_seq` :3121-3123,
   `attention_stream_seq` :3125-3127, `attention_item_version` :3103-3117,
   the availability transition epoch :1748-1794 and its event ids :2177-2230.
3. `crates/console-eventstore/src/lib.rs` — the unique index :48-50,
   `insert or ignore` :502.
4. `AGENTS.md` — credential wrapper, mutation protocol, `gh pr checks --json`.

## Half the charter has shipped

`livespec-console-beads-fabro-25rvmd` (source-availability tally deduping across
a recovery) closed 2026-08-20 via merged PR #721, post-merge janitor green. The
landed design is the ruled one: a per-`(source, repo)` transition epoch threaded
into `source_event_id` itself, incremented only on a state CHANGE, persisted
alongside the adapter checkpoint, applied symmetrically to the `not_observed`
and `observed_idle` markers, with durable-state-loss handled explicitly
(:1764-1766 starts a fresh local epoch rather than pretending continuity).
Consecutive same-state polls still dedupe; each down↔up transition gets a fresh
`global_seq`. The fold-latest-by-`global_seq` projection was left unchanged.

The maintainer epoch decision the retired handoff listed as a blocking gate was
made on 2026-08-20 and is recorded on that work-item's `design` field.

## The remaining carrier — `-ag0`, and it grew

`stable_version` (:2271) folds each part's bytes then an unconditional `0x1f`,
with no escaping and no length prefix. It is injective only while no part can
CONTAIN that byte. Because it emits `part_bytes ++ 0x1f` per part, a trailing
`0x1f` in part N is indistinguishable from a leading `0x1f` in part N+1, for ANY
fixed values of the surrounding parts. The MECHANISM is the claim; no hash value
is asserted here — recompute against live code if a test wants a concrete pair.

`length_prefixed` (:615) already exists but is still reached only from
`WorkItemDetail::digest`, exactly as PR #309 / commit `14499d5` scoped it
("The shared `stable_version` is left alone, so lifecycle hashes do not churn
again").

Three call sites now pass wire-arbitrary parts, not the two `-ag0` was filed
against:

1. **Work-item snapshot identity** — `source_stream_seq` at :2467. Parts are
   `repo, id, lane.label, lane_reason.label, rank, status, admission_policy,
   acceptance_policy, detail_digest`. `rank` and `status` are ADJACENT (indices
   4 and 5) and are plain `String` fields deserialized straight from
   `list-work-items --json`; nothing trims or rejects control characters. So
   `rank="a\x1f", status="b"` collides with `rank="a", status="\x1fb"`.
2. **Attention item identity** — `attention_item_version` at :3103. `summary()`
   (index 4) is ADJACENT to `source_ref().repo()` (index 5); `handoff().command()`
   (index 10) is free text. The adjacency matters: the outer `repo` at index 0
   is NOT adjacent to `summary` and would not collide.
3. **NEW — impl-attention ready repair** — `normalize_impl_attention_ready_snapshot`
   at :1431, introduced 2026-08-18 by `00ed7e1` ("fix: refresh lanes from impl
   attention rows"), a month AFTER `-ag0` was filed. It feeds `item.summary()`
   (index 6) adjacent to `handoff().kind()` (index 7), and
   `handoff().action_id()` (index 8) adjacent to `handoff().command()` (index 9)
   — two more free-text adjacencies.

`-ag0`'s title and description still say "two pre-existing call sites". The
blast radius is three, and it will keep growing while the separator scheme
stands: every new `source_stream_seq` call site inherits the hazard silently.

Impact is unchanged: a colliding version lands in `source_event_id`, the unique
index short-circuits the append, and the console shows the pre-edit record
indefinitely — the staleness class `14499d5` was written to kill.

Fix direction: netstring-style length prefixing, reusing `length_prefixed`,
applied inside `stable_version` so every call site is covered at once rather
than call-site-by-call-site.

## Sequencing — the old coordinated window is moot

The retired handoff prescribed landing `-ag0` and `-25rvmd` in ONE window so
operators ate a single re-observation storm. `-25rvmd` merged alone on
2026-08-20. That constraint cannot be honored and no longer applies.

What survives: `-ag0` re-keys every stored work-item and attention-item version,
so merging it forces a one-time re-observation across every deployed console
store. Land it when no other session is mid-E2E-verification; the churn reads as
a regression to anyone watching the attention pane.

## Explicitly NOT in this thread

`crates/console-spec-check/src/lib.rs` `derive_gap_id` joins with the same raw
`0x1f`. Same abstract family, different contract: its doc comment declares
byte-identity with counterparts in SIBLING repos checked out alongside this one
under `/data/projects/` — `livespec/dev-tooling/spec_clauses.py` and a vendored
third copy under
`livespec-orchestrator-beads-fabro/.claude-plugin/scripts/_vendor/livespec_spec_clauses.py`.
Neither resolves inside this repo; `git ls-files` will not find them. Changing it
re-keys every persisted `gap_id` and severs gap↔work-item associations
fleet-wide. Livespec core owns that decision and the console can never move
first. Filed in the livespec tenant as `livespec-6bndap` ("accepted risk" is a
legitimate disposition there; the point of filing was that the hazard was
recorded nowhere). Never put it in the same PR as `-ag0`.

## What rotted

The retired `handoff.md` claimed `-25rvmd` was `blocked` awaiting a maintainer
epoch decision, and prescribed the one-window sequencing above. Both were true
at the 2026-07-19 split and false by 2026-08-20. It also carried refreshed-then-
stale line anchors and the two-call-site count. Live status is read from the
ledger; that file is removed by this note's commit, per the ratified Planning
Lane rule that live git `handoff.md` files are not a conforming carrier.
