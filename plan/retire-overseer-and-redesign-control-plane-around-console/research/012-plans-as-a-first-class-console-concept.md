# 012 — Plans as a first-class console concept: where the requirement went, what a plan is, and the design question it leaves

Plan: `retire-overseer-and-redesign-control-plane-around-console`
(epic `livespec-console-beads-fabro-pzbdbo`). Research note; the reasoning
behind the 2026-09-29 reprioritisation and the one-term ruling recorded on
the epic. Status, ownership and the next action live in the ledger, not
here.

## The question this note answers

The maintainer's read on 2026-09-29: a plan is the highest level of hierarchy
in the livespec paradigm (optional, but above the work-item), the original UI
requirements made plan management first-class, and the shipped TUI shows no
representation of it anywhere. Where did the requirement go — and what,
precisely, is a plan today?

## Finding 1: the console spec never carried the requirement

The premise that a seed requirement was diluted does not hold. Across all 52
console spec revisions:

| revisions | occurrences of `plan`/`plans` in `SPECIFICATION/history/vNNN/*.md` |
|---|---|
| v001–v013 | 0 |
| v014 | 1 (a citation path) |
| v016–v045 | 5–7 (citation paths, plus one inbound mention) |
| v046+ | 8 (adds "the roster of plans") |

The seed (`SPECIFICATION/history/v001/contracts.md:369-383`) required eight
views — Attention, Spec, Ready, Factory, Manual, Done, Events, Repos — and
its vocabulary tops out at the work-item. v010 (2026-06-29, `a696125`)
collapsed the four lifecycle views into `Lanes` with no tier above, fixing
the console's hierarchy at **lane → work-item**.

The obligation reached the console spec once, in v046 (2026-09-01,
`4d32060`, `SPECIFICATION/spec.md:52-62`):

> Every operator-facing capability the retired `livespec-overseer` roles
> provided -- **the roster of plans and their work state**, attention
> triage, valve disposition under a stated policy, and account rotation on
> provider limits -- is realized in the console ONLY as rendering of, and
> configuration through, the orchestrator's published surfaces
> (`needs-attention`, `list-work-items`, `next`, the settings and valve
> command surface, and any surface the orchestrator ratifies later). [...]
> where the orchestrator has not published a surface for one, the console
> MUST leave that capability un-built rather than realize it itself.

Three words, undefined. Scenario 29 (`scenarios.md:1184-1216`) renders
"roster, attention, valves" and never binds "roster" to a plan, a slug, or a
surface. The proposal that produced v046 existed to unblock phase 1 of this
plan, not to specify the roster; the b1–b3 console hold
(`research/program-board.md:119`) then parked it with everything else.

The stronger textual requirement sits on the OTHER side of the boundary, in
the orchestrator's spec (v095, `contracts.md` §"Plan identity"): the plan
slug is "the human-readable handle that listings, tooling, and **the
Control-Plane surface** anchor to." The console is that surface. An
obligation written in the producer's spec and never mirrored in the
consumer's is how it stayed un-built.

## Finding 2: what a plan is — the dated record

Read in order, the fleet's own commits settle the model and expose the
vocabulary problem.

| date | event |
|---|---|
| 2026-05-26 | `epic` enters livespec **core** as "coordinating epic" (`d0351f9b`), before beads existed in the fleet — ordinary tracker idiom, on a GitHub-issues substrate. |
| 2026-06-08 | beads-fabro orchestrator spec authored (`54b60e03`); `epic` is one of beads' five native `issue_type`s from day one. |
| 2026-06-25 | **`plan` is born**, the same day in two repos: "Planning Lane" in core (`54f8763b`) and "plan as 6th heavyweight skill" in the orchestrator (`ee33f9d4`). |
| 2026-06-28/29 | first `plan/` directories in core, git-jsonl and beads-fabro — each opened *with an anchor epic* ("open work-item-state-machine L1a thread + anchor epic bd-ib-vvrxcb", `81bc8b52`). |
| 2026-08-09 | orchestrator v059 (`6fae8205`): first thinning — "handoff entries are comments on the plan epic, and the ledger's comment/timeline read path is the authoritative resume source — not git." |
| 2026-08-31 | this plan's charter (`2c11dd0`). `redesign-brainstorm-and-decisions.md:85` is the maintainer's **verbatim framing** (§2, "verbatim intent, not paraphrase"): plans-as-they-were duplicated the epic and left mutable state in git nobody read. **D6** (§4, `:196`) is the decision written from it; its *title* says "retire the plan operation; epics are plans", its *body* (points 3–6) keeps `plan/` directories as write-once research plus one `associated_work_item_id` anchor, moves handoffs to typed epic metadata, and adds doctor checks for bidirectional consistency. |
| 2026-09-04 | orchestrator **v095** (`f65c4a0e`, decision *modify*, ratification review NO BLOCKERS) ratifies D6: "An epic IS a plan: the slug is the human-readable handle [...] whether or not a `plan/<slug>/` directory exists." "A plan is a first-class directory `plan/<slug>/` anchored by a ledger epic. The plan store MUST contain only write-once research inputs [...] and exactly one write-once metadata anchor." Mutable planning-state documents in git are forbidden for plans created after ratification. The `plan` front-end was NOT retired (`contracts.md:1681`); it was thinned. |
| 2026-09-04 | v097 (`7e8d91ac`): the `context` read primitive and `discuss-work-item` — the drill-in. |

