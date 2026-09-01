# Doc custody — the living record

**This is the permanent home of the recurring doc-custody obligation.** It is
the "final disposition" that plan `04-mvp-unbroken-walk-and-close`
(`livespec-console-beads-fabro-9nb`) was chartered to make at arc close. The
obligation is tracked as a standing, never-closing work-item that points here;
the audit log at the bottom is a living artifact that each audit pass appends
to.

> **Do not delete this file, and do not close the standing work-item on a
> single clean pass.** An audit is a snapshot, not a fix. If this record is
> lost, the obligation is lost — which is the exact outcome every archival in
> the custody chain was conditioned on avoiding.

## Provenance — the custody chain

The obligation has been preserved, with an accepting owner at each hop, across
five plan threads:

- `cockpit-ux-docs-release` wrote the `docs/` tree; archived 2026-07-21 →
  inherited by `console-happy-path-mvp`.
- `console-happy-path-mvp` held it; archived 2026-08-03 → "rides plan 02, final
  home decided by plan 04".
- plan 02 `02-menu-shell-primacy` (`livespec-console-beads-fabro-et3`) accepted
  it 2026-08-03 as active recurring work and stated "whatever survives
  generation is what 04 disposes of"; archived 2026-08-21.
- plan 04 `04-mvp-unbroken-walk-and-close` (`livespec-console-beads-fabro-9nb`)
  is the named terminus. **Its final disposition is this file plus the standing
  work-item that points at it**, established at arc close (2026-08-29), verified
  by an independent completeness reviewer.

