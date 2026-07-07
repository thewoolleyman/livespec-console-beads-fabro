# Fix stage — the janitor gate is red

The previous stage's `mise exec -- just check` run FAILED in this
sandbox clone. Its output is in the prior stage context above.

## Your assignment (unchanged)

{{ goal }}

## What to do

1. Read the janitor failure output and diagnose the root cause.
2. Fix it IN THIS CLONE, honoring every rule from the implement
   stage: no `--no-verify` (if a hook fails, fix the cause or end with
   the needs-human protocol below, reporting its output verbatim);
   `mise exec -- git ...` for all git writes; Red-Green-Replay for any
   product `.py` change (a test-file change means a fresh Red commit,
   never an edit under an existing Green); commit trailer
   `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`; never
   create or switch branches, never touch `.beads/` or `core.bare`.
3. Re-run `mise exec -- just check` yourself until it is green.
4. Summarize the diagnosis and the fix in your final reply.

## When the failure is not auto-resolvable (needs-human protocol)

If the failure needs a human decision, is NOT caused by this branch's
changes (e.g. an upstream red inherited from master — say so with
evidence), or you have proven you cannot legitimately fix it, do NOT go
quiet and do NOT paper over it. End your final reply with the failed
outcome and a STRUCTURED reason, as a JSON object on the last line:

    {"outcome": "failed", "failure_reason": "<what is blocked; what you tried; what decision is needed>"}

When the fix loop's budget exhausts, the graph parks the run at an
in-loop human gate; your structured reason is what the operator reads
first — make it actionable.
