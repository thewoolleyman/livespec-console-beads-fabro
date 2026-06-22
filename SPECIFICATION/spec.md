# spec.md -- livespec-console-beads-fabro

`livespec-console-beads-fabro` is the LiveSpec-family operator console
for repositories whose implementation work is tracked in Beads and
driven through the Beads/Fabro orchestrator. It is a separate product
from LiveSpec core, the Beads/Fabro orchestrator, and Fabro itself.

## Purpose

The console gives a human operator one coherent place to answer:

- What needs attention now?
- What spec-side action is pending?
- What implementation work is ready?
- What is currently in the factory?
- Which Fabro runs are blocked on human input?
- Which work is manual or host-only and must not enter Fabro?
- What commands can be safely issued next?

The console is event-sourced. It consumes source facts from LiveSpec,
Beads, Dispatcher, Fabro, GitHub, and local repository state, translates
them into canonical console events, and derives operator projections such
as attention inboxes, cards, timelines, and repository health.

## Scope Boundary

The console owns:

- canonical console events and commands
- source adapters and ingestion checkpoints
- event store and command persistence
- projections/read models for operator use
- TUI-first operator UI and later GUI/API surfaces
- human-attention routing and notification-ready alert semantics

The console does not own:

- `/livespec:*` spec mutation semantics
- Beads issue storage semantics
- Dispatcher's factory execution behavior
- Fabro workflow execution, run internals, logs, or sandbox UI
- GitHub pull request merge policy

The console may invoke existing CLIs or APIs through ports/adapters, but
those systems remain the source of truth for their own domains.

```mermaid
flowchart LR
  subgraph Console["livespec-console-beads-fabro"]
    Events["Canonical events"]
    Commands["Commands"]
    Projections["Operator projections"]
    UI["TUI / future GUI"]
  end

  LiveSpec["livespec core\n/spec lifecycle"]
  Orchestrator["livespec-orchestrator-beads-fabro\n/Beads + Dispatcher"]
  Fabro["Fabro\n/run execution + human gates"]
  GitHub["GitHub\n/PRs + checks"]

  LiveSpec -->|"observed through adapter"| Events
  Orchestrator -->|"observed through adapter"| Events
  Fabro -->|"observed through adapter"| Events
  GitHub -->|"observed through adapter"| Events
  Events --> Projections --> UI
  UI --> Commands
  Commands -->|"ports invoke existing systems"| Orchestrator
  Commands -->|"ports invoke existing systems"| LiveSpec
  Commands -->|"ports invoke existing systems"| Fabro
```

## Product Shape

The steady-state product is a single Rust executable:

```text
livespec-console-beads-fabro serve
```

`serve` starts ingestion, the durable event store, projections, API, live
event fanout, and the operator UI surface. Supporting subcommands may
include:

```text
livespec-console-beads-fabro tui
livespec-console-beads-fabro backfill
livespec-console-beads-fabro events tail
livespec-console-beads-fabro snapshot
livespec-console-beads-fabro doctor
livespec-console-beads-fabro arch-check
```

The first UI is a TUI with arrow-driven selection lists, detail panes,
command modals, and live updates. A GUI can later consume the same
events, commands, and projections.

```mermaid
flowchart TB
  Binary["Single Rust executable"]
  Serve["serve"]
  Tui["tui"]
  Backfill["backfill"]
  Tail["events tail"]
  Snapshot["snapshot"]
  Doctor["doctor"]
  Arch["arch-check"]

  Binary --> Serve
  Binary --> Tui
  Binary --> Backfill
  Binary --> Tail
  Binary --> Snapshot
  Binary --> Doctor
  Binary --> Arch

  Serve --> Ingest["ingestors"]
  Serve --> Store["SQLite WAL event store"]
  Serve --> Projectors["projectors"]
  Serve --> Api["API + live fanout"]
  Serve --> Ui["operator UI"]
```

## Architecture

The architecture follows event sourcing, domain-driven design, and
ports/adapters.

```text
source systems
  -> pull adapters
  -> canonical events
  -> durable event log
  -> projectors
  -> TUI / GUI / API

UI commands
  -> command store
  -> bounded-context handlers
  -> ports/adapters
  -> outcome events
```

Adapters start as pull shims over existing systems. Over time, upstream
systems may emit stronger native events, but the console contract is the
canonical event shape it consumes.

