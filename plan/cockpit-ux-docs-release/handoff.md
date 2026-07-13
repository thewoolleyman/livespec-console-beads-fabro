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
scenarios that today have ONLY scripted-runner coverage — at minimum Scenario 5
(TUI-first workflow), 9 (autonomous enable), 11 (valve/policy — the valve path this
session proved manually), 13 (source availability). Each existing scenario that
asserts TUI-visible behavior needs a tmux E2E test.

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

## RESUME ORDER (fresh session)
1. **Build deliverable #0** — the tmux real-TUI E2E harness (decide placement/lang;
   ensure CI has tmux); prove it by asserting one existing known-good behavior (e.g.
   the valve path from Scenario 11) end-to-end via tmux, for two repos.
2. **B1 sources** — root-cause + propose-change refining Scenario 13 + tmux E2E +
   fix. (Good first behavior: concrete, maps to an existing scenario.)
3. Then **B2 → B3 → B5 → B4** (sequenced TUI), interleaving **backfill** E2E tests.
4. **B6/B7 docs** after the TUI settles; **B8 release pipeline** in parallel.
5. Every step spec-first, Fable-reviewed, tmux-E2E-driven, two-repo live-verified.

Related but SEPARATE: the autonomous-mode MVP Stage-2 acceptance
(`livespec/plan/autonomous-mode/handoff.md`, `livespec-bvuy4w`) is still maintainer-
gated and independent of this program.
