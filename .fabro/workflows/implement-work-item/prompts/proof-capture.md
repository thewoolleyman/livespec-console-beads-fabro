# Proof-capture stage — prove the Definition of Done on the draft PR

The janitor gate is green and the `publish_draft` stage has pushed this
run's publish branch and left a DRAFT pull request open on it. Your job is
to prove, for each assertion of this item's Definition of Done whose proof
mode is `factory_captured`, that the assertion actually holds — by
authoring reproduction steps, executing them, capturing what they produce,
and posting ONE new Proof of Done record comment on that pull request.

You are the first of two independent legs. A later `proof_verify` stage
runs on a DIFFERENT adapter and replays your published steps verbatim on
the tree it receives. Everything you write is written for that stranger:
steps that only you could follow are steps that will not reproduce.

## Where you are

You are in the SAME isolated Fabro sandbox clone the implement and janitor
stages produced the committed work in — your CURRENT WORKING DIRECTORY is
that clone. Run every `git` and `gh` command here, in the current
directory. The assignment below may mention a `Repo:` path: that is the
dispatcher's host-side checkout, it does NOT exist in this sandbox, and
you must NEVER `cd` to it.

## Your assignment

The complete work-item goal is in the Fabro-injected `Goal:` preamble
above. Its `## Definition of Done` section is what you are proving.

## You MUST NOT modify the tree

This stage is READ-ONLY on the repository. The janitor has already passed
on these exact bytes and the reviewer is about to read them, so this node
is the one surface that could quietly make the tree disagree with both.

- Do NOT edit, add or delete any file in the repository.
- Do NOT commit, and do NOT push.
- Write every capture artifact to a scratch directory OUTSIDE the clone —
  use `"${TMPDIR:-/tmp}/proof-capture"`. Before the first reproduction step,
  run `mkdir -p "${TMPDIR:-/tmp}/proof-capture"`; the idempotent precondition
  lets the replay start on either a clean sandbox or the capture's prior leavings.
- Before you post, confirm `git status --porcelain` is EMPTY. If it is
  not, something you ran wrote into the tree: clean it up (`git checkout
  -- .` plus removing the untracked files you created) and re-confirm.

If an assertion genuinely CANNOT be proved without a code change — the
behaviour is missing, a command the assertion names does not exist, a
surface the assertion names is broken — that is an IMPLEMENTATION DEFECT,
not your problem to fix. Do not fix it. Report it: see "A capture that
needs a code change" below.

## Step 1 — read the Definition of Done and classify every assertion

Parse the `## Definition of Done` section out of the assignment. Each `- `
bullet is one assertion, and the declaration is POSITIONAL.
Bullets under a `### Host-captured` sub-heading have proof mode
`host_captured`; bullets under a `### Human-attested` sub-heading have proof
mode `human_attested`; every other bullet has proof mode `factory_captured`.

Keep the section's own order. You will publish per assertion in
**Definition of Done order**, and the replay leg and the post-merge
acceptance pass both index your record against that order.

You capture ONLY the `factory_captured` assertions, and you capture EVERY one
of them. Neither of the other two modes is yours:

- You never attempt to capture a `host_captured` one. Its proof needs the
  released, normally installed artifact, or a surface of an operator host
  that no sandbox image carries, so **an agent session on a host records it
  after merge** and a different session replays it.
- You never attempt to capture a `human_attested` one — a human attests
  those on the same pull request later.

You MUST still list both kinds in your record, under the TWO SEPARATE
headings named in Step 5. They are different pending legs and a reader has
to be able to tell them apart: an agent owes the host leg, and no agent can
owe an attestation.

A pending host-captured assertion NEVER excuses an uncaptured
`factory_captured` one. A record that lists a host assertion as pending the
host leg while leaving a factory assertion with no steps and no proof is an
INCOMPLETE factory proof record — the review and replay stages reject it as
such, and "pending" is not a label a factory assertion may borrow because
capturing it turned out to be awkward.

## Step 2 — author the reproduction steps for each factory_captured assertion

For each one, write NUMBERED reproduction steps that a different agent on
a different adapter can follow with no further context. Each step is one
concrete, copy-pasteable action or command. The steps must be:

