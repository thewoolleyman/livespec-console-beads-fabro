# 04-mvp-unbroken-walk-and-close — opening measurement

Written 2026-08-19 when this thread was first OPENED (epic
`livespec-console-beads-fabro-9nb` had an empty timeline until this session).
Everything below is measured against master `5b5f736`, not narrated from the
charter.

## 1. Menus now EXIST and RENDER — re-measured 2026-08-20 at master `a0b380c`

This section has been re-measured twice as plan 02 moved underneath it. Numbers
are stated with the commit they were taken at, because every earlier version of
this section went stale within a day.

**Registry entry count**, and the first draft was wrong even for its own day:

| measured at | entries | top-level nodes |
| --- | --- | --- |
| first draft of this note (claimed) | 10 | 1 (`Work item`) |
| master `4748daf`, actual | 11 | 1 (`Work item`) |
| master `80d2cc6` (PR #694, chords) | 15 | 4 |
| master `a0b380c` (PR #703, rendering) | **16** | **4** — 11 `Work item`, 3 `View`, 1 `Help`, 1 `File` |

**`menu_path` now has consumers, and a menu renders.** The first draft's central
finding — declared but unconsumed — held through `80d2cc6` and is now obsolete.
Re-measured at `a0b380c`:

- `action_registry::menu_tree()` (`action_registry.rs:552`) derives the bar and
  submenus from `menu_path`.
- `crates/console-tui/src/lib.rs` consumes it (`:2117`) and renders it as
  `TuiOverlay::Menu { top, selected }` (`:1404`), with its own selection and
  navigation arms.
- Opening it is a registry action like any other: `id: "open-menu"`, label
  `Menu bar`, `hotkeys: &[KeyChord::plain('v')]`, `menu_path: &["View", "Menu bar"]`.
- Menu items stage through `menu_confirm_step` (`console-tui/src/lib.rs:492`),
  which `console-arch-check` enforces as one of exactly three permitted
  registry-staging functions (`REGISTRY_STAGING_FNS` at
  `console-arch-check/src/main.rs:994`, alongside `registry_action_input` and
  `invoker_confirm_step`). So menu invocation is the same staging path as hotkeys
  and the invoker, mechanically, not by convention.

**So milestone 1 is no longer unperformable for want of a menu.** What now gates
it is a different and narrower thing, recorded in §2b: the menu that exists is an
OVERLAY, and the maintainer has ruled menu primacy to mean a PERMANENT BAR.

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

## 2a. Milestone 1 is PROTECTED, not pre-discharged — ruled by plan 02, 2026-08-19

The scoping question §2 raises — whether the menus-only walk is one walk or two —
was put to plan 02 rather than resolved from this parked thread, and plan 02 ruled
it **two walks**:

- Plan 02's own dogfood leg and plan 03's transferred leg **collapse into one
  shared walk** performed in plan 02, because plan 03's leg is strictly contained
  in a menus-only lifecycle SEGMENT.
- **Plan 04's milestone 1 stays SEPARATE and is explicitly NOT discharged by it.**
  A full unbroken lifecycle in one sitting is a strictly stronger claim than a
  segment, and folding the two together would let a segment-sized walk be reported
  as a full-pass proof.

Recorded as **deferral D1 on `-et3`**, naming this plan's epic `-9nb` as where it
is reconsidered; the foreman accepted the ruling. A successor must not treat plan
02's shared walk as satisfying milestone acceptance 1 — the charter's "not
assembled from legs walked separately" clause is exactly what D1 preserves.

## 2b. What the rendering slice unblocked, and what it did NOT — 2026-08-20

Three maintainer rulings landed with the rendering slice and this plan must be
designed against them, not against what shipped:

- **(a) Menu primacy means a PERMANENT BAR** — a permanently-visible screen
  region with submenus on demand, acknowledged as a real slice of its own (a
  layout change to every view plus a Status-band rework). What `-5tlleh` shipped
  is an OVERLAY.
- **(b) The menu key stays `v`** for now.
- **(c) Plan 02's next slice is R4** (hotkeys-provably-additional), factory probe
  first. The permanent-bar slice is RULED but NOT YET SCHEDULED, and its
  sequencing against this plan's milestone 1 is still open.

**UNBLOCKED NOW: milestone acceptance 2's design.** Milestone 2 requires capturing
the offered-actions state BEFORE each invocation, the confirmation's exact target
read back before committing, and a ledger check after. That was undesignable while
nothing rendered, because "the offered actions" is only observable once a surface
offers them. It is designable now. Design it against the PERMANENT BAR per ruling
(a): the bar is a persistent region whose contents are visible without opening
anything, so "the offered-actions state before invocation" is a different capture
against a bar than against an overlay that must first be opened.

**NOT unblocked: milestone 1's walk, and this is a judgement this plan owns.**
The menu exists and stages correctly, so a walk is mechanically possible today
through the `v` overlay. It should still wait for the permanent bar. Ruling (a)
makes the overlay a surface that is going away, and milestone 1's whole content is
a PRIMACY claim — one unbroken pass driven by menus as the primary navigation
mechanism. A pass driven through an overlay that must be summoned by a hotkey
before each use proves primacy of the hotkey at least as much as of the menu, and
it would have to be re-walked once the bar lands. Walking it twice is precisely
what the charter's "not assembled from legs walked separately" clause and D1 exist
to prevent one level up.

This is not a maintainer question; it follows from ruling (a) plus D1. What IS
open, and belongs to plan 02 and the maintainer rather than here, is whether the
permanent-bar slice is scheduled before or after R4.

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

## 4. The archive gate this thread inherits — MEASURED 2026-08-19, superseding the first draft

An earlier draft of this section asserted, as settled fact, that
`_plan_archive_review.py` "conflates a downstream consumer's `blocks` edge with
an actual undisposed child, so plan 01 reads as having undisposed children
`-1df` and `-et3` when those are in fact its DOWNSTREAM consumers", citing
`bd-ib-r6tjhr` and a 3/3 consensus panel. **That claim does not reproduce.** It
is corrected here rather than quietly deleted, because a successor reading
`bd-ib-r6tjhr` will otherwise go hunting for a shape that is not there.

What the source actually does, read at plugin build `becb27fc76c4`.
`_plan_child_records` builds its "child" set from exactly two inclusion paths:
`_has_parent_child_edge_to_epic` (records carrying a `parent_child` edge whose
`depends_on_id` is the epic — legitimate), unioned with `_blocking_ids_for_epic`,
which finds THE EPIC'S OWN record and returns `blocking_dependency_ids(record=epic)`
— every id the epic itself depends on through a `blocks` edge. `tracks` edges
are not an input to either path. `is_blocks_dependency_edge` is a deliberate,
documented type check, so the gate IS edge-type-aware.

Because `blocking_dependency_ids` only ever reads the epic's own dependencies,
this path can surface UPSTREAM BLOCKERS only. A downstream-consumer variant is
not expressible in it. Measured live against the tenant, running the gate's own
functions rather than inferring from the ledger:

```text
undisposed_plan_child_ids(-dvv)  -> ()                    # plan 01 reads CLEAN
_blocking_ids_for_epic(-dvv)     -> []                    # -dvv has NO blocks edges
undisposed_plan_child_ids(-1df)  -> ('-dvv',)             # from -1df's own blocks edge
undisposed_plan_child_ids(-9nb)  -> ('-1df', '-et3')      # THIS plan's epic
```

So the epic that reads as having undisposed children `-1df`/`-et3` is `-9nb` —
plan 04, this one — which is the epic that carries `blocks` edges to both. Plan
01 reads empty. The original report appears to have attributed `-9nb`'s edges to
`-dvv`.

**Two live consequences, both still true after the correction.**

First, plan 03's archive gate refused on 2026-08-19 with
`undisposed child work-items: livespec-console-beads-fabro-dvv`, reproduced
exactly by the run above. `-dvv` is `-1df`'s upstream blocker, reached solely
through `-1df`'s own `blocks` edge; there is no parent-child edge and no direct
child anywhere in that refusal. Whether an upstream blocker SHOULD count under a
gate whose prose reads "refuse archive if any CHILD of the plan epic is not
disposed" is a design question for the tooling owner, not a defect this repo
should assert. It is recorded here as contested and left there.

Second, and independent of that design question: **the gate does not check this
repo's plan epics' actual work-items.** `tracks` is the relation plan epics here
use for their children, and it is excluded. `-dvv` tracks six items, one of
which (`-3yx`) is open at `backlog` right now, and its undisposed set is still
empty — plan 01 could pass the mechanical leg over a live open tracked child.
Plan 03 is not exposed to this in fact (all seven of its tracked items are
closed), but that is circumstance, not the gate working. This may be the
converse-gap item `livespec-dev-tooling-q3emww`; if so, the reproduction above
is a concrete one.

Disposition: `bd-ib-r6tjhr` remains filed against
livespec-orchestrator-beads-fabro, manual-admission, pending maintainer triage,
and the `-dvv` disposition question is escalated to the maintainer through the
foreman. Neither is dispatchable from this repo. What this thread owns is the
correction above, not the fix.

## 5. Out of scope, decided here

`livespec-console-beads-fabro-wnlcnj` (drop the redundant `check-test` CI matrix
job now that nextest is canonical) was transferred as a residual from the
archived delivery-path-speed-and-caching thread and offered to this thread. It
is CI-harness hygiene with no bearing on the MVP walk or on archive sequencing,
and it is already `ready` (Dispatcher-admitted). It stays standalone; this plan
does not adopt it.

## 6. Correction to §1, found after the first draft: plan 02 is half-built, unpushed

> **SUPERSEDED by §1's re-measurements.** Kept as the record of how the
> unpushed branch was found and why it was unpushed. That work has since landed
> on master, banked red first, as `26843ee` (born red) -> `62d6efd` (tokenizers)
> -> `80d2cc6` (chords) -> `5a8be56` (rendering). The rescue bundle it prompted
> is no longer load-bearing.

§1 stands as a statement about master. But plan 02 is not the blank charter its
handoff makes it look like, and the thread that resumes this arc must not
re-derive what already exists.

Auditing this repo's ~40 stale worktrees turned up branch
`feat/menu-completeness-gate` (worktree `slice-b`, commit `8bc1e3e`,
2026-08-03), **committed local-only and never pushed**, verified absent from
master. It carries `crates/console-cli/tests/menu_completeness.rs` (199 lines),
`tests/fixtures/menu-completeness-carveout.json`, and
`plan/02-menu-shell-primacy/research/completeness-gate-born-red.md`.

That research note banks the gate's RED output per the standing born-red rule
(`cargo test --test menu_completeness`, RC=101 read unpiped):

```text
Operator-reachable behaviours with NO menu path and NO argued exclusion:
  - KeyCode::Char('c')
  - KeyCode::Char('/')
  - KeyCode::Char(':')
  - KeyCode::Char('?')
  - KeyCode::Char('q')
  - KeyCode::F
  - KeyCode::Media
  - KeyCode::Modifier
Registry currently holds 11 entries.
```

Two things follow, both load-bearing for this arc:

1. **Plan 02's "do not pick this silently" scoping question was already ruled**,
   on 2026-08-03, in favour of *every operator-reachable BEHAVIOUR* rather than
   *every REGISTERED action* — and the ruling is implemented, not merely
   recorded. The five real names above (`Ctrl-C` quit, `/` search, `:` palette,
   `?` help, `q` quit) are precisely the ones a registry-row-quantified gate
   could never have produced, because it would quantify over its own input and
   pass. That is the verifier-vs-tautology distinction measured rather than
   argued. `F` / `Media` / `Modifier` are called out in the note as a PARSER
   DEFECT and explicitly NOT to be papered over with carve-out entries:
   `handled_key_arms` keeps `Char(...)` whole while `inert_arms` keeps the
   parens, so the two scanners tokenize differently and need one shared
   tokenizer.
2. **The remaining work is enumerated and ordered** in that note: unify the
   tokenizers; register the five keys with `menu_path`s — which introduces the
   first top-level nodes beyond `Work item` and therefore the first real menu
   BAR, on the already-approved ">= 2 top-level nodes" design basis, with an
   open sub-question on whether the `Ctrl-C` chord is expressible at all given
   `hotkey: Option<char>` cannot carry a modifier; re-run green; then
   mutation-demonstrate by deleting one carve-out entry and one registration;
   and only then push, as one PR carrying the banked red.

It was never pushed for a legitimate reason rather than neglect: the gate is red
by design, pre-push runs the full suite and correctly refuses it, and
`--no-verify` is never an option here.

None of this changes §1's conclusion — menus still do not exist on master, so
milestone 1 remains unperformable. What it changes is the price of fixing that:
plan 02 opens onto banked red evidence plus a four-step plan. It also makes
landing that branch worthwhile on its own merits, since as of this note the work
survives only as an unpushed commit in one worktree on one machine.
