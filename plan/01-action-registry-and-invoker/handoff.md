# 01-action-registry-and-invoker — handoff

**Epic anchor:** `livespec-console-beads-fabro-dvv` — status is READ from the ledger
(`list-work-items` / `next`), never stored here.
Opened 2026-08-02 on the maintainer's menu-primary decision.

## Mission

One **ACTION REGISTRY** as the single source of truth for every operator action, plus a
minimal generic **INVOKER** so every action is reachable TODAY, before menus exist.

This plan is the foundation the whole arc stands on. Plans 02, 03 and 04 all consume the
registry; if it ships flat or half-wired, each of them inherits a migration.

## The decision this plan implements

Maintainer, 2026-08-02, **quoted verbatim — do not paraphrase it anywhere**:

> "I would rather every action be available via MENUS and DIALOGS as the first-class,
> required, primary navigation UX mechanism. And hotkeys are only provided IN ADDITION
> to first-class menu operation AS A POWER-USER CONVENIENCE."

and on proof:

> "ideally some mechanical, generic test to prove that EVERY action can be driven via
> MENUs, and hotkeys are ONLY ADDITIONAL."

**This is not a compromise reached under constraint.** The maintainer confirmed it
independently: *"I'm already decided on menu primary - definitely want this, it's a
better UX for me, and for agents."* Do not re-litigate it, and do not treat the invoker
this plan builds as the destination — see § "The invoker is scaffolding".

## Requirements — design contract, not suggestions

1. **Registry entry carries, from day one:** action id (stable), human label, parameter
   schema, availability predicate, handler, **and menu_path / category taxonomy**.
   The taxonomy is REQUIRED NOW even though menus do not exist yet, because 02 GENERATES
   menus from it. **Shipping a flat registry creates exactly the schema migration this
   sequencing exists to avoid.**

2. **Availability predicates centralized and multi-dimensional.** A predicate MUST be
   able to depend on everything the action needs — at minimum lifecycle lane **AND**
   effective admission policy. One derivation, consumed by BOTH presentation (is the
   action offered?) and invocation (does it fire?).
   *This is the `-0uw` fix.* Measured 2026-07-30: the Status band advertised `p approve`
   on a `pending-approval` item whose effective admission was `auto`; the valve could
   not fire; the operator pressed the advertised key twice and nothing happened.
   The predicate keyed on lane only. `awaits_dispatcher_admission` already models the
   missing dimension orchestrator-side — consume it, do not re-derive it.

3. **REWIRE the existing hotkeys and Status-band hints THROUGH the registry, in THIS
   plan.** No parallel encodings.
   **This is 01's definition of done and it is non-negotiable.** It is also the piece
   that gets cut when a milestone runs long, which is exactly why it is named here.
   Leaving the legacy hint tables standing beside the registry reintroduces the
   second-unbound-encoding defect `-mbohw3` existed to kill — that item found THREE
   encodings of one vocabulary, and its sibling `-nvflph` found FOUR.

4. **Minimal generic INVOKER:** select an action from the registered list, fill a
   parameter form, invoke, **and RENDER THE RESULT — success or the full refusal
   payload.**
   - Fixes `-w7d`: `set-workflow-scope-override:<id>:citation-only` becomes reachable
     in-cockpit. Today it is a human valve the orchestrator instructs operators to use
     and the console binds no key to it at all.
   - Fixes the action-invocation half of `-ectqye`: the refusal payloads are RICH,
     STRUCTURED, ACTIONABLE and ALREADY WRITTEN — one literally names the command that
     would unblock the operator — and they are discarded at the presentation boundary.
     The background/journal half belongs to 03; do not absorb it here.

5. **Tests, each MUTATION-DEMONSTRATED RED** (this repo's standing "a verifier must be
   able to fail" rule; a gate demonstrated red in one direction is evidence about that
   direction only):
   - **cross-repo parity** — the registry accounts for the orchestrator's PUBLISHED
     human action surface. **This check should be BORN RED against the known-missing
     valve actions, and that IS its red demonstration** — bank the red output before
     making it green.
   - **no-orphan-hotkeys** — every key binding maps to a registry id.
   - **console-arch-check lint** — no key handler bypasses the registry.

