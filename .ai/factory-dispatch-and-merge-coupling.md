# Factory dispatch and merge are ONE act — there is no merge-window pin

Durable operational knowledge for any session that dispatches work in this
repo. Written 2026-08-21 after a plan session spent several rounds designing a
"implement now, merge later" route that cannot exist here.

## The rule

**Dispatching a work-item to the factory merges it.** If you are not prepared
for the change to land in `master`, do not dispatch it. Readiness (`ready`) does
NOT authorize dispatch when a merge needs to wait.

## Why — the mechanism

`.fabro/workflows/implement-work-item/prompts/pr.md` step 5 unconditionally arms
rebase auto-merge from inside the sandbox, and step 6 verifies it armed. Nothing
conditions that step: no policy dial, no per-item field, no label. So the
factory bot authors AND merges its own PR — visible in `gh pr list`, where
recent factory PRs show `author == mergedBy == app/thewoolleyman-factory-bot`.

## Three levers that look like they would hold it, and do not

- **The `do-not-merge` label.** It gates `.github/workflows/auto-enable-merge.yml`,
  whose `if:` allowlists PR author `thewoolleyman` and the release-please App on
  `release-please--` head branches. A factory-bot PR on a `feat/<work-item-id>`
  branch is never in that workflow's scope, so the label suppresses a path the
  PR was never on. It is not a no-op in general — it works fine on a
  human-authored PR — it just cannot reach a factory PR.
- **`merge_on_review_cap`.** The review-cap escape hatch (ship-anyway vs
  escalate-to-`blocked`), not a scheduling hold.
- **`factory_safety`.** Setting it non-null does route the item host-only and
  off the auto-merging path, but per the orchestrator's `contracts.md` it is the
  INTRINSIC, permanently-host-only runnability axis. Using it as a temporary
  scheduling device permanently mislabels the item to buy a one-time hold. Do
  not do this.

## And you cannot simply go off-factory instead

The `implement` operation's Step 0 is dispatch-first. In-session Red→Green on a
product changeset is permitted ONLY on one of four recorded exceptions:
factory-ineligible (host mutation, interactive credentials, mid-implementation
human judgment), factory unavailable, master-health-restoration, or **the
maintainer explicitly directing in-session execution for that item in that
session**. Deciding to go off-factory yourself because the factory will not hold
the merge is not one of them — that authorization is the maintainer's.

## What to actually do

If a merge must wait for a window, leave the item `ready` and **undispatched**,
and record on the item that readiness does not authorize dispatch. The work
waits with it. That is the conformant answer today, and its cost is real.

Lifting the restriction is tracked upstream as **`bd-ib-vlhp`** (P1) in the
`livespec-orchestrator-beads-fabro` tenant — a per-item hold marker carried from
the ledger into the sandbox, where a held item publishes the PR and skips the
auto-merge arming. It is filed there rather than here because merge-hold
plumbing is Fabro dispatch mechanics, which that repo owns; see CLAUDE.md
§"Repository scope".

## Related trap: the workflow-edit refusal does not cover `.fabro/`

The Dispatcher's declared-workflow-edit refusal keys on the literal prefix
`.github/workflows/` (`_dispatcher_host_only.py`). An item that edits
`.fabro/workflows/implement-work-item/` — the factory's own prompt fork — does
NOT trip it and IS dispatchable. That fork is gated instead by
`console-fork-drift-check`, which requires every divergence to be pinned and
explained. Two different gates; do not assume the first one covers the second.
