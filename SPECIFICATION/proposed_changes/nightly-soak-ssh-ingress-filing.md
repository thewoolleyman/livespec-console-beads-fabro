---
topic: nightly-soak-ssh-ingress-filing
author: claude-opus-5
created_at: 2026-09-06T00:00:00Z
---

## Proposal: Nightly soak files through the on-tailnet SSH write ingress and holds no database credential

### Target specification files

- non-functional-requirements.md

### Summary

Align the ratified nightly chore-filing clause with the on-tailnet CI beads write surface `dolt-server` has now delivered. The clause's dedup SEMANTICS are unchanged and remain correct — dedup against OPEN chores, re-filing after a chore is closed. What changes is the filing MECHANISM (an SSH forced-command ingress instead of CI reaching the work-items backend directly), the way the finding identity is carried (a bounded fingerprint in a metadata field plus an external ref), and the credential posture: CI no longer holds a database credential at all. Adds a fail-closed requirement that the nightly run on the self-hosted, on-tailnet runner pool.

### Motivation

The ratified clause (v045) requires the nightly to derive a stable finding signature, persist it on the filed work-item, and file only when no NON-CLOSED chore already carries it. That behavior is correct and is PRESERVED here.

What is no longer accurate is the clause's closing sentence, "This requires CI to hold credentialed access to the work-items backend per the Beads/Fabro Family Secret Convention below." The sanctioned path for off-host CI to write beads is now settled in `dolt-server` and is deliberately narrower:

- The Dolt database port MUST remain bound to loopback and MUST NOT be exposed for CI writes (`dolt-server` constraints.md, "Localhost-only" and "Beads CI write surface"). An earlier console proposal to expose the SQL port over the tailnet was rejected for contradicting that ratified posture.
- CI files by SSH to the host, through a forced command pinned in the `ci-writer` account's `authorized_keys`, whose request shape is exactly one line: `create <fingerprint> <base64url-title> [<base64url-body>]`. The tenant and issue prefix are baked into the `command=` by the operator and are never chosen by the caller.
- `dolt-server` requires that CI "MUST NOT carry the database password" and "MUST authenticate to the ingress with an SSH key alone"; the host runs its own pinned `bd` under a sealed `ci-writer` credential profile holding only the beads write set.
- Deriving the finding fingerprint is explicitly the consuming caller's responsibility — i.e. ours, specified here — and the ingress constrains it to `^[a-z0-9]([a-z0-9-]{0,62}[a-z0-9])?$` (a DNS-label-like slug of at most 64 characters).

The surface's dedup matches this clause: it lists OPEN chores carrying the finding's metadata field and no-ops when one exists, and it deliberately assigns NO fixed work-item id so that re-filing after a close does not collide with the closed chore. This was verified empirically on 2026-09-06 against the live ingress: filing a fingerprint created one chore; re-sending it while that chore was open filed nothing; closing the chore and re-sending the SAME fingerprint filed a NEW chore. Re-file-after-close therefore works end to end and this clause's semantics need no change.

(For the record, `dolt-server`'s own constraints.md currently describes this dedup as a deterministic `bd --id` that suppresses re-filing permanently. That text does not match its shipped dispatcher, which is what was verified above; the divergence is reported to that repo and tracked in `dolt-server-2u6jll`. This proposal follows the verified behavior, not the stale prose.)

Finally, the ingress is reachable only from the tailnet. A GitHub-hosted runner has no tailnet identity, so a nightly scheduled onto one could not file at all. That must be an explicit, loud failure rather than a silent no-op that leaves the soak looking healthy while tracking nothing.

### Proposed Changes

In non-functional-requirements.md, AMEND the nightly chore-filing clause (the paragraph beginning "Nightly chore filing is **idempotent**: ..."):

- KEEP UNCHANGED the dedup semantics: derive a stable finding identity from the inputs already ratified (for a fuzz crash, a hash of the reproducing input or the crash backtrace; for a surviving mutant, its `(source file, line, mutation-operator)` identity); file only when no NON-CLOSED chore already carries that identity; re-file once a previous chore for the finding has been closed. Keep the recorded trade-off that a pre-existing open chore MAY be stale.
- ADD that the finding identity MUST be rendered as a FINGERPRINT matching `^[a-z0-9]([a-z0-9-]{0,62}[a-z0-9])?$` (a DNS-label-like slug of at most 64 characters), which is the dedup key.
- REPLACE the filing mechanism: the nightly MUST file through the `dolt-server` on-tailnet SSH write ingress using its single `create <fingerprint> <base64url-title> [<base64url-body>]` request, rather than reaching the work-items backend itself. The ingress performs the host-side filing, persists the fingerprint on the work-item (a metadata field plus an external reference) so the existence check stays cheap, and assigns no fixed work-item id so a re-file after close cannot collide.
- REPLACE the sentence "This requires CI to hold credentialed access to the work-items backend per the Beads/Fabro Family Secret Convention below." with: CI MUST NOT hold the work-items database credential. CI authenticates to the host write ingress with an SSH key ALONE; the host runs its own pinned `bd` under a sealed `ci-writer` credential profile, and the database port remains bound to loopback. The Family Secret Convention continues to govern host-side credentials, not CI-held ones.
- ADD a fail-closed requirement: the nightly soak MUST run on the self-hosted, on-tailnet runner pool. Because the write ingress is reachable only from the tailnet, a nightly scheduled onto a runner without tailnet identity (for example a GitHub-hosted runner) MUST fail loudly rather than complete while filing nothing.

In non-functional-requirements.md, AMEND the embedded contributor scenario "A nightly finding opens a chore instead of failing master": its "through the orchestrator's capture surface" step is superseded — the chore is filed through the on-tailnet SSH write ingress, which performs the host-side filing. Keep unchanged: the canonical branch does not fail, and the chore is filed at the top of the rank order in the `livespec-console-beads-fabro` tenant.

Because the clause's linked top-of-pyramid coverage moves with the filer, `tests/heading-coverage.json` MUST be co-edited atomically in the same change so the clause -> scenario/test linkage stays green under `console-spec-check`: the entry currently naming `crates/console-nightly-soak/src/lib.rs::tests::fuzz_finding_with_no_open_chore_files_exactly_one_chore` is re-pointed at the tests covering the reworked filer.

Keep unchanged: the MUST-NOT-fail-the-canonical-branch requirement, top-of-rank-order filing, and the intake Definition-of-Ready and `admission_policy` routing (an item is never filed directly into `ready`).
