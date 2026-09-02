---
topic: ci-run-supersession
author: claude-opus-4-8
created_at: 2026-09-02T08:16:19Z
---

## Proposal: Merge-gate run supersession: a PR push cancels that PR's stale run; canonical-branch post-merge runs never cancel each other

### Target specification files

- SPECIFICATION/non-functional-requirements.md

### Summary

Add a **Run supersession** clause to non-functional-requirements.md §Quality Gate (between the merge-gate list and the nightly paragraph) and two Gherkin scenarios to Contributor Scenario C. The merge-gate CI workflow MUST declare a concurrency group keyed on the workflow and the pull-request ref with cancel-in-progress enabled for pull-request events, so a new push to an open PR branch cancels only that branch's still-running run. Post-merge runs on the canonical branch are the evidence that the canonical branch is green and MUST NOT cancel or displace one another; their group MUST be keyed on the commit SHA, not the ref, because the hosting service keeps at most one pending run per group and replaces it, so a ref-keyed master group would silently drop the middle run of three fast merges even with cancellation off. The clause adds normative MUST sentences to non-functional-requirements.md, so the revision that lands it MUST update the pinned clause counts in crates/console-spec-check/src/tests.rs (extract_rules_matches_real_spec_ground_truth) in the same change, per .ai/spec-check-and-ci-discipline.md.

### Motivation

Work-item livespec-console-beads-fabro-s3kwxt (2026-09-02 CI incident review, plan retire-overseer-and-redesign-control-plane-around-console). .github/workflows/ci.yml fans out ~17 jobs per run and declared NO concurrency group, so every push to an open PR started a fresh full run without cancelling the run it superseded. MEASURED 2026-09-02: five PRs from one thread (#927 #928 #931 #932 #933), each re-pushed after a rebase, kept their superseded runs alive alongside the new ones; the self-hosted pool served the e2e jobs of all five concurrently (~85 pods at once) and every one flaked at the storage array's measured ~1,000 write/s, ~100 ms service ceiling (livespec plan ci-runner-pod-lifecycle-reliability research/004). Nothing reads a superseded PR run's verdict, so those runs were pure load. The host-side fixes (tmpfs datastore, dedicated NVMe) cannot remove this repo's own fan-out; only the workflow can. The asymmetry for the canonical branch is load-bearing: post-merge master runs are the orchestrator's post-merge janitor evidence that master is green, and the incident fix must not trade PR waste for lost master verdicts. The implementation (the concurrency block in ci.yml, keyed per-ref for pull_request and per-SHA for push) lands in the same PR as this proposal; the spec records the requirement so a future workflow rewrite cannot silently drop it.

### Proposed Changes

In `SPECIFICATION/non-functional-requirements.md` §Quality Gate, directly AFTER the **Merge gate** bullet list (the bullet ending "...or a permanently-ignored class such as `Debug` implementations.") and BEFORE the paragraph beginning "**Nightly -- scheduled run against the canonical branch.**", insert:

    **Run supersession.** The merge-gate CI workflow MUST declare a
    concurrency group so that a new push to a pull-request branch cancels
    that branch's still-running run. For pull-request events the group
    MUST be keyed on the workflow and the pull-request ref with
    cancel-in-progress enabled, so that only same-branch runs supersede
    one another and a push to one pull request never cancels another's
    run. A superseded pull-request run has no consumer -- nothing reads
    its verdict -- so keeping it alive is pure load on the shared runner
    pool (measured 2026-09-02: five re-pushed pull requests kept their
    stale runs alive beside the new ones, put ~85 pods on the pool at
    once, and every e2e job flaked). Post-merge runs on the canonical
    branch are the evidence that the canonical branch is green -- the
    orchestrator's post-merge janitor consumes them -- and MUST NOT cancel
    or displace one another: the group for a canonical-branch push MUST
    be keyed on the commit SHA rather than the ref. Keying it on the ref
    would be wrong even with cancellation disabled, because the hosting
    service holds at most one pending run per group and replaces it, so
    three fast merges would silently drop the middle commit's run. The
    workflow file MUST record, in its header comment, the incident that
    motivates the group, so a future rewrite of the workflow cannot drop
    it as an unexplained line.

In the `mermaid` flowchart that follows the nightly paragraph in the same section, add one node to the `Merge` subgraph after `Mutants`:

    Supersede["same-PR push cancels the stale run; master runs never cancelled"]

In `## Contributor Scenario C -- Quality gate enforces the inner and merge loops`, add the same node to the `Merge` subgraph of its flowchart after `Mut`:

    Sup["same-PR push cancels stale run; master runs kept"]

and append two scenarios to its `gherkin` block, after "Scenario: A nightly finding opens a chore instead of failing master":

      Scenario: A new push to a pull-request branch supersedes its in-flight run
        Given an open pull request whose merge-gate CI run is still in progress
        When a second commit is pushed to that pull request's branch
        Then the still-running run for that branch is cancelled
        And the new push's run proceeds
        And runs for other pull requests and for the canonical branch are
          unaffected

      Scenario: Consecutive canonical-branch pushes keep every post-merge run
        Given two or more pull requests merged to the canonical branch in
          quick succession
        When their post-merge CI runs are created
        Then no post-merge run is cancelled or displaced by a later one
        And each merged commit's run completes and reports its own verdict

Revision note (implementation coupling, not spec text): the inserted clause adds MUST sentences to `non-functional-requirements.md`; the revision that accepts it MUST update the pinned per-file and total clause counts in `crates/console-spec-check/src/tests.rs` (`extract_rules_matches_real_spec_ground_truth`) in the same change, or `check-test`, `check-nextest` and `check-coverage` fail on a markdown-only diff.
