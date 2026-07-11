//! Scenario 11 -- Reject with mode regroom maps onto the reject action id
//! (SPECIFICATION/scenarios.md). Reject is the first work-item command carrying
//! a payload beyond the aggregate id; its `mode` rides the command payload and
//! lands in the third segment of the `reject:<work-item-id>:<mode>` action id.
//! Like approve, reject rides the shared orchestrator-action port and the shared
//! `work_item` outcome family, and never writes the ledger directly.

use console_application::{
    ApplicationResult, OrchestratorActionOutcome, OrchestratorActionPort,
    OrchestratorActionRequest, project_lane_board,
    source_adapters::{
        AcceptancePolicy, AdmissionPolicy, Lane, WorkItemSnapshot, work_item_snapshot_payload_json,
    },
};
use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};
use console_eventstore::{CommandAppend, EventAppend, SqliteEventStore};
use livespec_console_beads_fabro::{ConsoleRuntimeError, handle_pending_work_item_commands};

/// The whole reject-regroom scene: an `acceptance` work-item is rejected with
/// mode regroom, the console persists the `work_item.reject_requested` command
/// carrying the mode, routes `reject:<work-item-id>:regroom` through the shared
/// port, appends the outcome events, and never writes the ledger directly (the
/// orchestrator owns the lane transition, observed on a subsequent poll).
#[test]
fn scenario_11_reject_with_mode_regroom_maps_onto_the_reject_action_id()
-> Result<(), ConsoleRuntimeError> {
    let mut store = SqliteEventStore::open_in_memory()?;

    // Given an `acceptance` work-item the operator judges wrongly scoped.
    append_work_item_snapshot(&mut store, "evt_wi_v1", Lane::Acceptance, 1)?;
    let before = store.list_console_events()?;
    assert_eq!(lane_ids(&before, Lane::Acceptance), ["con-reject-1"]);

    // When the operator invokes Reject with mode regroom: the console persists a
    // `work_item.reject_requested` command carrying mode regroom in its payload.
    store.append_command(&CommandAppend::new(
        reject_command(),
        "2026-07-10T00:00:00Z".to_owned(),
        Some("con-reject-1".to_owned()),
        "corr_cmd_reject".to_owned(),
        r#"{"mode":"regroom"}"#.to_owned(),
    ))?;
    let mut port = RecordingActionPort::default();

    let outcomes =
        handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port)?;

    // Then the command is accepted, and the console invokes the orchestrator's
    // published action surface with `reject:<work-item-id>:regroom`.
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].command_status(), "completed");
    assert_eq!(port.observed_action_ids, ["reject:con-reject-1:regroom"]);

    // And the persisted command carries mode regroom, resolved to "completed".
    let commands = store.list_commands()?;
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].command_type(), "work_item.reject_requested");
    assert_eq!(commands[0].payload_json(), r#"{"mode":"regroom"}"#);
    assert_eq!(commands[0].status(), "completed");

    // And appends the outcome events keyed by the reject action-id on the shared
    // work_item outcome family.
    let events = store.list_console_events()?;
    let handler_events: Vec<&ConsoleEvent> = events
        .iter()
        .filter(|event| event.source() == "console:work-item-command-handler")
        .collect();
    assert_eq!(
        handler_events
            .iter()
            .map(|event| event.event_type())
            .collect::<Vec<_>>(),
        [
            &EventType::CommandAccepted,
            &EventType::WorkItemActionStarted,
            &EventType::WorkItemActionCompleted,
        ]
    );
    for event in &handler_events {
        assert_eq!(
            event.payload_json(),
            r#"{"action_id":"reject:con-reject-1:regroom"}"#
        );
    }

    // And never writes the ledger directly: the console only routed the action
    // through the port and appended its own events; the item's lane is unchanged
    // until the orchestrator moves it and the console observes that on a later
    // poll (here: regroom moved it out of `acceptance`).
    assert_eq!(lane_ids(&events, Lane::Acceptance), ["con-reject-1"]);
    append_work_item_snapshot(&mut store, "evt_wi_v2", Lane::Backlog, 2)?;
    let after = store.list_console_events()?;
    assert_eq!(lane_ids(&after, Lane::Backlog), ["con-reject-1"]);
    assert_eq!(lane_ids(&after, Lane::Acceptance), [] as [&str; 0]);
    Ok(())
}

fn reject_command() -> CommandEnvelope {
    CommandEnvelope::new(
        "cmd_reject".to_owned(),
        CommandType::WorkItemRejectRequested,
        "con-reject-1".to_owned(),
        "con-reject-1:work_item.reject_requested".to_owned(),
        "operator".to_owned(),
    )
}

fn append_work_item_snapshot(
    store: &mut SqliteEventStore,
    event_id: &str,
    lane: Lane,
    source_version: u64,
) -> Result<(), ConsoleRuntimeError> {
    let snapshot = WorkItemSnapshot::new(
        "livespec-console-beads-fabro",
        "con-reject-1",
        lane,
        None,
        "a0",
        lane.label(),
        AdmissionPolicy::Manual,
        AcceptancePolicy::AiThenHuman,
        source_version,
    )?;
    let payload = work_item_snapshot_payload_json(&snapshot);
    let event = ConsoleEvent::new(
        event_id.to_owned(),
        1,
        "factory".to_owned(),
        EventType::WorkItemSnapshotObserved,
        "orchestrator".to_owned(),
        "livespec-console-beads-fabro:con-reject-1".to_owned(),
        source_version,
    )
    .with_payload_json(payload.clone());
    store.append_event(&EventAppend::new(
        event,
        "livespec-console-beads-fabro:con-reject-1".to_owned(),
        "2026-07-10T00:00:00Z".to_owned(),
        "2026-07-10T00:00:00Z".to_owned(),
        None,
        format!("corr_{event_id}"),
        Some(event_id.to_owned()),
        payload,
        "{}".to_owned(),
    ))?;
    Ok(())
}

fn lane_ids(events: &[ConsoleEvent], lane: Lane) -> Vec<String> {
    project_lane_board(events)
        .column(lane)
        .map(|column| {
            column
                .items()
                .iter()
                .map(|item| item.work_item_id().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Default)]
struct RecordingActionPort {
    observed_action_ids: Vec<String>,
}

impl OrchestratorActionPort for RecordingActionPort {
    fn run_action(
        &mut self,
        request: &OrchestratorActionRequest,
    ) -> ApplicationResult<OrchestratorActionOutcome> {
        self.observed_action_ids
            .push(request.action_id().to_owned());
        Ok(OrchestratorActionOutcome::completed())
    }
}
