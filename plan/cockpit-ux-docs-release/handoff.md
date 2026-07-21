# Console cockpit UX + user-docs + release-pipeline — SPEC-DRIVEN program (livespec-console-beads-fabro)

## OPERATING DIRECTIVE (maintainer-declared 2026-07-13)
**Everything in this program is SPEC-DRIVEN.** Each behavior below is a
`/livespec:propose-change` against THIS repo's `SPECIFICATION/` (a **scenario**,
plus any contract/non-functional edits), independently Fable-reviewed, ratified via
`/livespec:revise`, and driven by a **top-of-the-pyramid E2E test that exercises the
REAL TUI via tmux**. Missing scenarios AND tests are **backfilled** (including for
existing behavior). Impl work-items are DERIVED from spec gaps
(`capture-impl-gaps`) AFTER the propose-changes ratify — NOT filed freeform up
front (the freeform epic tried that and was retired; see bottom).

Cross-cutting acceptance (maintainer): every deliverable is exercised LIVE and, where
it runs the console, verified against **TWO DIFFERENT REPOS**. The release
pre-delivery test uses the **DOWNLOADED release asset** (not a source build), run
from a **random PWD like `/tmp`**, against two repos. The docs walkthrough is
validated by an **agent walking it on a DUMMY item, driving a REAL TUI in a tmux
pane, for two repos**.

## STATUS (updated 2026-07-21) — deliverable #0 + B1–B8 ALL DONE; nothing docs/release-shaped remains

