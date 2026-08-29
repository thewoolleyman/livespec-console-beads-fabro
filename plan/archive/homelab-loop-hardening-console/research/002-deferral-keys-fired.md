# 002 — Deferral keys: four have fired; the code claims re-verified

Recorded 2026-08-25 by the `homelab-loop-hardening-console` plan thread,
executing the next action the previous handoff recorded ("verify the
deferral keys ... and file that leg's console child when one has
fired"). This note is the verification record; the scope event on the
plan epic is the decision record it feeds.

Baseline for every claim below: this repository at `master` `2c40e31`.

## The keys

Each deferral recorded in the seeding scope event names a concrete
reconsideration point. Four have now fired, and each was verified as a
RELEASE, not merely a ratified snapshot — the distinction the previous
handoff insisted on.

| Leg | Recorded key | Verdict |
| --- | --- | --- |
| 1 — additive-kind fixture tests + real producer-payload compatibility test | the runtime baseline is RELEASED (not merely ratified) | **FIRED** |
| 2 — identity (real principal; `requested_by` forwarding) | the orchestrator filing designs the CLI identity input and the resolution-order contract | **FIRED** |
| 3 — the ONE bundled propose-change | orchestrator per-key declarations + runtime attention-surface ratification | **INPUTS LANDED** |
| 6 — settings lockstep, per key | the orchestrator declares a key API-configurable, ratified AND released | **FIRED for one key of two** |
| 7 — envelope-tolerance two-item test | the envelope ratification decides the consumer-tolerance posture | **FIRED** |

### Leg 1 — `livespec-runtime` v0.21.4

The v012 attention-surface baseline ratification (`970eea1`,
`chore(spec): ratify v012 -- the shared attention surface baseline`) is
an ancestor of tag **`v0.21.4`** (release commit `d5b5ea4`, cut
2026-08-25 13:57 UTC). It is NOT an ancestor of `v0.21.3`, which is what
the previous handoff measured when it recorded "ratified but no release
cut". The release also carries `6421fcc`, `fix: accept the internal
prefix in the attention stable-ID grammar` — a composer-side fix landed
between the ratification and the release, which is directly relevant to
the "v012 ratifies invariants the shipped composer does not yet satisfy"
caveat: the wire test must still grade against the released producer
payload, not against the ratified prose.

Runtime `v013` and `v014` are ratified on master but are NOT in any
release tag. They are later baselines and are not this leg's key.

### Leg 2 — orchestrator v073, released in v0.72.4

`SPECIFICATION/contracts.md` §"Journal invoker attribution" (introduced
by `6def7aa0`, `fix(spec): ratify journal invoker attribution (v073)`,
first released in **`v0.72.4`**) is the pairing this leg waited for. It
fixes the resolution order the console must adopt:

1. `--invoker <id>` on the invocation (`invoker_source: flag`),
2. otherwise `LIVESPEC_INVOKER` when set and non-empty (`env`),
3. otherwise `unattributed:<os-user>@<hostname>` (`fallback`) — "a MARK,
   not an identity: it records that no caller asserted who acted".

The recommended convention is `<role>:<name>`, and the contract names
**`console:<principal>`** explicitly as one of its examples, so the
console's own identity form is already anticipated upstream.

Note for the settings leg: `dispatcher.require_invoker` is deliberately
NOT API-configurable and MUST NOT be editable through the console
Settings surface — "a dial that relaxes attribution MUST NOT be
reachable over the surface whose acts it attributes". That is a negative
obligation on this repository, and the settings-completeness check must
keep honouring it.

### Leg 7 — orchestrator v077, released in v0.72.8

`3ec58721`, `fix(spec): ratify the needs-attention machine envelope
(v077)`, first released in **`v0.72.8`**, decides the posture:

> A consumer MUST be able to skip an item it cannot parse — malformed
> fields, or an unknown `kind` it chooses not to render — while
> consuming the rest of the envelope, surfacing what it skipped; a
> consumer whose parse discards the WHOLE envelope on one bad item is
> non-conforming (one malformed item blinding the entire inbox is the
> failure mode this posture exists to forbid).

It further binds `kind` as an open string set: "an unknown `kind` is a
well-formed item". The clause states it "binds this repository's own
consuming surfaces and is the producer-declared contract downstream
consumers pin" — so it reaches this console as a pinned contract, and
the current behaviour (below) is non-conforming against it TODAY.

### Leg 6 — orchestrator v078, released in v0.72.9 — one key of two

`4dcd6ae1`, `fix(spec): ratify detection coverage records and staleness
facts (v078)`, first released in **`v0.72.9`**, declares
`dispatcher.drift_capture_merge_threshold` (positive integer, default
`1`) **API-configurable**, with the consumer-side legs explicitly
assigned to this repository's own specification. No per-item override —
"detection recency is a repository property".

The second key the previous handoff named,
`dispatcher.ready_aging_threshold_hours`, is **NOT** ratified: it
appears only in the orchestrator's pending
`SPECIFICATION/proposed_changes/needs-attention-completeness.md`. This
leg therefore fires for ONE row now; the second row stays deferred on
its own revise-and-release.

## The code claims, re-verified

The seeding note required the reviews' code claims to be re-verified in
this repository before any child that builds on them files. Verified at
`2c40e31`:

- **The hardcoded principal (leg 2) — CONFIRMED.**
  `crates/console-cli/src/main.rs:236` passes the string literal
  `"operator"` as `requested_by` into
  `run_store_backed_tui_session`. There is no `--invoker` flag and no
  `LIVESPEC_INVOKER` read anywhere in the CLI; the only environment
  inputs it reads are `LIVESPEC_CONSOLE_REPO` and
  `LIVESPEC_CONSOLE_STORE_PATH`. Every other `"operator"` occurrence is
  test scaffolding.

- **The all-or-nothing envelope parse (leg 7) — CONFIRMED, on two
  separate axes.** `parse_needs_attention_snapshot`
  (`crates/console-application/src/source_adapters.rs:2882`) fails the
  WHOLE envelope when any ONE item is bad:
  1. `serde_json::from_str::<Envelope>` deserializes
     `attention: Vec<AttentionItemSnapshot>` in one shot, so a single
     item with a malformed field aborts the entire parse; and
  2. the id loop `return Err(...)`s on the FIRST item with an empty
     `id`, discarding the well-formed items alongside it.
  The caller (same file, `:2854`) turns that `Err` into a not-observed
  finding, so one bad item blinds the entire inbox — precisely the
  failure mode v077 names and forbids. Nothing in the parser surfaces
  WHAT was skipped, because nothing is skipped: everything is dropped.

- **`kind` is already an open string set (leg 1) — no local enum.**
  `AttentionItemSnapshot.kind` is a plain `String`
  (`source_adapters.rs:2734`), so ruling R2's generic-not-local
  requirement already holds at the type level. What is missing is the
  evidence: additive-kind fixture tests and a real producer-payload
  compatibility test.

- **The settings capture is stale for the new key (leg 6) — CONFIRMED.**
  `tests/fixtures/orchestrator-config-manifest.json` carries six keys
  (`auto_approve_ready`, `merge_on_review_cap`, `acceptance_mode`,
  `review_fix_cap`, `acceptance_rework_cap`, `wip_cap`) and does NOT
  carry `drift_capture_merge_threshold`. The capture is digest-stamped
  and fails closed on a key-set change, so `just refresh-config-manifest`
  against the released orchestrator is the trigger that surfaces the
  missing row.

- **The attention→`Ready` normalization is GONE.** Already removed by
  `livespec-console-beads-fabro-cddfxl` (PR #830); re-confirmed absent.
  No re-work is owed on that claim.

## What needs a console spec change, and what does not

This determines factory-eligibility per leg, so it is decided here
rather than at filing time.

- **Leg 6 needs NO console propose-change to add the key.**
  `SPECIFICATION/contracts.md` already states it outright: the console
  "MUST NOT hardcode that list: it MUST read the orchestrator's
  published declaration of its API-configurable keys, so a key the
  orchestrator adds needs no console spec change to appear." The
  adjacent prose sentence "The six settings the console commands and
  observes are ..." does go stale at seven, but that is a prose
  consistency repair, not a gate.

- **Leg 7 DOES need one.** The console's needs-attention adapter
  contract describes the flat `attention[]` read but says nothing about
  per-item tolerance, while the adapter-honesty rule reserves a
  not-observed finding for "an uninterpretable payload" — which is
  exactly the licence the current whole-envelope parse claims. Adopting
  v077's per-item posture NARROWS that clause (a bad ITEM is not an
  uninterpretable ENVELOPE) and adds the surface-what-was-skipped
  obligation. That is a genuine spec amendment.

- **Leg 2 DOES need one.** The console spec carries `requested_by` only
  as a command payload field ("user-or-agent"); it fixes no resolution
  order and no principal form. The three-step order and the
  `console:<principal>` form are load-bearing and must be specified
  before they are implemented.

Leg 3 was cut at seeding as "the ONE bundled propose-change". Legs 2
and 7 each now owe a spec clause, and leg 6 owes a prose de-enumeration.
Filing three separate revisions would contradict that cut, so the
bundled propose-change is the carrier for all of them — this is the
sequencing decision the scope event records.

## Next

Record the scope event promoting legs 1, 2, 6 (one row), and 7 to
requirement carriers with leg 3 as their spec carrier, then file the
children under the plan epic.
