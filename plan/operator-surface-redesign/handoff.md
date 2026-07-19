# Operator-surface redesign — state-valid verbs, truthful detail panes, driver handoff

**Epic anchor:** `livespec-console-beads-fabro-6msemd`

**Supersedes:** `plan/archive/impl-dispatch/SUPERSEDED-BY.md` (split 2026-07-19), which
carries the routing table showing how these items landed here. Do NOT resume the
archived `handoff.md` beside it.

## ENTRY GATE — read before doing anything

**This thread's work cannot start without maintainer brainstorm participation.** That
is an absolute gate, not a step in a list.

The two standalone defects named under §Sequencing (`-6hbfq6`, `-qwjfsw`) are
explicitly OUTSIDE this gate — they are separate items that merely touch code this
thread will later rewrite. Landing them is not "starting this thread", and this thread
does not own them.

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
2. `SPECIFICATION/contracts.md` — the TUI contract; the panes-render-operational-
   content-only rule at `:659` (called "B5" only in the cockpit thread, never in the
   spec itself); the per-item verb-suppression hint clause at `:653`.
3. `SPECIFICATION/scenarios.md` — Scenario 5 (TUI-first operator workflow); Scenario 11
   (`:354`, the reject/regroom path).
4. `crates/console-cli/src/lib.rs` — the discarded attach effects at `:1501-1505`, cited
   below as evidence that the copy-command scaffold is inert.
5. `crates/console-tui/src/lib.rs` — key dispatch `key_event_to_terminal_input`
   :459-531, `valve_open_input` :823-843, `override_open_input` :845-864, Enter
   drill-in :616-618, help sections :1666-1700.
6. `crates/console-application/src/lib.rs` — `selected_move_status_valve` :1275-1284
   (the ONE state-aware verb; the model to generalize), `pane_footer_hint` :1467-1503,
   `attention_snapshots` :4886-4892, `build_needs_attention_detail` :5059-5073,
   `build_attention_detail` :5363-5374, `fabro_run_id` :5496-5501.
7. `plan/archive/console-cruft-cleanup/` — precedent for audit → proposals →
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

**Its body was CORRECTED 2026-07-19 and is now accurate — do not re-strike anything.**
It previously claimed "Enter is inert" (false; Enter opens the work-item drill-in,
`console-tui/src/lib.rs:616-618`, landed as `e724b9c` and spec-ratified) and omitted the
live `g`/`f`/`k` override dials (`console-tui/src/lib.rs:494-508`). Both are fixed in
the item, along with some malformed doubled-prefix ids. The correction is recorded as a
comment there. Read the item as it stands.

Generalize from `s` (move-status), which already consults
`status_move_targets(lane)` and returns `None` when a lane has no drivable target.

**Do NOT read "groom is absent" as "no code exists."** The item itself carries the
nuance: `RejectMode::Regroom` (`console-application/src/lib.rs:3378-3394`) is live
production code reachable via `r`, emitting `reject:<id>:regroom` (Scenario 11,
`scenarios.md:354`). It is a REJECT mode — semantically the opposite of grooming a
backlog item — so it is not the transport this thread needs, but a partial one shipped
and the design must account for it.

### `-l4p3ce` — no paradigm for handing off to an LLM driver session

VERIFIED absent: no clipboard backend of any kind exists (zero hits for
clipboard/xclip/pbcopy/OSC 52), no tmp-file prompt mechanism.

**Its body was CORRECTED 2026-07-19 and is now accurate — do not re-strike.** It
previously claimed "the existing Copy-attach-command effect generalizes." It does not:
the `CopyFabroAttach`/`OpenFabroAttach` scaffold is DEAD — the actions vec is hardcoded
`Vec::new()` (`console-application/src/lib.rs:5372`, `:5070`), a test asserts it empty
(`:6291`), and the runtime effect is discarded (`console-cli/src/lib.rs:1501-1505`).
It copies `fabro attach <run-id>`, a tmux attach, not a driver invocation. Inert
scaffolding that could be mistaken for a shipped feature, not a starting point.

