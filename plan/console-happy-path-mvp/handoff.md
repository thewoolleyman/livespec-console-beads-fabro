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

- **2026-07-21 (archival session).** Full pass over **all five operator docs**
  against current master (`ab6e567`) — `detailed-usage.md`,
  `lifecycle-walkthrough.md`, `cli-options.md`, `overview-quickstart.md`,
  `installing.md`. **Clean — no drift found.** (No source landed since the
  prior audit `907736d`/`1c1b07f`, but I re-verified rather than trusting the
  "checked clean" list — which this pass confirms was accurate, in contrast to
  the handoff's stale reconciliation claims.) Sampled at source, not skimmed:
  focus ring (`Nav → Content → Detail → Header`, Lanes skips Detail, tested
  `console-application/src/lib.rs:6605`); `HEADER_SCROLL_STEP = 8` (`:2671`);
  six Views + seven-lane order (`Lane::all()` `source_adapters.rs:292`, tested
  `:6149`); five auto-disposition strings; six dispatcher settings
  (`DispatcherSetting::all()` `:4229`); exactly eleven `LIVESPEC_CONSOLE_*` env
  vars (matches the doc's tables); `events tail` limit `20` (`lib.rs:2016`);
  drain invoked `loop --repo` (`main.rs:139`); poll cadences 2 s
  (`main.rs:57`) / 250 ms keyboard (`console-tui/src/lib.rs:208`); reject
  warned dangerous (`lib.rs:211`); `:` palette → `drain` (tested
  `console-tui/src/lib.rs:3177`). The Spec-pane prose is the correct B5→B6
  relocation, not drift; the Attach:/Fabro-run split and the walkthrough
  keystrokes are gate-/E2E-pinned. Confirmed no doc claims a failed valve
  surfaces an operator error — consistent with `-ectqye`. One refinement to the
  archived handoff's "known-silent" record-modal-footer item: paging **is**
  on screen — the Status band hint (`lib.rs:1500`) shows
  `PgUp/PgDn page`; only the modal's own terse internal footer
  (`console-tui/src/lib.rs:1357`) omits it. Cosmetic terseness, not
  undocumented paging; left as-is.
- **2026-07-26 (happy-path session, delta pass on an overseer nudge).**
  Delta audit against master `ac61669`: since the prior baseline
  `ab6e567`, the only non-docs commits are `940647b` (repeatable-command
  identity — approve/accept deliberately left static-keyed) and
  `2665cad` (command-queue single-consumer claim semantics) — both
  internal command-spine changes. Verified no operator doc claims moved:
  zero hits for command identity / retry / queue-consumption prose
  across `docs/` (the one "idempotent" hit, `detailed-usage.md:158`, is
  about autonomous-decision reflection, untouched by either commit).
  **Clean by delta** — NOT a full re-verify of the skip-list. Scope
  note: `docs/factory-confirmations.md` appeared 2026-07-24 (PR #408),
  so the custody obligation now covers six files. Side findings recorded
  on the ledger, not here: `-276inb`'s subject was delivered via
  `2cd1f28`+`6262f66` while the item sat stranded, and `940647b` is
  pre-implementation context for `-u3w3er` — see each item's 2026-07-26
  `bd` comment. **Mechanism corrected later the same day:**
  `2cd1f28`+`6262f66` are PR #358 — `-276inb`'s OWN dispatched run, not
  another route; the run merged and died at post-merge bookkeeping (see
  `research/strand-capture-2026-07-21/`). The disposition (close on
  recovery, never re-dispatch) stands; a correcting `bd` comment was
  appended to the item.

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
  paging), `-276inb` (attention record modal), and — filed BY this
  thread's 2026-07-21 real-stack walk — `-ectqye` (silent valve failures;
  FLAGGED at the 2026-07-23 valve review, amendment owed, see
  `research/valve-review-amendments.md`) and `-u3w3er` (unretryable
  failed approve/accept).
- **parent-child**: `-sreeqc` (lane rows show no title).

**Adjacent, custody elsewhere** (filed 2026-07-20 by another session).
**CORRECTED 2026-07-26** — the 2026-07-25 version of this block relayed
the five filed titles as present-tense fact; per-item verification
against master and the dispatch journal found two already fixed and one
mis-framed. Verified state:

- `-m36` (drain once-per-store): **FIXED 2026-07-20 by `4241fc3`** —
  `FactoryDrainRequested` is in `is_repeatable_command`
  (`crates/console-cli/src/lib.rs:1713-1725`); the 2026-07-21 drain's
  attempt-suffixed command id confirms it live. Ledger item stale-open;
  closing it is a prepared maintainer decision.
- `-8i9` (bundled workflow ignores the repo's Fabro override): **FIXED
  by 2026-07-21** — every `dispatch-id` journal entry from the
  2026-07-21 and 2026-07-23 runs records the REPO's own
  `workflow_toml`, and three Rust-compiling PRs went green and merged.
  Ledger item stale-open; closing prepared likewise.
- `-9ts` (budget discarded, `--budget 50` hardcoded): **LIVE** —
  `drain_ready_queue` ignores `_request` and pushes
  `OPERATOR_DRAIN_BUDGET` (`console-application/src/lib.rs:1849,1869`).
  Over-dispatches; does not block.
- `-htp` (drain inline on the UI thread): **LIVE** — the drain call
  site (`lib.rs:3363`) runs synchronously in effect handling; the one
  `thread::spawn` in console-cli (`main.rs:207`) is the source poller,
  not the drain. Freezes the cockpit; does not block.
- `-6ma` (strands): **CLOSED 2026-07-26 as superseded** by epic
  `bd-ib-waov` (P1) in the `livespec-orchestrator-beads-fabro` tenant —
  fixing thread `plan/dispatch-claim-liveness/` there (its PR #947),
  verified on that repo's origin/master before closing. The diagnosis
  was CORRECT (stale `active` LEDGER rows shrink WIP capacity —
  `_dispatcher_admission.py` counts `status == "active"` — with nothing
  running behind them; `active` conflates "executing" with "awaiting a
  human"); it was MIS-FILED in this tenant. Beads has no cross-tenant
  edge — the close reason and this line ARE the link.

**The strand obligation is DISCHARGED (2026-07-26, supervisor-
authorized).** All four rows were recovered through the guarded valve
`dispatcher.py reconcile-merged` — merge re-confirmed from the forge,
post-merge janitor green per item, parked at `acceptance` under
`ai-then-human`, verified in the ledger. Never routed through
`backlog`/`ready`, nothing re-dispatched. The capture
(`research/strand-capture-2026-07-21/`, recovery record appended)
remains the reproduction for the orchestrator's
`plan/dispatch-claim-liveness/` thread.

Nothing here blocks a dispatch. The dispatch leg is not dead and there
is no "Stage-0.5 dispatch repair" project.

Deliberately NOT tied: `-irdwyb` (exactly-once command spine —
multi-client hardening, parallel, not needed for a single-operator MVP;
its sibling `-ipwtll` is CLOSED — `done` 2026-07-23); `-8aw` (per-item
dispatch commands — the queue-level palette drain suffices for MVP; stays
PARKED per `plan/archive/command-queue-semantics/`). `-6hbfq6`
(help-overlay navigation) was admitted to `ready` by the 2026-07-23 valve
review — still off the happy path, custody unchanged.

**Measured 2026-07-26, post-recovery (dated snapshot — re-measure
before trusting):** `-276inb`, `-sreeqc`, `-qwjfsw`, `-ogpok4` sit at
**`acceptance`** (assignee `fabro`, the normal factory shape) — their
PRs merged 2026-07-21 (#352, #354, #358, #359), the skipped post-merge
janitor re-ran green during recovery, and each now awaits the **human
accept valve (`c`) under `ai-then-human`** — which is precisely the
happy path's own acceptance leg, walkable at the TUI once a FRESH
cockpit is launched (see Next action). `-u3w3er` and `-6hbfq6` sit
`ready`; the drain is functional (`-m36` fixed), so dispatching them is
an operator choice, not a blocked path.

## The track

**Stage 0 — truthfulness/usability, no design gate. Landed 2026-07-21 —
with an honesty ledger.** All items were admitted and a drain issued; the
full session evidence is `research/real-stack-walk-findings.md`. What
actually counts as walked: `-276inb` was admitted at the TUI valve (`p`)
cleanly; `-qwjfsw` was routed `backlog → ready` at the TUI `s` valve
cleanly; **`-sreeqc`'s TUI approve leg is OPEN** — its first valve press
failed silently (now `-ectqye`) and every retry was swallowed (now
`-u3w3er`), so it was admitted via `drive.py` as a workaround, which
advances the ledger but does NOT exercise the surface this thread exists
to prove. **The dispatch leg (corrected 2026-07-26): the drain
dispatched all five picked items and every implementation MERGED** —
four of the five runs then died at post-merge bookkeeping
(`pull-primary` blocked by this session's own uncommitted primary-
checkout edits; see § "Status composition" snapshot and
`research/strand-capture-2026-07-21/`), so their ledger rows never
reached acceptance. The leg is discharged through merge; the
acceptance legs remain open pending strand recovery. `-7rcps4` was
already `done` before the walk.

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

This thread runs **under supervision** since 2026-07-25 — read
`plan/console-happy-path-mvp/supervisor-handoff.md` FIRST, and re-measure
everything per its § "Reactivating a parked thread" (fetch + lanes + new
items + cockpit binary age) before trusting any claim here.

Then, in order:

1. **Strand recovery is COMPLETE (2026-07-26, supervisor-authorized;
   see § "Status composition"). The live next leg it opens: the accept
   valve.** Four real merged items sit at `acceptance` awaiting the
   operator's `c` under `ai-then-human` — the happy path's own
   acceptance leg, on real work, at the real stack. Before walking it:
   launch a FRESH cockpit (`just tui` — the previous serve was killed
   2026-07-26; never trust a stale binary, never run two clients).
   `-6ma` is CLOSED (superseded by `bd-ib-waov`); `-m36`/`-8i9`
   closures remain prepared maintainer decisions (both fixed on master
   — see the corrected block above).
2. **Stage-1 brainstorm: all seven vocabulary points DECIDED**
   (2026-07-21..25) — recorded with their verification in
   `research/verb-vocabulary-brainstorm.md`. Next brainstorm output: the
   `-l4p3ce` handoff-transport design, then drafting the amendment set.
   The output routes as an ORCHESTRATOR-side propose-change first
   (`livespec-orchestrator-beads-fabro` owns the per-state valid-verb
   vocabulary and has not authored it yet — verified 2026-07-25 against
   that repo's SPECIFICATION); the console side is presentation,
   consumed after ratification.
3. **`-ectqye` routing decided 2026-07-25: reconcile with `-k0w` before
   any amendment or split** — `-k0w` (filed 2026-07-20, factory-drain
   path) already covers both halves of the defect. The store/UI custody
   proposal is with the supervisor (see the research note's § "`-ectqye`
   routing"); nothing is filed until it returns. The valve-review FLAG's
   technical guidance stands: the diagnostic lives in drive's
   already-captured `--json` stdout — never re-plumb stderr through
   `SourceProbe`.
4. **Cockpit hygiene before any further walk**: kill the stale `serve`
   (the 2026-07-21 binary polled for four days), relaunch `just tui`
   fresh, and check for a stray second client first (`ps` for `serve` —
   the single-operator MVP assumes exactly ONE live client).
