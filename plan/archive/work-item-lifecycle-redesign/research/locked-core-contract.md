# The locked core contract the console MUST honor

These were decided and locked in livespec core's plan thread
(`/data/projects/livespec/plan/work-item-state-machine/`, core epic
`livespec-35s3zo`). They are pinned here as the fixed inputs to E-1..E-4 —
**reference, not re-decision**.

## 1. Seven stored states + one derived overlay

Stored states:

```
backlog · pending-approval · ready · active · acceptance · blocked · done
```

The single **derived** overlay: stored `ready` + any open dependency →
rendered **`blocked:dependency`** (auto-clears when the blocker closes). The
console renders this; it does not store it.

## 2. `lane_of` is the single authority

Lives in `livespec_runtime/work_items/lifecycle.py`:

```python
def lane_of(*, item: WorkItem, index: dict[str, WorkItem],
            manifest: CrossRepoManifest) -> Lane

@dataclass(frozen=True, slots=True, kw_only=True)
class Lane:
    name: LaneName               # the 7 rendered lanes
    reason: BlockedReason | None # non-None iff name == "blocked"
```

- `LaneName = Literal["backlog","pending-approval","ready","active","acceptance","blocked","done"]`
- `BlockedReason = Literal["needs-human","infra-external","dependency"]`

## 3. `list-work-items --json` emits two FLAT computed keys per item

- **`lane`** — one of the 7 lane names.
- **`lane_reason`** — `dependency` / `needs-human` / `infra-external` / null.

All other new `WorkItem` fields (`rank`, `admission_policy`,
`acceptance_policy`, stored `blocked_reason`, `assignee`, the 7-state
`status`) auto-emit via `asdict`.

**Console rule (the drift hazard the redesign retires):** the console MUST
consume `lane` / `lane_reason` **directly** and **NEVER re-derive a lane**.

## 4. Console hard constraints (core decision 16)

- **Zero Beads knowledge** — no `bd`, no Dolt, no "beads" anywhere in the
  console; its ONLY external interface is the **orchestrator CLI**. (The
  current `Beads*` cluster names are themselves Beads references that this
  retires.)
- **Zero primary lifecycle state** — every lane / attention item /
  projection is **rebuildable from the ledger**.
- **Snooze/ack are killed** — the attention inbox is a **pure derivation**
  of the state machine: an item needs attention **iff** its state requires a
  human. "Not now" is `defer` (a ledger state) or a re-rank (a ledger
  field), never a console-local dismissal.
- **No console→driver dependency** — harness abstraction lives in the
  **driver layer**, reached *transitively through the orchestrator*. The
  console takes no direct console→driver dependency.

## 5. Acceptance is POST-MERGE / in-production

Ship-on-green, then AI/human confirm the shipped artifact against tests +
telemetry; `reject` = revert / fix-forward. `just check` stays the
**pre-merge correctness floor**. The console's acceptance UI **commands the
orchestrator**; it does **not** gate merges.
