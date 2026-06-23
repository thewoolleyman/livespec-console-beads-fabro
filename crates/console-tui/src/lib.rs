#![forbid(unsafe_code)]

use console_application::{
    AttentionDetail, AttentionItem, OperatorAction, TimelineEntry, TuiInteraction, TuiOverlay,
    TuiScreenModel,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Widget, Wrap};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiRenderError {
    EmptyArea,
}

pub type TuiRenderResult<T> = Result<T, TuiRenderError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiTerminalInput {
    Interaction(TuiInteraction),
    Confirm,
    Quit,
}

#[must_use]
pub const fn key_event_to_terminal_input(
    event: KeyEvent,
    overlay: &TuiOverlay,
) -> Option<TuiTerminalInput> {
    if event.modifiers.contains(KeyModifiers::CONTROL) && matches!(event.code, KeyCode::Char('c')) {
        return Some(TuiTerminalInput::Quit);
    }
    match event.code {
        KeyCode::Up => Some(TuiTerminalInput::Interaction(up_interaction(overlay))),
        KeyCode::Down => Some(TuiTerminalInput::Interaction(down_interaction(overlay))),
        KeyCode::Esc => Some(TuiTerminalInput::Interaction(TuiInteraction::CloseOverlay)),
        KeyCode::Enter => Some(enter_input(overlay)),
        KeyCode::Backspace => Some(TuiTerminalInput::Interaction(TuiInteraction::Backspace)),
        KeyCode::Char('/') => slash_input(overlay),
        KeyCode::Char(':') => colon_input(overlay),
        KeyCode::Char('q') => q_input(overlay),
        KeyCode::Char(value) => text_input(value, overlay),
        KeyCode::Left
        | KeyCode::Right
        | KeyCode::Home
        | KeyCode::End
        | KeyCode::PageUp
        | KeyCode::PageDown
        | KeyCode::Tab
        | KeyCode::BackTab
        | KeyCode::Delete
        | KeyCode::Insert
        | KeyCode::F(_)
        | KeyCode::Null
        | KeyCode::CapsLock
        | KeyCode::ScrollLock
        | KeyCode::NumLock
        | KeyCode::PrintScreen
        | KeyCode::Pause
        | KeyCode::Menu
        | KeyCode::KeypadBegin
        | KeyCode::Media(_)
        | KeyCode::Modifier(_) => None,
    }
}

const fn up_interaction(overlay: &TuiOverlay) -> TuiInteraction {
    if matches!(overlay, TuiOverlay::CommandModal { .. }) {
        return TuiInteraction::SelectPreviousAction;
    }
    TuiInteraction::SelectPrevious
}

const fn down_interaction(overlay: &TuiOverlay) -> TuiInteraction {
    if matches!(overlay, TuiOverlay::CommandModal { .. }) {
        return TuiInteraction::SelectNextAction;
    }
    TuiInteraction::SelectNext
}

const fn enter_input(overlay: &TuiOverlay) -> TuiTerminalInput {
    if matches!(overlay, TuiOverlay::CommandModal { .. }) {
        return TuiTerminalInput::Confirm;
    }
    TuiTerminalInput::Interaction(TuiInteraction::OpenCommandModal)
}

const fn slash_input(overlay: &TuiOverlay) -> Option<TuiTerminalInput> {
    if matches!(overlay, TuiOverlay::None) {
        return Some(TuiTerminalInput::Interaction(TuiInteraction::OpenSearch));
    }
    text_input('/', overlay)
}

const fn colon_input(overlay: &TuiOverlay) -> Option<TuiTerminalInput> {
    if matches!(overlay, TuiOverlay::None) {
        return Some(TuiTerminalInput::Interaction(
            TuiInteraction::OpenCommandPalette,
        ));
    }
    text_input(':', overlay)
}

const fn q_input(overlay: &TuiOverlay) -> Option<TuiTerminalInput> {
    if matches!(overlay, TuiOverlay::None) {
        return Some(TuiTerminalInput::Quit);
    }
    text_input('q', overlay)
}

const fn text_input(value: char, overlay: &TuiOverlay) -> Option<TuiTerminalInput> {
    if matches!(
        overlay,
        TuiOverlay::Search { .. } | TuiOverlay::CommandPalette { .. }
    ) {
        return Some(TuiTerminalInput::Interaction(TuiInteraction::TypeChar(
            value,
        )));
    }
    None
}

