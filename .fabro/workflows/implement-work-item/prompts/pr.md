# PR stage — publish the work and arm rebase auto-merge

The janitor gate is green. Publish this sandbox clone's committed work
as a PR per the family merge discipline. You are on a Fabro-managed run
branch — its name is run-internal and MUST NOT be published; the PR
rides a feature branch named after the work-item instead.

## Where you are

You are in the SAME isolated Fabro sandbox clone the implement/janitor
stages produced the committed work in — your CURRENT WORKING DIRECTORY
is that clone. Run every `git` and `gh` command here, in the current
directory. The assignment below may mention a `Repo:` path: that is the
dispatcher's host-side checkout, it does NOT exist in this sandbox, and
you must NEVER `cd` to it or treat the absence of any such path as
"no committed work". The committed work is reachable as
`git log --oneline origin/master..HEAD` from where you already are.

## Your assignment (for the PR description)

{{ goal }}

## What to do, in order

1. Confirm there is committed work: `git log --oneline
   origin/master..HEAD`. If there are zero commits, STOP — reply
   explaining that nothing was produced, and end your reply with
   `{"preferred_next_label": "done"}`.
2. Refresh the base IMMEDIATELY before publishing: run
   `mise exec -- git fetch origin master --quiet`, then run
   `mise exec -- git rebase origin/master`. If the rebase reports
   conflicts you cannot legitimately resolve, report the rebase output
   verbatim and end with the needs-human protocol below. After a
   successful rebase, re-check committed work with
   `git log --oneline origin/master..HEAD`; if there are zero commits,
   STOP as in step 1.
3. Publish under the feature branch named in your assignment (the
   "Publish branch" line — `feat/<work-item-id>`), NEVER under the
   current run branch's own name:
   `mise exec -- git push -u origin HEAD:refs/heads/feat/<work-item-id>`.
   NEVER `--no-verify`; if the pre-push hook fails and you cannot
   legitimately fix the cause, report its output verbatim and end with
   the needs-human protocol below.
   - If the remote rejects the push with the exact signature
     `refusing to allow a GitHub App to create or update workflow .github/workflows/ci.yml without workflows permission`
     (the workflow path is named here only as the quoted rejection
     signature; this stage must not edit files under `.github/workflows/`),
     retry EXACTLY ONCE: run
     `mise exec -- git fetch origin master --quiet`, then
     `mise exec -- git rebase origin/master`, then repeat the same
     `mise exec -- git push -u origin HEAD:refs/heads/feat/<work-item-id>`
     command. If that retry gets the same rejection, or if any other
     push failure occurs, report the output verbatim and end with the
     needs-human protocol below. Do NOT loop and do NOT retry on any
     different error signature.
4. Open the PR against master with
   `gh pr create --head feat/<work-item-id>` — title from the
   work-item, body drafted from the work-item acceptance criteria in
   the assignment above, and including the work-item id.
   The body MUST end with the line:

   🤖 Generated with [Claude Code](https://claude.com/claude-code)

5. Arm auto-merge: `gh pr merge --rebase --auto --delete-branch <pr-url-or-number>`.
6. VERIFY it armed: `gh pr view --json number,autoMergeRequest,mergeStateStatus`.
   - If `autoMergeRequest` is null, retry the arming once and re-verify.
   - If `mergeStateStatus` is `BEHIND`, the repo automation updates the
     branch; if it stays `BEHIND` for more than 10 minutes, report it —
     do NOT attempt a manual update.
7. Do NOT wait for the merge (it lands server-side after CI), do NOT
   clean anything up, and do NOT switch branches — the Dispatcher owns
   merge confirmation and the post-merge janitor, and Fabro owns this
   sandbox's lifecycle.
8. Final reply: report the PR number on its own line in exactly this
   form — `PR_NUMBER=<n>` — plus whether auto-merge is armed, and any
   deviation verbatim.

## When publishing is blocked (needs-human protocol)

If the push or PR flow is blocked in a way you cannot legitimately
resolve (hook rejection you must not bypass, gh/auth failure, branch
protection surprise), end your final reply with the failed outcome and
a STRUCTURED reason, as a JSON object on the last line:

    {"outcome": "failed", "failure_reason": "<what is blocked; what you tried; what decision is needed>"}

The graph routes a failed outcome to an in-loop human gate where an
operator answers and routes the run back into the loop.
