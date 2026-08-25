# 001 — Seed: the homelab Phase 2 charge and the code-and-ledger rebaseline

Created 2026-08-25 by this repository's plan lifecycle, on request of the
homelab `steady-state-loop-hardening` worker session (maintainer ruling,
handoff 12 on `homelab/hl-nkuzaz`; homelab-initiates model). This is the
console leg of the fleet's steady-state loop-hardening programme — the
third of four reserved upstream plan slugs (`homelab-loop-hardening-{orchestrator,core,console,runtime}`).

**Read-first chain** (all in `mi-homelab/homelab`, branch `main`):

1. `plan/steady-state-loop-hardening/research/009-console-review-triage.md`
   — the triage of this repository's two adversarial reviews; carries the
   binding rulings R1–R3 and the per-finding dispositions this seed
   restates.
2. The two console reviews landed by homelab PR #1031:
   `plan/steady-state-loop-hardening/research/reviews/livespec-console-beads-fabro-review-{fable,sol}.md`
   (finding numbers below refer to these).
3. `research/007` (orchestrator-review triage) and `research/008`
   (core-review triage + the shared-runtime routing rule) for the
   programme-wide rulings the console inherits.

**Rebaseline, not Phase V.** This thread seeds from a CODE-AND-LEDGER
REBASELINE, not homelab `research/005` Phase V, which is superseded per
console-sol finding 3: the generic console ingestion path (CN1) already
shipped, so the open console work is narrower and different than the
phase that plan drafted.

## The seven charge legs

1. **Additive-kind fixture tests (wire-consumer leg).** Fixture tests
   through the EXISTING Rust port for additive attention kinds — no
   local enum. Per ruling R2 the console is a WIRE consumer: no
   `.vendor.jsonc`, no pin to bump; the locked dev-group
   `livespec-runtime` (v0.21.3) is tooling synchronization, never
   product-consumption evidence. The console leg for a runtime release
   is: spec-prose (composition-class enumerations), kind-specific UI
   affordances, and a real producer-payload compatibility test.
   (console-fable 7, console-sol 5.)
2. **Identity leg** (console-fable 3): resolve a real principal instead
   of the hardcoded `"operator"` constant, and forward `requested_by` on
   every action-port invocation. Pairs with the orchestrator's CLI
   identity input, which the orchestrator filing designs; coordinate on
   the resolution-order contract.
3. **ONE bundled propose-change** (console-fable 4): composition-class
   enumerations; a ratified home for the probe-outcome view plus the
   journal-riding ingestion path; Settings enumeration growth; and any
   R1 boundary decision. R1 (binding): foreman-origin items enter this
   console's inbox ONLY via a fresh propose-change that explicitly
   revisits the v040 Scope Boundary — a named decision, never inherited
   from an upstream release. Foreman/overseer wait states reach the
   operator as LEDGER STATE on the owning plan epics, which the ratified
   composition classes already cover.
4. **Disposition of `livespec-console-beads-fabro-ipi`** — the
   TUI-migration item ("Console TUI needs-attention render path: migrate
   from lane-derived to the attention_item.* stream"; verified live in
   this tenant at seeding, status `backlog`). Note the overlap with leg
   5: the disposition decides whether it is absorbed by, or sequenced
   under, the conformance slice.
5. **The conformance slice** (console-sol 4): remove the
   attention→`Ready` normalization (the forbidden shadow-state pattern);
   finish the `attention_item.*` migration; join detail by ID without
   synthesizing lifecycle state; add the negative architecture test (an
   `impl:` attention row creates ONLY `attention_item.*` events). No new
   fact class may mutate a work-item lane projection.
6. **Settings-lockstep leg** (console-fable 2): settings row + help +
   docs + completeness-check extension + the TUI-enumeration
   propose-change, triggered PER KEY by the orchestrator filing's
   per-key API-configurable declarations.
7. **Envelope tolerance** (console-fable 5): the two-item test — one
   well-formed item of an unknown kind plus one malformed item —
   asserting the consumer-tolerance posture the envelope ratification
   decides. Per-item FIELD stability matters: the current all-or-nothing
   parse blinds the entire inbox, the detection-staleness backstop
   included.

## Sequencing and constraints (binding)

- The orchestrator files first (`homelab-loop-hardening-orchestrator`).
- The runtime baseline (`homelab-loop-hardening-runtime`, seeded in
  parallel) ratifies the attention surface FIRST; this repository's
  fixture/wire tests grade against its RELEASED baseline.
- Generic-not-local: capabilities land upstream in their owning repo;
  homelab proves consumption with negative controls.
- Merging is not deploying: nothing here counts as rolled out until
  exercised live per the fleet's done-means-exercised rule.

## Local verification at seeding (2026-08-25)

- Tenant sweep over the full cached dump (266 records, all statuses):
  zero hits for `homelab-loop-hardening-console` and
  `homelab-loop-hardening` — nothing pre-exists, consistent with the
  four-reviewer upstream-emptiness sweep and with console-fable 8 /
  console-sol 7.
- `livespec-console-beads-fabro-ipi` exists as described (leg 4's
  target).
- The reviews' code claims (the `"operator"` constant, argv without
  identity, the all-or-nothing envelope parse, the attention→`Ready`
  normalization) are accepted from the reviews at seeding and MUST be
  re-verified at execution time, per research/009's "re-verified when
  the console charge files".

## Next

Record the scoping event: cut the seven legs into requirement carriers
and explicit deferrals — the upstream-gated legs (1, 2's orchestrator
half, 6's trigger, 7's ratification input) deferred to named
reconsideration points keyed to the orchestrator and runtime filings —
then admit the first children (the conformance slice and the
`livespec-console-beads-fabro-ipi` disposition are executable without
upstream input).
