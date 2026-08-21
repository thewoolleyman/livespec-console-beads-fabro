# The per-item launcher argv already exists — measured 2026-08-20, while the console was held for plan 04's drain

Banked during the R10 walk's forced wait, so the window bought something. This
is the research half of `-et3.10`; it is NOT the spec change, and it does not
close the item.

## What `-et3.10` says is missing

`DispatcherFactoryDispatchItemPort::dispatch_item` returns `not_wired()`
unconditionally, and its own doc comment gives the reason:

> The current specification confirms the `factory.dispatch_item_requested`
> command contract but does not define a stable concrete Dispatcher argv for
> one-item dispatch.

That is accurate about the SPEC. `SPECIFICATION/contracts.md:584` carries
"Factory-drain launcher argv" and there is no per-item analogue; the command
vocabulary at `:412` lists `factory.dispatch_item_requested` with no launcher
obligation attached.

## What is actually true of the ORCHESTRATOR — and this is the finding

The one-item launcher argv is not missing, undesigned, or waiting on a decision.
It exists, it is public, and it is in live fleet use RIGHT NOW:

    dispatcher.py dispatch --repo <path> --item <id> [common flags]

- Declared in the Dispatcher's own usage banner (`commands/dispatcher.py:28`).
- `--item` is `required=True` on the `dispatch` subparser (`:328`) — the
  subcommand cannot even be invoked without one, so there is no ambiguity about
  what "one item" means.
- The module docstring states the split outright (`:111-113`): "There are NO run
  modes: `dispatch --item` drives one ... while `loop` with no `--item` drains
  the ranked ready [queue]."
- Observed running against a sibling repo while this note was written:
  `dispatcher.py dispatch --repo /data/projects/livespec-orchestrator-beads-fabro --item bd-ib-veid`.

So the console is honest but under-informed: it declines to fabricate an argv it
was never told, while the argv sits in the CLI it already shells out to.

## The seam is already the right shape — no port redesign is implied

`DispatcherFactoryDrainPort` (lib.rs:2394) takes a `program` plus base `args`
and appends the per-run arguments at call time; `console-cli/src/main.rs:147`
supplies `&["loop", "--repo", <repo>]` and the port appends
`--budget`/`--parallel`. The per-item analogue is the same construction with a
different subcommand and one appended flag:

    program: <same dispatcher program>
    base:    ["dispatch", "--repo", <repo>]
    append:  ["--item", <work-item-id>]

This matters for scoping the spec change: it is a NEW CONTRACT CLAUSE, not new
architecture. Nothing about the port trait, the command vocabulary, or the event
vocabulary has to move.

## What the spec change must actually say, and the one real decision in it

A per-item analogue of the "Factory-drain launcher argv" section, carrying:

1. The concrete argv: `dispatch --repo <path> --item <id>`.
2. The same no-policy-arming obligation the drain clause carries. Dispatch-time
   policy is not armed per run — the Dispatcher reads `dispatcher.*` settings
   itself — so the per-item path MUST pass no policy-arming argument either.
   The reason generalizes unchanged: the argument parser recognizes none, and
   passing one fails the run.
3. **The one genuine decision, which is why this is a `propose-change` and not
   an implementation ticket:** `dispatch` and `loop` are different subcommands
   with different semantics. `loop --item <id>` (`:334`, `action="append"`)
   ALSO exists and constrains a ranked drain to a named set. So the spec must
   say WHICH the console's per-item verb means: a direct one-item drive
   (`dispatch`), or a drain narrowed to one item (`loop --item`). They differ in
   admission and ranking behaviour, and picking silently in the adapter would
   re-commit exactly the error `-et3.10` was filed for.

My recommendation, to be argued in the proposal rather than assumed here:
`dispatch`, because the operator's mental model of "dispatch THIS item" is a
direct drive, and because `loop --item` inherits ranking machinery whose
observable effect on a single-element set is a second thing to specify.

## What does NOT follow from this note

- `-et3.10` is NOT unblocked by having found the argv. It is spec-change-tier
  because the console may not wire a port against an argv the spec does not
  carry; that remains true.
- The stub must NOT be quietly wired ahead of the proposal. R8's original
  acceptance permitted "an honest not_wired fallback" and the fallback shipped
  as the only path — an acceptance criterion a stub can satisfy is not an
  acceptance criterion. `-et3.10` is written so a stub fails it, and this note
  must not be used to route around that.
