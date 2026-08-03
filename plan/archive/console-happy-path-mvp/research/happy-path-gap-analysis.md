# Happy-path gap analysis — what the TUI can and cannot drive today

Verified live 2026-07-20 against master (post-`185426b`, the work-item
record modal) by driving the real TUI in a tmux pane, plus code reading.
The happy path under analysis (this thread's MVP scope):

> An **existing filed backlog work-item** is taken — entirely via the TUI —
> through **groom → slices admitted (approve) → ready → dispatched →
> active → acceptance → accept → done**. Impl-side lanes only; the walked
> item is implementation work, not spec work. Autonomous mode, spec-side
> lifecycle actions (propose-change etc.), and multi-repo coverage are out
> of scope for the MVP.

## Leg-by-leg status

| Happy-path leg | TUI surface today | Status |
|---|---|---|
| Find the item | Lanes view → `enter drill` into backlog | ✅ works |
| Read the item | `enter item` → full-record modal (all fields, honest "—") | ✅ works (Lanes path); ❌ NOT reachable from Attention (`-276inb`); ❌ PgUp/PgDn skips content in short panes (`-7rcps4`); ❌ lane rows show no title, so triage requires opening items one by one (new gap) |
| Groom it | — | ❌ **absent entirely**: `groom` appears in zero production source; no verb, no hint, no help entry (`-zweohm`), and no LLM-driver handoff paradigm to run it through (`-l4p3ce`) |
| Approve slices | `p` valve on pending-approval (Scenario 11 command mapping) | ✅ works |
| Dispatch | `:` command palette → drain command ("type a drain command", `console-application` hint; drains the ready queue via the Dispatcher port) | ✅ exists (queue-level drain; per-item dispatch is `-8aw`, parked — NOT needed for MVP) |
| Monitor active | Attention rows + Detail pane | ⚠️ works for real `fabro:*` runs; ❌ fabricates a never-working `fabro attach <event-id>` for orchestrator-sourced items (`-qwjfsw`, split from `-vc7lmq`) |
| Acceptance | Attention "Acceptance review" row → `c` accept / `r` reject | ✅ works |
| Verb truthfulness throughout | Hints are selection-aware since `185426b` but NOT state-aware: `p/c/r` advertised on backlog items where they are meaningless | ❌ (`-zweohm` / `-vc7lmq` design territory) |

**The chasm is the groom leg** — everything else exists, with truthfulness
and reachability defects around the edges.

## Binding constraints on any fix (locked core contract)

From `plan/archive/work-item-lifecycle-redesign/research/locked-core-contract.md`
and `plan/archive/work-item-lifecycle-redesign/research/decision-log.md` —
these bound every slice this thread files:

- **Zero Beads knowledge** — the console's only external interface is the
  orchestrator CLI; every lifecycle transition the operator "drives" is a
  command to the orchestrator's published surface, never a local state write.
- **Lane is consumed, never re-derived** (`lane_of` is the single authority).
- **Attention is a pure derivation** — no console-local dismissal state.
- **No console→driver dependency** — an LLM-handoff surface may EMIT a
  driver command for the human to run; it must not link the console to a
  driver harness. (This is why `-l4p3ce`'s copy-paste/tmp-file MVP is the
  right altitude and an in-app embedded LLM is not.)
- **Acceptance is post-merge** — the accept valve commands the orchestrator;
  it does not gate merges.
- The `next` ranker only surfaces stored-`ready` items; the admission valve
  and dependency edges are the only routes there. The happy path must ride
  those, not bypass them.

## Custody map (whose thread owns what)

| Piece | Owner | This thread's relationship |
|---|---|---|
| State-valid verb vocabulary, groom exposure design, LLM-handoff design (`-zweohm`, `-l4p3ce`, `-vc7lmq`, `-ipi`) | `plan/operator-surface-redesign/` (epic `-6msemd`) — design-only, maintainer-brainstorm-gated | Consumer & forcing function: this thread scopes the MINIMAL subset the happy path needs and drives that brainstorm to happen; ratified design → impl slices |
| Attach-command defect (`-qwjfsw`) | `-6msemd` custody (inherited from the split) | On this thread's critical path (monitor-leg truthfulness); needs only admission, no design |
| Modal paging (`-7rcps4`), Attention record-modal reachability (`-276inb`) | Freestanding bugs (filed from exploratory test 2026-07-19/20) | Stage-0: admit + dispatch now |
| Command-queue exactly-once (`-ipwtll`, epic `-irdwyb`) | `plan/command-queue-semantics/` | Parallel hardening; NOT on the single-operator MVP critical path |
| Help-overlay navigation (`-6hbfq6`) | Freestanding | Nice-to-have; not on the happy path |
| B7 walkthrough doc + Stage-2 acceptance | ~~`plan/cockpit-ux-docs-release/`~~ — **DELIVERED and ARCHIVED 2026-07-21** (`plan/archive/cockpit-ux-docs-release/`) | B7 shipped `docs/lifecycle-walkthrough.md` with its two-repo tmux acceptance; nothing to coordinate with. Its **Stage-2 was STRUCK as dead** (autonomous-mode MVP acceptance; that mode is retired for good), so there is no successor to defer to. This thread's Stage-3 real-stack pass is now the only remaining validation. Doc custody transferred HERE — see the handoff's § "Doc custody" |

## Existing-surface inventory (evidence)

- Record modal (Lanes path): shipped in `185426b` + follow-ups (`14499d5`,
  `cb32eaf`); verified live — full field set incl. `acceptance_criteria`,
  `notes`, policy-assumption annotations, clamped scrolling, honest hints.
- Valves + policy edits + move: Scenario 10/11 command vocabulary, mapped
  1:1 onto orchestrator `drive` action-ids; live keys `p/c/r`, `m/n`, `s`.
- Palette drain: `TuiOverlay::CommandPalette`, hint "type a drain command".
- Groom: zero production occurrences (only test-fixture strings) — the
  operator-surface-redesign handoff's own finding, re-confirmed.
- Valve→shipped walkthrough: `docs/lifecycle-walkthrough.md` (B7, landed
  2026-07-20 — hours after this thread opened) documents Steps 1–8 from
  the approve valve to done, with a hermetic stateful tmux fixture
  (`crates/console-cli/tests/support/lifecycle.rs`) that mirrors the drive
  grammar. It begins where the groom chasm ends; its fixture is reusable
  for this thread's Stage-3 E2E of the upstream legs.
