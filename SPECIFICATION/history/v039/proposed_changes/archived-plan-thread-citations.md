---
topic: archived-plan-thread-citations
author: claude-opus-5
created_at: 2026-08-02T07:58:32Z
---

## Proposal: Repoint two design-record citations whose plan threads were archived

### Target specification files

- SPECIFICATION/contracts.md

### Summary

Two `design record:` citations in the TUI contract name plan-thread paths that have since
moved under `plan/archive/`. Repoint both. No normative text changes — the MUST/MUST NOT
clauses, their scope, and their meaning are untouched. This is a citation-accuracy fix.

### Motivation

A design-record citation is load-bearing in this spec: it is how a reader checks WHY a
clause says what it says. A citation that resolves to nothing degrades silently — nothing
in CI resolves plan-thread paths, so it rots without announcing itself.

Both instances share ONE mechanism, which is worth naming because it will recur:
**a plan thread is archived and the spec citation pointing into it is not updated.**
Archival is exactly when this happens, and it is exactly when nobody is looking at the
spec.

### The two citations

**1. `plan/operator-surface-redesign/research/l4p3ce-handoff-transport.md` — LOCAL, and
this one is newly broken.**

In the driver-handoff verb clause. `plan/operator-surface-redesign/` was absorbed and
archived on 2026-08-02 (maintainer decision; PR #576), moving to
`plan/archive/operator-surface-redesign/`. The research file travelled with it and is
present at the new path.

    plan/operator-surface-redesign/research/l4p3ce-handoff-transport.md
 -> plan/archive/operator-surface-redesign/research/l4p3ce-handoff-transport.md

**2. `plan/needs-attention/research/design.md` — CROSS-REPO, and pre-existing.**

In the attention-port clause, explicitly qualified `repo thewoolleyman/livespec`. This is
NOT a path in this repository, and an earlier note in
`plan/01-action-registry-and-invoker/research/operator-surface-redesign-decision.md`
wrongly called it a local dangling reference — that claim is corrected in the same PR
that files this proposal.

Verified against the livespec marketplace checkout at
`/home/ubuntu/.claude/plugins/marketplaces/livespec`: the thread was archived UPSTREAM
and the file now sits at `plan/archive/needs-attention/research/design.md`. So the
citation is stale by the same mechanism, one repo over.

    repo thewoolleyman/livespec, plan/needs-attention/research/design.md
 -> repo thewoolleyman/livespec, plan/archive/needs-attention/research/design.md

Because it is cross-repo, this repo cannot keep it honest mechanically, and the
correction is a point-in-time fix that can rot again if upstream moves the file. Stated
so the limitation is inherited knowingly rather than discovered later.

### Why this is filed rather than edited

A direct one-line edit was attempted on 2026-08-02 and the pre-push gate refused it,
twice over — recorded because it establishes that this cannot be a drive-by fix:

- `doctor-out-of-band-edits`: `out-of-band edits detected at HEAD against history/v037:
  contracts.md`.
- `check-behavior-coverage`: `clause not linked to a scenario [gap-vvl5pllp]` — clauses
  are content-linked to their scenarios, so changing the text broke the link.

The doctor additionally auto-materialized a synthetic `SPECIFICATION/history/v038/`
snapshot of the edit. Both the edit and that snapshot were reverted.

Note for whoever runs the revise: because clause text is content-linked, **the scenario
link for the driver-handoff clause will need re-establishing** as part of accepting this,
which is precisely the work the propose-change → revise flow exists to do properly.

### Proposed change

In `SPECIFICATION/contracts.md`, in the TUI Contract section, replace the two citation
paths above with their `plan/archive/` forms. Change nothing else in either clause.

### Out of scope

- Any normative change to the driver-handoff verb or the attention-port clause.
- Any mechanical gate that would resolve plan-thread citation paths. That would be a real
  improvement and would have caught both of these, but it is a separate proposal with its
  own cost, and the cross-repo case cannot be fully covered by it.