**STILL UNCORRECTED, and the one body that needs editing:** `-vc7lmq` retains the same
"a starting point to generalize" framing plus its pre-drift anchors. Its correction
exists only as a comment. Fix the body when this thread next touches it.

Carries an explicit RESEARCH TASK: survey how lazygit / k9s / tig / gitui structure
external-command handoff, including tmux-aware variants.

### `-vc7lmq` (redesign half only) — detail pane should offer only state-valid commands

The immediate defect has been SPLIT OUT of this item into its own freeform bug,
**`livespec-console-beads-fabro-qwjfsw`**, and must not wait on this design. What
remains here is the valid-commands detail-pane contract, which is spec-governed.

Refresh its stale anchors during grooming: `fabro_run_id` is now :5496 (not :5290),
`build_attention_detail` :5363 (not :5157), `build_needs_attention_detail` :5059.

**Cross-thread obligation (mirrored from `plan/command-queue-semantics/`):** whoever
fixes the attach command must retire the test at `crates/console-cli/src/lib.rs:2312`
(`tui_command_projects_demo_attention_items`, which asserts the bogus
`Attach: fabro attach evt_demo_1`). Retire it AFTER PR #316 merges — same file, though
a different region, so the rebase is trivial either way. Two sibling tests in
`console-application/src/lib.rs` (:7753 and :7669-7675) pin the same behaviour and go
with it.

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

Epic anchor + the four `backlog` problem statements above, plus `-qwjfsw` — the defect
split out of `-vc7lmq`, which the epic inherited custody of rather than derived. Five
children in the ledger; four design inputs. **No impl items until ratification.** A ratification-gate item with dep-linked slices behind it is filed only
once concrete proposals exist (the `console-cruft-cleanup` / `iblkzp` precedent), and
`capture-impl-gaps`-derived items come after ratification.

Do NOT file these as `blocked: needs-human` — that surfaces them in the attention inbox
as though a nameable human unblock action existed, when what they actually need is a
design conversation.

## Sequencing

1. Land the standalone defects that touch this code FIRST, before any redesign impl:
   `-6hbfq6` (help focus/scroll) and `-qwjfsw` (the split-out attach-command bug). Both
   are fully SPECIFIED, but **neither is currently DISPATCHABLE, and each needs a
   different unblock** — read status live rather than trusting this line:
   - `-6hbfq6` sits at `pending-approval` → needs the maintainer's approve valve to
     reach `ready`.
   - `-qwjfsw` sits at `backlog` → needs admission (a status move to `ready`).

   Neither will be returned by the Dispatcher or `next` until then, because the ranker
   only surfaces `ready`. Do not run a drain, see nothing, and conclude the queue is
   broken — that misreading is exactly what made the predecessor thread look paralysed.

   Note `-qwjfsw` IS parented to this thread's epic in the ledger (it was split out of
   `-vc7lmq`, which lives here), so the epic has FIVE children: four design-input
   problem statements plus this one split-out defect. That does not contradict "no impl
   items until ratification" — `-qwjfsw` is a pre-existing defect this thread inherited
   custody of, not an impl slice derived from an unratified design.
2. Orchestrator-side valid-verb vocabulary ratifies → console proposals.
3. `-l4p3ce`'s design precedes `-zweohm`'s implementation — the groom verb has no
   transport without the handoff paradigm.
4. The three surfaces (`-zweohm`, `-l4p3ce`, `-vc7lmq`-redesign) share ONE spec
   conversation, not three.
5. Impl slices are strictly sequenced within one session. This thread owns the hottest
   region of `console-application/src/lib.rs` (11,464 lines, 7 items contend for it)
   and of `console-tui/src/lib.rs` — the cockpit program had to SEQUENCE its four TUI behaviours one worktree at a time for exactly this reason (see `plan/cockpit-ux-docs-release/handoff.md:188`).

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
