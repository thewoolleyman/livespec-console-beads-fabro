use console_application::{
    ApplicationError, FactoryDrainPort, FactoryDrainPortOutcome, FactoryDrainRequest,
};
use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};
use console_eventstore::{CommandAppend, EventAppend, SqliteEventStore};
use livespec_console_beads_fabro::{ConsoleRuntimeError, handle_pending_factory_commands};

#[test]
fn scenario_7_reconciliation_reconstructs_missing_outcome_after_crash()
-> Result<(), ConsoleRuntimeError> {
    let mut store = SqliteEventStore::open_in_memory()?;
    let command = factory_drain_command();
    store.append_command(&CommandAppend::new(
        command.clone(),
        "2026-06-30T00:00:00Z".to_owned(),
        None,
        "corr_cmd_drain".to_owned(),
        "{}".to_owned(),
    ))?;
    append_command_event(
        &mut store,
        &command,
        EventType::CommandAccepted,
        "accepted",
        1,
    )?;
    append_command_event(
        &mut store,
        &command,
        EventType::FactoryDrainStarted,
        "started",
        2,
    )?;
    let mut adapter = ObservedDrainOutcome {
        outcome: FactoryDrainPortOutcome::completed(2),
        observed_requests: Vec::new(),
    };

    let outcomes =
        handle_pending_factory_commands(&mut store, "2026-06-30T00:01:00Z", &mut adapter)?;

    let events = store.list_console_events()?;
    let commands = store.list_commands()?;
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].command_status(), "completed");
    assert_eq!(outcomes[0].appended_event_count(), 1);
    assert_eq!(commands[0].status(), "completed");
    assert_eq!(adapter.observed_requests.len(), 1);
    assert_eq!(
        adapter.observed_requests[0].aggregate_id(),
        "fleet:livespec"
    );
    assert_eq!(
        events
            .iter()
            .map(ConsoleEvent::event_type)
            .collect::<Vec<_>>(),
        [
            &EventType::CommandAccepted,
            &EventType::FactoryDrainStarted,
            &EventType::FactoryDrainCompleted,
        ]
    );
    Ok(())
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

fn append_command_event(
    store: &mut SqliteEventStore,
    command: &CommandEnvelope,
    event_type: EventType,
    suffix: &str,
    stream_seq: u64,
) -> Result<(), ConsoleRuntimeError> {
    let event_id = format!("evt_{}_{}", command.command_id(), suffix);
    let event = ConsoleEvent::new(
        event_id.clone(),
        1,
        event_context(event_type).to_owned(),
        event_type,
        "console:factory-command-handler".to_owned(),
        command.aggregate_id().to_owned(),
        stream_seq,
    );
    store.append_event(&EventAppend::new(
        event,
        command.aggregate_id().to_owned(),
        "2026-06-30T00:00:00Z".to_owned(),
        "2026-06-30T00:00:00Z".to_owned(),
        Some(command.command_id().to_owned()),
        "corr_cmd_drain".to_owned(),
        Some(event_id),
        "{}".to_owned(),
        "{}".to_owned(),
    ))?;
    Ok(())
}

const fn event_context(event_type: EventType) -> &'static str {
    match event_type {
        EventType::CommandAccepted | EventType::CommandRejected => "command",
        _ => "factory",
    }
}

struct ObservedDrainOutcome {
    outcome: FactoryDrainPortOutcome,
    observed_requests: Vec<FactoryDrainRequest>,
}

impl FactoryDrainPort for ObservedDrainOutcome {
    fn drain_ready_queue(
        &mut self,
        request: &FactoryDrainRequest,
    ) -> Result<FactoryDrainPortOutcome, ApplicationError> {
        self.observed_requests.push(request.clone());
        Ok(self.outcome.clone())
    }
}
