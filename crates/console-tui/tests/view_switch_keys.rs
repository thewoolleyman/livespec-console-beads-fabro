//! Direct view-switch keys: `1`..`6` select the six Views in `TuiView::all()`
//! order, from any body pane, without walking the nav or cycling focus.
//!
//! # Why this gate exists
//!
//! Measured at the real TUI on 2026-09-08 (dogfood waves 2 and 3, reported on
//! `livespec-console-beads-fabro-ohtig5`): reaching `Repos` from `Lanes`, the
//! operator pressed `Tab` and landed on the HEADER pane — `LiveSpec Console
//! [focus]`, hints `left/right scroll | esc/tab leave | ? help | q quit` — then
//! had to `Tab` twice more to arrive. Reaching a ready item from Attention cost
//! six keystrokes before any verb was available. `Tab` cycles focus, which is
//! not what an operator reaching for another VIEW means, and the header is a
//! surprising place to land.
//!
//! One key per view removes the detour. The binding is registry-backed like
//! every other key, so it carries a menu path and a Help entry by construction;
//! this gate pins the three operator-visible consequences — the keys act, the
//! Views pane shows which digit is which, and the Status line names the range.

use console_application::{
    FocusPane, HelpFocus, TuiInteractionState, TuiOverlay, TuiView, build_tui_model_for_state,
    reduce_tui_interaction,
};
use console_tui::{TuiRenderError, TuiTerminalInput, key_event_to_terminal_input, render_to_text};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The digit an operator presses for each view, in `TuiView::all()` order.
///
/// Written out rather than computed from the index: this is the OPERATOR-facing
/// claim the gate makes, and a derivation would restate whatever the production
/// code does instead of pinning what the keyboard must do.
const VIEW_DIGITS: [char; 6] = ['1', '2', '3', '4', '5', '6'];

/// The digit an operator presses for the view at `index` of `TuiView::all()`,
/// or `'?'` for an index the binding does not cover — which
/// [`every_view_has_a_digit`] refuses outright.
fn digit_for(index: usize) -> char {
    VIEW_DIGITS.get(index).copied().unwrap_or('?')
}

/// Every view carries a digit, so no case below can pass vacuously on a view the
/// digit table forgot.
#[test]
fn every_view_has_a_digit() {
    assert_eq!(VIEW_DIGITS.len(), TuiView::all().len());
}

/// Press `key` against `state` exactly as the interactive loop does: resolve the
/// key event through the TUI's key handler, then reduce whatever interaction it
/// yields. A key the handler declines leaves the state untouched.
fn press(state: &TuiInteractionState, key: char) -> TuiInteractionState {
    let model = build_tui_model_for_state(&[], state);
    let event = KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE);
    match key_event_to_terminal_input(event, &model) {
        Some(TuiTerminalInput::Interaction(interaction)) => {
            reduce_tui_interaction(state, &[], interaction)
        }
        _other => state.clone(),
    }
}

/// ACCEPTANCE 1, first half: the digit switches the active view from ANY body
/// pane. The reported detour started on the nav and ended on the header, so the
/// header is in the population rather than assumed equivalent.
#[test]
fn every_view_digit_switches_the_active_view_from_every_pane() {
    for focus in [
        FocusPane::Nav,
        FocusPane::Content,
        FocusPane::Detail,
        FocusPane::Header,
    ] {
        for (index, view) in TuiView::all().iter().enumerate() {
            let digit = digit_for(index);
            let start = TuiInteractionState::for_view(TuiView::Attention, 0, TuiOverlay::None)
                .with_focus(focus);
            let after = press(&start, digit);
            assert_eq!(
                after.active_view(),
                *view,
                "`{digit}` pressed with {focus:?} focused must select {}",
                view.label()
            );
        }
    }
}

/// ACCEPTANCE 1, second half: with an overlay open the digit never switches the
/// view. A text overlay takes it as a literal character (typing `3` into a
/// search query must not teleport the board); every other overlay ignores it.
#[test]
fn a_view_digit_never_switches_the_view_while_an_overlay_is_open() {
    let overlays = [
        TuiOverlay::Help {
            focus: HelpFocus::Menu,
            selected_section: 0,
            scroll: 0,
        },
        TuiOverlay::ActionInvoker { selected_action: 0 },
        TuiOverlay::CommandModal {
            selected_action_index: 0,
        },
        TuiOverlay::Search {
            query: String::new(),
        },
        TuiOverlay::CommandPalette {
            query: String::new(),
        },
    ];
    for overlay in overlays {
        let start = TuiInteractionState::for_view(TuiView::Attention, 0, overlay);
        for index in 0..TuiView::all().len() {
            let digit = digit_for(index);
            assert_eq!(
                press(&start, digit).active_view(),
                TuiView::Attention,
                "`{digit}` must not switch the view behind {:?}",
                start.overlay()
            );
        }
    }
}

/// ACCEPTANCE 2, first half: the Views pane renders the digit beside the name,
/// so the binding is discoverable where the operator is already looking.
#[test]
fn the_views_pane_renders_each_digit_beside_its_view_name() -> Result<(), TuiRenderError> {
    let state = TuiInteractionState::for_view(TuiView::Attention, 0, TuiOverlay::None);
    let model = build_tui_model_for_state(&[], &state);
    let rendered = render_to_text(&model, 160, 40)?;
    for (index, view) in TuiView::all().iter().enumerate() {
        let row = format!("{} {}", digit_for(index), view.label());
        assert!(
            rendered.contains(&row),
            "the Views pane must render `{row}`:\n{rendered}"
        );
    }
    Ok(())
}

/// ACCEPTANCE 2, second half: the modal Help's `Global actions` section lists
/// the binding, one line per view, so the roster and the keys agree.
#[test]
fn the_global_actions_help_section_lists_every_view_switch_binding() -> Result<(), TuiRenderError> {
    let state = TuiInteractionState::for_view(
        TuiView::Attention,
        0,
        TuiOverlay::Help {
            focus: HelpFocus::Menu,
            selected_section: 0,
            scroll: 0,
        },
    );
    let model = build_tui_model_for_state(&[], &state);
    let rendered = render_to_text(&model, 160, 44)?;
    for (index, view) in TuiView::all().iter().enumerate() {
        let line = format!(
            "{}            go to the {} view",
            digit_for(index),
            view.label()
        );
        assert!(
            rendered.contains(&line),
            "the Global actions help section must list `{line}`:\n{rendered}"
        );
    }
    Ok(())
}

/// ACCEPTANCE 3: the Status line names the binding as one range token, on the
/// Views pane and on every other pane the keys act from.
#[test]
fn the_status_line_names_the_view_switch_range() {
    for focus in [
        FocusPane::Nav,
        FocusPane::Content,
        FocusPane::Detail,
        FocusPane::Header,
    ] {
        for view in TuiView::all() {
            let state = TuiInteractionState::for_view(*view, 0, TuiOverlay::None).with_focus(focus);
            let footer = build_tui_model_for_state(&[], &state).footer();
            assert!(
                footer.contains("1-6 view"),
                "the Status line for {} with {focus:?} focused must name `1-6 view`: {footer}",
                view.label()
            );
        }
    }
}
