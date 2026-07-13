use console_application::{
    ApplicationError, AutonomousAudit, AutonomousDecisionsPort, AutonomousModeArmingOutcome,
    AutonomousModeArmingPort, AutonomousModeArmingRequest, FactoryDrainPort,
    FactoryDrainPortOutcome, FactoryDrainRequest, OrchestratorActionOutcome,
    OrchestratorActionPort, OrchestratorActionRequest, build_tui_model,
    source_adapters::{
        AcceptancePolicy, AdapterPoll, AdapterPollRequest, AdmissionPolicy, Lane, LaneReason,
        NeedsAttentionReadOutcome, NeedsAttentionSnapshotPort, PullSourcePort, WorkItemSnapshot,
        normalize_work_item_snapshot,
    },
};
use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};
use console_eventstore::{CommandAppend, EventAppend, SqliteEventStore};
use console_tui::{TuiRuntimeEffect, TuiRuntimeEffectSink};
use livespec_console_beads_fabro::{
    ConsoleRuntimeError, NeedsAttentionIngest, SourceAdapterRef, TuiSessionOutcome,
    TuiSessionRunner, backfill_source_report, handle_pending_factory_commands,
    run_store_backed_tui_session,
};

#[test]
fn scenario_2_factory_drain_command_dispatches_and_projects_outcome()
-> Result<(), ConsoleRuntimeError> {
    let mut store = SqliteEventStore::open_in_memory()?;
    let command = factory_drain_command();
    store.append_command(&CommandAppend::new(
        command,
        "2026-07-08T00:00:00Z".to_owned(),
        Some("fleet:livespec".to_owned()),
        "corr_cmd_drain".to_owned(),
        "{}".to_owned(),
    ))?;
    append_ready_work_item(&mut store)?;
    let mut port = CompletingDrainPort::default();

    let outcomes = handle_pending_factory_commands(&mut store, "2026-07-08T00:00:01Z", &mut port)?;

    let events = store.list_console_events()?;
    let commands = store.list_commands()?;
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].command_status(), "completed");
    assert_eq!(commands[0].status(), "completed");
    assert_eq!(port.observed_requests.len(), 1);
    assert_eq!(
        events
            .iter()
            .map(ConsoleEvent::event_type)
            .collect::<Vec<_>>(),
        [
            &EventType::WorkItemSnapshotObserved,
            &EventType::CommandAccepted,
            &EventType::FactoryDrainStarted,
            &EventType::FactoryDrainCompleted,
        ]
    );
    Ok(())
}

#[test]
fn scenario_3_pull_adapter_backfill_replays_window_and_advances_checkpoint_after_append()
-> Result<(), ConsoleRuntimeError> {
    let mut store = SqliteEventStore::open_in_memory()?;
    let source = ScriptedWorkItemSource::new("7")?;
    let sources: Vec<SourceAdapterRef<'_>> = vec![("orchestrator:fleet", &source)];
    let empty_attention = EmptyNeedsAttentionPort;
    let needs_attention = NeedsAttentionIngest::new(&empty_attention, "fleet");

    let report = backfill_source_report(
        &mut store,
        "2026-07-08T00:00:00Z",
        &sources,
        &needs_attention,
    )?;

    let events = store.list_console_events()?;
    assert_eq!(report, "backfill source adapters: adapters 1, events 2");
    assert_eq!(source.observed_safety_window(), 1);
    assert_eq!(source.observed_checkpoint(), None);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type(), &EventType::WorkItemSnapshotObserved);
    assert_eq!(
        events[1].event_type(),
        &EventType::SourceCompletenessFindingObserved
    );
    assert_eq!(
        store.load_checkpoint("orchestrator:fleet")?,
        Some("7".to_owned())
    );
    Ok(())
}

#[test]
fn scenario_4_snapshot_only_source_emits_completeness_finding() -> Result<(), ConsoleRuntimeError> {
    let mut store = SqliteEventStore::open_in_memory()?;
    let source = ScriptedWorkItemSource::new("8")?;
    let sources: Vec<SourceAdapterRef<'_>> = vec![("orchestrator:fleet", &source)];
    let empty_attention = EmptyNeedsAttentionPort;
    let needs_attention = NeedsAttentionIngest::new(&empty_attention, "fleet");

    backfill_source_report(
        &mut store,
        "2026-07-08T00:00:00Z",
        &sources,
        &needs_attention,
    )?;

    let events = store.list_console_events()?;
    let finding = events
        .iter()
        .find(|event| event.event_type() == &EventType::SourceCompletenessFindingObserved);
    assert!(matches!(finding.map(ConsoleEvent::context), Some("source")));
    Ok(())
}