pub fn render_to_text(model: &TuiScreenModel, width: u16, height: u16) -> TuiRenderResult<String> {
    if width == 0 || height == 0 {
        return Err(TuiRenderError::EmptyArea);
    }
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    render_model(model, area, &mut buffer);
    Ok(buffer_to_text(&buffer, area))
}

pub fn render_model(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    if area.is_empty() {
        return;
    }
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(area);
    render_header(model, vertical[0], buffer);
    render_body(model, vertical[1], buffer);
    render_footer(model, vertical[2], buffer);
    render_overlay(model, area, buffer);
}

fn render_header(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    Paragraph::new(model.header())
        .block(Block::new().borders(Borders::ALL).title("LiveSpec Console"))
        .render(area, buffer);
}

fn render_body(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(18),
            Constraint::Percentage(38),
            Constraint::Percentage(62),
        ])
        .split(area);
    render_navigation(model, horizontal[0], buffer);
    render_attention(model, horizontal[1], buffer);
    render_detail(model.detail(), horizontal[2], buffer);
}

fn render_footer(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    Paragraph::new(model.footer())
        .block(Block::new().borders(Borders::ALL).title("Status"))
        .render(area, buffer);
}

fn render_overlay(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    match model.overlay() {
        TuiOverlay::None => {}
        TuiOverlay::Search { query } => {
            render_prompt_overlay("Search", format!("/{query}"), area, buffer);
        }
        TuiOverlay::CommandPalette { query } => {
            render_prompt_overlay("Command Palette", format!(":{query}"), area, buffer);
        }
        TuiOverlay::CommandModal {
            selected_action_index,
        } => {
            render_command_modal(
                model.detail(),
                *selected_action_index,
                overlay_rect(area),
                buffer,
            );
        }
    }
}

fn render_prompt_overlay(title: &'static str, value: String, area: Rect, buffer: &mut Buffer) {
    let overlay = overlay_rect(area);
    Clear.render(overlay, buffer);
    Paragraph::new(value)
        .block(Block::new().borders(Borders::ALL).title(title))
        .render(overlay, buffer);
}

fn render_command_modal(
    detail: Option<&AttentionDetail>,
    selected_action_index: usize,
    area: Rect,
    buffer: &mut Buffer,
) {
    Clear.render(area, buffer);
    let items = detail
        .map(AttentionDetail::actions)
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(index, action)| action_item_line(index, *action, selected_action_index));
    List::new(items)
        .block(Block::new().borders(Borders::ALL).title("Command Modal"))
        .render(area, buffer);
}

fn action_item_line(
    index: usize,
    action: OperatorAction,
    selected_action_index: usize,
) -> ListItem<'static> {
    let marker = if index == selected_action_index {
        ">"
    } else {
        " "
    };
    ListItem::new(format!("{marker} {}", action.label())).style(if index == selected_action_index {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    })
}

fn overlay_rect(area: Rect) -> Rect {
    let width = (area.width.saturating_mul(3) / 4).max(1);
    let height = (area.height / 3).max(1);
    Rect::new(
        area.x + ((area.width - width) / 2),
        area.y + ((area.height - height) / 2),
        width,
        height,
    )
}

fn render_navigation(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    let items = model.navigation().iter().map(|view| {
        let label = if *view == model.active_view() {
            format!("> {}", view.label())
        } else {
            format!("  {}", view.label())
        };
        ListItem::new(label)
    });
    List::new(items)
        .block(Block::new().borders(Borders::ALL).title("Views"))
        .render(area, buffer);
}

fn render_attention(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    let items = model
        .attention_items()
        .iter()
        .enumerate()
        .map(|(index, item)| attention_item_line(model, index, item))
        .collect::<Vec<_>>();
    List::new(items)
        .block(Block::new().borders(Borders::ALL).title("Attention"))
        .render(area, buffer);
}

fn attention_item_line(
    model: &TuiScreenModel,
    index: usize,
    item: &AttentionItem,
) -> ListItem<'static> {
    let marker = if Some(index) == model.selected_attention_index() {
        ">"
    } else {
        " "
    };
    ListItem::new(format!(
        "{marker} {} [{}]",
        item.title(),
        item.next_action().label()
    ))
    .style(if Some(index) == model.selected_attention_index() {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    })
}

