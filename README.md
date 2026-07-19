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

The product is a single Rust executable with an event-sourced core, pull
adapters with durable checkpoint/backfill, SQLite WAL as the first durable
event log, and a TUI-first operator experience. A GUI can reuse the same
command/event backend later.

## Documentation

**User documentation lives in [`docs/`](docs/README.md)** — installing the
console, a quick start, the CLI options and environment variables, and
detailed per-pane usage. Start at [`docs/README.md`](docs/README.md).

The live specification is in [SPECIFICATION/](SPECIFICATION/).

The rest of this file is contributor-facing.

## Developer build

You do not need this to *use* the console — see
[`docs/installing.md`](docs/installing.md) for the release-binary path.

```bash
cargo build --release --package livespec-console-beads-fabro
# → target/release/livespec-console-beads-fabro
```

Or build-and-run in one step:
`cargo run --package livespec-console-beads-fabro -- serve`.

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
the real-TUI tmux E2E suite, the behavioral-coverage link check
(`check-behavior-coverage`), and the settings-completeness check
(`check-completeness`) — which asserts every API-configurable dispatcher
key the orchestrator declares (its published config-manifest, captured at
`tests/fixtures/orchestrator-config-manifest.json`) reaches a Settings row,
its inline help, and the settings doc's **Dispatcher settings** section.
Per the specification's User Documentation Contract the settings doc is
[`docs/detailed-usage.md`](docs/detailed-usage.md), not this README.
Refresh the captured manifest with `just refresh-config-manifest` after the
orchestrator's dispatcher key set changes.

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