#[test]
fn scenario_5_tui_first_workflow_backfills_presents_and_dispatches_operator_command()
-> Result<(), ConsoleRuntimeError> {
    let mut store = SqliteEventStore::open_in_memory()?;
    let source = ScriptedWorkItemSource::new("9")?;
    let sources: Vec<SourceAdapterRef<'_>> = vec![("orchestrator:fleet", &source)];
    let empty_attention = EmptyNeedsAttentionPort;
    let needs_attention = NeedsAttentionIngest::new(&empty_attention, "fleet");
    let mut runner = CommandingTuiRunner::default();
    let mut port = CompletingDrainPort::default();
    let mut work_item_port = NoWorkItemActionPort;
    let mut config_port = NoArmingPort;
    let decisions_port = NoDecisionsPort;

    let outcome = run_store_backed_tui_session(
        &mut store,
        "2026-07-08T00:00:00Z",
        "operator",
        &mut runner,
        &sources,
        &mut port,
        &mut work_item_port,
        &mut config_port,
        &decisions_port,
        &needs_attention,
    )?;

    assert_eq!(outcome, TuiSessionOutcome::new(2, 2, 1, 1, 5, 0));
    assert_eq!(runner.observed_requested_by, "operator");
    assert_eq!(runner.observed_events, 2);
    assert_eq!(store.list_commands()?[0].status(), "completed");
    assert_eq!(port.observed_requests.len(), 1);
    Ok(())
}

#[test]
fn scenario_8_corrupted_projection_rebuilds_by_replaying_event_log()
-> Result<(), ConsoleRuntimeError> {
    let mut store = SqliteEventStore::open_in_memory()?;
    append_ready_work_item(&mut store)?;
    append_blocked_work_item(&mut store)?;
    let original_events = store.list_console_events()?;
    let original_model = build_tui_model(&original_events, 0);

    let mut rebuilt = SqliteEventStore::open_in_memory()?;
    for event in &original_events {
        rebuilt.append_event(&replayed_append(event))?;
    }
    let rebuilt_events = rebuilt.list_console_events()?;
    let rebuilt_model = build_tui_model(&rebuilt_events, 0);

    assert_eq!(rebuilt_events.len(), original_events.len());
    assert_eq!(rebuilt_model.lane_board(), original_model.lane_board());
    assert_eq!(
        rebuilt_model.attention_items(),
        original_model.attention_items()
    );
    assert_eq!(rebuilt_model.detail(), original_model.detail());
    Ok(())
}

// Scenario 5 exercises the factory-drain path; no work-item command is pending,
// so the work-item port is never invoked. It still must be supplied because the
// session handles both command families.
struct NoWorkItemActionPort;

impl OrchestratorActionPort for NoWorkItemActionPort {
    fn run_action(
        &mut self,
        _request: &OrchestratorActionRequest,
    ) -> Result<OrchestratorActionOutcome, ApplicationError> {
        Ok(OrchestratorActionOutcome::not_wired())
    }
}

struct NoArmingPort;

impl AutonomousModeArmingPort for NoArmingPort {
    fn arm(
        &mut self,
        _request: &AutonomousModeArmingRequest,
    ) -> Result<AutonomousModeArmingOutcome, ApplicationError> {
        Ok(AutonomousModeArmingOutcome::not_wired())
    }
}

struct NoDecisionsPort;

impl AutonomousDecisionsPort for NoDecisionsPort {
    fn read_autonomous_decisions(&self) -> AutonomousAudit {
        AutonomousAudit::default()
    }
}

#[derive(Default)]
struct CompletingDrainPort {
    observed_requests: Vec<FactoryDrainRequest>,
}

impl FactoryDrainPort for CompletingDrainPort {
    fn drain_ready_queue(
        &mut self,
        request: &FactoryDrainRequest,
    ) -> Result<FactoryDrainPortOutcome, ApplicationError> {
        self.observed_requests.push(request.clone());
        Ok(FactoryDrainPortOutcome::completed(1))
    }
}

#[derive(Default)]
struct CommandingTuiRunner {
    observed_events: usize,
    observed_requested_by: String,
}

impl TuiSessionRunner for CommandingTuiRunner {
    fn run_tui(
        &mut self,
        events: &[ConsoleEvent],
        requested_by: &str,
        _effect_sink: &mut dyn TuiRuntimeEffectSink,
    ) -> Result<Vec<TuiRuntimeEffect>, ConsoleRuntimeError> {
        self.observed_events = events.len();
        requested_by.clone_into(&mut self.observed_requested_by);
        Ok(vec![TuiRuntimeEffect::PersistCommand(
            factory_drain_command(),
        )])
    }
}