fn render_detail(detail: Option<&AttentionDetail>, area: Rect, buffer: &mut Buffer) {
    let lines = detail.map_or_else(
        || vec![Line::from("No attention item selected")],
        detail_lines,
    );
    Paragraph::new(lines)
        .block(Block::new().borders(Borders::ALL).title("Detail"))
        .wrap(Wrap { trim: true })
        .render(area, buffer);
}

fn detail_lines(detail: &AttentionDetail) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(format!("Repo: {}", detail.repo())),
        Line::from(format!("Work item: {}", detail.work_item())),
        Line::from(format!("Fabro run: {}", detail.fabro_run())),
        Line::from(format!("Attach: {}", detail.attach_command())),
        Line::from(format!(
            "Actions: {}",
            detail
                .actions()
                .iter()
                .map(console_application::OperatorAction::label)
                .collect::<Vec<_>>()
                .join(", ")
        )),
        Line::from("Timeline:"),
    ];
    lines.extend(detail.timeline().iter().map(timeline_line));
    lines
}

fn timeline_line(entry: &TimelineEntry) -> Line<'static> {
    Line::from(format!(
        "- {} [{}] {}",
        entry.event_id(),
        entry.source(),
        entry.label()
    ))
}

fn buffer_to_text(buffer: &Buffer, area: Rect) -> String {
    let mut rows = Vec::new();
    for y in area.top()..area.bottom() {
        let mut row = String::new();
        for x in area.left()..area.right() {
            if let Some(cell) = buffer.cell((x, y)) {
                row.push_str(cell.symbol());
            }
        }
        rows.push(row.trim_end().to_owned());
    }
    rows.join("\n")
}

#[cfg(test)]
mod tests {
    use console_application::{
        TuiInteraction, TuiInteractionState, TuiOverlay, build_tui_model, build_tui_model_for_state,
    };
    use console_domain::{ConsoleEvent, EventType};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    use super::{
        TuiRenderError, TuiTerminalInput, buffer_to_text, key_event_to_terminal_input,
        render_model, render_to_text,
    };

