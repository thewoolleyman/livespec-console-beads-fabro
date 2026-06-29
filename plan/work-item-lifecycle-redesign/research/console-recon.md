# Current console state (recon — captured from the core brief)

Recon was already done by the core session; this is the durable capture so
nothing is lost. File/line references are as of the brief and should be
re-verified at E-resolution time.

## Crate layout

- `console-domain` — events / commands.
- `console-application` — projections + source adapters.
- `console-eventstore` — SQLite.
- `console-cli` — wiring.
- `console-tui` — ratatui.
- `console-arch-check` / `console-spec-check` — guardrails.

## Finding 1 — the (only) work-item source reaches around the plane

- `crates/console-cli/src/lib.rs:359` shells **`bd ready --json`** directly.
- Parser `parse_beads_observation`
  (`crates/console-application/src/source_adapters.rs:1139`) does **not**
  JSON-parse — it substring-grabs the *first* `id`+`status`
  (`first_json_string`) and does a **3-way** `match status_text` →
  `BeadsWorkItemStatus::{Ready, Closed, NeedsRegroom}`
  (`source_adapters.rs:1143`; `"blocked"` → `NeedsRegroom`, everything else →
  `Ready`). The `Manual` variant exists but is never produced.
- `BeadsWorkItemSnapshot` (`:212`) carries `repo, work_item_id, status,
  source_version`.
- **This whole `Beads*` cluster is what core decisions 40 + 16 retire**
  (consume the emitted `lane`; zero Beads knowledge — the names are Beads
  references). → **E-1**.

## Finding 2 — the other four sources stay (separate concerns)

- `dispatcher` — journal file `tmp/dispatcher-journal.jsonl`.
- `fabro` — `fabro ps --json`.
- `livespec` — **`livespec next --json`** — this is the **SPEC-side** next
  (action ∈ `revise`/`critique`/none), **NOT a work-item source**.
- `github` — `gh pr list`.

These four are non-authoritative enrichment / spec-side and are **out of
scope** for the work-item source switch.

## Finding 3 — no "lane"/"board" concept exists at all

- Zero hits for lane/board. Closest is the `TuiView` enum
  (`crates/console-application/src/lib.rs:62`, `::all()` at `:76`) — **8 nav
  tabs**: `Attention, Spec, Ready, Factory, Manual, Done, Events, Repos`.
- Only **Attention** lists items (`render_attention`,
  `crates/console-tui/src/lib.rs:448`); the other 7 render **count
  summaries** (`view_summary_items`, `console-application/src/lib.rs:1079`).
- So a real per-lane board is **net-new**. → **E-2**.

## Finding 4 — snooze/ack are wired across 5 layers but already NOT in the inbox derivation

- `requires_attention()` (`console-application/src/lib.rs:1244`) keys off 3
  event types (`FabroHumanGateObserved | LivespecReviseRequired |
  DispatcherNeedsRegroomObserved`) and **never consults the commands table**.
- Snooze/ack plumbing: `CommandType::{AttentionAcknowledgeRequested,
  AttentionSnoozeRequested}` (`console-domain/src/lib.rs:204`),
  `OperatorAction::{Acknowledge, Snooze}`
  (`console-application/src/lib.rs:106`), action menus (`:1280`),
  `attention_command` (`:1052`), TUI render
  (`console-tui/src/lib.rs:448,903`).
- **Killing snooze/ack = deleting this plumbing, NOT unwinding a projection
  filter (there is none).** `inbox` / `dismiss` appear nowhere. → **E-3**.

## Finding 5 — zero-primary-lifecycle-state is ALMOST already true

- SQLite store (`console-eventstore/src/lib.rs`): table **`events`** =
  append-only observation cache (`payload_json` / `metadata_json`; the ledger
  is the real authority); table **`projections`** is declared but **dead**
  (never read/written outside a table-exists test); views are recomputed
  in-memory each render.
- Two residues to decide on: **`commands.status` is mutated in place**
  (`:568`, not event-sourced) and the **dead `projections` table**. → **E-4**.

## Finding 6 — no rebuild-from-ledger / projection-determinism conformance test exists

- Net-new. The only rebuild-flavored test is
  `list_console_events_rebuilds_domain_events`
  (`console-eventstore/src/lib.rs:748`, row → domain only). → **E-4**.
