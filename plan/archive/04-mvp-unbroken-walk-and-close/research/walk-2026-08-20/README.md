# Milestone 1 walk, 2026-08-20 — ATTEMPTED, REFUSED

The charter's milestone acceptance 3 says a pass needing intervention is still
worth having and is simply not a clean one. This is a stronger outcome than that:
the walk could not be started, for a reason that is a property of the product
rather than of the attempt.

Everything here was captured LIVE, in the order shown, against the real binary
built from master and run under the credential wrapper against this repo's LIVE
tenant. Nothing was reconstructed afterwards, per the charter's standing rule.
The captures are `tmux capture-pane -p` — plain text, no `-e`, the same form the
e2e harness uses and the form the availability markers were made textual to
survive.

Session: 2026-08-20T10:28:46Z to 10:31:23Z. Pane 200x50.

## The release condition: MET

`00-resting-bar-visible.txt` is the resting frame, captured after launch with NO
keystroke sent. Line 4:

```
Menu:  Work item   View   Help   File
```

That is this plan's release condition discharged as stated — the bar renders as a
permanently-visible region without being summoned, and it survives a plain-text
capture.

## The walk: REFUSED, because dispatch has no menu representation

`07-bar-node-1/2/3.txt` capture the other three bar nodes. The complete menu
vocabulary is:

| node | contents |
| --- | --- |
| Work item | Driver handoff, Move status, Approve, Accept, Reject, Set admission, Set override x3, Set acceptance, Set workflow scope override |
| View | Search, Command palette, Menu bar |
| Help | Help |
| File | Quit |

**None of them dispatches.** Confirmed in source: `ACTION_REGISTRY` holds exactly
sixteen ids — `driver-handoff`, `move`, `approve`, `accept`, `reject`,
`set-admission`, `set-merge-on-review-cap`, `set-review-fix-cap`,
`set-acceptance`, `set-acceptance-rework-cap`, `set-workflow-scope-override`,
`open-search`, `open-command-palette`, `open-menu`, `open-help`, `quit`. Drain is
not among them; it exists only as a command-palette query string
(`crates/console-application/src/lib.rs:4318`,
`normalized == "drain" || normalized == "drain ready queue"`).

The amended mission is `groom -> ready -> MENU-DRIVEN DISPATCH -> acceptance ->
accept`. The dispatch leg has no menu representation, so a menus-only unbroken
pass is not performable.

**`:drain` was deliberately NOT used.** Reaching for the palette would have
assembled the walk from a non-menu leg, which is what the charter's "not
assembled from legs walked separately" clause and deferral D1 exist to forbid.
The walk stopped instead. Nothing was mutated: no policy set, no dispatch, no
ledger write; the console was quit cleanly.

## What DID work: milestone 2's leg A, proven on the real surface

`06-legA-offered.txt`, with `livespec-console-beads-fabro-zfcp` (a `ready` item)
selected:

```
>   Driver handoff [h]  (unavailable here)
    Move status [s]
    Approve work-item [p]  (unavailable here)
    Accept work-item [c]  (unavailable here)
    Reject work-item [r]  (unavailable here)
    Set admission [m]  (unavailable here)
    Set override [g]
    Set override [f]
    Set acceptance [n]
    Set override [k]
    Set workflow scope override [menu]  (unavailable here)
```

This is the design in `../milestone-2-evidence-design.md` working end to end: the
markers are textual, they survive a plain capture, and the offered set is
derivable as the unmarked subset without any out-of-band query. The
capture-visibility constraint recorded in that note's §5 was implemented
correctly.

## Two further findings, both measured

**The permanent bar is INERT when closed.** `05-bar-focus.txt` is the frame after
pressing `Right` with the menu closed: no bar highlight, no submenu, selection
unchanged. The bar becomes navigable only after `v` opens it, after which
`Left`/`Right` walk it correctly (`07-bar-node-*`). So a permanently-VISIBLE bar
is not permanently-REACHABLE, and with hotkeys disabled there was no way in that
this walk could find.

**Three menu rows all read "Set override"** — `[g]`, `[f]`, `[k]`
(`06-legA-offered.txt`). Their registry ids are `set-merge-on-review-cap`,
`set-review-fix-cap` and `set-acceptance-rework-cap`, so the labels are lossy
exactly where the menu is meant to be the primary vocabulary.

## Disposition

Both blockers were ruled on 2026-08-20, both the strong way, and both are plan
02's to build:

- a dispatch action joins `ACTION_REGISTRY` with a `menu_path`, availability-gated
  to `ready` items, retiring the palette-string special case — the maintainer
  chose making the walk performable as chartered over amending the mission again;
- the bar becomes reachable without a hotkey, and R4's gate grows to cover
  menu-ENTRY reachability rather than only reachability of actions once inside.

The "Set override" label lossiness rides along with those.

**This plan's release condition is now BOTH of those landing on master**, each
re-measured on the live surface exactly as today's captures were taken.

## A correction this walk forced to the design note

`../milestone-2-evidence-design.md` §3 argues that a permanent bar collapses leg
A's "before invocation" capture to the resting frame. Measured, that is too
strong: the bar shows only the four TOP-LEVEL node labels at rest, and the
per-action availability markers live in a submenu that must be opened. Leg A's
capture is therefore of the OPENED submenu, not of the resting frame. The rest of
§3 stands — the resting frame is still a real capture of the state the operator
was in, and the overlay's summon-before-capture problem is still gone.

## File index

| file | what it captures |
| --- | --- |
| `00-timestamp.txt` | walk start, UTC |
| `00-resting-bar-visible.txt` | resting frame, no keystroke sent — the release-condition measurement |
| `01-lanes.txt` | Lanes view |
| `02-ready-selected.txt` | lane navigation moves lane-by-lane, not row-by-row |
| `03-ready-drilled.txt` | drilled into the `ready` lane, all 7 items |
| `04-zfcp-selected.txt` | walk target selected |
| `05-bar-focus.txt` | `Right` with the menu closed — the inert-bar finding |
| `06-legA-offered.txt` | leg A: offered-actions state with textual markers |
| `07-bar-node-1.txt` | View node |
| `07-bar-node-2.txt` | Help node |
| `07-bar-node-3.txt` | File node |
