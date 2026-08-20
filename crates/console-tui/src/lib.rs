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

use console_application::source_adapters::Lane;
use console_application::{
    ApplicationError, AttentionDetail, AttentionItem, DispatcherSettingsRead, FocusPane,
    HELP_SECTION_COUNT, HelpFocus, LaneColumn, LaneFocus, LaneWorkItem, OperatorAction,
    OperatorActionOutcome, PendingValve, PluginResolution, SettingRow, TimelineEntry,
    TuiInteraction, TuiInteractionState, TuiOverlay, TuiScreenModel, TuiView, ViewSummaryItem,
    action_registry, build_tui_model_for_state, dispatcher_setting_rows, header_help_section,
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

const UNAVAILABLE_HERE_MARKER: &str = "  (unavailable here)";

#[cfg(all(not(test), not(coverage)))]
use std::io;
// `Duration` backs the loop's keyboard poll only (source polling is off-thread
// now), so it lives with the terminal loop's build.
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
        PluginResolution::unresolved(),
        &mut effect_sink,
    )
}

#[cfg(all(not(test), not(coverage)))]
/// Run interactive tui with a live session and return deferred effects.
///
/// The `session` both applies the operator's effects and re-projects the latest
/// events on the loop's poll cadence (see [`TuiLiveSession`]), so the cockpit
/// stays live rather than rendering a snapshot frozen at startup.
pub fn run_interactive_tui_with_effect_sink(
    events: &[ConsoleEvent],
    requested_by: &str,
    selected_repo: &str,
    dispatcher_settings: DispatcherSettingsRead,
    plugin_resolution: PluginResolution,
    session: &mut dyn TuiLiveSession,
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
        plugin_resolution,
        session,
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
    plugin_resolution: PluginResolution,
    session: &mut dyn TuiLiveSession,
) -> io::Result<Vec<TuiRuntimeEffect>> {
    let mut state = TuiInteractionState::new(0, TuiOverlay::None)
        .with_selected_repo(selected_repo.to_owned())
        .with_dispatcher_settings(dispatcher_settings)
        .with_plugin_resolution(plugin_resolution);
    // The event log is OWNED and re-projected every iteration (Bug B fix): each
    // projection reduces over the LATEST events, not a snapshot frozen at
    // startup, so the board and detail panes stay live.
    let mut events = events.to_vec();
    let mut effects = Vec::new();
    loop {
        let model = build_tui_model_for_state(&events, &state);
        // Measure the Detail pane's wrapped max scroll while drawing and feed it
        // back into the state, so the next ScrollDetailDown clamps to the true
        // wrapped bottom (the SAME count the scrollbar is sized from) rather than
        // a width-agnostic logical line count (Finding G).
        let mut extents = RenderScrollExtents::ZERO;
        terminal.draw(|frame| {
            extents = render_model(&model, frame.area(), frame.buffer_mut());
        })?;
        state = state
            .with_detail_max_scroll(extents.detail_max_scroll)
            .with_header_max_scroll(extents.header_max_scroll)
            .with_help_scroll_extents(extents.help_max_scroll, extents.help_page_rows)
            .with_work_item_detail_scroll_extents(
                extents.work_item_detail_max_scroll,
                extents.work_item_detail_page_rows,
            );
        let tick = process_input_tick(&mut state, &events, requested_by, &mut effects, session)?;
        if matches!(tick, LoopTick::Quit) {
            return Ok(effects);
        }
        // Re-list the store every iteration (cheap — source polling is off-thread
        // now, so this never blocks). After a ledger-mutating effect, `refresh_events`
        // also pings the off-thread poller to re-poll sources at once, so the
        // operator's own action AND the ledger's lane change appear promptly.
        if let Some(fresh) = session.refresh_events(matches!(tick, LoopTick::Mutated))? {
            events = fresh;
        }
    }
}

/// The outcome of one 250 ms input tick, telling the loop whether to re-poll
/// sources at once or return.
#[cfg(all(not(test), not(coverage)))]
enum LoopTick {
    /// A poll timeout or an inert key — just re-project on the normal cadence.
    Idle,
    /// A ledger-mutating effect was applied — re-poll the sources at once so the
    /// operator sees their own action's lane change without waiting.
    Mutated,
    /// The operator asked to quit; return the deferred effects.
    Quit,
}

/// Handle at most one keyboard event: map it to an interaction, step the pure
/// runtime, apply the resulting effect through the session, and report whether
/// it mutated the ledger or asked to quit. Terminal-bound (blocks on
/// `event::poll`), so excluded from tests; the logic it composes
/// (`key_event_to_terminal_input`, `step_tui_runtime`,
/// `effect_triggers_source_poll`) is exercised directly.
#[cfg(all(not(test), not(coverage)))]
fn process_input_tick(
    state: &mut TuiInteractionState,
    events: &[ConsoleEvent],
    requested_by: &str,
    effects: &mut Vec<TuiRuntimeEffect>,
    session: &mut dyn TuiLiveSession,
) -> io::Result<LoopTick> {
    if !event::poll(Duration::from_millis(250))? {
        return Ok(LoopTick::Idle);
    }
    let Event::Key(key_event) = event::read()? else {
        return Ok(LoopTick::Idle);
    };
    if key_event.kind != KeyEventKind::Press {
        return Ok(LoopTick::Idle);
    }
    let model = build_tui_model_for_state(events, state);
    let Some(input) = key_event_to_terminal_input(key_event, &model) else {
        return Ok(LoopTick::Idle);
    };
    let step = step_tui_runtime(state, events, input, requested_by);
    *state = step.state().clone();
    let effect = step.effect().clone();
    let should_quit = matches!(effect, TuiRuntimeEffect::Quit);
    let mutated = effect_triggers_source_poll(&effect);
    if session.handle_runtime_effect(&effect)? == TuiRuntimeEffectSinkOutcome::Deferred {
        effects.push(effect);
    }
    if should_quit {
        return Ok(LoopTick::Quit);
    }
    Ok(if mutated {
        LoopTick::Mutated
    } else {
        LoopTick::Idle
    })
}

// `effect_triggers_source_poll` is used by the terminal loop (excluded from tests
// and coverage) and by the unit tests, so it is present exactly where it has a
// caller and absent only in the coverage-plain-lib build (`coverage`, no `test`),
// where the loop and the tests are both compiled out.
/// Whether a runtime effect mutated the ledger and so warrants an immediate
/// out-of-band source re-poll (so the operator sees their own action's lane
/// change promptly). Only the command-bearing effects — an approve / accept /
/// reject / move / policy write — change the ledger; navigation (`Render`), the
/// attach helpers, quit, and errors do not.
#[cfg(any(test, not(coverage)))]
#[must_use]
const fn effect_triggers_source_poll(effect: &TuiRuntimeEffect) -> bool {
    matches!(
        effect,
        TuiRuntimeEffect::PersistCommand(_) | TuiRuntimeEffect::PersistCommandWithPayload { .. }
    )
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
    /// Copy driver handoff command variant. This is render/copy only: it never
    /// becomes a persisted console command and never triggers a source poll.
    CopyDriverHandoff(String),
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

/// A live session driving the interactive loop.
///
/// It applies the operator's runtime effects (as a [`TuiRuntimeEffectSink`]) AND
/// re-projects the latest events, so every projection reduces over the newest
/// event log rather than a snapshot frozen at startup.
///
/// This is the testable seam behind the terminal loop: the loop itself is
/// terminal-bound and excluded from tests, but [`refresh_events`] is a cheap
/// re-list (source polling runs off-thread) and is exercised directly.
///
/// [`refresh_events`]: TuiLiveSession::refresh_events
pub trait TuiLiveSession: TuiRuntimeEffectSink {
    /// Re-project the latest events, returning `Some(events)` to replace the
    /// loop's current snapshot or `None` to keep it (the legacy no-store path).
    ///
    /// This is a CHEAP store re-list — it NEVER shells out, so the UI thread never
    /// blocks. Source polling runs on an off-thread poller that appends to the
    /// store on its cadence and on demand; this call just re-projects the current
    /// log. When `request_poll` is set (right after a ledger-mutating effect) the
    /// implementation additionally pings that poller to re-poll sources at once,
    /// so the ledger's lane change appears promptly; the operator's OWN
    /// just-appended outcome is already in the re-listed log.
    ///
    /// # Errors
    /// Returns an IO error when the store read fails.
    fn refresh_events(&mut self, request_poll: bool) -> std::io::Result<Option<Vec<ConsoleEvent>>>;
}

/// Effect sink that preserves the legacy end-of-session flush behavior.
pub struct DeferredTuiRuntimeEffectSink;

impl TuiRuntimeEffectSink for DeferredTuiRuntimeEffectSink {
    fn handle_runtime_effect(
        &mut self,
        effect: &TuiRuntimeEffect,
    ) -> std::io::Result<TuiRuntimeEffectSinkOutcome> {
        #[cfg(all(not(test), not(coverage)))]
        if let TuiRuntimeEffect::CopyDriverHandoff(command) = effect {
            write_osc52_copy(&mut std::io::stdout(), command)?;
            return Ok(TuiRuntimeEffectSinkOutcome::Applied);
        }
        let _ = effect;
        Ok(TuiRuntimeEffectSinkOutcome::Deferred)
    }
}

#[cfg(all(not(test), not(coverage)))]
fn write_osc52_copy(writer: &mut impl std::io::Write, text: &str) -> std::io::Result<()> {
    write!(writer, "\x1b]52;c;{}\x07", base64_encode(text.as_bytes()))?;
    writer.flush()
}

#[cfg(all(not(test), not(coverage)))]
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = *chunk.get(1).unwrap_or(&0);
        let third = *chunk.get(2).unwrap_or(&0);
        let packed = (u32::from(first) << 16) | (u32::from(second) << 8) | u32::from(third);
        encoded.push(char::from(ALPHABET[((packed >> 18) & 0x3f) as usize]));
        encoded.push(char::from(ALPHABET[((packed >> 12) & 0x3f) as usize]));
        if chunk.len() > 1 {
            encoded.push(char::from(ALPHABET[((packed >> 6) & 0x3f) as usize]));
        } else {
            encoded.push('=');
        }
        if chunk.len() > 2 {
            encoded.push(char::from(ALPHABET[(packed & 0x3f) as usize]));
        } else {
            encoded.push('=');
        }
    }
    encoded
}

