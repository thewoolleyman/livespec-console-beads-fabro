//! The Status line reports the terminal outcome of the operator's last command.
//!
//! Regression coverage for the silent-command defect: a valve, move, drain or
//! per-item dispatch that reached a terminal outcome surfaced NOTHING in the
//! TUI. Silent success and silent failure looked identical, and the only way to
//! learn that a command had failed was to open the `SQLite` store and read its
//! `commands` table -- where a failure could still be a bare `failed` beside an
//! empty error payload.
//!
//! These assertions are deliberately about the Status LINE (`footer()`), the
//! surface the operator is already looking at, and they assert the shortcut
//! hints survive beside the message: the hint line stays non-empty and honest
//! while the outcome is shown.

use console_application::{TuiInteractionState, TuiOverlay, TuiView, build_tui_model_for_state};
use console_domain::{ConsoleEvent, EventType};

/// The Lanes overview with no overlay open: an ordinary operator screen whose
/// hint line is non-empty, so an outcome message is seen BESIDE the hints
/// rather than in place of them.
const fn lanes_state() -> TuiInteractionState {
    TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
}

fn event(event_id: &str, event_type: EventType, payload_json: &str) -> ConsoleEvent {
    ConsoleEvent::fixture(event_id, event_type, "console:factory-command-handler")
        .with_payload_json(payload_json.to_owned())
}

fn status_line(events: &[ConsoleEvent]) -> String {
    build_tui_model_for_state(events, &lanes_state())
        .footer()
        .into_owned()
}

#[test]
fn a_drain_that_succeeded_says_so_beside_the_shortcut_hints() {
    let events = [event(
        "evt_drain_completed",
        EventType::FactoryDrainCompleted,
        "{}",
    )];

    let status = status_line(&events);

    assert!(status.contains("drain succeeded"), "{status}");
    // The hint line is still there, non-empty and honest, beside the message.
    assert!(status.contains("enter drill"), "{status}");
}

#[test]
fn a_drain_that_failed_carries_the_cause_its_error_payload_stored() {
    let events = [event(
        "evt_drain_failed",
        EventType::FactoryDrainFailed,
        r#"{"domain_error":"dispatch_refused","summary":"no ready work-item is factory-safe"}"#,
    )];

    let status = status_line(&events);

    assert!(status.contains("drain failed"), "{status}");
    assert!(status.contains("dispatch_refused"), "{status}");
    assert!(
        status.contains("no ready work-item is factory-safe"),
        "{status}"
    );
    assert!(status.contains("enter drill"), "{status}");
}

#[test]
fn a_failure_whose_error_payload_is_empty_says_the_cause_is_absent() {
    // The reported defect exactly: `status: failed` with `error_json: {}`. A
    // bare "failed" is not diagnosable, so the absence is stated rather than
    // left to look like a message that simply had nothing to add.
    let events = [event(
        "evt_drain_failed_silent",
        EventType::FactoryDrainFailed,
        "{}",
    )];

    let status = status_line(&events);

    assert!(status.contains("drain failed"), "{status}");
    assert!(status.contains("cause not reported"), "{status}");
}

#[test]
fn a_failed_valve_names_the_action_and_its_refusal() {
    let events = [event(
        "evt_approve_failed",
        EventType::WorkItemActionFailed,
        r#"{"action_id":"approve:lcbf-k0w","domain_error":"invalid_state","summary":"item is not at pending-approval"}"#,
    )];

    let status = status_line(&events);

    assert!(status.contains("approve:lcbf-k0w failed"), "{status}");
    assert!(status.contains("invalid_state"), "{status}");
    assert!(
        status.contains("item is not at pending-approval"),
        "{status}"
    );
}

#[test]
fn a_move_that_succeeded_names_the_move_it_confirms() {
    let events = [event(
        "evt_move_completed",
        EventType::WorkItemActionCompleted,
        r#"{"action_id":"move-status:lcbf-k0w:ready"}"#,
    )];

    let status = status_line(&events);

    assert!(
        status.contains("move-status:lcbf-k0w:ready succeeded"),
        "{status}"
    );
}

#[test]
fn a_newly_requested_command_retires_the_previous_verdict() {
    // The message is TRANSIENT through the event stream itself: the moment the
    // operator launches the next command, the previous command's verdict stops
    // being reported beside hints that now describe a different action.
    let events = [
        event(
            "evt_drain_completed",
            EventType::FactoryDrainCompleted,
            "{}",
        ),
        event(
            "evt_drain_requested",
            EventType::FactoryDrainRequested,
            "{}",
        ),
    ];

    let status = status_line(&events);

    assert!(!status.contains("succeeded"), "{status}");
    assert_eq!(status, status_line(&[]));
}
