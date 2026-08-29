# The gate that was not one, and what the staging actually bought

**Ledger anchor:** `livespec-console-beads-fabro-thu6gp`

This note carries the durable REASONING from the 2026-08-21 arc that finished
this thread's implementation work. It holds no status: status is composed from
the ledger, and the next action is the newest handoff comment on the plan epic.

It also SUPERSEDES the "What remains" section of
`003-what-landed-and-one-weak-premise.md`, both of whose claims are now false —
`-mcj.3` closed (PR 780), and `-mvu22t` was never the maintainer gate that note
called it. See below.

## The gate was self-imposed, and that is the finding

For roughly a month, and in eight consecutive handoff entries, this thread
recorded `-mvu22t` as "the ONLY genuine maintainer gate" and parked the epic on
it. That reading was wrong against this repo's own standing directives, which
`CLAUDE.md` reproduces verbatim: directive 4 says a decision with a sound
recommendation is DECIDED, and names groom cuts, acceptance amendments and scope
splits as included. A written recommendation had been sitting on the item since
its 2026-08-21 14:33 research comment.

Two claims had been inflating the gate, and both were measured false:

- **"Fleet-wide blast radius."** `lefthook.yml` is this repository's own config
  and `.git/hooks/commit-msg` delegates to it. The radius is PER-REPO. The
  commit-refuse hook BODY is reused from `livespec-dev-tooling`, which is
  probably where the impression came from — but a reused body is not shared
  enforcement.
- **"Exempt `docs(...)`/`chore(...)`."** This CONTRADICTS the upstream design. A
  2026-06-11 correction removed prefix-based rejection outright, and the ported
  checker states the rule positively at
  `crates/console-red-green-replay-check/src/lib.rs:3-5`: "Content is the
  trigger. Subject prefixes never exempt product Rust from the ritual."

The generalisable lesson is not "directive 4 exists". It is that **a gate can
survive on its own restatement.** Nobody re-derived the blast radius for a month
because every handoff quoted the previous handoff. The cheap test that broke it
was measuring the thing the claim asserted, once.

## What the staging bought — and it was not what the staging was for

The three-stage rollout (port wired to nothing → range mode in `just check` →
commit-msg hook) was designed to split one irreversible-feeling decision into
smaller ones. That is not what it turned out to be worth.

Stage 1 landed and, because the checker then EXISTED, it could be RUN. Running it
against a real factory commit produced:

    red-green-replay-range-missing-trailers: commits touch product Rust without
    TDD trailer shape: d9c14e3...   EXIT=1

Stage 2 as specified would therefore have reddened `just check` on every future
factory branch touching Rust — it would have stopped the factory. The root cause
was that `.fabro/.../prompts/implement.md` headed its Red-Green-Replay ritual
"REQUIRED for any product `.py` change" and exempted "changesets with no product
`.py`", which in a pure-Rust repo swallows everything.

**Neither research pass predicted this, and both were careful.** Both established
that a history-wide check would fail everywhere while a RANGE check would not.
Neither asked WHO AUTHORS THE RANGE. Establishing that a check is safely scoped
says nothing about whether the code entering that scope can satisfy it.

So the value of staging was not the smaller decisions. It was that an
intermediate stage produced a RUNNABLE ARTEFACT before the stage that would have
done harm. Prefer a staging that makes something executable early over one that
merely subdivides a judgement.

## Evidence discipline: a green result and a discriminating result

The verification gate for the prompt fix (`-mvu22t.6`) was answered with a PAIR,
not a pass:

| commit | dispatched | product `.rs` | checker |
| --- | --- | --- | --- |
| `d9c14e3` | before the fix | 3 | **EXIT=1** |
| `be5a6b7` | 52 min after | 2 | **EXIT=0** |

Green alone would have proven the checker RUNS. The pair proves it
DISCRIMINATES. Two honest limits were recorded alongside it rather than letting
"both directions closed" stand unqualified: only the SuiteGreen leg was
exercised, and it is one run.

The same shape applied to the over-triggering direction, where two post-fix
commits (`22542b7`, `89c27f9`) carry no trailers and are CORRECTLY exempt because
they stage zero product Rust. Recording those as positive self-consistency
evidence rather than as "does not qualify" matters: filed the second way, a
reader tallies them as three commits missing trailers.

## Two mechanism lessons that are now in `.ai/`, not here

Both came from mistakes made during this arc, and both live in
`.ai/factory-dispatch-and-merge-coupling.md` because they are dispatch mechanics
rather than plan reasoning:

- **A gate written as prose is not a gate.** Dispatch merges the item AND closes
  it, mechanically, without reading acceptance criteria. A gate that must survive
  a merge belongs in the dependency graph.
- **The journal filter cannot see terminal rows.** The doc's own prescribed
  filter selects on a top-level `work_item_id`; the terminal `outcome` row nests
  the id inside the outcome object. 182 outcome rows, zero with a top-level id.
  Combined with "absence of that row means the run has not finished", following
  the documentation guaranteed a false negative.

The reason they are recorded THERE and only referenced here: a plan thread
archives, and these bind every future dispatch in this repo regardless of which
thread is running.

## What the thread does not own, and left routed rather than absorbed

`-1xtg` — the ROP-railway exemption names this repo as its example while its own
criterion now excludes it, because `dev-tooling/` checks "ALL COUNT" and this
repo acquired two. That is a spec text-vs-intent defect whose route is
`/livespec:propose-change` in the `livespec` repo, not work this epic can do. It
is filed, held at `backlog` so a factory implementer cannot pick the expensive
branch by default, and cross-referenced from `-t895`.

The boundary this thread has held throughout: it asserts NAMED STRUCTURAL RULES
mechanically. A dependency declaration, a spec correction, and a railway
conversion are all adjacent and none of them are that.
