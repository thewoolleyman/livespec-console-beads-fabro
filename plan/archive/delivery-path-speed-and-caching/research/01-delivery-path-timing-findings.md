# Delivery-path timing findings (2026-08-17)

Evidence-based follow-up to `00-origin-and-scope.md`, gathered via `gh run
list`/`gh run view --json jobs`, `gh pr checks`, direct read of
`.github/workflows/ci.yml` and `justfile`, and live Honeycomb queries against
the `livespec` environment's `github-ci` and `fabro` datasets (the telemetry
export wired in eafe0fb is live and already carries real data for both CI
runs and factory-dispatch stages).

## 1. Local dev loop

`just check` (justfile:196) runs 18 targets **serially**, in-process, on the
dev machine. Two concrete redundancies found by reading the recipes:

- `check-test` (`cargo test --workspace --all-features`) and `check-nextest`
  (`cargo nextest run --workspace --all-features`) both compile and run the
  **entire test suite**, back to back, under two different test harnesses.
  `check-coverage` then runs a **third** full instrumented build+test pass
  via `cargo llvm-cov`. Each is a from-scratch `cargo` invocation against
  (mostly) the same workspace — profile differences (test/coverage) limit
  target-dir reuse between them, per the CI comment at ci.yml:159.
- `check-deps`/`check-nextest`/`check-coverage` each gate on
  `ensure-rust-quality-tools`, which falls back to `cargo install --locked`
  (source build) for any missing tool — CI comments this at ~740s/810s per
  cold job (ci.yml:196-201) before switching to `taiki-e/install-action`
  prebuilt binaries. Not currently a problem on the CI image (tools are
  baked/prebuilt), but worth confirming local dev machines have the same
  prebuilt-binary path rather than falling back to a source build.

No local run was timed this session (bounded-time constraint); the redundant
full-suite passes (test -> nextest -> coverage) are the highest-confidence
target since they are structural, not measured-once.

## 2. Factory dispatch (Fabro) — real per-stage telemetry

The `fabro` Honeycomb dataset already carries per-node `Stage completed`
spans with `wall_time_ms`. Query over the last 24h (`node_id` breakdown,
n=112 stage completions across ~20 runs):

| node_id      | count | P50 wall time | P95 wall time | max      |
|--------------|-------|---------------|---------------|----------|
| review_fix   | 6     | 515s (8.6m)   | 1055s (17.6m) | 1055s    |
| implement    | 14    | 447s (7.5m)   | 777s (13.0m)  | 777s     |
| fix          | 3     | 408s (6.8m)   | 493s (8.2m)   | 493s     |
| pr           | 10    | 129s (2.2m)   | 170s (2.8m)   | 170s     |
| review       | 21    | 116s (1.9m)   | 358s (6.0m)   | 395s     |
| disposition  | 7     | 89s (1.5m)    | 154s (2.6m)   | 154s     |
| janitor      | 21    | 70s (1.2m)    | 100s (1.7m)   | 101s     |
| start/exit   | 20/10 | ~0s           | ~0s           | ~0s      |

Sandbox provisioning is **not** the bottleneck the origin note hypothesized:
`Sandbox ready` P50 is 2.6s (max 7.5s across 20 runs), and `Setup completed`
(post-ready workflow setup) P50 is 22.8s. Combined provisioning+setup is
~25s P50 — negligible next to the multi-minute stage times above. The
`livespec-fabro-sandbox` image is pinned by tag
(`python-rust-agent-v1.19.0` per `.fabro/workflows/implement-work-item/workflow.toml:197`)
and pulled once per run by the sandbox runtime; no evidence of cold-pull
cost dominating.

**The real cost is `implement`/`review_fix`/`fix` node wall time**, which per
`inference_time_ms`/`tool_time_ms` attributes on the same spans is
predominantly LLM inference and tool-call time, not infra/provisioning —
i.e. not directly cacheable. PR #666 itself: opened 13:52:24Z, merged
19:07:53Z (~5h15m), but its CI checks only ran 18:55-19:07 (12 min) —
confirming the multi-hour gap was dispatch/stage queueing and agent work,
not CI.

## 3. CI (GitHub Actions)

**Not on a purely "hosted" runner as the origin note assumed.** `ci.yml`
routes `runs-on` through `vars.CI_RUNNER_LABELS`, currently pointed at a
self-hosted ARC k3s scale set (`livespec-console-beads-k3s`, per the
2026-08-17 cutover note at ci.yml:37-43), with `ubuntu-latest` only as the
fail-closed fallback. This matters directly for the plan's caching premise.

