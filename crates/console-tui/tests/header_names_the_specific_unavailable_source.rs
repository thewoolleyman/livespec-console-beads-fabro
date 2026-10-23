//! The header and the Event sources roster name the SOURCE that is down, not
//! the family it belongs to (livespec-console-beads-fabro-rhh56m AC3/AC4/AC7).
//!
//! MEASURED 2026-09-12T19:04Z on the real store, TUI build 19cc391: the header
//! read `event sources: 3 unavailable (fabro, +2 more)` while the store
//! recorded exactly TWO not-observed sources -- `fabro:vps`, the deliberately
//! down standby, and `reconcile-runs`. The third was a RETIRED row: the
//! pre-z4y7kp build polled an unqualified `fabro` source, and nothing has
//! polled that name since 2026-09-11, yet its last finding still sat in the log
//! counting against the tally and sorting FIRST, so the one name the header had
//! room for was the one name no running adapter owns.
//!
//! The cost was not cosmetic. An operator reading "fabro" unavailable concludes
//! the factory their work runs on is down; hp was healthy and executing.
//! Measured twice on the real console: once when the maintainer asked why the
//! active lane showed executing 2 while the Fabro UI showed nothing running,
//! and again on 2026-09-29 when this very reading produced a walkthrough step
//! telling the maintainer to "start the local fabro" -- no factory runs on the
//! console host at all.
//!
//! So the tally and the roster are both restricted to the sources the RUNNING
//! BUILD polls. The restriction only ever REMOVES a row whose name no current
//! adapter owns; it never invents an availability state for a source the
//! console made no observation of, which is AC7.

use console_application::{
    EventsFocus, TuiInteractionState, TuiOverlay, TuiView, build_tui_model_for_state,
};
use console_domain::{ConsoleEvent, EventType};
use console_tui::render_to_text;

/// The source instances the current build polls, in the order
/// `live_source_adapters_with_programs` mints them for a two-factory checkout.
/// The retired unqualified `fabro` is deliberately ABSENT: no adapter owns it.
fn current_sources() -> Vec<String> {
    [
        "orchestrator",
        "dispatcher",
        "fabro:hp",
        "fabro:vps",
        "livespec",
        "github",
        "reconcile-runs",
    ]
    .into_iter()
    .map(ToOwned::to_owned)
    .collect()
}

fn observed_event(id: &str, source: &str) -> ConsoleEvent {
    ConsoleEvent::fixture(id, EventType::SourceObservedFindingObserved, source)
}

fn not_observed_event(id: &str, source: &str, reason: &str) -> ConsoleEvent {
    ConsoleEvent::fixture(id, EventType::SourceNotObservedFindingObserved, source)
        .with_payload_json(format!(
            r#"{{"reason":"{reason}","repo":"livespec-console-beads-fabro"}}"#
        ))
}

/// The measured 2026-09-12 store, in append order: the retired `fabro` row's
/// own history ending not-observed, then the per-factory sources the current
/// build polls, with only the standby down.
fn measured_store() -> Vec<ConsoleEvent> {
    vec![
        observed_event("evt:fabro:retired:observed", "fabro"),
        not_observed_event(
            "evt:fabro:retired:not_observed",
            "fabro",
            "source command exited non-zero (exit status: 1)",
        ),
        observed_event("evt:fabro-hp:observed", "fabro:hp"),
        not_observed_event(
            "evt:fabro-vps:not_observed",
            "fabro:vps",
            "request failed with status 502 Bad Gateway",
        ),
        observed_event("evt:livespec:observed", "livespec"),
        observed_event("evt:reconcile-runs:observed", "reconcile-runs"),
    ]
}

