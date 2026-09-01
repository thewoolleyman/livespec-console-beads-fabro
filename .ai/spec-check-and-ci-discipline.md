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

## Jobs queued with nothing starting, THIRD case: runner-pod LIFECYCLE stall

**Check the routing before any of this.** Every diagnosis in this family
applies only while `vars.CI_RUNNER_LABELS` is set on the repo
(`gh variable get CI_RUNNER_LABELS -R thewoolleyman/livespec-console-beads-fabro`).
When the variable is absent every gating job runs on GitHub-hosted
`ubuntu-latest` and a queued job is GitHub's queue, not the pool's. The
variable was deleted 2026-09-01 as the ratified availability fallback, with
its restore condition recorded on the plan epic — so do not assume the pool
is in the path; read the variable.

The wedge scan above answers "is a runner pod dead to GitHub?" and the
capacity signals answer "is the pool full?". On 2026-09-01 both said no —
0 wedge hits over 7-8 `Running` runner pods, 700m CPU requested of 72 cores,
8 of 64 churn slots allocated, Kueue 16 admitted / 0 pending — while PRs
#915-#917 sat with every job queued for 17-60 minutes. The pool was failing
to BRING PODS UP, which neither earlier case describes:

- runner pods were created, but each one's `local-path` work PVC took up to
  ~11 minutes to provision, so the scheduler's volume-bind deadline (600 s)
  expired (`FailedScheduling: ... PreBind plugin "VolumeBinding": binding
  volumes: context deadline exceeded`, 94 times in 20 minutes) and the pod
  went back to the queue, adding stale claims to the provisioner's backlog;
