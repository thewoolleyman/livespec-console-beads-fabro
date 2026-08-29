# 003 — The v043 landing, what ratification review changed, and the factory block

Recorded 2026-08-26. Covers the arc from the deferral-key verification
(note 002) through the v043 revision and the closure of
`livespec-console-beads-fabro-2fr7hy`, and records the factory-dispatch
block that stopped the code legs. Persisted here rather than left in
session scrollback because every item below is durable: the traps cost
real rework, and the next session should not rediscover them.

## What landed

| Artifact | Item / leg | Evidence |
| --- | --- | --- |
| Deferral-key verification; leg 1 re-keyed | — | PR #831, `154fb20` |
| Bundled propose-change | leg 3 (spec carrier) | PR #832, `9cb9873` |
| **`SPECIFICATION/history/v043/`** | legs 2, 6, 7 spec halves | PR #833, `6ce1e10` |
| `2fr7hy` closed completed | leg 3 | epic 2/6 |
| `e55mov`, `ets4om` unblocked | legs 7, 2 code | pending-approval |
| `evasgx` ready | leg 6 row 1 | factory-eligible |
| `sc23vv` blocked | leg 1 | charge-point-8 matrix |
| `gkeopn` filed | side finding | coverage-gate hole |

## Ratification review changed the outcome

This was the first exercise of this repo's armed `spec_governance`
where the independent reviewer was not a rubber stamp. It returned
BLOCKERS twice before `NO BLOCKERS`, and the accepted bytes reach
further than the proposal drafted.

The proposal de-enumerated ONE frozen settings count. Six were fixed:

- **Round 1** — `contracts.md`'s "the six global defaults", sitting
  EIGHT LINES above the clause forbidding frozen counts; `spec.md`'s
  "governed by six orchestrator-owned `dispatcher.*` policy settings …
  (the ratified set: …)"; and `spec.md`'s "Five of the six settings
  admit a per-item override". This pulled `SPECIFICATION/spec.md` into
  the change set — it was not a proposal target.
- **Round 2** — `scenarios.md`'s lowercase "the **five** overridable
  settings are …" plus "the remaining three settings", and a
  `contracts.md` serves-list forming a complete partition with no
  illustrative marker.
- **Self-found** — `contracts.md`'s "the orchestrator's **three**
  per-setting override actions".

The sharpest finding is the interaction: round 1's fix ADDED a spec.md
clause saying the console must take the overridable set from the
orchestrator's declaration "rather than from a count fixed here" — and
round 2 then found that `scenarios.md` still fixed exactly that count.
**Fixing one instance of this defect created the contradiction the next
round caught.** A single-pass sweep would have shipped it.

Recorded as `modify`, not `accept`, so the v043 revision's
`## Modifications` section durably names all six fixes, which round
forced each, and the four counts deliberately LEFT with their stability
reasoning. Editing the merged proposal to match would have erased the
evidence that review changed anything.

### Counts deliberately left, and why

Not every number is rot. These were checked and kept:

- "eight commands, seven of them mapping 1:1" — the console command
  vocabulary is console-owned and does NOT grow when the orchestrator's
  key set grows, because a new overridable key rides the generic eighth
  command. It moves only with a `drive` action-id ratification, a
  different and versioned upstream surface.
- "in lockstep, in three places" — structural (row / inline help /
  settings doc), one per key.
- "the two human valves" — the orchestrator's structural valve pair.
- "differs between two contexts" — generic phrasing, not an enumeration.

The distinguishing test: does the number count something the
ORCHESTRATOR can grow by ratifying a key? If yes it is rot; if it counts
a console-owned or structural set, it is stable.

## Two mechanical traps in the spec-check coupling

**1. The coverage gate binds a registered test per scenario HEADING.**
`check-behavior-coverage` fails on any live H2 with no test
(`untested_scenarios`). The first draft of this proposal opened two NEW
scenario headings — which would have been untested until their code legs
land, breaking the gate on a spec-only change and forcing spec and code
to land together, destroying the charge's spec-before-code sequencing.

