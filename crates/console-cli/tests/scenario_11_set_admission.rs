//! Scenario 11 -- A policy edit never moves an item between states
//! (SPECIFICATION/scenarios.md). Set-admission is the admission policy dial; its
//! `policy` rides the command payload (`{"policy": ...}`) and lands in the third
//! segment of the `set-admission:<work-item-id>:<policy>` action id. Like the
//! valves it rides the shared orchestrator-action port and the shared `work_item`
//! outcome family, but -- in contrast to reject -- a policy edit is a pure dial
//! change: the console never writes the ledger and the item's lifecycle state
//! (its lane) is unchanged by the edit.

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

/// The whole set-admission scene: a work-item whose stored `admission_policy` is
/// manual is re-policied to auto. The console persists the
/// `work_item.set_admission_requested` command carrying the policy, routes
/// `set-admission:<work-item-id>:auto` through the shared port, appends the
/// outcome events, and never writes the ledger -- so the item's lifecycle state
/// (its lane) is unchanged by the edit.
#[test]
fn scenario_11_set_admission_never_moves_an_item_between_states() -> Result<(), ConsoleRuntimeError>
{
    let mut store = SqliteEventStore::open_in_memory()?;

    // Given a work-item whose stored admission_policy is manual, sitting in
    // Backlog awaiting admission.
    append_work_item_snapshot(&mut store, "evt_wi_v1", AdmissionPolicy::Manual, 1)?;
    let before = store.list_console_events()?;
    assert_eq!(lane_ids(&before, Lane::Backlog), ["con-set-admission-1"]);

    // When the operator invokes set-admission with policy auto: the console
    // persists a `work_item.set_admission_requested` command carrying the policy
    // in its payload.
    store.append_command(&CommandAppend::new(
        set_admission_command(),
        "2026-07-10T00:00:00Z".to_owned(),
        Some("con-set-admission-1".to_owned()),
        "corr_cmd_set_admission".to_owned(),
        r#"{"policy":"auto"}"#.to_owned(),
    ))?;
    let mut port = RecordingActionPort::default();

    let outcomes =
        handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port)?;

    // Then the command is accepted, and the console invokes the orchestrator's
    // published action surface with `set-admission:<work-item-id>:auto`.
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].command_status(), "completed");
    assert_eq!(
        port.observed_action_ids,
        ["set-admission:con-set-admission-1:auto"]
    );

    // And the persisted command carries the policy, resolved to "completed".
    let commands = store.list_commands()?;
    assert_eq!(commands.len(), 1);
    assert_eq!(
        commands[0].command_type(),
        "work_item.set_admission_requested"
    );
    assert_eq!(commands[0].payload_json(), r#"{"policy":"auto"}"#);
    assert_eq!(commands[0].status(), "completed");

    // And appends the outcome events keyed by the set-admission action-id on the
    // shared work_item outcome family.
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
            r#"{"action_id":"set-admission:con-set-admission-1:auto"}"#
        );
    }

    // And the item's lifecycle state is unchanged: unlike reject (which lets the
    // orchestrator move the item on a subsequent poll), a policy edit is a pure
    // dial change. The console appended only its own action events and no new
    // work-item snapshot, so the lane projection is identical to before -- the
    // item is still in Backlog and appears in no other lane.
    assert_eq!(lane_ids(&events, Lane::Backlog), ["con-set-admission-1"]);
    for lane in [
        Lane::PendingApproval,
        Lane::Ready,
        Lane::Active,
        Lane::Acceptance,
        Lane::Blocked,
        Lane::Done,
    ] {
        assert_eq!(lane_ids(&events, lane), [] as [&str; 0]);
    }
    // No work-item snapshot event was appended by the edit: the only observed
    // snapshot remains the pre-edit one, so nothing re-laned the item.
    let snapshot_events = events
        .iter()
        .filter(|event| event.event_type() == &EventType::WorkItemSnapshotObserved)
        .count();
    assert_eq!(snapshot_events, 1);
    Ok(())
}

fn set_admission_command() -> CommandEnvelope {
    CommandEnvelope::new(
        "cmd_set_admission".to_owned(),
        CommandType::WorkItemSetAdmissionRequested,
        "con-set-admission-1".to_owned(),
        "con-set-admission-1:work_item.set_admission_requested".to_owned(),
        "operator".to_owned(),
    )
}

fn append_work_item_snapshot(
    store: &mut SqliteEventStore,
    event_id: &str,
    admission_policy: AdmissionPolicy,
    source_version: u64,
) -> Result<(), ConsoleRuntimeError> {
    let snapshot = WorkItemSnapshot::new(
        "livespec-console-beads-fabro",
        "con-set-admission-1",
        Lane::Backlog,
        None,
        "a0",
        Lane::Backlog.label(),
        admission_policy,
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
        "livespec-console-beads-fabro:con-set-admission-1".to_owned(),
        source_version,
    )
    .with_payload_json(payload.clone());
    store.append_event(&EventAppend::new(
        event,
        "livespec-console-beads-fabro:con-set-admission-1".to_owned(),
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
