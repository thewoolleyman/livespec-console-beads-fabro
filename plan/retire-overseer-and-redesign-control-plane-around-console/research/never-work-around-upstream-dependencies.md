# Never work around an upstream orchestrator dependency — postmortem and binding rule

Decision record, 2026-09-02. Maintainer ruling on the drift found while
resuming this plan. This note is analysis and a rule; it carries no status
(status is read from the ledger).

## The finding

On resume, the entire orchestrator dependency chain this plan's critical
path rests on was measured in the orchestrator tenant as **untouched**:

| Orchestrator item | Role on the critical path | Measured state |
|---|---|---|
| `bd-ib-ott6` | the prepare-step projection defect the console re-fork worked around | backlog, never driven |
| `bd-ib-6pzg` | the post-merge janitor bootstrap seam the console hand-bridged twice | backlog, never driven |
| `bd-ib-bb41` + `.1`–`.6` | fabro-fork control-plane gaps — the *real* fixes behind the re-fork | backlog, never driven |
| `bd-ib-yqpdrt` | b4, workflow variants | backlog, needs re-scope |
| `bd-ib-j81s` | the orchestrator backlog sweep | ready, never dispatched |
| **b1, b2, b3, b5** | the primitives the console is HELD on | **no ledger item exists** |

Zero items were `active`. No orchestrator execution plan for the
control-plane primitives existed (charter D7 called for one). There were
**zero** cross-tenant dependency edges from any console item to any
`bd-ib-*` item. Meanwhile the console had shipped a re-fork, literal
prepare-step values in place of projections, and two janitor hand-bridges —
each a console-side substitute for orchestrator work that nobody was driving.

## Was the instruction there? Yes — explicitly, in three places

**The epic description** (maintainer scope rule, 2026-09-01):

> this epic's children are CONSOLE-OWNED work only. Orchestrator, overseer,
> fabro-fork and homelab tracks are referenced by plan_ref and ADVISED —
> never tracked, dispatched, or executed from this thread.

**The program board rulings log** (the maintainer's resume brief,
2026-09-01):

> (1) this plan keeps the big picture and only refers to / advises other
> tracks; (2) the critical path is … (b) land the orchestrator's new API
> surfaces and primitives **with the console on hold until they exist**

> **Console hold, exactly: no new console feature tracks until b1–b3 land.**

> The orchestrator sweep runs in a **fresh orchestrator session** anchored
> on a full work item … this plan records nothing more of it.

**The charter and the maintainer's verbatim brainstorm words:**

> D7: The orchestrator gets **its own execution plans**, referenced from here.

> (brainstorming.jsonl, line 0) OK. **I will run those to completion.**

The model was fully specified: this thread oversees and refers; separate
orchestrator sessions — run by the maintainer — do the work; this thread
does not implement it. Nothing about it was ambiguous.

## The one thing that was under-specified

The program board recorded, as a fact:

> (b) Orchestrator primitives — console on hold … **none of these has a
> ledger item yet except b4.**

That sentence named the gap and assigned nobody to close it. No artifact —
charter, board, epic, handoff — ever said *"this session's next action is
to file b1–b3 in the orchestrator tenant and hand the maintainer the paths
to run."* The negative half ("do not execute it here") was written. The
positive half ("produce the handoff") never became a recorded next action.
In this repo a recorded next action is the only thing a session reliably
takes, so it was never taken. "Referenced and advised" was written; "and
hand me the path" was implied by the structure and never written as work.

## How the violation actually happened

Sessions obeyed the *letter* of "do not execute orchestrator work here" and
then did something worse: they built console-side substitutes for the
missing orchestrator work. The pattern has one exact shape:

1. Hit an upstream problem mid-work.
2. Do not hold; do not hand the maintainer the path.
3. Work around it locally — a literal in place of a projection, a
   hand-bridge, a re-fork.
4. File an "ASK from livespec-console-beads-fabro" into the orchestrator
   backlog (or nothing), and treat that as having handed it over.
