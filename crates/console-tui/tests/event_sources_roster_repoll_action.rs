//! The Event sources roster's per-row action surface
//! (livespec-console-beads-fabro-mx9u.20.3).
//!
//! The roster named each source, its health, the verbatim cause of its latest
//! failed poll and its last-successful-read -- and then ended the operator's
//! journey at a true sentence they could not act on. They read the cause and
//! left the console to hand-run a command the console already knows.
//!
//! An unavailable row now ADVERTISES a pressable action the same way every
//! other pressable row in this console does -- the `[enter <verb>]` affordance
//! the Attention group row carries -- and a healthy row advertises nothing,
//! because there is nothing about it to determine. These gates pin both
//! directions at the rendered screen, which is where the operator reads them.

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

fn observed_event(source: &str) -> ConsoleEvent {
    ConsoleEvent::fixture(
        &format!("evt:{source}:observed"),
        EventType::SourceObservedFindingObserved,
        source,
    )
    .with_payload_json(r#"{"repo":"livespec-console-beads-fabro"}"#.to_owned())
}

const fn roster_state() -> TuiInteractionState {
    TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None)
        .with_events_focus(EventsFocus::EventSources)
}

/// AC1/AC3: an unavailable row offers a pressable re-poll, advertised on the
/// row itself in the console's existing `[enter <verb>]` style.
#[test]
fn an_unavailable_row_advertises_the_re_poll_action_on_the_row() {
    let events = [not_observed_event(
        "livespec",
        "livespec: No such file or directory (os error 2)",
    )];
    let model = build_tui_model_for_state(&events, &roster_state());

    let rendered = render_to_text(&model, 200, 40).unwrap_or_default();
    let row = rendered
        .lines()
        .find(|line| line.contains("livespec — unavailable"));
    assert!(
        row.is_some_and(|line| line.contains("[enter re-poll]")),
        "expected the unavailable row to advertise its re-poll action: {rendered}"
    );
}

/// AC5: the offer reaches the row whose last successful read is NEVER
/// OBSERVED too -- currently the least actionable row in the roster, and the
/// one most likely to be genuinely broken.
#[test]
fn a_never_observed_row_advertises_the_re_poll_action_too() {
    let events = [not_observed_event(
        "reconcile-runs",
        "source command exited non-zero",
    )];
    let model = build_tui_model_for_state(&events, &roster_state());

    let rendered = render_to_text(&model, 200, 40).unwrap_or_default();
    assert!(
        rendered.contains("last successful read: never observed"),
        "the fixture must be the never-observed case: {rendered}"
    );
    let row = rendered
        .lines()
        .find(|line| line.contains("reconcile-runs — unavailable"));
    assert!(
        row.is_some_and(|line| line.contains("[enter re-poll]")),
        "expected the never-observed row to advertise its re-poll action: {rendered}"
    );
}

/// AC2: the offer is DERIVED from the row's own facts. A healthy source has
/// nothing to determine, so it advertises nothing rather than a placeholder
/// key that would fail when pressed.
#[test]
fn a_healthy_row_advertises_no_action_at_all() {
    let events = [observed_event("github")];
    let model = build_tui_model_for_state(&events, &roster_state());

    let rendered = render_to_text(&model, 200, 40).unwrap_or_default();
    let row = rendered
        .lines()
        .find(|line| line.contains("github — healthy"));
    assert!(
        row.is_some(),
        "expected the healthy row to render: {rendered}"
    );
    assert!(
        !row.unwrap_or_default().contains("re-poll"),
        "a healthy row must offer nothing to press: {rendered}"
    );
}
