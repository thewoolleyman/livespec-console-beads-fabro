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
//!
//! The second half of the file is the WIDTH contract for the same line
//! (`livespec-console-beads-fabro-mx9u.1`). A verdict the band has no room to
//! draw is as silent as a verdict never composed, and the 2026-09-08 dogfood
//! passes measured exactly that: at 105 columns the drilled ready lane drew
//! four rarely-used policy dials while the way back out of the lane and the
//! whole `last command: dispatch item failed — cause not reported` were gone.

use console_application::source_adapters::Lane;
use console_application::{
    LaneFocus, TuiInteractionState, TuiOverlay, TuiView, build_tui_model_for_state,
};
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

/// The policy dials the drilled ready lane offers: the class the dogfood
/// passes measured surviving while the way back out and the verdict were gone.
const POLICY_DIALS: [&str; 4] = [
    "g merge cap",
    "f fix cap",
    "n set-acceptance",
    "k rework cap",
];

/// How the operator moves over the lane, opens a row, and leaves it again.
const WAY_AROUND: [&str; 3] = ["up/down move", "enter item", "esc lane list"];

/// One `ready` work-item on the console repo's lane board.
fn ready_lane_event(event_id: &str, work_item_id: &str, rank: &str) -> ConsoleEvent {
    let payload = format!(
        r#"{{"repo":"console","work_item_id":"{work_item_id}","lane":"ready","lane_reason":null,"rank":"{rank}","status":"ready","detail":{{"title":"Fixture"}},"source_version":1}}"#
    );
    ConsoleEvent::fixture(
        event_id,
        EventType::WorkItemSnapshotObserved,
        "orchestrator",
    )
    .with_payload_json(payload)
}

/// The dogfooded screen: a TWO-row drilled-in `ready` lane with the first row
/// selected -- so `up/down move` is honestly on offer -- and a per-item
/// dispatch that has just come back failed with a payload naming no cause.
fn drilled_ready_lane_events() -> Vec<ConsoleEvent> {
    vec![
        ready_lane_event("evt_ready_a", "console-ready-a", "a0"),
        ready_lane_event("evt_ready_b", "console-ready-b", "a1"),
        event(
            "evt_dispatch_item_failed",
            EventType::FactoryDispatchItemFailed,
            "{}",
        ),
    ]
}

/// Whether the line still reports the dispatch verdict, in EITHER the full
/// form a band with room draws or the short form a narrow band abbreviates to.
fn reports_the_verdict(line: &str) -> bool {
    line.contains("last command: dispatch item failed") || line.contains("last: dispatch failed")
}

/// That screen's Status line, fitted to a `width`-column band.
fn drilled_ready_lane_line(width: usize) -> String {
    let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
        .with_lane_focus(LaneFocus::Lane(Lane::Ready))
        .with_selected_lane_item_index(0);
    build_tui_model_for_state(&drilled_ready_lane_events(), &state).footer_line(width)
}

#[test]
fn the_dogfooded_widths_keep_the_way_around_the_lane_and_drop_the_dials() {
    // The two panes the report measured. At neither width does the band have
    // room for everything, and at both the operator can still see how to move,
    // how to open a row, and how to get back out.
    for width in [80, 105] {
        let line = drilled_ready_lane_line(width);

        assert!(line.chars().count() <= width, "{width}: {line}");
        for token in WAY_AROUND {
            assert!(line.contains(token), "{width}: {line}");
        }
        for dial in POLICY_DIALS {
            assert!(!line.contains(dial), "{width}: {line}");
        }
    }
}

#[test]
fn a_policy_dial_is_never_drawn_while_the_way_around_or_the_verdict_is_missing() {
    // The ordering stated as the property it is, across every width the band
    // can be handed rather than the two that happened to be dogfooded. A dial
    // is configuration the operator reaches for occasionally; the way out of
    // the lane and the verdict of the command just run are neither optional
    // nor recoverable from the Help roster the overflow marker opens.
    for width in 1..=220 {
        let line = drilled_ready_lane_line(width);
        let drew_a_dial = POLICY_DIALS.iter().any(|dial| line.contains(dial));

        assert!(
            !drew_a_dial || WAY_AROUND.iter().all(|token| line.contains(token)),
            "{width}: {line}"
        );
        assert!(
            !drew_a_dial || reports_the_verdict(&line),
            "{width}: {line}"
        );
    }
}

#[test]
fn the_verdict_abbreviates_rather_than_vanishing_from_the_measured_pane() {
    // 105 columns, the pane the maintainer runs beside the plan session. The
    // full verdict is 55 of them, so it used to be shed whole and the operator
    // was left with no sign that the dispatch they had just pressed had failed.
    let line = drilled_ready_lane_line(105);

    assert!(line.contains("last: dispatch failed"), "{line}");
    assert!(!line.contains("cause not reported"), "{line}");
}

#[test]
fn the_overflow_marker_names_the_key_that_reopens_what_it_counted() {
    // `+N more` was a count with no door: it told the operator that hints
    // existed and nothing about how to read them. It now names `?`, which
    // opens Help on the section for the pane they are looking at.
    let line = drilled_ready_lane_line(105);

    assert!(line.ends_with("+7 more: ?"), "{line}");
}
