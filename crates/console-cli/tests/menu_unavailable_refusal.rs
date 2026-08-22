use console_application::{
    TuiInteractionState, TuiOverlay, TuiView, action_registry, build_tui_model_for_state,
};
use console_tui::TuiTerminalInput;

fn menu_position_for(action_id: &str) -> Option<(usize, usize)> {
    action_registry::menu_tree()
        .iter()
        .enumerate()
        .find_map(|(top_index, _top)| {
            action_registry::menu_actions(top_index)
                .iter()
                .position(|spec| spec.id == action_id)
                .map(|action_index| (top_index, action_index))
        })
}

#[test]
fn enter_on_an_unavailable_menu_row_renders_the_registry_refusal() {
    let spec = action_registry::action_for_id("dispatch-ready");
    assert!(
        spec.is_some(),
        "dispatch-ready must remain a registered menu action"
    );
    let spec = spec.unwrap_or(&action_registry::ACTION_REGISTRY[0]);
    let position = menu_position_for(spec.id);
    assert!(
        position.is_some(),
        "dispatch-ready must remain reachable through the menu"
    );
    let (top, selected) = position.unwrap_or((0, 0));
    let state =
        TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::Menu { top, selected });

    let step = console_tui::step_tui_runtime(&state, &[], TuiTerminalInput::Confirm, "operator");
    let model = build_tui_model_for_state(&[], step.state());
    let rendered = console_tui::render_to_text(&model, 120, 24).unwrap_or_default();

    assert_eq!(step.state().overlay(), &TuiOverlay::Menu { top, selected });
    assert!(
        rendered.contains(spec.availability_summary),
        "rendered menu refusal should come from the registry availability summary:\n{rendered}"
    );
}
