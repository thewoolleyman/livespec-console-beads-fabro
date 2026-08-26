# Spec-check & CI discipline (console)

Durable, learned agent knowledge for `livespec-console-beads-fabro`,
loaded on demand from `AGENTS.md`. Captured after a session where a
"spec-only" PR was wrongly declared a pre-existing/environmental CI
failure without reading the log.

## A "spec-only" change CAN break Rust CI in this repo

`crates/console-spec-check/src/tests.rs` has
`extract_rules_matches_real_spec_ground_truth`. It reads the **real**
`SPECIFICATION/` files and asserts the normative-clause (`MUST`/`SHOULD`)
count per file as pinned ground truth, for:
`spec.md`, `contracts.md`, `constraints.md`,
`non-functional-requirements.md` (plus a `total`).

Any spec revision that adds/removes normative clauses in those files
changes the counts and **fails this test** — and therefore `check-test`,
`check-nextest`, and `check-coverage` — even though only `SPECIFICATION/`
markdown changed. This is intentional: it forces a conscious ground-truth
update whenever the spec's clause surface moves.

**Rule: when a spec revision changes clause counts, update the pinned
counts in the SAME change.** Measure the real counts by running the
actual `extract_rules` over the revised spec (e.g. temporarily
`eprintln!` `extract_rules(file, &text).len()` per file, run
`cargo test -p console-spec-check <test> -- --nocapture`), then set the
`cases` array and the `total` assertion (and the comment). Never assume
"spec-only ⇒ no Rust impact" here. Example: the v013 full-autonomous-mode
revision moved spec/contracts/constraints `3/20/7` → `14/32/18`,
total `82` → `116`.

## Read the actual CI log before attributing a cause

Never label a CI failure "pre-existing" or "environmental" from
inference — read the failing job's log first.

- `gh run view <run> --log-failed` sometimes returns **empty** on this
  project. That is a `gh` quirk, NOT missing observability — do not give
  up when it happens.
- Reliable retrieval:
  `gh api /repos/{owner}/{repo}/actions/jobs/{job_id}/logs`
  (get `job_id` from `gh run view <run> --json jobs`), or
  `gh run view <run> --log` for the full log.
- `gh run view <run> --json jobs --jq '.jobs[] | select(.name|test("<job>")) | .steps[] | "[\(.conclusion)] \(.name)"'`
  shows WHICH step failed (setup vs. the actual test step).

## Jobs queued with nothing starting: WEDGED RUNNER vs. real saturation

CI here runs on the self-hosted ARC k3s scale set
(`livespec-console-beads-k3s`, via `vars.CI_RUNNER_LABELS`). Two very
different failures present **identically** — jobs queued, nothing
starting — and they have **opposite** fixes:

- **Saturation.** Runner pods really are `SchedulingGated` and the
  node's churn-slot capacity is consumed. Waiting is correct; the queue
  drains. Adding capacity helps.
- **Wedged runner.** A runner pod is `Running` and `ready=true` to
  Kubernetes while permanently dead to GitHub: its log loops
  `Registration <uuid> was not found` → reload credentials → sleep 55s,
  forever, never exiting. ARC counts the zombie as a live runner, so the
  listener concludes it already has enough runners and re-patches
  replicas every ~50s without ever creating a pod that could take the
  job. Self-perpetuating, and **invisible to every capacity signal**
  (pod phase `Running`, readiness true, zero gated pods, node headroom
  to spare). Raising capacity CANNOT clear it.

**The GitHub REST runner list is NOT a signal here — do not reach for it.**
Both confirmations below need cluster access. A session that has the forge API
but no `kubectl` will naturally try
`gh api repos/<owner>/<repo>/actions/runners`, and on this pool that reads like
a total outage while nothing is wrong. Measured 2026-08-22: it returned every
runner `offline`, `busy: false`, with `os: unknown`, `version: null` and empty
`labels`, and `total_count` churning between reads (14 → 13) — while that same
commit's checks were starting and completing normally. These are EPHEMERAL ARC
registrations; rows linger after their pods exit, so lingering `offline` rows
are this pool's normal steady state, not a health reading. **Measure the jobs,
not the runners:** the check-runs for the commit in question tell you whether
work is actually starting. Reporting a fleet outage off that endpoint is a false
alarm this repo has already almost raised once.