- **Deterministic.** No "check that it looks right"; name the exact
  command and the exact expected observation.
  A step that reads git history MUST name a fixed commit sha, or use a
  depth-independent predicate. It MUST NEVER expect a commit inside a
  fixed-size window counted from HEAD.
  The reason: the engine appends a checkpoint commit after every node,
  so HEAD moves between capture and replay. A commit the window held
  when you captured sits below it by the time the replay leg looks.
  Rejected: `git log --max-count=2 --format=%H HEAD`, expecting a given
  sha among its lines.
  Accepted: `git merge-base --is-ancestor <sha> HEAD`.
- **Rooted in this sandbox.** Start from the clone's working directory and
  the tooling the sandbox actually has.
- **Credential-safe.** Where a step needs a credential, name it by its
  **environment-variable name** and name the wrapper that supplies it —
  **never a value**, never a fragment of one, and never a command whose
  output would print one. `$GITHUB_TOKEN`, supplied by the run's
  credential projection, is a reference; the token itself must appear
  nowhere.
- **Ordinal-bearing.** A step that produces a proof artifact MUST name the
  asset ordinal it produces, e.g. "produces proof 01". The replay leg
  compares its `verify` asset to the `capture` asset of the SAME ordinal,
  so a step that names no ordinal makes its own proof uncomparable.
- **Self-contained in its PROGRAM TEXT.** Every program a step executes that
  is NEITHER tracked in the repository at the commit this record names NOR an
  installed tool invoked by its name MUST be written into the record itself —
  its COMPLETE SOURCE in a fenced block, as part of the step or beside its
  proof — or into a proof asset the record names through the proof asset
  store. The test is replayability from three things alone: this record, that
  commit, and the sandbox's installed tools.
  - **These two are exempt, and naming them is enough.** A program the
    repository TRACKS at that commit may be invoked by its path, and an
    INSTALLED tool may be invoked by its name; neither has to be copied into
    the record, because the replayer has both.
  - **Everything else is not.** A heredoc into an interpreter (`python -`,
    `sh -s`), a script you authored in the scratch directory — anything under
    `"${TMPDIR:-/tmp}/proof-capture"` is untracked by construction — a `-c`
    one-liner, a patch or a mutation applied to a file: publish the source.
    Publishing only the invocation line and the program's OUTPUT does not
    satisfy this: the replayer is a stranger who may not reconstruct a
    program from its stdout, and may not infer one from the surrounding
    prose, so a step whose program is absent is UNREPLAYABLE, it is a
    blocking finding at review, and the assertion it carries is graded
    `not_reproduced` however correct the implementation is. Measured
    2026-10-20: records whose steps ran untracked `/tmp/proof-capture/*.py`
    programs the record never contained were rejected round after round
    until the run died on its firing budget.
  - A phrase such as "with the program shown in the capture" is a PROMISE
    this bullet makes binding — before you publish, re-read each such step
    and confirm the source it points at is really there.

## Step 3 — execute the steps verbatim

Now **execute them verbatim**, in order, exactly as written. This is not
a formality: the steps you publish are the steps that get replayed, so any
divergence between what you wrote and what you ran is a defect you are
shipping into the replay.

If a step does not work, FIX THE STEP, re-run from the top of that
assertion's sequence, and publish the corrected version. Never publish a
step you did not run, and never publish output from a command other than
the one the step names.

## Step 4 — capture the proof

### Exercise the behaviour, not the test suite

A behavioural assertion names an observable behaviour or state of the
DELIVERED ARTIFACT on a surface a user or operator of that artifact reaches
— a command and what it prints or changes, an API call and its response, an
interface and what it shows, a configuration file and the key it carries.
For such an assertion you MUST **exercise the behaviour through the surface
the assertion names**, driving the delivered artifact **as a user or operator
would**.

The output of this repository's own test suite **MUST NOT be the proof of a
behavioural assertion** — not alone, and not as the assertion's only step. It
MAY appear only as **supporting evidence beside a real exercise**. A suite
run proves that a test passed; it is silent on whether the artifact does the
thing, which is exactly what the assertion claims. (Suite output IS the right
proof for the one case where the assertion's own subject is a test, a check
or a gate — the gate has already refused that assertion on any other item.)

So: where the assertion names a command, RUN THAT COMMAND. Where it names an
entry point, invoke that entry point the way an operator invokes it. Where it
names a file and the text it carries, read that file out of the delivered
tree. Reaching for `pytest` because it is nearer to hand is the failure this
paragraph exists to stop.

### What to capture, per assertion

