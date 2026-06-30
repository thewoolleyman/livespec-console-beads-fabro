# scenarios.md -- livespec-console-beads-fabro

Behavioral journeys for the console.

## Scenario 1 -- Operator sees one attention inbox

```mermaid
flowchart LR
  Fabro["Fabro human gate"]
  Spec["LiveSpec revise needed"]
  Dispatcher["Dispatcher needs-regroom bounce"]
  Events["Canonical events"]
  Projection["Attention projection"]
  TUI["Attention view"]

  Fabro --> Events
  Spec --> Events
  Dispatcher --> Events
  Events --> Projection --> TUI
```

```gherkin
Feature: Unified attention inbox
  As a LiveSpec operator
  I want one place to see work requiring my attention
  So that I do not have to poll LiveSpec, Beads, Dispatcher, Fabro, and GitHub separately

Scenario: Mixed source signals appear as attention items
  Given the Fabro adapter observes a blocked run with a human gate
  And the LiveSpec adapter observes pending proposed changes requiring revise
  And the Dispatcher adapter observes a non-converging item bounced to needs-regroom
  When the console projects the event log
  Then the Attention view lists all three items
  And each item carries a source reference and next operator action
```

## Scenario 2 -- Factory drain command

```mermaid
sequenceDiagram
  participant Operator
  participant TUI
  participant Factory as Factory context
  participant Dispatcher
  participant Events as Event log

  Operator->>TUI: select Drain ready queue
  TUI->>Factory: factory.drain_requested
  Factory->>Events: command.accepted
  Factory->>Dispatcher: invoke configured drain program via drain port
  Dispatcher-->>Factory: terminal outcome
  Factory->>Events: factory.drain.completed or failed
  Events-->>TUI: live projection update
```

```gherkin
Feature: Factory drain command
  As an operator
  I want to request a bounded factory drain from the console
  So that ready Beads work can enter Dispatcher/Fabro without manual command assembly

Scenario: A bounded drain emits command and outcome events
  Given a repo has ready implementation work
  When the operator selects "Drain ready queue" with budget 1 and parallel 1
  Then the console persists a `factory.drain_requested` command
  And the Factory context validates and accepts the command
  And invokes Dispatcher through its port
  And appends started and terminal outcome events
  And the TUI updates live from projections
```

## Scenario 3 -- Pull adapter backfill avoids silent missed data

```mermaid
flowchart TB
  Start["checkpoint N"]
  Window["read from N minus safety window"]
  Normalize["normalize records"]
  Append["idempotent append"]
  Advance["advance checkpoint"]
  Repeat["next poll"]

  Start --> Window --> Normalize --> Append --> Advance --> Repeat
  Repeat --> Window
```

```gherkin
Feature: Checkpointed pull ingestion
  As a console maintainer
  I want every adapter to checkpoint and backfill
  So that polling does not silently miss source activity

Scenario: Adapter replays a reconciliation window idempotently
  Given an adapter has checkpointed source position N
  When it polls again
  Then it reads from N minus its configured safety window
  And emits canonical events with stable source event ids
  And duplicate events are ignored by the event store
  And the checkpoint advances only after durable append
```

## Scenario 4 -- Source cannot prove full transition history

```mermaid
flowchart LR
  Source["Beads current state"]
  Snapshot["state snapshot events"]
  Finding["completeness finding"]
  Projection["projection"]
  Operator["operator sees current truth + caveat"]

  Source --> Snapshot --> Projection --> Operator
  Source --> Finding --> Projection
```

```gherkin
Feature: Honest completeness findings
  As an operator
  I want incomplete source history to be visible
  So that the console never overclaims certainty

Scenario: Beads current-state snapshot lacks transition history
  Given the Beads adapter can observe current work-item state
  And the source cannot prove every historical transition
  When the adapter backfills the repo
  Then it emits state snapshot events
  And emits an ingestion completeness finding
  And the projection shows current truth without pretending full transition history is known
```

## Scenario 5 -- TUI-first operator workflow

```mermaid
flowchart TB
  List["Attention list"]
  Select["Arrow selection"]
  Detail["Detail pane"]
  Timeline["Latest timeline"]

  List --> Select --> Detail --> Timeline
```

```gherkin
Feature: TUI operator workflow
  As an operator using a terminal
  I want arrow-driven views and detail panes
  So that I can drive common orchestration actions before the GUI exists

Scenario: Operator inspects a lane-derived attention item
  Given a selected Attention item is derived from a blocked needs-human work-item lane
  When the operator opens the detail pane
  Then the TUI shows the repo, work item, and latest timeline events
  And no local dismiss command is offered from the attention lens
```

## Scenario 6 -- Policy-rejected command produces no side effect

```mermaid
flowchart LR
  Operator["Operator submits command"]
  Policy["Context policy validation"]
  Rejected["command.rejected event"]
  NoEffect["No port invoked"]
  TUI["TUI shows rejection + reason"]

  Operator --> Policy
  Policy -->|"invalid"| Rejected --> TUI
  Policy -->|"invalid"| NoEffect
```

```gherkin
Feature: Policy-rejected command
  As an operator
  I want commands that violate context policy to be rejected without side effects
  So that the console never acts on an invalid request

Scenario: An invalid drain is rejected and nothing is dispatched
  Given a repo has no ready implementation work
  When the operator requests a factory drain
  Then the Factory context validates the command against policy
  And persists a `command.rejected` event carrying the rejection reason
  And no Dispatcher port is invoked
  And the TUI shows the command as rejected with its reason
```

## Scenario 7 -- Crash-gap recovery reconstructs a missing outcome

```mermaid
flowchart LR
  SideEffect["Port side effect performed"]
  Crash["Crash before outcome append"]
  Restart["Console restart"]
  Reconcile["Reconciliation observes external result"]
  Outcome["Outcome event reconstructed"]

  SideEffect --> Crash --> Restart --> Reconcile --> Outcome
```

```gherkin
Feature: Crash-gap recovery
  As an operator
  I want the console to recover when it crashes between a side effect and its outcome event
  So that the event log eventually reflects what actually happened

Scenario: Reconciliation reconstructs a missing outcome after a crash
  Given a command's port side effect has been performed
  And the console crashed before appending the outcome event
  When the console restarts and reconciliation runs
  Then it observes the external result through the adapter
  And appends the corresponding outcome event
  And the command status reflects the true outcome
```

## Scenario 8 -- Corrupted projection rebuilds by replay

```mermaid
flowchart LR
  Corrupt["Projection snapshot corrupted"]
  Detect["Corruption detected"]
  Drop["Drop snapshot"]
  Replay["Replay append-only event log"]
  Rebuilt["Projection rebuilt"]

  Corrupt --> Detect --> Drop --> Replay --> Rebuilt
```

```gherkin
Feature: Snapshot corruption recovery
  As an operator
  I want corrupted read models to rebuild from the event log
  So that projection corruption never loses durable truth

Scenario: A corrupted projection is rebuilt by replay
  Given a projection snapshot is detected as corrupt
  When the console recovers the projection
  Then it drops the corrupt snapshot
  And rebuilds the projection by replaying the append-only event log
  And the rebuilt projection matches the event log
```
