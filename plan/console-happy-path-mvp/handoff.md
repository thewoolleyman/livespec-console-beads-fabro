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

**A THIRD STRUCTURAL MECHANISM, found 2026-07-30 and filed as `-2ckgiy`: ABSENT prose
for a shipped verb, which no gate can catch even in principle.** The two mechanisms
above are both ROT — prose that was true and became false. This one is different and
the custody section needs it stated separately, because an auditor looking for rot will
not find it. `-cxu4eu` shipped an entire new operator verb (`h`, the driver-handoff
overlay) and `docs/` never mentions it: `grep -c "h handoff" docs/detailed-usage.md` =
0, and no mention anywhere of the overlay or OSC 52. All 13 CI checks were green.
**Neither arm of `docs_status_hint_lockstep` can see it**: the value arm is
DELIBERATELY one-directional (`doc ⊆ source`, its own docstring saying "a hint may exist
in source without appearing in the table"), and the completeness arm only requires the
ten reachable CONTEXTS to HAVE ROWS — the backlog row exists, it is merely missing a
key. So the failure mode is: **a new key added to an existing hint arm is invisible to
both arms, forever.** Practical rule for this custody: after any slice that adds an
operator key, diff the LIVE Status band against the documented row for that context,
by hand, because nothing mechanical will.

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
- **2026-07-29 (delta pass, `ac61669..a5af510`). FINDING — FOUR reachable
  Status-line states are undocumented, two of them on this thread's own happy
  path, and no gate can see any of them.** Only two commits
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
  Status band showed exactly that string, which `grep -F` puts nowhere in `docs/`.
  Before `2d5ce11` a single row ("Attention, a work-item-backed row selected")
  covered every work-item-backed selection; splitting it per-lane covered two of
  three.
  **The same split left the LANES table short by three, and these matter more.**
  `lane_item_footer_hint` (`:1623-1652`) has SEVEN arms — one per `Lane::all()` —
  and the doc carries four lane-item rows (backlog, pending-approval, acceptance,
  done). Undocumented, all reachable by drilling into the lane:
    - `Lane::Ready` → `up/down move | enter item | esc lane list | s move-status |
      g merge cap | f fix cap | n set-acceptance | k rework cap | ? help | q quit`
    - `Lane::Active` → `up/down move | enter item | esc lane list |
      n set-acceptance | k rework cap | ? help | q quit`
    - `Lane::Blocked` → `up/down move | enter item | esc lane list | s move-status |
      ? help | q quit`
  `ready` and `active` are the two lanes this thread's happy path runs THROUGH —
  every item admitted at the approve valve lands in `ready`, and every dispatched
  item sits in `active`. So the operator drilling into the most-walked lane in the
  product meets a Status line the documentation never describes. That is a bigger
  miss than the Attention one and it is the reason this entry is not a one-liner.
  The
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
- **2026-07-29T23:1x–23:2xZ (delta pass, `a5af510..5e91d0e`; 2026-07-30 LOCAL —
  we run at UTC+2). FINDING — `docs/lifecycle-walkthrough.md` Steps 5–7 are
  UNREACHABLE on this repo as configured, and the E2E the doc cites as its own
  guarantee structurally cannot catch it. Filed `-6zqv2w`.** Every link verified
  against `5e91d0e`: `.livespec.jsonc:71` sets `"acceptance_mode": "ai-only"`
  (`6f5f6b6`, maintainer-directed); `requires_attention_from_lane`
  (`crates/console-application/src/lib.rs:5426-5443`) admits an `Acceptance`
  item only under `AiThenHuman | HumanOnly`, its docstring (`:5422-5424`)
  recording that an `ai-only` item "auto-completes to `done` rather than resting
  in `acceptance`"; and no item carries a per-item `acceptance_policy` override
  (all `None`), so a freshly dispatched item inherits `ai-only`. Against that,
  the walkthrough asserts at `:140-141` that the factory "parks the result in
  `acceptance` for a human to judge", at `:143-150` that the item is "back in
  your inbox" with `attention: 1`, and presses `c` at `:155+` — with NO
  precondition stated anywhere (it never names `acceptance_mode`, `ai-only`,
  `ai-then-human`, `human-only`, or the `n set-acceptance` valve; `n` appears
  only inside a quoted hint string at `:88`). **Why no gate sees it, which is
  the whole point:** the doc opens (`:5-10`) by staking its correctness on
  `tmux_tui_e2e_lifecycle_walkthrough_two_repos` — "if this page and the binary
  disagree, that test fails" — but that test advances the item with
  `fixture.factory_move("acceptance")` (`tmux_tui_e2e.rs:1074`), SCRIPTING the
  transition against a hermetic fixture that never reads `.livespec.jsonc`. The
  item is PLACED in `acceptance` unconditionally, so Step 6's `c` always has a
  target and the test is green for any `acceptance_mode`. Note the fix was
  ALREADY KNOWN and merely unpropagated: § 1 hit this for the thread's own
  Stage 3(b) walk and resolved it with an early `n set-acceptance` press, and
  nobody carried that into the operator doc describing the same walk.
  **The 2026-07-29 FINDING is CLOSED, and my predecessor's prediction about it
  was wrong in our favour — do not re-derive it.** That entry expected `mbohw3`
  to leave the blocked/needs-human row out; the MERGED `514a326` added all four
  missing rows (`Attention, blocked`; Lanes `ready`/`active`/`blocked`) and
  upgraded the gate past `doc ⊆ rendered contexts` to a hard completeness list
  requiring TEN contexts, `Attention, blocked work-item selected` among them.
  Each of the four doc strings was compared to its rendering arm in the
  bits-keyed `attention_item_footer_hint_for_bits` / `lane_item_footer_hint_for_bits`
  (`lib.rs:1670-1759`) and matches. All four docs gate FILES are green (19
  tests), and **both arms of `docs_status_hint_lockstep` were mutation-proved
  RED, independently** (delete the `Attention, blocked` row → only the
  completeness arm fails; typo a hint → only the value arm fails; RC=101 each,
  exit codes read UNPIPED, tree restored byte-identically). Also re-verified
  because `57e94a4` moved it OFF the skip-list: the `s` move-status table
  (`detailed-usage.md:364-372`) — all SEVEN lanes now present and every row
  equals `status_move_targets` (`lib.rs:473-480`), and the three restatements of
  the ship-guard claim in `lifecycle-walkthrough.md` (`:15-18`, `:22-28`,
  `:152-153`) each hold. Release-scoped claim clean, checked against the thing
  no in-repo gate can reach — the FORGE: `gh release list` latest is `v0.3.0`,
  equal to `.release-please-manifest.json` and to the gate's
  `DOCS_REVIEWED_AGAINST`. Custody inventory confirmed at SEVEN files.
  **NOT re-verified**: the rest of the skip-list, and the five docs no commit in
  this range touched. **Method note, since this track values them:** my first
  read of the move-status table reported `done` missing, because my grep
  enumerated six lanes and not the seventh — the row was there all along. That
  is § 6's "an absence never announces itself in a grep for the wrong token"
  catching the person quoting it, one entry after the last author said the same.
- **2026-07-30 (SPOT CHECK ONLY — do not credit this as a delta pass).** While walking
  the `c` accept leg (§ 0h) the live Status band on the selected `Acceptance review`
  row was compared to its documented row and matched byte for byte:
  `detailed-usage.md:268` = `up/down move | enter open | c accept | r reject | ? help |
  q quit`. The blocked row selected moments earlier correctly showed no `c`. **Scope
  searched: ONE row, one context, one moment.** Nothing else was checked. Note against
  that: `docs/lifecycle-walkthrough.md` GAINED a new "Human accept precondition"
  section and a renumbering of Steps 3-9 in this same session (PR #530), and no audit
  has been run over the result — **treat the whole walkthrough as UNVERIFIED prose.**
  Recorded because an unrecorded check is indistinguishable from one never made, not
  because it discharges any part of the custody.
- **2026-08-02 (delta pass, `5e91d0e..69ea9d4`). FINDING — `-2ckgiy` is PARTIALLY fixed
  and its title is now wrong; everything else checked is CLEAN, including the walkthrough
  the last entry flagged UNVERIFIED.** 77 commits in range, all but ~12 dep bumps. Twelve
  touched `crates/`, only THREE touched `docs/` — that gap is where this pass looked.
  **The walkthrough flag is DISCHARGED.** `docs/lifecycle-walkthrough.md`'s new "Human
  accept precondition" (`:47-55`) is accurate against config, not just internally
  consistent: it states this repo's default `acceptance_mode` is `ai-only` so a new item
  auto-completes to `done`, and `.livespec.jsonc:71` is `"acceptance_mode": "ai-only"`.
  Steps 1–9 are sequential with no gaps or duplicates after PR #530's renumbering, and
  BOTH back-references resolve — `:130` "Step 7 needs" → Step 7 is the accept valve, and
  `:174` "Step 3 set the item to `ai-then-human`" → Step 3 is "Set the human acceptance
  leg". A renumbering that leaves a dangling back-reference is invisible to every gate,
  which is why they were checked by hand.
  **Help overlay re-verified AT SOURCE because `886011d` moved it off the skip-list.**
  `detailed-usage.md:290` (`left/right pane | up/down act | PgUp/PgDn page | esc close
  help`) matches the new focus model, and `:408`'s claim that PgUp/PgDn pages "regardless
  of which help pane is focused" is TRUE: `page_scroll_input` (`console-tui/src/lib.rs:810`)
  matches `TuiOverlay::Help { .. }` and never inspects `HelpFocus`. Its "vertical only"
  clause holds too — the docstring records no horizontal counterpart exists.
  Also clean: `c540a96` (approve-valve retry) introduced NO doc claim to rot — the single
  `idempotent` hit (`:158`) is still the autonomous-decision reflection sentence, as the
  2026-07-26 entry found; the `-0uw` fix IS documented (`:300-306`, `p approve` renders
  only while effective admission is `manual`); and `docs/README.md`'s core-contract claim
  survives the new `h` verb — `lib.rs:3068` RENDERS a `claude "/livespec-orchestrator-
  beads-fabro:<op>"` string for the operator to run and never issues it, so "issues every
  mutation through the orchestrator's `drive` API" still holds.
  **THE FINDING.** `-2ckgiy` recorded that `h` shipped with `grep -c "h handoff"` = 0.
  That is now 3: PR #573 added both hint rows (`:273`, `:277`) and the availability prose
  (`:304-306`), and the upgraded lockstep gate pins those rows. **But no doc anywhere says
  what pressing `h` DOES** — that it opens a full-width `TuiOverlay::DriverHandoff`
  (`console-tui/src/lib.rs:465`) rendering a copyable driver invocation. The other two
  `handoff` hits (`:116`, `:121`) are the ATTENTION ROW's `handoff.command`, a different
  thing. So the item's own thesis survives its own partial fix, one level in: the
  completeness arm requires reachable CONTEXTS to have ROWS, and nothing requires prose
  describing the surface a key OPENS. Reframe recorded on the item.
  **Do NOT "fix" the OSC 52 silence.** `docs/` never mentions OSC 52 and that is CORRECT —
  the shipped binary never sends it (the store-backed sink drops `CopyDriverHandoff` while
  the overlay says "copy sent to terminal"). Documenting it would document a dead path;
  the defect is the overlay's claim, and it is a separate item.
  **SECOND INCREMENT, same pass — THREE SKIP-LIST ENTRIES PULLED BACK AND RE-VERIFIED,
  all CLEAN.** PR #573 made Status hints, hotkeys and Help rosters registry-derived and
  deliberately made `h` INERT on the Attention surface, and `886011d` changed Help
  navigation — so "every Status-line hint", "the 8-section Help modal" and "global key
  inertness under overlays" all had their AREA CHANGE and came off the skip list. Each
  was re-checked at source, not skimmed:
    - **8-section Help modal.** `HELP_SECTION_COUNT = 1 + TuiView::all().len() + 1`
      (`lib.rs:5554`) and `TuiView::all()` has SIX entries (`:130-136`), so 1+6+1 = 8.
      `detailed-usage.md:401-402` says "eight sections: Global actions first, then one
      per view (Attention, Spec, Lanes, Events, Repos, Settings), and Header last" —
      matching the count, both bookends, AND the source order.
    - **The `h`-inert-on-Attention change landed correctly in docs.** Zero of the five
      `Attention, …` hint rows carry `h handoff`; it appears only on the two drilled-in
      LANES rows (`:273`, `:277`), and `:304-306` explains the asymmetry ("a drilled-in
      backlog item (groom) or a drilled-in host-only-refused ready item (implement)").
      Because `h` was previously live-but-UNHINTED, the behaviour change made the docs
      more accurate rather than less — worth noting, since a deliberate behaviour change
      is normally where prose rots.
    - **Global key inertness, two claims proved at source.** `:36`/`:328` "`Tab` and
      `Shift-Tab` are inert while any overlay is open" — `tab_input`
      (`console-tui/src/lib.rs:759-762`) returns `None` on `overlay().is_open()`, so the
      "any" quantifier is exact. `:411` "`?` is inert while Help is open" —
      `question_input` (`:790-798`) returns `None` for `Help`, `WorkItemDetail` and
      `DriverHandoff`. That claim is also correctly SCOPED: under Search, the palette,
      the command modal and valve-confirm, `?` is `text_input('?')` — it types a literal
      `?` rather than being inert, and the doc does not claim otherwise.
  **THIRD INCREMENT, same pass — the `s` MOVE-STATUS TABLE and the PALETTE claim
  re-verified, both CLEAN.**
    - **Move-status table** (`detailed-usage.md:374-383`) vs `status_move_targets`
      (`lib.rs:488-495`). Extracted BOTH SIDES AND DIFFED AS SETS rather than grepping for
      tokens, deliberately: the 2026-07-29T23 entry records an auditor reporting `done`
      missing because a grep enumerated six lanes and not the seventh, and the table's
      seventh row does sit past a natural stopping point. All SEVEN lanes present and
      every row matches — `backlog`→ready,blocked; `pending-approval`/`ready`/`acceptance`
      →backlog,blocked (one source arm, three doc rows, all three correct);
      `blocked`→backlog,ready; `active` and `done`→nothing. The doc's added annotations
      ("through resolve-blocked", "active is entered by the factory", "a shipped item
      offers no onward move") are explanatory and contradicted by nothing.
    - **Palette** (`:386-388`) "accepts exactly two commands, `drain` and `drain ready
      queue`" — `command_palette_query_matches_drain` (`lib.rs:3533-3536`) is exactly
      `normalized == "drain" || normalized == "drain ready queue"`. Correct.
      **FORWARD FLAG: this sentence is a KNOWN future-change point.** Slice B adds
      `actions` to the palette, so it becomes three. The branch already updates this prose;
      the claim is correct on master TODAY and must not be "corrected" ahead of the merge.
    - **`overview-quickstart.md` and `cli-options.md` stay legitimately SKIPPED** — checked
      rather than assumed: neither quotes a hint string or a key binding, so the registry
      rewrite could not have rotted them. Their skip-list entry survives on
      "area unchanged", which is the condition the skip list is actually written against.
  **NOT re-verified**: the header degrade ladder, the attention row format, and the
  whole-record modal claim (none of their areas moved in this range), and the docs no
  commit in this range touched.

## Read-first chain

1. `plan/console-happy-path-mvp/research/why-it-never-happened.md` — why
   every predecessor stopped short; the fracture map.
2. `plan/console-happy-path-mvp/research/happy-path-gap-analysis.md` —
   leg-by-leg live-verified status of the happy path, the binding
   constraints (locked core contract), and the custody map.
3. `plan/archive/operator-surface-redesign/handoff.md` — the design thread this
   one consumes: maintainer-brainstorm entry gate, "no impl items until
   ratification", cross-repo verb-vocabulary sequencing. **ARCHIVED 2026-08-02**
   (absorbed into the 01–04 arc); read its archive banner first — the body's
   "do not archive" and sequencing instructions are historical.
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

> **RE-MEASURED 2026-08-02 — the enumeration below is a DATED SNAPSHOT and the live
> edge set has outgrown it. Read the edges, not this list; that is what this section
> already tells you to do.** Live at 2026-08-02: SEVENTEEN edges on `-b3k5hi` —
> 7 backlog/`tracks`, 5 closed/`tracks`, 4 pending-approval/`tracks`, 1 backlog/`blocks`.
> Items filed after this prose was written (`-ekb5vq`, `-3tg`, `-3lxx7t`, `-drn`, `-0uw`,
> `-pj5g3f`, `-6zqv2w`, `-2ckgiy`, `-htz`, `-w7d`) carry edges and appear nowhere below.
>
> **ONE EDGE IS STALE IN SUBSTANCE: `-6msemd` is still wired `via blocks`.** That gate is
> SATISFIED — the maintainer's menu-primary decision answered the design question, the
> thread is archived at `plan/archive/operator-surface-redesign/`, and its children were
> re-parented to plans 02 and 03. The epic itself stays open only as a PREPARED
> maintainer decision to close, so removing the `blocks` edge is that same decision's
> to make and was NOT taken here. Recorded because a stale `blocks` edge is precisely
> what corrupts a `next` ranking, and this thread is parked where nobody would notice.
>
> **Two entries below are NOT epic edges, and both are fine:** `-u3w3er` is CLOSED, and
> `-ectqye` is tracked by plan 01 (`-dvv`) instead — verified, not assumed. It is NOT
> custody-less. `-sreeqc`'s parent-child edge IS present.
>
> **MEASUREMENT GOTCHA, cost me two near-false findings today — `bd dep list <epic>`
> DOES NOT SHOW parent-child CHILDREN.** The edge is stored on the CHILD and points up,
> so an epic with children can report "no dependencies" (that is exactly what `-6msemd`
> did) and a child can be absent from its own parent's listing (that is what `-sreeqc`
> did). Closed items DO appear, so absence is not a status filter. To enumerate children,
> query the CHILD (`bd show <child> --json` → `parent`), or you will report a wired item
> as orphaned.
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
  **Re-verified 2026-08-02: still true, anchor now `:1731-1743` (moved 18 lines), and
  `-m36`/`-8i9`/`-6ma` are all CLOSED on the ledger — the prepared decisions were taken.**

  > **DO NOT read `is_repeatable_command` as the whole retry story — `-u3w3er`'s fix is
  > NOT there and looking for it there suggests, wrongly, that the item was closed
  > without one.** Approve and accept are DELIBERATELY absent from that list: they are
  > once-per-item valves and making them repeatable would fire the valve twice. `c540a96`
  > fixed retry through a separate, narrower path — `distinguish_retryable_command`
  > (`crates/console-cli/src/lib.rs:1769-1781`), which applies the sequence discriminator
  > to a once-only valve ONLY when `is_failed_once_only_valve_retry` says the prior static
  > row is TERMINAL-FAILED, and otherwise preserves the static key. So the idempotency
  > guarantee is intact and the retry works, by design, in two different places.
  > Recorded because this repo has a named pattern of concluding "fixed-same-day-never-
  > closed" or "closed-but-not-fixed" from looking in the one obvious place.
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

> **RE-CHECKED 2026-08-02 — ONE OF THE TWO DEFECTS THAT MADE THIS LEG FAIL IS NOW FIXED,
> so the leg is no longer blocked by what blocked it.** The record above stays as written
> (it is the honest account of what happened on 2026-07-21) but a reader planning a
> re-walk needs the delta:
>
> - **`-u3w3er` ("every retry was swallowed") is FIXED and CLOSED.** `c540a96` added
>   `distinguish_retryable_command` (`crates/console-cli/src/lib.rs:1769-1781`): a
>   once-only valve whose prior static row is TERMINAL-FAILED now gets a sequence
>   discriminator on retry, so a failed `p` press can be pressed again and lands. The
>   swallowed-retry half of this leg's failure is gone. **Note it is NOT in
>   `is_repeatable_command`** — approve/accept stay once-only by design; see the
>   do-not-conclude note in § "Adjacent, custody elsewhere".
> - **`-ectqye` ("first valve press failed silently") is STILL `pending-approval`**, with
>   routing undecided per § 3. Its ACTION-INVOCATION half is built on the unmerged
>   `feat/action-invoker` branch (refusal captured and rendered) and is tracked by plan 01;
>   the background/journal half belongs to plan 03. So the silence is addressed in code
>   that has not merged.
>
> **Practical consequence for plan 04's walk:** a failed approve is now RETRYABLE at the
> TUI even though it may still be SILENT about why it failed. Do not plan around the
> retry defect; do plan around the silence until slice C lands.

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

### RESUME HERE — 2026-08-02T~10:30Z. SUPERSEDES EVERY BLOCK BELOW.

**THIS THREAD IS STILL PARKED. The live work is plan 01 — go to
`plan/01-action-registry-and-invoker/handoff.md`.** This block exists because a session
restart inherits ONLY this file, so the arc's state has to be reachable from here.

**Landed to master today:** PR #576 (wrapup + `plan/operator-surface-redesign/` absorbed
and archived, custody accepted by plans 02/03 as LEDGER EDGES), #579 (spec
propose-change filed), #581 (doc-custody delta pass). Master was `ff52b37` at wind-down.

**OPEN AND AWAITING MERGE — check both before doing anything:**

| PR | what | state at wind-down |
|---|---|---|
| **#585** | **slice A** — parity gate + `set-workflow-scope-override` + the coverage-gate disposition | **14/14 CI GREEN**, open |
| #588 | slice B — DRAFT, **plan only, no implementation**, based on A's branch | open |

**THE COVERAGE BLOCKER IS DISPOSITIONED, NARROWLY — read this before touching
`check-coverage`.** Maintainer-authorized 2026-08-02. `just check-coverage` no longer
uses `--fail-under-lines 100`; it compares llvm-cov's summary against its own
`--show-missing-lines` listing and caps ONLY the unnameable residue via
`tests/fixtures/coverage-unnameable-disposition.json` (mandatory reason, fork-drift
idiom), logic in `dev-tooling/coverage-gate.py`. **The 100% requirement for ATTRIBUTABLE
lines is UNCHANGED and § 0c is UNTOUCHED — a single nameable miss still fails.** Four
mutation demos red + green control; fail-closed on a missing fixture. Tracking `-3yx`.
**Slices B and C inherit it ONLY on the IDENTICAL signature; a nameable miss is ordinary
work.** The allowance may be REDUCED, never raised without new recorded authorization.

**NEXT ACTIONS, in order:**

1. **Merge #585** (supervisor arms merges), then rebase `feat/slice-b-action-invoker`.
2. **Build slice B** — `plan/01-action-registry-and-invoker/research/slice-b-build-plan.md`
   has the two cherry-pick conflict resolutions (both non-obvious) and the enumerated
   54-site slice-C removal list. Do not re-derive them.
3. **Then slice C.** Order A -> B -> C is maintainer-approved.
4. **DOGFOOD LEG IS HELD and lifts only when A AND B are BOTH MERGED** — B is currently a
   plan, so it is NOT satisfied. Then: fresh `just tui` build (the live cockpit binary is
   2026-07-30 and STALE), ONE client verified via `/proc/*/exe`, plugin-root override
   INSIDE the credential wrapper (`-pj5g3f` re-measured 2026-08-02: entry `[0]` still
   `58a6467325e7`, four builds behind), then
   `set-workflow-scope-override:...-ccycuk:citation-only` FROM THE COCKPIT -> drain at the
   TUI -> monitor -> `c`. `-ccycuk` is untouched at `ready` + `ai-then-human`.
5. **`SPECIFICATION/proposed_changes/` and `history/` were OFF-LIMITS at wind-down** — the
   maintainer's `revise` pass was running in the supervisor pane. Confirm it finished
   (BRIEF 18) before touching either.

**Also closed today:** `-6hbfq6` reconciled and CLOSED under a maintainer-authorized
reduced janitor argv (THIS ITEM ONLY, not standing policy; argv on the item). It went
straight to CLOSED rather than back to `acceptance`, so **it must NOT be counted as TUI
`c`-valve evidence.**

**Ledger records written today:** `-3yx` (filed: the coverage phantom, its falsifiable
prediction, the slice-A verdict, the INCONCLUSIVE compilation-count experiment, and the
disposition), plus findings on `-2ckgiy`, `-ekb5vq`, `-3lxx7t`, `-pj5g3f`, `-9ts`,
`-htp`, `-6hbfq6`.

**A measurement gotcha that cost two near-false findings — do not relearn it:**
`bd dep list <epic>` does NOT show parent-child CHILDREN. The edge is stored on the CHILD
and points up, so an epic with children can report "no dependencies". Closed items DO
appear, so absence reads convincingly as an orphan. Query the child.

---

**RESUME HERE (2026-07-29T22:47Z — HISTORICAL; superseded by the block above).**
Timestamps here are UTC; we run at UTC+2, so anything after 22:00 UTC dates to the
next LOCAL day — build every timestamp with `date -u`, never by hand (that mistake
cost PR #478).

### 0-SUCCESSION. THIS THREAD HAS A SUCCESSOR ARC — READ BEFORE ANY ARCHIVE

**Added 2026-08-02 on the maintainer's menu-primary decision.** This thread stays
**PARKED** until `plan/01-action-registry-and-invoker/`'s PR merges, then its archive is
PREPARED — with custody NAMED AND ACCEPTED first, never after.

The maintainer decided that menus and dialogs are the first-class, required, primary
navigation mechanism and hotkeys only an additional power-user convenience. Four
decoupled, numbered plans now carry this arc:

| plan | epic | owns |
|---|---|---|
| `plan/01-action-registry-and-invoker/` | `-dvv` | the registry + generic invoker; **banks the interim unbroken-pass evidence** |
| `plan/02-menu-shell-primacy/` | `-et3` | menus generated from the registry; generated docs |
| `plan/03-dispatch-ux-and-outcome-surfacing/` | `-1df` | drain off the UI thread; refusal surfacing |
| `plan/04-mvp-unbroken-walk-and-close/` | `-9nb` | the amended unbroken walk; **archive sequencing + final doc-custody home** |

**CUSTODY TRANSFERS, named so none is dropped silently:**

- **The walk deliverable → plan 04.** 01 banks the interim pass as its own dogfood leg;
  04 owns the menus-only unbroken pass and the close.
- **DOC CUSTODY → rides plan 02, final home decided by plan 04.** 02's
  registry-generated docs shrink it substantially; whatever remains becomes a standing
  item or a named successor. **This section's § "Doc custody" is STILL LIVE until 04
  says otherwise** — it is recurring work, not a dormant label, and deleting it without
  a successor is the specific outcome archival was conditioned on avoiding.
- **The staged asset → plan 01.** `-ccycuk` sits at `ready` with
  `acceptance_policy=ai-then-human`, untouched. **Do not re-stage it, do not move it, do
  not re-run `n`.** 01's dogfood leg resumes it through the new invoker.

**DO NOT ARCHIVE THIS THREAD BEFORE THOSE TRANSFERS ARE ACCEPTED.** "Another plan owns
it" is not a handoff; a handoff is complete only when the successor has confirmed it.

`plan/operator-surface-redesign/`: the menu-primary decision SATISFIES its brainstorm
entry gate (see `plan/01-action-registry-and-invoker/research/operator-surface-redesign-decision.md`).

**DECIDED AND DONE 2026-08-02 — this paragraph used to say the disposition "is with the
maintainer and is NOT this thread's call", which was true when written and is now FALSE.**
The maintainer ruled ABSORB AND ARCHIVE. The thread lives at
`plan/archive/operator-surface-redesign/`, with custody named and ACCEPTED before
archival: `-zweohm`/`-vc7lmq`/`-l4p3ce` to plan 02, `-ipi` to plan 03, and the cross-repo
verb-vocabulary dependency to plan 01 (whose parity fixture now consumes the
orchestrator's published surface mechanically). The ledger `parent-child` edges were
re-pointed, so the transfer is an edge and not prose. Epic `-6msemd`'s own closure is a
PREPARED maintainer decision, not taken.

### 0-RESUME. READ THIS FIRST — state at 2026-07-30T15:0xZ (supersedes the 14:0xZ block)

**EVERY LEG IS NOW INDIVIDUALLY WALKED, INCLUDING `c` ACCEPT. THE MISSION IS STILL
NOT DISCHARGED, AND THE GAP IS NOT A TECHNICALITY — READ THE SECOND TABLE.**

| leg | state |
|---|---|
| 1 — find + groom | **WALKED 2026-07-30 (attempt 4) — the groom RAN, on `-drn`.** § 0j. § 0i explains why the earlier "WALKED" was only the `h` handoff. |
| — | **§ 0j found the MISSION'S PREMISE is wrong: groomed slices are DISPATCHER-admitted, so "slices admitted at the approve valve" describes behaviour the system does not have. Maintainer-owned; raised, not decided.** |
| 2a — admit at `p` | **WALKED** on `-6zqv2w` — ledger `ready` |
| 3a — `n` set-acceptance | **WALKED** on `-6zqv2w` — ledger `ai-then-human` |
| 2b — dispatch (palette drain) | **WALKED WITH AN EXPLICIT PLUGIN-ROOT OVERRIDE** — § 0g. NEVER write this as a bare "WALKED". |
| 3b — `c` accept | **WALKED 2026-07-30T15:04Z — first time ever.** § 0h |

**WHAT IS STILL OWED, stated as the deliverable and not as a caveat.** Stage 3(b) is
**ONE CONTINUOUS SINGLE-ITEM WALK**: find → groom → admit → dispatch → monitor →
accept. That has NOT happened. What now exists is every leg proven individually, plus
one item (`-6zqv2w`) carried from `pending-approval` to `done` at the TUI. Three things
separate that from the deliverable, and none of them is cosmetic:

| gap | detail |
|---|---|
| **the groom leg never actually ran** | § 0i. Only the `h` HANDOFF was walked (on `-zweohm`, a different item, which produced NO slices). Since groom is the ONLY route from `backlog` to `pending-approval`, the walk has no spine without it. |
| **it was not continuous** | The pass spans THREE sessions and was interrupted twice by external failures (a dispatcher staleness refusal, then a red PR). |
| **three interventions were needed** | a plugin-root override to dispatch at all (`-pj5g3f`); a HAND FIX of the factory's red PR (#530); and `reconcile-merged` for post-merge bookkeeping the dead drain loop never did. |

**So the epic does NOT close on this.** Anyone reading the first table alone and
archiving the thread has thrown away the deliverable, exactly as § 0 warned about
"Stage 2 complete". The honest summary is: *the surface is proven; the unbroken pass
is not.*

**PR #530 IS MERGED — and it is NOT an unassisted factory landing.** The run
(`01KYSGS1CYKQSQTG6PFH9SR1YM`) implemented the slice, published the PR, and then
terminated `succeeded` while the PR was RED on `check-e2e-tmux` (13 other checks
green). Because the run was already terminal there was nothing to steer or retry, so
the final commit is a maintainer-authorised HAND FIX. Merged `27ccb73` at
2026-07-30T14:53:11Z. **Do not tally #530 as a clean factory landing** — the commit
message and a PR comment both say so, deliberately, because a future reader computing
factory success rates would otherwise count it.

The factory's miss was real and small: it correctly ADDED the `n set-acceptance` step
to the documented walk and updated the non-exhaustive sibling assertion at
`tmux_tui_e2e.rs:1053`, but not the EXHAUSTIVE ordered-action assertion at `:1169`.

    tmux_tui_e2e.rs:1169  assertion `left == right` failed
      left:  ["set-acceptance:...dummy1:ai-then-human", "approve:...dummy1", "accept:...dummy1"]
      right: ["approve:...dummy1", "accept:...dummy1"]

**A NEW FINDING THAT EXPLAINS HOW A RED PR GOT PUBLISHED AT ALL — file it if it is not
filed yet.** The janitor gate is `just check`, whose target list is THIRTEEN entries
(`check-format` … `check-fork-drift`) and **`check-e2e-tmux` IS NOT AMONG THEM**. CI
runs it; the janitor does not. So a run can go green through implement → janitor →
review → publish and open a PR that CI immediately fails. This will recur on every
slice touching the walkthrough. Verified by reading the `check:` recipe in `justfile`.

**A CORRECTION TO §§ 0e/0f THAT MUST NOT HARDEN INTO A FALSEHOOD.** Those sections say
`tmux_tui_e2e_lifecycle_walkthrough_two_repos` "structurally cannot catch" the
acceptance drift. That is true ONLY for what was tested — it scripts
`fixture.factory_move("acceptance")`, so it cannot see `acceptance_mode`. **It CAN
catch a change in the documented action sequence, and on 2026-07-30 it did exactly
that.** Its blind spot is narrow and specific. Do not read those sections as "that E2E
is useless".

**WHAT TO DO NEXT — THIS THREAD IS NO LONGER THE ACTIVE ONE. READ THIS FIRST.**

**Updated 2026-08-02 at wind-down. The work has MOVED to a new four-plan arc; do NOT
resume the old continuous pass from here.** The maintainer decided that menus and
dialogs are the first-class, required, primary navigation UX mechanism and hotkeys only
an additional power-user convenience. That decision reshapes the whole surface, so the
walk this thread owed is now plan 04's, on the new UX.

**GO HERE:**

| plan | epic | what it is |
|---|---|---|
| **`plan/01-action-registry-and-invoker/handoff.md`** | `-dvv` | **START HERE — full handoff, it is next** |
| `plan/02-menu-shell-primacy/` | `-et3` | charter; blocked by 01 |
| `plan/03-dispatch-ux-and-outcome-surfacing/` | `-1df` | charter; blocked by 01 |
| `plan/04-mvp-unbroken-walk-and-close/` | `-9nb` | charter; blocked by 02 AND 03 — owns the unbroken walk and this arc's archive |

Dependencies are LEDGER EDGES (`bd dep list`), not prose. If those plan directories are
absent from your checkout, **PR #569 has not merged yet** — check it
(`gh pr view 569`), and read this file's § 0-SUCCESSION if present.

**THE ONE THING NOT TO DISTURB.** `livespec-console-beads-fabro-ccycuk` sits at `ready`
with `acceptance_policy=ai-then-human`, staged deliberately since 2026-07-30 and
untouched. **Do not re-stage it, do not move it, do not re-run `n`, do not drain it from
here.** Plan 01's dogfood leg resumes it through the invoker it builds. It is currently
blocked by a factory-safety refusal whose remedy
(`set-workflow-scope-override:<id>:citation-only`) has no console binding — that is
`-w7d`, and 01 fixes it.

**Everything this thread learned is already banked** — §§ 0e–0j here, and on the ledger:
`-0uw`, `-w7d`, `-drn` (regroomed out), `-3tg`, `-htz`, plus escalations on `-pj5g3f`,
`-3lxx7t` and four instances on `-ectqye`. Nothing is owed that lives only in a
transcript.

**COCKPIT STATE AT WIND-DOWN.** tmux `happy-path-tui`, **PID 1883911**, ONE live
client, binary rebuilt **2026-07-30T14:52:52Z** from `be09e26` — current for all
runtime code (the only later commit, `27ccb73`, is test-only and does not enter the
release binary). Launched with the plugin-root override because **a DEFAULT OPERATOR
CANNOT DISPATCH ON THIS HOST TODAY** (`-pj5g3f`, unfixed — entry `[0]` of
`installed_plugins.json` is still `58a6467325e7`). The env var does NOT survive
`just tui`; it must be injected INSIDE the credential wrapper:

    /usr/local/bin/with-livespec-env.sh -- env \
      LIVESPEC_CONSOLE_ORCHESTRATOR_PLUGIN_ROOT=/home/ubuntu/.claude/plugins/marketplaces/livespec-orchestrator-beads-fabro \
      /data/projects/livespec-console-beads-fabro/target/release/livespec-console-beads-fabro serve

That resolves build **`eacbb88ead9c` (release 0.49.3)** — MEASURED from the marketplace
checkout's HEAD this session. § 0g's `0ea3e7bc5465` is STALE; do not re-inherit it.

**The cockpit is NOT frozen any more** — the drain loop returned on its own and the TUI
unfroze, so the previous section's "do not kill it" warning has served its purpose. If
you find it frozen again, that warning and its probe are still correct: check for a
live `dispatcher.py loop` behind it before deciding, and key the probe on `python3` so
it cannot self-match the shell running it (mine did).

If you relaunch, `ps` by `/proc/*/exe` first (NOT `ps | grep`, which self-matches),
keep exactly ONE client, and rebuild if any `console-*` crate SOURCE has moved — a
test-only commit does not require it, but verify rather than assume.

**LEDGER AT WIND-DOWN 2026-07-30T15:0xZ (re-measure — a claim with a timestamp):**
master `27ccb73`. `-6zqv2w` **`done`** (accepted at the TUI `c` valve, § 0h);
`-u3w3er` `done`; **`-6hbfq6` still `active` though `886011d` MERGED** — a second live
instance of `-3lxx7t`, owed a `reconcile-merged`. `pending-approval`: `-ectqye`
(LEAVE IT, § 3), `-ekb5vq`, `-pj5g3f`, `-2ckgiy`, `-3lxx7t`. `backlog`: the three new
items below.

**FILED 2026-07-30 (attempt 3), all with epic `tracks` edges:**

- **`-drn`** — `just check`'s 13-target list omits `check-e2e-tmux`, so a run can go
  green through implement -> janitor -> review -> publish and open a PR that CI
  immediately fails. This is exactly what happened to #530. Recipe read at source.
- **`-3tg` (P1)** — our fork's `workflow.toml` lacks upstream's
  `[run.integrations.github.permissions]` block, so Fabro builds no mintable token
  source and in-sandbox `gh` 401s past the ~60-min installation-token TTL. **This
  DIAGNOSES § 0d's recorded-as-unexplained 401**: upstream's own comment says `git push`
  is refreshed separately via origin-URL rotation while `gh` reads a static token minted
  once at dispatch — which is precisely the push-succeeded/gh-401 contradiction § 0d
  could not resolve. The gate's own rationale is "upstream once fixed the pr-stage
  publish leg and our fork silently kept the broken one for three weeks"; this is the
  publish leg AGAIN.
- **`-htz`** — our committed `review_adapter` names the DEPRECATED
  `@zed-industries/claude-code-acp`, whose `@latest` is frozen at a tombstone (0.16.2,
  2026-03-26), while `acp_adapter` runs the current package at 0.44.0 — the same adapter
  ~28 minor versions apart. Latent, because recent dispatches OVERRODE the adapter.

**`-pj5g3f` ESCALATED** with a new measurement: `installed_plugins.json` now holds
THIRTEEN records under one plugin key across SIX distinct builds, entry `[0]` unchanged.
It is no longer "entry [0] is stale" — the resolution surface is unbounded and
diverging, and it now blocks PUSHES as well as the drain.

**DO NOT PORT `-3tg`/`-htz` CASUALLY.** Both sit on maintainer-owned surfaces (dispatch
credentials; model adapter selection), and the adapter one collides with the standing
maintainer decision pointing `review_adapter` at Codex while the Claude subscription is
exhausted. They are filed, not fixed, deliberately.

**STANDING: this plan is PARKED WITH A NAMED SUCCESSOR ARC, NOT ARCHIVED, and NOT the
place to resume work.** Stage 2 is complete; every Stage 3(b) leg is individually walked
(`h` handoff, a real groom, `n`, a correct `p` refusal, `c` accept) and the ONE UNBROKEN
PASS was never achieved — it is now plan 04's deliverable, on the menu-primary surface.
**Do not archive this thread until plans 01/02/04 have ACCEPTED their custody transfers**
(walk deliverable → 04, doc custody → rides 02 with its final home set by 04, staged
asset → 01). "Another plan owns it" is not a handoff.

### 0. STAGE 2 IS COMPLETE. THE THREAD IS **PARKED, NOT DONE**. START HERE.

**Read this paragraph before anything else, because the obvious misreading is
expensive: "Stage 2 complete" is NOT "thread complete".** All five Stage-2 slices are
`done` and the maintainer's dispatch scope is fully discharged — and Stage 3(b), the
one-continuous real-stack walk that this whole epic exists to produce, **has not been
attempted**. The epic does NOT close until it passes (§ "The track", Stage 3). This
plan is **PARKED DELIBERATELY, NOT ARCHIVED.** Anyone who archives it on the strength
of the table below has thrown away the deliverable.

**Stage-2 scope, closed 2026-07-30 — all five, forge-verified:**

| slice | item | landed |
|---|---|---|
| A | `-dm5f7q` | accepted at the real TUI `c` valve |
| B (hints) | `-mbohw3` | PR #505 -> `514a326` |
| B1 | `-nvflph` | PR #509 -> `46783ad` |
| B2-B4 | `-vwxyj4` `-cyixzi` `-zvnjef` | verify-closed (fixed by slice A; see § 1) |
| C | `-cxu4eu` | PR #515 -> `21ff727` (hand-published, § 0d) |
| tier bug | `-ff6aue` | PR #517 -> `2132155`, auto-merged, 34 min |

**`-ff6aue` was RED-DEMONSTRATED, not taken on a green CI** — it was a
vacuous-verifier bug, so a passing build proves nothing about it. Run against the
item's OWN repro on current master: baseline `RC=0` ("behavioral coverage clean, 0
unlinked, 0 untested"); one TODO entry's reason set to `covers the acceptance lane` ->
`RC=1`, naming the scenario and reporting "1 untested scenario(s)"; tree restored
byte-identically. That exact input reported CLEAN before the fix. The new
`acknowledges_top_of_pyramid_tier` (`crates/console-spec-check/src/lib.rs:533-553`)
requires the reason to BEGIN with `Test tier:` and parses the first token as the
label, instead of substring-matching `integration`/`acceptance` anywhere — which is
why the old one could not fail on realistic input.

**WHAT IS LEFT, and it is only Stage 3(b) — three legs, one unbroken pass.** The legs
are individually proven; what has never happened is ONE continuous walk:

1. **find a backlog item and groom it** via the LLM-driver handoff. **WALKED
   2026-07-30 — see § 0e. This leg needs NO dispatcher and is NOT blocked; any
   earlier claim that it wants "the vocabulary ratification plus the `-l4p3ce`
   transport" is STALE and struck (§ 0e explains why the claim survived four days
   past its expiry).**
2. **admit it at the TUI `p` valve** -> `ready` -> **dispatch** (palette drain).
   Admission WALKED 2026-07-30; **the DISPATCH half is BLOCKED — § 0f.**
3. **press `n` set-acceptance BEFORE it reaches `acceptance`**, then monitor, then
   **`c` accept**. The `n` step is REQUIRED, not cautious — see § 0d for the live
   proof, and do not skip it. `n` WALKED 2026-07-30; **`c` is UNREACHABLE until the
   dispatch leg clears**, because nothing ever arrives at `acceptance`.

**COCKPIT HYGIENE IS A PRECONDITION OF THE WALK, NOT A CHORE.** Before any 3(b)
attempt: `ps` for stray `serve` processes FIRST — the single-operator MVP assumes
EXACTLY ONE live client, and a four-day-old binary was once caught still polling —
then a FRESH `just tui` build. Verify the binary is not older than any merge touching
a `console-*` crate; `cargo` correctly no-ops when only non-console crates moved, but
that must be VERIFIED rather than assumed. **Key the `ps` on `/proc/*/exe`, NOT on
`ps | grep`** — a grep for a console/serve string self-matches any supervisor watcher
whose argv carries it, which cost a false reading this session. The cockpit runs in
tmux `happy-path-tui` and is the PRODUCT, not an agent session.

**TWO ITEMS ARE WAITING AT THE APPROVE VALVE ON PURPOSE — do not sweep them.**

- **`-6zqv2w`** (the walkthrough doc-rot bug, § 0d) is `pending-approval`
  **deliberately**, maintainer-approved: admitting it at the TUI `p` valve IS a walk
  leg, and its subject matter is the very walkthrough 3(b) exercises, so admitting it
  there makes it evidence instead of overhead. Do not admit it from `drive.py`.
- **`-ekb5vq`** (the clippy/llvm-cov gate pincer, § 0c item 3) is `pending-approval`,
  filed 2026-07-30 with the measurements attached. Off the happy path; route it
  whenever, but it will bite the next slice that adds match arms.

`-ectqye` stays `pending-approval` with routing undecided (§ 3). `-u3w3er` and
`-6hbfq6` sit `ready`, off the happy path, operator choice.

### 0-historical. THE FACTORY WORKS. TWO SLICES LEFT. (superseded by § 0 above)

**State at wind-down, measured — re-measure anyway, it is a claim with a timestamp:**

| item | lane | note |
|---|---|---|
| `-dm5f7q` | **closed** | slice A; accepted at the real TUI `c` valve |
| `-mbohw3` | **closed** | PR #505 -> `514a326`; first Codex-reviewed slice |
| `-nvflph` | **closed** | B1; PR #509 -> `46783ad`; three review visits |
| `-vwxyj4` `-cyixzi` `-zvnjef` | **closed** | B2-B4, verify-closed (see § 1) |
| `-cxu4eu` | **`done`** | C — LANDED 2026-07-30, PR #515 -> `21ff727`. See § 0c. |
| `-ff6aue` | `ready` -> dispatched | **THE LAST STAGE-2 SLICE — IN FLIGHT, see below** |
| `-ectqye` | `pending-approval` | LEAVE IT — routing undecided, § 3 |
| `-u3w3er` `-6hbfq6` | `ready` | off the happy path; operator choice |

At wind-down: master `724b9e1`, primary clean, **0 fabro runs in flight, both host
slots free**, no cockpit running.

**RE-MEASURED 2026-07-29T23:0x–23:1xZ, and the table above held exactly** — every
lane, every closure. Master is now `5e91d0e` (the wind-down commit itself),
primary clean and level with the forge; no work-item has been filed against this
thread's surface since `-mbohw3` on 2026-07-28; no cockpit process was running
(checked by `/proc/*/exe`, NOT `ps | grep`, which self-matches a supervisor
watcher whose argv carries the search string); 362 fabro runs, all terminal, and
no admission slot locks — both host slots free.

**`-cxu4eu` IS DISPATCHED AND IN FLIGHT as of 2026-07-29T23:10:08Z** — run
`01KYR29FNM83F64C2ZGG7H06GF`, publish branch
`feat/livespec-console-beads-fabro-cxu4eu`, launched with the § 0 command below.
Verified before trusting it: the run's own spec carries
`"review_adapter": "npx --no-install @zed-industries/codex-acp"`, and
`git status --short .fabro/` is empty (the committed fork untouched — the copy
differed from it by that ONE line and nothing else). Exactly ONE run is in
flight, so a host slot remains for `-ff6aue`. When it lands, **check the review's
verdict FORM before its prose** per § 0 — `review -> pr` is the unconditional
fallback edge, so a malformed verdict publishes rather than failing.

Two cautions a successor should not have to rediscover. **`fabro ps --json`
reports `status` as an OBJECT (`{"kind": "running"}`), not a string** — a filter
that stringifies it matches no terminal kind and reports every run as in-flight;
key on `status.kind`. And **`npx --no-install @zed-industries/codex-acp` FAILS on
the host** ("missing packages and no YES option") while being correct in the run:
the adapter is baked into the `python-rust-agent-` sandbox layer, and
`--no-install` is deliberate so a regression to the slim CI image fails loudly
instead of silently re-downloading. Do not "fix" it to `-y` from a host probe.

**THE DISPATCH COMMAND — copy it, and read § 0b before changing any part of it:**

    P=/home/ubuntu/.claude/plugins/cache/livespec-orchestrator-beads-fabro/livespec-orchestrator-beads-fabro/856d699b5f7d
    # 1. Build an UNTRACKED copy of the workflow dir and point review at Codex.
    #    The WHOLE dir: `graph` is relative and the prompts hang off the graph.
    W=$(mktemp -d)/wf-codex-review
    mkdir -p "$W" && cp -r /data/projects/livespec-console-beads-fabro/.fabro/workflows/implement-work-item/. "$W/"
    sed -i 's|^review_adapter = .*|review_adapter = "npx --no-install @zed-industries/codex-acp"|' "$W/workflow.toml"
    # 2. Dispatch UNDER the credential wrapper, in the BACKGROUND (30-40 min).
    /usr/local/bin/with-livespec-env.sh -- "$P/scripts/bin/dispatcher.py" dispatch \
      --repo /data/projects/livespec-console-beads-fabro \
      --item livespec-console-beads-fabro-cxu4eu \
      --workflow "$W/workflow.toml" --json

Then verify `git status --short .fabro/` is EMPTY — the committed `workflow.toml` must
stay untouched — and confirm the run's spec carries
`review_adapter: npx --no-install @zed-industries/codex-acp` before trusting it.

**CHECK THE REVIEW'S VERDICT FORM FIRST, EVERY TIME, BEFORE READING ITS PROSE.**
`review -> pr` is the UNCONDITIONAL fallback edge (`workflow.fabro:285`) and only
`review -> review_fix` is guarded, so a MALFORMED verdict does not fail — it
PUBLISHES. Pull `stages/*review*/response.md` from a `fabro dump` and confirm the last
line parses as `{"preferred_next_label": "approve"|"fix"}`. Cheap, and silent when it
breaks. Codex has produced it correctly 4/4 times (approve, fix, fix, approve).

**WHAT THE REVIEWER IS WORTH, at n=4 — do not re-litigate this downward.** It
discriminates: it blocked `-nvflph` twice on real operator-visible contradictions, and
when `review_fix@1` silently failed to address a finding it RESTATED THE SAME FINDING
UNCHANGED at the same file:line. A reviewer that forgets what it asked for is worse
than none; this one does not. It under-grades advisories (it called one thing
"advisory" that was arguably blocking, and raised one advisory that MEASUREMENT
REFUTED — see § 4). An earlier revision of this file called it "adequate, not good"
from n=1; that was an over-read of one sample and is withdrawn.

**AND THE LESSON THAT PAID FOR ITSELF THREE TIMES TODAY:** every slice that narrows a
ratified vocabulary should ASSUME a second and third restatement of it exists elsewhere
and go hunting, rather than discovering them one review visit at a time. `-mbohw3` had
three encodings of the per-item verb vocabulary; `-nvflph` had four of the move-status
vocabulary (picker/handler, tests, Help modal, and the `WorkItemMoveRequested` domain
contract prose). In BOTH cases the implementation was closer to correct than its
descriptions were.

### 0e. STAGE 3(b) ATTEMPT 1, 2026-07-30 — THE `h` HANDOFF IS WALKED (first time ever)

> **SCOPE CORRECTED 2026-07-30, see § 0i — READ THAT BEFORE THIS.** This section
> originally read "LEG 1 IS WALKED". What it verifies is the **`h` DRIVER-HANDOFF
> OVERLAY**, exhaustively and correctly. It does NOT establish that a groom happened:
> `groom.py:292` files slices at `pending-approval`, and `-zweohm` produced none. The
> console's half of leg 1 is walked; the driver's half never ran. Every clause checked
> below still holds — only the LABEL was too wide.

**The driver-handoff overlay has been walked at the real TUI keyboard, on the real
stack.** It was never blocked. Recorded first because it is the leg this thread has
owed since it opened.

Preconditions verified, not assumed: no console `serve` process and no
`happy-path-tui` session at start (checked via `/proc/*/exe`, NOT `ps | grep`);
`just tui` REBUILT (24.65s, recompiling `console-tui` + `console-cli`) because the
existing release binary predated the Jul-30 console merges — **precondition (b) FAILED
on the pre-existing binary, so the rebuild was real work, not a no-op**; exactly ONE
live client throughout.

Walked on `-zweohm`, chosen deliberately: it is the item titled *"Lane items expose no
state-appropriate next action — groom is the natural verb for a backlog item but the UX
neither mentions nor enables it"*. Demonstrating the feature on the item that asked for
it is the cleanest available evidence that the gap is closed.

Captured live at the keyboard, in order:

    Lane: backlog [focus], selection livespec-console-beads-fabro-zweohm
    hint BEFORE the keypress:
      up/down move | enter item | esc lane list | h handoff | s move-status |
      m set-admission | g merge cap | f fix cap | n set-acceptance | k rework cap |
      ? help | q quit
    after `h`:
      ┌Driver Handoff───────────────────────────── (full pane width) ─┐
      │claude "/livespec-orchestrator-beads-fabro:groom livespec-console-beads-fabro-zweohm"
      │enter copy sent to terminal | esc cancel
    after Enter: overlay closed, band restored, ledger lane STILL `backlog`

Every clause of `SPECIFICATION/contracts.md:690` checked against that render:
backlog -> **groom** invocation; **id-only** (no prompt file); **full-width** overlay;
wording describes what the console did and **`Copied` appears zero times**; the id is
exactly the selected one; and the lane does NOT move, which matches the ratified rule
that **groom needs no door** — a groomed item stays `backlog` throughout.

**WHY THIS LEG SAT "BLOCKED" FOR FOUR DAYS ON A FALSE PREMISE — the most reusable
lesson of the day, and it is a fresh instance of this thread's own canonical rule.**
The blocker was inherited: the vocabulary was said to be unratified. It was ratified
on 2026-07-26 (orchestrator PR #975, its v050), and the transport decisions were cut
into OUR OWN contract as console v037 and IMPLEMENTED by `-cxu4eu`. The check that
produced the false blocker looked in the orchestrator's
`SPECIFICATION/proposed_changes/`, found nothing relevant, and concluded "not
authored". **But an EMPTY `proposed_changes/` is the signature of work that LANDED** —
the revise pass MOVES a ratified proposal into `history/vNNN/proposed_changes/`. The
strongest evidence of completion was read as evidence of absence. That is § 6's *"an
absence never announces itself in a check aimed at the wrong token"*, in its purest
form yet: the check was aimed at the wrong DIRECTORY, and the directory's emptiness
meant the opposite of what it was taken to mean. Diagnosed and owned by the supervisor.
Its sibling cause: `research/verb-vocabulary-brainstorm.md` says "verified 2026-07-25
… has not been authored there yet", which was TRUE when written and went stale the next
day — **a dated verification treated as a standing fact.**
`-l4p3ce` sitting `backlog` is unbuilt RESIDUE (the in-app suspend/spawn survey), not
unauthored design.

### 0f. STAGE 3(b) ATTEMPT 1 — THE DISPATCH LEG IS BLOCKED, TWICE, AND THE COCKPIT SAYS NOTHING

**Legs walked and ledger-verified:** `n` set-acceptance on `-6zqv2w` ->
`acceptance_policy='ai-then-human'`; `p` approve on the same item -> `lane='ready'`, no
silent failure. Each valve's `Target:` line was read back before Enter, and the
pending-approval hint matched `detailed-usage.md:267` exactly (`p`/`r`/`m` present,
**no `c accept`**). `n` was pressed at `pending-approval` — the EARLIEST valid point —
which removes the `ai-only` auto-complete race rather than racing it.

**The drain was refused twice and the operator saw nothing either time.** Second
attempt, journal record written two seconds after the keypress (Enter 11:21:38Z):

    {"at":"2026-07-30T11:21:40Z","blocking":true,
     "stage":"dispatcher-staleness-refused",
     "detail":"executing build 58a6467325e7 predates latest release 0ea3e7bc5465"}

**This is a SHARPER finding than `-ectqye`/`-k0w` as filed, and the distinction changes
the fix.** It is not "failed commands persist no diagnostic". The diagnostic EXISTS,
carries `blocking: true`, and sits in the dispatcher journal that the console ACTIVELY
INGESTS as its `dispatcher` source adapter — the Stored-events count rose 1566 -> 1579
across the attempt, so ingestion was live. And still: no error, no banner, no attention
row, and a screen-wide grep for `stale|refus|error|fail` returned **0 hits**. The gap
is purely presentational. Full evidence is on `-ectqye` as a dated comment.

**A SECOND, STRUCTURAL half — while ANY `pending-approval` item exists, a fully
successful drain ALWAYS reports failure.** `is_dispatch_candidate`
(`_dispatcher_loop_selection.py:126-145`) deliberately admits `pending-approval` items
as candidates by projecting them to `ready`; a manual item is then HELD, and
`admission_held_outcome` (`_dispatcher_admission.py:172-188`) returns `status="failed"`
with its docstring stating the intent outright — *"so the dispatch exit code flips to 1
and the maintainer's eyes are required"*. The console maps ANY non-zero probe to
`FactoryDrainPortOutcome::failed()` and renders nothing. Measured: the candidate set
was SIX (`6hbfq6`, `6zqv2w`, `ectqye`, `ekb5vq`, `u3w3er`, `xmcau7`), two of them
pending-approval. **Do NOT over-read this: the admission gate itself is INTACT** —
manual items are correctly held and never launched, so the approve valve is respected.
The defect is a SEAM between two behaviours each correct in isolation, which is a
different fix from either side alone.

**ROOT CAUSE OF THE REFUSAL IS THE CONSOLE'S PLUGIN RESOLUTION, and
`claude plugin update` CANNOT fix it.** `~/.claude/plugins/installed_plugins.json`
holds an ARRAY of records under one plugin key with MIXED versions; the console took
entry `[0]` = `58a6467325e7` (stale), while the update wrote `1fc573da09c5` into other
entries and the then-latest release `0ea3e7bc5465` was installed nowhere. Proven
resolution-specific rather than environmental: the marketplace checkout's dispatcher
PASSED the same gate, exit 0 read UNPIPED, same repo and argv shape. Filed as
**`-pj5g3f`**.

**TWO MORE THINGS THE WALK PRODUCED, both filed:**

- **`-2ckgiy`: the `h` driver-handoff verb shipped COMPLETELY UNDOCUMENTED.**
  `grep -c "h handoff" docs/detailed-usage.md` = 0, and `docs/` never mentions the
  overlay or OSC 52. The live backlog hint carries `h handoff`; the documented row
  (`:272`) is the same string without it. **No gate can catch this** — the lockstep
  value arm is deliberately doc-subset-of-source, and the completeness arm only
  requires the ten CONTEXTS to have rows, which the backlog row does. Third recorded
  instance of the same shape.
- **A selection hazard, reproduced.** Completing a valve makes the row LEAVE the
  Attention list, and the selection silently lands on the NEXT row — which here was
  `-ectqye`, the one item under a do-not-press rule. Nothing was typed to move it.
  Reading the Detail pane's `Work item:` line before every press is what prevented
  admitting the wrong item; this is why that discipline is not ceremony.

### 0g. ATTEMPT 2 — LEG 2b IS WALKED **WITH AN EXPLICIT PLUGIN-ROOT OVERRIDE**

**Read the qualifier as part of the claim, not a footnote. A DEFAULT OPERATOR CANNOT
DISPATCH ON THIS HOST TODAY.** `-pj5g3f` is the reason and it is unfixed. The override
below ROUTES AROUND that defect without repairing it; anyone reading this section as
"the dispatch path is healthy" has misread it.

Maintainer-authorized 2026-07-30 after the § 0f refusal. Reproducible exactly:

    /usr/local/bin/with-livespec-env.sh -- env \
      LIVESPEC_CONSOLE_ORCHESTRATOR_PLUGIN_ROOT=/home/ubuntu/.claude/plugins/marketplaces/livespec-orchestrator-beads-fabro \
      /data/projects/livespec-console-beads-fabro/target/release/livespec-console-beads-fabro serve

resolved build **`0ea3e7bc5465`** (the marketplace checkout — the same build the § 0f
refusal named as "latest release"), verified to pass the staleness gate at exit 0 read
UNPIPED before relaunching.

**THE OVERRIDE IS UNREACHABLE THROUGH `just tui`, AND THAT IS ITS OWN DEFECT.**
Exporting the variable and then running `just tui` does NOT work: the recipe launches
`with-livespec-env.sh -- <binary> serve`, and that credential wrapper has a FAIL-CLOSED
env allowlist that does not include `LIVESPEC_CONSOLE_*`. Verified both directions by
reading `/proc/<pid>/environ` of the live `serve` process — ABSENT when exported ahead
of the recipe, PRESENT when injected INSIDE the wrapper with `env`. So the console's own
documented knob (`docs/cli-options.md:82`) silently does nothing when set the obvious
way, with no error and no warning. Recorded on `-pj5g3f`, which that upgrades from a
resolution-order bug to a documented-option-that-cannot-be-used defect.

**The drain then worked, at the TUI, with no fallback:** `:` palette -> `drain` ->
Enter at 11:52:21Z; journal `dispatch-id` for `-6hbfq6` at 11:52:47Z with
`workflow_toml` pointing at our committed fork, so the run inherits the
`commit_timeout = "10m"` ported in `da2d1eb`. **No `drive.py`, nothing dispatched
outside the cockpit.**

**A PRECISION FINDING THAT CAUGHT ME MID-CLAIM, and the correction is part of the
evidence.** I reported that the walk subject "has been dispatched". The artifact check
refuted my own claim: `-6zqv2w`, `-u3w3er` and `-6hbfq6` were ALL `active` on the
ledger, while exactly ONE run existed (`01KYSDXENCZSF5A17TASS9RNRC`, `-6hbfq6`) and
only ONE `dispatch-id` record had been written. **The drain flips every picked item to
`active` at CLAIM time and launches them SERIALLY, so `active` means CLAIMED, not
EXECUTING.** Three `active` rows, one process. That is a live reproduction of exactly
what `-6ma` diagnosed — and `-6ma` was CLOSED as mis-filed in this tenant while the
behaviour demonstrably persists, which is how knowledge gets lost. Filed fresh with
today's artifacts as **`-3lxx7t`**. The rule that caught this is the standing one:
*requested is not dispatched; the dispatch leg is done when an implementation exists.*

**STATUS OF 2b/3b AT THE TIME OF WRITING: IN PROGRESS, NOT COMPLETE.** The drain is
walked. `-6zqv2w`'s own run had not launched — it is queued behind `-6hbfq6`, and the
cockpit is frozen inline (`-htp`) until the whole drain returns. `c` accept is NOT yet
pressed and MUST NOT be recorded as walked until an implementation exists and the item
RESTS at `acceptance`. **If `-6zqv2w` reaches `done` without ever presenting the `c`
valve, the `ai-then-human` override did not hold and that is a FINDING to report, not
to paper over.**

**Honest status of the pass: ATTEMPT 1 WAS INTERRUPTED BY AN EXTERNAL DISPATCHER
REFUSAL, NOT COMPLETED.** Legs 1, the admit half of 2, and the `n` half of 3 are
walked with captured evidence. The dispatch half of 2 is an OPEN leg — and it was NOT
driven around: no `drive.py`, no external dispatch. `c` accept remains unreachable
until dispatch clears. **`-xmcau7` IS dispatched but that was NOT this walk** — its
journal record is `11:16:18Z, budget: 1`, five minutes before the keypress, and the
console pushes `--budget 50`; it is another track's work on this host.

**A NEW MEASURED CAVEAT ABOUT THE STATUS BAND, because this thread hands out "read the
Status band" as advice.** The band lagged a closed modal by ~2s (the poll interval), so
a capture taken immediately after confirming showed MODAL hints while no modal was
open. Reading the band alone would have implied an open modal. **The modal's own text
is the reliable signal; the band is eventually-consistent.**

### 0j. ATTEMPT 4, 2026-07-30T16:39–18:1xZ — THE GROOM RAN FOR THE FIRST TIME, AND IT BROKE THE MISSION'S PREMISE

**A real groom has now been executed in this tenant — the half § 0i said had never
run.** Subject `-drn`, chosen with the maintainer. What it produced invalidates a
sentence the Mission has carried since this thread opened.

**THE MISSION'S PREMISE IS WRONG, AND THIS IS THE SESSION'S MOST IMPORTANT FINDING.**
§ "Mission" says the walk is *"groom (via LLM-driver handoff) → **slices admitted at
the approve valve** → ready → dispatched"*. Measured, on the two slices the groom
actually filed:

| slice | landed at | `admission_policy` | approve valve |
|---|---|---|---|
| `-ccycuk` (A) | **`ready`** | — | never offered; bypassed the valve entirely |
| `-koykn7` (B) | `pending-approval` | **`auto`** | **CANNOT fire** — Dispatcher owns admission |

`can_approve_item` (`_drive_valve_predicates.py:26-31`) requires
`effective_admission_policy == manual`. Measured against siblings for contrast:
`-2ckgiy` and `-ectqye` are `None` → `manual` and ARE approvable; `-koykn7` is `auto`.
`awaits_dispatcher_admission` (`:34-39`) names precisely this state — *"the Dispatcher,
not a human approve valve, owns admission"*.

**So groomed slices are DISPATCHER-ADMITTED BY DESIGN, and the walk as SPECIFIED cannot
be performed.** This is not a leg failure, not a console bug, and not something to route
around: it is the specification describing behaviour the system does not have.
**Redefining what the walk must prove is the MAINTAINER's call** — raised with them via
the supervisor, and deliberately NOT decided here.

**THE `p` REFUSAL WAS CORRECT. Record it that way, never as a blocked leg.** Pressed
twice on `-koykn7` (18:03:50Z, 18:06:31Z), valve opened with the right `Target:` both
times, ledger unchanged both times. The orchestrator's own answer on the documented
`--json` surface:

    {"action_id": "approve:livespec-console-beads-fabro-koykn7",
     "domain_error": "invalid-source-state", "status": "failed",
     "summary": "approve requires an effective-manual pending-approval item."}

**Nothing was driven around the console** — that `drive.py` call was DIAGNOSIS and it
RETURNED FAILED, so no workaround was applied and `-koykn7` sits exactly where the TUI
left it.

**Legs walked in this attempt, in one continuous sitting:**

    16:40:10Z  `h` on -drn (backlog)  -> Driver Handoff overlay, full-width, id-only:
                 claude "/livespec-orchestrator-beads-fabro:groom livespec-console-beads-fabro-drn"
               Enter -> overlay closed, band restored, ledger lane STILL backlog
    ~17:5xZ    GROOM RAN. Two slices drafted, maintainer approved the cut and the
               slice-B decision rule. -drn regroomed OUT -> `done` (escalate-don't-drop).
    18:02:38Z  `n` on -ccycuk (ready) -> "Set acceptance work-item /
                 Target: ...-ccycuk / Policy-mode: ai-then-human" -> ledger
                 acceptance_policy=ai-then-human, lane still `ready`
    18:03:50Z  `p` on -koykn7 -> correctly refused (above)

**THE RESUMABLE ASSET — DO NOT RE-STAGE IT.** `-ccycuk` sits at **`ready` with
`acceptance_policy=ai-then-human`**, which is exactly the state the drain leg needs. Its
staged state IS the asset. Do not move it to another lane, do not re-run `n`, and do not
re-groom `-drn` (it is `done`; re-grooming would refuse anyway — groom only accepts a
`backlog` target).

**TWO DEFECTS FILED FROM THIS ATTEMPT:**

- **`-0uw`** — the Status band ADVERTISED `p approve` on the auto-admission `-koykn7`,
  so the operator was invited to press a key that could not work and then told nothing
  when it didn't. The per-item verb predicate keys on LANE but not on admission policy,
  breaking the exact rule `-dm5f7q`/`-mbohw3` exist to enforce. `awaits_dispatcher_admission`
  already models the state; the hint layer simply does not consume it.
- **`-ectqye` third instance, and materially the strongest.** The first two were
  drain-path. This is the human APPROVE valve, and the payload is not merely absent from
  the screen — it is **well-formed, structured, on the documented surface, and dropped**.
  That upgrades the item from "no diagnostic exists" to "the diagnostic exists and is
  discarded", which is a different and easier fix.

**THREE DIFFERENT PLUGIN BUILDS WERE RESOLVED INSIDE THIS ONE WALK** — more `-pj5g3f`
evidence, and worse than the filed form: the console resolves entry `[0]`
`58a6467325e7`; the operator override resolves `eacbb88ead9c` (0.49.3); and the `groom`
SKILL binding resolved a third, `1fc573da09c5`, from the cache tree. One operator, one
task, three builds.

**A PROSE/CODE DRIFT worth one line:** `prose/groom.md` § Step 3's example calls
`file_approved_slices(path=, regroom_item_id=, slices=)`, but the function additionally
requires keyword-only `local_repo`. The example as written raises `TypeError`. Minor,
but it is the plugin's own LLM-facing driving prose, so it misleads every runtime.

**THE DRAIN LEG IS BLOCKED — `-ccycuk` IS HOST-ONLY-REFUSED, AND THE COCKPIT SAID
NOTHING (FOURTH TIME TODAY).** Palette drain at 18:32:51Z on a fresh binary, one client.
The drain RETURNED (it did not even freeze the cockpit) and `-ccycuk` stayed `ready`.
Journal, 11 s later:

    {"stage": "loop-pick", "budget": 50, "picked": [], "dry_run": false}
    stage: host-only-refused  status: failed
    detail: "factory-safety refusal: ...-ccycuk ... declares an edit under
      .github/workflows/, a withheld sandbox capability. It MUST NOT be dispatched to a
      fabro sandbox ... If the workflow path is citation-only, set the recorded override
      with `set-workflow-scope-override:<id>:citation-only`, or add an inline negation
      declaration stating that the item ships no files under .github/workflows/."

**THIS IS PARTLY MY OWN AUTHORING AND THAT IS THE INTERESTING PART.** Slice A's
description says, deliberately and prominently, *"this slice READS
`.github/workflows/ci.yml` and MUST NOT create or update any file under
`.github/workflows/`"* — which is exactly the "inline negation declaration" the refusal
says would satisfy it. It was refused anyway. **So the factory-safety check appears to
fire on the PATH MENTION rather than the INTENT, which inverts the incentive: it
rewards not writing the constraint down.** Filed as part of `-w7d`; the matcher lives in
`_dispatcher_host_only.py` and should be MEASURED before anyone "fixes" it.

**AND THE REMEDY IS UNREACHABLE FROM THE COCKPIT.**
`set-workflow-scope-override:<id>:citation-only` is a HUMAN VALVE
(`_drive_policy_valves.py:128`) and the console binds NO KEY to it — the per-item verb
set is `p r m n k g f s h`. So an operator whose dispatch is refused for factory-safety
cannot clear it without leaving the cockpit. Filed as **`-w7d`**.

**THREE DISTINCT SILENT REFUSAL PATHS ARE NOW ON RECORD IN ONE DAY**, all invisible at
the cockpit: `dispatcher-staleness-refused` (§ 0f), `human-valve` `invalid-source-state`
(the `p` press above), and `host-only-refused` (this one). All four instances are on
`-ectqye`, whose framing should harden: the diagnostics are RICH, ACTIONABLE and ALREADY
WRITTEN — one of them literally names the command that would unblock the operator — and
they are discarded at the presentation boundary. That is a smaller fix and a bigger
payoff than "failed commands persist no diagnostic" suggests.

**STATE OF THE PASS: legs 1 (find), groom, and `n` are WALKED; `p` was correctly refused
(not a leg failure); the DRAIN leg is BLOCKED on `-w7d` and `c` is unreachable behind
it.** `-ccycuk` remains `ready` + `ai-then-human` — the staged asset. Do not re-stage it.

**PR #543 MERGED (`bb53f53`) during this attempt** — the fork sync carrying upstream's
GitHub token-refresh block, i.e. the fix for § 0d's 401. **A dispatch started AFTER this
commit is materially safer than one started before**, because a run whose `pr` node is
reached past the ~60-min installation-token TTL should no longer 401. `-3tg`/`-htz`
should be read as superseded-pending-verification by it, not as open.

### 0i. THE GROOM LEG IS ONLY HALF WALKED — and groom is the walk's SPINE, not a nicety

**§ 0e records leg 1 as "WALKED". That claim covers the `h` HANDOFF OVERLAY only. The
groom itself never ran, and no groomed slice has ever existed in this tenant.** This is
this thread's own named honesty hazard — a leg recorded as walked when only part of it
was — and it is recorded here rather than quietly amended because the mission depends
on it.

**The mission has always said so, in its own words** (§ "Mission"):

> groom (via LLM-driver handoff) → **slices** admitted at the approve valve → ready →
> dispatched (palette drain) → active/monitored → acceptance → accept → done.

**Slices.** The thing admitted at the `p` valve is the OUTPUT of a groom, not the
groomed item.

**The evidence, four links, each checked at source 2026-07-30:**

1. `groom.py:292` (orchestrator, marketplace `eacbb88ead9c`) constructs each approved
   slice with `status="pending-approval"`. **Groom is the producer of approve-valve
   work.**
2. `-zweohm`, § 0e's groom subject, is **still `backlog`**, has **zero** items depending
   on it, and its only edge is `parent-child` to `-6msemd`. **No slice was ever filed.**
3. `status_move_targets(Lane::Backlog) => &[Lane::Ready, Lane::Blocked]`
   (`console-application/src/lib.rs`) — and `drive.py --action move:` accepts only
   `backlog|ready|blocked`. **There is NO operator route from `backlog` to
   `pending-approval`.**
4. Therefore groom is **load-bearing for the walk's spine**. Without it the `p` approve
   leg has nothing to act on that originated from a backlog item, and the walk cannot be
   continuous by construction.

**WHAT § 0e DID PROVE, and it is worth keeping.** Every clause it checked against
`SPECIFICATION/contracts.md:690` is valid and independently useful: the overlay renders
full-width, emits `claude "/livespec-orchestrator-beads-fabro:groom <id>"` id-only with
no prompt file, the id matches the selection, `Copied` appears zero times, and the lane
correctly does not move. **The CONSOLE's half of leg 1 is genuinely walked.** What was
never done is the DRIVER's half — opening that session, running the groom, and approving
a cut.

**WHY THAT HALF IS NOT A FORMALITY.** `/livespec-orchestrator-beads-fabro:groom` is a
"read-only drafting conversation — the maintainer OWNS the cut and the acceptance; the
front-end drafts and files NOTHING until approval". So the unbroken pass contains a
**maintainer-owned decision point** partway through, which no worker session can
discharge alone. Any plan for the continuous walk has to schedule that, not assume it.

**HOW THE OVERSTATEMENT HAPPENED, because the mechanism recurs.** § 0e verified the
console-side contract exhaustively and truthfully, then named the result by the LEG
("the groom leg has been walked") rather than by what was verified ("the groom HANDOFF
renders and emits correctly"). The lane not moving was read as confirmation — "a groomed
item stays `backlog` throughout" — when it is equally consistent with no groom having
happened at all. **A check that cannot distinguish success from absence confirms
nothing**, which is this track's dominant defect class (correct-looking state that
nothing was checking) reappearing inside its own evidence.

### 0h. ATTEMPT 3, 2026-07-30 — THE `c` ACCEPT LEG IS WALKED, AND `ai-then-human` HELD

**The last leg is walked at the real TUI keyboard, on a fresh binary, with exactly one
live client.** Captured live in order, never reconstructed afterwards.

Preconditions verified, not assumed. The drain loop (PID 3514295) had RETURNED, so the
cockpit unfroze on its own — the maintainer's "frozen cockpit is expected" addendum
was correct end to end, and killing it would have stranded three items. The running
binary was then STALE: `c540a96` (`-u3w3er`) and `886011d` (`-6hbfq6`) had merged after
it was built, so it was rebuilt (a REAL rebuild — it recompiled `console-tui` and
`console-cli`) and relaunched. Exactly ONE client throughout, checked by `/proc/*/exe`.

**The relaunch override, MEASURED not inherited — and the inherited value was stale:**

    /usr/local/bin/with-livespec-env.sh -- env \
      LIVESPEC_CONSOLE_ORCHESTRATOR_PLUGIN_ROOT=/home/ubuntu/.claude/plugins/marketplaces/livespec-orchestrator-beads-fabro \
      /data/projects/livespec-console-beads-fabro/target/release/livespec-console-beads-fabro serve

resolves build **`eacbb88ead9c` (release 0.49.3)**, read from the marketplace
checkout's own HEAD. § 0g records `0ea3e7bc5465`; that is now WRONG and re-inheriting
it would misdate the evidence. **A DEFAULT OPERATOR STILL CANNOT DISPATCH ON THIS HOST**
— `installed_plugins.json` entry `[0]` is still `58a6467325e7`, unchanged since
`-pj5g3f` was filed. The override routes around that defect; it does not repair it.

**Captured at the keyboard, in order:**

    BEFORE the keypress — selection on the Acceptance review row:
      Detail : Work item: livespec-console-beads-fabro-6zqv2w
      Status : up/down move | enter open | c accept | r reject | ? help | q quit
      header : attention: 71
    15:03:56Z  `c`  ->  ┌Valve┐ Accept work-item
                        Target: livespec-console-beads-fabro-6zqv2w
                        Enter to confirm | Esc to cancel
    15:04:07Z  Enter -> modal closed, no error
    15:04:32Z  ledger: lane=done status=done (was acceptance/ai-then-human at 15:01:07Z)
               header: attention: 71 -> 70, the Acceptance review row gone

**THE `ai-then-human` OVERRIDE HELD — this was the named failure case and it did not
fire.** § 0g warned that if `-6zqv2w` reached `done` without ever presenting the valve,
that was a FINDING. It did not: it RESTED at `acceptance` (ledger-verified at
15:01:07Z, and the reconcile's own surface line said "parked in acceptance under
acceptance_policy ai-then-human — awaits a human's final acceptance before done"), the
`c accept` hint appeared, the valve opened on the right target, and the press moved it.
So the `n set-acceptance` step § 1 added to the walk is now proven end-to-end on a real
item, not merely reasoned from source.

**A DOC-CUSTODY SPOT CHECK PASSED ON THE EXACT ROW THE WALK USES.** The live Status
band equalled `detailed-usage.md:268` byte for byte
(`up/down move | enter open | c accept | r reject | ? help | q quit`). Compare the
blocked row selected moments earlier, which correctly showed no `c`.

**THE SELECTION HAZARD REPRODUCED AGAIN, third recorded time.** Completing the valve
made the row leave the Attention list and the selection silently landed on the NEXT
row — `-ectqye`, the one item under a standing do-not-press rule. Nothing was typed to
move it. Reading the Detail pane's `Work item:` line before every press is what keeps
this from becoming a wrong-item valve press; it is not ceremony.

**`reconcile-merged` WAS NEEDED, AND WHY THAT IS NOT "DRIVING AROUND THE CONSOLE".**
The drain loop died before the merge, so nothing did post-merge bookkeeping and
`-6zqv2w` sat `active` with a merged PR. The console offers NO door out of `active`
(`status_move_targets(Lane::Active) => &[]`, ratified — the factory owns that lane), so
this is the sanctioned guarded door from § 0d, used before for `-cxu4eu`. It returned
`stage: done, status: green, "merged, post-merge janitor green"`, merge_sha `27ccb73`.
**The accept VALVE itself was pressed at the TUI** — no `drive.py`, nothing accepted
outside the cockpit. Bookkeeping through the documented door is not the same as
discharging a leg outside the surface, and the two must not be blurred.

### 0b-bis. `-ff6aue` LANDED — and it is the natural experiment on the 401

Dispatched **2026-07-30T01:19:13Z**, run `01KYR9NWQYTZMTNJK951B3V37J`; **auto-merged as
PR #517 -> `2132155` and returned `stage: done, status: green` ("merged, post-merge
janitor green") with dispatch RC=0.** Total elapsed **34 minutes**.

**That 34 minutes is evidence, so record it as such.** Slice C ran **1h54m** and lost
its `pr` node to `HTTP 401: Bad credentials`; this run reached `pr` well inside an hour
and published without incident, on the same adapter, same workflow copy, same host,
same credentials path. That is consistent with § 0d's expired-installation-token
hypothesis and is the closest thing to a controlled comparison available without
instrumenting the sandbox. It is still NOT proof — n=1 each side, and § 0d's
contradicting evidence (the `git push` succeeded in the same stage that 401'd) is
unexplained either way. **Practical rule, which holds regardless of the mechanism: a
run that passes roughly an hour before reaching `pr` should be EXPECTED to need the
by-hand publish recovery, and that recovery costs about ten minutes.**

### 0c. Slice C landed — and the four things it taught that will recur

`-cxu4eu` is `done`: PR #515 -> `21ff727`, four substantive commits, all 13 CI checks
green, reconciled through `dispatcher.py reconcile-merged` ("merged, post-merge janitor
green"). The reviewer caught two real things — `7b2ddd6` restricted the handoff to the
host-only set (the item's own forbidden-widening clause) and `21ff727` aligned the Help
modal, which is the § 0 "assume a second restatement exists" lesson landing for the
THIRD slice running. Contract clauses verified in the diff, not assumed:
`Lane::Backlog => "groom"`, `Lane::Ready if factory_safety.is_some() => "implement"`,
suppressed elsewhere, overlay wording `enter copy sent to terminal` carrying an explicit
NEGATIVE assertion that `Copied` is absent, OSC 52 a fire-and-forget stdout write.

**1. THE COVERAGE GATE IS CORRECT. Do not adjust it, and do not re-litigate this.**
`implement` escalated asking for "maintainer guidance on whether to adjust the coverage
gate/tooling behavior or accept the implementation with the coverage-gate anomaly". The
answer is NEITHER, and one control measurement settles it — same command, same base:

    clean 5e91d0e, no diff : TOTAL 20700 lines, 0 missed, 100.00% -> RC=0 GREEN
    + the implement diff   : TOTAL 21055 lines, 5 missed,  99.98% -> gate FAILS

Master satisfies `--fail-under-lines 100` EXACTLY, with zero missed lines. A diff that
fails it introduced its own misses. **ALWAYS RUN THAT CONTROL** before believing any
future "the coverage tooling is anomalous" claim — it converts an unfalsifiable
complaint into a two-minute measurement.

**2. `cargo llvm-cov report --show-missing-lines` is the tool. The implement stage
burned most of a 1h51m run never finding it**, fighting lcov, JSON and HTML instead.
It names ordinary misses in one shot. Its limit, which is itself the diagnosis: it
listed only ONE of the five. The other four were in `console-application` reporting
`LF=7438 LH=7434` while carrying only 7357 `DA` records, NONE zero-count and no
duplicates — 4 lines counted in the line summary that map to no listable zero-hit
source row (expansion/region-attributed accounting). Still caused by the diff, so still
closable by changing what the new code executes. The one ordinary miss was the `?`
error edge of `render_to_text(...)?` inside a TEST — a class the same run had already
fixed once and left a second instance of.

**3. A GENUINE PINCER BETWEEN TWO OF OUR OWN GATES — this will bite again.** Splitting
grouped or-pattern match arms to win llvm-cov line coverage trips clippy
`match_same_arms` at the commit hook (measured: it forced a revert mid-run). The
clippy-satisfying shape leaves an unexercised alternative that llvm-cov counts as a
missed line. **The shape that satisfies BOTH is to EXERCISE the untaken alternative
with a test** — never to reshape the match and never to weaken either gate. Not filed
as a work-item yet; it cost this run most of its wall-clock and deserves one.

**4. `[R]` ROUTES TO `fix`, NOT TO THE FAILED NODE.** `escalate -> fix
[label="[R] Retry the fix"]` — so when the `pr` node is what failed, there is NO option
that retries publishing. `[R]` re-runs fix -> janitor -> review -> pr, i.e. minutes of
rework on already-green code before it reaches the thing that actually broke. Read the
graph before assuming a retry resumes where the failure was.

**What DID work, and it is worth repeating verbatim: answer the gate, then steer.**
`[R]` was answered and the diagnosis above was pushed in with
`fabro steer <run> --text-stdin` (confirmed by a `run.steer` event). The janitor then
went green three times and review approved. A steer is how an operator gets knowledge
into a run that the dispatch goal did not carry — a `bd` comment added AFTER dispatch
does NOT reach the running run, only the next one.

**A LABEL THAT LIES: the run's own progress output says "Review (Claude Opus 4.8)"
while the review actually ran on the Codex adapter.** That string is static graph text,
not the resolved adapter. Trust the run spec's `review_adapter`, never the node label.

### 0d. The publish leg failed on credentials, and the branch survived it

The `pr` node pushed `feat/livespec-console-beads-fabro-cxu4eu` successfully — pre-push
`just check` passed — and then failed:

    gh pr create -> HTTP 401: Bad credentials
    "operator must refresh gh auth/login credentials before PR creation"

**That message misdirects. Do not go refresh your host `gh` login.** The host `gh` was
fine throughout (it opened PRs #514 and #515 in the same session). The failure is
inside the sandbox.

**Hypothesis, strong but NOT established — and the complicating evidence stated so
nobody inherits it as fact.** The `pr` node ran **1h51m** after run start
(23:10:25Z -> 01:01:50Z) against a **1-hour** GitHub App installation-token lifetime,
which fits an expired minted token exactly. AGAINST that reading: the `git push` in
that same stage SUCCEEDED. So either push and the `gh` API draw on different
credentials (a helper-refreshed token vs a `GH_TOKEN` minted once at run start), or the
cause is something else. Whoever owns factory hardening should measure it rather than
adopt this paragraph. **The operational consequence is real either way: any run whose
implement+review cycles exceed ~1h can lose its publish leg**, and the ~30-40 min
estimate in § 0 is optimistic once review_fix cycles are involved (this run: 1h54m).

**The recovery is cheap and lossless — the push already happened.** Open the PR BY HAND
from the pushed ref (`gh pr create --head feat/<item>`), which also has the merit of
arming NO auto-merge, so a human merges after CI. Then `[A]` the run and, after the
merge, use **`dispatcher.py reconcile-merged --repo <path> --item <id>`** — it
re-confirmed the merge from the forge and ran the post-merge janitor green. Note this
extends what § 0a recorded: `reconcile-merged` works for an ABANDONED-then-manually-
merged run, not only for a run that merged on its own. The `move:<id>:ready` strand
door remains the one for a run that died WITHOUT merging.

**LIVE CONFIRMATION OF `-6zqv2w`, which arrived free with this merge.** After
reconciliation `-cxu4eu` went **straight to `done`, skipping `acceptance` entirely**
(`acceptance_policy: None`, inheriting `acceptance_mode: "ai-only"` from
`.livespec.jsonc:71`). So a real, freshly dispatched work-item offered NO human
`c accept` valve — exactly what `docs/lifecycle-walkthrough.md` Steps 5-7 promise will
happen instead. That upgrades `-6zqv2w` from reachable-by-reading to observed in
production, and it independently proves the `n set-acceptance` step § 1 added to the
Stage 3(b) walk is REQUIRED, not belt-and-braces: press `n` before the item reaches
`acceptance` or there is no accept leg to walk.

### 0a. The ceiling that stopped us — HISTORICAL, routed around, still true of the default adapter

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

**RE-TESTED AND CONFIRMED — this is a measurement now, not a message.** A limit
string is a claim with a timestamp, so it was re-run rather than believed
(maintainer-directed "try again", 2026-07-29). Answered `[R]`; the loop went
`fix -> janitor` (GREEN, ~1 min with the sandbox's tools already cached)
`-> review`, and review failed **both** attempts again with the byte-identical
cause, an hour after the first pair:

    05:27:14.910Z  wall 25967ms  will_retry=true
    05:27:29.756Z  wall  7710ms  will_retry=false
    06:29:35.017Z  wall  9053ms  will_retry=true     <- the re-test
    06:29:47.878Z  wall  8025ms  will_retry=false    <- the re-test

all four `transient_infra | ACP turn failed`, cause `Internal error: You've hit your
limit · resets Jul 31, 5am (UTC)`. So the ceiling is genuinely blocking and the date
is genuinely 07-31; it is not a stale cached string. **Nothing published** — the run
parked back at the escalate gate.

**DO NOT KEEP PRESSING `[R]` — each cycle silently adds a speculative commit.** The
`fix` node has `max_visits=3`, so two more `[R]` cycles are available and each one
runs an agent that WILL find something to do. Evidence from this cycle below.

**NO RECONCILE IS OWED — the previous § 0 was wrong on its premise.** It predicted
`mbohw3` would strand at `active` when its PR merged. There is no PR and no branch:
the run never reached the `pr` node. Verified on the forge — `git ls-remote origin
refs/heads/feat/livespec-console-beads-fabro-mbohw3` is empty. Do not run
`reconcile-merged`; there is nothing merged to reconcile.

**SCOPE CORRECTION RE-AFFIRMED 2026-07-30, supervisor-owned, and it belongs at the TOP
of this subsection because the paragraph below it reads stronger than the evidence.**
The ~2h figure is measured for the **`escalate` node, in ONE observation**. It is NOT a
property of parked runs generally: a concurrent orphaned run in the orchestrator tenant
survived **3h25m** in `waiting` without being killed. The supervisor who first
generalised this has now flagged it twice as their own over-generalisation, not the
worker's. Treat the two-hour number as a per-node datum with n=1. Do not plan around it
as a rule, and do not repeat it as one.

Corroborating datum from 2026-07-30, since it bears on how much urgency a parked gate
deserves: slice C's second park (the `pr`-node 401) was read from outside as "WAITING
1h55m44s", which was the **run's total age**, not its time at the gate — the gate had
been open about twenty seconds. **If you are deciding urgency from a duration, confirm
what the clock is actually measuring before acting on it.**

**DO NOT PARK A RUN AT THE ESCALATE GATE — THE STALL WATCHDOG KILLED THE ONE RUN THAT
SAT THERE. Measured (n=1), and see the scope correction ABOVE before generalising.**
The paragraph this replaces told a successor to leave
run `01KYP37TZJ9MRTSDR3A0138W4M` parked and answer `[R]` "at or after
2026-07-31T05:00Z". **That was never achievable and the run is now dead.** It blocked
at 06:30:05Z and Fabro killed it at 08:50:00Z:

    status: failed / workflow_error
    detail: stall watchdog: node "escalate" had no activity for ...

~2h20m, matching `graph [stall_timeout="7200s"]`. `fabro resume` is documented for a
dead ENGINE (`workflow.fabro:164-166`); it does not resurrect a run the watchdog has
failed.

**SCOPE OF THAT CLAIM — CORRECTED 2026-07-29T21:4xZ, and read this before relying on
it.** An earlier revision of this paragraph (mine) generalised the single observation
above into "a human has roughly two hours to answer an escalate interview, or the run
dies", and that general form is FALSIFIED. A concurrent orphaned run in the
orchestrator tenant (`01KYQHB99WE87N1MAXKQ8MR8HP`) was still alive and `waiting` at
**3h25m** — far past the 7200s fuse — while a sibling did free its slot at roughly the
predicted time.

What the evidence actually supports is narrow, and should be stated no wider:
**one run, parked at the `escalate` node, was killed after ~2h20m, and the failure
detail named that node.** That is a per-node observation with n=1. It is NOT a
demonstrated property of parked runs in general.

Candidate explanations, NONE verified and none worth the queue's time to chase: the
surviving run is parked at a DIFFERENT node; or its node still emits events so the
silence timer never accumulates; or the timeout is per-node rather than per-run (the
failure text says `node "escalate" had no activity`, which is at least consistent with
per-node). Whoever needs the general rule should measure it deliberately rather than
inherit it from here.

**The operational advice survives the correction, on a weaker basis.** Do not plan to
park a run at `escalate` and come back tomorrow: at least once that cost a green
implement+janitor cycle. Treat an escalate answer as SAME-SESSION work, and if the
decision genuinely must wait, expect to ABANDON AND RE-DISPATCH rather than return to
a held run. The design tension is still worth naming for whoever owns the gate
upstream: an in-loop HUMAN gate whose purpose is to wait for a person can be killed by
a silence timer for doing exactly that.

**And the meta-lesson, because this is the track's named defect class biting the
person documenting it.** This handoff warns that "durable guidance SHOULD NAME THE
CONDITION it depends on, so the next reader can check whether it still holds instead
of inheriting a conclusion." The 2h sentence did the opposite — it converted one
measurement into a rule within hours, in the same file that warns against it. The
measurement was sound; the generalisation was not.

`implement` **succeeded**, `janitor` (`mise exec -- just check`) **succeeded — green**,
and `review` failed on the ceiling above; both green cycles (`96f3ca9`, `a5f51fa`) died
with the run, unpublished. The discarded cycle is recorded on `mbohw3`'s ledger
comments with the commit ids and where the diffs survive.

**Recovery performed 2026-07-29T17:48Z.** The dead run left `mbohw3` stranded at
`active` with nothing merged, so `reconcile-merged` did not apply. The CONSOLE offers
no door out — `status_move_targets(Lane::Active) => &[]` (`lib.rs:473-480`), by
ratified design, because the factory owns that lane. The orchestrator does:
`drive.py --action move:<id>:ready` (`move:<id>:backlog|ready|blocked`), which returned
`active -> ready` green. That is the strand door for a died-without-merging run; note
it, because this handoff previously recorded only the merged-item route.

**The `fix` stage's prompt does not match our situation, and on 2026-07-29 that
produced a real, measured failure mode — worse than predicted, and subtler.**
`[R]` is named "Retry the fix" because the gate was designed for a RED JANITOR:
`prompts/fix.md` opens by asserting "The janitor gate is red … Its output is in the
prior stage context above". Our janitor was GREEN and the failure was one node later.

The prediction was that an agent would invent changes to a green tree to satisfy the
false brief. What actually happened is more interesting. The agent **correctly
rejected the false brief** — its own words: *"the provided prior janitor output was
green; the actual red stage in the local history was `review (failed)`"*. It then
went looking for the review's findings, **found none** (there were none — review died
on the ceiling before emitting anything), and **manufactured a plausible substitute**:
*"I'm treating this as a code-review recovery: identify the likely blocker from the
diff, tighten it, and prove it with tests."* It then changed PRODUCTION code to
satisfy a review finding that never existed, and committed it
(`a5f51fa fix: route status hints through per-item predicate`, on top of the implement
stage's `96f3ca9`).

**Name the failure mode, because the gate will keep doing this:** an escalate gate
whose only retry option presumes ONE upstream failure will, when the actual failure
was a different one, cause the agent to invent a substitute failure rather than
no-op. A well-behaved agent that correctly detects the mismatch still ends up
fabricating work, because the option it was handed obliges it to fix *something*.

**The change itself is defensible, which is exactly why it needs saying out loud.**
The implement stage had derived the hints from a NEW `per_item_verb_kind_is_state_valid`
helper it introduced (implement patch lines 110/113/321/338) — a kind-keyed parallel to
the ratified valve-keyed predicate, i.e. arguably a FOURTH encoding of the thing this
item exists to collapse. The fix stage routed both the production filter and the
verifier through `per_item_verb_is_state_valid` instead, which is what the item's own
ACCEPTANCE text demands by name. It ran a mutation proof and a full green `just check`.
So: unrequested and unreviewed, but toward the brief rather than away from it. Judge it
on merit when review can finally run; do not assume "the fix stage touched it" means
"revert it", and do not assume green means reviewed.

**Standing precondition for any future `[R]`: diff the result against the banked
`stages/002-implement@1/diff.patch` BEFORE anything publishes** — that is why the dump
is worth keeping. A `fabro dump` of a parked run costs nothing and each stage carries
its own `diff.patch`.

The green implementation was BANKED independently of the run: `fabro dump` of all 23
files, including `stages/002-implement@1/diff.patch` (`+207/-107`
`console-application/src/lib.rs`, `+130/-24` `docs_status_hint_lockstep.rs`,
`+33/-3` `console-tui/src/lib.rs`, `+2/-1` `detailed-usage.md`), plus
`stages/006-fix@1/diff.patch`. Those live in a SESSION scratchpad and **the run they
came from is now failed**, so they cannot be re-dumped once it is reaped — treat them
as gone unless a live copy is found. This is the argument for dumping EARLY: the dump
outlived the run, which is exactly what it was taken for.

**NEVER run a dispatch in the foreground — it is a ~30-40 minute operation.** The
previous session's dispatch was SIGTERM'd by a 20-minute tool timeout, which is why
no dispatcher remains to do post-run bookkeeping. Verified 2026-07-29: no dispatcher
process is running, so nothing will auto-dispatch a `ready` item into the dead gate.

### 0b. Re-dispatching with a different reviewer — four things a successor gets wrong

Maintainer decision 2026-07-29: point `review_adapter` at the Codex ACP adapter
(`npx --no-install @zed-industries/codex-acp`) — implement and fix both went green on
that account while only review died on the exhausted Claude subscription. Each item
below was MEASURED, because three of the four contradict the obvious guess.

1. **You CANNOT pass `review_adapter` as a `--input`.** `fabro_run_argv`
   (`commands/_dispatcher_fabro_argv.py:200-218`) is a HARDCODED list — `acp_adapter`,
   `review_fix_visit_cap`, `merge_on_review_cap_outcome` — with no knob for
   `review_adapter`, and its own comment says the review node "uses its own
   `review_adapter` default (Slice A) and is unaffected". The route that works is
   `--workflow <path>`, which `workflow_toml()` honours as precedence rule 1 ("an
   explicit `--workflow <path>` always wins"): copy the whole
   `.fabro/workflows/implement-work-item/` dir somewhere untracked, change
   `review_adapter` in the COPY, and point `--workflow` at it. The whole dir, because
   `graph = "workflow.fabro"` is relative and the prompts hang off the graph.
   The committed file stays untouched — verify with `git status --short .fabro/`.
2. **`resume`, `fork` and `rewind` ALL lack `--input`.** A parked or dead run can
   never adopt a new adapter. Re-dispatch is the only route; do not go looking for a
   clever in-place fix.
3. **`dispatcher.py` must be invoked ALREADY UNDER the credential wrapper.** Run bare,
   it self-re-invokes `/usr/local/bin/with-livespec-env.sh` and that NESTED call fails
   with `credential_wrapper could not run in this environment ... required secret env
   var(s) ['BEADS_DOLT_PASSWORD','GITHUB_APP_ID','GITHUB_PRIVATE_KEY']`. The message
   blames a sandbox and it is MISLEADING: the wrapper is healthy and resolves all four
   secrets from the same session. Prefix it yourself —
   `with-livespec-env.sh -- <plugin>/scripts/bin/dispatcher.py dispatch ...` — and it
   passes straight through. Isolated with a clean two-arm probe run from a FILE, so
   command-composition could not be the cause.
4. **There is a HOST DISPATCH CAP of 2, shared across every tenant on this host.**
   `ERROR: dispatch admission cap refused this dispatch: 2 Fabro run(s) already in
   flight ... meets the host dispatch cap (2)`. On 2026-07-29 both slots were held by
   the `livespec-overseer` tenant, which is not this thread's work and not something to
   route around. Wait for a slot. Raising
   `livespec-orchestrator-beads-fabro.dispatcher.host_dispatch_cap` is a
   `.livespec.jsonc` dispatcher lever and therefore MAINTAINER-OWNED — these levers were
   maintainer-directed once already and silently broke the approve valve for a session.

Also seen on every dispatch attempt, and not ours to fix: `WARNING: master contains
unreleased dispatcher commit(s): fad53c4fe4dc; a release must be cut before this code
takes effect.`

**A caution about the reviewer swap itself.** `prompts/review.md` requires the reviewer
to emit a ROUTING VERDICT (`preferred_next_label`) that the `review -> review_fix` and
`review -> pr` edge guards key on. A different vendor's reviewer has to satisfy that
contract, not merely write good prose. If the verdict is malformed the run MISROUTES
rather than failing cleanly, so read the first Codex review's routing decision before
trusting the pattern across the remaining slices.

### 1. Then continue the queue, serial — MAINTAINER SCOPE DECISION 2026-07-29

**Run ALL FIVE Stage-2 slices through to MERGE, then PARK BEFORE Stage 3(b).**
*(Standing directive, still in force. Progress 2026-07-29: steps 1-3 of the five are
DONE — `-mbohw3`, B1 `-nvflph`, and B2-B4 verify-closed. Two remain: C `-cxu4eu`, then
`-ff6aue`. The PARK-before-3(b) half is untouched and still binding.)* The
maintainer's rationale, recorded so it is not re-litigated: finishing the slices gets
the implementation complete and the ledger honest, and leaves the Stage 3(b) walk —
which needs a fresh cockpit and continuous operator attention — for a DELIBERATE
session rather than the tail of a long one. The walk is the evidence this whole thread
exists to produce, and *doing it exhausted is how a leg gets recorded as walked when it
was driven*.

**THIS ORDER IS FULLY DISCHARGED AS OF 2026-07-30 — the PARK half is now in force.**
(1) `mbohw3` **DONE**; (2) B1 `-nvflph` **DONE**; (3) B2-B4
(`-vwxyj4`/`-cyixzi`/`-zvnjef`) **VERIFY-CLOSED**; (4) C `-cxu4eu` **DONE**
(PR #515 -> `21ff727`, § 0c/§ 0d); (5) the tier-check bug `-ff6aue` **DONE**
(PR #517 -> `2132155`, red-demonstrated — § 0b-bis). **Stage 2 is complete; the thread
is PARKED with Stage 3(b) queued and named in § 0. Do not archive it.**

**`-ff6aue` was RE-VERIFIED STILL LIVE 2026-07-30 before being queued**, because two
items on this track turned out already-fixed-never-closed and B2-B4 were fixed by a
sibling slice. `acknowledges_top_of_pyramid_tier`
(`crates/console-spec-check/src/lib.rs:534-539`) is byte-for-byte what the item
describes — `lower.contains("top-of-pyramid") || lower.contains("integration") ||
lower.contains("acceptance")` — and NO commit has touched that file since before the
item was filed. So this dispatch is warranted; it is not another B2-B4. It is also
itself a vacuous-verifier bug (a check that "cannot fail for realistic inputs" because
it substring-matches ubiquitous domain vocabulary), which makes it a fitting last
slice for a track whose dominant defect class is correct-looking state that nothing
was checking.

**B2-B4: WHY THEY WERE CLOSED WITHOUT DISPATCH, and the distinction matters.** All
three clauses (`contracts.md:453-464` — the picker MUST NOT offer `active` from any
lane, MUST NOT offer `ready` on `pending-approval`, MUST NOT offer `done` from any
lane) are satisfied by `status_move_targets` (`console-application/src/lib.rs:473-480`),
consumed by BOTH the picker cycle (`:306`) and the command validator (`:3435`), and
asserted by a green test
(`move_status_valve_cycles_targets_and_status_move_targets_are_the_pre_terminal_set`).

But **they were REAL gaps when filed, and B1 is NOT what fixed them.** Measured:

    2026-07-27T23:48-23:49Z   B2-B4 filed
    2026-07-28T01:24:21Z      commit 2d5ce11 narrowed the table

At filing time the table read `Backlog => [Ready, Active, Blocked]`,
`PendingApproval => [Backlog, Ready, Active, Blocked]`,
`Acceptance => [Backlog, Ready, Active, Blocked, Done]` — every one of the three
clauses violated. So the gap census did NOT over-report. `2d5ce11` is
"feat: pin per-state verb suppression" = **SLICE A (`-dm5f7q`, PR #466)**, not slice B.
Slice B never touched the table — byte-identical across PR #509.

The filing note predicted discharge "by the slice-B lead's single
`status_move_targets` change" and **named the wrong sibling**. The honest disposition,
and the wording used in each close reason: **fixed by slice A, verified and closed
behind slice B** — not "never open", and not "fixed by B1". Those three readings mean
different things and the ledger must not blur them.

Slice B's real deliverable was the FOUR stale descriptions of a door slice A had
already narrowed: the tests, `docs/`, the in-app Help modal
(`console-tui/src/lib.rs:1737`) and the `WorkItemMoveRequested` domain-contract prose
(`console-domain/src/lib.rs:362`). That is why it needed three review visits.

**TWO ADVISORIES REMAIN OPEN from `-nvflph`'s final review**, and they are the good
kind — a verifier that does not prove what it claims:
`crates/console-cli/tests/tmux_tui_e2e.rs:946` asserts the backlog picker offers
`ready/blocked` but only proves the initial `ready` option and the ABSENCE of `active`;
`:971` is the same for acceptance, never proving `blocked` is reachable. Cycling once
would make each explicit. Non-blocking (the doors are handler-enforced and unit-tested)
but this is the repo's recorded "a verifier must be able to fail" class, so it deserves
a follow-up rather than a shrug.

**The admission half IS DONE (2026-07-29T05:4xZ, at the real TUI `p` valve, NOT
`drive.py`).** `-cxu4eu`, `-cyixzi`, `-vwxyj4`, `-zvnjef` were each admitted and
verified on the ledger after its own valve press. Every valve rendered
`Approve work-item / Target: <exact id>` and was confirmed only after the id was
read back; the pending-approval hint was captured verbatim at all five rows first:
`up/down move | enter open | p approve | r reject | m set-admission | ? help | q quit`
— `p`/`r`/`m` present, **no `c accept`**, matching `attention_item_footer_hint`'s
`PendingApproval` arm as it stood that day. No silent failure on any press. That is a
THIRD live proof of v037 consumption, on a third lane. **Full per-item evidence, with
the one gap named (`zvnjef` was pressed from its sweep position and so has a single
capture, not two), is in `research/stage2-evidence-2026-07-29.md` § 1 — the hint
strings there are now HISTORICAL, since `514a326` rewrote the hint tables.**

`-ectqye` was DELIBERATELY LEFT `pending-approval` — its routing is undecided per § 3
below (reconcile with `-k0w` first). It sits adjacent to the others in the Attention
list and the selection landed on it twice during navigation; re-verify the `Target:`
line before any press near it.

Then park with the Stage 3(b) walk legs QUEUED and named, not attempted: the groom leg
(STRUCK — it needed neither; the `h` HANDOFF walked 2026-07-30 § 0e, but the GROOM
ITSELF has never run — § 0i) and ONE CONTINUOUS
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
APPROVE half of the walk is unaffected.

**RESOLVED — do NOT touch `.livespec.jsonc` for this.** `acceptance_mode` is the
repo-level DEFAULT and carries a documented PER-ITEM OVERRIDE: the `n` set-acceptance
valve (`detailed-usage.md:419` "Per-item override | `n` set-acceptance"; the valve is
`PendingValve::SetAcceptance` → `CommandType::WorkItemSetAcceptanceRequested`,
`lib.rs:223,3295-3297`). `requires_attention_from_lane` keys on the ITEM's
`acceptance_policy`, not the repo default, so pressing `n` on the walked item to set
`ai-then-human` or `human-only` makes that one item rest at `acceptance` for a real
keyboard `c accept` while every other item stays `ai-only`.

So the walk gains a step rather than losing a leg: **… → dispatch → `n` set-acceptance
→ monitor → `c` accept**. That is strictly better evidence than the old path — it
exercises a policy-dial valve this thread has never walked, and it needs no config
edit, no flip-flop on a lever that once silently broke the approve valve for a whole
session, and no maintainer decision. Press `n` BEFORE the item reaches `acceptance`;
an `ai-only` item auto-completes to `done` on arrival and there is no valve left to
press. (The already-banked accept-valve evidence stands: 2026-07-26's four items and
2026-07-29's `dm5f7q` were accepted under the old repo default.)

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

**Merged 2026-07-30, forge-verified by patch-id (`git cherry`, since this repo
rebase-merges so branch SHAs never become ancestors):** **#514** `dd803c0` (docs-custody
delta audit — the walkthrough accept-leg finding, `-6zqv2w`), **#515** `21ff727`
(**`-cxu4eu` / slice C implementation**, opened by hand after the run's publish leg
401'd — § 0d), **#516** `9c0f2ab` (slice C's lessons — the coverage control
measurement), **#517** `2132155` (**`-ff6aue` implementation**, auto-merged by the
factory, red-demonstrated afterwards — § 0b-bis).

**BOTH RED-DEMO OBLIGATIONS ARE DISCHARGED — and BOTH state their blind spot, which is
the half that matters.** `console-fork-drift-check`: RC=1 when an upstream digest
moves, **RC=0 when we edit our OWN committed file** — so a green run says nothing about
our side of the diff (`research/stage2-evidence-2026-07-29.md` § 2). `console-spec-check`
tier gate: RC=1 on the exact input that used to report clean (§ 0b-bis). Also
discharged and durable, in that same note's § 1: the four TUI approve-valve admissions
with hints captured verbatim, a per-item table, and the ONE gap stated rather than
glossed (`zvnjef` was pressed from its sweep position, so it has one capture and not
two). **All three of those were re-requested repeatedly while already on master; if you
are about to ask for them, read `research/stage2-evidence-2026-07-29.md` first.** On #515's patch-id check the four SUBSTANTIVE commits read `-` (upstream)
while fabro's contentless stage-marker commits read `+`; that is correct and expected,
not a partial merge — the rebase-merge drops the empty markers.

**Merged 2026-07-29 (the Codex-reviewer session), all forge-verified:** **#490**
`e4c77cd` (§ 0 rewrite — the subscription ceiling), **#503** `259627a` (watchdog kill,
strand door, re-dispatch mechanics), **#505** `514a326` (**`-mbohw3` implementation**),
**#506** `7476ecf` (`research/stage2-evidence-2026-07-29.md` — the durable evidence
note), **#508** `fc77ce8` (scoped my own over-broad watchdog claim), **#509**
`46783ad` (**`-nvflph` implementation**), **#510** `724b9e1` (struck my own wrong
reachability refinement).

**THREE OF THOSE SEVEN CORRECT SOMETHING THIS SESSION ITSELF ASSERTED** (#503's
"park until 07-31" was impossible; #508 scoped a rule I generalised from n=1; #510
retracted a refinement I had filed as a rider). That is not noise — it is why the
record is more trustworthy tonight than this morning. A successor should expect to
correct this file too, and should not treat its confident sentences as settled merely
because they are confident.

**Two owed items were delivered LIVE and then made durable in #506**, because they had
been re-requested four times while living only in session chat: the
`console-fork-drift-check` RED DEMO (four arms — baseline RC=0, our-own-edit RC=0
**blind**, upstream-digest-moved RC=1 **catches it**, restored RC=0, all read UNPIPED,
tree restored) and the four admissions' captured hints. **Evidence that lives in a
transcript has to be re-derived by whoever asks next; put it in a file.**

Older, still true: **#472** `edc3b29` (brief-29 correction),
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
  **THE RIDER WAS RIGHT. A mid-day "refinement" of mine that doubted it was WRONG and
  has been struck — read this instead.** The rider calls the Backlog divergence "a live
  operator-visible defect", which needs a Backlog row selectable in the ATTENTION view.
  It is, and there are five live right now.

  The mechanism, which the struck refinement did get right: the valve-actionable lane
  fold cannot produce one — `requires_attention_from_lane` (`:5336-5353`) admits only
  `PendingApproval`(manual), `Acceptance`(`ai-then-human`|`human-only`),
  `Blocked`(`needs-human`) — so the route is an INGESTED needs-attention row whose
  work-item reference resolves to a Backlog item, because `selected_work_item_lane`
  (`:1352`) resolves through `work_item_by_id`, i.e. from the LANE collection and not
  from the attention snapshot the row arrived on.

  What the refinement got wrong was calling that route rare. **It is a standing hygiene
  lane.** `needs_attention.py --json` returns five `hygiene:untriaged-backlog:<id>`
  items whose `source_ref` is `{"path": null, "repo": ..., "work_item": "<id>"}` —
  `-9ts`, `-htp`, `-mvu22t`, `-oqm`, `-topr34`, every one `lane=backlog` on the ledger.
  `source_ref.work_item` is exactly what `AttentionItem::work_item_id()` reads. So the
  chain closes and the Backlog hint arm renders in production.

  **How the wrong refinement got written, because the method matters more than the
  correction:** it cited a live 61-row Attention list "holding no Backlog row" as
  corroboration. That list was almost entirely worktree-hygiene rows whose `source_ref`
  carries a `path` and NO `work_item` — and the check never asked whether any row
  carried a `work_item` RESOLVING to backlog. Absence of a Backlog LANE row is not
  absence of a backlog-RESOLVING row. This is § 6's own rule biting the author who was
  quoting it: **an absence never announces itself in a grep for the wrong token.**

  Consequences: the docs row `mbohw3` added is CORRECT — do not remove it if a reviewer
  calls it unreachable (one did; it was refuted). The three-encodings defect was a real
  operator-visible defect, so the re-tier to correctness was right. And the divergence
  IS reproducible in the Attention view — select one of the five untriaged-backlog rows.

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

**`git fetch` BEFORE YOU RE-PIN — THE RE-PIN YOU ARE ABOUT TO MAKE MAY ALREADY EXIST.**
Earned 2026-07-30, when this gate refused a push carrying a one-line TEST change that
could not possibly affect `.fabro/`. The trigger was this session's own SessionStart
hook bumping the plugin (`1fc573da09c5` -> `eacbb88ead9c`), which staled pins taken
against the older build. Two re-pins had already landed that day — `5a6148a` (against
`1fc573da09c5`) and `be09e26` (against `eacbb88ead9c`) — and a fetch showed master
ALREADY carried the second, taken against the very build this host resolves. **Rebasing
the blocked branch onto that master turned the gate green with no new pin commit.**
Re-pinning over a newer pin restarts the ping-pong instead of ending it, so the order
is: fetch, rebase, re-run the gate, and only re-pin if it is still red. When you do
re-pin, put YOUR resolved build id in the `reason` as "resolved on this host as <id>" —
the re-pin is not idempotent across hosts and the reason field is the only place that
pattern is visible. Full measurement (13 records, 6 builds) is on `-pj5g3f`.

**KNOW WHAT IT DOES NOT CATCH: it is blind to OUR OWN edits.** Verified both
directions 2026-07-29, exit codes read UNPIPED, tree restored after each:
changing `review_adapter` in our committed `workflow.toml` -> **RC=0**, "fork in
lockstep with its pins"; zeroing a pinned UPSTREAM digest -> **RC=1**, naming
`prompts/pr.md`, pinned-vs-live, and the recorded reason. The fixture stores
`upstream_sha256`, so the gate answers "did UPSTREAM move?" and nothing else. It does
NOT answer "did we change our fork without recording why?" — that remains a human
responsibility, and a reader who assumes otherwise will trust a green run that never
looked at our side of the diff. (Corollary, in case a future brief argues from it:
editing our committed fork does not RED this gate and does not force a pins refresh.
The reasons not to edit it are review and provenance, not the guard.)

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

**A THIRD NAMED PATTERN, earned 2026-07-30: A CORRECTION THAT REACHED ONE DOCUMENT AND
NOT ITS TWIN.** This thread discovered that `acceptance_mode: ai-only` cuts the accept
leg out from under a walk, and fixed it — for its OWN Stage 3(b) walk, by adding an `n
set-acceptance` step (§ 1). The operator-facing document describing the same walk,
`docs/lifecycle-walkthrough.md`, was never touched, so its Steps 5-7 still promise a
human `c accept` that production does not produce (filed `-6zqv2w`; confirmed live in
§ 0d). The defect is not that the fix was wrong — it was right — but that a correction
was applied where it was NOTICED rather than everywhere the claim LIVED. It is the
sibling of § 0's "assume a second and third restatement exists elsewhere", promoted from
a coding lesson to a documentation one: **when you correct a behavioral claim, grep for
every OTHER place that claim is made, including prose and including your own plan
files.** Diagnosed by the supervisor, and worth more than the finding that prompted it.

**The wrong-token rule bit twice more in the same session, once inside a mutation proof
and once inside a safety audit — it is not a beginner error.** First, a grep enumerating
six lanes reported the `s` move-status table's `done` row missing; the row was there and
the pattern lacked the seventh token. Second, an audit for coverage-exclusion
gate-gaming searched `cfg(not(coverage` and reported none, while the diff carried
`#[cfg(all(not(test), not(coverage)))]` — nested one level deeper, and it turned out to
be a legitimate ~30-use repo idiom documented at `justfile:47`. Both were caught by
reading source rather than by a better grep. **Prefer enumerating the source's own
vocabulary over pattern-matching what you expect it to say.**

This thread runs **under supervision** since 2026-07-25 — read
`plan/console-happy-path-mvp/supervisor-handoff.md` FIRST, and re-measure
everything per its § "Reactivating a parked thread" (fetch + lanes + new
items + cockpit binary age) before trusting any claim here.

Then, in order:

1. **Accept AND approve legs are both WALKED at the keyboard** — accept on
   2026-07-26 (`research/accept-valve-walk-2026-07-26.md`) and again on
   2026-07-29 for `dm5f7q`; approve on 2026-07-29 for `ff6aue`/`mbohw3`/
   `nvflph`, which discharges the leg `-sreeqc` never got. **`-sreeqc` is
   DISCHARGED by maintainer ruling 2026-07-30: the four clean TUI `p`
   admissions of 2026-07-29 prove the approve valve at the keyboard more
   strongly than one `-sreeqc` re-walk would. Do NOT re-walk it. `-u3w3er`
   stays UNFIXED and was simply never triggered — record it that way, never
   as disproven.** **The `h` DRIVER-HANDOFF is WALKED (2026-07-30, § 0e) and
   never needed the ratification or the transport — that clause was a phantom
   blocker and is struck. But the GROOM ITSELF has never run (§ 0i), and since
   groom is the only route from `backlog` to `pending-approval`, that half is
   the walk's spine.** Still owed for Stage 3(b): ONE CONTINUOUS
   single-item walk (find → groom → admit → dispatch → monitor → accept).
   Attempt 1 got legs 1, admit and `n`, then stopped at the dispatch
   refusal (§ 0f) — so every leg except `c` accept is now individually
   proven, and what is missing is the unbroken pass.
   `-6ma`/`-m36`/`-8i9` are all CLOSED with verified reasons.
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
