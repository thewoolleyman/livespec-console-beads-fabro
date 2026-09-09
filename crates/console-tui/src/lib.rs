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

#[cfg(all(not(test), not(coverage)))]
use console_application::build_identity::BuildIdentity;
use console_application::build_identity::{BuildStaleness, build_identity_segment};
use console_application::source_adapters::{
    Lane, OrphanedFactoryRun, event_source_roster_help_lines,
};
use console_application::writer_identity::WriterLeaseStatus;
use console_application::{
    ApplicationError, AttentionDetail, AttentionItem, DispatcherSettingsRead, EventsFocus,
    FocusPane, HELP_SECTION_COUNT, HelpFocus, LaneColumn, LaneExecutionState, LaneFocus,
    LaneWorkItem, OperatorAction, OperatorActionOutcome, PendingValve, PluginResolution,
    SettingRow, TimelineEntry, TuiInteraction, TuiInteractionState, TuiOverlay, TuiScreenModel,
    TuiView, ViewSummaryItem, action_registry, build_tui_model_for_state, dispatcher_setting_rows,
    header_help_section, reduce_tui_interaction, resolve_command_palette_action,
    resolve_dispatcher_setting_edit, resolve_valve_action, validate_operator_action,
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

// `io`, `Event`, and `KeyEventKind` back the testable burst-drain seam
// (`InputSource`, `drain_input_burst`, `apply_tick_refresh`, `LoopTick`) as
// well as the real terminal loop, so they are available whenever either is
// compiled (`test`, or the real non-coverage build).
#[cfg(any(test, not(coverage)))]
use crossterm::event::{Event, KeyEventKind};
#[cfg(any(test, not(coverage)))]
use std::io;
// `Duration` and the `event` module (`event::poll`/`event::read`) back the
// REAL crossterm-backed input source and the loop's own initial wait only, so
// they stay with the terminal loop's build.
#[cfg(all(not(test), not(coverage)))]
use std::time::Duration;

#[cfg(all(not(test), not(coverage)))]
use crossterm::event;
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
        None,
        BuildStaleness::Unknown,
        &mut effect_sink,
    )
}

#[cfg(all(not(test), not(coverage)))]
#[allow(clippy::too_many_arguments)]
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
    build_identity: Option<BuildIdentity>,
    build_staleness: BuildStaleness,
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
        build_identity,
        build_staleness,
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
#[allow(clippy::too_many_arguments)]
fn run_terminal_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    events: &[ConsoleEvent],
    requested_by: &str,
    selected_repo: &str,
    dispatcher_settings: DispatcherSettingsRead,
    plugin_resolution: PluginResolution,
    build_identity: Option<BuildIdentity>,
    build_staleness: BuildStaleness,
    session: &mut dyn TuiLiveSession,
) -> io::Result<Vec<TuiRuntimeEffect>> {
    let mut state = TuiInteractionState::new(0, TuiOverlay::None)
        .with_selected_repo(selected_repo.to_owned())
        .with_dispatcher_settings(dispatcher_settings)
        .with_plugin_resolution(plugin_resolution)
        .with_build_identity(build_identity)
        .with_build_staleness(build_staleness)
        // Seeded from the session BEFORE the first frame draws
        // (livespec-console-beads-fabro-pzbdbo.27): the very first `terminal.draw`
        // below must already say so if the background poller has not completed
        // its first sweep yet, rather than warming up silent and correcting
        // itself a tick later.
        .with_startup_ingest_pending(session.first_ingest_in_progress());
    // The event log is OWNED and re-projected every iteration (Bug B fix): each
    // projection reduces over the LATEST events, not a snapshot frozen at
    // startup, so the board and detail panes stay live.
    let mut events = events.to_vec();
    let mut effects = Vec::new();
    // The event-log-derived half of the model, cached across ticks and rebuilt
    // only when the log or the active search query actually changed (see
    // `ProjectionCache`) -- NOT on every keystroke. This, together with
    // `process_input_tick` reusing the model it draws instead of rebuilding its
    // own copy to interpret the key, and `reduce_tui_interaction_with_model`
    // taking that same model instead of rebuilding a THIRD copy, is the fix for
    // livespec-console-beads-fabro-mx9u.8 ("moving between attention items is
    // very laggy"): before this, a single keystroke rebuilt the whole
    // projection from scratch four times over.
    let mut projection_cache = ProjectionCache::new();
    loop {
        let search_query = console_application::tui_search_query(state.overlay());
        let projection = projection_cache.get_or_build(&events, search_query);
        let model = console_application::render_tui_model(projection, &events, &state);
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
        let tick = process_input_tick(
            &mut state,
            &events,
            projection,
            requested_by,
            &mut effects,
            session,
        )?;
        if matches!(tick, LoopTick::Quit) {
            return Ok(effects);
        }
        // Drain the out-of-band worker channel every tick, BEFORE the re-list.
        // A command the worker could not execute leaves no trace in the event
        // log — that is the whole defect — so the log refresh below can never
        // surface it and this is the only place the operator is told. This is a
        // non-blocking in-memory channel read, never a store or backing-CLI
        // call, so it stays unconditional even on the fast navigation path.
        apply_worker_status(&mut state, session.take_worker_status());
        // Re-read the background-probed build staleness every tick, same as the
        // worker status above: a non-blocking in-memory read (a `Mutex` lock
        // around a `Copy` enum), never a `git` shell-out on this thread. The
        // shell-out itself runs off-thread on the source poller's cadence --
        // see `console_application::build_identity::SharedBuildStaleness` for
        // why that cadence, not this tick, is where the IO belongs
        // (livespec-console-beads-fabro-mx9u.26).
        apply_build_staleness(&mut state, session.take_build_staleness());
        // Same cadence: a cheap, non-blocking check of whether the session's
        // first background ingest is still in flight, so the header's
        // `event sources: loading` tell clears the moment that sweep lands
        // rather than lingering a tick behind it (livespec-console-beads-
        // fabro-pzbdbo.27).
        apply_startup_ingest_pending(&mut state, session.first_ingest_in_progress());
        // Re-read the background-observed writer-lease status every tick, same
        // as the build staleness above: a non-blocking `Mutex` read, never IO
        // on this thread (livespec-console-beads-fabro-mx9u.23 AC3).
        apply_writer_lease_status(&mut state, session.take_writer_lease_status());
        // Whether -- and how -- this tick's outcome warrants a store refresh.
        // `LoopTick::HandledInput` (one or more keys handled, none mutating)
        // skips it entirely: no store read, no backing-CLI call on the
        // selection-change path (livespec-console-beads-fabro-mx9u.8, acceptance
        // criterion 2). See `apply_tick_refresh`.
        if let Some(fresh) = apply_tick_refresh(tick, session)? {
            events = fresh;
        }
        // The operator's own settings write has now reported an outcome, so the
        // effective policy is re-read ONCE and folded in. Gated on the outcome
        // event because the `config` read is a real orchestrator invocation and
        // because "the worker has not run it yet" and "it ran and changed
        // nothing" are indistinguishable in a bare re-read.
        if console_application::dispatcher_setting_write_settled(
            state.dispatcher_setting_write(),
            &events,
        ) && let Some(fresh) = session.refresh_dispatcher_settings()?
        {
            apply_dispatcher_settings_reread(&mut state, fresh);
        }
    }
}

/// Caches the event-log-derived projection ([`console_application::TuiProjection`])
/// across terminal-loop ticks, rebuilding it only when
/// [`ProjectionCacheKey`] changes -- i.e. only when the event log or the
/// active search query actually changed since the last build. A run of
/// navigation keystrokes with no intervening store refresh shares ONE
/// projection across every render, which is exactly the property
/// livespec-console-beads-fabro-mx9u.8's acceptance criterion 3 ("one
/// projection, ten renders") measures.
/// The cache holds a REAL entry from construction on (an empty-log projection,
/// keyed to match) rather than an `Option`, so `get_or_build` never needs to
/// prove-then-unwrap a value it just inserted; the empty-log entry is simply
/// correct-by-construction for an empty log and gets replaced on the first
/// real one, same as any other stale entry.
#[cfg(all(not(test), not(coverage)))]
struct ProjectionCache {
    entry: (ProjectionCacheKey, console_application::TuiProjection),
}

#[cfg(all(not(test), not(coverage)))]
impl ProjectionCache {
    fn new() -> Self {
        Self {
            entry: (
                ProjectionCacheKey::new(&[], None),
                console_application::project_tui_events(&[], None),
            ),
        }
    }

    fn get_or_build(
        &mut self,
        events: &[ConsoleEvent],
        search_query: Option<&str>,
    ) -> &console_application::TuiProjection {
        let key = ProjectionCacheKey::new(events, search_query);
        if self.entry.0 != key {
            self.entry = (
                key,
                console_application::project_tui_events(events, search_query),
            );
        }
        &self.entry.1
    }
}

/// A cheap fingerprint of the inputs [`console_application::project_tui_events`]
/// depends on. The event log is append-only within one session and event ids
/// are stable, so `(event count, last event id)` is enough to detect a real
/// change without comparing the whole log on every tick.
#[cfg(all(not(test), not(coverage)))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectionCacheKey {
    event_count: usize,
    last_event_id: Option<String>,
    search_query: Option<String>,
}

#[cfg(all(not(test), not(coverage)))]
impl ProjectionCacheKey {
    fn new(events: &[ConsoleEvent], search_query: Option<&str>) -> Self {
        Self {
            event_count: events.len(),
            last_event_id: events.last().map(|event| event.event_id().to_owned()),
            search_query: search_query.map(str::to_owned),
        }
    }
}

/// The outcome of one input tick, telling the loop whether -- and how -- to
/// refresh from the store afterward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(any(test, not(coverage)))]
enum LoopTick {
    /// A true poll timeout: no key arrived at all this tick. Still worth a
    /// refresh on the loop's normal cadence, so the inbox keeps tracking the
    /// poller's own background work while the operator is idle.
    Idle,
    /// One or more keys were handled by [`drain_input_burst`] and NONE of them
    /// mutated the ledger -- the fast navigation path
    /// livespec-console-beads-fabro-mx9u.8 exists for. Skips the refresh
    /// entirely: no store read, no backing-CLI call.
    HandledInput,
    /// A ledger-mutating effect was applied — re-poll the sources at once so the
    /// operator sees their own action's lane change without waiting.
    Mutated,
    /// The operator asked to quit; return the deferred effects.
    Quit,
}

/// Decide whether this tick's outcome warrants a store refresh, and perform
/// it. Split out from the terminal-bound loop for the same reason
/// `apply_sink_outcome` is: this DECISION -- that a
/// [`LoopTick::HandledInput`] tick skips the refresh outright -- is exactly
/// what livespec-console-beads-fabro-mx9u.8's acceptance criterion 2 ("no
/// backing-CLI invocation and no ledger read" on the selection-change path)
/// requires, and it needs to be provable without a real terminal.
#[cfg(any(test, not(coverage)))]
fn apply_tick_refresh(
    tick: LoopTick,
    session: &mut dyn TuiLiveSession,
) -> io::Result<Option<Vec<ConsoleEvent>>> {
    match tick {
        LoopTick::Idle => session.refresh_events(false),
        LoopTick::Mutated => session.refresh_events(true),
        LoopTick::HandledInput | LoopTick::Quit => Ok(None),
    }
}

/// A source of terminal input events the burst-drain loop reads from -- the
/// crossterm-backed implementation the terminal loop uses, or a scripted
/// in-memory queue in tests. This is the seam that makes burst coalescing
/// (livespec-console-beads-fabro-mx9u.9) testable without a real terminal.
#[cfg(any(test, not(coverage)))]
trait InputSource {
    /// Whether another input event is already buffered, WITHOUT blocking.
    fn poll_now(&mut self) -> io::Result<bool>;
    /// Read the next input event. Callers call this only once they (or the
    /// tick's initial wait) already know one is ready, so it never blocks.
    fn read(&mut self) -> io::Result<Event>;
}

#[cfg(all(not(test), not(coverage)))]
struct CrosstermInputSource;

#[cfg(all(not(test), not(coverage)))]
impl InputSource for CrosstermInputSource {
    fn poll_now(&mut self) -> io::Result<bool> {
        event::poll(Duration::ZERO)
    }

    fn read(&mut self) -> io::Result<Event> {
        event::read()
    }
}

/// The most buffered movement keystrokes one tick will coalesce before
/// stopping to render — livespec-console-beads-fabro-mx9u.9, acceptance
/// criterion 2: "the buffer is bounded". Generous enough that a real burst of
/// held-down arrow keys never hits it in practice, but finite so a runaway
/// input source cannot starve the render loop forever.
#[cfg(any(test, not(coverage)))]
const MAX_COALESCED_MOVEMENT_KEYS: usize = 64;

/// Whether `input` is the one class of interaction `drain_input_burst` is
/// willing to coalesce -- i.e. apply without returning to the caller for a
/// terminal redraw: plain Attention-list navigation. Every key the burst reads
/// is applied against a freshly re-rendered model regardless (see
/// `drain_input_burst`'s doc comment), so this is not a correctness boundary;
/// it is a UX one -- a verb, Enter, or Escape gets its own visible frame right
/// away rather than being buried inside a movement batch. Scoped narrowly to
/// `SelectNext`/`SelectPrevious` (Up/Down in the Attention list, the exact
/// keystroke livespec-console-beads-fabro-mx9u.8 and -mx9u.9 report on) rather
/// than every interaction that happens to be navigation-shaped today, so a
/// future addition to that set is a deliberate choice, not an accident.
#[cfg(any(test, not(coverage)))]
const fn is_navigation_interaction(input: &TuiTerminalInput) -> bool {
    matches!(
        input,
        TuiTerminalInput::Interaction(TuiInteraction::SelectNext | TuiInteraction::SelectPrevious)
    )
}

/// Drain a burst of ALREADY-BUFFERED terminal input in one tick, coalescing a
/// run of pure navigation keys into one state transition (and so one eventual
/// TERMINAL render) instead of redrawing between each —
/// livespec-console-beads-fabro-mx9u.9: "it queues keystrokes and does them
/// all ... you have to wait for an unresponsive queue of keystrokes to finish
/// before you can SLOWLY move to what you want." Reads from `source` rather
/// than a real terminal, so it is exercised directly in tests; the terminal
/// loop's own entry point ([`process_input_tick`]) is a thin, untestable
/// wrapper that supplies the real crossterm-backed source.
///
/// Every key is applied in the order read — verb, Enter, and Escape keys are
/// NEVER dropped or reordered (acceptance criterion 3). What is coalesced is
/// only the caller's TERMINAL frame: `drain_input_burst` keeps draining
/// without returning (and so without the caller calling `terminal.draw`) for
/// as long as the next buffered key is more navigation, up to
/// [`MAX_COALESCED_MOVEMENT_KEYS`]. The first non-navigation key — a verb, or
/// one that maps to no interaction at all — is still applied, but ends the
/// burst, so it always gets its own visible frame rather than being buried
/// inside a movement batch.
///
/// `model` is rebuilt from `projection` (cheap — see
/// [`console_application::TuiProjection`]) at the TOP of every loop pass,
/// reflecting `state` as of that pass. This is not an optimization to skip;
/// it is required for correctness: `select_next`/`select_previous` prefer
/// `model.selected_attention_index()` (the anchor re-resolved against the
/// model a fresh render would carry) over the state's own raw index, so
/// reducing several keys against ONE stale model — the model from before any
/// of them — would resolve every step from the BURST'S starting position
/// instead of the previous step's result.
#[cfg(any(test, not(coverage)))]
fn drain_input_burst(
    state: &mut TuiInteractionState,
    projection: &console_application::TuiProjection,
    events: &[ConsoleEvent],
    requested_by: &str,
    effects: &mut Vec<TuiRuntimeEffect>,
    session: &mut dyn TuiLiveSession,
    source: &mut dyn InputSource,
) -> io::Result<LoopTick> {
    let mut handled_any = false;
    let mut mutated = false;
    let mut coalesced = 0_usize;
    loop {
        let model = console_application::render_tui_model(projection, events, state);
        let Event::Key(key_event) = source.read()? else {
            // A non-key event (resize, mouse, paste, focus) carries nothing to
            // coalesce; stop here and let the caller redraw.
            break;
        };
        if key_event.kind != KeyEventKind::Press {
            if !source.poll_now()? {
                break;
            }
            continue;
        }
        let Some(input) = key_event_to_terminal_input(key_event, &model) else {
            if !source.poll_now()? {
                break;
            }
            continue;
        };
        let navigation = is_navigation_interaction(&input);
        let step = step_tui_runtime_with_model(state, &model, events, input, requested_by);
        *state = step.state().clone();
        handled_any = true;
        let effect = step.effect().clone();
        let should_quit = matches!(effect, TuiRuntimeEffect::Quit);
        mutated |= effect_triggers_source_poll(&effect);
        let outcome = session.handle_runtime_effect(&effect)?;
        apply_sink_outcome(state, effects, effect, outcome, events.len());
        if should_quit {
            return Ok(LoopTick::Quit);
        }
        coalesced += 1;
        if !navigation || coalesced >= MAX_COALESCED_MOVEMENT_KEYS {
            break;
        }
        if !source.poll_now()? {
            break;
        }
    }
    if !handled_any {
        return Ok(LoopTick::Idle);
    }
    Ok(if mutated {
        LoopTick::Mutated
    } else {
        LoopTick::HandledInput
    })
}

/// Wait for the first key of a tick, then hand off to [`drain_input_burst`]
/// against the real terminal. Terminal-bound (blocks on `event::poll`), so
/// excluded from tests; the burst-coalescing DECISION it delegates to is
/// exercised directly there instead.
#[cfg(all(not(test), not(coverage)))]
fn process_input_tick(
    state: &mut TuiInteractionState,
    events: &[ConsoleEvent],
    projection: &console_application::TuiProjection,
    requested_by: &str,
    effects: &mut Vec<TuiRuntimeEffect>,
    session: &mut dyn TuiLiveSession,
) -> io::Result<LoopTick> {
    if !event::poll(Duration::from_millis(250))? {
        return Ok(LoopTick::Idle);
    }
    let mut source = CrosstermInputSource;
    drain_input_burst(
        state,
        projection,
        events,
        requested_by,
        effects,
        session,
        &mut source,
    )
}

/// Fold one sink outcome into the loop's state and deferred-effect list.
///
/// Split out of the terminal-bound tick so the DECISION is testable: the tick
/// itself blocks on `event::poll` and is excluded from tests, which is how the
/// session-killing behaviour this replaces went unnoticed.
#[cfg(any(test, not(coverage)))]
fn apply_sink_outcome(
    state: &mut TuiInteractionState,
    effects: &mut Vec<TuiRuntimeEffect>,
    effect: TuiRuntimeEffect,
    outcome: TuiRuntimeEffectSinkOutcome,
    event_count: usize,
) {
    // A dispatcher-setting write that the sink APPLIED is now in flight on the
    // command worker. Record it so the row it targets renders as pending rather
    // than presenting the pre-edit value as current -- the launch-snapshot
    // defect this closes (livespec-console-beads-fabro-30c). Only an applied
    // effect counts: a deferred or store-busy one never reached the worker.
    if matches!(outcome, TuiRuntimeEffectSinkOutcome::Applied)
        && let TuiRuntimeEffect::PersistCommandWithPayload {
            command,
            payload_json,
        } = &effect
        && let Some(pending) = console_application::DispatcherSettingWriteState::submitted(
            command.command_type(),
            payload_json,
            event_count,
        )
    {
        *state = state.clone().with_dispatcher_setting_write(pending);
    }
    match outcome {
        TuiRuntimeEffectSinkOutcome::Deferred => effects.push(effect),
        TuiRuntimeEffectSinkOutcome::Applied => {}
        // The store was busy: the effect did NOT land. Say so where the operator
        // is already looking and keep the session alive, rather than killing the
        // TUI over a momentary lock wait. Mirrors how a refused action is
        // surfaced (see `unavailable_action_refusal`). The effect is NOT pushed
        // to `effects`: it was not applied and must not be flushed later as if
        // it had been.
        TuiRuntimeEffectSinkOutcome::NotApplied(reason) => {
            *state = state.clone().with_transient_status(Some(reason));
        }
    }
}

/// Fold an out-of-band worker status into the loop's state.
///
/// Split out of the terminal-bound tick for exactly the reason `apply_sink_outcome`
/// was: the loop around it is excluded from tests and coverage, and a
/// honesty-carrying decision that nothing measures is how the silent-drop family
/// got here in the first place (livespec-console-beads-fabro-zbnnlv).
///
/// It reuses the SAME transient-status surface a store-busy `NotApplied` and a
/// refused action already use. There is no second channel to learn: whatever
/// most recently contradicted the operator's expectation is what the header
/// says. `None` — the overwhelmingly common case, one per render tick — leaves
/// the state untouched rather than clearing a status the operator may not have
/// read yet.
#[cfg(any(test, not(coverage)))]
fn apply_worker_status(state: &mut TuiInteractionState, status: Option<String>) {
    if let Some(status) = status {
        *state = state.clone().with_transient_status(Some(status));
    }
}

/// Fold a freshly re-probed [`BuildStaleness`] into the loop's state.
///
/// Split out of the terminal-bound loop for the same reason `apply_worker_status`
/// was: the loop around it is excluded from tests and coverage. `None` means
/// the session behind `TuiLiveSession` has no live probe at all (the legacy
/// `run_interactive_tui` entry point, and every test double that does not
/// override the trait's default) -- in that case the state keeps whatever
/// [`TuiInteractionState::with_build_staleness`] was seeded with at launch and
/// this is a no-op, exactly like a worker status nobody sent.
/// `livespec-console-beads-fabro-mx9u.26`.
#[cfg(any(test, not(coverage)))]
fn apply_build_staleness(state: &mut TuiInteractionState, fresh: Option<BuildStaleness>) {
    if let Some(staleness) = fresh {
        *state = state.clone().with_build_staleness(staleness);
    }
}

/// Fold the session's current startup-ingest status into the loop's state.
///
/// Split out for the same reason `apply_build_staleness` is: the loop around
/// it is excluded from tests and coverage. Unlike that one this reads
/// UNCONDITIONALLY every tick rather than gating on `Some` -- every
/// [`TuiLiveSession`] has an opinion (`first_ingest_in_progress` defaults
/// `false`), so there is no "session has nothing to say" case to skip, and the
/// read is a cheap atomic load behind the real session
/// (livespec-console-beads-fabro-pzbdbo.27).
#[cfg(any(test, not(coverage)))]
fn apply_startup_ingest_pending(state: &mut TuiInteractionState, pending: bool) {
    *state = state.clone().with_startup_ingest_pending(pending);
}

/// Fold a freshly observed [`WriterLeaseStatus`] into the loop's state.
///
/// Mirrors [`apply_build_staleness`] exactly: `None` means the session has no
/// live poller behind it (the legacy entry point, and every test double that
/// does not override the trait's default), in which case the state keeps
/// whatever it was seeded with and this is a no-op
/// (`livespec-console-beads-fabro-mx9u.23` AC3).
#[cfg(any(test, not(coverage)))]
fn apply_writer_lease_status(state: &mut TuiInteractionState, fresh: Option<WriterLeaseStatus>) {
    if let Some(status) = fresh {
        *state = state.clone().with_writer_lease_status(status);
    }
}

/// Fold a fresh effective-policy read into the loop's state.
///
/// Split out of the terminal-bound loop for the same reason `apply_sink_outcome`
/// and `apply_worker_status` were: the loop is excluded from tests and coverage,
/// and this is where the console decides whether the operator's edit LANDED.
/// The read replaces what every row renders; the fold decides whether the row
/// the operator edited now reports the new value, or reports that it did not
/// move.
#[cfg(any(test, not(coverage)))]
fn apply_dispatcher_settings_reread(
    state: &mut TuiInteractionState,
    fresh: DispatcherSettingsRead,
) {
    let folded = console_application::fold_dispatcher_setting_reread(
        state.dispatcher_setting_write(),
        &fresh,
    );
    *state = state
        .clone()
        .with_dispatcher_setting_write(folded)
        .with_dispatcher_settings(fresh);
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
    /// Copy driver handoff command variant. This is render/copy only: it never
    /// becomes a persisted console command and never triggers a source poll.
    CopyDriverHandoff(String),
    /// Quit variant.
    Quit,
    /// Application error variant.
    ApplicationError(ApplicationError),
}

// NOT `Copy`: `NotApplied` carries the operator-facing reason, and the reason
// is the point — a bare marker would tell the operator their action vanished
// without saying why.
#[derive(Debug, Clone, PartialEq, Eq)]
/// Outcome from handling one TUI runtime effect.
pub enum TuiRuntimeEffectSinkOutcome {
    /// The sink applied the effect immediately; callers must not flush it again.
    Applied,
    /// The sink deferred the effect; callers should return it for later handling.
    Deferred,
    /// The effect was NOT applied because the store was transiently contended,
    /// carrying the operator-facing reason.
    ///
    /// The session MUST survive this: a momentary lock wait is not a reason to
    /// destroy the operator's session. The caller surfaces the reason and keeps
    /// running. It is deliberately NOT an `Err` — a real fault still is, and
    /// still carries its cause.
    NotApplied(String),
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

    /// Take the next OUT-OF-BAND worker status the operator has not seen yet.
    ///
    /// The operator's mutating commands execute on a worker thread, so by the
    /// time one of them fails the valve has already confirmed and the modal has
    /// already closed. Without this the session had no way to be told, and a
    /// dropped command was indistinguishable from a completed one
    /// (livespec-console-beads-fabro-zbnnlv).
    ///
    /// Non-blocking: it returns what has already arrived and never waits, so the
    /// render loop's tick is unaffected. Defaults to `None` for sessions with no
    /// worker behind them.
    fn take_worker_status(&mut self) -> Option<String> {
        None
    }

