#![forbid(unsafe_code)]

use console_application::source_adapters::Lane;
use console_application::{
    ApplicationError, AttentionDetail, AttentionItem, LaneColumn, LaneFocus, LaneWorkItem,
    OperatorAction, OperatorActionOutcome, TimelineEntry, TuiInteraction, TuiInteractionState,
    TuiOverlay, TuiScreenModel, TuiView, ViewSummaryItem, build_tui_model_for_state,
    reduce_tui_interaction, resolve_command_palette_action, resolve_selected_operator_action,
};
use console_domain::{CommandEnvelope, ConsoleEvent};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Widget, Wrap};

#[cfg(all(not(test), not(coverage)))]
use std::io;
#[cfg(all(not(test), not(coverage)))]
use std::time::Duration;

#[cfg(all(not(test), not(coverage)))]
use crossterm::event::{self, Event, KeyEventKind};
#[cfg(all(not(test), not(coverage)))]
use crossterm::execute;
#[cfg(all(not(test), not(coverage)))]
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
#[cfg(all(not(test), not(coverage)))]
use ratatui::Terminal;
#[cfg(all(not(test), not(coverage)))]
use ratatui::backend::CrosstermBackend;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiRenderError {
    EmptyArea,
}

pub type TuiRenderResult<T> = Result<T, TuiRenderError>;

#[cfg(all(not(test), not(coverage)))]
pub fn run_interactive_tui(
    events: &[ConsoleEvent],
    requested_by: &str,
) -> io::Result<Vec<TuiRuntimeEffect>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    if let Err(error) = execute!(stdout, EnterAlternateScreen) {
        let _raw_mode_result = disable_raw_mode();
        return Err(error);
    }
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = match Terminal::new(backend) {
        Ok(terminal) => terminal,
        Err(error) => {
            let _raw_mode_result = disable_raw_mode();
            return Err(error);
        }
    };
    let result = run_terminal_loop(&mut terminal, events, requested_by);
    let raw_mode_result = disable_raw_mode();
    let alternate_screen_result = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let cursor_result = terminal.show_cursor();
    raw_mode_result?;
    alternate_screen_result?;
    cursor_result?;
    result
}