/// AC3/AC4: the header names `fabro:vps` and counts ONE, at both widths the
/// TUI UX rubric grades. The exact string is asserted, so neither the count nor
/// the name can drift back to the family.
#[test]
fn the_header_names_the_specific_down_source_at_105_and_211_columns() {
    let state = TuiInteractionState::new(0, TuiOverlay::None)
        .with_selected_repo("livespec-console-beads-fabro".to_owned())
        .with_current_source_names(current_sources());
    let model = build_tui_model_for_state(&measured_store(), &state);

    for width in [105_u16, 211_u16] {
        let rendered = render_to_text(&model, width, 40).unwrap_or_default();
        assert!(
            rendered.contains("event sources: 1 unavailable (fabro:vps)"),
            "at {width} columns the header must name the standby that is \
             actually down, and count only it: {rendered}"
        );
    }
}

/// AC3/AC5: the model's own tally -- the one number the header renders and the
/// roster reads -- holds exactly the sources the store records as not-observed
/// among those the running build polls. The retired row is neither counted nor
/// named.
#[test]
fn the_retired_source_is_neither_counted_nor_named() {
    let state =
        TuiInteractionState::new(0, TuiOverlay::None).with_current_source_names(current_sources());
    let model = build_tui_model_for_state(&measured_store(), &state);

    assert_eq!(model.unavailable_sources(), ["fabro:vps".to_owned()]);
    assert!(
        !model.observed_source_names().contains(&"fabro".to_owned()),
        "a source no running adapter polls must not appear in the roster at \
         all: {:?}",
        model.observed_source_names()
    );
}

/// AC3: the Event sources ROSTER agrees with the header, row for row. It lists
/// `fabro:hp` healthy and `fabro:vps` unavailable under their own names, and no
/// bare `fabro` row at all, because the roster and the tally are derived from
/// one restricted view rather than from two walks that could filter
/// differently.
#[test]
fn the_roster_lists_each_factory_under_its_own_name() {
    let state = TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None)
        .with_events_focus(EventsFocus::EventSources)
        .with_current_source_names(current_sources());
    let model = build_tui_model_for_state(&measured_store(), &state);
    let rendered = render_to_text(&model, 211, 40).unwrap_or_default();

    for expected in [
        "fabro:hp — healthy",
        "fabro:vps — unavailable",
        "livespec — healthy",
        "reconcile-runs — healthy",
    ] {
        assert!(
            rendered.contains(expected),
            "expected the roster to carry `{expected}`: {rendered}"
        );
    }
    assert!(
        !rendered.contains("fabro — "),
        "a source no running adapter polls must not be rostered under the bare \
         family name: {rendered}"
    );
}

/// AC7: the restriction REMOVES rows, it never adds one. A source the current
/// build polls but has made no observation of yet has no availability state at
/// all -- it is neither available nor unavailable -- because every state the
/// console shows is still derived from an observation it actually made.
#[test]
fn a_polled_source_with_no_observation_yet_gets_no_inferred_state() {
    let state =
        TuiInteractionState::new(0, TuiOverlay::None).with_current_source_names(current_sources());
    let model = build_tui_model_for_state(&measured_store(), &state);

    for never_polled in ["orchestrator", "dispatcher", "github"] {
        assert!(
            !model
                .observed_source_names()
                .contains(&never_polled.to_owned()),
            "{never_polled} has no observation in this log, so no state may be \
             invented for it: {:?}",
            model.observed_source_names()
        );
        assert!(
            !model
                .unavailable_sources()
                .contains(&never_polled.to_owned()),
            "{never_polled} must not be branded unavailable without an \
             observation saying so: {:?}",
            model.unavailable_sources()
        );
    }
}

/// The startup window, which must not be a blackout: before the poller has
/// named the adapter set, nothing is restricted. An empty current-source list
/// means the set is not yet KNOWN, and hiding every source on that basis would
/// turn a cockpit-blind screen into a blank one -- the opposite of this item's
/// own defect.
#[test]
fn an_unknown_adapter_set_restricts_nothing() {
    let state = TuiInteractionState::new(0, TuiOverlay::None);
    let model = build_tui_model_for_state(&measured_store(), &state);

    assert_eq!(
        model.unavailable_sources(),
        ["fabro".to_owned(), "fabro:vps".to_owned()]
    );
}
