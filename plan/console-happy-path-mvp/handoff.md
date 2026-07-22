# console-happy-path-mvp — handoff

**Epic anchor:** `livespec-console-beads-fabro-b3k5hi` — status is READ from
the ledger (`list-work-items` / `next`), never stored here.
Opened 2026-07-20 (session `exploratory-test-tui`).

## Mission

Make the console usable as an MVP operator cockpit: an **existing filed
backlog work-item** is taken — every keystroke in the TUI — through

> groom (via LLM-driver handoff) → slices admitted at the approve valve →
> ready → dispatched (palette drain) → active/monitored → acceptance →
> accept → done.

Impl-side lanes only. **Out of scope:** spec-side lifecycle actions in the
walked path (propose-change etc.), autonomous mode (retired for good —
dispatcher drains by default), and multi-repo coverage (B7's two-repo doc
acceptance is DELIVERED and archived at
`plan/archive/cockpit-ux-docs-release/`).

This requirement predates this thread and was never delivered because it
fractured across three re-scopes and ended custody-less — the full trace,
with citations, is `research/why-it-never-happened.md`. This thread is the
missing **delivery/integration owner**.

## Doc custody

**Inherited 2026-07-21** when `plan/cockpit-ux-docs-release/` was archived
to `plan/archive/cockpit-ux-docs-release/`. That thread wrote the `docs/`
tree and would not archive until this obligation had somewhere live to
sit. It is now here. **It is recurring work, not a dormant label** — if
this section is deleted without a successor, the obligation is lost, which
is the specific outcome archival was conditioned on avoiding.

**What it is: periodically re-audit `docs/` against source.** Not a
one-time cleanup. Measured rate of rot, three times:

- B6's docs were wrong within ONE DAY of landing (`185426b`).
- B7's fixes were wrong within a day (five claims, PR #356).
- One of those had been false since the day it was written — the B6
  rewrite was ITSELF an audit that corrected 16 README errors, and it
  introduced a new one.

Several sessions commit to this repo concurrently, which is why prose rots
this fast. **An audit is a snapshot, not a fix.**

**Six gates already run in CI** — do not re-derive them:
`docs_status_hint_lockstep`, `docs_enter_key_lockstep`,
`docs_release_asset_lockstep`, `docs_release_version_lockstep`, and two
tmux scenes pinning the Detail-pane `Attach:` split. They pin the
STRUCTURE of claims — a hint, key binding, asset name, release version, or
detail line moving out from under the prose. **They do NOT verify that
prose describing a named behavior is correct**, and there are two recorded
cases of every gate staying green while the description rotted.

**What a fresh audit can SKIP** (checked clean, unless their area
changes): every Status-line hint, the `s` move-to-status transition table,
the header degrade ladder, global key inertness under overlays, the
8-section Help modal, the attention row format, the whole-record modal
claim, and every TUI claim in `overview-quickstart.md` and
`cli-options.md`.

**Known-silent, deliberately left:** the record modal's footer prints
`up/down scroll | esc to close` while `PgUp`/`PgDn` also page it. That is
an inconsistency inside the source, not doc drift — a small TUI-text fix
or a work-item, not a docs pass.

**One class no source-binding gate can catch:** a claim scoped to a
RELEASED artifact has a second lifetime independent of master. The doc can
accurately describe `v0.2.0` while master moves on, with nothing in the
repo inconsistent. `docs_release_version_lockstep` exists for exactly this
and forces a re-read on every release. Practical rule: **a doc sentence
describing behavior a filed work-item would change should name that
work-item**, so the fix makes the prose self-announcing.

The archived handoff's § "DOC CUSTODY IS ACTIVE" and § "DOCS-ROT
POSTSCRIPT" carry the full case studies. Read them before the first audit.

### Audit log

Keep this short — one dated line per pass, so the next auditor sees what was
last verified against source and can skip it unless its area moved.

- **2026-07-21 (archival session).** Full pass over `docs/detailed-usage.md`
  against current master (`ab6e567`). **Clean — no drift found.** Verified at
  source: the focus ring (`Nav → Content → Detail → Header`, Lanes skips
  Detail — tested at `console-application/src/lib.rs:6605`), header horizontal
  scroll step (`HEADER_SCROLL_STEP = 8`, `:2671`), the six Views and the
  seven-lane canonical order (`Lane::all()`, `source_adapters.rs:292`, tested
  `:6149`), all five auto-disposition vocab strings, the six dispatcher
  settings (`DispatcherSetting::all()`, `:4229`), the Spec-pane prose (correct
  B5→B6 relocation, not drift), the Attach:/Fabro-run split (gate-covered),
  and the Help modal. Also confirmed no doc falsely claims a failed valve
  surfaces an error to the operator — consistent with `-ectqye`'s finding that
  no such surface exists. No source has landed on master since the prior audit
  (`907736d`/`1c1b07f`), so `overview-quickstart.md` and `cli-options.md`
  were NOT re-read this pass (handoff marks them clean; area unchanged).

## Read-first chain

