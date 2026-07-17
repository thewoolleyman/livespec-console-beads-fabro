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
> download yet. Standalone release binaries — published on the version schedule
> via release-please — are a tracked deliverable (work-item
> `livespec-console-beads-fabro-z62`), whose scope explicitly includes building
> the linux x86_64 binary in CI **and** downloading-and-testing the published
> release artifact before it is considered done.
>
> **Current install path (until then):** build from source per
> [Developer build](#developer-build) — a one-line `cargo build --release`.

Once the pipeline lands, downloading the latest release will be a one-liner:

```bash
gh release download --repo thewoolleyman/livespec-console-beads-fabro \
  --pattern 'livespec-console-beads-fabro'
# then put it on your PATH as `livespec-console-beads-fabro`
```

or fetch the binary for your platform from the repository's Releases page.

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
| `LIVESPEC_CONSOLE_REPO` | selected repo id; drives the header and the `--repo` passed to the drive program | `livespec-console-beads-fabro` |
| `LIVESPEC_CONSOLE_STORE_PATH` | SQLite event-store path (parent dir auto-created) | `tmp/livespec-console.sqlite` |
| `LIVESPEC_CONSOLE_NEEDS_ATTENTION_PROGRAM` | attention-snapshot program, called `<prog> --json` | `needs-attention` |
| `LIVESPEC_CONSOLE_DRAIN_PROGRAM` | factory drain program, called `<prog> drain` | `livespec-dispatcher-drain` |
| `LIVESPEC_CONSOLE_DRIVE_PROGRAM` | orchestrator drive/valve program, called `<prog> --repo <repo> --action <id>`; every setting write and per-item valve rides this one program | `livespec-orchestrator-drive` |

### The screen

A single screen laid out in three rows:

- **Header** (`LiveSpec Console`) — a status line:
  `fleet: livespec | mode: tui | repo: <repo> | view: <view> | attention: <N>`.
  On a narrow terminal the header degrades gracefully (dropping the constant
  fields first); while any backing source is unavailable it also carries a
  `sources: <N> unavailable (…)` segment so a cockpit-blind screen is never
  mistaken for an idle factory.
- **Body** — a left **Views** navigation list plus a middle list and a
  **Detail** pane. There are six views: **Attention, Spec, Lanes, Events,
  Repos, Settings**. Focus starts on the **Views** menu (the focused pane's
  title carries a `[focus]` tag): `↑`/`↓` walk the menu, and `Enter`/`→` move
  focus into the content pane. The **Lanes** view shows a lane overview (each
  lane with its count and a few preview items); in the content pane `Enter`
  drills into a single lane, where `↑`/`↓` then select an individual
  work-item. The **Settings** view lists the dispatcher policy settings (see
  [Dispatcher settings](#dispatcher-settings)).
- **Footer** (`Status`) — the shortcut hint line.

### Keys

Focus lives in one of two panes — the left **Views** menu or the **Content**
pane — and `↑`/`↓` drive whichever holds focus. `Enter` dives in; `Esc` steps
back. The focused pane's title carries a `[focus]` tag.

| Key | Action |
|---|---|
| `↑` / `↓` | **Views** focus: move the highlighted view up/down the menu. **Content** focus: move the list / lane / **drilled-in lane's per-item** / settings-row / modal-action selection |
| `Enter` | dive from the Views menu into the content pane; in content, open the selected item's details, drill into the selected lane, edit the selected Settings row, or confirm a modal |
| `Esc` | step back (content → Views menu; a drilled-in lane → its overview first); closes an open overlay first |
| `←` / `→` | `←` previous view (Views focus) or step out to the menu (content focus); `→` dive into content (Views focus) or next view (content focus) |
| `/` | open search |
| `:` | open the command palette (drain) |
| `Enter` / `Space` | edit the selected **Settings** row (an ordinary recorded write — no arming ceremony) |
| `s` | **move the selected work-item to a status** — any pre-terminal pipeline status (`backlog` / `ready` / `active` / `blocked`), plus the semantic `approve` (→ ready), `accept` (→ done), and `resolve-blocked`; opens a confirm modal, `↑`/`↓` change the target, `Enter` confirms. `done` is reached only via `accept`, and a shipped `done` item offers no onward move |
| `p` / `c` / `r` | **approve** / **accept** / **reject** the selected work-item (confirm modal; reject is warned as dangerous) |
| `m` / `n` | **set-admission** / **set-acceptance** — the per-item override of `auto_approve_ready` / `acceptance_mode` for the selected work-item |
| `g` / `f` / `k` | **per-item override** of `merge_on_review_cap` / `review_fix_cap` / `acceptance_rework_cap` on the selected work-item (confirm modal; `↑`/`↓` cycle the value, including `clear` to inherit the global default) |
| `?` | toggle the help overlay |
| `q` | quit (only when no overlay is open) |
| `Ctrl-C` | quit (any time) |

The per-item valves `p` / `c` / `r` / `m` / `n` / `g` / `f` / `k` act on the
**selected work-item** — the selected item in the **Attention** view, or the
individually selected item in a **drilled-in lane** (`Lanes` view). The **`s`
move-to-status** valve acts on the selected item in a **drilled-in lane only** —
it needs the item's current lane to offer the statuses it may be driven to, so it
is inert in the Attention view. Every one is issued through the orchestrator's
`drive` API; the console never writes the ledger directly.

Press `?` for a help overlay that lists every keybinding, **scoped to the active
view** (the Settings view describes the dispatcher settings; the item views
describe selection, the status move, and the per-item valves). The footer line
is the always-visible affordance summary. `Tab` does **not** switch views — walk
the **Views** menu with `↑` / `↓` (or use `←` / `→`).

### Dispatcher settings

The factory's routine autonomy is governed by six **dispatcher policy
settings**. The orchestrator OWNS every setting; the console only **commands and
observes** them — it holds no setting state of its own, derives every value it
shows from the orchestrator's published read surface, and issues every write
through the orchestrator's `drive` API (never by editing the orchestrator's
`.livespec.jsonc` or the ledger directly). There is **no autonomous-mode master
switch**: each setting is an independent dial, and enabling a dangerous one is
an ordinary recorded write — no type-the-repo-name ceremony.

Open the **Settings** view and press `Enter` / `Space` on a row to edit it (a
bool toggles, an enum cycles, an int increments/wraps). A setting whose
non-default value lets the factory act without a human is labelled **"dangerous
/ use with caution"** wherever it appears.

The six settings (by their orchestrator key), and each one's **per-item
override** — the mechanism for departing from the global default on a single
work-item:

| Setting (global default) | Type | Dangerous? | Per-item override |
|---|---|---|---|
| `auto_approve_ready` | bool | **yes** — auto-approves a ready item with no human | **`m`** set-admission (`auto`/`manual`) on the selected item |
| `merge_on_review_cap` | bool | **yes** — ships past the review cap with no sign-off | **`g`** on the selected item (`on`/`off`/`clear`) |
| `acceptance_mode` | enum `ai-then-human` \| `ai-only` \| `human-only` | **yes** when `ai-only` (AI auto-accepts) | **`n`** set-acceptance (the policy) on the selected item |
| `review_fix_cap` | int | no | **`f`** on the selected item (positive int, or `clear`) |
| `acceptance_rework_cap` | int | no | **`k`** on the selected item (positive int, or `clear`) |
| `wip_cap` | int | no | **none** — a per-repo concurrency ceiling, structurally not per-item |

Every overridable setting has a per-item valve: `auto_approve_ready` and
`acceptance_mode` ride the established `set-admission` / `set-acceptance` actions
(`m` / `n`), and `merge_on_review_cap` / `review_fix_cap` / `acceptance_rework_cap`
ride the orchestrator's per-cap override actions (`g` / `f` / `k`). Each
per-item valve sets the override for one work-item, or clears it (value `clear`)
to inherit the global default. Only `wip_cap` — a per-repo concurrency ceiling —
admits no per-item override.

### Acting on work

Needs-attention items (pending approval, acceptance review, blocked-on-human)
appear in the **Attention** view, with their details in the **Detail** pane. The
**Lanes** view shows the full seven-lane board; drill into a lane and `↑`/`↓`
select an **individual work-item**.

On the selected work-item the per-item valves are **bound to keys** and drive
the orchestrator's `drive` API:

- **`s` move to a status** the item may be driven to — **drilled-in lane only** —
  any pre-terminal pipeline status (`backlog` / `ready` / `active` / `blocked`)
  via the guarded `move` action, plus the semantic `approve` (pending-approval →
  ready), `accept` (acceptance → done), and `resolve-blocked` (blocked →
  ready/backlog). `done` is reached only via `accept`, and the picker never
  un-ships a `done` item;
- **`p` / `c` / `r`** approve / accept / reject; **`m` / `n`** set the admission
  / acceptance policy override; **`g` / `f` / `k`** set the `merge_on_review_cap`
  / `review_fix_cap` / `acceptance_rework_cap` per-item override (value or `clear`)
  — on the selected item in the **Attention** view or a **drilled-in lane**;
- **drain the ready queue** — press `:`, type `drain` (or `drain ready queue`),
  then `Enter`.

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