So the model in force is: **a plan is the ledger record** — realized in
this substrate as a beads `epic` carrying `plan_slug`, typed `next_action`
and `last_session` metadata, its timeline and scope events as comments, and
its children via parent-child edges — with a `plan/<slug>/research/`
directory as an *optional, write-once attribute* linked by the anchor file.
An earlier draft of this note quoted the line-85 framing and D6's title as
"the demotion of plans" and skipped D6's body; that misread the record and
is corrected here.

## Finding 3: two words for one thing, and the ruling

`plan` was never older than `epic` and never independent of it: it was
designed from birth as a wrapper around an epic (git research dir + ledger
epic + an operation), and v095 collapsed the wrapper into the epic without
retiring either word. Every epic MUST carry `plan_slug` (v095), so the two
sets coincide exactly; one word suffices.

`epic` is not literally fixed — beads supports `types.custom`
(`beads/docs/CONFIG.md:396`) — but the behavior D6 relies on (`bd epic
close-eligible`, `bd epic status`, the "N/N complete" child-disposition
gate) and beads-web's rendering are keyed to that type. Fixed in practice.

**Maintainer ruling, 2026-09-29 (scope event on epic pzbdbo): "plan" is the
domain word everywhere a human reads — console UI, spec prose, skill names,
metadata. "epic" survives only at the beads realization boundary — the
orchestrator's storage clauses and bd's own CLI — the way "row" describes
storage.** Grounds: `plan` is the substrate-independent term (it names the
store, the slug, the lane, the skill, the enumerator; the git-jsonl sibling
has no epics at all), while `epic` is a storage type with tracker baggage,
and binding the domain vocabulary to a substrate detail is what livespec's
own architecture forbids. The console never renders the word "epic". The
console spec's propose-change defines the term once — *a plan is the ledger
record the orchestrator realizes as a beads `epic` carrying `plan_slug`* —
and says "plan" thereafter.

## What ships today

- `TuiView` (`crates/console-application/src/lib.rs:341-353`): Attention,
  Spec, Lanes, Events, Repos, Settings. No plan surface.
- Plan rows on Attention are the explicitly *un*-actionable case
  (`lib.rs:3354`, `:3461`): "a plan thread [...] selects no work-item and
  admits no per-item key."
- The console consumes `needs-attention`, `list-work-items` and `next`
  (`source_adapters.rs`). It consumes neither `list-plans` nor `context`.
- **One orphan surface, outside the TUI.** `livespec-console-beads-fabro
  plans <epic-id>` (`console-cli/src/lib.rs:4281-4289`) prints a static HTML
  page from `project_plan_page` / `render_plan_page_html`
  (`console-application/src/lib.rs:4606-4830`): the record, its children,
  its handoff entries. Added by Fabro 2026-08-16 (`23b7209`), before the
  v046 clause; no spec revision mentions it; it has no listing and nothing
  in the TUI links to it. Keying it on the record's id is not wrong under
  v095 — the id IS the plan's identity — but the surface is unspecified and
  unreachable, and its CLI name says "plans" while its parameter says
  "epic-id": the vocabulary problem in miniature.

## What the roster needs, and where each field already is