    #[test]
    fn keymap_maps_base_navigation_and_modal_opening() {
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Down), &TuiOverlay::None),
            Some(TuiTerminalInput::Interaction(TuiInteraction::SelectNext))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Up), &TuiOverlay::None),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::SelectPrevious
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &TuiOverlay::None),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::OpenCommandModal
            ))
        );
    }

    #[test]
    fn keymap_maps_command_modal_navigation_and_confirm() {
        let overlay = TuiOverlay::CommandModal {
            selected_action_index: 1,
        };

        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Down), &overlay),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::SelectNextAction
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Up), &overlay),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::SelectPreviousAction
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &overlay),
            Some(TuiTerminalInput::Confirm)
        );
    }

    #[test]
    fn keymap_maps_overlay_open_close_and_query_editing() {
        let search = TuiOverlay::Search {
            query: "fab".to_owned(),
        };
        let palette = TuiOverlay::CommandPalette {
            query: "dra".to_owned(),
        };

        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('/')), &TuiOverlay::None),
            Some(TuiTerminalInput::Interaction(TuiInteraction::OpenSearch))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char(':')), &TuiOverlay::None),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::OpenCommandPalette
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Esc), &search),
            Some(TuiTerminalInput::Interaction(TuiInteraction::CloseOverlay))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Backspace), &search),
            Some(TuiTerminalInput::Interaction(TuiInteraction::Backspace))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('x')), &search),
            Some(TuiTerminalInput::Interaction(TuiInteraction::TypeChar('x')))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('/')), &search),
            Some(TuiTerminalInput::Interaction(TuiInteraction::TypeChar('/')))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char(':')), &palette),
            Some(TuiTerminalInput::Interaction(TuiInteraction::TypeChar(':')))
        );
    }

    #[test]
    fn keymap_maps_quit_and_ignores_unhandled_keys() {
        let search = TuiOverlay::Search {
            query: "q".to_owned(),
        };

        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('q')), &TuiOverlay::None),
            Some(TuiTerminalInput::Quit)
        );
        assert_eq!(
            key_event_to_terminal_input(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                &TuiOverlay::None,
            ),
            Some(TuiTerminalInput::Quit)
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('q')), &search),
            Some(TuiTerminalInput::Interaction(TuiInteraction::TypeChar('q')))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('x')), &TuiOverlay::None),
            None
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Left), &TuiOverlay::None),
            None
        );
    }

    #[test]
    fn render_to_text_draws_required_tui_regions() {
        let model = build_tui_model(&demo_events(), 0);

        let output = render_to_text(&model, 96, 24);

        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("LiveSpec Console")),
            Ok(true)
        );
        assert_eq!(
            output.as_ref().map(|rendered| rendered.contains("Views")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("> Attention")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("Fabro human gate")),
            Ok(true)
        );
        assert_eq!(
            output.as_ref().map(|rendered| rendered.contains("Detail")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("Repo: livespec-console-beads-fabro")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("Actions: Acknowledge, Snooze, Open Fabro")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("attach, Copy Fabro attach")),
            Ok(true)
        );
        assert_eq!(
            output.as_ref().map(|rendered| rendered.contains("Status")),
            Ok(true)
        );
    }

    #[test]
    fn render_to_text_rejects_empty_area() {
        let model = build_tui_model(&[], 0);

        assert_eq!(
            render_to_text(&model, 0, 24),
            Err(TuiRenderError::EmptyArea)
        );
        assert_eq!(
            render_to_text(&model, 80, 0),
            Err(TuiRenderError::EmptyArea)
        );
    }

    #[test]
    fn render_model_leaves_empty_area_untouched() {
        let model = build_tui_model(&demo_events(), 0);
        let area = Rect::new(0, 0, 20, 5);
        let mut buffer = Buffer::empty(area);
        let before = buffer_to_text(&buffer, area);

        render_model(&model, Rect::new(0, 0, 0, 0), &mut buffer);

        assert_eq!(buffer_to_text(&buffer, area), before);
    }

    #[test]
    fn render_to_text_handles_empty_attention_list() {
        let model = build_tui_model(&[], 0);

        let output = render_to_text(&model, 80, 16);

        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("No attention item selected")),
            Ok(true)
        );
    }

    #[test]
    fn render_to_text_draws_search_overlay() {
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::Search {
                query: "gate".to_owned(),
            },
        );
        let model = build_tui_model_for_state(&demo_events(), &state);

        let output = render_to_text(&model, 96, 24);

        assert_eq!(
            output.as_ref().map(|rendered| rendered.contains("Search")),
            Ok(true)
        );
        assert_eq!(
            output.as_ref().map(|rendered| rendered.contains("/gate")),
            Ok(true)
        );
    }

    #[test]
    fn render_to_text_draws_command_palette_overlay() {
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::CommandPalette {
                query: "drain".to_owned(),
            },
        );
        let model = build_tui_model_for_state(&demo_events(), &state);

        let output = render_to_text(&model, 96, 24);

        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("Command Palette")),
            Ok(true)
        );
        assert_eq!(
            output.as_ref().map(|rendered| rendered.contains(":drain")),
            Ok(true)
        );
    }

    #[test]
    fn render_to_text_draws_command_modal_overlay() {
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::CommandModal {
                selected_action_index: 2,
            },
        );
        let model = build_tui_model_for_state(&demo_events(), &state);

        let output = render_to_text(&model, 96, 24);

        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("Command Modal")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("> Open Fabro attach")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("  Copy Fabro attach")),
            Ok(true)
        );
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn demo_events() -> [ConsoleEvent; 2] {
        [
            ConsoleEvent::new(
                "evt_demo_1".to_owned(),
                1,
                "factory".to_owned(),
                EventType::FabroHumanGateObserved,
                "fabro:run_demo_1".to_owned(),
                "repo:livespec-console-beads-fabro".to_owned(),
                1,
            ),
            ConsoleEvent::new(
                "evt_demo_2".to_owned(),
                1,
                "factory".to_owned(),
                EventType::DispatcherNeedsRegroomObserved,
                "dispatcher".to_owned(),
                "repo:livespec-console-beads-fabro".to_owned(),
                2,
            ),
        ]
    }
}