6. **Spec first.** The propose-change is FILED at
   `SPECIFICATION/proposed_changes/menu-primary-operator-ux.md` (this plan's PR).
   **File, do not ratify** — ratification is the maintainer's `revise` pass.

## The invoker is scaffolding, deliberately

Menu-primacy is DECIDED. The invoker exists so that (a) every action is reachable before
menus land, and (b) 01 has a real dogfood leg. It is **not** a feature and **not** the
destination.

- Do **not** polish it. Do **not** write a docs section for it.
- Do **not** grow it into a command palette. The existing `:` palette accepts exactly
  two commands and is not the model here.
- When 02 lands menus, the invoker becomes a completeness surface behind them.

Recording this because the obvious failure mode is a successor investing in the
temporary surface and then defending it.

## Milestone acceptance — including the DOGFOOD leg

Dogfooding is not a final phase; every plan in this arc ends with a real-stack leg.

**01's dogfood leg resumes the staged asset from the parked thread.**
`livespec-console-beads-fabro-ccycuk` sits at **`ready` with
`acceptance_policy=ai-then-human`** — deliberately staged and untouched since
2026-07-30. **Do not re-stage it, do not move it, do not re-run `n`.**

The leg, in order:

1. From the cockpit, via the NEW INVOKER, apply
   `set-workflow-scope-override:livespec-console-beads-fabro-ccycuk:citation-only`.
   **This is the legitimate in-cockpit surface that did not exist when the old pass
   stopped.** The claim is factually true: slice A only READS `.github/workflows/ci.yml`
   and never writes it. The maintainer's authorization for the override is implicit in
   commissioning this design — but say so in the record rather than leaving it inferred.
2. Drain at the TUI. **No `drive.py` fallback** — if it wedges, STOP AND REPORT.
3. Monitor.
4. `c` accept.

**Record it as "walked via the registry invoker", with the same honesty labels this arc
has always used.** A leg discharged outside the surface is an OPEN leg with a
workaround. Requested is not dispatched. A hermetic pass is not a real-stack pass.

This completes the old thread's unbroken-pass evidence ON THE NEW SURFACE, at the FIRST
milestone rather than the last — which is the point of the sequencing.

## Why -ccycuk was blocked, since the next session will hit it

The drain refused it on 2026-08-02's predecessor session:

    stage: host-only-refused   status: failed
    "factory-safety refusal: ... declares an edit under .github/workflows/, a withheld
     sandbox capability ... If the workflow path is citation-only, set the recorded
     override with `set-workflow-scope-override:<id>:citation-only`"

**And the refusal fires on the PATH MENTION, not the intent.** Slice A's description
says prominently that it READS `ci.yml` and MUST NOT create or update anything under
`.github/workflows/` — which is precisely the "inline negation declaration" the refusal
says would satisfy it. It was refused anyway. **That inverts the incentive: it rewards
not writing the constraint down.** Filed as part of `-w7d`; the matcher lives in
`_dispatcher_host_only.py` and should be MEASURED before anyone "fixes" it.

## Ledger

Epic `livespec-console-beads-fabro-dvv`. Edges are on the ledger, not in this prose —
read them with `bd dep list`. Tracked: `-w7d`, `-0uw`, `-ectqye` (action-invocation half
only), `-ccycuk`, `-koykn7`. Blocks: `-et3` (02) and `-1df` (03).

## Read-first

1. `research/operator-surface-redesign-decision.md` — how this decision satisfies the
   operator-surface-redesign thread's brainstorm entry gate, and the custody map from
   absorbing it. That thread is ARCHIVED at `plan/archive/operator-surface-redesign/`.
2. `plan/console-happy-path-mvp/handoff.md` §§ 0h–0j — the walk evidence this plan
   inherits, including the three silent-refusal paths and the staged asset.
3. `plan/archive/work-item-lifecycle-redesign/research/locked-core-contract.md` — the
   invariants every slice obeys (zero Beads knowledge; commands only through the
   orchestrator surface; lane consumed never re-derived; attention as pure derivation;
   no console→driver dependency).


## RESUME HERE — state at 2026-08-02T~10:30Z (SUPERSEDES every block below)

**MILESTONE 2 IS UNBLOCKED AND SPLIT. Slice A is BUILT, GREEN, and awaiting merge.**

| PR | slice | state |
|---|---|---|
| **#585** | **A** — parity gate + `set-workflow-scope-override` + the coverage disposition | **14/14 CI GREEN**, open |
| #588 | B — DRAFT, **plan only, no implementation**, based on A's branch | open |

**THE COVERAGE BLOCKER IS DISPOSITIONED, NARROWLY.** Maintainer-authorized 2026-08-02
after ONE bounded pass FAILED and the follow-up compilation-count experiment came back
INCONCLUSIVE. `check-coverage` now compares llvm-cov's summary against its own
`--show-missing-lines` listing and caps only the unnameable residue via
`tests/fixtures/coverage-unnameable-disposition.json`; logic in
`dev-tooling/coverage-gate.py`. **The 100% requirement for attributable lines is
UNCHANGED, § 0c is untouched, and a single NAMEABLE miss still fails** — proven by four
red mutation demos plus a green control, and fail-closed if the fixture is missing.
Tracking `-3yx`. **B and C inherit it ONLY on the IDENTICAL signature.** Allowance may be
reduced, never raised without new recorded authorization.

**NEXT, in order:** merge #585 -> rebase `feat/slice-b-action-invoker` -> build slice B
from `research/slice-b-build-plan.md` (it carries the TWO non-obvious cherry-pick
conflict resolutions and the enumerated 54-site slice-C removal list — do not re-derive
them) -> then slice C.

**The dogfood leg is HELD until A AND B are BOTH MERGED.** B is a plan today, so it is
not satisfied. Conditions unchanged (fresh build, one client via `/proc/*/exe`,
plugin-root override inside the credential wrapper, invoker -> drain -> monitor -> `c`).
`-ccycuk` untouched at `ready` + `ai-then-human`.

**Milestone-1 component note:** the menu-primary propose-change was going through the
maintainer's `revise` pass in the supervisor pane at wind-down.
**`SPECIFICATION/proposed_changes/` and `history/` were OFF-LIMITS** — confirm it landed
before touching either.

**Verified for plan 02 before it opens** (see its charter): `menu_path` shipped as
required, but `hotkey` shipped MANDATORY on master and only slice A widens it to
`Option<char>` — so no keyless menu entry is expressible until A merges. And 02's
"every action via menus" is UNDEFINED: all ten registry entries are per-item verbs under
one top-level node, and structural keys are forbidden as registry hotkeys. That choice
decides whether 02's completeness gate is a verifier or a tautology.

---

## Earlier block — state at 2026-08-02T08:5xZ (HISTORICAL)

**MILESTONE 1 IS MERGED. MILESTONE 2 IS BUILT AND CANNOT PUSH. THE COVERAGE QUESTION
WENT TO THE MAINTAINER AND CAME BACK: ONE BOUNDED PASS WAS SPENT AND IT FAILED. THE
DECIDED PATH IS NOW A THREE-WAY SPLIT — A -> B -> C.** Re-measure everything; every
claim here is timestamped.

### The bounded pass — SPENT, FAILED, NOT TO BE RE-ENTERED

Authorized as ONE keyed pass (regions newly-missed vs master, not blind execution
fixes) under a stop-regardless discipline. It did not find the line;
`just check-coverage` is still `RC=1`. **Do not open a second pass** — that decision has
been taken once and the blind alley is documented.

What it settled, and why the record is worth more than the failure costs:

- **FIVE listing surfaces are blind to it**, four of them newly proven this pass:
  `--show-missing-lines` prints no missing-line section; JSON segment reconstruction
  yields ZERO uncovered lines; the annotated `--text` listing renders 7,836 instrumented
  lines with none at count 0; and the keyed step found **all 42 zero-count segments sit
  on lines the diff never touched** — the 591 added lines contain no zero-count segment
  at all.
- **The commit-boundary bisect is a PROVEN dead end.** `ab49fc4` measures 43 missed
  lines, but only because its tests land in the tip commit. Intermediate commits are not
  coverage-gated, so that boundary cannot localize anything. Do not repeat it.
- **Filed as `-3yx`** (P2 bug, `tracks` under `-dvv`) with a falsifiable prediction: the
  phantom tracks the NUMBER OF COMPILATIONS of `console-application` in the merged
  profile, not the diff. Two crate disambiguators are present and 91 function records
  read `count == 0` while the summary reports zero missed functions.
- Forensics: `research/coverage-phantom-bounded-pass.md`.

### THE SPLIT — APPROVED A -> B -> C. This is the path forward.

Each slice is a separate PR carrying its own tests, so **each must independently pass
the coverage gate** — the constraint the commit boundary never imposed, and the reason
this localizes where the bisect could not. `-3yx` records the verdict: either the
phantom localizes to one slice, or it vanishes/persists uniformly. **Either outcome is
evidence.**

| slice | content | note |
|---|---|---|
| **A** | cross-repo parity gate + `set-workflow-scope-override` binding (`-w7d`) | smallest, `lib.rs` +78 |
| **B** | the action invoker (`TuiOverlay::ActionInvoker`, palette roster, confirm staging) + docs | **LEG-CRITICAL** |
| **C** | the `-ectqye` action half (refusal capture, `error_json`, latest-failure projection, record-modal render) | on no critical path |

**A + B is the minimum that unblocks the dogfood leg, and C is not needed for it.**
`set-workflow-scope-override` ships `hotkey: None` — the first deliberately
menu/invoker-only action — so A alone does not make it reachable.

**Separability was MEASURED, not inferred:** the refusal renders in the record modal,
the invoker overlay references `refusal` nowhere, and they share no rendering path. The
charter states requirement 4 as one clause; **the charter wording was wrong, not the
reading** — maintainer-confirmed 2026-08-02.

> **That anchor is BRANCH-RELATIVE — `console-tui/src/lib.rs:1454` on the
> `feat/action-invoker` branch, NOT on master.** Slice C is unmerged, so on master that
> line is `let detail = item.detail();` and a reader following it lands in unrelated
> code. Caught 2026-08-02 by sweeping every `file.rs:NNNN` citation in the live plan
> handoffs against master. **General rule for this arc, since three of the four plans now
> carry source anchors: an anchor into unmerged work MUST say which branch it is on.**
> A stale anchor and a branch-relative anchor fail the same way — the reader lands
> somewhere plausible and wrong — but only the stale one gets caught by re-measuring
> against master.

**SLICE A ACCEPTANCE CARRIES AN EXTRA ITEM:** the parity gate's **omission-reason arm is
still NOT mutation-demonstrated**. The forward arm was born red and the reverse arm is
now demonstrated red; the third is owed and is pinned to slice A so it cannot slip
again.

### Deliberate deferral, recorded as a DECISION not an oversight

The full workspace suite was **not re-run after the rebase**. Verified post-rebase:
`check-e2e-tmux` (GREEN, 11/11), format, clippy, arch. The "everything else green" line
in the older block below is a PRE-rebase measurement. Accepted by the maintainer
2026-08-02 on the grounds that **each split PR runs the full gates at pre-push**, which
converts it into a current measurement per-slice.

### The dogfood leg — HOLD LIFTS once A and B are BOTH merged

Then, in order: fresh `just tui` build (**the live cockpit binary is 2026-07-30 and is
STALE** — PID 1883911 was still running at 08:0xZ), verify exactly ONE live client via
`/proc/*/exe`, then `set-workflow-scope-override:livespec-console-beads-fabro-ccycuk:
citation-only` **FROM THE COCKPIT**, drain at the TUI (no `drive.py` fallback), monitor,
`c` accept. `-ccycuk` remains `ready` + `ai-then-human`, untouched. Recording conditions
unchanged: a leg discharged outside the surface is an OPEN leg with a workaround.

### Also decided 2026-08-02

- **`-6hbfq6` janitor authorization GRANTED, THIS ITEM ONLY**: reduced janitor argv
  dropping ONLY `check-fork-drift`, recorded with the time-inconsistency rationale (its
  own pre-push ran that gate green at merge time). The `dm5f7q` precedent applies
  verbatim. **NOT a standing policy** — expect recurrence per-item for any pre-bump merge
  until `-3r6` lands, and each recurrence goes back to the maintainer.
- **`plan/operator-surface-redesign/` is ABSORBED AND ARCHIVED** at
  `plan/archive/operator-surface-redesign/`, custody named and accepted BEFORE archival
  (`-zweohm`/`-vc7lmq`/`-l4p3ce` -> 02; `-ipi` -> 03; ledger edges re-pointed and
  verified). See `research/operator-surface-redesign-decision.md`.
- **Plan 04's mission is AMENDED and SETTLED** — the assumption flag is out.
- **Plans 02–04 implement IN-SESSION**: worktree -> PR -> full gates -> rebase-merge;
  factory dispatch only for well-bounded sandbox-safe slices, choice recorded per slice.

### Branch state after this session

`feat/action-invoker` REBASED onto master `d765aae`: `ab49fc4` -> **`e3d82eb`**,
`35a8c2b` -> **`66b4fd2`**, plus `787ead7` and `f51dceb` carrying the banked evidence.
Still unpushable (same one line). The evidence docs are duplicated into THIS PR so they
have a forge copy independent of that branch.

---

## The 07:4xZ block (HISTORICAL — read the block above first)

### What is DONE and on master

- **PR #573 -> `64131cd`**: the ACTION REGISTRY (`console_application::action_registry`)
  with hints, hotkeys, and Help rosters DERIVED from it; the ten-bit hint mask, both
  hint string tables, and the four bespoke key helpers deleted; byte-identity with every
  pinned hint string test-proven; the `-0uw` fix (approve suppressed AND inert on
  dispatcher-admitted items, docs rows added); invocation-side enforcement (reducer +
  confirm consult the same derivation); `docs_status_hint_lockstep` upgraded with a
  per-context RENDERED-EQUALITY arm + fourteen-context completeness list;
  no-orphan-hotkeys tests; console-arch-check `check_registry_bypass` (syn rule, paired
  tests). Three live mutation demos recorded in the PR body. Behavior change recorded
  deliberately: `h` is now INERT on the Attention surface (was live-but-unhinted).
- **PR #571 -> `f3c8c93`**: fork pins re-pinned against plugin `c402de396ee3` (fifth
  firing; one-line upstream docker bump, not ported).

