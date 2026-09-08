//! `TuiScreenModel::attention_total` is the needs-attention INBOX total, never
//! the search-filtered match count -- and it is that on the model itself, not
//! merely in `console-tui`'s rendered text.
//!
//! `livespec-console-beads-fabro-mx9u.4` fixed the header's `attention:` count
//! to stop following the active search filter; `crates/console-tui/tests/
//! search_overlay_and_confirm_box_sizing.rs` pins that fix at the rendered
//! text. `just check-mutants` mutates `console-application` alone
//! (`--package console-domain --package console-application`), so a survivor
//! there needs a test living IN `console-application`'s own test scope to be
//! seen -- the `console-tui` assertion, correct as it is, never runs under
//! that gate. Measured 2026-09-08: `TuiScreenModel::attention_total -> usize`
//! replaced with `0` or `1`, and the `!query.is_empty()` guard in
//! `build_tui_model_for_state` replaced with `false` or with `!` deleted, all
//! survived `check-mutants` on PR #1096 despite the `console-tui` coverage.

use console_application::source_adapters::{Lane, LaneReason};
use console_application::{
    TuiInteractionState, TuiOverlay, TuiScreenModel, build_tui_model_for_state,
};
use console_domain::{ConsoleEvent, EventType};

/// A `blocked` / `needs-human` work-item snapshot: the lane that rests on a
/// human step, so the item lands in the needs-attention inbox.
fn blocked_event(event_id: &str, work_item_id: &str) -> ConsoleEvent {
    let payload = format!(
        r#"{{"repo":"console","work_item_id":"{work_item_id}","lane":"{}","lane_reason":"{}","rank":"a0","status":"blocked","detail":{{"title":"Routine lane fixture item"}},"source_version":1}}"#,
        Lane::Blocked.label(),
        LaneReason::NeedsHuman.label()
    );
    ConsoleEvent::fixture(
        event_id,
        EventType::WorkItemSnapshotObserved,
        "orchestrator",
    )
    .with_payload_json(payload)
}

/// A two-item inbox whose ids differ, so a query can match exactly one of them
/// and the filtered count and the inbox total are distinguishable numbers.
fn inbox_events() -> [ConsoleEvent; 2] {
    [
        blocked_event("evt_alpha", "console-alpha"),
        blocked_event("evt_beta", "console-beta"),
    ]
}

fn model(overlay: TuiOverlay) -> TuiScreenModel {
    build_tui_model_for_state(&inbox_events(), &TuiInteractionState::new(0, overlay))
}

fn searching(query: &str) -> TuiOverlay {
    TuiOverlay::Search {
        query: query.to_owned(),
    }
}

#[test]
fn attention_total_is_the_full_inbox_count_with_no_active_search() {
    let unfiltered = model(TuiOverlay::None);
    assert_eq!(unfiltered.attention_items().len(), 2);
    assert_eq!(unfiltered.attention_total(), 2);
}

/// The mutation-testing-relevant case: a non-empty query narrows what
/// `attention_items` shows (to 1 of the 2) but `attention_total` must stay at
/// the inbox total (2), not fall to the filtered count. This is what kills
/// the `attention_total -> 0` / `-> 1` getter mutants (2 is neither) and the
/// `!query.is_empty()` guard's `false` / `delete !` mutants (both would fall
/// through to the filtered-count branch and report 1).
#[test]
fn attention_total_stays_the_inbox_total_while_a_search_narrows_the_list() {
    let filtered = model(searching("alpha"));
    assert_eq!(filtered.attention_items().len(), 1);
    assert_eq!(filtered.attention_total(), 2);
}

/// An overlay open with nothing typed yet narrows nothing, so the filtered
/// and unfiltered counts agree -- this is the case the removed
/// `!query.is_empty()` guard used to special-case, and it agrees with the
/// unconditional recompute because the underlying matchers already treat an
/// empty query as matching everything.
#[test]
fn attention_total_matches_the_filtered_count_when_the_search_query_is_still_empty() {
    let empty_query = model(searching(""));
    assert_eq!(empty_query.attention_items().len(), 2);
    assert_eq!(empty_query.attention_total(), 2);
}
