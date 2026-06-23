# TUI first milestone bootstrap plan

Captured on 2026-06-23 while starting
`livespec-console-beads-fabro` as a new LiveSpec-family member.

This is a planning and research artifact, not normative specification
text. Requirements become authoritative only through the live
`SPECIFICATION/` lifecycle. Concrete implementation work becomes
authoritative only when filed in the Beads ledger.

## Bottom Line

The console repo has a useful seed specification but is not yet ready
for its own factory. The first milestone should be bootstrapped in two
tracks: first install family infrastructure, Rust quality gates, and the
Beads tenant; then file concrete Beads work-items and let the
Beads/Fabro Dispatcher build independent slices.

The product implementation must be Rust. The quality bar should be the
family maximum: strict linting, 100% intended coverage gates, fuzzing,
property tests, mutation testing, dependency policy checks, custom
architecture checks, typed `Result`-based error handling, and enforced
hexagonal dependency boundaries.

## Current State

Evidence gathered on 2026-06-23:

- Repo is at `/data/projects/livespec-console-beads-fabro`.
- `master` is clean and tracks `origin/master`.
- The seed spec exists under `SPECIFICATION/`.
- `.livespec.jsonc` selects `livespec-orchestrator-beads-fabro` and
  names a `livespec-console-beads-fabro` Beads tenant, but `.beads/`
  pointer files are absent.
- `bd` is installed at version `1.0.5`.
- The family 1Password environment wrapper injects a non-empty bare
  `BEADS_DOLT_PASSWORD`; the value was not printed.
- `bd list` in the console repo fails with `no beads database found`.
- `livespec-orchestrator-beads-fabro:orchestrate plan` reports no
  spec-side actions and fails impl-side discovery on the missing Beads
  database.
- Codex plugins for livespec core, `livespec-driver-codex`, and
  `livespec-orchestrator-beads-fabro` are installed and enabled
  host-wide.

## Planning Workflow

The livespec planning-workflow research in
`../livespec/research/planning-workflow-gap/missing-planning-workflow-thread.md`
recommends using:

- `research/<topic>/...md` for reasoning, options, trade-offs, and open
  questions.
- `prompts/<track>-handoff.md` for active, resumable session
  instructions.
- Beads only after work is concrete enough to rank, depend on,
  implement, and verify.
- The live spec lifecycle only after a conclusion becomes contractual.

This file is the reasoning artifact. The active prompt is
`prompts/tui-first-milestone-handoff.md`.

## Driver-Codex Precedent

The recent `livespec-driver-codex` introduction shows the family birth
pattern for a new sibling:

- Bootstrap repo contents in one coherent pass:
  - `AGENTS.md`
  - `README.md`
  - `.livespec.jsonc`
  - live `SPECIFICATION/`
  - toolchain pins
  - `justfile`
  - `lefthook.yml`
  - development tooling
  - tests
  - CI
  - release and fleet shim workflows
- Add runtime-specific product surface with structural tests.
- Add Claude project-scope settings where the repo needs Claude
  `/livespec:*` support.
- Add Codex host-wide plugin provisioning in `just bootstrap` /
  `just ensure-codex-plugins`.
- Provision the Beads/Dolt tenant as a separate family-infra step:
  committed `.beads/config.yaml`, gitignored regenerable metadata, and
  matching `.livespec.jsonc` connection fields.
- Verify the Codex TUI picker separately from colon-qualified
  `codex exec` invocation.
- Keep behavior in the owning repo: driver-codex kept only Codex
  mechanics; livespec core kept operation prose and wrapper behavior.

Relevant commits inspected:

- `livespec-driver-codex` `948f904`:
  bootstrap Codex Driver plugin, family infra, CI, tests, and spec.
- `livespec-driver-codex` `6d8694b`:
  add `.beads/config.yaml` for a provisioned tenant.
- `livespec-driver-codex` `6322d16`:
  provision family Codex plugins in bootstrap.
- `livespec-driver-codex` `8fa296d`:
  verify Codex skills picker discovery.
- `livespec` `d623768`:
  adopt the distributed Codex Driver contract.
- `livespec` `4f60277`:
  provision Codex plugins through core bootstrap and templates.

Known external issue: `livespec-driver-codex` is not currently present
in `livespec/fleet-manifest.jsonc`, despite being an active family repo.
That bug is being addressed separately. Do not block this console
bootstrap on fixing that unrelated manifest entry, but account for the
same fleet-registration risk when making this repo a family member.

