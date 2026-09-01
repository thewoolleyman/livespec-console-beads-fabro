# Fix stage — the janitor gate is red

The loop routed here after a red janitor check or a human-requested
retry from an implement, disposition, review_fix, or PR-stage failure.
The relevant failure output or operator context is in the prior stage
context above.

## Your assignment (unchanged)

{{ goal }}

## What to do

1. Read the janitor failure output and diagnose the root cause.
2. Fix it IN THIS CLONE, honoring every rule from the implement
   stage: no `--no-verify` (if a hook fails, fix the cause or end with
   the needs-human protocol below, reporting its output verbatim);
   plain `git` for all writes (the installed hooks fire on their own);
   Red-Green-Replay for any
   product `.py` change (a test-file change means a fresh Red commit,
   never an edit under an existing Green); commit trailer
   `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`; never
   create or switch branches, never touch `.beads/` or `core.bare`.
3. Re-run the repository's check suite (`{{ inputs.sandbox_check_suite }}`) yourself until it is green.
4. Summarize the diagnosis and the fix in your final reply.

## When the failure is not auto-resolvable (needs-human protocol)

If the failure needs a human decision, is NOT caused by this branch's
changes (e.g. an upstream red inherited from {{ inputs.default_branch }} — say so with
evidence), or you have proven you cannot legitimately fix it, do NOT go
quiet and do NOT paper over it. End your final reply with the failed
outcome and a STRUCTURED reason, as a JSON object on the last line:

    {"outcome": "failed", "failure_reason": "<what is blocked; what you tried; what decision is needed>"}

When the fix loop's budget exhausts, the graph terminates the run at the
`needs_human` node and rests the work-item at `blocked / needs-human` in
the ledger; your structured reason is what the human reads first — make
it actionable.
