# Overview and quick start

## What the console is

The console is the human operator's view of a LiveSpec factory. Work flows
through the orchestrator autonomously up to the points where a human must
decide something; the console is where you see that state and make those
decisions.

It owns the operator view across:

- LiveSpec spec-side lifecycle state
- Beads work-items and their seven lifecycle lanes
- Dispatcher waves and journals
- Fabro runs and human gates
- GitHub pull requests and checks
- work held for explicit approval rather than autonomous dispatch

Internally it is a single Rust executable with an event-sourced core, pull
adapters with durable checkpoint/backfill, and SQLite WAL as the durable event
log. The TUI is the first frontend; a GUI could reuse the same command/event
backend later.

**The console commands and observes; it never writes.** It holds no setting
state of its own, derives every value it shows from a published read surface,
and issues every write through the orchestrator's `drive` API — never by
editing the orchestrator's `.livespec.jsonc` or the ledger directly.

## Quick start

Launch the interactive TUI from a source checkout:

```bash
just tui
```

or, from an installed binary, under the credential wrapper:

```bash
/usr/local/bin/with-livespec-env.sh -- livespec-console-beads-fabro serve
```

The console starts the full-screen TUI when standard output is a terminal.
If stdout is not a terminal, or you pass `--preview`, it prints a one-shot
text report instead of taking over the screen.

### Find what needs you

Focus starts on the **Views** menu on the left. `↑`/`↓` walk it; `Enter` or
`→` moves focus into the content pane.

The **Attention** view is the point of the console: it lists exactly the items
waiting on a human — pending approval, acceptance review, and blocked-on-human.
If it is empty, the factory does not need you.

### Act on an item

With an item selected in **Attention**, the per-item valves are bound to
single keys:

| Key | Action |
|---|---|
| `p` | approve |
| `c` | accept |
| `r` | reject (warned as dangerous) |

Each opens a confirmation modal; `Enter` confirms, `Esc` cancels. Every one is
issued through the orchestrator's `drive` API.

### Work the board

The **Lanes** view shows all seven lanes. In the content pane `Enter` drills
into a lane, and `↑`/`↓` then select an individual work-item. Inside a
drilled-in lane you also get `s` — **move to status** — which offers the
statuses that item may legally be driven to.

### Drain the ready queue

Press `:` to open the command palette, type `drain` (or `drain ready queue`),
then `Enter`.

### Get help, and get out

`?` opens the Help modal, auto-focused on the section for whichever pane you
had focused. `Esc` closes it. `q` quits when no overlay is open; `Ctrl-C`
quits at any time, and the terminal is restored on exit.

The **Status** line along the bottom always shows the shortcuts that apply
right now — it changes as you move focus and as modals open and close, so it
is the fastest reminder of what the current keys do.

Next: [Detailed usage](detailed-usage.md) for a section per pane and the full
keybinding reference.
