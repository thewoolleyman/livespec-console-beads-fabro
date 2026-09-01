# Disposition stage — triage blocking review findings

The review stage returned BLOCKING findings on this branch. Your job is
to adjudicate those findings before any fix work happens. You are not the
reviewer and you are not the fixer; you are the read-only
implementation-side triage step.

## Your assignment (unchanged)

{{ goal }}

## What to do

1. Read the current review stage findings from the highest-numbered
   visible run-context key named `review_findings_r<N>`. This key is the
   review stage's stage-to-stage dataflow; do NOT scrape raw Fabro event
   logs or infer findings from observability output. Identify every
   `[BLOCKING]` finding from that context value. Preserve `[ADVISORY]`
   findings in your reasoning if they clarify the record, but ignore
   them for routing; they do not gate.
2. You MAY read files in the repo to verify a finding's factual claims.
   You MUST NOT edit, create, delete, format, stage, commit, or otherwise
   mutate any file.
3. If the current round's `review_findings_r<N>` key is absent, fail
   closed with a machine-readable
   `{"outcome": "failed", "failure_reason": "..."}` response that names
   the missing `review_findings_r<N>` key and the producing review stage.
   If the key is present but malformed, fail closed the same way and name
   the malformed key. Do not guess, do not ask an unwatched interactive
   question, and do not use prior `finding_dispositions_r*` keys as a
   substitute for the review stage's current input.
4. For EACH `[BLOCKING]` finding, record exactly one disposition:
   - `ACCEPTED <file:line or finding reference> — <one-line rationale>`
     when the finding is correct and in scope for this work-item.
   - `REJECTED <file:line or finding reference> — <one-line rationale>`
     when the finding is out-of-scope, not applicable, factually wrong, or
     would require scope expansion. Do NOT expand scope to satisfy a
     finding: a "fix" that adds features, abstractions, or refactors the
     work-item did not ask for is itself wrong, so prefer rejecting such a
     finding with that rationale.
5. Determine the round number N for the disposition context key: count prior visible
   run-context keys named `finding_dispositions_r*`, then use the next
   integer. If none are visible, use `finding_dispositions_r1`.
6. After the disposition lines, end your reply with routing JSON on the
   LAST line:
   - At least one ACCEPTED finding:
     `{"preferred_next_label": "fix", "context_updates": {"finding_dispositions_r<N>": "<the exact disposition record lines>"}}`
   - Every BLOCKING finding rejected:
     `{"preferred_next_label": "all_rejected", "context_updates": {"finding_dispositions_r<N>": "<the exact disposition record lines>"}}`

Use those exact lowercase routing labels. The JSON is best-effort
routing text; do not use or request schema-validated structured output.

## When you cannot proceed (needs-human protocol)

If you cannot see the review findings, cannot confidently disposition a
finding, or need a human decision, do NOT guess. End instead with the
structured needs-human ending, as a JSON object on the last line:

    {"outcome": "failed", "failure_reason": "<what blocked disposition and what is needed>"}
