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
- **2026-07-29 (delta pass, `ac61669..a5af510`). ONE FINDING — a reachable
  Status-line state is undocumented, and no gate can see it.** Only two commits
  in the range touched `docs/`, both in lockstep with their source: `2d5ce11`
  (split the Status-hint table per-lane) and `77ed854` (walkthrough Step-2 hint).
  `cargo test --test docs_status_hint_lockstep` is GREEN, so every hint the doc
  QUOTES exists in source. **The gap is completeness, not falsity.**
  `requires_attention_from_lane` (`console-application/src/lib.rs:5336-5353`)
  admits exactly THREE lanes into the Attention list — `PendingApproval`(manual),
  `Acceptance`(`ai-then-human`|`human-only`), and **`Blocked`(`needs-human`)**.
  `detailed-usage.md`'s table documents the first two and has no row for the
  third, which renders the catch-all arm (`:1617-1619`)
  `up/down move | enter open | ? help | q quit`. **Live-confirmed at the real
  cockpit**, not only read: on launch the Attention list's first row was
  `Blocked: needs-human` (Detail: `livespec-console-beads-fabro-25rvmd`) and the
  Status band showed exactly that string. Before `2d5ce11` a single row
  ("Attention, a work-item-backed row selected") covered every work-item-backed
  selection; splitting it per-lane covered two of three. The
  `docs_status_hint_lockstep` gate CANNOT catch this — its own docstring commits
  to `doc ⊆ source`, "a hint may exist in source without appearing in the table".
  **`mbohw3` does not close it either**: its pending diff adds an
  "Attention, backlog" row and upgrades the gate to `doc ⊆ rendered contexts`
  (a genuinely stronger verifier), but still omits the blocked/needs-human row —
  worth folding in when that item resumes, since it is already editing this table.
  Checked clean in the same pass: the `acceptance_mode` prose
  (`detailed-usage.md:144-145,346,419`) is still accurate after `6f5f6b6`;
  `842a316`/`ad4d023` added `just` recipes that no operator doc references. The
  focus-ring claims at `:81` and `:318` (`Enter` or `→` moves focus into content)
  were exercised at the keyboard and hold. **NOT re-verified**: the rest of the
  skip-list, and the five docs no commit in this range touched.
  **Scope correction: custody covers SEVEN files, not six.** `docs/README.md`
  has been present since B6 (`7df1ea2`) and was never counted. It is mostly a
  table of contents, but it is not inert prose — it asserts the console "never
  writes the ledger, the orchestrator's settings files, or a Fabro run directly"
  and "issues every mutation through the orchestrator's `drive` API", which is a
  checkable restatement of the locked core contract and exactly the kind of claim
  that rots silently. Audit it with the rest.

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