### Milestone 2 — COMMITTED LOCAL-ONLY, worktree `feat/action-invoker`

Branch `feat/action-invoker` at **`35a8c2b`** (parent `ab49fc4`, parent `64131cd`),
worktree `~/.worktrees/livespec-console-beads-fabro/feat/action-invoker`, CLEAN. NOT
pushed: the pre-push gate runs `just check-coverage`, which is RED by exactly ONE line
(below). Everything else measured GREEN on this tree: workspace tests 38/38 binaries
(app 298, tui 108, cli 133+), fmt, clippy, arch (the registry-bypass lint now allows
`invoker_confirm_step` as a second registry-consulting staging path), docs gates.

What the two commits carry:

- `ab49fc4` — the **cross-repo parity gate BORN RED** (fixture
  `tests/fixtures/drive-human-action-surface.json` pinning the orchestrator's
  eleven-prefix human valve surface, bidirectional, mandatory omission reasons; red
  output banked in `research/parity-gate-born-red.md`) and then green by binding
  **`set-workflow-scope-override`** (`-w7d`): new CommandType, `PendingValve::
  SetWorkflowScopeOverride`, registry entry with `hotkey: None` — the FIRST
  menu/invoker-only action — full spine rehydration, scope allowlist `citation-only`.
- `35a8c2b` — the **ACTION INVOKER** (palette `:` -> `actions` -> roster of every
  registered action, unavailable rows marked `(unavailable here)` and inert, Enter
  stages the normal confirm flow; `TuiOverlay::ActionInvoker`) and the **`-ectqye`
  action half**: `run_action` failure now CAPTURES the drive stdout
  (`OrchestratorActionOutcome::Failed { refusal }`), the failure event + command
  `error_json` carry it, the model projects latest-failure-per-item (cleared on a later
  completed action), and the record modal renders `Last action: <id> refused —
  <domain_error>: <summary>`. Docs updated (palette prose, action-invoker section,
  overlay hint rows).