    /// Read the latest background-probed [`BuildStaleness`], if this session
    /// has a live probe behind it.
    ///
    /// `livespec-console-beads-fabro-mx9u.26`: staleness is a function of the
    /// build sha (fixed for the process) AND the repo's HEAD (not fixed), so a
    /// value read once at launch goes stale itself the moment HEAD moves. This
    /// is the render loop's read side of that fix -- called every tick, same
    /// as [`Self::take_worker_status`], and just as cheap: the real
    /// implementation is a non-blocking `Mutex` lock around a `Copy` enum,
    /// never a `git` shell-out. The shell-out that keeps the value fresh runs
    /// off this thread, on the background source poller's own cadence (see
    /// `console_application::build_identity::SharedBuildStaleness` for why
    /// that cadence is the right one). Defaults to `None` -- unchanged state --
    /// for the legacy `run_interactive_tui` entry point and every test double
    /// that has no probe to report.
    fn take_build_staleness(&mut self) -> Option<BuildStaleness> {
        None
    }

    /// Read the latest background-observed [`WriterLeaseStatus`], if this
    /// session has a live poller behind it.
    ///
    /// `livespec-console-beads-fabro-mx9u.23` AC3: the background poller is
    /// the only place the store's writer lease is actually contended for
    /// (`refresh_sources`), so it is also the only place that can know
    /// whether this process still holds it. Read the same way and on the
    /// same cadence as [`Self::take_build_staleness`] -- a non-blocking
    /// `Mutex` lock, never IO on this thread. Defaults to `None` -- unchanged
    /// state -- for the legacy entry point and every test double with no
    /// lease to report.
    fn take_writer_lease_status(&mut self) -> Option<WriterLeaseStatus> {
        None
    }

    /// Re-read the EFFECTIVE dispatcher policy from the orchestrator's published
    /// read surface, returning `Some(read)` to replace what the view renders or
    /// `None` when this session has no read surface behind it.
    ///
    /// Unlike [`Self::refresh_events`] this DOES shell out, which is why the
    /// loop calls it only once a submitted write's outcome has landed rather
    /// than on the render cadence. That is the whole fix: the composition root
    /// used to read the effective policy exactly once, before the TUI started,
    /// so every later frame rendered a launch-time snapshot and a landed edit
    /// looked lost (livespec-console-beads-fabro-30c).
    ///
    /// # Errors
    /// Returns an IO error when the read surface cannot be consulted at all.
    /// An orchestrator that answers untrustworthily is NOT an error -- it is
    /// `DispatcherSettingsRead::NotObserved`, the named finding the view already
    /// renders.
    fn refresh_dispatcher_settings(&mut self) -> std::io::Result<Option<DispatcherSettingsRead>> {
        Ok(None)
    }