**Measured 2026-07-26, post-accept-walk (dated snapshot — re-measure
before trusting):** `-276inb`, `-sreeqc`, `-qwjfsw`, `-ogpok4` are
**`done`** — accepted at the TUI `c` valve on a fresh current-master
cockpit, per-item verified (`research/accept-valve-walk-2026-07-26.md`;
their PRs merged 2026-07-21: #352, #354, #358, #359). `-m36` and
`-8i9` are **CLOSED** (maintainer-decided 2026-07-26; both verified
fixed in source before closing — see their close reasons; both exhibit
the fixed-same-day-never-closed pattern). `-u3w3er` and `-6hbfq6` sit
`ready`; the drain is functional, so dispatching them is an operator
choice, not a blocked path. `-9ts` and `-htp` remain the two live
drain defects (over-dispatch; UI-thread freeze) — neither blocks.

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

**RESUME HERE (2026-07-29T05:45Z — supersedes every earlier RESUME HERE block).**
Timestamps here are UTC; we run at UTC+2, so anything after 22:00 UTC dates to the
next LOCAL day — build every timestamp with `date -u`, never by hand (that mistake
cost PR #478).

### 0. THE FACTORY IS DOWN UNTIL 2026-07-31T05:00Z. Read this before planning work.

**The review gate cannot run. Every Stage-2 dispatch will park at the human gate
until the Claude subscription's weekly ceiling resets.** Measured 2026-07-29T05:27Z
from the run's own event log, not inferred:

    stage.failed review — category: transient_infra
      "Internal error: You've hit your limit · resets Jul 31, 5am (UTC)"

The `review` node runs on the Claude SUBSCRIPTION (`workflow.toml:85-95`,
`review_adapter`, `CLAUDE_CODE_OAUTH_TOKEN`). `implement` survived only because it is
overridden to Codex (`acp_adapter`) — a DIFFERENT account. So the outage is
review-only, and it is total: both attempts burned in 34 s.

**Ship-on-cap does NOT rescue this.** That path needs review to RUN and return a
verdict; a review that cannot start never emits one, and `workflow.fabro:283` routes
`review -> escalate` unconditionally on `outcome=failed`. This is the same ceiling
§ 2 already noted for factory-hardening; it has now reached our own dispatch path.

**NO RECONCILE IS OWED — the previous § 0 was wrong on its premise.** It predicted
`mbohw3` would strand at `active` when its PR merged. There is no PR and no branch:
the run never reached the `pr` node. Verified on the forge — `git ls-remote origin
refs/heads/feat/livespec-console-beads-fabro-mbohw3` is empty. Do not run
`reconcile-merged`; there is nothing merged to reconcile.

**State of run `01KYP37TZJ9MRTSDR3A0138W4M` (maintainer-directed 2026-07-29: LEAVE
PARKED).** `implement` **succeeded**, `janitor` (`mise exec -- just check`)
**succeeded — green**, `review` failed on the ceiling above, and the run now sits
`blocked: human_input_required` at the `escalate` gate with a three-option interview
(`[R]` retry / `[I]` re-implement / `[A]` abandon). At or after **2026-07-31T05:00Z**,
answer **`[R]`** via `fabro attach 01KYP37TZJ9MRTSDR3A0138W4M`; `[R]` routes
`fix -> janitor -> review`, so the green work is re-checked rather than rebuilt.
Do NOT answer `[I]` — it discards a completed implement cycle for no benefit, and
would hit the same wall.

A green implementation is BANKED independently of the run: `fabro dump` of all 23
files, including `stages/002-implement@1/diff.patch` (`+207/-107`
`console-application/src/lib.rs`, `+130/-24` `docs_status_hint_lockstep.rs`,
`+33/-3` `console-tui/src/lib.rs`, `+2/-1` `detailed-usage.md`). It lives in a
SESSION scratchpad, so if it still matters after a restart, re-dump rather than
hunt for it. If the holding engine dies before Jul 31, `fabro resume` is the
documented recovery (`workflow.fabro:164-166`) — `attach` is only for a live engine.

**NEVER run a dispatch in the foreground — it is a ~30-40 minute operation.** The
previous session's dispatch was SIGTERM'd by a 20-minute tool timeout, which is why
no dispatcher remains to do post-run bookkeeping. Verified 2026-07-29: no dispatcher
process is running, so nothing will auto-dispatch a `ready` item into the dead gate.

### 1. Then continue the queue, serial — MAINTAINER SCOPE DECISION 2026-07-29

**Run ALL FIVE Stage-2 slices through to MERGE, then PARK BEFORE Stage 3(b).** The
maintainer's rationale, recorded so it is not re-litigated: finishing the slices gets
the implementation complete and the ledger honest, and leaves the Stage 3(b) walk —
which needs a fresh cockpit and continuous operator attention — for a DELIBERATE
session rather than the tail of a long one. The walk is the evidence this whole thread
exists to produce, and *doing it exhausted is how a leg gets recorded as walked when it
was driven*.

Order, serial: (1) `mbohw3`'s live run through to merge, applying the § 2 refusal
expectation below; (2) B1 `-nvflph`; (3) B2-B4 (`-vwxyj4`/`-cyixzi`/`-zvnjef`)
verify-close behind B1; (4) C `-cxu4eu`; (5) the tier-check bug `-ff6aue`.

**BLOCKED ON THE § 0 CEILING, NOT ON THIS ORDER.** The order stands and needs no
re-litigation; nothing in it can reach MERGE before 2026-07-31T05:00Z, because every
step traverses the dead `review` node. Resume the order at the reset — do not
re-plan it, and do not read the delay as a scope change.

**The admission half IS DONE (2026-07-29T05:4xZ, at the real TUI `p` valve, NOT
`drive.py`).** `-cxu4eu`, `-cyixzi`, `-vwxyj4`, `-zvnjef` are all `ready`, each
verified on the ledger after its own valve press. Every valve rendered
`Approve work-item / Target: <exact id>` and was confirmed only after the id was
read back; the pending-approval hint was captured verbatim at all five rows first:
`up/down move | enter open | p approve | r reject | m set-admission | ? help | q quit`
— `p`/`r`/`m` present, **no `c accept`**, matching `attention_item_footer_hint`'s
`PendingApproval` arm exactly. No silent failure on any press. That is a THIRD live
proof of v037 consumption, on a third lane.

`-ectqye` was DELIBERATELY LEFT `pending-approval` — its routing is undecided per § 3
below (reconcile with `-k0w` first). It sits adjacent to the others in the Attention
list and the selection landed on it twice during navigation; re-verify the `Target:`
line before any press near it.

Then park with the Stage 3(b) walk legs QUEUED and named, not attempted: the groom leg
(needs the vocabulary ratification + `-l4p3ce` transport) and ONE CONTINUOUS
single-item walk (find → groom → admit → dispatch → monitor → accept). Individual legs
are now proven; what is missing is one unbroken pass.

**Stage 3(b)'s ACCEPT leg has been cut out from under the walk — maintainer-directed,
and it needs a decision before the walk is attempted.** `6f5f6b6` (2026-07-29) set
`acceptance_mode` to `ai-only` fleet-wide, deliberately reversing the 2026-07-21
restore that this thread made *because the happy-path walk ships at the accept valve*.
Consequence, from source: `requires_attention_from_lane`
(`console-application/src/lib.rs:5336-5353`) surfaces an `acceptance` item only under
`ai-then-human` or `human-only`, and its docstring records that an `ai-only` item
"auto-completes to `done` rather than resting in `acceptance`". So a newly-dispatched
item will never rest at a human `c accept`. `auto_approve_ready` stays `false`, so the
APPROVE half of the walk is unaffected. The walk therefore cannot end at a keyboard
accept unless `acceptance_mode` is set back for the duration — which is a dispatcher
lever in `.livespec.jsonc` and so maintainer-owned. Decide it BEFORE the walk, not
during. (The already-banked accept-valve evidence stands: 2026-07-26's four items and
2026-07-29's `dm5f7q` were accepted under the old setting.)

### 2. On a publish refusal — two cases, do not conflate them

Our `.fabro` fork's `pr.md` was SYNCED from upstream (#476), so the publish leg now
runs an unconditional `git fetch origin master` + `git rebase origin/master`
immediately before the push. The bounded retry, however, keys on an EXACT signature
naming `.github/workflows/ci.yml` (`prompts/pr.md:44-45`) and explicitly forbids
generalising (`:55`), so it is INERT for any other filename.

- refusal naming **`ci.yml`** → the retry SHOULD have fired. If it did not, that IS a
  finding: STOP AND REPORT.
- refusal naming **any other** workflow file → inert by upstream design, NOT novel.
  Apply the known recovery — answer the run's interview (Retry), THEN `fabro steer` the
  in-sandbox `git fetch && git rebase origin/master` — and record it as an instance of
  the defect already filed with factory-hardening. On this host
  `bump-pin-from-dispatch.yml` is the LIKELIER trigger; every pin bump rewrites it.

Residual exposure is narrow: the retry only matters if master moves between that
rebase and the push. As of 2026-07-29 the upstream fix had NOT shipped (`pr.md` did
not move in release `856d699b5f7d`); factory-hardening filed it but is at its weekly
account ceiling until 2026-07-31.

### 3. DONE — do not redo

Merged this session, all forge-verified: **#472** `edc3b29` (brief-29 correction),
**#474** `ad4d023` (adopt the `check-no-workflow-edits` janitor recipe), **#476**
`6b3c434` (SYNC the forked `pr.md` publish leg), **#477** `f935ac8` (scope step 5's
retry expectation), **#478** `24c75e1` (UTC dates), **#479** `842a316` (the fork-drift
guard), **#487** `3277d74` (re-pin after upstream `856d699b5f7d`).

- **`dm5f7q` is `done`.** Recovered via a maintainer-authorized reduced janitor argv
  (dropping only the provably-vacuous `check-no-workflow-edits`), which returned it to
  `acceptance` through the legitimate door — not a hand-close — and it was then
  **ACCEPTED AT THE REAL TUI `c` VALVE**. Hints captured verbatim first; no silent
  failure. Its `bd` comments carry the exact argv and the vacuity evidence.
- **The APPROVE leg is WALKED at the keyboard** (the leg `-sreeqc` never got):
  `ff6aue`, `mbohw3`, `nvflph` admitted at the TUI `p` valve, each verified on the
  ledger. Hints captured verbatim before any keypress:
  `... | s move-status | p approve | r reject | m set-admission | g merge cap |
  f fix cap | n set-acceptance | k rework cap | ? help | q quit` — `p`/`r`/`m` present,
  **no `c accept`**, matching `lane_item_footer_hint`'s `PendingApproval` arm exactly.
  Together with the acceptance walk that is TWO live proofs of v037 consumption on two
  different lanes.
- **Cleanup done.** Remote `feat/livespec-console-beads-fabro-dm5f7q` deleted after a
  CONFIRMED backup ref `refs/backup/feat-dm5f7q-20260728`. NOTE: an ancestor test is
  the WRONG check in this repo — it rebase-merges, so branch SHAs never become
  ancestors; verify by patch-id with `git cherry`.

### 4. Corrections a successor will otherwise get wrong

- **`gap-23tps2nk` will NEVER "close" in `detect_impl_gaps`.** That command is a
  SPEC-CLAUSE CENSUS — its own docstring: it "enumerates every MUST / MUST NOT / SHOULD
  / SHOULD NOT rule", ids are "a pure function of the spec-file path + canonical heading
  path + rule text", and it is "intrinsically non-mutating". It was 179 before the
  accept and 179 after. CLOSURE LIVES IN THE GAP-TIED WORK-ITEM:
  `list_work_items.py --with-gap-id gap-23tps2nk --json` → count 1, `dm5f7q`,
  `status=done`. Do not re-run it expecting 178.
- **`mbohw3` is a CORRECTNESS fix, not a tidy-up, and there are THREE encodings.**
  `attention_item_footer_hint` (`:1611-1621`) and `lane_item_footer_hint` (`:1623-`)
  are separate per-lane tables and they CONTRADICT each other on `Lane::Backlog` — the
  lane view advertises `m set-admission`, the Attention view does not, and the predicate
  (`:495-497`) admits it, agreeing with the lane table. The fix must collapse all three
  to one derivation and assert cross-view consistency. Both `bd` comments are on the
  item and rode into the dispatch as operator riders.
  **REFINEMENT 2026-07-29 — the STRUCTURAL claim holds, the REACHABILITY claim as
  written does not.** The item's rider calls the Backlog divergence "a live
  operator-visible defect", which requires a Backlog row to be selectable in the
  ATTENTION view. The valve-actionable lane fold cannot produce one:
  `requires_attention_from_lane` (`:5336-5353`) admits only
  `PendingApproval`(manual), `Acceptance`(`ai-then-human`|`human-only`) and
  `Blocked`(`needs-human`). The only route left is an ingested needs-attention row
  whose `work_item_id` happens to reference a Backlog item — because
  `selected_work_item_lane` (`:1352`) resolves the lane via `work_item_by_id`, i.e.
  from the LANE fold, not from the attention snapshot the row came from.
  Corroborated live 2026-07-29: a 61-row Attention list held exactly one `Blocked`
  and five `Pending approval` work-item rows and no Backlog/Ready/Active/Done row.
  So the three-encodings defect and the fix are both still correct — collapse them —
  but do NOT expect to reproduce the divergence by selecting a Backlog item in
  Attention, and do not let a failed reproduction be read as "no defect". Pin the
  rider's "first thing to pin down" through the LANE view, where the Backlog arm is
  plainly reachable.

### 5. The fork, and the guard that now protects it

`.fabro/workflows/implement-work-item/` is a COMMITTED FORK, read from our tree
regardless of plugin version. **DELETE IS NOT AVAILABLE**: it is the SANCTIONED
mechanism — `_dispatcher_paths.py::workflow_toml()` prefers the dispatch target's own
committed `workflow.toml`, its docstring naming our exact case ("a Rust repo needing
the `python-rust-agent-` layer, against the orchestrator's Python-only pin") — and
upstream still pins a `python-agent` layer its own comment documents as carrying "no
Rust". There is NO narrow override: `graph` is relative and resolves beside the chosen
toml, so a repo-local toml drags in the graph AND every prompt. The real fix is an
upstream narrow image-pin override, which the supervisor is requesting.

**DO NOT sync `review.md` or `review-fix.md`.** Our review gate is ADVISORY /
ship-on-cap by recorded decision (`workflow.fabro:9-10`, `bd-ib-egms32`) while
upstream's is BLOCKING; syncing would silently revert a ratified policy.

`just check-fork-drift` (crate `console-fork-drift-check`) pins the UPSTREAM digest of
every fork file plus a mandatory reason, and fires when UPSTREAM moves. It is immune to
our own pin-bump rewrites because it pins upstream's bytes. Its upstream lane cannot run
in CI (that would need a `.github/workflows/` edit, which factory branches must not
make), so it prints a LOUD skip there and runs for real on every dispatching machine
including the pre-push hook. **It fired for real within hours of landing** — caught
upstream `856d699b5f7d` moving `workflow.toml`, which on review was a one-line docker
pin bump needing no port, re-pinned deliberately in #487. Re-pin with
`just refresh-fork-upstream-pins` only AFTER reviewing what upstream changed — never to
make a red build green.

**It was DEMONSTRATED RED before it was accepted** (#479), three ways, exit codes read
UNPIPED because a piped `$?` is the last command's: an upstream digest that no longer
matches its pin (named `prompts/pr.md`), an undeclared file added to the fork (named
it), and a pin left with an empty `reason` — all `RC=1`, with green restoring
byte-identically afterwards. 26 unit tests; `lib.rs` at 100% line coverage per the
workspace gate. It is a justified-divergence tracker rather than an allowlist: an
allowlist says "ignore this", the `reason` field says "here is why", and
`present_in_fork: false` records a KNOWN omission (upstream's `disposition` stage) so
the gate does not cry missing-file every run.

### 6. Standing rules

worktree → PR → rebase-merge, never commit on the primary; `mise exec -- git` so
lefthook runs; a fresh worktree needs `just install-worktree-pack` first and its
`.livespec.jsonc` write reverted; `bd` needs the `/usr/local/bin/with-livespec-env.sh --`
prefix; verify against the FORGE after a fetch; outcomes from ARTIFACTS not exit codes,
and an exit code read through a pipe is the last command's; never `--no-verify`; never
touch `.github/workflows/` or another session's worktrees.

**A NAMED PATTERN ON THIS TRACK: an instruction outliving the condition that made it
correct.** It has happened twice — "expect the stale-base refusal" (true under the
pre-2026-07-24 plugin, false after) and step 5's "a refusal is NOT expected" (true only
if the retry fired on any signature). Durable guidance here SHOULD NAME THE CONDITION
it depends on, so the next reader can check whether it still holds instead of
inheriting a conclusion. Its sibling: **an absence never announces itself in a grep for
the wrong token** — `pr.md` had two innocent "rebase" hits and the missing thing was
`fetch`. And the track's dominant defect class remains **correct-looking state that
nothing was checking**.

This thread runs **under supervision** since 2026-07-25 — read
`plan/console-happy-path-mvp/supervisor-handoff.md` FIRST, and re-measure
everything per its § "Reactivating a parked thread" (fetch + lanes + new
items + cockpit binary age) before trusting any claim here.

Then, in order:

1. **Accept AND approve legs are both WALKED at the keyboard** — accept on
   2026-07-26 (`research/accept-valve-walk-2026-07-26.md`) and again on
   2026-07-29 for `dm5f7q`; approve on 2026-07-29 for `ff6aue`/`mbohw3`/
   `nvflph`, which discharges the leg `-sreeqc` never got. Still owed for
   Stage 3(b): the groom leg (needs the vocabulary ratification +
   `-l4p3ce` transport) and ONE CONTINUOUS single-item walk
   (find → groom → admit → dispatch → monitor → accept) — the legs are
   now individually proven but have not been walked end-to-end in one
   pass. `-6ma`/`-m36`/`-8i9` are all CLOSED with verified reasons.
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
4. **Cockpit hygiene before any further walk**: `ps` for stray `serve`
   processes FIRST (the single-operator MVP assumes exactly ONE live
   client; a four-day-old binary was once caught still polling), then
   relaunch `just tui`. Check the binary is not older than any merge that
   touched a `console-*` crate — `cargo` will no-op the rebuild when only
   non-console crates moved, which is correct but must be VERIFIED rather
   than assumed. The cockpit runs in tmux `happy-path-tui`; it is the
   PRODUCT, not an agent session.
