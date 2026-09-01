//! Scenario 30 (gap-d6pnkxbs, gap-3u467mzw, gap-7omomfch, gap-yhqrzmia,
//! gap-5w5tehmy, gap-jzwqyqk6): needs-human terminal as a ledger valve.
//!
//! - The console renders no "fabro attach" for a needs-human work item, even
//!   when a matching `FabroHumanGateObserved` event exists (gap-d6pnkxbs).
//! - The run id is read from `dispatch_fabro_run_id` metadata, not by scanning
//!   `FabroHumanGateObserved` events (gap-3u467mzw).
//! - The orphaned-factory-runs lane is fed exclusively by the orchestrator's
//!   `reconcile-runs --dry-run --json` projection (gap-7omomfch).
//! - The Fabro adapter reads the actual `status_kind` instead of hardcoding
//!   `HumanGate` (gap-yhqrzmia).
//! - Observing a fabro run via `FabroHumanGateObserved` does NOT add it to the
//!   orphaned-factory-runs list (gap-5w5tehmy).
//! - `dispatch_factory` is preserved in the work-item detail and survives the
//!   event payload round-trip (gap-jzwqyqk6).

use console_application::{
    AttentionDetail, build_tui_model,
    source_adapters::{
        AcceptancePolicy, AdmissionPolicy, FabroRunState, Lane, LaneReason, ObservedSource,
        SourceAdapterKind, SourcePayload, WorkItemDetail, WorkItemSnapshot,
        parse_fabro_observation, parse_reconcile_runs_snapshot,
        work_item_snapshot_from_payload_json, work_item_snapshot_payload_json,
    },
};
use console_domain::{ConsoleEvent, EventType};
use livespec_console_beads_fabro::ConsoleRuntimeError;

/// Build a `WorkItemSnapshotObserved` event for a blocked/needs-human item
/// carrying the given dispatch metadata in its detail.
fn blocked_needs_human_event(
    event_id: &str,
    work_item_id: &str,
    dispatch_fabro_run_id: Option<&str>,
    dispatch_factory: Option<&str>,
) -> Result<ConsoleEvent, ConsoleRuntimeError> {
    let snapshot = WorkItemSnapshot::new(
        "console",
        work_item_id,
        Lane::Blocked,
        Some(LaneReason::NeedsHuman),
        "a0",
        "blocked",
        AdmissionPolicy::Manual,
        AcceptancePolicy::AiThenHuman,
        1,
    )?
    .with_detail(WorkItemDetail {
        dispatch_fabro_run_id: dispatch_fabro_run_id.map(str::to_owned),
        dispatch_factory: dispatch_factory.map(str::to_owned),
        ..Default::default()
    });
    Ok(ConsoleEvent::new(
        event_id.to_owned(),
        1,
        "factory".to_owned(),
        EventType::WorkItemSnapshotObserved,
        "orchestrator".to_owned(),
        format!("console:{work_item_id}"),
        1,
    )
    .with_payload_json(work_item_snapshot_payload_json(&snapshot)))
}

/// Build a `FabroHumanGateObserved` event for a given repo + work-item + run.
fn fabro_gate_event(event_id: &str, repo: &str, work_item_id: &str, run_id: &str) -> ConsoleEvent {
    let payload = serde_json::json!({
        "repo": repo,
        "work_item_id": work_item_id,
        "run_id": run_id,
        "state": "human-gate",
        "source_version": 2
    });
    ConsoleEvent::new(
        event_id.to_owned(),
        1,
        "factory".to_owned(),
        EventType::FabroHumanGateObserved,
        "fabro".to_owned(),
        format!("{repo}:{work_item_id}"),
        2,
    )
    .with_payload_json(payload.to_string())
}

