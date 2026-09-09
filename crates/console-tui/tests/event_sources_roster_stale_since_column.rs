//! The Event sources roster's stale-since column (livespec-console-beads-
//! fabro-mx9u.17, pzbdbo.29's roster).
//!
//! An unavailable row's Detail carries a SECOND line -- the not-observed
//! reason, then "last successful read: <...>" -- and the two must render as
//! genuinely separate visual lines, not run together on one. Measured while
//! dogfooding this item: `ViewSummaryItem::detail` flows into a single
//! `ratatui::text::Line`, which does not treat an embedded `\n` as a line
//! break, so a naive `"{reason}\n{stale_since}"` join rendered as
//! `"...(os error 2)last successful read: never observed"` with no space or
//! break at all. `summary_detail_lines` now splits a detail string on `\n`
//! into one `Line` per fragment; these gates pin that the roster's two facts
//! land on two rows.

use console_application::{
    EventsFocus, TuiInteractionState, TuiOverlay, TuiView, build_tui_model_for_state,
};
use console_domain::{ConsoleEvent, EventType};
use console_tui::render_to_text;

fn not_observed_event(source: &str, reason: &str) -> ConsoleEvent {
    ConsoleEvent::fixture(
        &format!("evt:{source}:not_observed"),
        EventType::SourceNotObservedFindingObserved,
        source,
    )
    .with_payload_json(format!(
        r#"{{"reason":"{reason}","repo":"livespec-console-beads-fabro"}}"#
    ))
}

fn roster_state(last_success: std::collections::BTreeMap<String, String>) -> TuiInteractionState {
    TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None)
        .with_events_focus(EventsFocus::EventSources)
        .with_source_last_success(last_success)
}

/// AC (mx9u.17): the reason and the stale-since fact render as two SEPARATE
/// lines in the Detail pane, over a KNOWN last-successful-read.
#[test]
fn the_reason_and_a_known_stale_since_render_on_separate_lines() {
    let events = [not_observed_event(
        "dispatcher",
        "dispatcher binary not found",
    )];
    let mut last_success = std::collections::BTreeMap::new();
    last_success.insert("dispatcher".to_owned(), "2026-09-08T14:05:14Z".to_owned());
    let model = build_tui_model_for_state(&events, &roster_state(last_success));

    let rendered = render_to_text(&model, 200, 40).unwrap_or_default();
    // `rendered` is the WHOLE screen buffer -- one text row per terminal row,
    // spanning all three panes side by side -- so a "line" here is checked by
    // substring, not equality, and separation is proven by the reason's OWN
    // row NOT also carrying the stale-since fact.
    let reason_row = rendered
        .lines()
        .find(|line| line.contains("dispatcher binary not found"));
    assert!(
        reason_row.is_some(),
        "expected the reason to appear: {rendered}"
    );
    assert!(
        !reason_row
            .unwrap_or_default()
            .contains("last successful read"),
        "the reason and the stale-since fact must never run together on the same row: {rendered}"
    );
    assert!(
        rendered.contains("last successful read: 2026-09-08T14:05:14Z"),
        "expected the stale-since fact to appear on its own row: {rendered}"
    );
}

/// AC (mx9u.17): the SAME line separation holds for the honest
/// never-observed case.
#[test]
fn the_reason_and_a_never_observed_stale_since_render_on_separate_lines() {
    let events = [not_observed_event(
        "reconcile-runs",
        "source command exited non-zero",
    )];
    let model =
        build_tui_model_for_state(&events, &roster_state(std::collections::BTreeMap::new()));

    let rendered = render_to_text(&model, 200, 40).unwrap_or_default();
    let reason_row = rendered
        .lines()
        .find(|line| line.contains("source command exited non-zero"));
    assert!(
        reason_row.is_some(),
        "expected the reason to appear: {rendered}"
    );
    assert!(
        !reason_row
            .unwrap_or_default()
            .contains("last successful read"),
        "the reason and the stale-since fact must never run together on the same row: {rendered}"
    );
    assert!(
        rendered.contains("last successful read: never observed"),
        "expected the never-observed fact to appear on its own row: {rendered}"
    );
}
