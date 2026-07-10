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
<!-- part-3 credential-refresh A/B probe -->
