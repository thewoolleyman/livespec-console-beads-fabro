# 04-mvp-unbroken-walk-and-close — charter

**Epic anchor:** `livespec-console-beads-fabro-9nb` — status is READ from the ledger.
**Blocked by:** `livespec-console-beads-fabro-et3` (02) **AND**
`livespec-console-beads-fabro-1df` (03) — LEDGER EDGES.
Opened 2026-08-02.

## Mission

The amended MVP walk: **one unbroken single-item pass, MENUS-ONLY, on the real stack** —
then close and archive the whole arc.

## THE AMENDED WALK — RULED ON AND SETTLED 2026-08-02

**The maintainer AMENDED the mission to system reality on 2026-08-02.** This is no
longer an assumption and is not to be re-litigated. The flag that stood here — "carry
this as a visible assumption, not a settled fact" — is REMOVED because the ruling it
waited for has been given.

The mission text is **AMENDED to what the system actually does** —

> groom → **ready** (Dispatcher-admitted; `admission_policy=auto` is BY DESIGN) →
> **menu-driven dispatch** → acceptance → accept

with the **approve valve proven SEPARATELY on manual-admission items** (already done:
four clean TUI admissions on 2026-07-29, plus one correctly-refused press on 2026-07-30).

**Why the amendment was made, kept because the evidence is the reason it survives.**
`plan/console-happy-path-mvp/` said "slices admitted at the approve valve". Measured
2026-07-30, on the two slices a real groom produced: `-ccycuk` landed at `ready`
outright, and `-koykn7` landed `pending-approval` carrying `admission_policy=auto`,
which `can_approve_item` refuses because it requires
`effective_admission_policy == manual`. `awaits_dispatcher_admission` names exactly that
state. So the original walk **could not be performed as written** — not because of a
bug, but because the specification described behaviour the system does not have.

The alternative the maintainer declined was a spec change making groomed slices
manual-admission. Recorded so a successor does not re-propose it as though it were
unconsidered.

## Why this handoff is thin, deliberately

This is a **CHARTER**, not a full handoff: mission, scope, milestone acceptance
(including the dogfood leg), dependencies, and the ledger items it owns. Detail is added
when this plan OPENS.

**That unevenness is the anti-yak-shave mechanism, not laziness.** Writing 40 pages of
design for a milestone two steps away is exactly the rabbitholing this numbering exists
to prevent, and it would be written against a registry that does not exist yet — so it
would be wrong as well as premature. Fill this in when you open it, from what 01
actually shipped.

## Milestone acceptance

1. **ONE UNBROKEN PASS**, single item, menus-only, real stack, one sitting. Not
   assembled from legs walked separately — that is precisely what the predecessor thread
   did three times, and it is not the deliverable.
2. Evidence captured LIVE: the offered-actions state BEFORE each invocation, the
   confirmation's exact target read back before committing, and a ledger check after.
   **Never reconstruct a capture afterwards.**
3. Any intervention needed (plugin-root override, hand-fixed PR, `reconcile-merged`) is
   NAMED in the record. A pass needing them is still worth having; it is just not a
   clean one, and the difference is the whole point of this arc.

## Also owns

- **Archive sequencing for the whole arc** (01–04 and `plan/console-happy-path-mvp/`).
- **Final disposition of DOC CUSTODY.** Registry-generated docs (02) shrink it
  substantially; what REMAINS becomes a standing item or a named successor thread.
  **Custody must NEVER be dropped silently** — that is the exact condition the last
  archived thread was conditioned on, and the reason its obligation survived at all.

## Implementation mode — RULED 2026-08-02

**Implement IN-SESSION**: worktree → PR → **full gates** → rebase-merge. That is the
default for every slice in this plan.

**Factory dispatch is the exception, not the fallback**: use it only for well-bounded,
sandbox-safe slices, and **record the choice per slice** with the reason. A slice that
touches host-coupled surfaces, plugin resolution, or anything under `.github/workflows/`
is not sandbox-safe — see the known live hazards in
`plan/01-action-registry-and-invoker/handoff.md`.

Recording the mode here rather than leaving it as session convention, because the two
routes have different evidence obligations and a successor cannot infer which was used
from the merge alone.

## Ledger

Blocked by `-et3` (02) and `-1df` (03).