```mermaid
flowchart LR
  subgraph Sources["Source systems"]
    LS["LiveSpec files + CLIs"]
    BD["Beads tenant via bd"]
    DJ["Dispatcher journal"]
    FR["Fabro API / ps / SSE"]
    GH["GitHub API"]
  end

  subgraph Adapters["Pull adapters"]
    LSA["LiveSpec adapter"]
    BDA["Beads adapter"]
    DJA["Dispatcher adapter"]
    FRA["Fabro adapter"]
    GHA["GitHub adapter"]
  end

  subgraph Core["Event-sourced console core"]
    Log["Append-only event log"]
    Cmd["Command inbox"]
    Proj["Rebuildable projections"]
    Health["Ingestion health"]
  end

  subgraph Frontends["Operator frontends"]
    TUI["TUI"]
    GUI["Future GUI"]
    API["API clients"]
  end

  LS --> LSA --> Log
  BD --> BDA --> Log
  DJ --> DJA --> Log
  FR --> FRA --> Log
  GH --> GHA --> Log
  Log --> Proj --> Frontends
  Frontends --> Cmd --> Core
  Health --> Proj
```

The hexagonal boundary keeps source-specific mechanics outside the domain:

```mermaid
flowchart TB
  subgraph Outer["Outer adapters"]
    TuiAdapter["TUI adapter"]
    WebAdapter["Future web adapter"]
    FabroAdapter["Fabro adapter"]
    BeadsAdapter["Beads adapter"]
    LivespecAdapter["LiveSpec adapter"]
    DispatcherAdapter["Dispatcher adapter"]
    GithubAdapter["GitHub adapter"]
    SqliteAdapter["SQLite event-store adapter"]
  end

  subgraph Application["Application layer"]
    CommandHandlers["Command handlers"]
    ProjectorRunners["Projector runners"]
    IngestionRunners["Ingestion runners"]
    Ports["Port traits"]
  end

  subgraph Domain["Domain layer"]
    Events["Event types"]
    CommandsDomain["Command types"]
    Aggregates["Aggregates"]
    Policies["Policies + invariants"]
    Errors["Typed domain errors"]
  end

  TuiAdapter --> CommandHandlers
  WebAdapter --> CommandHandlers
  FabroAdapter --> Ports
  BeadsAdapter --> Ports
  LivespecAdapter --> Ports
  DispatcherAdapter --> Ports
  GithubAdapter --> Ports
  SqliteAdapter --> Ports
  CommandHandlers --> Domain
  ProjectorRunners --> Domain
  IngestionRunners --> Domain
  Ports --> Domain
```

## Bounded Contexts

Initial bounded contexts:

- **Ingestion** -- source observation, checkpointing, backfill,
  reconciliation, source health.
- **Factory** -- Dispatcher/Fabro queue drains, selected item dispatch,
  factory pause/resume, human gate observation.
- **Spec Lifecycle** -- LiveSpec `next`, doctor, proposed changes,
  critique, revise-required signals.
- **Grooming** -- needs-regroom routing, slice proposal/approval events,
  factory/manual/spec routing.
- **Attention** -- alerts, acknowledgement, snooze, owner/triage state.
- **Repository Hygiene** -- janitor checks, stale PR/branch/worktree
  findings, primary checkout health.
- **Configuration** -- registered repos, source endpoints, notification
  policy, adapter enablement.

Each bounded context owns its command vocabulary, events, invariants,
aggregates, and projections.

```mermaid
flowchart LR
  Ingestion["Ingestion\nobserve + checkpoint + backfill"]
  Factory["Factory\ndrain + dispatch + gates"]
  Spec["Spec Lifecycle\nnext + doctor + revise signals"]
  Grooming["Grooming\nneeds-regroom + slicing"]
  Attention["Attention\nalerts + ack + snooze"]
  Hygiene["Repository Hygiene\njanitor + stale state"]
  Config["Configuration\nrepos + endpoints + policy"]

  Ingestion -->|"source health events"| Attention
  Factory -->|"human gate / failure events"| Attention
  Spec -->|"revise / doctor events"| Attention
  Grooming -->|"regroom events"| Attention
  Hygiene -->|"hygiene findings"| Attention
  Config --> Ingestion
  Config --> Factory
```

## Terminology

**Canonical event** -- A durable fact in the console event log. It may be
directly emitted by the console or synthesized by an adapter from a source
system's native fact.

**Command** -- An operator intention, such as requesting a factory drain.
Commands are persisted separately from domain events. A command may be
accepted, rejected, run, fail, or succeed; it is not itself proof that the
requested action occurred.

**Adapter** -- A source-specific translator that observes a system such as
Fabro, Beads, LiveSpec, Dispatcher, or GitHub and emits canonical events.

**Projection** -- A derived read model, rebuilt from events, such as the
attention inbox, work card list, event timeline, or repo health view.

**Attention item** -- A projection entry requiring human review or action,
such as a Fabro human gate, LiveSpec revise need, doctor failure, host-only
task, or non-converging factory item.

**Factory** -- The Beads/Fabro execution path: ready work-items selected for
Dispatcher, run in Fabro sandboxes, gated, merged, closed, bounced, or
surfaced.