#[cfg(all(not(test), not(coverage)))]
fn run_terminal_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    events: &[ConsoleEvent],
    requested_by: &str,
) -> io::Result<Vec<TuiRuntimeEffect>> {
    let mut state = TuiInteractionState::new(0, TuiOverlay::None);
    let mut effects = Vec::new();
    loop {
        let model = build_tui_model_for_state(events, &state);
        terminal.draw(|frame| render_model(&model, frame.area(), frame.buffer_mut()))?;
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        let Event::Key(key_event) = event::read()? else {
            continue;
        };
        if key_event.kind != KeyEventKind::Press {
            continue;
        }
        let model = build_tui_model_for_state(events, &state);
        let Some(input) = key_event_to_terminal_input(key_event, &model) else {
            continue;
        };
        let step = step_tui_runtime(&state, events, input, requested_by);
        state = step.state().clone();
        let effect = step.effect().clone();
        let should_quit = matches!(effect, TuiRuntimeEffect::Quit);
        effects.push(effect);
        if should_quit {
            return Ok(effects);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiTerminalInput {
    Interaction(TuiInteraction),
    Confirm,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiRuntimeEffect {
    Render,
    PersistCommand(CommandEnvelope),
    OpenAttachCommand(String),
    CopyAttachCommand(String),
    Quit,
    ApplicationError(ApplicationError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiRuntimeStep {
    state: TuiInteractionState,
    effect: TuiRuntimeEffect,
}

impl TuiRuntimeStep {
    #[must_use]
    pub const fn new(state: TuiInteractionState, effect: TuiRuntimeEffect) -> Self {
        Self { state, effect }
    }

    #[must_use]
    pub const fn state(&self) -> &TuiInteractionState {
        &self.state
    }

    #[must_use]
    pub const fn effect(&self) -> &TuiRuntimeEffect {
        &self.effect
    }
}

#[must_use]
pub fn step_tui_runtime(
    state: &TuiInteractionState,
    events: &[ConsoleEvent],
    input: TuiTerminalInput,
    requested_by: &str,
) -> TuiRuntimeStep {
    match input {
        TuiTerminalInput::Interaction(interaction) => TuiRuntimeStep::new(
            reduce_tui_interaction(state, events, interaction),
            TuiRuntimeEffect::Render,
        ),
        TuiTerminalInput::Confirm => confirm_operator_action(state, events, requested_by),
        TuiTerminalInput::Quit => TuiRuntimeStep::new(state.clone(), TuiRuntimeEffect::Quit),
    }
}

fn confirm_operator_action(
    state: &TuiInteractionState,
    events: &[ConsoleEvent],
    requested_by: &str,
) -> TuiRuntimeStep {
    let model = build_tui_model_for_state(events, state);
    let outcome = match model.overlay() {
        TuiOverlay::CommandPalette { .. } => resolve_command_palette_action(&model, requested_by),
        TuiOverlay::None | TuiOverlay::Search { .. } | TuiOverlay::CommandModal { .. } => {
            resolve_selected_operator_action(&model, requested_by)
        }
    };
    let effect = match outcome {
        Ok(outcome) => action_outcome_effect(outcome),
        Err(error) => TuiRuntimeEffect::ApplicationError(error),
    };
    TuiRuntimeStep::new(
        reduce_tui_interaction(state, events, TuiInteraction::CloseOverlay),
        effect,
    )
}

fn action_outcome_effect(outcome: OperatorActionOutcome) -> TuiRuntimeEffect {
    match outcome {
        OperatorActionOutcome::PersistCommand(command) => TuiRuntimeEffect::PersistCommand(command),
        OperatorActionOutcome::OpenAttachCommand(command) => {
            TuiRuntimeEffect::OpenAttachCommand(command)
        }
        OperatorActionOutcome::CopyAttachCommand(command) => {
            TuiRuntimeEffect::CopyAttachCommand(command)
        }
    }
}

#[must_use]
pub fn key_event_to_terminal_input(
    event: KeyEvent,
    model: &TuiScreenModel,
) -> Option<TuiTerminalInput> {
    let overlay = model.overlay();
    if event.modifiers.contains(KeyModifiers::CONTROL) && matches!(event.code, KeyCode::Char('c')) {
        return Some(TuiTerminalInput::Quit);
    }
    match event.code {
        KeyCode::Up => Some(TuiTerminalInput::Interaction(up_interaction(overlay))),
        KeyCode::Down => Some(TuiTerminalInput::Interaction(down_interaction(overlay))),
        KeyCode::Esc => Some(TuiTerminalInput::Interaction(esc_interaction(model))),
        KeyCode::Enter => enter_input(model),
        KeyCode::Backspace => Some(TuiTerminalInput::Interaction(TuiInteraction::Backspace)),
        KeyCode::Char('/') => slash_input(overlay),
        KeyCode::Char(':') => colon_input(overlay),
        KeyCode::Char('q') => q_input(overlay),
        KeyCode::Char(value) => text_input(value, overlay),
        KeyCode::Left => Some(TuiTerminalInput::Interaction(
            TuiInteraction::SelectPreviousView,
        )),
        KeyCode::Right => Some(TuiTerminalInput::Interaction(
            TuiInteraction::SelectNextView,
        )),
        KeyCode::Home
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

/// Enter: confirm a command modal; in the lane overview, drill into the
/// selected lane; in a drilled-in lane, Enter is inert (no per-item action yet);
/// otherwise open the command modal on the selected attention item.
fn enter_input(model: &TuiScreenModel) -> Option<TuiTerminalInput> {
    if matches!(model.overlay(), TuiOverlay::CommandModal { .. }) {
        return Some(TuiTerminalInput::Confirm);
    }
    if model.active_view() == TuiView::Lanes {
        return match model.lane_focus() {
            LaneFocus::Overview => {
                Some(TuiTerminalInput::Interaction(TuiInteraction::DrillIntoLane))
            }
            LaneFocus::Lane(_lane) => None,
        };
    }
    Some(TuiTerminalInput::Interaction(
        TuiInteraction::OpenCommandModal,
    ))
}

/// Esc: close an open overlay first; otherwise, in a drilled-in lane, return to
/// the lane overview; otherwise it is the (inert) close-overlay no-op.
fn esc_interaction(model: &TuiScreenModel) -> TuiInteraction {
    if !model.overlay().is_open()
        && model.active_view() == TuiView::Lanes
        && matches!(model.lane_focus(), LaneFocus::Lane(_lane))
    {
        return TuiInteraction::ReturnToLaneOverview;
    }
    TuiInteraction::CloseOverlay
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
    // The Lanes view spans the full body width beside the nav; the attention
    // and summary views keep the list/detail split.
    if model.active_view() == TuiView::Lanes {
        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(18), Constraint::Min(3)])
            .split(area);
        render_navigation(model, horizontal[0], buffer);
        render_lanes(model, horizontal[1], buffer);
        return;
    }
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(18),
            Constraint::Percentage(38),
            Constraint::Percentage(62),
        ])
        .split(area);
    render_navigation(model, horizontal[0], buffer);
    if model.active_view() == TuiView::Attention {
        render_attention(model, horizontal[1], buffer);
        render_detail(model.detail(), horizontal[2], buffer);
        return;
    }
    render_summary(model, horizontal[1], buffer);
    render_summary_detail(model.view_items(), horizontal[2], buffer);
}

/// The number of top rank-ordered items the lane overview previews per lane.
const LANE_OVERVIEW_PREVIEW: usize = 3;

fn render_lanes(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    match model.lane_focus() {
        LaneFocus::Overview => render_lane_overview(model, area, buffer),
        LaneFocus::Lane(lane) => render_lane_drilldown(model, lane, area, buffer),
    }
}

/// The lane-overview home: every lane with its count and a preview of its top
/// rank-ordered items, the selected lane highlighted.
fn render_lane_overview(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    let selected = model.selected_lane_index();
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (index, column) in model.lane_board().columns().iter().enumerate() {
        let is_selected = Some(index) == selected;
        let marker = if is_selected { ">" } else { " " };
        let header = Line::from(format!(
            "{marker} {} ({})",
            column.lane().label(),
            column.count()
        ));
        lines.push(if is_selected {
            header.style(Style::new().add_modifier(Modifier::BOLD))
        } else {
            header
        });
        for item in column.items().iter().take(LANE_OVERVIEW_PREVIEW) {
            lines.push(Line::from(lane_item_summary(item)));
        }
    }
    Paragraph::new(lines)
        .block(Block::new().borders(Borders::ALL).title("Lanes"))
        .render(area, buffer);
}

/// A single drilled-in lane: its full rank-ordered item list, full width.
fn render_lane_drilldown(model: &TuiScreenModel, lane: Lane, area: Rect, buffer: &mut Buffer) {
    let items: &[LaneWorkItem] = model
        .lane_board()
        .column(lane)
        .map(LaneColumn::items)
        .unwrap_or_default();
    let lines = if items.is_empty() {
        vec![Line::from("No work-items in this lane")]
    } else {
        items.iter().map(lane_item_detail).collect::<Vec<_>>()
    };
    Paragraph::new(lines)
        .block(
            Block::new()
                .borders(Borders::ALL)
                .title(format!("Lane: {}", lane.label())),
        )
        .render(area, buffer);
}

/// A compact overview-line for one work-item: id, status, and (when blocked)
/// its lane reason.
fn lane_item_summary(item: &LaneWorkItem) -> String {
    format!(
        "    - {} [{}]{}",
        item.work_item_id(),
        item.status(),
        lane_reason_suffix(item)
    )
}

/// A full drill-in line for one work-item: id, repo, rank, status, and reason.
fn lane_item_detail(item: &LaneWorkItem) -> Line<'static> {
    Line::from(format!(
        "- {}  {}  rank {}  [{}]{}",
        item.work_item_id(),
        item.repo(),
        item.rank(),
        item.status(),
        lane_reason_suffix(item)
    ))
}

/// The ` (reason)` suffix for a blocked work-item, or empty when none.
fn lane_reason_suffix(item: &LaneWorkItem) -> String {
    item.lane_reason()
        .map(|reason| format!(" ({})", reason.label()))
        .unwrap_or_default()
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

fn render_summary(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    let items = model
        .view_items()
        .iter()
        .map(|item| ListItem::new(format!("  {}", item.title())));
    List::new(items)
        .block(
            Block::new()
                .borders(Borders::ALL)
                .title(model.active_view().label()),
        )
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
    let label = item.next_action().map_or_else(
        || format!("{marker} {}", item.title()),
        |action| format!("{marker} {} [{}]", item.title(), action.label()),
    );
    ListItem::new(label).style(if Some(index) == model.selected_attention_index() {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    })
}

fn render_summary_detail(items: &[ViewSummaryItem], area: Rect, buffer: &mut Buffer) {
    let lines = if items.is_empty() {
        vec![Line::from("No projection rows")]
    } else {
        items
            .iter()
            .flat_map(|item| {
                [
                    Line::from(format!("{}:", item.title())),
                    Line::from(item.detail().to_owned()),
                ]
            })
            .collect::<Vec<_>>()
    };
    Paragraph::new(lines)
        .block(Block::new().borders(Borders::ALL).title("Detail"))
        .wrap(Wrap { trim: true })
        .render(area, buffer);
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
    ];
    if !detail.actions().is_empty() {
        lines.push(Line::from(format!(
            "Actions: {}",
            detail
                .actions()
                .iter()
                .map(console_application::OperatorAction::label)
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    lines.push(Line::from("Timeline:"));
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
    use console_application::source_adapters::Lane;
    #[cfg(test)]
    use console_application::source_adapters::LaneReason;
    use console_application::{
        AttentionDetail, AttentionItem, LaneFocus, OperatorAction, OperatorActionOutcome,
        TuiInteraction, TuiInteractionState, TuiOverlay, TuiScreenModel, TuiView, build_tui_model,
        build_tui_model_for_state,
    };
    use console_domain::{CommandType, ConsoleEvent, EventType};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    use super::{
        TuiRenderError, TuiRuntimeEffect, TuiTerminalInput, action_outcome_effect,
        attention_item_line, buffer_to_text, detail_lines, key_event_to_terminal_input,
        render_command_modal, render_model, render_summary_detail, render_to_text,
        step_tui_runtime,
    };

    #[test]
    fn keymap_maps_base_navigation_and_modal_opening() {
        let model = attention_model(TuiOverlay::None);
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Down), &model),
            Some(TuiTerminalInput::Interaction(TuiInteraction::SelectNext))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Up), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::SelectPrevious
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::OpenCommandModal
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Right), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::SelectNextView
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Left), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::SelectPreviousView
            ))
        );
    }

    #[test]
    fn keymap_routes_enter_and_esc_through_the_lane_sub_view() {
        let overview = lanes_model(LaneFocus::Overview, TuiOverlay::None);
        // Enter drills into the selected lane from the overview.
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &overview),
            Some(TuiTerminalInput::Interaction(TuiInteraction::DrillIntoLane))
        );
        // Esc in the overview (no overlay open) is the inert close-overlay.
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Esc), &overview),
            Some(TuiTerminalInput::Interaction(TuiInteraction::CloseOverlay))
        );

        let drilled = lanes_model(LaneFocus::Lane(Lane::Ready), TuiOverlay::None);
        // Enter is inert while a lane is drilled in (no per-item action yet).
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &drilled),
            None
        );
        // Esc returns to the overview from a drilled-in lane.
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Esc), &drilled),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::ReturnToLaneOverview
            ))
        );

        // With an overlay open, Esc closes it first even while drilled in.
        let drilled_with_overlay = lanes_model(
            LaneFocus::Lane(Lane::Ready),
            TuiOverlay::Search {
                query: "x".to_owned(),
            },
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Esc), &drilled_with_overlay),
            Some(TuiTerminalInput::Interaction(TuiInteraction::CloseOverlay))
        );
    }

    #[test]
    fn keymap_maps_command_modal_navigation_and_confirm() {
        let model = attention_model(TuiOverlay::CommandModal {
            selected_action_index: 1,
        });

        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Down), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::SelectNextAction
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Up), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::SelectPreviousAction
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &model),
            Some(TuiTerminalInput::Confirm)
        );
    }

    #[test]
    fn keymap_maps_overlay_open_close_and_query_editing() {
        let none = attention_model(TuiOverlay::None);
        let search = attention_model(TuiOverlay::Search {
            query: "fab".to_owned(),
        });
        let palette = attention_model(TuiOverlay::CommandPalette {
            query: "dra".to_owned(),
        });

        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('/')), &none),
            Some(TuiTerminalInput::Interaction(TuiInteraction::OpenSearch))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char(':')), &none),
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
        let none = attention_model(TuiOverlay::None);
        let search = attention_model(TuiOverlay::Search {
            query: "q".to_owned(),
        });

        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('q')), &none),
            Some(TuiTerminalInput::Quit)
        );
        assert_eq!(
            key_event_to_terminal_input(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                &none,
            ),
            Some(TuiTerminalInput::Quit)
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('q')), &search),
            Some(TuiTerminalInput::Interaction(TuiInteraction::TypeChar('q')))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('x')), &none),
            None
        );
        assert_eq!(key_event_to_terminal_input(key(KeyCode::Home), &none), None);
    }

    #[test]
    fn runtime_step_applies_interaction_without_side_effects() {
        let state = TuiInteractionState::new(0, TuiOverlay::None);
        let step = step_tui_runtime(
            &state,
            &demo_events(),
            TuiTerminalInput::Interaction(TuiInteraction::SelectNext),
            "operator",
        );

        assert_eq!(step.state().selected_attention_index(), 1);
        assert_eq!(step.effect(), &TuiRuntimeEffect::Render);
    }

    #[test]
    fn runtime_step_applies_view_navigation_without_side_effects() {
        let state = TuiInteractionState::new(0, TuiOverlay::None);
        let step = step_tui_runtime(
            &state,
            &demo_events(),
            TuiTerminalInput::Interaction(TuiInteraction::SelectNextView),
            "operator",
        );

        assert_eq!(step.state().active_view(), TuiView::Spec);
        assert_eq!(step.effect(), &TuiRuntimeEffect::Render);
    }

    #[test]
    fn runtime_step_turns_command_palette_drain_into_persisted_command_effect() {
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::CommandPalette {
                query: "drain".to_owned(),
            },
        );
        let step = step_tui_runtime(
            &state,
            &demo_events(),
            TuiTerminalInput::Confirm,
            "operator",
        );

        let command = persisted_command(step.effect());
        assert_eq!(
            command.map(console_domain::CommandEnvelope::command_type),
            Some(&CommandType::FactoryDrainRequested)
        );
        assert_eq!(
            command.map(console_domain::CommandEnvelope::aggregate_id),
            Some("fleet:livespec")
        );
        assert_eq!(step.state().overlay(), &TuiOverlay::None);
    }

    #[test]
    fn runtime_step_has_no_deleted_attention_command_effects() {
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::CommandModal {
                selected_action_index: 0,
            },
        );
        let step = step_tui_runtime(
            &state,
            &demo_events(),
            TuiTerminalInput::Confirm,
            "operator",
        );

        assert_eq!(
            step.effect(),
            &TuiRuntimeEffect::ApplicationError(
                console_application::ApplicationError::NoSelectedOperatorAction,
            )
        );
        assert_eq!(persisted_command(step.effect()), None);
        assert_eq!(step.state().overlay(), &TuiOverlay::None);
    }

    #[test]
    fn runtime_step_reports_application_errors_for_invalid_confirmation() {
        let state = TuiInteractionState::new(0, TuiOverlay::None);
        let step = step_tui_runtime(
            &state,
            &demo_events(),
            TuiTerminalInput::Confirm,
            "operator",
        );

        assert_eq!(
            step.effect(),
            &TuiRuntimeEffect::ApplicationError(
                console_application::ApplicationError::NoSelectedOperatorAction
            )
        );
        assert_eq!(persisted_command(step.effect()), None);
    }

    #[test]
    fn runtime_step_quit_preserves_state() {
        let state = TuiInteractionState::new(
            1,
            TuiOverlay::Search {
                query: "gate".to_owned(),
            },
        );
        let step = step_tui_runtime(&state, &demo_events(), TuiTerminalInput::Quit, "operator");

        assert_eq!(step.state(), &state);
        assert_eq!(step.effect(), &TuiRuntimeEffect::Quit);
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
                .map(|rendered| rendered.contains("Blocked: needs-human")),
            Ok(true)
        );
        assert_eq!(
            output.as_ref().map(|rendered| rendered.contains("Detail")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("Repo: console")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| !rendered.contains("Actions:")),
            Ok(true)
        );
        assert_eq!(
            output.as_ref().map(|rendered| rendered.contains("Status")),
            Ok(true)
        );
    }

    #[test]
    fn render_to_text_draws_non_attention_view_summary() {
        let state = TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None);
        let model = build_tui_model_for_state(&factory_events(), &state);

        let output = render_to_text(&model, 96, 24);

        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("> Events")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("Stored events: 2")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("The event log is the canonical source")),
            Ok(true)
        );
    }

    #[test]
    fn render_to_text_draws_the_lane_overview_with_counts_and_top_items() {
        // Lane index 2 is `ready` in canonical order; select it for the marker.
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_selected_lane_index(2);
        let model = build_tui_model_for_state(&lane_render_events(), &state);

        let output = render_to_text(&model, 96, 24);

        // The board title and a count per lane.
        assert_eq!(output.as_ref().map(|r| r.contains("Lanes")), Ok(true));
        assert_eq!(output.as_ref().map(|r| r.contains("ready (2)")), Ok(true));
        assert_eq!(output.as_ref().map(|r| r.contains("blocked (1)")), Ok(true));
        // The selected lane row (index 2 == ready) is marked.
        assert_eq!(output.as_ref().map(|r| r.contains("> ready (2)")), Ok(true));
        // Top rank-ordered items are previewed under their lane, with the
        // blocked item carrying its lane reason.
        assert_eq!(
            output
                .as_ref()
                .map(|r| r.contains("- console-ready-a [ready]")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|r| r.contains("- console-blocked [blocked] (needs-human)")),
            Ok(true)
        );
    }

    #[test]
    fn render_to_text_drills_into_a_lane_with_a_full_item_list() {
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_lane_focus(LaneFocus::Lane(Lane::Ready));
        let model = build_tui_model_for_state(&lane_render_events(), &state);

        let output = render_to_text(&model, 96, 24);

        assert_eq!(output.as_ref().map(|r| r.contains("Lane: ready")), Ok(true));
        // The drill-in shows repo + rank alongside the id.
        assert_eq!(
            output
                .as_ref()
                .map(|r| r.contains("- console-ready-a  console  rank a0  [ready]")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|r| r.contains("- console-ready-b  console  rank a1  [ready]")),
            Ok(true)
        );
    }

    #[test]
    fn render_to_text_drills_into_an_empty_lane_with_a_placeholder() {
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_lane_focus(LaneFocus::Lane(Lane::Done));
        let model = build_tui_model_for_state(&lane_render_events(), &state);

        let output = render_to_text(&model, 96, 24);

        assert_eq!(output.as_ref().map(|r| r.contains("Lane: done")), Ok(true));
        assert_eq!(
            output
                .as_ref()
                .map(|r| r.contains("No work-items in this lane")),
            Ok(true)
        );
    }

    #[test]
    fn render_summary_detail_draws_empty_projection_state() {
        let area = Rect::new(0, 0, 40, 5);
        let mut buffer = Buffer::empty(area);

        render_summary_detail(&[], area, &mut buffer);

        assert!(buffer_to_text(&buffer, area).contains("No projection rows"));
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
    fn render_command_modal_draws_available_attach_actions() {
        let detail = AttentionDetail::new(
            "repo".to_owned(),
            "work-item".to_owned(),
            "run".to_owned(),
            "fabro attach run".to_owned(),
            vec![],
            vec![
                OperatorAction::OpenFabroAttach,
                OperatorAction::CopyFabroAttach,
            ],
        );
        let area = Rect::new(0, 0, 40, 6);
        let mut buffer = Buffer::empty(area);

        render_command_modal(Some(&detail), 1, area, &mut buffer);
        let output = buffer_to_text(&buffer, area);

        assert!(output.contains("Open Fabro attach"));
        assert!(output.contains("> Copy Fabro attach"));
    }

    #[test]
    fn detail_lines_include_attach_actions_when_present() {
        let detail = AttentionDetail::new(
            "repo".to_owned(),
            "work-item".to_owned(),
            "run".to_owned(),
            "fabro attach run".to_owned(),
            vec![],
            vec![
                OperatorAction::OpenFabroAttach,
                OperatorAction::CopyFabroAttach,
            ],
        );

        let rendered = detail_lines(&detail)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("Actions: Open Fabro attach, Copy Fabro attach"));
    }

    #[test]
    fn attention_item_line_keeps_optional_next_action_label() {
        let model = attention_model(TuiOverlay::None);
        let item = AttentionItem::new(
            "work-item".to_owned(),
            "Needs review".to_owned(),
            "source".to_owned(),
            "repo".to_owned(),
            Some(OperatorAction::OpenFabroAttach),
        );

        let rendered = format!("{:?}", attention_item_line(&model, 0, &item));

        assert!(rendered.contains("> Needs review [Open Fabro attach]"));
    }

    #[test]
    fn action_outcome_effect_maps_attach_outcomes() {
        assert_eq!(
            action_outcome_effect(OperatorActionOutcome::OpenAttachCommand(
                "fabro attach run".to_owned()
            )),
            TuiRuntimeEffect::OpenAttachCommand("fabro attach run".to_owned())
        );
        assert_eq!(
            action_outcome_effect(OperatorActionOutcome::CopyAttachCommand(
                "fabro attach run".to_owned()
            )),
            TuiRuntimeEffect::CopyAttachCommand("fabro attach run".to_owned())
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
                .map(|rendered| !rendered.contains("Open Fabro attach")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| !rendered.contains("Copy Fabro attach")),
            Ok(true)
        );
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn persisted_command(effect: &TuiRuntimeEffect) -> Option<&console_domain::CommandEnvelope> {
        match effect {
            TuiRuntimeEffect::PersistCommand(command) => Some(command),
            TuiRuntimeEffect::Render
            | TuiRuntimeEffect::OpenAttachCommand(_)
            | TuiRuntimeEffect::CopyAttachCommand(_)
            | TuiRuntimeEffect::Quit
            | TuiRuntimeEffect::ApplicationError(_) => None,
        }
    }

    fn demo_events() -> [ConsoleEvent; 2] {
        [
            lane_event(
                "evt_demo_1",
                "console-blocked",
                Lane::Blocked,
                Some(LaneReason::NeedsHuman),
                "a0",
                "blocked",
            ),
            lane_event(
                "evt_demo_2",
                "console-accept",
                Lane::Acceptance,
                None,
                "a1",
                "acceptance",
            ),
        ]
    }

    fn factory_events() -> [ConsoleEvent; 2] {
        [
            ConsoleEvent::new(
                "evt_drain".to_owned(),
                1,
                "console".to_owned(),
                EventType::FactoryDrainRequested,
                "console:factory-command-handler".to_owned(),
                "factory:livespec-console-beads-fabro".to_owned(),
                1,
            ),
            ConsoleEvent::new(
                "evt_done".to_owned(),
                1,
                "console".to_owned(),
                EventType::FactoryDrainCompleted,
                "console:factory-command-handler".to_owned(),
                "factory:livespec-console-beads-fabro".to_owned(),
                2,
            ),
        ]
    }

    /// An Attention-view model carrying the given overlay, for keymap tests
    /// that exercise overlay-driven behavior in the default view.
    fn attention_model(overlay: TuiOverlay) -> TuiScreenModel {
        build_tui_model_for_state(&demo_events(), &TuiInteractionState::new(0, overlay))
    }

    /// A Lanes-view model in the given lane focus + overlay, over a small board.
    fn lanes_model(lane_focus: LaneFocus, overlay: TuiOverlay) -> TuiScreenModel {
        let state =
            TuiInteractionState::for_view(TuiView::Lanes, 0, overlay).with_lane_focus(lane_focus);
        build_tui_model_for_state(&lane_render_events(), &state)
    }

    /// A small board fixture: two ready items and one blocked (needs-human).
    fn lane_render_events() -> [ConsoleEvent; 3] {
        [
            lane_event(
                "evt_ra",
                "console-ready-a",
                Lane::Ready,
                None,
                "a0",
                "ready",
            ),
            lane_event(
                "evt_rb",
                "console-ready-b",
                Lane::Ready,
                None,
                "a1",
                "ready",
            ),
            lane_event(
                "evt_bl",
                "console-blocked",
                Lane::Blocked,
                Some(LaneReason::NeedsHuman),
                "a0",
                "blocked",
            ),
        ]
    }

    // Build a snapshot-observation event by writing the canonical `payload_json`
    // directly, mirroring the orchestrator emission the lane board rebuilds from.
    fn lane_event(
        event_id: &str,
        work_item_id: &str,
        lane: Lane,
        lane_reason: Option<LaneReason>,
        rank: &str,
        status: &str,
    ) -> ConsoleEvent {
        let reason_json = lane_reason.map_or_else(
            || "null".to_owned(),
            |reason| format!("\"{}\"", reason.label()),
        );
        let payload = format!(
            r#"{{"repo":"console","work_item_id":"{work_item_id}","lane":"{}","lane_reason":{reason_json},"rank":"{rank}","status":"{status}","source_version":1}}"#,
            lane.label()
        );
        ConsoleEvent::fixture(
            event_id,
            EventType::WorkItemSnapshotObserved,
            "orchestrator",
        )
        .with_payload_json(payload)
    }
}
