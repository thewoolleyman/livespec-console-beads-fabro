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

> **Acceptance-verified on linux `x86_64` only, for the download path.** The
> published `v0.2.0` asset was downloaded with the commands above, checksum-
> verified, and run from an arbitrary working directory outside any git
> repository, against two different repositories, with each launched from
> inside its own checkout (see [Running against a different
> repository](#running-against-a-different-repository)). That is the extent of
> what has been exercised: one host, one architecture, the `serve` read path.
> Other platforms have no published asset and no acceptance run — use the
> source build below.

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

**Run the console from inside the repository you want to observe.** This is
the supported invocation, and the one the acceptance run exercised:

```bash
cd /path/to/some-other-repo
/usr/local/bin/with-livespec-env.sh -- livespec-console-beads-fabro serve
```

Both settings then default correctly from the working directory.

### Why the working directory is load-bearing

Setting `LIVESPEC_CONSOLE_REPO_PATH` is **not** a substitute for changing
directory. The Beads tenant is resolved from the working directory's
`.beads/`, and the orchestrator plugin root is discovered relative to the
working directory too — so a console started outside a repository reaches
neither, whatever the environment says. It still launches and still exits
cleanly, because every unreachable source degrades to a *not observed*
finding rather than crashing; the panes are simply empty and the header names
what is unavailable.

Measured on the `v0.2.0` asset against
`/data/projects/livespec-orchestrator-beads-fabro`:

| Launched from | Sources observed |
|---|---|
| inside the repository | live work items, PRs, and attention items |
| any other directory, with `LIVESPEC_CONSOLE_REPO_PATH` set | none — all five sources *not observed* |

An empty cockpit from the second form is this limitation, not an outage.

### Passing environment variables through the credential wrapper

`with-livespec-env.sh` executes its command in a **clean environment**, so
variables set in front of the wrapper are dropped before the console ever
sees them:

```bash
# WRONG — the wrapper strips this; the console never sees it
LIVESPEC_CONSOLE_STORE_PATH=/tmp/store.sqlite \
  /usr/local/bin/with-livespec-env.sh -- livespec-console-beads-fabro serve

# RIGHT — set the variable inside the wrapper's environment
/usr/local/bin/with-livespec-env.sh -- env \
  LIVESPEC_CONSOLE_STORE_PATH=/tmp/store.sqlite \
  livespec-console-beads-fabro serve
```

This applies to every `LIVESPEC_CONSOLE_*` variable in
[CLI options](cli-options.md), not just the ones shown here.

Give each repository its own event store if you want their histories kept
apart — see `LIVESPEC_CONSOLE_STORE_PATH` in
[CLI options](cli-options.md).
