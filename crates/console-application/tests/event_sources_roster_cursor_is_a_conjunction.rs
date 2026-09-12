//! Which cursor `SelectNext` moves around the Event sources roster, asserted
//! inside the crate that OWNS the routing
//! (livespec-console-beads-fabro-mx9u.20.3).
//!
//! The roster is the one drilled-in summary sub-view whose rows are
//! individually actionable, so it is the one that carries a cursor of its own:
//! up/down walk the SOURCE rows rather than falling through to the Attention
//! cursor every other drilled-in summary surface uses. Whether that cursor
//! engages is decided by a CONJUNCTION over two facts -- the active view is
//! `Events` AND the container is drilled into `Event sources`.
//!
//! A suite that only ever exercises states where BOTH facts hold, or where
//! NEITHER does, cannot tell that conjunction apart from a disjunction.
//!
//! WHY THIS FILE EXISTS ALONGSIDE THE console-tui ONE. The same three cases are
//! already pinned at the keyboard in
//! `crates/console-tui/tests/event_sources_roster_cursor_routing.rs`, and that
//! test is kept: it pins the Down-key MAPPING, which is a different claim.
//! What it cannot do is defend the routing itself, because cargo-mutants runs
//! the MUTATED PACKAGE's own test suite -- and the routing lives here, in
//! `console-application`. Measured on PR #1227 (2026-09-12): the `&&` -> `||`
//! mutant on `is_event_sources_roster` survived a full sweep with the
//! console-tui test green, while a sibling test in THIS directory killed both
//! of its own mutants. A gate that never runs against the code it describes is
//! not a gate, so the routing claim is restated here, against the public
//! reducer, with no key mapping and no console-tui dependency in the way.
//!
//! ORDERING MATTERS TO WHICH NEGATIVE DISCRIMINATES. `select_next` tests the
//! picker-home predicate BEFORE the roster's, so the (`Events`, `Overview`)
//! state never reaches the roster predicate at all -- the earlier branch
//! consumes it, and a disjunction reads identically there. The two states that
//! DO reach it, and that a disjunction gets backwards, are `Events` +
//! `Stored events` and `Event sources` + some other view. Those are the
//! negatives below; the picker home is kept as an ordering control that says
//! so.

use console_application::source_adapters::{
    AttentionHandoff, AttentionItemSnapshot, AttentionSourceRef, attention_item_payload_json,
};
use console_application::{
    EventsFocus, FocusPane, TuiInteraction, TuiInteractionState, TuiOverlay, TuiView,
    build_tui_model_for_state, reduce_tui_interaction,
};
use console_domain::{ConsoleEvent, EventType};

/// A needs-attention row that groups with nothing: its id carries no `:`, so
/// it has no group key and stands as a row of its own. Two of these give the
/// Attention list the second row a `SelectNext` needs in order to MOVE.
fn attention_event(event_id: &str, id: &str, summary: &str) -> ConsoleEvent {
    let item = AttentionItemSnapshot::new(
        id,
        "impl-ready",
        "high",
        summary,
        AttentionSourceRef::new("test-repo", None, None),
        AttentionHandoff::new("inspect", None, "inspect:test"),
    );
    ConsoleEvent::fixture(
        event_id,
        EventType::AttentionItemAppeared,
        "needs-attention",
    )
    .with_payload_json(attention_item_payload_json(&item))
}

/// A not-observed marker, which is what puts `source` on the roster at all.
fn source_event(source: &str) -> ConsoleEvent {
    ConsoleEvent::fixture(
        &format!("evt:{source}:not_observed"),
        EventType::SourceNotObservedFindingObserved,
        source,
    )
    .with_payload_json(
        r#"{"reason":"source command exited non-zero","repo":"livespec-console-beads-fabro"}"#
            .to_owned(),
    )
}

/// One event slice every case shares, carrying two Attention rows and two
/// roster rows so that whichever cursor an interaction routes to has somewhere
/// to go. A case that routed the wrong way therefore reads as a cursor that did
/// NOT move, never as an ambiguous no-op.
fn fixture_events() -> Vec<ConsoleEvent> {
    vec![
        attention_event("evt_attn_alpha", "wi-alpha", "First ready item"),
        attention_event("evt_attn_beta", "wi-beta", "Second ready item"),
        source_event("livespec"),
        source_event("reconcile-runs"),
    ]
}

/// A state parked on `view` with the `Events` container drilled to
/// `events_focus`, arrow keys driving the Content pane (which is where the
/// operator is once they have drilled into anything at all).
const fn state_in(view: TuiView, events_focus: EventsFocus) -> TuiInteractionState {
    TuiInteractionState::for_view(view, 0, TuiOverlay::None)
        .with_events_focus(events_focus)
        .with_focus(FocusPane::Content)
}