- **Anything reachable through a web interface** — a screenshot. The
  sandbox carries a headless browser layer; drive it to the surface the
  assertion names and capture a PNG.
- **Text-only behaviour** — a terminal capture: the command and its real
  output, captured verbatim.

Write each image into the scratch directory under this EXACT name:

    <work-item-id>__<run-id>__capture__<NN>__<slug>.<ext>

where `NN` is the two-digit ordinal of the proof within THIS record
(`01`, `02`, …) and `slug` is a short lowercase-kebab description of what
the image shows. The run id is part of the name so that no run can ever
overwrite another run's asset. Read it from `git config --get
livespec.factoryRunId` — the dispatch id the sandbox's own prepare step
declared, unique per dispatch, and the ONLY run-id source a shell in this
sandbox can read. If it answers nothing, STOP and end with the needs-human
protocol rather than inventing one.

Upload each image to the repository's standing proof-assets prerelease —
the Dispatcher created it before this dispatch — and keep the URL it
returns:

    gh release upload "$LIVESPEC_PROOF_ASSETS_RELEASE_TAG" \
      "${TMPDIR:-/tmp}/proof-capture/<asset-name>" --clobber

Proof binaries **MUST NOT be committed** to the repository tree; the
release asset store is the only place they live.

How the image is REFERENCED in the record depends on what the Dispatcher
measured for this repository and projected as
`$LIVESPEC_PROOF_ASSET_RENDERING`:

- `inline` — reference it inline so an authorized viewer sees the image in
  the comment: `![<slug> (proof NN)](<asset url>)`.
- `authenticated_link` — the forge cannot render this repository's assets
  inline for an authorized viewer, so the inline half is waived here.
  Reference it as ONE authenticated link per image instead:
  `[<slug> (proof NN) — authenticated link](<asset url>)`.

Use whichever the variable says; do not decide it yourself.

When the variable is ABSENT, use `authenticated_link`. That is the measured
fail-safe rather than a guess: an unset value means no visibility
measurement reached this run, and of the two forms only the inline one can
publish a reference that leaks from a repository nobody established was
public. The waiver costs an inline rendering; it never costs the proof. Say
in your final reply that you took the fallback, so a reader can tell it
from a measured `authenticated_link`.

### Keep every assertion section gradeable on its own

The `proof_grade` node grades the REPLAY's record with the same parser the
post-merge acceptance pass uses, and the replay re-prints your assertion
texts and your steps verbatim — so the section shape you author here is
the shape that gets graded. That parser resolves one assertion to the
NARROWEST heading-rooted section whose text contains it, and FIRST MATCH
WINS; it reads a `Reproduced:` line only outside a fenced block. Two
authoring habits defeat it, both silently and both in the fail-OPEN
direction: the record renders perfectly on the forge, and an assertion
reads as evidenced off a verdict belonging to some other assertion.

**Fence a payload that itself contains a fence using a LONGER delimiter.**
The parser closes a fenced block only on the same character with a run
AT LEAST AS LONG as the one that opened it, so a three-backtick line
inside the payload closes a three-backtick fence early, and every line
after it is read as record PROSE — a heading your proof merely PRINTED
opens a section, and a `Reproduced:` line your proof printed becomes a
verdict you never authored. Count the longest backtick run in the payload and open
with a strictly longer one: four backticks around a payload containing
three, five around one containing four.

**Never let one assertion's section carry ANOTHER assertion's text.** Not
the Definition of Done quoted as a block, not an enumeration of every
assertion, not a sibling assertion restated for context — and this binds
your PROOF PAYLOADS too, not only your prose, because the first-match
search reads fenced lines as well. A capture that prints this item's own
Definition of Done pulls every later assertion into the section that
printing sits in, and each of them then reports whatever verdict that one
section states. Each section carries its own assertion's text, verbatim,
and no other's. Where a step genuinely must print the whole Definition of
Done, bound it AT THE STEP so it prints only the assertion under proof —
the step you publish is the step that gets replayed, so a bound authored
here is one the replay inherits. Where even a bounded payload would carry
another assertion's text, do NOT truncate it: ATTACH that proof instead,
exactly as an over-allowance proof is attached below. The four reference
lines carry no assertion text, so the absorbing bytes leave the record
body while the full proof stays published and digest-checkable.

### Keep the record within its size budget

A record is ONE forge comment, and the forge enforces a ceiling on it. A
record the forge REJECTS is a LOST proof that reads downstream as an ABSENT
one: the acceptance pass finds no record, reports every assertion
unevidenced, and the item parks on a refusal naming the missing record
rather than the size that lost it. Nothing points at the real cause.

Three declared numbers bound this, all in **UTF-8 BYTES**, never characters:

- **262144 bytes** — the measured, enforced forge comment ceiling.
- **196608 bytes** — the declared record budget. Stay under it.
- **32768 bytes** — the per-assertion **inline allowance**: what ONE
  assertion's proof may spend inline before it must travel as an attachment.

Do not take the forge's own word for the ceiling. Its rejection message
reads `Body is too long (maximum is 65536 characters)`, and that message is
wrong in BOTH its number and its unit — the enforced ceiling is four times
it, and it counts bytes. The figures above were measured against live
GitHub; the measurement and its controls are in
`plan/archive/definition-and-proof-of-done/research/005-forge-comment-ceiling-measurement-2026-10-07.md`.

Measure in BYTES. `wc -c` counts bytes; `wc -m` counts characters, and on
multibyte proof — box-drawing tables, `git log --graph` glyphs, em dashes —
the two differ by up to a factor of four, in the direction that loses the
proof.

**Plan the recipe before you run it.** The allowance exists so you can
choose a bounded capture up front rather than discover at post time that
your evidence does not fit, by which point the steps have already run. Where
a step would print an unbounded amount, bound it AT THE STEP — name the
`| tail -50`, the `--max-count`, the summarising command — and publish the
bounded command as the step, because the step you publish is the step that
gets replayed.

**When a proof genuinely exceeds the inline allowance, attach it.** Never
truncate a proof to fit, and never drop an assertion to fit. A truncated
proof is a proof of something else, and a dropped assertion reads
downstream as unevidenced. Instead write the full captured
output to the scratch directory, upload it as an asset exactly as an image
is uploaded, and reference it from the record **in place of** the fenced
block. Name it with its own digest, so the name itself identifies the bytes:

    <work-item-id>__<run-id>__capture__<NN>__proof-sha256-<first 16 hex>.txt

Compute the digest and the size from the file you upload:

    sha256sum "<file>" | cut -d' ' -f1
    wc -c < "<file>"

and render these FOUR plain lines where the fenced block would have gone —
plain lines, not bullets, and each on its own line:

    Attached proof: <asset name>
    Attached proof bytes: <byte size>
    Attached proof digest: sha256:<hex digest>
    Attached proof asset: <asset url>

All four are load-bearing. The replay leg and the post-merge acceptance pass
FETCH that asset and check its digest against the one the record states; an
asset that is missing, or whose bytes hash to something else, is graded
**absent evidence** rather than a pass. So a digest that does not match what
you uploaded fails the assertion as surely as no proof at all.

## Step 5 — publish the record

Post exactly **one NEW comment** on the draft pull request for this item's
publish branch. Find it with `gh pr list --head <publish branch> --state
open --json number`, and post with `gh pr comment <number> --body-file
<file>`.

**MEASURE THE BODY FILE BEFORE YOU POST IT, and refuse to post it over
budget:**

    wc -c < <file>

If that is more than **196608** bytes, **do NOT post**. Attach the largest
proofs per the budget subsection of Step 4 and re-measure, until the body
fits. Report, in your final reply, the measured size, the budget, and the
assertion whose proof overflowed.

The measurement goes BEFORE the post and not after, because a record comment
**MUST NOT be edited after posting**: a body you have already posted is one
you can no longer withhold, and a rejected post leaves nothing behind to
read at all.

If every proof is already attached and the body is still over budget, then
the record is over budget in AGGREGATE rather than on any single proof — the
remedy is a smaller proof recipe, not another attachment. Say so and end
with the needs-human protocol, naming the measured size and the budget.

A record comment **MUST NOT be edited after posting**; if you get it
wrong, post a new record rather than editing the old one. Do not delete
any earlier record.

The comment's FIRST LINE must be exactly:

    Proof of Done — captured — run <run-id> — <UTC timestamp>

**Write that line with its run id resolved IN THE SAME COMMAND**, so no step
between resolving the id and writing the header can leave it empty:

    run_id="$(git config --get livespec.factoryRunId || true)"
    test -n "$run_id" || { echo 'LIVESPEC_PROOF_CAPTURE_NO_RUN_ID' >&2; exit 1; }
    stamp="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'Proof of Done — captured — run %s — %s\n' "$run_id" "$stamp" > <file>

If that resolution answers nothing, **never post a header with an empty run
id**: say what you measured and end with the needs-human protocol instead. A
record comment MUST NOT be edited after posting, so an empty run id cannot be
repaired within the run.

Do NOT reach for `$FABRO_RUN_ID` here. It is exported to HOOKS only and is
unset in this shell, so `printf '... run %s ...' "$FABRO_RUN_ID"` writes the
bare word `run` as the header's publishing identity. The record reader then
treats the WHOLE comment as no record at all: the replay still runs and still
publishes `verified`, review still approves, and the run fails two stages
later at publication with no route back, because a resume restarts at the
failed stage and never re-enters this one. Measured on
`livespec-dev-tooling-r6kagr`, run 01M5E8H65Z04. Report the id you wrote by
quoting the resolved value, never the source you believe it came from — the
run above reported `$FABRO_RUN_ID` as its source after writing nothing.

The SECOND line must declare, exactly once, the publish-branch head these
steps ran on:

    Publish-branch head: <the full 40-character commit sha of the pull request's head>

Read it from the FORGE, not from the clone: `gh pr view <number> --json
headRefOid --jq .headRefOid` for the draft pull request you are commenting
on, and paste the full sha — never an abbreviation, and never a branch name.
The engine commits a checkpoint after every node, publish_draft included, so
the clone's own `git rev-parse HEAD` is usually one or more UNPUSHED commits
above what the pull request carries, and a record naming it anchors nothing
the pull request has. Then confirm the pull request's head is the TREE your
steps ran on: run `git fetch --quiet origin "refs/pull/<number>/head"` and
then `git diff --quiet FETCH_HEAD HEAD`. Exit status 0 means the clone
differs from the pull request's head only by commits that change no file,
so that head is the one these steps proved — declare it. Any other status
means the clone carries changes the pull request does not: then do NOT
write the label at all, and say in one prose sentence that no head is
declared because the clone's HEAD (give its full sha) carries changes the
pull request's head (give its full sha) does not. It is what lets a
later `resume` finish this run from this pull request instead of
re-implementing it: the resume refuses when the pull request's head no
longer equals the head this record names, because proof verified on one
tree is not proof of another. Declare it in PROSE, outside every fenced
block; a head line inside a fence is proof OUTPUT and is deliberately
ignored. If any proof below happens to print this label, that printed line
is ignored too — so the declaration above is the only one that counts, and
naming two different shas in prose makes the record anchor nothing.

Then, **ahead of every numbered step in the body**, declare the directory
those steps ran from — in PROSE, outside every fenced block:

    Working directory: .

Write that line
**exactly when every one of its steps ran from the repository root**,
which is where a reproduction step should run unless the behaviour under
proof is about somewhere else. Where any step ran
somewhere else, say so by naming that directory on the same `Working
directory:` line instead, and do NOT write the root form: the line is read
mechanically by a later `pr_refresh`, which carries this proof across a
refresh only when it can re-run every step from the root itself, so a
root declaration over steps that ran elsewhere would have it re-run them
in the wrong place. Declaring another directory costs nothing but that
carry-forward; it never fails the record.

The body then carries, **per assertion in Definition of Done order**:

1. The assertion text, verbatim.
2. Its proof mode.
3. The **governing scenario**: the referenced heading and **the title of the
   scenario your steps exercise**. Where the item's reference line names a
   `## Scenario NN — <title>` heading of `scenarios.md` that states this
   assertion's behaviour, that heading GOVERNS the proof, and your
   reproduction steps SHOULD **follow one of that heading's scenarios step
   for step** — the scenario's own Given/When/Then is what the proof is
   meant to walk. Where no referenced scenario heading governs this
   assertion, say so: write that **no scenario governs** it. Do not leave
   the entry out — an absent entry is indistinguishable from one you forgot
   to fill in, and a reader cannot tell the two apart.
