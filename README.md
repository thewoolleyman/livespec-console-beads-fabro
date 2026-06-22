# livespec-console-beads-fabro

`livespec-console-beads-fabro` is the operator console for the
LiveSpec family when work is tracked in Beads and executed through the
Beads/Fabro orchestrator. It is intentionally substrate-specific: it is
not the console for the git-jsonl orchestrator.

The console owns the human operator view across:

- LiveSpec spec-side lifecycle state
- Beads work-items
- Dispatcher waves and journals
- Fabro runs and human gates
- GitHub pull requests and checks
- manual / host-only work that must not enter the factory

The initial product direction is a single Rust executable with an
event-sourced core, pull adapters with durable checkpoint/backfill,
SQLite WAL as the first durable event log, and a TUI-first operator
experience. A GUI can reuse the same command/event backend later.

The live specification seed is in [SPECIFICATION/](SPECIFICATION/).

