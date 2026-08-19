# 04-mvp-unbroken-walk-and-close — opening measurement

Written 2026-08-19 when this thread was first OPENED (epic
`livespec-console-beads-fabro-9nb` had an empty timeline until this session).
Everything below is measured against master `5b5f736`, not narrated from the
charter.

## 1. The charter's milestone requires MENUS. Menus do not exist yet.

`menu_path` is declared on every `ACTION_REGISTRY` entry
(`crates/console-application/src/action_registry.rs:119`, ten entries across
`Work item > Hand off`, `Work item > Lifecycle`, `Work item > Policy dials`,
`Work item > Factory safety`), and the registry test at `:532` asserts it is
non-empty per entry.

**Nothing consumes it.** Grepping `menu_path` across `crates/` returns only the
registry definitions and that one test. The only `Menu` token in
`crates/console-tui/src/lib.rs` is `HelpFocus::Menu` — the HELP overlay's section
focus, unrelated to action navigation. So the taxonomy field shipped (plan 01's
requirement 1) but the surface that renders it (plan 02's mission) has not.

Consequence, and this is the thread's central fact: milestone acceptance 1 —
"one unbroken pass, single item, **menus-only**, real stack, one sitting" — is
not merely blocked by policy, it is **unperformable**, because there is no menu
to perform it through.

## 2. What HAS been walked, and why it is not this milestone

On 2026-08-17 21:28–22:19 UTC the plan-01 thread executed a full live
end-to-end walk through the shipped binary against this repo's live tenant, on
`livespec-console-beads-fabro-0c5`: `:actions` → ActionInvoker overlay → set
acceptance valve → `:drain` → monitor → accept. It is recorded in full as a
`plan-handoff-entry` on epic `livespec-console-beads-fabro-dvv`, timestamped
2026-08-17T22:20:40Z.

That walk satisfies plan 01's dogfood gate, and it is genuine evidence that the
registry+invoker spine works end to end. It is **not** this plan's milestone 1,
for two independent reasons:

- It was driven through the COMMAND PALETTE (`:` → `actions`), not through a
  menu. The invoker overlay is a flat registry roster, not the `menu_path`
  taxonomy; reaching it requires knowing a palette verb. "Menus-only" is a
  primacy claim about navigation, and the palette is precisely the surface
  menu-primacy is meant to displace.
- It was not clean. Three findings were captured live during it and all three
  remain relevant here: `-htp` (drain freezes the UI thread — reproduced twice
  more that session), a stale work-item snapshot that survived a full console
  relaunch and caused the drain policy to refuse a legitimate dispatch, and a
  drain command row recording `status=FAILED` for a dispatch that had in fact
  succeeded through merge (false negative for every `ai-then-human` item).

Milestone acceptance 3 says a pass needing intervention is still worth having
but is not a clean one. The 2026-08-17 walk needed interventions and was not
menus-driven; recording it here so a successor does not mistake it for this
plan's deliverable, and equally does not re-walk it from scratch believing
nothing has been proven.

## 3. Dependency state, measured

- `-dvv` (01): work COMPLETE. All children closed (`-0uw`, `-ccycuk`,
  `-ectqye`, `-koykn7`, `-w7d`) except `-3yx`, left open by explicit maintainer
  decision 2026-08-17 as a standalone coverage-tooling investigation. Epic
  itself still `backlog` — it cannot archive, see §4.
- `-et3` (02): NOT STARTED. Its handoff is still a charter. It tracks
  `-2ckgiy` (`active`). This is the real blocker on plan 04.
- `-1df` (03): work COMPLETE per the track-03 session (seven PRs: #664, #666,
  #671, #672, #673, #679, #684). Epic status still `backlog`.

So of plan 04's two ledger-edge blockers, 03 is satisfied in substance and 02 is
untouched.

## 4. The archive-gate bug this thread inherits

Plan 04 "also owns" archive sequencing for the whole 01–04 arc. That sequencing
is currently obstructed by a confirmed UPSTREAM bug, not by anything in this
repo: `_undisposed_plan_children` / `_has_blocks_edge_to_epic` in
livespec-orchestrator-beads-fabro's `_plan_archive_review.py` conflates a
downstream consumer's `blocks` edge with an actual undisposed child, so plan 01
reads as having undisposed children `-1df` and `-et3` when those are in fact its
DOWNSTREAM consumers. A 3/3 consensus panel confirmed the direction of the bug
after reading the source. Filed as `bd-ib-r6tjhr` against
livespec-orchestrator-beads-fabro, manual-admission, pending maintainer triage.
Not dispatchable from this repo.

## 5. Out of scope, decided here

`livespec-console-beads-fabro-wnlcnj` (drop the redundant `check-test` CI matrix
job now that nextest is canonical) was transferred as a residual from the
archived delivery-path-speed-and-caching thread and offered to this thread. It
is CI-harness hygiene with no bearing on the MVP walk or on archive sequencing,
and it is already `ready` (Dispatcher-admitted). It stays standalone; this plan
does not adopt it.
