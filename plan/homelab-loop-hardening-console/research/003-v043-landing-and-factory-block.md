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

## Next

Retry `impl:livespec-console-beads-fabro-e55mov` once the credential
condition clears. Do not re-attempt before then, and do not hand-hunt
the credential.

*(Superseded in-session: the maintainer rotated the token and directed a
retry; the retry cleared the credential gate. `gkeopn` is filed, ready,
and deliberately UNDISPATCHED — the maintainer's instruction covered
retrying prior dispatches, and `gkeopn` had never been dispatched.)*
