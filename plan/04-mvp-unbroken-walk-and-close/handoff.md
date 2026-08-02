# 04-mvp-unbroken-walk-and-close — charter

**Epic anchor:** `livespec-console-beads-fabro-9nb` — status is READ from the ledger.
**Blocked by:** `livespec-console-beads-fabro-et3` (02) **AND**
`livespec-console-beads-fabro-1df` (03) — LEDGER EDGES.
Opened 2026-08-02.

## Mission

The amended MVP walk: **one unbroken single-item pass, MENUS-ONLY, on the real stack** —
then close and archive the whole arc.

## THE AMENDED WALK — CARRY THIS AS A VISIBLE ASSUMPTION, NOT A SETTLED FACT

**The maintainer has NOT explicitly ruled on this. It is an assumption pending their
confirmation at review, and it must stay flagged until they confirm it.**

The assumption: the mission text is **AMENDED to what the system actually does** —

> groom → **ready** (Dispatcher-admitted; `admission_policy=auto` is BY DESIGN) →
> dispatch → acceptance → accept

with the **approve valve proven SEPARATELY on manual-admission items** (already done:
four clean TUI admissions on 2026-07-29, plus one correctly-refused press on 2026-07-30).

**Why the amendment is proposed.** `plan/console-happy-path-mvp/` said "slices admitted
at the approve valve". Measured 2026-07-30, on the two slices a real groom produced:
`-ccycuk` landed at `ready` outright, and `-koykn7` landed `pending-approval` carrying
`admission_policy=auto`, which `can_approve_item` refuses because it requires
`effective_admission_policy == manual`. `awaits_dispatcher_admission` names exactly that
state. So the original walk **cannot be performed as written** — not because of a bug,
but because the specification described behaviour the system does not have.

**If the maintainer rejects the amendment, this plan's mission changes** and the
alternative is a spec change making groomed slices manual-admission. Do not quietly
proceed on the assumption; surface it at review.

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

## Ledger

Blocked by `-et3` (02) and `-1df` (03).
