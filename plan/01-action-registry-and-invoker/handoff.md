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

1. `research/operator-surface-redesign-decision.md` — how this decision satisfies
   `plan/operator-surface-redesign/`'s brainstorm entry gate.
2. `plan/console-happy-path-mvp/handoff.md` §§ 0h–0j — the walk evidence this plan
   inherits, including the three silent-refusal paths and the staged asset.
3. `plan/archive/work-item-lifecycle-redesign/research/locked-core-contract.md` — the
   invariants every slice obeys (zero Beads knowledge; commands only through the
   orchestrator surface; lane consumed never re-derived; attention as pure derivation;
   no console→driver dependency).

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
