# scenarios.md -- livespec-console-beads-fabro

Behavioral journeys for the console.

## Scenario 1 -- Operator sees one needs-attention inbox

```mermaid
flowchart LR
  Fabro["Fabro human gate"]
  Spec["LiveSpec revise needed"]
  Dispatcher["Dispatcher backlog bounce"]
  Snapshot["product needs-attention snapshot"]
  Adapter["needs-attention adapter (diff at ingest)"]
  AttnEvents["attention_item.* events"]
  Projection["needs-attention projection"]
  TUI["needs-attention view"]

  Fabro --> Snapshot
  Spec --> Snapshot
  Dispatcher --> Snapshot
  Snapshot --> Adapter --> AttnEvents --> Projection --> TUI
```

```gherkin
Feature: Unified needs-attention inbox
  As a LiveSpec operator
  I want one place to see work requiring my attention
  So that I do not have to poll LiveSpec, the orchestrator's work-items surface, Dispatcher, Fabro, and GitHub separately

Scenario: Mixed source signals appear as needs-attention items
  Given the product needs-attention snapshot composes a blocked Fabro run with a human gate, pending proposed changes requiring revise, and a non-converging item bounced to `backlog` for re-grooming
  When the needs-attention adapter ingests the snapshot and diffs it into attention_item events
  Then the needs-attention view lists all three items from the attention_item stream
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
  So that ready work-items can enter Dispatcher/Fabro without manual command assembly

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
  Source["Work-items current state (orchestrator CLI)"]
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

Scenario: Work-items current-state snapshot lacks transition history
  Given the Work-items adapter can observe current work-item state through the orchestrator CLI
  And the source cannot prove every historical transition
  When the adapter backfills the repo
  Then it emits state snapshot events
  And emits an ingestion completeness finding
  And the projection shows current truth without pretending full transition history is known
```

## Scenario 5 -- TUI-first operator workflow

```mermaid
flowchart TB
  List["needs-attention list"]
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

Scenario: Operator inspects a lane-derived needs-attention item
  Given a selected needs-attention item is derived from a blocked needs-human work-item lane
  When the operator opens the detail pane
  Then the TUI shows the repo, work item, and latest timeline events
  And no local dismiss command is offered from the needs-attention lens
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

## Scenario 9 -- Operator sets a dispatcher policy setting from the console

```mermaid
flowchart LR
  Settings["Settings > Dispatcher settings row"]
  Label["Dangerous / use-with-caution label"]
  Command["config.dispatcher_setting_set (one setting)"]
  Port["Orchestrator published command surface"]
  Audit["config.dispatcher_setting.changed event"]
  Observe["Effective value re-read from the orchestrator"]

  Settings --> Label --> Command --> Port --> Audit --> Observe
```

```gherkin
Feature: Dispatcher settings are commanded, recorded, and observed
  As a LiveSpec operator
  I want to set each dispatcher policy setting from the console
  So that I can tune the factory's autonomy one dial at a time, with the orchestrator owning the setting state

Scenario: Setting one dial is an ordinary recorded write with no arming ceremony
  Given a registered repo whose dispatcher settings the console observed from the orchestrator
  When the operator edits the Auto-approve ready row in Settings > Dispatcher settings
  Then the TUI shows a "dangerous / use with caution" label on that row
  And the console persists a `config.dispatcher_setting_set` command carrying that one setting and its value
  And no type-to-confirm modal or other arming ceremony is required
  And the handler effects the write through the orchestrator's published command surface
  And appends a `config.dispatcher_setting.changed` audit event
  And never writes the orchestrator's `.livespec.jsonc` key itself

Scenario: The console holds no setting state of its own
  Given the orchestrator reports the effective value of every dispatcher setting
  When the console renders the Settings view
  Then every value shown is the effective value derived from the orchestrator's published read surface
  And the console persists no console-owned copy of any setting
  And an unreadable orchestrator surface degrades to a named not-observed finding rather than an assumed value

Scenario: A simulated orchestrator port surfaces not-wired rather than fabricating success
  Given the orchestrator command port is simulated or unimplemented
  When the operator edits a dispatcher setting row
  Then the console surfaces a not-wired / not-observed outcome
  And appends no event asserting a setting change it did not achieve
