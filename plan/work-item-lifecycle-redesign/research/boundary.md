# The boundary — who owns what (load-bearing; do not cross)

The redesign spans two repos. Keeping the seam clean is the point: the
drift hazard the whole effort retires is the console re-deriving things the
contract already decides.

## livespec CORE owns the CONTRACT (the producer side)

These are decided in core and are **not ours to re-decide**:

- The lifecycle state-machine **vocabulary** (the 7 stored states, the 2
  human-delegable valves, the derived overlay).
- The `WorkItem` **schema** in `livespec_runtime`.
- **`lane_of`** + the `list-work-items --json` **emission shape** (the
  computed `lane` / `lane_reason` keys).
- **`rank`** (the first-class fractional order).
- The **acceptance model** (post-merge / in-production).

## THIS console repo owns the HOW (the consumer side)

- The **Rust redesign** of the work-item source + ingestion.
- The **TUI lane/view model**.
- The **ingestion / event model** (granularity, event shapes).
- The **attention model** (inbox as a pure derivation).
- **Snooze/ack removal**.
- The **rebuild-from-ledger conformance test**.
- Any genuinely **console-LOCAL spec invariants** (e.g. its own Control-Plane
  constraints).

## The spec-hygiene rule

- **Do NOT copy core design decisions into this repo's `SPECIFICATION/`.**
  *Reference* them instead — "per livespec core's work-item lifecycle state
  machine." Only genuinely console-local contracts belong in this repo's
  spec.
- **Do NOT modify livespec core** or any other repo. Do not touch, push, or
  force-push any branch/worktree this thread did not create.
- For any commits in THIS repo, follow the repo's `AGENTS.md` mutation
  protocol: **worktree → PR → merge → cleanup**. Never commit on the primary
  checkout.

## Cross-repo linkage

- Parent fleet epic: **`livespec-35s3zo`** (livespec core tenant).
- Console anchor epic: **`livespec-console-beads-fabro-vqh36l`** (this
  tenant).
- The link is a **prose cross-reference**, not a typed `depends_on`: this
  repo's `WorkItem.depends_on` is a flat list of **same-tenant** ids with no
  cross-repo dependency kind. A cross-tenant id placed in `depends_on` would
  dangle and pollute the blocked:dependency derivation, so it is deliberately
  kept out. The brief's conditional ("IF the schema supports a cross-repo
  `depends_on` kind") therefore resolves to: **description cross-reference is
  sufficient.**
