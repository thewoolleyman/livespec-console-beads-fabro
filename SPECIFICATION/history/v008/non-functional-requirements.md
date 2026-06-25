# non-functional-requirements.md -- livespec-console-beads-fabro

This document MUST be read alongside `spec.md`, `contracts.md`,
`constraints.md`, and `scenarios.md`. It enumerates the project's
non-functional requirements: contributor-facing concerns -- the
development environment, repository tooling, build and test discipline,
architectural invariants on the implementation, and contributor
workflow -- that are NOT observable at the console's operator-facing
TUI/CLI/API surface.

The four top-level `##` sections below mirror the same four-file
boundary the operator-facing spec uses (`Spec` / `Contracts` /
`Constraints` / `Scenarios`) plus a `Boundary` preamble, so
contributors and agents apply the same categorization rule when landing
new content.

## Boundary

`non-functional-requirements.md` covers concerns of the form "how the
console is built, tested, and maintained". The litmus test for new
content: a constraint a console operator could observe stays in the
operator-facing functional files; a constraint that binds only the
project's contributors lives here.

The boundary against the operator-facing functional files:

- Operator-facing intent or behavior MUST stay in `spec.md`.
- Operator-facing wire contracts (event/command envelopes, persistence
  schemas, adapter and TUI contracts) MUST stay in `contracts.md`.
- Constraints whose violation a console operator could observe MUST
  stay in `constraints.md` (the single-binary multi-mode runtime shape
  and the event-sourcing safety guarantees).
- Operator-facing scenarios MUST stay in `scenarios.md`.

The trickiest boundary is `constraints.md` <->
`non-functional-requirements.md`: constraints whose violation an
operator could observe MUST stay in `constraints.md`; constraints that
bind only the project's contributors MUST move here. The implementation
language, the railway-oriented error discipline, the bounded-context
layering, the architecture tests, the quality gate, and the behavioral
coverage discipline are all contributor-facing and live here; the
event-sourcing safety guarantees an operator relies on stay in
`constraints.md`.

The decision rule for each section below:

- `## Spec` -- contributor-facing process intent and behavior: the
  commit discipline and what "done" means. Mirrors `spec.md`'s role.
- `## Contracts` -- contributor-facing toolchain and invocation
  surface: the tools the project depends on, the `just check`
  aggregate, the quality gate, the behavioral-coverage linkage, and the
  family secret convention. Mirrors `contracts.md`'s role.
- `## Constraints` -- architectural invariants on the implementation:
  language, error handling, bounded-context layering, and architecture
  tests. Mirrors `constraints.md`'s role.
- `## Scenarios` -- Gherkin-style scenarios for contributor-facing
  workflows. Empty initially; populated when a specific contributor
  flow needs to be pinned.

## Spec

This section enumerates the project's contributor-facing process intent
and behavior -- the analogue of `spec.md`'s role for the
operator-facing surface.

### Red-Green-Replay

Rust product changes MUST follow the family Red-Green-Replay commit
discipline, and the repo MUST enforce it mechanically:

1. The Red commit stages the test only and records the failing-test
   evidence.
2. The Green amend stages the implementation and records passing
   evidence.
3. The final commit carries test and implementation plus both trailer
   sets.