R7 (`livespec-console-beads-fabro-jfqtpu`, PR #771) discharged the part that
could be mechanized: it made the key/action reference and the help modal
**generated from the registry** and re-pointed the `docs_*_lockstep` gates. That
shrank custody but did not end it. This record scopes **only what survives
generation**.

## What it is

**Periodically re-audit the hand-authored `docs/` operator prose against
source.** Not a one-time cleanup. Several sessions commit concurrently, so prose
rots fast — measured, historically, wrong within a day of landing more than
once. An audit is a snapshot, not a fix.

### Custody inventory — SEVEN hand-authored files

`docs/reference/key-action-reference.md` is **registry-generated (R7)** and is
NOT in scope here — its correctness is mechanized. The recurring audit covers
the seven files no generator owns:

- `docs/detailed-usage.md`
- `docs/lifecycle-walkthrough.md`
- `docs/installing.md`
- `docs/overview-quickstart.md`
- `docs/cli-options.md`
- `docs/factory-confirmations.md`
- `docs/README.md`

## Gates already in CI — do not re-derive them

These pin the **structure** of claims (a hint, key binding, asset name, release
version, or detail line moving out from under the prose):
`docs_status_hint_lockstep`, `docs_enter_key_lockstep`,
`docs_release_asset_lockstep`, `docs_release_version_lockstep`, and two tmux
scenes pinning the Detail-pane `Valve:` split. **They do NOT verify that prose
describing a named behavior is correct**, and there are recorded cases of every
gate staying green while the description rotted.

## The three mechanisms no gate can catch — even in principle

1. **Rot.** Prose that was true and became false. The lockstep gates catch only
   quoted hints/keys/assets/versions moving, not a behavioral sentence going
   stale.
2. **ABSENT prose for a shipped verb** (the `-2ckgiy` / `-cxu4eu` class). A new
   operator surface ships with zero prose. The completeness arm only requires
   reachable *contexts* to have *rows*; nothing requires prose describing the
   surface a key OPENS, or a modal's body content. An auditor looking for rot
   will not find it, because nothing rotted — it was never written.
3. **Released-artifact doc drift.** A claim scoped to a RELEASED artifact has a
   second lifetime independent of master: the doc can accurately describe an old
   release while master moves on, with nothing in the repo inconsistent.
   `docs_release_version_lockstep` forces a re-read on every release.

## Practical rules

- **After any slice that adds an operator key**, diff the LIVE Status band
  against the documented row for that context, **by hand** — nothing mechanical
  will catch a new key added to an existing hint arm.
- **A doc sentence describing behavior a filed work-item would change should
  name that work-item**, so the fix makes the prose self-announcing.
- **Diff tables as SETS, not by grepping for tokens** — an absence never
  announces itself in a grep for the wrong token (recorded twice: an auditor
  reporting `done` missing from the move-status table because a grep enumerated
  six lanes, not seven).
- **A pathspec that silently matches less than you meant produces a CLEAN audit
  over the wrong range** — worse than a noisy one. Confirm the range's file
  count against what you know landed.

## What a fresh audit can SKIP

Checked clean historically, unless their area changes: every Status-line hint,
the `s` move-to-status transition table, the header degrade ladder, global key
inertness under overlays, the 8-section Help modal, the attention row format,
the whole-record modal claim, and every TUI claim in `overview-quickstart.md`
and `cli-options.md`. The skip-list survives on "area unchanged"; when a slice
touches an area, pull that entry back and re-verify at source.

## Known-silent, deliberately left

- The record modal's terse internal footer prints `up/down scroll | esc to
  close` while `PgUp`/`PgDn` also page it. Paging IS on screen via the Status
  band hint (`PgUp/PgDn page`); only the modal's own footer omits it. Cosmetic
  terseness inside the source, not doc drift — a small TUI-text fix or a
  work-item, not a docs pass.
- `docs/` never mentions OSC 52, and that is CORRECT — the shipped binary never
  sends it. Documenting it would document a dead path; the defect is a separate
  item about the overlay's own claim.

## Audit log

One dated line per pass, so the next auditor sees what was last verified against
source and can skip it unless its area moved. **Append new passes here.**

- **2026-07-21 (archival session).** Full pass over all five operator docs then
  in scope against master (`ab6e567`) — `detailed-usage.md`,
  `lifecycle-walkthrough.md`, `cli-options.md`, `overview-quickstart.md`,
  `installing.md`. **Clean — no drift found.** Sampled at source (focus ring,
  `HEADER_SCROLL_STEP`, six Views + seven-lane order, auto-disposition strings,
  six dispatcher settings, eleven `LIVESPEC_CONSOLE_*` env vars, `events tail`
  limit, poll cadences, reject-warns-dangerous, palette→`drain`), not skimmed.
- **2026-07-26 (delta pass, `ab6e567..ac61669`).** Only internal command-spine
  commits in range (`940647b`, `2665cad`); no operator doc claim moved. **Clean
  by delta**, NOT a full re-verify. Scope note: `docs/factory-confirmations.md`
  appeared 2026-07-24 (PR #408) → custody now covers six files.
- **2026-07-29 (delta pass, `ac61669..a5af510`). FINDING — four reachable
  Status-line states undocumented** (Attention `Blocked: needs-human`; Lanes
  `ready`/`active`/`blocked` footers), two on the happy path; no gate can see a
  completeness gap of this shape. Live-confirmed at the cockpit. **Scope
  correction: custody covers SEVEN files** — `docs/README.md` had never been
  counted and asserts a checkable restatement of the locked core contract.
- **2026-07-30 local (delta pass, `a5af510..5e91d0e`). FINDING —
  `lifecycle-walkthrough.md` Steps 5–7 unreachable under this repo's
  `acceptance_mode: ai-only`; the cited E2E scripts the transition against a
  hermetic fixture and structurally cannot catch it. Filed `-6zqv2w`.** The
  2026-07-29 completeness finding CLOSED by merged `514a326` (all four rows
  added; gate upgraded to a hard ten-context completeness list; both arms
  mutation-proved RED). `s` move-status table re-verified (came off skip-list).
  Custody inventory confirmed at SEVEN.
- **2026-07-30 (SPOT CHECK ONLY — not a delta pass).** One row/one context: the
  `Acceptance review` Status band matched `detailed-usage.md:268` byte for byte;
  blocked row correctly showed no `c`. Note: `lifecycle-walkthrough.md` gained a
  "Human accept precondition" section + Step renumbering (PR #530) — **treat the
  whole walkthrough as UNVERIFIED prose** until a real pass runs over it.
- **2026-08-02 (delta pass, `5e91d0e..69ea9d4`). FINDING — `-2ckgiy` partially
  fixed, title now wrong; else clean.** Walkthrough flag DISCHARGED (new
  precondition accurate vs `.livespec.jsonc`; Steps 1–9 sequential; both
  back-references resolve). Help overlay, move-status table, palette re-verified
  at source. `h` now hinted (3 hits) but **no doc says what pressing `h`
  DOES** — the surface-a-key-opens gap survives its own partial fix. Three
  skip-list entries pulled back and re-verified clean (8-section Help modal;
  `h`-inert-on-Attention; global key inertness).
- **2026-08-03 (delta pass, `69ea9d4..67c58d4`). FINDING — the third mechanism
  fired on the auditing session's OWN slice, one day after it was named; FIXED
  in the same pass.** Slice C (`67c58d4`) shipped a record-modal `Last action:
  … refused — …` render with zero prose; added `### When an action is refused`
  carrying all four `display_line` shapes, each verified at `lib.rs`. Slice A's
  hotkey-less verb clean (no hint to drift). The 2026-08-02 palette forward-flag
  discharged (`actions` added, verified LIVE at the cockpit).

> **Custody re-homed here 2026-08-29** at plan 04's arc close. Future audit
> passes append below this line.