```

## Scenario 10 -- A per-item override beats the global default, except `wip_cap`

```mermaid
flowchart LR
  Global["Global default (.livespec.jsonc dispatcher.*)"]
  Item["Per-item override (ledger label)"]
  Effective["Effective value the orchestrator reports"]
  Console["Console renders the effective value"]
  WipCap["wip_cap: per-repo ceiling, no per-item override"]

  Global --> Effective
  Item --> Effective
  Effective --> Console
  WipCap --> Console
```

```gherkin
Feature: Per-item override valve
  As a LiveSpec operator
  I want to override a dispatcher setting for one work-item
  So that a single item can depart from the repo-wide default without changing it for everything

Scenario: A per-item override beats the global default for that item
  Given a repo whose global `merge_on_review_cap` default is false
  When the operator sets a per-item `merge_on_review_cap` override of true on one work-item
  Then the console persists a `work_item.set_dispatcher_override_requested` command
  And invokes the orchestrator's published per-setting override action through its port
  And the orchestrator reports that item's effective value as true while every unlabelled item still inherits false
  And the console renders the effective value the orchestrator reports rather than re-deriving the precedence

Scenario: wip_cap admits no per-item override
  Given a work-item selected in the console
  When a `work_item.set_dispatcher_override_requested` command names `wip_cap` as its setting
  Then the handler rejects the command because `wip_cap` is a per-repo concurrency ceiling
  And the TUI offers no per-item override control for `wip_cap`
  And no ledger label is written

Scenario: Each overridable setting has exactly one console command
  Given the five overridable settings are auto_approve_ready, acceptance_mode, merge_on_review_cap, review_fix_cap, and acceptance_rework_cap
  When the operator overrides admission or acceptance policy on a work-item
  Then the console uses the established `work_item.set_admission_requested` and `work_item.set_acceptance_requested` commands
  And a `work_item.set_dispatcher_override_requested` command naming auto_approve_ready or acceptance_mode is rejected
  And the remaining three settings are served by `work_item.set_dispatcher_override_requested`
  And the Work-item Lifecycle vocabulary is therefore eight commands, seven of them mapping 1:1 onto the orchestrator's `drive` action-id surface
```

## Scenario 11 -- Human valve and policy-edit commands map onto the orchestrator surface

The approve and policy-edit scenes below realize the orchestrator's ratified
work-item state semantics (repo
`thewoolleyman/livespec-orchestrator-beads-fabro`, `SPECIFICATION/contracts.md`,
its Work-item state semantics section and its `drive` action-id surface):
approve is the `pending-approval -> ready` transition and a policy edit never
moves an item between states.

```gherkin
Feature: Human valve and policy-edit commands
  As a LiveSpec operator
  I want to approve, accept, reject, and re-policy work-items from the console
  So that the two human valves and the policy dials are one keystroke away, with the orchestrator owning the ledger write

Scenario: Approve routes through the orchestrator's published action surface
  Given a `pending-approval` work-item whose effective admission_policy is manual, shown in needs-attention
  When the operator invokes Approve on it
  Then the console persists a `work_item.approve_requested` command
  And invokes the orchestrator's published action surface with `approve:<work-item-id>` through its port
  And appends the outcome events from the orchestrator result
  And observes the item's lane change on a subsequent work-items poll

Scenario: Reject with mode regroom maps onto the reject action id
  Given an `acceptance` work-item the operator judges wrongly scoped
  When the operator invokes Reject with mode regroom
  Then the console persists a `work_item.reject_requested` command carrying mode regroom
  And invokes the orchestrator's published action surface with `reject:<work-item-id>:regroom`
  And never writes the ledger directly

Scenario: A policy edit never moves an item between states
  Given a work-item whose stored admission_policy is manual
  When the operator invokes set-admission with policy auto
  Then the console persists a `work_item.set_admission_requested` command
  And invokes the orchestrator's published action surface with `set-admission:<work-item-id>:auto`
  And the item's lifecycle state is unchanged
```

## Scenario 12 -- needs-attention snapshot diffed at ingest into attention_item events

```mermaid
flowchart LR
  Prior["Last ingested needs-attention snapshot"]
  Poll["Poll product needs-attention --json"]
  Diff["Diff by stable id"]
  Appeared["attention_item.appeared (new id)"]
  Changed["attention_item.changed (content changed)"]
  Resolved["attention_item.resolved (id now absent)"]
  Unchanged["unchanged id: emit nothing"]

  Poll --> Diff
  Prior --> Diff
  Diff --> Appeared
  Diff --> Changed
  Diff --> Resolved
  Diff --> Unchanged
