# 004 — Deferral key B has fired; the config-manifest capture is fabricated

Recorded 2026-08-26 by the `homelab-loop-hardening-console` plan thread,
executing the next action handoff 13 recorded ("verify the two deferral
keys ... and when one has fired, file or unblock that leg's console
child"). This note is the verification record for both keys, plus one
defect the verification surfaced in already-merged work.

Baseline for every claim below: this repository at `master` `9637b98`
(PR #838 merged 2026-08-26T00:59:48Z, merge sha `9637b98b`), the
orchestrator at `origin/master` `e9209bfb` plus its tags, and
`livespec-runtime` at `origin/master` `aa59c69` plus its tags.

## Key A — leg 1 / `sc23vv` — HAS NOT FIRED

The recorded key: runtime charge point 8's ordered per-consumer release
matrix must EXIST and must explicitly disposition each pre-matrix tag
(bless or supersede) before this console grades fixtures against it.

Verified independently rather than taken from the homelab relay:

- `livespec-runtime` carries no matrix artifact. A tree-wide search for
  `release matrix`, `per-consumer release`, and `pre-matrix` across
  `SPECIFICATION/` and `plan/` returns exactly one hit, and it is a
  historical proposal snapshot
  (`SPECIFICATION/history/v014/proposed_changes/emu-validation-seam-and-pre-major-provision.md`),
  not the matrix.
- The runtime's own plan thread `plan/homelab-loop-hardening-runtime/`
  holds two research notes, `001-charge-and-baseline-scope.md` and
  `002-contracts-drift-false-green.md`. Charge point 8 is stated in
  001 as an unstarted deliverable ("Release + ORDERED per-consumer
  fan-out matrix built from ACTUAL shapes and pins, to be re-verified at
  filing time"); 002 is a different charge point. Nothing has authored
  the matrix.
- `v0.22.0` has since been cut, so the matrix — when it exists — now has
  at least two pre-matrix tags to disposition (`v0.21.4` and `v0.22.0`),
  which is the outcome the leg-1 re-key anticipated.

The homelab `steady-state-loop-hardening` thread reached the same
conclusion independently on its own timeline (`hl-nkuzaz` handoff 34,
2026-08-26T00:56:13Z: "console — ... sc23vv holds on the charge-point-8
key"). Two independent readings agree.

`livespec-console-beads-fabro-sc23vv` stays `blocked`, undispatched,
with its provisional-tag flag intact.

## Key B — leg 6 row 2 — HAS FIRED

The recorded key: `dispatcher.ready_aging_threshold_hours` is
reconsidered when the orchestrator's pending
`SPECIFICATION/proposed_changes/needs-attention-completeness.md` is
revised AND the carrying orchestrator version is released — the same bar
row 1 cleared.

Both halves are met:

- **Revised.** The proposal is out of `proposed_changes/` (which now
  holds only `factory-headroom-preflight.md`, `wip-cap-bound-honesty.md`,
  and `wip-cap-naming-collision.md`) and snapshotted at
  `SPECIFICATION/history/v079/proposed_changes/needs-attention-completeness.md`
  with its `-revision.md` sibling. The ratifying commit is `7c54eb39`,
  `fix(spec): ratify the orchestrator-owned attention facts (v079)`.
- **Declared API-configurable.** `SPECIFICATION/contracts.md` states it
  outright: "**`dispatcher.ready_aging_threshold_hours`** (sourced from
  this repo's `.livespec.jsonc`, positive integer, default **24**) — the
  aging trigger. Declared **API-configurable**: it appears in the console
  Settings surface per §'API-configurable completeness'. No per-item
  override — aging is a repository property."
- **Released.** `git tag --contains 7c54eb39` yields `v0.72.10`,
  `v0.73.0`, `v0.74.0`, `v0.75.0`. The first is `v0.72.10`, cut
  2026-08-25T15:55:52Z. This matches the orchestrator's own release plan
  as relayed on `hl-nkuzaz` handoff 32 — "spec surface v0.72.10 (only tag
  with all of v071–v079; v0.72.9 is a mis-pin trap)".

So the key fires on precisely the bar row 1 cleared (v078, released in
`v0.72.9`). Leg 6's second row is admissible.

## The defect the verification surfaced: the capture is not a capture

Verifying key B meant looking at how row 1 actually landed, and row 1's
merged work does not do what the gate documents.

`crates/console-completeness-check/src/lib.rs` describes its input
precisely: "The published-key surface is read from a COMMITTED capture of
the orchestrator's `config-manifest` (hermetic — `just check`/CI run
offline, no live orchestrator)", refreshed by `just
refresh-config-manifest`, which shells
`livespec-orchestrator-drive --action config-manifest --json` into the
checker's `--refresh` mode.

`tests/fixtures/orchestrator-config-manifest.json` now carries SEVEN
keys, the seventh being `drift_capture_merge_threshold`, added by
`69404cd7` ("feat: pin drift capture settings row", PR #836) along with a
re-stamped `captured_key_set_digest`.

**No orchestrator build publishes that key.** The declaration lives in
`CONFIG_KEYS` in
`.claude-plugin/scripts/livespec_orchestrator_beads_fabro/commands/_drive_config_schema.py`,
and that tuple holds SIX entries — `auto_approve_ready`,
`merge_on_review_cap`, `acceptance_mode`, `review_fix_cap`,
`acceptance_rework_cap`, `wip_cap` — in every one of:

| Surface | Keys |
| --- | --- |
| orchestrator `origin/master` (`e9209bfb`) | 6 |
| tag `v0.72.10` (the v079 spec release) | 6 |
| tag `v0.75.0` (newest) | 6 |
| installed plugin build `96b13b96975b` (= `v0.72.9`, the build the `evasgx` item itself named as the refresh source) | 6 |
| installed plugin builds `fa53a71e9bac`, `7089489f937f` | 6 |

A tree-wide search of the orchestrator for `drift_capture_merge_threshold`
outside `SPECIFICATION/` returns nothing. The key exists upstream as
ratified PROSE only; the orchestrator's own `config-manifest`
implementation has not caught up to its own v078/v079 declarations.

Three consequences, in ascending severity:

1. **The fixture is a hand-authored expectation wearing a capture's
   name.** Running `just refresh-config-manifest` today against any
   available orchestrator build removes the seventh key and re-stamps the
   digest — the refresh path actively reverts the merged work. Nothing
   in CI can catch this, because the capture is hermetic by design: the
   check has no way to tell a genuine capture from a fabricated one.
2. **The console surfaces a setting no orchestrator accepts.** The
   Settings row writes through the orchestrator's `set-config` action,
   which resolves the key via `config_key_by_name(key=key)` and returns
   `invalid-config-key` for anything outside `CONFIG_KEYS`. An operator
   who edits the `drift_capture_merge_threshold` row is refused by the
   producer. This is user-facing, not hygiene.
3. **Filing leg 6 row 2 the same way would fabricate a second row** and
   double the divergence.

The one-directional shape of the check is why this went unnoticed:
`CompletenessReport` reports only declared keys MISSING from console
surfaces. An extra console row for an UNDECLARED key is not a finding, so
a genuine six-key refresh would leave `check-completeness` green while
the console carried a seventh row the producer rejects.

## What this means for row 2's key

Key B has fired on the bar the scope event recorded, and that bar was
about the SPEC declaration. The mechanism the console uses to consume a
declaration — a capture of the producer's `config-manifest` — needs the
producer's IMPLEMENTATION, which is a strictly later event. Row 1
crossed that gap by fabricating; row 2 must not.

So row 2 is re-keyed one notch, from "ratified and released in the
orchestrator's specification" to "published by the orchestrator's
released `config-manifest` implementation". This is a narrowing of the
same key, made explicit because the first row's experience proved the
spec-level bar insufficient to execute against. The orchestrator's
implementation surface is advancing under `bd-ib-ujihbw` (relayed on
`hl-nkuzaz` handoff 32: "implementation surface beginning v0.73.0 ... and
ADVANCING as children merge"), so this is a bounded wait with a named
carrier upstream, not an open-ended deferral.

The capture-integrity repair is NOT keyed to upstream and is executable
now.

## Upstream report owed

The orchestrator ratified two keys as API-configurable —
`drift_capture_merge_threshold` (v078) and
`ready_aging_threshold_hours` (v079) — and its own `config-manifest`
publishes neither. That is an orchestrator spec→impl gap, not a console
defect, and its consumers cannot honour a declaration the producer does
not publish. Reported to the homelab `steady-state-loop-hardening` thread
for relay, since that thread already carries the per-consumer
consumption surface.

## Next

Record the scope event promoting the capture-integrity repair to a
requirement carrier and re-recording leg 6 row 2 against its narrowed
key, then file the children under the plan epic.