### THE ONE BLOCKER — a coverage phantom the maintainer must disposition

> **RESOLVED 2026-08-02 — the three options below were put to the maintainer and option
> (a) was chosen, spent, and FAILED. The decided path is now the A -> B -> C split. Do
> not re-offer these options and do not open a second iteration pass.** The measurements
> in this subsection remain accurate and are superseded only in their disposition; the
> current record is the block at the top of this file plus
> `research/coverage-phantom-bounded-pass.md` and ledger item `-3yx`.

`just check-coverage` reports **one missed line in `console-application/src/lib.rs`
(7928 lines, 7927 covered)** that NO llvm-cov listing surface can name. Measured
exhaustively on this tree, each probe run fresh:

- lcov export: LF=7917/7928 vs **7823 DA records, NONE zero** (LF > #DA — the same
  `LF/LH vs DA` inconsistency `plan/console-happy-path-mvp/handoff.md` § 0c item 2
  recorded for slice C, where 4 such lines existed).
- JSON export: zero zero-count non-gap region entries unaccounted, zero zero-count
  expansions (recursive), zero all-zero function instantiation GROUPS (after fixes).
- Four independent reconstructions of llvm's line accounting (max-entry, gap-aware,
  per-function-group union, per-group sums) each report 100% covered.
- **CONTROL: master (`64131cd`) is clean — 21,783/21,783.** The phantom is
  diff-attributed but unattributable to a line.
