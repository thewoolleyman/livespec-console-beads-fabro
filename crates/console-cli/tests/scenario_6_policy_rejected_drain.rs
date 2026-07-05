use console_application::{
    ApplicationError, FactoryDrainPort, FactoryDrainPortOutcome, FactoryDrainRequest,
};
use console_domain::{CommandEnvelope, CommandType, EventType};
use console_eventstore::{CommandAppend, SqliteEventStore};
use livespec_console_beads_fabro::{ConsoleRuntimeError, handle_pending_factory_commands};

#[test]
fn scenario_6_policy_rejected_drain_emits_rejection_without_invoking_port()
-> Result<(), ConsoleRuntimeError> {
    let mut store = SqliteEventStore::open_in_memory()?;
    let command = factory_drain_command();
    store.append_command(&CommandAppend::new(
        command,
        "2026-06-30T00:00:00Z".to_owned(),
        Some("fleet:livespec".to_owned()),
        "corr_cmd_drain".to_owned(),
        "{}".to_owned(),
    ))?;
    let mut port = ObservedDrainPort::default();

    let outcomes = handle_pending_factory_commands(&mut store, "2026-06-30T00:01:00Z", &mut port)?;

    let events = store.list_console_events()?;
    let commands = store.list_commands()?;
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].command_status(), "rejected");
    assert_eq!(outcomes[0].appended_event_count(), 1);
    assert_eq!(commands[0].status(), "rejected");
    assert_eq!(port.observed_requests, []);
    assert_eq!(events[0].event_type(), &EventType::CommandRejected);
    assert_eq!(events[0].context(), "command");
    assert_eq!(
        events[0].payload_json(),
        r#"{"reason":"no ready implementation work"}"#
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

#[derive(Default)]
struct ObservedDrainPort {
    observed_requests: Vec<FactoryDrainRequest>,
}

impl FactoryDrainPort for ObservedDrainPort {
    fn drain_ready_queue(
        &mut self,
        request: &FactoryDrainRequest,
    ) -> Result<FactoryDrainPortOutcome, ApplicationError> {
        self.observed_requests.push(request.clone());
        Ok(FactoryDrainPortOutcome::completed(1))
    }
}
