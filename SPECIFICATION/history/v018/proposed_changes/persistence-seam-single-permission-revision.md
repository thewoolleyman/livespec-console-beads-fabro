---
proposal: persistence-seam-single-permission.md
decision: accept
revised_at: 2026-07-11T04:22:42Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Driver ratified ACCEPT after the independent read-only Fable review returned
NO-BLOCKERS. Resolves the persistence-model seam now that the orchestrator's O1
arming contract has FROZEN: the console STOPS persisting its own per-repo
autonomous-mode preference and the SINGLE persistent permission becomes the
orchestrator plane's key `livespec-orchestrator-beads-fabro.dispatcher.autonomous_mode`
in the repo's `.livespec.jsonc`. Three sites are re-cast in contracts.md
§Autonomous Mode and spec.md §Full Autonomous Mode: (1) the console-owned
persisted block is DROPPED and the current per-repo mode is DERIVED by reading
the orchestrator's key; (2) the `config.autonomous_mode_set` handler is
RE-TARGETED to effect enable/disable by writing the orchestrator's key through
its published command surface, rather than persisting a console-owned block;
and (3) spec.md's "mode preference is persisted per-repo" sentence is re-cast to
the single-permission model. A scenarios.md drift-sweep re-scopes Scenario 9's
`.livespec.jsonc`-persist mermaid node, flow edge, and Gherkin step so no
scenario still asserts the console persists its own preference. This alters no
`## ` heading.

## Co-edit (console clause-coverage lockstep)

Dropping the one contracts.md normative persistence clause changes the
ground-truth clause totals, so the console behavioral-coverage gate was
re-synced in this same revise commit (mirroring the C1 v017 lockstep in revise
`8aa5d54`):

- `crates/console-spec-check/src/tests.rs` — contracts.md 37 -> 36 and total
  123 -> 122 (tuple 15/36/19/52); the golden comment refreshed to describe the
  v018 delta.
- `tests/heading-coverage.json` — the Scenario 9 entry drops the removed
  clause's gap-id `gap-dchrh3if`, rebinds the three reworded clauses'
  gap-ids from the ratified text (`gap-d24kqbpi` -> `gap-dkeuago5`,
  `gap-cu3t3prv` -> `gap-i2x4e2r2`, `gap-lswx3ste` -> `gap-limelxvu`), and
  rewords its stale TODO reason from "persisted repo setting" to the derived
  "armed orchestrator permission key" model.

## Resulting Changes

- spec.md
- contracts.md
- scenarios.md