The foundational tmux real-TUI E2E harness (#0) and the ENTIRE **B1–B8**
program are LANDED on console master. This thread's mission — cockpit UX,
user-docs, release pipeline — is DELIVERED.

**This thread is a candidate for archival.** What is still open is either
already split out as a standalone work-item (`bamsy3`), better owned by
`plan/console-happy-path-mvp/` (the Scenario 5/11 E2E backfill), or dead
(Stage-2). The one thing genuinely still resident here is doc custody — which
an audit later the same day showed is ACTIVE, not passive (see §"DOC CUSTODY
IS ACTIVE").
See §"RESUME ORDER" for the disposition of each and what archival would need.

| Item | State | Ref |
|---|---|---|
| #0 tmux real-TUI E2E harness | ✅ DONE | PR #262 (+ dedicated-socket width fix in #286) |
| B1 source-availability honesty (Scenario 13) | ✅ DONE | PR #268 |
| B2 Status-line context hints (Scenario 19, v026) | ✅ DONE | PR #278 |
| B3 top/header-pane focus + h-scroll (Scenario 20, v027) | ✅ DONE | PR #286 (`4e8598f`) |
| B4 navigable modal Help (Scenario 18, v025) | ✅ DONE | PR #267 |
| B5 panes operational-content-only (Scenario 21, v028) | ✅ DONE | propose #280 → revise #288 → impl #289 (`1bfdb41d`) |
| B6 user-docs → `docs/` tree (4 sub-docs) | ✅ DONE | spec: v029 (`8839d63`) + corrections **v030** (PR #297, `2fac510`); impl: PR #300 (`7df1ea2`) |
| B7 key-by-key lifecycle walkthrough doc | ✅ DONE | PR #327 (`b8ff009`) — `docs/lifecycle-walkthrough.md` + two-repo tmux E2E acceptance |
| B8 release capstone | ✅ DONE | acceptance run 2026-07-21 (§"B8 POSTSCRIPT"); pipeline + v0.2.0 asset PR #243 |
| **Backfill** real-TUI tmux E2E, Scenarios **5/9/11** (13 already covered) | ⬜ NOT STARTED — **RE-HOME, not this thread's work** | §"BACKFILL" below |
| ~~**Stage-2** autonomous-mode MVP acceptance~~ | ❌ **STRUCK 2026-07-21 — DEAD** (thread archived; mode retired) | §"RESUME ORDER" below |

### Ledger reconciliation owed (found 2026-07-19 — see `plan/impl-dispatch/handoff.md`)
Five `pending-approval` items — **W3 `-636m46` / W4 `-j3ts23` / W5 `-2ctzhm` /
W6 `-zmunjo` / W7 `-yvikqp.1`** — plus their parent epic **`-yvikqp`**
(`backlog`) are **already DELIVERED and merged**, verified against the code. They
are stale records, NOT admission gates; do not walk them as valves. This matches
`livespec/plan/archive/autonomous-mode/handoff.md` cont.22 ("genuinely DONE:
W3/W4/W5-valves/W6/W7"). Close as completed, per the `-0tu` / `-6tn` pattern.

Also: **12 stacked pin-bump PRs are all red** on `check-completeness` because the
bump automation rewrites `.livespec.jsonc` `compat.pinned` without refreshing
`tests/fixtures/orchestrator-config-manifest.json`. Unfiled. Details in the
impl-dispatch handoff.

### Open follow-up work items (console beads ledger)
- **`livespec-console-beads-fabro-25rvmd`** (P2, blocked) — B1 transition-epoch source-availability tally (re-down-after-recovery dedups in a persistent cross-run store).
- **`livespec-console-beads-fabro-ble`** (P2, backlog) — extend `distinguish_repeatable_command` idempotency-key fix to ALL repeatable operator actions (currently move-only).
- ~~**`livespec-console-beads-fabro-bamsy3`**~~ — **FIXED** by `7110eca`; sits in `acceptance` awaiting the human accept valve. Independently re-verified against its filed reproduction (see §"RESUME ORDER" item 1).
- ~~**`livespec-console-beads-fabro-7wy`**~~ — **RESOLVED** in the v030 PR (#297). **Ledger status confirmed `done` on 2026-07-21** — the "close the record if still open" instruction below is SATISFIED; no action remains. The revise CLI's gating post-step doctor flagged it, so it was fixed rather than deferred; there were THREE such citations, not one (`console-application/src/lib.rs:1987` and `:2453`, `source_adapters.rs:1865`), all now file-level. `grep -rn '§' crates --include='*.rs'` returns nothing and `doctor-no-spec-section-citation-in-code` passes. **Close the ledger record if still open.**

The B6/B7/B8 deliverables live in THIS plan by design — the freeform work-item vehicle for them was RETIRED (see §"RETIRED"); concrete follow-up bugs live as the work items above. Four stale worktrees (`docs-console-tui-usage`, `console-release-pipeline`, `cap-test-parallelism`, `phase3-selfhosted-cutover`) — leftover from ALREADY-MERGED PRs (#165 / #243 / #266 / #250) — were reaped 2026-07-19; they were NOT holding in-progress B6/B7/B8 work. (A prior revision of this file flagged worktree `b6-spec-review-fixes` as holding
un-PR'd B6 corrections; that work is now MERGED as v030 via PR #297 and the
worktree is reaped — nothing to pick up.) A fifth, `ci-concurrency-group`, was LEFT untouched: its head (`79305bc`, the merged E2E-targetdir fix) is in master but it carries UNCOMMITTED CI work (`.github/workflows/ci.yml` + a `Cargo.lock` drift) — another session's in-progress/abandoned CI-infra worktree, not part of this cockpit track.

## KEY FINDING — the real TUI has ZERO automated coverage today
`run_interactive_tui` (raw-mode / alternate-screen) in
`crates/console-tui/src/lib.rs` is `#[cfg(all(not(test), not(coverage)))]` —
compiled OUT of every test/coverage build. The existing `crates/console-cli/tests/
scenario_*.rs` tests drive `run_store_backed_tui_session` with **scripted in-process
`TuiSessionRunner` fakes**, never a real terminal. There is **NO tmux/pty harness
anywhere** in the repo (grep for tmux/send-keys/capture-pane/expectrl/portable_pty →
none). So the real keypress→raw-mode→render path has only ever been exercised by a
human hand-driving tmux. **This is the gap the directive closes**, and it is why a
foundational harness is deliverable #0.

## THE CONSOLE SPEC TODAY (`SPECIFICATION/`, 31 scenarios)
Adjacent existing scenarios these behaviors map onto / must refine:
- **Scenario 5** — TUI-first operator workflow.
- **Scenario 9/10/11** — autonomous-mode enable; resolve/escalate; human valve + policy edits.
- **Scenario 13** — Operator distinguishes cockpit-blind from factory-idle:
  "Unavailable sources are counted and named in the header"; "A healthy cycle shows
  no phantom unavailability count." ← the sources bug lives here.

## DELIVERABLE #0 (FOUNDATIONAL) — real-TUI E2E harness driven via tmux
A reusable test harness that:
- launches the RELEASE binary in a dedicated tmux session/pane (pinned size, e.g.
  112×28) under the credential wrapper, with an isolated
  `LIVESPEC_CONSOLE_STORE_PATH` scratch store;
- `send-keys` sequences and `capture-pane` the rendered screen;
- asserts on rendered content AND on side effects (console `commands` sqlite rows,
  orchestrator ledger changes) — the pattern proven manually this session;
- is parameterized by REPO so every scenario can run against TWO different repos;
- is wired into the test pyramid as the TOP tier (a `just` target + CI job). NOTE:
  CI must have `tmux` available (add to the CI image if absent). Decide harness
  language/placement: a Rust integration test that shells to `/usr/bin/tmux`, or a
  shell/pytest E2E driver — pick what the repo's pyramid supports; it must be a
  first-class, always-run gate, not a manual script.
This harness is a prerequisite for the E2E test of EVERY behavior below and for the
backfill.

## THE BEHAVIORS (each → propose-change → scenario → tmux E2E → impl)

### B1 — Sources: all but the console's own appear unavailable  (refines Scenario 13)
Header shows "sources: N unavailable"; maintainer reports ALL sources except the
console-own source are unavailable under a normal launch. ROOT-CAUSE why the
orchestrator/github/fabro/dispatcher/livespec adapters resolve unavailable (start
from `serve --preview` source detail + `BackingCliResolution` +
`SystemSourceProbe.run_command`; likely a resolution/exec issue like the earlier
Finding-E python3 exec or a cwd/env gap). Scenario: a normally-launched console
against a real tenant shows the expected sources AVAILABLE; only genuinely-absent
ones are counted + named with a reason. E2E: tmux launch, capture header, assert
availability, for two repos.

### B2 — Status line always empty → context-specific shortcut hints
README describes a "shortcut hint line" in the Status pane; it is ALWAYS empty.
Scenario: the Status line renders context-specific key hints that CHANGE with the
focused pane and any open modal. E2E: tmux, focus each pane / open each modal,
assert the hint text changes appropriately.

### B3 — Top pane focusable + horizontal-scrollable on narrow viewports
The header/top pane cannot be focused or scrolled; on narrow viewports its content
clips. Scenario: the top pane joins the focus cycle; while focused, left/right
scroll it horizontally to read clipped content; on blur it snaps back to the
left-justified default. E2E: tmux at 112 cols, focus top pane, scroll right, assert
previously-clipped content visible, blur, assert snap-back.

### B4 — Navigable, context-specific Help modal (`?`)
Replace non-contextual help with a navigable multi-page modal: LEFT-side menu +
RIGHT-side help text scrollable UP/DOWN only (not left/right). Sections: one
"Global actions" section + one section PER focusable pane. `?` while a pane is
focused opens Help auto-focused to THAT pane's section. The modal is a window ON TOP
of the main screen: near-full-viewport, only a **3-character border** on each side
and top/bottom; never wider than the viewport. Esc exits; **"esc to exit" is printed
at the bottom ALWAYS**. Scenario + E2E: `?` from a focused pane lands on its
section; menu navigates; right pane scrolls up/down; border geometry (3 chars);
"esc to exit" always present; Esc closes.

### B5 — Remove baked-in explanatory doc prose from the TUI
The TUI renders useless doc sentences inside panes, e.g. "Spec lifecycle status is
projected from LiveSpec adapter observations." and "Revise-required events stay
visible in the Spec view until resolved." Sweep and REMOVE such baked-in prose;
relocate any genuinely-useful explanation into the docs tree (B6). Scenario: panes
show operational content only — no explanatory doc prose. E2E: tmux, capture each
pane, assert the named sentences (and similar) are absent.

### B6 — User-docs restructure to a `docs/` tree
Move ALL user-facing docs out of the top-level README into `docs/`. Top-level README
only LINKS to `docs/README.md`. `docs/README.md` = overview + TABLE OF CONTENTS
only; real docs in sub-docs linked relative from the TOC:
- `docs/installing.md` — TUI launch AND download-install (incl. usage from different
  repos).
- `docs/overview-quickstart.md` — general overview + quick start.
- `docs/cli-options.md` — env vars, CLI options, sub-commands.
- `docs/detailed-usage.md` — detailed usage/behavior, with a sub-section PER pane.
(Docs describing TUI behavior must be authored/updated AFTER B2–B5 land so they
match the shipped TUI.) This is docs, but still spec-anchored: the console spec
should carry a scenario/contract that user docs live in `docs/` with the README as a
pointer, if that invariant is worth enforcing.

### B7 — Key-by-key lifecycle walkthrough doc  (acceptance = real-TUI two-repo agent run)
A `docs/*.md` section: a detailed, step-by-step, key-by-key walkthrough of running a
work-item through the ENTIRE livespec lifecycle via the TUI. ACCEPTANCE: an agent
walks the documented steps on a DUMMY work-item, driving a REAL TUI in a tmux pane,
end-to-end, with NO doc/behavior mismatch, for TWO different repos. This is itself a
tmux E2E scenario.

### B8 — Release pipeline (distribution scenario/contract)  (was retired z62)
release-please on the console (version tags + GitHub Releases on feat/fix) + a
release-triggered CI job that BUILDS the linux x86_64 binary and ATTACHES it as an
asset. Distribution scenario/contract in the spec. Pre-delivery TEST (top-of-pyramid
E2E, NOT tmux-TUI): DOWNLOAD the published asset (`gh release download`), NOT a
source build; run it from a RANDOM PWD like `/tmp/<rand>`; exercise `serve`/`serve
--preview` against TWO DIFFERENT repos. Then de-gate the README/`docs/installing.md`
download instructions. Green CI build alone is NOT acceptance — the downloaded
artifact must run live from /tmp against two repos.

## BACKFILL — existing TUI behavior lacking real-TUI E2E coverage
Once the harness (#0) exists, backfill real-TUI tmux E2E tests for the existing
scenarios that today have ONLY scripted-runner coverage. Each existing scenario
that asserts TUI-visible behavior needs a tmux E2E test.

**Corrected 2026-07-21 — this section carried three errors.** Verified against
`SPECIFICATION/scenarios.md` and `git show origin/master:crates/console-cli/
tests/tmux_tui_e2e.rs`:

1. **Scenario 13 is ALREADY COVERED** — B1 landed
   `tmux_tui_e2e_all_reachable_sources_are_idle_not_unavailable` and
   `tmux_tui_e2e_unreachable_source_is_counted_named_and_reasoned`. Drop it
   from the target list; the remainder is **5 / 9 / 11**.
2. **Scenario 9 is NOT "autonomous enable."** The spec
   (`SPECIFICATION/scenarios.md:241`) reads *"Operator sets a dispatcher policy
   setting from the console."* It is unrelated to autonomous mode and is
   therefore UNAFFECTED by that mode's retirement. The old shorthand here sent
   at least one session looking for a connection that does not exist.
3. **This backfill is probably not THIS thread's work** — see below.

### Ownership: 5 and 11 belong with `console-happy-path-mvp`
This thread is cockpit-UX + user-docs + release-pipeline. Test-coverage
backfill for pre-existing TUI behavior rode along only because deliverable #0
happened to build the harness here.

`plan/console-happy-path-mvp/` is the natural owner of **Scenario 5**
(TUI-first operator workflow) and **Scenario 11** (human valve + policy-edit):
it is the declared delivery/integration owner of the happy path, it walks those
exact flows, and its Stage-3 already reuses B7's stateful tmux fixture
(`crates/console-cli/tests/support/lifecycle.rs`) — the same fixture these
scenes need. **Scenario 9** has no thread affinity and is a standalone
work-item, not plan work.

Re-home before starting; do not begin the backfill under this thread by
default.

## FIX ORDER + conflict analysis
- **#0 harness first** — everything depends on it.
- **B1 sources** — independent (adapter/resolution code); can proceed in parallel
  with the harness build once the harness exists to test it.
- **B2 → B3 → B5 → B4** — all touch `console-tui/src/lib.rs` (render/input); SEQUENCE
  them (one worktree at a time) to avoid conflicts. B4 (Help modal) is the largest.
- **B6/B7 docs** — author AFTER B2–B5 land (docs must match the final TUI). B6
  restructure (file moves) can start early; per-pane detailed-usage + B7 walkthrough
  come last.
- **B8 release pipeline** — independent infra; its two-repo/downloaded-asset/-/tmp
  test is a capstone acceptance.
- **Backfill** — after the harness, interleave with the above.

Each behavior: `/livespec:propose-change` → independent Fable review → `/livespec:revise`
→ tmux E2E test (RED) → implement (GREEN) → live-verify two repos. Follow the console
repo mutation protocol (worktree → PR → merge) and `just check` discipline throughout.

## RETIRED (do not re-open) — freeform items closed 2026-07-13
Epic `livespec-console-beads-fabro-0ak` + children `-5rw` (sources), `-rjo`
(status-line), `-bdy` (top-pane), `-8c1` (help modal), `-aoi` (docs restructure),
`-clt` (walkthrough), and `-z62` (release pipeline) were CLOSED as "wrong vehicle"
— superseded by this spec-driven program. Their descriptions/acceptance are folded
into B1–B8 above.

## B6 POSTSCRIPT (2026-07-20) — what landed, and one process lesson

B6 is DONE. On master: the `docs/` tree (`docs/README.md` index +
`installing.md` / `overview-quickstart.md` / `cli-options.md` /
`detailed-usage.md`), the README cut 297 → 104 lines to overview + pointer +
contributor material, the settings-completeness gate repointed from `README.md`
to `docs/detailed-usage.md` via a new `pub SETTINGS_DOC`, and Scenario 22 bound
by `crates/console-completeness-check/tests/scenario_22_user_docs_tree.rs`
(8 cases).

**The docs are a REWRITE, not a move — do not trust the pre-B6 README for any
behavioral claim.** A source audit found 16 places it contradicted the shipped
binary: the help key does not toggle Help (inert while open, Esc-only close);
Help is a navigable 8-section modal, not a per-view overlay; there are FOUR
focusable panes, not two; left/right never change the view; the header
degradation order is mode → fleet → source-name elision → view → attention, with
`repo` and the source COUNT never dropped; the drain program takes a `loop`
sub-command and the repo PATH; the drive program's `--repo` carries a FILESYSTEM
PATH from `LIVESPEC_CONSOLE_REPO_PATH`, not the id from `LIVESPEC_CONSOLE_REPO`
(which itself defaults to the CWD basename); six env vars were undocumented;
`serve --preview` runs the store-backed report, not the demo render;
`arch-check` was missing; `events tail` has a hard-coded limit of 20. All are
corrected against source in `docs/`.

**Ledger item `-0tu` is now genuinely satisfied.** Its second clause ("useful
explanation relocated to docs/*.md") was unmet when it was closed as "met in
full" by B5 alone; the three sentences B5 stripped from the pane bodies now live
in `docs/detailed-usage.md`. The `console-autonomous-mode` session was asked to
correct that close reason (leave closed; record that B5 delivered removal and B6
delivered relocation) — **verify it did.**

**Process lesson — the v029/v030 split.** The B6 propose-change was merged and
ratified as v029 by a DIFFERENT session (`autonomous-mode`, cwd
`/data/projects/livespec`, via a subagent named `b6-revise`) roughly 15 minutes
before this session's independent-review fixes landed on the proposal branch.
v029 therefore ratified an un-reviewed draft carrying two substantive defects:
the completeness gate had no spec-anchored path to read (clause 2 called
sub-document identity an implementation detail while clause 3 anchored the gate
to "the detailed-usage sub-document", never naming a file), and the
maintainer's chosen NFR-Boundary widening was absent. v030 (PR #297) corrected
both.

TWO takeaways for future spec work in this repo:
1. **Ratify IN-BRANCH.** The revise CLI only needs `--spec-target` pointed at a
   worktree; the proposal does NOT have to reach master first. Landing propose +
   revise as ONE atomic PR closes the window in which another session can merge
   a half-reviewed proposal. The merge-then-revise round-trip is what produced
   v029's defects.
2. **Multiple sessions operate on this repo concurrently**, and they are
   distinguishable ONLY by the `Claude-Session` commit trailer — the GitHub
   actor is identical for all of them. This file's B6 row was itself stale for
   ~30 minutes because another session audited the repo mid-flight. Before
   trusting a STATUS row, spot-check the filesystem (`git ls-tree origin/master
   --name-only docs/`) rather than the row.

## B7 POSTSCRIPT (2026-07-20) — the harness gap, and a drift gate

B7 is DONE: `docs/lifecycle-walkthrough.md` (linked from the docs TOC) plus
`tmux_tui_e2e_lifecycle_walkthrough_two_repos`, which walks the documented
keystrokes against the REAL TUI for two repos on every CI run.

**The harness could not test a lifecycle at all, and this is reusable.** Every
backing CLI in the tmux harness is stubbed to print `{}` — correct for the
cockpit-chrome scenes, but `{}` is not even a legal `list-work-items --json`
payload (the parser wants an array), so every E2E ran against an EMPTY board by
construction and no test asserted on a work-item.
`crates/console-cli/tests/support/lifecycle.rs` adds a stateful fixture that
serves a work-item, accepts the drive actions the TUI issues (`approve:<id>`,
`accept:<id>`, `move:<id>:<target>`), mutates the lane, and answers the bare
`config` read so Settings is populated. Hermetic — no tenant, no network.
**The E2E BACKFILL below should use it**; the Scenario **5/9/11** scenes need
exactly this. (Scenario 13 is already covered and is NOT a backfill target —
see §"BACKFILL" for the correction, and for why 5 and 11 are re-homed to
`plan/console-happy-path-mvp/`.)

**The ship-guard is load-bearing for any operator documentation.** `move`
refuses `acceptance`, `done`, and `pending-approval`, so `active` ->
`acceptance` is the FACTORY's step and no keystroke performs it. The walkthrough
documents who drives what; the E2E models the factory step with
`factory_move()` rather than pretending an operator key exists. Do not write
operator docs that imply otherwise.

**B6's docs drifted within ONE DAY of landing.** `185426b` ("stop advertising
inert keys") made the Status hints state-dependent, and the B6 table silently
became wrong in four places (one `Lanes` row was really three, `Attention` was
two, `enter drill` became `enter item` inside a lane, and a new work-item record
overlay arrived with its own hint). Nothing failed, because prose is not
executable. All corrected, and
`crates/console-cli/tests/docs_status_hint_lockstep.rs` now binds every hint the
doc quotes to a string literal in the module that renders it — the same lockstep
idea as `console-completeness-check`, negative-tested by injecting the exact
drift that occurred.

**The generalizable lesson: bind docs mechanically or they rot at the team's
commit rate.** With several sessions on this repo, that is fast. When B8
de-gates the download instructions in `docs/installing.md`, give them the same
treatment — a test that fails if the documented asset name stops matching what
the release actually publishes.

## B8 ENTRY NOTE (2026-07-21) — an UNRESOLVED scope question, ask before running

**B8 is the first item in this plan that is not hermetic, and its scope was
never settled. Get a maintainer answer before running it.**

Everything through B7 ran against stub CLIs, scratch stores, and no credential
wrapper. B8 is the opposite BY DESIGN: the point is exercising the real
downloaded artifact against real repos, which means live Beads/Dolt tenants and
the credential wrapper. That is a different risk class from anything else in
this plan, so it does not inherit the "just do it" latitude the earlier items had.

Three specifics a fresh session should raise rather than assume:

1. **Read-only or mutating?** The directive says "exercise `serve`/`serve
   --preview` against TWO DIFFERENT repos" but never says read-only. A console
   pointed at a live tenant CAN issue drive actions (every valve rides the
   `drive` program). Confirm the intent is observation, not mutation, before
   pointing the binary at a real tenant.
2. **Which two repos?** The fleet has several (`livespec`,
   `livespec-orchestrator-beads-fabro`, `fabro`, this console). See
   `.ai/fleet-repo-naming.md` — never use the bare "beads-fabro" form, and
   target repos by full `/data/projects/<full-name>` path.
3. **De-gating claims more than one run proves.** Removing the
   "not yet acceptance-verified" notice from `docs/installing.md` asserts the
   download path works for USERS — on their machines. A local run proves it on
   THIS host, architecture, and PATH. Scope the de-gated wording to what was
   actually verified (linux x86_64, downloaded asset, runs from an arbitrary
   PWD) rather than a blanket "this works".

RECOMMENDED shape, if the maintainer approves: download the published asset,
run it read-only (`serve --preview`) from `/tmp/<rand>` against two repos,
report exactly what that proves AND what it does not, then de-gate with scoped
wording. Per the directive, a green CI build is explicitly NOT acceptance.

Then give the install instructions the same mechanical binding the Status hints
got (see §"B7 POSTSCRIPT"): a test that fails when the documented asset name
stops matching what the release publishes. Note `docs/installing.md` currently
names the asset by GLOB (`livespec-console-beads-fabro-*-x86_64-unknown-linux-gnu`),
so such a test should assert the glob still matches a real published asset name.

## B8 POSTSCRIPT (2026-07-21) — DONE, and it found two doc bugs

The acceptance run happened, read-only, per the maintainer's answers to the
three §"B8 ENTRY NOTE" questions (read-only; console +
orchestrator-beads-fabro; de-gate scoped to what was proven).

**What was verified.** The published `v0.2.0` asset was downloaded with the
documented `gh release download` globs into `/tmp/b8-accept-<rand>`,
`sha256sum -c` OK, and confirmed distinct from the local source build
(`49ec6d06…` vs `686a403a…`) so there is no doubt which binary ran. It runs
from a PWD that is not a git repository, exit 0, and against both repos with
correct per-repo tenant scoping — console surfaced PR #317 and
`livespec-console-beads-fabro-e8y`; orchestrator-beads-fabro surfaced 637
events, 21 attention items, `bd-ib-98c.*`. `docs/installing.md` is de-gated
with wording scoped to exactly that: linux x86_64, one host, the `serve` read
path.

**Doc bug 1 — the credential wrapper strips ALL caller env vars.**
`with-livespec-env.sh` execs in a clean environment, so the
`VAR=x /usr/local/bin/with-livespec-env.sh -- <binary>` form the doc showed
could never work; sentinel vars vanish. The working form is
`with-livespec-env.sh -- env VAR=x <binary>`. This path had never been
exercised: `just tui` sets no env vars, it relies on CWD.

**Doc bug 2 — "operate on another repository without changing directory"
does not observe anything.** A PWD×PATH control matrix showed PWD is what
matters and PATH is irrelevant. From any non-repo PWD with
`LIVESPEC_CONSOLE_REPO_PATH` set, the v0.2.0 asset reports **5/5 sources
`not_observed`**; from inside the repo, real data. The Beads tenant resolves
from the working directory's `.beads/` and the orchestrator plugin root is
discovered relative to CWD, so neither is reachable from `/tmp`.

**This is NOT a stale-release artifact — it is unfixed on master.** The
current master source build recovers only `dispatcher` and `fabro` (3/5 still
dark); the orchestrator source, the one a cockpit exists for, still needs
CWD, and zero work-items or attention items surface either way.
`docs/installing.md` now documents the cd-into-repo requirement rather than
the broken form.

Root cause is one omission at a single chokepoint:
`SystemSourceProbe::run_command` (`crates/console-cli/src/main.rs:321`) spawns
every backing CLI without ever calling `.current_dir(...)` — that call appears
NOWHERE in the workspace — so children inherit the console's own CWD. The
Beads tenant resolves from the child's CWD `.beads/`, `gh` infers its repo
from CWD, and `livespec` reads the spec tree from CWD: exactly the three
sources that go dark, while `dispatcher` and `fabro` (absolute-path
resolvers) survive. `BackingCliResolution` ALREADY honors
`LIVESPEC_CONSOLE_REPO_PATH` for plugin-root discovery
(`backing_cli.rs:260-262`) and for drive/drain `--repo` (`main.rs:135-144`) —
the child working directory is the one consumer that was missed, and the fix
is default-identity because the env var already defaults to the CWD.

**Filed as `livespec-console-beads-fabro-bamsy3`** (bug, DoR verdict `ready`
AT FILING — it has since been FIXED and sits in `acceptance`; see §"RESUME
ORDER" item 1 for the fix and its verification). The maintainer resolved the
design question on 2026-07-21:
`REPO_PATH` SHOULD be sufficient for full observation, so the acceptance is
autonomously verifiable. Note `SystemSourceProbe` is
`#[cfg(all(not(test), not(coverage)))]` — compiled out of every test build —
so the tmux E2E harness is the only verification path.

**The install glob now has the mechanical binding B7 asked for.**
`crates/console-cli/tests/docs_release_asset_lockstep.rs` reconstructs the
asset name from `release-binary.yml`'s own `target=`/`asset=` shell
assignments and asserts every documented `--pattern` glob matches it.
Deliberately hermetic — no GitHub API call, so an outage cannot redden CI.
Negative-tested by injecting both real drift modes (musl retarget; binary
rename); each turns the gate red. The live half was verified once, by this
run.

## DOC CUSTODY IS ACTIVE (2026-07-21) — an audit, and a claim this file got wrong

An earlier revision of this file called the remaining doc custody **passive**,
and used that to argue the thread was nearly archivable. A `docs/` audit run
hours later disproved it. Recording the correction here because the archive
decision turns on it.

**Five doc claims were contradicted by current source** (all fixed, PR #356):

| Claim | Reality | Cause |
|---|---|---|
| `Enter` offers `Open Fabro attach` / `Copy Fabro attach` | modal opens EMPTY | wrong since authoring |
| Detail pane always shows `Attach:` | conditional on a matching Fabro run | `fd6c622` |
| lane overview row `- <id> [<status>] (<reason>)` | gained a `<title>` field | `2120e62` |
| lane drill-in row, repo second | repo moved after title, `repo ` prefix | `2120e62` |
| key table: `Enter` opens command modal in a lane | opens the work-item record | `e724b9c` |

**The first one is the instructive one.** It was false the day `docs/` was
written — both production `AttentionDetail::new` call sites have always passed
`Vec::new()` for actions, verified back at `7df1ea2`. So the B6 rewrite, which
was itself an audit that corrected 16 README errors, introduced a new one. An
audit is not a permanent fix; it is a snapshot.

**Rate of rot, measured twice now.** B7 found the Status hints drifted within
ONE DAY of B6 landing. This audit found five more within a day of B7. With
several sessions committing to this repo, doc drift is continuous, so custody
means *periodically re-auditing*, not *owning a finished artifact*.

**Direct evidence that gating works and prose does not — from a single
commit.** Hours after the audit fixes landed, another session shipped
`2cd1f28` ("fix: open attention work-item records"), which changed `Enter` in
the Attention view from *open the command modal* to *open the selected row's
work-item record*. That commit **did** update the Status-hint lines in
`docs/detailed-usage.md` — because `docs_status_hint_lockstep` fails the build
otherwise — and **did not** update the Attention-pane prose or the by-focus key
table, which are not gated. Both were left asserting the old behavior; both had
been corrected only hours earlier by PR #356. Same file, same commit, same
session: the gated half moved, the ungated half rotted. Fixed again in the
auto-dispositions PR.

The practical read: a doc claim worth keeping accurate is worth a lockstep
assertion, because a conscientious author updating the same file will still
miss the ungated half. Do not treat "someone will notice" as a control.

**Acted on: `crates/console-cli/tests/docs_enter_key_lockstep.rs`.** The
by-focus `Enter` cell was the most-drifted claim in `docs/` — twice in one day
— so it is now bound to source: every `TuiView` variant `enter_content_input`
branches on must be NAMED in that cell. Verified by replaying the real
regression (delete the Attention clause, the gate fails naming `Attention`).
One-directional like its siblings: it catches a view silently gaining or
losing an `Enter` binding, and it CANNOT catch a cell that describes a named
view's behavior wrongly — prose is still prose. Three doc gates now exist
(`docs_status_hint_lockstep`, `docs_release_asset_lockstep` +
`docs_release_version_lockstep`, and this one); the honest summary is that
they pin the STRUCTURE of the doc's claims, and a periodic human audit is
still what catches wrong prose.

**What NOT to re-audit.** These were checked against source and found clean —
skip them next time unless their area changes: every Status-line hint
(gated by `docs_status_hint_lockstep`), the `s` move-to-status transition
table, the header degrade ladder, global key inertness under overlays, the
8-section Help modal, the attention row format, the whole-record modal claim,
and every TUI claim in `overview-quickstart.md` and `cli-options.md`.

**One known-silent item still deliberately left.** The record modal's own
footer prints `up/down scroll | esc to close` while `PgUp`/`PgDn` also page it
— an inconsistency INSIDE the source, not doc drift, and the Status band
documents the paging correctly. Not a false doc claim, so it stays out of
scope for a docs pass; it is a small TUI-text fix or a work-item, not a
rewrite.

**The other silent item is now DOCUMENTED.** `5938212`'s auto-disposition
vocabulary was a gap in a RATIFIED scenario — `SPECIFICATION/scenarios.md`
Scenario 15 ("Orchestrator auto-dispositions and escalations reach the
operator") is spec'd, implemented, and tested by
`scenario_15_orchestrator_auto_dispositions.rs`, yet no user doc mentioned it.
`docs/detailed-usage.md` now carries an Attention sub-section covering all
five dispositions, which four RESOLVE an inbox row and which one MAKES one
APPEAR, that the console observes rather than re-derives (the journal supplies
the governing setting verbatim), that reflection is idempotent, and that an
unparseable or out-of-vocabulary line is skipped silently rather than surfaced
as a phantom row. Worth knowing an operator could previously watch a row
vanish with nothing in the docs explaining why.

**The empty-command-modal product question ANSWERED ITSELF.** It was raised
but not filed; another session shipped `2cd1f28` hours later, making `Enter`
in Attention open the selected row's work-item record and early-returning the
command modal when it has no actions. The dead surface is gone. This is a
reason to keep raising such questions in the handoff even when not filing
them — a second session picked it up from there.

## DOCS-ROT POSTSCRIPT (2026-07-21) — my own docs were false in four hours

The B7 postscript warned that prose rots at the team's commit rate. It did so
again, faster than any previous instance, and to the very docs written to
document a bug.

The B8 run found the cross-repo gap, filed it as `bamsy3`, and landed
`docs/installing.md` wording that told users the working directory was
**load-bearing** — with a measured table showing cross-repo observation
returning nothing. Within hours the factory drained `bamsy3` and shipped
`7110eca`, which sets each backing CLI's working directory to the selected
repo. The documented limitation ceased to exist, and the doc became a
confident, evidence-backed statement of something false.

Corrected here: `docs/installing.md` now documents the working cross-repo
invocation and carries a scoped note that `v0.2.0` — the currently published
asset — still requires `cd`. That distinction matters because the de-gate
notice points readers at `v0.2.0`, so the doc must describe TWO behaviors at
once until the next release.

**The transferable lesson is narrower than "bind docs mechanically."** The
`docs_status_hint_lockstep` and `docs_release_asset_lockstep` gates bind doc
claims to SOURCE — they cannot catch this, because nothing in the repo was
inconsistent: the doc accurately described `v0.2.0` while the source moved on.
**Any doc claim scoped to a RELEASED artifact acquires a second lifetime,
independent of master.** When you document a limitation, also note what would
retire it — otherwise the fix silently invalidates the prose and no gate
fires.

Practical rule for this repo: a doc sentence describing behavior that a filed
work-item would change should name that work-item. `bamsy3` existed and was
`ready` when the false wording was written; a one-line "until `bamsy3` lands"
would have made the rot self-announcing.

## VERIFICATION DISCIPLINE (2026-07-21) — four false greens in one session

Every one was caught only by reading actual OUTPUT, never by trusting a status.
Worth internalizing before the next push:

- **A piped exit code is the pipe's.** `just check | tail` reports `tail`'s
  success. `just check > log 2>&1; echo "EXIT=$?"` is the honest form — the
  gate once "passed" this way while `check-format` and `check-clippy` were red.
- **`cargo test` is not `just check`.** All 8 tmux E2E scenes passed green while
  clippy was failing the build. Only `just check` gates a merge.
- **A negative test can pass for the wrong reason.** Removing one settings-doc
  table row left the gate GREEN — correctly, since the section's prose also
  named the key. Only removing EVERY mention proved the repoint worked. A
  negative test that passes has proven nothing until you know why it failed.
- **A commit can silently not happen.** zsh glob-expanded `?` and `<prog>` in an
  inline `-m` message; the commit aborted, the subsequent `echo PUSHED` still
  printed, and the branch pushed with NO commit on it. Use `git commit -F <file>`
  for any message containing shell metacharacters, and verify with `git log`.

## RESUME ORDER (fresh session) — updated 2026-07-20
Deliverable #0 + **B1–B8 are DONE** (see §"STATUS", §"B6 POSTSCRIPT",
§"B7 POSTSCRIPT", and §"B8 POSTSCRIPT"). §"B8 ENTRY NOTE" above is now
HISTORICAL — its three questions were answered and the run is complete; keep
it only for the reasoning. Remaining, in order:
**Nothing docs- or release-shaped remains in this thread.** What is left is
either owned elsewhere or already split out as a standalone work-item:

1. **`livespec-console-beads-fabro-bamsy3`** — ✅ **FIXED by the factory the
   same day**, now in `acceptance` awaiting the human accept valve. Landed as
   `7110eca` "fix: run backing CLIs from selected repo"
   (`.current_dir(&self.cwd)`, `main.rs:334`). Nothing to do here beyond the
   accept decision — it never needed a plan thread.

   **Independently verified 2026-07-21** against the exact reproduction filed
   with the bug: from `/tmp/<rand>` (not a git repo) with
   `LIVESPEC_CONSOLE_REPO_PATH` pointed at orchestrator-beads-fabro, the
   rebuilt master binary reports **0 `not_observed`** (was 5/5 on `v0.2.0`,
   3/5 on pre-fix master) and reaches the `bd-ib-*` tenant. Against the
   acceptance wording — "observes the same source set as one launched from
   inside that repo" — the two runs are **identical**: 620 backfill events,
   649 events, 21 attention items either way.
2. **Backfill Scenarios 5 / 9 / 11** — RE-HOME, do not start here. 5 and 11 to
   `plan/console-happy-path-mvp/`; 9 as a standalone work-item. See §"BACKFILL"
   for why, and for the three errors that section used to carry.
3. **Doc custody** — the only standing responsibility left, and it is
   **ACTIVE, recurring work** — see §"DOC CUSTODY IS ACTIVE" for the evidence
   that corrected an earlier claim in this very file.
   `plan/console-happy-path-mvp/handoff.md` explicitly defers to this thread
   for it ("Doc custody stays with `plan/cockpit-ux-docs-release/`"). Whoever
   ends up holding it inherits a periodic audit, not a dormant label — that is
   the thing to weigh before archiving this thread.

**~~Stage-2~~ (autonomous-mode MVP acceptance) — STRUCK 2026-07-21, DEAD.**
Its tracking thread no longer exists at
`livespec/plan/autonomous-mode-acceptance/` — it is ARCHIVED at
`livespec/plan/archive/autonomous-mode-acceptance/` — and autonomous mode is
retired for good (four independent sources, incl. `.livespec.jsonc:47` and
`plan/console-happy-path-mvp/research/why-it-never-happened.md:71`). Do not
resume it.

> **Cross-thread inconsistency, UNRESOLVED.**
> `plan/console-happy-path-mvp/handoff.md:95` still reads *"cockpit's Stage-2
> (multiple real items, two repos) remains cockpit's"*, treating Stage-2 as
> live. That contradicts the strike above. Whichever thread is archived first,
> that line needs correcting or the contradiction outlives both. NOT edited
> here — it is another thread's handoff.

Each behavior: `/livespec:propose-change` → independent Fable review →
`/livespec:revise` → tmux E2E (RED) → implement (GREEN) → two-repo live-verify.
Console repo mutation protocol (worktree → PR → merge) + `just check` throughout.
