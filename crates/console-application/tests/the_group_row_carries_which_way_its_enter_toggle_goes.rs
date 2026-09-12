//! Group row expansion state is stored on the row and survives interaction,
//! and the model's accessor reports it correctly for selected rows.
//!
//! `livespec-console-beads-fabro-mx9u.33` added Help text and Status-line hints
//! for the group row Enter toggle, rooting both in the row's expanded state. The
//! two new accessors that carry that state must assert it at two levels:
//!
//! 1. `AttentionItem::group_is_expanded()` distinguishes a collapsed group row
//!    from an expanded one (the two :156 mutants: `-> true` and `-> false`).
//! 2. `TuiScreenModel::selected_attention_group_expanded()` reports whether the
//!    selected row is a group row and its expanded state, or `None` for an
//!    ordinary row (the :2799 mutant: `-> None`).
//!
//! A first implementation of mx9u.33 was published with both accessors passing
//! compiled tests but unasserted: Help prose and a footer string were generated
//! without consulting either accessor. The rendered-text assertions on Help and
//! the footer do not kill a mutant that survives if the text can be produced
//! without calling the getter — so this gate creates unit tests that call them
//! directly, in both the collapsed and expanded states, to ensure any future
//! change that loses the state also loses the test.

use console_application::source_adapters::{
    AttentionHandoff, AttentionItemSnapshot, AttentionSourceRef, attention_item_payload_json,
};
use console_application::{
    TuiInteraction, TuiInteractionState, TuiOverlay, TuiView, build_tui_model_for_state,
    reduce_tui_interaction,
};
use console_domain::{ConsoleEvent, EventType};

/// Create a needs-attention item that will be grouped with other items of
/// the same "kind:class" key.
fn attention_event_with_groupable_id(event_id: &str, id: &str, summary: &str) -> ConsoleEvent {
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

#[test]
fn group_is_expanded_distinguishes_collapsed_and_expanded_rows() {
    // Two needs-attention items with IDs like "impl-ready:stale-repo:<id>"
    // will be grouped because they share the same "kind:class" key.
    let events = vec![
        attention_event_with_groupable_id(
            "evt_1",
            "impl-ready:stale-repo:test1",
            "First stale repo",
        ),
        attention_event_with_groupable_id(
            "evt_2",
            "impl-ready:stale-repo:test2",
            "Second stale repo",
        ),
    ];
    let state = TuiInteractionState::for_view(TuiView::Attention, 0, TuiOverlay::None);
    let model = build_tui_model_for_state(&events, &state);

    // The first item should be a group row (since we have two similar items).
    // In the initial state, all groups are collapsed.
    let first_item = &model.attention_items()[0];
    assert!(
        first_item.group_key().is_some(),
        "first item should be a group row; attention items: {:?}",
        model
            .attention_items()
            .iter()
            .map(|i| (i.id(), i.group_key()))
            .collect::<Vec<_>>()
    );
    assert!(
        !first_item.group_is_expanded(),
        "group row should be collapsed initially"
    );

    // Now toggle the group by simulating an Enter keystroke.
    let toggled_state =
        reduce_tui_interaction(&state, &events, TuiInteraction::ToggleAttentionGroup);
    let toggled_model = build_tui_model_for_state(&events, &toggled_state);

    // Check the same group row after toggle.
    let first_item_after = &toggled_model.attention_items()[0];
    assert!(
        first_item_after.group_key().is_some(),
        "first item should still be a group row after toggle"
    );
    assert!(
        first_item_after.group_is_expanded(),
        "group row should be expanded after toggle"
    );

    // Toggle it back.
    let toggled_back_state = reduce_tui_interaction(
        &toggled_state,
        &events,
        TuiInteraction::ToggleAttentionGroup,
    );
    let toggled_back_model = build_tui_model_for_state(&events, &toggled_back_state);

    let first_item_after_toggle_back = &toggled_back_model.attention_items()[0];
    assert!(
        !first_item_after_toggle_back.group_is_expanded(),
        "group row should be collapsed again after second toggle"
    );
}

#[test]
fn selected_attention_group_expanded_returns_some_for_group_rows_and_none_for_others() {
    let grouped = vec![
        attention_event_with_groupable_id(
            "evt_1",
            "impl-ready:stale-repo:test1",
            "First stale repo",
        ),
        attention_event_with_groupable_id(
            "evt_2",
            "impl-ready:stale-repo:test2",
            "Second stale repo",
        ),
    ];
    let state_grouped = TuiInteractionState::for_view(TuiView::Attention, 0, TuiOverlay::None);
    let model_grouped = build_tui_model_for_state(&grouped, &state_grouped);

    // The first row should be a group row, and collapsed.
    let selected_state_collapsed = model_grouped.selected_attention_group_expanded();
    assert_eq!(
        selected_state_collapsed,
        Some(false),
        "group row should report Some(false) when collapsed and selected"
    );

    // Expand the group.
    let expanded_state = reduce_tui_interaction(
        &state_grouped,
        &grouped,
        TuiInteraction::ToggleAttentionGroup,
    );
    let model_expanded = build_tui_model_for_state(&grouped, &expanded_state);

    let selected_state_expanded = model_expanded.selected_attention_group_expanded();
    assert_eq!(
        selected_state_expanded,
        Some(true),
        "group row should report Some(true) when expanded and selected"
    );
}

#[test]
fn selected_attention_group_expanded_returns_none_for_ordinary_rows() {
    // A single needs-attention item won't be grouped (groups require 2+ items).
    let events = vec![attention_event_with_groupable_id(
        "evt_ungrouped",
        "impl-ready:unique:test1",
        "Ungrouped item",
    )];
    let state = TuiInteractionState::for_view(TuiView::Attention, 0, TuiOverlay::None);
    let model = build_tui_model_for_state(&events, &state);

    // The first item should be an ordinary row (not a group).
    let first_item = &model.attention_items()[0];
    assert!(
        first_item.group_key().is_none(),
        "item should be an ordinary row (not a group)"
    );

    // The model's accessor should return None.
    let selected_expanded = model.selected_attention_group_expanded();
    assert_eq!(
        selected_expanded, None,
        "ordinary row should report None for selected_attention_group_expanded"
    );
}
