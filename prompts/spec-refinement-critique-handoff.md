# Spec-refinement handoff: critique → revise cycles (console)

Goal: refine and harden the `livespec-console-beads-fabro` `SPECIFICATION/`
through repeated `/livespec:critique` + `/livespec:revise` cycles BEFORE any
further implementation. No code changes in this track — spec only.

## How to run a cycle

1. Run `/livespec:critique` against the main spec target (`SPECIFICATION/`),
   feeding the steering text below as the critique focus. Critique files a
   `<author>-critique.md` proposed-change under
   `SPECIFICATION/proposed_changes/` containing findings (ambiguities,
   contradictions, untestable language, missing/over-constrained rules).
2. Review the findings, then run `/livespec:revise` to accept / modify /
   reject each, cutting the next `history/vNNN/`.
3. Repeat until a critique pass surfaces no material findings.

Expected, non-blocking: until upstream `livespec-lly4` lands, revise's
post-step doctor reports ONE `doctor-template-files-present` failure for
`diagrams/example.{plantuml,svg}`. That exit-3 is informational; the snapshot
still cuts. Confirm it is the ONLY fail.

## Steering text for `/livespec:critique`

Focus this critique on the following areas. For each, surface ambiguities,
contradictions, untestable language, and over- or under-constraint as
findings; do NOT propose implementation.

1. **Adapter architecture.** (`spec.md` §Architecture / §Bounded Contexts;
   `non-functional-requirements.md` §Constraints → Domain-Driven Design +
   Architecture Tests.) The spec mandates separate `console-adapter-*` crates
   that "MUST NOT depend on each other," yet the same boundary could be
   honored by a single adapters module behind per-source ports. Surface
   whether the per-crate split is an intended hard requirement or an
   over-constraint; the rule must be unambiguous and match the intended design.

2. **Architecture-test enforcement strength.**
   (`non-functional-requirements.md` §Constraints → Architecture Tests.)
   "cargo metadata MAY enforce crate graph rules" and "Source-level checks MAY
   use Rust syntax parsing" are permissive. Surface whether a crate-graph
   dependency check is required vs optional, and whether each enforced rule is
   stated falsifiably (a text scan satisfies the letter today).

3. **Quality-gate thresholds.** (`non-functional-requirements.md` §Contracts →
   Quality Gate.) "coverage with a declared threshold" declares no number;
   "fuzz tests … where practical" and "mutation testing where practical" are
   unfalsifiable. Surface that the spec should state the concrete coverage
   threshold and the criteria under which fuzz and mutation graduate from
   smoke checks to hard gates.

4. **Initial-adapter fidelity and honest incompleteness.** (`spec.md` §Product
   Shape / §Architecture; `contracts.md` §Adapter Contract / §Initial Adapters
   / §Command Handling.) The spec does not resolve whether first-milestone
   adapters may be minimal/simulated vs must perform real source I/O, nor what
   "honest incompleteness" requires — e.g., a stub MUST signal "not observed"
   rather than fabricate success (today a simulated drain port reports success
   without acting). Surface this as a missing rule; it is the spec-side
   counterpart to the simulated-boundary implementation gap tracked separately.

5. **Post-split coherence.** Now that `constraints.md` was reduced and
   `non-functional-requirements.md` added, surface any behavior left in prose
   without a `scenarios.md` scenario, any rule duplicated or contradictory
   across `constraints.md` ↔ `non-functional-requirements.md`, and any
   `spec.md` architecture text inconsistent with the NFR layering rules.

## Out of scope for this track (impl work-items, NOT spec)

- Wiring real source/factory I/O (replacing `ScriptedSource` +
  `SimulatedFactoryDrainPort`) — behavior already specified; an impl
  realization gap, not a spec change.
- Implementing the richer arch-check, the fuzz corpus, the mutation gate, and
  coverage tooling — impl realizations of the (refined) NFRs above.
- Repo doc hygiene (retiring `research/tui-first-milestone-bootstrap-plan.md`,
  the root `README.md` pointer, the `AGENTS.md` "seed state" language) —
  one-off corrections of non-normative docs.