4. The numbered reproduction steps you authored and ran, each carrying the
   COMPLETE SOURCE of any program it runs, per the self-contained bullet of
   Step 2.
5. The proof — an inline image reference (or authenticated link, per Step
   4) for each screenshot, and a fenced code block for each text capture.
   A text capture shows the command AND its output; when the command fed a
   program to an interpreter, the program's source is part of what is
   published, never replaced by the output it produced.

When this item has any `host_captured` assertions, the record MUST also
carry a heading stating that those assertions are **pending the host leg**,
listing each of them. When it has any `human_attested` assertions, the
record MUST carry a SEPARATE heading stating that those are **pending human
attestation**, listing each of them. Where the item has both, both headings
appear and neither absorbs the other — they name different legs, owed by
different parties, and the post-merge acceptance pass reads them to decide
what the item is still waiting on. A reader of this record must be able to
see what the factory did not prove AND who owes each remaining piece.

A worked shape, for an item with two factory-captured assertions, one
host-captured one and one human-attested one:

    Proof of Done — captured — run 01M3EXAMPLERUNID — 2026-10-01T06:00:00Z

    ## Assertion 1 — `list-work-items --json` projects the `parent` field.

    Proof mode: `factory_captured`

    Governing scenario: `## Scenario 42 — <title>` — "A listing projects an
    item's parent", whose steps these follow.

    Reproduction steps:

    1. From the clone root, run
       `uv run livespec-orchestrator-beads-fabro-list-work-items --json`.
       Produces proof 01.
    2. Confirm every record in the emitted array carries a `parent` key.

    Proof 01:

    ```
    $ uv run ... --json | python -c 'import json,sys; ...'
    all 638 records carry parent
    ```

    ## Assertion 2 — The console renders the capacity banner.

    Proof mode: `factory_captured`

    Governing scenario: none — the reference line names only
    `## Runtime requirements`, and no scenario states this behaviour.

    Reproduction steps:

    1. Start the console with `… --port 8099`; requires `$GITHUB_TOKEN`,
       supplied by the run's credential projection.
    2. Screenshot `http://127.0.0.1:8099/runs` headlessly. Produces proof 02.

    Proof 02:

    ![capacity-banner (proof 02)](<asset url>)

    ## Pending the host leg

    These assertions are NOT captured here. Their proof needs the released
    build on an operator host, so an agent session there records them after
    merge and a different session replays them:

    - The installed plugin resolves its prompts from the released build.

    ## Pending human attestation

    These assertions are NOT captured here and await a human record on this
    pull request:

    - The banner's wording reads naturally to a maintainer.

