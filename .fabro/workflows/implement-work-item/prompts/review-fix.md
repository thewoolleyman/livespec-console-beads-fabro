# Review-fix stage — implement accepted review findings

The disposition stage adjudicated the review stage's BLOCKING findings.
Your job is to implement ONLY the findings that the disposition record
marks ACCEPTED. Rejected findings are out of bounds. You have no
adjudication authority in this stage.

## Your assignment (unchanged)

{{ goal }}

## What to do

1. Read the disposition record from the prior stage context, including any
   visible `finding_dispositions_r<N>` run-context keys and the prior stage
   transcript. Identify every finding marked ACCEPTED.
2. Fix each ACCEPTED finding in this clone, strictly within the work-item
   scope. Do NOT implement rejected findings, advisories, unrelated cleanup,
   new features, broad refactors, or speculative abstractions.
3. If an ACCEPTED finding cannot be implemented honestly within scope, that
   is a needs-human `failed` outcome. Do not silently skip it and do not
   reclassify it as rejected here.
4. Honor every rule from the implement stage: no `--no-verify` (if a hook
   fails, fix the cause or end with the needs-human protocol below,
   reporting its output verbatim); plain `git` for all git writes (the
   installed hooks fire on their own); Red-Green-Replay for any product `.py` change (a test-file
   change means a fresh Red commit, never an edit under an existing
   Green); commit trailer `Co-Authored-By: Claude Fable 5
   <noreply@anthropic.com>`; never create or switch branches, never touch
   `.beads/` or `core.bare`.
5. Re-run the repository's check suite (`{{ inputs.sandbox_check_suite }}`) yourself until it is green — the
   janitor re-validates this clone after this stage.
6. In your final reply, summarize each ACCEPTED finding as
   `FIXED — <what you changed>`. Mention rejected findings only to say
   they were intentionally left untouched per the disposition record.

## HONEST checks — no detector evasion (non-negotiable)

Make every check pass HONESTLY — by satisfying the condition it exists
to enforce, never by hiding the violation from its detector. A green
tree obtained by evasion is a FAILED outcome, not a success. This stage
is where such dodges have been authored under pressure from a
`[BLOCKING]` finding; resolving a finding by evasion is itself a
failure. Specifically FORBIDDEN:

- Forking or repointing a shared, externally-owned check (the fleet's
  dev-tooling Verifiers the prepare chain installed) to a weaker local copy;
  editing `dev-tooling/checks/**` or changing a `check-*` justfile recipe to
  invoke anything other than the pinned shared check module.
- Rewriting a banned call into a form the matcher doesn't recognize but
  that does the same thing (e.g. `sys.stdout.write`/`sys.stderr.write`
  -> `.buffer.write`).
- Constructing a class dynamically (`type(name, (Base,), ...)`) or
  otherwise restructuring code to hide a disallowed inheritance/pattern
  from an AST check.
- Silencing a check with `: Any`, `# type: ignore`, `# noqa`, symbol
  renames, or getattr/indirection instead of satisfying it.

If a check genuinely conflicts with a LEGITIMATE, required pattern —
i.e. the check over-applies (a domain exception that must subclass a
stdlib error; a launcher that can't carry `__all__`; a module correctly
invoked via `python -m`) — that is NOT yours to resolve by evasion or by
weakening the check. SURFACE it via the needs-human protocol: end with
`{"outcome":"failed","failure_reason":"check <name> over-applies to <legitimate pattern> at <file>; needs an upstream gate decision"}` so a
maintainer fixes the check upstream. Reserve this for a genuine
check-vs-legitimate-pattern conflict, not a check you simply find
inconvenient to satisfy.

## When you cannot proceed (needs-human protocol)

If an accepted finding needs a human decision, is NOT caused by this
branch's changes, or you have proven you cannot legitimately resolve it,
do NOT go quiet and do NOT paper over it. End your final reply with the
failed outcome and a STRUCTURED reason, as a JSON object on the last line:

    {"outcome": "failed", "failure_reason": "<what is blocked; what you tried; what decision is needed>"}

When the loop's budget exhausts, the graph terminates the run at the
`needs_human` node and rests the work-item at `blocked / needs-human` in
the ledger; your structured reason is what the human reads first — make
it actionable.
