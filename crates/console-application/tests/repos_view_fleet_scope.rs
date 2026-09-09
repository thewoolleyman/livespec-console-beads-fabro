//! The Repos view attributes REPOSITORIES, and the fleet stream is not one.
//!
//! Regression coverage for the second-tenant defect: dogfooded at the real TUI
//! against this repo's own single-tenant store, the Repos view reported
//! "Repos observed: 2: livespec, livespec-console-beads-fabro". There is no
//! `livespec` tenant in that store. All 56 events under the fleet prefix live
//! on ONE stream, `fleet:livespec` -- every operator factory-drain command is
//! keyed under it, matching the header's fixed `fleet: livespec` -- and the
//! projection read that key as `{context}:{repo}` and extracted the product
//! FAMILY name as a repository.
//!
//! A fleet stream carries state of the product family as a whole, so it
//! belongs to no repository. It is not therefore invisible: dropping its
//! events unremarked would trade one wrong number for a quietly incomplete
//! one, which is the failure the "Events with no derivable repo" row already
//! exists to prevent. The fleet events get their own row instead.

use console_application::{TuiInteractionState, TuiOverlay, TuiView, build_tui_model_for_state};
use console_domain::{ConsoleEvent, EventType};

/// An event on `stream_id`. The type is the drain-command shape that actually
/// streams under `fleet:livespec` in the live store.
fn event(event_id: &str, stream_id: &str, stream_seq: u64) -> ConsoleEvent {
    ConsoleEvent::new(
        event_id.to_owned(),
        1,
        "console".to_owned(),
        EventType::FactoryDrainRequested,
        "console:factory-command-handler".to_owned(),
        stream_id.to_owned(),
        stream_seq,
    )
}

/// The Repos view's projection rows as the operator reads them: one
/// `title: detail` line per row.
fn repos_view_text(events: &[ConsoleEvent]) -> String {
    let state = TuiInteractionState::for_view(TuiView::Repos, 0, TuiOverlay::None);
    build_tui_model_for_state(events, &state)
        .view_items()
        .iter()
        .map(|item| format!("{}: {}", item.title(), item.detail()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_fleet_scoped_stream_contributes_no_repository() {
    let events = [
        event("evt_repo", "repo:livespec-console-beads-fabro", 1),
        event("evt_fleet", "fleet:livespec", 1),
        event("evt_fleet_2", "fleet:livespec", 2),
    ];

    let rendered = repos_view_text(&events);

    assert!(rendered.contains("Repos observed: 1"), "{rendered}");
    // The roster is the tenant alone; the family name is NOT a repo row.
    assert!(
        rendered.contains("Repos observed: 1: livespec-console-beads-fabro"),
        "{rendered}"
    );
}

#[test]
fn fleet_scoped_events_are_reported_on_their_own_row() {
    let events = [
        event("evt_repo", "repo:livespec-console-beads-fabro", 1),
        event("evt_fleet", "fleet:livespec", 1),
        event("evt_fleet_2", "fleet:livespec", 2),
    ];

    let rendered = repos_view_text(&events);

    // Counted out loud, with the stream roster as its operational detail --
    // the same shape as the repo row above it.
    assert!(rendered.contains("Fleet-scoped events: 2"), "{rendered}");
    assert!(
        rendered.contains("Fleet-scoped events: 2: fleet:livespec"),
        "{rendered}"
    );
}

#[test]
fn the_three_attributions_partition_the_store() {
    // Every event lands in exactly one of the three rows: attributed to a repo,
    // fleet-scoped, or carrying no derivable repo at all. The counts must add
    // up to the store, or the projection is once again reporting a number that
    // quietly excludes part of it.
    let events = [
        event("evt_repo", "repo:livespec-console-beads-fabro", 1),
        event("evt_fleet", "fleet:livespec", 1),
        event("evt_colonless", "livespec-console-beads-fabro-3lxx7t", 1),
    ];

    let rendered = repos_view_text(&events);

    assert!(rendered.contains("Repos observed: 1"), "{rendered}");
    assert!(rendered.contains("Fleet-scoped events: 1"), "{rendered}");
    assert!(
        rendered.contains("Events with no derivable repo: 1"),
        "{rendered}"
    );
}

#[test]
fn a_store_with_no_fleet_stream_carries_no_fleet_row() {
    // NEGATIVE CONTROL. An unconditional row is noise on a store that has none,
    // and a row an operator learns to skip has stopped being read.
    let events = [event("evt_repo", "repo:livespec-console-beads-fabro", 1)];

    let rendered = repos_view_text(&events);

    assert!(rendered.contains("Repos observed: 1"), "{rendered}");
    assert!(!rendered.contains("Fleet-scoped events"), "{rendered}");
}

/// A colonless event of the given type, the shape `command.accepted` and
/// `attention_item.resolved` actually stream under in a real store: a bare
/// work-item / command id, with no `{context}:` prefix at all.
fn colonless_event(
    event_id: &str,
    event_type: EventType,
    stream_id: &str,
    stream_seq: u64,
) -> ConsoleEvent {
    ConsoleEvent::new(
        event_id.to_owned(),
        1,
        "orchestrator".to_owned(),
        event_type,
        "livespec".to_owned(),
        stream_id.to_owned(),
        stream_seq,
    )
}

#[test]
fn the_unattributable_row_names_the_event_families_not_only_a_count() {
    // livespec-console-beads-fabro-mx9u.21: measured on the real store, 390
    // unattributable events were five DIFFERENT event families
    // (command.accepted, attention_item.resolved, work_item.action.*,
    // config.*, factory.*), and the view said only "Events with no derivable
    // repo: 390" -- honest about the count, silent about the composition, so
    // the operator could not tell a real attribution hole from console's own
    // command/action streams (which legitimately carry no repo).
    let events = [
        colonless_event("evt_cmd_1", EventType::CommandAccepted, "bd-ib-aaa", 1),
        colonless_event("evt_cmd_2", EventType::CommandAccepted, "bd-ib-bbb", 1),
        colonless_event(
            "evt_action_started",
            EventType::WorkItemActionStarted,
            "bd-ib-ccc",
            1,
        ),
        colonless_event(
            "evt_action_completed",
            EventType::WorkItemActionCompleted,
            "bd-ib-ddd",
            1,
        ),
        colonless_event(
            "evt_config",
            EventType::ConfigDispatcherSettingChanged,
            "auto_admission",
            1,
        ),
    ];

    let rendered = repos_view_text(&events);

    assert!(
        rendered.contains("Events with no derivable repo: 5"),
        "{rendered}"
    );
    // A family with exactly one participating event type is named exactly...
    assert!(rendered.contains("command.accepted 2"), "{rendered}");
    assert!(
        rendered.contains("config.dispatcher_setting.changed 1"),
        "{rendered}"
    );
    // ...one with several collapses to a wildcard so the row stays readable.
    assert!(rendered.contains("work_item.action.* 2"), "{rendered}");
}

#[test]
fn the_repos_view_states_what_observed_means() {
    // AC5: "observed" is a projection over the event log, not a configured
    // roster and not a filesystem scan -- stated on the view itself so the
    // maintainer's "why aren't ALL repos observed?" has an answer on screen.
    let events = [event("evt_repo", "repo:livespec-console-beads-fabro", 1)];

    let rendered = repos_view_text(&events);
    let lowered = rendered.to_lowercase();

    assert!(
        lowered.contains("not a configured roster")
            && lowered.contains("not")
            && lowered.contains("disk"),
        "{rendered}"
    );
}