## Step 6 — final reply

Report, in your final reply: the pull request number, the comment id or
URL of the record you posted, each asset name you uploaded, and the
confirmation that `git status --porcelain` was empty. Report any deviation
verbatim.

Do NOT emit a routing label on success. The graph's edge from this node to
`review` is UNCONDITIONAL, so a label would be ignored — and a label that
reads like `fix` would route a successful capture into the fix loop.

## A capture that needs a code change

When an assertion cannot be proved because the IMPLEMENTATION is wrong or
incomplete, do not edit anything. PUBLISH THE FINDING AS A RECORD, then
route to the `fix` node.

Publishing it is not optional, and the reason is that your stage output
alone can be lost. The finding reaches `fix` by TWO routes: the engine's
preamble of preceding stage output, and your record on the pull request.
When the preamble carries no finding, the record is the only place left to
read it from — so a capture that routed to `fix` while publishing nothing
would leave a run whose only account of the defect dies with its preamble.

Post it exactly as Step 5 posts a successful capture: ONE NEW comment on
the same draft pull request, never an edit of an earlier record and never a
deletion of one. Its FIRST LINE must be exactly:

    Proof of Done — not_captured — run <run-id> — <UTC timestamp>

written with its run id resolved in the SAME command, exactly as Step 5 writes
the captured header:

    run_id="$(git config --get livespec.factoryRunId || true)"
    test -n "$run_id" || { echo 'LIVESPEC_PROOF_CAPTURE_NO_RUN_ID' >&2; exit 1; }
    stamp="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'Proof of Done — not_captured — run %s — %s\n' "$run_id" "$stamp" > <file>

