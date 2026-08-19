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

## Commit / push mechanics

- Commits and pushes are **refused at the primary checkout** (baseline
  worktree-discipline hook: refuses when `git-dir == git-common-dir`).
  Work from a git worktree.
- The pre-push hook runs the full `just check` gate. Push **through** it
  (no `--no-verify`) so a local pass guarantees CI passes; only bypass
  for a genuinely unrelated failure you have verified independently.
- The repo allows **rebase merges only** (no squash, no merge commit) —
  `gh pr merge --auto --rebase`.