- underneath, containerd sandbox calls were timing out (`KillPodSandbox ...
  DeadlineExceeded`) because `fs.inotify.max_user_instances` was at the
  kernel default 128 and ~100 concurrent containers had exhausted it
  (`failed to create inotify fd: too many open files` in containerd's log);
- a second variant the same afternoon: the k3s SQLite datastore on the
  CI-churn disk stalled, single-replica Kueue lost its leader lease and
  exited by design, and its fail-closed pod webhook (`mpod.kb.io`) took
  every pod creation in the fleet down with it for the restart window.

**The two discriminating commands** (cluster host, as for the wedge scan):

```bash
kubectl get pvc -n arc-runners --no-headers | awk '$2=="Pending"' | wc -l
sudo grep -c 'failed to create inotify fd' /var/lib/rancher/k3s/agent/containerd/containerd.log
```

A Pending-PVC count that grows while runner pods cycle `Pending` →
`FailedScheduling` is the lifecycle stall whatever the second command says;
a non-zero second count names the 2026-09-01 root cause specifically (the
cap is now 8192, persisted on the host). For the Kueue variant,
`kubectl -n kueue-system get pods` shows a fresh restart and job logs carry
`failed calling webhook "mpod.kb.io"`.

**What THIS repo's job log shows** — a signature distinct from both earlier
cases: the job is claimed, then fails at `Initialize containers` with the
ARC hook's `Executing the custom container implementation failed. Please
contact your self hosted runner administrator.` (PR #916 `check-e2e-tmux`,
14:23-14:31Z). That line means the `-workflow` pod could not be created on
the host. The remedy is a re-run of the failed job on the same commit once
the host condition has cleared — not a change to the test. The other
console-visible effect of the same host load was a slow pod rather than a
failed one: `check-e2e-tmux`'s 20 s wall-clock waits and the event store's
SQLite open (`database is locked`) both timed out on the saturated disk,
which is `livespec-console-beads-fabro-l7unt3`'s subject.

Neither prior remedy applies. Deleting pods (the wedge fix) adds churn to
the provisioner's queue, and raising capacity (the saturation fix) adds
containers to an exhausted kernel budget — the 2026-08-30 raise from C = 16
to 64 is what pushed the container count into the cap. The pool is
fleet-owned; the measured chain, the host fixes (inotify 128 → 8192,
provisioner worker/QPS tuning, kubelet `max-pods` 110 → 200) and the open
legs live in livespec plan `ci-runner-pod-lifecycle-reliability` (epic
`livespec-ifwnqj`, research 001-004). Report the two counts; do not resize
anything from here.

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

`-bss4rq` is CLOSED (2026-08-26) on a product-defect referral, not on a
fix to the job. Its cause WAS captured — `SQLITE_BUSY`, extended code 5,
mid-session — and then run to ground: three code-level candidates were each
refuted, and local contention proved to be absorbed by a **20 ms** budget
where the CI failure exhausted **5000 ms**. The decisive variable was
environmental, not logical: `/tmp` on the dev host is **tmpfs**, so the e2e
store never touches a disk locally, while CI runs in a k3s pod on shared
disk. **Do not spend a session trying to reproduce this locally at the
shipped settings** — 150 trials and a deliberately hostile rig cannot.
The residue is `-ddfbcx.1`: the console *terminates the whole TUI session*
on a transient store contention instead of degrading. Its predecessor
`-4vsy7u` (PR #842) fixed the diagnosability half described next, and
`-ddfbcx.2` (PR #848) closed the second hole `-4vsy7u` left — see below.

Two more prohibitions carried from `-bss4rq`, both earned:

- **Do not raise `busy_timeout`.** It waits longer on an unbounded
  operation instead of bounding it.
- **Do not "fix" the pragma ORDER in `initialize_connection`.** It reads
  like free hygiene — arm `busy_timeout` before `journal_mode` — and it is
  a **no-op**: rusqlite calls `sqlite3_busy_timeout(db, 5000)` inside
  `Connection::open` (`inner_connection.rs:119`), before any user pragma
  runs. A whole session recommended landing it as "strictly-better
  hygiene ... costs nothing". It would have been a green commit that
  changed no behaviour.

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

## `check-coverage` cannot name the line it fails on — attribute it yourself

The gate fails with *"N unnameable missed line(s) exceeds the recorded
allowance"* and tells you to **take it to the maintainer and not raise the
allowance**. Taking that literally can be wrong in both directions, because
**the gate fails on the allowance check BEFORE its attribution step runs**,
so it names nothing — and the miss may be a genuinely uncovered line *your
change just introduced* rather than the llvm-cov instantiation-group merge
artifact the disposition fixture describes.

Measured 2026-08-26 on PR #848. The method, in order:

1. **Get the baseline from clean master first.** `just check-coverage` on
   `master` passes at 1 unnameable miss
   (`console-application/src/lib.rs` `reduce_tui_interaction`). That is what
   proves whether the extra one is yours.
2. **Make the gate attribute it.** Produce the artifacts by hand —
   `cargo llvm-cov --workspace --all-features --lib --json --output-path X`
   and `cargo llvm-cov report --show-missing-lines > Y` — then run
   `python3 dev-tooling/coverage-gate.py X Y Z` where `Z` is a **temporary,
   never-committed** copy of the disposition fixture with a raised
   allowance. It then prints the attribution block naming the exact
   `file:line:col` and the mangled symbol.
3. **Fix the code, not the fixture.** On #848 the attributed miss was
   `console-cli/src/lib.rs:2547:30` — a `map_err(|error| helper(&error))`
   **closure body** no test executes, where the bare-enum form it replaced
   generated no such region. The cure was to take the error **by value** so
   the mapping becomes a direct function reference
   (`.map_err(checkpoint_save_failed)`) with no closure at all — the shape
   `main.rs:562` already uses for `tui_runtime_io_failed`. Coverage then
   passed at the **original** allowance with missed functions back to 0.

Raising `max_unnameable_missed_lines` is the last resort the fixture says it
is. Reach for it only after step 2 shows the miss really is the merge
artifact and really is not attributable to your diff.

## A null result needs a POSITIVE CONTROL, or it means nothing

"Verified, nothing found" and "the instrument was never running" produce
byte-identical output. Before reporting a null as evidence, prove the check
can fire at all.

Three of this repo's own measurements were re-audited under this rule on
2026-08-26. Two survived because they happened to have controls; one did not
and was withdrawn:

- **Survived.** A write-latency instrument reported "no write took >= 50 ms".
  Dropping the threshold showed 106 logged lines — the instrument was
  demonstrably running.
- **Survived.** A stress rig reported 150/150 passes. Turning `busy_timeout`
  down to 0 produced 5 failures in 12 carrying the exact CI signature — the
  rig can detect the failure it was looking for.
- **WITHDRAWN.** A disk-backed run reported `max_write_ms=0` under induced
  fsync load. The harness read the peak out of a log with
  `m=$(... ) ; [ -z "$m" ] && m=0`, which turns *an empty or absent log*
  into *"the slowest write took 0 ms"*. A later positive control showed the
  induced load was real (400 `synchronous=FULL` commits on ext4: ~3.5 s
  quiet, ~5.3 s loaded) and that a commit on that filesystem costs ~8.7 ms —
  an order of magnitude above the reported figure. Those runs had measured
  nothing.

That defaulting line is the same anti-pattern `CLAUDE.md` already forbids for
CI polling — *never treat empty captured output as success; a `|| echo "[]"`
fallback swallows real errors*. It is just as wrong in a measurement rig, and
harder to notice there because the fabricated value looks like data.

**A shell that eats an argument produces the same fabricated null.** The
default shell here is **zsh**, where an UNQUOTED `--include=*.rs` is treated
as a glob; with no file of that name it raises `no matches found` and
**aborts the whole command**. A `grep -c ... | wc -l` wrapper then reports a
confident `0` for a command that never ran. Observed twice on 2026-08-26 —
once in this repo (`grep -rn ... --include=*.rs` → `(eval):1: no matches
found`) and once in a sibling repo, where it nearly invalidated a parking
decision because the empty result read as "this token appears nowhere".
**Quote the glob** (`--include='*.rs'`), and check the exit status rather
than only the count.

`set -o pipefail` **is not a defence, and it is what a careful reader reaches
for.** Measured here: with the unquoted flag,
`n=$(grep -rl ... --include=*.py 2>/dev/null | wc -l)` under `pipefail`
yields `n=0` with a non-zero status. The VALUE is still fabricated — the
abort happens at glob expansion, before the pipeline exists, so there is no
failing stage for pipefail to catch and `wc -l` never sees input. What pipefail
DOES do is **preserve the exit status** — and without it the status is lost
too, because the one you read is then `wc -l`'s, which is always 0. Measured
side by side on the same fixture:

| form | count | status |
|---|---|---|
| unquoted, **no** pipefail | `0` | **`0`** — value *and* signal lost |
| unquoted, **with** pipefail | `0` | `1` |
| quoted (positive control) | `2` | `0` |

So the rule has three parts and drops dead without any one of them: **set
pipefail, read the STATUS, never the count alone.** Advice to "check the exit
status" is actively misleading on its own — in the bare idiom the guard reads
clean while both the value and the signal are gone. And `2>/dev/null` is
harmful here regardless: it deletes the `no matches found` line, the only
human-visible signal, while leaving the plausible `0`.

**If you would rather not depend on a shell option**, drop the pipe so there
is nothing to swallow the status:

```sh
if out=$(grep -rl "$pat" "$dir" --include='*.rs' 2>/dev/null); then
  n=$(printf '%s\n' "$out" | grep -c .)
else
  echo "grep failed — do not trust a count" >&2
fi
```

Verified both ways: the quoted form yields `n=2` on a two-file fixture, and a
failing grep reports `grep FAILED (status 1)` instead of a silent `0`.

Precise mechanism, because it bounds the risk: the glob carries the
`--include=` prefix, so zsh is matching files literally named
`--include=<something>.rs`, not `*.rs`. Since no such file normally exists,
the outcome is a LOUD abort — stderr carries the signal unless you suppress
it, or unless someone has created a file with that exact name. (A file literally named `--include=zzz.py` does make zsh rewrite
the flag silently and grep then searches the wrong set; verified, but
pathological.) The practical rule is therefore narrow: never redirect stderr
away from a glob-bearing command, and never read its count without its
status.

**A count of 0 from `grep -c` is not a measurement** until the same pattern
has been shown returning non-zero somewhere it should. The cheapest control
needs no fixture: run it against a prior revision of the file you just
changed — `git show <pre-fix-sha>:<path> | grep -c "<pattern>"`. That is how
this repo's "zero bare cause-discarding sites remain" claim was upgraded from
an assertion to evidence (2 before the fix, 0 after, same pattern).

**Two rules follow.** Never let a harness substitute a plausible default for
a missing measurement — fail loudly instead. And when a null is load-bearing,
run the experiment that should make it non-null and show that it does.

**The limit of this rule, which matters more than the rule.** A positive
control proves the check *fires*. It does NOT prove the check measures the
thing you asked about — and there is a failure mode that defeats it. Reading
`${pipestatus[1]}` after a command-substitution assignment returns a real,
correctly-formed exit status **for a different pipeline entirely**. Nothing is
absent, nothing is malformed, no status is suppressed, and the instrument
fires perfectly every time. A control that only shows "it produced output"
passes it happily. (Observed in a sibling repo on 2026-08-26, where it nearly
reversed a correct conclusion.)

So the control has to be a **fixture with a known EXPECTED VALUE**, not merely
a non-empty result. "The grep returned something" is not a control; "the grep
returned 2 on a fixture built to contain exactly 2" is. That difference is
the only thing separating a working check from one that is confidently
answering a question you never asked.

Related: a verification step can fail this way too — see the `ls-remote`
false negative under *Commit / push mechanics*, where a missed grep and a
failed push were indistinguishable.

## Commit / push mechanics

- Commits and pushes are **refused at the primary checkout** (baseline
  worktree-discipline hook: refuses when `git-dir == git-common-dir`).
  Work from a git worktree.
- The pre-push hook runs the full `just check` gate. Push **through** it
  (no `--no-verify`) so a local pass guarantees CI passes; only bypass
  for a genuinely unrelated failure you have verified independently.
- The repo allows **rebase merges only** (no squash, no merge commit) —
  `gh pr merge --auto --rebase`.
- **A backgrounded `git push` can report exit 0 while the push FAILED.**
  The wrapper's status is not the push's. The pre-push `just check` takes
  ~250-320s here and routinely exceeds a 10-minute foreground tool timeout,
  so it gets backgrounded — and then a `FAILED targets: ...` line sits in
  the captured output under a zero exit code. **Grep the output for
  `FAILED targets:` AND confirm the ref actually moved** with
  `git ls-remote --heads origin refs/heads/<branch>`. Observed twice on
  2026-08-26, the second time by the same session that had just warned
  another session about it.
  **Pass the FULL refname, not a substring typed from memory.** That same
  session then got a FALSE NEGATIVE out of its own check: `ls-remote | grep
  jsonc-changelog` against a branch actually named
  `docs/livespec-jsonc-reviewer-changelog`, where that substring never
  occurs. The push had succeeded. A missed grep is indistinguishable from a
  failed push, so a sloppy pattern turns the verification step into a second
  way to be wrong — worse than not checking, because it reads as evidence.
  Use the exact `refs/heads/<branch>` and check the exit status, or compare
  the returned sha against `git rev-parse HEAD`.
- **A KILLED background push writes a log the failure-grep reads as CLEAN.**
  This is the sharper form of the bullet above, and the grep cannot catch
  it: a push killed during the pre-push hook produces a **282-byte** log
  containing only the lefthook banner, so `grep -c "FAILED targets:"`
  returns **0** — not because the gate passed, but because the log never got
  far enough to contain anything. Zero hits means "no failure line was
  written", which is a different claim from "no failure occurred", and only
  one of the two is what you wanted to test. **The ref check is the only
  discriminator**, and it is why that check is not optional even when the
  log looks clean. Measured 2026-08-27 on `fix/pr-node-off-codex`: two
  backgrounded pushes killed at an identical 282 bytes with no remote ref,
  then the IDENTICAL command run in the FOREGROUND succeeded in 283s — so
  the kills were harness-side, not a gate failure, and re-running rather
  than debugging the change was correct. Raising the background timeout did
  not help; running it in the foreground did.
  The general rule this instance earns: **an absent signal is not a negative
  result.** Before trusting a zero, confirm the producer ran far enough to
  have emitted a non-zero — the same positive-control discipline this file
  requires of null measurements, applied to a grep.
- **A fresh `git worktree add` has no `dev-tooling/`,** so
  `just check-baseline` fails there with `worktree_pack_absent` and blocks
  the push. Run **`just install-worktree-pack`** in every new worktree.
  Prefer it to `just bootstrap`: bootstrap reconciles the claude-plugins row
  and **advances the local plugin install**, which is what turns
  `check-fork-drift` red on clean master (see that gate's own note).
- **A `fix(`/`feat(` subject forces a test-only commit into Red mode, and a
  passing test then bounces.** The `red-green-replay` commit-msg hook
  classifies STAGED CONTENT, not just product Rust. When a changeset stages
  an integration test (`crates/**/tests/*.rs`) and NO product-impl Rust, the
  decision is SUBJECT-driven: `declares_red_intent` matches only the
  `feat:` / `fix:` / `feat(` / `fix(` prefixes, and such a subject is read as
  a TDD **Red** commit that REQUIRES the staged test to FAIL first. A test
  that PASSES is rejected with `red-green-replay-test-passed-at-red: Red mode
  requires the staged Rust test to fail first` — and the commit is aborted, so
  `git status` (not `git log`) is what tells you the change is still staged.
  This bites the common case the checker's Red/Green model does not cover: a
  **flake-hardening or robustness change to an existing, passing test**. Give
  it a NON-red-intent subject — `test(...)`, `chore(...)`, `docs(...)` — so the
  checker routes to the **SuiteGreen** path instead: it runs the (ignored-aware)
  suite for that test binary and writes the `TDD-Suite-Green-*` trailers
  itself. Product-impl Rust in the changeset takes a different path
  (Green / SuiteGreen keyed off HEAD's red-awaiting state), so this trap is
  specific to test-ONLY commits; it is a classification quirk, NOT a licence to
  relabel a genuine Red→Green feature away from `feat`/`fix`. Measured
  2026-08-30 on livespec-console-beads-fabro-7yq7dk: a `fix(test): …` commit
  hardening the PASSING `tmux_tui_e2e` walkthrough was rejected
  `red-green-replay-test-passed-at-red`; retitling verbatim to `test(e2e): …`
  passed via SuiteGreen and wrote the suite-green trailers. The general rule:
  content picks the ritual, but for a test-only changeset the SUBJECT PREFIX
  still picks Red-vs-SuiteGreen — so match the prefix to whether the test is
  meant to be failing (`fix`/`feat`) or already green (`test`/`chore`/`docs`).
