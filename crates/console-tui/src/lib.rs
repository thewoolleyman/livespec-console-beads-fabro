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
    ApplicationError, AttentionDetail, AttentionItem, DispatcherSettingsRead, FocusPane,
    LaneColumn, LaneFocus, LaneWorkItem, OperatorAction, OperatorActionOutcome, PendingValve,
    RejectMode, SettingRow, TimelineEntry, TuiInteraction, TuiInteractionState, TuiOverlay,
    TuiScreenModel, TuiView, ViewSummaryItem, build_tui_model_for_state, dispatcher_setting_rows,
    reduce_tui_interaction, resolve_command_palette_action, resolve_dispatcher_setting_edit,
    resolve_selected_operator_action, resolve_valve_action,
};
use console_domain::{CommandEnvelope, ConsoleEvent};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
    ScrollbarState, StatefulWidget, Widget, Wrap,
};

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
    dispatcher_settings: DispatcherSettingsRead,
) -> io::Result<Vec<TuiRuntimeEffect>> {
    let mut effect_sink = DeferredTuiRuntimeEffectSink;
    run_interactive_tui_with_effect_sink(
        events,
        requested_by,
        selected_repo,
        dispatcher_settings,
        &mut effect_sink,
    )
}

#[cfg(all(not(test), not(coverage)))]
/// Run interactive tui with a per-effect sink and return deferred effects.
pub fn run_interactive_tui_with_effect_sink(
    events: &[ConsoleEvent],
    requested_by: &str,
    selected_repo: &str,
    dispatcher_settings: DispatcherSettingsRead,
    effect_sink: &mut dyn TuiRuntimeEffectSink,
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
        dispatcher_settings,
        effect_sink,
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
    dispatcher_settings: DispatcherSettingsRead,
    effect_sink: &mut dyn TuiRuntimeEffectSink,
) -> io::Result<Vec<TuiRuntimeEffect>> {
    let mut state = TuiInteractionState::new(0, TuiOverlay::None)
        .with_selected_repo(selected_repo.to_owned())
        .with_dispatcher_settings(dispatcher_settings);
    let mut effects = Vec::new();
    loop {
        let model = build_tui_model_for_state(events, &state);
        // Measure the Detail pane's wrapped max scroll while drawing and feed it
        // back into the state, so the next ScrollDetailDown clamps to the true
        // wrapped bottom (the SAME count the scrollbar is sized from) rather than
        // a width-agnostic logical line count (Finding G).
        let mut detail_max_scroll = 0usize;
        terminal.draw(|frame| {
            detail_max_scroll = render_model(&model, frame.area(), frame.buffer_mut());
        })?;
        state = state.with_detail_max_scroll(detail_max_scroll);
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
        if effect_sink.handle_runtime_effect(&effect)? == TuiRuntimeEffectSinkOutcome::Deferred {
            effects.push(effect);
        }
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
    /// Persist a command carrying an operator-supplied JSON payload (for example
    /// the `config.dispatcher_setting_set` write's `{ repo, setting, value }`).
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Outcome from handling one TUI runtime effect.
pub enum TuiRuntimeEffectSinkOutcome {
    /// The sink applied the effect immediately; callers must not flush it again.
    Applied,
    /// The sink deferred the effect; callers should return it for later handling.
    Deferred,
}

/// Sink for applying TUI runtime effects as the interactive loop produces them.
pub trait TuiRuntimeEffectSink {
    /// Handle one runtime effect.
    ///
    /// # Errors
    /// Returns an IO error when the effect cannot be applied.
    fn handle_runtime_effect(
        &mut self,
        effect: &TuiRuntimeEffect,
    ) -> std::io::Result<TuiRuntimeEffectSinkOutcome>;
}

/// Effect sink that preserves the legacy end-of-session flush behavior.
pub struct DeferredTuiRuntimeEffectSink;

impl TuiRuntimeEffectSink for DeferredTuiRuntimeEffectSink {
    fn handle_runtime_effect(
        &mut self,
        _effect: &TuiRuntimeEffect,
    ) -> std::io::Result<TuiRuntimeEffectSinkOutcome> {
        Ok(TuiRuntimeEffectSinkOutcome::Deferred)
    }
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
        TuiOverlay::ValveConfirm { .. } => resolve_valve_action(&model, requested_by),
        // `Enter`/`Space` on a Settings row is an ordinary recorded setting write
        // (no overlay, no arming ceremony).
        TuiOverlay::None if model.active_view() == TuiView::Settings => {
            resolve_dispatcher_setting_edit(&model, requested_by)
        }
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
        KeyCode::Char(' ') => space_input(model, overlay),
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
        KeyCode::Char('s') => move_status_open_input(model),
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
/// act within the focused pane (move the Views selection on the nav, the content
/// selection in the content list, or scroll the Detail pane up); behind any
/// other overlay it is the harmless content move.
const fn up_interaction(model: &TuiScreenModel) -> TuiInteraction {
    match model.overlay() {
        TuiOverlay::CommandModal { .. } => TuiInteraction::SelectPreviousAction,
        TuiOverlay::ValveConfirm { .. } => TuiInteraction::CycleValveOption(false),
        TuiOverlay::None => match model.focus() {
            FocusPane::Nav => TuiInteraction::SelectPreviousView,
            FocusPane::Content => TuiInteraction::SelectPrevious,
            FocusPane::Detail => TuiInteraction::ScrollDetailUp,
        },
        TuiOverlay::Search { .. } | TuiOverlay::CommandPalette { .. } | TuiOverlay::Help => {
            TuiInteraction::SelectPrevious
        }
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
            FocusPane::Detail => TuiInteraction::ScrollDetailDown,
        },
        TuiOverlay::Search { .. } | TuiOverlay::CommandPalette { .. } | TuiOverlay::Help => {
            TuiInteraction::SelectNext
        }
    }
}

/// Enter: confirm a command modal / valve-confirm modal; behind a text/help
/// overlay it is inert; with no overlay open it dives into the focused pane (see
/// [`enter_content_input`]).
fn enter_input(model: &TuiScreenModel) -> Option<TuiTerminalInput> {
    match model.overlay() {
        TuiOverlay::CommandModal { .. }
        | TuiOverlay::CommandPalette { .. }
        | TuiOverlay::ValveConfirm { .. } => Some(TuiTerminalInput::Confirm),
        TuiOverlay::Search { .. } | TuiOverlay::Help => None,
        TuiOverlay::None => enter_content_input(model),
    }
}

/// Enter with no overlay open: from the Views nav it dives focus into the
/// Content pane; in the Content pane it drills into the selected lane (lane
/// overview), edits the selected Settings row, is inert in a drilled-in lane, or
/// opens the command modal on the selected attention item for any other view; on
/// the Detail pane it is inert (the command modal is opened from the Content
/// pane).
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
            // A Settings row edit is an ordinary recorded write resolved on
            // `Confirm`; every other view opens the command modal.
            if model.active_view() == TuiView::Settings {
                return Some(TuiTerminalInput::Confirm);
            }
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::OpenCommandModal,
            ))
        }
        FocusPane::Detail => None,
    }
}