The same rule binds here and for the same reason: never post a header with an
empty run id, and end with the needs-human protocol instead if the resolution
answers nothing. This record is the `fix` stage's only durable account of the
finding once the preamble is gone, so a header that parses as no record loses
the work order as surely as a non-record capture loses the proof.

The body then carries, per assertion in Definition of Done order:

1. The assertion text, verbatim — for EVERY `factory_captured` assertion you
   could not capture, one heading each, so a reader can see how much of the
   Definition of Done is unproved rather than only the first thing that
   broke.
2. Its proof mode.
3. The governing scenario, exactly as Step 5 describes it — the referenced
   heading and the scenario title your steps exercise, or that no scenario
   governs the assertion.
4. The numbered reproduction steps you authored and ran.
5. The FINDING: the step that failed, the result that step expected, what
   you observed instead, and what the implementation would have to do for
   the step to pass. Write it as a work order for a different agent on a
   different adapter — it is the `fix` stage's input, and a finding only you
   could act on is a finding that stage cannot act on at all.

Any assertion you DID capture in the same visit belongs in the same record,
under its own heading with its proof, exactly as Step 5 describes. One visit
publishes one record.

Then end your final reply with exactly this JSON object on the last line:

    {"preferred_next_label": "fix"}

That routes to the `fix` node, where a code change belongs, and the loop
re-earns a green janitor and re-enters this stage with a fresh tree. The
route is CAPPED: the third `fix` verdict from this node in one run routes to
the `non_converged` terminal instead, which the Dispatcher reads as
`needs-regroom`. A Definition of Done that will not capture three times
running is a sizing problem, not a fix-loop problem.

