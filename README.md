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
- work held for the operator's explicit approval rather than
  autonomous dispatch

The initial product direction is a single Rust executable with an
event-sourced core, pull adapters with durable checkpoint/backfill,
SQLite WAL as the first durable event log, and a TUI-first operator
experience. A GUI can reuse the same command/event backend later.

The live specification seed is in [SPECIFICATION/](SPECIFICATION/).

## Installing

The console is meant to be run as a **standalone binary** — you should not
need a Rust toolchain to use it.

> **Status:** automated release publishing is **not yet in place**. There is
> no release-please pipeline and no CI job that builds and attaches the binary
> to a GitHub Release, so no release has been cut and there is nothing to
> download yet. Standalone release binaries, published on the version schedule
> via release-please, are a tracked deliverable (see the autonomous-mode plan
> thread). Until then, build from source per [Developer build](#developer-build).

Once releases are published, download the binary for your platform from the
repository's Releases page and put it on your `PATH` as
`livespec-console-beads-fabro`.

## Running the console (the TUI)

### Launch

From a source checkout the primary launch command is:

```bash
just tui
```

which builds the release binary and runs it under the family credential
wrapper (so the bare `BEADS_DOLT_PASSWORD` is injected). `just serve` is an
alias for the same recipe, and extra arguments pass through — for example
`just tui --preview` prints the one-shot text summary described below. The
equivalent raw invocation (prefer `just tui`, which avoids the hyphenated
binary name splitting on copy-paste) is:

```bash
/usr/local/bin/with-livespec-env.sh -- ./target/release/livespec-console-beads-fabro serve
```

`livespec-console-beads-fabro serve` (equivalently `tui`) starts the
interactive full-screen TUI when standard output is a terminal. `serve
--preview` (or any run whose stdout is not a terminal) prints a one-shot text
summary instead of taking over the screen. The console also exposes
non-interactive, store-backed sub-commands: `serve`, `backfill`, `events
tail`, `snapshot`, and `doctor`.

### Prerequisites

The console observes live state by shelling out to the orchestrator and
reading its journals, so a useful session needs:

- a reachable Beads tenant (the server-side Dolt tenant must exist) with the
  credential wrapper on `PATH` so `BEADS_DOLT_PASSWORD` is injected — run the
  console under the wrapper, e.g.
  `/usr/local/bin/with-livespec-env.sh -- livespec-console-beads-fabro serve`;
- the orchestrator CLIs it pulls from resolvable on `PATH`
  (`list-work-items`, `needs-attention`, `fabro`, `livespec next`, `gh`).

Any source it cannot reach degrades to a "not observed" finding rather than
crashing, so the TUI still launches without a live tenant — it just shows
empty panes.

Configuration (all optional environment variables):

| Variable | Purpose | Default |
|---|---|---|
| `LIVESPEC_CONSOLE_REPO` | selected repo id; drives the header, the autonomous-mode target, and the `--repo` passed to the drive program | `livespec-console-beads-fabro` |
| `LIVESPEC_CONSOLE_STORE_PATH` | SQLite event-store path (parent dir auto-created) | `tmp/livespec-console.sqlite` |
| `LIVESPEC_CONSOLE_NEEDS_ATTENTION_PROGRAM` | attention-snapshot program, called `<prog> --json` | `needs-attention` |
| `LIVESPEC_CONSOLE_LIVESPEC_JSONC_PATH` | the `.livespec.jsonc` read to derive, and written to arm, autonomous mode | `.livespec.jsonc` |
| `LIVESPEC_CONSOLE_DRAIN_PROGRAM` | factory drain program, called `<prog> drain` | `livespec-dispatcher-drain` |
| `LIVESPEC_CONSOLE_DRIVE_PROGRAM` | orchestrator drive/valve program, called `<prog> --repo <repo>` | `livespec-orchestrator-drive` |

### The screen

A single screen laid out in three rows:

- **Header** (`LiveSpec Console`) — a status line:
  `fleet: livespec | mode: tui | repo: <repo> | autonomous: on|off | view: <view> | attention: <N>`.
  The `autonomous:` segment is *derived* from the orchestrator's
  `dispatcher.autonomous_mode` key in `.livespec.jsonc` — the console reflects
  the mode, it does not own it. An unreadable config reads as `off`.
- **Body** — a left **Views** navigation list plus a middle list and a
  **Detail** pane. There are five views: **Attention, Spec, Lanes, Events,
  Repos**. Focus starts on the **Views** menu (the focused pane's title carries
  a `[focus]` tag): `↑`/`↓` walk the menu, and `Enter`/`→` move focus into the
  content pane. The **Lanes** view shows a lane overview (each lane with its
  count and a few preview items); in the content pane `Enter` drills into a
  single lane.
- **Footer** (`Status`) — the shortcut hint line.

### Keys

Focus lives in one of two panes — the left **Views** menu or the **Content**
pane — and `↑`/`↓` drive whichever holds focus. `Enter` dives in; `Esc` steps
back. The focused pane's title carries a `[focus]` tag.

| Key | Action |
|---|---|
| `↑` / `↓` | **Views** focus: move the highlighted view up/down the menu. **Content** focus: move the list / lane / modal-action selection |
| `Enter` | dive from the Views menu into the content pane; in content, open the selected item's details, drill into the selected lane, or confirm a modal |
| `Esc` | step back (content → Views menu; a drilled-in lane → its overview first); closes an open overlay first |
| `←` / `→` | `←` previous view (Views focus) or step out to the menu (content focus); `→` dive into content (Views focus) or next view (content focus) |
| `/` | open search |
| `:` | open the command palette |
| `a` | toggle autonomous mode (see below) |
| `?` | toggle the help overlay |
| `q` | quit (only when no overlay is open) |
| `Ctrl-C` | quit (any time) |

Press `?` for a help overlay that lists every keybinding; the footer line is
the always-visible affordance summary. `Tab` does **not** switch views — walk
the **Views** menu with `↑` / `↓` (or use `←` / `→`).

### Enabling autonomous mode (the dangerous switch)

Full autonomous mode lets the factory drive ready work to `done` unattended,
so enabling it is guarded. Press `a`:

1. A confirm modal titled **"Autonomous Mode (dangerous)"** opens, labelled
   **"dangerous / use with caution"**.
2. **Type the repo name exactly** (the modal shows which repo — it is
   `LIVESPEC_CONSOLE_REPO`, default `livespec-console-beads-fabro`) and press
   **Enter**. A mismatched entry is rejected with no effect.
3. The header's `autonomous:` segment flips to `on`, and the mode is persisted
   to the orchestrator's `dispatcher.autonomous_mode` key in `.livespec.jsonc`.

Press `a` again to **disable** — disabling is immediate and takes no
confirmation.

### Acting on work (current scope)

From the running TUI today you can:

- **toggle autonomous mode** (`a`), and
- **drain the ready queue** — press `:`, type `drain` (or `drain ready
  queue`), then `Enter`.

Needs-attention items (pending approval, acceptance review, blocked-on-human)
appear in the **Attention** view, with their details in the **Detail** pane.
The per-item operator valves (approve / accept / reject / set-admission /
set-acceptance) exist in the command/event backend but are **not yet bound to
a TUI key** — surfacing them as in-TUI actions is tracked work. Until then,
those valves are driven through the orchestrator directly.

### Quitting

Press `q` (with no overlay open) or `Ctrl-C`; the terminal is restored on exit.
If an overlay is open, `q` types a literal `q` — close the overlay with `Esc`
first.

## Developer build

You do not need this to *use* the console (see [Installing](#installing)); it
is for contributors and for the interim before release binaries are published.

```bash
cargo build --release --package livespec-console-beads-fabro
# → target/release/livespec-console-beads-fabro
```

Or build-and-run in one step: `cargo run --package livespec-console-beads-fabro -- serve`.

## Development

First-touch setup:

```bash
just bootstrap
```

Run the local gate:

```bash
just check
```

The enforced gate runs Rust formatting, strict Clippy, `cargo test`,
`cargo-nextest`, 100% library line coverage through `cargo-llvm-cov`,
dependency policy through `cargo-deny`, unused dependency detection
through `cargo-machete`, the repo-local AST-based architecture check,
and the behavioral-coverage link check (`check-behavior-coverage`).

Two higher-cost probes are exposed as explicit smoke targets:

```bash
just check-fuzz-smoke
just check-mutants-smoke
```

Fuzzing uses `cargo +nightly fuzz` because sanitizer-backed libFuzzer
targets require nightly-only compiler flags. The product workspace
itself remains pinned to stable Rust through `rust-toolchain.toml`.
The mutation smoke target enumerates candidate mutants only; full
mutation-score enforcement is tracked for the later milestone gate once
the domain model has enough behavior to make surviving getter mutants a
useful failure signal.

Remaining quality and feature work is tracked in the Beads ledger, with
authoritative requirements in [SPECIFICATION/](SPECIFICATION/). Known
follow-ups include growing the fuzz corpus with real event-store inputs
and turning mutation testing from smoke coverage into a hard release
gate. The original bootstrap plan in
`archive/research/tui-first-milestone-bootstrap-plan.md` is retained only as
historical rationale and is no longer a live work tracker.

## Beads

The repo carries non-secret Beads pointer files in `.beads/`. The
server-side tenant still has to exist before `bd list` and the
Beads/Fabro Dispatcher can operate. Run Beads commands through the
family environment wrapper so the bare `BEADS_DOLT_PASSWORD` is present;
never print the secret value.
