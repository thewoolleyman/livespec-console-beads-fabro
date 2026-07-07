# Review-fix stage — the reviewer flagged blocking issues

The review stage (a senior-engineer code review) returned BLOCKING
findings on this branch's change. Its findings are in the prior stage
context above, each tagged `[BLOCKING]` or `[ADVISORY]`.

## Your assignment (unchanged)

{{ goal }}

## What to do

1. Read the reviewer's `[BLOCKING]` findings. Ignore `[ADVISORY]` ones
   unless a fix is trivial AND clearly in scope — advisories do not gate.
2. For EACH `[BLOCKING]` finding, do exactly one of:
   - **Fix it** in this clone, strictly within the work-item's scope; OR
   - **Reject it** with a one-line rationale when it is out-of-scope,
     not-applicable, or wrong. You are the implementer — you may decline a
     finding you judge incorrect or beyond this work-item. State the
     rationale plainly; the next review pass is told to HONOR it unless it
     is a genuine correctness/security defect.
   Do NOT expand scope to satisfy a finding: a "fix" that adds features,
   abstractions, or refactors the work-item did not ask for is itself
   wrong — prefer REJECTING such a finding with that rationale.
3. Honor every rule from the implement stage: no `--no-verify` (if a hook
   fails, fix the cause or end with the needs-human protocol below,
   reporting its output verbatim); `mise exec -- git ...` for all git
   writes; Red-Green-Replay for any product `.py` change (a test-file
   change means a fresh Red commit, never an edit under an existing
   Green); commit trailer `Co-Authored-By: Claude Fable 5
   <noreply@anthropic.com>`; never create or switch branches, never touch
   `.beads/` or `core.bare`.
4. Re-run `mise exec -- just check` yourself until it is green — the
   janitor re-validates this clone after this stage.
5. In your final reply, summarize EACH blocking finding as either
   `FIXED — <what you changed>` or `REJECTED — <one-line rationale>`.

## When you cannot proceed (needs-human protocol)

If a finding needs a human decision, is NOT caused by this branch's
changes, or you have proven you cannot legitimately resolve it, do NOT go
quiet and do NOT paper over it. End your final reply with the failed
outcome and a STRUCTURED reason, as a JSON object on the last line:

    {"outcome": "failed", "failure_reason": "<what is blocked; what you tried; what decision is needed>"}

When the loop's budget exhausts, the graph parks the run at an in-loop
human gate; your structured reason is what the operator reads first —
make it actionable.
