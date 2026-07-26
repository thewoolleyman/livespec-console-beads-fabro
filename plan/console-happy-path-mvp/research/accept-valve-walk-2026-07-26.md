# Accept-valve walk — 2026-07-26, real stack, at the keyboard

Maintainer-directed (supervisor brief 08). This pass exercises the
ACCEPT LEG on real data. **It is NOT Stage 3(b)** — the scoping section
below records exactly what it proves and what it does not.

## Preconditions (verified before any valve press)

- Zero stray `serve` clients (`ps` count of the binary process: 0 —
  the stale 2026-07-21 instance had already been killed 2026-07-26).
- Primary at current master `0b434cd`, pristine; release binary
  rebuilt from it; ONE client launched (`just tui` in tmux; binary
  process count re-verified = 1).
- The four targets sat at `acceptance` under `ai-then-human` with the
  AI pass already recorded PASS at recovery (reconcile output per
  item), `assignee: fabro` retained.

## The walk (every press at the TUI keyboard; per-item verification)

For each item: Attention list → select the `Acceptance review` row →
confirm the Detail pane id → `c` → confirm the Valve modal's `Target:`
line → `Enter` → verify the command row `completed` in the console
store and the lane from `list-work-items`.

| item | valve result | ledger after |
|---|---|---|
| `livespec-console-beads-fabro-qwjfsw` | accept `completed` | `done` |
| `livespec-console-beads-fabro-sreeqc` | accept `completed` | `done` |
| `livespec-console-beads-fabro-276inb` | accept `completed` | `done` |
| `livespec-console-beads-fabro-ogpok4` | accept `completed` | `done` |

Attention count dropped 57 → 53 as the four `Acceptance review` rows
cleared. Neither predicted obstacle bit: the `ai-then-human` valve
accepted first press (the AI leg had run at recovery), and the retained
`assignee: fabro` did not interfere — one more corroborating data point
for the assignee-retention observation in
`strand-capture-2026-07-21/README.md`. The cockpit was quit cleanly
afterwards (`q`); resting state is again zero clients.

## What the happy path has now exercised on real data, and what not

Exercised, cumulatively, at the real stack:

- **Admission** (2026-07-21): approve valve `p` (`-276inb`, clean) and
  `s` move `backlog → ready` (`-qwjfsw`, clean).
- **Dispatch** (2026-07-21): palette `:drain` → five items dispatched,
  five PRs merged.
- **Acceptance → done** (2026-07-26, this pass): four items accepted at
  the `c` valve.

NOT exercised — still owed before Stage 3(b) can be called done:

- **The groom leg** (LLM-driver handoff on a backlog item): the verb
  does not exist yet; it awaits the vocabulary ratification and the
  `-l4p3ce` transport decisions.
- **One continuous single-item walk** — find a backlog item, groom it,
  admit it, dispatch it, monitor it, accept it, on ONE dummy item in
  one pass. Every leg above was exercised across DIFFERENT items and
  days, with a strand recovery in the middle.
- **`-sreeqc`'s approve leg** remains discharged-by-workaround
  (`drive.py`), an open TUI leg until `-u3w3er` lands and the retry is
  re-exercised at the keyboard.
- **Stage 3(a)** — the upstream extension of
  `docs/lifecycle-walkthrough.md` (groom legs) — untouched.
- Active-lane monitoring was incidental (journal reads), never a
  deliberate observed leg.