## A surface this sandbox cannot reach

An assertion whose surface **cannot be reached with the sandbox's
capabilities** — a terminal multiplexer, a browser, a tool the image does
not carry — ends the run. You **MUST NOT** stand anything in its place: no
**a test run, a fixture or a test double**, no equivalent command against a
different surface, and no record claiming the assertion from any of them. A
record that claims an assertion from a stand-in is worse than no record,
because the replay leg will reproduce the stand-in faithfully and publish
`verified` for a behaviour nobody exercised.

The capability set is what the `dod_gate` node resolved at the head of this
run: the file `/etc/livespec/sandbox-capabilities` where the image publishes
one, else the repository's committed `dispatcher.sandbox_capabilities` array.
Read it the same way; do not infer a capability from the item's prose or from
what this image usually ships.

End your final reply with the failed outcome, **naming the assertion and the
missing capability**, as a JSON object on the last line:

    {"outcome": "failed", "failure_reason": "<the assertion; the capability its proof needs; that no capability in the resolved set provides it>"}

**A missing capability is not an implementation defect, so do NOT route it
to `fix`.** The
discriminator is whether a code change could fix it: a missing behaviour, a
command the assertion names that does not exist, a broken surface — those are
defects and belong in the section above. Here the implementation may be
perfectly correct and **no code change could add the capability** to the
image, so routing to `fix` would send an implementer to repair code that is
not wrong. The remedy is the gate's own first remedy — add the capability to
the sandbox image — or a re-declaration of the assertion's proof mode, and
both are human decisions taken off the run.

## When capture is blocked (needs-human protocol)

If capture is blocked in a way that is NOT an implementation defect and
that you cannot legitimately resolve — no run id resolvable, no
`$LIVESPEC_PROOF_ASSETS_RELEASE_TAG` value, no draft pull request on the
publish branch, the asset upload refused, `gh` auth failure — end your
final reply with the failed outcome and a STRUCTURED reason, as a JSON
object on the last line:

    {"outcome": "failed", "failure_reason": "<what is blocked; what you tried; what decision is needed>"}

The graph routes a failed outcome to the terminal `needs_human` node: the
run preserves the tree on a run-scoped ref and ends, and the work-item
rests in the ledger at `blocked / needs-human` until a human decides.

## Ending the turn — leave nothing running in the background

The ACP turn this stage runs inside cannot COMPLETE while the agent
session still has work outstanding, so anything you leave running holds
the turn open until the node's own timeout kills it — and work that had
already finished is then recorded as a timed-out stage instead of the
green result it was. Measured repeatedly on this factory: stages that had
already emitted their final message sat idle for 28, 75 and 89 minutes
before the ceiling fired, and four further runs were lost in one night to
a single backgrounded command.

So, in this stage:

- NEVER background a tool call. Do not pass `run_in_background` (or any
  other detach flag) to a shell tool, and do not start a poller, a
  watcher, a `tail -f`, or a loop that waits for a condition.
- Run a long command in the FOREGROUND and raise THAT call's own timeout
  instead. A full check suite, a dependency install, or a long
  verification wait belongs in ONE blocking call whose output you read,
  never in a background job you poll.
- If something IS still running when you are ready to finish, STOP it
  before your final message (`TaskStop`, `KillShell`, or whatever kills
  what you started) and confirm it is gone.
- Your final message must be the LAST thing the turn does. Do not start
  any new tool call, probe, or cleanup after it.