**Rule: before attributing a stall to capacity, prove it.** Zero gated
pods plus spare capacity plus a job sitting queued with an empty
`runner_name` means wedge, not saturation. A fleet detector
(`livespec-s43svm.30`) now scans runner-pod logs on a 5-minute timer and
auto-clears wedges, guarded so it cannot delete a pod that has claimed a
job; because it reads the pod's own log rather than job-start patterns,
it catches a wedge regardless of how the stall presents. To confirm by
hand, on the cluster host (`poweredge-xubuntu`,
`KUBECONFIG=/etc/rancher/k3s/k3s.yaml`) scan each `Running`
non-`-workflow` runner pod:

```bash
kubectl logs <pod> -n arc-runners --tail=40 | grep "was not found"
```

Any hit is wedged with certainty — the runner emits that line only after
the broker told it its registration does not exist, and it has no code
path that re-registers. Deleting that pod is safe by construction — it is
ephemeral and can never do work — and ARC creates a healthy replacement
within seconds. The runner pool is owned by the fleet track, so report
what the scan shows rather than resizing anything from this repo.

Captured 2026-08-19 after the delivery-path-speed-and-caching plan
misread its own stall. The observed shape was a **trickling serial
drain**, not a hard freeze: on CI run 32199534021 (PR #682, created
00:00:34Z) jobs started at 00:11:10Z, 00:17:17Z, 00:29:40Z, then a
42-minute gap to 01:11:42Z. A point-in-time "N queued, zero
in_progress" reading was taken between jobs and reported to the fleet
track as a total freeze — **sample the same run twice before calling it
stopped.**

**That gap's cause was never established, and this note deliberately does
not claim one.** It was first diagnosed fleet-wide as capacity starvation
and the pool was resized 8→16 on that basis; that diagnosis was later
retracted as asserted-from-an-earlier-incident rather than verified. A
wedged pod was independently confirmed on this repo's own scale set
during the window, but console's `nominalQuota` was 1 at the time, so
quota-floor-plus-borrowing contention fits the same evidence, and the
capacity raise landed near the end of the gap. Several causes fit; none
was proven. Recording it as unresolved is the point — the expensive part
of this incident was successive confident attributions, not the stall.
A complementary "abnormal inter-job gap with spare capacity" alarm would
catch this class of unknown-unknown; it belongs to the CI observability
work (`livespec-s43svm.20`), which carries the 42-minute gap as a seed
measurement.

## A local test run is only evidence if it's the SAME commit CI tested

`cargo test` passing locally proves nothing unless the working tree is
the exact commit CI ran. The primary checkout is usually on `master`;
spec/feature work lives on `spec/*` branches in worktrees, so a bare
`cargo test` in the primary checkout tests master, not the branch. Check
out (or worktree) the CI'd SHA before saying "passes locally."

## Flaky vs. caused: only a re-run of the FAILED JOB on the SAME COMMIT tells them apart

A red check on a PR has two very different causes — the PR broke it, or the
job is flaky — and the evidence that feels most convincing does not
distinguish them.

Measured 2026-08-26 on PR #840. `check-e2e-tmux` failed
(`tmux_tui_e2e_lifecycle_walkthrough_two_repos`, "timed out after 20s
waiting for a settled frame containing `attention: 0`", last capture
`tui error: TuiRuntimeFailed` / `TUI_EXIT=1`). Two independent measurements
both pointed at the PR's own change:

- **A green control PR.** #841 — `master` plus a prose-only change — passed
  the same job in the same CI, and `master`'s last three runs were green.
- **A green local run.** The same suite on #840's exact branch passed 11/11
  locally.