The fix, and the technique v042 also used: append the new Gherkin
`Scenario:` blocks under EXISTING headings that already carry registered
tests. Individual `Scenario:` blocks inside a `Feature:` are not H2s, so
they add no untested heading. Here: Scenario 12 for the tolerance
scenarios, Scenario 2 for the attribution scenarios.

**2. Clause ids hash the HARD-WRAPPED SOURCE LINE.** Reflowing a
`MUST`-bearing line silently re-keys that clause and orphans its
`tests/heading-coverage.json` link — no error, just a newly "unlinked"
clause plus a stale registry entry. One v043 fix was deliberately
rewrapped to leave every `MUST` line untouched, and the unlinked-id set
was verified byte-identical before and after (20 ids, exact match).

A practical consequence: several of v043's 20 clause ids are FRAGMENTS
of a single hard-wrapped sentence. Any future reflow of those paragraphs
invalidates the links and forces a re-pin.

**Ground truth** moved 213 → 230 (`18/133/22/57`). `contracts.md` is
**+16 net, not +18**, because the settings rewrite replaced existing
clauses as well as adding them. Take these numbers from the ground-truth
test's own assertion output; inferring them from the unlinked-clause
count gives the wrong answer.

## The coverage gate accepts a placeholder (filed as `gkeopn`)

`console-spec-check` treats a scenario as tested whenever its registry
entry carries ANY `test` value, reporting `InvalidTestRegistration` only
for a CONCRETE registration that fails to resolve. So `"test": "TODO"`
satisfies it. Scenario 10 carries exactly that, with nine clauses
already behind it before v043 added a tenth.

This is self-concealing: the gate prints "behavioral coverage clean
(0 unlinked, 0 untested, 0 invalid test registrations)" while the hole
is open. Filed as `livespec-console-beads-fabro-gkeopn`.

## The factory block

Dispatch of `e55mov` was attempted after maintainer authorization and
refused. Two distinct failures, worth separating:

**Stale plugin BINDING, not a stale install.** The dispatcher refuses to
run a build older than the latest release; the session resolved
`ed1013833793` against release `fa53a71e9bac`. But
`claude plugin update … --scope project` reported the cache was ALREADY
at `fa53a71e9bac` — the staleness was the SESSION'S SKILL BINDING, not
the installation. The update command is a no-op for that condition; a
session restart is the real fix, and the in-session workaround is to
invoke the newer build's `bin/` scripts by absolute path. This
distinction belongs in
[`.ai/livespec-plugin-currency.md`](../../../.ai/livespec-plugin-currency.md)
if it recurs.

**The real blocker.** From the current build:

> C-mode dispatch refused before sandbox launch:
> `CLAUDE_CODE_OAUTH_TOKEN` is exhausted or rate-limited (HTTP 429,
> rate_limit_error). Observed condition: exhausted.

Refused BEFORE sandbox launch, so no run was created, no PR opened, no
merge occurred — every item is untouched and safe to re-dispatch. The
condition is credential-level, so `ets4om`, `evasgx`, and `gkeopn` were
deliberately NOT attempted: they share the wrapper credential and would
fail identically while consuming quota.

Every remedy the dispatcher names — wait out a rolling limit, raise the
org billing limit, re-mint via `claude setup-token` and rotate the
wrapper secret — is a secret or host operation requiring maintainer
authorization.

**On the in-session alternative.** The `implement` operation's Step 0
exception permits driving Red→Green in-session when "the factory is
unavailable AND the work must not wait". The first half holds; the
second does NOT clearly, because the dispatcher's own prescribed remedy
for a rolling rate limit is to WAIT. Switching route is therefore a
maintainer call, not a session default.

## The diagnostic was wrong, and how

The maintainer's read — an EXPIRED token, rotated maintainer-side —
was correct; this session's read of a provider rate limit was not.
Tracing why matters more than the mistake.

The provider supplied exactly two values: `http_status` 429 and
`error_type` `rate_limit_error`. Everything else in that message is
composed by the Dispatcher
(`_dispatcher_claude_credential.py`, `_dispatcher_credentials.py:285`).
Its classifier is an ordered branch chain:

