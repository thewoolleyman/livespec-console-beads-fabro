---
topic: nightly-soak-deterministic-id-dedup
author: claude-opus-5
created_at: 2026-09-06T00:00:00Z
---

## Proposal: Nightly soak files through the on-tailnet SSH write ingress with deterministic-id dedup, and never holds a database credential

### Target specification files

- non-functional-requirements.md

### Summary

Reconcile the ratified nightly chore-filing clause with the on-tailnet CI beads write surface that `dolt-server` has now delivered (its SPECIFICATION v017-v020). Three mechanism changes: (1) dedup moves from a console-side "is there an OPEN chore carrying this signature" query to the surface's DETERMINISTIC issue id, which suppresses re-filing permanently; (2) CI stops holding the work-items database credential and authenticates to the host ingress with an SSH key alone; (3) the nightly job MUST run on the self-hosted on-tailnet runner pool and MUST fail closed elsewhere. The clause's ratified OBLIGATION — that each distinct finding is tracked at least once — is unchanged and still met.

### Motivation

The ratified clause (v045) requires the nightly to derive a stable finding signature, persist it on the filed work-item, and file only when no NON-CLOSED chore already carries it — which implies re-filing after a chore is closed. It also states this "requires CI to hold credentialed access to the work-items backend per the Beads/Fabro Family Secret Convention".

Both mechanisms are now unimplementable as written, because the sanctioned path for off-host CI to write beads has been decided in `dolt-server` and is deliberately narrower:

- The Dolt database port MUST remain bound to loopback and MUST NOT be exposed for CI writes (`dolt-server` constraints.md "Localhost-only" and "Beads CI write surface"). An earlier console proposal to expose the SQL port over the tailnet was rejected for contradicting that ratified posture.
- The delivered ingress is a single-verb SSH forced command: `create <fingerprint> <base64url-title> [<base64url-body>]`. There is NO `list`/query verb, so the console can no longer ask whether an open chore carries a signature.
- Dedup is by a deterministic issue id `${PREFIX}-ci-${fingerprint}`; if that id already exists the host files nothing and exits 0. A CLOSED id still resolves, so re-filing the same fingerprint is suppressed permanently.
- `dolt-server` requires that "CI MUST NOT carry the database password" and "MUST authenticate to the ingress with an SSH key alone"; the host-pinned `bd` runs under a sealed `ci-writer` wrapper profile. Deriving the fingerprint is explicitly the consuming caller's responsibility — i.e. ours, specified here.

The cross-repo reconciliation is tracked as `dolt-server-2u6jll`. Adopting the surface as delivered is the correct resolution rather than growing it, for three reasons. First, our ratified obligation is already stated as "each distinct finding is tracked at least once, not that every open chore stays current" — the deterministic id satisfies that; re-file-after-close was an emergent consequence of the "open (non-closed)" mechanism wording, never the stated requirement. Second, adding a query verb would widen a CI-reachable surface into ledger reads to restore a guarantee the specification never made. Third, the change strictly REDUCES CI's authority: no database credential in CI at all.

The accepted trade-off is recorded honestly: once a finding's chore has been closed, an identical recurring fingerprint is not re-filed, so that specific recurrence is not re-surfaced automatically. This is bounded in practice — a surviving mutant's identity includes its source line, so a genuine later regression usually yields a DIFFERENT fingerprint and does file — and the already-ratified fix-verification path (the soak is expensive; a fix re-runs the soak manually to surface the current finding set) is unchanged.

Finally, the ingress is reachable only from the tailnet. A GitHub-hosted runner has no tailnet identity, so a nightly scheduled onto one could not file at all. That must be an explicit, loud failure rather than a silent no-op that makes the soak look healthy while tracking nothing.

### Proposed Changes

In non-functional-requirements.md, AMEND the nightly chore-filing clause (the paragraph beginning "Nightly chore filing is **idempotent**: ..."):

- KEEP the requirement to derive a stable finding identity from the same inputs already ratified: for a fuzz crash, a hash of the reproducing input (or the crash backtrace); for a surviving mutant, its `(source file, line, mutation-operator)` identity.
- ADD that this identity MUST be rendered as a FINGERPRINT conforming to the write ingress's grammar — matching `^[a-z0-9]([a-z0-9-]{0,62}[a-z0-9])?$` (a DNS-label-like slug of at most 64 characters) — and that the fingerprint is the dedup key.
- REPLACE the "MUST NOT file a chore when an open (non-closed) chore already carries that signature" mechanism with: the nightly MUST file through the `dolt-server` on-tailnet SSH write ingress using its single `create <fingerprint> <base64url-title> [<base64url-body>]` request, and the ingress files with a DETERMINISTIC work-item id derived from the fingerprint, filing nothing when an item with that id already exists.
- ADD the explicit superseding trade-off: because a closed work-item id still resolves, dedup is PERMANENT — a finding whose chore has since been CLOSED is NOT re-filed. This supersedes the previous re-file-after-close behavior. It satisfies the clause's unchanged obligation that each distinct finding is tracked AT LEAST ONCE; it does not guarantee that a recurrence after closure is re-surfaced.
- REPLACE the sentence "This requires CI to hold credentialed access to the work-items backend per the Beads/Fabro Family Secret Convention below." with: CI MUST NOT hold the work-items database credential. CI authenticates to the host write ingress with an SSH key ALONE; the host runs its own pinned `bd` under a sealed `ci-writer` credential profile, and the database port remains bound to loopback. The Family Secret Convention continues to govern host-side credentials, not CI-held ones.
- ADD a fail-closed requirement: the nightly soak MUST run on the self-hosted, on-tailnet runner pool. Because the write ingress is reachable only from the tailnet, a nightly scheduled onto a runner without tailnet identity (for example a GitHub-hosted runner) MUST fail loudly rather than complete while filing nothing.

In non-functional-requirements.md, AMEND the embedded contributor scenario "A nightly finding opens a chore instead of failing master": its "through the orchestrator's capture surface" step is superseded — the chore is filed through the on-tailnet SSH write ingress, which performs the host-side `bd` filing. Keep unchanged: the canonical branch does not fail, and the chore is filed at the top of the rank order in the `livespec-console-beads-fabro` tenant.

ADD a companion scenario covering the new load-bearing MUST NOT, for example: "Given a prior nightly filed a chore for a finding, When a later soak re-discovers the same finding and re-sends its fingerprint, Then no new work-item is created — whether the existing one is open or closed — and the run still succeeds", together with the converse that a finding whose fingerprint has never been filed DOES create exactly one work-item.

Because the clause's linked top-of-pyramid coverage changes with the filer, `tests/heading-coverage.json` MUST be co-edited atomically in the same change so the clause -> scenario/test linkage stays green under `console-spec-check`: entry currently naming `crates/console-nightly-soak/src/lib.rs::tests::fuzz_finding_with_no_open_chore_files_exactly_one_chore` is re-pointed at the deterministic-fingerprint tests that replace it.

Keep unchanged: the MUST-NOT-fail-the-canonical-branch requirement, top-of-rank-order filing, the intake Definition-of-Ready and `admission_policy` routing (an item is never filed directly into `ready`), and the accepted staleness of a pre-existing chore.