`actions/cache` was deliberately **removed** from `ci.yml` (comment at
ci.yml:149-160) with measured numbers on this exact image/host: cold clippy
37s vs warm-cached 53s on `ubuntu-latest`; cold test 35s vs warm-cached 48s.
The self-hosted host's local disk/CPU already beats a cache round-trip, and
persistent `actions/cache` would fight the runner's 10GB/repo cap while
buying nothing. **This closes the "hosted runner enables caching" premise
for the self-hosted lane** — caching there is a net loss, already measured
and rejected once.

Real run timing (`gh run view --json jobs`, two recent master-push CI runs,
18-job matrix + 3 standalone jobs):

- Run 32059097292: created 19:12:53Z, first job started 19:25:32Z (**~13min
  queue delay**), last job (`export-telemetry`) completed 19:32:29Z. Total
  wall clock 19.6min, of which only ~7min was actual job execution.
- Run 32058757831: created 19:09:14Z, first job started 19:15:18Z (**~6min
  queue delay**), completed 19:21:29Z. Total 12.25min, ~6min queue + ~6min
  execution.
- Honeycomb `github-ci.ci.run` over the last 2h: n=8, AVG duration 410s
  (6.8min), MAX 1170s (19.5min) — consistent with the above.

Individual job execution times are already fast (most complete in 30s-3min
once started); **the dominant, currently-unaddressed CI cost is queue delay
before a job gets a runner**, not per-job compute or missing dependency
caches. This points at self-hosted runner-pool concurrency (scale-set size)
vs. the 18-wide job matrix, not caching, as the lever.

### Measurement closure: runner-pool admission resize

The runner-pool escalation leg is discharged by fleet work item
`livespec-s43svm.24`: all eight fleet ClusterQueues, including
`livespec-console-beads-fabro-cq`, now have `nominalQuota=1` churn slot
for a summed guaranteed floor of eight real node slots, with cohort borrowing
allowing burst-to-eight behavior when other repos are idle. This in-repo
measurement leg does not reopen runner-pool sizing.

Fresh GitHub Actions job timestamps after the resize-live handoff verified
first-job-start under the two-minute target on at least three console CI runs.
Measured via the Actions API run `created_at` and the minimum job `started_at`:

| Run | Event | Created | First job started | Queue delay | Result |
|-----|-------|---------|-------------------|-------------|--------|
| 32195331926 | push | 2026-08-18T23:00:54Z | 2026-08-18T23:01:05Z | 11s | success |
| 32194334812 | pull_request | 2026-08-18T22:47:40Z | 2026-08-18T22:48:11Z | 31s | success |
| 32185531047 | push | 2026-08-18T21:01:43Z | 2026-08-18T21:01:45Z | 2s | success |
| 32185135716 | pull_request | 2026-08-18T20:57:25Z | 2026-08-18T20:57:39Z | 14s | success |
| 32184768287 | push | 2026-08-18T20:53:22Z | 2026-08-18T20:53:41Z | 19s | success |

The widest observed delay in the sampled completed runs was 31s, so the
acceptance target is met with margin. Full-matrix wall-clock comparisons are
still fleet-load-sensitive because the guaranteed floor is one slot with
borrowing, but the first-job-start queue symptom that motivated this item is
no longer reproducing. No HTTP 429 `actions/checkout` download failure was
observed in these completed successful samples; runner-side action caching
remains a separate follow-up only if that failure mode reappears.

## Requirement carriers vs. explicit deferrals (draft, for the scoping event)

**Requirement carriers (in-scope for this plan's ledger children):**

- Measure/right-size the self-hosted ARC k3s scale-set concurrency against
  the 18-job matrix width to cut the 6-13min queue delay (biggest, most
  reproducible number in this research).
- Eliminate the redundant `check-test`/`check-nextest`/`check-coverage`
  triple full-suite run in the local `just check` path (and confirm whether
  CI's matrix pays the same redundancy — it runs them as separate parallel
  jobs, so redundancy there is parallelism-hidden but still burns compute).
- Confirm local dev machines use the prebuilt-binary path for quality tools
  (avoid the measured 740-810s cold source-build fallback).

**Explicit deferrals (out of scope / belongs elsewhere):**

- `livespec-fabro-sandbox` image-level changes (build/pull mechanics) —
  measured NOT to be the bottleneck (provisioning ~25s P50), and ownership
  sits outside this repo regardless.
- Re-adding `actions/cache` to the self-hosted CI lane — already measured
  and rejected with hard numbers in ci.yml; would need new evidence to
  reopen, not assumed as this plan's work.
- Reducing `implement`/`review_fix` node wall time — this is LLM
  inference/tool-call time, not an infra/caching problem; any optimization
  here belongs to workflow/prompt design, not this plan's caching scope.
- The two unrelated sandbox infra failures noted in the origin note (-9ts
  npm pre-cache gap, -2ckgiy GitHub App token rate limit) — distinct defects
  already tracked under their own ledger items, not rolled into this plan.
