# Installing

The console is a **standalone binary**. Using it does not require a Rust
toolchain.

## Download a release binary

Releases are published on the version schedule by release-please, and each
release carries a linux `x86_64` binary plus its SHA-256 sum:

```bash
gh release download --repo thewoolleyman/livespec-console-beads-fabro \
  --pattern 'livespec-console-beads-fabro-*-x86_64-unknown-linux-gnu'
```

Verify the checksum, then put the binary on your `PATH`:

```bash
gh release download --repo thewoolleyman/livespec-console-beads-fabro \
  --pattern 'livespec-console-beads-fabro-*-x86_64-unknown-linux-gnu.sha256'
sha256sum -c livespec-console-beads-fabro-*-x86_64-unknown-linux-gnu.sha256

chmod +x livespec-console-beads-fabro-*-x86_64-unknown-linux-gnu
mv livespec-console-beads-fabro-*-x86_64-unknown-linux-gnu \
  ~/.local/bin/livespec-console-beads-fabro
```

Or fetch the binary for your platform from the repository's Releases page.
Only linux `x86_64` is published today.

> **Not yet acceptance-verified.** The release pipeline and its published
> asset exist, but the pre-delivery acceptance run — downloading the published
> asset, running it from an arbitrary working directory, and exercising it
> against two different repositories — has not been completed. Until it has,
> prefer the source build below if you hit anything unexpected, and report the
> failure.

## Build from source

Building is also the path for contributors and for platforms with no
published asset:

```bash
cargo build --release --package livespec-console-beads-fabro
# → target/release/livespec-console-beads-fabro
```

From a source checkout the console is normally launched through `just`, which
wraps it in the family credential wrapper:

```bash
just tui
```

`just serve` is an alias for the same recipe, and extra arguments pass
through. See [Overview and quick start](overview-quickstart.md).

## Prerequisites

The console observes live state by shelling out to the orchestrator and
reading its journals, so a useful session needs:

- **A reachable Beads tenant** — the server-side Dolt tenant must exist, with
  the credential wrapper on `PATH` so `BEADS_DOLT_PASSWORD` is injected. Run
  the console under the wrapper:

  ```bash
  /usr/local/bin/with-livespec-env.sh -- livespec-console-beads-fabro serve
  ```

- **The programs it pulls from resolvable** — the orchestrator's
  work-item lister, needs-attention, drive, and dispatcher-drain programs,
  plus `livespec`, `fabro`, and `gh`. Each is independently overridable; see
  [CLI options](cli-options.md).

Any source the console cannot reach degrades to a *not observed* finding
rather than crashing, so the TUI still launches without a live tenant — it
just shows empty panes, and the header names what is unavailable.

## Running against a different repository

The console is not pinned to its own repository. Two independent settings
select what it operates on:

| Setting | Selects | Default |
|---|---|---|
| `LIVESPEC_CONSOLE_REPO_PATH` | the repository **filesystem path** — supplies `--repo` to the drive and drain programs, and roots the dispatcher journal | the current working directory |
| `LIVESPEC_CONSOLE_REPO` | the repository **id** — drives the header and keys the attention stream | the current directory's basename |

Running from inside the target repository sets both correctly by default:

```bash
cd /path/to/some-other-repo
/usr/local/bin/with-livespec-env.sh -- livespec-console-beads-fabro serve
```

To operate on another repository without changing directory, set both
explicitly — the path and the id are separate knobs, and setting only the id
leaves the drive program pointed at your current directory:

```bash
LIVESPEC_CONSOLE_REPO_PATH=/path/to/some-other-repo \
LIVESPEC_CONSOLE_REPO=some-other-repo \
  /usr/local/bin/with-livespec-env.sh -- livespec-console-beads-fabro serve
```

Give each repository its own event store if you want their histories kept
apart — see `LIVESPEC_CONSOLE_STORE_PATH` in
[CLI options](cli-options.md).