```

```gherkin
Feature: needs-attention snapshot diffed at ingest
  As a console maintainer
  I want the stateless product needs-attention snapshot turned into durable events at ingest
  So that the event-sourced console can project appeared, changed, and resolved attention items without the source keeping history

Scenario: The needs-attention adapter diffs a point-in-time snapshot into keyed events
  Given the needs-attention adapter has a prior ingested snapshot of the product needs-attention surface
  And the surface is stateless and point-in-time with no transition history
  When the adapter polls the surface and diffs the new snapshot against the prior one by stable id
  Then it emits an attention_item.appeared event for each id not present before
  And an attention_item.changed event for each present id whose composed content changed
  And an attention_item.resolved event for each previously-present id now absent
  And emits nothing for an unchanged id
  And every emitted event is keyed by the item's stable id
```

## Scenario 13 -- Operator distinguishes cockpit-blind from factory-idle

```mermaid
flowchart LR
  Poll["Poll each backing source"]
  NotObserved["source.not_observed_finding_observed"]
  Observed["observed snapshot"]
  Header["Header source-health indicator"]
  Operator["Operator sees which sources are unavailable"]

  Poll --> NotObserved --> Header --> Operator
  Poll --> Observed
```

```gherkin
Feature: Source-unavailability is legible in the header
  As an operator
  I want the header to show when backing sources are unavailable
  So that a cockpit-blind screen is never mistaken for an idle factory

Scenario: Unavailable sources are counted and named in the header
  Given one or more backing sources degraded to a not-observed finding this cycle
  When the operator screen is rendered
  Then the header shows how many sources are unavailable
  And the header names which sources are unavailable

Scenario: A healthy cycle shows no phantom unavailability count
  Given every backing source was observed this cycle
  When the operator screen is rendered
  Then the header shows no source-unavailability indicator
```

## Scenario 14 -- Settings surface stays in lockstep with the orchestrator's declared keys

```mermaid
flowchart LR
  Declared["Orchestrator's declared API-configurable keys"]
  Check["console-side completeness check"]
  Rows["Settings rows"]
  Help["TUI inline / context help"]
  Doc["README.md"]
  Fail["Check FAILS on a missing key"]

  Declared --> Check
  Rows --> Check
  Doc --> Check
  Help --> Check
  Check --> Fail
```

```gherkin
Feature: API-configurable completeness
  As a console maintainer
  I want every orchestrator-declared setting to reach the operator
  So that a key added upstream can never be silently unreachable from the console

Scenario: A declared key missing from the Settings surface fails the check
  Given the orchestrator declares a dispatcher key the console's Settings surface does not render
  When the console-side completeness check runs
  Then the check fails and names the missing key

Scenario: A declared key missing from the settings doc fails the check
  Given the orchestrator declares a dispatcher key that `README.md` does not document
  When the console-side completeness check runs
  Then the check fails and names the missing key

Scenario: The check reads the producer and never the other way round
  Given the No-Circular-Dependency Directive forbids the orchestrator reading into the console
  When the completeness check runs
  Then it lives in this consumer repo and reads the orchestrator's declared API-configurable-key surface
  And the console does not hardcode the key list
  And no orchestrator-side check reads into the console
```

## Scenario 15 -- Orchestrator auto-dispositions and escalations reach the operator

```mermaid
flowchart LR
  Setting["A dispatcher setting enables an auto-disposition"]
  Journal["Orchestrator journals it (published read surface)"]
  Escalation["Orchestrator escalates what no setting may auto-dispose"]
  Adapter["Console reads the journal"]
  Attention["Escalation surfaces as a needs-attention item"]

  Setting --> Journal --> Adapter
  Escalation --> Journal
  Adapter --> Attention
```

```gherkin
Feature: Auto-dispositions and escalations are observed, never re-derived
  As an operator
  I want every machine disposition and every escalation to be visible in the console
  So that no auto-disposition is silent and no escalation is lost

Scenario: An auto-disposition is read from the orchestrator's journal
  Given a dispatcher setting enabled an auto-approve, an AI auto-accept, an AI-fail auto-rework, a ship-on-cap, or a cap-exceeded escalation
  When the console ingests the orchestrator's published journal read surface
  Then the console reflects that auto-disposition through its own event path
  And attributes it to the setting that governed it