- the `revoked` branch fires ONLY on `401` or `authentication_error`.
  Its text says "revoked, EXPIRED, or malformed" and its remedy is the
  correct one — `claude setup-token`, then rotate the wrapper secret;
- the `exhausted` branch fires on `http_status in {402, 429}` OR
  `error_type in {billing_error, rate_limit_error}`, and its remedy
  LEADS WITH "For a rolling rate limit, wait before retrying".

So an expired token presenting as 429 is **structurally incapable** of
reaching the branch carrying its own fix. It matches `exhausted` on both
disjuncts and the operator is handed the one remedy that can never work.
Reordering would not fix it — 429 is genuinely ambiguous between
capacity and expiry. The fix is to treat 429 as UNDETERMINED and emit
both remedies with expiry named first (rotation is cheap; waiting is
unbounded), or to probe distinguishingly before classifying.

**And the correct remedy is on screen, misattributed.** This is worse
than it being merely unreachable. The `exhausted` branch's own remedy
text reads, in full: *"For a rolling rate limit, wait before retrying;
for an org spend or billing limit, raise the billing limit or re-mint
with `claude setup-token` under a healthy org and rotate the wrapper
secret."* Rotation — the actual fix — is right there in the text the
operator reads, attributed EXCLUSIVELY to a billing condition. So an
operator with an expired token reads the rotation advice and correctly
rules it out, because their situation is not a billing limit. An absent
remedy invites a question; a present-but-miscaused one closes it.
Verified at `_dispatcher_claude_credential.py:101-103`.

Note the classifier already KNOWS 429 is ambiguous — it pools `402` and
`429` into a single capacity set — so this is a distinction it collapsed,
not one it never had. The argument for treating 429 as undetermined is
asymmetric cost: rotation is cheap and bounded, waiting is unbounded and
silent, so an ambiguous signal should break toward the remedy whose
failure mode is a wasted minute rather than an indefinite stall.

The line an operator reads first is `Observed condition: exhausted`.
Nothing observed "exhausted" — the only observation was 429. The word
`Observed` is an f-string label applied to the classifier's own guess,
laundering a classification into a measurement. Filed upstream via
homelab `hl-allzdn`.

**The generalizable reading skill:** elapsed time alone proves nothing,
but elapsed time PAST A GATE THAT PREVIOUSLY REFUSED IN SECONDS is a
real observation. The pre-rotation dispatch failed almost immediately;
the post-rotation retry running for many minutes is positive evidence it
cleared the credential check, available long before any terminal result.

## The species this shares with `gkeopn`

Both are checks reporting a conclusion stronger than their evidence, and
both fail SILENTLY and AFFIRMATIVELY:

- `Observed condition: exhausted` asserts a measurement it did not make.
- `behavioral coverage clean (0 unlinked, 0 untested, 0 invalid test
  registrations)` is printed while ten clauses sit behind `"test":
  "TODO"`. The gate has verified that every scenario carries a non-empty
  test STRING, and reports it as every scenario being exercised.

The affirmative direction is the dangerous one: a check that merely
misses something leaves you uncertain and still looking, while a check
that prints all-clear stops the search.

