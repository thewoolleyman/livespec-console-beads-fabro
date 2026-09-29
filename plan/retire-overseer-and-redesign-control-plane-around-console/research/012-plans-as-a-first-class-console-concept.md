# 012 — Plans as a first-class console concept: how the requirement was never written, and the design question it leaves

Plan: `retire-overseer-and-redesign-control-plane-around-console`
(epic `livespec-console-beads-fabro-pzbdbo`). Research note; the reasoning
behind the 2026-09-29 reprioritisation ruling recorded on the epic. Status,
who owns what, and the next action live in the ledger, not here.

## The question this note answers

The maintainer's read on 2026-09-29: a plan is the highest level of hierarchy
in the livespec paradigm (optional, but above epic and work-item), the
original UI requirements made plan management first-class, and the shipped
TUI shows no representation of it anywhere. Where did the requirement go?

## Finding: it was never in the spec to lose

The premise that a seed requirement was diluted does not hold. Mechanically,
across all 52 spec revisions:

| revisions | occurrences of `plan`/`plans` in `SPECIFICATION/history/vNNN/*.md` |
|---|---|
| v001–v013 | 0 |
| v014 | 1 (a citation path) |
| v016–v045 | 5–7 (citation paths, plus one inbound mention) |
| v046+ | 8 (adds "the roster of plans") |

The seed (`SPECIFICATION/history/v001/contracts.md:369-383`) required eight
views — Attention, Spec, Ready, Factory, Manual, Done, Events, Repos — and
its domain vocabulary tops out at the work-item. No plan, no epic, no
plan → epic → work-item hierarchy.

The one structural change ever made to the view list was v010 (2026-06-29,
`a696125`): the four lifecycle views collapsed into `Lanes`, "the work-item
consumer", with Spec / Events / Repos kept as orthogonal non-lane views. That
fixed the console's hierarchy at **lane → work-item**, permanently, with no
tier above it.

### Where the plan tier actually came from

It was invented in this plan's own charter, not inherited from the spec:

- `research/redesign-brainstorm-and-decisions.md:85` — the demotion of the
  plan *operation*: "Plans as they exist duplicate the beads epic; the plan
  operation compensates for beads lacking a human surface, which leaves
  garbage in the ledger nobody reads. Lean into beads as the surface."
- `:196` — **D6 — Retire the plan operation; epics are plans.**
  "`metadata.plan_slug` required on every epic, dash-case, unique per tenant;
  the human-readable handle that overseer/console listings and autocomplete
  use."