Scenario: An escalation the orchestrator did not dispose reaches the operator
  Given the orchestrator escalated a decision no setting may auto-dispose
  When the console ingests the orchestrator's published journal read surface
  Then the escalation appears as a needs-attention item with its source reference and next operator action
  And the console neither drops, silently defers, nor fabricates the decision
  And the console does not re-derive the escalation from any other source
```

## Scenario 16 -- Factory drain passes the Dispatcher no policy-arming argument

```mermaid
flowchart LR
  Drain["factory.drain_requested"]
  Launcher["Console drain launcher"]
  Argv["Dispatcher invocation argv"]
  NoFlag["NO per-run policy-arming argument"]
  Settings["Dispatcher reads dispatcher.* settings itself"]

  Drain --> Launcher --> Argv --> NoFlag
  NoFlag --> Settings
```

```gherkin
Feature: Dispatch-time policy is never armed per run
  As a LiveSpec operator
  I want the factory-drain launcher to pass no policy flag
  So that dispatch-time policy comes only from the orchestrator-owned settings, and a drain can never be armed behind the settings surface

Scenario: The drain launcher passes no policy-arming argument
  Given a repo whose dispatcher policy settings live in the orchestrator's `.livespec.jsonc`
  When the operator requests a factory drain and the console invokes the Dispatcher through its drain port
  Then the invocation carries no per-run policy-arming argument
  And the Dispatcher reads the `dispatcher.*` settings for itself
  And the console arms no dispatch-time policy of its own

Scenario: A per-run policy flag is not a settings write
  Given the console's settings surface writes exactly one `dispatcher.*` setting per command
  When a drain is launched
  Then no settings write is issued as part of the launch
  And no policy-arming argument is substituted for one
```

## Scenario 17 -- Operator selects a work-item and moves it along the pipeline

```mermaid
flowchart LR
  Lane["Drilled-in lane"]
  Select["Operator selects one work-item"]
  Picker["Move-to-status picker (s)"]
  Approve["pending-approval to ready: approve"]
  ResolveBlocked["blocked to ready|backlog: resolve-blocked"]
  Move["pre-terminal to backlog|ready|blocked|active: move"]
  Accept["acceptance to done: accept"]
  Drive["Orchestrator drive action surface"]

  Lane --> Select --> Picker
  Picker --> Approve --> Drive
  Picker --> ResolveBlocked --> Drive
  Picker --> Move --> Drive
  Picker --> Accept --> Drive
```

```gherkin
Feature: Individual work-item selection and pipeline status moves
  As a LiveSpec operator
  I want to select one work-item in a lane and move it to a status it may be driven to
  So that I can shepherd a single item along the pipeline, with the orchestrator owning every transition

Scenario: The operator selects an individual work-item in a drilled-in lane
  Given the Lanes view drilled into a lane holding more than one work-item
  When the operator moves the selection within the lane
  Then an individual work-item is selected, not merely the lane
  And the per-item valves act on the selected work-item

Scenario: Moving a pending-approval item to ready routes through approve
  Given a selected `pending-approval` work-item in a drilled-in lane
  When the operator moves it to `ready` from the move-to-status picker
  Then the console persists a `work_item.approve_requested` command
  And invokes the orchestrator's published action surface with `approve:<work-item-id>`
  And never writes the ledger directly

Scenario: Moving a blocked item routes through resolve-blocked
  Given a selected `blocked` work-item
  When the operator moves it to `ready` or `backlog` from the picker
  Then the console persists a `work_item.resolve_blocked_requested` command carrying that target status
  And invokes the orchestrator's published action surface with `resolve-blocked:<work-item-id>:ready|backlog`

Scenario: Moving an item to a pre-terminal status routes through the guarded move action
  Given a selected work-item whose target is not served by a semantic valve
  When the operator moves it to `backlog`, `ready`, `blocked`, or `active`
  Then the console persists a `work_item.move_requested` command carrying that target status
  And invokes the orchestrator's published action surface with `move:<work-item-id>:<target-status>`
  And the orchestrator refuses `done`, `acceptance`, and `pending-approval` as move targets

Scenario: Reaching done requires the acceptance path and the picker never un-ships a done item
  Given a selected work-item
  When the operator opens the move-to-status picker
  Then `done` is offered only for an `acceptance` item and routes through `accept:<work-item-id>`
  And the picker offers no move to `done` for any other source status
  And the picker offers no move out of a `done` work-item
```
