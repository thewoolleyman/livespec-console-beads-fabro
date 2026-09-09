//! livespec-console-beads-fabro-mx9u.17 AC2: the header's stale-since rider is
//! dated by the last SUCCESSFUL read of a currently-unavailable source, never
//! by the moment it started failing and never by "now" (the render).
//!
//! This drives the exact composition the composition root performs (`doctor`'s
//! `last_successful_observed_at` feeding `source_staleness`'s
//! `oldest_unavailable_since`) over a fixture with ONE success followed by
//! THREE later failures, proving the timestamp pins to the success and never
//! advances across any of the three not-observed transitions that follow it.

use console_application::doctor::last_successful_observed_at;
use console_application::source_staleness::{SourceStaleness, oldest_unavailable_since};
use console_application::{TuiInteractionState, TuiOverlay, build_tui_model_for_state};
use console_domain::{ConsoleEvent, EventType};

const SOURCE: &str = "dispatcher";
const LAST_SUCCESS_AT: &str = "2026-09-08T13:00:00Z";

fn not_observed_event(event_id: &str) -> ConsoleEvent {
    ConsoleEvent::fixture(
        event_id,
        EventType::SourceNotObservedFindingObserved,
        SOURCE,
    )
}

fn observed_idle_event(event_id: &str) -> ConsoleEvent {
    ConsoleEvent::fixture(event_id, EventType::SourceObservedFindingObserved, SOURCE)
}

#[test]
fn the_stale_since_timestamp_pins_to_the_last_success_across_three_later_failures() {
    // ONE success, then THREE later not-observed transitions -- the source is
    // failing right now, and has been failing for a while, but the fact this
    // derivation must recover is WHEN it last actually worked.
    let success = observed_idle_event("evt:success");
    let failure_1 = not_observed_event("evt:failure_1");
    let failure_2 = not_observed_event("evt:failure_2");
    let failure_3 = not_observed_event("evt:failure_3");

    let events = [
        success.clone(),
        failure_1.clone(),
        failure_2.clone(),
        failure_3.clone(),
    ];
    let events_with_observed_at = [
        (success, LAST_SUCCESS_AT.to_owned()),
        (failure_1, "2026-09-08T14:00:00Z".to_owned()),
        (failure_2, "2026-09-08T15:00:00Z".to_owned()),
        (failure_3, "2026-09-08T16:00:00Z".to_owned()),
    ];

    let last_success = last_successful_observed_at(&events_with_observed_at, &[]);
    assert_eq!(
        last_success.get(SOURCE).map(String::as_str),
        Some(LAST_SUCCESS_AT),
        "expected the last-successful-read to stay pinned to the ONE success, not any of the \
         three later failures"
    );

    let unavailable = vec![SOURCE.to_owned()];
    let staleness = oldest_unavailable_since(&unavailable, &last_success);
    assert_eq!(
        staleness,
        SourceStaleness::Since(LAST_SUCCESS_AT.to_owned()),
        "expected the header-facing staleness fact to name the last success, never a failure \
         moment"
    );

    // And the header rider built from that fact names the SAME timestamp --
    // not the render's own clock, which this derivation never reads at all.
    let state = TuiInteractionState::new(0, TuiOverlay::None).with_source_staleness(staleness);
    let model = build_tui_model_for_state(&events, &state);
    assert!(
        model.header().contains(LAST_SUCCESS_AT),
        "expected the rendered header to carry the last-successful-read timestamp, not a \
         render-time or failure-time stand-in: {}",
        model.header()
    );
}
