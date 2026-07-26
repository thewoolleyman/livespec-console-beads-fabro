# `-l4p3ce` handoff transport — design input (DECIDED 2026-07-26)

Drafted 2026-07-25 by the console-happy-path-mvp brainstorm leg, under
this thread's maintainer entry gate (satisfied; the seven vocabulary
points are decided — see
`plan/console-happy-path-mvp/research/verb-vocabulary-brainstorm.md`).
**All four open questions were maintainer-decided 2026-07-26 (relayed
via the supervisor session) — see § "The four transport decisions".**
This remains the INPUT to the `-l4p3ce` spec conversation and still
routes through this thread's ratification gates; the vocabulary halves
route orchestrator-side first (filed there as that repo's PR #975).

## Fixed constraints (decided or contractual, with verification)

- **No console→driver dependency** (locked core contract): the console
  never spawns, monitors, or depends on the driver session. It prepares
  a handoff the operator executes.
- **No clipboard backend exists** — re-verified 2026-07-25 on master:
  zero hits for OSC 52 / xclip / pbcopy / wl-copy / clipboard across
  `crates/`. The `CopyFabroAttach` scaffold remains dead code (this
  thread's handoff, § `-l4p3ce`).
- **Verb targets** (decided points 1, 3, 7): `groom` fires on any
  `backlog` item and the item STAYS `backlog` through the conversation;
  `driver-implement` fires on factory-unsafe `ready` items only (the
  set the dispatch-admission host-only refusal already refuses to
  sandbox, `_dispatcher_admission.py:82-86`) and is a journaled door
  `ready → active`.
- **MVP shape** (happy-path Stage-1 agenda): prompt to a tmp file;
  short copy-paste-safe driver command; full-width render + Copy.

## Prior-art survey (the item's explicit research task)

⚠ From working knowledge, to be spot-verified before ratification —
none of these claims is load-bearing for the MVP cut, they locate it.

- **lazygit** `customCommands`: key-bound shell templates interpolating
  the selection (`{{.SelectedCommit.Sha}}`…), with output modes
  (suspend-and-run in terminal, output panel, background) and prompt
  menus for arguments.
- **k9s** `plugins.yaml`: key-bound shell with `$NAMESPACE`/`$NAME`
  interpolation from the selection, optional confirm, foreground
  (suspends the TUI) or background.
- **tig**: key-bound external commands with `%(commit)`/`%(file)`
  interpolation; flag characters select exec mode (silent, refresh,
  pager).
- **gitui**: fixed external-editor suspension only — no general
  mechanism.

Common shape: **selection-interpolated shell template + exec-mode
flag**. Tmux-aware community configs wrap the template in
`tmux new-window …` when `$TMUX` is set, so the TUI keeps running while
the handoff opens beside it.

**The key divergence:** all four RUN the command themselves. Our locked
contract points the other way — the MVP renders the command and the
operator executes it. Auto-spawn (`tmux new-window`) is a possible
post-MVP upgrade **contingent on a contract conversation**, not part of
this cut.

## Proposed MVP design (to react to)

1. **The command is just the skill invocation — the tmp-file leg may be
   unnecessary.** This is the draft's main finding, and it SIMPLIFIES
   the agenda's sketch: both heavyweight verbs resolve to skills that
   take only the work-item id and read the ledger themselves
   (`/livespec-orchestrator-beads-fabro:groom <id>`, and the
   driver-implement equivalent). The console therefore needs to render
   only, e.g.:

   ```
   claude "/livespec-orchestrator-beads-fabro:groom livespec-console-beads-fabro-zweohm"
   ```

   No prompt composition, no file lifetime management, no staleness
   window between writing the prompt and pasting it. The tmp-file slot
   (`tmp/livespec-console-handoffs/<item-id>-<verb>.md`, repo-local
   `tmp/` like the console store, never host `/tmp`) stays RESERVED in
   the design for future verbs that genuinely need composed context,
   but the MVP ships WITHOUT it — decided 2026-07-26, see § "The four
   transport decisions" (a), including why the slot must stay unbuilt.

2. **Render: full-width overlay, not the detail pane.** `-vc7lmq`'s
   history shows pane truncation destroys copyability; the command
   renders in a dedicated full-width modal (one line, horizontal
   scroll if the terminal is narrower than the command).

3. **Copy: OSC 52 with render-only fallback.** OSC 52 needs no
   external binary (works through modern terminals and tmux with
   `set-clipboard on`), which respects the no-new-dependency posture;
   where unsupported, the overlay still shows the command for manual
   copy. IN-CUT — decided 2026-07-26, WITH the normative
   no-claimed-success wording constraint in § "The four transport
   decisions" (b).

4. **Journaling stays driver-side — the console renders, the driver
   reports in.** `groom`: no journal, no lane move (the groom skill's
   own `close_regroomed_out` writes the outcome). `driver-implement`:
   the DRIVER session opens with a drive action (working name
   `driver-dispatch:<id>`) that journals actor + session ref and moves
   `ready → active` — decided point 3's door, realized without the
   console knowing whether the paste ever happened. This keeps the
   no-console→driver-dependency contract exactly: an un-pasted handoff
   changes nothing anywhere.

5. **New orchestrator surface implied (for the amendment set):** the
   single `driver-dispatch:<id>` drive action (valid on factory-unsafe
   `ready` items, journaled, session ref required). `groom` implies no
   new orchestrator surface.

## The four transport decisions (maintainer-decided 2026-07-26)

**(a) Tmp-file leg: the simplification is ACCEPTED — the MVP ships
with NO tmp-file leg.** The slot stays RESERVED in this design and is
NOT built. Reasoning, recorded so a future reader does not "helpfully"
build it: this repo already carries `CopyFabroAttach`, a scaffold built
ahead of need that later read as a shipped feature and misled two
sessions into believing a copy path existed. A reserved slot with a
written rationale is the antidote; an unbuilt-but-half-plumbed
mechanism is the disease.

**(b) OSC 52: IN-CUT, with a NORMATIVE wording constraint.** Copy via
OSC 52 with render-only fallback. Normative, not a style note:

> The overlay MUST NOT claim the copy succeeded. OSC 52 is
> fire-and-forget — the terminal returns no acknowledgement, so
> success is UNKNOWABLE to the console. Permitted wording describes
> what the console DID ("copy sent to terminal"); forbidden wording
> asserts an outcome it cannot observe ("Copied!", "Copied to
> clipboard").

The reason is part of the design: on a terminal with clipboard
disabled, an outcome-asserting message leaves the operator pasting
stale content while believing they copied — a new lie in the exact
surface this thread exists to make truthful, and one a test can pass
straight through.

**(c) tmux auto-spawn: POST-MVP, gated on a contract conversation —
explicitly OUT of this cut and explicitly NOT dead.** Both halves,
because "out of scope" alone reads as "rejected": the locked
contract's PURPOSE is to stop the console taking a dependency on
driver liveness, and a fire-and-forget `tmux new-window` arguably does
not — nothing is monitored, awaited, or depended upon — while the
contract's LETTER ("never spawns") forbids it. All four surveyed TUIs
run the command themselves, so the ergonomic norm is on the other
side. That gap is a real contract conversation worth having later.
What must NOT happen is auto-spawn arriving as a convenience without
that conversation.

**(d) Driver-implement form: REUSE the existing `implement`
operation.** The handoff renders the existing
`/livespec-orchestrator-beads-fabro:implement <id>` invocation; no new
driver-side surface is built. Reason: every added surface carries an
ongoing availability cost, and the fleet has a live cautionary case —
`supervise-plan` is BUILT but available in nobody's session, which is
the entire subject of livespec-overseer's `ship-overseer-to-fleet`
thread.

**Custody flag on (d), not yet routed:** this was a DRIVER-PLUGIN
question answered in the console's thread. The decision stands, but
`livespec-driver-claude` and `livespec-driver-codex` now have an
implied consumer they do not know about — the console will render
their `implement` invocation as a handoff target. Telling them is
owed; routing that notice is the supervisor's, and nothing is filed in
those repos from here.

## Keybinding

Deliberately NOT designed here: key assignment is console-side
presentation, sequenced after the orchestrator-side vocabulary
ratifies (`g` is currently taken by the merge-on-review-cap override).
