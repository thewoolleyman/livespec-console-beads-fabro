# Why "drive an item fully through the lifecycle via the TUI" never shipped

Researched 2026-07-20 (session `exploratory-test-tui`), from the plan trees,
archives, spec, and git history. Conclusion up front: **the requirement was
never dropped — it was fractured across three re-scoping events**, and the
operator-facing remainder is parked, unstarted, behind a maintainer-gated
design thread. No thread today owns *delivering* the end-to-end path; this
thread exists to own exactly that.

## Where the requirement was stated

1. **Spec (live, normative).** Scenario 5 "TUI-first operator workflow"
   (`SPECIFICATION/scenarios.md` §Scenario 5): "As an operator using a
   terminal / I want arrow-driven views and detail panes / **So that I can
   drive common orchestration actions before the GUI exists**." The current
   (v032) executable steps only cover *inspection* — the "drive actions"
   ambition lives in the Feature narrative, not in any executable step.
2. **Cockpit deliverable B7** (`plan/cockpit-ux-docs-release/handoff.md`
   §B7): a key-by-key walkthrough doc of "running a work-item through the
   ENTIRE livespec lifecycle via the TUI", acceptance = an agent walks it on
   a dummy item, real TUI in tmux, end-to-end, two repos. **Status: NOT
   STARTED.**
3. **Cockpit Stage-2 acceptance** (same handoff, §Stage-2): "Drive multiple
   REAL fleet items end-to-end SOLELY through the live TUI, parking in
   `acceptance`, with the maintainer's final accept via the TUI."
   **Maintainer-gated, last in that program.**
4. **Autonomous-mode design** (`plan/archive/console-autonomous-mode/design.md`):
   "From the console TUI, a human operator can (a) drive individual
   work-items through the human valves manually …".

## The three fracture events

1. **2026-07-01 — foundation shipped, operator surface deferred.** The
   `work-item-lifecycle-redesign` epic delivered the 7-lane state machine,
   `lane_of` authority, attention-as-derivation, and the valves *as data* —
   and explicitly deferred the operator-command surface to a "future console
   operator-cockpit milestone" (**decision 47**,
   `plan/archive/work-item-lifecycle-redesign/handoff.md`).
2. **2026-07-19 — the autonomous operator surface was superseded, not
   shipped.** Orchestrator step O2 retired full-autonomous arming (the
   dispatcher drains by default), and the console spec re-baselined around
   dispatcher *policy settings* (today's Scenarios 9–11). "C3's operator
   surface was superseded rather than shipped as designed"
   (`plan/archive/console-autonomous-mode/handoff.md`).
3. **2026-07-19 — impl-dispatch split.** The thread that had accreted the
   remaining operator-surface work was retired as "coupled, non-cohesive,
   and off track" (`plan/archive/impl-dispatch/SUPERSEDED-BY.md`) and split:
   design inputs (`-zweohm`, `-l4p3ce`, `-vc7lmq`, `-ipi`) →
   `plan/operator-surface-redesign/` (epic `-6msemd`); command spine
   (`-ipwtll`, `-ble`, `-8aw` parked) → `plan/command-queue-semantics/`
   (epic `-irdwyb`); B7/backfill → `plan/cockpit-ux-docs-release/`.

## Net disposition (as of 2026-07-20)

- The **state-valid-verbs / groom-exposure / LLM-handoff design** sits in
  `plan/operator-surface-redesign/` — a design-only thread with a **hard
  maintainer-brainstorm entry gate** ("no impl items until ratification"),
  additionally sequenced behind a cross-repo orchestrator valid-verb
  vocabulary ratification. It has not started.
- The **end-to-end walkthrough** is B7 — not started.
- The **end-to-end acceptance** is Stage-2 — maintainer-gated, last.
- The **autonomous** variant is retired for good (orchestrator drains by
  default; that part is NOT coming back and is out of scope here).

So the reason it "never got done" is structural, not accidental: every
predecessor either shipped the *substrate* (lifecycle machine, valves,
event spine, cockpit UX polish) or *designed* the driving surface and was
then superseded — and after the split, **no live thread owns integrating
the pieces into a walkable happy path**. The design thread is gated on a
brainstorm that has not happened; the walkthrough doc is gated on the
surface existing; the surface is gated on the design. This thread breaks
that deadlock by scoping an MVP happy path and sequencing the minimum set
of pieces to walk it.
