//! Terminal UI rendering and interaction runtime for the operator console.
//!
//! This crate maps keyboard input to application interactions, steps the TUI
//! runtime, renders [`console_application::TuiScreenModel`] values with ratatui,
//! and exposes a text renderer for tests and CLI previews.
//!
//! ```rust,ignore
//! use console_application::build_tui_model;
//! use console_tui::render_to_text;
//!
//! let model = build_tui_model(&[], 0);
//! let rendered = render_to_text(&model, 80, 24)?;
//! assert!(rendered.contains("Attention"));
//! # Ok::<(), console_tui::TuiRenderError>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use console_application::source_adapters::{AcceptancePolicy, AdmissionPolicy, Lane};
use console_application::{
    ApplicationError, AttentionDetail, AttentionItem, FocusPane, LaneColumn, LaneFocus,
    LaneWorkItem, OperatorAction, OperatorActionOutcome, PendingValve, RejectMode, TimelineEntry,
    TuiInteraction, TuiInteractionState, TuiOverlay, TuiScreenModel, TuiView, ViewSummaryItem,
    build_tui_model_for_state, reduce_tui_interaction, resolve_autonomous_mode_disable,
    resolve_autonomous_mode_enable, resolve_command_palette_action,
    resolve_selected_operator_action, resolve_valve_action,
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
/// Variants for tui render error state or outcome values.
pub enum TuiRenderError {
    /// Empty area variant.
    EmptyArea,
}

/// Type alias for tui render result values.
pub type TuiRenderResult<T> = Result<T, TuiRenderError>;

#[cfg(all(not(test), not(coverage)))]
/// Run interactive tui and return its outcome.
pub fn run_interactive_tui(
    events: &[ConsoleEvent],
    requested_by: &str,
    selected_repo: &str,
    autonomous_mode_enabled: bool,
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
    let result = run_terminal_loop(
        &mut terminal,
        events,
        requested_by,
        selected_repo,
        autonomous_mode_enabled,
    );
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
    selected_repo: &str,
    autonomous_mode_enabled: bool,
) -> io::Result<Vec<TuiRuntimeEffect>> {
    let mut state = TuiInteractionState::new(0, TuiOverlay::None)
        .with_selected_repo(selected_repo.to_owned())
        .with_autonomous_mode_enabled(autonomous_mode_enabled);
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
/// Variants for tui terminal input state or outcome values.
pub enum TuiTerminalInput {
    /// Interaction variant.
    Interaction(TuiInteraction),
    /// Confirm variant.
    Confirm,
    /// Toggle the selected repo's autonomous mode: enabling opens the dangerous
    /// type-to-confirm modal, disabling submits directly with no confirmation.
    ToggleAutonomousMode,
    /// Quit variant.
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Variants for tui runtime effect state or outcome values.
pub enum TuiRuntimeEffect {
    /// Render variant.
    Render,
    /// Persist command variant.
    PersistCommand(CommandEnvelope),
    /// Persist a command carrying an operator-supplied JSON payload (the
    /// autonomous-mode arming command's `{ repo, enabled, confirmed }`).
    PersistCommandWithPayload {
        /// The command envelope to persist.
        command: CommandEnvelope,
        /// The command's `{ ... }` payload JSON.
        payload_json: String,
    },
    /// Open attach command variant.
    OpenAttachCommand(String),
    /// Copy attach command variant.
    CopyAttachCommand(String),
    /// Quit variant.
    Quit,
    /// Application error variant.
    ApplicationError(ApplicationError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents tui runtime step data used by the console.
pub struct TuiRuntimeStep {
    state: TuiInteractionState,
    effect: TuiRuntimeEffect,
}

impl TuiRuntimeStep {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(state: TuiInteractionState, effect: TuiRuntimeEffect) -> Self {
        Self { state, effect }
    }

    #[must_use]
    /// Return the stored value.
    pub const fn state(&self) -> &TuiInteractionState {
        &self.state
    }

    #[must_use]
    /// Return the stored value.
    pub const fn effect(&self) -> &TuiRuntimeEffect {
        &self.effect
    }
}

#[must_use]
/// Return the step tui runtime value.
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
        TuiTerminalInput::ToggleAutonomousMode => {
            toggle_autonomous_mode(state, events, requested_by)
        }
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
        TuiOverlay::AutonomousModeConfirm { .. } => {
            resolve_autonomous_mode_enable(&model, requested_by)
        }
        TuiOverlay::ValveConfirm { .. } => resolve_valve_action(&model, requested_by),
        TuiOverlay::None
        | TuiOverlay::Search { .. }
        | TuiOverlay::CommandModal { .. }
        | TuiOverlay::Help => resolve_selected_operator_action(&model, requested_by),
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

/// Toggle the selected repo's autonomous mode. Enabling is dangerous, so it
/// opens the type-to-confirm modal (no submit yet); disabling requires no
/// confirmation, so it submits the disarm command directly with the overlay
/// unchanged.
fn toggle_autonomous_mode(
    state: &TuiInteractionState,
    events: &[ConsoleEvent],
    requested_by: &str,
) -> TuiRuntimeStep {
    let model = build_tui_model_for_state(events, state);
    if model.autonomous_mode_enabled() {
        let effect = match resolve_autonomous_mode_disable(&model, requested_by) {
            Ok(outcome) => action_outcome_effect(outcome),
            Err(error) => TuiRuntimeEffect::ApplicationError(error),
        };
        return TuiRuntimeStep::new(state.clone(), effect);
    }
    TuiRuntimeStep::new(
        reduce_tui_interaction(state, events, TuiInteraction::OpenAutonomousModeConfirm),
        TuiRuntimeEffect::Render,
    )
}

fn action_outcome_effect(outcome: OperatorActionOutcome) -> TuiRuntimeEffect {
    match outcome {
        OperatorActionOutcome::PersistCommand(command) => TuiRuntimeEffect::PersistCommand(command),
        OperatorActionOutcome::PersistCommandWithPayload {
            command,
            payload_json,
        } => TuiRuntimeEffect::PersistCommandWithPayload {
            command,
            payload_json,
        },
        OperatorActionOutcome::OpenAttachCommand(command) => {
            TuiRuntimeEffect::OpenAttachCommand(command)
        }
        OperatorActionOutcome::CopyAttachCommand(command) => {
            TuiRuntimeEffect::CopyAttachCommand(command)
        }
    }
}

#[must_use]
/// Return the key event to terminal input value.
pub fn key_event_to_terminal_input(
    event: KeyEvent,
    model: &TuiScreenModel,
) -> Option<TuiTerminalInput> {
    let overlay = model.overlay();
    if event.modifiers.contains(KeyModifiers::CONTROL) && matches!(event.code, KeyCode::Char('c')) {
        return Some(TuiTerminalInput::Quit);
    }
    match event.code {
        KeyCode::Up => Some(TuiTerminalInput::Interaction(up_interaction(model))),
        KeyCode::Down => Some(TuiTerminalInput::Interaction(down_interaction(model))),
        KeyCode::Esc => Some(TuiTerminalInput::Interaction(esc_interaction(model))),
        KeyCode::Enter => enter_input(model),
        KeyCode::Backspace => Some(TuiTerminalInput::Interaction(TuiInteraction::Backspace)),
        KeyCode::Char('/') => slash_input(overlay),
        KeyCode::Char(':') => colon_input(overlay),
        KeyCode::Char('?') => question_input(overlay),
        KeyCode::Char('q') => q_input(overlay),
        KeyCode::Char('a') => autonomous_toggle_input(overlay),
        KeyCode::Char('p') => valve_open_input(model, PendingValve::Approve, 'p'),
        KeyCode::Char('c') => valve_open_input(model, PendingValve::Accept, 'c'),
        KeyCode::Char('r') => {
            valve_open_input(model, PendingValve::Reject(RejectMode::Rework), 'r')
        }
        KeyCode::Char('m') => valve_open_input(
            model,
            PendingValve::SetAdmission(AdmissionPolicy::Manual),
            'm',
        ),
        KeyCode::Char('n') => valve_open_input(
            model,
            PendingValve::SetAcceptance(AcceptancePolicy::AiThenHuman),
            'n',
        ),
        KeyCode::Char(value) => text_input(value, overlay),
        KeyCode::Left => left_input(model),
        KeyCode::Right => right_input(model),
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

/// Up: in a command modal, move the action selection; with no overlay open,
/// move within the focused pane (the Views nav when focus is on the nav, else
/// the content list); behind any other overlay it is the harmless content move.
const fn up_interaction(model: &TuiScreenModel) -> TuiInteraction {
    match model.overlay() {
        TuiOverlay::CommandModal { .. } => TuiInteraction::SelectPreviousAction,
        TuiOverlay::ValveConfirm { .. } => TuiInteraction::CycleValveOption(false),
        TuiOverlay::None => match model.focus() {
            FocusPane::Nav => TuiInteraction::SelectPreviousView,
            FocusPane::Content => TuiInteraction::SelectPrevious,
        },
        TuiOverlay::Search { .. }
        | TuiOverlay::CommandPalette { .. }
        | TuiOverlay::AutonomousModeConfirm { .. }
        | TuiOverlay::Help => TuiInteraction::SelectPrevious,
    }
}

/// Down: the mirror of [`up_interaction`].
const fn down_interaction(model: &TuiScreenModel) -> TuiInteraction {
    match model.overlay() {
        TuiOverlay::CommandModal { .. } => TuiInteraction::SelectNextAction,
        TuiOverlay::ValveConfirm { .. } => TuiInteraction::CycleValveOption(true),
        TuiOverlay::None => match model.focus() {
            FocusPane::Nav => TuiInteraction::SelectNextView,
            FocusPane::Content => TuiInteraction::SelectNext,
        },
        TuiOverlay::Search { .. }
        | TuiOverlay::CommandPalette { .. }
        | TuiOverlay::AutonomousModeConfirm { .. }
        | TuiOverlay::Help => TuiInteraction::SelectNext,
    }
}

/// Enter: confirm a command modal / autonomous-confirm modal; behind a
/// text/help overlay it is inert; with no overlay open it dives into the
/// focused pane (see [`enter_content_input`]).
fn enter_input(model: &TuiScreenModel) -> Option<TuiTerminalInput> {
    match model.overlay() {
        TuiOverlay::CommandModal { .. }
        | TuiOverlay::AutonomousModeConfirm { .. }
        | TuiOverlay::ValveConfirm { .. } => Some(TuiTerminalInput::Confirm),
        TuiOverlay::Search { .. } | TuiOverlay::CommandPalette { .. } | TuiOverlay::Help => None,
        TuiOverlay::None => enter_content_input(model),
    }
}

/// Enter with no overlay open: from the Views nav it dives focus into the
/// Content pane; in the Content pane it drills into the selected lane (lane
/// overview), is inert in a drilled-in lane, or opens the command modal on the
/// selected attention item for any other view.
fn enter_content_input(model: &TuiScreenModel) -> Option<TuiTerminalInput> {
    match model.focus() {
        FocusPane::Nav => Some(TuiTerminalInput::Interaction(TuiInteraction::FocusContent)),
        FocusPane::Content => {
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
    }
}

/// Esc: close an open overlay first; with no overlay open and focus on the
/// Content pane, step back (return a drilled-in lane to its overview, else
/// return focus to the Views nav); on the nav it is the inert close-overlay.
fn esc_interaction(model: &TuiScreenModel) -> TuiInteraction {
    if model.overlay().is_open() {
        return TuiInteraction::CloseOverlay;
    }
    match model.focus() {
        FocusPane::Content => content_back_interaction(model),
        FocusPane::Nav => TuiInteraction::CloseOverlay,
    }
}

/// The Content-pane "step back" interaction shared by Esc and Left: a drilled-in
/// lane returns to its overview first, otherwise focus returns to the Views nav.
fn content_back_interaction(model: &TuiScreenModel) -> TuiInteraction {
    if model.active_view() == TuiView::Lanes && matches!(model.lane_focus(), LaneFocus::Lane(_lane))
    {
        return TuiInteraction::ReturnToLaneOverview;
    }
    TuiInteraction::FocusNav
}

/// Left: behind an overlay it is inert; on the Views nav it switches to the
/// previous view (the global bonus switcher); in the Content pane it steps back
/// (see [`content_back_interaction`]).
fn left_input(model: &TuiScreenModel) -> Option<TuiTerminalInput> {
    if model.overlay().is_open() {
        return None;
    }
    Some(TuiTerminalInput::Interaction(match model.focus() {
        FocusPane::Nav => TuiInteraction::SelectPreviousView,
        FocusPane::Content => content_back_interaction(model),
    }))
}

/// Right: behind an overlay it is inert; on the Views nav it dives focus into
/// the Content pane; in the Content pane it switches to the next view (the
/// global bonus switcher).
const fn right_input(model: &TuiScreenModel) -> Option<TuiTerminalInput> {
    if model.overlay().is_open() {
        return None;
    }
    Some(TuiTerminalInput::Interaction(match model.focus() {
        FocusPane::Nav => TuiInteraction::FocusContent,
        FocusPane::Content => TuiInteraction::SelectNextView,
    }))
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

/// `?`: with no overlay open, open the Help overlay; with Help open, close it
/// (so `?` toggles); otherwise it is a literal character typed into the open
/// text overlay.
const fn question_input(overlay: &TuiOverlay) -> Option<TuiTerminalInput> {
    match overlay {
        TuiOverlay::None => Some(TuiTerminalInput::Interaction(TuiInteraction::OpenHelp)),
        TuiOverlay::Help => Some(TuiTerminalInput::Interaction(TuiInteraction::CloseOverlay)),
        TuiOverlay::Search { .. }
        | TuiOverlay::CommandPalette { .. }
        | TuiOverlay::CommandModal { .. }
        | TuiOverlay::AutonomousModeConfirm { .. }
        | TuiOverlay::ValveConfirm { .. } => text_input('?', overlay),
    }
}

const fn q_input(overlay: &TuiOverlay) -> Option<TuiTerminalInput> {
    if matches!(overlay, TuiOverlay::None) {
        return Some(TuiTerminalInput::Quit);
    }
    text_input('q', overlay)
}

/// `a`: with no overlay open, toggle the selected repo's autonomous mode;
/// otherwise it is a literal character typed into the open text overlay.
const fn autonomous_toggle_input(overlay: &TuiOverlay) -> Option<TuiTerminalInput> {
    if matches!(overlay, TuiOverlay::None) {
        return Some(TuiTerminalInput::ToggleAutonomousMode);
    }
    text_input('a', overlay)
}

/// A valve key (`p`/`c`/`r`/`m`/`n`): with no overlay open and a selected
/// work-item in the Attention view, open the valve-confirm modal staging the
/// given valve; on any other view or with nothing selected it is inert; behind
/// an open text overlay it is a literal character.
fn valve_open_input(
    model: &TuiScreenModel,
    valve: PendingValve,
    character: char,
) -> Option<TuiTerminalInput> {
    let overlay = model.overlay();
    if matches!(overlay, TuiOverlay::None) {
        if model.active_view() == TuiView::Attention && model.detail().is_some() {
            return Some(TuiTerminalInput::Interaction(
                TuiInteraction::OpenValveConfirm(valve),
            ));
        }
        return None;
    }
    text_input(character, overlay)
}

const fn text_input(value: char, overlay: &TuiOverlay) -> Option<TuiTerminalInput> {
    if matches!(
        overlay,
        TuiOverlay::Search { .. }
            | TuiOverlay::CommandPalette { .. }
            | TuiOverlay::AutonomousModeConfirm { .. }
    ) {
        return Some(TuiTerminalInput::Interaction(TuiInteraction::TypeChar(
            value,
        )));
    }
    None
}

/// Return the render to text value.
pub fn render_to_text(model: &TuiScreenModel, width: u16, height: u16) -> TuiRenderResult<String> {
    if width == 0 || height == 0 {
        return Err(TuiRenderError::EmptyArea);
    }
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    render_model(model, area, &mut buffer);
    Ok(buffer_to_text(&buffer, area))
}

/// Return the render model value.
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
    let title = focus_title("Lanes", content_focused(model));
    Paragraph::new(lines)
        .block(Block::new().borders(Borders::ALL).title(title))
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
    let title = focus_title(&format!("Lane: {}", lane.label()), content_focused(model));
    Paragraph::new(lines)
        .block(Block::new().borders(Borders::ALL).title(title))
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
        TuiOverlay::AutonomousModeConfirm { typed } => {
            render_autonomous_mode_confirm(
                model.selected_repo(),
                typed,
                overlay_rect(area),
                buffer,
            );
        }
        TuiOverlay::ValveConfirm { valve } => {
            render_valve_confirm(
                *valve,
                model.detail().map_or("", AttentionDetail::work_item),
                overlay_rect(area),
                buffer,
            );
        }
        TuiOverlay::Help => render_help_overlay(overlay_rect(area), buffer),
    }
}

/// Render the valve-confirm modal: the staged valve, its target work-item, the
/// dialed-in mode/policy for a payload valve (cycled with up/down), and a
/// "dangerous / use with caution" caution before a destructive reject. `Enter`
/// submits; `Esc` cancels.
fn render_valve_confirm(valve: PendingValve, work_item: &str, area: Rect, buffer: &mut Buffer) {
    Clear.render(area, buffer);
    let mut lines = vec![
        Line::from(format!("{} work-item", valve.valve_label())),
        Line::from(format!("Target: {work_item}")),
    ];
    if let Some(option) = valve.option_label() {
        lines.push(Line::from(format!(
            "Policy/mode: {option}  (up/down to change)"
        )));
    }
    if valve.is_destructive() {
        lines.push(
            Line::from("dangerous / use with caution")
                .style(Style::new().add_modifier(Modifier::BOLD)),
        );
    }
    lines.push(Line::from("Enter to confirm | Esc to cancel"));
    Paragraph::new(lines)
        .block(Block::new().borders(Borders::ALL).title("Valve"))
        .render(area, buffer);
}

/// Render the read-only Help overlay: a one-line "how to use" note plus every
/// keybinding with a short description. The text MUST stay in lock-step with the
/// key handler and the footer hint.
fn render_help_overlay(area: Rect, buffer: &mut Buffer) {
    Clear.render(area, buffer);
    let lines = vec![
        Line::from("Navigate the Views menu with up/down; Enter drills in, Esc goes back."),
        Line::from("Toggle autonomous mode with a; drain via :; quit with q."),
        Line::from(""),
        Line::from("up / down    move within the focused pane (Views nav or Content list)"),
        Line::from("enter        dive from the nav into content, or open the selected item"),
        Line::from("esc          step back (content -> nav; drilled lane -> overview)"),
        Line::from("left / right  previous / next view, or step out / in of the content pane"),
        Line::from("/            open search"),
        Line::from(":            open the command palette (drain)"),
        Line::from("a            toggle autonomous mode (dangerous / type-to-confirm)"),
        Line::from("p / c / r    approve / accept / reject the selected work-item (confirm modal)"),
        Line::from("m / n        set-admission / set-acceptance policy for the selected work-item"),
        Line::from("             (in a valve modal: up/down change mode/policy, Enter confirms)"),
        Line::from("q / ctrl-c   quit"),
        Line::from("?            toggle this help"),
        Line::from(""),
        Line::from("Esc or ? closes this help."),
    ];
    Paragraph::new(lines)
        .block(Block::new().borders(Borders::ALL).title("Help"))
        .wrap(Wrap { trim: true })
        .render(area, buffer);
}

/// Render the dangerous autonomous-mode type-to-confirm modal: the enable is
/// labelled "dangerous / use with caution" and gated until the operator types
/// the repo name.
fn render_autonomous_mode_confirm(repo: &str, typed: &str, area: Rect, buffer: &mut Buffer) {
    Clear.render(area, buffer);
    let lines = vec![
        Line::from("Enable full autonomous mode"),
        Line::from("dangerous / use with caution").style(Style::new().add_modifier(Modifier::BOLD)),
        Line::from(format!("Type the repo name to confirm: {repo}")),
        Line::from(format!("> {typed}")),
        Line::from("Enter to confirm | Esc to cancel"),
    ];
    Paragraph::new(lines)
        .block(
            Block::new()
                .borders(Borders::ALL)
                .title("Autonomous Mode (dangerous)"),
        )
        .render(area, buffer);
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

/// A focusable pane's block title: the base title plus a `[focus]` tag when the
/// arrow keys are currently driving that pane, so the operator can see which
/// pane `up`/`down` control.
fn focus_title(base: &str, focused: bool) -> String {
    if focused {
        format!("{base} [focus]")
    } else {
        base.to_owned()
    }
}

/// Whether the content pane currently holds focus (so its title carries the
/// `[focus]` tag while the Views nav's does not, and vice versa).
fn content_focused(model: &TuiScreenModel) -> bool {
    model.focus() == FocusPane::Content
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
    let title = focus_title("Views", model.focus() == FocusPane::Nav);
    List::new(items)
        .block(Block::new().borders(Borders::ALL).title(title))
        .render(area, buffer);
}

fn render_attention(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    let items = model
        .attention_items()
        .iter()
        .enumerate()
        .map(|(index, item)| attention_item_line(model, index, item))
        .collect::<Vec<_>>();
    let title = focus_title("Attention", content_focused(model));
    List::new(items)
        .block(Block::new().borders(Borders::ALL).title(title))
        .render(area, buffer);
}

fn render_summary(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    let items = model
        .view_items()
        .iter()
        .map(|item| ListItem::new(format!("  {}", item.title())));
    let title = focus_title(model.active_view().label(), content_focused(model));
    List::new(items)
        .block(Block::new().borders(Borders::ALL).title(title))
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
    #[cfg(test)]
    use console_application::source_adapters::LaneReason;
    use console_application::source_adapters::{AcceptancePolicy, AdmissionPolicy, Lane};
    use console_application::{
        ApplicationError, AttentionDetail, AttentionItem, FocusPane, LaneFocus, OperatorAction,
        OperatorActionOutcome, PendingValve, RejectMode, TuiInteraction, TuiInteractionState,
        TuiOverlay, TuiScreenModel, TuiView, build_tui_model, build_tui_model_for_state,
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
    fn keymap_maps_views_nav_focus_navigation_and_dive_in() {
        // Default focus is the Views nav: up/down walk the vertical Views menu,
        // Enter and Right dive focus into the Content pane, Left is the global
        // previous-view switcher, and Esc is the inert close-overlay no-op.
        let model = attention_model(TuiOverlay::None);
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Down), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::SelectNextView
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Up), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::SelectPreviousView
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &model),
            Some(TuiTerminalInput::Interaction(TuiInteraction::FocusContent))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Right), &model),
            Some(TuiTerminalInput::Interaction(TuiInteraction::FocusContent))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Left), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::SelectPreviousView
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Esc), &model),
            Some(TuiTerminalInput::Interaction(TuiInteraction::CloseOverlay))
        );
    }

    #[test]
    fn keymap_maps_content_focus_navigation_and_modal_opening() {
        // In the Content pane: up/down move the content selection, Enter opens
        // the command modal on the selected attention item, Right is the global
        // next-view switcher, and Left/Esc step focus back to the Views nav.
        let model = attention_model_in(TuiOverlay::None, FocusPane::Content);
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
            Some(TuiTerminalInput::Interaction(TuiInteraction::FocusNav))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Esc), &model),
            Some(TuiTerminalInput::Interaction(TuiInteraction::FocusNav))
        );
    }

    #[test]
    fn keymap_toggles_the_help_overlay_with_question_mark() {
        // `?` with no overlay open opens Help; `?` again (Help open) closes it;
        // `?` typed into a text overlay is a literal char; `?` behind the
        // command modal is inert.
        let none = attention_model(TuiOverlay::None);
        let help = attention_model(TuiOverlay::Help);
        let search = attention_model(TuiOverlay::Search {
            query: String::new(),
        });
        let modal = attention_model(TuiOverlay::CommandModal {
            selected_action_index: 0,
        });

        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('?')), &none),
            Some(TuiTerminalInput::Interaction(TuiInteraction::OpenHelp))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('?')), &help),
            Some(TuiTerminalInput::Interaction(TuiInteraction::CloseOverlay))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('?')), &search),
            Some(TuiTerminalInput::Interaction(TuiInteraction::TypeChar('?')))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('?')), &modal),
            None
        );

        // Behind the Help overlay, up/down are the harmless content moves, Enter
        // is inert, and Esc closes the overlay.
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Up), &help),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::SelectPrevious
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Down), &help),
            Some(TuiTerminalInput::Interaction(TuiInteraction::SelectNext))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &help),
            None
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Esc), &help),
            Some(TuiTerminalInput::Interaction(TuiInteraction::CloseOverlay))
        );
    }

    #[test]
    fn keymap_left_right_and_enter_are_inert_behind_a_text_overlay() {
        let search = attention_model(TuiOverlay::Search {
            query: "x".to_owned(),
        });
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Left), &search),
            None
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Right), &search),
            None
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &search),
            None
        );
    }

    #[test]
    fn render_to_text_draws_the_help_overlay() {
        let state = TuiInteractionState::new(0, TuiOverlay::Help);
        let model = build_tui_model_for_state(&demo_events(), &state);

        let output = render_to_text(&model, 96, 24);

        assert_eq!(output.as_ref().map(|r| r.contains("Help")), Ok(true));
        assert_eq!(
            output
                .as_ref()
                .map(|r| r.contains("Navigate the Views menu")),
            Ok(true)
        );
    }

    #[test]
    fn render_to_text_marks_the_focused_pane() {
        // Default focus is the Views nav: its title carries the focus tag.
        let nav = build_tui_model_for_state(
            &demo_events(),
            &TuiInteractionState::new(0, TuiOverlay::None),
        );
        let nav_output = render_to_text(&nav, 96, 24);
        assert_eq!(
            nav_output.as_ref().map(|r| r.contains("Views [focus]")),
            Ok(true)
        );

        // Content focus: the Attention content list carries the focus tag while
        // the Views title does not.
        let content = build_tui_model_for_state(
            &demo_events(),
            &TuiInteractionState::new(0, TuiOverlay::None).with_focus(FocusPane::Content),
        );
        let content_output = render_to_text(&content, 96, 24);
        assert_eq!(
            content_output
                .as_ref()
                .map(|r| r.contains("Attention [focus]")),
            Ok(true)
        );
        assert_eq!(
            content_output.as_ref().map(|r| r.contains("Views [focus]")),
            Ok(false)
        );
    }

    #[test]
    fn keymap_routes_enter_and_esc_through_the_lane_sub_view() {
        // From the Views nav, Enter dives focus into the Content pane; the
        // overview -> drill -> overview flow itself lives in Content focus.
        let nav_overview = lanes_model(LaneFocus::Overview, TuiOverlay::None);
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &nav_overview),
            Some(TuiTerminalInput::Interaction(TuiInteraction::FocusContent))
        );

        let overview = lanes_model_content(LaneFocus::Overview, TuiOverlay::None);
        // Enter drills into the selected lane from the content overview.
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &overview),
            Some(TuiTerminalInput::Interaction(TuiInteraction::DrillIntoLane))
        );
        // Esc on the content overview (no overlay open) steps focus back to nav.
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Esc), &overview),
            Some(TuiTerminalInput::Interaction(TuiInteraction::FocusNav))
        );

        let drilled = lanes_model_content(LaneFocus::Lane(Lane::Ready), TuiOverlay::None);
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
        // Left mirrors Esc: it also returns the drilled-in lane to its overview.
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Left), &drilled),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::ReturnToLaneOverview
            ))
        );

        // With an overlay open, Esc closes it first even while drilled in.
        let drilled_with_overlay = lanes_model_content(
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
    fn render_to_text_distinguishes_cockpit_blind_from_factory_idle_in_the_header() {
        // Cockpit-blind: three backing sources degraded to a not-observed
        // finding this cycle. The header MUST show how many and which sources
        // are unavailable, so this is never mistaken for an idle factory.
        let blind_events = [
            ConsoleEvent::fixture(
                "evt_orchestrator_not_observed",
                EventType::SourceNotObservedFindingObserved,
                "orchestrator",
            ),
            ConsoleEvent::fixture(
                "evt_github_not_observed",
                EventType::SourceNotObservedFindingObserved,
                "github",
            ),
            ConsoleEvent::fixture(
                "evt_fabro_not_observed",
                EventType::SourceNotObservedFindingObserved,
                "fabro",
            ),
        ];
        // The header counts the unavailable sources and names which ones, so a
        // cockpit-blind screen surfaces its blindness instead of a bare count.
        let blind = build_tui_model(&blind_events, 0);
        let blind_output = render_to_text(&blind, 96, 24);
        assert_eq!(
            blind_output
                .as_ref()
                .map(|rendered| rendered.contains("sources: 3 unavailable")),
            Ok(true)
        );
        assert_eq!(
            blind_output
                .as_ref()
                .map(|rendered| rendered.contains("fabro, github, orchestrator")),
            Ok(true)
        );

        // Factory-idle: every source was observed, there is simply nothing
        // actionable. The header carries no phantom unavailability count, so a
        // true-empty screen is never dressed as a false alarm.
        let idle = build_tui_model(&demo_events(), 0);
        let idle_output = render_to_text(&idle, 96, 24);
        assert_eq!(
            idle_output
                .as_ref()
                .map(|rendered| rendered.contains("unavailable")),
            Ok(false)
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
            TuiRuntimeEffect::PersistCommand(command)
            | TuiRuntimeEffect::PersistCommandWithPayload { command, .. } => Some(command),
            TuiRuntimeEffect::Render
            | TuiRuntimeEffect::OpenAttachCommand(_)
            | TuiRuntimeEffect::CopyAttachCommand(_)
            | TuiRuntimeEffect::Quit
            | TuiRuntimeEffect::ApplicationError(_) => None,
        }
    }

    /// The `{ ... }` payload JSON carried by a payload-bearing persist effect.
    fn persisted_payload(effect: &TuiRuntimeEffect) -> Option<&str> {
        match effect {
            TuiRuntimeEffect::PersistCommandWithPayload { payload_json, .. } => Some(payload_json),
            TuiRuntimeEffect::PersistCommand(_)
            | TuiRuntimeEffect::Render
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
    /// that exercise overlay-driven behavior in the default (Views nav) focus.
    fn attention_model(overlay: TuiOverlay) -> TuiScreenModel {
        build_tui_model_for_state(&demo_events(), &TuiInteractionState::new(0, overlay))
    }

    /// An Attention-view model carrying the given overlay + focus pane, for
    /// keymap tests that exercise the Content-pane focus.
    fn attention_model_in(overlay: TuiOverlay, focus: FocusPane) -> TuiScreenModel {
        build_tui_model_for_state(
            &demo_events(),
            &TuiInteractionState::new(0, overlay).with_focus(focus),
        )
    }

    /// A Lanes-view model in the given lane focus + overlay, in the default
    /// (Views nav) focus, over a small board.
    fn lanes_model(lane_focus: LaneFocus, overlay: TuiOverlay) -> TuiScreenModel {
        let state =
            TuiInteractionState::for_view(TuiView::Lanes, 0, overlay).with_lane_focus(lane_focus);
        build_tui_model_for_state(&lane_render_events(), &state)
    }

    /// A Lanes-view model in the given lane focus + overlay with the Content
    /// pane focused, where the overview/drill flow lives.
    fn lanes_model_content(lane_focus: LaneFocus, overlay: TuiOverlay) -> TuiScreenModel {
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, overlay)
            .with_lane_focus(lane_focus)
            .with_focus(FocusPane::Content);
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

    // -----------------------------------------------------------------------
    // Autonomous-mode toggle / type-to-confirm modal (C3 slice 2), covering the
    // Scenario 9 enable path at the TUI runtime level.
    // -----------------------------------------------------------------------

    const CONFIRM_REPO: &str = "livespec-console-beads-fabro";

    /// An interaction state over the given overlay whose selected repo carries
    /// the given derived autonomous mode.
    fn autonomous_state(overlay: TuiOverlay, autonomous_mode_enabled: bool) -> TuiInteractionState {
        TuiInteractionState::new(0, overlay)
            .with_selected_repo(CONFIRM_REPO.to_owned())
            .with_autonomous_mode_enabled(autonomous_mode_enabled)
    }

    /// A model over the given overlay whose selected repo carries the given mode.
    fn autonomous_model(overlay: TuiOverlay, autonomous_mode_enabled: bool) -> TuiScreenModel {
        build_tui_model_for_state(&[], &autonomous_state(overlay, autonomous_mode_enabled))
    }

    #[test]
    fn keymap_toggles_autonomous_mode_and_types_into_the_confirm_modal() {
        // `a` with no overlay open toggles autonomous mode.
        let none = autonomous_model(TuiOverlay::None, false);
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('a')), &none),
            Some(TuiTerminalInput::ToggleAutonomousMode)
        );

        // With the confirm modal open, `a` and any char are literal input, and
        // Enter confirms.
        let confirm = autonomous_model(
            TuiOverlay::AutonomousModeConfirm {
                typed: String::new(),
            },
            false,
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('a')), &confirm),
            Some(TuiTerminalInput::Interaction(TuiInteraction::TypeChar('a')))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('x')), &confirm),
            Some(TuiTerminalInput::Interaction(TuiInteraction::TypeChar('x')))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &confirm),
            Some(TuiTerminalInput::Confirm)
        );
    }

    #[test]
    fn toggling_a_disabled_repo_opens_the_confirm_modal_without_submitting() {
        let state = autonomous_state(TuiOverlay::None, false);
        let step = step_tui_runtime(
            &state,
            &[],
            TuiTerminalInput::ToggleAutonomousMode,
            "operator",
        );
        assert_eq!(step.effect(), &TuiRuntimeEffect::Render);
        assert_eq!(
            step.state().overlay(),
            &TuiOverlay::AutonomousModeConfirm {
                typed: String::new(),
            }
        );
        assert_eq!(persisted_command(step.effect()), None);
    }

    #[test]
    fn confirming_the_typed_modal_submits_a_confirmed_enable_command() {
        let state = autonomous_state(
            TuiOverlay::AutonomousModeConfirm {
                typed: CONFIRM_REPO.to_owned(),
            },
            false,
        );
        let step = step_tui_runtime(&state, &[], TuiTerminalInput::Confirm, "operator");

        let command = persisted_command(step.effect());
        assert_eq!(
            command.map(console_domain::CommandEnvelope::command_type),
            Some(&CommandType::ConfigAutonomousModeSet)
        );
        let payload = persisted_payload(step.effect());
        assert_eq!(
            payload.map(|value| value.contains(r#""enabled":true"#)),
            Some(true)
        );
        assert_eq!(
            payload.map(|value| value.contains(r#""confirmed":true"#)),
            Some(true)
        );
        assert_eq!(
            payload.map(|value| value.contains(r#""repo":"livespec-console-beads-fabro""#)),
            Some(true)
        );
        // The modal closes after the submit.
        assert_eq!(step.state().overlay(), &TuiOverlay::None);
    }

    #[test]
    fn confirming_a_mismatched_modal_does_not_submit() {
        let state = autonomous_state(
            TuiOverlay::AutonomousModeConfirm {
                typed: "wrong".to_owned(),
            },
            false,
        );
        let step = step_tui_runtime(&state, &[], TuiTerminalInput::Confirm, "operator");
        assert_eq!(
            step.effect(),
            &TuiRuntimeEffect::ApplicationError(
                ApplicationError::AutonomousModeConfirmationMismatch
            )
        );
        assert_eq!(persisted_command(step.effect()), None);
        assert_eq!(persisted_payload(step.effect()), None);
    }

    #[test]
    fn toggling_an_enabled_repo_submits_a_disable_without_confirmation() {
        let state = autonomous_state(TuiOverlay::None, true);
        let step = step_tui_runtime(
            &state,
            &[],
            TuiTerminalInput::ToggleAutonomousMode,
            "operator",
        );

        let payload = persisted_payload(step.effect());
        assert_eq!(
            payload.map(|value| value.contains(r#""enabled":false"#)),
            Some(true)
        );
        assert_eq!(
            payload.map(|value| value.contains(r#""confirmed":false"#)),
            Some(true)
        );
        // No confirmation modal is opened for a disable.
        assert_eq!(step.state().overlay(), &TuiOverlay::None);
    }

    #[test]
    fn toggling_an_enabled_repo_without_a_selected_repo_surfaces_an_error() {
        let state =
            TuiInteractionState::new(0, TuiOverlay::None).with_autonomous_mode_enabled(true);
        let step = step_tui_runtime(
            &state,
            &[],
            TuiTerminalInput::ToggleAutonomousMode,
            "operator",
        );
        assert_eq!(
            step.effect(),
            &TuiRuntimeEffect::ApplicationError(ApplicationError::InvalidAutonomousModePayload)
        );
    }

    #[test]
    fn renders_the_dangerous_label_in_the_confirm_modal() {
        let model = autonomous_model(
            TuiOverlay::AutonomousModeConfirm {
                typed: String::new(),
            },
            false,
        );
        let rendered = render_to_text(&model, 96, 24);
        assert_eq!(
            rendered
                .as_ref()
                .map(|value| value.contains("dangerous / use with caution")),
            Ok(true)
        );
        assert_eq!(
            rendered
                .as_ref()
                .map(|value| value.contains("Type the repo name to confirm")),
            Ok(true)
        );
        assert_eq!(
            rendered.as_ref().map(|value| value.contains(CONFIRM_REPO)),
            Ok(true)
        );
    }

    #[test]
    fn action_outcome_effect_maps_the_payload_bearing_persist_outcome() {
        let effect = action_outcome_effect(OperatorActionOutcome::PersistCommandWithPayload {
            command: console_domain::CommandEnvelope::new(
                "cmd".to_owned(),
                CommandType::ConfigAutonomousModeSet,
                CONFIRM_REPO.to_owned(),
                "key".to_owned(),
                "operator".to_owned(),
            ),
            payload_json: r#"{"repo":"r","enabled":true,"confirmed":true}"#.to_owned(),
        });
        assert_eq!(
            persisted_payload(&effect),
            Some(r#"{"repo":"r","enabled":true,"confirmed":true}"#)
        );
    }

    // -----------------------------------------------------------------------
    // Operator valve keys (S4b): p/c/r/m/n bind the five human-valve/policy
    // commands to the valve-confirm modal against the selected work-item; each
    // rides the shared orchestrator action port. Reject is confirmed as
    // dangerous. Scenario 11 at the TUI runtime level.
    // -----------------------------------------------------------------------

    #[test]
    fn keymap_binds_the_five_valve_keys_on_a_selected_attention_item() {
        // Attention view with a selected item: each valve key opens the
        // valve-confirm modal staging that valve at its default option.
        let model = attention_model(TuiOverlay::None);
        for (code, valve) in [
            ('p', PendingValve::Approve),
            ('c', PendingValve::Accept),
            ('r', PendingValve::Reject(RejectMode::Rework)),
            ('m', PendingValve::SetAdmission(AdmissionPolicy::Manual)),
            (
                'n',
                PendingValve::SetAcceptance(AcceptancePolicy::AiThenHuman),
            ),
        ] {
            assert_eq!(
                key_event_to_terminal_input(key(KeyCode::Char(code)), &model),
                Some(TuiTerminalInput::Interaction(
                    TuiInteraction::OpenValveConfirm(valve)
                ))
            );
        }
    }

    #[test]
    fn keymap_valve_keys_are_inert_off_the_attention_view_and_literal_in_a_text_overlay() {
        // A non-Attention view has no selected work-item target, so a valve key
        // is inert.
        let events = demo_events();
        let non_attention = build_tui_model_for_state(
            &events,
            &TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None),
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('p')), &non_attention),
            None
        );

        // Behind a text overlay a valve key is a literal character.
        let search = attention_model(TuiOverlay::Search {
            query: String::new(),
        });
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('r')), &search),
            Some(TuiTerminalInput::Interaction(TuiInteraction::TypeChar('r')))
        );
    }

    #[test]
    fn keymap_valve_confirm_modal_cycles_the_option_and_confirms() {
        let model = attention_model(TuiOverlay::ValveConfirm {
            valve: PendingValve::Reject(RejectMode::Rework),
        });
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Down), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::CycleValveOption(true)
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Up), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::CycleValveOption(false)
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &model),
            Some(TuiTerminalInput::Confirm)
        );
    }

    #[test]
    fn confirming_a_valve_modal_persists_its_work_item_command_and_closes() {
        // Payloadless approve persists a plain work_item.approve_requested command
        // for the selected work-item and closes the modal.
        let approve_state = TuiInteractionState::new(
            0,
            TuiOverlay::ValveConfirm {
                valve: PendingValve::Approve,
            },
        );
        let approve = step_tui_runtime(
            &approve_state,
            &demo_events(),
            TuiTerminalInput::Confirm,
            "operator",
        );
        assert_eq!(
            persisted_command(approve.effect()).map(console_domain::CommandEnvelope::command_type),
            Some(&CommandType::WorkItemApproveRequested)
        );
        assert_eq!(persisted_payload(approve.effect()), None);
        assert_eq!(approve.state().overlay(), &TuiOverlay::None);

        // A payload valve persists the mode/policy payload.
        let reject_state = TuiInteractionState::new(
            0,
            TuiOverlay::ValveConfirm {
                valve: PendingValve::Reject(RejectMode::Regroom),
            },
        );
        let reject = step_tui_runtime(
            &reject_state,
            &demo_events(),
            TuiTerminalInput::Confirm,
            "operator",
        );
        assert_eq!(
            persisted_command(reject.effect()).map(console_domain::CommandEnvelope::command_type),
            Some(&CommandType::WorkItemRejectRequested)
        );
        assert_eq!(
            persisted_payload(reject.effect()),
            Some(r#"{"mode":"regroom"}"#)
        );
        assert_eq!(reject.state().overlay(), &TuiOverlay::None);
    }

    #[test]
    fn render_to_text_draws_the_valve_confirm_modal_with_option_and_danger() {
        // A destructive reject shows the target, the cycled mode, and the danger
        // caution.
        let reject = build_tui_model_for_state(
            &demo_events(),
            &TuiInteractionState::new(
                0,
                TuiOverlay::ValveConfirm {
                    valve: PendingValve::Reject(RejectMode::Regroom),
                },
            ),
        );
        let output = render_to_text(&reject, 96, 24);
        assert_eq!(output.as_ref().map(|r| r.contains("Valve")), Ok(true));
        assert_eq!(
            output.as_ref().map(|r| r.contains("Reject work-item")),
            Ok(true)
        );
        assert_eq!(
            output.as_ref().map(|r| r.contains("Policy/mode: regroom")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|r| r.contains("dangerous / use with caution")),
            Ok(true)
        );

        // A payload-free, non-destructive approve shows neither a policy line nor
        // the danger caution.
        let approve = build_tui_model_for_state(
            &demo_events(),
            &TuiInteractionState::new(
                0,
                TuiOverlay::ValveConfirm {
                    valve: PendingValve::Approve,
                },
            ),
        );
        let output = render_to_text(&approve, 96, 24);
        assert_eq!(
            output.as_ref().map(|r| r.contains("Approve work-item")),
            Ok(true)
        );
        assert_eq!(
            output.as_ref().map(|r| r.contains("Policy/mode:")),
            Ok(false)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|r| r.contains("dangerous / use with caution")),
            Ok(false)
        );
    }

    #[test]
    fn help_overlay_lists_the_valve_keys() {
        let state = TuiInteractionState::new(0, TuiOverlay::Help);
        let model = build_tui_model_for_state(&demo_events(), &state);
        // A tall area so the full help body (including the valve keys near the
        // bottom) renders inside the centered modal rather than being clipped.
        let output = render_to_text(&model, 120, 72);
        assert_eq!(
            output
                .as_ref()
                .map(|r| r.contains("approve / accept / reject")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|r| r.contains("set-admission / set-acceptance")),
            Ok(true)
        );
    }
}