impl TuiLiveSession for DeferredTuiRuntimeEffectSink {
    fn refresh_events(
        &mut self,
        _poll_sources: bool,
    ) -> std::io::Result<Option<Vec<ConsoleEvent>>> {
        // The legacy no-store path has no live source, so it keeps its startup
        // snapshot rather than re-projecting.
        Ok(None)
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

/// Enter on the invoker roster: stage the selected action's normal confirm
/// flow — the valve modal (with its own Target line and parameter cycling) or
/// the driver-handoff overlay — exactly as its hotkey would, THROUGH the same
/// registry availability derivation. An unavailable selection is inert.
fn invoker_confirm_step(
    state: &TuiInteractionState,
    events: &[ConsoleEvent],
    model: &TuiScreenModel,
    selected_action: usize,
    requested_by: &str,
) -> TuiRuntimeStep {
    let staged = action_registry::ACTION_REGISTRY
        .get(selected_action)
        .and_then(|spec| staged_without_selection(model, spec));
    let interaction = match staged {
        Some(action_registry::StagedAction::Valve(valve)) => {
            TuiInteraction::OpenValveConfirm(valve)
        }
        Some(action_registry::StagedAction::DriverHandoff) => TuiInteraction::OpenDriverHandoff,
        Some(action_registry::StagedAction::FactoryDrain) => {
            return TuiRuntimeStep::new(
                reduce_tui_interaction(state, events, TuiInteraction::CloseOverlay),
                TuiRuntimeEffect::PersistCommand(console_application::factory_drain_command(
                    requested_by,
                )),
            );
        }
        // Quit is the one global that is not an interaction: it ends the
        // session rather than transforming its state.
        Some(action_registry::StagedAction::Global(action)) => match global_interaction(action) {
            Some(global) => global,
            None => return TuiRuntimeStep::new(state.clone(), TuiRuntimeEffect::Quit),
        },
        None => return TuiRuntimeStep::new(state.clone(), TuiRuntimeEffect::Render),
    };
    TuiRuntimeStep::new(
        reduce_tui_interaction(state, events, interaction),
        TuiRuntimeEffect::Render,
    )
}

/// Enter on a menu item: stage the selected action's normal confirm flow,
/// through the SAME `staged_without_selection` path the hotkey and the invoker
/// row use. A menu that staged actions its own way would be a third encoding of
/// invocation, which is the defect this plan exists to retire.
fn menu_confirm_step(
    state: &TuiInteractionState,
    events: &[ConsoleEvent],
    model: &TuiScreenModel,
    top: usize,
    selected: usize,
    requested_by: &str,
) -> TuiRuntimeStep {
    let staged = action_registry::menu_actions(top)
        .get(selected)
        .and_then(|spec| staged_without_selection(model, spec));
    let interaction = match staged {
        Some(action_registry::StagedAction::Valve(valve)) => {
            TuiInteraction::OpenValveConfirm(valve)
        }
        Some(action_registry::StagedAction::DriverHandoff) => TuiInteraction::OpenDriverHandoff,
        Some(action_registry::StagedAction::FactoryDrain) => {
            return TuiRuntimeStep::new(
                reduce_tui_interaction(state, events, TuiInteraction::CloseOverlay),
                TuiRuntimeEffect::PersistCommand(console_application::factory_drain_command(
                    requested_by,
                )),
            );
        }
        Some(action_registry::StagedAction::Global(action)) => match global_interaction(action) {
            Some(global) => global,
            None => return TuiRuntimeStep::new(state.clone(), TuiRuntimeEffect::Quit),
        },
        None => {
            return TuiRuntimeStep::new(
                state.clone(),
                TuiRuntimeEffect::ApplicationError(ApplicationError::UnavailableOperatorAction),
            );
        }
    };
    TuiRuntimeStep::new(
        reduce_tui_interaction(state, events, interaction),
        TuiRuntimeEffect::Render,
    )
}

fn confirm_operator_action(
    state: &TuiInteractionState,
    events: &[ConsoleEvent],
    requested_by: &str,
) -> TuiRuntimeStep {
    let model = build_tui_model_for_state(events, state);
    // The palette's `actions` command OPENS the invoker roster rather than
    // resolving an operator action, so it returns a state transition here,
    // before the outcome match.
    if let TuiOverlay::CommandPalette { query } = model.overlay()
        && console_application::command_palette_query_opens_action_invoker(query)
    {
        return TuiRuntimeStep::new(
            reduce_tui_interaction(state, events, TuiInteraction::OpenActionInvoker),
            TuiRuntimeEffect::Render,
        );
    }
    if let TuiOverlay::ActionInvoker { selected_action } = model.overlay() {
        return invoker_confirm_step(state, events, &model, *selected_action, requested_by);
    }
    // A menu item stages through the SAME path as its hotkey and the invoker
    // row. Anything else would be a third invocation route for one action.
    if let TuiOverlay::Menu { top, selected } = model.overlay() {
        return menu_confirm_step(state, events, &model, *top, *selected, requested_by);
    }
    let outcome = match model.overlay() {
        TuiOverlay::CommandPalette { .. } => resolve_command_palette_action(&model, requested_by),
        TuiOverlay::ValveConfirm { .. } => resolve_valve_action(&model, requested_by),
        // `Enter`/`Space` on a Settings row is an ordinary recorded setting write
        // (no overlay, no arming ceremony).
        TuiOverlay::None if model.active_view() == TuiView::Settings => {
            resolve_dispatcher_setting_edit(&model, requested_by)
        }
        TuiOverlay::DriverHandoff { command } => Ok(OperatorActionOutcome::CopyDriverHandoff(
            command.to_owned(),
        )),
        TuiOverlay::None
        | TuiOverlay::Search { .. }
        | TuiOverlay::CommandModal { .. }
        // The invoker confirm returned above; this arm is unreachable for it.
        | TuiOverlay::ActionInvoker { .. }
        // The work-item detail modal is READ-ONLY: `enter_input` yields no
        // `Confirm` while it is open, so it never actually reaches here.
        | TuiOverlay::WorkItemDetail { .. }
        // The menu confirm returned above; this arm is unreachable for it.
        | TuiOverlay::Menu { .. }
        | TuiOverlay::Help { .. } => resolve_selected_operator_action(&model, requested_by),
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
        OperatorActionOutcome::CopyDriverHandoff(command) => {
            TuiRuntimeEffect::CopyDriverHandoff(command)
        }
    }
}

/// The interaction a global registry action reduces to, or `None` for the one
/// global that is not an interaction at all: quitting ends the session rather
/// than transforming its state.
///
/// Split out from [`global_input`] so the invoker roster can branch on
/// "interaction or quit" without carrying a `TuiTerminalInput::Confirm` arm
/// that nothing can reach.
const fn global_interaction(action: action_registry::GlobalAction) -> Option<TuiInteraction> {
    match action {
        action_registry::GlobalAction::OpenSearch => Some(TuiInteraction::OpenSearch),
        action_registry::GlobalAction::OpenCommandPalette => {
            Some(TuiInteraction::OpenCommandPalette)
        }
        action_registry::GlobalAction::OpenHelp => Some(TuiInteraction::OpenHelp),
        action_registry::GlobalAction::OpenMenu => Some(TuiInteraction::OpenMenu),
        action_registry::GlobalAction::Quit => None,
    }
}

/// The terminal input a global registry action produces.
fn global_input(action: action_registry::GlobalAction) -> TuiTerminalInput {
    global_interaction(action).map_or(TuiTerminalInput::Quit, TuiTerminalInput::Interaction)
}

/// A Control-held chord, resolved THROUGH the registry.
///
/// Honoured regardless of which overlay is open: `Ctrl-C` must quit from
/// anywhere, including mid-typing in the search field, which is why it is
/// checked before the overlay-sensitive match rather than inside it.
///
/// Deliberately defined ABOVE `key_event_to_terminal_input`: the
/// menu-completeness gate parses that function's body for its behaviour
/// population, and a helper's `KeyCode::` mention inside that span would be
/// double-counted as an arm.
fn control_chord_input(event: KeyEvent) -> Option<TuiTerminalInput> {
    if !event.modifiers.contains(KeyModifiers::CONTROL) {
        return None;
    }
    let KeyCode::Char(value) = event.code else {
        return None;
    };
    let action = action_registry::global_action_for_chord(action_registry::KeyChord::ctrl(value))?;
    Some(global_input(action))
}

#[must_use]
/// Return the key event to terminal input value.
pub fn key_event_to_terminal_input(
    event: KeyEvent,
    model: &TuiScreenModel,
) -> Option<TuiTerminalInput> {
    let overlay = model.overlay();
    if let Some(input) = control_chord_input(event) {
        return Some(input);
    }
    match event.code {
        KeyCode::Up => up_interaction(model).map(TuiTerminalInput::Interaction),
        KeyCode::Down => down_interaction(model).map(TuiTerminalInput::Interaction),
        KeyCode::Esc => Some(TuiTerminalInput::Interaction(esc_interaction(model))),
        KeyCode::Enter => enter_input(model),
        KeyCode::Backspace => Some(TuiTerminalInput::Interaction(TuiInteraction::Backspace)),
        // `/`, `:`, `?` and `q` USED to be matched here, ahead of the registry
        // lookup below. That shadowing was a second encoding of the
        // key-to-action mapping and made those four unreachable from a
        // generated menu; they are registry entries now, so the generic arm
        // resolves them. Space stays: it is argued out in the gate's carve-out
        // fixture as pure focus movement, so it has no registry entry to find.
        KeyCode::Char(' ') => space_input(model, overlay),
        KeyCode::Char(value) => {
            action_registry::action_for_chord(action_registry::KeyChord::plain(value)).map_or_else(
                || text_input(value, overlay),
                |spec| registry_action_input(model, spec, value),
            )
        }
        KeyCode::Left => left_input(model),
        KeyCode::Right => right_input(model),
        KeyCode::Tab => tab_input(model, true),
        KeyCode::BackTab => tab_input(model, false),
        KeyCode::PageUp => page_scroll_input(overlay, false),
        KeyCode::PageDown => page_scroll_input(overlay, true),
        KeyCode::Home
        | KeyCode::End
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

/// Up: in a command modal, move the action selection; behind the modal Help
/// overlay, act in the focused Help pane; with no overlay open, act
/// within the focused pane (move the Views selection on the nav, the content
/// selection in the content list, or scroll the Detail pane up); behind any
/// other overlay it is the harmless content move.
const fn up_interaction(model: &TuiScreenModel) -> Option<TuiInteraction> {
    Some(match model.overlay() {
        TuiOverlay::CommandModal { .. }
        | TuiOverlay::ActionInvoker { .. }
        | TuiOverlay::Menu { .. } => TuiInteraction::SelectPreviousAction,
        TuiOverlay::ValveConfirm { .. } => TuiInteraction::CycleValveOption(false),
        TuiOverlay::Help { focus, .. } => match focus {
            HelpFocus::Menu => TuiInteraction::HelpSelectPreviousSection,
            HelpFocus::Text => TuiInteraction::HelpScrollUp,
        },
        // The item modal is a READING surface, so up/down scroll its body
        // rather than moving a selection -- there is nothing to select in it.
        TuiOverlay::WorkItemDetail { .. } => TuiInteraction::WorkItemDetailScrollUp(1),
        TuiOverlay::DriverHandoff { .. } => return None,
        TuiOverlay::None => match model.focus() {
            FocusPane::Nav => TuiInteraction::SelectPreviousView,
            FocusPane::Content => TuiInteraction::SelectPrevious,
            FocusPane::Detail => TuiInteraction::ScrollDetailUp,
            // The focused Header pane scrolls only horizontally (left/right);
            // up/down are inert on it.
            FocusPane::Header => return None,
        },
        TuiOverlay::Search { .. } | TuiOverlay::CommandPalette { .. } => {
            TuiInteraction::SelectPrevious
        }
    })
}

/// Down: the mirror of [`up_interaction`].
const fn down_interaction(model: &TuiScreenModel) -> Option<TuiInteraction> {
    Some(match model.overlay() {
        TuiOverlay::CommandModal { .. }
        | TuiOverlay::ActionInvoker { .. }
        | TuiOverlay::Menu { .. } => TuiInteraction::SelectNextAction,
        TuiOverlay::ValveConfirm { .. } => TuiInteraction::CycleValveOption(true),
        TuiOverlay::Help { focus, .. } => match focus {
            HelpFocus::Menu => TuiInteraction::HelpSelectNextSection,
            HelpFocus::Text => TuiInteraction::HelpScrollDown,
        },
        TuiOverlay::WorkItemDetail { .. } => TuiInteraction::WorkItemDetailScrollDown(1),
        TuiOverlay::DriverHandoff { .. } => return None,
        TuiOverlay::None => match model.focus() {
            FocusPane::Nav => TuiInteraction::SelectNextView,
            FocusPane::Content => TuiInteraction::SelectNext,
            FocusPane::Detail => TuiInteraction::ScrollDetailDown,
            // The focused Header pane scrolls only horizontally; up/down inert.
            FocusPane::Header => return None,
        },
        TuiOverlay::Search { .. } | TuiOverlay::CommandPalette { .. } => TuiInteraction::SelectNext,
    })
}

/// Enter: confirm a command modal / valve-confirm modal; behind a text/help
/// overlay it is inert; with no overlay open it dives into the focused pane (see
/// [`enter_content_input`]).
fn enter_input(model: &TuiScreenModel) -> Option<TuiTerminalInput> {
    match model.overlay() {
        TuiOverlay::CommandModal { .. }
        | TuiOverlay::CommandPalette { .. }
        | TuiOverlay::ActionInvoker { .. }
        | TuiOverlay::Menu { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::DriverHandoff { .. } => Some(TuiTerminalInput::Confirm),
        TuiOverlay::Search { .. } | TuiOverlay::Help { .. } | TuiOverlay::WorkItemDetail { .. } => {
            None
        }
        TuiOverlay::None => enter_content_input(model),
    }
}

/// Enter with no overlay open: from the Views nav it dives focus into the
/// Content pane; in the Content pane it drills into the selected lane (lane
/// overview), opens the selected work-item's detail modal (Attention or a
/// drilled-in lane), edits the selected Settings row, or opens the command modal
/// only through explicit interactions that have actions to offer; on the Detail
/// pane it is inert.
fn enter_content_input(model: &TuiScreenModel) -> Option<TuiTerminalInput> {
    match model.focus() {
        FocusPane::Nav => Some(TuiTerminalInput::Interaction(TuiInteraction::FocusContent)),
        FocusPane::Content => {
            if model.active_view() == TuiView::Lanes {
                return match model.lane_focus() {
                    LaneFocus::Overview => {
                        Some(TuiTerminalInput::Interaction(TuiInteraction::DrillIntoLane))
                    }
                    // Inside a drilled-in lane Enter carries the drill one level
                    // FURTHER IN -- from the lane's item list to the selected
                    // item's own record. It was inert here, which left the
                    // console with no surface at all for a work-item's title or
                    // description; only a row with a real item behind it opens,
                    // so an empty lane keeps Enter inert.
                    LaneFocus::Lane(_lane) => model.selected_lane_item().map(|_item| {
                        TuiTerminalInput::Interaction(TuiInteraction::OpenWorkItemDetail)
                    }),
                };
            }
            if model.active_view() == TuiView::Attention {
                return model.selected_work_item_id().map(|_work_item_id| {
                    TuiTerminalInput::Interaction(TuiInteraction::OpenWorkItemDetail)
                });
            }
            // A Settings row edit is an ordinary recorded write resolved on
            // `Confirm`; the read-only summary views have no Enter action.
            if model.active_view() == TuiView::Settings {
                return Some(TuiTerminalInput::Confirm);
            }
            None
        }
        // Enter is inert on the Detail pane and the focused Header pane (the
        // header scrolls, it does not open).
        FocusPane::Detail | FocusPane::Header => None,
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
        // Esc leaves the focused Header pane, returning to the Views nav (and
        // resetting the header scroll via `with_focus`).
        FocusPane::Header => TuiInteraction::FocusNav,
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

/// Left: inside Help it focuses the section menu; inside Menu it walks the
/// top-level nodes; behind any other overlay it is inert. With no overlay open,
/// it walks focus one pane toward the nav, then enters the visible menu bar from
/// the resting left edge so the bar is reachable without its registry hotkey.
fn left_input(model: &TuiScreenModel) -> Option<TuiTerminalInput> {
    if matches!(model.overlay(), TuiOverlay::Help { .. }) {
        return Some(TuiTerminalInput::Interaction(TuiInteraction::HelpFocusMenu));
    }
    if matches!(model.overlay(), TuiOverlay::Menu { .. }) {
        return Some(TuiTerminalInput::Interaction(
            TuiInteraction::MenuPreviousTop,
        ));
    }
    if model.overlay().is_open() {
        return None;
    }
    let interaction = match model.focus() {
        // Leftmost pane: enter the permanent bar that is rendered above it.
        FocusPane::Nav => TuiInteraction::OpenMenu,
        FocusPane::Content => content_back_interaction(model),
        FocusPane::Detail => TuiInteraction::FocusContent,
        // On the focused Header pane, left/right scroll horizontally instead of
        // walking the body panes.
        FocusPane::Header => TuiInteraction::ScrollHeaderLeft,
    };
    Some(TuiTerminalInput::Interaction(interaction))
}

/// Right: inside Help it focuses the prose pane; behind any other overlay it is
/// inert; otherwise it walks focus one pane toward the Detail pane, clamped at
/// the rightmost.
const fn right_input(model: &TuiScreenModel) -> Option<TuiTerminalInput> {
    if matches!(model.overlay(), TuiOverlay::Help { .. }) {
        return Some(TuiTerminalInput::Interaction(TuiInteraction::HelpFocusText));
    }
    if matches!(model.overlay(), TuiOverlay::Menu { .. }) {
        return Some(TuiTerminalInput::Interaction(TuiInteraction::MenuNextTop));
    }
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
        // On the focused Header pane, left/right scroll horizontally instead of
        // walking the body panes.
        FocusPane::Header => TuiInteraction::ScrollHeaderRight,
    };
    Some(TuiTerminalInput::Interaction(interaction))
}

/// Whether the active view renders a right-hand Detail pane (every view except
/// `Lanes`, which spans the full body width beside the nav). Used to clamp the
/// rightmost focus step at Content on the Lanes view.
const fn view_has_detail_pane(model: &TuiScreenModel) -> bool {
    !matches!(model.active_view(), TuiView::Lanes)
}

/// `Tab` / `BackTab`: behind an overlay it is inert (the overlay owns navigation);
/// otherwise it cycles focus one pane forward (`Tab`) or backward (`BackTab`)
/// around the pane ring, which — unlike the spatial `left`/`right` body walk —
/// INCLUDES the top/header pane, so the header can be focused like any other pane.
const fn tab_input(model: &TuiScreenModel, forward: bool) -> Option<TuiTerminalInput> {
    if model.overlay().is_open() {
        return None;
    }
    Some(TuiTerminalInput::Interaction(if forward {
        TuiInteraction::FocusNextPane
    } else {
        TuiInteraction::FocusPreviousPane
    }))
}

/// `PageUp` / `PageDown`: while the modal Help overlay or the work-item detail
/// modal is open they page that surface's text up/down; everywhere else they are
/// inert. Both surfaces scroll UP and DOWN only, so no horizontal counterpart
/// exists.
const fn page_scroll_input(overlay: &TuiOverlay, down: bool) -> Option<TuiTerminalInput> {
    match overlay {
        TuiOverlay::Help { .. } => Some(TuiTerminalInput::Interaction(if down {
            TuiInteraction::HelpPageDown
        } else {
            TuiInteraction::HelpPageUp
        })),
        TuiOverlay::WorkItemDetail { .. } => Some(TuiTerminalInput::Interaction(if down {
            TuiInteraction::WorkItemDetailPageDown
        } else {
            TuiInteraction::WorkItemDetailPageUp
        })),
        TuiOverlay::None
        | TuiOverlay::Search { .. }
        | TuiOverlay::CommandPalette { .. }
        | TuiOverlay::CommandModal { .. }
        | TuiOverlay::ActionInvoker { .. }
        | TuiOverlay::Menu { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::DriverHandoff { .. } => None,
    }
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

/// A registered action's hotkey: with no overlay open, stage the action for
/// the selected work-item exactly where its Status-line hint is offered — the
/// key consults the SAME registry availability derivation the hints derive
/// from, so hidden hints and inert keys cannot diverge. On a selection the
/// registry does not offer the action for, the key is inert; behind an open
/// text overlay the character is a literal.
fn registry_action_input(
    model: &TuiScreenModel,
    spec: &'static action_registry::ActionSpec,
    character: char,
) -> Option<TuiTerminalInput> {
    let overlay = model.overlay();
    if !matches!(overlay, TuiOverlay::None) {
        return text_input(character, overlay);
    }
    // Global actions are answered BEFORE a selection is demanded: search, the
    // palette, help and quit are reachable with nothing selected, which is
    // exactly why they lived outside the registry before chords existed.
    match staged_without_selection(model, spec)? {
        action_registry::StagedAction::Valve(valve) => Some(TuiTerminalInput::Interaction(
            TuiInteraction::OpenValveConfirm(valve),
        )),
        action_registry::StagedAction::DriverHandoff => Some(TuiTerminalInput::Interaction(
            TuiInteraction::OpenDriverHandoff,
        )),
        action_registry::StagedAction::FactoryDrain => None,
        action_registry::StagedAction::Global(action) => Some(global_input(action)),
    }
}

/// Stage `spec`, demanding a selection only where the action needs one.
///
/// A global action needs NO selection: requiring one would make search, the
/// palette, help and quit inert on an empty lane — and inert in the invoker
/// roster, the one surface whose entire purpose is that every registered
/// action is reachable.
fn staged_without_selection(
    model: &TuiScreenModel,
    spec: &'static action_registry::ActionSpec,
) -> Option<action_registry::StagedAction> {
    if matches!(
        spec.staging,
        action_registry::ActionStaging::Global(_) | action_registry::ActionStaging::FactoryDrain
    ) {
        return action_registry::stage_action(spec, &model.global_action_context());
    }
    let ctx = model.selected_action_context()?;
    action_registry::stage_action(spec, &ctx)
}

fn action_available_for_model(
    model: &TuiScreenModel,
    spec: &'static action_registry::ActionSpec,
) -> bool {
    if matches!(
        spec.staging,
        action_registry::ActionStaging::Global(_) | action_registry::ActionStaging::FactoryDrain
    ) {
        return (spec.availability)(&model.global_action_context());
    }
    model
        .selected_action_context()
        .is_some_and(|ctx| (spec.availability)(&ctx))
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

/// The per-frame scroll extents a full [`render_model`] pass measures.
///
/// The interactive loop feeds them back into the interaction state so the next
/// scroll press clamps to the true content edge at the CURRENT viewport size.
/// Mirrors how the Detail pane's `detail_max_scroll` is measured-and-fed-back,
/// now also carrying the Header pane's horizontal `header_max_scroll`.
pub struct RenderScrollExtents {
    /// The Detail pane's maximum vertical scroll offset — the wrapped-aware
    /// largest offset that keeps the pane's last row visible, or `0` for a view
    /// without a Detail pane (the SAME wrapped line count the scrollbar is sized
    /// from).
    pub detail_max_scroll: usize,
    /// The Header pane's maximum horizontal scroll offset — the header's display
    /// width beyond the pane's inner width — or `0` unless the Header pane is
    /// focused and its full line overflows the current viewport width.
    pub header_max_scroll: usize,
    /// The work-item detail modal's maximum vertical scroll offset measured from
    /// its wrapped body, or `0` when that modal is not open.
    pub work_item_detail_max_scroll: usize,
    /// The work-item detail modal's visible content rows, or `1` when that modal
    /// is not open or is too small to measure.
    pub work_item_detail_page_rows: usize,
    /// The Help overlay prose pane's maximum vertical scroll offset measured
    /// from its wrapped body, or `0` when Help is not open.
    pub help_max_scroll: usize,
    /// The Help overlay prose pane's visible content rows, or `1` when Help is
    /// not open or is too small to measure.
    pub help_page_rows: usize,
}

impl RenderScrollExtents {
    /// The zero extents returned when there is nothing to render (an empty area),
    /// so neither pane's scroll clamp advances off a frame that drew nothing.
    const ZERO: Self = Self {
        detail_max_scroll: 0,
        header_max_scroll: 0,
        work_item_detail_max_scroll: 0,
        work_item_detail_page_rows: 1,
        help_max_scroll: 0,
        help_page_rows: 1,
    };
}

/// Render the whole screen and return the per-frame [`RenderScrollExtents`].
///
/// The extents are the Detail pane's maximum vertical scroll and the Header
/// pane's maximum horizontal scroll — so the interactive loop can clamp the
/// persisted scroll state to what actually fits at the current viewport size.
pub fn render_model(
    model: &TuiScreenModel,
    area: Rect,
    buffer: &mut Buffer,
) -> RenderScrollExtents {
    if area.is_empty() {
        return RenderScrollExtents::ZERO;
    }
    // The Status line is a bordered box like the header: height 3 leaves exactly
    // ONE inner content row for the context-specific shortcut hints (a height-2
    // box would leave zero inner rows, which is why the old static hint never
    // rendered). This bottom band sits BELOW the Help modal's 3-row bottom margin
    // (the modal insets by 3 on every side), so the Status hints stay visible and
    // tmux-capturable while a modal is open -- the modal's `Clear` never reaches
    // this row. See `help_overlay_rect` and `render_footer`.
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(area);
    let header_max_scroll = render_header(model, vertical[0], buffer);
    render_menu_bar(model, vertical[1], buffer);
    let detail_max_scroll = render_body(model, vertical[2], buffer);
    render_footer(model, vertical[3], buffer);
    let menu_area = Rect::new(
        area.x,
        vertical[1].y,
        area.width,
        vertical[1].height.saturating_add(vertical[2].height),
    );
    let overlay_extents = render_overlay(model, area, menu_area, buffer);
    RenderScrollExtents {
        detail_max_scroll,
        header_max_scroll,
        work_item_detail_max_scroll: overlay_extents.work_item_detail.max_scroll,
        work_item_detail_page_rows: overlay_extents.work_item_detail.page_rows,
        help_max_scroll: overlay_extents.help.max_scroll,
        help_page_rows: overlay_extents.help.page_rows,
    }
}

/// Render the top Header pane and return its maximum horizontal scroll offset
/// (`0` unless the pane is focused AND the full header overflows the pane's inner
/// width).
///
/// A BLURRED header keeps the shrink-to-fit default (`header_line`), which
/// degrades gracefully on a narrow viewport — dropping low-value fields rather
/// than letting a long field clip the ones after it — so B1's cockpit-blind
/// "sources unavailable" tell always survives. A FOCUSED header instead renders
/// the FULL, un-degraded header line panned by the pane's horizontal scroll
/// offset, so content clipped at the current width is reachable by scrolling
/// left/right; its block title carries the `[focus]` marker every other focused
/// pane uses.
fn render_header(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) -> usize {
    let inner_width = usize::from(area.width.saturating_sub(2));
    let focused = model.focus() == FocusPane::Header;
    let title = focus_title("LiveSpec Console", focused);
    if !focused {
        Paragraph::new(model.header_line(inner_width))
            .block(Block::new().borders(Borders::ALL).title(title))
            .render(area, buffer);
        return 0;
    }
    // Focused: pan the FULL, un-degraded line. The max scroll is the header's
    // display width beyond the pane's inner width; clamp the offset to it so a
    // stale scroll (for example after a resize) never pans past the right edge,
    // and return the max so the interactive loop feeds it back and the reducer's
    // scroll-right clamp agrees with what is actually clipped at this width.
    let full = model.header();
    let full_width = full.chars().count();
    let max_scroll = full_width.saturating_sub(inner_width);
    let offset = model.header_scroll().min(max_scroll);
    let visible: String = full.chars().skip(offset).take(inner_width).collect();
    Paragraph::new(visible)
        .block(Block::new().borders(Borders::ALL).title(title))
        .render(area, buffer);
    max_scroll
}

fn render_menu_bar(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    let selected_top = match model.overlay() {
        TuiOverlay::Menu { top, .. } => Some(*top),
        TuiOverlay::None
        | TuiOverlay::Search { .. }
        | TuiOverlay::CommandPalette { .. }
        | TuiOverlay::CommandModal { .. }
        | TuiOverlay::ActionInvoker { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::DriverHandoff { .. }
        | TuiOverlay::WorkItemDetail { .. }
        | TuiOverlay::Help { .. } => None,
    };
    render_menu_bar_for_top(selected_top, area, buffer);
}

fn render_menu_bar_for_top(selected_top: Option<usize>, area: Rect, buffer: &mut Buffer) {
    let tree = action_registry::menu_tree();
    let selected_top = selected_top.map(|top| top.min(tree.len().saturating_sub(1)));
    let bar: String = tree
        .iter()
        .enumerate()
        .map(|(index, node)| {
            if Some(index) == selected_top {
                format!("[{}]", node.label)
            } else {
                format!(" {} ", node.label)
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let bar_rect = Rect::new(area.x, area.y, area.width, 1.min(area.height));
    Clear.render(bar_rect, buffer);
    Widget::render(Line::from(format!("Menu: {bar}")), bar_rect, buffer);
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
            "{marker} {} ({}){}",
            column.lane().label(),
            column.count(),
            lane_execution_summary(column),
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

/// A compact overview-line for one work-item: id, status, title, and (when
/// blocked) its lane reason.
fn lane_item_summary(item: &LaneWorkItem) -> String {
    format!(
        "    - {} [{}]{}  {}{}",
        item.work_item_id(),
        item.status(),
        lane_execution_state_suffix(item),
        lane_item_title(item),
        lane_reason_suffix(item)
    )
}

/// A full drill-in line for one work-item: id, rank, status, title, repo, and
/// reason. Repo sits after the human title so narrow panes keep the triage
/// fields operators scan first: id, rank, status, and title.
fn lane_item_detail_text(item: &LaneWorkItem) -> String {
    format!(
        "{}  rank {}  [{}]{}  {}  repo {}{}",
        item.work_item_id(),
        item.rank(),
        item.status(),
        lane_execution_state_suffix(item),
        lane_item_title(item),
        item.repo(),
        lane_reason_suffix(item)
    )
}

/// The title rendered in lane rows, or a stable placeholder for legacy
/// snapshots that predate standardized work-item details.
fn lane_item_title(item: &LaneWorkItem) -> &str {
    item.detail().title.as_deref().unwrap_or("(untitled)")
}

/// The ` (reason)` suffix for a blocked work-item, or empty when none.
fn lane_reason_suffix(item: &LaneWorkItem) -> String {
    item.lane_reason()
        .map(|reason| format!(" ({})", reason.label()))
        .unwrap_or_default()
}

fn lane_execution_summary(column: &LaneColumn) -> String {
    if column.lane() != Lane::Active || column.count() == 0 {
        return String::new();
    }
    if column.finished_unreconciled_count() == 0 {
        return format!(
            "; executing {} claimed {}",
            column.executing_count(),
            column.claimed_count()
        );
    }
    format!(
        "; executing {} claimed {} finished? {}",
        column.executing_count(),
        column.claimed_count(),
        column.finished_unreconciled_count()
    )
}

fn lane_execution_state_suffix(item: &LaneWorkItem) -> String {
    if item.lane() == Lane::Active {
        format!(" {}", item.execution_state().label())
    } else {
        String::new()
    }
}

fn render_footer(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    Paragraph::new(model.footer())
        .block(Block::new().borders(Borders::ALL).title("Status"))
        .render(area, buffer);
}

fn render_overlay(
    model: &TuiScreenModel,
    area: Rect,
    menu_area: Rect,
    buffer: &mut Buffer,
) -> OverlayScrollExtents {
    match model.overlay() {
        TuiOverlay::None => OverlayScrollExtents::ZERO,
        TuiOverlay::Search { query } => {
            render_prompt_overlay("Search", format!("/{query}"), area, buffer);
            OverlayScrollExtents::ZERO
        }
        TuiOverlay::CommandPalette { query } => {
            render_prompt_overlay("Command Palette", format!(":{query}"), area, buffer);
            OverlayScrollExtents::ZERO
        }
        TuiOverlay::ActionInvoker { selected_action } => {
            render_action_invoker(model, *selected_action, overlay_rect(area), buffer);
            OverlayScrollExtents::ZERO
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
            OverlayScrollExtents::ZERO
        }
        TuiOverlay::Menu { top, selected } => {
            render_menu_overlay(model, *top, *selected, menu_area, buffer);
            OverlayScrollExtents::ZERO
        }
        TuiOverlay::DriverHandoff { command } => {
            render_driver_handoff_overlay(command, area, buffer);
            OverlayScrollExtents::ZERO
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
            OverlayScrollExtents::ZERO
        }
        TuiOverlay::WorkItemDetail {
            work_item_id,
            scroll,
        } => {
            // Near-full-screen (the Help modal's inset), because the record it
            // shows is long. It resolves the record by the id PINNED when the
            // modal opened, NOT by the lane selection index: ingestion keeps
            // appending while the modal is open, and a re-ranked or
            // newly-inserted sibling would otherwise slide a different
            // work-item under the same index and silently swap the record the
            // operator is reading.
            OverlayScrollExtents {
                work_item_detail: render_work_item_detail(
                    model.work_item_by_id(work_item_id),
                    work_item_id,
                    model.action_failure_for(work_item_id),
                    help_overlay_rect(area),
                    buffer,
                    *scroll,
                ),
                help: HelpScrollExtents::ZERO,
            }
        }
        TuiOverlay::Help {
            focus,
            selected_section,
            scroll,
        } => OverlayScrollExtents {
            work_item_detail: WorkItemDetailScrollExtents::ZERO,
            help: render_help_overlay(
                help_overlay_rect(area),
                buffer,
                *focus,
                *selected_section,
                *scroll,
            ),
        },
    }
}

/// Render the valve-confirm modal: the staged valve, its target work-item, the
/// dialed-in mode/policy for a payload valve (cycled with up/down), and a
/// "dangerous / use with caution" caution before a destructive reject. `Enter`
/// submits; `Esc` cancels.
fn render_driver_handoff_overlay(command: &str, area: Rect, buffer: &mut Buffer) {
    let overlay = full_width_overlay_rect(area);
    Clear.render(overlay, buffer);
    let lines = vec![
        Line::from(command.to_owned()),
        Line::from("enter copy sent to terminal | esc cancel"),
    ];
    Paragraph::new(lines)
        .block(Block::new().borders(Borders::ALL).title("Driver Handoff"))
        .render(overlay, buffer);
}

fn full_width_overlay_rect(area: Rect) -> Rect {
    let height = 5.min(area.height.max(1));
    Rect::new(
        area.x,
        area.y + ((area.height - height) / 2),
        area.width.max(1),
        height,
    )
}

fn render_valve_confirm(valve: PendingValve, work_item: &str, area: Rect, buffer: &mut Buffer) {
    Clear.render(area, buffer);
    let mut lines = vec![
        Line::from(format!("{} work-item", valve.valve_label())),
        Line::from(format!("Target: {work_item}")),
    ];
    if let Some(option) = valve.option_display() {
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

/// The placeholder rendered for a record field the orchestrator did not emit.
///
/// An absent field is shown as absent rather than hidden: a blank row would let
/// the operator mistake "not set" for "not displayed", and the point of this
/// modal is that the record on screen is the whole record.
const ITEM_FIELD_ABSENT: &str = "—";

/// Render the work-item detail modal: a near-full-screen bordered window over
/// the main screen showing the FULL standardized record of `item` — the surface
/// that makes a work-item's title and description readable inside the console at
/// all.
///
/// `scroll` is the topmost visible wrapped row, clamped here to the record's
/// wrapped height exactly as the Help pane clamps its own, so a long description
/// scrolls to its true bottom and no further. `esc to close` sits on a reserved
/// bottom row, always visible regardless of the scroll offset.
fn render_work_item_detail(
    item: Option<&LaneWorkItem>,
    work_item_id: &str,
    failure: Option<&console_application::ActionFailure>,
    area: Rect,
    buffer: &mut Buffer,
    scroll: usize,
) -> WorkItemDetailScrollExtents {
    Clear.render(area, buffer);
    // The title always names the PINNED id, so it stays correct even in the
    // window where the item has left the board and no record resolves.
    let title = format!("Work item: {work_item_id}");
    let outer = Block::new().borders(Borders::ALL).title(title);
    let inner = outer.inner(area);
    outer.render(area, buffer);
    if inner.width == 0 || inner.height == 0 {
        return WorkItemDetailScrollExtents::ZERO;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let mut body = item.map_or_else(
        || {
            // Honest, not blank: the item was on the board when the modal
            // opened and is not now, so say THAT rather than substituting a
            // neighbouring record or rendering an empty box.
            vec![Line::from(format!(
                "{work_item_id} is no longer on the board (it may have been re-observed or removed)"
            ))]
        },
        work_item_detail_lines,
    );
    if let Some(failure) = failure {
        // The refusal the action surface emitted for this item's latest failed
        // action — rendered on the reading surface instead of being discarded
        // at the presentation boundary.
        body.push(Line::from(""));
        body.push(Line::from(format!(
            "Last action: {}",
            failure.display_line()
        )));
    }
    let paragraph = Paragraph::new(body).wrap(Wrap { trim: false });
    let max_scroll = paragraph
        .line_count(rows[0].width)
        .saturating_sub(usize::from(rows[0].height));
    let offset = scroll.min(max_scroll);
    paragraph
        .scroll((u16::try_from(offset).unwrap_or(u16::MAX), 0))
        .render(rows[0], buffer);
    Paragraph::new(Line::from("up/down scroll | esc to close")).render(rows[1], buffer);
    WorkItemDetailScrollExtents {
        max_scroll,
        page_rows: usize::from(rows[0].height).max(1),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OverlayScrollExtents {
    work_item_detail: WorkItemDetailScrollExtents,
    help: HelpScrollExtents,
}

impl OverlayScrollExtents {
    const ZERO: Self = Self {
        work_item_detail: WorkItemDetailScrollExtents::ZERO,
        help: HelpScrollExtents::ZERO,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WorkItemDetailScrollExtents {
    max_scroll: usize,
    page_rows: usize,
}

impl WorkItemDetailScrollExtents {
    const ZERO: Self = Self {
        max_scroll: 0,
        page_rows: 1,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HelpScrollExtents {
    max_scroll: usize,
    page_rows: usize,
}

impl HelpScrollExtents {
    const ZERO: Self = Self {
        max_scroll: 0,
        page_rows: 1,
    };
}

/// The full standardized work-item record as display lines.
///
/// Ordered so the operator reads identity first (title, id, repo), then the
/// lifecycle placement the lane row already showed, then provenance, then the
/// free-text body last — a long description must not push the one-line facts off
/// the top of the modal. Every field renders, absent ones as
/// [`ITEM_FIELD_ABSENT`], so the record on screen is the whole record.
fn work_item_detail_lines(item: &LaneWorkItem) -> Vec<Line<'static>> {
    let detail = item.detail();
    let mut lines = vec![
        Line::from(optional_field(detail.title.as_deref()))
            .style(Style::new().add_modifier(Modifier::BOLD)),
        Line::from(String::new()),
    ];
    let depends_on = if detail.depends_on.is_empty() {
        ITEM_FIELD_ABSENT.to_owned()
    } else {
        detail.depends_on.join(", ")
    };
    for (label, value) in [
        ("id", item.work_item_id().to_owned()),
        ("repo", item.repo().to_owned()),
        ("type", optional_field(detail.item_type.as_deref())),
        ("status", item.status().to_owned()),
        ("execution_state", item.execution_state().label().to_owned()),
        ("lane", item.lane().label().to_owned()),
        ("rank", item.rank().to_owned()),
        (
            "lane_reason",
            item.lane_reason().map_or_else(
                || ITEM_FIELD_ABSENT.to_owned(),
                |reason| reason.label().to_owned(),
            ),
        ),
        (
            "admission_policy",
            policy_field(
                detail.admission_policy.as_deref(),
                item.admission_policy().label(),
            ),
        ),
        (
            "acceptance_policy",
            policy_field(
                detail.acceptance_policy.as_deref(),
                item.acceptance_policy().label(),
            ),
        ),
        ("origin", optional_field(detail.origin.as_deref())),
        ("gap_id", optional_field(detail.gap_id.as_deref())),
        ("assignee", optional_field(detail.assignee.as_deref())),
        ("depends_on", depends_on),
        ("captured_at", optional_field(detail.captured_at.as_deref())),
        ("resolution", optional_field(detail.resolution.as_deref())),
        ("reason", optional_field(detail.reason.as_deref())),
        ("audit", optional_field(detail.audit.as_deref())),
        (
            "superseded_by",
            optional_field(detail.superseded_by.as_deref()),
        ),
        (
            "spec_commitment_hint",
            optional_field(detail.spec_commitment_hint.as_deref()),
        ),
        ("supersedes", optional_field(detail.supersedes.as_deref())),
        (
            "blocked_reason",
            optional_field(detail.blocked_reason.as_deref()),
        ),
        (
            "factory_safety",
            optional_field(detail.factory_safety.as_deref()),
        ),
    ] {
        lines.push(Line::from(format!("{label:<21}{value}")));
    }
    push_text_block(
        &mut lines,
        "acceptance_criteria",
        detail.acceptance_criteria.as_deref(),
    );
    push_text_block(&mut lines, "notes", detail.notes.as_deref());
    lines.push(Line::from(String::new()));
    lines.push(Line::from("description").style(Style::new().add_modifier(Modifier::BOLD)));
    // The markdown body is carried VERBATIM -- split only on its own newlines,
    // never re-wrapped or trimmed here (the paragraph's soft wrap handles width),
    // so what the operator reads is what the ledger holds.
    match detail.description.as_deref() {
        Some(description) => {
            lines.extend(description.lines().map(|line| Line::from(line.to_owned())));
        }
        None => lines.push(Line::from(ITEM_FIELD_ABSENT)),
    }
    lines
}

/// Append a labelled free-text block for a possibly-multi-line record field.
///
/// The long free-text fields (`acceptance_criteria`, `notes`) get the same
/// treatment as the description -- their own heading and their text carried
/// verbatim -- because squeezing them onto a label-and-value row would truncate
/// real operator content. An unset one still renders, as the absent placeholder,
/// so it cannot be mistaken for a field the surface simply does not show.
fn push_text_block(lines: &mut Vec<Line<'static>>, label: &str, value: Option<&str>) {
    lines.push(Line::from(String::new()));
    lines.push(Line::from(label.to_owned()).style(Style::new().add_modifier(Modifier::BOLD)));
    match value {
        Some(text) => lines.extend(text.lines().map(|line| Line::from(line.to_owned()))),
        None => lines.push(Line::from(ITEM_FIELD_ABSENT)),
    }
}

/// A policy field as display text: the value the orchestrator EMITTED, or the
/// absent placeholder plus the default the console falls back to.
///
/// The wire emits `null` for both policies on most records, and `null` does not
/// mean the default -- the orchestrator resolves it from the nearest ancestor
/// epic. The console cannot see that graph, so it must not print its own
/// fallback as though it were the record's value: that would show an
/// explicitly-set policy and an unset one identically, and would be flatly wrong
/// for an item inheriting a non-default policy. The fallback is still worth
/// showing, because it IS what this console acts on -- so it is shown, labelled
/// as the console's own assumption rather than as the item's field.
fn policy_field(emitted: Option<&str>, console_default: &str) -> String {
    emitted.map_or_else(
        || format!("{ITEM_FIELD_ABSENT} (not emitted; console assumes {console_default})"),
        str::to_owned,
    )
}

/// One optional record field as display text, or [`ITEM_FIELD_ABSENT`] when the
/// orchestrator emitted no value.
fn optional_field(value: Option<&str>) -> String {
    value.map_or_else(|| ITEM_FIELD_ABSENT.to_owned(), str::to_owned)
}

/// The character frame (margin) the modal Help window leaves between its box and
/// the viewport edge on every side, per the TUI Contract: the window occupies
/// nearly the full viewport with only a 3-character border on each side and on
/// top and bottom, and it never renders wider than the viewport.
const HELP_MODAL_MARGIN: u16 = 3;

/// Width of the modal Help left-side section menu column (fits the longest
/// section label plus its `> ` selection marker, beside a right-border divider).
const HELP_MENU_WIDTH: u16 = 22;

/// The modal Help window rect: the viewport inset by [`HELP_MODAL_MARGIN`] on
/// every side, so a 3-character frame of the underlying screen shows around it,
/// it occupies nearly the full viewport, and it never renders wider than the
/// viewport. Degrades to a minimal rect on a viewport too small to inset.
fn help_overlay_rect(area: Rect) -> Rect {
    let margin = HELP_MODAL_MARGIN;
    let width = area.width.saturating_sub(margin.saturating_mul(2)).max(1);
    let height = area.height.saturating_sub(margin.saturating_mul(2)).max(1);
    Rect::new(
        area.x.saturating_add(margin),
        area.y.saturating_add(margin),
        width,
        height,
    )
}

/// Render the navigable, pane-specific modal Help overlay (Scenario 18 / B4): a
/// bordered window drawn ON TOP of the main screen, laid out as a LEFT-side
/// section menu beside a RIGHT-side help-text pane that scrolls UP and DOWN only.
///
/// `selected_section` is the menu section in focus -- `0` is `Global actions`,
/// then one section per focusable pane in `TuiView::all()` order; `scroll` is the
/// right pane's topmost visible wrapped row, clamped here to the section's
/// wrapped height. `esc to exit` is printed at the bottom at all times, and the
/// modal closes ONLY on `Esc`. The text MUST stay in lock-step with the key
/// handler and the footer hint.
fn render_help_overlay(
    area: Rect,
    buffer: &mut Buffer,
    focus: HelpFocus,
    selected_section: usize,
    scroll: usize,
) -> HelpScrollExtents {
    Clear.render(area, buffer);
    let outer = Block::new().borders(Borders::ALL).title("Help");
    let inner = outer.inner(area);
    outer.render(area, buffer);
    if inner.width == 0 || inner.height == 0 {
        return HelpScrollExtents::ZERO;
    }
    // Reserve the bottom row for the always-visible `esc to exit` line; the menu
    // and the scrollable text pane share the region above it.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(HELP_MENU_WIDTH), Constraint::Min(1)])
        .split(rows[0]);
    render_help_menu(
        columns[0],
        buffer,
        focus == HelpFocus::Menu,
        selected_section,
    );
    let extents = render_help_section_text(columns[1], buffer, selected_section, scroll);
    // `esc to exit` sits on its own reserved row, so it is printed at the bottom
    // at all times regardless of the selected section or the scroll offset.
    Paragraph::new(Line::from("esc to exit")).render(rows[1], buffer);
    extents
}

/// Render the modal Help LEFT-side section menu: one row per section (`Global
/// actions` plus one per focusable pane), the selected row marked `>` and drawn
/// bold/reversed, beside a right-border divider separating it from the text pane.
fn render_help_menu(area: Rect, buffer: &mut Buffer, focused: bool, selected_section: usize) {
    let divider = Block::new().borders(Borders::RIGHT);
    let inner = divider.inner(area);
    divider.render(area, buffer);
    let items = (0..HELP_SECTION_COUNT)
        .map(|section| {
            let marker = if section == selected_section {
                "> "
            } else {
                "  "
            };
            let line = Line::from(format!("{marker}{}", help_section_label(section)));
            if section == selected_section {
                let modifier = if focused {
                    Modifier::BOLD | Modifier::REVERSED
                } else {
                    Modifier::BOLD
                };
                line.style(Style::new().add_modifier(modifier))
            } else {
                line
            }
        })
        .collect::<Vec<_>>();
    Paragraph::new(items).render(inner, buffer);
}

/// The stable label for Help menu section `section`: `0` is `Global actions`;
/// the middle sections are the focusable view panes in `TuiView::all()` order;
/// the LAST section is the top/header pane.
fn help_section_label(section: usize) -> &'static str {
    if section == header_help_section() {
        return "Header";
    }
    section
        .checked_sub(1)
        .and_then(|view_index| TuiView::all().get(view_index))
        .map_or("Global actions", |view| view.label())
}

/// Render the modal Help RIGHT-side text pane for the selected section: the
/// section's help lines, wrapped (so the pane scrolls UP and DOWN only, never
/// left or right) and clamped so `scroll` never runs past the last wrapped row.
fn render_help_section_text(
    area: Rect,
    buffer: &mut Buffer,
    selected_section: usize,
    scroll: usize,
) -> HelpScrollExtents {
    let lines = help_section_lines(selected_section);
    let viewport = usize::from(area.height);
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    let content_rows = paragraph.line_count(area.width);
    let max_scroll = content_rows.saturating_sub(viewport);
    let offset = scroll.min(max_scroll);
    paragraph
        .scroll((u16::try_from(offset).unwrap_or(u16::MAX), 0))
        .render(area, buffer);
    HelpScrollExtents {
        max_scroll,
        page_rows: viewport.max(1),
    }
}

/// The help lines for menu section `section`: section `0` is the `Global
/// actions` keybinding reference; the middle sections are the per-view panes; the
/// LAST section is the top/header pane.
fn help_section_lines(section: usize) -> Vec<Line<'static>> {
    if section == header_help_section() {
        return header_help_lines();
    }
    section
        .checked_sub(1)
        .and_then(|view_index| TuiView::all().get(view_index).copied())
        .map_or_else(global_help_lines, help_lines_for_view)
}

/// The top/header pane's help section: what the pane shows plus the keys usable
/// while it is focused. Kept in lock-step with the key handler and Scenario 20.
fn header_help_lines() -> Vec<Line<'static>> {
    vec![
        Line::from("Header -- the top status line: fleet / mode / repo / view / attention,"),
        Line::from("plus a source-health tell when any backing source is unavailable."),
        Line::from(""),
        Line::from("On a narrow viewport the blurred header shrinks to fit, dropping its"),
        Line::from("low-value fields; focus it to read the FULL line and scroll it sideways."),
        Line::from(""),
        Line::from("tab / shift-tab  cycle focus onto (and off) the header, like any pane"),
        Line::from("left / right     scroll the focused header horizontally to reveal"),
        Line::from("                 content clipped at the current width"),
        Line::from("esc              leave the header (returns to the Views nav)"),
        Line::from("On blur the header snaps back to its left-justified default."),
    ]
}

/// The `Global actions` section: the navigation and command keys available from
/// every view, plus how the modal Help itself is navigated.
fn global_help_lines() -> Vec<Line<'static>> {
    vec![
        Line::from("Global actions -- available from every view:"),
        Line::from(""),
        Line::from("up / down    navigate the focused pane; in this help, the section menu"),
        Line::from("left / right move focus across the body panes (Views -> Content -> Detail);"),
        Line::from("             left from Views opens the menu; focused header scrolls instead"),
        Line::from("tab / s-tab  cycle focus across every pane, including the top header"),
        Line::from("enter        dive from the nav into content, or open the selected item"),
        Line::from(
            "esc          step focus back (Detail -> Content -> nav; drilled lane -> overview)",
        ),
        Line::from("/            open search"),
        Line::from(":            open the command palette (drain, actions)"),
        Line::from("?            open this help"),
        Line::from("q / ctrl-c   quit"),
        Line::from(""),
        Line::from("In this help: left/right choose menu or text pane; up/down act there,"),
        Line::from("PgUp/PgDn page this pane, and only Esc closes it."),
    ]
}

/// The per-item action roster, DERIVED from the action registry: one line per
/// action the surface can offer, in canonical order, so the Help modal cannot
/// drift from the real action set (the second-encoding defect class the
/// registry exists to retire).
fn registry_help_lines(surface: action_registry::ActionSurface) -> Vec<Line<'static>> {
    action_registry::ACTION_REGISTRY
        .iter()
        .filter(|spec| action_registry::action_offered_on_surface(spec, surface))
        .map(|spec| {
            let text = spec.parameter.map_or_else(
                || format!("{} (confirm modal)", spec.label),
                |parameter| {
                    format!(
                        "{} -- {}: {} (up/down cycle)",
                        spec.label,
                        parameter.name,
                        parameter.choices.join(" | ")
                    )
                },
            );
            let key_display = action_registry::accelerator_display(spec);
            Line::from(format!("{key_display:<13}{text}"))
        })
        .collect()
}

/// The per-pane help section for `view`: what the pane shows plus the keys usable
/// while it is focused. Kept in lock-step with the key handler.
fn help_lines_for_view(view: TuiView) -> Vec<Line<'static>> {
    match view {
        TuiView::Attention => {
            let mut lines = vec![
                Line::from("Attention -- the default view: the merged, ranked needs-attention"),
                Line::from("list across the fleet, with the selected item's detail on the right."),
                Line::from(""),
                Line::from("up / down    move the Content selection, or scroll the Detail pane"),
                Line::from("enter        open the command modal for the selected work-item"),
            ];
            lines.extend(registry_help_lines(
                action_registry::ActionSurface::Attention,
            ));
            lines
        }
        TuiView::Spec => vec![
            Line::from("Spec -- the spec-side status view (read-only): the specification's"),
            Line::from("lifecycle state for the selected repo."),
            Line::from(""),
            Line::from("up / down    move the Content selection, or scroll the Detail pane"),
            Line::from("left / right move focus; left from Views opens the menu bar"),
        ],
        TuiView::Lanes => vec![
            Line::from("Lanes -- the work-item lane board: every lane column beside the nav."),
            Line::from("Enter drills into a lane for its full rank-ordered list, then into"),
            Line::from("the selected work-item for its full standardized record."),
            Line::from(""),
            Line::from("up / down    move the lane selection; in a drilled-in lane,"),
            Line::from("             select an individual work-item"),
            Line::from("enter        drill into the selected lane; in a drilled-in lane,"),
            Line::from("             open the selected work-item's record (title, description,"),
            Line::from("             type, origin, gap_id, assignee, depends_on, captured_at,"),
            Line::from("             resolution / reason / audit / superseded_by, and the"),
            Line::from("             spec commitment hint; up/down and PgUp/PgDn scroll it)"),
            Line::from("esc          close the work-item record, then return a drilled-in"),
            Line::from("             lane to its overview"),
        ]
        .into_iter()
        .chain(registry_help_lines(
            action_registry::ActionSurface::LaneDrill,
        ))
        .collect(),
        TuiView::Events => vec![
            Line::from("Events -- the console event timeline (read-only): the observed"),
            Line::from("source events for the selected repo."),
            Line::from(""),
            Line::from("up / down    move the Content selection, or scroll the Detail pane"),
            Line::from("left / right move focus; left from Views opens the menu bar"),
        ],
        TuiView::Repos => vec![
            Line::from("Repos -- the fleet repo roster (read-only): the repos the console"),
            Line::from("observes, with the selected repo's detail on the right."),
            Line::from(""),
            Line::from("up / down    move the Content selection, or scroll the Detail pane"),
            Line::from("left / right move focus; left from Views opens the menu bar"),
        ],
        TuiView::Settings => vec![
            Line::from("Settings -- the dispatcher-settings surface: one row per orchestrator"),
            Line::from("setting, each showing the effective value and inline help."),
            Line::from(""),
            Line::from("The six dispatcher policy settings:"),
            Line::from("  auto_approve_ready, merge_on_review_cap, acceptance_mode,"),
            Line::from("  review_fix_cap, acceptance_rework_cap, wip_cap."),
            Line::from("enter / space  edit the selected setting row (an ordinary recorded write)"),
            Line::from("A non-default value that lets the factory act without a human is"),
            Line::from("labelled \"dangerous / use with caution\"."),
        ],
    }
}

fn render_prompt_overlay(title: &'static str, value: String, area: Rect, buffer: &mut Buffer) {
    let overlay = overlay_rect(area);
    Clear.render(overlay, buffer);
    Paragraph::new(value)
        .block(Block::new().borders(Borders::ALL).title(title))
        .render(overlay, buffer);
}

/// Render the generic action-invoker roster: EVERY registered action, in
/// canonical order, with its hotkey (or `menu` for the hotkey-less ones) and
/// an `unavailable` marker where the registry does not offer the action for
/// the current selection — offered actions stage their normal confirm flow on
/// Enter; unavailable rows are selectable but inert (the spec scenario's
/// "presented as unavailable").
fn render_action_invoker(
    model: &TuiScreenModel,
    selected_action: usize,
    area: Rect,
    buffer: &mut Buffer,
) {
    Clear.render(area, buffer);
    let items = action_registry::ACTION_REGISTRY
        .iter()
        .enumerate()
        .map(|(index, spec)| {
            let marker = if index == selected_action { ">" } else { " " };
            let key_display = action_registry::accelerator_display(spec);
            let availability = if action_available_for_model(model, spec) {
                ""
            } else {
                UNAVAILABLE_HERE_MARKER
            };
            let row = format!("{marker} {} [{key_display}]{availability}", spec.label);
            ListItem::new(row).style(if index == selected_action {
                Style::new().add_modifier(Modifier::BOLD)
            } else {
                Style::new()
            })
        });
    Widget::render(
        List::new(items).block(Block::new().borders(Borders::ALL).title("Actions")),
        area,
        buffer,
    );
}

/// Render the menu BAR plus the open node's submenu, both GENERATED from the
/// registry's `menu_path` taxonomy.
///
/// The bar is the full top row so every top-level node is visible at once —
/// menus are the PRIMARY navigation mechanism, and a bar you must already be
/// inside to see does not navigate. Group labels render as headers but are not
/// selectable; selection addresses the flattened action list, which is what
/// `menu_actions` returns.
///
/// Accelerators render BESIDE their items. That is what makes "hotkeys are only
/// additional" visible to an operator rather than merely asserted: the menu is
/// the route, the key is a shortcut printed next to it.
fn render_menu_overlay(
    model: &TuiScreenModel,
    top: usize,
    selected: usize,
    area: Rect,
    buffer: &mut Buffer,
) {
    let tree = action_registry::menu_tree();
    // The open node is addressed through `get` rather than an `is_empty` early
    // return plus indexing: ACTION_REGISTRY is a non-empty const, so an
    // empty-tree return is a line no test can ever reach, and an unreachable
    // guard is worse than a total expression that simply renders nothing.
    let top = top.min(tree.len().saturating_sub(1));
    let node = tree.get(top);
    render_menu_bar_for_top(Some(top), area, buffer);

    if area.height <= 1 {
        return;
    }
    let body = Rect::new(area.x, area.y + 1, area.width, area.height - 1);
    Clear.render(body, buffer);

    let mut rows: Vec<ListItem<'static>> = Vec::new();
    let mut action_index = 0usize;
    for group in node.into_iter().flat_map(|node| node.groups.iter()) {
        rows.push(
            ListItem::new(format!("  {}", group.label))
                .style(Style::new().add_modifier(Modifier::DIM)),
        );
        for spec in &group.actions {
            let marker = if action_index == selected { ">" } else { " " };
            let accelerator = action_registry::accelerator_display(spec);
            let available = action_available_for_model(model, spec);
            let availability = if available {
                ""
            } else {
                UNAVAILABLE_HERE_MARKER
            };
            let mut style = Style::new();
            if action_index == selected {
                style = style.add_modifier(Modifier::BOLD);
            }
            if !available {
                style = style.add_modifier(Modifier::DIM);
            }
            rows.push(
                ListItem::new(format!(
                    "{marker}   {} [{accelerator}]{availability}",
                    spec.label
                ))
                .style(style),
            );
            action_index += 1;
        }
    }
    Widget::render(
        List::new(rows).block(
            Block::new()
                .borders(Borders::ALL)
                .title(node.map_or("", |node| node.label).to_owned()),
        ),
        body,
        buffer,
    );
}

fn render_command_modal(
    detail: Option<&AttentionDetail>,
    selected_action_index: usize,
    area: Rect,
    buffer: &mut Buffer,
) {
    let actions = detail.map(AttentionDetail::actions).unwrap_or_default();
    if actions.is_empty() {
        return;
    }
    Clear.render(area, buffer);
    let items = actions
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

/// The Detail-pane lines for a summary view: one `title` line per projection
/// row, followed by a `detail` line only when the row carries operational
/// detail (a repo list, the latest-event summary); a row whose operational
/// content is fully carried by its title contributes the title line alone, with
/// no trailing `:` and no empty detail line. A single placeholder renders when
/// there are no rows. A standalone builder so the scroll behavior can be
/// exercised over its length.
fn summary_detail_lines(items: &[ViewSummaryItem]) -> Vec<Line<'static>> {
    if items.is_empty() {
        return vec![Line::from("No projection rows")];
    }
    items
        .iter()
        .flat_map(|item| {
            if item.detail().is_empty() {
                vec![Line::from(item.title().to_owned())]
            } else {
                vec![
                    Line::from(format!("{}:", item.title())),
                    Line::from(item.detail().to_owned()),
                ]
            }
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
            let items = std::iter::once(plugin_resolution_row(model.plugin_resolution()))
                .chain(
                    rows.iter()
                        .enumerate()
                        .map(|(index, row)| settings_row_line(model, index, row)),
                )
                .collect::<Vec<_>>();
            let count = items.len();
            let list = List::new(items).block(block);
            let mut list_state = ListState::default();
            list_state.select(model.selected_setting_index().map(|index| index + 1));
            StatefulWidget::render(list, area, buffer, &mut list_state);
            render_vertical_scrollbar(area, buffer, count, list_state.offset());
        }
        DispatcherSettingsRead::NotObserved => {
            Paragraph::new(vec![
                plugin_resolution_line(model.plugin_resolution()),
                Line::from("Dispatcher settings not observed"),
            ])
            .block(block)
            .render(area, buffer);
        }
    }
}

fn plugin_resolution_row(plugin: &PluginResolution) -> ListItem<'static> {
    ListItem::new(plugin_resolution_line(plugin))
}

fn plugin_resolution_line(plugin: &PluginResolution) -> Line<'static> {
    Line::from(format!(
        "  Orchestrator plugin  [ {} ]",
        plugin.version().unwrap_or("unknown build")
    ))
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
    let mut lines = plugin_resolution_detail_lines(model.plugin_resolution());
    let DispatcherSettingsRead::Observed(settings) = model.dispatcher_settings() else {
        lines.push(Line::from(String::new()));
        lines.push(Line::from("Dispatcher settings not observed"));
        return lines;
    };
    let rows = dispatcher_setting_rows(settings);
    lines.push(Line::from(String::new()));
    lines.extend(
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
            ),
    );
    lines
}

fn plugin_resolution_detail_lines(plugin: &PluginResolution) -> Vec<Line<'static>> {
    vec![
        Line::from("Orchestrator plugin"),
        Line::from(format!("Source: {}", plugin.source())),
        Line::from(format!(
            "Build: {}",
            plugin.version().unwrap_or("unknown build")
        )),
        Line::from(format!("Root: {}", plugin.root().unwrap_or("not resolved"))),
    ]
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
    ];
    if let Some(command) = detail.attach_command() {
        lines.push(Line::from(format!("Attach: {command}")));
    }
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
            row.push_str(buffer[(x, y)].symbol());
        }
        rows.push(row.trim_end().to_owned());
    }
    rows.join("\n")
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    use console_application::source_adapters::LaneReason;
    use console_application::source_adapters::{
        AcceptancePolicy, AdmissionPolicy, AttentionHandoff, AttentionItemSnapshot,
        AttentionSourceRef, DispatcherJournalEntry, DispatcherJournalKind, Lane,
        attention_item_payload_json, dispatcher_journal_payload_json,
    };
    use console_application::{
        AttentionDetail, AttentionItem, DispatcherOverride, DispatcherSettings,
        DispatcherSettingsRead, FocusPane, HelpFocus, LaneFocus, LaneWorkItem, OperatorAction,
        OperatorActionOutcome, OverrideBool, OverrideInt, PendingValve, PluginResolution,
        RejectMode, TimelineEntry, TuiInteraction, TuiInteractionState, TuiOverlay, TuiScreenModel,
        TuiView, action_registry, build_tui_model, build_tui_model_for_state, header_help_section,
        help_section_for_view, reduce_tui_interaction,
    };

    use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::text::Line;

    use super::{
        DeferredTuiRuntimeEffectSink, ITEM_FIELD_ABSENT, TuiLiveSession, TuiRenderError,
        TuiRenderResult, TuiRuntimeEffect, TuiRuntimeEffectSink, TuiRuntimeEffectSinkOutcome,
        TuiTerminalInput, action_outcome_effect, attention_item_line, buffer_to_text, detail_lines,
        effect_triggers_source_poll, help_lines_for_view, key_event_to_terminal_input,
        menu_confirm_step, registry_action_input, render_command_modal, render_detail,
        render_menu_overlay, render_model, render_summary_detail, render_to_text,
        render_work_item_detail, settings_detail_lines, step_tui_runtime,
    };

    #[test]
    fn deferred_runtime_effect_sink_defers_effects() {
        let mut sink = DeferredTuiRuntimeEffectSink;

        let outcome = sink.handle_runtime_effect(&TuiRuntimeEffect::Quit);

        assert!(matches!(outcome, Ok(TuiRuntimeEffectSinkOutcome::Deferred)));
    }

    /// A minimal command envelope for building command-bearing runtime effects.
    fn sample_command() -> CommandEnvelope {
        CommandEnvelope::new(
            "cmd_sample".to_owned(),
            CommandType::FactoryDrainRequested,
            "fleet:livespec".to_owned(),
            "fleet:livespec:factory.drain_requested".to_owned(),
            "operator".to_owned(),
        )
    }

    #[test]
    fn deferred_runtime_effect_sink_keeps_its_startup_snapshot() {
        // The legacy no-store path has no live source: a refresh returns None so
        // the loop keeps its startup snapshot rather than re-projecting.
        let mut sink = DeferredTuiRuntimeEffectSink;

        let refreshed = sink.refresh_events(true);

        assert!(matches!(refreshed, Ok(None)));
    }

    #[test]
    fn effect_triggers_source_poll_only_for_ledger_mutating_effects() {
        // A command-bearing effect (the operator's approve / move / policy write)
        // triggers an immediate source re-poll; navigation and quit do not.
        assert!(effect_triggers_source_poll(
            &TuiRuntimeEffect::PersistCommand(sample_command())
        ));
        assert!(effect_triggers_source_poll(
            &TuiRuntimeEffect::PersistCommandWithPayload {
                command: sample_command(),
                payload_json: "{}".to_owned(),
            }
        ));
        assert!(!effect_triggers_source_poll(&TuiRuntimeEffect::Render));
        assert!(!effect_triggers_source_poll(&TuiRuntimeEffect::Quit));
    }

    #[test]
    fn keymap_maps_views_nav_focus_navigation_and_dive_in() {
        // Default focus is the Views nav: up/down walk the vertical Views menu,
        // Enter and Right dive focus into the Content pane, Left enters the
        // permanent menu bar, and Esc is the inert close-overlay no-op.
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
            Some(TuiTerminalInput::Interaction(TuiInteraction::OpenMenu))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Esc), &model),
            Some(TuiTerminalInput::Interaction(TuiInteraction::CloseOverlay))
        );
    }

    #[test]
    fn keymap_maps_content_focus_navigation_and_modal_opening() {
        // In the Content pane: up/down move the content selection, Enter opens
        // the selected attention item's record, Right steps focus into the
        // Detail pane, and Left/Esc step focus back to the Views nav.
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
                TuiInteraction::OpenWorkItemDetail
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Right), &model),
            Some(TuiTerminalInput::Interaction(TuiInteraction::FocusDetail))
        );
        for view in [TuiView::Spec, TuiView::Events, TuiView::Repos] {
            let model = build_tui_model_for_state(
                &demo_events(),
                &TuiInteractionState::for_view(view, 0, TuiOverlay::None)
                    .with_focus(FocusPane::Content),
            );
            assert_eq!(
                key_event_to_terminal_input(key(KeyCode::Enter), &model),
                None
            );
        }
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

    #[test]
    fn keymap_maps_header_pane_focus_scroll_leave_and_inert_keys() {
        // On the focused top/header pane: left/right scroll it horizontally,
        // up/down are inert, Enter is inert, and Esc leaves the header (returning
        // to the Views nav). Tab is the ring cycle, tested separately.
        let model = attention_model_in(TuiOverlay::None, FocusPane::Header);
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Left), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::ScrollHeaderLeft
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Right), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::ScrollHeaderRight
            ))
        );
        assert_eq!(key_event_to_terminal_input(key(KeyCode::Up), &model), None);
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Down), &model),
            None
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &model),
            None
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Esc), &model),
            Some(TuiTerminalInput::Interaction(TuiInteraction::FocusNav))
        );
    }

    #[test]
    fn keymap_tab_cycles_the_focus_ring_and_is_inert_behind_an_overlay() {
        // Tab / BackTab drive the focus ring (which includes the header); behind
        // an open overlay they are inert (the overlay owns navigation).
        let model = attention_model(TuiOverlay::None);
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Tab), &model),
            Some(TuiTerminalInput::Interaction(TuiInteraction::FocusNextPane))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::BackTab), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::FocusPreviousPane
            ))
        );
        let overlaid = attention_model(TuiOverlay::Search {
            query: String::new(),
        });
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Tab), &overlaid),
            None
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::BackTab), &overlaid),
            None
        );
    }

    /// The header content row of a rendered frame (row 0 is the top border, row 1
    /// is the header's inner content), so header assertions never false-match on
    /// body text lower in the frame.
    fn header_row(frame: &str) -> String {
        frame.lines().nth(1).unwrap_or_default().to_owned()
    }

    #[test]
    fn render_header_focused_pans_the_full_line_and_blurred_shrinks_to_fit() {
        // Scenario 20: a FOCUSED header renders the full, un-degraded line panned
        // by its scroll offset and carries the `[focus]` title marker; a BLURRED
        // header keeps the shrink-to-fit default (dropping low-value fields on a
        // narrow viewport) with no marker.
        let base = TuiInteractionState::new(0, TuiOverlay::None)
            .with_selected_repo("e2e-top-pane".to_owned());

        // Focused, narrow, left edge: the left field shows, the right is clipped.
        let left = render_to_text(
            &build_tui_model_for_state(&demo_events(), &base.clone().with_focus(FocusPane::Header)),
            56,
            12,
        )
        .unwrap_or_default();
        assert!(left.contains("LiveSpec Console [focus]"), "{left}");
        let left_header = header_row(&left);
        assert!(left_header.contains("fleet: livespec"), "{left_header}");
        assert!(!left_header.contains("attention:"), "{left_header}");

        // Focused, narrow, scrolled far right (clamped to the measured max): the
        // previously-clipped right field is revealed, the left field panned off.
        let right = render_to_text(
            &build_tui_model_for_state(
                &demo_events(),
                &base
                    .clone()
                    .with_focus(FocusPane::Header)
                    .with_header_scroll(100),
            ),
            56,
            12,
        )
        .unwrap_or_default();
        let right_header = header_row(&right);
        assert!(right_header.contains("attention:"), "{right_header}");
        assert!(!right_header.contains("fleet: livespec"), "{right_header}");

        // Blurred, narrow: no `[focus]` marker; shrink-to-fit drops the fleet
        // field but keeps the repo field.
        let blurred = render_to_text(
            &build_tui_model_for_state(&demo_events(), &base.with_focus(FocusPane::Nav)),
            56,
            12,
        )
        .unwrap_or_default();
        // The header title carries no focus marker (the Views nav is focused
        // instead, so `[focus]` appears on ITS title, not the header's).
        assert!(!blurred.contains("LiveSpec Console [focus]"), "{blurred}");
        let blurred_header = header_row(&blurred);
        assert!(!blurred_header.contains("fleet: livespec"));
        assert!(blurred_header.contains("repo: e2e-top-pane"));
    }

    #[test]
    fn render_model_reports_the_header_scroll_extent_only_when_focused_and_clipped() {
        // The render measures the focused header's overflow and returns it so the
        // loop can feed it back: positive when a focused header overflows, zero
        // when it fits, zero when blurred, and zero for an empty area.
        let events = demo_events();
        let focused = TuiInteractionState::new(0, TuiOverlay::None)
            .with_selected_repo("e2e-top-pane".to_owned())
            .with_focus(FocusPane::Header);

        let narrow = Rect::new(0, 0, 56, 12);
        let mut narrow_buffer = Buffer::empty(narrow);
        let narrow_extents = render_model(
            &build_tui_model_for_state(&events, &focused),
            narrow,
            &mut narrow_buffer,
        );
        assert!(narrow_extents.header_max_scroll > 0);

        let wide = Rect::new(0, 0, 160, 12);
        let mut wide_buffer = Buffer::empty(wide);
        let wide_extents = render_model(
            &build_tui_model_for_state(&events, &focused),
            wide,
            &mut wide_buffer,
        );
        assert_eq!(wide_extents.header_max_scroll, 0);

        let blurred = focused.with_focus(FocusPane::Nav);
        let mut blurred_buffer = Buffer::empty(narrow);
        let blurred_extents = render_model(
            &build_tui_model_for_state(&events, &blurred),
            narrow,
            &mut blurred_buffer,
        );
        assert_eq!(blurred_extents.header_max_scroll, 0);

        // Empty area: the ZERO extents path (neither pane advances off a frame
        // that drew nothing).
        let empty_area = Rect::new(0, 0, 0, 0);
        let mut empty_buffer = Buffer::empty(Rect::new(0, 0, 1, 1));
        let zero = render_model(
            &build_tui_model_for_state(&events, &blurred),
            empty_area,
            &mut empty_buffer,
        );
        assert_eq!(zero.header_max_scroll, 0);
        assert_eq!(zero.detail_max_scroll, 0);
    }

    #[test]
    fn help_overlay_renders_the_header_pane_section() {
        // `?` on the focused header opens Help auto-focused to the header section,
        // which lists a "Header" menu row and renders the header pane's help body.
        let state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_overlay(TuiOverlay::Help {
                focus: HelpFocus::Menu,
                selected_section: header_help_section(),
                scroll: 0,
            })
            .with_focus(FocusPane::Header);
        let frame = render_to_text(&build_tui_model_for_state(&demo_events(), &state), 100, 24)
            .unwrap_or_default();
        assert!(frame.contains("Header"));
        assert!(frame.contains("scroll the focused header"));
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
    fn left_walks_focus_detail_to_content_to_nav_and_enters_the_menu_from_nav() {
        let events = demo_events();
        let detail = TuiInteractionState::new(0, TuiOverlay::None).with_focus(FocusPane::Detail);

        let content = press(&detail, &events, KeyCode::Left);
        assert_eq!(content.focus(), FocusPane::Content);

        let nav = press(&content, &events, KeyCode::Left);
        assert_eq!(nav.focus(), FocusPane::Nav);

        // Left at the resting left edge enters the visible menu bar, so the
        // primary menu surface is reachable without its registry hotkey.
        let opened = press(&nav, &events, KeyCode::Left);
        assert_eq!(
            opened.overlay(),
            &TuiOverlay::Menu {
                top: 0,
                selected: 0
            }
        );
        assert_eq!(opened.active_view(), TuiView::Attention);
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
            Some("fabro attach run".to_owned()),
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
            Some("fabro attach fabro-run-5137117035853731187".to_owned()),
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
    fn keymap_opens_help_and_closes_it_only_on_esc() {
        // `?` with no overlay open opens Help; while Help is open `?` is INERT
        // (Esc-only close -- no toggle); `?` typed into a text overlay is a
        // literal char; `?` behind the command modal is inert.
        let none = attention_model(TuiOverlay::None);
        let help = attention_model(TuiOverlay::Help {
            focus: HelpFocus::Menu,
            selected_section: 1,
            scroll: 0,
        });
        let help_text = attention_model(TuiOverlay::Help {
            focus: HelpFocus::Text,
            selected_section: 1,
            scroll: 0,
        });
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
        // Esc-only close: `?` while Help is open no longer dismisses it.
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('?')), &help),
            None
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('?')), &search),
            Some(TuiTerminalInput::Interaction(TuiInteraction::TypeChar('?')))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('?')), &modal),
            None
        );

        // Behind the Help overlay: left/right focus the Help panes, up/down act
        // on the focused Help pane, PgUp/PgDn page the right pane, and only Esc
        // closes the overlay.
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Up), &help),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::HelpSelectPreviousSection
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Down), &help),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::HelpSelectNextSection
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Up), &help_text),
            Some(TuiTerminalInput::Interaction(TuiInteraction::HelpScrollUp))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Down), &help_text),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::HelpScrollDown
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::PageDown), &help),
            Some(TuiTerminalInput::Interaction(TuiInteraction::HelpPageDown))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::PageUp), &help),
            Some(TuiTerminalInput::Interaction(TuiInteraction::HelpPageUp))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Left), &help),
            Some(TuiTerminalInput::Interaction(TuiInteraction::HelpFocusMenu))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Right), &help),
            Some(TuiTerminalInput::Interaction(TuiInteraction::HelpFocusText))
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
    fn render_to_text_draws_the_modal_help_overlay() {
        // The modal renders its title, the always-visible `esc to exit` footer,
        // the `Global actions` menu section, and one section per focusable pane.
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::Help {
                focus: HelpFocus::Menu,
                selected_section: help_section_for_view(TuiView::Attention),
                scroll: 0,
            },
        );
        let model = build_tui_model_for_state(&demo_events(), &state);

        let output = render_to_text(&model, 96, 24).unwrap_or_default();

        assert!(output.contains("Help"), "modal title missing");
        assert!(output.contains("esc to exit"), "esc-to-exit footer missing");
        assert!(output.contains("Global actions"), "Global actions missing");
        for pane in ["Attention", "Lanes", "Settings"] {
            assert!(output.contains(pane), "menu section {pane:?} missing");
        }
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
        // Enter inside a drilled-in lane opens the selected work-item's record.
        // It used to be inert here while the Status line still advertised
        // "enter drill" -- the hint lied and there was no way to read an item.
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &drilled),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::OpenWorkItemDetail
            ))
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
    fn runtime_step_rejects_command_palette_drain_as_a_parallel_encoding() {
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

        assert_eq!(
            step.effect(),
            &TuiRuntimeEffect::ApplicationError(
                console_application::ApplicationError::UnknownCommandPaletteAction
            )
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
    fn render_to_text_draws_the_menu_bar_without_opening_the_menu() {
        let model = build_tui_model(&demo_events(), 0);
        assert_eq!(model.overlay(), &TuiOverlay::None);

        let output = render_to_text(&model, 96, 24);

        let rendered = output.unwrap_or_default();
        for top in action_registry::menu_tree() {
            assert!(rendered.contains(top.label), "{}:\n{rendered}", top.label);
        }
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
        // The detail pane carries the operational latest-event row, not the
        // removed "canonical source" documentation sentence (B5).
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("Latest event")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("The event log is the canonical source")),
            Ok(false)
        );
    }

    #[test]
    fn render_to_text_spec_view_shows_counts_without_doc_prose() {
        let state = TuiInteractionState::for_view(TuiView::Spec, 0, TuiOverlay::None);
        let model = build_tui_model_for_state(&factory_events(), &state);

        let output = render_to_text(&model, 96, 24);

        // The Spec pane bodies render their operational counts...
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("LiveSpec next snapshots:")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("Revise required:")),
            Ok(true)
        );
        // ...with no baked-in documentation sentence anywhere in the panes (B5).
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("Spec lifecycle status is projected")),
            Ok(false)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("Revise-required events stay visible")),
            Ok(false)
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
        // Top rank-ordered items are previewed under their lane with titles;
        // the blocked item still carries its lane reason.
        assert_eq!(
            output.as_ref().map(|r| {
                r.contains("- console-ready-a [ready]  Fix the paging bug in the backlog lane")
            }),
            Ok(true)
        );
        assert_eq!(
            output.as_ref().map(|r| {
                r.contains("- console-blocked [blocked]  Unblock factory acceptance (needs-human)")
            }),
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
        // The drill-in keeps id/rank/status intact and adds the title before
        // the lower-priority repo field; the first item is the selected
        // per-item cursor, marked with `>`.
        assert_eq!(
            output.as_ref().map(|r| {
                r.contains(
                    "> console-ready-a  rank a0  [ready]  Fix the paging bug in the backlog lane",
                )
            }),
            Ok(true)
        );
        assert_eq!(
            output.as_ref().map(|r| {
                r.contains("console-ready-b  rank a1  [ready]  Wire the status valve")
            }),
            Ok(true)
        );
    }

    #[test]
    fn render_to_text_keeps_lane_item_identity_visible_at_narrow_width() {
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_lane_focus(LaneFocus::Lane(Lane::Ready));
        let model = build_tui_model_for_state(&lane_render_events(), &state);

        let output = render_to_text(&model, 64, 12);

        assert_eq!(
            output
                .as_ref()
                .map(|r| r.contains("> console-ready-a  rank a0  [ready]")),
            Ok(true)
        );
        assert_eq!(output.as_ref().map(|r| r.contains("Fix")), Ok(true));
        assert_eq!(
            output
                .as_ref()
                .map(|r| { r.contains("Fix the paging bug in the backlog lane") }),
            Ok(false)
        );
        assert_eq!(
            output.as_ref().map(|r| r.contains("repo console")),
            Ok(false)
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
            Some("fabro attach run".to_owned()),
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
    fn detail_lines_omit_attach_when_absent() {
        let detail = AttentionDetail::new(
            "repo".to_owned(),
            "work-item".to_owned(),
            "-".to_owned(),
            None,
            vec![],
            vec![],
        );

        let rendered = detail_lines(&detail)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("Fabro run: -"));
        assert!(!rendered.contains("Attach:"));
    }

    #[test]
    fn detail_lines_include_attach_actions_when_present() {
        let detail = AttentionDetail::new(
            "repo".to_owned(),
            "work-item".to_owned(),
            "run".to_owned(),
            Some("fabro attach run".to_owned()),
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
            Some("work-item".to_owned()),
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
    fn render_to_text_suppresses_command_modal_overlay_without_actions() {
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
                .map(|rendered| !rendered.contains("Command Modal")),
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

    #[test]
    fn render_to_text_suppresses_empty_command_modal_overlay() {
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::CommandModal {
                selected_action_index: 0,
            },
        );
        let model = build_tui_model_for_state(&lane_render_events(), &state);
        assert_eq!(
            model.detail().map(AttentionDetail::actions),
            Some([].as_slice())
        );

        let output = render_to_text(&model, 96, 24);

        assert_eq!(
            output
                .as_ref()
                .map(|rendered| !rendered.contains("Command Modal")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| !rendered.contains("enter run")),
            Ok(true)
        );
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    /// The key event for a registered chord, Control included when it carries
    /// one. Chords are why `ctrl-c` is expressible at all.
    fn chord_event(chord: action_registry::KeyChord) -> KeyEvent {
        let modifiers = if chord.ctrl {
            KeyModifiers::CONTROL
        } else {
            KeyModifiers::empty()
        };
        KeyEvent::new(KeyCode::Char(chord.key), modifiers)
    }

    fn persisted_command(effect: &TuiRuntimeEffect) -> Option<&console_domain::CommandEnvelope> {
        match effect {
            TuiRuntimeEffect::PersistCommand(command)
            | TuiRuntimeEffect::PersistCommandWithPayload { command, .. } => Some(command),
            TuiRuntimeEffect::Render
            | TuiRuntimeEffect::OpenAttachCommand(_)
            | TuiRuntimeEffect::CopyAttachCommand(_)
            | TuiRuntimeEffect::CopyDriverHandoff(_)
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
            | TuiRuntimeEffect::CopyDriverHandoff(_)
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

    #[test]
    fn every_registry_hotkey_stages_its_action_where_offered_and_is_inert_elsewhere() {
        // no-orphan-hotkeys, keymap half: the keymap dispatches EVERY
        // registered hotkey to exactly the action the registry stages for the
        // selection, and the key is inert where the registry offers nothing —
        // key behavior is a pure function of the registry, with no
        // hand-written arm left to drift.
        let lanes = [
            (Lane::Backlog, "backlog"),
            (Lane::PendingApproval, "pending-approval"),
            (Lane::Acceptance, "acceptance"),
            (Lane::Done, "done"),
        ];
        for (lane, label) in lanes {
            let events = [lane_event(
                "evt_no_orphan",
                "console-no-orphan",
                lane,
                None,
                "a0",
                label,
            )];
            let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
                .with_lane_focus(LaneFocus::Lane(lane))
                .with_selected_lane_item_index(0);
            let model = build_tui_model_for_state(&events, &state);
            let ctx = model.selected_action_context();
            assert!(ctx.is_some(), "{label}: no action context");
            for (spec, chord) in ctx.iter().flat_map(|_ctx| {
                action_registry::ACTION_REGISTRY
                    .iter()
                    .flat_map(move |spec| spec.hotkeys.iter().map(move |chord| (spec, *chord)))
            }) {
                let input = key_event_to_terminal_input(chord_event(chord), &model);
                let staged = registry_action_input(&model, spec, chord.key);
                assert_eq!(input, staged, "{label}/{}", spec.id);
            }
        }
    }

    #[test]
    fn the_palette_actions_command_opens_the_invoker_and_enter_stages_the_selection() {
        // Confirming the palette's `actions` query opens the invoker roster.
        let palette = TuiInteractionState::new(
            0,
            TuiOverlay::CommandPalette {
                query: "actions".to_owned(),
            },
        );
        let opened = step_tui_runtime(
            &palette,
            &pending_events(),
            TuiTerminalInput::Confirm,
            "operator",
        );
        assert_eq!(
            opened.state().overlay(),
            &TuiOverlay::ActionInvoker { selected_action: 0 }
        );

        // Enter on an AVAILABLE row stages its normal valve confirm flow:
        // walk the selection to the approve row over the pending item.
        let approve_index = action_registry::ACTION_REGISTRY
            .iter()
            .position(|spec| spec.id == "approve")
            .unwrap_or_default();
        let mut state = opened.state().clone();
        for _step in 0..approve_index {
            state =
                reduce_tui_interaction(&state, &pending_events(), TuiInteraction::SelectNextAction);
        }
        let staged = step_tui_runtime(
            &state,
            &pending_events(),
            TuiTerminalInput::Confirm,
            "operator",
        );
        assert_eq!(
            staged.state().overlay(),
            &TuiOverlay::ValveConfirm {
                valve: PendingValve::Approve
            }
        );

        // Enter on an UNAVAILABLE row is inert: accept is invalid on a
        // pending-approval item, so the roster stays open and nothing stages.
        let accept_index = action_registry::ACTION_REGISTRY
            .iter()
            .position(|spec| spec.id == "accept")
            .unwrap_or_default();
        let parked = TuiInteractionState::new(
            0,
            TuiOverlay::ActionInvoker {
                selected_action: accept_index,
            },
        );
        let inert = step_tui_runtime(
            &parked,
            &pending_events(),
            TuiTerminalInput::Confirm,
            "operator",
        );
        assert_eq!(
            inert.state().overlay(),
            &TuiOverlay::ActionInvoker {
                selected_action: accept_index,
            }
        );
    }

    #[test]
    fn the_invoker_reaches_the_hotkeyless_scope_override_on_a_refused_ready_item() {
        // The whole point of the hotkey-less entry: on a drilled-in ready item
        // the orchestrator says is awaiting a scope override, the invoker
        // stages the workflow-scope override no key can reach.
        //
        // THE DEFECT THIS CATCHES on real producer output: the availability is
        // read from the PUBLISHED signal, not inferred from `factory_safety`.
        // An item marked factory-unsafe is refused by the dispatcher's first
        // arm, before the override label is consulted, so inferring it there
        // would stage an action that cannot clear the refusal. The sibling
        // assertion in `action_registry` pins the negative half.
        let events = [scope_override_pending_event("console-refused")];
        let scope_index = action_registry::ACTION_REGISTRY
            .iter()
            .position(|spec| spec.id == "set-workflow-scope-override")
            .unwrap_or_default();
        let state = TuiInteractionState::for_view(
            TuiView::Lanes,
            0,
            TuiOverlay::ActionInvoker {
                selected_action: scope_index,
            },
        )
        .with_lane_focus(LaneFocus::Lane(Lane::Ready))
        .with_selected_lane_item_index(0);
        let staged = step_tui_runtime(&state, &events, TuiTerminalInput::Confirm, "operator");
        assert_eq!(
            staged.state().overlay(),
            &TuiOverlay::ValveConfirm {
                valve: PendingValve::SetWorkflowScopeOverride
            }
        );
    }

    fn invoker_state_for(action_id: &str) -> TuiInteractionState {
        let index = action_registry::ACTION_REGISTRY
            .iter()
            .position(|spec| spec.id == action_id)
            .unwrap_or_default();
        TuiInteractionState::for_view(
            TuiView::Lanes,
            0,
            TuiOverlay::ActionInvoker {
                selected_action: index,
            },
        )
    }

    /// Render the menu overlay at `top` and return the screen text.
    fn menu_screen(top: usize) -> String {
        let state =
            TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::Menu { top, selected: 0 });
        let model = build_tui_model_for_state(&[], &state);
        // Empty on a render error rather than panicking: the caller's assertions
        // then fail naming the missing label, which is the useful message.
        render_to_text(&model, 120, 40).unwrap_or_default()
    }

    #[test]
    fn every_registered_action_is_reachable_by_walking_the_rendered_menu() {
        // THE MILESTONE PROPERTY, quantified over the RENDERED surface and
        // derived generically from the registry. A hand-listed expectation
        // would be the same second-encoding defect the generated menu exists to
        // retire, and would go stale the moment an entry is added.
        //
        // Rendering is what makes this stronger than the registry-level tree
        // test: a taxonomy that walks correctly but never reaches a pane would
        // satisfy that one and still leave the operator with no menu.
        let tree = action_registry::menu_tree();
        for (top_index, top) in tree.iter().enumerate() {
            let screen = menu_screen(top_index);
            for group in &top.groups {
                for spec in &group.actions {
                    // Bound as locals so the assert fits on ONE line: rustfmt
                    // would otherwise put the failure-only message on a line
                    // llvm-cov counts as never executed -- the same pincer
                    // `action_registry.rs` documents.
                    let label = spec.label;
                    let node = top.label;
                    assert!(screen.contains(label), "{node}/{label}:\n{screen}");
                }
            }
        }
        // And nothing is reachable ONLY by walking: the rendered set is the
        // registry set, so the menu cannot quietly omit an action.
        let rendered: usize = tree
            .iter()
            .flat_map(|top| top.groups.iter())
            .map(|group| group.actions.len())
            .sum();
        assert_eq!(rendered, action_registry::ACTION_REGISTRY.len());
    }

    #[test]
    fn the_rendered_menu_bar_shows_every_top_level_node_at_once() {
        // The bar is the navigation surface, so every node must be visible
        // without first entering one: a bar you must already be inside to see
        // does not navigate.
        let screen = menu_screen(0);
        let tree = action_registry::menu_tree();
        for top in &tree {
            assert!(screen.contains(top.label), "{}:\n{screen}", top.label);
        }
        // Same one-line binding for the same llvm-cov reason.
        let nodes = tree.len();
        assert!(nodes >= 2, "the bar is a single degenerate node");
    }

    #[test]
    fn a_menu_item_stages_exactly_what_its_hotkey_stages() {
        // The menu is a ROUTE to the registry, not a second invocation path.
        // If these ever diverge, the menu has become a parallel encoding of
        // invocation -- the defect this plan exists to retire.
        let events = [lane_event(
            "evt_menu_parity",
            "console-menu-parity",
            Lane::PendingApproval,
            None,
            "a0",
            "pending-approval",
        )];
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_lane_focus(LaneFocus::Lane(Lane::PendingApproval))
            .with_selected_lane_item_index(0);
        let model = build_tui_model_for_state(&events, &state);
        for (top_index, _) in action_registry::menu_tree().iter().enumerate() {
            for (action_index, spec) in action_registry::menu_actions(top_index).iter().enumerate()
            {
                let via_menu =
                    menu_confirm_step(&state, &events, &model, top_index, action_index, "operator");
                let via_registry = registry_action_input(&model, spec, ' ');
                // A menu row that cannot stage its action must refuse
                // explicitly; an unchanged Render is the old silent no-op.
                let menu_unavailable = matches!(
                    via_menu.effect(),
                    TuiRuntimeEffect::ApplicationError(
                        console_application::ApplicationError::UnavailableOperatorAction
                    )
                );
                assert_eq!(menu_unavailable, via_registry.is_none(), "{}", spec.id);
            }
        }
    }

    /// The menu coordinates — bar node, then flattened action index — of the
    /// registered action `action_id`.
    fn menu_position_for(action_id: &str) -> Option<(usize, usize)> {
        action_registry::menu_tree()
            .iter()
            .enumerate()
            .find_map(|(top_index, _)| {
                action_registry::menu_actions(top_index)
                    .iter()
                    .position(|spec| spec.id == action_id)
                    .map(|action_index| (top_index, action_index))
            })
    }

    #[test]
    fn the_menu_stages_the_driver_handoff_from_its_row() {
        // Enter on the menu's driver-handoff row opens the handoff overlay,
        // exactly as the `h` key and the invoker row do.
        //
        // This drives the FULL confirm path — step_tui_runtime ->
        // confirm_operator_action -> the menu branch — rather than calling
        // menu_confirm_step directly, so the menu's claim to be a ROUTE to the
        // registry is tested where an operator's Enter actually lands. The
        // parity test above calls the step function directly and therefore
        // never proves the menu overlay is dispatched to it at all.
        let events = [driver_handoff_event(
            "console-groomable",
            Lane::Backlog,
            None,
        )];
        let position = menu_position_for("driver-handoff");
        assert!(position.is_some(), "missing menu action driver-handoff");
        let (top, selected) = position.unwrap_or_default();
        let state =
            TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::Menu { top, selected })
                .with_lane_focus(LaneFocus::Lane(Lane::Backlog))
                .with_selected_lane_item_index(0);

        let staged = step_tui_runtime(&state, &events, TuiTerminalInput::Confirm, "operator");

        // The match is bound so the assert fits on ONE line, the same llvm-cov
        // pincer the invoker's twin test above records.
        let overlay = staged.state().overlay();
        let handed_off = matches!(overlay, TuiOverlay::DriverHandoff { .. });
        assert!(handed_off, "{overlay:?}");
    }

    #[test]
    fn left_and_right_walk_the_menu_bar_rather_than_the_panes() {
        // Behind an open menu the arrow keys belong to the BAR. Walking the
        // panes underneath instead would move a focus the operator cannot see,
        // and would leave the bar — the primary navigation surface — with no
        // way to traverse it.
        let state = TuiInteractionState::for_view(
            TuiView::Lanes,
            0,
            TuiOverlay::Menu {
                top: 0,
                selected: 0,
            },
        );
        let model = build_tui_model_for_state(&[], &state);

        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Left), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::MenuPreviousTop
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Right), &model),
            Some(TuiTerminalInput::Interaction(TuiInteraction::MenuNextTop))
        );
    }

    #[test]
    fn the_menu_renders_its_bar_alone_when_no_room_remains_for_a_submenu() {
        // A one-row area still gets the BAR. The bar is the navigation surface,
        // so dropping it in a short terminal would leave the operator with an
        // open menu showing nothing at all — worse than no menu, because the
        // overlay still swallows the keys.
        let area = Rect::new(0, 0, 120, 1);
        let mut buffer = Buffer::empty(area);

        let model = build_tui_model_for_state(&[], &TuiInteractionState::new(0, TuiOverlay::None));

        render_menu_overlay(&model, 0, 0, area, &mut buffer);

        let screen = buffer_to_text(&buffer, area);
        let label = action_registry::menu_tree()
            .first()
            .map_or("", |node| node.label)
            .to_owned();
        assert!(screen.contains(&label), "{screen}");
        assert_eq!(screen.lines().count(), 1, "{screen}");
    }

    #[test]
    fn every_action_is_reachable_by_menu_navigation_with_no_hotkeys() {
        // R4, THE MILESTONE PROPERTY: hotkeys are ADDITIONAL. Disable every
        // registry hotkey and every action must STILL be reachable, by entering
        // the CLOSED rendered menu bar and then walking it with navigation keys
        // alone.
        //
        // Walked through the REAL key layer -- `press` -> key_event_to_terminal_
        // input -> step_tui_runtime -- rather than by indexing the registry. That
        // is the whole point: indexing menu_actions would prove the taxonomy
        // contains everything, which the tree test already says, and would prove
        // nothing about whether an operator pressing arrow keys can GET there.
        //
        // Left from the resting left edge enters the closed bar. Right then walks
        // the bar, and Down walks the actions. Rights come FIRST because a bar
        // move resets the selection, so interleaving them would silently land
        // somewhere else.
        let events: [ConsoleEvent; 0] = [];
        let tree = action_registry::menu_tree();
        let mut reached: std::collections::BTreeSet<&'static str> =
            std::collections::BTreeSet::new();
        let closed = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None);
        let closed_screen = render_to_text(&build_tui_model_for_state(&events, &closed), 120, 40)
            .unwrap_or_default();
        for top in &tree {
            let visible = closed_screen.contains(top.label);
            assert!(visible, "{}:\n{closed_screen}", top.label);
        }

        for top_index in 0..tree.len() {
            for action_index in 0..action_registry::menu_actions(top_index).len() {
                let mut state = press(&closed, &events, KeyCode::Left);
                assert_eq!(
                    state.overlay(),
                    &TuiOverlay::Menu {
                        top: 0,
                        selected: 0
                    }
                );
                for _ in 0..top_index {
                    state = press(&state, &events, KeyCode::Right);
                }
                for _ in 0..action_index {
                    state = press(&state, &events, KeyCode::Down);
                }
                // The walk must LAND where it aimed. Without this the set
                // comparison below would pass on a menu whose navigation never
                // moved at all.
                let landed = TuiOverlay::Menu {
                    top: top_index,
                    selected: action_index,
                };
                assert_eq!(state.overlay(), &landed);
                reached.insert(action_registry::menu_actions(top_index)[action_index].id);
            }
        }

        let registered: std::collections::BTreeSet<&'static str> = action_registry::ACTION_REGISTRY
            .iter()
            .map(|spec| spec.id)
            .collect();
        assert_eq!(reached, registered);
    }

    #[test]
    fn the_menu_navigation_keys_are_not_themselves_registry_hotkeys() {
        // Without this the R4 claim is CIRCULAR: "every action is reachable by
        // navigation once hotkeys are disabled" is worthless if a navigation key
        // is itself a hotkey, because disabling hotkeys would disable the
        // navigation the claim depends on.
        //
        // KeyChord carries a `char` plus Control, so an arrow or Enter cannot be
        // expressed as a chord by construction. Asserted against the RESOLVER
        // rather than against that type argument, because the type could widen
        // (F10/Alt is an open question on this plan) and silently make an arrow
        // claimable.
        let state = TuiInteractionState::for_view(
            TuiView::Lanes,
            0,
            TuiOverlay::Menu {
                top: 0,
                selected: 0,
            },
        );
        let events: [ConsoleEvent; 0] = [];
        let model = build_tui_model_for_state(&events, &state);

        for code in [
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Enter,
        ] {
            let resolved = key_event_to_terminal_input(key(code), &model);
            let navigates = matches!(
                resolved,
                Some(
                    TuiTerminalInput::Interaction(
                        TuiInteraction::MenuNextTop
                            | TuiInteraction::MenuPreviousTop
                            | TuiInteraction::SelectNextAction
                            | TuiInteraction::SelectPreviousAction
                    ) | TuiTerminalInput::Confirm
                )
            );
            assert!(navigates, "{code:?} resolved to {resolved:?}");
        }
    }

    #[test]
    fn menu_reachability_is_not_a_claim_that_every_action_applies() {
        // THE LIMIT OF R4 MOST LIKELY TO BE MISREAD. REACHABLE and APPLICABLE are
        // different properties: this slice proves every action can be NAVIGATED
        // TO, and nothing whatever about whether invoking it does anything.
        //
        // It matters because the menu, unlike the ActionInvoker, does NOT mark
        // unavailable rows and swallows Enter on them silently
        // (render_menu_overlay consults availability nowhere; menu_confirm_step
        // returns an unchanged state with a Render effect when staging yields
        // None). So a green R4 must never be cited as evidence that the menu
        // works as a primary surface. That gap is tracked as its own defect.
        //
        // Asserted as an INEQUALITY rather than by pinning today's behaviour, so
        // it survives the fix: once unavailable rows are marked, the two counts
        // are still different and this still passes.
        let events = [lane_event(
            "evt_menu_applies",
            "console-menu-applies",
            Lane::Backlog,
            None,
            "a0",
            "backlog",
        )];
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_lane_focus(LaneFocus::Lane(Lane::Backlog))
            .with_selected_lane_item_index(0);
        let model = build_tui_model_for_state(&events, &state);

        let reachable: usize = action_registry::menu_tree()
            .iter()
            .flat_map(|top| top.groups.iter())
            .map(|group| group.actions.len())
            .sum();
        let applicable = action_registry::ACTION_REGISTRY
            .iter()
            .filter(|spec| registry_action_input(&model, spec, ' ').is_some())
            .count();

        // Everything is reachable -- that is R4.
        assert_eq!(reachable, action_registry::ACTION_REGISTRY.len());
        // But not everything applies to this selection, and some do.
        assert!(applicable < reachable, "{applicable} of {reachable}");
        assert!(applicable > 0, "{applicable}");
    }

    #[test]
    fn the_menu_opener_registry_key_is_additional_to_closed_bar_entry() {
        // THE OLD LIMIT OF R4, re-pointed: the registry still has exactly one
        // menu-opener chord, but it is now a shortcut TO the primary route, not
        // the only way into it. With every registry hotkey disabled, Left from
        // the resting left edge still opens the rendered bar.
        let openers: Vec<&'static action_registry::ActionSpec> = action_registry::ACTION_REGISTRY
            .iter()
            .filter(|spec| {
                matches!(
                    spec.staging,
                    action_registry::ActionStaging::Global(action_registry::GlobalAction::OpenMenu)
                )
            })
            .collect();

        // Exactly one: two openers would be two entry points into the bar with
        // different starting nodes, and the operator could not predict either.
        let count = openers.len();
        assert_eq!(count, 1);
        let opener = openers[0];
        let keyed = !opener.hotkeys.is_empty();
        assert!(keyed, "{}", opener.id);
        // And it carries a menu path like everything else, so it appears in the
        // menu it opens rather than being a privileged hidden key.
        let pathed = !opener.menu_path.is_empty();
        assert!(pathed, "{}", opener.id);

        let events: [ConsoleEvent; 0] = [];
        let closed = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None);
        let entered_state = press(&closed, &events, KeyCode::Left);
        assert_eq!(
            entered_state.overlay(),
            &TuiOverlay::Menu {
                top: 0,
                selected: 0
            }
        );
    }

    #[test]
    fn the_invoker_reaches_a_global_action_with_nothing_selected() {
        // A global action needs NO work-item, and the invoker must reach it on
        // an EMPTY event set — no lane, no selection, no action context. The
        // roster's whole claim is that every registered action is reachable;
        // demanding a selection would quietly except the four globals from it.
        let staged = step_tui_runtime(
            &invoker_state_for("open-search"),
            &[],
            TuiTerminalInput::Confirm,
            "operator",
        );
        // Bound as a local so the assert fits on ONE line: a failure-only
        // message on its own line is a line llvm-cov counts as never executed,
        // the same pincer `action_registry.rs` documents.
        let overlay = staged.state().overlay();
        assert!(matches!(overlay, TuiOverlay::Search { .. }), "{overlay:?}");
    }

    #[test]
    fn the_invoker_dispatches_ready_work_from_the_dispatch_row() {
        let dispatch_index = action_registry::ACTION_REGISTRY
            .iter()
            .position(|spec| spec.id == "dispatch-ready");
        assert!(dispatch_index.is_some(), "missing dispatch-ready action");
        let state = TuiInteractionState::for_view(
            TuiView::Lanes,
            0,
            TuiOverlay::ActionInvoker {
                selected_action: dispatch_index.unwrap_or_default(),
            },
        );
        let ready_events = [lane_event(
            "evt_invoker_dispatch",
            "console-invoker-dispatch",
            Lane::Ready,
            None,
            "a0",
            "ready",
        )];

        let staged = step_tui_runtime(&state, &ready_events, TuiTerminalInput::Confirm, "operator");
        let command = persisted_command(staged.effect());

        assert_eq!(
            command.map(console_domain::CommandEnvelope::command_type),
            Some(&CommandType::FactoryDrainRequested)
        );
    }

    #[test]
    fn dispatch_ready_has_no_character_key_path() {
        let spec = action_registry::ACTION_REGISTRY
            .iter()
            .find(|spec| spec.id == "dispatch-ready");
        assert!(spec.is_some(), "missing dispatch-ready action");
        let ready_events = [lane_event(
            "evt_no_dispatch_key",
            "console-no-dispatch-key",
            Lane::Ready,
            None,
            "a0",
            "ready",
        )];
        let state = TuiInteractionState::new(0, TuiOverlay::None);
        let model = build_tui_model_for_state(&ready_events, &state);

        assert_eq!(
            spec.and_then(|spec| registry_action_input(&model, spec, 'd')),
            None
        );
    }

    #[test]
    fn the_invoker_quits_from_the_quit_row() {
        // Quit is the ONE global that is not an interaction: it ends the
        // session rather than transforming its state, so the invoker returns
        // the Quit EFFECT rather than an overlay transition.
        let staged = step_tui_runtime(
            &invoker_state_for("quit"),
            &[],
            TuiTerminalInput::Confirm,
            "operator",
        );
        assert_eq!(staged.effect(), &TuiRuntimeEffect::Quit);
    }

    #[test]
    fn a_control_chord_on_a_non_character_key_is_inert() {
        // `control_chord_input` resolves Control-held CHARACTER chords through
        // the registry. Ctrl with a non-character key reaches no chord at all
        // and must fall through to the normal arm rather than being swallowed.
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None);
        let model = build_tui_model_for_state(&[], &state);
        let control_up = KeyEvent::new(KeyCode::Up, KeyModifiers::CONTROL);
        // ctrl-Up must behave exactly as Up: no chord is bound to it. Stated
        // here rather than as an assert message, which llvm-cov counts as an
        // unexecuted line.
        let plain_up = key_event_to_terminal_input(key(KeyCode::Up), &model);
        assert_eq!(key_event_to_terminal_input(control_up, &model), plain_up);
        // And a Control chord that IS a character but is bound to no action is
        // likewise inert rather than firing the plain-key action.
        let control_z = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL);
        assert_eq!(key_event_to_terminal_input(control_z, &model), None);
    }

    #[test]
    fn the_invoker_stages_the_driver_handoff_from_its_row() {
        // Enter on the driver-handoff row of a drilled-in backlog item opens
        // the handoff overlay, exactly as the `h` key would.
        let events = [driver_handoff_event(
            "console-groomable",
            Lane::Backlog,
            None,
        )];
        let handoff_index = action_registry::ACTION_REGISTRY
            .iter()
            .position(|spec| spec.id == "driver-handoff")
            .unwrap_or_default();
        let state = TuiInteractionState::for_view(
            TuiView::Lanes,
            0,
            TuiOverlay::ActionInvoker {
                selected_action: handoff_index,
            },
        )
        .with_lane_focus(LaneFocus::Lane(Lane::Backlog))
        .with_selected_lane_item_index(0);
        let staged = step_tui_runtime(&state, &events, TuiTerminalInput::Confirm, "operator");
        assert!(matches!(
            staged.state().overlay(),
            TuiOverlay::DriverHandoff { .. }
        ));
    }

    #[test]
    fn the_record_modal_renders_the_latest_refusal_for_its_item() {
        // The refusal payload a failed action carried rides the failure event
        // into the model and renders on the item's reading surface.
        // A raw (non-JSON) refusal exercises the fallback display; the
        // structured domain_error/summary parse is pinned by the application
        // display tests.
        let refusal_payload = concat!(
            r#"{"action_id":"approve:console-pending","#,
            r#""refusal":"dispatcher-staleness-refused: executing build predates latest release"}"#,
        );
        // The failure event's STREAM is the work-item aggregate, exactly as
        // the command handler emits it — the projection keys on it.
        let failed = ConsoleEvent::new(
            "evt_refused".to_owned(),
            1,
            "work_item".to_owned(),
            EventType::WorkItemActionFailed,
            "console:work-item-command-handler".to_owned(),
            "console-pending".to_owned(),
            9,
        )
        .with_payload_json(refusal_payload.to_owned());
        let mut events: Vec<ConsoleEvent> = pending_events().into_iter().collect();
        events.push(failed);
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::WorkItemDetail {
                work_item_id: "console-pending".to_owned(),
                // The failure line is the record's LAST row; the renderer
                // clamps this to the true bottom.
                scroll: 10_000,
            },
        );
        let model = build_tui_model_for_state(&events, &state);
        assert!(model.action_failure_for("console-pending").is_some());
        let rendered = render_to_text(&model, 110, 40).unwrap_or_default();
        assert!(rendered.contains("Last action:"), "{rendered}");
        assert!(rendered.contains("dispatcher-staleness-refused"));
    }

    #[test]
    fn the_record_modal_renders_dispatcher_journalled_refusals() {
        let entries: Vec<DispatcherJournalEntry> = DispatcherJournalEntry::new(
            "console",
            "console-pending",
            "dispatch-journal-refusal",
            DispatcherJournalKind::HostOnlyRefused,
            9,
        )
        .ok()
        .into_iter()
        .collect();
        assert_eq!(entries.len(), 1);
        let entry = entries[0]
            .clone()
            .with_diagnostic("factory-safety refusal requires host-only execution");
        let failed = ConsoleEvent::new(
            "evt_dispatch_refused".to_owned(),
            1,
            "factory".to_owned(),
            EventType::DispatcherRefusalObserved,
            "dispatcher".to_owned(),
            "repo:console".to_owned(),
            9,
        )
        .with_payload_json(dispatcher_journal_payload_json(&entry));
        let mut events: Vec<ConsoleEvent> = pending_events().into_iter().collect();
        events.push(failed);
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::WorkItemDetail {
                work_item_id: "console-pending".to_owned(),
                scroll: 10_000,
            },
        );
        let model = build_tui_model_for_state(&events, &state);

        assert!(model.action_failure_for("console-pending").is_some());
        let rendered = render_to_text(&model, 110, 40).unwrap_or_default();
        assert!(rendered.contains("Last action:"));
        assert!(rendered.contains("refused"));
        assert!(rendered.contains("host-only-refused"));
        assert!(rendered.contains("factory-safety refusal"));
        assert!(rendered.contains("requires host-only execution"));
    }

    #[test]
    fn render_action_invoker_lists_every_action_with_availability_markers() {
        let state = TuiInteractionState::new(0, TuiOverlay::ActionInvoker { selected_action: 0 });
        let model = build_tui_model_for_state(&pending_events(), &state);
        // Tall enough that the eleven-row roster fits inside the third-height
        // overlay box; the hotkey-less action is the LAST row.
        let rendered = render_to_text(&model, 110, 60).unwrap_or_default();
        assert!(rendered.contains("Actions"));
        assert!(rendered.contains("Approve work-item [p]"));
        // The hotkey-less action renders with its menu marker, unavailable on
        // a pending-approval selection.
        assert!(rendered.contains("Set workflow scope override [menu]  (unavailable here)"));
        // Accept cannot fire on a pending item: marked, not hidden.
        assert!(rendered.contains("Accept work-item [c]  (unavailable here)"));
        // The overlay owns the Status hints while open.
        assert!(model.footer().contains("enter stage"));
    }

    #[test]
    fn render_menu_overlay_marks_unavailable_actions_as_literal_text() {
        let position = menu_position_for("accept");
        assert!(position.is_some(), "missing menu action accept");
        let (top, selected) = position.unwrap_or_default();
        let state =
            TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::Menu { top, selected })
                .with_lane_focus(LaneFocus::Lane(Lane::PendingApproval))
                .with_selected_lane_item_index(0);
        let model = build_tui_model_for_state(&pending_events(), &state);
        let rendered = render_to_text(&model, 110, 60).unwrap_or_default();

        assert!(rendered.contains("Accept work-item [c]  (unavailable here)"));
    }

    #[test]
    fn rendered_menu_reaches_dispatch_and_marks_it_by_ready_work_availability() {
        let position = menu_position_for("dispatch-ready");
        assert!(position.is_some(), "missing menu action dispatch-ready");
        let (top, selected) = position.unwrap_or_default();
        let state =
            TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::Menu { top, selected });
        let empty_model = build_tui_model_for_state(&[], &state);
        let unavailable = render_to_text(&empty_model, 110, 60).unwrap_or_default();

        assert!(unavailable.contains("Dispatch ready work [menu]  (unavailable here)"));

        let ready_events = [lane_event(
            "evt_dispatch_ready",
            "console-dispatch-ready",
            Lane::Ready,
            None,
            "a0",
            "ready",
        )];
        let ready_model = build_tui_model_for_state(&ready_events, &state);
        let available = render_to_text(&ready_model, 110, 60).unwrap_or_default();

        assert!(available.contains("Dispatch ready work [menu]"));
        assert!(!available.contains("Dispatch ready work [menu]  (unavailable here)"));
    }

    #[test]
    fn the_menu_dispatches_ready_work_from_its_rendered_row() {
        let position = menu_position_for("dispatch-ready");
        assert!(position.is_some(), "missing menu action dispatch-ready");
        let (top, selected) = position.unwrap_or_default();
        let state =
            TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::Menu { top, selected });
        let ready_events = [lane_event(
            "evt_dispatch_ready_enter",
            "console-dispatch-ready-enter",
            Lane::Ready,
            None,
            "a0",
            "ready",
        )];

        let step = step_tui_runtime(&state, &ready_events, TuiTerminalInput::Confirm, "operator");
        let command = persisted_command(step.effect());

        assert_eq!(
            command.map(console_domain::CommandEnvelope::command_type),
            Some(&CommandType::FactoryDrainRequested)
        );
    }

    #[test]
    fn unavailable_menu_dispatch_refuses_instead_of_dispatching() {
        let position = menu_position_for("dispatch-ready");
        assert!(position.is_some(), "missing menu action dispatch-ready");
        let (top, selected) = position.unwrap_or_default();
        let state =
            TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::Menu { top, selected });
        let step = step_tui_runtime(&state, &[], TuiTerminalInput::Confirm, "operator");

        assert_eq!(
            step.effect(),
            &TuiRuntimeEffect::ApplicationError(
                console_application::ApplicationError::UnavailableOperatorAction
            )
        );
        assert_eq!(step.state(), &state);
    }

    #[test]
    fn unavailable_menu_confirmation_refuses_instead_of_silent_render() {
        let position = menu_position_for("accept");
        assert!(position.is_some(), "missing menu action accept");
        let (top, selected) = position.unwrap_or_default();
        let state =
            TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::Menu { top, selected })
                .with_lane_focus(LaneFocus::Lane(Lane::PendingApproval))
                .with_selected_lane_item_index(0);
        let step = step_tui_runtime(
            &state,
            &pending_events(),
            TuiTerminalInput::Confirm,
            "operator",
        );

        assert_eq!(
            step.effect(),
            &TuiRuntimeEffect::ApplicationError(
                console_application::ApplicationError::UnavailableOperatorAction
            )
        );
        assert_eq!(step.state(), &state);
    }

    /// A single manual-admission pending-approval item: the context that
    /// admits the approve valve and the cap-override dials.
    fn pending_events() -> [ConsoleEvent; 1] {
        [lane_event(
            "evt_demo_pending",
            "console-pending",
            Lane::PendingApproval,
            None,
            "a0",
            "pending-approval",
        )]
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

    fn attention_model_for_lane(lane: Lane, overlay: TuiOverlay) -> TuiScreenModel {
        let event = lane_event(
            "evt_attention_lane",
            "console-selected",
            lane,
            None,
            "a0",
            lane.label(),
        );
        build_tui_model_for_state(&[event], &TuiInteractionState::new(0, overlay))
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

    fn active_claim_execution_events() -> Option<Vec<ConsoleEvent>> {
        Some(vec![
            lane_event(
                "evt_claimed_a",
                "console-claimed-a",
                Lane::Active,
                None,
                "a1",
                "active",
            ),
            lane_event(
                "evt_claimed_b",
                "console-claimed-b",
                Lane::Active,
                None,
                "a2",
                "active",
            ),
            lane_event(
                "evt_executing",
                "console-executing",
                Lane::Active,
                None,
                "a3",
                "active",
            ),
            dispatcher_execution_event("evt_dispatch", "console-executing", "dispatch_1")?,
        ])
    }

    // Build a snapshot-observation event by writing the canonical `payload_json`
    // directly, mirroring the orchestrator emission the lane board rebuilds from.
    fn lane_event_title(work_item_id: &str) -> &str {
        match work_item_id {
            "console-ready-a" => "Fix the paging bug in the backlog lane",
            "console-ready-b" => "Wire the status valve",
            "console-blocked" => "Unblock factory acceptance",
            _ => "Routine lane fixture item",
        }
    }

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
        let title = lane_event_title(work_item_id);
        let payload = format!(
            r#"{{"repo":"console","work_item_id":"{work_item_id}","lane":"{}","lane_reason":{reason_json},"rank":"{rank}","status":"{status}","detail":{{"title":"{title}"}},"source_version":1}}"#,
            lane.label()
        );
        ConsoleEvent::fixture(
            event_id,
            EventType::WorkItemSnapshotObserved,
            "orchestrator",
        )
        .with_payload_json(payload)
    }

    fn dispatcher_execution_event(
        event_id: &str,
        work_item_id: &str,
        dispatch_id: &str,
    ) -> Option<ConsoleEvent> {
        let payload = dispatcher_journal_payload_json(
            &DispatcherJournalEntry::new(
                "console",
                work_item_id,
                dispatch_id,
                DispatcherJournalKind::BacklogBounce,
                2,
            )
            .ok()?,
        );
        Some(
            ConsoleEvent::fixture(
                event_id,
                EventType::DispatcherBacklogBounceObserved,
                "dispatcher",
            )
            .with_payload_json(payload),
        )
    }

    fn dispatcher_terminal_events(work_item_id: &str, dispatch_id: &str) -> Vec<ConsoleEvent> {
        let payload = format!(
            r#"{{"repo":"console","work_item_id":"{work_item_id}","dispatch_id":"{dispatch_id}","kind":"backlog-bounce","terminal_status":"completed","source_version":3}}"#
        );
        vec![
            ConsoleEvent::fixture(
                "evt_terminal",
                EventType::DispatcherBacklogBounceObserved,
                "dispatcher",
            )
            .with_payload_json(payload),
        ]
    }

    #[test]
    fn active_lane_render_distinguishes_claimed_from_executing() -> TuiRenderResult<()> {
        let events = active_claim_execution_events().ok_or(TuiRenderError::EmptyArea)?;
        let overview = build_tui_model_for_state(
            &events,
            &TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
                .with_selected_lane_index(3),
        );
        let overview_text = render_to_text(&overview, 120, 30)?;

        assert!(overview_text.contains("active (3); executing 1 claimed 2"));
        assert!(overview_text.contains("console-claimed-a [active] claimed"));
        assert!(overview_text.contains("console-executing [active] executing"));

        let drilled = build_tui_model_for_state(
            &events,
            &TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
                .with_lane_focus(LaneFocus::Lane(Lane::Active))
                .with_focus(FocusPane::Content),
        );
        let drilled_text = render_to_text(&drilled, 120, 30)?;

        assert!(drilled_text.contains("console-claimed-b  rank a2  [active] claimed"));
        assert!(drilled_text.contains("console-executing  rank a3  [active] executing"));
        Ok(())
    }

    #[test]
    fn active_lane_render_marks_terminal_signalled_item_finished() -> TuiRenderResult<()> {
        let mut events = vec![
            lane_event(
                "evt_finished",
                "console-finished",
                Lane::Active,
                None,
                "a1",
                "active",
            ),
            dispatcher_execution_event("evt_dispatch", "console-finished", "dispatch_done")
                .ok_or(TuiRenderError::EmptyArea)?,
        ];
        events.extend(dispatcher_terminal_events(
            "console-finished",
            "dispatch_done",
        ));
        let overview = build_tui_model_for_state(
            &events,
            &TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
                .with_selected_lane_index(3),
        );
        let overview_text = render_to_text(&overview, 120, 30)?;

        assert!(overview_text.contains("active (1); executing 0 claimed 0 finished? 1"));
        assert!(overview_text.contains("console-finished [active] finished?"));

        let detail = build_tui_model_for_state(
            &events,
            &TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
                .with_lane_focus(LaneFocus::Lane(Lane::Active))
                .with_focus(FocusPane::Content)
                .with_overlay(TuiOverlay::WorkItemDetail {
                    work_item_id: "console-finished".to_owned(),
                    scroll: 0,
                }),
        );
        let detail_text = render_to_text(&detail, 120, 30)?;

        assert!(detail_text.contains("execution_state      finished?"));
        Ok(())
    }

    // -----------------------------------------------------------------------
    // The work-item detail modal: the drill-in from a lane row to the FULL
    // standardized record. Before it existed the console could not show an
    // item's title or description anywhere, and the Status line advertised
    // "enter drill" in a drilled-in lane where Enter was inert.
    // -----------------------------------------------------------------------

    /// The work-item the modal fixtures pin: the first item of the Ready lane
    /// in `lane_render_events`, which is what `Enter` opens there.
    const MODAL_ITEM: &str = "console-ready-a";

    /// The long description used to prove the modal body scrolls: enough
    /// distinct lines to overflow the test viewport several times over.
    fn scrolling_description() -> String {
        (0..40)
            .map(|index| format!("body line {index}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// A drilled-in Ready lane whose selected item carries EVERY standardized
    /// record field, so a render assertion can prove each one reaches the
    /// screen rather than being silently dropped in the plumbing.
    fn fully_populated_item_events(description: &str) -> [ConsoleEvent; 1] {
        // Written as canonical `payload_json` text (this crate carries no JSON
        // dependency), mirroring the orchestrator emission the board rebuilds
        // from. `description` is escaped for the newlines the scroll test needs.
        let escaped = description.replace('\n', "\\n");
        let payload = format!(
            concat!(
                r#"{{"repo":"console","work_item_id":"console-full","#,
                r#""lane":"{}","lane_reason":null,"rank":"a3","status":"ready","#,
                r#""source_version":1,"detail":{{"#,
                r#""title":"Render the whole record","description":"{}","#,
                r#""item_type":"bug","origin":"freeform","gap_id":"gap-77","#,
                r#""assignee":"fabro","#,
                r#""depends_on":["console-dep-a","console-dep-b (cross-repo)"],"#,
                r#""captured_at":"2026-07-19T00:00:00Z","resolution":"completed","#,
                r#""reason":"landed via PR #123","audit":"{{\"commits\":[\"abc123\"]}}","#,
                r#""superseded_by":"console-newer","#,
                r#""spec_commitment_hint":"scenario-23-work-item-drill-in","#,
                r#""acceptance_criteria":"it renders","notes":"an operator note","#,
                r#""supersedes":"console-older","blocked_reason":"waiting on review","#,
                r#""factory_safety":"needs-host-secrets","admission_policy":"auto"}}}}"#,
            ),
            Lane::Ready.label(),
            escaped,
        );
        [ConsoleEvent::fixture(
            "evt_full",
            EventType::WorkItemSnapshotObserved,
            "orchestrator",
        )
        .with_payload_json(payload)]
    }

    fn fully_populated_item_model(description: &str) -> TuiScreenModel {
        let events = fully_populated_item_events(description);
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_lane_focus(LaneFocus::Lane(Lane::Ready))
            .with_focus(FocusPane::Content);
        build_tui_model_for_state(&events, &state)
    }

    #[test]
    fn work_item_detail_modal_renders_every_standardized_record_field() -> Result<(), String> {
        // The acceptance for the drill-in: with a fixture item whose every
        // field is populated, each one is READABLE in the modal. The title and
        // description especially -- the lane row shows neither, so before this
        // surface the operator could not tell what an item even WAS without
        // leaving the console.
        let model = fully_populated_item_model("a short body");
        let item = model
            .selected_lane_item()
            .ok_or("the drilled-in Ready lane has a selected item")?;
        let area = Rect::new(0, 0, 100, 40);
        let mut buffer = Buffer::empty(area);
        render_work_item_detail(Some(item), item.work_item_id(), None, area, &mut buffer, 0);
        let text = buffer_to_text(&buffer, area);

        for expected in [
            "Render the whole record",
            "a short body",
            "console-full",
            "bug",
            "ready",
            "a3",
            "freeform",
            "gap-77",
            "fabro",
            "console-dep-a",
            "console-dep-b (cross-repo)",
            "2026-07-19T00:00:00Z",
            "completed",
            "landed via PR #123",
            "abc123",
            "console-newer",
            "scenario-23-work-item-drill-in",
            "it renders",
            "an operator note",
            "console-older",
            "waiting on review",
            "needs-host-secrets",
            // The policy the orchestrator DID emit renders verbatim...
            "auto",
        ] {
            assert!(text.contains(expected), "record field missing: {expected}");
        }
        // ...and the one it did NOT emit is shown as unset, with the console's
        // own fallback labelled as the console's rather than as the item's. A
        // null policy does not mean the default -- the orchestrator resolves it
        // from an ancestor epic the console cannot see -- so printing the
        // fallback bare would make an explicitly-set policy and an unset one
        // indistinguishable, and would be wrong for an inheriting item.
        assert!(text.contains("not emitted; console assumes ai-then-human"));

        // Every field is LABELLED, so the operator can tell which is which.
        for label in [
            "id",
            "repo",
            "type",
            "status",
            "lane",
            "rank",
            "origin",
            "gap_id",
            "assignee",
            "depends_on",
            "captured_at",
            "resolution",
            "reason",
            "audit",
            "superseded_by",
            "spec_commitment_hint",
            "supersedes",
            "blocked_reason",
            "factory_safety",
            "acceptance_criteria",
            "notes",
            "lane_reason",
            "admission_policy",
            "acceptance_policy",
            "description",
        ] {
            assert!(text.contains(label), "record label missing: {label}");
        }
        Ok(())
    }

    /// A `ready` snapshot on which the orchestrator HAS published
    /// `awaits_scope_override`.
    ///
    /// The field is a consumption seam: the producer publishes the
    /// third-arm-refusal signal, and the console consumes it AS DATA instead of
    /// re-deriving the dispatcher's workflow-path regex.
    fn scope_override_pending_event(work_item_id: &str) -> ConsoleEvent {
        let payload = format!(
            concat!(
                r#"{{"repo":"console","work_item_id":"{}","#,
                r#""lane":"ready","lane_reason":null,"rank":"a0","status":"ready","#,
                r#""source_version":1,"detail":{{"title":"Scope override pending fixture","#,
                r#""factory_safety":null,"awaits_scope_override":true}}}}"#,
            ),
            work_item_id,
        );
        ConsoleEvent::fixture(
            &format!("evt_{work_item_id}"),
            EventType::WorkItemSnapshotObserved,
            "orchestrator",
        )
        .with_payload_json(payload)
    }

    fn driver_handoff_event(
        work_item_id: &str,
        lane: Lane,
        factory_safety: Option<&str>,
    ) -> ConsoleEvent {
        let factory_safety_json =
            factory_safety.map_or_else(|| "null".to_owned(), |value| format!("\"{value}\""));
        let payload = format!(
            concat!(
                r#"{{"repo":"console","work_item_id":"{}","#,
                r#""lane":"{}","lane_reason":null,"rank":"a0","status":"{}","#,
                r#""source_version":1,"detail":{{"title":"Driver handoff fixture","#,
                r#""factory_safety":{}}}}}"#,
            ),
            work_item_id,
            lane.label(),
            lane.label(),
            factory_safety_json,
        );
        ConsoleEvent::fixture(
            &format!("evt_{work_item_id}"),
            EventType::WorkItemSnapshotObserved,
            "orchestrator",
        )
        .with_payload_json(payload)
    }

    fn driver_handoff_attention_event(work_item_id: &str) -> ConsoleEvent {
        let item = AttentionItemSnapshot::new(
            &format!("attention-{work_item_id}"),
            "human-valve",
            "high",
            "Ready item needs host-only implementation",
            AttentionSourceRef::new("console", Some(work_item_id), None),
            AttentionHandoff::new("implement", None, &format!("implement:{work_item_id}")),
        );
        ConsoleEvent::fixture(
            &format!("evt_attention_{work_item_id}"),
            EventType::AttentionItemAppeared,
            "needs-attention",
        )
        .with_payload_json(attention_item_payload_json(&item))
    }

    fn driver_handoff_model(
        work_item_id: &str,
        lane: Lane,
        factory_safety: Option<&str>,
    ) -> TuiScreenModel {
        let events = [driver_handoff_event(work_item_id, lane, factory_safety)];
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_lane_focus(LaneFocus::Lane(lane))
            .with_focus(FocusPane::Content);
        build_tui_model_for_state(&events, &state)
    }

    #[test]
    fn driver_handoff_attention_selection_resolves_the_handoff_command() {
        let work_item_id = "livespec-console-beads-fabro-attention-ready";
        let events = [
            driver_handoff_event(work_item_id, Lane::Ready, Some("needs-privileged-host")),
            driver_handoff_attention_event(work_item_id),
        ];
        let model = build_tui_model_for_state(
            &events,
            &TuiInteractionState::new(0, TuiOverlay::None).with_focus(FocusPane::Content),
        );

        assert_eq!(
            model.selected_driver_handoff_command().as_deref(),
            Some(
                r#"claude "/livespec-orchestrator-beads-fabro:implement livespec-console-beads-fabro-attention-ready""#
            )
        );

        let inert = build_tui_model_for_state(
            &events,
            &TuiInteractionState::for_view(TuiView::Spec, 0, TuiOverlay::None),
        );
        assert_eq!(inert.selected_driver_handoff_command(), None);
    }

    #[test]
    fn driver_handoff_overlay_renders_lane_appropriate_id_only_invocations() {
        for (work_item_id, lane, factory_safety, operation) in [
            (
                "livespec-console-beads-fabro-backlog",
                Lane::Backlog,
                None,
                "groom",
            ),
            (
                "livespec-console-beads-fabro-unsafe",
                Lane::Ready,
                Some("needs-privileged-host"),
                "implement",
            ),
        ] {
            let model = driver_handoff_model(work_item_id, lane, factory_safety);
            assert_eq!(
                key_event_to_terminal_input(key(KeyCode::Char('h')), &model),
                Some(TuiTerminalInput::Interaction(
                    TuiInteraction::OpenDriverHandoff
                ))
            );
            let opened = reduce_tui_interaction(
                &TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
                    .with_lane_focus(LaneFocus::Lane(lane))
                    .with_focus(FocusPane::Content),
                &[driver_handoff_event(work_item_id, lane, factory_safety)],
                TuiInteraction::OpenDriverHandoff,
            );
            let rendered = render_to_text(
                &build_tui_model_for_state(
                    &[driver_handoff_event(work_item_id, lane, factory_safety)],
                    &opened,
                ),
                112,
                28,
            )
            .unwrap_or_default();
            let command = format!(
                r#"claude "/livespec-orchestrator-beads-fabro:{operation} {work_item_id}""#
            );

            assert!(rendered.contains("Driver Handoff"));
            assert!(rendered.contains(&command), "{rendered}");
            assert!(rendered.contains("enter copy sent to terminal"));
            assert!(!rendered.contains("Copied"));
            assert!(!rendered.contains("prompt"));
            assert!(!rendered.contains("tmp/livespec-console-handoffs"));
        }
    }

    #[test]
    fn driver_handoff_is_suppressed_outside_backlog_and_host_only_ready() {
        for (lane, factory_safety) in [
            (Lane::Ready, None),
            (Lane::PendingApproval, None),
            (Lane::Active, Some("needs-privileged-host")),
            (Lane::Acceptance, Some("needs-privileged-host")),
            (Lane::Blocked, Some("needs-privileged-host")),
            (Lane::Done, Some("needs-privileged-host")),
        ] {
            let model =
                driver_handoff_model("livespec-console-beads-fabro-safe", lane, factory_safety);
            assert_eq!(
                key_event_to_terminal_input(key(KeyCode::Char('h')), &model),
                None
            );
            assert!(!model.footer().contains("handoff"));

            let events = [driver_handoff_event(
                "livespec-console-beads-fabro-safe",
                lane,
                factory_safety,
            )];
            let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
                .with_lane_focus(LaneFocus::Lane(lane))
                .with_focus(FocusPane::Content);
            let suppressed =
                reduce_tui_interaction(&state, &events, TuiInteraction::OpenDriverHandoff);
            assert_eq!(suppressed.overlay(), &TuiOverlay::None);
        }
    }

    #[test]
    fn driver_handoff_key_remains_literal_text_inside_text_overlays() {
        let events = [driver_handoff_event(
            "livespec-console-beads-fabro-backlog",
            Lane::Backlog,
            None,
        )];
        let model = build_tui_model_for_state(
            &events,
            &TuiInteractionState::for_view(
                TuiView::Lanes,
                0,
                TuiOverlay::Search {
                    query: String::new(),
                },
            )
            .with_lane_focus(LaneFocus::Lane(Lane::Backlog))
            .with_focus(FocusPane::Content),
        );

        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('h')), &model),
            Some(TuiTerminalInput::Interaction(TuiInteraction::TypeChar('h')))
        );
    }

    #[test]
    fn driver_handoff_confirm_copies_without_persisting_or_polling() {
        let work_item_id = "livespec-console-beads-fabro-backlog";
        let events = [driver_handoff_event(work_item_id, Lane::Backlog, None)];
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_lane_focus(LaneFocus::Lane(Lane::Backlog))
            .with_focus(FocusPane::Content);
        let model = build_tui_model_for_state(&events, &state);
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('h')), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::OpenDriverHandoff
            ))
        );
        let opened = reduce_tui_interaction(&state, &events, TuiInteraction::OpenDriverHandoff);
        let opened_model = build_tui_model_for_state(&events, &opened);

        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Up), &opened_model),
            None
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Down), &opened_model),
            None
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &opened_model),
            Some(TuiTerminalInput::Confirm)
        );
        let step = step_tui_runtime(&opened, &events, TuiTerminalInput::Confirm, "operator");
        let effect = format!("{:?}", step.effect());

        assert!(effect.contains("Copy"));
        assert!(effect.contains("/livespec-orchestrator-beads-fabro:groom"));
        assert!(effect.contains(work_item_id));
        assert!(!effect.contains("PersistCommand"));
        assert!(!effect_triggers_source_poll(step.effect()));
    }

    #[test]
    fn work_item_detail_modal_shows_absent_fields_as_absent_and_handles_no_selection()
    -> Result<(), String> {
        // An unpopulated record renders placeholders rather than blank rows: the
        // operator must be able to tell "not set" from "not displayed".
        let model = lanes_model_content(LaneFocus::Lane(Lane::Ready), TuiOverlay::None);
        let item = model
            .selected_lane_item()
            .ok_or("the drilled-in Ready lane has a selected item")?;
        let area = Rect::new(0, 0, 80, 30);
        let mut buffer = Buffer::empty(area);
        render_work_item_detail(Some(item), item.work_item_id(), None, area, &mut buffer, 0);
        let text = buffer_to_text(&buffer, area);
        assert!(text.contains(ITEM_FIELD_ABSENT));
        // The lifecycle fields the lane row already carried still render.
        assert!(text.contains("console-ready-a") && text.contains("ready"));

        // With nothing selected the modal says so instead of rendering an empty
        // box that reads like a broken screen.
        let mut empty = Buffer::empty(area);
        render_work_item_detail(None, MODAL_ITEM, None, area, &mut empty, 0);
        assert!(buffer_to_text(&empty, area).contains("no longer on the board"));

        // A BLOCKED item renders its lane reason rather than the placeholder --
        // the one field on the record that comes from the lane assignment.
        let blocked_model = lanes_model_content(LaneFocus::Lane(Lane::Blocked), TuiOverlay::None);
        let blocked = blocked_model
            .selected_lane_item()
            .ok_or("the drilled-in Blocked lane has a selected item")?;
        let mut blocked_buffer = Buffer::empty(area);
        render_work_item_detail(
            Some(blocked),
            blocked.work_item_id(),
            None,
            area,
            &mut blocked_buffer,
            0,
        );
        let blocked_text = buffer_to_text(&blocked_buffer, area);
        assert!(blocked_text.contains("lane_reason") && blocked_text.contains("needs-human"));

        // A viewport too small to inset degrades without panicking.
        let tiny = Rect::new(0, 0, 2, 2);
        let mut tiny_buffer = Buffer::empty(tiny);
        render_work_item_detail(
            Some(item),
            item.work_item_id(),
            None,
            tiny,
            &mut tiny_buffer,
            0,
        );
        Ok(())
    }

    #[test]
    fn work_item_detail_modal_scrolls_a_long_description_to_its_bottom() -> Result<(), String> {
        // A long markdown body must be reachable: scrolling reveals the tail and
        // clamps at the true bottom rather than running past it into blankness.
        let model = fully_populated_item_model(&scrolling_description());
        let item = model
            .selected_lane_item()
            .ok_or("the drilled-in Ready lane has a selected item")?;
        let area = Rect::new(0, 0, 80, 20);

        let mut top = Buffer::empty(area);
        render_work_item_detail(Some(item), item.work_item_id(), None, area, &mut top, 0);
        let top_text = buffer_to_text(&top, area);
        assert!(top_text.contains("Render the whole record"));
        assert!(!top_text.contains("body line 39"));

        // A far-past-the-end offset clamps to the last row: the tail is visible
        // and the head has scrolled away.
        let mut bottom = Buffer::empty(area);
        render_work_item_detail(
            Some(item),
            item.work_item_id(),
            None,
            area,
            &mut bottom,
            10_000,
        );
        let bottom_text = buffer_to_text(&bottom, area);
        assert!(bottom_text.contains("body line 39"));
        assert!(!bottom_text.contains("Render the whole record"));
        // The close hint stays on its reserved row at every offset.
        assert!(top_text.contains("esc to close") && bottom_text.contains("esc to close"));
        Ok(())
    }

    #[test]
    fn enter_opens_the_item_modal_and_esc_closes_it_back_to_the_lane() {
        // The full round trip the acceptance names: Enter opens the record, Esc
        // closes it and lands back in the drilled-in lane (NOT out at the lane
        // overview -- Esc unwinds exactly one level).
        let events = lane_render_events();
        let drilled = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_lane_focus(LaneFocus::Lane(Lane::Ready))
            .with_focus(FocusPane::Content);
        let model = build_tui_model_for_state(&events, &drilled);
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::OpenWorkItemDetail
            ))
        );

        let opened = reduce_tui_interaction(&drilled, &events, TuiInteraction::OpenWorkItemDetail);
        assert_eq!(
            opened.overlay(),
            &TuiOverlay::WorkItemDetail {
                work_item_id: MODAL_ITEM.to_owned(),
                scroll: 0
            }
        );

        // Esc over the open modal closes the overlay...
        let opened_model = build_tui_model_for_state(&events, &opened);
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Esc), &opened_model),
            Some(TuiTerminalInput::Interaction(TuiInteraction::CloseOverlay))
        );
        let closed = reduce_tui_interaction(&opened, &events, TuiInteraction::CloseOverlay);
        assert_eq!(closed.overlay(), &TuiOverlay::None);
        // ...and leaves the operator in the lane they drilled into.
        assert_eq!(closed.lane_focus(), LaneFocus::Lane(Lane::Ready));
    }

    #[test]
    fn attention_enter_opens_the_item_modal_and_esc_preserves_selection() {
        let events = lane_render_events();
        let state = TuiInteractionState::for_view(TuiView::Attention, 1, TuiOverlay::None)
            .with_focus(FocusPane::Content);
        let model = build_tui_model_for_state(&events, &state);
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::OpenWorkItemDetail
            ))
        );

        let opened = reduce_tui_interaction(&state, &events, TuiInteraction::OpenWorkItemDetail);
        assert_eq!(
            opened.overlay(),
            &TuiOverlay::WorkItemDetail {
                work_item_id: "console-blocked".to_owned(),
                scroll: 0
            }
        );

        let opened_model = build_tui_model_for_state(&events, &opened);
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Esc), &opened_model),
            Some(TuiTerminalInput::Interaction(TuiInteraction::CloseOverlay))
        );
        let closed = reduce_tui_interaction(&opened, &events, TuiInteraction::CloseOverlay);
        assert_eq!(closed.overlay(), &TuiOverlay::None);
        assert_eq!(closed.selected_attention_index(), 1);
        assert_eq!(closed.active_view(), TuiView::Attention);
    }

    #[test]
    fn item_modal_scroll_keys_move_the_offset_and_are_inert_elsewhere() {
        let events = lane_render_events();
        let opened = TuiInteractionState::for_view(
            TuiView::Lanes,
            0,
            TuiOverlay::WorkItemDetail {
                work_item_id: MODAL_ITEM.to_owned(),
                scroll: 0,
            },
        )
        .with_lane_focus(LaneFocus::Lane(Lane::Ready))
        .with_focus(FocusPane::Content);
        let model = build_tui_model_for_state(&events, &opened);

        // up/down step one row; PgUp/PgDn move a page.
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Down), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::WorkItemDetailScrollDown(1)
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Up), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::WorkItemDetailScrollUp(1)
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::PageDown), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::WorkItemDetailPageDown
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::PageUp), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::WorkItemDetailPageUp
            ))
        );

        // The modal is READ-ONLY: Enter and `?` do nothing over it, so neither
        // fires an action nor dismisses it -- only Esc closes.
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &model),
            None
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('?')), &model),
            None
        );

        // Scrolling accumulates down and saturates at the top going up.
        let measured = opened.with_work_item_detail_scroll_extents(20, 6);
        let down = reduce_tui_interaction(
            &measured,
            &events,
            TuiInteraction::WorkItemDetailScrollDown(1),
        );
        assert_eq!(
            down.overlay(),
            &TuiOverlay::WorkItemDetail {
                work_item_id: MODAL_ITEM.to_owned(),
                scroll: 1,
            }
        );
        let up =
            reduce_tui_interaction(&down, &events, TuiInteraction::WorkItemDetailScrollUp(999));
        assert_eq!(
            up.overlay(),
            &TuiOverlay::WorkItemDetail {
                work_item_id: MODAL_ITEM.to_owned(),
                scroll: 0
            }
        );

        // The scroll interactions are inert against any other overlay.
        let elsewhere = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None);
        let unchanged = reduce_tui_interaction(
            &elsewhere,
            &events,
            TuiInteraction::WorkItemDetailScrollDown(1),
        );
        assert_eq!(unchanged.overlay(), &TuiOverlay::None);
    }

    #[test]
    fn item_modal_page_keys_use_the_render_measured_viewport_height() {
        for page_rows_u16 in [3_u16, 40] {
            let long_description = (0..90)
                .map(|index| format!("page marker {index:02}"))
                .collect::<Vec<_>>()
                .join("\n");
            let events = fully_populated_item_events(&long_description);
            let state = TuiInteractionState::for_view(
                TuiView::Lanes,
                0,
                TuiOverlay::WorkItemDetail {
                    work_item_id: "console-full".to_owned(),
                    scroll: 0,
                },
            );
            let page_rows = usize::from(page_rows_u16);
            let area = Rect::new(0, 0, 120, page_rows_u16 + 9);
            let mut buffer = Buffer::empty(area);
            let extents = render_model(
                &build_tui_model_for_state(&events, &state),
                area,
                &mut buffer,
            );
            assert_eq!(extents.work_item_detail_page_rows, page_rows);
            assert!(extents.work_item_detail_max_scroll >= page_rows);
            let state = state.with_work_item_detail_scroll_extents(
                extents.work_item_detail_max_scroll,
                extents.work_item_detail_page_rows,
            );

            let down =
                reduce_tui_interaction(&state, &events, TuiInteraction::WorkItemDetailPageDown);
            assert_eq!(down.overlay().work_item_detail_scroll(), Some(page_rows));

            let near_bottom = state.clone().with_overlay(TuiOverlay::WorkItemDetail {
                work_item_id: "console-full".to_owned(),
                scroll: extents
                    .work_item_detail_max_scroll
                    .saturating_sub(page_rows / 2),
            });
            let clamped = reduce_tui_interaction(
                &near_bottom,
                &events,
                TuiInteraction::WorkItemDetailPageDown,
            );
            assert_eq!(
                clamped.overlay().work_item_detail_scroll(),
                Some(extents.work_item_detail_max_scroll),
            );

            let up = reduce_tui_interaction(&down, &events, TuiInteraction::WorkItemDetailPageUp);
            assert_eq!(up.overlay().work_item_detail_scroll(), Some(0));
        }
    }

    #[test]
    fn item_modal_paging_sweep_displays_every_description_line() {
        for page_rows_u16 in [3_u16, 40] {
            let description = (0..90)
                .map(|index| format!("sweep marker {index:02}"))
                .collect::<Vec<_>>()
                .join("\n");
            let mut state = TuiInteractionState::for_view(
                TuiView::Lanes,
                0,
                TuiOverlay::WorkItemDetail {
                    work_item_id: "console-full".to_owned(),
                    scroll: 0,
                },
            );
            let area = Rect::new(0, 0, 120, page_rows_u16 + 9);
            let mut seen = Vec::new();

            loop {
                let events = fully_populated_item_events(&description);
                let model = build_tui_model_for_state(&events, &state);
                let mut buffer = Buffer::empty(area);
                let extents = render_model(&model, area, &mut buffer);
                let text = buffer_to_text(&buffer, area);
                for index in 0..90 {
                    let marker = format!("sweep marker {index:02}");
                    if text.contains(&marker) && !seen.contains(&index) {
                        seen.push(index);
                    }
                }
                state = state.with_work_item_detail_scroll_extents(
                    extents.work_item_detail_max_scroll,
                    extents.work_item_detail_page_rows,
                );
                let next = reduce_tui_interaction(
                    &state,
                    &lane_render_events(),
                    TuiInteraction::WorkItemDetailPageDown,
                );
                if next.overlay().work_item_detail_scroll()
                    == state.overlay().work_item_detail_scroll()
                {
                    break;
                }
                state = next;
            }

            for index in 0..90 {
                assert!(seen.contains(&index));
            }
        }
    }

    #[test]
    fn status_hint_stops_claiming_drill_where_enter_no_longer_drills() {
        // The reported bug: the Status line advertised "enter drill" inside a
        // drilled-in lane, where Enter did nothing at all. The hint must name
        // the action Enter ACTUALLY performs in the current context.
        let overview = lanes_model_content(LaneFocus::Overview, TuiOverlay::None);
        assert!(overview.footer().contains("enter drill"));

        let drilled = lanes_model_content(LaneFocus::Lane(Lane::Ready), TuiOverlay::None);
        assert!(!drilled.footer().contains("enter drill"));
        assert!(drilled.footer().contains("enter item"));

        // The open modal owns the hint line and names its own keys.
        let modal = lanes_model_content(
            LaneFocus::Lane(Lane::Ready),
            TuiOverlay::WorkItemDetail {
                work_item_id: MODAL_ITEM.to_owned(),
                scroll: 0,
            },
        );
        assert!(modal.footer().contains("esc close item"));
        assert!(!modal.footer().contains("enter drill"));
    }

    #[test]
    fn the_open_item_modal_draws_over_the_whole_screen() -> TuiRenderResult<()> {
        // End-to-end through the real render path (not just the modal fn): with
        // the overlay open the record is drawn ON TOP of the lane board, so the
        // operator actually sees it on screen.
        let state = TuiInteractionState::for_view(
            TuiView::Lanes,
            0,
            TuiOverlay::WorkItemDetail {
                work_item_id: MODAL_ITEM.to_owned(),
                scroll: 0,
            },
        )
        .with_lane_focus(LaneFocus::Lane(Lane::Ready))
        .with_focus(FocusPane::Content);
        let model = build_tui_model_for_state(&lane_render_events(), &state);
        let text = render_to_text(&model, 100, 40)?;
        assert!(text.contains("Work item: console-ready-a"));
        assert!(text.contains("esc to close"));
        Ok(())
    }

    #[test]
    fn an_open_item_modal_keeps_its_own_item_when_the_lane_re_ranks_beneath_it()
    -> TuiRenderResult<()> {
        // The modal stays open across source refreshes, and ingestion keeps
        // appending. If it re-resolved its record from the lane SELECTION INDEX,
        // a sibling re-ranked above the pinned item would slide a DIFFERENT
        // work-item under that index and silently swap the record the operator
        // is reading -- with nothing on screen to say so. It resolves by the id
        // pinned at open instead, so the record cannot drift.
        let state = TuiInteractionState::for_view(
            TuiView::Lanes,
            0,
            TuiOverlay::WorkItemDetail {
                work_item_id: MODAL_ITEM.to_owned(),
                scroll: 0,
            },
        )
        .with_lane_focus(LaneFocus::Lane(Lane::Ready))
        .with_focus(FocusPane::Content);

        // A newcomer ranked ABOVE the pinned item takes over index 0.
        let mut events = lane_render_events().to_vec();
        events.push(lane_event(
            "evt_jump",
            "console-ready-jumped-ahead",
            Lane::Ready,
            None,
            // Sorts ahead of the fixture's "a0" (prefix), so it really does
            // take over index 0 rather than tie-breaking behind it by id.
            "a",
            "ready",
        ));
        let model = build_tui_model_for_state(&events, &state);
        // The fixture really does move a DIFFERENT item under the selection --
        // without this the rest of the test would pass vacuously.
        assert_eq!(
            model.selected_lane_item().map(LaneWorkItem::work_item_id),
            Some("console-ready-jumped-ahead")
        );

        let text = render_to_text(&model, 100, 40)?;
        // Still the item it was opened on, NOT the one that took over the index.
        assert!(text.contains(&format!("Work item: {MODAL_ITEM}")));
        assert!(!text.contains("Work item: console-ready-jumped-ahead"));
        Ok(())
    }

    #[test]
    fn an_item_that_leaves_the_board_is_reported_not_silently_replaced() {
        // Once the pinned item is gone entirely there is no record to show. Say
        // so, naming the item, rather than rendering a neighbour's record or an
        // empty box that reads as a broken screen.
        let area = Rect::new(0, 0, 90, 20);
        let mut buffer = Buffer::empty(area);
        render_work_item_detail(None, "console-vanished", None, area, &mut buffer, 0);
        let text = buffer_to_text(&buffer, area);
        assert!(text.contains("Work item: console-vanished"));
        assert!(text.contains("no longer on the board"));
    }

    #[test]
    fn the_lanes_help_section_documents_the_item_drill_in() {
        // The Help text is required to stay in lock-step with the key handler.
        let text = help_lines_for_view(TuiView::Lanes)
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("open the selected work-item's record"));
        assert!(text.contains("description"));
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
            .with_plugin_resolution(PluginResolution::resolved(
                "installed Claude plugin cache".to_owned(),
                "/home/operator/.claude/plugins/marketplaces/livespec-orchestrator-beads-fabro"
                    .to_owned(),
                Some("newest-build".to_owned()),
            ))
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
    fn renders_the_resolved_orchestrator_plugin_build_in_settings() {
        let rendered = render_to_text(&settings_model(0), 120, 24);
        let text = rendered.unwrap_or_default();

        assert!(text.contains("Orchestrator plugin"), "{text}");
        assert!(text.contains("newest-build"), "{text}");
        assert!(text.contains("Root:"), "{text}");
        assert!(text.contains("/home/operator/.claude/plugins/marketplaces/livespec-o"));
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
        assert!(settings_detail_lines(&model).contains(&Line::from("No setting selected")));
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
        let pending = attention_model_for_lane(Lane::PendingApproval, TuiOverlay::None);
        for (code, valve) in [
            ('p', PendingValve::Approve),
            ('r', PendingValve::Reject(RejectMode::Rework)),
            ('m', PendingValve::SetAdmission(AdmissionPolicy::Manual)),
            (
                'n',
                PendingValve::SetAcceptance(AcceptancePolicy::AiThenHuman),
            ),
        ] {
            assert_eq!(
                key_event_to_terminal_input(key(KeyCode::Char(code)), &pending),
                Some(TuiTerminalInput::Interaction(
                    TuiInteraction::OpenValveConfirm(valve)
                ))
            );
        }
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('c')), &pending),
            None
        );

        let acceptance = attention_model_for_lane(Lane::Acceptance, TuiOverlay::None);
        for (code, valve) in [
            ('c', PendingValve::Accept),
            ('r', PendingValve::Reject(RejectMode::Rework)),
        ] {
            assert_eq!(
                key_event_to_terminal_input(key(KeyCode::Char(code)), &acceptance),
                Some(TuiTerminalInput::Interaction(
                    TuiInteraction::OpenValveConfirm(valve)
                ))
            );
        }
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('p')), &acceptance),
            None
        );
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
        // `s` stages the move-status valve at the first ratified target:
        // pending-approval can withdraw to backlog or park as blocked, while
        // admission to ready stays on the approve valve.
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('s')), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::OpenValveConfirm(PendingValve::MoveStatus {
                    from: Lane::PendingApproval,
                    to: Lane::Backlog,
                })
            ))
        );
        // A per-item override key stages the override valve at its `clear` start.
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('f')), &model),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::OpenValveConfirm(PendingValve::SetOverride(
                    DispatcherOverride::ReviewFixCap(OverrideInt::Clear)
                ))
            ))
        );
    }

    #[test]
    fn move_status_on_a_pending_approval_lane_item_persists_the_move_command() {
        // From a drilled-in lane, selecting a pending-approval item and
        // confirming the move-to-status valve at its first ratified target
        // persists the guarded move command, not the approve valve.
        let state = drilled_pending_state(TuiOverlay::ValveConfirm {
            valve: PendingValve::MoveStatus {
                from: Lane::PendingApproval,
                to: Lane::Backlog,
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
            Some(&CommandType::WorkItemMoveRequested)
        );
        assert_eq!(
            persisted_payload(step.effect()),
            Some(r#"{"target_status":"backlog"}"#)
        );
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
        // Drilled into the done lane, whose shipped item has no operator-drivable
        // onward move, so `s` is inert (the picker never un-ships a done item).
        let done_events = [lane_event(
            "evt_done",
            "console-done",
            Lane::Done,
            None,
            "a0",
            "done",
        )];
        let done = build_tui_model_for_state(
            &done_events,
            &TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
                .with_lane_focus(LaneFocus::Lane(Lane::Done))
                .with_selected_lane_item_index(0)
                .with_focus(FocusPane::Content),
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('s')), &done),
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
    fn override_keys_are_inert_without_a_selection_and_literal_in_a_text_overlay() {
        // A view with no selected work-item makes g/f/k inert.
        let events = demo_events();
        let non_item = build_tui_model_for_state(
            &events,
            &TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None),
        );
        for code in ['g', 'f', 'k'] {
            assert_eq!(
                key_event_to_terminal_input(key(KeyCode::Char(code)), &non_item),
                None
            );
        }
        // Behind a text overlay an override key is a literal character.
        let search = attention_model(TuiOverlay::Search {
            query: String::new(),
        });
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('g')), &search),
            Some(TuiTerminalInput::Interaction(TuiInteraction::TypeChar('g')))
        );
    }

    #[test]
    fn override_keys_stage_each_setting_and_confirming_persists_the_override_command() {
        // g/f/k stage the three per-item cap overrides at their `clear` start.
        let attention = lanes_model_content(LaneFocus::Lane(Lane::Ready), TuiOverlay::None);
        for (code, dial) in [
            (
                'g',
                DispatcherOverride::MergeOnReviewCap(OverrideBool::Clear),
            ),
            ('f', DispatcherOverride::ReviewFixCap(OverrideInt::Clear)),
            (
                'k',
                DispatcherOverride::AcceptanceReworkCap(OverrideInt::Clear),
            ),
        ] {
            assert_eq!(
                key_event_to_terminal_input(key(KeyCode::Char(code)), &attention),
                Some(TuiTerminalInput::Interaction(
                    TuiInteraction::OpenValveConfirm(PendingValve::SetOverride(dial))
                ))
            );
        }
        // Confirming a staged override persists the set-dispatcher-override command
        // carrying the setting and value payload.
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::ValveConfirm {
                valve: PendingValve::SetOverride(DispatcherOverride::ReviewFixCap(
                    OverrideInt::Value(3),
                )),
            },
        );
        let step = step_tui_runtime(
            &state,
            &pending_events(),
            TuiTerminalInput::Confirm,
            "operator",
        );
        assert_eq!(
            persisted_command(step.effect()).map(console_domain::CommandEnvelope::command_type),
            Some(&CommandType::WorkItemSetDispatcherOverrideRequested)
        );
        assert_eq!(
            persisted_payload(step.effect()),
            Some(r#"{"setting":"review_fix_cap","value":3}"#)
        );
    }

    #[test]
    fn help_overlay_auto_focuses_the_active_pane_section() {
        // `?` from the Settings pane auto-focuses the Settings section: the right
        // pane names the six settings and the edit, not the item-lane actions,
        // and the menu marks Settings as selected.
        let settings = build_tui_model_for_state(
            &[],
            &TuiInteractionState::for_view(
                TuiView::Settings,
                0,
                TuiOverlay::Help {
                    focus: HelpFocus::Menu,
                    selected_section: help_section_for_view(TuiView::Settings),
                    scroll: 0,
                },
            ),
        );
        let settings_help = render_to_text(&settings, 120, 72).unwrap_or_default();
        assert!(settings_help.contains("> Settings"), "{settings_help}");
        assert!(settings_help.contains("auto_approve_ready"));
        assert!(settings_help.contains("edit the selected setting row"));
        assert!(!settings_help.contains("move the selected work-item to a status"));

        // `?` from the Lanes pane auto-focuses the Lanes section: item selection
        // and the move-to-status action, with Lanes marked selected.
        let lanes = build_tui_model_for_state(
            &lane_render_events(),
            &TuiInteractionState::for_view(
                TuiView::Lanes,
                0,
                TuiOverlay::Help {
                    focus: HelpFocus::Menu,
                    selected_section: help_section_for_view(TuiView::Lanes),
                    scroll: 0,
                },
            ),
        );
        let lanes_help = render_to_text(&lanes, 120, 72).unwrap_or_default();
        assert!(lanes_help.contains("> Lanes"), "{lanes_help}");
        assert!(lanes_help.contains("Move status"));
        assert!(lanes_help.contains("select an individual work-item"));
        // The roster derives from the registry: the driver handoff, the
        // narrowed move-status picker, and the override dials are all named.
        assert!(lanes_help.contains("Driver handoff"));
        assert!(lanes_help.contains("target status: backlog | ready | blocked"));
        assert!(!lanes_help.contains("any pre-terminal status"));
        assert!(!lanes_help.contains("approve -> ready"));
        assert!(lanes_help.contains("merge_on_review_cap"));
        // The Lanes right pane must not spill the Settings section's text.
        assert!(!lanes_help.contains("edit the selected setting row"));
    }

    #[test]
    fn render_valve_confirm_shows_the_override_dial_value() {
        // The per-item override modal renders its dynamic `key = value` dial via
        // option_display (the `'static` option_label path cannot carry the int).
        let model = build_tui_model_for_state(
            &demo_events(),
            &TuiInteractionState::new(
                0,
                TuiOverlay::ValveConfirm {
                    valve: PendingValve::SetOverride(DispatcherOverride::ReviewFixCap(
                        OverrideInt::Value(4),
                    )),
                },
            ),
        );
        let rendered = render_to_text(&model, 96, 24).unwrap_or_default();
        assert!(rendered.contains("Set override work-item"));
        assert!(rendered.contains("review_fix_cap = 4"));
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
            &pending_events(),
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
            1,
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
        // The Attention section (auto-focused on the default view) names the
        // per-item valve keys.
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::Help {
                focus: HelpFocus::Menu,
                selected_section: help_section_for_view(TuiView::Attention),
                scroll: 0,
            },
        );
        let model = build_tui_model_for_state(&demo_events(), &state);
        // A tall area so the full section body (including the valve keys near the
        // bottom) renders inside the modal rather than being clipped.
        let output = render_to_text(&model, 120, 72);
        // The roster is DERIVED from the registry: every action the Attention
        // surface can offer is listed by hotkey and label.
        let rendered = output.unwrap_or_default();
        for spec in action_registry::ACTION_REGISTRY {
            let offered = action_registry::action_offered_on_surface(
                spec,
                action_registry::ActionSurface::Attention,
            );
            assert_eq!(rendered.contains(spec.label), offered, "{}", spec.id);
        }
    }

    #[test]
    fn help_menu_navigation_switches_the_right_pane_section() {
        // Navigating the left menu (HelpSelectNextSection) switches the right
        // pane's content: from the Lanes section (auto-focused) to the Events
        // section, the Lanes-only text disappears and the Events text appears.
        let events = lane_render_events();
        let opened = reduce_tui_interaction(
            &TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None),
            &events,
            TuiInteraction::OpenHelp,
        );
        let lanes = build_tui_model_for_state(&events, &opened);
        let lanes_text = render_to_text(&lanes, 120, 40).unwrap_or_default();
        assert!(lanes_text.contains("lane board"), "{lanes_text}");
        assert!(!lanes_text.contains("event timeline"), "{lanes_text}");

        let next = reduce_tui_interaction(&opened, &events, TuiInteraction::HelpSelectNextSection);
        let events_model = build_tui_model_for_state(&events, &next);
        let events_text = render_to_text(&events_model, 120, 40).unwrap_or_default();
        assert!(events_text.contains("> Events"), "{events_text}");
        assert!(events_text.contains("event timeline"), "{events_text}");
        assert!(!events_text.contains("lane board"), "{events_text}");
    }

    #[test]
    fn help_modal_is_inset_by_a_three_character_border() {
        // The modal is a window on top of the main screen with a 3-character
        // border on each side and on top and bottom, never wider than the
        // viewport. At 112x28 the box's corners sit at column 3 and column 108.
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::Help {
                focus: HelpFocus::Menu,
                selected_section: 0,
                scroll: 0,
            },
        );
        let model = build_tui_model_for_state(&demo_events(), &state);
        let output = render_to_text(&model, 112, 28).unwrap_or_default();
        let lines: Vec<&str> = output.lines().collect();
        // The char AT a given column (one char per cell); the nav's own box lives
        // at column 0, so read column 3 directly rather than the first corner.
        let at = |line: &str, col: usize| line.chars().nth(col);
        // A 3-char left/right margin puts the modal's box corners at column 3 and
        // column 108 (112 - 1 - 3). Top and bottom rows are inset 3 from the edges.
        let top_row = lines.iter().position(|line| at(line, 3) == Some('┌'));
        assert_eq!(top_row, Some(3), "top-border inset to column 3");
        assert_eq!(at(lines[3], 108), Some('┐'), "top-right corner");
        let bottom_row = lines.iter().position(|line| at(line, 3) == Some('└'));
        assert_eq!(bottom_row, Some(24), "bottom-border at row 24");
        assert_eq!(at(lines[24], 108), Some('┘'), "bottom-right corner");
        // Never wider than the viewport.
        for line in &lines {
            assert!(line.chars().count() <= 112, "line exceeds viewport");
        }
    }

    #[test]
    fn render_help_overlay_renders_every_pane_section() {
        // Each menu section renders its own text: Global actions plus one per
        // focusable pane (Attention, Spec, Lanes, Events, Repos, Settings), each
        // marked selected, with the always-visible `esc to exit` footer.
        let render_section = |section: usize| {
            let state = TuiInteractionState::new(
                0,
                TuiOverlay::Help {
                    focus: HelpFocus::Menu,
                    selected_section: section,
                    scroll: 0,
                },
            );
            let model = build_tui_model_for_state(&demo_events(), &state);
            render_to_text(&model, 120, 72).unwrap_or_default()
        };
        let global = render_section(0);
        assert!(global.contains("Global actions -- available from every view"));
        let expectations = [
            (TuiView::Attention, "merged, ranked needs-attention"),
            (TuiView::Spec, "spec-side status"),
            (TuiView::Lanes, "work-item lane board"),
            (TuiView::Events, "console event timeline"),
            (TuiView::Repos, "fleet repo roster"),
            (TuiView::Settings, "dispatcher-settings surface"),
        ];
        for (view, needle) in expectations {
            let output = render_section(help_section_for_view(view));
            assert!(output.contains(needle), "{view:?} missing {needle:?}");
            let marker = format!("> {}", view.label());
            assert!(output.contains(&marker), "{view:?} not marked selected");
            assert!(output.contains("esc to exit"), "{view:?} footer missing");
        }
    }

    #[test]
    fn render_help_overlay_reports_scroll_extents_for_page_keys() {
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::Help {
                focus: HelpFocus::Menu,
                selected_section: 0,
                scroll: 0,
            },
        );
        let model = build_tui_model_for_state(&demo_events(), &state);
        let area = Rect::new(0, 0, 120, 18);
        let mut buffer = Buffer::empty(area);
        let extents = render_model(&model, area, &mut buffer);

        // 18 rows total - 3 top/bottom Help margins * 2 - 2 Help borders - 1
        // reserved footer row = 9 visible prose rows.
        assert_eq!(extents.help_page_rows, 9);
        assert!(extents.help_max_scroll > 0);
        assert_eq!(extents.work_item_detail_page_rows, 1);
        assert_eq!(extents.work_item_detail_max_scroll, 0);
    }

    #[test]
    fn render_help_overlay_keeps_selected_menu_row_marked_when_text_focused() {
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::Help {
                focus: HelpFocus::Text,
                selected_section: 0,
                scroll: 0,
            },
        );
        let model = build_tui_model_for_state(&demo_events(), &state);
        let output = render_to_text(&model, 120, 24).unwrap_or_default();

        assert!(output.contains("> Global actions"), "{output}");
        assert!(output.contains("esc to exit"), "{output}");
    }

    #[test]
    fn render_help_overlay_survives_a_viewport_too_small_to_inset() {
        // A viewport smaller than the 3-char frame collapses the modal's inner
        // area to nothing; the renderer must guard it and not panic.
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::Help {
                focus: HelpFocus::Menu,
                selected_section: 0,
                scroll: 0,
            },
        );
        let model = build_tui_model_for_state(&demo_events(), &state);
        let output = render_to_text(&model, 7, 7);
        assert!(output.is_ok(), "tiny render must not error: {output:?}");
    }

    #[test]
    fn keymap_page_keys_scroll_only_the_help_pane() {
        // PageUp/PageDown scroll the Help right pane; on every other surface they
        // are inert.
        let help = attention_model(TuiOverlay::Help {
            focus: HelpFocus::Menu,
            selected_section: 0,
            scroll: 0,
        });
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::PageDown), &help),
            Some(TuiTerminalInput::Interaction(TuiInteraction::HelpPageDown))
        );
        let none = attention_model(TuiOverlay::None);
        let search = attention_model(TuiOverlay::Search {
            query: String::new(),
        });
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::PageDown), &none),
            None
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::PageUp), &search),
            None
        );
    }

    #[test]
    fn keymap_up_down_move_the_selection_behind_a_text_overlay() {
        // Behind Search / Command Palette, up/down are the harmless content
        // moves; the section-menu navigation is Help-only.
        let search = attention_model(TuiOverlay::Search {
            query: String::new(),
        });
        let palette = attention_model(TuiOverlay::CommandPalette {
            query: String::new(),
        });
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Up), &search),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::SelectPrevious
            ))
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Down), &palette),
            Some(TuiTerminalInput::Interaction(TuiInteraction::SelectNext))
        );
    }
}
