//! The header's stale-since rider (livespec-console-beads-fabro-mx9u.17).
//!
//! `mx9u.11` fixed a systemic header freeze, but left the CONDITION untouched:
//! a source unavailable since some past moment keeps having its last good
//! snapshot rendered with nothing on screen saying it is no longer current.
//! These gates pin the fix's presentation contract: the rider names WHEN the
//! picture went stale (never a fabricated "since never" when that moment is
//! genuinely unknown), clears the instant the source is observed again, and
//! survives the header's own narrow-width field dropping.

use console_application::source_staleness::SourceStaleness;
use console_application::{TuiInteractionState, TuiOverlay, build_tui_model_for_state};
use console_domain::{ConsoleEvent, EventType};
use console_tui::render_to_text;

/// A fixture event marking `source` currently unavailable, so the header's own
/// `event sources: N unavailable (...)` tell is present alongside the rider
/// under test -- the rider's own condition (a source unavailable) is real,
/// not vacuous.
fn not_observed_event(source: &str) -> ConsoleEvent {
    ConsoleEvent::fixture(
        &format!("evt:{source}:not_observed"),
        EventType::SourceNotObservedFindingObserved,
        source,
    )
}

fn state_with_staleness(staleness: SourceStaleness) -> TuiInteractionState {
    TuiInteractionState::new(0, TuiOverlay::None)
        .with_selected_repo("livespec-console-beads-fabro".to_owned())
        .with_source_staleness(staleness)
}

/// AC1 (known timestamp): a source unavailable since a known last successful
/// read renders the rider naming that timestamp.
#[test]
fn a_known_last_successful_read_is_named_in_the_header() {
    let events = [not_observed_event("dispatcher")];
    let state = state_with_staleness(SourceStaleness::Since("2026-09-08T14:05:14Z".to_owned()));
    let model = build_tui_model_for_state(&events, &state);
    let rendered = render_to_text(&model, 200, 40).unwrap_or_default();
    assert!(
        rendered.contains("STALE since 2026-09-08T14:05:14Z"),
        "expected the header to name the last successful read: {rendered}"
    );
}

/// AC1 (do not present unknown as known): when the last successful read is
/// genuinely unknown, the rider says so in words -- never a fabricated
/// timestamp, and never the bare word "never" standing in for a measurement.
#[test]
fn a_never_observed_source_is_named_honestly_not_as_a_fabricated_timestamp() {
    let events = [not_observed_event("reconcile-runs")];
    let state = state_with_staleness(SourceStaleness::NeverObserved);
    let model = build_tui_model_for_state(&events, &state);
    let rendered = render_to_text(&model, 200, 40).unwrap_or_default();
    assert!(
        rendered.contains("STALE, never observed"),
        "expected an honest never-observed rider, not a fabricated timestamp: {rendered}"
    );
}

/// AC1 (clears on recovery): with the source observed again, the rider is
/// absent -- a healthy console must never carry a leftover stale claim.
#[test]
fn the_rider_is_absent_once_every_source_is_observed_again() {
    let state = state_with_staleness(SourceStaleness::AllObserved);
    let model = build_tui_model_for_state(&[], &state);
    let rendered = render_to_text(&model, 200, 40).unwrap_or_default();
    assert!(
        !rendered.contains("STALE"),
        "expected no stale rider once every source is observed: {rendered}"
    );
}

/// AC3: at both widths the rubric grades, the rider survives the header's
/// own narrow-width field dropping, in a CROWDED fixture (multiple
/// unavailable sources plus a nonzero attention count competing for the same
/// budget) -- the exact shape measured on the real store
/// (livespec-console-beads-fabro-pzbdbo.25/mx9u.11): a stale count must never
/// be presented as current merely because the source-health tell itself had
/// to shrink to make room.
#[test]
fn the_rider_survives_the_crowded_narrow_width_header_at_105_and_159_columns() {
    let events = [
        not_observed_event("dispatcher"),
        not_observed_event("livespec"),
        not_observed_event("reconcile-runs"),
        ConsoleEvent::fixture(
            "evt:attention:1",
            EventType::WorkItemSnapshotObserved,
            "orchestrator",
        )
        .with_payload_json(
            r#"{"repo":"livespec-console-beads-fabro","work_item_id":"livespec-console-beads-fabro-a1","lane":"blocked","lane_reason":"needs-human","rank":"a1","status":"blocked","source_version":1}"#
                .to_owned(),
        ),
    ];
    let state = state_with_staleness(SourceStaleness::Since("2026-09-08T14:05:14Z".to_owned()));
    let model = build_tui_model_for_state(&events, &state);

    for width in [105_u16, 159_u16] {
        let rendered = render_to_text(&model, width, 40).unwrap_or_default();
        assert!(
            rendered.contains("STALE since"),
            "expected the stale rider to survive a crowded header at {width} columns: {rendered}"
        );
    }
}