## First Milestone Definition

The first milestone is a Rust TUI version that satisfies the current
specification's TUI-first requirements:

- A single executable with at least `tui`, `serve`, `backfill`,
  `events tail`, `snapshot`, `doctor`, and `arch-check` command shape,
  even if some source adapters are initially minimal but honest.
- Durable SQLite WAL event store with event, command, checkpoint, and
  projection tables.
- Canonical event and command envelopes matching `SPECIFICATION/contracts.md`.
- Idempotent event append by `(source, source_event_id)` when present.
- Rebuildable projections, including the attention inbox.
- TUI attention workflow with arrow selection, detail pane, action list,
  acknowledge, snooze, and Fabro attach-command visibility.
- Factory drain command path that persists a command, validates it,
  invokes Dispatcher through a port, and appends terminal outcome events.
- Pull adapter framework with durable checkpoints, reconciliation
  windows, health findings, and explicit incompleteness findings.
- Initial adapters for Beads, Dispatcher, Fabro, LiveSpec, and GitHub
  that call existing stable CLIs/APIs through ports.
- `doctor` and `arch-check` commands that expose repo-local health and
  architectural violations.

If implementation exposes an ambiguity in the seed spec, file a
spec-side proposed change before relying on an interpretation.

## Bootstrap Checklist

Track 0: durable planning and handoff

- [x] Read the missing-planning-workflow research.
- [x] Inspect the console seed spec and repo state.
- [x] Research the `livespec-driver-codex` git/spec/family history.
- [x] Confirm orchestrator planning is blocked by missing Beads
  database pointers.
- [x] Add this research artifact.
- [x] Add an active kickoff prompt under `prompts/`.

Track 1: family infrastructure

- [ ] Set `livespec.primaryPath` through the repo bootstrap path once
  the bootstrap exists.
- [ ] Create a `just bootstrap` recipe that installs family hooks,
  verifies toolchain prerequisites, and provisions both Claude and Codex
  plugin support.
- [ ] Add `.mise.toml` for non-Rust host tools used by the repo.
- [ ] Add Rust toolchain pinning with `rust-toolchain.toml`.
- [ ] Add `just check` as the full enforcement aggregate.
- [ ] Add `lefthook.yml` with family commit gates.
- [ ] Add `.claude/settings.json` for project-scoped Claude plugin
  enablement:
  `livespec`, `livespec-driver-claude`, and
  `livespec-orchestrator-beads-fabro`.
- [ ] Preserve Codex support as host-wide install instructions and
  `just ensure-codex-plugins`; do not commit host-wide Codex state.
- [ ] Add CI with the repo's Rust checks and family telemetry script if
  applicable.
- [ ] Add fleet shim workflows expected by the family conformance
  surface.
- [ ] Decide the fleet class for this repo. Current manifest classes are
  `core`, `enforcement-suite`, `impl-plugin`, `driver-plugin`, and
  `library`; a Rust application may need either `library` as a practical
  first fit or a spec/dev-tooling change for an `application` class.
- [ ] Register the repo in the fleet manifest once the owner confirms
  the class and the separate driver-codex manifest bug is clear.
- [ ] Run the fleet `wire-fleet-member` reconcile once this repo is
  declared in the manifest.

Track 2: Beads and factory readiness

- [ ] Provision the `livespec-console-beads-fabro` Dolt tenant through
  the family tenant onboarding path.
- [ ] Commit `.beads/config.yaml` with TCP `127.0.0.1:3307` Dolt keys
  and no socket key.
- [ ] Commit `.beads/.gitignore` for regenerable metadata and local
  database artifacts.
- [ ] Regenerate `.beads/metadata.json` outside the primary checkout and
  copy it in only if it is meant to be local/untracked.
- [ ] Verify `.livespec.jsonc` connection fields match `.beads/config.yaml`.
- [ ] Run `bd list` under the family environment wrapper.
- [ ] File the first concrete Beads epic and slice work-items.
- [ ] Re-run `livespec-orchestrator-beads-fabro:orchestrate plan` and
  confirm impl-side actions appear.
- [ ] Switch eligible slices to Dispatcher/Fabro execution after the
  repo can run `just check` locally and in CI.

Status update, 2026-06-23: the non-secret `.beads/` pointer files
merged in `3c82407`; the server-side tenant was provisioned with the
family password; local gitignored `metadata.json` was regenerated from a
scratch directory; `bd list` works under the livespec env wrapper; and
four ready Beads slices now appear in `orchestrate plan`. The remaining
step is operator selection of the first mutating factory action.