- `:272` — the capability table declares the roster ("plans × session × work
  state") as `exists` — realised by `needs-attention + bd list --type epic on
  plan_slug + fabro ps`, i.e. outside the console, with raw `bd`.
- `:319` — the phase-1 deliverable states the console half in six words:
  "**Console: list plans from the ledger.**" That is the only place the
  requirement is written down, and it is a plan note, not a spec clause.

### How it reached the spec, and why that reads as satisfied

v046 (2026-09-01, `4d32060`, "ratify console-control-plane-boundary") is the
only revision that names plan management as a console obligation, in
`SPECIFICATION/spec.md:52-62`:

> Every operator-facing capability the retired `livespec-overseer` roles
> provided -- **the roster of plans and their work state**, attention triage,
> valve disposition under a stated policy, and account rotation on provider
> limits -- is realized in the console ONLY as rendering of, and
> configuration through, the orchestrator's published surfaces
> (`needs-attention`, `list-work-items`, `next`, the settings and valve
> command surface, and any surface the orchestrator ratifies later). [...]
> where the orchestrator has not published a surface for one, the console
> MUST leave that capability un-built rather than realize it itself.

Two things happen in that one sentence. The obligation is stated in three
words. And it is routed to an enumeration of orchestrator surfaces that
**omits `list-plans`** — a ratified orchestrator thin-transport surface
("List unarchived plans from the repo's `plan/` store. Required
thin-transport surface per livespec-orchestrator-beads-fabro
`SPECIFICATION/contracts.md`"). With the surface unnamed, the clause's own
escape hatch — leave it un-built — reads as met. Scenario 29
(`scenarios.md:1184-1216`) renders "roster, attention, valves" and never
binds "roster" to plans, `plan_slug`, or `list-plans`. The proposal that
produced v046 (`acca58b`) existed to unblock phase 1 of this plan, not to
specify the roster.

The required-views list (`SPECIFICATION/contracts.md:805-813`) is
needs-attention / Spec / Lanes / Events / Repos / Settings. No Plans.
`SPECIFICATION/proposed_changes/` holds nothing touching plans in the UI.

Then the console hold (`research/program-board.md:119`): "no new console
feature tracks until b1–b3 land." "Console: list plans from the ledger" sat
behind b1–b3 with the rest, and the parallel `tui-ux-improvements` track
charter (2026-09-08) does not carry it either.

## What ships today

- `TuiView` (`crates/console-application/src/lib.rs:341-353`):
  Attention, Spec, Lanes, Events, Repos, Settings. Six views, no plan
  surface.
- Plan rows in the attention list are explicitly the *un*-actionable case
  (`lib.rs:3354`, `:3461`): "a plan thread [...] selects no work-item and
  admits no per-item key, so only the globals are advertised."
- `grep -rn 'list-plans\|list_plans' crates/` → nothing. The console
  consumes `needs-attention`, `list-work-items` and `next`; no `list-plans`
  adapter exists.
- **One orphan surface exists, outside the TUI.** `livespec-console-beads-fabro
  plans <epic-id>` (`crates/console-cli/src/lib.rs:4281-4289`) prints a
  static HTML page from `project_plan_page` / `render_plan_page_html`
  (`console-application/src/lib.rs:4606-4830`): the epic, its children, and
  its handoff entries. Added by Fabro on 2026-08-16 (`23b7209`, "render
  ledger plan pages"), BEFORE the v046 clause existed. No spec revision, past
  or present, mentions "plan page", `/plans/`, or `plan_page`. It is keyed
  on epic id rather than `plan_slug` (so it does not implement the D6
  handle), it has no listing, and nothing in the TUI links to it.

So the console has a plan *page* nobody specified and can't reach, and no
plan *concept* anywhere an operator looks.

## The design question, framed for the discussion (NOT decided here)

The maintainer's first instinct is a new top-level **Plans** view. The
question that has to be answered before building it is how a plan tier
relates to the lane model, because both are groupings over the same
work-items and two overlapping partitions of one list is worse than either.

What a plan IS in the D6 world: an epic carrying `plan_slug`, its children
(work-items in various lanes), its typed `next_action`, its handoff
timeline, and its research directory. The orchestrator publishes it through
`list-plans` and the `context` loader. A plan is therefore a *vertical* slice
(one goal, items across every lane); a lane is a *horizontal* slice (one
lifecycle state, items across every plan).

Candidate shapes, with the trade-off each carries:

1. **Plans as a seventh top-level view.** A roster (`plan_slug`, goal line,
   next-action kind, child counts per lane, last-session age, stale
   warnings), drilling into one plan's children and timeline. Lanes stays
   as-is. Cost: two peer views over one item set; an operator asks "where do
   I go to see item X" and has two right answers.
2. **Plan as the primary grouping INSIDE Lanes.** Each lane groups its rows
   by plan (mirroring the mx9u.6 group rows on Attention). No new view; the
   hierarchy shows up where the items already are. Cost: no place to see one
   plan *whole* (its next action, its timeline), which is the thing the
   retired overseer roster provided.
3. **Plans as a container view with Lanes as a sub-view** (the shape
   mx9u.20 gives Events: Stored events / Event sources). Plans is the entry;
   selecting a plan scopes the lanes to it; an "all plans" pseudo-scope
   preserves today's Lanes. Cost: the biggest navigation change; plan-less
   items need a home ("unplanned").
4. **Plan as a scope filter, not a view.** A global plan selector (like the
   repo selector) that scopes Attention, Lanes and Events to one plan. Cost:
   still no roster; and the roster is the stated requirement.

Whichever shape wins, three things follow from the D6 contract and from the
control-plane clause, and are worth settling in the same discussion:

- The console **consumes `list-plans`** (and the `context` loader for the
  drill-in); it does not derive plans from `bd list --type epic` locally. If
  those surfaces lack a field the roster needs (e.g. per-lane child counts),
  that is an orchestrator gather fact to file upstream, per the
  never-work-around rule.
- `SPECIFICATION/spec.md:52-62` needs a propose-change that names
  `list-plans` in the enumeration and defines "roster"; the required-views
  list changes with the chosen shape. The spec change is part of the work,
  not an afterthought — it is precisely how the requirement got lost.
- The orphan `plans <epic-id>` page is either promoted into the chosen shape
  (re-keyed on `plan_slug`) or removed; it must not remain a third,
  unspecified answer.

## Dogfood

This plan is the dogfood: its epic carries a `plan_slug`, a typed
`next_action`, 86 handoff comments, three blocked upstream proxies, and a
research directory. If the Plans surface cannot answer "what is this plan
waiting on, and what did the last session leave me" for
`retire-overseer-and-redesign-control-plane-around-console` itself, it has
not met the requirement — the STATUS RE-PRINT a restarted session prints by
hand today is the specification of what the roster row and its drill-in
must show.
