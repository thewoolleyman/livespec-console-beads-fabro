//! Scenario 11 -- Approve routes through the orchestrator's published action
//! surface (SPECIFICATION/scenarios.md). The approve valve is the first rider
//! on the shared orchestrator-action port every work-item command rides.

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

/// The whole approve scene: a `pending-approval` work-item is approved, the
/// console persists the command, routes `approve:<work-item-id>` through the
/// port, appends the outcome events, and later observes the lane change on a
/// subsequent work-items poll (the orchestrator owns the ledger write, so the
/// console never fabricates the transition).
#[test]
fn scenario_11_approve_routes_through_the_orchestrators_published_action_surface()
-> Result<(), ConsoleRuntimeError> {
    let mut store = SqliteEventStore::open_in_memory()?;

    // Given a `pending-approval` work-item whose effective admission_policy is
    // manual, shown in needs-attention.
    append_work_item_snapshot(&mut store, "evt_wi_v1", Lane::PendingApproval, 1)?;
    let before = store.list_console_events()?;
    assert_eq!(lane_ids(&before, Lane::PendingApproval), ["con-approve-1"]);
    assert_eq!(lane_ids(&before, Lane::Ready), [] as [&str; 0]);

    // When the operator invokes Approve on it: the console persists a
    // `work_item.approve_requested` command.
    store.append_command(&CommandAppend::new(
        approve_command(),
        "2026-07-10T00:00:00Z".to_owned(),
        Some("con-approve-1".to_owned()),
        "corr_cmd_approve".to_owned(),
        "{}".to_owned(),
    ))?;
    let mut port = RecordingActionPort::default();

    let outcomes =
        handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port)?;

    // Then the command is accepted, and the port was invoked with
    // `approve:<work-item-id>` through the orchestrator's published surface.
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].command_status(), "completed");
    assert_eq!(port.observed_action_ids, ["approve:con-approve-1"]);

    // And appends the outcome events from the orchestrator result, keyed by
    // action-id on the shared work_item outcome family.
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
            r#"{"action_id":"approve:con-approve-1"}"#
        );
    }
    assert_eq!(store.list_commands()?[0].status(), "completed");

    // And observes the item's lane change on a subsequent work-items poll: the
    // orchestrator moved it `pending-approval -> ready`, and the console
    // observes that on the next poll rather than fabricating the transition.
    append_work_item_snapshot(&mut store, "evt_wi_v2", Lane::Ready, 2)?;
    let after = store.list_console_events()?;
    assert_eq!(lane_ids(&after, Lane::Ready), ["con-approve-1"]);
    assert_eq!(lane_ids(&after, Lane::PendingApproval), [] as [&str; 0]);
    Ok(())
}

fn approve_command() -> CommandEnvelope {
    CommandEnvelope::new(
        "cmd_approve".to_owned(),
        CommandType::WorkItemApproveRequested,
        "con-approve-1".to_owned(),
        "con-approve-1:work_item.approve_requested".to_owned(),
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
        "con-approve-1",
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
        "livespec-console-beads-fabro:con-approve-1".to_owned(),
        source_version,
    )
    .with_payload_json(payload.clone());
    store.append_event(&EventAppend::new(
        event,
        "livespec-console-beads-fabro:con-approve-1".to_owned(),
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