/// Move the selection down through the public reducer, exactly as the
/// interactive loop does once its key handler has resolved a Down keypress.
fn select_next(state: &TuiInteractionState, events: &[ConsoleEvent]) -> TuiInteractionState {
    reduce_tui_interaction(state, events, TuiInteraction::SelectNext)
}

/// Neither cursor can MOVE unless its list has a second row, so a fixture that
/// quietly lost one would let every case below pass while asserting nothing.
#[test]
fn the_fixture_gives_both_cursors_somewhere_to_move() {
    let events = fixture_events();

    let attention = build_tui_model_for_state(
        &events,
        &state_in(TuiView::Attention, EventsFocus::Overview),
    );
    assert!(
        attention.attention_items().len() >= 2,
        "the Attention list needs a second row for SelectNext to move it"
    );

    let roster = build_tui_model_for_state(
        &events,
        &state_in(TuiView::Events, EventsFocus::EventSources),
    );
    assert!(
        roster.view_items().len() >= 2,
        "the roster needs a second source row for SelectNext to move it"
    );
}

/// The control: with BOTH halves true the roster cursor is the one that walks,
/// and the Attention cursor is left alone.
#[test]
fn the_roster_cursor_walks_the_source_rows_when_both_halves_hold() {
    let events = fixture_events();

    let moved = select_next(
        &state_in(TuiView::Events, EventsFocus::EventSources),
        &events,
    );

    assert_eq!(
        moved.selected_event_source_index(),
        1,
        "SelectNext in the roster must walk to the next SOURCE row"
    );
    assert_eq!(
        moved.selected_attention_index(),
        0,
        "the Attention cursor must not move while the roster owns up/down"
    );
}

/// Exactly ONE half holds: the view IS `Events`, but the container is drilled
/// into `Stored events`, which keeps its pre-container behaviour and falls
/// through to the Attention cursor like every other drilled-in summary
/// surface. A disjunction would engage the roster cursor here instead, on the
/// strength of the view alone.
#[test]
fn stored_events_falls_through_to_the_attention_cursor() {
    let events = fixture_events();

    let moved = select_next(
        &state_in(TuiView::Events, EventsFocus::StoredEvents),
        &events,
    );

    assert_eq!(
        moved.selected_attention_index(),
        1,
        "Stored events must keep falling through to the Attention cursor"
    );
    assert_eq!(
        moved.selected_event_source_index(),
        0,
        "the roster cursor must stay put while the roster is not on screen"
    );
}

/// Exactly ONE half holds the other way round: the container REMEMBERS it was
/// drilled into `Event sources`, but the operator has left for another view.
/// The remembered sub-view must not follow them there and capture up/down --
/// which is what a disjunction does, on the strength of the focus alone.
#[test]
fn a_remembered_event_sources_focus_does_not_follow_the_operator_out_of_events() {
    let events = fixture_events();

    for view in [TuiView::Attention, TuiView::Spec, TuiView::Repos] {
        let moved = select_next(&state_in(view, EventsFocus::EventSources), &events);

        assert_eq!(
            moved.selected_attention_index(),
            1,
            "{view:?} must route SelectNext to the Attention cursor"
        );
        assert_eq!(
            moved.selected_event_source_index(),
            0,
            "{view:?} is not the roster, so the roster cursor must stay put"
        );
    }
}

/// The `Events` container's own picker home keeps its cursor AHEAD of the
/// roster's in the routing chain.
///
/// This case does not discriminate the conjunction -- the picker-home
/// predicate is tested first and wins whichever way the roster's predicate
/// reads -- and that is exactly what it is here to pin. It is the reason the
/// discriminating "view is `Events`" negative above has to be `Stored events`
/// rather than the picker home, so a future reordering that quietly changed
/// which surface owns up/down fails here rather than eroding that case.
#[test]
fn the_events_picker_home_keeps_its_own_cursor_ahead_of_the_roster() {
    let events = fixture_events();

    let moved = select_next(&state_in(TuiView::Events, EventsFocus::Overview), &events);

    assert_eq!(
        moved.selected_events_index(),
        1,
        "the picker home must walk its own sub-view rows"
    );
    assert_eq!(
        moved.selected_event_source_index(),
        0,
        "the roster cursor must stay put on the picker home"
    );
    assert_eq!(
        moved.selected_attention_index(),
        0,
        "the Attention cursor must stay put on the picker home"
    );
}
