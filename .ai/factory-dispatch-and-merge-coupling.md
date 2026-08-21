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

### The workflow-edit refusal offers two overrides. Usually you must not take them.

When the refusal fires it names its own escape hatches — `set-workflow-scope-override:<id>:citation-only`, or an inline declaration that the item ships no
files under `.github/workflows/`. Both exist for items whose workflow path is
INCIDENTAL: the item merely cites the path. Neither is a way to make a real
workflow edit dispatchable.

For an item that genuinely SHIPS a change under `.github/workflows/`, either
override is a FALSE DECLARATION, and it would be recorded as the item's own
statement about itself. The refusal already names the correct route in the same
breath — "host-route it through an attended host session instead; the item remains
open for that route" — so the item is not blocked, only routed. Take that.

This is the one place in the dispatch path where the error message hands you a
way to silence it, so it is worth naming the discipline: an override that changes
what an item CLAIMS, rather than what it DOES, is not a fix.

Confirmed 2026-08-21 on `-mcj.3`, which edited BOTH `.github/workflows/ci.yml`
and `.fabro/workflows/implement-work-item/workflow.toml`: only the former tripped
the refusal, exactly as the `.fabro/` note above predicts. It was host-routed and
merged normally. The refusal fires at stage `host-only-refused` BEFORE any run
exists, so it strands no claim — the item stays `ready`.

## Reading a dispatch outcome: three traps, measured 2026-08-21

A dispatch that does not return green has several distinct shapes, and they call
for opposite responses. Do not collapse them.

**`fabro ps` showing no run is NOT evidence that no run happened.** It lists
active and blocked runs; a **completed** run is invisible to it. During the
2026-08-21 credential outage a query against the hp factory showed two blocked
runs in other tenants and no console run — while the console item's work had
already merged to master. Releasing its "phantom" claim would have reopened
finished work.

**Separate a phantom claim from a finished run by the JOURNAL, never the run
listing.** `tmp/fabro-dispatch-journal.jsonl` carries the per-stage record:

- *Phantom* — refused before sandbox launch: `fabro_run_id` is null, no branch,
  no PR. The claim is stranded and SHOULD be released.
- *Finished* — a real PR, real CI rows, and a merge (`pull-primary` showing a
  fast-forward, then `janitor-*` stages). The item is DONE and should be closed,
  not released.

**Never run a dispatch under a short tool timeout.** Killing it mid-flight
strands a claim on work that actually SUCCEEDED: a 20-minute timeout SIGTERM'd a
dispatch during its post-merge janitor, after the merge landed but before closure
was recorded, producing an `active`/`fabro` item whose change was already on
master. Run dispatches unbounded in the background instead.

### The stale-plugin refusal is the one shape that strands nothing

`ERROR: dispatcher plugin build is stale; executing build <X> predates latest
release <Y>` exits 3 with `stdout_json: null` **before** ledger-admit runs, so no
claim is taken and none needs releasing. It is also purely a SESSION artifact:
the project pin is usually already current (`claude plugin update ... --scope
project` reports "already at the latest version"), because a running session
keeps the build it resolved at kickoff. The marketplace moved twice in one
session on 2026-08-21. Re-invoke the dispatcher from the currently pinned build
path, or restart the session — see `.ai/livespec-plugin-currency.md`.

### Credential refusal: let time discriminate before touching a secret

`CLAUDE_CODE_OAUTH_TOKEN ... HTTP 429, rate_limit_error, condition "exhausted"`
names BOTH a rolling rate limit and an org spend cap, and does not say which. Do
not assert one. A rolling limit heals itself and a billing cap does not, so a
retry after a real interval separates them at zero cost. A retry two minutes
later is not evidence; on 2026-08-21 a retry ~78 minutes later ran clean through
to a merge, which settled it as the rolling face with no secret rotated.

## The sandbox never sees your work-item COMMENTS

Measured 2026-08-21, after annotating eleven slices with the constraints their
implementers would need and then discovering none of it would arrive.

`.fabro/workflows/implement-work-item/workflow.fabro` says it plainly:

> The per-item brief (work-item id, title, description, publish branch) arrives
> via the run goal (`--goal-file`), rendered into the prompts as the `goal`
> template variable.

**Four fields. Comments are not among them.** A dispatched implementer sees the
title and the description and nothing else from the ledger record — not the
comment where you recorded the measurement, not the one warning it off the
approach that looks right and is not, not the scope verification that says the
item is twice the size its title implies.

