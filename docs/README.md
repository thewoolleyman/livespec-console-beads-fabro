# livespec-console-beads-fabro — user documentation

`livespec-console-beads-fabro` is the operator console for the LiveSpec family
when work is tracked in Beads and executed through the Beads/Fabro
orchestrator. It gives one human operator a single terminal cockpit over the
spec lifecycle, the work-item ledger, dispatcher waves, Fabro runs, and the
GitHub pull requests those runs produce.

The console is a **projection consumer and command producer**. It never writes
the ledger, the orchestrator's settings files, or a Fabro run directly: it
reads published surfaces, renders them, and issues every mutation through the
orchestrator's `drive` API.

## Table of contents

- [Installing](installing.md) — download a release binary or build from
  source, and point the console at the repository you want to operate.
- [Overview and quick start](overview-quickstart.md) — what the console
  shows you, and the shortest path from launch to acting on a work-item.
- [CLI options](cli-options.md) — every sub-command, flag, and environment
  variable the console reads, with defaults.
- [Detailed usage](detailed-usage.md) — a section per pane, the full
  keybinding reference, the Help modal, and the dispatcher settings.

Contributor-facing material — building the workspace, the quality gate, and
the Beads runtime prerequisites — lives in the repository's top-level
[`README.md`](../README.md), not here.