A repository hook (`commit-msg`) and the `just check` aggregate MUST
enforce this discipline; a commit that violates the staged-phase or
trailer requirements MUST be rejected. The enforcing check MUST be a
first-class check in this repository (this console is currently the
family's only Rust component), porting the discipline of livespec's
`dev-tooling/checks/red_green_replay.py`. Until that check is wired,
this requirement is unmet, not waived.

Non-product or spec-only changes MAY use the family
non-Python/non-product exemption pattern.

## Contracts

This section enumerates the contributor-facing toolchain and invocation
surface -- the analogue of `contracts.md`'s role for the
operator-facing surface.

### Quality Gate

The contributor quality gate is split across three surfaces by cost and
determinism. Fast, deterministic checks run in the inner loop; slow or
non-deterministic checks run on the merge gate and nightly so they
cannot slow or thrash the implementation loop.

**Inner loop -- `just check` (fast + deterministic; runs locally and in
CI on every push and pull request).** It MUST include:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings` (the
  workspace already denies `unwrap`, `expect`, `panic`, `todo`, and
  `unimplemented`, and forbids `unsafe`)
- tests with a modern Rust test runner (`cargo nextest`)
- coverage gated at **100% line** today
  (`cargo llvm-cov --workspace --lib --fail-under-lines 100`), with
  **100% region** coverage as the stated next target -- adding
  `--fail-under-regions 100` -- tracked as the `coverage-region-gate`
  impl obligation, NOT yet a present gate. Both bind **every** workspace
  library (`--lib`) target with **no per-crate carve-outs** --
  `console-domain`, `console-application`, `console-eventstore`,
  `console-tui`, `console-cli`, and any future library crate.
  Coverage is a design forcing-function, not a QA afterthought: code
  that cannot be exercised by a meaningful unit test MUST be treated as
  a cohesion/coupling smell and redesigned. **No coverage exclusions are
  permitted** -- regions the language makes uncoverable (macro-generated
  code, exhaustiveness arms for states the type system already makes
  impossible) MUST be eliminated by restructuring, never annotated away;
  a UI or I/O branch that resists coverage (e.g. in the TUI or the SQLite
  event store) is a redesign signal, not grounds for a carve-out. The
  binary entry point (`main.rs`) is the only uncovered shim (e.g.
  `console-cli`'s `main.rs`); all testable logic lives in the covered
  library targets. (`--branch` coverage is unstable in `cargo llvm-cov`,
  so **line** coverage is the falsifiable knob that gates today; region
  coverage is the mature next knob the gate is moving to.)
- property tests for pure logic and replay/projector behavior
- dependency audit/deny checks (`cargo deny`)
- architecture checks (see `## Constraints` -> Architecture Tests)
- the behavioral-coverage linkage check, once its checker lands (see
  Behavioral Coverage below)

`just check` MUST NOT include fuzz or mutation runs.

**Merge gate -- CI on every pull request (in addition to `just check`).**
It MUST include:

- **Fuzzing.** Each fuzz target -- at minimum canonical event-envelope
  decoding, adapter normalization, and source-payload parsing -- MUST
  run a bounded libFuzzer pass of at least 60 seconds per target,
  seeded from a committed regression corpus. Any new crash, timeout, or
  OOM MUST fail the merge. Every input that has ever crashed a target
  MUST be committed to the corpus so the crash can never silently
  regress.
- **Mutation.** `cargo mutants` MUST run scoped to the changed lines
  (`--in-diff`) over the logic crates (`console-domain`,
  `console-application`), using the project test runner
  (`--test-tool nextest`) with a bounded `--timeout`. The merge MUST
  fail on any surviving (MISSED) mutant not on the justified-survivor
  allow-list. The score is NOT gated at a blind 100% -- equivalent
  mutants are unkillable by construction. The allow-list is maintained
  in-source via `#[mutants::skip]` with a mandatory justification
  comment (preferred: visible and diff-reviewable), or via a
  `mutants.toml` path/regex filter for a whole untestable module or a
  permanently-ignored class such as `Debug` implementations.

**Nightly -- scheduled run against the canonical branch.** It MUST
include a full fuzz soak (a longer per-target budget) and a full
`cargo mutants` sweep over the logic crates. A nightly finding (a new
crash, or a new surviving mutant not on the allow-list) MUST NOT fail
the canonical branch; it MUST instead open a **high-priority chore
work-item, filed ready for pickup**, in the project's Beads tenant
(`livespec-console-beads-fabro`). This requires CI to hold credentialed
access to the work-items backend per the Beads/Fabro Family Secret
Convention below.

```mermaid
flowchart TB
  subgraph Inner["just check -- inner loop (local + CI, every push/PR)"]
    Fmt["fmt"]
    Clippy["clippy -D warnings"]
    Tests["nextest"]
    Coverage["coverage 100% line (lib); region next"]
    Props["property tests"]
    Audit["audit / deny"]
    Arch["architecture checks"]
    BehLink["behavioral-coverage link"]
  end

  subgraph Merge["CI merge gate (per PR)"]
    Fuzz["fuzz >=60s/target, no new crash"]
    Mutants["mutants --in-diff, no unjustified survivor"]
  end

  subgraph Night["nightly (canonical branch)"]
    FuzzSoak["fuzz soak"]
    MutantsFull["full mutation sweep"]
    Chore["finding -> open work-item (never fail master)"]
  end

  Inner --> Merge --> Night
  FuzzSoak --> Chore
  MutantsFull --> Chore
```

### Behavioral Coverage

Every normative behavior clause -- every `MUST` / `MUST NOT` / `SHOULD`
/ `SHOULD NOT` clause in `spec.md`, `contracts.md`, `constraints.md`,
and this document -- MUST link to a Gherkin scenario, and every scenario
MUST have a corresponding top-of-pyramid acceptance/integration test.
Operator-facing clauses link to a `##` H2 section in `scenarios.md`;
this document's own contributor-facing clauses link to a `##` H2 section
in `## Scenarios` below. This clause -> scenario -> test chain is the
primary guard that the implementation realizes the specification and
that no specified behavior silently regresses.

The linkage MUST be enforced by a mechanical check, run inside
`just check` and CI, implemented as a first-class check in this
repository -- porting the discipline of livespec's Python plumbing
(`dev-tooling/checks/behavior_scenario_link.py` for the clause ->
scenario guardrail, the shared `spec_clauses.py` gap-id primitive, and
the `tests/heading-coverage.json` link registry).

Once it exists, the check MUST run in **`fail` mode** -- not advisory:
the build fails on any normative clause not linked to a scenario, and on
any scenario without a corresponding test. Implementing that Rust checker
-- and backfilling every clause -> scenario -> test link so it passes --
is a **release-blocking, highest-priority obligation**, tracked as the
`scenario-test-rust-checker` work-item; the gate attaches to that real
checker and runs in `just check` and CI the moment it lands.

Until the checker lands, the requirement is enforced by that tracked
release-blocking obligation, NOT by a fail-closed CI placeholder. A
placeholder that hard-fails CI for a not-yet-built checker was found to
deadlock the merge gate -- it blocks every merge, including the checker's
own PR and unrelated work -- so it is NOT used. Enforcement attaches to
the real checker, never to its absence.

**Binding mechanism.** A scenario is identified by its `scenarios.md`
(or `## Scenarios`) H2 section heading. A clause is bound to a scenario,
and a scenario to its top-of-pyramid acceptance/integration test,
through the `tests/heading-coverage.json` link registry (the `clauses[]`
link shape ported from livespec). A link whose scenario name does not
resolve to a live H2 heading, or a scenario with no registered test,
MUST NOT satisfy the guardrail.

### Beads/Fabro Family Secret Convention

The console and its docs MUST use the current family secret convention:
the 1Password Environment wrapper exports one bare `BEADS_DOLT_PASSWORD`.
There is no per-tenant `BEADS_DOLT_PASSWORD_<tenant>` variable and no
per-tenant-to-bare mapping. Secrets MUST never be committed or echoed.
CI MUST obtain `BEADS_DOLT_PASSWORD` through the same convention when it
needs work-items access (e.g. the nightly chore-opening above).

## Constraints

This section enumerates the architectural invariants on the
implementation -- the analogue of `constraints.md`'s role for the
operator-facing surface.

### Implementation Language

- Product code MUST be Rust.
- `unsafe` is forbidden by default: crates MUST use
  `#![forbid(unsafe_code)]` unless a future spec revision grants a
  narrow exception.

The single-binary multi-mode runtime shape is operator-observable and
its normative force is stated in `constraints.md`.

### Railway-Oriented Programming

- Expected failures MUST be represented with typed `Result` values.
- Panics are bugs, not domain control flow.
- Domain and application code MUST NOT use `unwrap` or `expect` outside
  tests and startup wiring.
- Error types MUST distinguish domain rejection from infrastructure
  failure.
- Use cases SHOULD read as railway pipelines: validate, transform, call
  port, map errors, and emit events.

### Domain-Driven Design

The workspace layering invariants (the single source of truth that the
Architecture Tests below enforce):

- Bounded contexts MUST own their language, commands, events,
  invariants, and projections.
- Domain crates MUST NOT depend on infrastructure: adapters, the SQLite
  event store, web server, terminal UI, HTTP, subprocess, or filesystem
  APIs.
- Application crates MAY depend on domain crates and port traits.
- Source adapters MUST sit behind their own per-source port and MUST NOT
  depend on one another's internals. This isolation is the binding
  invariant; the granularity at which adapters are packaged (separate
  `console-adapter-*` crates versus per-source modules in a single
  adapters crate) is NOT mandated -- the implementation MAY use either,
  and currently realizes adapters as per-source modules. The Architecture
  Tests MUST enforce the isolation at whatever granularity is in use.
- UI crates MUST talk only to projections and command APIs, never
  directly to source systems (Beads, Fabro, LiveSpec, Dispatcher,
  GitHub).

```mermaid
flowchart TB
  subgraph Forbidden["Forbidden dependencies"]
    DomainX["domain"]
    AdapterX["adapters"]
    DbX["SQLite event store"]
    WebX["web"]
    TuiX["tui"]
    ProcessX["subprocess"]
  end

  DomainX -. "MUST NOT depend on" .-> AdapterX
  DomainX -. "MUST NOT depend on" .-> DbX
  DomainX -. "MUST NOT depend on" .-> WebX
  DomainX -. "MUST NOT depend on" .-> TuiX
  DomainX -. "MUST NOT depend on" .-> ProcessX
```

```mermaid
flowchart LR
  Domain["console-domain"]
  App["console-application (+ per-source adapter modules)"]
  EventStore["console-eventstore"]
  Tui["console-tui"]
  Web["console-web future"]
  Cli["console-cli"]
  Arch["console-arch-check"]

  App --> Domain
  EventStore --> App
  Tui --> App
  Web --> App
  Cli --> App
  Arch --> App
```

### Architecture Tests

The repo MUST include architecture tests inspired by ArchUnitTS and
ArchUnitPython, adapted to Rust. They exist as a first-class compiled
check (`console-arch-check`) and run inside `just check` and CI.

The Architecture Tests MUST enforce the Domain-Driven Design layering
invariants stated above; that bullet list is the single source of truth
for the rule set, and this section MUST NOT restate a divergent copy of
it. Concretely, the checks MUST enforce at least:

- the workspace crate-graph layering (no forbidden dependency
  direction), and
- domain has no dependency on adapters, the SQLite event store, web
  server, TUI, HTTP, subprocess, or filesystem APIs, and
- source adapters do not depend on one another's internals (at the
  packaging granularity in use), and