- Six targeted execution-fixes were applied during the hunt; five closed REAL gaps
  (misses went 2 -> 1): the `valve_is_available` scope arm in the app binary, the
  projection's malformed-payload edge, the handler's empty-id edge, the inherited
  `read_action` default on the cli test port, ActionFailure derives. The sixth
  (derived Debug arms of the new enum variants) did not move the number.

**The maintainer decision** (never a worker's: the never-weaken-a-check rule):
(a) authorize continued execution-iteration (§ 0c precedent, unbounded cost);
(b) authorize splitting/bisecting the branch into smaller PRs to isolate the
construct; or (c) an explicitly-authorized narrow disposition of the one-line
summary artifact. Until then the branch stays local; do NOT `--no-verify`.

### Remaining after the blocker clears

1. Push, PR (body draft exists but is session-local — rewrite from this section and
   `research/parity-gate-born-red.md`; include the three PR-1-style mutation demos:
   parity reverse-arm red is still OWED as a live demo), `just check-e2e-tmux` before
   push (green on this tree as of `64131cd`+M2? — NOT yet re-run after the invoker
   commits; RUN IT), rebase-merge.
2. **The dogfood leg** (charter § Milestone acceptance): rebuild the cockpit, invoker ->
   `set-workflow-scope-override:livespec-console-beads-fabro-ccycuk:citation-only`,
   drain at the TUI (NO drive.py fallback), monitor, `c` accept. `-ccycuk` still sits
   `ready` + `ai-then-human`, untouched. The plugin-root override for the cockpit and
   the known dispatch hazards are in `plan/console-happy-path-mvp/handoff.md` § 0-RESUME.

### Findings surfaced this session (recorded, not fixed here)

- **`-6hbfq6` reconcile is STRUCTURALLY blocked** (bd comment on the item, 2026-08-02):
  the post-merge janitor checks out THE MERGE SHA (`_dispatcher_engine_janitor.py:171`,
  `ref = merged.merge_sha`), so `886011d`'s committed pins can never satisfy the
  host-coupled `check-fork-drift` after a plugin bump; the master-tip re-pin cannot
  reach it. ANY item merged before a plugin bump is unreconcilable on this host. The
  sanctioned exit is the `dm5f7q` precedent — a maintainer-authorized reduced janitor
  argv dropping only the provably-time-inconsistent `check-fork-drift` (which ran green
  in PR #527's own pre-push). Decision owed.
- **Upstream reject divergence, verified at source**: the orchestrator's RATIFIED
  per-lane table (contracts.md:1297) admits `reject` at `pending-approval`; shipped
  `_reject_item` (`_drive_valves.py:184-213`) refuses any status except `acceptance`.
  The console's vocabulary-derived `r reject` hint on a pending-approval item is
  therefore advertised-but-refused today (silently, pre-`-ectqye`-fix). Upstream-owned.
- **The shipped binary never sends the driver-handoff OSC 52 copy**: only the deferred
  test sink handles `CopyDriverHandoff`; the store-backed sink drops it
  (`command_append_from_tui_effect` drop arm) while the overlay says "copy sent to
  terminal". Worth a work-item.
- The parity capture records two more upstream divergences with reasons in the fixture:
  `set-workflow-scope-override` shipped-but-unpublished; `driver-dispatch`/`defer`
  published-but-unimplemented.

## Standing rules

worktree → PR → rebase-merge, never the primary; `mise exec -- git` so lefthook runs; a
fresh worktree needs `just install-worktree-pack` and its `.livespec.jsonc` write
reverted; `bd` needs the `/usr/local/bin/with-livespec-env.sh --` prefix; verify against
the FORGE by CONTENT or patch-id (this repo rebase-merges, so ancestry is the wrong
check); outcomes from ARTIFACTS not exit codes, and an exit code read through a pipe is
the last command's; never `--no-verify`; never touch `.github/workflows/` or another
session's worktrees.

**Known live hazards, all measured 2026-07-30:** a DEFAULT operator cannot dispatch on
this host (`-pj5g3f`; the plugin-root override must be injected INSIDE the credential
wrapper); the drain runs INLINE and freezes the cockpit (`-htp`, 03 owns it); the janitor
gate omits `check-e2e-tmux` so a run can publish a PR that CI immediately fails (`-drn`,
regroomed into `-ccycuk`/`-koykn7`); and a probe keyed on an argv string self-matches its
own shell — key on `/proc/*/exe` or `python3 …`.
