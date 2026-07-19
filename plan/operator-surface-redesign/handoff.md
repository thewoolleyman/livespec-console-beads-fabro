# Operator-surface redesign — state-valid verbs, truthful detail panes, driver handoff

**Epic anchor:** `livespec-console-beads-fabro-6msemd`

**Supersedes:** `plan/archive/impl-dispatch/handoff.md` (split 2026-07-19).

## THIS IS A DESIGN THREAD, NOT A DELIVERY THREAD

It produces a **brainstorm, research, and a spec-amendment set**. It files NO
implementation work-items up front. Impl items are DERIVED from spec gaps via
`capture-impl-gaps` AFTER the propose-changes ratify.

That ordering is not style. Epic `-0ak` and its seven children were CLOSED as "wrong
vehicle" for filing impl work-items up front on spec-driven work. The parent thread
being superseded here repeated the same mistake one level up: it held three
SPEC-CHANGE-TIER problem statements inside a dispatch-queue frame whose ranker can
never surface `backlog` items — a queue view over things that cannot queue, which is
why it showed a permanently zero-ready queue.

## Charter

Define the operator-action contract: which verbs are valid in which lane/attention
state, how they are presented, and how heavyweight LLM-driven verbs reach a driver
session.

## Read first

1. This file.
2. `SPECIFICATION/contracts.md` — the TUI contract, the B5 pane-content rule, the
   per-item verb-suppression hint clause.
3. `SPECIFICATION/scenarios.md` — Scenario 5 (TUI-first operator workflow).
4. `crates/console-tui/src/lib.rs` — key dispatch `key_event_to_terminal_input`
   :459-531, `valve_open_input` :823-843, `override_open_input` :845-864, Enter
   drill-in :616-618, help sections :1666-1700.
5. `crates/console-application/src/lib.rs` — `selected_move_status_valve` :1275-1284
   (the ONE state-aware verb; the model to generalize), `pane_footer_hint` :1467-1503,
   `attention_snapshots` :4886-4892, `build_needs_attention_detail` :5059-5073,
   `build_attention_detail` :5363-5374, `fabro_run_id` :5496-5501.
6. `plan/archive/console-cruft-cleanup/` — precedent for audit → proposals →
   ratification gate → impl handed to the ledger.

## Status is read live, never stored here

```
/livespec-orchestrator-beads-fabro:list-work-items --json
```

## CROSS-REPO DESIGN DEPENDENCY — read before designing anything

`contracts.md`'s hint clause states that per-item verb suppression "depends on the
per-state valid-verb vocabulary, which is **owned by
`livespec-orchestrator-beads-fabro`** and not yet consumed here."

So the vocabulary is not the console's to invent. Expect an **orchestrator-side
proposal and ratification to precede the console's**. Sequencing this wrong means
designing a console surface against a vocabulary that then changes underneath it.

## The design inputs (all `backlog` problem statements — none dispatchable)

### `-zweohm` — lane items expose no state-appropriate next action

Headline grievance VERIFIED: no verb is state-filtered. `valve_open_input` gates only
on "is a work-item selected", never on `item.lane()`, so `p`/`c`/`r`
(approve/accept/reject) all fire on a backlog item where they are meaningless. `groom`
appears in ZERO production source — only two test-fixture strings.

**THIS ITEM CONTAINS TWO FALSE CLAIMS. They must be struck during grooming or a
session will rebuild shipped work:**
- "Enter is inert" — FALSE. Enter opens the work-item record drill-in
  (`console-tui/src/lib.rs:610-618`), landed as `e724b9c`, spec-ratified.
- Its key enumeration omits the LIVE `g`/`f`/`k` per-item override dials
  (`console-tui/src/lib.rs:494-508`), which share the valves' gate.

Also strike the malformed doubled-prefix ids in its body
("livespec-console-beads-fabro-livespec-console-beads-fabro-mwzrby").

Generalize from `s` (move-status), which already consults
`status_move_targets(lane)` and returns `None` when a lane has no drivable target.

### `-l4p3ce` — no paradigm for handing off to an LLM driver session