1. `plan/console-happy-path-mvp/research/why-it-never-happened.md` — why
   every predecessor stopped short; the fracture map.
2. `plan/console-happy-path-mvp/research/happy-path-gap-analysis.md` —
   leg-by-leg live-verified status of the happy path, the binding
   constraints (locked core contract), and the custody map.
3. `plan/operator-surface-redesign/handoff.md` — the design thread this one
   consumes: maintainer-brainstorm entry gate, "no impl items until
   ratification", cross-repo verb-vocabulary sequencing.
4. `plan/archive/work-item-lifecycle-redesign/research/locked-core-contract.md`
   — the invariants every slice must obey (zero Beads knowledge; commands
   only through the orchestrator surface; lane consumed never re-derived;
   attention as pure derivation; no console→driver dependency).
5. `docs/lifecycle-walkthrough.md` — B7, landed 2026-07-20: the key-by-key
   walk from the approve valve to shipped, with its hermetic stateful
   fixture. The happy path's downstream legs, already documented; this
   thread adds the upstream (groom) legs and the real-stack walk.

## Status composition (no shadow queue)

Compose live status from the `list-work-items` operation. The epic's edge
set IS the tracked set:

- **blocks** (critical-path gate): `-6msemd` (operator-surface-redesign
  design ratification).
- **tracks** (collected pieces, custody unchanged): `-zweohm` (groom /
  state-valid verbs), `-l4p3ce` (LLM handoff MVP), `-vc7lmq`
  (valid-commands detail), `-qwjfsw` (bogus attach), `-7rcps4` (modal
  paging), `-276inb` (attention record modal).
- **parent-child**: `-sreeqc` (lane rows show no title).

Deliberately NOT tied: `-irdwyb`/`-ipwtll` (exactly-once command spine —
multi-client hardening, parallel, not needed for a single-operator MVP);
`-6hbfq6` (help-overlay navigation — nice-to-have, off the happy path);
`-8aw` (per-item dispatch commands — the queue-level palette drain
suffices for MVP; stays PARKED per `plan/command-queue-semantics/`).

## The track

**Stage 0 — truthfulness/usability, no design gate.** `-7rcps4`, `-276inb`,
`-sreeqc` sit DoR-passed at `pending-approval`; `-qwjfsw` (custody
`-6msemd`, but factory-safe as scoped) needs admission from `backlog`.
Operator admits at the TUI valve (`p`); implementation is **factory-side**:
the Dispatcher drains admitted `ready` items (or `drive` with
`impl:<id>`) — never in-session.

**Stage 1 — the minimal-verb brainstorm (critical path).** Satisfy
`plan/operator-surface-redesign/`'s maintainer entry gate with a
happy-path-minimal agenda: (a) groom-verb exposure on `backlog` /
regroom-flagged items; (b) the `-l4p3ce` handoff MVP (prompt written to a
tmp file; short copy-paste-safe driver command; full-width render + Copy);
(c) state-valid verb filtering for exactly the happy-path lanes. Anything
beyond that minimal subset stays in that thread's own backlog. Output: that
thread's ratified spec-amendment set — authored there, not here.

**Stage 2 — impl slices.** Filed only AFTER Stage-1 ratification (that
thread's hard rule), under whichever epic the brainstorm rules
custodially correct, and dispatched via the factory path (Dispatcher
drain / `drive` `impl:<id>`).

**Stage 3 — validation.** The MVP acceptance, in two parts. (a) Extend
`docs/lifecycle-walkthrough.md` UPSTREAM: today it starts at the approve
valve (B7, landed 2026-07-20); after Stage 2 it gains the missing first
legs — find a backlog item, open its record, groom it via the LLM-driver
handoff — reusing B7's stateful tmux fixture for the E2E. (b) Execute the
FULL walk once against the REAL stack (live tenant + Dispatcher, one repo,
a dummy work-item) — something B7's hermetic acceptance deliberately does
not do. This thread owns the new legs and the one real-stack pass. When
(b) passes, this epic closes.

**Corrected 2026-07-21.** This paragraph used to say "doc custody stays
with `plan/cockpit-ux-docs-release/`" and that "cockpit's Stage-2
(multiple real items, two repos) remains cockpit's". Both are now wrong.
That thread is ARCHIVED (`plan/archive/cockpit-ux-docs-release/`) and doc
custody moved HERE — see § "Doc custody" below. Stage-2 was STRUCK as
dead before the archival: it was autonomous-mode MVP acceptance, and that
mode is retired for good. Nothing about Stage-2 remains to inherit.

## Next action

Open this handoff. Then: launch the console TUI with `just tui` from the
repo root (builds the release binary and serves under the family env
wrapper), and admit the Stage-0 items at
the approve valve (`p`): `-7rcps4`, `-276inb`, `-sreeqc`, and route
`-qwjfsw` to admission; let the Dispatcher drain them (factory path — do
NOT implement in-session). In parallel, schedule the Stage-1 maintainer
brainstorm by resuming the `plan` operation on `operator-surface-redesign`
with this handoff's Stage-1 agenda as the brainstorm input.
