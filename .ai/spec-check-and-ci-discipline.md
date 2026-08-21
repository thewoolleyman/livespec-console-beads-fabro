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