This cuts against the natural habit. Comments are the right place for an
ongoing record: they are append-only, timestamped, attributed, and they do not
destroy what was there before. `bd update --description` replaces. So the
instinct is to leave the description as filed and let the comment stream carry
what you learned — which is exactly the choice that makes the learning invisible
to the only reader who has to act on it.

**The rule:** anything a dispatched implementer MUST know belongs in the
DESCRIPTION, even if that means rewriting a description you already wrote.
Comments remain right for the audit trail, for cross-session coordination, and
for anything a HUMAN or a host-only session will read — those readers see the
whole record. They are wrong as the sole carrier of a constraint the factory
needs.

A cheap way to catch this: before dispatching, ask what the item would look like
with every comment deleted. If the remaining title and description would lead a
competent implementer to the wrong approach, the item is not ready to dispatch,
however complete the ledger record looks to you.

### Filter the journal by FIELD, not by substring

Reading the journal is right; grepping it by work-item id is not. Fleet-wide
rows list many ids inside a single field — a `reflection` row on 2026-08-21
carried a `stage-retry` finding naming seventy-plus items in one string. A
`grep <id> tmp/fabro-dispatch-journal.jsonl` matches those rows, so a dispatch
that is still mid-flight appears to have reached `reflection`, which is the LAST
stage of a completed run. The conclusion is exactly backwards and it looks
authoritative.

Those rows are identifiable: `work_item_id` is `null` on them, because they
belong to the loop rather than to any one item. So select on the field:

```python
rows = [d for d in map(json.loads, open("tmp/fabro-dispatch-journal.jsonl"))
        if d.get("work_item_id") == "<id>"]
```

The same caution applies to `outcome`: an item's real terminal row carries
`stage: "outcome"` with an `outcome` object holding `status`, `pr_number`,
`merge_sha` and `fabro_run_id`. Absence of that row means the run has not
finished, whatever else the file appears to say about the id.

## A gate written as prose is not a gate — dispatch also CLOSES the item

Measured 2026-08-21, one level up from the comments trap above and with the
same shape.

Dispatching merges the item; it also **closes** it. The close is mechanical —
`Fabro dispatch landed PR #797 (merged, post-merge janitor green)` — and it
does not read acceptance criteria to decide whether to close. So an item whose
acceptance says it must stay open closes anyway.

The worked example. `-mvu22t.4` widened the factory prompt so dispatched
branches emit Rust TDD trailers. Its acceptance 4 required post-merge
end-to-end evidence that could not exist yet, because a run executes the prompt
as it existed AT DISPATCH and therefore cannot exercise its own edit. So the
item said, in capitals:

    Leave this item OPEN until the evidence is recorded, even after its own
    PR merges.

It closed on merge. The harm was immediate and was exactly what the clause
existed to prevent: `-mvu22t.2` — the slice that would have reddened `just
check` on every factory branch touching Rust — was blocked on `-mvu22t.4`
alone. The moment `.4` closed, `.2` became mechanically dispatchable with its
real gate still open, and a drain picking by rank would have taken it.

**The rule:** a gate that must survive a merge belongs in the DEPENDENCY GRAPH,
not in a sentence. Express it as a blocking edge, or as a separate item that
holds the edge. Acceptance prose can DESCRIBE a gate; it can never ENFORCE one.

The remedy in that instance was a verification-only item (`-mvu22t.6`, no code
change) carrying the measurement, made to block `-mvu22t.2` directly. Give such
an item an explicit disposition for a NEGATIVE result — record it, close it as
answered, file the follow-up — or it becomes a permanent block nobody may close.

### The diagnostic that makes it findable

Named by the foreman seat while scanning for other instances, and it is the
generalisable half:

> The tell is **an instruction addressed to a HUMAN sitting in a field only
> MACHINES act on.**

That is greppable, and a ledger-wide scan for enforce-shaped prose (`do not
close`, `leave open`, `must remain open`, `even after its merge`, `not closed
until`) takes one query.

**But most hits are not the defect.** That scan returned three across 254
closed-inclusive items, and only one was real. The prose has to be trying to
survive an event the machinery will act on anyway:

- `-4s1h` said "do not close it against 4nrwmp" — a scope instruction to a
  human triager, honoured, never at risk from the Dispatcher.
- `-mvu22t.5` said "DO NOT CLOSE THIS BY LANDING -mvu22t.4" — a warning against
  confusing two items. `.5` closing on its OWN merge is correct, and the gate
  that matters there (`.5` blocks `.3`) was already an edge.

So read every hit for the hazard, not the string. The same discipline as the
comments trap: ask which reader acts on the field, and whether the instruction
is addressed to that reader.