/// gap-d6pnkxbs: No attach command for a needs-human work item even when a
/// matching `FabroHumanGateObserved` event is present.
///
/// Currently FAILS (Red): `build_attention_detail` scans `FabroHumanGateObserved`
/// events and synthesizes "fabro attach run-42".
#[test]
fn no_attach_command_for_needs_human_work_item_with_gate_event() -> Result<(), ConsoleRuntimeError>
{
    let events = [
        blocked_needs_human_event("evt_wi", "wi-needs-human", Some("run-42"), None)?,
        fabro_gate_event("evt_gate", "console", "wi-needs-human", "run-42"),
    ];

    let model = build_tui_model(&events, 0);

    assert_eq!(
        model.attention_items().len(),
        1,
        "item must appear in attention list"
    );
    assert!(
        model.detail().is_some(),
        "selected row renders a detail pane"
    );
    assert_eq!(
        model.detail().and_then(AttentionDetail::attach_command),
        None,
        "no attach command for a needs-human work item (gap-d6pnkxbs)"
    );
    Ok(())
}

/// gap-3u467mzw: Run id is read from `dispatch_fabro_run_id` metadata, not by
/// scanning `FabroHumanGateObserved` events.
///
/// Currently FAILS (Red): `fabro_run_id_for_attention` scans events and returns
/// `None` (→ "-") when no `FabroHumanGateObserved` event exists for this item.
#[test]
fn fabro_run_id_from_dispatch_metadata_not_event_scan() -> Result<(), ConsoleRuntimeError> {
    let events = [blocked_needs_human_event(
        "evt_wi",
        "wi-dispatch-meta",
        Some("run-from-meta"),
        None,
    )?];

    let model = build_tui_model(&events, 0);

    assert_eq!(
        model.attention_items().len(),
        1,
        "item must appear in attention list"
    );
    assert!(
        model.detail().is_some(),
        "selected row renders a detail pane"
    );
    assert_eq!(
        model.detail().map(AttentionDetail::fabro_run),
        Some("run-from-meta"),
        "run id must come from dispatch_fabro_run_id metadata (gap-3u467mzw)"
    );
    Ok(())
}

/// gap-yhqrzmia: `parse_fabro_observation` reads the actual `status_kind` from
/// the JSON instead of hardcoding `HumanGate`.
///
/// Currently FAILS (Red): `parse_fabro_observation` always uses
/// `FabroRunState::HumanGate` regardless of what `status_kind` the JSON carries.
#[test]
fn parse_fabro_observation_reads_status_kind_not_hardcoded() -> Result<(), String> {
    let observed_active = ObservedSource::new(
        SourceAdapterKind::Fabro,
        "console",
        r#"{"run_id":"run-active","work_item_id":"wi-1","status_kind":"active"}"#,
    );
    let observed_needs_human = ObservedSource::new(
        SourceAdapterKind::Fabro,
        "console",
        r#"{"run_id":"run-nh","work_item_id":"wi-2","status_kind":"needs-human"}"#,
    );

    let parsed_active = parse_fabro_observation(&observed_active)?;
    let parsed_needs_human = parse_fabro_observation(&observed_needs_human)?;

    let active_state = parsed_active
        .events()
        .iter()
        .find_map(|e| match e.payload() {
            SourcePayload::FabroRunSnapshot(s) => Some(s.state()),
            _ => None,
        })
        .ok_or_else(|| "must produce a FabroRunSnapshot event for active".to_owned())?;
    assert_eq!(
        active_state,
        FabroRunState::Active,
        "status_kind 'active' must map to FabroRunState::Active (gap-yhqrzmia)"
    );

    let nh_state = parsed_needs_human
        .events()
        .iter()
        .find_map(|e| match e.payload() {
            SourcePayload::FabroRunSnapshot(s) => Some(s.state()),
            _ => None,
        })
        .ok_or_else(|| "must produce a FabroRunSnapshot event for needs-human".to_owned())?;
    assert_eq!(
        nh_state,
        FabroRunState::NeedsHuman,
        "status_kind 'needs-human' must map to FabroRunState::NeedsHuman (gap-yhqrzmia)"
    );
    Ok(())
}

