# Campaign lifecycle #5 — `erb2ud` — PARKED, DOES NOT COUNT

Walked 2026-08-21T00:16:29Z onward on master `7a26279`. Menus-only through the
dispatch; the run then parked at Fabro's **in-loop human gate**, never reached
the acceptance valve, and therefore could not be accepted at it.

**Campaign count stays 4 of 6.** A lifecycle that cannot reach acceptance is not
a dogfooded lifecycle. Recorded here because the walk produced three findings — one of
them a deterministic, expensive sandbox fault — and two retractions against
itself, which are worth more than the count would have been.

## Counting note

Under the standing orders as clarified 2026-08-21, the campaign counts
LIFECYCLES DOGFOODED THROUGH THE TUI, not walks performed by a session, and plan
attribution follows where the record lands. Plan 02's R10 capstone on `ekb5vq`
drove a full lifecycle through the human valve and is therefore campaign
lifecycle **#4**; this walk is **#5** and is uncounted. The unified sequence is
#1 `5deyqc`, #2 `4nrwmp`, #3 `5zjk5b`, #4 `ekb5vq`, #5 `erb2ud` (parked).

## What the TUI did correctly

1. **Named non-menu step** (`01-`): ranked consult returned `erb2ud` top of five,
   all `rank a0`.
2. **Target confirmed by ID, not position** (`04-`), per `-x6lj`. `erb2ud`
   rendered SIXTH with `rank ~`.
3. **Armed from the menu** (`08-`): leg B read back `Set acceptance work-item` /
   `Target: livespec-console-beads-fabro-erb2ud` / `Policy/mode: ai-then-human`.
4. **Dispatched from the menu** at 00:25:13Z (`11-`). Journal 00:25:20Z:
   `loop-pick {budget: 1, picked: ["...erb2ud"]}` — exactly the armed item.
   **Arm-first has now held on every walk that used it.**

The console's part of this lifecycle was clean. What follows is not a console
defect.

## Finding 1 — the stale projection blanks `acceptance_policy`

Full write-up in `06-stale-projection-blanks-policy.md`. In short: the console
rendered `erb2ud` as `rank ~` with `acceptance_policy — (not emitted)` while the
**ledger** carried `rank: a0` for that same item — `-v8un` confirmed against
ground truth rather than inferred from catching a re-sort. `rank` and
`acceptance_policy` degrade together, which falsifies half of a technique this
plan recommended: reading `ai-then-human` confirms armed, but reading
`— (not emitted)` confirms nothing, because a stale row prints a default where
it has no reading. Walk #3's own "unarmed control two rows away" was weaker than
claimed for the same reason, and that is corrected against this plan, not
someone else's.

**And the item's own command did not refresh it** (`10-`): the re-arm completed
at 00:23:42.356Z, and 54 seconds later the pane still could not report the
policy it had just set. `-v8un`'s "until an unrelated command forces a refresh"
overstates what an operator can provoke.

Plan 02 owns `-v8un` and sharpened this into its acceptance: **unknown must be
distinguishable from unset**, because a projection printing an assumed default is
doing exactly what this repo's contracts forbid its ports to do.

**The staleness is in the projection only.** This walk armed against a stale row
and the drain still picked correctly — the ledger and the ranker were right
throughout.

## Finding 2 — the sandbox's orchestrator plugin cache is missing `scripts/bin`

`13-parked-outcome.txt`. The run ended `blocked / human_input_required` at
02:04:19Z after 1h39m with `pr_number: null`. The cause, verbatim from the run:

    tui error: orchestrator plugin root
    /root/.claude/plugins/cache/.../livespec-orchestrator-beads-fabro/271b1f3fa14c
    is missing a scripts/bin directory (neither .claude-plugin/scripts/bin nor scripts/bin)

    FAILED targets: check-nextest check-coverage check-e2e-tmux
    error: Recipe `check-pre-push` failed on line 504 with exit code 1

The chain: sandbox plugin cache incomplete → every tmux e2e test fails →
`just check` fails → the pre-push hook blocks the publish → the `pr` stage fails
deterministically → `escalate` → the human gate. The `github_token_refresh_limited`
notices in the same run are INFO-level noise about ACP env at process launch and
are **not** implicated; an earlier guess by this session that the park shared
`et3.7`'s token-permission cause was wrong.

**The plugin is fine; the sandbox provisioning is not.** The same pin on this
host carries `scripts/bin` with 18 entries. It is the container's copy that lacks
it, so the sandbox materialises the plugin directory — it knows the pin — without
populating `scripts/`.

This is repo-agnostic: any dispatched repo whose `just check` shells the
orchestrator's `scripts/bin` fails identically, at the pre-push hook, after
paying the full run cost — and it will present as a different failure each time,
depending which check fires first.

**It is NOT, however, what was stalling the factory fleet-wide.** This session
suspected the other run blocked in the hp pool at the time
(`livespec-dev-tooling-thw26i`) shared the cause. The console foreman checked it
against this signature and it carries none of it — no `missing a scripts/bin`,
no `orchestrator plugin root`, no `check-e2e-tmux`, no `failed to push some
refs`. That run is blocked on a maintainer policy question about a gate's scope.
Two blocked runs, two unrelated causes. The coincidence this session doubted was
in fact a coincidence, and the defect is routed on its own considerable merits
rather than as a fleet-wide stall.