- UI does not call Beads/Fabro/LiveSpec/GitHub directly, and
- product crates do not use `unwrap`/`expect` outside allowed scopes,
  and
- event and command types live in domain/application contracts, not
  adapters, and
- all use cases return typed `Result`.

Each enforced rule MUST be stated and checked falsifiably -- strongly
enough that a reviewer can name an input that makes it fail:

- The crate-graph dependency rules MUST be enforced from a structured
  crate-graph source (`cargo metadata` or equivalent), NOT a text scan.
- Source-level rules (e.g. the `unwrap`/`expect` ban, the
  event/command-type placement, the adapter-isolation rule when adapters
  are modules) MUST be checked at the Rust AST level, distinguishing
  real calls/items from substrings in comments, strings, and identifiers
  such as `unwrap_or`. A bare text scan does NOT satisfy these rules.

```mermaid
flowchart TB
  Metadata["cargo metadata (crate graph)"]
  Ast["Rust AST scan"]
  Rules["architecture rule set (= DDD layering)"]
  Findings["arch-check findings"]
  Check["just check / CI"]

  Metadata --> Rules
  Ast --> Rules
  Rules --> Findings
  Findings --> Check
```

## Scenarios

No contributor-facing scenarios are pinned yet. Operator-facing
scenarios live in `scenarios.md`.