VERIFIED absent: no clipboard backend of any kind exists (zero hits for
clipboard/xclip/pbcopy/OSC 52), no tmp-file prompt mechanism.

**FALSE CLAIM to strike:** "the existing Copy-attach-command effect generalizes." The
`CopyFabroAttach`/`OpenFabroAttach` scaffold is DEAD — the actions vec is hardcoded
`Vec::new()` (`console-application/src/lib.rs:5372`, `:5070`), a test asserts it empty
(`:6291`), and the runtime effect is discarded (`console-cli/src/lib.rs:1501-1505`).
It copies `fabro attach <run-id>`, a tmux attach, not a driver invocation. Treat it as
inert scaffolding that could be mistaken for a shipped feature, not a starting point.

Carries an explicit RESEARCH TASK: survey how lazygit / k9s / tig / gitui structure
external-command handoff, including tmux-aware variants.

### `-vc7lmq` (redesign half only) — detail pane should offer only state-valid commands

The immediate defect has been SPLIT OUT of this item into its own freeform bug,
**`livespec-console-beads-fabro-qwjfsw`**, and must not wait on this design. What
remains here is the valid-commands detail-pane contract, which is spec-governed.

Refresh its stale anchors during grooming: `fabro_run_id` is now :5496 (not :5290),
`build_attention_detail` :5363 (not :5157), `build_needs_attention_detail` :5059.

### `-ipi` — migrate the attention render path to the `attention_item.*` stream

Currently `WorkItemSnapshot(Observed)` drives rendering. The migration is explicitly
blocked on reconciling with ratified Scenario 5, so a propose-change MUST precede the
code — that is why this item is in a design thread rather than a delivery one.

The `attention_item.*` stream carries `handoff.command`, which is precisely the
truthful replacement for the fabricated attach command — this migration and the
detail-pane contract are one subject.

Cross-tenant bookkeeping: prose-associated with core-tenant epic `livespec-yes5`.
Closing `-ipi` silently strands that epic's bookkeeping — report back on close.

## The groomed form for this thread (deliberate)

Epic anchor + the four `backlog` problem statements above. **No impl items until
ratification.** A ratification-gate item with dep-linked slices behind it is filed only
once concrete proposals exist (the `console-cruft-cleanup` / `iblkzp` precedent), and
`capture-impl-gaps`-derived items come after ratification.

Do NOT file these as `blocked: needs-human` — that surfaces them in the attention inbox
as though a nameable human unblock action existed, when what they actually need is a
design conversation.

## Sequencing

1. Land the standalone defects that touch this code FIRST, before any redesign impl:
   `-6hbfq6` (help focus/scroll) and `-qwjfsw` (the split-out attach-command bug). Both
   are fully-specified factory work; the redesign is many gates away. Neither is a child
   of this thread's epic in the planning sense — they just must not be blocked by it.
2. Orchestrator-side valid-verb vocabulary ratifies → console proposals.
3. `-l4p3ce`'s design precedes `-zweohm`'s implementation — the groom verb has no
   transport without the handoff paradigm.
4. The three surfaces (`-zweohm`, `-l4p3ce`, `-vc7lmq`-redesign) share ONE spec
   conversation, not three.
5. Impl slices are strictly sequenced within one session. This thread owns the hottest
   region of `console-application/src/lib.rs` (11,464 lines, 7 items contend for it)
   and of `console-tui/src/lib.rs` — this is the B2→B3→B5→B4 situation reborn.

## Gates

1. Maintainer brainstorm participation — entry gate; nothing proceeds without it.
2. Cross-repo: orchestrator-side vocabulary proposal/ratification.
3. Independent review per proposal.
4. Maintainer ratification via `/livespec:revise` — the hard gate between design and
   any impl item existing.
5. Post-ratification, each derived slice passes the normal admission valve.

## Dispatch

Post-ratification slices go **factory-side** — the Dispatcher drains `ready`, or run
`/livespec-orchestrator-beads-fabro:drive --action impl:<id>`. A planning session FILES
ripe work; it never hand-codes the implementation inline.