**Two independent occurrences, in two repositories, on one day, is a
class and not a coincidence.** Name it when it appears: a check whose
output asserts more than its evidence supports. The tell is a summary
line that reports a CONCLUSION (`clean`, `exhausted`, `verified`) where
the check only measured a PROXY (a non-empty string, an HTTP status, a
config key's presence). The remedy is the same in both cases — make the
affirmative line unreachable unless the thing it claims was actually
measured.

Programme doctrine, now binding and applicable to both: **verifying that
a lever EXISTS is not verifying that it DOES what it is believed to do.**
`gkeopn`'s acceptance was widened accordingly — after the fix, a tree
containing a non-resolving registration MUST NOT be able to print the
clean line. Rejecting the placeholder while leaving the all-clear
reachable would not close it.

## A bound on unbounded work — two instances, opposite failures

A factory dispatch is implement → PR → CI → merge → post-merge janitor.
That is unbounded work. Bounding it with a timeout produced two failures
in one day, in OPPOSITE directions, from the same root:

- **This session, near-miss.** The dispatch was wrapped in
  `timeout 1100`, sized for a command that had previously failed in
  SECONDS. It came within 86 seconds of SIGTERM-ing a live run. The
  timeout wrapper was killed on its own (`kill -9` on the `timeout` PID
  only), which leaves the child running and reparented — `timeout`
  signals its child on expiry and cannot do so once dead.
- **Orchestrator, actual.** A 20-minute foreground dispatch timeout
  FIRED during the post-merge janitor and STRANDED the claim: item
  `active`, lock held, no live run, and the work ALREADY MERGED.

The bound in this session's case was correctly sized for the behavior
that had been OBSERVED — and was wrong precisely because the observed
behavior was itself the defect. **A bound calibrated on broken behavior
becomes a hazard the moment the thing starts working.** That is the same
family as the rest of this note: a measurement standing in for a fact
never established.

**Rule:** never bound a factory dispatch with a timeout. Use an
unbounded background run plus the completion notification.

**If a dispatch IS severed:** the item may be `active` with a held lock
while the work is already merged. The remedy is `reconcile-merged` —
**never re-dispatch**, which would duplicate merged work against a held
claim.

## The stale-build refusal recurs, and why the obvious fix is a no-op

The Dispatcher refuses to run a plugin build older than the latest
release. This fired TWICE in one session:

- session build `ed1013833793` vs release `fa53a71e9bac`;
- then `fa53a71e9bac` vs release `3382863e68bd`, after the orchestrator
  cut 0.73.0 mid-session.

Both refusals happen BEFORE sandbox launch, so the item is untouched and
safe to re-dispatch — this is a benign failure, but a confusing one.

The trap: `claude plugin update … --scope project` reports **"already at
the latest version"** both times, because the INSTALL is current. What
is stale is the SESSION'S RESOLVED BINDING — the plugin-root path fixed
when the session started. The update command cannot fix that; only a
session restart re-binds it.

**Workaround:** resolve the build id FRESH at each dispatch from the
plugin's own authoritative version report, and invoke that build's
`bin/drive.py` by absolute path. Do not reuse the path resolved at
session start.

```bash
BUILD=$(claude plugin update livespec-orchestrator-beads-fabro@livespec-orchestrator-beads-fabro \
          --scope project 2>&1 | grep -oE '[0-9a-f]{12}' | tail -1)
```

**Do NOT derive the build from the filesystem.** Two attempts at that
failed here, and both failed SILENTLY enough to look plausible:

- `ls -1t | grep -E '^[0-9a-f]{12}$'` returned nothing, because this
  host's `ls` is decorated (inode, permissions, size, date columns), so
  no line is a bare directory name. The variable came back empty and the
  invocation ran against a malformed path.
- Sorting the cache directories by mtime picked an OLDER build
  (`96b13b96975b`) than the current one: directory mtimes do not track
  install order.

Only the authoritative report was right — and it returned `db09c5a0c98d`
minutes after a refusal had named `3382863e68bd`, so the build moves
fast enough that a value cached even briefly can already be stale.

## The pattern behind several of today's failures

Two distinct families are worth naming, because instances of both keep
appearing and each is invisible from the error text alone.

**Conclusion-over-proxy** — a summary line reports a CONCLUSION where
the check measured only a PROXY:

| Claim | Actually measured |
| --- | --- |
| `Observed condition: exhausted` | an ambiguous HTTP 429 |
| `behavioral coverage clean (0 untested)` | a non-empty `test` string |
| `bump-minor-pre-major VERIFIED LIVE` | a config key's presence |
| a factory run reporting `green` | the dispatcher's report, while the run's own `run_turn` telemetry span never landed |

The tell is an affirmative summary; the remedy is to make that line
unreachable unless the thing it claims was actually measured.

**Remedy-mismatched-to-condition** — a diagnostic whose prescribed fix
cannot touch the actual condition:

- "wait before retrying" for an EXPIRED credential (waiting never
  clears it, and rotation appears on screen attributed to billing);
- "run `claude plugin update`" for a stale session BINDING (the install
  is already current; only a restart re-binds).

Both send a competent operator confidently in the wrong direction, which
is worse than an unhelpful error: the operator stops looking. When
writing a diagnostic, the test is not "is this remedy true of some
condition that produces this signal" but "is it true of EVERY condition
that produces this signal" — and where the signal is ambiguous, name the
ambiguity and order the remedies by asymmetric cost.

This belongs in `.ai/livespec-plugin-currency.md` if it recurs a third
time — the existing note covers keeping plugins current, but not the
stale-BINDING-vs-stale-INSTALL distinction, which is what makes the
diagnostic misleading.

## Merged work stays `active`, and nothing says so

Found while cross-checking the plan epic's completion count against the
forge, to write an accurate handoff. The epic read 2/6 while three
children had merged green.

Raw status after three successful factory dispatches:

    e55mov -> active | resolution: None   (PR #834 merged, janitor green)
    ets4om -> active | resolution: None   (PR #835 merged, janitor green)
    evasgx -> active | resolution: None   (PR #836 merged, janitor green)

Each dispatch had reported stage `done`, status `green`, detail
"merged, post-merge janitor green", and each fix was verified present in
`origin/master`. `dispatcher.py reconcile-merged --item <id>` (unforced)
re-reported the SAME terminal record and did NOT change the status.

**Neither the factory dispatch nor `reconcile-merged` closes a freeform
factory-dispatched work-item.** Closure is the `implement` operation's
freeform completed path. Absent someone running it, the ledger
under-reports completed work indefinitely, with no signal emitted.

**Why this is worse than bookkeeping.** The plan archive gate refuses
while any child is undisposed, and undisposed means "status is not
closed". Merged-but-active children therefore silently DEADLOCK the
archive of the epic that owns them — an epic whose work is 100% merged
can sit un-archivable forever, presenting as a stubborn gate rather than
a wrong store.

**It is the pattern inverted.** Every other instance in this note is a
check claiming MORE than it measured. This is the ledger claiming LESS
than reality. Both corrupt the record, but this direction is sneakier:
there is no CRITICAL to ignore and no misleading remedy to follow,
because nothing is emitted at all. The only way to find it is to
cross-check the epic's completion count against the forge.

**One datapoint, and the half that stays open.** The orchestrator's own
per-item dispatch of `bd-ib-ujihbw.1` (PR #1863) ended `closed` with
`resolution: completed` — but closed BY THE SESSION that drove it, which
supports this finding rather than contradicting it: closure is a session
responsibility, that session discharged it, and nothing discharged it
here. What remains unanswered is whether the Dispatcher's DRAIN path
(`dispatcher.py loop`, as opposed to a per-item `impl:<id>` dispatch)
closes what it drains. If the drain closes and only per-item dispatch
does not, the defect narrows considerably. Neither this session nor the
orchestrator's could answer it; it belongs in the upstream filing.

**Standing practice until upstream rules**, adopted program-wide off
this finding: after any green dispatch report, verify the merge on the
forge, close the item explicitly with the evidence, and never trust an
epic's completion count without a forge cross-check.

Closed here against verified forge AND code evidence, with the trap
recorded on each item's timeline. Not filed upstream: whether "the
factory does not close items" is a defect or the intended division of
labour between dispatch and `implement` is the orchestrator's call — but
if it is intended, the gap is that nothing tells a caller their merged
item still needs closing, which is still a defect.

## Next

Retry `impl:livespec-console-beads-fabro-e55mov` once the credential
condition clears. Do not re-attempt before then, and do not hand-hunt
the credential.

*(Superseded in-session: the maintainer rotated the token and directed a
retry; the retry cleared the credential gate. `gkeopn` is filed, ready,
and deliberately UNDISPATCHED — the maintainer's instruction covered
retrying prior dispatches, and `gkeopn` had never been dispatched.)*