/// gap-7omomfch: Orphaned factory runs are populated from reconcile-runs JSON,
/// not inferred from event observations.
///
/// Currently PASSES at Red: `parse_reconcile_runs_snapshot` already
/// deserializes records correctly.
#[test]
fn orphaned_factory_runs_from_reconcile_runs_projection() -> Result<(), String> {
    let json = serde_json::json!([
        {
            "run_id": "run-orphan-1",
            "factory": "factory-beads",
            "status_kind": "done",
            "work_item_id": "wi-orphan-1",
            "work_item_status": "blocked",
            "orphan_reason": "run finished but work-item still blocked",
            "remedy_command": "bd resolve wi-orphan-1"
        }
    ])
    .to_string();

    let runs = parse_reconcile_runs_snapshot(&json)?;

    assert_eq!(
        runs.len(),
        1,
        "one orphaned run must be returned (gap-7omomfch)"
    );
    assert_eq!(runs[0].run_id(), "run-orphan-1");
    assert_eq!(runs[0].factory(), "factory-beads");
    assert_eq!(runs[0].status_kind(), "done");
    assert_eq!(runs[0].work_item_id(), "wi-orphan-1");
    assert_eq!(runs[0].work_item_status(), "blocked");
    assert_eq!(
        runs[0].orphan_reason(),
        "run finished but work-item still blocked"
    );
    assert_eq!(runs[0].remedy_command(), "bd resolve wi-orphan-1");
    Ok(())
}

/// gap-5w5tehmy: Observing a fabro run through `FabroHumanGateObserved` does NOT
/// add it to the orphaned-factory-runs list. Orphan state is only fed by the
/// reconcile-runs projection, never inferred from observations.
///
/// Currently PASSES at Red (`orphaned_factory_runs` always empty from events).
/// Remains required as a guard against future regressions.
#[test]
fn fabro_gate_event_does_not_populate_orphaned_runs() -> Result<(), ConsoleRuntimeError> {
    let events = [
        blocked_needs_human_event("evt_wi", "wi-observed", Some("run-observed"), None)?,
        fabro_gate_event("evt_gate", "console", "wi-observed", "run-observed"),
    ];

    let model = build_tui_model(&events, 0);

    assert!(
        model.orphaned_factory_runs().is_empty(),
        "FabroHumanGateObserved must not add to orphaned_factory_runs (gap-5w5tehmy)"
    );
    Ok(())
}

/// gap-jzwqyqk6: `dispatch_factory` is preserved in `WorkItemDetail` and
/// survives the event payload round-trip through serialization.
///
/// Currently PASSES at Red: the field is declared and serialized correctly.
#[test]
fn dispatch_factory_preserved_in_work_item_detail_round_trip() -> Result<(), ConsoleRuntimeError> {
    let snapshot = WorkItemSnapshot::new(
        "console",
        "wi-factory-meta",
        Lane::Blocked,
        Some(LaneReason::NeedsHuman),
        "a0",
        "blocked",
        AdmissionPolicy::Manual,
        AcceptancePolicy::AiThenHuman,
        1,
    )?
    .with_detail(WorkItemDetail {
        dispatch_fabro_run_id: Some("run-99".to_owned()),
        dispatch_factory: Some("factory-99".to_owned()),
        ..Default::default()
    });

    let payload_json = work_item_snapshot_payload_json(&snapshot);
    let recovered = work_item_snapshot_from_payload_json(&payload_json).ok_or(
        ConsoleRuntimeError::BackingCliResolution("snapshot must round-trip".to_owned()),
    )?;

    assert_eq!(
        recovered.detail().dispatch_fabro_run_id.as_deref(),
        Some("run-99"),
        "dispatch_fabro_run_id must survive event payload round-trip"
    );
    assert_eq!(
        recovered.detail().dispatch_factory.as_deref(),
        Some("factory-99"),
        "dispatch_factory must survive event payload round-trip (gap-jzwqyqk6)"
    );
    Ok(())
}
