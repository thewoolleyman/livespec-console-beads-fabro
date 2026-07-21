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
> repository, against two different repositories — each launched from inside
> its own checkout, which `v0.2.0` required. That is the extent of what has
> been exercised: one host, one architecture, the `serve` read path.
> Other platforms have no published asset and no acceptance run — use the
> source build below.
>
> **The acceptance run has not been repeated since.** The globs above fetch
> the CURRENT release, `v0.3.0`, which is a superset of the acceptance-run
> build — it carries the cross-repo fix described under
> [Running against a different repository](#running-against-a-different-repository),
> so it does not need the `cd` that `v0.2.0` did. That fix is
> covered by the test suite, not by a download-and-run acceptance exercise;
> no published asset after `v0.2.0` has been through one.

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

To operate on another repository **without changing directory**, set both
explicitly — the path and the id are separate knobs, and setting only the id
leaves the drive program pointed at your current directory:

```bash
/usr/local/bin/with-livespec-env.sh -- env \
  LIVESPEC_CONSOLE_REPO_PATH=/path/to/some-other-repo \
  LIVESPEC_CONSOLE_REPO=some-other-repo \
  livespec-console-beads-fabro serve
```

Note the `-- env` form: the credential wrapper drops variables set in front of
it. See [below](#passing-environment-variables-through-the-credential-wrapper).

The console runs each backing CLI **with its working directory set to the
selected repository**, so the Beads tenant and the orchestrator plugin root
resolve against that repository rather than against your shell's current
directory. Both invocations above observe the same sources — verified against
`/data/projects/livespec-orchestrator-beads-fabro` from a directory outside
any git repository: identical event, backfill, and attention counts either
way, with zero sources reported *not observed*.

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