Read together those look conclusive, and they compose into one specific
wrong story: *the PR broke it, and the local pass is an environment
difference.* Both are true statements that say nothing about causation.

**Re-running the FAILED JOB on the IDENTICAL commit passed.** Same bytes,
both outcomes — the only measurement that separates the two hypotheses,
because it holds the code fixed and varies only the run. Use the run-rerun
subcommand with its failed-only flag.

The near-miss is the point: the verdict the control invited was "this change
broke the e2e test", which would have sent a session hunting a defect that
does not exist. **Do not attribute a red check to a change until you have
re-run that job on the same commit.** A green control and a green local run
are both compatible with flakiness, and neither is evidence against it.

### `check-e2e-tmux` is known-flaky, and is MERGE-BLOCKING on purpose

Treat a `check-e2e-tmux` red as unattributed until re-run. Two standing
prohibitions, both from `ci.yml`'s own comment above the job:

- **Do not revert it to advisory.** That comment records why: PRs #360,
  #361, #362 and #365 each merged with `check-e2e-tmux = FAILURE` while
  `ci-green` reported SUCCESS, and master's CI sat red for that whole span,
  "because an advisory check is one nobody is required to read." It assigns
  the remedy explicitly: "If that dependency makes the job flaky, fix the
  job."
- **Do not raise `RENDER_TIMEOUT`**
  (`crates/console-cli/tests/tmux_tui_e2e.rs`). In the observed failure the
  TUI *exited*; it did not render slowly. A longer timeout only makes the
  same failure take longer to report.