    /// Whether the session's FIRST background source ingest is still in
    /// flight (livespec-console-beads-fabro-pzbdbo.27).
    ///
    /// The interactive launch used to run that first ingest SYNCHRONOUSLY
    /// before drawing anything at all, which is exactly what left the
    /// operator staring at a blank pane for 30-60+ seconds. Now the first
    /// frame draws from whatever the store already holds and the ingest runs
    /// on the background poller instead; this is how the terminal loop finds
    /// out whether that first sweep has landed yet, so it can seed and clear
    /// [`console_application::TuiInteractionState::with_startup_ingest_pending`]
    /// honestly rather than presenting a not-yet-confirmed screen as current.
    ///
    /// A cheap, non-blocking read (an atomic flag behind the real session),
    /// checked every tick exactly like [`Self::take_worker_status`]. Sessions
    /// with no background poller behind them -- every test double, and the
    /// legacy no-store path -- default to `false`: already caught up, nothing
    /// to wait for.
    fn first_ingest_in_progress(&self) -> bool {
        false
    }
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
///
/// Builds its own model from `events` and `state` before stepping -- the
/// convenience form for a caller with no already-built model to reuse (most
/// tests, and any one-shot caller). The terminal loop's hot path calls
/// [`step_tui_runtime_with_model`] directly against its cached model instead;
/// see that function and [`console_application::TuiProjection`] for why.
pub fn step_tui_runtime(
    state: &TuiInteractionState,
    events: &[ConsoleEvent],
    input: TuiTerminalInput,
    requested_by: &str,
) -> TuiRuntimeStep {
    let model = build_tui_model_for_state(events, state);
    step_tui_runtime_with_model(state, &model, events, input, requested_by)
}

#[must_use]
/// The cheap half of [`step_tui_runtime`].
///
/// Steps `input` against an ALREADY-BUILT `model` rather than deriving one
/// from the raw event log (which, for the `Interaction` arm, used to mean
/// `reduce_tui_interaction` building yet ANOTHER copy internally — see
/// [`console_application::reduce_tui_interaction_with_model`]).
pub fn step_tui_runtime_with_model(
    state: &TuiInteractionState,
    model: &TuiScreenModel,
    events: &[ConsoleEvent],
    input: TuiTerminalInput,
    requested_by: &str,
) -> TuiRuntimeStep {
    match input {
        TuiTerminalInput::Interaction(interaction) => TuiRuntimeStep::new(
            console_application::reduce_tui_interaction_with_model(state, model, interaction),
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
    _requested_by: &str,
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
            TuiInteraction::OpenFactoryDrainConfirm
        }
        Some(action_registry::StagedAction::FactoryDispatchItem) => {
            TuiInteraction::OpenFactoryDispatchItemConfirm
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
    _requested_by: &str,
) -> TuiRuntimeStep {
    let selected_spec = action_registry::menu_actions(top).get(selected).copied();
    let staged = selected_spec.and_then(|spec| staged_without_selection(model, spec));
    let interaction = match staged {
        Some(action_registry::StagedAction::Valve(valve)) => {
            TuiInteraction::OpenValveConfirm(valve)
        }
        Some(action_registry::StagedAction::DriverHandoff) => TuiInteraction::OpenDriverHandoff,
        Some(action_registry::StagedAction::FactoryDrain) => {
            TuiInteraction::OpenFactoryDrainConfirm
        }
        Some(action_registry::StagedAction::FactoryDispatchItem) => {
            TuiInteraction::OpenFactoryDispatchItemConfirm
        }
        Some(action_registry::StagedAction::Global(action)) => match global_interaction(action) {
            Some(global) => global,
            None => return TuiRuntimeStep::new(state.clone(), TuiRuntimeEffect::Quit),
        },
        None => {
            let next_state = selected_spec.map_or_else(
                || state.clone(),
                |spec| {
                    state
                        .clone()
                        .with_transient_status(Some(unavailable_action_refusal(spec)))
                },
            );
            return TuiRuntimeStep::new(
                next_state,
                TuiRuntimeEffect::ApplicationError(ApplicationError::UnavailableOperatorAction),
            );
        }
    };
    TuiRuntimeStep::new(
        reduce_tui_interaction(state, events, interaction),
        TuiRuntimeEffect::Render,
    )
}

fn unavailable_action_refusal(spec: &action_registry::ActionSpec) -> String {
    format!("{} unavailable: {}", spec.label, spec.availability_summary)
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
    if let TuiOverlay::CommandModal { .. } = model.overlay() {
        return TuiRuntimeStep::new(
            reduce_tui_interaction(state, events, TuiInteraction::OpenCommandExplainer),
            TuiRuntimeEffect::Render,
        );
    }
    if let TuiOverlay::CommandExplainer {
        selected_action_index,
    } = model.overlay()
    {
        return command_explainer_confirm_step(
            state,
            events,
            &model,
            *selected_action_index,
            requested_by,
        );
    }
    // A menu item stages through the SAME path as its hotkey and the invoker
    // row. Anything else would be a third invocation route for one action.
    if let TuiOverlay::Menu { top, selected } = model.overlay() {
        return menu_confirm_step(state, events, &model, *top, *selected, requested_by);
    }
    let outcome = match model.overlay() {
        TuiOverlay::CommandPalette { .. } => resolve_command_palette_action(&model, requested_by),
        TuiOverlay::FactoryDrainConfirm { .. } => Ok(OperatorActionOutcome::PersistCommand(
            console_application::factory_drain_command(requested_by),
        )),
        TuiOverlay::FactoryDispatchItemConfirm { work_item_id } => {
            Ok(OperatorActionOutcome::PersistCommand(
                console_application::factory_dispatch_item_command(work_item_id, requested_by),
            ))
        }
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
        | TuiOverlay::CommandExplainer { .. }
        // The invoker confirm returned above; this arm is unreachable for it.
        | TuiOverlay::ActionInvoker { .. }
        // The work-item detail modal is READ-ONLY: `enter_input` yields no
        // `Confirm` while it is open, so it never actually reaches here.
        | TuiOverlay::WorkItemDetail { .. }
        // The menu confirm returned above; this arm is unreachable for it.
        | TuiOverlay::Menu { .. }
        // Nothing to confirm on any of these: each is read-only or has already
        // returned above. They used to fall through to a resolver whose only
        // resolvable actions were the two Fabro-attach ones, and those are gone
        // with the handoff they served, so the arm now refuses directly --
        // still validating the requester first, so a blank one is reported as
        // the bad request it is rather than absorbed into the refusal.
        | TuiOverlay::Help { .. } => validate_operator_action(requested_by)
            .and(Err(ApplicationError::UnavailableOperatorAction)),
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
        OperatorActionOutcome::CopyDriverHandoff(command) => {
            TuiRuntimeEffect::CopyDriverHandoff(command)
        }
    }
}

fn command_explainer_confirm_step(
    state: &TuiInteractionState,
    events: &[ConsoleEvent],
    model: &TuiScreenModel,
    selected_action_index: usize,
    requested_by: &str,
) -> TuiRuntimeStep {
    let action = model
        .detail()
        .and_then(|detail| detail.actions().get(selected_action_index))
        .copied();
    let Some(action) = action else {
        return TuiRuntimeStep::new(
            state.clone(),
            TuiRuntimeEffect::ApplicationError(ApplicationError::NoSelectedOperatorAction),
        );
    };
    match action {
        OperatorAction::Registered(id) => {
            let staged = action_registry::action_for_id(id)
                .and_then(|spec| staged_without_selection(model, spec));
            staged_action_step(state, events, staged, requested_by)
        }
    }
}

fn staged_action_step(
    state: &TuiInteractionState,
    events: &[ConsoleEvent],
    staged: Option<action_registry::StagedAction>,
    _requested_by: &str,
) -> TuiRuntimeStep {
    let interaction = match staged {
        Some(action_registry::StagedAction::Valve(valve)) => {
            TuiInteraction::OpenValveConfirm(valve)
        }
        Some(action_registry::StagedAction::DriverHandoff) => TuiInteraction::OpenDriverHandoff,
        Some(action_registry::StagedAction::FactoryDrain) => {
            TuiInteraction::OpenFactoryDrainConfirm
        }
        Some(action_registry::StagedAction::FactoryDispatchItem) => {
            TuiInteraction::OpenFactoryDispatchItemConfirm
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

/// The interaction a global registry action reduces to, or `None` for the one
/// global that is not an interaction at all: quitting ends the session rather
/// than transforming its state.
///
/// Split out from [`global_input`] so the invoker roster can branch on
/// "interaction or quit" without carrying a `TuiTerminalInput::Confirm` arm
/// that nothing can reach.
const fn global_interaction(action: action_registry::GlobalAction) -> Option<TuiInteraction> {
    match action {
        action_registry::GlobalAction::GoToView(view) => Some(TuiInteraction::SelectView(view)),
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
        TuiOverlay::CommandExplainer { .. }
        | TuiOverlay::DriverHandoff { .. }
        | TuiOverlay::FactoryDrainConfirm { .. }
        | TuiOverlay::FactoryDispatchItemConfirm { .. } => return None,
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
        TuiOverlay::CommandExplainer { .. }
        | TuiOverlay::DriverHandoff { .. }
        | TuiOverlay::FactoryDrainConfirm { .. }
        | TuiOverlay::FactoryDispatchItemConfirm { .. } => return None,
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
        | TuiOverlay::CommandExplainer { .. }
        | TuiOverlay::CommandPalette { .. }
        | TuiOverlay::ActionInvoker { .. }
        | TuiOverlay::FactoryDrainConfirm { .. }
        | TuiOverlay::FactoryDispatchItemConfirm { .. }
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
            if model.active_view() == TuiView::Events {
                return match model.events_focus() {
                    EventsFocus::Overview => Some(TuiTerminalInput::Interaction(
                        TuiInteraction::DrillIntoEventsSubView,
                    )),
                    // Neither sub-view has a per-row action surface yet (that is
                    // deferred, not excluded -- see the epic's constraint), so
                    // Enter is inert once drilled in.
                    EventsFocus::StoredEvents | EventsFocus::EventSources => None,
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
/// returns a drilled-in lane (or a drilled-in `Events` sub-view) to its
/// overview (else focus to the Views nav); on the nav (leftmost) it is the
/// inert close-overlay.
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
/// lane or `Events` sub-view returns to its own overview first, otherwise
/// focus returns to the Views nav.
fn content_back_interaction(model: &TuiScreenModel) -> TuiInteraction {
    if model.active_view() == TuiView::Lanes && matches!(model.lane_focus(), LaneFocus::Lane(_lane))
    {
        return TuiInteraction::ReturnToLaneOverview;
    }
    if model.active_view() == TuiView::Events && model.events_focus() != EventsFocus::Overview {
        return TuiInteraction::ReturnToEventsOverview;
    }
    TuiInteraction::FocusNav
}

/// Left: inside Help it focuses the section menu; inside Menu it walks the
/// top-level nodes; behind any other overlay it is inert. With no overlay open,
/// it normally walks focus one pane toward the nav, then enters the visible menu
/// bar from the resting left edge so the bar is reachable without its registry
/// hotkey. A drilled-in lane is already at the Lanes view's left content edge,
/// so Left opens the bar in place instead of discarding the item selection.
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
        FocusPane::Content
            if model.active_view() == TuiView::Lanes
                && matches!(model.lane_focus(), LaneFocus::Lane(_lane)) =>
        {
            TuiInteraction::OpenMenu
        }
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
        | TuiOverlay::CommandExplainer { .. }
        | TuiOverlay::ActionInvoker { .. }
        | TuiOverlay::Menu { .. }
        | TuiOverlay::FactoryDrainConfirm { .. }
        | TuiOverlay::FactoryDispatchItemConfirm { .. }
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
        // The ranked drain is keyless by registry declaration: no chord
        // resolves to `dispatch-ready`, so the key path has nothing to offer
        // it and the menu, palette and invoker remain its only routes.
        action_registry::StagedAction::FactoryDrain => None,
        // The per-item dispatch DOES carry a key (v047 gap-uqotpmdo), and it
        // must open the SAME confirmation the `Factory > Dispatch` row opens
        // rather than a second staging path — which is why this reduces to the
        // interaction `menu_confirm_step` and `invoker_confirm_step` reduce to.
        action_registry::StagedAction::FactoryDispatchItem => Some(TuiTerminalInput::Interaction(
            TuiInteraction::OpenFactoryDispatchItemConfirm,
        )),
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
    // The valve-confirm modal takes typed characters ONLY where it offers the
    // free-text answer field (the resolve-blocked dialog); on every other valve
    // a character stays inert, as it always was.
    let takes_text = match overlay {
        TuiOverlay::Search { .. } | TuiOverlay::CommandPalette { .. } => true,
        TuiOverlay::ValveConfirm { valve, .. } => valve.accepts_answer(),
        _other => false,
    };
    if takes_text {
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
    let body_area = vertical[2];
    let detail_max_scroll = render_body(model, body_area, buffer);
    render_footer(model, vertical[3], buffer);
    let menu_area = Rect::new(
        area.x,
        vertical[1].y,
        area.width,
        vertical[1].height.saturating_add(body_area.height),
    );
    let overlay_extents = render_overlay(
        model,
        OverlayAreas {
            screen: area,
            menu: menu_area,
            body: body_area,
        },
        buffer,
    );
    RenderScrollExtents {
        detail_max_scroll,
        header_max_scroll,
        work_item_detail_max_scroll: overlay_extents.work_item_detail.max_scroll,
        work_item_detail_page_rows: overlay_extents.work_item_detail.page_rows,
        help_max_scroll: overlay_extents.help.max_scroll,
        help_page_rows: overlay_extents.help.page_rows,
    }
}

/// The Header pane's block title: `LiveSpec Console`, the `[focus]` marker
/// when focused, and the running build's identity when one is known.
///
/// livespec-console-beads-fabro-mx9u.13: the identity lives HERE, in the
/// title, rather than as a content-line field. Measured against the
/// maintainer's own real, busy 159-column header (a factory alert AND
/// several unavailable sources beside a two-digit attention count): even a
/// sha-only short form of the identity had no room left in the content line
/// once every genuinely-content field had its say, and an operator checking
/// which build is running is exactly as likely to do it during a busy
/// moment as a quiet one. The title costs nothing from that content-line
/// budget and is unconditionally visible, on every pane draw, focused or
/// blurred alike -- so it is the one place the identity is GUARANTEED to
/// show. The stale-build tell stays a content-line field (`TransientState`
/// priority, alongside `factory:`/`status:`): it is a live anomaly the
/// existing shed ladder already protects, not an identity fact.
///
/// The `[focus]` marker is applied to the BARE name FIRST, and the (longer,
/// unbounded-length) build segment appended after -- not the other way
/// round. `ratatui` truncates a title that overflows the border from the
/// RIGHT, so if the marker sat after the build segment a merely-narrow
/// viewport could truncate it away entirely, and the operator would lose
/// the one signal that says which pane `up`/`down` currently drive. Measured
/// while dogfooding this fix at `NARROW_COLS` (56): with the marker last,
/// the captured title read `LiveSpec Console — build e893bf2 (built
/// 2026-09-09T00:` with `[focus]` gone.
fn header_pane_title(model: &TuiScreenModel, focused: bool) -> String {
    let base = focus_title("LiveSpec Console", focused);
    model.build_identity().map_or_else(
        || base.clone(),
        |identity| format!("{base} — {}", build_identity_segment(identity)),
    )
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
/// left/right; its block title (see [`header_pane_title`]) carries the
/// `[focus]` marker every other focused pane uses.
fn render_header(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) -> usize {
    let inner_width = usize::from(area.width.saturating_sub(2));
    let focused = model.focus() == FocusPane::Header;
    let title = header_pane_title(model, focused);
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
        | TuiOverlay::CommandExplainer { .. }
        | TuiOverlay::ActionInvoker { .. }
        | TuiOverlay::FactoryDrainConfirm { .. }
        | TuiOverlay::FactoryDispatchItemConfirm { .. }
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

/// The `Views` navigation pane's fixed width, and so the column the CONTENT
/// pane begins at in every view. Named because the Search overlay anchors to
/// that edge ([`search_overlay_rect`]) and must not drift from the layout it is
/// aligning with.
const NAVIGATION_PANE_WIDTH: u16 = 18;

/// The Search overlay's content rows: the query input, and the `N of M match`
/// feedback beneath it.
const SEARCH_OVERLAY_ROWS: u16 = 2;

/// Render the body panes and return the Detail pane's maximum scroll offset
/// (`0` for the Lanes view, which has no Detail pane).
fn render_body(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) -> usize {
    // The Lanes view spans the full body width beside the nav; the attention
    // and summary views keep the list/detail split.
    if model.active_view() == TuiView::Lanes {
        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(NAVIGATION_PANE_WIDTH),
                Constraint::Min(3),
            ])
            .split(area);
        render_navigation(model, horizontal[0], buffer);
        render_lanes(model, horizontal[1], buffer);
        return 0;
    }
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(NAVIGATION_PANE_WIDTH),
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
    items.extend(orphaned_factory_runs_lane_rows(model));
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
        let selection_notice = model.missing_selected_lane_item_id().map_or_else(
            || "No work-items in this lane".to_owned(),
            |work_item_id| format!("{work_item_id} is no longer in this lane; selection not moved"),
        );
        Paragraph::new(vec![Line::from(selection_notice)])
            .block(block)
            .render(area, buffer);
        return;
    }
    let selected = model.selected_lane_item_index();
    let mut list_items = Vec::new();
    if let Some(work_item_id) = model.missing_selected_lane_item_id() {
        list_items.push(ListItem::new(format!(
            "{work_item_id} is no longer in this lane; selection not moved"
        )));
    }
    list_items.extend(
        items
            .iter()
            .enumerate()
            .map(|(index, item)| lane_item_line(item, Some(index) == selected)),
    );
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

/// The orphaned-factory-runs lane's header label.
const ORPHANED_RUNS_LANE_LABEL: &str = "orphaned factory runs";

/// The orphaned-factory-runs lane, rendered below the seven lifecycle lanes on
/// the lane-overview home.
///
/// It sits with the lanes rather than in a view of its own because it answers
/// the same operator question they do -- what is holding a slot -- and every
/// row is EXPANDED rather than previewed: unlike a lifecycle lane, whose top
/// items are a sample of a list the operator can drill into, an orphaned run
/// the console renders three of and silently drops the rest is a slot nobody
/// is watching. The count header renders even at zero, so "no orphans" is a
/// stated fact rather than a missing section.
fn orphaned_factory_runs_lane_rows(model: &TuiScreenModel) -> Vec<ListItem<'static>> {
    let runs = model.orphaned_factory_runs();
    let mut rows = vec![ListItem::new(Line::from(format!(
        "  {ORPHANED_RUNS_LANE_LABEL} ({})",
        runs.len()
    )))];
    rows.extend(
        runs.iter()
            .map(|run| ListItem::new(Line::from(orphaned_run_summary(run)))),
    );
    rows
}

/// One orphaned-run row, carrying every field the reconciler's projection
/// reports and nothing the console decided: run id, factory, status kind,
/// work-item id and status, orphan reason, and the remedy the orchestrator
/// prescribes under its own name.
fn orphaned_run_summary(run: &OrphanedFactoryRun) -> String {
    format!(
        "    - {} on {} [{}]  {}{}  ({})  remedy {}",
        run.run_id(),
        run.factory_name(),
        // `run_state` resolves an unreported kind to the neutral `unknown`
        // rather than to any real kind -- and never to a gate.
        run.run_state().label(),
        run.work_item_id(),
        orphaned_run_work_item_status_suffix(run),
        run.orphan_reason(),
        run.termination_route()
    )
}

/// The ` [status]` suffix for an orphaned run's work-item, or empty when the
/// ledger holds no such item.
///
/// Absent stays absent. The `item-missing` orphan reason means the work-item is
/// not in the ledger at all, so it HAS no status; rendering a placeholder in the
/// status position would show the operator a status the ledger never held, and
/// the reason field beside it already says why the slot is empty.
fn orphaned_run_work_item_status_suffix(run: &OrphanedFactoryRun) -> String {
    run.work_item_status()
        .map(|status| format!(" [{status}]"))
        .unwrap_or_default()
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
        "{}  rank {}  [{}]{}{}  {}  repo {}{}",
        item.work_item_id(),
        item.rank(),
        item.status(),
        lane_execution_state_suffix(item),
        lane_unconfirmed_suffix(item),
        lane_item_title(item),
        item.repo(),
        lane_reason_suffix(item)
    )
}

/// The marker on a row whose backing source could not be read on the latest
/// poll, or empty when the console can vouch for the row.
///
/// It sits immediately after the lifecycle fields rather than at the end of the
/// line, because a lane row TRUNCATES at the pane width and an honesty marker
/// that only survives on a wide terminal is not one
/// (`livespec-console-beads-fabro-v8un`, operator rider [2]). The row carries
/// the flag; the detail pane carries the sentence.
const fn lane_unconfirmed_suffix(item: &LaneWorkItem) -> &'static str {
    if item.observation_confirmed() {
        ""
    } else {
        "  (unconfirmed)"
    }
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

/// Draw the Status band, fitted to the room INSIDE its borders.
///
/// `Paragraph` clips whatever overruns the line silently, so handing it the raw
/// hint row let a narrow pane hide an available action with nothing on screen
/// saying so. `footer_line` sheds by declared priority and declares the count
/// instead; the band is the only place that knows its own width, so it is the
/// place that measures.
fn render_footer(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    let inner_width = usize::from(area.width.saturating_sub(2));
    Paragraph::new(model.footer_line(inner_width))
        .block(Block::new().borders(Borders::ALL).title("Status"))
        .render(area, buffer);
}

/// The rects an overlay may anchor itself to: the whole viewport, the menu bar
/// band the generated menu drops out of, and the body band whose content pane
/// the search filter aligns with.
#[derive(Clone, Copy)]
struct OverlayAreas {
    screen: Rect,
    menu: Rect,
    body: Rect,
}

fn render_overlay(
    model: &TuiScreenModel,
    areas: OverlayAreas,
    buffer: &mut Buffer,
) -> OverlayScrollExtents {
    let area = areas.screen;
    let menu_area = areas.menu;
    match model.overlay() {
        TuiOverlay::None => OverlayScrollExtents::ZERO,
        TuiOverlay::Search { query } => {
            render_search_overlay(
                query,
                model.attention_items().len(),
                model.attention_total(),
                search_overlay_rect(areas.body),
                buffer,
            );
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
        TuiOverlay::CommandExplainer {
            selected_action_index,
        } => {
            render_command_explainer(
                model.detail(),
                *selected_action_index,
                full_width_explainer_rect(area),
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
        TuiOverlay::FactoryDispatchItemConfirm { work_item_id } => {
            render_factory_dispatch_item_confirm(work_item_id, area, buffer);
            OverlayScrollExtents::ZERO
        }
        TuiOverlay::FactoryDrainConfirm { work_item_id, rank } => {
            render_factory_drain_confirm(work_item_id, rank, area, buffer);
            OverlayScrollExtents::ZERO
        }
        TuiOverlay::ValveConfirm { valve, answer } => {
            // The modal's consent target MUST read from the SAME source `Enter`
            // dispatches on (`selected_work_item_id` — the Attention detail OR the
            // drilled-in lane selection), never from `detail()` alone: in a
            // drilled-in lane the dispatch acts on the lane item, so reading the
            // Attention detail here would show a DIFFERENT (or blank) target and
            // let the operator confirm against the wrong work-item.
            render_valve_confirm(
                *valve,
                model.selected_work_item_id().unwrap_or(""),
                model.selected_work_item(),
                answer,
                area,
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
            let modal_rect = clear_modal_band(area, buffer);
            OverlayScrollExtents {
                work_item_detail: render_work_item_detail(
                    model.work_item_by_id(work_item_id),
                    work_item_id,
                    model.action_failure_for(work_item_id),
                    modal_rect,
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
        } => {
            let modal_rect = clear_modal_band(area, buffer);
            OverlayScrollExtents {
                work_item_detail: WorkItemDetailScrollExtents::ZERO,
                help: render_help_overlay(modal_rect, buffer, *focus, *selected_section, *scroll),
            }
        }
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

fn render_valve_confirm(
    valve: PendingValve,
    work_item: &str,
    selected_item: Option<&LaneWorkItem>,
    answer: &str,
    area: Rect,
    buffer: &mut Buffer,
) {
    let mut lines = vec![
        Line::from(format!("{} work-item", valve.valve_label())),
        Line::from(format!("Target: {work_item}")),
    ];
    // The staged parameter, under the caption the MODEL gives it: the move
    // valve's is the whole `<from> -> <to>` transition Enter would fire, which
    // the modal opens pre-staged and nothing else on screen names.
    if let Some(option) = valve.option_display() {
        lines.push(Line::from(format!(
            "{}: {option}  (up/down to change)",
            valve.option_caption()
        )));
    }
    // The optional free-text answer sits BESIDE the target-status choice, on the
    // resolve-blocked dialog alone. It is echoed verbatim as typed: the console
    // never edits or rewords the operator's text, here or on the wire.
    if valve.accepts_answer() {
        lines.push(Line::from(format!(
            "Answer (optional, type to edit): {answer}"
        )));
    }
    if valve.is_destructive() {
        lines.push(
            Line::from("dangerous / use with caution")
                .style(Style::new().add_modifier(Modifier::BOLD)),
        );
    }
    if set_acceptance_cannot_gate_in_flight(valve, selected_item) {
        lines.push(Line::from(
            "Notice: this policy cannot gate the run in flight.",
        ));
    }
    lines.push(Line::from("Enter to confirm | Esc to cancel"));
    render_confirm_box("Valve", lines, area, buffer);
}

fn render_factory_dispatch_item_confirm(work_item_id: &str, area: Rect, buffer: &mut Buffer) {
    render_confirm_box(
        "Factory Dispatch",
        vec![
            Line::from("Dispatch selected work-item"),
            Line::from(format!("Target: {work_item_id}")),
            Line::from("Uses Dispatcher loop --budget 1 --parallel 1 --item"),
            Line::from("Enter to dispatch | Esc to cancel"),
        ],
        area,
        buffer,
    );
}

fn render_factory_drain_confirm(work_item_id: &str, rank: &str, area: Rect, buffer: &mut Buffer) {
    render_confirm_box(
        "Factory Dispatch",
        vec![
            Line::from("Dispatch ready work"),
            Line::from(format!("Target: {work_item_id}")),
            Line::from(format!("Next drain rank: rank {rank}")),
            Line::from("Uses Dispatcher loop --budget 1 --parallel 1"),
            Line::from("Enter to dispatch | Esc to cancel"),
        ],
        area,
        buffer,
    );
}

/// Draw a confirm dialog over `area`, sized to the lines it actually carries.
///
/// A confirm box is a DIALOG: a fixed fraction of the viewport left it with a
/// tail of empty rows below its last line (four lines inside an eight-row box
/// at 24 rows, dogfooded 2026-09-08), which reads as a pane that failed to
/// paint rather than as a question waiting for an answer. Sizing to the content
/// makes the border land right under the last line.
fn render_confirm_box(title: &'static str, lines: Vec<Line<'_>>, area: Rect, buffer: &mut Buffer) {
    let box_rect = confirm_rect(area, lines.len());
    Clear.render(box_rect, buffer);
    Paragraph::new(lines)
        .block(Block::new().borders(Borders::ALL).title(title))
        .render(box_rect, buffer);
}

fn set_acceptance_cannot_gate_in_flight(
    valve: PendingValve,
    selected_item: Option<&LaneWorkItem>,
) -> bool {
    matches!(valve, PendingValve::SetAcceptance(_))
        && selected_item.is_some_and(|item| {
            matches!(
                item.execution_state(),
                LaneExecutionState::Claimed | LaneExecutionState::Executing
            )
        })
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
                item.observation_confirmed(),
            ),
        ),
        (
            "acceptance_policy",
            policy_field(
                detail.acceptance_policy.as_deref(),
                item.acceptance_policy().label(),
                item.observation_confirmed(),
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
///
/// UNKNOWN IS NOT UNSET, and an absent policy means different things on either
/// side of that line (`livespec-console-beads-fabro-v8un`, operator rider [2]).
/// When the backing source WAS read and emitted nothing, the field is genuinely
/// unset and the labelled fallback above is the honest reading. When the source
/// could NOT be read, the console has no reading at all -- and printing its own
/// default there would be exactly the fabrication the port discipline forbids
/// ("never emit a success or outcome event for an effect the port did not
/// actually achieve"). Measured consequence: plan 02's R10 walk recorded a
/// blank policy on an unreadable row as a VERIFIED UNARMED baseline. It was not
/// evidence of anything, and nothing on screen said so.
fn policy_field(
    emitted: Option<&str>,
    console_default: &str,
    observation_confirmed: bool,
) -> String {
    match (emitted, observation_confirmed) {
        (Some(policy), _) => policy.to_owned(),
        (None, true) => {
            format!("{ITEM_FIELD_ABSENT} (not emitted; console assumes {console_default})")
        }
        (None, false) => format!(
            "{ITEM_FIELD_ABSENT} (not read; the orchestrator source was not observed on the latest poll)"
        ),
    }
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

/// Clear the full-width band of rows a near-full-screen modal covers, and return
/// the modal's own inset rect.
///
/// A modal's own `Clear` reaches only [`help_overlay_rect`], so the
/// [`HELP_MODAL_MARGIN`] columns BESIDE it kept whatever the main screen had
/// already drawn there — the menu bar's `Men`, the Views pane's `┌Vi`, the Detail
/// pane's `──┐` — which reads as the modal's border bleeding rather than as the
/// frame the TUI Contract asks for (dogfooded 2026-09-08 at 200x55 and 120x40).
/// Clearing the whole row band first makes that frame genuinely blank.
///
/// The band spans the modal's ROWS only, so the Header above it and the Status
/// line below it stay on screen and tmux-capturable exactly as before — see the
/// bottom-band note in [`render_model`].
fn clear_modal_band(area: Rect, buffer: &mut Buffer) -> Rect {
    let modal = help_overlay_rect(area);
    Clear.render(Rect::new(area.x, modal.y, area.width, modal.height), buffer);
    modal
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
///
/// Defines the console's core vocabulary (livespec-console-beads-fabro-mx9u.19,
/// extending a maintainer ruling of 2026-09-09): what an EVENT SOURCE is, what
/// it means for one to be unavailable, and that the tally is latest-state, not
/// historical. The event-source roster is derived from
/// [`console_application::source_adapters::event_source_roster_help_lines`],
/// itself folded over [`console_application::source_adapters::SourceAdapterKind::all`]
/// -- the same enum the header's own tally and `doctor` read -- so an added
/// source cannot silently drop out of what Help tells the operator.
fn header_help_lines() -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from("Header -- the top status line: fleet / mode / repo / view / attention,"),
        Line::from("plus an event-source health tell when a backing source is down."),
        Line::from(""),
        Line::from("The console holds no truth of its own. Each poll cycle it shells out to"),
        Line::from("external programs -- EVENT SOURCES -- and records what they say as"),
        Line::from("events; every screen the console renders is built from that recorded"),
        Line::from("log, never from a fresh read of anything else. An event source is one"),
        Line::from("of those external programs."),
        Line::from(""),
        Line::from("`event sources: N unavailable` means N of those programs did not answer"),
        Line::from("on the LATEST poll cycle, so anything the console derives from them is"),
        Line::from("STALE, not current -- treat it as a snapshot from before the source"),
        Line::from("went quiet, not as a live read. This is a LATEST-STATE tally, not a"),
        Line::from("history: a source clears from it the moment a LATER poll observes it"),
        Line::from("successfully, even if that poll's own data dedupes away and nothing"),
        Line::from("else on screen moves -- so \"unavailable right now\" and \"broke once"),
        Line::from("earlier today\" are never the same reading."),
        Line::from(""),
        Line::from("The event sources, and what each observes:"),
    ];
    lines.extend(event_source_roster_help_lines().into_iter().map(Line::from));
    lines.extend(vec![
        Line::from(""),
        Line::from("The pane's OWN title, `LiveSpec Console — build <sha> (built"),
        Line::from("<timestamp>)`, names the RUNNING BINARY's compiled-in commit --"),
        Line::from("always visible, unlike a status-line field. A"),
        Line::from("`build STALE: N commits behind` tell appears once it trails HEAD."),
        Line::from(""),
        Line::from("On a narrow viewport the blurred header shrinks to fit, dropping its"),
        Line::from("low-value fields; focus it to read the FULL line and scroll it sideways."),
        Line::from(""),
        Line::from("tab / shift-tab  cycle focus onto (and off) the header, like any pane"),
        Line::from("left / right     scroll the focused header horizontally to reveal"),
        Line::from("                 content clipped at the current width"),
        Line::from("esc              leave the header (returns to the Views nav)"),
        Line::from("On blur the header snaps back to its left-justified default."),
    ]);
    lines
}

/// The `Global actions` section: the navigation and command keys available from
/// every view, plus how the modal Help itself is navigated.
fn global_help_lines() -> Vec<Line<'static>> {
    action_registry::global_help_reference_lines()
        .into_iter()
        .map(Line::from)
        .collect()
}

/// What invoking this action's ACCELERATOR actually opens, derived from its
/// registry `staging`.
///
/// The Help modal used to infer this from `spec.parameter.is_none()` and print
/// "(confirm modal)" for every payload-free action. Parameterless is not the
/// same as confirm-modal, and the result was a help screen that asserted
/// something false about five entries: `?` opens the Help overlay, `/` a Search
/// panel, `:` the command palette, `v` the menu bar, and `q` quits outright.
/// None of them confirms anything.
///
/// That is the SECOND-ENCODING defect the registry exists to retire, one level
/// down: the roster was derived from the registry while the DESCRIPTION beside
/// each entry was guessed from an unrelated field. `staging` already carries the
/// answer — `global_interaction` maps each global straight onto the overlay it
/// opens — so this reads it instead of guessing.
///
/// Found by dogfooding the real TUI (livespec-console-beads-fabro-ddfbcx), and
/// the `/` case was confirmed by pressing it: the pane that opens is titled
/// "Search".
///
/// SCOPE NOTE, deliberately conservative: the three non-global, non-handoff
/// stagings keep "confirm modal". Their accelerator-path behaviour was NOT
/// independently verified here — they are menu-routed rather than hotkeyed —
/// so they are left reading exactly as they did rather than restated on an
/// unchecked assumption.
const fn help_outcome(spec: &action_registry::ActionSpec) -> &'static str {
    match spec.staging {
        action_registry::ActionStaging::DriverHandoff => "driver-handoff overlay",
        action_registry::ActionStaging::Global(action) => match action {
            action_registry::GlobalAction::GoToView(_view) => "that view",
            action_registry::GlobalAction::OpenSearch => "search overlay",
            action_registry::GlobalAction::OpenCommandPalette => "command palette",
            action_registry::GlobalAction::OpenHelp => "help overlay",
            action_registry::GlobalAction::OpenMenu => "menu bar",
            action_registry::GlobalAction::Quit => "quits at once",
        },
        action_registry::ActionStaging::Valve(_)
        | action_registry::ActionStaging::FactoryDrain
        | action_registry::ActionStaging::FactoryDispatchItem => "confirm modal",
    }
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
                || format!("{} ({})", spec.label, help_outcome(spec)),
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
            Line::from("Events -- a container for two sub-views: Stored events (the observed"),
            Line::from("source events, read-only, exactly as before) and Event sources (the"),
            Line::from("per-source roster)."),
            Line::from(""),
            Line::from("up / down    move the sub-view selection; inside a drilled-in"),
            Line::from("             sub-view, scroll the Detail pane"),
            Line::from("enter        drill into the selected sub-view"),
            Line::from("esc          return a drilled-in sub-view to the container list"),
            Line::from("left / right move focus; left from Views opens the menu bar"),
        ],
        TuiView::Repos => vec![
            Line::from("Repos -- the fleet repo roster (read-only): the repos the console"),
            Line::from("observes, with the selected repo's detail on the right."),
            Line::from(""),
            Line::from("\"Repos observed: N\" is NOT a configured roster -- it is the count of"),
            Line::from("distinct repos the EVENT LOG carries events for, derived from each"),
            Line::from("event's stream key. A repo the console has never logged an event for"),
            Line::from("is not counted, however real it is; this is a different axis from the"),
            Line::from("header's `event sources: N unavailable` (which counts external"),
            Line::from("programs polled, not repos mentioned in the log)."),
            Line::from(""),
            Line::from("The companion rows exist so that count can never quietly mislead:"),
            Line::from("\"Fleet-scoped events\" is events attributed to the whole fleet rather"),
            Line::from("than one repo; \"Events with no derivable repo\" is events whose stream"),
            Line::from("key carries no repo at all. Neither row appears when it would be zero."),
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

fn render_command_explainer(
    detail: Option<&AttentionDetail>,
    selected_action_index: usize,
    area: Rect,
    buffer: &mut Buffer,
) {
    let Some(action) = detail
        .and_then(|detail| detail.actions().get(selected_action_index))
        .copied()
    else {
        return;
    };
    Clear.render(area, buffer);
    Paragraph::new(command_explainer_lines(action))
        .block(
            Block::new()
                .borders(Borders::ALL)
                .title("Command Explainer"),
        )
        .render(area, buffer);
}

fn command_explainer_lines(action: OperatorAction) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(action.label().to_owned()),
        Line::from(command_line_for_action(action)),
        Line::from(""),
    ];
    lines.extend(command_explanation_for_action(action));
    lines.push(Line::from(""));
    lines.push(Line::from(
        "Enter continues through the normal console action path; Esc cancels.",
    ));
    lines
}

fn command_line_for_action(action: OperatorAction) -> String {
    match action {
        OperatorAction::Registered(id) => format!("registry action: {id}"),
    }
}

fn command_explanation_for_action(action: OperatorAction) -> Vec<Line<'static>> {
    match action {
        OperatorAction::Registered(id) => action_registry::action_for_id(id).map_or_else(
            || {
                vec![Line::from(
                    "The registry entry for this action is not available.",
                )]
            },
            registry_command_explanation,
        ),
    }
}

fn registry_command_explanation(spec: &'static action_registry::ActionSpec) -> Vec<Line<'static>> {
    vec![
        Line::from(format!("Registry id: {}", spec.id)),
        Line::from(format!("Menu path: {}", spec.menu_path.join(" > "))),
        Line::from(format!(
            "Accelerator: {}",
            action_registry::accelerator_display(spec)
        )),
        Line::from(registry_staging_explanation(spec)),
    ]
}

fn registry_staging_explanation(spec: &'static action_registry::ActionSpec) -> String {
    match spec.staging {
        action_registry::ActionStaging::Valve(_stager) => {
            let parameter = spec.parameter.map_or_else(
                || "no payload".to_owned(),
                |parameter| format!("{}: {}", parameter.name, parameter.choices.join(", ")),
            );
            format!("Continuing stages the registry valve-confirm flow ({parameter}).")
        }
        action_registry::ActionStaging::DriverHandoff => {
            "Continuing opens the driver-handoff overlay for the selected work-item.".to_owned()
        }
        action_registry::ActionStaging::FactoryDrain => {
            "Continuing persists the factory-drain command for ready work.".to_owned()
        }
        action_registry::ActionStaging::FactoryDispatchItem => {
            "Continuing opens selected-item factory dispatch confirmation.".to_owned()
        }
        action_registry::ActionStaging::Global(_action) => {
            "Continuing invokes the registered global console action.".to_owned()
        }
    }
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

/// A confirm dialog's rect: the overlay's usual centred three-quarter width,
/// but exactly as tall as `content_lines` plus its two border rows, clamped to
/// the viewport so a tiny terminal still gets a drawable box.
fn confirm_rect(area: Rect, content_lines: usize) -> Rect {
    let width = (area.width.saturating_mul(3) / 4).max(1);
    let wanted = u16::try_from(content_lines)
        .unwrap_or(u16::MAX)
        .saturating_add(2);
    let height = wanted.min(area.height).max(1);
    Rect::new(
        area.x + ((area.width - width) / 2),
        area.y + (area.height.saturating_sub(height) / 2),
        width,
        height,
    )
}

/// The Search overlay's rect: anchored to the CONTENT pane's left edge, running
/// to the viewport's right edge, and exactly [`SEARCH_OVERLAY_ROWS`] rows plus
/// its two borders tall.
///
/// Both halves answer the same dogfooded complaint. The old centred
/// three-quarter box opened seven columns INTO the content pane, so a column of
/// half-cut list rows (`propo`, `Resol`, `Adopt`, `Repai`) stayed visible
/// beside it and the region read as one that had failed to repaint; and it
/// spanned a third of the viewport under a single input line, so a one-line
/// filter looked like a pane with its contents missing. Starting at the content
/// pane's edge and running to the right edge means every row the overlay
/// occupies is fully the overlay's, with the navigation pane -- a different
/// pane, legitimately still on screen -- untouched to its left.
fn search_overlay_rect(body: Rect) -> Rect {
    let left = body
        .x
        .saturating_add(NAVIGATION_PANE_WIDTH)
        .min(body.right().saturating_sub(1));
    Rect::new(
        left,
        body.y,
        body.right().saturating_sub(left).max(1),
        SEARCH_OVERLAY_ROWS
            .saturating_add(2)
            .min(body.height)
            .max(1),
    )
}

/// Render the Search overlay: the query being typed, and how much of the inbox
/// it currently matches.
///
/// The `N of M match` row is the feedback the header used to carry by mistake.
/// The header's `attention:` count is the inbox TOTAL (see
/// `TuiScreenModel::attention_total`); the narrowing belongs here, beside the
/// query that caused it, where the operator is already looking.
fn render_search_overlay(
    query: &str,
    matches: usize,
    total: usize,
    area: Rect,
    buffer: &mut Buffer,
) {
    Clear.render(area, buffer);
    Paragraph::new(vec![
        Line::from(format!("/{query}")),
        Line::from(format!("{matches} of {total} match")),
    ])
    .block(Block::new().borders(Borders::ALL).title("Search"))
    .render(area, buffer);
}

fn full_width_explainer_rect(area: Rect) -> Rect {
    let vertical_margin = 3.min(area.height / 4);
    let height = area
        .height
        .saturating_sub(vertical_margin.saturating_mul(2));
    Rect::new(
        area.x,
        area.y + vertical_margin,
        area.width.max(1),
        height.max(1),
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

/// Each row carries the digit that jumps straight to it (`1 Attention`), so the
/// direct view-switch keys are discoverable where the operator is already
/// looking rather than only in the Help roster. The digit comes from the ACTION
/// REGISTRY, not from the row's ordinal: the pane must show the key that is
/// actually bound, and a row whose view carries no chord shows a blank of the
/// same width so the names stay aligned.
fn navigation_row_label(view: TuiView, active: bool) -> String {
    let marker = if active { ">" } else { " " };
    let digit = view_switch_accelerator(view);
    format!("{marker} {digit} {}", view.label())
}

/// The single character bound to `view`'s direct view-switch action, or a space
/// when the registry binds none.
fn view_switch_accelerator(view: TuiView) -> char {
    action_registry::ACTION_REGISTRY
        .iter()
        .filter(|spec| {
            matches!(
                spec.staging,
                action_registry::ActionStaging::Global(action_registry::GlobalAction::GoToView(
                    target
                )) if target == view
            )
        })
        .find_map(|spec| spec.hotkeys.first().map(|chord| chord.key))
        .unwrap_or(' ')
}

fn render_navigation(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    let items = model
        .navigation()
        .iter()
        .map(|view| ListItem::new(navigation_row_label(*view, *view == model.active_view())));
    let title = focus_title("Views", model.focus() == FocusPane::Nav);
    Widget::render(
        List::new(items).block(Block::new().borders(Borders::ALL).title(title)),
        area,
        buffer,
    );
}

/// The Attention pane's placeholder row while the session's first background
/// ingest has not landed and the list is empty.
///
/// An EMPTY list is ambiguous on its own: it is indistinguishable from a
/// genuinely idle inbox with nothing needing attention. This row disambiguates
/// the two (livespec-console-beads-fabro-pzbdbo.27 AC2/AC3): a source that has
/// not returned yet reads LOADING, never as an unlabeled blank pane and never
/// as the unavailable tell (which means a source was polled and failed, not
/// that this session has not asked it yet). It disappears the moment either
/// the first sweep lands (whether or not it found anything) or the operator
/// narrows an empty inbox with a search query -- neither of which this
/// function can tell apart from the other, deliberately: both mean the
/// still-loading condition no longer holds.
const ATTENTION_LOADING_PLACEHOLDER: &str =
    "Loading… waiting for the first event source poll to complete";

fn render_attention(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    let inner_width = usize::from(area.width.saturating_sub(2));
    let items = if model.attention_items().is_empty() && model.startup_ingest_pending() {
        vec![ListItem::new(ATTENTION_LOADING_PLACEHOLDER)]
    } else {
        model
            .attention_items()
            .iter()
            .enumerate()
            .map(|(index, item)| attention_item_line(model, index, item, inner_width))
            .collect::<Vec<_>>()
    };
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
    let selected = summary_selected_index(model);
    let items = model.view_items().iter().enumerate().map(|(index, item)| {
        let marker = if Some(index) == selected { ">" } else { " " };
        ListItem::new(format!("{marker} {}", item.title()))
    });
    let title = focus_title(&summary_pane_title(model), content_focused(model));
    Widget::render(
        List::new(items).block(Block::new().borders(Borders::ALL).title(title)),
        area,
        buffer,
    );
}

/// The row the Content pane highlights while rendering a summary view.
///
/// Only the `Events` container's own overview has a real per-row selection
/// today (its "Stored events" / "Event sources" picker); `Spec`, `Repos`, and
/// a drilled-in `Events` sub-view render their rows exactly as before this
/// container existed -- an unmarked static list (AC3: no behaviour change to
/// "Stored events").
fn summary_selected_index(model: &TuiScreenModel) -> Option<usize> {
    (model.active_view() == TuiView::Events && model.events_focus() == EventsFocus::Overview)
        .then(|| model.selected_events_index())
}

/// The Content pane's block title for a summary view: a breadcrumb
/// (`Events > Stored events`) once the `Events` container is drilled into a
/// sub-view, else the plain view label every other summary view has always
/// shown.
fn summary_pane_title(model: &TuiScreenModel) -> String {
    if model.active_view() == TuiView::Events {
        return match model.events_focus() {
            EventsFocus::Overview => TuiView::Events.label().to_owned(),
            EventsFocus::StoredEvents | EventsFocus::EventSources => {
                format!(
                    "{} > {}",
                    TuiView::Events.label(),
                    model.events_focus().label()
                )
            }
        };
    }
    model.active_view().label().to_owned()
}

/// The elision indicator a row appends when it cannot hold its whole label, so
/// the operator can SEE that text was cut rather than read a clean-looking
/// sentence that silently stops.
const ROW_ELISION_INDICATOR: char = '…';

fn attention_item_line(
    model: &TuiScreenModel,
    index: usize,
    item: &AttentionItem,
    inner_width: usize,
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
    let label = elide_to_width(&label, inner_width);
    ListItem::new(label).style(if Some(index) == model.selected_attention_index() {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    })
}

/// `text` cut to `width` display cells with [`ROW_ELISION_INDICATOR`] in the
/// last cell when anything was cut, or `text` unchanged when it already fits.
///
/// The projected account a needs-human row carries is frequently longer than
/// the pane: the spec forbids TRUNCATING it away silently and allows a row to
/// elide it PROVIDED the elision is indicated, with the whole text held by the
/// detail. A width of zero (a pane too narrow to hold even the indicator)
/// renders nothing rather than an indicator with no content.
fn elide_to_width(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let mut elided: String = text.chars().take(width.saturating_sub(1)).collect();
    if width > 0 {
        elided.push(ROW_ELISION_INDICATOR);
    }
    elided
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
            let rows = dispatcher_setting_rows(settings, model.dispatcher_setting_write());
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
    // The row's WORDS are model-layer (`SettingRow::text`) so a pending or
    // unchanged write is reported in the one sentence the operator reads; the
    // renderer adds only the selection marker.
    let label = format!("{marker} {}", row.text());
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
    let rows = dispatcher_setting_rows(settings, model.dispatcher_setting_write());
    lines.push(Line::from(String::new()));
    lines.extend(
        model
            .selected_setting_index()
            .and_then(|index| rows.get(index))
            .map_or_else(
                || vec![Line::from("No setting selected")],
                |row| {
                    vec![
                        // The SAME model-layer sentence the content row renders,
                        // so the detail pane cannot contradict it about whether
                        // an edit is in flight.
                        Line::from(row.text()),
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
    // The factory is a SEPARATE line rather than a suffix on the run: it names
    // the server the run lives on, and a triager reading a bare run id against
    // a multi-factory fleet has no way to tell which host holds it.
    if let Some(factory) = detail.fabro_factory() {
        lines.push(Line::from(format!("Factory: {factory}")));
    }
    // The pressable commands the orchestrator's attention projection
    // advertises. This line used to read `Attach: fabro attach <run>`, composed
    // by the console; for a needs-human item it now carries that item's
    // `resolve-blocked` valve, because the run it would have attached to no
    // longer exists (scenarios.md Scenario 30).
    for command in detail.valve_commands() {
        lines.push(Line::from(format!("Valve: {command}")));
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
    // The projection's account of the item — for a needs-human item, the
    // terminated run's own story as the orchestrator composed it. Rendered
    // WHOLE and verbatim: the pane wraps and scrolls, so nothing here truncates
    // the reason or the prompt, and nothing composes or augments a field the
    // projection left out. Its own lines are preserved as its own rows.
    if let Some(account) = detail.account() {
        lines.push(Line::from("Account:"));
        lines.extend(account.lines().map(|line| Line::from(line.to_owned())));
    }
    // The orchestrator's `livespec-human-answer` comments, read back from the
    // context surface after a successful resolve and shown exactly as returned.
    for comment in detail.answer_comments() {
        lines.extend(comment.lines().map(|line| Line::from(line.to_owned())));
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
    use crate::{
        ATTENTION_LOADING_PLACEHOLDER, HELP_MODAL_MARGIN, apply_build_staleness,
        apply_dispatcher_settings_reread, apply_sink_outcome, apply_startup_ingest_pending,
        apply_worker_status, apply_writer_lease_status,
    };
    use console_application::DispatcherSettingWriteState;
    #[cfg(test)]
    use console_application::source_adapters::LaneReason;
    use console_application::source_adapters::{
        AcceptancePolicy, AdapterResult, AdmissionPolicy, AttentionHandoff, AttentionItemSnapshot,
        AttentionSourceRef, DispatcherJournalEntry, DispatcherJournalKind, Lane,
        OrphanedFactoryRun, ReconcileRunsSnapshot, SourceAdapterKind, attention_item_payload_json,
        dispatcher_journal_payload_json, reconcile_runs_snapshot_payload_json,
    };
    use console_application::writer_identity::{WriterIdentity, WriterLeaseStatus};
    use console_application::{
        AttentionDetail, AttentionItem, DispatcherOverride, DispatcherSettings,
        DispatcherSettingsRead, EventsFocus, FocusPane, HelpFocus, LaneFocus, LaneWorkItem,
        OperatorAction, OperatorActionOutcome, OverrideBool, OverrideInt, PendingValve,
        PluginResolution, RejectMode, TimelineEntry, TuiInteraction, TuiInteractionState,
        TuiOverlay, TuiScreenModel, TuiView, action_registry, build_tui_model,
        build_tui_model_for_state, header_help_section, help_section_for_view,
        reduce_tui_interaction,
    };

    use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::text::Line;

    use super::{
        DeferredTuiRuntimeEffectSink, ITEM_FIELD_ABSENT, InputSource, LANE_OVERVIEW_PREVIEW,
        LoopTick, MAX_COALESCED_MOVEMENT_KEYS, TuiLiveSession, TuiRenderError, TuiRenderResult,
        TuiRuntimeEffect, TuiRuntimeEffectSink, TuiRuntimeEffectSinkOutcome, TuiTerminalInput,
        action_available_for_model, action_outcome_effect, apply_tick_refresh, attention_item_line,
        buffer_to_text, command_explainer_confirm_step, command_explainer_lines,
        command_explanation_for_action, detail_lines, drain_input_burst,
        effect_triggers_source_poll, elide_to_width, full_width_explainer_rect, global_help_lines,
        header_help_lines, help_lines_for_view, help_outcome, is_navigation_interaction,
        key_event_to_terminal_input, menu_confirm_step, registry_action_input,
        registry_staging_explanation, render_command_explainer, render_command_modal,
        render_detail, render_footer, render_menu_overlay, render_model, render_summary_detail,
        render_to_text, render_work_item_detail, settings_detail_lines, staged_action_step,
        step_tui_runtime, step_tui_runtime_with_model, text_input,
    };

    macro_rules! assert {
        ($condition:expr $(,)?) => {{
            check($condition, stringify!($condition));
        }};
        ($condition:expr, $($arg:tt)+) => {{
            let _ = format_args!($($arg)+);
            check($condition, stringify!($condition));
        }};
    }

    macro_rules! assert_eq {
        ($left:expr, $right:expr $(,)?) => {{
            match (&$left, &$right) {
                (left, right) => check(left == right, stringify!($left == $right)),
            }
        }};
        ($left:expr, $right:expr, $($arg:tt)+) => {{
            let _ = format_args!($($arg)+);
            match (&$left, &$right) {
                (left, right) => check(left == right, stringify!($left == $right)),
            }
        }};
    }

    #[test]
    #[should_panic(expected = "check failed")]
    fn check_panics() {
        check(false, "check failed");
    }

    #[test]
    #[should_panic(expected = "ok_render_text failed")]
    fn ok_render_text_panics() {
        ok_render_text(Err(TuiRenderError::EmptyArea));
    }

    #[test]
    #[should_panic(expected = "ok_dispatcher_journal_entry failed")]
    fn ok_dispatcher_journal_entry_panics() {
        ok_dispatcher_journal_entry(DispatcherJournalEntry::new(
            "console",
            "work-item",
            "dispatch",
            DispatcherJournalKind::Progress,
            0,
        ));
    }

    #[test]
    #[should_panic(expected = "selected_lane_item failed")]
    fn selected_lane_item_panics() {
        selected_lane_item(None);
    }

    #[test]
    fn a_not_applied_effect_is_shown_to_the_operator_and_not_flushed_later() {
        // livespec-console-beads-fabro-ddfbcx.1. A transient store contention
        // must leave the session ALIVE, say so where the operator is already
        // looking, and must NOT queue the effect for a later flush — it did not
        // land, and flushing it later would apply an action the operator was
        // told had failed.
        let mut state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_selected_repo("contention-test".to_owned());
        let mut effects = Vec::new();

        apply_sink_outcome(
            &mut state,
            &mut effects,
            TuiRuntimeEffect::Render,
            TuiRuntimeEffectSinkOutcome::NotApplied(
                "store busy - action NOT applied, press the key again to retry".to_owned(),
            ),
            0,
        );

        check(
            effects.is_empty(),
            "a NOT-applied effect must never be queued for a later flush",
        );

        let rendered =
            render_to_text(&build_tui_model_for_state(&[], &state), 200, 40).unwrap_or_default();
        assert!(
            rendered.contains("NOT applied"),
            "the operator must SEE that the action did not land: {rendered}"
        );
    }

    #[test]
    fn an_applied_effect_leaves_no_status_and_a_deferred_one_is_queued() {
        // NEGATIVE CONTROL for the above, both directions: the status must not
        // appear when nothing failed, and Deferred must still queue.
        let base = TuiInteractionState::new(0, TuiOverlay::None)
            .with_selected_repo("contention-test".to_owned());

        let mut applied_state = base.clone();
        let mut applied_effects = Vec::new();
        apply_sink_outcome(
            &mut applied_state,
            &mut applied_effects,
            TuiRuntimeEffect::Render,
            TuiRuntimeEffectSinkOutcome::Applied,
            0,
        );
        let rendered = render_to_text(&build_tui_model_for_state(&[], &applied_state), 200, 40)
            .unwrap_or_default();
        assert!(
            !rendered.contains("NOT applied"),
            "an applied effect must not claim it failed: {rendered}"
        );
        check(
            applied_effects.is_empty(),
            "an applied effect must not be queued",
        );

        let mut deferred_state = base;
        let mut deferred_effects = Vec::new();
        apply_sink_outcome(
            &mut deferred_state,
            &mut deferred_effects,
            TuiRuntimeEffect::Render,
            TuiRuntimeEffectSinkOutcome::Deferred,
            0,
        );
        check(
            deferred_effects.len() == 1,
            "a deferred effect must still be queued for later handling",
        );
    }

    #[test]
    fn a_worker_that_could_not_execute_a_command_reaches_the_operator() {
        // livespec-console-beads-fabro-zbnnlv. The mutating command runs on a
        // WORKER thread, so by the time it fails the valve has confirmed and the
        // modal has closed — and the failure leaves no event in the log, so the
        // render loop's re-list can never surface it. This fold is the only
        // place the operator is told, so it is asserted on the RENDERED frame
        // rather than on the state field.
        let mut state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_selected_repo("worker-status-test".to_owned());

        apply_worker_status(
            &mut state,
            Some(
                "action NOT executed - the factory-command lane failed at store-open (busy)"
                    .to_owned(),
            ),
        );

        let rendered =
            render_to_text(&build_tui_model_for_state(&[], &state), 200, 40).unwrap_or_default();
        assert!(
            rendered.contains("NOT executed"),
            "the operator must SEE that their command never ran: {rendered}"
        );
        assert!(
            rendered.contains("factory-command"),
            "the surfaced line must name WHICH lane dropped the command: {rendered}"
        );
    }

    #[test]
    fn a_quiet_worker_leaves_the_operator_s_view_alone() {
        // MUST-NOT-FLAG CONTROL. `None` is the overwhelmingly common case — once
        // per render tick — so a fold that wrote on every tick would either
        // manufacture a failure or wipe a status the operator has not read yet.
        let base = TuiInteractionState::new(0, TuiOverlay::None)
            .with_selected_repo("worker-status-test".to_owned());

        let mut quiet = base.clone();
        apply_worker_status(&mut quiet, None);
        let rendered =
            render_to_text(&build_tui_model_for_state(&[], &quiet), 200, 40).unwrap_or_default();
        assert!(
            !rendered.contains("NOT executed"),
            "a worker with nothing to report must not claim a failure: {rendered}"
        );

        // And an EARLIER status survives a quiet tick, rather than being cleared
        // before the operator could read it.
        let mut carried = base.with_transient_status(Some("earlier report".to_owned()));
        apply_worker_status(&mut carried, None);
        let rendered =
            render_to_text(&build_tui_model_for_state(&[], &carried), 200, 40).unwrap_or_default();
        assert!(
            rendered.contains("earlier report"),
            "a quiet tick must not erase a status the operator may not have read: {rendered}"
        );
    }

    #[test]
    fn a_freshly_probed_staleness_replaces_the_state_the_operator_sees() {
        // livespec-console-beads-fabro-mx9u.26 AC1/AC3: a session left running
        // learns it has fallen behind, and the count is ACCURATE as it changes
        // -- 1 behind, then 5 -- without a restart. This is the render loop's
        // fold: the background poller's fresh read replaces whatever the state
        // carried before, on every tick that reports one.
        let base = TuiInteractionState::new(0, TuiOverlay::None)
            .with_selected_repo("build-staleness-test".to_owned())
            .with_build_staleness(super::BuildStaleness::Behind(1));

        let mut one_behind = base.clone();
        apply_build_staleness(&mut one_behind, Some(super::BuildStaleness::Behind(1)));
        assert_eq!(
            one_behind.build_staleness(),
            super::BuildStaleness::Behind(1)
        );

        let mut five_behind = base;
        apply_build_staleness(&mut five_behind, Some(super::BuildStaleness::Behind(5)));
        assert_eq!(
            five_behind.build_staleness(),
            super::BuildStaleness::Behind(5)
        );
        let rendered = render_to_text(&build_tui_model_for_state(&[], &five_behind), 200, 40)
            .unwrap_or_default();
        assert!(
            rendered.contains("build STALE: 5 commits behind"),
            "a widening gap must reach the rendered tell without a restart: {rendered}"
        );
    }

    #[test]
    fn an_absent_staleness_probe_leaves_the_state_alone() {
        // MUST-NOT-FLAG CONTROL, same shape as `a_quiet_worker_leaves_the_operator_s_view_alone`:
        // `None` is what every session with no live probe behind it reports
        // (the legacy `run_interactive_tui` entry point, and any test double
        // that has not overridden `take_build_staleness`) -- AC4 requires this
        // stays silent-by-construction rather than manufacturing a claim.
        let mut state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_selected_repo("build-staleness-test".to_owned())
            .with_build_staleness(super::BuildStaleness::Current);

        apply_build_staleness(&mut state, None);

        assert_eq!(state.build_staleness(), super::BuildStaleness::Current);
    }

    #[test]
    // livespec-console-beads-fabro-pzbdbo.27: the render loop's fold for the
    // startup-ingest tell, exercised the same way `apply_build_staleness` is --
    // the loop itself is terminal-bound and excluded from tests, this seam is
    // not.
    fn apply_startup_ingest_pending_folds_the_sessions_current_reading_every_tick() {
        let mut state =
            TuiInteractionState::new(0, TuiOverlay::None).with_startup_ingest_pending(true);
        apply_startup_ingest_pending(&mut state, true);
        assert!(state.startup_ingest_pending());

        // Unlike `apply_build_staleness`, there is no `None`/"no opinion" case
        // to leave alone: every session answers `first_ingest_in_progress`
        // (defaulting `false`), so the fold always applies what it is given --
        // including the transition that matters, first sweep landed:
        apply_startup_ingest_pending(&mut state, false);
        assert!(!state.startup_ingest_pending());
    }

    #[test]
    fn a_freshly_observed_read_only_status_reaches_the_rendered_header() {
        // livespec-console-beads-fabro-mx9u.23 AC3: a session left running
        // learns it lost the writer lease WITHOUT a restart, exactly the same
        // shape as the build-staleness tell above.
        let mut state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_selected_repo("writer-lease-test".to_owned())
            .with_writer_lease_status(WriterLeaseStatus::Writable);

        let holder =
            WriterIdentity::new(999, "/opt/other-console", "/data/projects/repo", "9999999");
        apply_writer_lease_status(&mut state, Some(WriterLeaseStatus::ReadOnly(holder)));

        assert!(!state.writer_lease_status().is_writable());
        let rendered =
            render_to_text(&build_tui_model_for_state(&[], &state), 200, 40).unwrap_or_default();
        assert!(
            rendered.contains("READ-ONLY: store owned by build 9999999 at /opt/other-console"),
            "the header must name the OTHER writer, not just say read-only: {rendered}"
        );
    }

    #[test]
    fn an_absent_writer_lease_probe_leaves_the_state_alone() {
        // MUST-NOT-FLAG CONTROL: `None` is what every session with no live
        // poller behind it reports (the legacy entry point, and any test
        // double that has not overridden `take_writer_lease_status`) -- this
        // stays silent-by-construction rather than manufacturing a claim.
        let mut state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_selected_repo("writer-lease-test".to_owned())
            .with_writer_lease_status(WriterLeaseStatus::Writable);

        apply_writer_lease_status(&mut state, None);

        assert!(state.writer_lease_status().is_writable());
    }

    #[test]
    fn a_session_with_no_worker_behind_it_reports_no_worker_status() {
        // The trait default. A legacy no-store session has no command worker, so
        // it must report nothing rather than inventing a failure — the same
        // reason `refresh_events` keeps its startup snapshot there.
        let mut session = DeferredTuiRuntimeEffectSink;

        check(
            session.take_worker_status().is_none(),
            "a session with no worker behind it reports no worker status",
        );
    }

    #[test]
    fn a_session_with_no_probe_behind_it_reports_no_build_staleness() {
        // The trait default (livespec-console-beads-fabro-mx9u.26). The legacy
        // no-store session has no background poller to read from, so it must
        // leave the loop's seeded state alone rather than manufacturing a
        // reading — same reasoning as the worker-status default above.
        let mut session = DeferredTuiRuntimeEffectSink;

        check(
            session.take_build_staleness().is_none(),
            "a session with no probe behind it reports no build staleness",
        );
    }

    #[test]
    // livespec-console-beads-fabro-pzbdbo.27: the trait default. The legacy
    // no-store session has no background poller to have a first sweep at all,
    // so it must report nothing PENDING -- already caught up, same reasoning
    // as the worker-status and build-staleness defaults above.
    fn a_session_with_no_poller_behind_it_reports_no_first_ingest_pending() {
        let session = DeferredTuiRuntimeEffectSink;

        check(
            !session.first_ingest_in_progress(),
            "a session with no poller behind it reports no first ingest pending",
        );
    }

    #[test]
    fn a_session_with_no_poller_behind_it_reports_no_writer_lease_status() {
        // The trait default (livespec-console-beads-fabro-mx9u.23 AC3), same
        // shape as the build-staleness default above: the legacy no-store
        // session has no background poller to read a lease decision from, so
        // it must leave the loop's seeded state alone rather than
        // manufacturing one.
        let mut session = DeferredTuiRuntimeEffectSink;

        check(
            session.take_writer_lease_status().is_none(),
            "a session with no poller behind it reports no writer lease status",
        );
    }

    #[track_caller]
    #[allow(clippy::manual_assert, clippy::panic)]
    fn check(condition: bool, context: &str) {
        if !condition {
            panic!("{context}");
        }
    }

    #[test]
    fn help_never_calls_a_navigation_action_a_confirm_modal() {
        // Found by DOGFOODING the real TUI. The Help modal printed
        // "Help (confirm modal)", "Search (confirm modal)", "Menu bar (confirm
        // modal)" and "Quit (confirm modal)" — none of which confirms anything.
        // Pressing `/` was checked against a live session: the pane that opens
        // is titled "Search".
        //
        // The cause was that the roster is DERIVED from the registry while the
        // description beside it was GUESSED from `parameter.is_none()`. This
        // asserts the description is read from `staging` instead.
        for spec in action_registry::ACTION_REGISTRY {
            let action_registry::ActionStaging::Global(global) = spec.staging else {
                continue;
            };
            let outcome = help_outcome(spec);
            check(
                outcome != "confirm modal",
                "a global navigation action must not be described as a confirm modal",
            );
            // And each one is described DISTINCTLY, so the help says which
            // surface opens rather than merely that it is not a confirm.
            let expected = match global {
                action_registry::GlobalAction::GoToView(_view) => "that view",
                action_registry::GlobalAction::OpenSearch => "search overlay",
                action_registry::GlobalAction::OpenCommandPalette => "command palette",
                action_registry::GlobalAction::OpenHelp => "help overlay",
                action_registry::GlobalAction::OpenMenu => "menu bar",
                action_registry::GlobalAction::Quit => "quits at once",
            };
            check(
                outcome == expected,
                "each global action names the surface its accelerator actually opens",
            );
        }
    }

    #[test]
    fn help_still_calls_a_staged_valve_a_confirm_modal() {
        // NEGATIVE CONTROL. The fix must not sweep the phrase away wholesale —
        // a staged valve genuinely does open a confirm modal, and losing that
        // would trade one dishonest help screen for another.
        // Collected rather than unwrapped: a `let ... else { panic! }` is a
        // branch no passing run takes, and the coverage gate correctly refuses
        // it.
        let staged: Vec<&'static str> = action_registry::ACTION_REGISTRY
            .iter()
            .filter(|spec| matches!(spec.staging, action_registry::ActionStaging::Valve(_)))
            .map(help_outcome)
            .collect();

        check(
            !staged.is_empty(),
            "the registry carries at least one valve-staged action to control against",
        );
        check(
            staged.iter().all(|outcome| *outcome == "confirm modal"),
            "a valve-staged action is still described as a confirm modal",
        );
    }

    #[track_caller]
    #[allow(clippy::panic)]
    fn ok_render_text(result: TuiRenderResult<String>) -> String {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_render_text failed: {error:?}"),
        }
    }

    #[track_caller]
    #[allow(clippy::panic)]
    fn ok_dispatcher_journal_entry(
        result: AdapterResult<DispatcherJournalEntry>,
    ) -> DispatcherJournalEntry {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_dispatcher_journal_entry failed: {error:?}"),
        }
    }

    #[track_caller]
    #[allow(clippy::option_if_let_else, clippy::panic)]
    fn selected_lane_item(result: Option<&LaneWorkItem>) -> &LaneWorkItem {
        match result {
            Some(value) => value,
            None => panic!("selected_lane_item failed"),
        }
    }

    #[track_caller]
    fn check_deferred_outcome(result: std::io::Result<TuiRuntimeEffectSinkOutcome>) {
        check(
            result.is_ok_and(|outcome| outcome == TuiRuntimeEffectSinkOutcome::Deferred),
            "check_deferred_outcome failed",
        );
    }

    /// Unwrap a `drain_input_burst` result in tests, matching this file's
    /// `check`-fn convention (a bare `panic!` needs its own `#[allow]`, same
    /// as `check` carries).
    ///
    /// NOT generic over the `Ok` type -- one non-generic function per return
    /// shape, like `check_refresh_none` / `check_deferred_outcome` above,
    /// rather than one `expect_io<T>` monomorphized per call site. A generic
    /// helper here would give the coverage gate's instantiation-group
    /// accounting a NEW multi-monomorphization signature (the same
    /// scalar-merge artifact `tests/fixtures/coverage-unnameable-disposition.json`
    /// already tracks one instance of) for no benefit -- these two callers
    /// are the only ones this file has.
    #[track_caller]
    #[allow(clippy::panic)]
    fn expect_tick(result: std::io::Result<LoopTick>, context: &str) -> LoopTick {
        match result {
            Ok(tick) => tick,
            Err(error) => panic!("{context}: {error}"),
        }
    }

    #[track_caller]
    #[allow(clippy::panic)]
    fn expect_more_input(result: std::io::Result<bool>, context: &str) -> bool {
        match result {
            Ok(more) => more,
            Err(error) => panic!("{context}: {error}"),
        }
    }

    #[track_caller]
    fn check_refresh_none(result: std::io::Result<Option<Vec<ConsoleEvent>>>) {
        check(
            result.is_ok_and(|events| events.is_none()),
            "check_refresh_none failed",
        );
    }

    #[track_caller]
    fn check_driver_handoff_overlay(overlay: &TuiOverlay) {
        let expected = TuiOverlay::DriverHandoff {
            command: String::new(),
        };
        check(
            std::mem::discriminant(overlay) == std::mem::discriminant(&expected),
            "check_driver_handoff_overlay failed",
        );
    }

    #[track_caller]
    fn check_search_overlay(overlay: &TuiOverlay) {
        let expected = TuiOverlay::Search {
            query: String::new(),
        };
        check(
            std::mem::discriminant(overlay) == std::mem::discriminant(&expected),
            "check_search_overlay failed",
        );
    }

    #[track_caller]
    fn check_menu_navigation_input(input: Option<TuiTerminalInput>, context: &str) {
        check(
            input == Some(TuiTerminalInput::Interaction(TuiInteraction::MenuNextTop))
                || input
                    == Some(TuiTerminalInput::Interaction(
                        TuiInteraction::MenuPreviousTop,
                    ))
                || input
                    == Some(TuiTerminalInput::Interaction(
                        TuiInteraction::SelectNextAction,
                    ))
                || input
                    == Some(TuiTerminalInput::Interaction(
                        TuiInteraction::SelectPreviousAction,
                    ))
                || input == Some(TuiTerminalInput::Confirm),
            context,
        );
    }

    #[test]
    fn deferred_runtime_effect_sink_defers_effects() {
        let mut sink = DeferredTuiRuntimeEffectSink;

        check_deferred_outcome(sink.handle_runtime_effect(&TuiRuntimeEffect::Quit));
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

        check_refresh_none(sink.refresh_events(true));
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

    /// A fake "backing CLI" for `apply_tick_refresh`'s decision: it panics if
    /// `refresh_events` is ever called, standing in for the local store read
    /// (and, in the real `StoreBackedTuiRuntimeEffectSink`, the worker channel
    /// behind it) that livespec-console-beads-fabro-mx9u.8's acceptance
    /// criterion 2 forbids on the selection-change path.
    struct PanicsOnRefresh;

    impl TuiRuntimeEffectSink for PanicsOnRefresh {
        fn handle_runtime_effect(
            &mut self,
            _effect: &TuiRuntimeEffect,
        ) -> std::io::Result<TuiRuntimeEffectSinkOutcome> {
            Ok(TuiRuntimeEffectSinkOutcome::Applied)
        }
    }

    impl TuiLiveSession for PanicsOnRefresh {
        #[allow(clippy::panic)]
        fn refresh_events(
            &mut self,
            _request_poll: bool,
        ) -> std::io::Result<Option<Vec<ConsoleEvent>>> {
            panic!(
                "refresh_events must not be called on a pure navigation tick \
                 (livespec-console-beads-fabro-mx9u.8, acceptance criterion 2)"
            );
        }
    }

    /// livespec-console-beads-fabro-mx9u.8, acceptance criterion 2: "The
    /// selection-change path performs no backing-CLI invocation and no ledger
    /// read." A `HandledInput` tick (one or more keys handled, none mutating --
    /// exactly what a run of Attention `SelectNext`/`SelectPrevious` keystrokes
    /// produces) must never reach the session's `refresh_events` at all.
    #[test]
    fn handled_input_tick_never_refreshes() {
        let mut session = PanicsOnRefresh;

        check_refresh_none(apply_tick_refresh(LoopTick::HandledInput, &mut session));
    }

    /// `Quit` also skips the refresh (the loop returns before it would
    /// matter); pinning it down keeps the match exhaustive and honest.
    #[test]
    fn quit_tick_never_refreshes() {
        let mut session = PanicsOnRefresh;

        check_refresh_none(apply_tick_refresh(LoopTick::Quit, &mut session));
    }

    /// Sanity check that `PanicsOnRefresh` genuinely panics for the two ticks
    /// that DO refresh. Without this, `handled_input_tick_never_refreshes`
    /// above could pass vacuously if `apply_tick_refresh` stopped calling the
    /// session for every tick, decision or no.
    #[test]
    #[should_panic(expected = "refresh_events must not be called")]
    fn idle_tick_does_refresh_so_the_panic_fixture_above_is_a_real_check() {
        let mut session = PanicsOnRefresh;
        let _ = apply_tick_refresh(LoopTick::Idle, &mut session);
    }

    #[test]
    #[should_panic(expected = "refresh_events must not be called")]
    fn mutated_tick_does_refresh_so_the_panic_fixture_above_is_a_real_check() {
        let mut session = PanicsOnRefresh;
        let _ = apply_tick_refresh(LoopTick::Mutated, &mut session);
    }

    /// A scripted, in-memory [`InputSource`] for `drain_input_burst` tests --
    /// the seam that makes burst coalescing
    /// (livespec-console-beads-fabro-mx9u.9) testable without a real terminal.
    /// `poll_now` reports more input while the queue is non-empty, so a test
    /// controls exactly how much looks "already buffered" versus "arrives
    /// later" by how many events it seeds up front.
    struct ScriptedInputSource {
        queued: std::collections::VecDeque<crossterm::event::Event>,
    }

    impl ScriptedInputSource {
        fn new(events: impl IntoIterator<Item = crossterm::event::Event>) -> Self {
            Self {
                queued: events.into_iter().collect(),
            }
        }

        fn of_keys(codes: impl IntoIterator<Item = KeyCode>) -> Self {
            Self::new(
                codes
                    .into_iter()
                    .map(|code| crossterm::event::Event::Key(key(code))),
            )
        }
    }

    impl InputSource for ScriptedInputSource {
        fn poll_now(&mut self) -> std::io::Result<bool> {
            Ok(!self.queued.is_empty())
        }

        fn read(&mut self) -> std::io::Result<crossterm::event::Event> {
            Ok(self
                .queued
                .pop_front()
                .unwrap_or(crossterm::event::Event::FocusLost))
        }
    }

    /// A 40-row Attention list, one `blocked / needs-human` work-item snapshot
    /// per row -- enough rows that a burst of moves never clamps at either
    /// edge.
    fn many_attention_rows(count: usize) -> Vec<ConsoleEvent> {
        (0..count)
            .map(|index| {
                let work_item_id = format!("wi-{index:04}");
                lane_event(
                    &format!("evt_{index:04}"),
                    &work_item_id,
                    Lane::Blocked,
                    Some(LaneReason::NeedsHuman),
                    &format!("a{index:04}"),
                    "blocked",
                )
            })
            .collect()
    }

    fn content_focused_state(selection: usize) -> TuiInteractionState {
        TuiInteractionState::new(selection, TuiOverlay::None).with_focus(FocusPane::Content)
    }

    /// livespec-console-beads-fabro-mx9u.9, acceptance criterion 1: "N buffered
    /// movement keystrokes result in exactly one selection change of N steps
    /// and one render." `drain_input_burst` never calls the CALLER's terminal
    /// render (the loop draws once after it returns), so this proves the
    /// state-transition half: five buffered `Down` keys move the selection by
    /// five in ONE call, having applied each key (`handle_runtime_effect`
    /// fires once per key -- bookkeeping only, never a store read) without
    /// stopping to let the terminal loop redraw between them.
    #[test]
    fn a_burst_of_movement_keys_advances_the_selection_by_the_whole_burst_in_one_call() {
        let events = many_attention_rows(40);
        let mut state = content_focused_state(0);
        let projection = console_application::project_tui_events(&events, None);
        let mut effects = Vec::new();
        // `PanicsOnRefresh` (never a store/backing-CLI read) doubling as the
        // burst's session proves `drain_input_burst` itself never reaches
        // `refresh_events` -- the SAME property `apply_tick_refresh`'s tests
        // prove for the tick-outcome decision, now proven end to end.
        let mut session = PanicsOnRefresh;
        let mut source = ScriptedInputSource::of_keys([
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Down,
        ]);

        let tick = expect_tick(
            drain_input_burst(
                &mut state,
                &projection,
                &events,
                "operator",
                &mut effects,
                &mut session,
                &mut source,
            ),
            "drain_input_burst does not fail against a scripted source",
        );

        assert_eq!(tick, LoopTick::HandledInput);
        assert_eq!(state.selected_attention_index(), 5);
    }

    /// livespec-console-beads-fabro-mx9u.9, acceptance criterion 2: buffered
    /// movement is coalesced and the buffer is bounded. A burst well past
    /// [`MAX_COALESCED_MOVEMENT_KEYS`] still lands the selection at exactly
    /// its own length (no overshoot from double-counting, no undershoot from
    /// dropping a key) and does not hang the drain.
    #[test]
    fn a_flood_of_movement_keys_lands_on_its_own_final_position_with_no_overshoot() {
        let flood = MAX_COALESCED_MOVEMENT_KEYS * 3;
        let events = many_attention_rows(flood + 10);
        let mut state = content_focused_state(0);
        let projection = console_application::project_tui_events(&events, None);
        let mut effects = Vec::new();
        let mut session = DeferredTuiRuntimeEffectSink;
        let mut source = ScriptedInputSource::of_keys(std::iter::repeat_n(KeyCode::Down, flood));

        // The bound means a big-enough flood takes more than one tick to fully
        // drain -- keep calling until the source is empty, exactly as
        // successive terminal-loop ticks would.
        let mut ticks = 0_usize;
        loop {
            let tick = expect_tick(
                drain_input_burst(
                    &mut state,
                    &projection,
                    &events,
                    "operator",
                    &mut effects,
                    &mut session,
                    &mut source,
                ),
                "drain_input_burst does not fail against a scripted source",
            );
            assert_eq!(tick, LoopTick::HandledInput);
            ticks += 1;
            if !expect_more_input(source.poll_now(), "scripted poll_now never fails") {
                break;
            }
        }

        assert_eq!(state.selected_attention_index(), flood);
        assert!(
            ticks > 1,
            "a flood past MAX_COALESCED_MOVEMENT_KEYS must take more than one tick to drain, proving the bound actually bit"
        );
    }

    /// livespec-console-beads-fabro-mx9u.9, acceptance criterion 3: "Verb,
    /// Enter and Escape keys are never coalesced away or reordered relative to
    /// each other." A burst of Down, Down, then Escape (a verb-class key: it
    /// resolves to `CloseOverlay`, not a coalesced navigation) applies all
    /// three, in order, and stops the burst at the Escape rather than
    /// swallowing or reordering it.
    #[test]
    fn a_verb_key_inside_a_burst_is_applied_in_order_and_ends_the_burst() {
        let events = many_attention_rows(40);
        let mut state = TuiInteractionState::new(
            0,
            TuiOverlay::Search {
                query: String::new(),
            },
        );
        let projection = console_application::project_tui_events(&events, Some(""));
        let mut effects = Vec::new();
        let mut session = DeferredTuiRuntimeEffectSink;
        // In the Search overlay, Down/Up move the selection (as in Content
        // focus) and Esc closes the overlay -- a verb relative to navigation.
        let mut source = ScriptedInputSource::of_keys([
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Esc,
            KeyCode::Down,
        ]);

        let tick = expect_tick(
            drain_input_burst(
                &mut state,
                &projection,
                &events,
                "operator",
                &mut effects,
                &mut session,
                &mut source,
            ),
            "drain_input_burst does not fail against a scripted source",
        );

        // The burst stops AT the Esc: it is not navigation, so it always gets
        // its own frame instead of being buried inside a movement batch. The
        // trailing Down is left buffered for the next tick.
        assert_eq!(tick, LoopTick::HandledInput);
        assert_eq!(state.overlay(), &TuiOverlay::None);
        assert!(expect_more_input(
            source.poll_now(),
            "scripted poll_now never fails"
        ));
    }

    /// A non-key terminal event (resize, mouse, focus) carries nothing to
    /// coalesce: the burst ends without applying anything.
    #[test]
    fn a_non_key_event_ends_the_burst_with_nothing_applied() {
        let events = many_attention_rows(40);
        let mut state = content_focused_state(0);
        let projection = console_application::project_tui_events(&events, None);
        let mut effects = Vec::new();
        let mut session = PanicsOnRefresh;
        let mut source = ScriptedInputSource::new([crossterm::event::Event::FocusLost]);

        let tick = expect_tick(
            drain_input_burst(
                &mut state,
                &projection,
                &events,
                "operator",
                &mut effects,
                &mut session,
                &mut source,
            ),
            "drain_input_burst does not fail against a scripted source",
        );

        assert_eq!(tick, LoopTick::Idle);
        assert_eq!(state.selected_attention_index(), 0);
    }

    /// A key-release event (not a press) is skipped rather than applied, and
    /// the burst keeps draining past it.
    #[test]
    fn a_non_press_key_event_is_skipped_and_the_burst_keeps_draining() {
        let events = many_attention_rows(40);
        let mut state = content_focused_state(0);
        let projection = console_application::project_tui_events(&events, None);
        let mut effects = Vec::new();
        let mut session = PanicsOnRefresh;
        let mut source = ScriptedInputSource::new([
            crossterm::event::Event::Key(crossterm::event::KeyEvent::new_with_kind(
                KeyCode::Down,
                KeyModifiers::empty(),
                crossterm::event::KeyEventKind::Release,
            )),
            crossterm::event::Event::Key(key(KeyCode::Down)),
        ]);

        let tick = expect_tick(
            drain_input_burst(
                &mut state,
                &projection,
                &events,
                "operator",
                &mut effects,
                &mut session,
                &mut source,
            ),
            "drain_input_burst does not fail against a scripted source",
        );

        assert_eq!(tick, LoopTick::HandledInput);
        assert_eq!(state.selected_attention_index(), 1);
    }

    /// A key that maps to no interaction at all (`key_event_to_terminal_input`
    /// returns `None`) is skipped, and the burst keeps draining past it too.
    #[test]
    fn an_unmapped_key_is_skipped_and_the_burst_keeps_draining() {
        let events = many_attention_rows(40);
        let mut state = content_focused_state(0);
        let projection = console_application::project_tui_events(&events, None);
        let mut effects = Vec::new();
        let mut session = PanicsOnRefresh;
        let mut source = ScriptedInputSource::of_keys([KeyCode::Home, KeyCode::Down]);

        let tick = expect_tick(
            drain_input_burst(
                &mut state,
                &projection,
                &events,
                "operator",
                &mut effects,
                &mut session,
                &mut source,
            ),
            "drain_input_burst does not fail against a scripted source",
        );

        assert_eq!(tick, LoopTick::HandledInput);
        assert_eq!(state.selected_attention_index(), 1);
    }

    /// A non-press key with NOTHING queued behind it: the tick has nothing to
    /// show for itself (no interaction was ever applied), so it reports
    /// `Idle` -- the same as a genuine 250ms poll timeout, which is exactly
    /// right: neither warrants skipping the store's normal-cadence refresh.
    #[test]
    fn a_lone_non_press_key_reports_idle() {
        let events = many_attention_rows(40);
        let mut state = content_focused_state(0);
        let projection = console_application::project_tui_events(&events, None);
        let mut effects = Vec::new();
        let mut session = PanicsOnRefresh;
        let mut source = ScriptedInputSource::new([crossterm::event::Event::Key(
            crossterm::event::KeyEvent::new_with_kind(
                KeyCode::Down,
                KeyModifiers::empty(),
                crossterm::event::KeyEventKind::Release,
            ),
        )]);

        let tick = expect_tick(
            drain_input_burst(
                &mut state,
                &projection,
                &events,
                "operator",
                &mut effects,
                &mut session,
                &mut source,
            ),
            "drain_input_burst does not fail against a scripted source",
        );

        assert_eq!(tick, LoopTick::Idle);
        assert_eq!(state.selected_attention_index(), 0);
    }

    /// A lone unmapped key, same shape as the lone-non-press case above:
    /// nothing queued behind it, nothing applied, `Idle`.
    #[test]
    fn a_lone_unmapped_key_reports_idle() {
        let events = many_attention_rows(40);
        let mut state = content_focused_state(0);
        let projection = console_application::project_tui_events(&events, None);
        let mut effects = Vec::new();
        let mut session = PanicsOnRefresh;
        let mut source = ScriptedInputSource::of_keys([KeyCode::Home]);

        let tick = expect_tick(
            drain_input_burst(
                &mut state,
                &projection,
                &events,
                "operator",
                &mut effects,
                &mut session,
                &mut source,
            ),
            "drain_input_burst does not fail against a scripted source",
        );

        assert_eq!(tick, LoopTick::Idle);
        assert_eq!(state.selected_attention_index(), 0);
    }

    /// The operator's own quit keystroke (`q`), landing mid-burst-drain
    /// exactly as it would from a real terminal: `drain_input_burst` returns
    /// `Quit` immediately rather than continuing to drain.
    #[test]
    fn a_quit_key_ends_the_burst_with_quit() {
        let events = many_attention_rows(40);
        let mut state = content_focused_state(0);
        let projection = console_application::project_tui_events(&events, None);
        let mut effects = Vec::new();
        let mut session = PanicsOnRefresh;
        let mut source = ScriptedInputSource::of_keys([KeyCode::Char('q')]);

        let tick = expect_tick(
            drain_input_burst(
                &mut state,
                &projection,
                &events,
                "operator",
                &mut effects,
                &mut session,
                &mut source,
            ),
            "drain_input_burst does not fail against a scripted source",
        );

        assert_eq!(tick, LoopTick::Quit);
    }

    /// A ledger-mutating verb (confirming a staged factory drain) reports
    /// `Mutated`, not `HandledInput` -- the terminal loop's cue to re-poll
    /// sources at once rather than skip the refresh.
    #[test]
    fn a_mutating_verb_key_reports_mutated() {
        let events = many_attention_rows(40);
        let mut state = TuiInteractionState::new(
            0,
            TuiOverlay::FactoryDrainConfirm {
                work_item_id: "console-staged-drain".to_owned(),
                rank: "a0".to_owned(),
            },
        );
        let projection = console_application::project_tui_events(&events, None);
        let mut effects = Vec::new();
        let mut session = DeferredTuiRuntimeEffectSink;
        let mut source = ScriptedInputSource::of_keys([KeyCode::Enter]);

        let tick = expect_tick(
            drain_input_burst(
                &mut state,
                &projection,
                &events,
                "operator",
                &mut effects,
                &mut session,
                &mut source,
            ),
            "drain_input_burst does not fail against a scripted source",
        );

        assert_eq!(tick, LoopTick::Mutated);
    }

    #[test]
    #[should_panic(expected = "boom")]
    fn expect_tick_panics_on_error() {
        expect_tick(Err(std::io::Error::other("boom")), "boom");
    }

    #[test]
    #[should_panic(expected = "boom")]
    fn expect_more_input_panics_on_error() {
        expect_more_input(Err(std::io::Error::other("boom")), "boom");
    }

    /// An [`InputSource`] whose `read` always fails -- exercises
    /// `drain_input_burst`'s `?` over `InputSource::read`. Ordinary error
    /// handling on an injected dependency, not "genuinely unreachable" code
    /// (CLAUDE.md's hidden-global heuristic): a fallible call on an injected
    /// port is testable, so it is tested here.
    struct AlwaysFailsRead;

    impl InputSource for AlwaysFailsRead {
        fn poll_now(&mut self) -> std::io::Result<bool> {
            Ok(false)
        }

        fn read(&mut self) -> std::io::Result<crossterm::event::Event> {
            Err(std::io::Error::other("scripted read failure"))
        }
    }

    /// An [`InputSource`] that returns ONE seeded event from `read`, then
    /// fails every subsequent `poll_now` call -- exercises `drain_input_burst`'s
    /// three `?`s over `InputSource::poll_now` (after a non-press key, after
    /// an unmapped key, and after a coalesced navigation move), by varying
    /// which event is seeded.
    struct FailsPollAfterOneEvent {
        event: Option<crossterm::event::Event>,
    }

    impl FailsPollAfterOneEvent {
        fn new(event: crossterm::event::Event) -> Self {
            Self { event: Some(event) }
        }
    }

    impl InputSource for FailsPollAfterOneEvent {
        fn poll_now(&mut self) -> std::io::Result<bool> {
            Err(std::io::Error::other("scripted poll_now failure"))
        }

        fn read(&mut self) -> std::io::Result<crossterm::event::Event> {
            // Every caller seeds an event and the burst never reads twice
            // before the SUBSEQUENT `poll_now` errors and ends it (see this
            // type's own doc comment), so the fallback below is never
            // actually reached -- a plain value rather than a closure, so it
            // carries no SEPARATE instantiation group for the coverage gate
            // to account for.
            Ok(self
                .event
                .take()
                .unwrap_or(crossterm::event::Event::FocusLost))
        }
    }

    /// A session whose `handle_runtime_effect` always fails -- exercises
    /// `drain_input_burst`'s `?` over `TuiRuntimeEffectSink::handle_runtime_effect`.
    struct FailingSink;

    impl TuiRuntimeEffectSink for FailingSink {
        fn handle_runtime_effect(
            &mut self,
            _effect: &TuiRuntimeEffect,
        ) -> std::io::Result<TuiRuntimeEffectSinkOutcome> {
            Err(std::io::Error::other("scripted sink failure"))
        }
    }

    impl TuiLiveSession for FailingSink {
        fn refresh_events(
            &mut self,
            _request_poll: bool,
        ) -> std::io::Result<Option<Vec<ConsoleEvent>>> {
            Ok(None)
        }
    }

    /// `AlwaysFailsRead::poll_now` and `FailingSink::refresh_events` are both
    /// present only to satisfy their traits -- `drain_input_burst` never
    /// reaches either (its own `read`/`handle_runtime_effect` failures
    /// short-circuit first). Exercised directly rather than left untested.
    #[test]
    fn the_unreached_trait_methods_on_the_failure_fixtures_still_behave() {
        let mut always_fails_read = AlwaysFailsRead;
        assert!(!expect_more_input(
            always_fails_read.poll_now(),
            "AlwaysFailsRead::poll_now"
        ));

        let mut failing_sink = FailingSink;
        assert!(
            failing_sink
                .refresh_events(false)
                .is_ok_and(|events| events.is_none())
        );
    }

    /// livespec-console-beads-fabro-mx9u.9: `drain_input_burst` propagates a
    /// failed `InputSource::read` rather than swallowing it.
    #[test]
    fn drain_input_burst_propagates_a_read_failure() {
        let events = many_attention_rows(40);
        let mut state = content_focused_state(0);
        let projection = console_application::project_tui_events(&events, None);
        let mut effects = Vec::new();
        let mut session = PanicsOnRefresh;
        let mut source = AlwaysFailsRead;

        let result = drain_input_burst(
            &mut state,
            &projection,
            &events,
            "operator",
            &mut effects,
            &mut session,
            &mut source,
        );

        assert!(result.is_err());
    }

    /// A failed `poll_now` after a non-press key propagates rather than being
    /// swallowed.
    #[test]
    fn drain_input_burst_propagates_a_poll_failure_after_a_non_press_key() {
        let events = many_attention_rows(40);
        let mut state = content_focused_state(0);
        let projection = console_application::project_tui_events(&events, None);
        let mut effects = Vec::new();
        let mut session = PanicsOnRefresh;
        let mut source = FailsPollAfterOneEvent::new(crossterm::event::Event::Key(
            crossterm::event::KeyEvent::new_with_kind(
                KeyCode::Down,
                KeyModifiers::empty(),
                crossterm::event::KeyEventKind::Release,
            ),
        ));

        let result = drain_input_burst(
            &mut state,
            &projection,
            &events,
            "operator",
            &mut effects,
            &mut session,
            &mut source,
        );

        assert!(result.is_err());
    }

    /// A failed `poll_now` after an unmapped key propagates rather than being
    /// swallowed.
    #[test]
    fn drain_input_burst_propagates_a_poll_failure_after_an_unmapped_key() {
        let events = many_attention_rows(40);
        let mut state = content_focused_state(0);
        let projection = console_application::project_tui_events(&events, None);
        let mut effects = Vec::new();
        let mut session = PanicsOnRefresh;
        let mut source =
            FailsPollAfterOneEvent::new(crossterm::event::Event::Key(key(KeyCode::Home)));

        let result = drain_input_burst(
            &mut state,
            &projection,
            &events,
            "operator",
            &mut effects,
            &mut session,
            &mut source,
        );

        assert!(result.is_err());
    }

    /// A failed `poll_now` checked after successfully coalescing one
    /// navigation move propagates rather than being swallowed.
    #[test]
    fn drain_input_burst_propagates_a_poll_failure_after_a_coalesced_move() {
        let events = many_attention_rows(40);
        let mut state = content_focused_state(0);
        let projection = console_application::project_tui_events(&events, None);
        let mut effects = Vec::new();
        let mut session = PanicsOnRefresh;
        let mut source =
            FailsPollAfterOneEvent::new(crossterm::event::Event::Key(key(KeyCode::Down)));

        let result = drain_input_burst(
            &mut state,
            &projection,
            &events,
            "operator",
            &mut effects,
            &mut session,
            &mut source,
        );

        assert!(result.is_err());
        // The move itself landed before the poll failure was checked.
        assert_eq!(state.selected_attention_index(), 1);
    }

    /// A failed `handle_runtime_effect` propagates rather than being
    /// swallowed.
    #[test]
    fn drain_input_burst_propagates_a_sink_failure() {
        let events = many_attention_rows(40);
        let mut state = content_focused_state(0);
        let projection = console_application::project_tui_events(&events, None);
        let mut effects = Vec::new();
        let mut session = FailingSink;
        let mut source = ScriptedInputSource::of_keys([KeyCode::Down]);

        let result = drain_input_burst(
            &mut state,
            &projection,
            &events,
            "operator",
            &mut effects,
            &mut session,
            &mut source,
        );

        assert!(result.is_err());
    }

    /// `step_tui_runtime_with_model` against an already-built model produces
    /// the SAME step `step_tui_runtime` does building its own -- it is the
    /// cheap half `drain_input_burst` calls per key, not a different reducer.
    #[test]
    fn step_tui_runtime_with_model_matches_step_tui_runtime_over_the_same_state() {
        let state = TuiInteractionState::new(0, TuiOverlay::None);
        let events = demo_events();
        let model = build_tui_model_for_state(&events, &state);

        let via_model = step_tui_runtime_with_model(
            &state,
            &model,
            &events,
            TuiTerminalInput::Interaction(TuiInteraction::SelectNext),
            "operator",
        );
        let via_events = step_tui_runtime(
            &state,
            &events,
            TuiTerminalInput::Interaction(TuiInteraction::SelectNext),
            "operator",
        );

        assert_eq!(via_model.state(), via_events.state());
        assert_eq!(via_model.effect(), via_events.effect());
    }

    #[test]
    fn is_navigation_interaction_is_true_only_for_attention_list_movement() {
        assert!(is_navigation_interaction(&TuiTerminalInput::Interaction(
            TuiInteraction::SelectNext
        )));
        assert!(is_navigation_interaction(&TuiTerminalInput::Interaction(
            TuiInteraction::SelectPrevious
        )));
        assert!(!is_navigation_interaction(&TuiTerminalInput::Interaction(
            TuiInteraction::CloseOverlay
        )));
        assert!(!is_navigation_interaction(&TuiTerminalInput::Confirm));
        assert!(!is_navigation_interaction(&TuiTerminalInput::Quit));
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
        for view in [TuiView::Spec, TuiView::Repos] {
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
        // Events is a container: on its own overview Enter drills into the
        // selected sub-view, exactly as the Lanes overview's Enter drills into
        // a lane; only a DRILLED-IN sub-view leaves Enter inert (no per-row
        // action surface yet).
        let events_overview = build_tui_model_for_state(
            &demo_events(),
            &TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None)
                .with_focus(FocusPane::Content),
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &events_overview),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::DrillIntoEventsSubView
            ))
        );
        let events_drilled = build_tui_model_for_state(
            &demo_events(),
            &reduce_tui_interaction(
                &TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None)
                    .with_focus(FocusPane::Content),
                &demo_events(),
                TuiInteraction::DrillIntoEventsSubView,
            ),
        );
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Enter), &events_drilled),
            None
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
    fn keymap_left_from_lanes_overview_content_returns_to_nav() {
        let model = lanes_model_content(LaneFocus::Overview, TuiOverlay::None);

        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Left), &model),
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
    fn the_header_pane_title_names_the_running_builds_sha_and_build_timestamp() {
        // livespec-console-beads-fabro-mx9u.13, acceptance criterion 1: the
        // chrome names the running build's sha AND build timestamp. The
        // TITLE is where it lives (not a content-line field -- see
        // `header_pane_title`'s doc), so this is the rendered-text
        // proof for that placement, blurred and focused alike (the title is
        // set on both branches of `render_header`).
        let state = TuiInteractionState::new(0, TuiOverlay::None).with_build_identity(Some(
            console_application::build_identity::BuildIdentity::new(
                "923a5a5",
                "2026-09-08T23:27:15Z",
            ),
        ));
        let blurred = render_to_text(&build_tui_model_for_state(&demo_events(), &state), 100, 12)
            .unwrap_or_default();
        assert!(
            blurred.contains("LiveSpec Console — build 923a5a5 (built 2026-09-08T23:27:15Z)"),
            "{blurred}"
        );

        let focused = render_to_text(
            &build_tui_model_for_state(&demo_events(), &state.with_focus(FocusPane::Header)),
            100,
            12,
        )
        .unwrap_or_default();
        assert!(
            focused
                .contains("LiveSpec Console [focus] — build 923a5a5 (built 2026-09-08T23:27:15Z)"),
            "{focused}"
        );
    }

    #[test]
    fn the_header_pane_title_falls_back_to_the_bare_name_with_no_build_identity() {
        // Every other test in this module builds a state without
        // `.with_build_identity`, so this pins the default explicitly rather
        // than leaving it implicit.
        let state = TuiInteractionState::new(0, TuiOverlay::None);
        let frame = render_to_text(&build_tui_model_for_state(&demo_events(), &state), 100, 12)
            .unwrap_or_default();
        assert!(frame.contains("LiveSpec Console"), "{frame}");
        assert!(!frame.contains("build "), "{frame}");
    }

    #[test]
    fn the_header_pane_title_survives_a_crowded_content_line_that_sheds_everything_else() {
        // livespec-console-beads-fabro-mx9u.13 review: the maintainer's own
        // real, busy header (a factory alert AND several unavailable
        // sources beside a two-digit attention count) left no room for the
        // build identity as a content-line field, even in a short form --
        // which is exactly why it moved to the title. This proves the
        // title survives that SAME crowded shape, since it costs nothing
        // from the content-line budget the crowding exhausts.
        let events = [
            ConsoleEvent::fixture(
                "evt_dispatcher_not_observed",
                EventType::SourceNotObservedFindingObserved,
                "dispatcher",
            ),
            ConsoleEvent::fixture(
                "evt_fabro_not_observed",
                EventType::SourceNotObservedFindingObserved,
                "fabro",
            ),
            ConsoleEvent::fixture(
                "evt_github_not_observed",
                EventType::SourceNotObservedFindingObserved,
                "github",
            ),
            ConsoleEvent::fixture(
                "evt_dispatch_item_not_wired",
                EventType::FactoryDispatchItemNotWired,
                "factory",
            ),
        ];
        let state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_selected_repo("livespec-console-beads-fabro".to_owned())
            .with_build_identity(Some(
                console_application::build_identity::BuildIdentity::new(
                    "923a5a5",
                    "2026-09-08T23:27:15Z",
                ),
            ));
        let frame = render_to_text(&build_tui_model_for_state(&events, &state), 159, 12)
            .unwrap_or_default();
        assert!(
            frame.contains("LiveSpec Console — build 923a5a5 (built 2026-09-08T23:27:15Z)"),
            "the title should survive even a crowded content line: {frame}"
        );
    }

    #[test]
    fn the_focus_marker_in_the_header_title_survives_truncation_at_a_narrow_width() {
        // Regression: `ratatui` truncates a block title that overflows the
        // border from the RIGHT. When the `[focus]` marker was appended
        // AFTER the (longer, unbounded-length) build segment, a merely-
        // narrow real pane (caught live at `NARROW_COLS` = 56 in
        // `tmux_tui_e2e_top_pane_focus_hscroll`) truncated the marker away
        // entirely: the captured title read `LiveSpec Console — build
        // e893bf2 (built 2026-09-09T00:` with no `[focus]` anywhere. The
        // fix orders the marker right after the bare name (see
        // `header_pane_title`'s doc); this pins that a focused header at
        // that same narrow width still shows the marker regardless of how
        // long the build segment is.
        let state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_focus(FocusPane::Header)
            .with_build_identity(Some(
                console_application::build_identity::BuildIdentity::new(
                    "e893bf2",
                    "2026-09-09T00:18:37Z",
                ),
            ));
        let frame = render_to_text(&build_tui_model_for_state(&demo_events(), &state), 56, 12)
            .unwrap_or_default();
        assert!(frame.contains("LiveSpec Console [focus]"), "{frame}");
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
        //
        // Viewport height is 80, not the round 24 this test used before the
        // event-source vocabulary was added to this section
        // (livespec-console-beads-fabro-mx9u.19): the section grew by the
        // event-source definitions and the roster
        // (`event_source_roster_help_lines`), so an unscrolled 24-row frame
        // no longer reaches text this far down the section. What is under
        // test is that the text IS there at `scroll: 0`, not a specific
        // terminal height, and the real Help modal scrolls regardless.
        let state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_overlay(TuiOverlay::Help {
                focus: HelpFocus::Menu,
                selected_section: header_help_section(),
                scroll: 0,
            })
            .with_focus(FocusPane::Header);
        let frame = render_to_text(&build_tui_model_for_state(&demo_events(), &state), 100, 80)
            .unwrap_or_default();
        assert!(frame.contains("Header"));
        assert!(frame.contains("scroll the focused header"));
    }

    #[test]
    fn help_overlay_names_where_the_build_tell_lives_and_what_stale_means() {
        // livespec-console-beads-fabro-mx9u.13, acceptance criterion 4: `?`
        // Help must name where the build tell lives and what its stale form
        // means, not merely leave an operator to infer it from the header.
        //
        // Viewport height bumped for the same reason as the sibling test
        // above: the section grew with the mx9u.19 event-source vocabulary.
        let state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_overlay(TuiOverlay::Help {
                focus: HelpFocus::Menu,
                selected_section: header_help_section(),
                scroll: 0,
            })
            .with_focus(FocusPane::Header);
        let frame = render_to_text(&build_tui_model_for_state(&demo_events(), &state), 100, 80)
            .unwrap_or_default();
        assert!(frame.contains("LiveSpec Console — build <sha> (built"));
        assert!(frame.contains("build STALE: N commits behind"));
        assert!(frame.contains("RUNNING BINARY's compiled-in"));
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
            Some("hp".to_owned()),
            vec!["drive resolve-blocked:work-item:ready".to_owned()],
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
            Some("hp".to_owned()),
            vec!["drive resolve-blocked:bd-ib-ss7rkr:ready".to_owned()],
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
        // Left enters the menu in place: Esc owns backing out to the lane
        // overview, while hotkey-free menu entry preserves the item selection.
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Left), &drilled),
            Some(TuiTerminalInput::Interaction(TuiInteraction::OpenMenu))
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
            &verb_free_attention_events(),
            TuiTerminalInput::Confirm,
            "operator",
        );

        assert_eq!(step.effect(), &TuiRuntimeEffect::Render);
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

        // Confirm with no overlay open has nothing to act on. It used to fall
        // through to a resolver whose only resolvable actions were the two
        // Fabro-attach ones (hence the "no selected action" wording); with the
        // attach handoff retired the arm refuses directly, and the refusal says
        // so.
        assert_eq!(
            step.effect(),
            &TuiRuntimeEffect::ApplicationError(
                console_application::ApplicationError::UnavailableOperatorAction
            )
        );
        assert_eq!(persisted_command(step.effect()), None);
    }

    /// A blank requester is still reported as the bad request it is.
    ///
    /// The refusal above must not swallow it: an empty `requested_by` is a
    /// caller defect, and collapsing the two would hide it behind "nothing to
    /// do here".
    #[test]
    fn runtime_step_rejects_a_blank_requester_before_refusing() {
        let state = TuiInteractionState::new(0, TuiOverlay::None);

        let step = step_tui_runtime(&state, &demo_events(), TuiTerminalInput::Confirm, "  ");

        assert_eq!(
            step.effect(),
            &TuiRuntimeEffect::ApplicationError(
                console_application::ApplicationError::EmptyOperatorAction
            )
        );
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
                .map(|rendered| rendered.contains("> 1 Attention")),
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
        // The selected `blocked` row's state-admitted verb, offered on the inbox
        // surface exactly as it is in the drilled-in lane (Scenario 31). This
        // assertion used to read `!contains("Actions:")`, which pinned the
        // drill-only surface split rather than any region of the layout.
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("Actions: Move status")),
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
                .map(|rendered| rendered.contains("event sources: 3 unavailable")),
            Ok(true)
        );

        // Wide: with room to spare the header names which sources are down.
        let wide = render_to_text(&blind, 160, 24);
        assert_eq!(
            wide.as_ref()
                .map(|rendered| rendered.contains("event sources: 3 unavailable")),
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
    fn render_to_text_events_container_presents_its_two_sub_views_first() {
        // AC2: selecting Events presents its two sub-views -- the container's
        // own picker home -- before either is drilled into.
        let state = TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None);
        let model = build_tui_model_for_state(&factory_events(), &state);

        let output = render_to_text(&model, 96, 24).unwrap_or_default();

        assert!(output.contains("> 4 Events"), "{output}");
        assert!(output.contains("Stored events"), "{output}");
        assert!(output.contains("Event sources"), "{output}");
    }

    #[test]
    fn render_to_text_draws_non_attention_view_summary() {
        // AC3: drilled into "Stored events", the sub-view renders EXACTLY what
        // the pre-container Events view rendered -- no behaviour change.
        let overview = TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None);
        let state = reduce_tui_interaction(
            &overview,
            &factory_events(),
            TuiInteraction::DrillIntoEventsSubView,
        );
        let model = build_tui_model_for_state(&factory_events(), &state);

        let output = render_to_text(&model, 96, 24);

        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("> 4 Events")),
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

    /// One `factory.run_orphans_observed` event, exactly as the reconciler
    /// source adapter writes it, carrying two orphaned runs: one whose
    /// work-item is merely inactive, and one whose work-item has left the
    /// ledger entirely and whose factory reported no status kind.
    fn orphan_projection_event() -> ConsoleEvent {
        let snapshot = ReconcileRunsSnapshot::new(
            vec![
                OrphanedFactoryRun::new(
                    "01M1ES066RHS8Y39B9WJW8WC8Q",
                    "hp",
                    "running",
                    "livespec-console-beads-fabro-h7jp",
                    Some("blocked"),
                    "item-not-active",
                    "none",
                ),
                OrphanedFactoryRun::new(
                    "01M1F34G6NY83A6Y24DJQCGDHQ",
                    "local",
                    "",
                    "livespec-console-beads-fabro-gone",
                    None,
                    "item-missing",
                    "export-then-terminate",
                ),
            ],
            Vec::new(),
        );
        ConsoleEvent::new(
            "evt_orphans".to_owned(),
            1,
            "console".to_owned(),
            EventType::FactoryRunOrphansObserved,
            "reconcile-runs".to_owned(),
            "reconcile_runs:console".to_owned(),
            1,
        )
        .with_payload_json(reconcile_runs_snapshot_payload_json(&snapshot))
    }

    /// Every field of the reconciler's projection reaches the operator: run id,
    /// factory, status kind, work-item id and status, orphan reason, and the
    /// remedy the orchestrator prescribes.
    #[test]
    fn render_to_text_draws_the_orphaned_factory_runs_lane() {
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None);
        let model = build_tui_model_for_state(&[orphan_projection_event()], &state);

        let output = render_to_text(&model, 200, 32);

        assert_eq!(
            output
                .as_ref()
                .map(|r| r.contains("orphaned factory runs (2)")),
            Ok(true)
        );
        assert_eq!(
            output.as_ref().map(|r| r.contains(
                "- 01M1ES066RHS8Y39B9WJW8WC8Q on hp [running]  \
                 livespec-console-beads-fabro-h7jp [blocked]  (item-not-active)  remedy none"
            )),
            Ok(true)
        );
        // The work-item left the ledger, so it HAS no status: absent stays
        // absent rather than being collapsed to a placeholder the ledger never
        // held. The factory reported no status kind, which resolves to the
        // NEUTRAL `unknown` -- never to a synthesized human gate.
        assert_eq!(
            output.as_ref().map(|r| r.contains(
                "- 01M1F34G6NY83A6Y24DJQCGDHQ on local [unknown]  \
                 livespec-console-beads-fabro-gone  (item-missing)  remedy export-then-terminate"
            )),
            Ok(true)
        );
    }

    /// Every row is rendered, not a preview: unlike a lifecycle lane, whose top
    /// items sample a list the operator can drill into, an orphan silently
    /// dropped past the preview cap is a slot nobody is watching.
    #[test]
    fn the_orphaned_runs_lane_renders_every_row_rather_than_a_preview() {
        let runs = (0..LANE_OVERVIEW_PREVIEW + 2)
            .map(|index| {
                OrphanedFactoryRun::new(
                    &format!("01M1RUN{index}"),
                    "hp",
                    "running",
                    &format!("console-orphan-{index}"),
                    Some("blocked"),
                    "superseded-run",
                    "none",
                )
            })
            .collect();
        let event = ConsoleEvent::new(
            "evt_orphans".to_owned(),
            1,
            "console".to_owned(),
            EventType::FactoryRunOrphansObserved,
            "reconcile-runs".to_owned(),
            "reconcile_runs:console".to_owned(),
            1,
        )
        .with_payload_json(reconcile_runs_snapshot_payload_json(
            &ReconcileRunsSnapshot::new(runs, Vec::new()),
        ));
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None);
        let model = build_tui_model_for_state(&[event], &state);

        let output = render_to_text(&model, 200, 40);

        assert_eq!(
            output
                .as_ref()
                .map(|r| r.contains("orphaned factory runs (5)")),
            Ok(true)
        );
        for index in 0..LANE_OVERVIEW_PREVIEW + 2 {
            assert_eq!(
                output
                    .as_ref()
                    .map(|r| r.contains(&format!("- 01M1RUN{index} on hp"))),
                Ok(true)
            );
        }
    }

    /// A clean board still STATES that it is clean. A section that vanished at
    /// zero would be indistinguishable from one the console forgot to render.
    #[test]
    fn the_orphaned_runs_lane_states_an_empty_projection() {
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None);
        let model = build_tui_model_for_state(&lane_render_events(), &state);

        let output = render_to_text(&model, 200, 32);

        assert_eq!(
            output
                .as_ref()
                .map(|r| r.contains("orphaned factory runs (0)")),
            Ok(true)
        );
    }

    /// The honest marker the adapter appends when a poll could not read a
    /// source, keyed on the same source name the lane snapshots carry.
    fn orchestrator_not_observed_event(event_id: &str) -> ConsoleEvent {
        ConsoleEvent::fixture(
            event_id,
            EventType::SourceNotObservedFindingObserved,
            "orchestrator",
        )
        .with_payload_json(
            r#"{"repo":"console","source":"orchestrator","reason":"source command exited non-zero"}"#
                .to_owned(),
        )
    }

    /// `livespec-console-beads-fabro-v8un`, operator rider [2]. A row whose
    /// backing source could not be read on the latest poll is serving
    /// last-known values, and the row SAYS SO instead of rendering them as
    /// confirmed. The marker sits with the lifecycle fields so it survives the
    /// truncation a narrow pane applies.
    #[test]
    fn render_to_text_marks_a_lane_row_whose_source_was_not_observed() {
        let mut events = lane_render_events().to_vec();
        events.push(orchestrator_not_observed_event("evt_not_observed"));
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_lane_focus(LaneFocus::Lane(Lane::Ready));

        let model = build_tui_model_for_state(&events, &state);
        let output = render_to_text(&model, 96, 24);

        assert_eq!(
            output
                .as_ref()
                .map(|r| r.contains("console-ready-a  rank a0  [ready]  (unconfirmed)")),
            Ok(true)
        );
        // The CONTROL: with every source observed, no row is marked.
        let confirmed = build_tui_model_for_state(&lane_render_events(), &state);
        assert_eq!(
            render_to_text(&confirmed, 96, 24)
                .as_ref()
                .map(|r| r.contains("unconfirmed")),
            Ok(false)
        );
    }

    /// The same rider's sharper half: an ABSENT policy on a row the console
    /// could not read must NOT render as the console's assumed default, because
    /// that shows "unknown" and "unset" identically. A positive read stays a
    /// reading; a read source that emitted nothing stays an honest unset.
    #[test]
    fn an_unreadable_policy_is_reported_as_unread_not_as_the_console_default() {
        assert_eq!(
            super::policy_field(Some("ai-only"), "ai-then-human", true),
            "ai-only"
        );
        assert_eq!(
            super::policy_field(None, "ai-then-human", true),
            "\u{2014} (not emitted; console assumes ai-then-human)"
        );
        assert_eq!(
            super::policy_field(None, "ai-then-human", false),
            "\u{2014} (not read; the orchestrator source was not observed on the latest poll)"
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
    fn render_to_text_reports_when_the_anchored_lane_selection_disappears() {
        let before = [
            lane_event(
                "evt_ready_stay_before",
                "console-ready-stay",
                Lane::Ready,
                None,
                "a0",
                "ready",
            ),
            lane_event(
                "evt_ready_vanish_before",
                "console-ready-vanish",
                Lane::Ready,
                None,
                "b0",
                "ready",
            ),
        ];
        let starting = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_lane_focus(LaneFocus::Lane(Lane::Ready));
        let selected = reduce_tui_interaction(&starting, &before, TuiInteraction::SelectNext);
        let after = [lane_event(
            "evt_ready_stay_after",
            "console-ready-stay",
            Lane::Ready,
            None,
            "a0",
            "ready",
        )];
        let model = build_tui_model_for_state(&after, &selected);

        let output = render_to_text(&model, 96, 24);

        assert_eq!(
            output.as_ref().map(|r| {
                r.contains("console-ready-vanish is no longer in this lane; selection not moved")
            }),
            Ok(true)
        );
        assert_eq!(model.selected_work_item_id(), None);

        let empty_lane = build_tui_model_for_state(&[], &selected);
        let empty_output = render_to_text(&empty_lane, 96, 24);
        assert_eq!(
            empty_output.as_ref().map(|r| {
                r.contains("console-ready-vanish is no longer in this lane; selection not moved")
            }),
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
        // A GENUINELY empty inbox (the session is not loading) must not look
        // like a still-loading one -- livespec-console-beads-fabro-pzbdbo.27
        // AC3, the counterpart to the loading case below.
        let rendered = output.unwrap_or_default();
        assert!(!rendered.contains(ATTENTION_LOADING_PLACEHOLDER));
    }

    #[test]
    // livespec-console-beads-fabro-pzbdbo.27 AC2: a source that has not
    // returned yet is shown as LOADING, not as unavailable and not as an
    // unlabeled empty pane -- distinguishing this from the true-empty case
    // right above it is the whole point.
    fn render_to_text_shows_loading_not_empty_while_startup_ingest_is_pending() {
        let state = TuiInteractionState::new(0, TuiOverlay::None).with_startup_ingest_pending(true);
        let model = build_tui_model_for_state(&[], &state);

        // Wide enough that the placeholder is not clipped by the Attention
        // pane's width -- this asserts the TEXT, not the pane's truncation
        // behaviour (which `attention_item_line`'s own elision tests cover).
        let rendered = render_to_text(&model, 200, 16).unwrap_or_default();

        assert!(
            rendered.contains(ATTENTION_LOADING_PLACEHOLDER),
            "an empty inbox during startup ingest must read as loading, not \
             empty: {rendered}"
        );
        assert!(!rendered.contains("unavailable"));
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
    fn render_to_text_draws_factory_dispatch_item_confirm_overlay() {
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::FactoryDispatchItemConfirm {
                work_item_id: "console-selected".to_owned(),
            },
        );
        let model = build_tui_model_for_state(&demo_events(), &state);

        let output = render_to_text(&model, 96, 24).unwrap_or_default();

        assert!(output.contains("Factory Dispatch"));
        assert!(output.contains("Dispatch selected work-item"));
        assert!(output.contains("Target: console-selected"));
        assert!(output.contains("loop --budget 1 --parallel 1 --item"));
        assert!(output.contains("enter dispatch selected item"));
    }

    #[test]
    fn render_command_modal_draws_available_actions() {
        let detail = AttentionDetail::new(
            "repo".to_owned(),
            "work-item".to_owned(),
            "run".to_owned(),
            None,
            vec![],
            vec![],
            vec![
                OperatorAction::Registered("approve"),
                OperatorAction::Registered("accept"),
            ],
        );
        let area = Rect::new(0, 0, 40, 6);
        let mut buffer = Buffer::empty(area);

        render_command_modal(Some(&detail), 1, area, &mut buffer);
        let output = buffer_to_text(&buffer, area);

        assert!(output.contains("Approve work-item"));
        assert!(output.contains("> Accept work-item"));
    }

    #[test]
    fn command_modal_enter_drills_into_explainer_before_staging() {
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::CommandModal {
                selected_action_index: 0,
            },
        );
        let step = step_tui_runtime(&state, &pending_events(), TuiTerminalInput::Confirm, "op");

        assert_eq!(
            step.state().overlay(),
            &TuiOverlay::CommandExplainer {
                selected_action_index: 0
            }
        );
        assert_eq!(step.effect(), &TuiRuntimeEffect::Render);
    }

    #[test]
    fn command_explainer_enter_continues_through_existing_action_path() {
        let events = pending_events();
        let selected_action_index = detail_action_index(
            &build_tui_model_for_state(&events, &TuiInteractionState::new(0, TuiOverlay::None)),
            "approve",
        );
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::CommandExplainer {
                selected_action_index,
            },
        );
        let step = step_tui_runtime(&state, &events, TuiTerminalInput::Confirm, "op");

        assert_eq!(
            step.state().overlay(),
            &TuiOverlay::ValveConfirm {
                valve: PendingValve::Approve,
                answer: String::new(),
            }
        );
        assert_eq!(step.effect(), &TuiRuntimeEffect::Render);
    }

    #[test]
    fn command_explainer_uses_a_full_terminal_width_command_line() {
        let detail = AttentionDetail::new(
            "repo".to_owned(),
            "work-item".to_owned(),
            "run".to_owned(),
            None,
            vec![],
            vec![],
            vec![OperatorAction::Registered("set-acceptance-rework-cap")],
        );
        let area = Rect::new(0, 0, 80, 24);
        let overlay = full_width_explainer_rect(area);
        let mut buffer = Buffer::empty(area);

        render_command_explainer(Some(&detail), 0, overlay, &mut buffer);
        let output = buffer_to_text(&buffer, area);
        let command_row = output
            .lines()
            .find(|line| line.contains("registry action: set-acceptance-rework-cap"))
            .unwrap_or_default();

        assert_eq!(overlay.x, area.x);
        assert_eq!(overlay.width, area.width);
        assert!(command_row.contains("registry action: set-acceptance-rework-cap"));
        assert!(!command_row.contains("Registry id:"));
    }

    #[test]
    fn command_explainer_prose_is_derived_from_the_registry_entry() {
        let detail = AttentionDetail::new(
            "repo".to_owned(),
            "work-item".to_owned(),
            "run".to_owned(),
            None,
            vec![],
            vec![],
            vec![OperatorAction::Registered("approve")],
        );
        let area = Rect::new(0, 0, 80, 24);
        let mut buffer = Buffer::empty(area);

        render_command_explainer(
            Some(&detail),
            0,
            full_width_explainer_rect(area),
            &mut buffer,
        );
        let output = buffer_to_text(&buffer, area);
        let approve = action_registry::action_for_id("approve");
        assert_eq!(
            approve.map(|spec| output.contains(&format!("Registry id: {}", spec.id))),
            Some(true)
        );
        assert_eq!(
            approve
                .map(|spec| output.contains(&format!("Menu path: {}", spec.menu_path.join(" > ")))),
            Some(true)
        );
        assert_eq!(
            approve.map(|spec| output.contains(&format!(
                "Accelerator: {}",
                action_registry::accelerator_display(spec)
            ))),
            Some(true)
        );
    }

    #[test]
    fn command_explainer_overlay_renders_through_the_model_overlay_path() {
        let selected_action_index = detail_action_index(
            &attention_model_for_lane(Lane::PendingApproval, TuiOverlay::None),
            "approve",
        );
        let model = attention_model_for_lane(
            Lane::PendingApproval,
            TuiOverlay::CommandExplainer {
                selected_action_index,
            },
        );
        let output = render_to_text(&model, 100, 30).unwrap_or_default();

        assert!(output.contains("Command Explainer"));
        assert!(output.contains("registry action: approve"));
    }

    #[test]
    fn command_explainer_lines_cover_known_and_unknown_registry_actions() {
        let known = command_explainer_lines(OperatorAction::Registered("approve"))
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let unknown = command_explanation_for_action(OperatorAction::Registered("missing-action"))
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(known.contains("registry action: approve"));
        assert!(known.contains("Registry id: approve"));
        assert!(unknown.contains("registry entry for this action is not available"));
    }

    #[test]
    fn command_explainer_confirm_stages_the_selected_detail_action() {
        let events = pending_events();
        let selected_action_index = detail_action_index(
            &build_tui_model_for_state(&events, &TuiInteractionState::new(0, TuiOverlay::None)),
            "approve",
        );
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::CommandExplainer {
                selected_action_index,
            },
        );
        let model = build_tui_model_for_state(&events, &state);

        let step =
            command_explainer_confirm_step(&state, &events, &model, selected_action_index, "op");

        assert_eq!(
            step.state().overlay(),
            &TuiOverlay::ValveConfirm {
                valve: PendingValve::Approve,
                answer: String::new(),
            }
        );
        assert_eq!(step.effect(), &TuiRuntimeEffect::Render);
    }

    #[test]
    fn command_explainer_render_is_inert_without_a_selected_action() {
        let detail = AttentionDetail::new(
            "repo".to_owned(),
            "work-item".to_owned(),
            "run".to_owned(),
            None,
            vec![],
            vec![],
            vec![],
        );
        let area = Rect::new(0, 0, 40, 8);
        let mut buffer = Buffer::empty(area);
        let before = buffer_to_text(&buffer, area);

        render_command_explainer(Some(&detail), 0, area, &mut buffer);

        assert_eq!(buffer_to_text(&buffer, area), before);
    }

    #[test]
    fn registry_staging_explainer_covers_every_staging_shape() {
        for action_id in [
            "approve",
            "set-admission",
            "driver-handoff",
            "dispatch-ready",
            "dispatch-selected-item",
            "open-help",
        ] {
            let rendered = action_registry::action_for_id(action_id)
                .map(registry_staging_explanation)
                .unwrap_or_default();
            assert!(!rendered.is_empty(), "{action_id}");
        }
        let no_payload = action_registry::action_for_id("driver-handoff")
            .map(registry_staging_explanation)
            .unwrap_or_default();
        assert!(no_payload.contains("driver-handoff overlay"));
    }

    #[test]
    fn staged_action_step_covers_every_registry_continuation_shape() {
        let state = TuiInteractionState::new(0, TuiOverlay::None);
        let events = pending_events();
        let model = attention_model_for_lane(Lane::PendingApproval, TuiOverlay::None);

        let no_action = command_explainer_confirm_step(&state, &events, &model, 99, "op");
        assert_eq!(
            no_action.effect(),
            &TuiRuntimeEffect::ApplicationError(
                console_application::ApplicationError::NoSelectedOperatorAction
            )
        );

        let valve = staged_action_step(
            &state,
            &events,
            Some(action_registry::StagedAction::Valve(PendingValve::Approve)),
            "op",
        );
        assert_eq!(valve.effect(), &TuiRuntimeEffect::Render);

        let handoff = staged_action_step(
            &state,
            &events,
            Some(action_registry::StagedAction::DriverHandoff),
            "op",
        );
        assert_eq!(handoff.effect(), &TuiRuntimeEffect::Render);

        let drain = staged_action_step(
            &state,
            &[lane_event(
                "evt_staged_drain",
                "console-staged-drain",
                Lane::Ready,
                None,
                "a0",
                "ready",
            )],
            Some(action_registry::StagedAction::FactoryDrain),
            "op",
        );
        assert_eq!(
            drain.state().overlay(),
            &TuiOverlay::FactoryDrainConfirm {
                work_item_id: "console-staged-drain".to_owned(),
                rank: "a0".to_owned(),
            }
        );
        assert_eq!(drain.effect(), &TuiRuntimeEffect::Render);

        let dispatch_item = staged_action_step(
            &state,
            &events,
            Some(action_registry::StagedAction::FactoryDispatchItem),
            "op",
        );
        assert_eq!(dispatch_item.effect(), &TuiRuntimeEffect::Render);

        let help = staged_action_step(
            &state,
            &events,
            Some(action_registry::StagedAction::Global(
                action_registry::GlobalAction::OpenHelp,
            )),
            "op",
        );
        assert_eq!(help.effect(), &TuiRuntimeEffect::Render);

        let quit = staged_action_step(
            &state,
            &events,
            Some(action_registry::StagedAction::Global(
                action_registry::GlobalAction::Quit,
            )),
            "op",
        );
        assert_eq!(quit.effect(), &TuiRuntimeEffect::Quit);

        let unavailable = staged_action_step(&state, &events, None, "op");
        assert_eq!(
            unavailable.effect(),
            &TuiRuntimeEffect::ApplicationError(
                console_application::ApplicationError::UnavailableOperatorAction
            )
        );
    }

    #[test]
    fn detail_lines_state_an_undispatched_item_without_inventing_a_valve() {
        let detail = AttentionDetail::new(
            "repo".to_owned(),
            "work-item".to_owned(),
            "-".to_owned(),
            None,
            vec![],
            vec![],
            vec![],
        );

        let rendered = detail_lines(&detail)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("Fabro run: -"));
        assert!(!rendered.contains("Factory:"));
        assert!(!rendered.contains("Valve:"));
        // The retired handoff must not come back under any spelling.
        assert!(!rendered.contains("Attach"));
    }

    /// Scenario 30 at the RENDER: the valve the projection advertises reaches
    /// the screen, beside the run's own factory, and no attach line does.
    #[test]
    fn detail_lines_render_the_advertised_valve_and_factory() {
        let detail = AttentionDetail::new(
            "repo".to_owned(),
            "work-item".to_owned(),
            "01RUN".to_owned(),
            Some("hp".to_owned()),
            vec!["drive resolve-blocked:work-item:ready".to_owned()],
            vec![],
            vec![
                OperatorAction::Registered("approve"),
                OperatorAction::Registered("accept"),
            ],
        );

        let lines = detail_lines(&detail);
        // Three fixed fields + factory + one valve + the actions line + the
        // `Timeline:` header = seven logical lines, before any wrapping.
        assert_eq!(lines.len(), 7);
        let rendered = lines
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("Fabro run: 01RUN"));
        assert!(rendered.contains("Factory: hp"));
        assert!(rendered.contains("Valve: drive resolve-blocked:work-item:ready"));
        assert!(rendered.contains("Actions: Approve work-item, Accept work-item"));
        assert!(!rendered.contains("Attach"));
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
            Some(OperatorAction::Registered("approve")),
        );

        let rendered = format!("{:?}", attention_item_line(&model, 0, &item, 80));

        assert!(rendered.contains("> Needs review [Approve work-item]"));
    }

    /// Scenario 32 at the RENDER: the projected account rides the detail WHOLE
    /// and the answer comment the orchestrator wrote rides it verbatim.
    #[test]
    fn detail_lines_render_the_projected_account_and_the_answer_comment() {
        const ACCOUNT: &str = "fabro run 01RUN on factory hp terminated at the needs-human node.\n\
                               Tree preserved on refs/heads/needs-human/01RUN.";
        const ANSWER_COMMENT: &str = "livespec-human-answer (operator via console, 2026-09-07T14:00:00Z, \
             resolve-blocked:work-item:ready): ship the narrow fix";
        let detail = AttentionDetail::new(
            "repo".to_owned(),
            "work-item".to_owned(),
            "01RUN".to_owned(),
            Some("hp".to_owned()),
            vec!["drive resolve-blocked:work-item:ready".to_owned()],
            vec![],
            vec![],
        )
        .with_account(Some(ACCOUNT.to_owned()))
        .with_answer_comments(vec![ANSWER_COMMENT.to_owned()]);

        let rendered = detail_lines(&detail)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        // The account's own lines survive as their own rows, whole and in order.
        assert!(rendered.contains(&format!("Account:\n{ACCOUNT}")));
        assert!(rendered.contains(ANSWER_COMMENT));
    }

    /// A row that cannot hold its label elides it WITH AN INDICATOR, and one
    /// that can is left exactly as composed.
    #[test]
    fn a_row_elides_only_what_it_cannot_hold_and_says_so() {
        assert_eq!(elide_to_width("held whole", 10), "held whole");
        assert_eq!(elide_to_width("held whole", 40), "held whole");
        assert_eq!(elide_to_width("held whole", 6), "held …");
        assert_eq!(elide_to_width("held whole", 1), "…");
        // A pane with no room at all renders nothing rather than a lone
        // indicator standing for content it never showed.
        assert_eq!(elide_to_width("held whole", 0), "");

        let model = attention_model(TuiOverlay::None);
        let item = AttentionItem::new(
            "work-item".to_owned(),
            Some("work-item".to_owned()),
            "Blocked: needs-human — fabro run 01RUN terminated at the needs-human node".to_owned(),
            "source".to_owned(),
            "repo".to_owned(),
            None,
        );
        let rendered = format!("{:?}", attention_item_line(&model, 0, &item, 24));
        assert!(rendered.contains('…'), "{rendered}");
        assert!(!rendered.contains("needs-human node"), "{rendered}");
    }

    /// The resolve-blocked dialog renders its optional answer field beside the
    /// target-status choice; no other valve does.
    #[test]
    fn the_resolve_blocked_dialog_renders_its_answer_field() {
        let answered = attention_model_for_lane(
            Lane::Blocked,
            TuiOverlay::ValveConfirm {
                valve: PendingValve::MoveStatus {
                    from: Lane::Blocked,
                    to: Lane::Ready,
                },
                answer: "ship the narrow fix".to_owned(),
            },
        );
        let rendered = render_to_text(&answered, 96, 24).unwrap_or_default();
        assert!(rendered.contains("Answer (optional, type to edit): ship the narrow fix"));

        let approve = attention_model(TuiOverlay::ValveConfirm {
            valve: PendingValve::Approve,
            answer: String::new(),
        });
        assert!(
            !render_to_text(&approve, 96, 24)
                .unwrap_or_default()
                .contains("Answer (optional"),
            "an answer-less valve offers no answer field"
        );
    }

    /// Typed characters reach the answer field only where the dialog offers one.
    #[test]
    fn typing_reaches_the_answer_field_only_on_the_resolve_blocked_dialog() {
        assert_eq!(
            text_input(
                'x',
                &TuiOverlay::ValveConfirm {
                    valve: PendingValve::MoveStatus {
                        from: Lane::Blocked,
                        to: Lane::Ready,
                    },
                    answer: String::new(),
                }
            ),
            Some(TuiTerminalInput::Interaction(TuiInteraction::TypeChar('x')))
        );
        assert_eq!(
            text_input(
                'x',
                &TuiOverlay::ValveConfirm {
                    valve: PendingValve::Approve,
                    answer: String::new(),
                }
            ),
            None
        );
        assert_eq!(text_input('x', &TuiOverlay::None), None);
    }

    #[test]
    fn action_outcome_effect_maps_the_terminal_copy_outcome() {
        assert_eq!(
            action_outcome_effect(OperatorActionOutcome::CopyDriverHandoff(
                "claude groom wi".to_owned()
            )),
            TuiRuntimeEffect::CopyDriverHandoff("claude groom wi".to_owned())
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
        let model = build_tui_model_for_state(&verb_free_attention_events(), &state);

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
        let model = build_tui_model_for_state(&verb_free_attention_events(), &state);
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
                valve: PendingValve::Approve,
                answer: String::new(),
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
                valve: PendingValve::SetWorkflowScopeOverride,
                answer: String::new(),
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
        check_driver_handoff_overlay(overlay);
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
    fn hotkey_free_menu_entry_preserves_per_item_action_availability() {
        // R4's hotkey-free route must prove USE, not just row reachability. A
        // selected drilled-in item carries the per-item availability context;
        // opening the menu with Left must preserve that context exactly as the
        // registry hotkey does.
        let events = pending_events();
        let closed = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_focus(FocusPane::Content)
            .with_lane_focus(LaneFocus::Lane(Lane::PendingApproval))
            .with_selected_lane_item_index(0);
        let before_model = build_tui_model_for_state(&events, &closed);
        let before_id = before_model.selected_work_item_id().map(str::to_owned);
        assert_eq!(before_id.as_deref(), Some("console-pending"));

        let per_item_available: Vec<&'static action_registry::ActionSpec> =
            action_registry::ACTION_REGISTRY
                .iter()
                .filter(|spec| {
                    !matches!(
                        spec.staging,
                        action_registry::ActionStaging::Global(_)
                            | action_registry::ActionStaging::FactoryDrain
                    ) && action_available_for_model(&before_model, spec)
                })
                .collect();
        let count = per_item_available.len();
        assert!(count > 0, "{count}");

        let opened = press(&closed, &events, KeyCode::Left);
        assert_eq!(
            opened.overlay(),
            &TuiOverlay::Menu {
                top: 0,
                selected: 0
            }
        );
        let opened_model = build_tui_model_for_state(&events, &opened);
        assert_eq!(opened_model.selected_work_item_id(), before_id.as_deref());

        for spec in per_item_available {
            let still_available = action_available_for_model(&opened_model, spec);
            assert!(still_available, "{}", spec.id);
        }
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
            check_menu_navigation_input(resolved, "menu navigation input");
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
        check_search_overlay(overlay);
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

        assert_eq!(
            staged.state().overlay(),
            &TuiOverlay::FactoryDrainConfirm {
                work_item_id: "console-invoker-dispatch".to_owned(),
                rank: "a0".to_owned(),
            }
        );
        assert_eq!(staged.effect(), &TuiRuntimeEffect::Render);
    }

    #[test]
    fn menu_dispatch_selected_item_opens_a_target_readback_confirm() {
        let position = menu_position_for("dispatch-selected-item");
        assert!(position.is_some());
        let (top, selected) = position.unwrap_or_default();
        let state =
            TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::Menu { top, selected })
                .with_lane_focus(LaneFocus::Lane(Lane::Ready))
                .with_selected_lane_item_index(0);
        let ready_events = [lane_event(
            "evt_menu_dispatch_item",
            "console-menu-dispatch-item",
            Lane::Ready,
            None,
            "a0",
            "ready",
        )];

        let staged = step_tui_runtime(&state, &ready_events, TuiTerminalInput::Confirm, "operator");

        assert_eq!(
            staged.state().overlay(),
            &TuiOverlay::FactoryDispatchItemConfirm {
                work_item_id: "console-menu-dispatch-item".to_owned()
            }
        );
        assert_eq!(staged.effect(), &TuiRuntimeEffect::Render);
    }

    #[test]
    fn dispatch_selected_item_confirm_persists_the_pinned_item_command() {
        let state = TuiInteractionState::for_view(
            TuiView::Lanes,
            0,
            TuiOverlay::FactoryDispatchItemConfirm {
                work_item_id: "console-menu-dispatch-item".to_owned(),
            },
        )
        .with_lane_focus(LaneFocus::Lane(Lane::Ready))
        .with_selected_lane_item_index(0);
        let ready_events = [lane_event(
            "evt_confirm_dispatch_item",
            "different-current-selection",
            Lane::Ready,
            None,
            "a0",
            "ready",
        )];

        let confirmed =
            step_tui_runtime(&state, &ready_events, TuiTerminalInput::Confirm, "operator");
        let command = persisted_command(confirmed.effect());

        assert_eq!(
            command.map(console_domain::CommandEnvelope::command_type),
            Some(&CommandType::FactoryDispatchItemRequested)
        );
        assert_eq!(
            command.map(console_domain::CommandEnvelope::aggregate_id),
            Some("console-menu-dispatch-item")
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

    /// The work-item every per-item dispatch-key test selects.
    const DISPATCH_KEY_ITEM: &str = "console-dispatch-key";

    /// A selected work-item in `lane`, reachable from BOTH per-item surfaces.
    ///
    /// The lane snapshot is what the drilled-in lane selects and what the
    /// verb's availability reads. The needs-attention row is carried
    /// explicitly because a `ready` item rests on no human step, so
    /// `requires_attention` never folds it into the inbox on its own — the
    /// inbox reaches it through an ingested needs-attention item whose source
    /// reference names it, exactly as the driver-handoff fixture does.
    fn dispatch_key_events(lane: Lane) -> Vec<ConsoleEvent> {
        let item = AttentionItemSnapshot::new(
            "attention-dispatch-key",
            "human-valve",
            "high",
            "Hand-picked dispatch candidate",
            AttentionSourceRef::new("console", Some(DISPATCH_KEY_ITEM), None),
            AttentionHandoff::new("implement", None, &format!("implement:{DISPATCH_KEY_ITEM}")),
        );
        vec![
            lane_event(
                "evt_dispatch_key",
                DISPATCH_KEY_ITEM,
                lane,
                None,
                "a0",
                lane.label(),
            ),
            ConsoleEvent::fixture(
                "evt_dispatch_key_attention",
                EventType::AttentionItemAppeared,
                "needs-attention",
            )
            .with_payload_json(attention_item_payload_json(&item)),
        ]
    }

    /// The state selecting that work-item on one per-item surface: the
    /// needs-attention row when `attention`, else the drilled-in lane row.
    fn dispatch_key_state(lane: Lane, attention: bool, overlay: TuiOverlay) -> TuiInteractionState {
        if attention {
            TuiInteractionState::new(0, overlay).with_focus(FocusPane::Content)
        } else {
            TuiInteractionState::for_view(TuiView::Lanes, 0, overlay)
                .with_lane_focus(LaneFocus::Lane(lane))
                .with_selected_lane_item_index(0)
        }
    }

    #[test]
    fn the_dispatch_key_stages_the_menu_rows_confirm_on_both_per_item_surfaces() {
        // v047 gap-uqotpmdo / Scenario 28's "reachable by one key": `d` on a
        // selected ready item stages the SAME confirmation
        // `Factory > Dispatch > Dispatch selected item` stages. Asserted
        // against the MENU PATH itself rather than against a copy of its
        // expected overlay, so the accelerator cannot become a second encoding
        // of invocation. Driven on BOTH per-item surfaces, because the hosting
        // view is never an availability input (Scenario 31).
        let position = menu_position_for("dispatch-selected-item");
        assert!(position.is_some(), "missing menu action");
        let (top, selected) = position.unwrap_or_default();
        let events = dispatch_key_events(Lane::Ready);
        for attention in [true, false] {
            let state = dispatch_key_state(Lane::Ready, attention, TuiOverlay::None);
            let model = build_tui_model_for_state(&events, &state);
            let via_key = key_event_to_terminal_input(
                chord_event(action_registry::KeyChord::plain('d')),
                &model,
            )
            .map(|input| step_tui_runtime(&state, &events, input, "operator"));

            let menu_state =
                dispatch_key_state(Lane::Ready, attention, TuiOverlay::Menu { top, selected });
            let via_menu =
                step_tui_runtime(&menu_state, &events, TuiTerminalInput::Confirm, "operator");

            // The menu row really does open the per-item confirmation, pinned
            // to the selection -- stated first so a broken fixture fails as a
            // fixture rather than as a spurious parity match.
            assert_eq!(
                via_menu.state().overlay(),
                &TuiOverlay::FactoryDispatchItemConfirm {
                    work_item_id: DISPATCH_KEY_ITEM.to_owned(),
                }
            );
            let keyed = via_key.map(|step| step.state().overlay().clone());
            check(
                keyed == Some(via_menu.state().overlay().clone()),
                "the dispatch key must stage the menu row's confirmation",
            );
        }
    }

    #[test]
    fn the_status_line_names_the_dispatch_key_exactly_where_the_verb_applies() {
        // The honesty half, at the RENDERED Status band: `d dispatch` is drawn
        // where the verb is available and is absent -- with the key inert --
        // where it is not, on either per-item surface. One derivation feeds
        // both the hint and the key, so they cannot disagree.
        let area = Rect::new(0, 0, 200, 3);
        for attention in [true, false] {
            for (lane, available) in [(Lane::Ready, true), (Lane::Acceptance, false)] {
                let events = dispatch_key_events(lane);
                let state = dispatch_key_state(lane, attention, TuiOverlay::None);
                let model = build_tui_model_for_state(&events, &state);

                let mut buffer = Buffer::empty(area);
                render_footer(&model, area, &mut buffer);
                let status = buffer_to_text(&buffer, area);
                check(
                    status.contains("d dispatch") == available,
                    "the Status band must name the dispatch key exactly where it acts",
                );

                let input = key_event_to_terminal_input(
                    chord_event(action_registry::KeyChord::plain('d')),
                    &model,
                );
                check(
                    input.is_some() == available,
                    "the dispatch key must be inert exactly where its hint is absent",
                );
            }
        }
    }

    /// The dogfooded drilled-ready-lane screen with a terminal outcome PENDING:
    /// the operator has pressed `d`, the dispatch has come back failed with a
    /// payload naming no cause, and they are looking at the band that has to
    /// tell them so.
    fn dispatch_failed_events() -> Vec<ConsoleEvent> {
        let mut events = dispatch_key_events(Lane::Ready);
        events.push(
            ConsoleEvent::fixture(
                "evt_dispatch_key_failed",
                EventType::FactoryDispatchItemFailed,
                "console:factory-command-handler",
            )
            .with_payload_json("{}".to_owned()),
        );
        events
    }

    /// The Status band as the operator reads it, drawn into a `width`-column
    /// pane. The band measures the room INSIDE its own borders, so this is the
    /// rendered text, not the model string.
    fn status_band_drawn_at(model: &TuiScreenModel, width: u16) -> String {
        let area = Rect::new(0, 0, width, 3);
        let mut buffer = Buffer::empty(area);
        render_footer(model, area, &mut buffer);
        buffer_to_text(&buffer, area)
    }

    #[test]
    fn the_narrow_status_band_marks_its_overflow_and_keeps_the_way_back_out() {
        // livespec-console-beads-fabro-pzbdbo.26, dogfooded in a 105-column
        // pane: a ready row was selected, the Status band ended mid-row, and
        // NOTHING on screen said a hint had been dropped -- widening to 240
        // columns was the only way to find out. Its follow-up, mx9u.1, is which
        // hints the band keeps when it cannot keep them all: the shed count is
        // now a DOOR, so the rarely-used policy dials yield first and `esc lane
        // list` -- the only way back out of the drilled-in lane -- stays.
        let events = dispatch_key_events(Lane::Ready);
        let state = dispatch_key_state(Lane::Ready, false, TuiOverlay::None);
        let model = build_tui_model_for_state(&events, &state);

        // The premise: this row genuinely cannot fit inside a 100-column band
        // (98 columns inside its borders), so this is the truncating case.
        assert!(model.footer().chars().count() > 98);

        let status = status_band_drawn_at(&model, 100);
        let fitted = model.footer_line(98);

        check(
            status.contains(&fitted) && fitted.ends_with(" more: ?"),
            "the band must draw the fitted row and name the key that reopens what it dropped",
        );
        check(
            status.contains("esc lane list"),
            "the way back out of the lane must survive the width squeeze",
        );
        check(
            !status.contains("g merge cap") && !status.contains("k rework cap"),
            "and the policy dials must be what yielded to make room for it",
        );
    }

    #[test]
    fn the_narrow_status_band_still_reports_how_the_last_command_ended() {
        // The measured 105-column row, with a dispatch that had just failed:
        // `last command: dispatch item failed — cause not reported` is 55 of
        // those columns, so the band shed it whole and the operator was left
        // with no sign at all that the key they had just pressed had failed.
        // It now ABBREVIATES instead, and yields only after every dial has.
        let events = dispatch_failed_events();
        let state = dispatch_key_state(Lane::Ready, false, TuiOverlay::None);
        let model = build_tui_model_for_state(&events, &state);
        assert!(
            model
                .footer()
                .contains("last command: dispatch item failed")
        );

        let status = status_band_drawn_at(&model, 107);

        check(
            status.contains("last: dispatch failed"),
            "a 105-column band must still say which command failed",
        );
        for dial in [
            "g merge cap",
            "f fix cap",
            "n set-acceptance",
            "k rework cap",
        ] {
            check(
                !status.contains(dial),
                "and every policy dial must have been dropped before it",
            );
        }
    }

    #[test]
    fn the_overflow_marker_is_a_door_onto_the_focused_panes_help_section() {
        // `+N more` counted what the band could not draw and said nothing about
        // how to read it, which made the overflow a dead end at exactly the
        // widths where the roster was most needed. The marker now names `?`,
        // and `?` opens Help on the section for the pane the operator is in --
        // so the count is a door, and the key it names is the key that works.
        let events = dispatch_failed_events();
        let state = dispatch_key_state(Lane::Ready, false, TuiOverlay::None);
        let model = build_tui_model_for_state(&events, &state);

        let status = status_band_drawn_at(&model, 107);
        let marker = status
            .lines()
            .find_map(|line| line.split(" | ").find(|segment| segment.starts_with('+')))
            .unwrap_or_default()
            .trim_end_matches(['│', ' ']);
        assert_eq!(marker, "+6 more: ?", "{status}");

        // The key the marker names is bound, and it opens Help on THIS pane's
        // section rather than on the global roster.
        assert_eq!(
            key_event_to_terminal_input(chord_event(action_registry::KeyChord::plain('?')), &model),
            Some(TuiTerminalInput::Interaction(TuiInteraction::OpenHelp))
        );
        let opened = reduce_tui_interaction(&state, &events, TuiInteraction::OpenHelp);
        assert_eq!(
            opened.overlay(),
            &TuiOverlay::Help {
                focus: HelpFocus::Menu,
                selected_section: help_section_for_view(TuiView::Lanes),
                scroll: 0,
            }
        );
    }

    #[test]
    fn the_wide_status_band_still_draws_every_hint_untouched() {
        // The no-regression half of the same clause: given room, the band is
        // exactly the hints the context owns, with no marker and nothing shed.
        let area = Rect::new(0, 0, 200, 3);
        let events = dispatch_key_events(Lane::Ready);
        let state = dispatch_key_state(Lane::Ready, false, TuiOverlay::None);
        let model = build_tui_model_for_state(&events, &state);

        let mut buffer = Buffer::empty(area);
        render_footer(&model, area, &mut buffer);
        let status = buffer_to_text(&buffer, area);

        check(
            status.contains(model.footer().as_ref()),
            "a band with room to spare must draw the whole hint row",
        );
        check(!status.contains(" more"), "and carry no overflow marker");
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
        check_driver_handoff_overlay(staged.state().overlay());
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
    fn rendered_menu_reaches_every_keyless_action_with_menu_accelerator() {
        let mut keyless_count = 0usize;
        for spec in action_registry::ACTION_REGISTRY
            .iter()
            .filter(|spec| spec.hotkeys.is_empty())
        {
            keyless_count += 1;
            let position = menu_position_for(spec.id);
            assert!(position.is_some(), "missing menu action {}", spec.id);
            let (top, selected) = position.unwrap_or_default();
            let state = TuiInteractionState::for_view(
                TuiView::Lanes,
                0,
                TuiOverlay::Menu { top, selected },
            )
            .with_lane_focus(LaneFocus::Lane(Lane::Ready))
            .with_selected_lane_item_index(0);
            let ready_events = [lane_event(
                "evt_keyless_menu",
                "console-keyless-menu",
                Lane::Ready,
                None,
                "a0",
                "ready",
            )];
            let model = build_tui_model_for_state(&ready_events, &state);
            let rendered = render_to_text(&model, 110, 60).unwrap_or_default();
            let row = format!("{} [menu]", spec.label);
            let row_present = rendered.contains(&row);

            assert!(row_present, "{}", spec.id);
        }
        assert!(keyless_count > 0);
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

        assert_eq!(
            step.state().overlay(),
            &TuiOverlay::FactoryDrainConfirm {
                work_item_id: "console-dispatch-ready-enter".to_owned(),
                rank: "a0".to_owned(),
            }
        );
        assert_eq!(step.effect(), &TuiRuntimeEffect::Render);
    }

    #[test]
    fn dispatch_ready_confirmation_enter_persists_the_drain_command() {
        let state = TuiInteractionState::for_view(
            TuiView::Lanes,
            0,
            TuiOverlay::FactoryDrainConfirm {
                work_item_id: "console-dispatch-ready-enter".to_owned(),
                rank: "a0".to_owned(),
            },
        );
        let ready_events = [lane_event(
            "evt_dispatch_ready_confirm",
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
    fn dispatch_ready_confirmation_names_the_ranked_drain_target_on_the_rendered_surface() {
        let position = menu_position_for("dispatch-ready");
        assert!(position.is_some(), "missing menu action dispatch-ready");
        let (top, selected) = position.unwrap_or_default();
        let state =
            TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::Menu { top, selected });
        let ready_events = [
            lane_event(
                "evt_dispatch_ready_later",
                "console-ready-later",
                Lane::Ready,
                None,
                "a1",
                "ready",
            ),
            lane_event(
                "evt_dispatch_ready_next",
                "console-ready-next",
                Lane::Ready,
                None,
                "a0",
                "ready",
            ),
        ];

        let staged = step_tui_runtime(&state, &ready_events, TuiTerminalInput::Confirm, "operator");
        let rendered = render_to_text(
            &build_tui_model_for_state(&ready_events, staged.state()),
            110,
            60,
        )
        .unwrap_or_default();

        assert_eq!(staged.effect(), &TuiRuntimeEffect::Render);
        assert!(rendered.contains("Dispatch ready work"));
        assert!(rendered.contains("Target: console-ready-next"));
        assert!(rendered.contains("rank a0"));
        assert!(rendered.contains("Enter to dispatch | Esc to cancel"));
        assert!(!rendered.contains("Target: console-ready-later"));
    }

    #[test]
    fn the_menu_dispatches_the_selected_ready_item() {
        let position = menu_position_for("dispatch-selected-item");
        assert!(position.is_some());
        let (top, selected) = position.unwrap_or_default();
        let state =
            TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::Menu { top, selected })
                .with_lane_focus(LaneFocus::Lane(Lane::Ready))
                .with_selected_lane_item_index(1);
        let ready_events = [
            lane_event(
                "evt_dispatch_selected_other",
                "console-dispatch-other",
                Lane::Ready,
                None,
                "a0",
                "ready",
            ),
            lane_event(
                "evt_dispatch_selected_item",
                "console-dispatch-selected",
                Lane::Ready,
                None,
                "a1",
                "ready",
            ),
        ];

        let step = step_tui_runtime(&state, &ready_events, TuiTerminalInput::Confirm, "operator");

        assert_eq!(
            step.state().overlay(),
            &TuiOverlay::FactoryDispatchItemConfirm {
                work_item_id: "console-dispatch-selected".to_owned()
            }
        );
        assert_eq!(step.effect(), &TuiRuntimeEffect::Render);
    }

    #[test]
    fn the_invoker_dispatches_the_selected_ready_item() {
        let action_index = action_registry::ACTION_REGISTRY
            .iter()
            .position(|spec| spec.id == "dispatch-selected-item")
            .unwrap_or_default();
        let state = TuiInteractionState::for_view(
            TuiView::Lanes,
            0,
            TuiOverlay::ActionInvoker {
                selected_action: action_index,
            },
        )
        .with_lane_focus(LaneFocus::Lane(Lane::Ready))
        .with_selected_lane_item_index(0);
        let ready_events = [lane_event(
            "evt_invoker_dispatch_selected",
            "console-invoker-dispatch-selected",
            Lane::Ready,
            None,
            "a0",
            "ready",
        )];

        let step = step_tui_runtime(&state, &ready_events, TuiTerminalInput::Confirm, "operator");

        assert_eq!(
            step.state().overlay(),
            &TuiOverlay::FactoryDispatchItemConfirm {
                work_item_id: "console-invoker-dispatch-selected".to_owned()
            }
        );
        assert_eq!(step.effect(), &TuiRuntimeEffect::Render);
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
        assert_eq!(step.state().overlay(), &TuiOverlay::Menu { top, selected });
        let model = build_tui_model_for_state(&[], step.state());
        assert!(
            model.header().contains(
                action_registry::action_for_id("dispatch-ready")
                    .map_or("", |spec| spec.availability_summary)
            )
        );
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
        assert_eq!(step.state().overlay(), &TuiOverlay::Menu { top, selected });
        let model = build_tui_model_for_state(&pending_events(), step.state());
        assert!(model.header().contains(
            action_registry::action_for_id("accept").map_or("", |spec| spec.availability_summary)
        ));
    }

    #[test]
    fn unavailable_menu_confirmation_without_a_row_keeps_the_menu_open() {
        let state = TuiInteractionState::for_view(
            TuiView::Lanes,
            0,
            TuiOverlay::Menu {
                top: 0,
                selected: usize::MAX,
            },
        );
        let step = step_tui_runtime(&state, &[], TuiTerminalInput::Confirm, "operator");

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

    /// The position of `action_id` in the selected row's detail roster.
    ///
    /// The explainer tests pin the ACTION they name, not a position: the roster
    /// is registry-derived and grows whenever a verb becomes available on the
    /// surface (Scenario 31 added the move-status picker to the inbox row, which
    /// shifted every hardcoded index by one).
    fn detail_action_index(model: &TuiScreenModel, action_id: &str) -> usize {
        model
            .detail()
            .map(AttentionDetail::actions)
            .unwrap_or_default()
            .iter()
            .position(|action| matches!(action, OperatorAction::Registered(id) if *id == action_id))
            .unwrap_or_default()
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

    /// An Events-view model in the given sub-view focus + overlay with the
    /// Content pane focused, where the container overview/drill flow lives.
    /// Mirrors [`lanes_model_content`].
    fn events_model_content(events_focus: EventsFocus, overlay: TuiOverlay) -> TuiScreenModel {
        let state = TuiInteractionState::for_view(TuiView::Events, 0, overlay)
            .with_events_focus(events_focus)
            .with_focus(FocusPane::Content);
        build_tui_model_for_state(&demo_events(), &state)
    }

    #[test]
    fn keymap_routes_esc_from_a_drilled_in_events_sub_view_to_the_container_overview() {
        // AC4: Escape from a sub-view returns to the CONTAINER, not out of the
        // view entirely. Mirrors the Lanes half of
        // `keymap_routes_enter_and_esc_through_the_lane_sub_view`.
        let overview = events_model_content(EventsFocus::Overview, TuiOverlay::None);
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Esc), &overview),
            Some(TuiTerminalInput::Interaction(TuiInteraction::FocusNav))
        );

        for sub_view in [EventsFocus::StoredEvents, EventsFocus::EventSources] {
            let drilled = events_model_content(sub_view, TuiOverlay::None);
            assert_eq!(
                key_event_to_terminal_input(key(KeyCode::Esc), &drilled),
                Some(TuiTerminalInput::Interaction(
                    TuiInteraction::ReturnToEventsOverview
                )),
                "{sub_view:?}"
            );
            // Left mirrors Esc's step-back here exactly as it does for a
            // drilled-in lane.
            assert_eq!(
                key_event_to_terminal_input(key(KeyCode::Left), &drilled),
                Some(TuiTerminalInput::Interaction(
                    TuiInteraction::ReturnToEventsOverview
                )),
                "{sub_view:?}"
            );
        }
    }

    /// A small board fixture: two ready items and one blocked (needs-human).
    /// A single needs-attention row that names a PATH and no work-item.
    ///
    /// The action-free selection: it resolves no standardized record, so it
    /// offers no per-item verb and the command modal has nothing honest to open.
    /// A `blocked` work-item row used to stand in for this, and stopped once the
    /// inbox row began offering its state-admitted verbs (Scenario 31) --
    /// "backed by no work-item id" is the condition the contract actually names.
    fn verb_free_attention_events() -> [ConsoleEvent; 1] {
        let item = AttentionItemSnapshot::new(
            "spec:revise:SPECIFICATION",
            "spec-revise",
            "medium",
            "Spec revision owed",
            AttentionSourceRef::new("console", None, Some("SPECIFICATION")),
            AttentionHandoff::new("livespec-op", None, "livespec revise"),
        );
        [ConsoleEvent::fixture(
            "evt_verb_free",
            EventType::AttentionItemAppeared,
            "needs-attention",
        )
        .with_payload_json(attention_item_payload_json(&item))]
    }

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

    fn active_claim_execution_events() -> Vec<ConsoleEvent> {
        vec![
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
            dispatcher_execution_event("evt_dispatch", "console-executing", "dispatch_1"),
        ]
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
    ) -> ConsoleEvent {
        let payload = dispatcher_journal_payload_json(&ok_dispatcher_journal_entry(
            DispatcherJournalEntry::new(
                "console",
                work_item_id,
                dispatch_id,
                DispatcherJournalKind::Progress,
                2,
            ),
        ));
        ConsoleEvent::fixture(
            event_id,
            EventType::DispatcherJournalProgressObserved,
            "dispatcher",
        )
        .with_payload_json(payload)
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
    fn active_lane_render_distinguishes_claimed_from_executing() {
        let events = active_claim_execution_events();
        let overview = build_tui_model_for_state(
            &events,
            &TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
                .with_selected_lane_index(3),
        );
        let overview_text = ok_render_text(render_to_text(&overview, 120, 30));

        assert!(overview_text.contains("active (3); executing 1 claimed 2"));
        assert!(overview_text.contains("console-claimed-a [active] claimed"));
        assert!(overview_text.contains("console-executing [active] executing"));

        let drilled = build_tui_model_for_state(
            &events,
            &TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
                .with_lane_focus(LaneFocus::Lane(Lane::Active))
                .with_focus(FocusPane::Content),
        );
        let drilled_text = ok_render_text(render_to_text(&drilled, 120, 30));

        assert!(drilled_text.contains("console-claimed-b  rank a2  [active] claimed"));
        assert!(drilled_text.contains("console-executing  rank a3  [active] executing"));
    }

    #[test]
    fn active_lane_render_marks_terminal_signalled_item_finished() {
        let mut events = vec![
            lane_event(
                "evt_finished",
                "console-finished",
                Lane::Active,
                None,
                "a1",
                "active",
            ),
            dispatcher_execution_event("evt_dispatch", "console-finished", "dispatch_done"),
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
        let overview_text = ok_render_text(render_to_text(&overview, 120, 30));

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
        let detail_text = ok_render_text(render_to_text(&detail, 120, 30));

        assert!(detail_text.contains("execution_state      finished?"));
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
    fn work_item_detail_modal_renders_every_standardized_record_field() {
        // The acceptance for the drill-in: with a fixture item whose every
        // field is populated, each one is READABLE in the modal. The title and
        // description especially -- the lane row shows neither, so before this
        // surface the operator could not tell what an item even WAS without
        // leaving the console.
        let model = fully_populated_item_model("a short body");
        let item = selected_lane_item(model.selected_lane_item());
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
    fn work_item_detail_modal_shows_absent_fields_as_absent_and_handles_no_selection() {
        // An unpopulated record renders placeholders rather than blank rows: the
        // operator must be able to tell "not set" from "not displayed".
        let model = lanes_model_content(LaneFocus::Lane(Lane::Ready), TuiOverlay::None);
        let item = selected_lane_item(model.selected_lane_item());
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
        let blocked = selected_lane_item(blocked_model.selected_lane_item());
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
    }

    #[test]
    fn work_item_detail_modal_scrolls_a_long_description_to_its_bottom() {
        // A long markdown body must be reachable: scrolling reveals the tail and
        // clamps at the true bottom rather than running past it into blankness.
        let model = fully_populated_item_model(&scrolling_description());
        let item = selected_lane_item(model.selected_lane_item());
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

    /// Two work-item rows in one lane, so a drilled-in cursor has somewhere to
    /// move. `done` is the lane that admits NO per-item lifecycle verb, which is
    /// what makes it the case the movement hint used to be dropped in.
    fn two_row_lane_events(lane: Lane) -> [ConsoleEvent; 2] {
        [
            lane_event("evt_row_a", "console-row-a", lane, None, "a0", lane.label()),
            lane_event("evt_row_b", "console-row-b", lane, None, "a1", lane.label()),
        ]
    }

    /// The Status band as the operator READS it, drilled into `lane`.
    fn rendered_status_band(lane: Lane) -> String {
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_lane_focus(LaneFocus::Lane(lane))
            .with_focus(FocusPane::Content);
        let model = build_tui_model_for_state(&two_row_lane_events(lane), &state);
        assert_eq!(model.selected_list_row_count(), 2, "{lane:?} fixture");
        let rendered = render_to_text(&model, 220, 40).unwrap_or_default();
        rendered
            .lines()
            .find(|line| line.contains("esc lane list"))
            .unwrap_or_default()
            .to_owned()
    }

    #[test]
    fn a_drilled_in_lane_with_no_per_item_action_still_hints_the_up_down_move() {
        // The reported bug, at the render: the drilled-in `done` lane held 337
        // rows, `Down` moved the selection and changed the detail, and the
        // Status band never named the key. `done` admits no per-item verb, and
        // the movement hint had been folded into the same arm that drops them.
        let band = rendered_status_band(Lane::Done);
        assert!(
            band.contains("up/down move"),
            "the done lane must name the move it performs: {band}"
        );
        assert!(band.contains("enter item") && band.contains("esc lane list"));
        // Still honest in the other direction: no verb the lane cannot take.
        assert!(!band.contains("s move-status"));
        assert!(!band.contains("c accept"));
    }

    #[test]
    fn a_drilled_in_lane_with_per_item_actions_keeps_the_move_and_its_verbs() {
        // The control: the lane that DOES admit verbs is unchanged, so the fix
        // added the movement hint to the verb-free lane rather than removing
        // the verbs from this one.
        let band = rendered_status_band(Lane::Ready);
        for token in [
            "up/down move",
            "enter item",
            "esc lane list",
            "s move-status",
            "g merge cap",
            "f fix cap",
            "n set-acceptance",
        ] {
            assert!(
                band.contains(token),
                "the ready lane must hint {token}: {band}"
            );
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
    fn the_open_item_modal_draws_over_the_whole_screen() {
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
        let text = ok_render_text(render_to_text(&model, 100, 40));
        assert!(text.contains("Work item: console-ready-a"));
        assert!(text.contains("esc to close"));
    }

    #[test]
    fn an_open_item_modal_keeps_its_own_item_when_the_lane_re_ranks_beneath_it() {
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

        let text = ok_render_text(render_to_text(&model, 100, 40));
        // Still the item it was opened on, NOT the one that took over the index.
        assert!(text.contains(&format!("Work item: {MODAL_ITEM}")));
        assert!(!text.contains("Work item: console-ready-jumped-ahead"));
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

    /// Renders [`header_help_lines`] to one joined string, the same shape every
    /// other help-section assertion in this module checks.
    fn header_help_text() -> String {
        header_help_lines()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn header_help_defines_what_an_event_source_is_and_what_unavailable_means() {
        // AC1 of livespec-console-beads-fabro-mx9u.19: the operator-facing
        // definition, leading with the event-sourcing framing the maintainer
        // ruling of 2026-09-09 found clarifying (the console observes external
        // programs and records what they say; it holds no truth of its own).
        let text = header_help_text();
        assert!(text.contains("EVENT SOURCES"));
        assert!(text.contains("holds no truth of its own"));
        assert!(text.contains("shells out to"));
        assert!(text.contains("STALE, not current"));
    }

    #[test]
    fn header_help_states_the_tally_is_latest_state_and_clears_on_recovery() {
        // AC3: the property that distinguishes "broken now" from "broke once
        // today" -- a later successful poll clears a source even when that
        // poll's own data dedupes away.
        let text = header_help_text();
        assert!(text.contains("LATEST-STATE"));
        assert!(text.contains("clears"));
        assert!(text.contains("dedupes away"));
    }

    #[test]
    fn header_help_roster_matches_the_source_roster_the_code_can_emit() {
        // AC2: the enumerated list cannot drift out of date silently -- it is
        // asserted against the SAME `SourceAdapterKind::all()` ground truth
        // `event_source_roster_help_lines` folds over, so an added-but-
        // undocumented source fails this test rather than shipping silently.
        let text = header_help_text();
        for kind in SourceAdapterKind::all() {
            check(
                text.contains(kind.source_name()),
                &format!(
                    "expected the header Help text to name source {:?}",
                    kind.source_name()
                ),
            );
            check(
                text.contains(kind.observes()),
                &format!(
                    "expected the header Help text to describe source {:?}",
                    kind.source_name()
                ),
            );
        }
    }

    #[test]
    fn the_repos_help_section_defines_repos_observed_against_event_sources() {
        // AC4: "Repos observed" is defined as a distinct log-derived count, NOT
        // a configured roster, and distinguished from the header's event-source
        // tally so the two axes an operator otherwise conflates ("things the
        // console watches") are told apart. The companion rows' purpose is
        // stated too, not left for the operator to infer.
        let text = help_lines_for_view(TuiView::Repos)
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("NOT a configured roster"));
        assert!(text.contains("event's stream key"));
        assert!(text.contains("event sources"));
        assert!(text.contains("Fleet-scoped events"));
        assert!(text.contains("Events with no derivable repo"));
    }

    #[test]
    fn global_help_actions_derive_from_the_registry_reference() {
        let rendered = global_help_lines()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        assert_eq!(rendered, action_registry::global_help_reference_lines());
    }

    // -----------------------------------------------------------------------
    // The Settings surface at the TUI runtime level: the six-row content pane,
    // the per-setting detail help, and the Enter/Space edit that persists a
    // `config.dispatcher_setting_set` command with no arming ceremony (Scenario
    // 9).
    // -----------------------------------------------------------------------

    const CONFIRM_REPO: &str = "livespec-console-beads-fabro";

    /// The `{ repo, setting, value }` payload a `wip_cap` 5 -> 6 edit persists.
    const WIP_CAP_6_PAYLOAD: &str =
        r#"{"repo":"livespec-console-beads-fabro","setting":"wip_cap","value":6}"#;

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
    fn a_session_with_no_read_surface_offers_no_effective_policy_to_re_read() {
        // The trait default. The legacy no-store path has no orchestrator behind
        // it, so it reports "nothing to replace" and the loop keeps rendering
        // what it has -- rather than inventing a read it cannot make.
        let mut session = DeferredTuiRuntimeEffectSink;
        assert_eq!(session.refresh_dispatcher_settings().ok().flatten(), None);
    }

    #[test]
    fn an_applied_settings_write_leaves_its_row_pending_in_the_very_next_frame() {
        // The submitted-write transition the terminal loop makes for real. Before
        // this the loop recorded nothing, so the row went on rendering the
        // launch-time snapshot as though no edit had been made
        // (livespec-console-beads-fabro-30c).
        let mut state = settings_state(5);
        let mut effects = Vec::new();
        let effect = TuiRuntimeEffect::PersistCommandWithPayload {
            command: CommandEnvelope::new(
                "cmd_wip_cap_6".to_owned(),
                CommandType::ConfigDispatcherSettingSet,
                CONFIRM_REPO.to_owned(),
                "key".to_owned(),
                "operator".to_owned(),
            ),
            payload_json: WIP_CAP_6_PAYLOAD.to_owned(),
        };

        apply_sink_outcome(
            &mut state,
            &mut effects,
            effect,
            TuiRuntimeEffectSinkOutcome::Applied,
            4,
        );

        assert_eq!(
            state.dispatcher_setting_write(),
            &DispatcherSettingWriteState::Pending {
                write: console_application::DispatcherSettingWrite::WipCap(6),
                submitted_after_events: 4,
            }
        );
        let text =
            render_to_text(&build_tui_model_for_state(&[], &state), 120, 24).unwrap_or_default();
        assert!(text.contains("WIP cap  [ 5 -> 6 ]  writing..."), "{text}");
    }

    #[test]
    fn a_settings_write_the_sink_refused_leaves_no_pending_row() {
        // A store-busy refusal never reached the command worker, so claiming the
        // row is writing would be the same lie in the other direction.
        let mut state = settings_state(5);
        let mut effects = Vec::new();

        apply_sink_outcome(
            &mut state,
            &mut effects,
            TuiRuntimeEffect::PersistCommandWithPayload {
                command: CommandEnvelope::new(
                    "cmd_wip_cap_6".to_owned(),
                    CommandType::ConfigDispatcherSettingSet,
                    CONFIRM_REPO.to_owned(),
                    "key".to_owned(),
                    "operator".to_owned(),
                ),
                payload_json: WIP_CAP_6_PAYLOAD.to_owned(),
            },
            TuiRuntimeEffectSinkOutcome::NotApplied("store busy".to_owned()),
            4,
        );

        assert_eq!(
            state.dispatcher_setting_write(),
            &DispatcherSettingWriteState::Idle
        );
    }

    #[test]
    fn a_fresh_effective_read_lands_in_the_settings_pane_without_a_restart() {
        // The acceptance case: the write completed, the orchestrator now reports
        // the new value, and the SAME session renders it.
        let mut state =
            settings_state(5).with_dispatcher_setting_write(DispatcherSettingWriteState::Pending {
                write: console_application::DispatcherSettingWrite::WipCap(6),
                submitted_after_events: 0,
            });

        apply_dispatcher_settings_reread(
            &mut state,
            DispatcherSettingsRead::Observed(DispatcherSettings::new(
                false,
                false,
                AcceptancePolicy::AiThenHuman,
                3,
                2,
                6,
            )),
        );

        assert_eq!(
            state.dispatcher_setting_write(),
            &DispatcherSettingWriteState::Idle
        );
        let text =
            render_to_text(&build_tui_model_for_state(&[], &state), 120, 24).unwrap_or_default();
        assert!(text.contains("WIP cap  [ 6 ]"), "{text}");
        assert!(!text.contains("writing..."), "{text}");
    }

    #[test]
    fn a_reread_that_did_not_carry_the_write_renders_the_unchanged_outcome() {
        let mut state =
            settings_state(5).with_dispatcher_setting_write(DispatcherSettingWriteState::Pending {
                write: console_application::DispatcherSettingWrite::WipCap(6),
                submitted_after_events: 0,
            });

        apply_dispatcher_settings_reread(
            &mut state,
            DispatcherSettingsRead::Observed(observed_settings()),
        );

        let text =
            render_to_text(&build_tui_model_for_state(&[], &state), 120, 24).unwrap_or_default();
        assert!(
            text.contains("WIP cap  [ 5 ]  unchanged (requested 6)"),
            "{text}"
        );
        // The detail pane tells the SAME story as the row it describes.
        let model = build_tui_model_for_state(&[], &state);
        assert!(
            settings_detail_lines(&model)
                .iter()
                .any(|line| line.to_string().contains("unchanged (requested 6)")),
            "detail pane contradicted the row"
        );
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
            answer: String::new(),
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
                answer: String::new(),
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
                answer: String::new(),
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
        // pane names the settings and the edit, not the item-lane actions,
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
                    answer: String::new(),
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
            answer: String::new(),
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
                answer: String::new(),
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
                answer: String::new(),
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
                    answer: String::new(),
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
                    answer: String::new(),
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

        // The move-status valve opens PRE-STAGED, so the modal names the whole
        // transition `Enter` would fire under its own caption -- an operator
        // must not have to know the status-move table to read it.
        let move_status = build_tui_model_for_state(
            &demo_events(),
            &TuiInteractionState::new(
                0,
                TuiOverlay::ValveConfirm {
                    valve: PendingValve::MoveStatus {
                        from: Lane::Backlog,
                        to: Lane::Ready,
                    },
                    answer: String::new(),
                },
            ),
        );
        let output = render_to_text(&move_status, 96, 24);
        assert_eq!(
            output.as_ref().map(|r| r.contains("Move status work-item")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|r| r.contains("Move: backlog -> ready  (up/down to change)")),
            Ok(true)
        );
    }

    #[test]
    fn set_acceptance_confirm_warns_when_selected_item_is_mid_dispatch() {
        let events = active_claim_execution_events();
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_lane_focus(LaneFocus::Lane(Lane::Active))
            .with_selected_lane_item_index(2);
        let hotkey_screen = build_tui_model_for_state(&events, &state);
        assert_eq!(
            key_event_to_terminal_input(key(KeyCode::Char('n')), &hotkey_screen),
            Some(TuiTerminalInput::Interaction(
                TuiInteraction::OpenValveConfirm(PendingValve::SetAcceptance(
                    AcceptancePolicy::AiThenHuman,
                ))
            ))
        );

        let modal_state = state.with_overlay(TuiOverlay::ValveConfirm {
            valve: PendingValve::SetAcceptance(AcceptancePolicy::AiThenHuman),
            answer: String::new(),
        });
        let warning_screen = build_tui_model_for_state(&events, &modal_state);
        let output = ok_render_text(render_to_text(&warning_screen, 120, 30));

        assert!(output.contains("Set acceptance work-item"));
        assert!(output.contains("Target: console-executing"));
        assert!(output.contains("Notice: this policy cannot gate the run in flight."));
    }

    #[test]
    fn set_acceptance_confirm_on_idle_item_is_unchanged() {
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::ValveConfirm {
                valve: PendingValve::SetAcceptance(AcceptancePolicy::AiThenHuman),
                answer: String::new(),
            },
        );
        let modal = build_tui_model_for_state(&pending_events(), &state);
        let output = ok_render_text(render_to_text(&modal, 120, 30));

        assert!(output.contains("Set acceptance work-item"));
        assert!(!output.contains("cannot gate the run in flight"));
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
        assert!(
            !lanes_text.contains("container for two sub-views"),
            "{lanes_text}"
        );

        let next = reduce_tui_interaction(&opened, &events, TuiInteraction::HelpSelectNextSection);
        let events_model = build_tui_model_for_state(&events, &next);
        let events_text = render_to_text(&events_model, 120, 40).unwrap_or_default();
        assert!(events_text.contains("> Events"), "{events_text}");
        assert!(
            events_text.contains("container for two sub-views"),
            "{events_text}"
        );
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
            (TuiView::Events, "container for two sub-views"),
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

    /// Assert a near-full-screen modal's frame is genuinely blank: on every row
    /// the modal occupies, nothing of the main screen survives to the LEFT of its
    /// left border or to the RIGHT of its right border.
    ///
    /// `boxed_title` is the modal's top-left corner plus the start of its title
    /// (`"┌Help"`), asserted AT the inset corner so the check cannot pass
    /// vacuously against a render that drew no modal at all.
    ///
    /// The frame's geometry is derived from [`HELP_MODAL_MARGIN`] and the render
    /// size rather than searched for, so a bleed-through character can never be
    /// mistaken for the border it is bleeding past.
    #[track_caller]
    fn assert_modal_frame_is_clear(text: &str, boxed_title: &str, width: u16, height: u16) {
        let margin = usize::from(HELP_MODAL_MARGIN);
        let top = margin;
        let bottom = usize::from(height) - margin - 1;
        let left = margin;
        let right = usize::from(width) - margin - 1;
        let rows: Vec<Vec<char>> = text.lines().map(|line| line.chars().collect()).collect();
        let top_row: String = rows[top].iter().skip(left).collect();
        assert!(
            top_row.starts_with(boxed_title),
            "modal {boxed_title:?} did not draw its top border at the inset corner:\n{text}"
        );
        for (index, row) in rows.iter().enumerate().take(bottom + 1).skip(top) {
            let rendered = row.iter().collect::<String>();
            assert!(
                row.iter().take(left).all(|ch| *ch == ' '),
                "row {index} bleeds through the modal's left frame: {rendered:?}"
            );
            // `buffer_to_text` trims each row, so a blank right frame ends the row
            // AT the modal's right border; anything past it is bleed-through.
            assert_eq!(
                row.len(),
                right + 1,
                "row {index} bleeds through the modal's right frame: {rendered:?}"
            );
        }
    }

    /// The two viewport sizes the overlay bleed-through was dogfooded at.
    const MODAL_FRAME_VIEWPORTS: [(u16, u16); 2] = [(200, 55), (120, 40)];

    #[test]
    fn the_work_item_modal_clears_the_region_it_covers() {
        // Dogfooded 2026-09-08: the modal drew over the main screen without
        // clearing the 3-column frame beside it, so the menu bar's `Men` and the
        // Views pane's `┌Vi` showed through its left edge and the Detail pane's
        // `──┐` through its right. The frame must be blank.
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

        for (width, height) in MODAL_FRAME_VIEWPORTS {
            let text = ok_render_text(render_to_text(&model, width, height));
            assert_modal_frame_is_clear(&text, "┌Work item:", width, height);
        }
    }

    #[test]
    fn the_help_modal_clears_the_region_it_covers() {
        // Same defect, same fix, on the Help modal: `Men┌Help─...┐` on its top
        // border row and `┌Vi│  Global actions ...│──┐` on the row below it.
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::Help {
                focus: HelpFocus::Menu,
                selected_section: 0,
                scroll: 0,
            },
        );
        let model = build_tui_model_for_state(&demo_events(), &state);

        for (width, height) in MODAL_FRAME_VIEWPORTS {
            let text = ok_render_text(render_to_text(&model, width, height));
            assert_modal_frame_is_clear(&text, "┌Help", width, height);
        }
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