It also **resurrects plan 02's original Defect A** — a
missing `.claude-plugin/scripts/bin/mint_app_token.py` in the sandbox, which 02
downgraded as "not the blocker" when a token error surfaced. Same family, and
here it is the direct cause of a lost lifecycle. Routed to the console foreman
for the sandbox-image owner; not filed by this plan.

## Finding 3 — and the finding this walk had to retract about itself

This session first reported the run as **unreachable**: `fabro inspect` and
`fabro events` both returned "No run found matching", with no engine process, so
neither `attach` nor `resume` — the two commands the dispatch outcome names —
could reach it. That was published as a dead end.

**It is false.** A local `fabro ps`/`inspect` queries a DIFFERENT POOL, and this
repo dispatches to the remote `hp` factory. With `--server https://hp-xubuntu…`
the run lists immediately as `blocked / human_input_required`. The run never
vanished; the query shape produced an absence and the absence was read as
information — the exact failure this arc spent the night cataloguing, committed
by this session after having written the lesson down.

What survives of it is smaller and real: the dispatch outcome's recovery text
omits `--server`, so an operator following it verbatim gets "No run found
matching" and concludes the run is gone. Contrary to a report made here earlier,
`fabro attach` DOES accept a piped answer and is not tty-only.

The gate itself was an interview with three options — Retry / Re-implement /
Abandon. It was answered **A (Abandon)**: `erb2ud`'s work had already been
recovered by hand, so a successful factory PR would have duplicated it, and the
sandbox fault above would have failed a retry regardless. The run is now
`failed / workflow_error` and its pool slot is released — the hp pool was holding
only blocked runs at the time, so the slot mattered to other repositories.

## The false alarm this walk raised, and retracted

A ref WAS pushed (`feat/livespec-console-beads-fabro-erb2ud` @ `346210bd`), so per
the standing discipline this is push-succeeds/PR-fails and not a console defect.
Reading that branch before landing it, this session reported that it carried a
foreign commit — plan 02's spec proposal — and escalated a **dispatch-isolation
defect** to the foreman.

**That was wrong and is fully retracted.** `6638c46` is the rebase-merge commit
of plan 02's PR #739 and was `origin/master`'s HEAD; the branch descends from it
and carries five commits and four files, none under `SPECIFICATION/`.

The cause was **not** misreading a rebase-merge signature. It was a **single-ref
fetch** — `git fetch origin feat/...erb2ud` — leaving `origin/master` at
`7a26279`, from before the 01:34 merge, and then computing a range against it.
Everything downstream was reasoning on a stale ref.

The durable lesson, recorded because the obvious one is wrong: a cheaper
falsifier that shares the failure mode is not a falsifier. The proposed check
`git diff --name-only origin/master <ref>` would ALSO have "confirmed" the
phantom against a stale `origin/master`, with more confidence. **Fetch all refs
before computing any range against `origin/master`.**

## What was recovered, and how

`erb2ud`'s work is real and was not stranded. The two genuine commits — `847ce2e`
"render non-outcome dispatcher journals as progress" and `346210b` "reserve
dispatcher backlog bounce for real bounces" — were cherry-picked onto current
master and landed through an ordinary PR, leaving the three `fabro(<run-id>):
<stage>` checkpoint commits off master. That is hygiene, not spec containment:
master carries 837 commits and exactly one stray `fabro(...)` stage commit, and
every recent factory item landed as one clean commit. Same recovery shape plan 02
used for `et3.7`.

`erb2ud` was **not re-dispatched** — the ref is already pushed and a second
dispatch would hit the non-fast-forward trap that stranded `-2ckgiy` for three
days.

## File index

| file | what it captures |
| --- | --- |
| `00-timestamp.txt` | walk start, UTC |
| `01-ranking-consult.json` | the named non-menu step |
| `02-resting.txt` | resting frame at 160 columns |
| `03-ready-drilled.txt` / `04-target-selected.txt` | ready lane; target confirmed BY ID |
| `05-arm-survived-from-walk3.txt` | the stale pane: `rank ~`, policy `— (not emitted)` |
| `06-stale-projection-blanks-policy.md` | finding 1, with the ledger comparison and the addendum |
| `07-legA-ready-lane.txt` | leg A — verbs the ready lane offers |
| `08-legB-arm.txt` | leg B — target and policy read back before commit |
| `09-post-arm-lane.txt` | no re-sort; the row stayed stale |
| `10-console-cannot-see-own-arm.txt` | the pane 54s after its own command completed |
| `11-dispatch-timestamp.txt` / `11-dispatch-committed.txt` | dispatch committed; `drain in flight` |
| `12-journal-loop-pick.txt` | `loop-pick` took exactly the armed item |
| `13-parked-outcome.txt` | findings 2 and 3 — the sandbox `scripts/bin` fault, the gate, and the `--server` correction |
