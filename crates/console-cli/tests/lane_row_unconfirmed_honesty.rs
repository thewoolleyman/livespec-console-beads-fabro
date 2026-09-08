//! A lane row the console cannot currently confirm says so
//! (`livespec-console-beads-fabro-v8un`, operator rider [2]).
//!
//! Freshness is repaired by the observation epoch, but a projection can still
//! be behind for a reason no epoch fixes: the orchestrator source could not be
//! READ on the latest poll. The console already knows this — it counts the
//! source in the header's `sources: N unavailable` tally — and then renders
//! every row it fed exactly as if the values were current.
//!
//! Two consequences, and the second is the one that misleads. A row serves
//! last-known `rank` / title with nothing saying the view is behind. And the
//! detail pane renders an ABSENT `acceptance_policy` as
//! `— (not emitted; console assumes ai-then-human)` — a policy READING — when
//! the truth is that the console could not read the item at all. An operator
//! has no way to tell "this item is unarmed" from "I could not read this
//! item's policy", which is exactly how plan 02's R10 walk recorded a stale row
//! as a verified unarmed baseline that was not evidence of anything.
//!
//! Unknown must be distinguishable from unset. That is the whole assertion
//! here, made against the RENDERED surface on both sides of the distinction.

use console_application::source_adapters::{
    AcceptancePolicy, AdmissionPolicy, Lane, WorkItemDetail, WorkItemSnapshot,
    work_item_snapshot_payload_json,
};
use console_application::{
    LaneFocus, TuiInteractionState, TuiOverlay, TuiView, build_tui_model_for_state,
};
use console_domain::{ConsoleEvent, EventType};
use console_tui::render_to_text;
use livespec_console_beads_fabro::ConsoleRuntimeError;

const REPO: &str = "livespec-console-beads-fabro";

/// The console's own name for the orchestrator backing source, which is what
/// the availability tally keys on.
const ORCHESTRATOR: &str = "orchestrator";

/// One genuinely-ingested ready-lane snapshot whose `acceptance_policy` the
/// orchestrator did NOT emit — the shape every field below is read against.
fn ready_snapshot_event(
    event_id: &str,
    work_item_id: &str,
    title: &str,
) -> Result<ConsoleEvent, ConsoleRuntimeError> {
    let snapshot = WorkItemSnapshot::new(
        REPO,
        work_item_id,
        Lane::Ready,
        None,
        "a0",
        "ready",
        AdmissionPolicy::Manual,
        AcceptancePolicy::AiThenHuman,
        1,
    )?
    .with_detail(WorkItemDetail {
        title: Some(title.to_owned()),
        ..WorkItemDetail::default()
    });
    Ok(ConsoleEvent::new(
        event_id.to_owned(),
        1,
        "factory".to_owned(),
        EventType::WorkItemSnapshotObserved,
        ORCHESTRATOR.to_owned(),
        format!("repo:{REPO}"),
        1,
    )
    .with_payload_json(work_item_snapshot_payload_json(&snapshot)))
}

/// The honest marker the adapter appends when a poll could not read a source.
/// Appended AFTER the snapshot, so it is that source's MOST RECENT observation.
fn source_not_observed_event(event_id: &str) -> ConsoleEvent {
    ConsoleEvent::new(
        event_id.to_owned(),
        1,
        "source".to_owned(),
        EventType::SourceNotObservedFindingObserved,
        ORCHESTRATOR.to_owned(),
        format!("repo:{REPO}"),
        2,
    )
    .with_payload_json(
        serde_json::json!({
            "repo": REPO,
            "source": ORCHESTRATOR,
            "reason": "source command exited non-zero",
        })
        .to_string(),
    )
}

/// The rendered screen for `overlay`, wide enough that no row is truncated.
fn rendered(events: &[ConsoleEvent], overlay: TuiOverlay) -> Result<String, ConsoleRuntimeError> {
    let state = TuiInteractionState::for_view(TuiView::Lanes, 0, overlay)
        .with_lane_focus(LaneFocus::Lane(Lane::Ready))
        .with_selected_lane_item_index(0);
    let model = build_tui_model_for_state(events, &state);
    render_to_text(&model, 160, 40)
        .map_err(|error| ConsoleRuntimeError::tui_runtime_failed(format!("{error:?}")))
}

/// The drilled-in ready lane.
fn rendered_lane(events: &[ConsoleEvent]) -> Result<String, ConsoleRuntimeError> {
    rendered(events, TuiOverlay::None)
}

/// The selected row's detail pane.
fn rendered_detail(
    events: &[ConsoleEvent],
    work_item_id: &str,
) -> Result<String, ConsoleRuntimeError> {
    rendered(
        events,
        TuiOverlay::WorkItemDetail {
            work_item_id: work_item_id.to_owned(),
            scroll: 0,
        },
    )
}

#[test]
fn a_row_whose_source_was_read_reports_an_absent_policy_as_unset() -> Result<(), ConsoleRuntimeError>
{
    // The CONTROL. The orchestrator was read successfully, and it emitted no
    // acceptance_policy. That is genuinely UNSET, and the console's existing
    // wording -- its own assumption, labelled as its own -- is correct here.
    let events = [ready_snapshot_event(
        "evt_ok",
        "wi-confirmed",
        "A confirmed row",
    )?];

    let lane = rendered_lane(&events)?;
    assert!(
        lane.contains("wi-confirmed  rank a0  [ready]"),
        "the row renders its rank and status: {lane}"
    );
    assert!(
        !lane.contains("unconfirmed"),
        "a row the console CAN confirm must not be marked unconfirmed: {lane}"
    );

    let detail = rendered_detail(&events, "wi-confirmed")?;
    assert!(
        detail.contains("not emitted; console assumes ai-then-human"),
        "an absent policy on a READ row stays an honest unset: {detail}"
    );
    Ok(())
}

#[test]
fn a_row_whose_source_could_not_be_read_says_so_and_reports_no_policy()
-> Result<(), ConsoleRuntimeError> {
    // Same row, same absent policy -- but the orchestrator's MOST RECENT
    // observation failed, so every value here is last-known rather than
    // current.
    let events = [
        ready_snapshot_event("evt_stale", "wi-unconfirmed", "An unconfirmed row")?,
        source_not_observed_event("evt_not_observed"),
    ];

    let lane = rendered_lane(&events)?;
    assert!(
        lane.contains("wi-unconfirmed  rank a0  [ready]  (unconfirmed)"),
        "a row the console could not confirm must SAY the view is behind: {lane}"
    );

    let detail = rendered_detail(&events, "wi-unconfirmed")?;
    // The console must not print its own default as though it had read one.
    assert!(
        !detail.contains("console assumes"),
        "an unreadable policy must not render as an assumed reading: {detail}"
    );
    assert!(
        detail.contains("not read; the orchestrator source was not observed on the latest poll"),
        "the policy line must say it could not be READ: {detail}"
    );
    Ok(())
}