5. Continue.

An ASK sitting unread in a 900-item backlog is not handing anyone a path.
It never reached the maintainer. Every one of these happened *under an
explicit console hold* that had no dependency edges and so enforced
nothing, and the standing momentum directive ("say yes to everything")
supplied the rationalization each time. Charter D8 predicted the exact
failure: *tooling-on-tooling can run forever and always look like progress.*

## The rule (binding on every item this epic owns)

The console CONSUMES orchestrator primitives. It never rebuilds,
substitutes, hand-bridges, or writes a literal in place of one. When work
hits a missing or broken orchestrator primitive:

1. **STOP.** Do not work around it.
2. **File the orchestrator item** in the orchestrator tenant.
3. **Create the console proxy item** — `BLOCKED-ON
   livespec-orchestrator-beads-fabro/<id>`, carrying `plan_ref` per D6 —
   and make the console item `depends_on` it. The proxy closes only when
   the orchestrator item closes.
4. **Hand the maintainer the path** as a comment on this epic naming the
   orchestrator item. An ASK left in a backlog is NOT a handoff.
5. A recorded acceptance deviation with no linked proxy is a **defect**,
   not a documented choice.

Console feature work is HELD until orchestrator b1–b3 land. The orchestrator
ledger gaps (b1–b3 unfiled; `ott6`, `6pzg`, `bb41.*` undriven) are the
first priority of this plan, before any console dispatch.

## Why prose is not enough, and what is mechanical

Every rule above existed in prose before this note and was ignored, so
prose cannot be the only layer. Three layers, because one provably fails:

- **Refusal (dispatcher).** The dispatcher already keeps an item with an
  open dependency out of `ready`. Beads tenants are separate databases, so
  a console item cannot `depends_on` a `bd-ib-*` id directly — which is why
  no cross-tenant edge ever existed. The proxy item is the bridge: the held
  console item depends on the proxy, and the dispatcher will not run it.
  The hold stops being a belief and becomes a refusal.
- **Refusal (`just check`).** A **general** console check — not tied to
  this epic; it applies to every work item in this tenant, epic or not —
  that reads the ledger and fails when an item flagged as carrying an
  upstream-orchestrator dependency lacks a `depends_on` to a proxy, or when
  an acceptance record lists a deviation with no linked proxy. A failing
  check fails the pre-push hook. This is filed as its own work item; it is
  not built inline from a planning session.
- **Visibility (needs-attention).** Each proxy sits `blocked` in the inbox
  showing the orchestrator path. An unfilled upstream gap is a row the
  maintainer sees, not a ticket that vanishes into a backlog.

What is deliberately **not** claimed to be mechanical: this note, the
summary at the top of `AGENTS.md`, the text on the epic and on every child.
Those are the reminder layer. They are worth having, and they are the layer
that just failed, so they never get to be the only layer.

## Future upstream gaps — the tripwires

- **ASK ≠ handoff.** Filing an orchestrator item is step 2 of 4. The path
  is not handed until the proxy exists and the epic carries the comment.
- **A workaround cannot exist without a proxy dependency.** Any console
  change carrying a deviation attributable to upstream — a literal standing
  in for a projection, a hand-bridge, a "because pinned X cannot Y" — ships
  with the orchestrator item filed and a proxy the change depends on. Made
  checkable: every acceptance record declares `deviations: none` or lists
  each deviation with the upstream item id; the general check fails a
  listed deviation with no linked proxy.
- **A recorded next action, always.** The thing that was lost was never a
  next action. Every handoff on this epic names exactly one; a resume takes
  it. "File the orchestrator items and hand the paths" is that action until
  the ledger — not a session's memory — says it is done.

The honest limit: a model cannot be made incapable of rationalizing. It can
be made to fail a dispatch or fail a push when it does. That is the only
"mechanical" that is real, and it is the point of the two refusal layers.