Track 3: Rust workspace and product architecture

- [ ] Create a Cargo workspace with narrow crates:
  - `console-domain`
  - `console-application`
  - `console-eventstore`
  - `console-adapter-beads`
  - `console-adapter-dispatcher`
  - `console-adapter-fabro`
  - `console-adapter-livespec`
  - `console-adapter-github`
  - `console-tui`
  - `console-cli`
  - `console-arch-check`
- [ ] Use `#![forbid(unsafe_code)]` in every product crate.
- [ ] Put canonical events, commands, IDs, typed errors, invariants, and
  projection contracts in domain/application crates, not adapters.
- [ ] Keep infrastructure crates out of domain dependencies.
- [ ] Make UI crates call projections and command APIs only.
- [ ] Implement explicit ports for Dispatcher, Beads, Fabro, LiveSpec,
  GitHub, filesystem, process execution, and time.
- [ ] Keep adapter crates independent from each other.
- [ ] Use typed `Result` throughout domain and application use cases.
- [ ] Avoid `unwrap` and `expect` outside tests and narrow startup
  wiring allowed by the specification.

Track 4: Rust quality gates

- [ ] `cargo fmt --check`.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [ ] `cargo nextest run --workspace --all-features`.
- [ ] Coverage gate with `cargo llvm-cov`; target 100% for product
  crates unless a specific generated/FFI line is documented and
  justified.
- [ ] Property tests for event replay, command validation, projection
  rebuilds, source normalization, and idempotent append.
- [ ] Fuzz targets for event decoding, adapter payload parsing, and
  checkpoint parsing.
- [ ] Mutation testing with `cargo-mutants` for domain/application
  crates and any pure adapter-normalization logic.
- [ ] Dependency policy with `cargo-deny` advisories, bans, licenses,
  and duplicate-version rules.
- [x] Unused dependency checks with a maintained Cargo tool such as
  `cargo-machete`.
- [ ] Unsafe scanning with a maintained Cargo tool such as
  `cargo-geiger`, alongside crate-level `forbid(unsafe_code)`.
- [ ] Architecture checks based on `cargo metadata` plus syntax-aware or
  conservative source checks.
- [ ] Filesize and complexity checks for product source files, with
  thresholds codified in the repo rather than enforced by convention.
- [x] CI must run the same `just check` aggregate as local development.

Track 5: TUI milestone slices

- [ ] CLI skeleton and configuration loading.
- [ ] Domain IDs, event envelope, command envelope, and error taxonomy.
- [x] SQLite WAL event store with schema and migrations.
- [ ] Command store and command status transitions.
- [ ] Projector framework and attention projection.
- [ ] TUI layout: attention list, detail pane, action list, status area.
- [ ] TUI input handling: arrow selection, command modal, acknowledge,
  snooze, copy/open attach command.
- [ ] Beads adapter backfill/poll and completeness findings.
- [ ] Dispatcher journal adapter and factory drain port.
- [ ] Fabro adapter for run/gate/terminal state.
- [ ] LiveSpec adapter for `next`, doctor, proposed changes, and spec
  filesystem state.
- [ ] GitHub adapter for PR/check/branch state.
- [ ] `serve` mode to run ingestion, projections, command handling, and
  UI/live updates from one binary.
- [ ] `doctor` command for console-local health.
- [ ] `arch-check` command for architecture rule violations.
- [ ] End-to-end terminal tests for the scenario set in
  `SPECIFICATION/scenarios.md`.

## Recommended Execution Order

1. Land this planning/handoff PR.
2. Create the family-infra bootstrap PR.
3. Provision and commit Beads pointer files.
4. File the Beads epic and first work-item slices.
5. Build a thin Rust skeleton that runs all quality gates before adding
   broad product behavior.
6. Dispatch independent implementation slices through the factory once
   `just check` and CI are reliable.
7. Keep this research file as rationale and refresh the handoff prompt
   whenever active status changes.

## Open Questions

- Which fleet class should a Rust application repo use before an
  `application` class exists?
- Is the first milestone allowed to ship with honest, minimal adapters
  for some sources, or must every initial adapter reach production-grade
  source coverage before milestone acceptance?
- Should the Rust Red-Green-Replay hook be implemented before the first
  product code, or can the first skeleton PR rely on normal Rust test
  review while the hook is being ported?