Under the ruled model the surface question simplifies. The **roster** is the
set of open plans: the epics `list-work-items` already returns, each with
`plan_slug`, `next_action {kind, ref, text}`, `last_session`, rank, and
children by parent-child edge — every field a roster row needs is in a
surface the console consumes today. `list-plans` (`contracts.md:516`) is a
pure directory enumerator that "MUST NOT consult the ledger"; it supplies
one *attribute* per row — whether a research directory exists — not the
roster. The **drill-in** ("what is this plan waiting on, what did the last
session leave me") is the `context` envelope (v097): record, comments,
children, dependency edges, next action, research directory, cited spec
clauses. The console does not consume it, and that is the one genuine
consumption gap. An earlier draft of this note claimed the omission of
`list-plans` from the v046 enumeration made the roster un-buildable; that
overstated it.

## The design question, and why the plan-less finding decided it

A plan is a *parent*; a lane is a *status*; every leaf work-item has both. A
plan is therefore a vertical slice (one goal, items in every lane) and a
lane a horizontal one (one lifecycle state, items from every plan). Two
partitions of one item set must compose, not compete. Candidate shapes:

1. **Plans as a seventh top-level view.** A roster with drill-in; Lanes
   unchanged. Cost: two peer views over one item set — "where do I go to see
   item X" has two right answers.
2. **Plan as the primary grouping inside Lanes** (mirroring the mx9u.6 group
   rows on Attention). No new view. Cost: no place to see one plan *whole*
   — its next action and timeline — which is what the retired overseer
   roster provided.
3. **Plans as a container view with Lanes as its sub-view** (the shape
   mx9u.20 gives Events). Plans is the entry; selecting a plan scopes the
   lanes to it; an "All plans" scope preserves today's Lanes. The drill-in is
   the `context` envelope rendered — which is what `discuss-work-item`
   computes today. Cost: the largest navigation change.
4. **Plan as a global scope filter**, like the repo selector. Cost: no
   roster, and the roster is the stated requirement.

A leaf item without a parent plan is the ORDINARY case, not a finding. An
earlier version of this note said the opposite — "under v095 a leaf item
without a parent plan is a hygiene finding" — and the 2026-09-29 02:42Z
handoff carried it into the shape 3 description as an "Unplanned" scope.
That was a misread, and it steered a recommendation. The v095 sentence it
leaned on ("a work-item that is not an epic MUST NOT carry `plan_slug`; its
plan is its parent chain", orchestrator `SPECIFICATION/contracts.md:1723-1726`,
commit `f65c4a0e`) forbids duplicating the slug onto children; it does not
require a parent to exist. The only doctor rule it arms is
`plan_slug_on_non_epic`, whose own scenario (`scenarios.md:2854`) accepts a
standalone bug, and no ratified hygiene fact (capacity, capacity-hold,
idle-factory, merge-hold, model-fallback, ready-aging, unrunnable-acceptance)
flags a parentless item. Measured in this tenant on 2026-09-29, unioning
the `parent` edge, the dotted-id form and `parent-child` dependencies: 18 of
31 open leaf items have no parent plan, both ready bugs among them. A plan is
an optional tier above the work-item. A Plans surface must not route the
majority of the work through a pseudo-plan, and nothing renders "no plan" as
a defect.

With plan-less items as the majority, the shapes re-sort. Shape 3 makes the
plan tier the door to every lane visit, so the most common path passes
through a tier most items do not belong to; pinning an "All plans" row
first removes the pseudo-plan but not the extra step. Shapes 2 and 4 never
produce a roster. Shape 1 fits the model — Lanes stays the complete home of
every item, Plans is a view over the optional tier — and its "two peer views"
cost was overstated, because Plans is a view over a subset. The decision,
recorded as a scope event on the epic on 2026-09-29, is shape 1 plus one
cross-link: Enter on a roster row opens Lanes scoped to that plan, Escape
clears it, and Lanes gains a plan column that is blank for plan-less items.
It is a strict subset of shape 3 and can grow into it if dogfood shows the
plan tier should be the entry point; the reverse would be a rewrite. That
reversibility is the deciding property for a recommendation made right after
a premise was found wrong.

Whichever shape wins, three things follow and belong in the same decision:

- The console **consumes** `list-work-items` for the roster and `context`
  for the drill-in, plus `list-plans` for the research-dir attribute; it
  never derives plans locally. A field none of them publish is an
  orchestrator gather fact: file upstream, proxy, `depends_on`, name the
  path on the epic (the never-work-around rule).
- The console spec propose-change comes FIRST: define "plan" per the ruling,
  name `context` and `list-plans` in the `spec.md:52-62` enumeration, define
  "roster", and amend the required-views list for the chosen shape. A
  three-word clause routed to the wrong enumeration is precisely how the
  requirement got lost.
- The orphan `plans <epic-id>` page is promoted into the chosen shape or
  removed — not left as a third, unspecified answer.

## Dogfood

This plan is the dogfood: `plan_slug`
`retire-overseer-and-redesign-control-plane-around-console`, a typed
`next_action`, 88 timeline comments, three blocked upstream proxies, and a
research directory. If the Plans surface cannot answer "what is this plan
waiting on, and what did the last session leave me" for this plan, it has
not met the requirement. The STATUS RE-PRINT a restarted session prints by
hand today — goal, landed, in flight, decisions taken, maintainer-owned,
next action — is the specification of what the roster row and its drill-in
must show.