/// Esc: close an open overlay first; with no overlay open, step focus back one
/// pane toward the nav — the Detail pane returns to Content, the Content pane
/// returns a drilled-in lane to its overview (else focus to the Views nav); on
/// the nav (leftmost) it is the inert close-overlay.
fn esc_interaction(model: &TuiScreenModel) -> TuiInteraction {
    if model.overlay().is_open() {
        return TuiInteraction::CloseOverlay;
    }
    match model.focus() {
        FocusPane::Detail => TuiInteraction::FocusContent,
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

/// Left: behind an overlay it is inert; otherwise it walks focus one pane toward
/// the nav, clamped at the leftmost — the Detail pane returns to Content, the
/// Content pane steps back (a drilled-in lane to its overview, else focus to the
/// Views nav), and on the Views nav (leftmost) left is inert.
fn left_input(model: &TuiScreenModel) -> Option<TuiTerminalInput> {
    if model.overlay().is_open() {
        return None;
    }
    let interaction = match model.focus() {
        // Leftmost pane: left clamps here.
        FocusPane::Nav => return None,
        FocusPane::Content => content_back_interaction(model),
        FocusPane::Detail => TuiInteraction::FocusContent,
    };
    Some(TuiTerminalInput::Interaction(interaction))
}

/// Right: behind an overlay it is inert; otherwise it walks focus one pane toward
/// the Detail pane, clamped at the rightmost — the Views nav dives into Content,
/// Content dives into Detail (on a view that has a Detail pane), and on the
/// Detail pane (rightmost) right is inert. The Lanes view has no Detail pane, so
/// right clamps at Content there.
const fn right_input(model: &TuiScreenModel) -> Option<TuiTerminalInput> {
    if model.overlay().is_open() {
        return None;
    }
    let interaction = match model.focus() {
        FocusPane::Nav => TuiInteraction::FocusContent,
        FocusPane::Content => {
            if view_has_detail_pane(model) {
                TuiInteraction::FocusDetail
            } else {
                return None;
            }
        }
        // Rightmost reachable pane: right clamps here.
        FocusPane::Detail => return None,
    };
    Some(TuiTerminalInput::Interaction(interaction))
}

/// Whether the active view renders a right-hand Detail pane (every view except
/// `Lanes`, which spans the full body width beside the nav). Used to clamp the
/// rightmost focus step at Content on the Lanes view.
const fn view_has_detail_pane(model: &TuiScreenModel) -> bool {
    !matches!(model.active_view(), TuiView::Lanes)
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
        | TuiOverlay::ValveConfirm { .. } => text_input('?', overlay),
    }
}

const fn q_input(overlay: &TuiOverlay) -> Option<TuiTerminalInput> {
    if matches!(overlay, TuiOverlay::None) {
        return Some(TuiTerminalInput::Quit);
    }
    text_input('q', overlay)
}

/// Space: with no overlay open, edit the selected Settings row (the `Enter`
/// alias on the Settings surface); otherwise it is a literal space typed into
/// the open text overlay.
fn space_input(model: &TuiScreenModel, overlay: &TuiOverlay) -> Option<TuiTerminalInput> {
    if matches!(overlay, TuiOverlay::None) {
        if model.active_view() == TuiView::Settings && model.focus() == FocusPane::Content {
            return Some(TuiTerminalInput::Confirm);
        }
        return None;
    }
    text_input(' ', overlay)
}

/// A valve key (`p`/`c`/`r`/`m`/`n`): with no overlay open and a selected
/// work-item -- either the selected Attention item OR the individually-selected
/// item in a drilled-in lane -- open the valve-confirm modal staging the given
/// valve; on any view without a selected work-item it is inert; behind an open
/// text overlay it is a literal character.
fn valve_open_input(
    model: &TuiScreenModel,
    valve: PendingValve,
    character: char,
) -> Option<TuiTerminalInput> {
    let overlay = model.overlay();
    if matches!(overlay, TuiOverlay::None) {
        if model.selected_work_item_id().is_some() {
            return Some(TuiTerminalInput::Interaction(
                TuiInteraction::OpenValveConfirm(valve),
            ));
        }
        return None;
    }
    text_input(character, overlay)
}

/// The move-status key (`s`): with no overlay open and an individually-selected
/// work-item in a drilled-in lane whose current lane has an operator-drivable
/// target, open the valve-confirm modal staging the move-status valve at its
/// first target; on a lane item with no drivable target, or off the drill-in, it
/// is inert; behind an open text overlay it is a literal character.
fn move_status_open_input(model: &TuiScreenModel) -> Option<TuiTerminalInput> {
    let overlay = model.overlay();
    if matches!(overlay, TuiOverlay::None) {
        return model
            .selected_move_status_valve()
            .map(|valve| TuiTerminalInput::Interaction(TuiInteraction::OpenValveConfirm(valve)));
    }
    text_input('s', overlay)
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

/// Render the whole screen.
///
/// Returns the Detail pane's maximum scroll offset — the wrapped-aware largest
/// offset that keeps the pane's last row visible, or `0` for a view without a
/// Detail pane — so the interactive loop can clamp the persisted scroll state to
/// what actually fits, using the SAME wrapped line count the scrollbar is sized
/// from.
pub fn render_model(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) -> usize {
    if area.is_empty() {
        return 0;
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
    let detail_max_scroll = render_body(model, vertical[1], buffer);
    render_footer(model, vertical[2], buffer);
    render_overlay(model, area, buffer);
    detail_max_scroll
}

fn render_header(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    // Fit the header to the space inside the block's left/right borders so a
    // narrow terminal degrades the header gracefully (see `header_line`) rather
    // than letting a long field clip the ones after it.
    let inner_width = usize::from(area.width.saturating_sub(2));
    Paragraph::new(model.header_line(inner_width))
        .block(Block::new().borders(Borders::ALL).title("LiveSpec Console"))
        .render(area, buffer);
}

/// Render the body panes and return the Detail pane's maximum scroll offset
/// (`0` for the Lanes view, which has no Detail pane).
fn render_body(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) -> usize {
    // The Lanes view spans the full body width beside the nav; the attention
    // and summary views keep the list/detail split.
    if model.active_view() == TuiView::Lanes {
        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(18), Constraint::Min(3)])
            .split(area);
        render_navigation(model, horizontal[0], buffer);
        render_lanes(model, horizontal[1], buffer);
        return 0;
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
    let detail_focused = model.focus() == FocusPane::Detail;
    if model.active_view() == TuiView::Attention {
        render_attention(model, horizontal[1], buffer);
        return render_detail(
            model.detail(),
            model.detail_scroll(),
            detail_focused,
            horizontal[2],
            buffer,
        );
    }
    if model.active_view() == TuiView::Settings {
        render_settings(model, horizontal[1], buffer);
        return render_settings_detail(
            model,
            model.detail_scroll(),
            detail_focused,
            horizontal[2],
            buffer,
        );
    }
    render_summary(model, horizontal[1], buffer);
    render_summary_detail(
        model.view_items(),
        model.detail_scroll(),
        detail_focused,
        horizontal[2],
        buffer,
    )
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
    // Build one list row per rendered line (a lane header, then its preview
    // rows). Tracking the selected lane's header row lets a stateful list scroll
    // it into view, so selecting a lane below the fold never leaves it invisible.
    let mut items: Vec<ListItem<'static>> = Vec::new();
    let mut selected_line: Option<usize> = None;
    for (index, column) in model.lane_board().columns().iter().enumerate() {
        let is_selected = Some(index) == selected;
        let marker = if is_selected { ">" } else { " " };
        let header = Line::from(format!(
            "{marker} {} ({})",
            column.lane().label(),
            column.count()
        ));
        if is_selected {
            selected_line = Some(items.len());
            items.push(ListItem::new(
                header.style(Style::new().add_modifier(Modifier::BOLD)),
            ));
        } else {
            items.push(ListItem::new(header));
        }
        for item in column.items().iter().take(LANE_OVERVIEW_PREVIEW) {
            items.push(ListItem::new(Line::from(lane_item_summary(item))));
        }
    }
    let count = items.len();
    let title = focus_title("Lanes", content_focused(model));
    let list = List::new(items).block(Block::new().borders(Borders::ALL).title(title));
    let mut list_state = ListState::default();
    list_state.select(selected_line);
    StatefulWidget::render(list, area, buffer, &mut list_state);
    render_vertical_scrollbar(area, buffer, count, list_state.offset());
}

/// A single drilled-in lane: its full rank-ordered item list, full width, with
/// the individually-selected work-item highlighted so the operator can pick one
/// item and act on it. Renders a stateful list (so the selected row scrolls into
/// view) plus a scrollbar; an empty lane shows a placeholder.
fn render_lane_drilldown(model: &TuiScreenModel, lane: Lane, area: Rect, buffer: &mut Buffer) {
    let items: &[LaneWorkItem] = model
        .lane_board()
        .column(lane)
        .map(LaneColumn::items)
        .unwrap_or_default();
    let title = focus_title(&format!("Lane: {}", lane.label()), content_focused(model));
    let block = Block::new().borders(Borders::ALL).title(title);
    if items.is_empty() {
        Paragraph::new(vec![Line::from("No work-items in this lane")])
            .block(block)
            .render(area, buffer);
        return;
    }
    let selected = model.selected_lane_item_index();
    let list_items = items
        .iter()
        .enumerate()
        .map(|(index, item)| lane_item_line(item, Some(index) == selected))
        .collect::<Vec<_>>();
    let count = list_items.len();
    let list = List::new(list_items).block(block);
    let mut list_state = ListState::default();
    list_state.select(selected);
    StatefulWidget::render(list, area, buffer, &mut list_state);
    render_vertical_scrollbar(area, buffer, count, list_state.offset());
}

/// One drilled-in lane row prepared for the selectable list: the full drill-in
/// line, with a `>` marker and bold style on the selected row.
fn lane_item_line(item: &LaneWorkItem, selected: bool) -> ListItem<'static> {
    let marker = if selected { ">" } else { " " };
    let label = format!("{marker} {}", lane_item_detail_text(item));
    ListItem::new(label).style(if selected {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    })
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
fn lane_item_detail_text(item: &LaneWorkItem) -> String {
    format!(
        "{}  {}  rank {}  [{}]{}",
        item.work_item_id(),
        item.repo(),
        item.rank(),
        item.status(),
        lane_reason_suffix(item)
    )
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
        TuiOverlay::ValveConfirm { valve } => {
            // The modal's consent target MUST read from the SAME source `Enter`
            // dispatches on (`selected_work_item_id` — the Attention detail OR the
            // drilled-in lane selection), never from `detail()` alone: in a
            // drilled-in lane the dispatch acts on the lane item, so reading the
            // Attention detail here would show a DIFFERENT (or blank) target and
            // let the operator confirm against the wrong work-item.
            render_valve_confirm(
                *valve,
                model.selected_work_item_id().unwrap_or(""),
                overlay_rect(area),
                buffer,
            );
        }
        TuiOverlay::Help => render_help_overlay(overlay_rect(area), buffer, model.active_view()),
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

/// Render the read-only Help overlay, SCOPED to the active view: the shared
/// navigation keys plus a view-specific section -- the Settings view's help
/// describes the six dispatcher settings and their edit, while the item views
/// (Attention / Lanes) describe item selection, the move-to-status action, and
/// the per-item valves. The text MUST stay in lock-step with the key handler and
/// the footer hint.
fn render_help_overlay(area: Rect, buffer: &mut Buffer, view: TuiView) {
    Clear.render(area, buffer);
    let mut lines = vec![
        Line::from("Navigate the Views menu with up/down; Enter drills in, Esc goes back."),
        Line::from(""),
        Line::from("left / right  move focus across panes (Views -> Content -> Detail), clamped"),
        Line::from("up / down    move the focused pane's selection, or scroll the Detail pane"),
        Line::from("enter        dive from the nav into content, or open the selected item"),
        Line::from(
            "esc          step focus back (Detail -> Content -> nav; drilled lane -> overview)",
        ),
        Line::from("/            open search"),
        Line::from(":            open the command palette (drain)"),
        Line::from("q / ctrl-c   quit"),
        Line::from("?            toggle this help"),
        Line::from(""),
    ];
    lines.extend(help_lines_for_view(view));
    lines.push(Line::from(""));
    lines.push(Line::from("Esc or ? closes this help."));
    Paragraph::new(lines)
        .block(Block::new().borders(Borders::ALL).title("Help"))
        .wrap(Wrap { trim: true })
        .render(area, buffer);
}

/// The view-scoped help section: the Settings view lists the six dispatcher
/// settings and their edit; every other item view lists item selection, the
/// move-to-status action, and the per-item valves.
fn help_lines_for_view(view: TuiView) -> Vec<Line<'static>> {
    match view {
        TuiView::Settings => vec![
            Line::from("Settings view -- the six dispatcher policy settings:"),
            Line::from(
                "  auto_approve_ready, merge_on_review_cap, acceptance_mode, review_fix_cap,",
            ),
            Line::from("  acceptance_rework_cap, wip_cap."),
            Line::from("enter/space  edit the selected setting row (ordinary recorded write)"),
        ],
        TuiView::Attention | TuiView::Spec | TuiView::Lanes | TuiView::Events | TuiView::Repos => {
            vec![
                Line::from("Lanes view -- select and act on an individual work-item:"),
                Line::from("up / down    (in a drilled-in lane) select an individual work-item"),
                Line::from(
                    "s            move the selected work-item to a status it may be driven to",
                ),
                Line::from("             (pending-approval -> ready, acceptance -> done,"),
                Line::from(
                    "              blocked -> ready/backlog; up/down change target, Enter confirms)",
                ),
                Line::from(
                    "p / c / r    approve / accept / reject the selected work-item (confirm modal)",
                ),
                Line::from(
                    "m / n        set-admission / set-acceptance override for the selected work-item",
                ),
            ]
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
    Widget::render(
        List::new(items).block(Block::new().borders(Borders::ALL).title("Command Modal")),
        area,
        buffer,
    );
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
    Widget::render(
        List::new(items).block(Block::new().borders(Borders::ALL).title(title)),
        area,
        buffer,
    );
}

fn render_attention(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    let items = model
        .attention_items()
        .iter()
        .enumerate()
        .map(|(index, item)| attention_item_line(model, index, item))
        .collect::<Vec<_>>();
    let count = items.len();
    let title = focus_title("Attention", content_focused(model));
    let list = List::new(items).block(Block::new().borders(Borders::ALL).title(title));
    // Render the list statefully so it scrolls to keep the selected row visible:
    // without a stateful list an off-screen selection is invisible on a small
    // terminal (the list would render from the top and never follow the cursor).
    let mut list_state = ListState::default();
    list_state.select(model.selected_attention_index());
    StatefulWidget::render(list, area, buffer, &mut list_state);
    render_vertical_scrollbar(area, buffer, count, list_state.offset());
}

/// Draw a vertical scrollbar on the right border of `area` when `content_len`
/// exceeds the rows visible inside the block, so the operator can tell there is
/// more content than fits and roughly where the viewport sits. `position` is the
/// index of the topmost visible row. A no-op when everything fits, so panes that
/// do not overflow render exactly as before (no stray scrollbar glyphs).
fn render_vertical_scrollbar(area: Rect, buffer: &mut Buffer, content_len: usize, position: usize) {
    let viewport = usize::from(area.height.saturating_sub(2));
    if viewport == 0 || content_len <= viewport {
        return;
    }
    let mut scrollbar_state = ScrollbarState::new(content_len).position(position);
    let track = area.inner(Margin {
        vertical: 1,
        horizontal: 0,
    });
    StatefulWidget::render(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None),
        track,
        buffer,
        &mut scrollbar_state,
    );
}

fn render_summary(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    let items = model
        .view_items()
        .iter()
        .map(|item| ListItem::new(format!("  {}", item.title())));
    let title = focus_title(model.active_view().label(), content_focused(model));
    Widget::render(
        List::new(items).block(Block::new().borders(Borders::ALL).title(title)),
        area,
        buffer,
    );
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

fn render_summary_detail(
    items: &[ViewSummaryItem],
    scroll: usize,
    focused: bool,
    area: Rect,
    buffer: &mut Buffer,
) -> usize {
    render_scrollable_detail(summary_detail_lines(items), scroll, focused, area, buffer)
}

/// The Detail-pane lines for a summary view: a `title:` line plus its detail line
/// per projection row, or a single placeholder when there are no rows. A
/// standalone builder so the scroll behavior can be exercised over its length.
fn summary_detail_lines(items: &[ViewSummaryItem]) -> Vec<Line<'static>> {
    if items.is_empty() {
        return vec![Line::from("No projection rows")];
    }
    items
        .iter()
        .flat_map(|item| {
            [
                Line::from(format!("{}:", item.title())),
                Line::from(item.detail().to_owned()),
            ]
        })
        .collect()
}

/// Render the `Settings` view content pane: one row per dispatcher setting,
/// `label [ value ]`, the selected row highlighted, or a not-observed placeholder
/// when the read surface produced no trustworthy values.
fn render_settings(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    let title = focus_title("Settings > Dispatcher settings", content_focused(model));
    let block = Block::new().borders(Borders::ALL).title(title);
    match model.dispatcher_settings() {
        DispatcherSettingsRead::Observed(settings) => {
            let rows = dispatcher_setting_rows(settings);
            let items = rows
                .iter()
                .enumerate()
                .map(|(index, row)| settings_row_line(model, index, row))
                .collect::<Vec<_>>();
            let count = items.len();
            let list = List::new(items).block(block);
            let mut list_state = ListState::default();
            list_state.select(model.selected_setting_index());
            StatefulWidget::render(list, area, buffer, &mut list_state);
            render_vertical_scrollbar(area, buffer, count, list_state.offset());
        }
        DispatcherSettingsRead::NotObserved => {
            Paragraph::new(vec![Line::from("Dispatcher settings not observed")])
                .block(block)
                .render(area, buffer);
        }
    }
}

/// One `Settings` content row: `> label  [ value ]`, with a compact `(dangerous)`
/// marker for a dangerous setting and the selected row bolded.
fn settings_row_line(model: &TuiScreenModel, index: usize, row: &SettingRow) -> ListItem<'static> {
    let selected = Some(index) == model.selected_setting_index();
    let marker = if selected { ">" } else { " " };
    let danger = if row.dangerous() { "  (dangerous)" } else { "" };
    let label = format!("{marker} {}  [ {} ]{danger}", row.label(), row.value());
    ListItem::new(label).style(if selected {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    })
}

/// Render the `Settings` view detail pane: the selected row's value plus its
/// inline help (which carries the "dangerous / use with caution" label for a
/// dangerous row), or a not-observed placeholder.
fn render_settings_detail(
    model: &TuiScreenModel,
    scroll: usize,
    focused: bool,
    area: Rect,
    buffer: &mut Buffer,
) -> usize {
    render_scrollable_detail(settings_detail_lines(model), scroll, focused, area, buffer)
}

/// The Detail-pane lines for the selected `Settings` row: the label and value, a
/// blank line, then the row's inline help. A standalone builder so the content
/// can be exercised directly.
fn settings_detail_lines(model: &TuiScreenModel) -> Vec<Line<'static>> {
    let DispatcherSettingsRead::Observed(settings) = model.dispatcher_settings() else {
        return vec![Line::from("Dispatcher settings not observed")];
    };
    let rows = dispatcher_setting_rows(settings);
    model
        .selected_setting_index()
        .and_then(|index| rows.get(index))
        .map_or_else(
            || vec![Line::from("No setting selected")],
            |row| {
                vec![
                    Line::from(format!("{}: {}", row.label(), row.value())),
                    Line::from(String::new()),
                    Line::from(row.help().to_owned()),
                ]
            },
        )
}

fn render_detail(
    detail: Option<&AttentionDetail>,
    scroll: usize,
    focused: bool,
    area: Rect,
    buffer: &mut Buffer,
) -> usize {
    let lines = detail.map_or_else(
        || vec![Line::from("No attention item selected")],
        detail_lines,
    );
    render_scrollable_detail(lines, scroll, focused, area, buffer)
}

/// Render the right Detail pane with vertical free-scroll: the given lines,
/// clamped so `scroll` never runs past the last row, a `[focus]` tag when the
/// pane holds focus, and a scrollbar affordance when the content overflows the
/// pane. `scroll` is the topmost visible row. Wrapping is enabled, so the row
/// count and clamp use the wrapped height at the pane's inner width, and the
/// bottom of an overflowing detail becomes reachable by scrolling down.
///
/// Returns the pane's maximum scroll offset — the largest topmost-row offset at
/// which the LAST wrapped row is still visible (`content_rows - viewport`) — so
/// the caller can clamp the persisted scroll state to the SAME wrapped line
/// count that sizes the scrollbar. This is what makes the scroll range and the
/// scrollbar agree even when fields and timeline entries wrap.
fn render_scrollable_detail(
    lines: Vec<Line<'static>>,
    scroll: usize,
    focused: bool,
    area: Rect,
    buffer: &mut Buffer,
) -> usize {
    let title = focus_title("Detail", focused);
    let inner_width = area.width.saturating_sub(2);
    let viewport = usize::from(area.height.saturating_sub(2));
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
    // Count the wrapped rows at the pane's inner width (no block on this
    // measurement, so the count is pure content rows) to clamp the offset and
    // size the scrollbar exactly, even when a long line wraps.
    let content_rows = paragraph.line_count(inner_width);
    let max_scroll = content_rows.saturating_sub(viewport);
    let offset = scroll.min(max_scroll);
    paragraph
        .block(Block::new().borders(Borders::ALL).title(title))
        .scroll((u16::try_from(offset).unwrap_or(u16::MAX), 0))
        .render(area, buffer);
    render_vertical_scrollbar(area, buffer, content_rows, offset);
    max_scroll
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
        AttentionDetail, AttentionItem, DispatcherSettings, DispatcherSettingsRead, FocusPane,
        LaneFocus, OperatorAction, OperatorActionOutcome, PendingValve, RejectMode, TimelineEntry,
        TuiInteraction, TuiInteractionState, TuiOverlay, TuiScreenModel, TuiView, build_tui_model,
        build_tui_model_for_state, reduce_tui_interaction,
    };
    use console_domain::{CommandType, ConsoleEvent, EventType};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::text::Line;

    use super::{
        DeferredTuiRuntimeEffectSink, TuiRenderError, TuiRuntimeEffect, TuiRuntimeEffectSink,
        TuiRuntimeEffectSinkOutcome, TuiTerminalInput, action_outcome_effect, attention_item_line,
        buffer_to_text, detail_lines, key_event_to_terminal_input, render_command_modal,
        render_detail, render_model, render_summary_detail, render_to_text, settings_detail_lines,
        step_tui_runtime,
    };

    #[test]
    fn deferred_runtime_effect_sink_defers_effects() {
        let mut sink = DeferredTuiRuntimeEffectSink;

        let outcome = sink.handle_runtime_effect(&TuiRuntimeEffect::Quit);

        assert!(matches!(outcome, Ok(TuiRuntimeEffectSinkOutcome::Deferred)));
    }

    #[test]
    fn keymap_maps_views_nav_focus_navigation_and_dive_in() {
        // Default focus is the Views nav: up/down walk the vertical Views menu,
        // Enter and Right dive focus into the Content pane, Left clamps at the
        // leftmost pane (inert), and Esc is the inert close-overlay no-op.
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
        // Left on the leftmost pane clamps: it produces no input.
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Left), &model),
            None
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Esc), &model),
            Some(TuiTerminalInput::Interaction(TuiInteraction::CloseOverlay))
        );
    }

    #[test]
    fn keymap_maps_content_focus_navigation_and_modal_opening() {
        // In the Content pane: up/down move the content selection, Enter opens
        // the command modal on the selected attention item, Right steps focus
        // into the Detail pane, and Left/Esc step focus back to the Views nav.
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
            Some(TuiTerminalInput::Interaction(TuiInteraction::FocusDetail))
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
    fn keymap_maps_detail_pane_scroll_step_back_and_inert_enter() {
        // On the rightmost Detail pane: up/down scroll the detail, Esc and Left
        // step focus back to Content, Right clamps (inert), and Enter is inert
        // (the command modal is opened from the Content pane).
        let model = attention_model_in(TuiOverlay::None, FocusPane::Detail);
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Down), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::ScrollDetailDown
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Up), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::ScrollDetailUp
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Esc), &model),
            Some(TuiTerminalInput::Interaction(TuiInteraction::FocusContent))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Left), &model),
            Some(TuiTerminalInput::Interaction(TuiInteraction::FocusContent))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Right), &model),
            None
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &model),
            None
        );
    }

    /// Drive one key through the full input -> reduce -> state loop, returning the
    /// resulting interaction state. A key that produces no input (a clamped or
    /// inert key) leaves the state unchanged.
    fn press(
        state: &TuiInteractionState,
        events: &[ConsoleEvent],
        code: KeyCode,
    ) -> TuiInteractionState {
        let model = build_tui_model_for_state(events, state);
        key_event_to_terminal_input(key(code), &model).map_or_else(
            || state.clone(),
            |input| {
                step_tui_runtime(state, events, input, "operator")
                    .state()
                    .clone()
            },
        )
    }

    #[test]
    fn right_walks_focus_nav_to_content_to_detail_and_clamps_at_detail() {
        // Finding F: right must reach the rightmost Detail pane and STOP there,
        // never wrapping around or switching the leftmost view.
        let events = demo_events();
        let nav = TuiInteractionState::new(0, TuiOverlay::None);
        assert_eq!(nav.focus(), FocusPane::Nav);

        let content = press(&nav, &events, KeyCode::Right);
        assert_eq!(content.focus(), FocusPane::Content);

        let detail = press(&content, &events, KeyCode::Right);
        assert_eq!(detail.focus(), FocusPane::Detail);

        // Third and fourth right presses stay clamped on Detail; the active view
        // never changes.
        let clamped = press(&detail, &events, KeyCode::Right);
        assert_eq!(clamped.focus(), FocusPane::Detail);
        let clamped_again = press(&clamped, &events, KeyCode::Right);
        assert_eq!(clamped_again.focus(), FocusPane::Detail);
        assert_eq!(clamped_again.active_view(), TuiView::Attention);
    }

    #[test]
    fn left_walks_focus_detail_to_content_to_nav_and_clamps_at_nav() {
        let events = demo_events();
        let detail = TuiInteractionState::new(0, TuiOverlay::None).with_focus(FocusPane::Detail);

        let content = press(&detail, &events, KeyCode::Left);
        assert_eq!(content.focus(), FocusPane::Content);

        let nav = press(&content, &events, KeyCode::Left);
        assert_eq!(nav.focus(), FocusPane::Nav);

        // Left clamps at the leftmost pane; the active view never changes.
        let clamped = press(&nav, &events, KeyCode::Left);
        assert_eq!(clamped.focus(), FocusPane::Nav);
        assert_eq!(clamped.active_view(), TuiView::Attention);
    }

    #[test]
    fn right_clamps_at_content_on_the_lanes_view_without_a_detail_pane() {
        // The Lanes view spans the full body width with no Detail pane, so the
        // rightmost focus step stops at Content (and render_body reports a zero
        // detail max scroll).
        let events = lane_render_events();
        let content = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_focus(FocusPane::Content);
        let clamped = press(&content, &events, KeyCode::Right);
        assert_eq!(clamped.focus(), FocusPane::Content);
        assert_eq!(clamped.active_view(), TuiView::Lanes);
    }

    #[test]
    fn up_down_on_the_nav_pane_still_reaches_every_view() {
        // With left/right repurposed to pane focus, view-switching lives on the
        // Views nav's up/down. Walking down must reach every view, then up walks
        // back and clamps at the first view.
        let events = demo_events();
        let mut state = TuiInteractionState::new(0, TuiOverlay::None);
        assert_eq!(state.focus(), FocusPane::Nav);
        let mut seen = vec![state.active_view()];
        for _ in 0..TuiView::all().len() {
            state = press(&state, &events, KeyCode::Down);
            seen.push(state.active_view());
        }
        // Every view must be reachable by walking the nav with up/down.
        for view in TuiView::all() {
            assert!(seen.contains(view));
        }
        for _ in 0..TuiView::all().len() {
            state = press(&state, &events, KeyCode::Up);
        }
        assert_eq!(state.active_view(), TuiView::Attention);
    }

    #[test]
    fn detail_pane_scrolls_clipped_lines_into_view_with_a_scrollbar() {
        // Finding F: an Attention detail that overflows the pane. At the top the
        // bottom timeline entries are clipped; scrolling down brings them into
        // the rendered buffer and pushes the first field off the top. A scrollbar
        // thumb marks the overflow.
        let timeline = (0..20)
            .map(|index| {
                TimelineEntry::new(
                    format!("evt_{index:02}"),
                    format!("timeline entry {index:02}"),
                    "src".to_owned(),
                )
            })
            .collect::<Vec<_>>();
        let detail = AttentionDetail::new(
            "repo".to_owned(),
            "work-item".to_owned(),
            "run".to_owned(),
            "fabro attach run".to_owned(),
            timeline,
            vec![],
        );
        // Inner height 8 rows (a 10-row pane minus its borders); the detail has
        // 4 + 1 (Timeline:) + 20 = 25 logical lines, so it overflows.
        let area = Rect::new(0, 0, 44, 10);

        // At the top: the first field shows, the last timeline entry is clipped,
        // the focused pane is tagged, and an overflow scrollbar thumb (█) draws.
        let mut top = Buffer::empty(area);
        render_detail(Some(&detail), 0, true, area, &mut top);
        let top_text = buffer_to_text(&top, area);
        assert!(top_text.contains("Repo: repo"));
        assert!(!top_text.contains("evt_19"));
        assert!(top_text.contains("Detail [focus]"));
        assert!(top_text.contains('\u{2588}'));

        // A large offset clamps to the bottom: the last entry is now visible and
        // the first field has scrolled off the top.
        let mut scrolled = Buffer::empty(area);
        render_detail(Some(&detail), 100, true, area, &mut scrolled);
        let scrolled_text = buffer_to_text(&scrolled, area);
        assert!(scrolled_text.contains("evt_19"));
        assert!(!scrolled_text.contains("Repo: repo"));
    }

    #[test]
    fn detail_scroll_down_reaches_the_true_wrapped_bottom_and_the_scrollbar_agrees() {
        // Finding G drift-guard: a detail whose fields and timeline entries WRAP
        // at a narrow pane width renders far more rows than its logical line
        // count. The scroll-down clamp must reach the true wrapped bottom — the
        // SAME wrapped count the scrollbar is sized from — not the width-agnostic
        // logical count, or the lower half of a long detail stays unreachable.
        // This pins the render's measured max scroll and the reducer's reachable
        // scroll to the ONE `Paragraph::line_count` measurement.
        let timeline = (0..6)
            .map(|index| {
                TimelineEntry::new(
                    format!(
                        "evt:orchestrator:livespec-orchestrator-beads-fabro:bd-ib-ss7rkr:{index}:snapshot"
                    ),
                    format!("timeline entry {index} with a long wrapping description marker-{index}"),
                    "orchestrator".to_owned(),
                )
            })
            .collect::<Vec<_>>();
        let detail = AttentionDetail::new(
            "livespec-orchestrator-beads-fabro".to_owned(),
            "bd-ib-ss7rkr".to_owned(),
            "fabro-run-5137117035853731187".to_owned(),
            "fabro attach fabro-run-5137117035853731187".to_owned(),
            timeline,
            vec![],
        );
        // A ~49-col-inner Detail pane (51 wide) only 8 rows tall (viewport 6),
        // mirroring the live 112x16 geometry where the bug reproduced. The
        // wrapped rows far exceed the 11 logical lines (4 fields + `Timeline:` +
        // 6 entries), so the logical clamp would strand the bottom.
        let area = Rect::new(0, 0, 51, 8);

        // Measure the wrapped max scroll the renderer clamps and sizes the
        // scrollbar to; the interactive loop feeds this back into the state.
        let mut probe = Buffer::empty(area);
        let max_scroll = render_detail(Some(&detail), 0, true, area, &mut probe);
        // The wrapped rows overflow well past the 11-line logical count (4 fields
        // + `Timeline:` + 6 entries), so the old logical clamp would strand the
        // bottom.
        assert!(max_scroll > 10);

        // Drive the reducer's ScrollDetailDown the way the loop does, with the
        // render-measured max fed into the state.
        let events: Vec<ConsoleEvent> = Vec::new();
        let mut state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_focus(FocusPane::Detail)
            .with_detail_max_scroll(max_scroll);
        for _ in 0..(max_scroll + 5) {
            state = reduce_tui_interaction(&state, &events, TuiInteraction::ScrollDetailDown);
        }
        // The scroll reaches EXACTLY the wrapped max — not a smaller logical count.
        assert_eq!(state.detail_scroll(), max_scroll);

        // The last wrapped line (the final timeline entry's tail) is below the
        // fold at the top, and reachable once scrolled to the clamped bottom.
        let mut top = Buffer::empty(area);
        let _top_max = render_detail(Some(&detail), 0, true, area, &mut top);
        assert!(!buffer_to_text(&top, area).contains("marker-5"));

        let mut bottom = Buffer::empty(area);
        let _bottom_max = render_detail(
            Some(&detail),
            state.detail_scroll(),
            true,
            area,
            &mut bottom,
        );
        let bottom_text = buffer_to_text(&bottom, area);
        // The last wrapped line (the final timeline entry's `marker-5` tail) is
        // reachable at the clamped bottom.
        assert!(bottom_text.contains("marker-5"));
        // With the pane scrolled to its clamped max, the scrollbar thumb reaches
        // the bottom of the track (an overflow thumb is drawn).
        assert!(bottom_text.contains('\u{2588}'));
    }

    #[test]
    fn detail_scroll_offset_clamps_at_the_render_measured_max_and_saturates_at_the_top() {
        let events = demo_events();
        // The renderer measures the Detail pane's wrapped max scroll and the loop
        // feeds it into the state; a Down keypress on the focused Detail pane
        // clamps to exactly that offset (not a width-agnostic logical count).
        let max = 7;
        let state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_focus(FocusPane::Detail)
            .with_detail_max_scroll(max);

        // Pressing down far past the end clamps the offset at the render-measured max.
        let mut scrolled = state;
        for _ in 0..(max + 5) {
            scrolled = press(&scrolled, &events, KeyCode::Down);
        }
        assert_eq!(scrolled.detail_scroll(), max);

        // Pressing up past the top saturates the offset at zero.
        let mut unscrolled = scrolled;
        for _ in 0..(max + 5) {
            unscrolled = press(&unscrolled, &events, KeyCode::Up);
        }
        assert_eq!(unscrolled.detail_scroll(), 0);
    }

    #[test]
    fn detail_scroll_resets_when_the_content_selection_changes() {
        // A scroll offset must not carry onto a different item's details, so
        // moving the content selection resets it to the top.
        let events = blocked_attention_events(4);
        let down = press(
            &TuiInteractionState::new(1, TuiOverlay::None)
                .with_focus(FocusPane::Content)
                .with_detail_scroll(3),
            &events,
            KeyCode::Down,
        );
        assert_eq!(down.selected_attention_index(), 2);
        assert_eq!(down.detail_scroll(), 0);

        let up = press(
            &TuiInteractionState::new(1, TuiOverlay::None)
                .with_focus(FocusPane::Content)
                .with_detail_scroll(3),
            &events,
            KeyCode::Up,
        );
        assert_eq!(up.selected_attention_index(), 0);
        assert_eq!(up.detail_scroll(), 0);
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

        // Detail focus: the right Detail pane carries the focus tag while neither
        // the Views nav nor the Attention content list does.
        let detail = build_tui_model_for_state(
            &demo_events(),
            &TuiInteractionState::new(0, TuiOverlay::None).with_focus(FocusPane::Detail),
        );
        let detail_output = render_to_text(&detail, 96, 24);
        assert_eq!(
            detail_output.as_ref().map(|r| r.contains("Detail [focus]")),
            Ok(true)
        );
        assert_eq!(
            detail_output
                .as_ref()
                .map(|r| r.contains("Attention [focus]")),
            Ok(false)
        );
        assert_eq!(
            detail_output.as_ref().map(|r| r.contains("Views [focus]")),
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
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &palette),
            Some(TuiTerminalInput::Confirm)
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
        let blind = build_tui_model(&blind_events, 0);

        // Narrow (the pinned small terminal): the header degrades gracefully but
        // the source COUNT — the cockpit-blind-vs-idle tell — always survives, so
        // a blind screen is never mistaken for an idle factory even when the names
        // cannot fit (see `header_line`).
        let narrow = render_to_text(&blind, 96, 24);
        assert_eq!(
            narrow
                .as_ref()
                .map(|rendered| rendered.contains("sources: 3 unavailable")),
            Ok(true)
        );

        // Wide: with room to spare the header names which sources are down.
        let wide = render_to_text(&blind, 160, 24);
        assert_eq!(
            wide.as_ref()
                .map(|rendered| rendered.contains("sources: 3 unavailable")),
            Ok(true)
        );
        assert_eq!(
            wide.as_ref()
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

    /// `count` blocked/needs-human work-items, each with a distinct id ranked in
    /// order, so they project into `count` attention rows for the scroll tests.
    fn blocked_attention_events(count: usize) -> Vec<ConsoleEvent> {
        (0..count)
            .map(|index| {
                lane_event(
                    &format!("evt_block_{index:03}"),
                    &format!("console-block-{index:03}"),
                    Lane::Blocked,
                    Some(LaneReason::NeedsHuman),
                    &format!("a{index:03}"),
                    "blocked",
                )
            })
            .collect()
    }

    #[test]
    fn render_scrolls_the_attention_list_to_keep_an_off_screen_selection_visible() {
        // A long attention list on the pinned small terminal (112x28): the
        // selected row sits far below the fold. A stateless, top-anchored list
        // would render only the first rows and never the selection, so its
        // `>`-marked row would be absent; scroll-to-selection brings the selected
        // row into view. The marker on a Blocked row appears nowhere else on the
        // screen, so its presence proves the list scrolled to the selection.
        let events = blocked_attention_events(40);
        let last = events.len() - 1;
        let state = TuiInteractionState::new(last, TuiOverlay::None).with_focus(FocusPane::Content);
        let model = build_tui_model_for_state(&events, &state);
        assert_eq!(model.attention_items().len(), 40);
        assert_eq!(model.selected_attention_index(), Some(last));

        // The selected row is visible only because the list scrolled to it: the
        // `>` marker on a Blocked row appears nowhere else on the screen.
        let output = render_to_text(&model, 112, 28);
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("> Blocked: needs-human")),
            Ok(true)
        );
    }

    /// Every lane filled with three work-items, so the lane overview's
    /// `7 headers + 21 preview rows` overflow the pinned pane height and the
    /// scroll behavior is exercised.
    fn full_board_events() -> Vec<ConsoleEvent> {
        let mut events = Vec::new();
        for (lane_index, lane) in Lane::all().iter().enumerate() {
            for item in 0..3 {
                events.push(lane_event(
                    &format!("evt_lane_{lane_index}_{item}"),
                    &format!("wi-{lane_index}-{item}"),
                    *lane,
                    None,
                    &format!("a{lane_index}{item}"),
                    "queued",
                ));
            }
        }
        events
    }

    #[test]
    fn render_scrolls_the_lane_overview_to_keep_the_selected_lane_visible() {
        // The last lane (`done`) sits below the fold in an overflowing overview.
        // A top-anchored render would never show it; scroll-to-selection brings
        // its `>`-marked header into view and pushes the first lane off the top.
        let last_lane = Lane::all().len() - 1;
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_selected_lane_index(last_lane);
        let model = build_tui_model_for_state(&full_board_events(), &state);

        let output = render_to_text(&model, 112, 28);
        // The selected bottom lane is scrolled into view...
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("> done (3)")),
            Ok(true)
        );
        // ...and the first lane is pushed off the top.
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("backlog (3)")),
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
        // The drill-in shows repo + rank alongside the id; the first item is the
        // selected per-item cursor, marked with `>`.
        assert_eq!(
            output
                .as_ref()
                .map(|r| r.contains("> console-ready-a  console  rank a0  [ready]")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|r| r.contains("console-ready-b  console  rank a1  [ready]")),
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

        render_summary_detail(&[], 0, false, area, &mut buffer);

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

        let lines = detail_lines(&detail);
        // The detail lines carry the optional actions line between the fixed
        // fields and the `Timeline:` header (the four fields + actions + header =
        // six logical lines here, before any wrapping).
        assert_eq!(lines.len(), 6);
        let rendered = lines
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
    // The Settings surface at the TUI runtime level: the six-row content pane,
    // the per-setting detail help, and the Enter/Space edit that persists a
    // `config.dispatcher_setting_set` command with no arming ceremony (Scenario
    // 9).
    // -----------------------------------------------------------------------

    const CONFIRM_REPO: &str = "livespec-console-beads-fabro";

    /// Six effective dispatcher settings for the Settings-surface tests, with
    /// `auto_approve_ready` off so editing it records a `false -> true` change.
    fn observed_settings() -> DispatcherSettings {
        DispatcherSettings::new(false, false, AcceptancePolicy::AiThenHuman, 3, 2, 5)
    }

    /// A Settings-view interaction state with row `selected` under the cursor and
    /// the Content pane focused (where an edit fires).
    fn settings_state(selected: usize) -> TuiInteractionState {
        TuiInteractionState::for_view(TuiView::Settings, 0, TuiOverlay::None)
            .with_selected_repo(CONFIRM_REPO.to_owned())
            .with_focus(FocusPane::Content)
            .with_selected_setting_index(selected)
            .with_dispatcher_settings(DispatcherSettingsRead::Observed(observed_settings()))
    }

    /// The Settings-view model for row `selected`.
    fn settings_model(selected: usize) -> TuiScreenModel {
        build_tui_model_for_state(&[], &settings_state(selected))
    }

    #[test]
    fn keymap_edits_a_settings_row_with_enter_and_space_and_leaves_the_a_key_free() {
        let model = settings_model(0);
        // Enter and Space on a Content-focused Settings row both resolve the edit.
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &model),
            Some(TuiTerminalInput::Confirm)
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char(' ')), &model),
            Some(TuiTerminalInput::Confirm)
        );
        // The `a` key is free: it is inert with no overlay open (the retired
        // autonomous toggle no longer binds it).
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('a')), &model),
            None
        );
        // Space is inert on a non-Settings view with no overlay open.
        assert_eq!(
            key_event_to_terminal_input(
                key(KeyCode::Char(' ')),
                &attention_model(TuiOverlay::None)
            ),
            None
        );
        // Space still types into an open search overlay.
        let searching = attention_model(TuiOverlay::Search {
            query: String::new(),
        });
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char(' ')), &searching),
            Some(TuiTerminalInput::Interaction(TuiInteraction::TypeChar(' ')))
        );
    }

    #[test]
    fn editing_a_settings_row_persists_a_dispatcher_setting_set_command_with_no_ceremony() {
        // Enter on the dangerous Auto-approve ready row submits an ordinary
        // `config.dispatcher_setting_set` command carrying that one setting -- no
        // confirm modal is opened.
        let state = settings_state(0);
        let step = step_tui_runtime(&state, &[], TuiTerminalInput::Confirm, "operator");

        let command = persisted_command(step.effect());
        assert_eq!(
            command.map(console_domain::CommandEnvelope::command_type),
            Some(&CommandType::ConfigDispatcherSettingSet)
        );
        let payload = persisted_payload(step.effect());
        assert_eq!(
            payload.map(|value| value.contains(r#""setting":"auto_approve_ready""#)),
            Some(true)
        );
        assert_eq!(
            payload.map(|value| value.contains(r#""value":true"#)),
            Some(true)
        );
        assert_eq!(
            payload.map(|value| value.contains(r#""repo":"livespec-console-beads-fabro""#)),
            Some(true)
        );
        // No overlay was opened -- the edit is not gated behind any modal.
        assert_eq!(step.state().overlay(), &TuiOverlay::None);
    }

    #[test]
    fn renders_six_settings_rows_the_selected_value_and_the_dangerous_label() {
        let rendered = render_to_text(&settings_model(0), 120, 24);
        let text = rendered.unwrap_or_default();
        // The six setting rows and their effective values.
        for label in [
            "Auto-approve ready",
            "Merge on review cap",
            "Acceptance mode",
            "Review fix cap",
            "Acceptance rework cap",
            "WIP cap",
        ] {
            assert!(text.contains(label), "missing row: {label}");
        }
        assert!(text.contains("ai-then-human"));
        // The selected dangerous row's detail help carries the required label.
        assert!(text.contains("dangerous / use with caution"));
    }

    #[test]
    fn renders_the_not_observed_placeholder_when_settings_are_unreadable() {
        let state = TuiInteractionState::for_view(TuiView::Settings, 0, TuiOverlay::None)
            .with_selected_repo(CONFIRM_REPO.to_owned());
        let model = build_tui_model_for_state(&[], &state);
        let rendered = render_to_text(&model, 120, 24);
        assert_eq!(
            rendered.map(|value| value.contains("Dispatcher settings not observed")),
            Ok(true)
        );
    }

    #[test]
    fn settings_detail_shows_the_no_selection_placeholder_when_no_row_is_selected() {
        // The detail builder is defensive: observed settings but no selected row
        // (a non-Settings view carries `selected_setting_index() == None`) yields
        // the no-selection placeholder rather than indexing out of range.
        let state = TuiInteractionState::for_view(TuiView::Attention, 0, TuiOverlay::None)
            .with_dispatcher_settings(DispatcherSettingsRead::Observed(observed_settings()));
        let model = build_tui_model_for_state(&[], &state);
        assert_eq!(model.selected_setting_index(), None);
        assert_eq!(
            settings_detail_lines(&model),
            vec![Line::from("No setting selected")]
        );
    }

    #[test]
    fn action_outcome_effect_maps_the_payload_bearing_persist_outcome() {
        let effect = action_outcome_effect(OperatorActionOutcome::PersistCommandWithPayload {
            command: console_domain::CommandEnvelope::new(
                "cmd".to_owned(),
                CommandType::ConfigDispatcherSettingSet,
                CONFIRM_REPO.to_owned(),
                "key".to_owned(),
                "operator".to_owned(),
            ),
            payload_json: r#"{"repo":"r","setting":"wip_cap","value":6}"#.to_owned(),
        });
        assert_eq!(
            persisted_payload(&effect),
            Some(r#"{"repo":"r","setting":"wip_cap","value":6}"#)
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

    /// One pending-approval work-item, so a drilled-in lane holds a selectable
    /// item the operator can approve.
    fn pending_lane_events() -> [ConsoleEvent; 1] {
        [lane_event(
            "evt_pa",
            "console-pending",
            Lane::PendingApproval,
            None,
            "a0",
            "pending-approval",
        )]
    }

    /// A Lanes-view state drilled into the pending-approval lane with its first
    /// item selected and the Content pane focused.
    fn drilled_pending_state(overlay: TuiOverlay) -> TuiInteractionState {
        TuiInteractionState::for_view(TuiView::Lanes, 0, overlay)
            .with_lane_focus(LaneFocus::Lane(Lane::PendingApproval))
            .with_selected_lane_item_index(0)
            .with_focus(FocusPane::Content)
    }

    #[test]
    fn valve_and_move_status_keys_fire_on_a_selected_lane_item() {
        let events = pending_lane_events();
        let model = build_tui_model_for_state(&events, &drilled_pending_state(TuiOverlay::None));
        // A per-item valve key now opens on a drilled-in lane item, not only in
        // the Attention view.
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('p')), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::OpenValveConfirm(PendingValve::Approve)
            ))
        );
        // `s` stages the move-status valve at the first drivable target.
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('s')), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::OpenValveConfirm(PendingValve::MoveStatus {
                    from: Lane::PendingApproval,
                    to: Lane::Ready,
                })
            ))
        );
    }

    #[test]
    fn move_status_on_a_pending_approval_lane_item_persists_the_approve_command() {
        // W7 proof: from a drilled-in lane, selecting a pending-approval item and
        // confirming the move-to-status valve persists the approve command that
        // the shared orchestrator port dispatches as `approve:<id>`
        // (pending-approval -> ready), with no payload.
        let state = drilled_pending_state(TuiOverlay::ValveConfirm {
            valve: PendingValve::MoveStatus {
                from: Lane::PendingApproval,
                to: Lane::Ready,
            },
        });
        let step = step_tui_runtime(
            &state,
            &pending_lane_events(),
            TuiTerminalInput::Confirm,
            "operator",
        );
        assert_eq!(
            persisted_command(step.effect()).map(console_domain::CommandEnvelope::command_type),
            Some(&CommandType::WorkItemApproveRequested)
        );
        assert_eq!(persisted_payload(step.effect()), None);
        assert_eq!(step.state().overlay(), &TuiOverlay::None);
    }

    #[test]
    fn valve_confirm_modal_targets_the_selected_lane_item_not_the_attention_detail() {
        // Operator-safety: in a drilled-in lane with a NON-EMPTY Attention inbox,
        // the valve-confirm modal's "Target:" line MUST name the LANE-selected
        // work-item (the same item `Enter` dispatches on via
        // `selected_work_item_id`), never the Attention detail. `lane_render_events`
        // puts two ready items in the Ready lane and one blocked (needs-human) item
        // that fills the Attention inbox, so the drilled Ready selection and the
        // Attention detail are guaranteed to be different work-items — the exact
        // condition under which reading `detail()` here would show the wrong id.
        let state = TuiInteractionState::for_view(
            TuiView::Lanes,
            0,
            TuiOverlay::ValveConfirm {
                valve: PendingValve::Approve,
            },
        )
        .with_lane_focus(LaneFocus::Lane(Lane::Ready))
        .with_selected_lane_item_index(0)
        .with_focus(FocusPane::Content);
        let model = build_tui_model_for_state(&lane_render_events(), &state);
        // Preconditions: the dispatch target is the lane selection, and the
        // Attention detail is a DIFFERENT, non-empty item.
        assert_eq!(model.selected_work_item_id(), Some("console-ready-a"));
        assert_eq!(
            model.detail().map(AttentionDetail::work_item),
            Some("console-blocked")
        );

        let rendered = render_to_text(&model, 96, 24).unwrap_or_default();
        // The modal names the lane selection, never the Attention detail.
        assert!(rendered.contains("Target: console-ready-a"));
        assert!(!rendered.contains("console-blocked"));
    }

    #[test]
    fn move_status_key_is_inert_without_a_drivable_target_and_literal_in_a_text_overlay() {
        // Drilled into the ready lane, whose items have no operator-drivable
        // onward transition, so `s` is inert.
        let ready = build_tui_model_for_state(
            &lane_render_events(),
            &TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
                .with_lane_focus(LaneFocus::Lane(Lane::Ready))
                .with_focus(FocusPane::Content),
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('s')), &ready),
            None
        );
        // Behind a text overlay `s` is a literal character.
        let search = attention_model(TuiOverlay::Search {
            query: String::new(),
        });
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('s')), &search),
            Some(TuiTerminalInput::Interaction(TuiInteraction::TypeChar('s')))
        );
    }

    #[test]
    fn help_overlay_is_scoped_to_the_active_view() {
        // The Settings view help names the six settings and the edit, not the
        // item-lane actions.
        let settings = build_tui_model_for_state(
            &[],
            &TuiInteractionState::for_view(TuiView::Settings, 0, TuiOverlay::Help),
        );
        let settings_help = render_to_text(&settings, 120, 72).unwrap_or_default();
        assert!(settings_help.contains("auto_approve_ready"));
        assert!(settings_help.contains("edit the selected setting row"));
        assert!(!settings_help.contains("move the selected work-item to a status"));

        // The Lanes view help describes item selection and the move-to-status
        // action.
        let lanes = build_tui_model_for_state(
            &lane_render_events(),
            &TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::Help),
        );
        let lanes_help = render_to_text(&lanes, 120, 72).unwrap_or_default();
        assert!(lanes_help.contains("move the selected work-item to a status"));
        assert!(lanes_help.contains("select an individual work-item"));
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
