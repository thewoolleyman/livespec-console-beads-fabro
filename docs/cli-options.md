# CLI options

## Sub-commands

```
livespec-console-beads-fabro <command>
```

| Command | What it does |
|---|---|
| `serve` | Start the console. Runs the interactive full-screen TUI when stdout is a terminal; otherwise prints a one-shot store-backed report. |
| `tui` | Alias for `serve` when stdout is a terminal. On a non-terminal stdout it prints a rendered demo screen rather than a store report. |
| `backfill` | Print the source backfill report. |
| `events tail` | Print the most recent stored events. |
| `snapshot` | Print the current projection snapshot. |
| `doctor` | Print the console's own health report. |
| `arch-check` | Points at `just check-arch`; the architecture check itself runs from the workspace. |
| `help`, `--help`, `-h` | Print the command list and exit 0. Also printed when invoked with no arguments. |

An unrecognized command exits **2** with `unknown command: <x>` and the help
text. `events` with any sub-command other than `tail` exits **2** with
`usage: livespec-console-beads-fabro events tail`.

### Flags

There is exactly one flag:

| Flag | Applies to | Effect |
|---|---|---|
| `--preview` | `serve`, `tui` | Suppress the interactive TUI. `serve --preview` still runs the store-backed path and prints the `serve` report; `tui --preview` prints the rendered demo screen. |

It is matched positionally, immediately after the sub-command. The console
has no general argument parser, so there are no other flags — `events tail`
has no limit flag, for instance; it prints the most recent 20 events.

## Environment variables

All are optional. The console reads eleven of its own, plus `HOME`.

### Selecting what to operate on

| Variable | Purpose | Default |
|---|---|---|
| `LIVESPEC_CONSOLE_REPO` | Repository **id**. Drives the header and keys the attention stream. | the current directory's basename, falling back to `livespec-console-beads-fabro` |
| `LIVESPEC_CONSOLE_REPO_PATH` | Repository **filesystem path**. Supplies `--repo` to the drive and drain programs, and roots the dispatcher journal. | the current working directory |
| `LIVESPEC_CONSOLE_STORE_PATH` | SQLite event-store path. The parent directory is created if absent. | `tmp/livespec-console.sqlite` |

`LIVESPEC_CONSOLE_REPO` and `LIVESPEC_CONSOLE_REPO_PATH` are independent. The
id is cosmetic and keys the stream; the path is what actually reaches the
orchestrator. Setting only the id will mislabel the header while still driving
your current directory. See
[Running against a different repository](installing.md#running-against-a-different-repository).

### Backing programs

Each names a program the console shells out to. Where a default resolves from
the orchestrator plugin, the console discovers the plugin root first (see
below) and uses the script it ships; otherwise it uses a bare command name
resolved on `PATH`.

| Variable | Invoked as | Default |
|---|---|---|
| `LIVESPEC_CONSOLE_NEEDS_ATTENTION_PROGRAM` | `<prog> --json` | `needs-attention`, or the plugin's `needs_attention.py` |
| `LIVESPEC_CONSOLE_LIST_WORK_ITEMS_PROGRAM` | work-item listing | `list-work-items`, or the plugin's `list_work_items.py` |
| `LIVESPEC_CONSOLE_DRIVE_PROGRAM` | `<prog> --repo <repo-path> --json` | `livespec-orchestrator-drive`, or the plugin's `drive.py` |
| `LIVESPEC_CONSOLE_DRAIN_PROGRAM` | `<prog> loop --repo <repo-path>` | `livespec-dispatcher-drain`, or the plugin's `dispatcher.py` |
| `LIVESPEC_CONSOLE_LIVESPEC_PROGRAM` | `<prog> next --json` | `livespec` — deliberately never resolved from the plugin directory |
| `LIVESPEC_CONSOLE_FABRO_PROGRAM` | Fabro binary | `fabro`, auto-resolved to `~/.local/bin/fabro` then `~/.fabro/bin/fabro` when present |
| `LIVESPEC_CONSOLE_GH_PROGRAM` | GitHub CLI | `gh` |

Every per-item valve and every settings write rides the **drive** program;
the console never writes the ledger itself.

A program path ending in `.py` is invoked as `python3 <script>`, so its
executable bit does not matter. Child processes get a null stdin, so a shelled
CLI cannot steal the terminal out from under the TUI.

### Discovery and environment

| Variable | Purpose | Default |
|---|---|---|
| `LIVESPEC_CONSOLE_ORCHESTRATOR_PLUGIN_ROOT` | Explicit orchestrator plugin root, used to resolve the backing programs above. | discovered — a repo-local `scripts/bin`, else the installed-plugin record under `~/.claude` |
| `HOME` | Roots plugin-cache and Fabro discovery. | — |

The credential wrapper supplies `BEADS_DOLT_PASSWORD` for the tenant. The
console never reads that secret itself; it runs under the wrapper, which
injects it per command.

## Refresh cadence

The console polls its backing sources on a background thread every 2 seconds,
and re-polls on demand immediately after any action that mutates the ledger,
so a valve you press is reflected without waiting for the next tick. Keyboard
input is polled every 250 ms.