The open item is `livespec-console-beads-fabro-bss4rq`, deliberately blocked
until a recurrence carries a cause. Its predecessor `-4vsy7u` (PR #842)
fixed the diagnosability half described next.

## A gate can be blind in the direction you did not check

Three shapes, all measured in this repo on 2026-08-26. Each gate ran,
reported, and the report was about the wrong question.

**A "capture" fixture can be hand-authored, and CI cannot tell.**
`tests/fixtures/orchestrator-config-manifest.json` is documented in
`crates/console-completeness-check/src/lib.rs` as a committed capture of the
orchestrator's `config-manifest`, refreshed by
`just refresh-config-manifest`. A seventh key was written into it by hand
with the `captured_key_set_digest` re-stamped to match. No orchestrator
build published that key — a live capture returned six. The capture is
hermetic by design (CI runs offline), so nothing in CI could distinguish a
genuine capture from a fabricated one, and a real refresh would have
*reverted* the merged work. If a pin must ever run ahead of its producer,
use the declared-hand-maintained form this repo already ships in
`tests/fixtures/drive-human-action-surface.json`, whose own leading comment
says it is hand-reviewed and which demands a reason per divergence — "an
allowlist says 'ignore this'; a reason says 'here is why'." Never silently
inside a file whose doc comment claims it was captured.

**A one-directional completeness check is blind to extras.**
`CompletenessReport` reported only *declared keys missing from* console
surfaces. An extra console row for an *undeclared* key was not a finding, so
the fabricated row above stayed invisible to the very gate meant to police
that lockstep — and the console offered an operator a dial the orchestrator
answers with `invalid-config-key`. The converse leg landed in PR #840. When
you write a "must appear in all N places" gate, ask what it says about the
N+1th thing that appears in only one place.

**An error mapped to a bare enum destroys the diagnosis.**
`crates/console-cli/src/main.rs` wrapped the whole TUI session launch in
`.map_err(|_error| ConsoleRuntimeError::TuiRuntimeFailed)`. Every distinct
runtime failure — store error, port failure, panic — collapsed into one
opaque name, so a CI log's *complete* available diagnosis was the string
`TuiRuntimeFailed`. A failure that cannot be diagnosed cannot be fixed.
Fixed in PR #842: the variant carries its cause and `Display` renders it, so
the binary's stderr line now puts the real cause in the tmux frame.

## A green PR is a statement about its TIP, not about the merge result

A PR's checks ran against its own branch tip. They say nothing about the
tree that results from merging it, and **the older the branch, the wider
that gap**. Nothing in the merge machinery closes it: a stale branch can
merge with no textual conflict at all and still break `master`, because
the breakage is semantic rather than textual.

Measured 2026-08-21 on PR #317: 14/14 green, branch tip from 2026-07-19,
`mergeable` reported `UNKNOWN`. Rebased onto current `master` it went
**red** on `check-arch`. Its new "never silently skip a symlink" rule
flagged `./CLAUDE.md`, a symlink to `AGENTS.md` committed in `af0b60a` on
2026-08-16 — a month AFTER the branch last ran. CI had honestly tested a
tree that did not contain the file that breaks it.

**Rule: before merging any PR whose branch is not current, refresh it and
let CI re-run.** `gh pr update-branch <n> --rebase` then wait. Treat an
old green as unevaluated, not as a pass. A `mergeable` of `UNKNOWN` is a
prompt to check rather than a blocker — but note it would NOT have warned
here, since the textual merge was clean.

The corollary for required checks: the check SET also moves. #317's green
predated `check-shell-quality` and `check-plan-no-tombstone` entirely, so
even a full green row was missing two gates the current set requires.

## When a binary reports something its source cannot say, the BINARY is stale

`cargo` considered a `console-arch-check` binary fresh while it had been
built from a different worktree's branch, so a run on `master` printed a
diagnostic whose message string **does not exist anywhere in master's
source**. That nearly produced a report of `master` being red, and of a
regression being "pre-existing" when it was the opposite.

The tell is cheap and decisive: `grep` the source for the exact message
before believing the run. If the string is not there, force a genuine
rebuild (`touch` the source, or use a clean worktree with its own target
dir) and re-measure. A `target/` directory shared across worktrees makes
this failure ordinary rather than exotic.

## The rate-limit guard reads your PR BODY as shell, line by line

`github_rate_limit_guard.py` (from the `livespec-driver-claude` plugin) blocks a
`gh pr`/`gh run` command that also contains a shell loop or `sleep`. It correctly
requires COMMAND POSITION, so ordinary prose containing "for" mid-sentence is
fine. But command position includes `^` under `re.MULTILINE`, and the pattern is
matched against the WHOLE command string — heredoc body included.

So a `gh pr create` whose body has a line BEGINNING with `For`, `While`, `Until`
or `Sleep` is denied, with a message about looped GitHub reads that describes
nothing you did. Measured 2026-08-21 (mid-line is allowed, line-start is not):

```text
real shell loop              -> DENIED   matched 'for'
prose, line-start 'For'      -> DENIED   matched 'For'
prose, mid-line 'for'        -> allowed
prose, line-start 'While'    -> DENIED   matched 'While'
```

`--body-file` does NOT help: the guard sees the `cat > file <<EOF` heredoc in the
same command. Two things that do:

- Reword so no body line starts with those words ("But an item that..." instead
  of "For an item that..."). Cheapest fix.
- Write the body file in a SEPARATE Bash call from the `gh pr create` call, so
  neither command contains both the prose and the `gh` invocation.

The guard is doing its job and the rule it enforces is right; this is a
false-positive shape in a sibling plugin, not something to work around by
disabling anything. Dispatcher and driver mechanics live outside this repo per
`AGENTS.md` "Repository scope", so this is recorded as working knowledge here
rather than filed as a defect here.

## Commit / push mechanics

- Commits and pushes are **refused at the primary checkout** (baseline
  worktree-discipline hook: refuses when `git-dir == git-common-dir`).
  Work from a git worktree.
- The pre-push hook runs the full `just check` gate. Push **through** it
  (no `--no-verify`) so a local pass guarantees CI passes; only bypass
  for a genuinely unrelated failure you have verified independently.
- The repo allows **rebase merges only** (no squash, no merge commit) —
  `gh pr merge --auto --rebase`.
