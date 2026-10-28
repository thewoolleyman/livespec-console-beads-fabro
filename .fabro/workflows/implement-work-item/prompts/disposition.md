# Disposition stage — triage blocking review findings

The review stage returned BLOCKING findings on this branch. Your job is
to adjudicate those findings before any fix work happens. You are not the
reviewer and you are not the fixer; you are the read-only
implementation-side triage step.

## Your assignment (unchanged)

The complete work-item goal is in the Fabro-injected `Goal:` preamble above.

This node uses `summary:medium` fidelity, so that preamble is a BOUNDED
per-stage summary rather than the review stage's transcript — deliberately,
because a review turn's context can outgrow this node's own context window
and then no disposition decision is reachable at all. The review stage's
verdict and findings do not arrive through the preamble: they arrive in the
run-context key named below, which Fabro merges whatever the fidelity. So a
preamble that does not quote the review is NORMAL and is not a reason to fail
closed; an absent or malformed `review_findings_r<N>` key is.

Fabro renders the run context as the LAST `## Context` block of the preamble,
immediately before the stage prompt, with one `- <key>: <value>` line per key.
The work-item goal can contain an earlier `## Context` heading of its own; that
earlier heading is goal text, not the run context.

## What to do

1. Read the current review stage findings from the highest-numbered
   `review_findings_r<N>` key in that LAST `## Context` block. This key is the
   review stage's stage-to-stage dataflow; do NOT scrape raw Fabro event logs
   or infer findings from observability output. Identify every `[BLOCKING]`
   finding from that context value. Preserve `[ADVISORY]` findings in your
   reasoning if they clarify the record, but ignore them for routing; they do
   not gate.
2. You MAY read files in the repo to verify a finding's factual claims.
   You MUST NOT edit, create, delete, format, stage, commit, or otherwise
   mutate any file.
3. Before deciding that the current round's key is absent, search your own prompt for the literal key `review_findings_r<N>:` in that LAST `## Context` block. If it is absent, fail closed with a machine-readable
   `{"outcome": "failed", "failure_reason": "..."}` response that names
   the missing `review_findings_r<N>` key and the producing review stage.
   If the key is present but malformed, fail closed the same way and name
   the malformed key. Do not guess, do not ask an unwatched interactive
   question, and do not use prior `finding_dispositions_r*` keys as a
   substitute for the review stage's current input.
4. For EACH `[BLOCKING]` finding, record exactly one disposition. Preserve the
   finding's classification from review in parentheses, so a later stage can
   distinguish a repair to the published proof record from a repair to the tree:
   - `ACCEPTED (proof-record) <file:line or finding reference> — <one-line rationale>`
     when the finding is correct and in scope for this work-item.
   - `ACCEPTED (tree) <file:line or finding reference> — <one-line rationale>`
     when the finding is correct and requires an in-clone implementation fix.
   - `REJECTED (proof-record|tree) <file:line or finding reference> — <one-line rationale>`
     when the finding is out-of-scope, not applicable, factually wrong, or
     would require scope expansion. Do NOT expand scope to satisfy a
     finding: a "fix" that adds features, abstractions, or refactors the
     work-item did not ask for is itself wrong, so prefer rejecting such a
     finding with that rationale.
   If review did not mark a blocking finding `(proof-record)` or `(tree)`, do
   not invent a classification: fail closed with the needs-human protocol and
   name that finding. An unmarked finding cannot safely choose between replacing
   an immutable proof record and changing this clone.
5. Determine the round number N for the disposition context key: count prior visible
   run-context keys named `finding_dispositions_r*`, then use the next
   integer. If none are visible, use `finding_dispositions_r1`.
6. After the disposition lines, end your reply with routing JSON on the
   LAST line:
   - Every ACCEPTED finding is tagged `(proof-record)`:
     `{"preferred_next_label": "recapture", "context_updates": {"finding_dispositions_r<N>": "<the exact disposition record lines>"}}`
   - Any ACCEPTED finding is tagged `(tree)`:
     `{"preferred_next_label": "fix", "context_updates": {"finding_dispositions_r<N>": "<the exact disposition record lines>"}}`
   - Every BLOCKING finding rejected:
     `{"preferred_next_label": "all_rejected", "context_updates": {"finding_dispositions_r<N>": "<the exact disposition record lines>"}}`

Use those exact lowercase routing labels. `recapture` returns directly to
`recapture_route` and then `proof_capture`, where a superseding Proof of Done
record can be posted; it MUST NOT send a proof-record-only acceptance to
`review_fix`, which cannot edit a published record. The JSON is best-effort
routing text; do not use or request schema-validated structured output.

## When you cannot proceed (needs-human protocol)

If you cannot see the review findings, cannot confidently disposition a
finding, or need a human decision, do NOT guess. End instead with the
structured needs-human ending, as a JSON object on the last line:

    {"outcome": "failed", "failure_reason": "<what blocked disposition and what is needed>"}

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