struct EmptyNeedsAttentionPort;

impl NeedsAttentionSnapshotPort for EmptyNeedsAttentionPort {
    fn read_snapshot(&self) -> NeedsAttentionReadOutcome {
        NeedsAttentionReadOutcome::Observed(Vec::new())
    }
}

struct ScriptedWorkItemSource {
    poll: AdapterPoll,
    observed: std::cell::RefCell<Vec<AdapterPollRequest>>,
}

impl ScriptedWorkItemSource {
    fn new(checkpoint: &str) -> Result<Self, ConsoleRuntimeError> {
        let snapshot = WorkItemSnapshot::new(
            "fleet",
            "work-ready",
            Lane::Ready,
            None,
            "a0",
            "ready",
            AdmissionPolicy::Manual,
            AcceptancePolicy::AiThenHuman,
            7,
        )?;
        Ok(Self {
            poll: AdapterPoll::new(
                checkpoint,
                normalize_work_item_snapshot(&snapshot).events().to_vec(),
            )?,
            observed: std::cell::RefCell::new(Vec::new()),
        })
    }

    fn observed_checkpoint(&self) -> Option<String> {
        self.observed
            .borrow()
            .first()
            .and_then(AdapterPollRequest::checkpoint)
            .map(str::to_owned)
    }

    fn observed_safety_window(&self) -> u64 {
        self.observed
            .borrow()
            .first()
            .map(AdapterPollRequest::safety_window)
            .unwrap_or_default()
    }
}

impl PullSourcePort for ScriptedWorkItemSource {
    fn poll(
        &self,
        request: &AdapterPollRequest,
    ) -> Result<AdapterPoll, console_application::source_adapters::AdapterError> {
        self.observed.borrow_mut().push(request.clone());
        Ok(self.poll.clone())
    }
}

fn factory_drain_command() -> CommandEnvelope {
    CommandEnvelope::new(
        "cmd_drain".to_owned(),
        CommandType::FactoryDrainRequested,
        "fleet:livespec".to_owned(),
        "fleet:livespec:factory.drain_requested:budget=1:parallel=1".to_owned(),
        "operator".to_owned(),
    )
}

fn append_ready_work_item(store: &mut SqliteEventStore) -> Result<(), ConsoleRuntimeError> {
    append_work_item(store, "evt_ready_work", "work-ready", Lane::Ready, None, 1)
}

fn append_blocked_work_item(store: &mut SqliteEventStore) -> Result<(), ConsoleRuntimeError> {
    append_work_item(
        store,
        "evt_blocked_work",
        "work-blocked",
        Lane::Blocked,
        Some(LaneReason::NeedsHuman),
        2,
    )
}

fn append_work_item(
    store: &mut SqliteEventStore,
    event_id: &str,
    work_item_id: &str,
    lane: Lane,
    lane_reason: Option<LaneReason>,
    stream_seq: u64,
) -> Result<(), ConsoleRuntimeError> {
    let payload = format!(
        r#"{{"repo":"fleet","work_item_id":"{work_item_id}","lane":"{}","lane_reason":{},"rank":"a0","status":"{}","source_version":1}}"#,
        lane.label(),
        lane_reason.map_or_else(
            || "null".to_owned(),
            |reason| format!(r#""{}""#, reason.label())
        ),
        lane.label(),
    );
    let event = ConsoleEvent::new(
        event_id.to_owned(),
        1,
        "factory".to_owned(),
        EventType::WorkItemSnapshotObserved,
        "orchestrator".to_owned(),
        format!("fleet:{work_item_id}"),
        stream_seq,
    )
    .with_payload_json(payload.clone());
    store.append_event(&EventAppend::new(
        event,
        format!("fleet:{work_item_id}"),
        "2026-07-08T00:00:00Z".to_owned(),
        "2026-07-08T00:00:00Z".to_owned(),
        None,
        format!("corr_{event_id}"),
        Some(event_id.to_owned()),
        payload,
        "{}".to_owned(),
    ))?;
    Ok(())
}

fn replayed_append(event: &ConsoleEvent) -> EventAppend {
    EventAppend::new(
        event.clone(),
        event.stream_id().to_owned(),
        "2026-07-08T00:01:00Z".to_owned(),
        "2026-07-08T00:01:00Z".to_owned(),
        None,
        format!("replay:{}", event.event_id()),
        Some(format!("replay:{}", event.event_id())),
        event.payload_json().to_owned(),
        "{}".to_owned(),
    )
}
