# ARCHIVED 2026-07-24 — charter delivered, everything closed

Do not resume this thread. Do not extend `handoff.md`. It is retained for the
reasoning: the semantic-conflict union resolution, the coverage-gate
forward-tolerance analysis (impl-first green / rider-first breaks / gap-id
line-wrap trap), and the factory publish-failure forensics are not derivable
from the code.

## Why it was archived

The charter — single-consumer (exactly-once) semantics for the console command
queue — is fully delivered and every gate is closed:

- **PR #399** (merge `2665cad`): the `-ipwtll` implementation — atomic
  `pending -> executing` claim, conditional finalize, conservative
  stale-claim recovery — with the contract amendment applied verbatim,
  Scenario 24, and the coverage links. Plus the `v035` out-of-band cut
  (`4ef9ebc`).
- **PR #316** (merge `940647b`): the `-ble` repeatable-action audit,
  union-resolved over master's independent `4241fc3`.
- **v036** (revise cut): the pending rider proposal formally dispositioned
  accept-as-already-applied.
- Ledger: `-ipwtll`, `-ble`, and epic `-irdwyb` CLOSED with delivery
  comments carrying merge SHAs and the delegated-acceptance record
  (maintainer: "get them done. you drive it.", 2026-07-24).
- Factory run `01KY6HC0CJ`: abandoned (`[A]`) and archived after its work
  product was rescued from `fabro dump` stage artifacts; its publish failure
  is the **`bd-ib-pums`** (P2) infra defect in the orchestrator tenant —
  silent synthetic-snapshot-base fallback after a hook-refused pre-clone
  push. That item is the live successor for the only unfinished business
  this thread touched, and it belongs to the orchestrator owners, not here.

## Onward pointers

- `bd-ib-pums` (livespec-orchestrator-beads-fabro tenant) — the factory
  staging defect this thread diagnosed. The ONLY live thread of work that
  originated here.
- `-8aw` (the four non-valve initial commands) stays `backlog`, explicitly
  NOT this thread's: regroom it when the operator-surface spec amendment has
  landed, per the handoff's parking rationale — that reasoning still holds
  and now lives only in the archived handoff.
- Two parked doctor observations (Scenario 23's two clause-less assertions;
  the long TUI-Contract paragraphs) were surfaced to the maintainer
  2026-07-23 and deliberately not acted on — candidates for a future
  spec-hygiene pass.
