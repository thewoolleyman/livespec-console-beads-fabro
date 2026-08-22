//! CLI orchestration for the operator console.
//!
//! This crate parses command arguments, wires store-backed runtime flows, ingests
//! live source adapters, persists TUI effects, and handles pending factory drain
//! commands. The binary supplies host probes and filesystem paths; this library
//! keeps the command behavior testable.
//!
//! ```rust,ignore
//! use livespec_console_beads_fabro::run;
//!
//! let output = run(["livespec-console-beads-fabro", "doctor"]);
//! assert_eq!(output.code(), 0);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

#[cfg(test)]
use console_application::source_adapters::{
    AcceptancePolicy, AdapterPoll, AdapterPollRequest, AdmissionPolicy, DispatcherJournalEntry,
    DispatcherJournalKind, FabroRunSnapshot, FabroRunState, GithubPullRequestSnapshot,
    GithubPullRequestState, Lane, LaneReason, LivespecNextAction, LivespecNextSnapshot,
    WorkItemSnapshot, normalize_dispatcher_journal_entry, normalize_fabro_run_snapshot,
    normalize_github_pull_request_snapshot, normalize_livespec_next_snapshot,
    normalize_work_item_snapshot,
};
use console_application::{
    ApplicationError, AutonomousDecision, AutonomousDecisionsPort, DispatcherSettingsPort,
    FactoryDispatchItemPort, FactoryDrainPolicy, FactoryDrainPort, OrchestratorActionPort,
    autonomous_reflection_attention_id, build_tui_model,
    handle_config_dispatcher_setting_set_command, handle_factory_dispatch_item_command,
    handle_factory_drain_command, handle_work_item_accept_command,
    handle_work_item_approve_command, handle_work_item_move_command,
    handle_work_item_reject_command, handle_work_item_resolve_blocked_command,
    handle_work_item_set_acceptance_command, handle_work_item_set_admission_command,
    handle_work_item_set_dispatcher_override_command,
    handle_work_item_set_workflow_scope_override_command, plan_page_url, project_attention,
    project_plan_page, render_plan_page_html,
    source_adapters::{
        AdapterError, AdapterIngestionSummary, AttentionHandoff, AttentionItemSnapshot,
        AttentionSourceRef, NeedsAttentionReadOutcome, NeedsAttentionSnapshotPort,
        NormalizeObservation, NormalizedSourceEvent, ObservedSourceAdapter, PullSourcePort,
        SourceAdapterKind, SourceCheckpointPort, SourceEventAppendPort, SourceObservationPlan,
        SourcePayload, SourceProbe, attention_item_payload_json, attention_resolved_payload_json,
        diff_needs_attention, dispatcher_journal_payload_json, fabro_run_snapshot_payload_json,
        materialize_attention_items, normalize_impl_attention_ready_snapshot,
        not_observed_finding_payload_json, parse_dispatcher_observation, parse_fabro_observation,
        parse_github_observation, parse_livespec_observation, parse_orchestrator_observation,
        run_adapter_poll, work_item_snapshot_payload_json,
    },
};
use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};
use console_eventstore::{
    AppendOutcome, AppendStatus, CommandAppend, CommandAppendOutcome, CommandAppendStatus,
    CommandStatusUpdateOutcome, EventAppend, EventStoreError, EventStoreResult, SqliteEventStore,
    StoredCommand,
};
use console_tui::{
    TuiLiveSession, TuiRuntimeEffect, TuiRuntimeEffectSink, TuiRuntimeEffectSinkOutcome,
};

mod backing_cli;

pub use backing_cli::{
    BackingCliPrograms, BackingCliResolution, BackingCliResolutionError, CommandShape,
    PluginResolution, ResolveInputs, python_normalized_invocation,
};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents run output data used by the console.
pub struct RunOutput {
    code: i32,
    message: String,
}

impl RunOutput {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(code: i32, message: String) -> Self {
        Self { code, message }
    }

    #[must_use]
    /// Return the process-style exit code.
    pub const fn code(&self) -> i32 {
        self.code
    }

    #[must_use]
    /// Return the message value.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Return the run value.
pub fn run<I>(args: I) -> RunOutput
where
    I: IntoIterator,
    I::Item: Into<String>,
{
    let values = args.into_iter().map(Into::into).collect::<Vec<_>>();
    run_static(&values)
}

/// Run with store and return its outcome.
#[allow(clippy::too_many_arguments)]
pub fn run_with_store(
    args: &[String],
    store: &mut SqliteEventStore,
    observed_at: &str,
    sources: &[SourceAdapterRef<'_>],
    factory_port: &mut dyn FactoryDrainPort,
    work_item_port: &mut dyn OrchestratorActionPort,
    decisions_port: &dyn AutonomousDecisionsPort,
    needs_attention: &NeedsAttentionIngest<'_>,
) -> RunOutput {
    let mut dispatch_item_port = CompatibilityNotWiredDispatchItemPort;
    run_with_store_and_dispatch_port(
        args,
        store,
        observed_at,
        sources,
        factory_port,
        &mut dispatch_item_port,
        work_item_port,
        decisions_port,
        needs_attention,
    )
}

/// Run with store and an explicitly wired selected-item dispatch port.
#[allow(clippy::too_many_arguments)]
pub fn run_with_store_and_dispatch_port(
    args: &[String],
    store: &mut SqliteEventStore,
    observed_at: &str,
    sources: &[SourceAdapterRef<'_>],
    factory_port: &mut dyn FactoryDrainPort,
    dispatch_item_port: &mut dyn FactoryDispatchItemPort,
    work_item_port: &mut dyn OrchestratorActionPort,
    decisions_port: &dyn AutonomousDecisionsPort,
    needs_attention: &NeedsAttentionIngest<'_>,
) -> RunOutput {
    match command_name(args) {
        Some("serve") => run_runtime_result(
            serve_report_with_dispatch_port(
                store,
                observed_at,
                sources,
                factory_port,
                dispatch_item_port,
                work_item_port,
                decisions_port,
                needs_attention,
            ),
            "serve",
        ),
        Some("backfill") => run_runtime_result(
            backfill_source_report(store, observed_at, sources, needs_attention),
            "backfill",
        ),
        Some("events") => run_events_with_store(args, store),
        Some("plans") => run_plans_with_store(args, store),
        Some("snapshot") => run_store_result(snapshot_report(store), "snapshot"),
        Some("doctor") => run_store_result(doctor_report(store), "doctor"),
        _other => run_static(args),
    }
}

fn command_name(values: &[String]) -> Option<&str> {
    values.get(1).map(String::as_str)
}

fn run_static(values: &[String]) -> RunOutput {
    match command_name(values) {
        None | Some("help" | "--help" | "-h") => RunOutput::new(0, help_text()),
        Some("tui") => RunOutput::new(0, tui_preview()),
        Some("serve") => RunOutput::new(0, "serve mode bootstrap: not yet wired".to_owned()),
        Some("backfill") => RunOutput::new(0, "backfill mode bootstrap: not yet wired".to_owned()),
        Some("events") => {
            let subcommand = values.get(2).map(String::as_str);
            run_events(subcommand)
        }
        Some("docs") => run_docs(values.get(2).map(String::as_str)),
        Some("plans") => RunOutput::new(
            2,
            "usage: livespec-console-beads-fabro plans <epic-id>".to_owned(),
        ),
        Some("snapshot") => RunOutput::new(0, "snapshot mode bootstrap: not yet wired".to_owned()),
        Some("doctor") => RunOutput::new(0, "doctor bootstrap: no findings".to_owned()),
        Some("arch-check") => RunOutput::new(
            0,
            "run `just check-arch` for architecture enforcement".to_owned(),
        ),
        Some(other) => RunOutput::new(2, format!("unknown command: {other}\n\n{}", help_text())),
    }
}

fn run_docs(subcommand: Option<&str>) -> RunOutput {
    match subcommand {
        Some("key-action-reference") => RunOutput::new(
            0,
            console_application::action_registry::operator_key_action_reference_markdown(),
        ),
        _other => RunOutput::new(
            2,
            "usage: livespec-console-beads-fabro docs key-action-reference".to_owned(),
        ),
    }
}

fn run_store_result(result: EventStoreResult<String>, command: &str) -> RunOutput {
    match result {
        Ok(message) => RunOutput::new(0, message),
        Err(error) => RunOutput::new(1, format!("{command} error: {error:?}")),
    }
}

fn run_runtime_result(result: ConsoleRuntimeResult<String>, command: &str) -> RunOutput {
    match result {
        Ok(message) => RunOutput::new(0, message),
        Err(error) => RunOutput::new(1, format!("{command} error: {error:?}")),
    }
}

/// Port interface for command append store behavior supplied by an outer layer.
pub trait CommandAppendStore {
    /// Append a command envelope and return whether it was inserted or deduplicated.
    fn append_command(&mut self, append: &CommandAppend) -> EventStoreResult<CommandAppendOutcome>;

    /// List persisted commands so the append path can preserve static valve
    /// idempotency unless the prior static row is terminal-failed.
    fn list_commands(&self) -> EventStoreResult<Vec<StoredCommand>>;

    /// The count of commands already appended — a monotonic sequence (commands
    /// are append-only) used to make each repeatable operator action (a broad
    /// MOVE, a fleet DRAIN, or a failed once-only valve retry) a distinct
    /// command so it always lands (see [`is_repeatable_command`] and
    /// [`command_append_from_tui_effect`]).
    fn command_count(&self) -> EventStoreResult<usize>;
}

impl CommandAppendStore for SqliteEventStore {
    fn append_command(&mut self, append: &CommandAppend) -> EventStoreResult<CommandAppendOutcome> {
        Self::append_command(self, append)
    }

    fn list_commands(&self) -> EventStoreResult<Vec<StoredCommand>> {
        Self::list_commands(self)
    }

    fn command_count(&self) -> EventStoreResult<usize> {
        Self::list_commands(self).map(|commands| commands.len())
    }
}

/// Return the persist tui runtime effects value.
pub fn persist_tui_runtime_effects(
    store: &mut dyn CommandAppendStore,
    effects: &[TuiRuntimeEffect],
    _requested_at: &str,
) -> EventStoreResult<Vec<CommandAppendOutcome>> {
    let mut outcomes = Vec::new();
    for effect in effects {
        // Read the monotonic command count BEFORE each append so a repeatable
        // action (move, drain, or a failed once-only valve retry) gets a
        // distinct key (an earlier append in this batch bumps the count for the
        // next).
        let sequence = store.command_count()?;
        let existing_commands = store.list_commands()?;
        let command_requested_at = current_command_requested_at()?;
        let Some(append) = command_append_from_tui_effect(
            effect,
            &command_requested_at,
            sequence,
            &existing_commands,
        ) else {
            continue;
        };
        outcomes.push(store.append_command(&append)?);
    }
    Ok(outcomes)
}

fn current_command_requested_at() -> EventStoreResult<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|_error| EventStoreError::InvalidSequence)
}

/// Port interface for event append store behavior supplied by an outer layer.
pub trait EventAppendStore {
    /// Append an event envelope and return whether it was inserted or deduplicated.
    /// Append a command-handling event and return the append outcome.
    fn append_event(&mut self, append: &EventAppend) -> EventStoreResult<AppendOutcome>;
}

impl EventAppendStore for SqliteEventStore {
    fn append_event(&mut self, append: &EventAppend) -> EventStoreResult<AppendOutcome> {
        Self::append_event(self, append)
    }
}

/// Append demo events to store to the backing store.
pub fn append_demo_events_to_store(
    store: &mut dyn EventAppendStore,
    observed_at: &str,
) -> EventStoreResult<Vec<AppendOutcome>> {
    let mut outcomes = Vec::new();
    for event in demo_events() {
        let append = event_append_from_console_event(&event, observed_at);
        outcomes.push(store.append_event(&append)?);
    }
    Ok(outcomes)
}

/// Load tui events from store from the backing store.
pub fn load_tui_events_from_store(store: &SqliteEventStore) -> EventStoreResult<Vec<ConsoleEvent>> {
    store.list_console_events()
}

/// Port interface for tui session runner behavior supplied by an outer layer.
pub trait TuiSessionRunner {
    /// Run an interactive TUI session over the supplied events for `requested_by`.
    ///
    /// # Errors
    /// Returns a runtime error when the session backend fails.
    fn run_tui(
        &mut self,
        events: &[ConsoleEvent],
        requested_by: &str,
        session: &mut dyn TuiLiveSession,
    ) -> ConsoleRuntimeResult<Vec<TuiRuntimeEffect>>;
}

/// Port for requesting an out-of-band source re-poll from the UI thread without
/// blocking it.
///
/// The store-backed sink holds one of these and pings it after a ledger-mutating
/// effect; the binary backs it with a channel to the poller thread. Keeping it a
/// port lets the UI-thread logic be exercised with a recording double, off the
/// real thread.
pub trait SourcePollRequester {
    /// Request an out-of-band source re-poll. Non-blocking and best-effort — a
    /// dropped request (e.g. the poller has already stopped) is ignored.
    fn request_poll(&self);
}

/// Port for requesting out-of-band pending-command handling from the UI thread
/// without blocking it on mutating backing CLIs.
pub trait PendingCommandRequester {
    /// Request pending command handling. Non-blocking and best-effort — a
    /// dropped request (for example, after shutdown) is ignored.
    fn request_pending_command_handling(&self);

    /// Whether the store-backed sink should execute pending handlers inline
    /// after the request. Production requesters return `false`; synchronous test
    /// doubles return `true` to keep existing end-to-end unit coverage local.
    fn handles_pending_commands_inline(&self) -> bool {
        false
    }
}

struct StoreBackedTuiRuntimeEffectSink<'a> {
    store: &'a mut SqliteEventStore,
    observed_at: &'a str,
    factory_port: &'a mut dyn FactoryDrainPort,
    work_item_port: &'a mut dyn OrchestratorActionPort,
    // The autonomous-decisions port so the UI thread can run the CHEAP
    // local-journal reflection on every refresh (an auto-disposition that lands
    // mid-session leaves the inbox live). The SLOW CLI source polls do NOT run
    // here — they run on the off-thread poller — so `refresh_events` never blocks
    // the UI thread.
    decisions_port: &'a dyn AutonomousDecisionsPort,
    // The out-of-band poll requester: after a ledger-mutating effect the sink
    // pings it so the off-thread poller re-polls sources at once.
    poll_requester: &'a dyn SourcePollRequester,
    // The out-of-band command handler: mutating backing CLIs run off the UI
    // thread, so `handle_runtime_effect` only appends the command row and asks
    // this worker to claim and handle it.
    command_requester: &'a dyn PendingCommandRequester,
    persisted_command_count: usize,
    handled_command_count: usize,
}

impl<'a> StoreBackedTuiRuntimeEffectSink<'a> {
    fn new(
        store: &'a mut SqliteEventStore,
        observed_at: &'a str,
        factory_port: &'a mut dyn FactoryDrainPort,
        work_item_port: &'a mut dyn OrchestratorActionPort,
        decisions_port: &'a dyn AutonomousDecisionsPort,
        poll_requester: &'a dyn SourcePollRequester,
        command_requester: &'a dyn PendingCommandRequester,
    ) -> Self {
        Self {
            store,
            observed_at,
            factory_port,
            work_item_port,
            decisions_port,
            poll_requester,
            command_requester,
            persisted_command_count: 0,
            handled_command_count: 0,
        }
    }

    const fn persisted_command_count(&self) -> usize {
        self.persisted_command_count
    }

    const fn handled_command_count(&self) -> usize {
        self.handled_command_count
    }
}

impl TuiRuntimeEffectSink for StoreBackedTuiRuntimeEffectSink<'_> {
    fn handle_runtime_effect(
        &mut self,
        effect: &TuiRuntimeEffect,
    ) -> std::io::Result<TuiRuntimeEffectSinkOutcome> {
        let persisted =
            persist_tui_runtime_effects(self.store, std::slice::from_ref(effect), self.observed_at)
                .map_err(effect_sink_io_error)?;
        if !persisted.is_empty() {
            if !self.command_requester.handles_pending_commands_inline() {
                append_factory_drain_requested_events(self.store, &persisted, self.observed_at)
                    .map_err(effect_sink_io_error)?;
            }
            self.command_requester.request_pending_command_handling();
            if self.command_requester.handles_pending_commands_inline() {
                let factory_handled = handle_pending_factory_commands(
                    self.store,
                    self.observed_at,
                    self.factory_port,
                )
                .map_err(effect_sink_io_error)?;
                let _work_item_handled = handle_pending_work_item_commands(
                    self.store,
                    self.observed_at,
                    self.work_item_port,
                )
                .map_err(effect_sink_io_error)?;
                let _config_handled = handle_pending_config_commands(
                    self.store,
                    self.observed_at,
                    self.work_item_port,
                )
                .map_err(effect_sink_io_error)?;
                self.handled_command_count += factory_handled.len();
            }
        }
        self.persisted_command_count += persisted.len();
        Ok(TuiRuntimeEffectSinkOutcome::Applied)
    }
}

fn append_factory_drain_requested_events(
    store: &mut SqliteEventStore,
    command_outcomes: &[CommandAppendOutcome],
    observed_at: &str,
) -> ConsoleRuntimeResult<usize> {
    let inserted_command_ids = command_outcomes
        .iter()
        .filter(|outcome| outcome.status() == CommandAppendStatus::Inserted)
        .map(CommandAppendOutcome::command_id)
        .collect::<Vec<_>>();
    if inserted_command_ids.is_empty() {
        return Ok(0);
    }
    let mut appended = 0;
    for stored in store.list_commands()? {
        if !inserted_command_ids.contains(&stored.command_id()) {
            continue;
        }
        let Some(command) = factory_command_from_stored(&stored)? else {
            continue;
        };
        let event_type = if *command.command_type() == CommandType::FactoryDrainRequested {
            EventType::FactoryDrainRequested
        } else {
            EventType::FactoryDispatchItemRequested
        };
        let event = ConsoleEvent::new(
            format!("evt_{}_requested", command.command_id()),
            1,
            "factory".to_owned(),
            event_type,
            "console:factory-command-handler".to_owned(),
            command.aggregate_id().to_owned(),
            0,
        );
        let append = event_append_from_command_event(&event, &command, observed_at);
        if store.append_event(&append)?.status() == AppendStatus::Inserted {
            appended += 1;
        }
    }
    Ok(appended)
}

impl TuiLiveSession for StoreBackedTuiRuntimeEffectSink<'_> {
    fn refresh_events(&mut self, request_poll: bool) -> std::io::Result<Option<Vec<ConsoleEvent>>> {
        // The CHEAP local-journal reflection runs on EVERY refresh, so an
        // auto-disposition that lands mid-session leaves the inbox live at once.
        // It reads a local file, never a slow CLI, so the UI thread does not block.
        observe_and_reflect_autonomous_decisions(self.store, self.observed_at, self.decisions_port)
            .map_err(effect_sink_io_error)?;
        // The SLOW CLI source polls run OFF this thread on the poller: on a
        // ledger-mutating effect (`request_poll`) ping the poller to re-poll at
        // once so the ledger's lane change appears promptly. Non-blocking; the
        // operator's OWN just-appended outcome is already in the re-list below.
        if request_poll {
            self.poll_requester.request_poll();
        }
        let events = self
            .store
            .list_console_events()
            .map_err(effect_sink_io_error)?;
        Ok(Some(events))
    }
}

/// The two SLOW source polls.
///
/// Backfill the source adapters (Lanes / `work_item.*` events) then diff-ingest
/// the needs-attention snapshot (the Attention list). Each shells a CLI, so this
/// runs OFF the UI thread on the poller (and once synchronously at startup as
/// part of [`ingest_and_reflect`]). Returns the source-adapter ingestion
/// summaries the startup path tallies into its `TuiSessionOutcome`.
pub fn refresh_sources(
    store: &mut SqliteEventStore,
    observed_at: &str,
    sources: &[SourceAdapterRef<'_>],
    needs_attention: &NeedsAttentionIngest<'_>,
) -> ConsoleRuntimeResult<Vec<AdapterIngestionSummary>> {
    let ingestion = backfill_source_adapters(store, observed_at, sources)?;
    let _attention_ingested = ingest_needs_attention(store, needs_attention, observed_at)?;
    Ok(ingestion)
}

/// The full launch ingest/reflect sequence.
///
/// The two slow source polls ([`refresh_sources`]) plus the cheap local-journal
/// auto-disposition reflection. Run ONCE synchronously at startup (Bug A — the
/// first frame reduces over the CURRENT ledger) and by the headless serve. During
/// the running session the two cadences split: the slow polls run off-thread on
/// the poller (via [`refresh_sources`]) while the reflection runs on the UI thread
/// every frame (the sink's `refresh_events`). Reflect runs AFTER the ingests so a
/// reflection wins over a lagging needs-attention surface still showing a resolved
/// item. Returns the source-adapter ingestion summaries the startup path tallies.
pub fn ingest_and_reflect(
    store: &mut SqliteEventStore,
    observed_at: &str,
    sources: &[SourceAdapterRef<'_>],
    needs_attention: &NeedsAttentionIngest<'_>,
    decisions_port: &dyn AutonomousDecisionsPort,
) -> ConsoleRuntimeResult<Vec<AdapterIngestionSummary>> {
    let ingestion = refresh_sources(store, observed_at, sources, needs_attention)?;
    let _reflected = observe_and_reflect_autonomous_decisions(store, observed_at, decisions_port)?;
    Ok(ingestion)
}

fn effect_sink_io_error(error: impl std::fmt::Debug) -> std::io::Error {
    std::io::Error::other(format!("{error:?}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents tui session outcome data used by the console.
pub struct TuiSessionOutcome {
    backfilled_events: usize,
    presented_events: usize,
    persisted_commands: usize,
    handled_commands: usize,
    final_events: usize,
    attention_items: usize,
}

impl TuiSessionOutcome {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(
        backfilled_event_count: usize,
        presented_event_count: usize,
        persisted_command_count: usize,
        handled_command_count: usize,
        final_event_count: usize,
        attention_count: usize,
    ) -> Self {
        Self {
            backfilled_events: backfilled_event_count,
            presented_events: presented_event_count,
            persisted_commands: persisted_command_count,
            handled_commands: handled_command_count,
            final_events: final_event_count,
            attention_items: attention_count,
        }
    }

    #[must_use]
    /// Return the stored value.
    pub const fn backfilled_event_count(&self) -> usize {
        self.backfilled_events
    }

    #[must_use]
    /// Return the stored value.
    pub const fn presented_event_count(&self) -> usize {
        self.presented_events
    }

    #[must_use]
    /// Return the stored value.
    pub const fn persisted_command_count(&self) -> usize {
        self.persisted_commands
    }

    #[must_use]
    /// Return the stored value.
    pub const fn handled_command_count(&self) -> usize {
        self.handled_commands
    }

    #[must_use]
    /// Return the stored value.
    pub const fn final_event_count(&self) -> usize {
        self.final_events
    }

    #[must_use]
    /// Return the stored value.
    pub const fn attention_count(&self) -> usize {
        self.attention_items
    }
}

/// Run store backed tui session and return its outcome.
#[allow(clippy::too_many_arguments)]
pub fn run_store_backed_tui_session(
    store: &mut SqliteEventStore,
    observed_at: &str,
    requested_by: &str,
    runner: &mut dyn TuiSessionRunner,
    sources: &[SourceAdapterRef<'_>],
    factory_port: &mut dyn FactoryDrainPort,
    work_item_port: &mut dyn OrchestratorActionPort,
    decisions_port: &dyn AutonomousDecisionsPort,
    needs_attention: &NeedsAttentionIngest<'_>,
    poll_requester: &dyn SourcePollRequester,
    command_requester: &dyn PendingCommandRequester,
) -> ConsoleRuntimeResult<TuiSessionOutcome> {
    // Run the full ingest/reflect sequence once on launch (Bug A fix): the first
    // frame must reduce over the CURRENT ledger, not a snapshot frozen at the
    // first-ever run. This ONE synchronous ingest happens before the UI loop
    // starts; ongoing polling then runs on the off-thread poller (see the
    // binary's `poller_loop`), so the UI thread never blocks on a source poll.
    let ingestion =
        ingest_and_reflect(store, observed_at, sources, needs_attention, decisions_port)?;
    let presented_events = store.list_console_events()?;
    let (effects, live_persisted_count, live_handled_count) = {
        let mut effect_sink = StoreBackedTuiRuntimeEffectSink::new(
            store,
            observed_at,
            factory_port,
            work_item_port,
            decisions_port,
            poll_requester,
            command_requester,
        );
        let effects = runner.run_tui(&presented_events, requested_by, &mut effect_sink)?;
        (
            effects,
            effect_sink.persisted_command_count(),
            effect_sink.handled_command_count(),
        )
    };
    let persisted = persist_tui_runtime_effects(store, &effects, observed_at)?;
    let handled = handle_pending_factory_commands(store, observed_at, factory_port)?;
    let _work_item_handled = handle_pending_work_item_commands(store, observed_at, work_item_port)?;
    let _config_handled = handle_pending_config_commands(store, observed_at, work_item_port)?;
    let final_events = store.list_console_events()?;
    let attention_count = project_attention(&final_events).len();
    let backfilled_event_count = ingestion
        .iter()
        .map(AdapterIngestionSummary::appended_event_count)
        .sum();
    let persisted_command_count = live_persisted_count + persisted.len();
    let handled_command_count = live_handled_count + handled.len();
    Ok(TuiSessionOutcome::new(
        backfilled_event_count,
        presented_events.len(),
        persisted_command_count,
        handled_command_count,
        final_events.len(),
        attention_count,
    ))
}

/// Return the backfill demo report value.
pub fn backfill_demo_report(
    store: &mut SqliteEventStore,
    observed_at: &str,
) -> EventStoreResult<String> {
    let outcomes = append_demo_events_to_store(store, observed_at)?;
    let inserted = outcomes
        .iter()
        .filter(|outcome| outcome.status() == AppendStatus::Inserted)
        .count();
    let duplicate = outcomes
        .iter()
        .filter(|outcome| outcome.status() == AppendStatus::Duplicate)
        .count();
    Ok(format!(
        "backfill demo events: inserted {inserted}, duplicate {duplicate}"
    ))
}

/// Return the backfill source report value.
pub fn backfill_source_report(
    store: &mut SqliteEventStore,
    observed_at: &str,
    sources: &[SourceAdapterRef<'_>],
    needs_attention: &NeedsAttentionIngest<'_>,
) -> ConsoleRuntimeResult<String> {
    let summaries = backfill_source_adapters(store, observed_at, sources)?;
    let event_count: usize = summaries
        .iter()
        .map(AdapterIngestionSummary::appended_event_count)
        .sum();
    // The needs-attention snapshot is diffed at ingest into the durable
    // `attention_item.*` stream; those events land in the store but are not part
    // of this report's pull-adapter tally (they carry no per-poll checkpoint).
    let _attention_ingested = ingest_needs_attention(store, needs_attention, observed_at)?;
    let skipped = skipped_source_event_ids(&summaries);
    let suffix = if skipped.is_empty() {
        String::new()
    } else {
        format!(", skipped {}", skipped.join(","))
    };
    Ok(format!(
        "backfill source adapters: adapters {}, events {event_count}{suffix}",
        summaries.len()
    ))
}

fn skipped_source_event_ids(summaries: &[AdapterIngestionSummary]) -> Vec<String> {
    summaries
        .iter()
        .flat_map(AdapterIngestionSummary::skipped_source_event_ids)
        .cloned()
        .collect()
}

fn backfill_source_adapters(
    store: &mut SqliteEventStore,
    observed_at: &str,
    sources: &[SourceAdapterRef<'_>],
) -> ConsoleRuntimeResult<Vec<AdapterIngestionSummary>> {
    let shared = SharedSqliteStore::new(store);
    let mut summaries = Vec::new();
    for &(adapter_id, source) in sources {
        let mut checkpoints = SqliteCheckpointPort::new(shared.clone(), observed_at);
        let mut event_log = SqliteSourceEventLog::new(shared.clone());
        summaries.push(run_adapter_poll(
            adapter_id,
            1,
            observed_at,
            source,
            &mut checkpoints,
            &mut event_log,
        )?);
    }
    Ok(summaries)
}

/// The orchestrator plane's Dispatcher journal file the console reads, as a
/// path RELATIVE to the selected repo checkout.
///
/// Both the dispatch source adapter and the autonomous per-decision audit
/// surface ([`observe_and_reflect_autonomous_decisions`]) ride this one journal.
/// Callers resolve it to an ABSOLUTE path against the selected repo via
/// [`BackingCliResolution::dispatcher_journal_path`] before reading, so the
/// dispatch source keeps reading the right tenant's journal regardless of the
/// process working directory.
pub const DISPATCHER_JOURNAL_PATH: &str = "tmp/fabro-dispatch-journal.jsonl";

/// A live source adapter paired with its adapter id, as references.
pub type SourceAdapterRef<'a> = (&'a str, &'a dyn PullSourcePort);

/// Build the real source adapters for the live ingestion path.
///
/// Each adapter observes its source through the host-backed probe (the
/// orchestrator's `list-work-items`, `gh`, the Dispatcher journal, `fabro`,
/// `livespec`) or emits an honest
/// not-observed finding. The binary supplies the probe and borrows the
/// returned adapters for the lifetime of a serve/tui run.
pub fn live_source_adapters<'a>(
    probe: &'a dyn SourceProbe,
    repo: &str,
) -> ConsoleRuntimeResult<Vec<(String, ObservedSourceAdapter<'a>)>> {
    let resolution = BackingCliResolution::from_environment()?;
    live_source_adapters_with_programs(
        probe,
        repo,
        resolution.programs(),
        &resolution.dispatcher_journal_path(),
    )
}

/// Build real source adapters with an explicit backing CLI resolution.
///
/// `journal_path` is the ABSOLUTE Dispatcher journal path
/// ([`BackingCliResolution::dispatcher_journal_path`]); the dispatch source
/// reads it directly so the source resolves the right tenant's journal
/// regardless of the process working directory.
pub fn live_source_adapters_with_programs<'a>(
    probe: &'a dyn SourceProbe,
    repo: &str,
    programs: &BackingCliPrograms,
    journal_path: &str,
) -> ConsoleRuntimeResult<Vec<(String, ObservedSourceAdapter<'a>)>> {
    let livespec_args = programs
        .livespec()
        .args()
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let specs: [(
        &str,
        SourceAdapterKind,
        SourceObservationPlan,
        NormalizeObservation,
    ); 5] = [
        (
            "orchestrator",
            SourceAdapterKind::Orchestrator,
            SourceObservationPlan::command(programs.list_work_items(), &["--json"]),
            parse_orchestrator_observation,
        ),
        (
            "dispatcher",
            SourceAdapterKind::Dispatcher,
            SourceObservationPlan::file(journal_path),
            parse_dispatcher_observation,
        ),
        (
            "fabro",
            SourceAdapterKind::Fabro,
            SourceObservationPlan::command(programs.fabro(), &["ps", "--json"]),
            parse_fabro_observation,
        ),
        (
            "livespec",
            SourceAdapterKind::LiveSpec,
            SourceObservationPlan::command(programs.livespec().program(), &livespec_args),
            parse_livespec_observation,
        ),
        (
            "github",
            SourceAdapterKind::GitHub,
            SourceObservationPlan::command(
                programs.github(),
                &["pr", "list", "--json", "number,state", "--limit", "1"],
            ),
            parse_github_observation,
        ),
    ];
    specs
        .into_iter()
        .map(|(prefix, source, plan, normalize)| {
            let adapter = ObservedSourceAdapter::new(probe, source, repo, plan, normalize)?;
            Ok((format!("{prefix}:{repo}"), adapter))
        })
        .collect()
}

/// Derive the observed tenant repo the cockpit is watching.
///
/// The console stamps this repo on every observed work-item / PR / spec snapshot,
/// keys the needs-attention `attention_item.*` streams under it, and fills the
/// header `repo:` segment. It MUST agree with the `source_ref.repo` the
/// orchestrator's `needs-attention` surface composes, so the "Repos observed"
/// projection collapses to the single observed tenant rather than splitting into
/// two names for the same repo.
///
/// That surface derives its repo from its own process `project_root.name` (the
/// working-directory basename), so the console derives it the same way — the
/// working-directory basename — NOT from the `.livespec.jsonc` / `.beads` tenant
/// name. The tenant name is the ABBREVIATED Dolt identity
/// (`livespec-orch-beads-fabro`, capped at 32 chars) and would mismatch the full
/// repo name (`livespec-orchestrator-beads-fabro`) the upstream surface stamps.
///
/// A non-empty `LIVESPEC_CONSOLE_REPO` override wins; when the working directory
/// yields no usable basename, fall back to the console's own package name.
#[must_use]
pub fn resolve_console_repo(env_override: Option<&str>, current_dir: Option<&Path>) -> String {
    if let Some(trimmed) = env_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return trimmed.to_owned();
    }
    current_dir
        .and_then(Path::file_name)
        .and_then(std::ffi::OsStr::to_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map_or_else(
            || "livespec-console-beads-fabro".to_owned(),
            ToOwned::to_owned,
        )
}

/// The needs-attention snapshot-source port paired with the console repo.
///
/// The repo names the stream the diffed `attention_item.*` events are keyed
/// under; the diff-at-ingest adapter ([`ingest_needs_attention`]) consumes this
/// bundle.
pub struct NeedsAttentionIngest<'a> {
    port: &'a dyn NeedsAttentionSnapshotPort,
    repo: String,
}

impl<'a> NeedsAttentionIngest<'a> {
    #[must_use]
    /// Construct a new value from its required fields.
    pub fn new(port: &'a dyn NeedsAttentionSnapshotPort, repo: &str) -> Self {
        Self {
            port,
            repo: repo.to_owned(),
        }
    }
}

/// Diff-at-ingest for the product needs-attention snapshot.
///
/// Rebuilds the prior ingested snapshot from the console's own
/// `attention_item.*` stream, reads the current snapshot through the port, diffs
/// them by stable id, and appends the resulting `attention_item.appeared` /
/// `.changed` / `.resolved` events. An unavailable read appends nothing — a
/// failed read must NOT resolve the whole inbox. Returns the count of
/// newly-inserted attention events.
pub fn ingest_needs_attention(
    store: &mut dyn FactoryCommandStore,
    needs_attention: &NeedsAttentionIngest<'_>,
    observed_at: &str,
) -> ConsoleRuntimeResult<usize> {
    let existing = store.list_console_events()?;
    let prior: Vec<_> = materialize_attention_items(&existing)
        .into_iter()
        .filter(|item| item.source_ref().repo() == needs_attention.repo)
        .collect();
    let next = match needs_attention.port.read_snapshot() {
        NeedsAttentionReadOutcome::Observed(items) => items,
        NeedsAttentionReadOutcome::Unavailable(_reason) => return Ok(0),
    };
    let events = diff_needs_attention(&needs_attention.repo, &prior, &next);
    let mut inserted = 0;
    for event in &events {
        let append = event_append_from_normalized_source_event(event, observed_at);
        if store.append_event(&append)?.status() == AppendStatus::Inserted {
            inserted += 1;
        }
    }
    for item in &next {
        let Some(event) = normalize_impl_attention_ready_snapshot(item) else {
            continue;
        };
        let append = event_append_from_normalized_source_event(&event, observed_at);
        if store.append_event(&append)?.status() == AppendStatus::Inserted {
            inserted += 1;
        }
    }
    Ok(inserted)
}

#[cfg(test)]
fn source_polls_from_seed(
    seed: &InitialSourceSeed<'_>,
) -> ConsoleRuntimeResult<Vec<(&'static str, AdapterPoll)>> {
    let work_item_snapshot = WorkItemSnapshot::new(
        seed.repo,
        seed.work_item_id,
        Lane::Blocked,
        Some(LaneReason::NeedsHuman),
        "a1",
        "blocked",
        AdmissionPolicy::Manual,
        AcceptancePolicy::AiThenHuman,
        1,
    )?;
    let dispatcher_entry = DispatcherJournalEntry::new(
        seed.repo,
        seed.work_item_id,
        seed.dispatch_id,
        DispatcherJournalKind::BacklogBounce,
        2,
    )?;
    let fabro_snapshot = FabroRunSnapshot::new(
        seed.repo,
        seed.work_item_id,
        seed.run_id,
        FabroRunState::HumanGate,
        3,
    )?;
    let livespec_snapshot = LivespecNextSnapshot::new(seed.repo, LivespecNextAction::Revise, 4)?;
    let github_snapshot =
        GithubPullRequestSnapshot::new(seed.repo, 24, GithubPullRequestState::ChecksPassing, 5)?;
    Ok(vec![
        (
            "orchestrator:livespec-console-beads-fabro",
            normalize_work_item_snapshot(&work_item_snapshot),
        ),
        (
            "dispatcher:livespec-console-beads-fabro",
            normalize_dispatcher_journal_entry(dispatcher_entry),
        ),
        (
            "fabro:livespec-console-beads-fabro",
            normalize_fabro_run_snapshot(fabro_snapshot),
        ),
        (
            "livespec:livespec-console-beads-fabro",
            normalize_livespec_next_snapshot(livespec_snapshot),
        ),
        (
            "github:livespec-console-beads-fabro",
            normalize_github_pull_request_snapshot(github_snapshot),
        ),
    ])
}

#[cfg(test)]
#[derive(Clone)]
struct InitialSourceSeed<'a> {
    repo: &'a str,
    work_item_id: &'a str,
    dispatch_id: &'a str,
    run_id: &'a str,
}

#[cfg(test)]
const fn initial_source_seed() -> InitialSourceSeed<'static> {
    InitialSourceSeed {
        repo: "livespec-console-beads-fabro",
        work_item_id: "livespec-console-beads-fabro-y45jhj",
        dispatch_id: "dispatch_1",
        run_id: "run_1",
    }
}

/// Return the events tail report value.
pub fn events_tail_report(store: &SqliteEventStore, limit: usize) -> EventStoreResult<String> {
    let events = store.list_console_events()?;
    if events.is_empty() {
        return Ok("events tail: no events".to_owned());
    }
    let start = events.len().saturating_sub(limit);
    let mut lines = vec!["events tail".to_owned()];
    for event in &events[start..] {
        lines.push(format!(
            "{} {} {} {}",
            event.stream_seq(),
            event.event_id(),
            event.event_type().contract_name(),
            event.source()
        ));
    }
    Ok(lines.join("\n"))
}

/// Return the snapshot report value.
pub fn snapshot_report(store: &SqliteEventStore) -> EventStoreResult<String> {
    let events = store.list_console_events()?;
    let commands = store.list_commands()?;
    let attention_count = project_attention(&events).len();
    let pending_count = count_commands_with_status(&commands, "pending");
    Ok(format!(
        "snapshot: events {}, attention {}, commands {}, pending {}",
        events.len(),
        attention_count,
        commands.len(),
        pending_count
    ))
}

/// Return the doctor report value.
pub fn doctor_report(store: &SqliteEventStore) -> EventStoreResult<String> {
    let events = store.list_console_events()?;
    let commands = store.list_commands()?;
    let attention_count = project_attention(&events).len();
    Ok(format!(
        "doctor: no findings\nstore events: {}\ncommands: {}\nattention: {}",
        events.len(),
        commands.len(),
        attention_count
    ))
}

/// Return the plan page report value.
pub fn plan_page_report(store: &SqliteEventStore, epic_id: &str) -> EventStoreResult<String> {
    let events = store.list_console_events()?;
    let page = project_plan_page(&events, epic_id);
    let html = render_plan_page_html(epic_id, &page);
    Ok(format!("url: {}\n{html}", plan_page_url(epic_id)))
}

/// Return the serve report value.
#[allow(clippy::too_many_arguments)]
pub fn serve_report(
    store: &mut SqliteEventStore,
    observed_at: &str,
    sources: &[SourceAdapterRef<'_>],
    factory_port: &mut dyn FactoryDrainPort,
    work_item_port: &mut dyn OrchestratorActionPort,
    decisions_port: &dyn AutonomousDecisionsPort,
    needs_attention: &NeedsAttentionIngest<'_>,
) -> ConsoleRuntimeResult<String> {
    let mut dispatch_item_port = CompatibilityNotWiredDispatchItemPort;
    serve_report_with_dispatch_port(
        store,
        observed_at,
        sources,
        factory_port,
        &mut dispatch_item_port,
        work_item_port,
        decisions_port,
        needs_attention,
    )
}

/// Return the serve report value with both factory ports wired.
#[allow(clippy::too_many_arguments)]
pub fn serve_report_with_dispatch_port(
    store: &mut SqliteEventStore,
    observed_at: &str,
    sources: &[SourceAdapterRef<'_>],
    factory_port: &mut dyn FactoryDrainPort,
    dispatch_item_port: &mut dyn FactoryDispatchItemPort,
    work_item_port: &mut dyn OrchestratorActionPort,
    decisions_port: &dyn AutonomousDecisionsPort,
    needs_attention: &NeedsAttentionIngest<'_>,
) -> ConsoleRuntimeResult<String> {
    // Run the full ingest/reflect sequence unconditionally on every serve (Bug A
    // fix): like the interactive launch, the headless report must reflect the
    // CURRENT ledger, not a first-run snapshot. Checkpointed/idempotent re-ingest
    // (Scenario 3) keeps this safe on a non-empty log.
    let ingestion =
        ingest_and_reflect(store, observed_at, sources, needs_attention, decisions_port)?;
    let handled = handle_pending_factory_commands_with_dispatch_port(
        store,
        observed_at,
        factory_port,
        dispatch_item_port,
    )?;
    let work_item_handled = handle_pending_work_item_commands(store, observed_at, work_item_port)?;
    let _config_handled = handle_pending_config_commands(store, observed_at, work_item_port)?;
    let events = store.list_console_events()?;
    let commands = store.list_commands()?;
    let attention_count = project_attention(&events).len();
    let pending_count = count_commands_with_status(&commands, "pending");
    let backfill_event_count: usize = ingestion
        .iter()
        .map(AdapterIngestionSummary::appended_event_count)
        .sum();
    Ok(format!(
        "serve: store ready\nbackfill events: {backfill_event_count}\nevents: {}\nattention: {attention_count}\ncommands: {}\npending: {pending_count}\nfactory commands handled: {}\nwork-item commands handled: {}",
        events.len(),
        commands.len(),
        handled.len(),
        work_item_handled.len()
    ))
}

fn count_commands_with_status(commands: &[StoredCommand], status: &str) -> usize {
    commands
        .iter()
        .filter(|command| command.status() == status)
        .count()
}

#[derive(Debug)]
/// Variants for console runtime error state or outcome values.
pub enum ConsoleRuntimeError {
    /// Adapter variant.
    Adapter(AdapterError),
    /// Application variant.
    Application(ApplicationError),
    /// Backing CLI resolution variant.
    BackingCliResolution(String),
    /// Event store variant.
    EventStore(EventStoreError),
    /// Missing command aggregate variant.
    MissingCommandAggregate(String),
    /// Tui runtime failed variant.
    TuiRuntimeFailed,
}

impl From<AdapterError> for ConsoleRuntimeError {
    fn from(error: AdapterError) -> Self {
        Self::Adapter(error)
    }
}

impl From<ApplicationError> for ConsoleRuntimeError {
    fn from(error: ApplicationError) -> Self {
        Self::Application(error)
    }
}

impl From<BackingCliResolutionError> for ConsoleRuntimeError {
    fn from(error: BackingCliResolutionError) -> Self {
        Self::BackingCliResolution(error.to_string())
    }
}

impl From<EventStoreError> for ConsoleRuntimeError {
    fn from(error: EventStoreError) -> Self {
        Self::EventStore(error)
    }
}

/// Type alias for console runtime result values.
pub type ConsoleRuntimeResult<T> = Result<T, ConsoleRuntimeError>;

#[derive(Debug, Clone, PartialEq, Eq)]
/// The store-side outcome of handling one pending command.
///
/// Carries the command id, the resolved status, and how many outcome events
/// were appended. Shared by the factory-drain and work-item pending-command
/// handlers.
pub struct PendingCommandOutcome {
    command_id: String,
    command_status: String,
    appended_event_count: usize,
}

impl PendingCommandOutcome {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(
        command_id: String,
        command_status: String,
        appended_event_count: usize,
    ) -> Self {
        Self {
            command_id,
            command_status,
            appended_event_count,
        }
    }

    #[must_use]
    /// Return the command id value.
    pub fn command_id(&self) -> &str {
        &self.command_id
    }

    #[must_use]
    /// Return the command status value.
    pub fn command_status(&self) -> &str {
        &self.command_status
    }

    #[must_use]
    /// Return the stored value.
    pub const fn appended_event_count(&self) -> usize {
        self.appended_event_count
    }
}

/// Port interface for factory command store behavior supplied by an outer layer.
pub trait FactoryCommandStore {
    /// List stored commands in command-log order.
    fn list_commands(&self) -> EventStoreResult<Vec<StoredCommand>>;

    /// List canonical console events in event-store order.
    fn list_console_events(&self) -> EventStoreResult<Vec<ConsoleEvent>>;

    /// Append a command and return whether it was inserted or deduplicated.
    fn append_command(&mut self, append: &CommandAppend) -> EventStoreResult<CommandAppendOutcome>;

    /// Append a command-handling event and return the append outcome.
    fn append_event(&mut self, append: &EventAppend) -> EventStoreResult<AppendOutcome>;

    /// Atomically claim one pending command for this consumer.
    fn claim_command(&mut self, command_id: &str, claimed_at: &str) -> EventStoreResult<bool>;

    /// Update a command status and optional result/error payloads.
    ///
    /// # Errors
    /// Returns an event-store error when the command cannot be found or persisted.
    fn update_command_status(
        &mut self,
        command_id: &str,
        status: &str,
        updated_at: &str,
        result_json: Option<&str>,
        error_json: Option<&str>,
    ) -> EventStoreResult<CommandStatusUpdateOutcome>;

    /// Finalize one command still owned by an executing consumer.
    fn finalize_executing_command_status(
        &mut self,
        command_id: &str,
        status: &str,
        updated_at: &str,
        result_json: Option<&str>,
        error_json: Option<&str>,
    ) -> EventStoreResult<CommandStatusUpdateOutcome>;

    /// Mark stale executing commands failed for operator-visible recovery.
    fn fail_stale_executing_commands(
        &mut self,
        stale_before: &str,
        recovered_at: &str,
        error_json: &str,
    ) -> EventStoreResult<usize>;
}

impl FactoryCommandStore for SqliteEventStore {
    fn list_commands(&self) -> EventStoreResult<Vec<StoredCommand>> {
        Self::list_commands(self)
    }

    fn list_console_events(&self) -> EventStoreResult<Vec<ConsoleEvent>> {
        Self::list_console_events(self)
    }

    fn append_command(&mut self, append: &CommandAppend) -> EventStoreResult<CommandAppendOutcome> {
        Self::append_command(self, append)
    }

    fn append_event(&mut self, append: &EventAppend) -> EventStoreResult<AppendOutcome> {
        Self::append_event(self, append)
    }

    fn claim_command(&mut self, command_id: &str, claimed_at: &str) -> EventStoreResult<bool> {
        Self::claim_command(self, command_id, claimed_at)
    }

    fn update_command_status(
        &mut self,
        command_id: &str,
        status: &str,
        updated_at: &str,
        result_json: Option<&str>,
        error_json: Option<&str>,
    ) -> EventStoreResult<CommandStatusUpdateOutcome> {
        Self::update_command_status(
            self,
            command_id,
            status,
            updated_at,
            result_json,
            error_json,
        )
    }

    fn finalize_executing_command_status(
        &mut self,
        command_id: &str,
        status: &str,
        updated_at: &str,
        result_json: Option<&str>,
        error_json: Option<&str>,
    ) -> EventStoreResult<CommandStatusUpdateOutcome> {
        Self::finalize_executing_command_status(
            self,
            command_id,
            status,
            updated_at,
            result_json,
            error_json,
        )
    }

    fn fail_stale_executing_commands(
        &mut self,
        stale_before: &str,
        recovered_at: &str,
        error_json: &str,
    ) -> EventStoreResult<usize> {
        Self::fail_stale_executing_commands(self, stale_before, recovered_at, error_json)
    }
}

const STALE_EXECUTING_COMMAND_RECOVERY_AFTER_HOURS: i64 = 24;
const STALE_EXECUTING_COMMAND_ERROR_JSON: &str =
    r#"{"reason":"stale executing command recovered as failed"}"#;

fn recover_stale_executing_commands(
    store: &mut dyn FactoryCommandStore,
    handled_at: &str,
) -> ConsoleRuntimeResult<usize> {
    let Ok(now) = OffsetDateTime::parse(handled_at, &Rfc3339) else {
        return Ok(0);
    };
    let fallback_stale_before = handled_at.to_owned();
    let stale_before = (now - Duration::hours(STALE_EXECUTING_COMMAND_RECOVERY_AFTER_HOURS))
        .format(&Rfc3339)
        .map_or(fallback_stale_before, std::convert::identity);
    match store.fail_stale_executing_commands(
        &stale_before,
        handled_at,
        STALE_EXECUTING_COMMAND_ERROR_JSON,
    ) {
        Ok(recovered) => Ok(recovered),
        Err(error) => Err(error.into()),
    }
}

fn claim_pending_command(
    store: &mut dyn FactoryCommandStore,
    command: &CommandEnvelope,
    claimed_at: &str,
) -> ConsoleRuntimeResult<bool> {
    Ok(store.claim_command(command.command_id(), claimed_at)?)
}

/// Handle pending factory commands.
pub fn handle_pending_factory_commands(
    store: &mut dyn FactoryCommandStore,
    handled_at: &str,
    port: &mut dyn FactoryDrainPort,
) -> ConsoleRuntimeResult<Vec<PendingCommandOutcome>> {
    let mut dispatch_item_port = CompatibilityNotWiredDispatchItemPort;
    handle_pending_factory_commands_with_dispatch_port(
        store,
        handled_at,
        port,
        &mut dispatch_item_port,
    )
}

/// Handle pending factory commands with both factory command ports wired.
pub fn handle_pending_factory_commands_with_dispatch_port(
    store: &mut dyn FactoryCommandStore,
    handled_at: &str,
    drain_port: &mut dyn FactoryDrainPort,
    dispatch_item_port: &mut dyn FactoryDispatchItemPort,
) -> ConsoleRuntimeResult<Vec<PendingCommandOutcome>> {
    let _recovered = recover_stale_executing_commands(store, handled_at)?;
    let policy_events = store.list_console_events()?;
    let policy = FactoryDrainPolicy::from_events(&policy_events);
    let mut outcomes = Vec::new();
    for stored_command in store.list_commands()? {
        if stored_command.status() != "pending" {
            continue;
        }
        let Some(command) = factory_command_from_stored(&stored_command)? else {
            continue;
        };
        if !claim_pending_command(store, &command, handled_at)? {
            continue;
        }
        let command_outcome = if *command.command_type() == CommandType::FactoryDrainRequested {
            handle_factory_drain_command(&command, &policy, drain_port)?
        } else {
            handle_factory_dispatch_item_command(&command, dispatch_item_port)?
        };
        outcomes.push(finalize_pending_command(
            store,
            &command,
            command_outcome.events(),
            command_outcome.command_status(),
            handled_at,
        )?);
    }
    Ok(outcomes)
}

struct CompatibilityNotWiredDispatchItemPort;

impl FactoryDispatchItemPort for CompatibilityNotWiredDispatchItemPort {
    fn dispatch_item(
        &mut self,
        _request: &console_application::FactoryDispatchItemRequest,
    ) -> Result<console_application::FactoryDispatchItemPortOutcome, ApplicationError> {
        Ok(console_application::FactoryDispatchItemPortOutcome::not_wired())
    }
}

/// Handle pending `work_item.*` commands through the shared orchestrator port.
///
/// Approve, accept, reject, set-admission, and set-acceptance all ride the
/// shared port; each is dispatched to its application handler, which derives the
/// action-id (approve/accept carry no payload; reject carries `mode`, and
/// set-admission and set-acceptance each carry `policy` in their payloads).
///
/// # Errors
/// Returns a console runtime error when a command is malformed or the store
/// cannot persist the outcome events.
pub fn handle_pending_work_item_commands(
    store: &mut dyn FactoryCommandStore,
    handled_at: &str,
    port: &mut dyn OrchestratorActionPort,
) -> ConsoleRuntimeResult<Vec<PendingCommandOutcome>> {
    let _recovered = recover_stale_executing_commands(store, handled_at)?;
    let commands = store.list_commands()?;
    let mut outcomes = Vec::new();
    for (index, stored_command) in commands.iter().enumerate() {
        if stored_command.status() != "pending" {
            continue;
        }
        if older_factory_command_blocks_control_command(stored_command, &commands[..index]) {
            continue;
        }
        let Some(pending) = work_item_command_from_stored(stored_command)? else {
            continue;
        };
        if !claim_pending_command(store, pending.command(), handled_at)? {
            continue;
        }
        let command_outcome = match &pending {
            PendingWorkItemCommand::Approve(command) => {
                handle_work_item_approve_command(command, port)?
            }
            PendingWorkItemCommand::Accept(command) => {
                handle_work_item_accept_command(command, port)?
            }
            PendingWorkItemCommand::Reject {
                command,
                payload_json,
            } => handle_work_item_reject_command(command, payload_json, port)?,
            PendingWorkItemCommand::SetAdmission {
                command,
                payload_json,
            } => handle_work_item_set_admission_command(command, payload_json, port)?,
            PendingWorkItemCommand::SetAcceptance {
                command,
                payload_json,
            } => handle_work_item_set_acceptance_command(command, payload_json, port)?,
            PendingWorkItemCommand::ResolveBlocked {
                command,
                payload_json,
            } => handle_work_item_resolve_blocked_command(command, payload_json, port)?,
            PendingWorkItemCommand::Move {
                command,
                payload_json,
            } => handle_work_item_move_command(command, payload_json, port)?,
            PendingWorkItemCommand::SetDispatcherOverride {
                command,
                payload_json,
            } => handle_work_item_set_dispatcher_override_command(command, payload_json, port)?,
            PendingWorkItemCommand::SetWorkflowScopeOverride {
                command,
                payload_json,
            } => handle_work_item_set_workflow_scope_override_command(command, payload_json, port)?,
        };
        outcomes.push(finalize_pending_command(
            store,
            pending.command(),
            command_outcome.events(),
            command_outcome.command_status(),
            handled_at,
        )?);
    }
    Ok(outcomes)
}

/// Handle pending `config.dispatcher_setting_set` commands through the settings
/// port.
///
/// Each pending command is dispatched to the Configuration context handler,
/// which effects the one-setting change through the orchestrator's published
/// `set-config` command surface (via the [`DispatcherSettingsPort`] built over
/// the shared orchestrator-action port) and appends the audit event.
/// `handled_at` is the audit event's `occurred_at`.
///
/// # Errors
/// Returns a console runtime error when a command is malformed or the store
/// cannot persist the outcome events.
pub fn handle_pending_config_commands(
    store: &mut dyn FactoryCommandStore,
    handled_at: &str,
    action_port: &mut dyn OrchestratorActionPort,
) -> ConsoleRuntimeResult<Vec<PendingCommandOutcome>> {
    let _recovered = recover_stale_executing_commands(store, handled_at)?;
    let mut settings_port = DispatcherSettingsPort::new(action_port);
    let commands = store.list_commands()?;
    let mut outcomes = Vec::new();
    for (index, stored_command) in commands.iter().enumerate() {
        if stored_command.status() != "pending" {
            continue;
        }
        if older_factory_command_blocks_control_command(stored_command, &commands[..index]) {
            continue;
        }
        let Some((command, payload_json)) = config_command_from_stored(stored_command)? else {
            continue;
        };
        if !claim_pending_command(store, &command, handled_at)? {
            continue;
        }
        let command_outcome = handle_config_dispatcher_setting_set_command(
            &command,
            &payload_json,
            handled_at,
            &mut settings_port,
        )?;
        outcomes.push(finalize_pending_command(
            store,
            &command,
            command_outcome.events(),
            command_outcome.command_status(),
            handled_at,
        )?);
    }
    Ok(outcomes)
}

/// Handle pending short control-plane commands.
///
/// This lane intentionally excludes long-running factory drain and selected
/// dispatch commands so policy/valve changes can complete while a drain is
/// still executing. Same-aggregate ordering is still causal: a control command
/// does not overtake an older pending/executing factory command for that
/// aggregate.
pub fn handle_pending_control_commands(
    store: &mut dyn FactoryCommandStore,
    handled_at: &str,
    action_port: &mut dyn OrchestratorActionPort,
) -> ConsoleRuntimeResult<Vec<PendingCommandOutcome>> {
    let mut outcomes = handle_pending_work_item_commands(store, handled_at, action_port)?;
    let config_outcomes = handle_pending_config_commands(store, handled_at, action_port)?;
    outcomes.extend(config_outcomes);
    Ok(outcomes)
}

fn older_factory_command_blocks_control_command(
    control_command: &StoredCommand,
    older_commands: &[StoredCommand],
) -> bool {
    let Some(aggregate_id) = control_command.aggregate_id() else {
        return false;
    };
    older_commands.iter().any(|older| {
        older.aggregate_id() == Some(aggregate_id)
            && is_factory_command_type(older.command_type())
            && matches!(older.status(), "pending" | "executing")
    })
}

fn is_factory_command_type(command_type: &str) -> bool {
    command_type == CommandType::FactoryDrainRequested.contract_name()
        || command_type == CommandType::FactoryDispatchItemRequested.contract_name()
}

/// Rebuild a `config.dispatcher_setting_set` command and its stored
/// `payload_json` (the `{ repo, setting, value }` object) from a stored command,
/// or `None` when the stored command is not a configuration command.
fn config_command_from_stored(
    stored_command: &StoredCommand,
) -> ConsoleRuntimeResult<Option<(CommandEnvelope, String)>> {
    if stored_command.command_type() != CommandType::ConfigDispatcherSettingSet.contract_name() {
        return Ok(None);
    }
    let Some(aggregate_id) = stored_command.aggregate_id() else {
        return Err(ConsoleRuntimeError::MissingCommandAggregate(
            stored_command.command_id().to_owned(),
        ));
    };
    let command = CommandEnvelope::new(
        stored_command.command_id().to_owned(),
        CommandType::ConfigDispatcherSettingSet,
        aggregate_id.to_owned(),
        stored_command.idempotency_key().to_owned(),
        stored_command.requested_by().to_owned(),
    );
    Ok(Some((command, stored_command.payload_json().to_owned())))
}

/// Observe the plane's published per-decision autonomous audit, reflect each
/// auto-resolution, and surface each escalation as needs-attention.
///
/// The reflection rides the console's own command-plus-outcome-event path. For
/// every auto-resolution the plane's engine made, the console records a
/// `factory.autonomous_decision_reflected` command (carrying the disposed
/// work-item, gate, and decision) and its outcome events -- a `CommandAccepted`
/// plus an `attention_item.resolved` for that item's human-gate needs-attention
/// id -- so the item leaves the inbox and the audit trail is complete. Every
/// escalation is surfaced as the work-item needs-human valve item. The console
/// resolves NO gate itself; it only reflects the engine's already-journaled
/// dispositions, and never races the engine. Reflection is idempotent across
/// runs -- each decision's command id is content-stable, so a re-observed
/// decision is a duplicate no-op. Returns the count of NEW reflections or
/// escalation attention items recorded this run.
///
/// # Errors
/// Returns a console runtime error when the store cannot persist the reflection
/// command or its outcome events.
pub fn observe_and_reflect_autonomous_decisions(
    store: &mut dyn FactoryCommandStore,
    observed_at: &str,
    decisions_port: &dyn AutonomousDecisionsPort,
) -> ConsoleRuntimeResult<usize> {
    let audit = decisions_port.read_autonomous_decisions();
    let mut reflected = 0;
    // Auto-resolutions are reflected as completed commands; escalations become
    // needs-human valve items from this same published surface.
    for decision in audit.auto_resolutions() {
        if reflect_autonomous_decision(store, observed_at, decision)? {
            reflected += 1;
        }
    }
    for decision in audit.escalations() {
        if surface_autonomous_escalation(store, observed_at, decision)? {
            reflected += 1;
        }
    }
    Ok(reflected)
}

/// Reflect one auto-resolution: append its reflection command (skipping a
/// decision already reflected on a prior run) and finalize it with the
/// `CommandAccepted` + `attention_item.resolved` outcome. Returns whether a NEW
/// reflection was recorded -- false when the decision was already reflected, or
/// when its gate maps to no needs-attention item.
fn reflect_autonomous_decision(
    store: &mut dyn FactoryCommandStore,
    observed_at: &str,
    decision: &AutonomousDecision,
) -> ConsoleRuntimeResult<bool> {
    let Some(attention_id) =
        autonomous_reflection_attention_id(decision.work_item_id(), decision.gate())
    else {
        return Ok(false);
    };
    let command = autonomous_reflection_command(decision);
    let append = CommandAppend::new(
        command.clone(),
        observed_at.to_owned(),
        Some(command.aggregate_id().to_owned()),
        command_correlation_id(&command),
        autonomous_reflection_payload_json(decision),
    );
    if store.append_command(&append)?.status() == CommandAppendStatus::Duplicate {
        // Already reflected on a prior run -- an idempotent no-op.
        return Ok(false);
    }
    if !store.claim_command(command.command_id(), observed_at)? {
        return Ok(false);
    }
    let events = [
        autonomous_reflection_event(
            &command,
            EventType::CommandAccepted,
            "command",
            "accepted",
            1,
            "{}",
        ),
        autonomous_reflection_event(
            &command,
            EventType::AttentionItemResolved,
            "source",
            "resolved",
            2,
            &attention_resolved_payload_json(&attention_id),
        ),
    ];
    let _outcome = finalize_pending_command(store, &command, &events, "completed", observed_at)?;
    Ok(true)
}

fn surface_autonomous_escalation(
    store: &mut dyn FactoryCommandStore,
    observed_at: &str,
    decision: &AutonomousDecision,
) -> ConsoleRuntimeResult<bool> {
    let item = autonomous_escalation_attention_item(decision);
    let source_event_id = format!(
        "auto-disposition-escalation:{}:{}:{}",
        decision.work_item_id(),
        decision.disposition(),
        decision.governing_settings().join("+")
    );
    let event = ConsoleEvent::new(
        format!("evt:{source_event_id}"),
        1,
        "orchestrator-journal".to_owned(),
        EventType::AttentionItemAppeared,
        "orchestrator-journal".to_owned(),
        format!("attention_item:orchestrator-journal:{}", item.id()),
        1,
    )
    .with_payload_json(attention_item_payload_json(&item));
    let append = event_append_from_console_event(&event, observed_at);
    Ok(store.append_event(&append)?.status() == AppendStatus::Inserted)
}

fn autonomous_escalation_attention_item(decision: &AutonomousDecision) -> AttentionItemSnapshot {
    let work_item_id = decision.work_item_id();
    let action_id = format!("set-admission:{work_item_id}");
    AttentionItemSnapshot::new(
        &format!("valve:set-admission:{work_item_id}"),
        "human-valve",
        "high",
        &format!(
            "{} requires operator attention for {}",
            decision.disposition(),
            work_item_id
        ),
        AttentionSourceRef::new(
            "orchestrator-journal",
            Some(work_item_id),
            Some(DISPATCHER_JOURNAL_PATH),
        ),
        AttentionHandoff::new(
            "drive",
            Some(&action_id),
            &format!("drive --action {action_id}"),
        ),
    )
}

/// The content-stable reflection command for one auto-resolution. Keyed by gate
/// and work-item so a re-observed decision re-appends as a duplicate no-op
/// (idempotent across the append-only journal's re-reads).
fn autonomous_reflection_command(decision: &AutonomousDecision) -> CommandEnvelope {
    CommandEnvelope::new(
        format!(
            "cmd_autonomous_reflect_{}_{}",
            decision.gate(),
            decision.work_item_id()
        ),
        CommandType::FactoryAutonomousDecisionReflected,
        decision.work_item_id().to_owned(),
        format!(
            "{}:factory.autonomous_decision_reflected:{}",
            decision.work_item_id(),
            decision.gate()
        ),
        "console:autonomous-reflect".to_owned(),
    )
}

/// The reflection command's persisted payload: the disposed work-item, the
/// collapsed gate, and what the plane's engine decided.
fn autonomous_reflection_payload_json(decision: &AutonomousDecision) -> String {
    serde_json::json!({
        "work_item_id": decision.work_item_id(),
        "gate": decision.gate(),
        "decision": decision.decision(),
        "disposition": decision.disposition(),
        "governing_settings": decision.governing_settings(),
    })
    .to_string()
}

/// One reflection outcome event, carrying its `payload_json`, keyed to the
/// reflection command's aggregate so the projection folds it deterministically.
fn autonomous_reflection_event(
    command: &CommandEnvelope,
    event_type: EventType,
    context: &str,
    suffix: &str,
    stream_seq: u64,
    payload_json: &str,
) -> ConsoleEvent {
    ConsoleEvent::new(
        format!("evt_{}_{}", command.command_id(), suffix),
        1,
        context.to_owned(),
        event_type,
        "console:autonomous-reflect".to_owned(),
        command.aggregate_id().to_owned(),
        stream_seq,
    )
    .with_payload_json(payload_json.to_owned())
}

/// Persist one handled command's outcome events and update its command status.
///
/// Shared by the factory and work-item pending handlers so both persist
/// identically.
fn finalize_pending_command(
    store: &mut dyn FactoryCommandStore,
    command: &CommandEnvelope,
    events: &[ConsoleEvent],
    command_status: &str,
    handled_at: &str,
) -> ConsoleRuntimeResult<PendingCommandOutcome> {
    let mut inserted_event_count = 0;
    for event in events {
        let append = event_append_from_command_event(event, command, handled_at);
        if store.append_event(&append)?.status() == AppendStatus::Inserted {
            inserted_event_count += 1;
        }
    }
    let result_json = format!(r#"{{"event_count":{inserted_event_count}}}"#);
    // A failed command's error_json carries the FAILURE EVENT's payload — the
    // action id plus any refusal the action surface emitted — instead of the
    // empty object that used to discard the diagnostic at the store boundary.
    let failure_payload = events
        .iter()
        .find(|event| {
            matches!(
                event.event_type(),
                EventType::WorkItemActionFailed
                    | EventType::FactoryDrainFailed
                    | EventType::CommandRejected
            )
        })
        .map(ConsoleEvent::payload_json);
    let error_json = if matches!(command_status, "failed" | "rejected") {
        Some(failure_payload.unwrap_or("{}"))
    } else {
        None
    };
    let status_update = store.finalize_executing_command_status(
        command.command_id(),
        command_status,
        handled_at,
        Some(&result_json),
        error_json,
    );
    let status_outcome = command_status_update_runtime_result(status_update)?;
    Ok(PendingCommandOutcome::new(
        status_outcome.command_id().to_owned(),
        status_outcome.status().to_owned(),
        inserted_event_count,
    ))
}

fn command_status_update_runtime_result(
    result: EventStoreResult<CommandStatusUpdateOutcome>,
) -> ConsoleRuntimeResult<CommandStatusUpdateOutcome> {
    match result {
        Ok(outcome) => Ok(outcome),
        Err(error) => Err(ConsoleRuntimeError::EventStore(error)),
    }
}

fn command_append_from_tui_effect(
    effect: &TuiRuntimeEffect,
    requested_at: &str,
    sequence: usize,
    existing_commands: &[StoredCommand],
) -> Option<CommandAppend> {
    match effect {
        TuiRuntimeEffect::PersistCommand(command) => {
            // The payload-LESS commands ride this arm, and a payload-less
            // command's identity has no operator-varied field at all — so a
            // repeatable one (the fleet drain) can only be distinguished here.
            // The once-per-item valves (approve/accept) also ride this arm: they
            // keep their static key unless that static row is terminal-failed,
            // in which case a retry gets the same sequence discriminator used
            // by repeatable actions.
            let command = distinguish_retryable_command(command, sequence, existing_commands);
            Some(CommandAppend::new(
                command.clone(),
                requested_at.to_owned(),
                Some(command.aggregate_id().to_owned()),
                command_correlation_id(&command),
                "{}".to_owned(),
            ))
        }
        TuiRuntimeEffect::PersistCommandWithPayload {
            command,
            payload_json,
        } => {
            let command = distinguish_retryable_command(command, sequence, existing_commands);
            Some(CommandAppend::new(
                command.clone(),
                requested_at.to_owned(),
                Some(command.aggregate_id().to_owned()),
                command_correlation_id(&command),
                payload_json.clone(),
            ))
        }
        TuiRuntimeEffect::Render
        | TuiRuntimeEffect::OpenAttachCommand(_)
        | TuiRuntimeEffect::CopyAttachCommand(_)
        | TuiRuntimeEffect::CopyDriverHandoff(_)
        | TuiRuntimeEffect::Quit
        | TuiRuntimeEffect::ApplicationError(_) => None,
    }
}

/// Whether an operator action is REPEATABLE — issuable any number of times, and
/// expected to take effect on every issue rather than be absorbed as a no-op.
///
/// The split is SEMANTIC, not structural, so it is enumerated rather than
/// derived from the envelope's shape:
///
/// * A payload-CARRYING action (the broad move, reject, the policy dials, the
///   dispatcher overrides, the config setting) has an identity that is a pure
///   function of `(aggregate, action, value)`, which means the A -> B -> A
///   sequence an operator naturally performs (set a dial, change it, set it
///   back) collides the third edit onto the first. Carrying the payload value in
///   the key — which every such command already does — narrows the collision but
///   cannot remove it, because the value itself repeats.
/// * A factory DRAIN is repeatable fleet-wide — draining the ready queue is a
///   gesture the operator repeats whenever the queue refills. Its aggregate is
///   the fleet (`fleet:livespec`), not an item, and it is PAYLOAD-LESS, so its
///   key (`fleet:livespec:factory.drain_requested:budget=1:parallel=1`) is
///   constant for all time: without a distinguisher, exactly ONE drain is ever
///   possible per console store, and every later `:drain` silently enqueues
///   nothing. Being payload-less is precisely WHY the distinguisher is needed —
///   there is no payload for the key to vary on.
///
/// The once-per-item transitions are deliberately EXCLUDED from unconditional
/// repeatability: approve (`pending-approval -> ready`) and accept (`acceptance
/// -> done`) are idempotent by design, so a double keypress SHOULD be absorbed
/// rather than fire the valve twice. Their failed terminal rows are handled by
/// [`is_failed_once_only_valve_retry`] instead.
const fn is_repeatable_command(command_type: CommandType) -> bool {
    matches!(
        command_type,
        CommandType::WorkItemMoveRequested
            | CommandType::FactoryDrainRequested
            | CommandType::FactoryDispatchItemRequested
            | CommandType::WorkItemRejectRequested
            | CommandType::WorkItemSetAdmissionRequested
            | CommandType::WorkItemSetAcceptanceRequested
            | CommandType::WorkItemResolveBlockedRequested
            | CommandType::WorkItemSetDispatcherOverrideRequested
            | CommandType::WorkItemSetWorkflowScopeOverrideRequested
            | CommandType::ConfigDispatcherSettingSet
    )
}

/// Give a REPEATABLE command a per-append identity so every distinct issue lands.
///
/// Each such command carries an identity that is a pure function of its content
/// (`<id>:work_item.<action>_requested[:<key>=<value>]`), so re-issuing an action
/// with a value it already held dedupes against the earlier row and silently
/// no-ops — the operator presses the key and nothing happens. Folding the
/// monotonic append `sequence` into BOTH the `command_id` and the
/// `idempotency_key` makes every distinct issue a distinct command, while an exact
/// re-persist AT THE SAME sequence still dedupes, so the store's replay-safety is
/// preserved. A sequence-distinguished key also means an already-spent terminal
/// row (a drain left at `status: failed`) can never block a later issue, so
/// recovery needs no store surgery.
///
/// Originally scoped to the broad MOVE (PR #258, commit 17154dd); widened to the
/// full repeatable set after an audit of every `CommandType` — see
/// [`is_repeatable_command`] for which actions are repeatable and why the
/// once-per-item valves are excluded.
fn distinguish_repeatable_command(command: &CommandEnvelope, sequence: usize) -> CommandEnvelope {
    if !is_repeatable_command(*command.command_type()) {
        return command.clone();
    }
    distinguish_command(command, sequence)
}

fn distinguish_retryable_command(
    command: &CommandEnvelope,
    sequence: usize,
    existing_commands: &[StoredCommand],
) -> CommandEnvelope {
    if is_repeatable_command(*command.command_type()) {
        return distinguish_repeatable_command(command, sequence);
    }
    if is_failed_once_only_valve_retry(command, existing_commands) {
        return distinguish_command(command, sequence);
    }
    command.clone()
}

fn distinguish_command(command: &CommandEnvelope, sequence: usize) -> CommandEnvelope {
    CommandEnvelope::new(
        format!("{}_{sequence}", command.command_id()),
        *command.command_type(),
        command.aggregate_id().to_owned(),
        format!("{}:{sequence}", command.idempotency_key()),
        command.requested_by().to_owned(),
    )
}

fn is_failed_once_only_valve_retry(
    command: &CommandEnvelope,
    existing_commands: &[StoredCommand],
) -> bool {
    if !matches!(
        *command.command_type(),
        CommandType::WorkItemApproveRequested | CommandType::WorkItemAcceptRequested
    ) {
        return false;
    }
    let mut saw_attempt = false;
    for stored in existing_commands.iter().filter(|stored| {
        stored.command_type() == command.command_type().contract_name()
            && stored.aggregate_id() == Some(command.aggregate_id())
            && is_valve_attempt_key(stored.idempotency_key(), command.idempotency_key())
    }) {
        saw_attempt = true;
        if !matches!(stored.status(), "failed" | "rejected") {
            return false;
        }
    }
    saw_attempt
}

fn is_valve_attempt_key(stored_key: &str, base_key: &str) -> bool {
    stored_key == base_key
        || stored_key
            .strip_prefix(base_key)
            .is_some_and(|suffix| suffix.starts_with(':'))
}

fn command_correlation_id(command: &CommandEnvelope) -> String {
    format!("corr_{}", command.command_id())
}

fn factory_command_from_stored(
    stored_command: &StoredCommand,
) -> ConsoleRuntimeResult<Option<CommandEnvelope>> {
    let is_drain =
        stored_command.command_type() == CommandType::FactoryDrainRequested.contract_name();
    let is_dispatch_item =
        stored_command.command_type() == CommandType::FactoryDispatchItemRequested.contract_name();
    if !(is_drain || is_dispatch_item) {
        return Ok(None);
    }
    let Some(aggregate_id) = stored_command.aggregate_id() else {
        return Err(ConsoleRuntimeError::MissingCommandAggregate(
            stored_command.command_id().to_owned(),
        ));
    };
    Ok(Some(CommandEnvelope::new(
        stored_command.command_id().to_owned(),
        if is_drain {
            CommandType::FactoryDrainRequested
        } else {
            CommandType::FactoryDispatchItemRequested
        },
        aggregate_id.to_owned(),
        stored_command.idempotency_key().to_owned(),
        stored_command.requested_by().to_owned(),
    )))
}

/// A pending `work_item.*` command rebuilt from its stored form, tagged with the
/// handler it dispatches to. Reject and set-admission carry the raw persisted
/// `payload_json` (the `{"mode": ...}` and `{"policy": ...}` objects) so the
/// application handler can parse and validate the payload; approve and accept
/// carry no payload.
enum PendingWorkItemCommand {
    /// A rebuilt `work_item.approve_requested` command.
    Approve(CommandEnvelope),
    /// A rebuilt `work_item.accept_requested` command.
    Accept(CommandEnvelope),
    /// A rebuilt `work_item.reject_requested` command plus its stored payload.
    Reject {
        /// The rebuilt command envelope.
        command: CommandEnvelope,
        /// The persisted `payload_json` carrying `{"mode": ...}`.
        payload_json: String,
    },
    /// A rebuilt `work_item.set_admission_requested` command plus its stored
    /// payload.
    SetAdmission {
        /// The rebuilt command envelope.
        command: CommandEnvelope,
        /// The persisted `payload_json` carrying `{"policy": ...}`.
        payload_json: String,
    },
    /// A rebuilt `work_item.set_acceptance_requested` command plus its stored
    /// payload.
    SetAcceptance {
        /// The rebuilt command envelope.
        command: CommandEnvelope,
        /// The persisted `payload_json` carrying `{"policy": ...}`.
        payload_json: String,
    },
    /// A rebuilt `work_item.resolve_blocked_requested` command plus its stored
    /// payload.
    ResolveBlocked {
        /// The rebuilt command envelope.
        command: CommandEnvelope,
        /// The persisted `payload_json` carrying `{"target_status": ...}`.
        payload_json: String,
    },
    /// A rebuilt `work_item.move_requested` command plus its stored payload.
    Move {
        /// The rebuilt command envelope.
        command: CommandEnvelope,
        /// The persisted `payload_json` carrying `{"target_status": ...}`.
        payload_json: String,
    },
    /// A rebuilt `work_item.set_dispatcher_override_requested` command plus its
    /// stored payload.
    SetDispatcherOverride {
        /// The rebuilt command envelope.
        command: CommandEnvelope,
        /// The persisted `payload_json` carrying `{"setting": ..., "value": ...}`.
        payload_json: String,
    },
    /// A rebuilt `work_item.set_workflow_scope_override_requested` command plus
    /// its stored payload.
    SetWorkflowScopeOverride {
        /// The rebuilt command envelope.
        command: CommandEnvelope,
        /// The persisted `payload_json` carrying `{"scope": ...}`.
        payload_json: String,
    },
}

impl PendingWorkItemCommand {
    /// The wrapped command envelope, shared by every dispatch outcome.
    const fn command(&self) -> &CommandEnvelope {
        match self {
            Self::Approve(command)
            | Self::Accept(command)
            | Self::Reject { command, .. }
            | Self::SetAdmission { command, .. }
            | Self::SetAcceptance { command, .. }
            | Self::ResolveBlocked { command, .. }
            | Self::Move { command, .. }
            | Self::SetDispatcherOverride { command, .. }
            | Self::SetWorkflowScopeOverride { command, .. } => command,
        }
    }
}

/// Rebuild a `work_item.*` command from a stored command, tagged for dispatch,
/// or `None` when the stored command is not a work-item command. Recognizes the
/// approve, accept, reject, set-admission, and set-acceptance commands; the
/// reject, set-admission, and set-acceptance variants also surface the stored
/// `payload_json` (the payload-parsing path the reject slice introduced),
/// carrying `{"mode": ...}` and `{"policy": ...}` respectively.
fn work_item_command_from_stored(
    stored_command: &StoredCommand,
) -> ConsoleRuntimeResult<Option<PendingWorkItemCommand>> {
    let contract_name = stored_command.command_type();
    let is_approve = contract_name == CommandType::WorkItemApproveRequested.contract_name();
    let is_accept = contract_name == CommandType::WorkItemAcceptRequested.contract_name();
    let is_reject = contract_name == CommandType::WorkItemRejectRequested.contract_name();
    let is_set_admission =
        contract_name == CommandType::WorkItemSetAdmissionRequested.contract_name();
    let is_set_acceptance =
        contract_name == CommandType::WorkItemSetAcceptanceRequested.contract_name();
    let is_resolve_blocked =
        contract_name == CommandType::WorkItemResolveBlockedRequested.contract_name();
    let is_move = contract_name == CommandType::WorkItemMoveRequested.contract_name();
    let is_set_override =
        contract_name == CommandType::WorkItemSetDispatcherOverrideRequested.contract_name();
    let is_set_workflow_scope =
        contract_name == CommandType::WorkItemSetWorkflowScopeOverrideRequested.contract_name();
    if !(is_approve
        || is_accept
        || is_reject
        || is_set_admission
        || is_set_acceptance
        || is_resolve_blocked
        || is_move
        || is_set_override
        || is_set_workflow_scope)
    {
        return Ok(None);
    }
    let Some(aggregate_id) = stored_command.aggregate_id() else {
        return Err(ConsoleRuntimeError::MissingCommandAggregate(
            stored_command.command_id().to_owned(),
        ));
    };
    let rebuild = |command_type| {
        CommandEnvelope::new(
            stored_command.command_id().to_owned(),
            command_type,
            aggregate_id.to_owned(),
            stored_command.idempotency_key().to_owned(),
            stored_command.requested_by().to_owned(),
        )
    };
    let pending = if is_approve {
        PendingWorkItemCommand::Approve(rebuild(CommandType::WorkItemApproveRequested))
    } else if is_accept {
        PendingWorkItemCommand::Accept(rebuild(CommandType::WorkItemAcceptRequested))
    } else if is_reject {
        // Reject surfaces the stored `payload_json` (the `{"mode": ...}` object)
        // for the application handler to parse.
        PendingWorkItemCommand::Reject {
            command: rebuild(CommandType::WorkItemRejectRequested),
            payload_json: stored_command.payload_json().to_owned(),
        }
    } else if is_set_admission {
        // Set-admission is the admission policy dial: it surfaces the stored
        // `payload_json` (the `{"policy": ...}` object) for the application
        // handler to parse.
        PendingWorkItemCommand::SetAdmission {
            command: rebuild(CommandType::WorkItemSetAdmissionRequested),
            payload_json: stored_command.payload_json().to_owned(),
        }
    } else if is_set_acceptance {
        // Set-acceptance is the acceptance policy dial: it surfaces the stored
        // `payload_json` (the `{"policy": ...}` object) for the application
        // handler to parse.
        PendingWorkItemCommand::SetAcceptance {
            command: rebuild(CommandType::WorkItemSetAcceptanceRequested),
            payload_json: stored_command.payload_json().to_owned(),
        }
    } else if is_resolve_blocked {
        // Resolve-blocked moves a `blocked` item to `ready`/`backlog`: it
        // surfaces the stored `payload_json` (the `{"target_status": ...}`
        // object) for the application handler to parse.
        PendingWorkItemCommand::ResolveBlocked {
            command: rebuild(CommandType::WorkItemResolveBlockedRequested),
            payload_json: stored_command.payload_json().to_owned(),
        }
    } else if is_move {
        // Move relocates an item to a pre-terminal pipeline status: it surfaces
        // the stored `payload_json` (the `{"target_status": ...}` object) for the
        // application handler to parse.
        PendingWorkItemCommand::Move {
            command: rebuild(CommandType::WorkItemMoveRequested),
            payload_json: stored_command.payload_json().to_owned(),
        }
    } else if is_set_override {
        // Set-dispatcher-override sets/clears one per-item cap override: it
        // surfaces the stored `payload_json` (the `{"setting": ..., "value": ...}`
        // object) for the application handler to parse.
        PendingWorkItemCommand::SetDispatcherOverride {
            command: rebuild(CommandType::WorkItemSetDispatcherOverrideRequested),
            payload_json: stored_command.payload_json().to_owned(),
        }
    } else {
        // Set-workflow-scope-override records a declared workflow path as
        // citation-only: it surfaces the stored `payload_json` (the
        // `{"scope": ...}` object) for the application handler to parse.
        PendingWorkItemCommand::SetWorkflowScopeOverride {
            command: rebuild(CommandType::WorkItemSetWorkflowScopeOverrideRequested),
            payload_json: stored_command.payload_json().to_owned(),
        }
    };
    Ok(Some(pending))
}

fn event_append_from_command_event(
    event: &ConsoleEvent,
    command: &CommandEnvelope,
    observed_at: &str,
) -> EventAppend {
    EventAppend::new(
        event.clone(),
        command.aggregate_id().to_owned(),
        observed_at.to_owned(),
        observed_at.to_owned(),
        Some(command.command_id().to_owned()),
        command_correlation_id(command),
        Some(event.event_id().to_owned()),
        event.payload_json().to_owned(),
        "{}".to_owned(),
    )
}

fn event_append_from_console_event(event: &ConsoleEvent, observed_at: &str) -> EventAppend {
    EventAppend::new(
        event.clone(),
        event.stream_id().to_owned(),
        observed_at.to_owned(),
        observed_at.to_owned(),
        None,
        format!("corr_{}", event.event_id()),
        Some(event.event_id().to_owned()),
        event.payload_json().to_owned(),
        "{}".to_owned(),
    )
}

fn event_append_from_normalized_source_event(
    normalized: &NormalizedSourceEvent,
    observed_at: &str,
) -> EventAppend {
    let event = normalized.event();
    EventAppend::new(
        event.clone(),
        event.stream_id().to_owned(),
        observed_at.to_owned(),
        observed_at.to_owned(),
        None,
        format!("corr_{}", event.event_id()),
        Some(normalized.source_event_id().to_owned()),
        normalized_payload_json(normalized.payload()),
        "{}".to_owned(),
    )
}

/// The persisted `payload_json` for a normalized observation. Work-item
/// snapshots are serialized in full so the lane board can rebuild from them; a
/// not-observed findings carry their human-readable reason so the operator can
/// DURABLY see WHY a source is unavailable (Adapter Contract honesty rule).
/// Work-item, attention-item, and Fabro-run payloads persist the projection
/// fields their replay paths need; source payloads with no replay state persist
/// as `{}`.
fn normalized_payload_json(payload: &SourcePayload) -> String {
    match payload {
        SourcePayload::WorkItemSnapshot(snapshot) => work_item_snapshot_payload_json(snapshot),
        SourcePayload::AttentionItemAppeared(item) | SourcePayload::AttentionItemChanged(item) => {
            attention_item_payload_json(item)
        }
        SourcePayload::AttentionItemResolved(id) => attention_resolved_payload_json(id),
        SourcePayload::NotObservedFinding(finding) => not_observed_finding_payload_json(finding),
        SourcePayload::FabroRunSnapshot(snapshot) => fabro_run_snapshot_payload_json(snapshot),
        SourcePayload::DispatcherJournalEntry(entry) => dispatcher_journal_payload_json(entry),
        SourcePayload::CompletenessFinding(_)
        | SourcePayload::GithubPullRequestSnapshot(_)
        | SourcePayload::LivespecNextSnapshot(_)
        | SourcePayload::ObservedIdle => "{}".to_owned(),
    }
}

struct SharedSqliteStore<'a> {
    store: Rc<RefCell<&'a mut SqliteEventStore>>,
}

impl<'a> SharedSqliteStore<'a> {
    fn new(store: &'a mut SqliteEventStore) -> Self {
        Self {
            store: Rc::new(RefCell::new(store)),
        }
    }
}

impl Clone for SharedSqliteStore<'_> {
    fn clone(&self) -> Self {
        Self {
            store: Rc::clone(&self.store),
        }
    }
}

struct SqliteCheckpointPort<'a> {
    shared: SharedSqliteStore<'a>,
    advanced_at: String,
}

impl<'a> SqliteCheckpointPort<'a> {
    fn new(shared: SharedSqliteStore<'a>, advanced_at: &str) -> Self {
        Self {
            shared,
            advanced_at: advanced_at.to_owned(),
        }
    }
}

impl SourceCheckpointPort for SqliteCheckpointPort<'_> {
    fn load_checkpoint(&self, adapter_id: &str) -> Result<Option<String>, AdapterError> {
        self.shared
            .store
            .borrow()
            .load_checkpoint(adapter_id)
            .map_err(|_error| AdapterError::CheckpointLoadFailed)
    }

    fn save_checkpoint(&self, adapter_id: &str, checkpoint: &str) -> Result<(), AdapterError> {
        self.shared
            .store
            .borrow_mut()
            .save_checkpoint(adapter_id, checkpoint, &self.advanced_at)
            .map_err(|_error| AdapterError::CheckpointSaveFailed)
    }
}

struct SqliteSourceEventLog<'a> {
    shared: SharedSqliteStore<'a>,
}

impl<'a> SqliteSourceEventLog<'a> {
    const fn new(shared: SharedSqliteStore<'a>) -> Self {
        Self { shared }
    }
}

impl SourceEventAppendPort for SqliteSourceEventLog<'_> {
    fn append_normalized_event(
        &mut self,
        event: &NormalizedSourceEvent,
        observed_at: &str,
    ) -> Result<(), AdapterError> {
        let append = event_append_from_normalized_source_event(event, observed_at);
        self.shared
            .store
            .borrow_mut()
            .append_event(&append)
            .map(|_outcome| ())
            .map_err(|error| AdapterError::AppendFailed(format!("{error:?}")))
    }
}

#[cfg(test)]
struct ScriptedSource {
    poll: AdapterPoll,
}

#[cfg(test)]
impl ScriptedSource {
    const fn new(poll: AdapterPoll) -> Self {
        Self { poll }
    }
}

#[cfg(test)]
impl PullSourcePort for ScriptedSource {
    fn poll(&self, _request: &AdapterPollRequest) -> Result<AdapterPoll, AdapterError> {
        Ok(self.poll.clone())
    }
}

fn run_events(subcommand: Option<&str>) -> RunOutput {
    match subcommand {
        Some("tail") => RunOutput::new(0, "events tail bootstrap: not yet wired".to_owned()),
        _ => RunOutput::new(
            2,
            "usage: livespec-console-beads-fabro events tail".to_owned(),
        ),
    }
}

fn run_events_with_store(values: &[String], store: &SqliteEventStore) -> RunOutput {
    match values.get(2).map(String::as_str) {
        Some("tail") => run_store_result(events_tail_report(store, 20), "events"),
        _other => RunOutput::new(
            2,
            "usage: livespec-console-beads-fabro events tail".to_owned(),
        ),
    }
}

fn run_plans_with_store(values: &[String], store: &SqliteEventStore) -> RunOutput {
    match values.get(2).map(String::as_str) {
        Some(epic_id) if !epic_id.trim().is_empty() => {
            run_store_result(plan_page_report(store, epic_id), "plans")
        }
        _other => RunOutput::new(
            2,
            "usage: livespec-console-beads-fabro plans <epic-id>".to_owned(),
        ),
    }
}

fn tui_preview() -> String {
    let events = demo_events();
    let model = build_tui_model(&events, 0);
    render_tui_preview(&model, 100, 28)
}

#[must_use]
/// Return the demo events value.
pub fn demo_events() -> [ConsoleEvent; 2] {
    [
        ConsoleEvent::fixture(
            "evt_demo_1",
            EventType::WorkItemSnapshotObserved,
            "orchestrator",
        )
        .with_payload_json(
            r#"{"repo":"console","work_item_id":"console-blocked","lane":"blocked","lane_reason":"needs-human","rank":"a0","status":"blocked","source_version":1}"#
                .to_owned(),
        ),
        ConsoleEvent::fixture(
            "evt_demo_2",
            EventType::WorkItemSnapshotObserved,
            "orchestrator",
        )
        .with_payload_json(
            r#"{"repo":"console","work_item_id":"console-accept","lane":"acceptance","lane_reason":null,"rank":"a1","status":"acceptance","source_version":1}"#
                .to_owned(),
        ),
    ]
}

fn render_tui_preview(
    model: &console_application::TuiScreenModel,
    width: u16,
    height: u16,
) -> String {
    match console_tui::render_to_text(model, width, height) {
        Ok(rendered) => rendered,
        Err(_error) => "TUI render error: empty area".to_owned(),
    }
}

fn help_text() -> String {
    [
        "livespec-console-beads-fabro",
        "",
        "Commands:",
        "  tui",
        "  serve",
        "  backfill",
        "  events tail",
        "  docs key-action-reference",
        "  plans <epic-id>",
        "  snapshot",
        "  doctor",
        "  arch-check",
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::manual_assert, clippy::option_if_let_else, clippy::panic)]

    use crate::effect_sink_io_error;

    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::error::Error;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use console_application::{
        ApplicationError, AttentionItem, AutonomousAudit, AutonomousDecision,
        AutonomousDecisionsPort, DispatcherOverride, FactoryCommandOutcome,
        FactoryDispatchItemPort, FactoryDrainPolicy, FactoryDrainPort, FactoryDrainPortOutcome,
        FactoryDrainRequest, LaneColumn, LaneFocus, OrchestratorActionOutcome,
        OrchestratorActionPort, OrchestratorActionRequest, OverrideInt, PendingValve, RejectMode,
        TuiInteraction, TuiInteractionState, TuiOverlay, TuiView, build_tui_model,
        handle_factory_drain_command, project_attention, project_lane_board,
        source_adapters::{
            AcceptancePolicy, AdapterError, AdapterIngestionSummary, AdapterPoll,
            AdapterPollRequest, AdmissionPolicy, AttentionHandoff, AttentionItemSnapshot,
            AttentionSourceRef, DispatcherJournalEntry, DispatcherJournalKind, Lane, LaneReason,
            NeedsAttentionReadOutcome, NeedsAttentionSnapshotPort, NormalizedSourceEvent,
            NotObservedFinding, ObservedSourceAdapter, PullSourcePort, SourceAdapterKind,
            SourceEventAppendPort, SourcePayload, SourceProbe, SourceProbeOutcome,
            WorkItemSnapshot, diff_needs_attention, normalize_work_item_snapshot,
        },
    };
    use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};
    use console_eventstore::{
        AppendOutcome, AppendStatus, CommandAppend, CommandAppendOutcome, CommandAppendStatus,
        CommandStatusUpdateOutcome, EventAppend, EventStoreError, EventStoreResult,
        SqliteEventStore, StoredCommand, StoredEvent,
    };
    use console_tui::{
        TuiLiveSession, TuiRuntimeEffect, TuiRuntimeEffectSink, TuiRuntimeEffectSinkOutcome,
        TuiTerminalInput,
    };

    use super::{
        BackingCliResolution, BackingCliResolutionError, CommandAppendStore, ConsoleRuntimeError,
        ConsoleRuntimeResult, EventAppendStore, FactoryCommandStore, InitialSourceSeed,
        NeedsAttentionIngest, PendingCommandOutcome, PendingCommandRequester, PluginResolution,
        ResolveInputs, ScriptedSource, SharedSqliteStore, SourceAdapterRef, SourcePollRequester,
        SqliteSourceEventLog, StoreBackedTuiRuntimeEffectSink, TuiSessionOutcome, TuiSessionRunner,
        append_demo_events_to_store, append_factory_drain_requested_events, backfill_demo_report,
        backfill_source_adapters, backfill_source_report, command_status_update_runtime_result,
        config_command_from_stored, demo_events, distinguish_repeatable_command, doctor_report,
        event_append_from_command_event, event_append_from_console_event, events_tail_report,
        factory_command_from_stored, handle_pending_config_commands,
        handle_pending_control_commands, handle_pending_factory_commands,
        handle_pending_factory_commands_with_dispatch_port, handle_pending_work_item_commands,
        ingest_needs_attention, initial_source_seed, is_failed_once_only_valve_retry,
        live_source_adapters, load_tui_events_from_store, normalized_payload_json,
        observe_and_reflect_autonomous_decisions, older_factory_command_blocks_control_command,
        persist_tui_runtime_effects, plan_page_report, python_normalized_invocation,
        refresh_sources, render_tui_preview, resolve_console_repo, run,
        run_store_backed_tui_session, run_with_store, serve_report,
        serve_report_with_dispatch_port, snapshot_report, source_polls_from_seed,
        work_item_command_from_stored,
    };

    #[test]
    fn resolve_console_repo_prefers_non_empty_env_override() {
        check(
            (resolve_console_repo(
                Some("  livespec-orchestrator-beads-fabro  "),
                Some(Path::new("/data/projects/livespec-console-beads-fabro")),
            )) == ("livespec-orchestrator-beads-fabro"),
            "assert_eq failed",
        );
    }

    #[test]
    fn resolve_console_repo_uses_working_directory_basename() {
        // Matches how the orchestrator's needs-attention surface derives
        // `source_ref.repo` (its `project_root.name`), so the two agree and
        // "Repos observed" collapses to the single observed tenant.
        check(
            (resolve_console_repo(
                None,
                Some(Path::new(
                    "/data/projects/livespec-orchestrator-beads-fabro",
                )),
            )) == ("livespec-orchestrator-beads-fabro"),
            "assert_eq failed",
        );
    }

    #[test]
    fn resolve_console_repo_falls_back_when_no_basename() {
        // An empty / whitespace override does not win; a working directory with no
        // usable basename falls back to the console's own package name.
        check(
            (resolve_console_repo(Some("   "), Some(Path::new("/"))))
                == ("livespec-console-beads-fabro"),
            "assert_eq failed",
        );
        check(
            (resolve_console_repo(None, None)) == ("livespec-console-beads-fabro"),
            "assert_eq failed",
        );
    }

    /// Scriptable needs-attention snapshot-source port double: returns a canned
    /// read outcome so ingestion tests can drive the diff-at-ingest with a real
    /// snapshot without a live orchestrator CLI.
    struct ScriptedNeedsAttentionPort {
        outcome: NeedsAttentionReadOutcome,
    }

    impl ScriptedNeedsAttentionPort {
        fn observing(items: Vec<AttentionItemSnapshot>) -> Self {
            Self {
                outcome: NeedsAttentionReadOutcome::Observed(items),
            }
        }

        fn unavailable(reason: &str) -> Self {
            Self {
                outcome: NeedsAttentionReadOutcome::Unavailable(reason.to_owned()),
            }
        }
    }

    impl NeedsAttentionSnapshotPort for ScriptedNeedsAttentionPort {
        fn read_snapshot(&self) -> NeedsAttentionReadOutcome {
            self.outcome.clone()
        }
    }

    /// A needs-attention port observing an empty snapshot — nothing to ingest —
    /// for the many store-backed tests that exercise the pull adapters and
    /// factory commands but not the needs-attention stream.
    fn empty_needs_attention_port() -> ScriptedNeedsAttentionPort {
        ScriptedNeedsAttentionPort::observing(Vec::new())
    }

    fn duplicate_collision_append(
        normalized: &NormalizedSourceEvent,
        observed_at: &str,
    ) -> EventAppend {
        let event = normalized.event();
        EventAppend::new(
            ConsoleEvent::new(
                format!("evt:collision:{}", normalized.source_event_id()),
                1,
                event.context().to_owned(),
                EventType::FabroHumanGateObserved,
                event.source().to_owned(),
                event.stream_id().to_owned(),
                event.stream_seq(),
            ),
            event.stream_id().to_owned(),
            observed_at.to_owned(),
            observed_at.to_owned(),
            None,
            format!("corr_collision_{}", normalized.source_event_id()),
            Some(normalized.source_event_id().to_owned()),
            "{}".to_owned(),
            "{}".to_owned(),
        )
    }

    /// A single well-shaped attention item for building snapshot fixtures.
    fn attention_item_fixture(id: &str, summary: &str) -> AttentionItemSnapshot {
        AttentionItemSnapshot::new(
            id,
            "human-valve",
            "high",
            summary,
            AttentionSourceRef::new("livespec-console-beads-fabro", Some(id), None),
            AttentionHandoff::new("approve", Some("approve"), &format!("approve:{id}")),
        )
    }

    fn scripted_source_list() -> Vec<(String, ScriptedSource)> {
        source_polls_from_seed(&initial_source_seed())
            .ok_test()
            .into_iter()
            .map(|(adapter_id, poll)| (adapter_id.to_owned(), ScriptedSource::new(poll)))
            .collect()
    }

    fn scripted_source_list_with_ready_work() -> Vec<(String, ScriptedSource)> {
        let mut sources = scripted_source_list();
        let snapshot = WorkItemSnapshot::new(
            "livespec-console-beads-fabro",
            "livespec-console-beads-fabro-ready",
            Lane::Ready,
            None,
            "a0",
            "ready",
            AdmissionPolicy::Manual,
            AcceptancePolicy::AiThenHuman,
            7,
        )
        .ok_test();
        sources.push((
            "orchestrator-ready:livespec-console-beads-fabro".to_owned(),
            ScriptedSource::new(normalize_work_item_snapshot(&snapshot)),
        ));
        sources
    }

    fn scripted_source_refs(sources: &[(String, ScriptedSource)]) -> Vec<SourceAdapterRef<'_>> {
        sources
            .iter()
            .map(|(adapter_id, source)| (adapter_id.as_str(), source as &dyn PullSourcePort))
            .collect()
    }

    // Most store-backed command tests do not care which sources or factory port
    // back the run, only that the command dispatches: drive them with the
    // scripted seed and a completing drain double.
    fn run_with_store_scripted(
        args: &[String],
        store: &mut SqliteEventStore,
        observed_at: &str,
    ) -> super::RunOutput {
        let scripted = scripted_source_list();
        let sources = scripted_source_refs(&scripted);
        let mut port = SimulatedFactoryDrainPort;
        let mut work_item_port = SimulatedWorkItemActionPort::default();
        let na_port = empty_needs_attention_port();
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");
        run_with_store(
            args,
            store,
            observed_at,
            &sources,
            &mut port,
            &mut work_item_port,
            &empty_decisions_port(),
            &needs_attention,
        )
    }

    fn resolver_empty_env() -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    fn resolver_temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok_test()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "livespec-console-backing-cli-{name}-{}-{nanos}",
            std::process::id()
        ));
        let _ignored = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).ok_test();
        path
    }

    fn resolver_plugin_root_with_bin(base: &Path, name: &str, bin_rel: &str) -> PathBuf {
        let root = base.join(name);
        let bin = root.join(bin_rel);
        fs::create_dir_all(&bin).ok_test();
        for script in [
            "needs_attention.py",
            "list_work_items.py",
            "drive.py",
            "dispatcher.py",
            "next.py",
        ] {
            fs::write(bin.join(script), "#!/usr/bin/env python3\n").ok_test();
        }
        root
    }

    /// Build a plugin root in the SOURCE layout (`<root>/.claude-plugin/scripts/bin`),
    /// the shape a governed spec checkout carries.
    fn resolver_plugin_root(base: &Path, name: &str) -> PathBuf {
        resolver_plugin_root_with_bin(base, name, ".claude-plugin/scripts/bin")
    }

    /// Build a plugin root in the FLATTENED installed-marketplace-cache layout
    /// (`<root>/scripts/bin`), the shape the Claude plugin installer produces
    /// after collapsing `.claude-plugin/scripts/` to `scripts/`.
    fn resolver_flattened_plugin_root(base: &Path, name: &str) -> PathBuf {
        resolver_plugin_root_with_bin(base, name, "scripts/bin")
    }

    fn resolver_inputs(
        env: BTreeMap<String, String>,
        current_dir: PathBuf,
        home_dir: Option<PathBuf>,
    ) -> ResolveInputs {
        ResolveInputs {
            env,
            current_dir,
            home_dir,
        }
    }

    struct UnavailableProbe;

    impl SourceProbe for UnavailableProbe {
        fn run_command(&self, _program: &str, _args: &[&str]) -> SourceProbeOutcome {
            SourceProbeOutcome::unavailable("test probe: no command sources")
        }

        fn read_file(&self, _path: &str) -> SourceProbeOutcome {
            SourceProbeOutcome::unavailable("test probe: no file sources")
        }
    }

    fn dispatcher_source_event(event_id: &str, stream_seq: u64) -> NormalizedSourceEvent {
        NormalizedSourceEvent::new(
            ConsoleEvent::new(
                event_id.to_owned(),
                1,
                "factory".to_owned(),
                EventType::DispatcherBacklogBounceObserved,
                "dispatcher".to_owned(),
                "dispatcher:console".to_owned(),
                stream_seq,
            ),
            event_id.to_owned(),
            SourcePayload::NotObservedFinding(NotObservedFinding::new(
                "console",
                SourceAdapterKind::Dispatcher,
                "test fixture",
            )),
        )
    }

    fn source_backfill_report_for_dispatcher_events(
        checkpoint: &str,
        observed_at: &str,
    ) -> ConsoleRuntimeResult<String> {
        let skipped = dispatcher_source_event(
            "evt:dispatcher:console:console-1:dispatch-too-large:18446744073709551615",
            u64::MAX,
        );
        let sibling = dispatcher_source_event("evt:dispatcher:console:console-2:dispatch-ok:2", 2);
        let Ok(poll) = console_application::source_adapters::AdapterPoll::new(
            checkpoint,
            vec![skipped, sibling],
        ) else {
            return Err(ConsoleRuntimeError::Adapter(AdapterError::EmptyCheckpoint));
        };
        let source = ScriptedSource::new(poll);
        let sources: [SourceAdapterRef<'_>; 1] = [("dispatcher:console", &source)];
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let na_port = empty_needs_attention_port();
        let needs_attention = NeedsAttentionIngest::new(&na_port, "console");

        backfill_source_report(&mut store, observed_at, &sources, &needs_attention)
    }

    #[test]
    fn help_lists_specified_command_shape() {
        let output = run(["bin", "help"]);

        check((output.code()) == (0), "assert_eq failed");
        check(output.message().contains("events tail"), "assert failed");
        check(
            output.message().contains("docs key-action-reference"),
            "assert failed",
        );
        check(output.message().contains("arch-check"), "assert failed");
    }

    #[test]
    fn docs_key_action_reference_command_prints_the_generated_reference() {
        let output = run(["bin", "docs", "key-action-reference"]);

        check((output.code()) == (0), "assert_eq failed");
        check(
            (output.message())
                == (console_application::action_registry::operator_key_action_reference_markdown()),
            "assert_eq failed",
        );
    }

    #[test]
    fn docs_without_a_known_subcommand_is_usage_error() {
        for args in [
            vec!["bin", "docs"],
            vec!["bin", "docs", "unknown-reference"],
        ] {
            let output = run(args);
            check((output.code()) == (2), "assert_eq failed");
            check(
                (output.message())
                    == ("usage: livespec-console-beads-fabro docs key-action-reference"),
                "assert_eq failed",
            );
        }
    }

    #[test]
    fn tui_command_projects_demo_attention_items() {
        let output = run(["bin", "tui"]);

        check((output.code()) == (0), "assert_eq failed");
        check(
            output.message().contains("LiveSpec Console"),
            "assert failed",
        );
        check(output.message().contains("> Attention"), "assert failed");
        check(
            output.message().contains("> Blocked: needs-human"),
            "assert failed",
        );
        check(output.message().contains("Repo: console"), "assert failed");
        check(output.message().contains("Fabro run: -"), "assert failed");
        check(
            !output.message().contains("Attach: fabro attach evt_demo_1"),
            "assert failed",
        );
        check(!output.message().contains("Attach:"), "assert failed");
        check(!output.message().contains("Actions:"), "assert failed");
    }

    #[test]
    fn unknown_command_is_usage_error() {
        let output = run(["bin", "bogus"]);

        check((output.code()) == (2), "assert_eq failed");
        check(
            output.message().contains("unknown command: bogus"),
            "assert failed",
        );
    }

    #[test]
    fn no_command_prints_help() {
        let output = run(["bin"]);

        check((output.code()) == (0), "assert_eq failed");
        check(output.message().contains("Commands:"), "assert failed");
    }

    #[test]
    fn bootstrap_commands_report_placeholder_modes() {
        for (command, expected) in [
            ("serve", "serve mode bootstrap: not yet wired"),
            ("backfill", "backfill mode bootstrap: not yet wired"),
            ("snapshot", "snapshot mode bootstrap: not yet wired"),
            ("doctor", "doctor bootstrap: no findings"),
            (
                "arch-check",
                "run `just check-arch` for architecture enforcement",
            ),
        ] {
            let output = run(["bin", command]);

            check((output.code()) == (0), "assert_eq failed");
            check((output.message()) == (expected), "assert_eq failed");
        }
    }

    #[test]
    fn events_tail_reports_placeholder_mode() {
        let output = run(["bin", "events", "tail"]);

        check((output.code()) == (0), "assert_eq failed");
        check(
            (output.message()) == ("events tail bootstrap: not yet wired"),
            "assert_eq failed",
        );
    }

    #[test]
    fn events_without_tail_is_usage_error() {
        let output = run(["bin", "events"]);

        check((output.code()) == (2), "assert_eq failed");
        check(
            (output.message()) == ("usage: livespec-console-beads-fabro events tail"),
            "assert_eq failed",
        );
    }

    #[test]
    fn plans_without_epic_id_is_usage_error() {
        let output = run(["bin", "plans"]);

        check((output.code()) == (2), "assert_eq failed");
        check(
            (output.message()) == ("usage: livespec-console-beads-fabro plans <epic-id>"),
            "assert_eq failed",
        );
    }

    #[test]
    fn store_backed_backfill_command_reports_source_adapter_counts() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();

        let first = run_with_store_scripted(
            &command_args(&["bin", "backfill"]),
            &mut store,
            "2026-06-23T00:00:00Z",
        );
        let second = run_with_store_scripted(
            &command_args(&["bin", "backfill"]),
            &mut store,
            "2026-06-23T00:00:00Z",
        );

        check((first.code()) == (0), "assert_eq failed");
        check(
            (first.message()) == ("backfill source adapters: adapters 5, events 6"),
            "assert_eq failed",
        );
        check((second.code()) == (0), "assert_eq failed");
        check(
            (second.message()) == ("backfill source adapters: adapters 5, events 6"),
            "assert_eq failed",
        );
        check(
            (store.list_console_events().ok_test().len()) == (6),
            "assert_eq failed",
        );
        check(
            (store
                .load_checkpoint("orchestrator:livespec-console-beads-fabro")
                .ok_test())
                == (Some("1".to_owned())),
            "assert_eq failed",
        );
        check(
            (store
                .load_checkpoint("dispatcher:livespec-console-beads-fabro")
                .ok_test())
                == (Some("2".to_owned())),
            "assert_eq failed",
        );
        check(
            (store
                .load_checkpoint("fabro:livespec-console-beads-fabro")
                .ok_test())
                == (Some("3".to_owned())),
            "assert_eq failed",
        );
        check(
            (store
                .load_checkpoint("livespec:livespec-console-beads-fabro")
                .ok_test())
                == (Some("4".to_owned())),
            "assert_eq failed",
        );
        check(
            (store
                .load_checkpoint("github:livespec-console-beads-fabro")
                .ok_test())
                == (Some("5".to_owned())),
            "assert_eq failed",
        );
    }

    #[test]
    fn source_backfill_report_names_skipped_source_record() {
        let report =
            source_backfill_report_for_dispatcher_events("ck", "2026-06-24T00:00:00Z").ok_test();

        check(
            (report)
                == ("backfill source adapters: adapters 1, events 1, skipped evt:dispatcher:console:console-1:dispatch-too-large:18446744073709551615"),
            "assert_eq failed",
        );
    }

    #[test]
    fn source_backfill_report_helper_covers_checkpoint_error() {
        check(
            format!(
                "{:?}",
                source_backfill_report_for_dispatcher_events(" ", "2026-06-24T00:00:00Z")
            )
            .contains("EmptyCheckpoint"),
            "assert failed",
        );
    }

    #[test]
    fn source_backfill_report_helper_covers_observed_at_error() {
        check(
            format!(
                "{:?}",
                source_backfill_report_for_dispatcher_events("ck", " ")
            )
            .contains("EmptyObservedAt"),
            "assert failed",
        );
    }

    #[test]
    fn sqlite_source_event_log_appends_top_bit_dispatcher_hash() {
        let high_hash = 10_161_696_490_713_690_059_u64;
        let event = dispatcher_source_event(
            "evt:dispatcher:console:console-1:dispatch-high:10161696490713690059",
            high_hash & 0x7fff_ffff_ffff_ffff,
        );
        check(
            i64::try_from(event.event().stream_seq()).is_ok(),
            "assert failed",
        );

        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let shared = SharedSqliteStore::new(&mut store);
        let mut event_log = SqliteSourceEventLog::new(shared);
        check(
            (event_log.append_normalized_event(&event, "2026-06-24T00:00:00Z")) == (Ok(())),
            "assert_eq failed",
        );

        check(
            (store.list_events().ok_test().len()) == (1),
            "assert_eq failed",
        );
    }

    #[test]
    fn source_backfill_rejects_empty_observed_at() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let scripted = scripted_source_list();
        let sources = scripted_source_refs(&scripted);
        let na_port = empty_needs_attention_port();
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");

        let result = backfill_source_report(&mut store, "", &sources, &needs_attention);

        check(
            format!("{result:?}").contains("EmptyObservedAt"),
            "assert failed",
        );
        check(
            store.list_console_events().ok_test().is_empty(),
            "assert_eq failed",
        );
    }

    #[test]
    fn source_seed_builder_rejects_invalid_static_identity_fields() {
        for (seed, expected_error) in [
            (
                InitialSourceSeed {
                    repo: " ",
                    ..initial_source_seed()
                },
                AdapterError::EmptyRepo,
            ),
            (
                InitialSourceSeed {
                    work_item_id: " ",
                    ..initial_source_seed()
                },
                AdapterError::EmptyWorkItemId,
            ),
            (
                InitialSourceSeed {
                    dispatch_id: " ",
                    ..initial_source_seed()
                },
                AdapterError::EmptyDispatchId,
            ),
            (
                InitialSourceSeed {
                    run_id: " ",
                    ..initial_source_seed()
                },
                AdapterError::EmptyRunId,
            ),
        ] {
            let result = source_polls_from_seed(&seed);

            check(
                format!("{result:?}").contains(&format!("{expected_error:?}")),
                "assert failed",
            );
        }
    }

    #[test]
    fn demo_backfill_report_counts_inserted_and_duplicate_events() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();

        let first = backfill_demo_report(&mut store, "2026-06-23T00:00:00Z").ok_test();
        let second = backfill_demo_report(&mut store, "2026-06-23T00:00:01Z").ok_test();

        check(
            (first) == ("backfill demo events: inserted 2, duplicate 0"),
            "assert_eq failed",
        );
        check(
            (second) == ("backfill demo events: inserted 0, duplicate 2"),
            "assert_eq failed",
        );
    }

    #[test]
    fn store_backed_events_tail_reports_persisted_events() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        append_demo_events_to_store(&mut store, "2026-06-23T00:00:00Z").ok_test();

        let output = run_with_store_scripted(
            &command_args(&["bin", "events", "tail"]),
            &mut store,
            "unused",
        );

        check((output.code()) == (0), "assert_eq failed");
        check(output.message().contains("events tail"), "assert failed");
        check(output.message().contains("evt_demo_1"), "assert failed");
        check(
            output.message().contains("work_item.snapshot_observed"),
            "assert failed",
        );
        check(output.message().contains("evt_demo_2"), "assert failed");
    }

    #[test]
    fn store_backed_serve_bootstraps_empty_store() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();

        let output = run_with_store_scripted(
            &command_args(&["bin", "serve"]),
            &mut store,
            "2026-06-23T00:00:00Z",
        );

        check((output.code()) == (0), "assert_eq failed");
        check(
            (output.message())
                == ("serve: store ready\nbackfill events: 6\nevents: 6\nattention: 0\ncommands: 0\npending: 0\nfactory commands handled: 0\nwork-item commands handled: 0"),
            "assert_eq failed",
        );
        check(
            (store.list_console_events().ok_test().len()) == (6),
            "assert_eq failed",
        );
        check(
            (store
                .load_checkpoint("github:livespec-console-beads-fabro")
                .ok_test())
                == (Some("5".to_owned())),
            "assert_eq failed",
        );
    }

    #[test]
    fn store_backed_serve_threads_injected_drain_port() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let persistence = persist_tui_runtime_effects(
            &mut store,
            &[factory_drain_effect()],
            "2026-06-23T00:00:01Z",
        );
        check(persistence.is_ok(), "assert failed");

        // The scripted run injects a completing drain double, so the pending
        // command is handled through the injected port: accepted + started +
        // completed (three events) and the command lands `completed`. The honest
        // not-wired behaviour of the real port is covered in console-application.
        let scripted = scripted_source_list_with_ready_work();
        let sources = scripted_source_refs(&scripted);
        let mut port = SimulatedFactoryDrainPort;
        let mut work_item_port = SimulatedWorkItemActionPort::default();
        let na_port = empty_needs_attention_port();
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");
        let output = run_with_store(
            &command_args(&["bin", "serve"]),
            &mut store,
            "2026-06-23T00:00:02Z",
            &sources,
            &mut port,
            &mut work_item_port,
            &empty_decisions_port(),
            &needs_attention,
        );

        check((output.code()) == (0), "assert_eq failed");
        check(
            (output.message())
                == ("serve: store ready\nbackfill events: 8\nevents: 11\nattention: 0\ncommands: 1\npending: 0\nfactory commands handled: 1\nwork-item commands handled: 0"),
            "assert_eq failed",
        );
        check(
            (store.list_commands().ok_test()[0].status()) == ("completed"),
            "assert_eq failed",
        );
    }

    #[test]
    fn store_backed_serve_reingests_sources_over_a_non_empty_store() {
        // Bug A fix: serve re-ingests the source adapters on EVERY run, not only
        // when the log is empty, so the report reflects the CURRENT ledger. Here
        // the store already holds the 2 demo events; the scripted seed adds its 6
        // source events on top (checkpointed/idempotent per Scenario 3), so the
        // report tallies backfill events 6 over a store that grows to 8 events.
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        append_demo_events_to_store(&mut store, "2026-06-23T00:00:00Z").ok_test();
        let scripted = scripted_source_list();
        let sources = scripted_source_refs(&scripted);
        let mut port = SimulatedFactoryDrainPort;
        let mut work_item_port = SimulatedWorkItemActionPort::default();
        let na_port = empty_needs_attention_port();
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");

        let report = serve_report(
            &mut store,
            "2026-06-23T00:00:01Z",
            &sources,
            &mut port,
            &mut work_item_port,
            &empty_decisions_port(),
            &needs_attention,
        );

        check(
            (report.ok_test())
                == ("serve: store ready\nbackfill events: 6\nevents: 8\nattention: 0\ncommands: 0\npending: 0\nfactory commands handled: 0\nwork-item commands handled: 0"),
            "assert_eq failed",
        );
    }

    #[test]
    fn store_backed_events_tail_reports_empty_store() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();

        let output = run_with_store_scripted(
            &command_args(&["bin", "events", "tail"]),
            &mut store,
            "unused",
        );

        check((output.code()) == (0), "assert_eq failed");
        check(
            (output.message()) == ("events tail: no events"),
            "assert_eq failed",
        );
    }

    #[test]
    fn store_backed_events_usage_keeps_error_code() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();

        let output =
            run_with_store_scripted(&command_args(&["bin", "events"]), &mut store, "unused");

        check((output.code()) == (2), "assert_eq failed");
        check(
            (output.message()) == ("usage: livespec-console-beads-fabro events tail"),
            "assert_eq failed",
        );
    }

    #[test]
    fn store_backed_runner_falls_back_to_static_commands() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();

        let output = run_with_store_scripted(&command_args(&["bin", "help"]), &mut store, "unused");

        check((output.code()) == (0), "assert_eq failed");
        check(output.message().contains("Commands:"), "assert failed");
    }

    #[test]
    fn store_result_reports_event_store_errors() {
        let output = super::run_store_result(Err(EventStoreError::InvalidSequence), "snapshot");

        check((output.code()) == (1), "assert_eq failed");
        check(
            (output.message()) == ("snapshot error: InvalidSequence"),
            "assert_eq failed",
        );
    }

    #[test]
    fn runtime_result_reports_console_runtime_errors() {
        let output = super::run_runtime_result(
            Err(ConsoleRuntimeError::Application(
                ApplicationError::FactoryDrainPortFailed,
            )),
            "serve",
        );

        check((output.code()) == (1), "assert_eq failed");
        check(
            (output.message()) == ("serve error: Application(FactoryDrainPortFailed)"),
            "assert_eq failed",
        );

        let output = super::run_runtime_result(
            Err(ConsoleRuntimeError::from(BackingCliResolutionError::new(
                "missing script".to_owned(),
            ))),
            "serve",
        );

        check((output.code()) == (1), "assert_eq failed");
        check(
            (output.message()) == ("serve error: BackingCliResolution(\"missing script\")"),
            "assert_eq failed",
        );
    }

    #[test]
    fn store_backed_snapshot_reports_projection_counts() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        append_demo_events_to_store(&mut store, "2026-06-23T00:00:00Z").ok_test();
        let persistence = persist_tui_runtime_effects(
            &mut store,
            &[factory_drain_effect()],
            "2026-06-23T00:00:01Z",
        );
        check(persistence.is_ok(), "assert failed");

        let output =
            run_with_store_scripted(&command_args(&["bin", "snapshot"]), &mut store, "unused");

        check((output.code()) == (0), "assert_eq failed");
        check(
            (output.message()) == ("snapshot: events 2, attention 0, commands 1, pending 1"),
            "assert_eq failed",
        );
    }

    #[test]
    fn store_backed_plan_page_renders_persisted_epic_children_and_handoffs() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        for event in [
            plan_snapshot_event(
                "evt_plan_epic",
                "plan-epic",
                "a0",
                "open",
                r#"{"title":"Migrated Plan","item_type":"epic","depends_on":[],"comments":[{"id":"c1","author":"operator","created_at":"2026-08-16T08:00:00Z","text":"handoff entry one"}]}"#,
            ),
            plan_snapshot_event(
                "evt_plan_child",
                "plan-child",
                "a1",
                "blocked",
                r#"{"title":"Child Work","depends_on":["plan-epic"]}"#,
            ),
        ] {
            store
                .append_event(&event_append_from_console_event(
                    &event,
                    "2026-08-16T08:30:00Z",
                ))
                .ok_test();
        }

        let output = run_with_store_scripted(
            &command_args(&["bin", "plans", "plan-epic"]),
            &mut store,
            "unused",
        );

        check((output.code()) == (0), "assert_eq failed");
        check(
            output.message().contains("url: /plans/plan-epic"),
            "assert failed",
        );
        check(output.message().contains("Migrated Plan"), "assert failed");
        check(output.message().contains("plan-child"), "assert failed");
        check(
            output.message().contains("status: blocked"),
            "assert failed",
        );
        check(
            output.message().contains("handoff entry one"),
            "assert failed",
        );
        check(
            (plan_page_report(&store, "plan-epic").ok_test()) == (output.message()),
            "assert_eq failed",
        );
    }

    #[test]
    fn store_backed_plan_page_usage_requires_an_epic_id() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();

        let output =
            run_with_store_scripted(&command_args(&["bin", "plans"]), &mut store, "unused");

        check((output.code()) == (2), "assert_eq failed");
        check(
            (output.message()) == ("usage: livespec-console-beads-fabro plans <epic-id>"),
            "assert_eq failed",
        );
    }

    #[test]
    fn store_backed_doctor_reports_no_findings_with_store_counts() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        append_demo_events_to_store(&mut store, "2026-06-23T00:00:00Z").ok_test();

        let output =
            run_with_store_scripted(&command_args(&["bin", "doctor"]), &mut store, "unused");

        check((output.code()) == (0), "assert_eq failed");
        check(
            (output.message())
                == ("doctor: no findings\nstore events: 2\ncommands: 0\nattention: 0"),
            "assert_eq failed",
        );
    }

    #[test]
    fn store_report_helpers_match_command_output() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let scripted = scripted_source_list();
        let sources = scripted_source_refs(&scripted);
        let na_port = empty_needs_attention_port();
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");

        let backfill = backfill_source_report(
            &mut store,
            "2026-06-23T00:00:00Z",
            &sources,
            &needs_attention,
        );
        check(
            (backfill.ok_test()) == ("backfill source adapters: adapters 5, events 6"),
            "assert_eq failed",
        );
        check(
            events_tail_report(&store, 1)
                .ok_test()
                .contains("pr.snapshot_observed"),
            "assert failed",
        );
        // Attention is now sourced from the `attention_item.*` stream; a store of
        // work-item snapshots alone carries no attention items until the
        // needs-attention snapshot is ingested (Scenario 12).
        check(
            (snapshot_report(&store).ok_test())
                == ("snapshot: events 6, attention 0, commands 0, pending 0"),
            "assert_eq failed",
        );
        check(
            (doctor_report(&store).ok_test())
                == ("doctor: no findings\nstore events: 6\ncommands: 0\nattention: 0"),
            "assert_eq failed",
        );
    }

    #[test]
    fn tui_preview_reports_render_errors() {
        let model = build_tui_model(&[], 0);

        check(
            (render_tui_preview(&model, 0, 28)) == ("TUI render error: empty area"),
            "assert_eq failed",
        );
    }

    #[test]
    fn tui_persistence_stores_command_effects() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let effects = [
            TuiRuntimeEffect::OpenAttachCommand("fabro attach run_1".to_owned()),
            TuiRuntimeEffect::PersistCommand(CommandEnvelope::new(
                "cmd_factory_drain_requested_budget_1_parallel_1".to_owned(),
                CommandType::FactoryDrainRequested,
                "fleet:livespec".to_owned(),
                "fleet:livespec:factory.drain_requested:budget=1:parallel=1".to_owned(),
                "operator".to_owned(),
            )),
            TuiRuntimeEffect::CopyAttachCommand("fabro attach run_1".to_owned()),
        ];

        let outcomes =
            persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z").ok_test();
        let commands = store.list_commands().ok_test();

        check((outcomes.len()) == (1), "assert_eq failed");
        check(
            (outcomes[0].status()) == (CommandAppendStatus::Inserted),
            "assert_eq failed",
        );
        // The drain is REPEATABLE, so it persists under a sequence-distinguished
        // identity (`_0` / `:0` at the first append into an empty command log)
        // rather than the static key it was authored with.
        check(
            (outcomes[0].command_id()) == ("cmd_factory_drain_requested_budget_1_parallel_1_0"),
            "assert_eq failed",
        );
        check((commands.len()) == (1), "assert_eq failed");
        check(
            (commands[0].command_id()) == ("cmd_factory_drain_requested_budget_1_parallel_1_0"),
            "assert_eq failed",
        );
        check(
            (commands[0].command_type()) == ("factory.drain_requested"),
            "assert_eq failed",
        );
        check(
            (commands[0].aggregate_id()) == (Some("fleet:livespec")),
            "assert_eq failed",
        );
        check(
            (commands[0].idempotency_key())
                == ("fleet:livespec:factory.drain_requested:budget=1:parallel=1:0"),
            "assert_eq failed",
        );
        check(
            (commands[0].requested_by()) == ("operator"),
            "assert_eq failed",
        );
        check((commands[0].status()) == ("pending"), "assert_eq failed");
    }

    #[test]
    fn tui_persistence_stamps_each_command_request_separately() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let first = [factory_drain_effect()];
        let second = [factory_drain_effect()];

        persist_tui_runtime_effects(&mut store, &first, "2026-06-23T00:00:02Z").ok_test();
        std::thread::sleep(std::time::Duration::from_millis(2));
        persist_tui_runtime_effects(&mut store, &second, "2026-06-23T00:00:02Z").ok_test();

        let commands = store.list_commands().ok_test();

        check((commands.len()) == (2), "assert_eq failed");
        check(
            (commands[0].requested_at()) != (commands[1].requested_at()),
            "assert_ne failed",
        );
        check(
            (commands[0].requested_at()) == (commands[0].updated_at()),
            "assert_eq failed",
        );
        check(
            (commands[1].requested_at()) == (commands[1].updated_at()),
            "assert_eq failed",
        );
    }

    #[test]
    fn tui_persistence_ignores_local_only_effects() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let effects = [
            TuiRuntimeEffect::Render,
            TuiRuntimeEffect::OpenAttachCommand("fabro attach run_1".to_owned()),
            TuiRuntimeEffect::CopyAttachCommand("fabro attach run_1".to_owned()),
            TuiRuntimeEffect::ApplicationError(ApplicationError::NoSelectedOperatorAction),
            TuiRuntimeEffect::Quit,
        ];

        let outcomes =
            persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z").ok_test();
        let commands = store.list_commands().ok_test();

        check(outcomes.is_empty(), "assert_eq failed");
        check(commands.is_empty(), "assert_eq failed");
    }

    #[test]
    fn tui_persistence_reports_command_append_errors() {
        let mut store = CommandAppendFailingStore;
        let effects = [TuiRuntimeEffect::PersistCommand(CommandEnvelope::new(
            "cmd_factory_drain_requested_budget_1_parallel_1".to_owned(),
            CommandType::FactoryDrainRequested,
            "fleet:livespec".to_owned(),
            "fleet:livespec:factory.drain_requested:budget=1:parallel=1".to_owned(),
            "operator".to_owned(),
        ))];

        let outcome = persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z");

        check(
            format!("{outcome:?}").contains("InvalidSequence"),
            "assert failed",
        );
    }

    #[test]
    fn store_backed_tui_session_backfills_runs_tui_and_handles_factory_command() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let mut runner = ScriptedTuiSessionRunner::new(vec![factory_drain_effect()]);
        let mut factory_port = SimulatedFactoryDrainPort;
        let mut work_item_port = SimulatedWorkItemActionPort::default();
        let scripted = scripted_source_list_with_ready_work();
        let sources = scripted_source_refs(&scripted);
        let na_port = empty_needs_attention_port();
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");

        let outcome = run_store_backed_tui_session(
            &mut store,
            "2026-06-23T00:00:02Z",
            "operator",
            &mut runner,
            &sources,
            &mut factory_port,
            &mut work_item_port,
            &empty_decisions_port(),
            &needs_attention,
            &poll_requester(),
            &command_requester(),
        );
        let commands = store.list_commands().ok_test();

        let outcome = outcome.ok_test();
        check(
            (outcome) == (TuiSessionOutcome::new(8, 8, 1, 1, 11, 0)),
            "assert_eq failed",
        );
        check(
            (outcome.backfilled_event_count()) == (8),
            "assert_eq failed",
        );
        check((outcome.presented_event_count()) == (8), "assert_eq failed");
        check(
            (outcome.persisted_command_count()) == (1),
            "assert_eq failed",
        );
        check((outcome.handled_command_count()) == (1), "assert_eq failed");
        check((outcome.final_event_count()) == (11), "assert_eq failed");
        check((outcome.attention_count()) == (0), "assert_eq failed");
        check((runner.observed_event_count()) == (8), "assert_eq failed");
        check(
            (runner.observed_requested_by()) == ("operator"),
            "assert_eq failed",
        );
        check((commands[0].status()) == ("completed"), "assert_eq failed");
    }

    #[test]
    fn store_backed_tui_session_persists_and_effects_a_payload_bearing_setting_write() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let mut runner = ScriptedTuiSessionRunner::new(vec![dispatcher_setting_set_effect()]);
        let mut factory_port = SimulatedFactoryDrainPort;
        let mut work_item_port = SimulatedWorkItemActionPort::default();
        let scripted = scripted_source_list();
        let sources = scripted_source_refs(&scripted);
        let na_port = empty_needs_attention_port();
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");

        let outcome = run_store_backed_tui_session(
            &mut store,
            "2026-07-11T00:00:02Z",
            "operator",
            &mut runner,
            &sources,
            &mut factory_port,
            &mut work_item_port,
            &empty_decisions_port(),
            &needs_attention,
            &poll_requester(),
            &command_requester(),
        );
        check(outcome.is_ok(), "assert failed");

        // The payload-bearing effect is persisted with its { repo, setting, value }
        // payload intact, so the Configuration handler parses it and completes the
        // command (an empty `{}` payload would be rejected).
        let commands = store.list_commands().ok_test();
        let setting = commands.iter().find(|command| {
            command.command_type() == CommandType::ConfigDispatcherSettingSet.contract_name()
        });
        check(
            (setting.map(StoredCommand::status)) == (Some("completed")),
            "assert_eq failed",
        );
        check(
            setting.is_some_and(|command| command.payload_json().contains(r#""value":true"#)),
            "assert failed",
        );

        // The setting write rode the shared orchestrator-action port, and the
        // change audit event is recorded through the same path.
        check(
            (work_item_port.observed_action_ids) == (["set-config:auto_approve_ready:true"]),
            "assert_eq failed",
        );
        let events = store.list_console_events().ok_test();
        check(
            events
                .iter()
                .any(|event| event.event_type() == &EventType::ConfigDispatcherSettingChanged),
            "assert failed",
        );
    }

    #[test]
    fn store_backed_tui_session_services_input_after_queued_drain_before_port_runs() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let calls = Rc::new(std::cell::Cell::new(0));
        let mut runner = DrainThenInputTuiSessionRunner::new(Rc::clone(&calls));
        let mut factory_port = CountingFactoryDrainPort::new(Rc::clone(&calls));
        let mut work_item_port = SimulatedWorkItemActionPort::default();
        let scripted = scripted_source_list_with_ready_work();
        let sources = scripted_source_refs(&scripted);
        let na_port = empty_needs_attention_port();
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");
        let commands = async_command_requester();

        let outcome = run_store_backed_tui_session(
            &mut store,
            "2026-08-17T23:50:00Z",
            "operator",
            &mut runner,
            &sources,
            &mut factory_port,
            &mut work_item_port,
            &empty_decisions_port(),
            &needs_attention,
            &poll_requester(),
            &commands,
        );
        check(outcome.is_ok(), "assert failed");

        check(
            (runner.port_calls_after_drain_effect) == (Some(0)),
            "assert_eq failed",
        );
        check(runner.serviced_input_after_drain_effect, "assert failed");
        check((commands.request_count()) == (1), "assert_eq failed");
        check((calls.get()) == (1), "assert_eq failed");
        check(
            (outcome
                .map(|outcome| outcome.handled_command_count())
                .unwrap_or_default())
                == (1),
            "assert_eq failed",
        );
    }

    #[test]
    fn store_backed_tui_session_applies_valve_effect_before_runner_returns() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let mut runner = ImmediateValveTuiSessionRunner;
        let mut factory_port = SimulatedFactoryDrainPort;
        let mut work_item_port = SimulatedWorkItemActionPort::default();
        let empty_sources: Vec<(String, ScriptedSource)> = Vec::new();
        let sources = scripted_source_refs(&empty_sources);
        let na_port = ScriptedNeedsAttentionPort::observing(vec![attention_item_fixture(
            "console-pending",
            "Set admission policy",
        )]);
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");
        append_work_item_lane(&mut store, "console-pending", "pending-approval", 1, TS0);

        let outcome = run_store_backed_tui_session(
            &mut store,
            "2026-07-13T00:00:00Z",
            "operator",
            &mut runner,
            &sources,
            &mut factory_port,
            &mut work_item_port,
            &empty_decisions_port(),
            &needs_attention,
            &poll_requester(),
            &command_requester(),
        );

        let commands = store.list_commands().ok_test();
        let command = commands.iter().find(|command| {
            command.command_type() == CommandType::WorkItemSetAdmissionRequested.contract_name()
        });
        check(
            (command.map(StoredCommand::status)) == (Some("completed")),
            "assert_eq failed",
        );
        check(
            (work_item_port.observed_action_ids) == (["set-admission:console-pending:auto"]),
            "assert_eq failed",
        );
        check(
            (outcome.ok_test().persisted_command_count()) == (1),
            "assert_eq failed",
        );
    }

    #[test]
    fn store_backed_tui_session_maps_live_effect_sink_errors() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let mut runner = ImmediateValveTuiSessionRunner;
        let mut factory_port = SimulatedFactoryDrainPort;
        let mut work_item_port = ErroringWorkItemActionPort;
        let empty_sources: Vec<(String, ScriptedSource)> = Vec::new();
        let sources = scripted_source_refs(&empty_sources);
        let na_port = ScriptedNeedsAttentionPort::observing(vec![attention_item_fixture(
            "console-pending",
            "Set admission policy",
        )]);
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");
        append_work_item_lane(&mut store, "console-pending", "pending-approval", 1, TS0);

        let outcome = run_store_backed_tui_session(
            &mut store,
            "2026-07-13T00:00:00Z",
            "operator",
            &mut runner,
            &sources,
            &mut factory_port,
            &mut work_item_port,
            &empty_decisions_port(),
            &needs_attention,
            &poll_requester(),
            &command_requester(),
        );

        check(
            format!("{outcome:?}").contains("TuiRuntimeFailed"),
            "assert failed",
        );
    }

    #[test]
    fn store_backed_tui_session_reingests_sources_over_existing_events() {
        // Bug A fix: the interactive launch re-ingests the source adapters on
        // EVERY run, not only when the log is empty, so the Lanes projection
        // reduces over the CURRENT ledger rather than a first-run snapshot. The
        // store starts with the 2 demo events; the scripted seed adds its 6
        // source events on top (idempotent per Scenario 3), so 8 events are
        // presented to the runner and left in the store.
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        append_demo_events_to_store(&mut store, "2026-06-23T00:00:00Z").ok_test();
        let mut runner = ScriptedTuiSessionRunner::new(vec![TuiRuntimeEffect::Quit]);
        let mut factory_port = SimulatedFactoryDrainPort;
        let mut work_item_port = SimulatedWorkItemActionPort::default();
        let scripted = scripted_source_list();
        let sources = scripted_source_refs(&scripted);
        let na_port = empty_needs_attention_port();
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");

        let outcome = run_store_backed_tui_session(
            &mut store,
            "2026-06-23T00:00:02Z",
            "operator",
            &mut runner,
            &sources,
            &mut factory_port,
            &mut work_item_port,
            &empty_decisions_port(),
            &needs_attention,
            &poll_requester(),
            &command_requester(),
        );

        check(
            (outcome.ok_test()) == (TuiSessionOutcome::new(6, 8, 0, 0, 8, 0)),
            "assert_eq failed",
        );
        check((runner.observed_event_count()) == (8), "assert_eq failed");
        check(
            (store.list_console_events().ok_test().len()) == (8),
            "assert_eq failed",
        );
    }

    /// A pull source returning a scripted SEQUENCE of polls — one per successive
    /// live `refresh_events` re-poll — so a test can observe an item move lanes
    /// across polls. Once the sequence is exhausted it repeats the final poll, so
    /// any extra re-poll stays stable.
    struct SequencedWorkItemSource {
        polls: Vec<AdapterPoll>,
        cursor: std::cell::Cell<usize>,
    }

    impl SequencedWorkItemSource {
        fn new(polls: Vec<AdapterPoll>) -> Self {
            Self {
                polls,
                cursor: std::cell::Cell::new(0),
            }
        }
    }

    impl PullSourcePort for SequencedWorkItemSource {
        fn poll(&self, _request: &AdapterPollRequest) -> Result<AdapterPoll, AdapterError> {
            let index = self.cursor.get().min(self.polls.len() - 1);
            self.cursor.set(index + 1);
            Ok(self.polls[index].clone())
        }
    }

    /// A [`SequencedWorkItemSource`] returning one work-item snapshot poll per
    /// `(id, lane, status, source_version)` spec — successive re-polls report the
    /// successive lanes, so a test can watch an item move across polls. Malformed
    /// specs are dropped (like [`scripted_source_list_with_ready_work`]), so a
    /// missing lane surfaces as an assertion failure rather than a panic.
    fn sequenced_work_item_source(specs: &[(&str, Lane, &str, u64)]) -> SequencedWorkItemSource {
        let polls = specs
            .iter()
            .filter_map(|&(work_item_id, lane, status, source_version)| {
                WorkItemSnapshot::new(
                    "livespec-console-beads-fabro",
                    work_item_id,
                    lane,
                    None,
                    "a0",
                    status,
                    AdmissionPolicy::Manual,
                    AcceptancePolicy::AiThenHuman,
                    source_version,
                )
                .ok()
                .map(|snapshot| normalize_work_item_snapshot(&snapshot))
            })
            .collect();
        SequencedWorkItemSource::new(polls)
    }

    /// The work-item ids the lane projection places in `lane`, for asserting a
    /// live refresh moved an item between lanes.
    fn lane_work_item_ids(events: &[ConsoleEvent], lane: Lane) -> Vec<String> {
        project_lane_board(events)
            .column(lane)
            .map(|column| {
                column
                    .items()
                    .iter()
                    .map(|item| item.work_item_id().to_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Extract the events a live refresh re-listed. The store-backed session
    /// always returns `Ok(Some(..))`, so the never-taken no-events / IO-error
    /// shapes map to a runtime error a test can `?`.
    fn refreshed_events(
        outcome: std::io::Result<Option<Vec<ConsoleEvent>>>,
    ) -> ConsoleRuntimeResult<Vec<ConsoleEvent>> {
        outcome
            .ok()
            .flatten()
            .ok_or(ConsoleRuntimeError::TuiRuntimeFailed)
    }

    #[test]
    fn refresh_sources_reprojects_a_lane_change_across_polls() {
        // Scenario 3 + Bug B: the off-thread poller runs `refresh_sources` on its
        // cadence. An item observed in one lane, then re-observed in another on a
        // subsequent poll (a higher `source_version`), reprojects to the NEW lane —
        // the poller keeping the board live without a restart.
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let source = sequenced_work_item_source(&[
            ("wi-live", Lane::Ready, "ready", 1),
            ("wi-live", Lane::Backlog, "backlog", 2),
        ]);
        let sources: Vec<SourceAdapterRef<'_>> =
            vec![("orchestrator:livespec-console-beads-fabro", &source)];
        let na_port = empty_needs_attention_port();
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");
        let (at1, at2) = ("2026-07-17T00:00:00Z", "2026-07-17T00:00:01Z");

        // First poll → item in the Ready lane.
        refresh_sources(&mut store, at1, &sources, &needs_attention).ok_test();
        let first = store.list_console_events().ok_test();
        check(
            (lane_work_item_ids(&first, Lane::Ready)) == (["wi-live"]),
            "assert_eq failed",
        );
        check(
            lane_work_item_ids(&first, Lane::Backlog).is_empty(),
            "assert failed",
        );

        // A subsequent poll (poll 2, higher version) → the SAME item now projects
        // to the Backlog lane, with no restart.
        refresh_sources(&mut store, at2, &sources, &needs_attention).ok_test();
        let second = store.list_console_events().ok_test();
        check(
            (lane_work_item_ids(&second, Lane::Backlog)) == (["wi-live"]),
            "assert_eq failed",
        );
        check(
            lane_work_item_ids(&second, Lane::Ready).is_empty(),
            "assert failed",
        );
    }

    #[test]
    fn needs_attention_impl_row_refreshes_stale_lane_and_drain_policy() {
        // Regression for aw6z: a non-orchestrator ledger update made the
        // needs-attention source see ready implementation work while the lane
        // stream still held an older Active snapshot. Ingesting the fresher
        // `impl:` attention row must repair the lane projection before the drain
        // policy gates a valid dispatch.
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        append_work_item_lane(
            &mut store,
            "livespec-console-beads-fabro-0c5",
            "active",
            1,
            "2026-08-17T12:18:00Z",
        );
        let attention_item = AttentionItemSnapshot::new(
            "impl:livespec-console-beads-fabro-0c5",
            "implementation",
            "high",
            "Ready implementation work",
            AttentionSourceRef::new(
                "livespec-console-beads-fabro",
                Some("livespec-console-beads-fabro-0c5"),
                None,
            ),
            AttentionHandoff::new(
                "implement",
                None,
                "implement:livespec-console-beads-fabro-0c5",
            ),
        );
        let na_port = ScriptedNeedsAttentionPort::observing(vec![attention_item]);
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");

        let before = store.list_console_events().ok_test();
        check(
            (lane_work_item_ids(&before, Lane::Active)) == (["livespec-console-beads-fabro-0c5"]),
            "assert_eq failed",
        );
        check(
            lane_work_item_ids(&before, Lane::Ready).is_empty(),
            "assert failed",
        );
        check(
            (super::FactoryDrainPolicy::from_events(&before).rejection_reason())
                == (Some("no ready implementation work")),
            "assert_eq failed",
        );

        let inserted =
            ingest_needs_attention(&mut store, &needs_attention, "2026-08-17T21:28:00Z").ok_test();
        check((inserted) == (2), "assert_eq failed");
        let after = store.list_console_events().ok_test();
        check(
            (lane_work_item_ids(&after, Lane::Ready)) == (["livespec-console-beads-fabro-0c5"]),
            "assert_eq failed",
        );
        check(
            lane_work_item_ids(&after, Lane::Active).is_empty(),
            "assert failed",
        );
        let policy = super::FactoryDrainPolicy::from_events(&after);
        check(policy.rejection_reason().is_none(), "assert_eq failed");
        let mut port = RecordingFactoryDrainPort::default();
        let command = CommandEnvelope::new(
            "cmd_drain".to_owned(),
            CommandType::FactoryDrainRequested,
            "fleet:livespec".to_owned(),
            "fleet:livespec:factory.drain_requested:budget=1:parallel=1".to_owned(),
            "operator".to_owned(),
        );
        let outcome = super::handle_factory_drain_command(&command, &policy, &mut port).ok_test();
        check(
            (outcome.command_status()) == ("completed"),
            "assert_eq failed",
        );
        check(
            (port.observed_aggregate_ids) == (["fleet:livespec"]),
            "assert_eq failed",
        );
    }

    #[test]
    fn needs_attention_ingest_counts_only_inserted_diff_and_impl_appends() {
        let mut sqlite = SqliteEventStore::open_in_memory().ok_test();
        let attention_item = AttentionItemSnapshot::new(
            "impl:livespec-console-beads-fabro-0c5",
            "implementation",
            "high",
            "Ready implementation work",
            AttentionSourceRef::new(
                "livespec-console-beads-fabro",
                Some("livespec-console-beads-fabro-0c5"),
                None,
            ),
            AttentionHandoff::new(
                "implement",
                None,
                "implement:livespec-console-beads-fabro-0c5",
            ),
        );
        let seeded_events = diff_needs_attention(
            "livespec-console-beads-fabro",
            &[],
            std::slice::from_ref(&attention_item),
        );
        for event in &seeded_events {
            let append = duplicate_collision_append(event, "2026-08-17T21:29:00Z");
            sqlite.append_event(&append).ok_test();
        }
        let na_port = ScriptedNeedsAttentionPort::observing(vec![attention_item]);
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");

        let first =
            ingest_needs_attention(&mut sqlite, &needs_attention, "2026-08-17T21:29:00Z").ok_test();
        let second =
            ingest_needs_attention(&mut sqlite, &needs_attention, "2026-08-17T21:29:00Z").ok_test();

        check((first) == (1), "assert_eq failed");
        check((second) == (0), "assert_eq failed");
    }

    #[test]
    fn store_backed_refresh_reflects_the_operators_action_and_signals_a_poll() {
        // Bug B: after the operator drains, a CHEAP `refresh_events` re-list (no
        // source poll on the UI thread) already reflects the operator's OWN
        // just-appended drain outcome. `refresh_events(true)` also pings the
        // off-thread poller to re-poll sources at once (so the ledger's lane
        // change appears promptly); `refresh_events(false)` never pings.
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        // Seed a ready work-item so the drain is accepted (not policy-rejected).
        let source = sequenced_work_item_source(&[("wi-ready", Lane::Ready, "ready", 1)]);
        let sources: Vec<SourceAdapterRef<'_>> =
            vec![("orchestrator:livespec-console-beads-fabro", &source)];
        let na_port = empty_needs_attention_port();
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");
        let seed_at = "2026-07-17T00:00:00Z";
        refresh_sources(&mut store, seed_at, &sources, &needs_attention).ok_test();

        let mut factory_port = SimulatedFactoryDrainPort;
        let mut work_item_port = SimulatedWorkItemActionPort::default();
        let decisions = empty_decisions_port();
        let requester = poll_requester();
        let commands = command_requester();
        {
            let mut sink = StoreBackedTuiRuntimeEffectSink::new(
                &mut store,
                "2026-07-17T00:00:01Z",
                &mut factory_port,
                &mut work_item_port,
                &decisions,
                &requester,
                &commands,
            );

            // A cheap re-list (request_poll = false) carries no drain outcome yet
            // and does NOT ping the poller.
            let before = refreshed_events(sink.refresh_events(false)).ok_test();
            check(
                !before
                    .iter()
                    .any(|event| event.event_type() == &EventType::FactoryDrainCompleted),
                "assert failed",
            );
            check((requester.poll_count()) == (0), "assert_eq failed");

            // The operator drains: the effect persists the command and appends its
            // outcome events through the injected drain port.
            let applied = sink
                .handle_runtime_effect(&factory_drain_effect())
                .ok()
                .ok_or(ConsoleRuntimeError::TuiRuntimeFailed)
                .ok_test();
            check(
                (applied) == (TuiRuntimeEffectSinkOutcome::Applied),
                "assert_eq failed",
            );

            // A ledger-mutating refresh (request_poll = true) re-lists the
            // operator's own outcome AND pings the poller exactly once.
            let after = refreshed_events(sink.refresh_events(true)).ok_test();
            check(
                after
                    .iter()
                    .any(|event| event.event_type() == &EventType::FactoryDrainCompleted),
                "assert failed",
            );
            check((requester.poll_count()) == (1), "assert_eq failed");
        }
    }

    #[test]
    fn store_backed_effect_sink_queues_factory_drain_without_running_port_inline() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let mut factory_port = RecordingFactoryDrainPort::default();
        let mut work_item_port = SimulatedWorkItemActionPort::default();
        let decisions = empty_decisions_port();
        let poller = poll_requester();
        let commands = async_command_requester();
        {
            let mut sink = StoreBackedTuiRuntimeEffectSink::new(
                &mut store,
                "2026-08-17T23:45:00Z",
                &mut factory_port,
                &mut work_item_port,
                &decisions,
                &poller,
                &commands,
            );
            let render_outcome = sink
                .handle_runtime_effect(&TuiRuntimeEffect::Render)
                .map_err(|_error| ConsoleRuntimeError::TuiRuntimeFailed)
                .ok_test();
            check(
                (render_outcome) == (TuiRuntimeEffectSinkOutcome::Applied),
                "assert_eq failed",
            );
            check((commands.request_count()) == (0), "assert_eq failed");

            let outcome = sink
                .handle_runtime_effect(&factory_drain_effect())
                .map_err(|_error| ConsoleRuntimeError::TuiRuntimeFailed)
                .ok_test();

            check(
                (outcome) == (TuiRuntimeEffectSinkOutcome::Applied),
                "assert_eq failed",
            );
            check((sink.persisted_command_count()) == (1), "assert_eq failed");
            check((sink.handled_command_count()) == (0), "assert_eq failed");
        }

        let commands_in_store = store.list_commands().ok_test();
        check((commands_in_store.len()) == (1), "assert_eq failed");
        check(
            (commands_in_store[0].status()) == ("pending"),
            "assert_eq failed",
        );
        check((commands.request_count()) == (1), "assert_eq failed");
        check(
            (factory_port.observed_aggregate_ids) == (Vec::<String>::new()),
            "assert_eq failed",
        );
    }

    #[test]
    fn factory_drain_requested_event_helper_ignores_non_inserted_missing_and_non_factory_commands()
    {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let duplicate = [CommandAppendOutcome::new(
            "cmd_missing".to_owned(),
            CommandAppendStatus::Duplicate,
        )];
        check(
            (append_factory_drain_requested_events(&mut store, &duplicate, "2026-08-17T23:55:00Z")
                .ok_test())
                == (0),
            "assert_eq failed",
        );
        let persisted = persist_tui_runtime_effects(
            &mut store,
            &[dispatcher_setting_set_effect()],
            "2026-08-17T23:55:02Z",
        );
        check(persisted.is_ok(), "assert failed");
        let persisted = persisted.unwrap_or_default();
        let missing = [CommandAppendOutcome::new(
            "cmd_missing".to_owned(),
            CommandAppendStatus::Inserted,
        )];
        check(
            (append_factory_drain_requested_events(&mut store, &missing, "2026-08-17T23:55:01Z")
                .ok_test())
                == (0),
            "assert_eq failed",
        );
        check(
            (append_factory_drain_requested_events(&mut store, &persisted, "2026-08-17T23:55:03Z")
                .ok_test())
                == (0),
            "assert_eq failed",
        );
        check(
            store.list_console_events().ok_test().is_empty(),
            "assert failed",
        );
    }

    #[test]
    fn factory_drain_requested_event_helper_counts_only_inserted_appends() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let persisted = persist_tui_runtime_effects(
            &mut store,
            &[factory_drain_effect()],
            "2026-08-17T23:56:00Z",
        )
        .ok_test();

        let first =
            append_factory_drain_requested_events(&mut store, &persisted, "2026-08-17T23:56:01Z")
                .ok_test();
        let second =
            append_factory_drain_requested_events(&mut store, &persisted, "2026-08-17T23:56:02Z")
                .ok_test();

        check((first) == (1), "assert_eq failed");
        check((second) == (0), "assert_eq failed");
    }

    #[test]
    fn pending_command_requester_default_is_non_inline() {
        struct DefaultRequester;

        impl PendingCommandRequester for DefaultRequester {
            fn request_pending_command_handling(&self) {
                let _ = self;
            }
        }

        let requester = DefaultRequester;
        requester.request_pending_command_handling();
        check(
            !requester.handles_pending_commands_inline(),
            "assert failed",
        );
    }

    #[test]
    fn store_backed_refresh_reflects_autonomous_decisions_on_every_refresh() {
        // Scenario 15 + Bug B (folds PR #256): the CHEAP local-journal reflection
        // runs on EVERY `refresh_events` — even a re-list that does not ping the
        // poller (`request_poll = false`) — so an auto-disposition that lands
        // mid-session leaves the needs-attention inbox live at once.
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        // The item is already surfaced in the needs-attention inbox.
        let na_port = ScriptedNeedsAttentionPort::observing(vec![attention_item_fixture(
            "valve:approve:wi-1",
            "Approve wi-1",
        )]);
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");
        ingest_needs_attention(&mut store, &needs_attention, "2026-07-17T00:00:00Z").ok_test();
        check(
            (project_attention(&store.list_console_events().ok_test()).len()) == (1),
            "assert_eq failed",
        );

        // The plane's engine has now auto-approved wi-1.
        let decisions = SimulatedDecisionsPort::returning(AutonomousAudit::new(
            vec![
                AutonomousDecision::from_auto_disposition(
                    "wi-1",
                    "auto-approve",
                    vec!["auto_approve_ready".to_owned()],
                )
                .ok_or(ConsoleRuntimeError::TuiRuntimeFailed)
                .ok_test(),
            ],
            Vec::new(),
        ));
        let mut factory_port = SimulatedFactoryDrainPort;
        let mut work_item_port = SimulatedWorkItemActionPort::default();
        let requester = poll_requester();
        let commands = command_requester();

        let events = {
            let mut sink = StoreBackedTuiRuntimeEffectSink::new(
                &mut store,
                "2026-07-17T00:00:01Z",
                &mut factory_port,
                &mut work_item_port,
                &decisions,
                &requester,
                &commands,
            );
            // A CHEAP refresh (request_poll = false) still runs the reflection.
            refreshed_events(sink.refresh_events(false)).ok_test()
        };

        // The auto-resolved item left the inbox on the cheap refresh.
        check(project_attention(&events).is_empty(), "assert failed");
    }

    #[test]
    fn distinguish_repeatable_command_folds_the_sequence_only_for_repeatable_actions() {
        // The pure-function contract, tested directly rather than through the
        // effect pipeline. Worth doing explicitly: EVERY payload-bearing command
        // the pipeline can produce is now repeatable, so the once-only guard is
        // unreachable from that direction — but it is still the property that
        // stops a future payload-bearing once-only command from being silently
        // sequence-distinguished into firing twice.
        let once_only = CommandEnvelope::new(
            "cmd_work_item_approve_requested_wi-1".to_owned(),
            CommandType::WorkItemApproveRequested,
            "wi-1".to_owned(),
            "wi-1:work_item.approve_requested".to_owned(),
            "operator".to_owned(),
        );
        let unchanged = distinguish_repeatable_command(&once_only, 7);
        check(
            (unchanged.command_id()) == (once_only.command_id()),
            "assert_eq failed",
        );
        check(
            (unchanged.idempotency_key()) == (once_only.idempotency_key()),
            "assert_eq failed",
        );

        let repeatable = CommandEnvelope::new(
            "cmd_work_item_set_admission_requested_wi-1_auto".to_owned(),
            CommandType::WorkItemSetAdmissionRequested,
            "wi-1".to_owned(),
            "wi-1:work_item.set_admission_requested:policy=auto".to_owned(),
            "operator".to_owned(),
        );
        let distinguished = distinguish_repeatable_command(&repeatable, 7);
        check(
            (distinguished.command_id()) == ("cmd_work_item_set_admission_requested_wi-1_auto_7"),
            "assert_eq failed",
        );
        check(
            (distinguished.idempotency_key())
                == ("wi-1:work_item.set_admission_requested:policy=auto:7"),
            "assert_eq failed",
        );
        // Same command, same sequence — an exact re-persist still dedupes, so
        // replay safety survives the widening.
        check(
            (distinguish_repeatable_command(&repeatable, 7).idempotency_key())
                == (distinguished.idempotency_key()),
            "assert_eq failed",
        );
    }

    /// Build the effect a staged valve produces for the operator-selected
    /// work-item, by driving the pure runtime's Confirm — the same key → valve →
    /// Confirm → effect path the interactive loop drives.
    fn valve_effect(events: &[ConsoleEvent], valve: PendingValve) -> TuiRuntimeEffect {
        let state = TuiInteractionState::new(0, TuiOverlay::ValveConfirm { valve });
        console_tui::step_tui_runtime(&state, events, TuiTerminalInput::Confirm, "operator")
            .effect()
            .clone()
    }

    /// Build the `work_item.move_requested` effect the `s`-move valve produces
    /// for the item selected in a DRILLED-IN lane — the surface where the
    /// move-status verb acts (it is inert on the Attention surface).
    fn move_effect(events: &[ConsoleEvent], from: Lane, to: Lane) -> TuiRuntimeEffect {
        let state = TuiInteractionState::for_view(
            TuiView::Lanes,
            0,
            TuiOverlay::ValveConfirm {
                valve: PendingValve::MoveStatus { from, to },
            },
        )
        .with_lane_focus(LaneFocus::Lane(from))
        .with_selected_lane_item_index(0);
        console_tui::step_tui_runtime(&state, events, TuiTerminalInput::Confirm, "operator")
            .effect()
            .clone()
    }

    /// Drive `effects` through a store-backed sink in order, returning the
    /// action-ids the orchestrator port observed and the number of commands of
    /// `command_type` that actually landed in the store.
    ///
    /// The two together are what distinguishes a repeatable action that LANDS
    /// from one that silently dedupes: a deduped command neither appends a row
    /// nor reaches the port.
    fn drive_effects(
        store: &mut SqliteEventStore,
        effects: &[TuiRuntimeEffect],
        kind: CommandType,
    ) -> (Vec<String>, usize) {
        let mut factory_port = SimulatedFactoryDrainPort;
        let mut work_item_port = SimulatedWorkItemActionPort::default();
        let decisions = empty_decisions_port();
        let requester = poll_requester();
        let commands = command_requester();
        {
            let mut sink = StoreBackedTuiRuntimeEffectSink::new(
                store,
                "2026-07-19T00:00:01Z",
                &mut factory_port,
                &mut work_item_port,
                &decisions,
                &requester,
                &commands,
            );
            for effect in effects {
                let outcome = sink.handle_runtime_effect(effect).ok();
                let applied = outcome
                    .ok_or(ConsoleRuntimeError::TuiRuntimeFailed)
                    .ok_test();
                check(
                    (applied) == (TuiRuntimeEffectSinkOutcome::Applied),
                    "assert_eq failed",
                );
            }
        }
        let commands = store.list_commands().ok_test();
        let landed = commands
            .iter()
            .filter(|command| command.command_type() == kind.contract_name())
            .count();
        (work_item_port.observed_action_ids, landed)
    }

    /// A store seeded with one selectable work-item `wi-1` in the inbox.
    fn store_with_selectable_item() -> (SqliteEventStore, Vec<ConsoleEvent>) {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let na_port = ScriptedNeedsAttentionPort::observing(vec![attention_item_fixture(
            "wi-1",
            "Repeatable actions on wi-1",
        )]);
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");
        ingest_needs_attention(&mut store, &needs_attention, "2026-07-19T00:00:00Z").ok_test();
        // The row resolves to a manual-admission pending-approval board record,
        // the lane that admits every per-item valve these tests stage.
        append_work_item_lane(
            &mut store,
            "wi-1",
            "pending-approval",
            1,
            "2026-07-19T00:00:00Z",
        );
        let events = store.list_console_events().ok_test();
        (store, events)
    }

    #[test]
    fn store_backed_repeated_admission_edits_all_land_and_drive_set_admission() {
        // The same latent static-key bug PR #258 fixed for MOVE, on the admission
        // policy dial. Its key carries the VALUE
        // (`<id>:work_item.set_admission_requested:policy=<p>`) but no per-action
        // distinguisher, so auto -> manual -> auto dedupes the THIRD edit onto the
        // first and the operator's dial silently stops responding.
        let (mut store, events) = store_with_selectable_item();
        let effects = [
            valve_effect(&events, PendingValve::SetAdmission(AdmissionPolicy::Auto)),
            valve_effect(&events, PendingValve::SetAdmission(AdmissionPolicy::Manual)),
            valve_effect(&events, PendingValve::SetAdmission(AdmissionPolicy::Auto)),
        ];
        let kind = CommandType::WorkItemSetAdmissionRequested;
        let (actions, landed) = drive_effects(&mut store, &effects, kind);
        check(
            (actions)
                == ([
                    "set-admission:wi-1:auto",
                    "set-admission:wi-1:manual",
                    "set-admission:wi-1:auto",
                ]),
            "assert_eq failed",
        );
        check((landed) == (3), "assert_eq failed");
    }

    #[test]
    fn store_backed_repeated_acceptance_edits_all_land_and_drive_set_acceptance() {
        // Same shape on the acceptance policy dial: ai-only -> human-only ->
        // ai-only must land three distinct edits.
        let (mut store, events) = store_with_selectable_item();
        let effects = [
            valve_effect(
                &events,
                PendingValve::SetAcceptance(AcceptancePolicy::AiOnly),
            ),
            valve_effect(
                &events,
                PendingValve::SetAcceptance(AcceptancePolicy::HumanOnly),
            ),
            valve_effect(
                &events,
                PendingValve::SetAcceptance(AcceptancePolicy::AiOnly),
            ),
        ];
        let kind = CommandType::WorkItemSetAcceptanceRequested;
        let (actions, landed) = drive_effects(&mut store, &effects, kind);
        check(
            (actions)
                == ([
                    "set-acceptance:wi-1:ai-only",
                    "set-acceptance:wi-1:human-only",
                    "set-acceptance:wi-1:ai-only",
                ]),
            "assert_eq failed",
        );
        check((landed) == (3), "assert_eq failed");
    }

    #[test]
    fn store_backed_set_clear_set_override_all_land() {
        // The per-item cap override is the clearest repeat case: set a cap, CLEAR
        // it back to inherit-global, then set the SAME cap again. The third edit
        // carries the same `{setting}={value}` as the first, so a value-only key
        // deduped it and the override stuck cleared.
        let (mut store, events) = store_with_selectable_item();
        let effects = [
            valve_effect(
                &events,
                PendingValve::SetOverride(DispatcherOverride::ReviewFixCap(OverrideInt::Value(2))),
            ),
            valve_effect(
                &events,
                PendingValve::SetOverride(DispatcherOverride::ReviewFixCap(OverrideInt::Clear)),
            ),
            valve_effect(
                &events,
                PendingValve::SetOverride(DispatcherOverride::ReviewFixCap(OverrideInt::Value(2))),
            ),
        ];
        let kind = CommandType::WorkItemSetDispatcherOverrideRequested;
        let (actions, landed) = drive_effects(&mut store, &effects, kind);
        // All three land, and the third re-issues the first's action-id — the
        // repeat a value-only key would have swallowed.
        check((actions.len()) == (3), "assert_eq failed");
        check((actions[0]) == (actions[2]), "assert_eq failed");
        check((landed) == (3), "assert_eq failed");
    }

    #[test]
    fn store_backed_repeated_rejects_all_land_and_drive_reject() {
        // Reject is repeatable across modes: rework -> regroom -> rework.
        let (mut store, events) = store_with_selectable_item();
        let effects = [
            valve_effect(&events, PendingValve::Reject(RejectMode::Rework)),
            valve_effect(&events, PendingValve::Reject(RejectMode::Regroom)),
            valve_effect(&events, PendingValve::Reject(RejectMode::Rework)),
        ];
        let kind = CommandType::WorkItemRejectRequested;
        let (actions, landed) = drive_effects(&mut store, &effects, kind);
        check((actions.len()) == (3), "assert_eq failed");
        check((actions[0]) == (actions[2]), "assert_eq failed");
        check((landed) == (3), "assert_eq failed");
    }

    #[test]
    fn store_backed_repeated_resolve_blocked_all_land() {
        // An item can be blocked, resolved to ready, blocked AGAIN, and resolved to
        // ready again. The second resolve-to-ready repeats the first key.
        let (mut store, _seeded) = store_with_selectable_item();
        // The board holds the item BLOCKED throughout: resolving it does not
        // move the board row until the next source poll, so every staging sees
        // the blocked record the registry availability check requires.
        append_work_item_lane(&mut store, "wi-1", "blocked", 2, "2026-07-19T00:00:00Z");
        let events = store.list_console_events().ok_test();
        let effects = [
            move_effect(&events, Lane::Blocked, Lane::Ready),
            move_effect(&events, Lane::Blocked, Lane::Backlog),
            move_effect(&events, Lane::Blocked, Lane::Ready),
        ];
        let kind = CommandType::WorkItemResolveBlockedRequested;
        let (actions, landed) = drive_effects(&mut store, &effects, kind);
        check(
            (actions)
                == ([
                    "resolve-blocked:wi-1:ready",
                    "resolve-blocked:wi-1:backlog",
                    "resolve-blocked:wi-1:ready",
                ]),
            "assert_eq failed",
        );
        check((landed) == (3), "assert_eq failed");
    }

    #[test]
    fn the_workflow_scope_override_rides_the_spine_to_its_action_id() {
        // The hotkey-less valve's full round trip: staged from a drilled-in
        // ready item the orchestrator reports as awaiting a scope override,
        // persisted, rebuilt from the store, and dispatched as
        // set-workflow-scope-override:<id>:citation-only.
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let payload = concat!(
            r#"{"repo":"livespec-console-beads-fabro","work_item_id":"wi-refused","#,
            r#""lane":"ready","lane_reason":null,"rank":"a0","status":"ready","#,
            r#""source_version":1,"detail":{"title":"Refused ready fixture","#,
            // Awaiting a scope override is the PUBLISHED signal, and it is
            // independent of `factory_safety` — the dispatcher refuses a
            // marked item on an earlier arm than the one this override
            // clears, so the two must not be conflated.
            r#""factory_safety":null,"awaits_scope_override":true}}"#,
        );
        let event = ConsoleEvent::new(
            "evt_wi_refused".to_owned(),
            1,
            "factory".to_owned(),
            EventType::WorkItemSnapshotObserved,
            "orchestrator".to_owned(),
            "repo:livespec-console-beads-fabro:wi-refused".to_owned(),
            1,
        )
        .with_payload_json(payload.to_owned());
        store
            .append_event(&EventAppend::new(
                event,
                "repo:livespec-console-beads-fabro:wi-refused".to_owned(),
                "2026-08-02T00:00:00Z".to_owned(),
                "2026-08-02T00:00:00Z".to_owned(),
                None,
                "corr_evt_wi_refused".to_owned(),
                Some("evt_wi_refused".to_owned()),
                payload.to_owned(),
                "{}".to_owned(),
            ))
            .ok_test();
        let events = store.list_console_events().ok_test();
        let state = TuiInteractionState::for_view(
            TuiView::Lanes,
            0,
            TuiOverlay::ValveConfirm {
                valve: PendingValve::SetWorkflowScopeOverride,
            },
        )
        .with_lane_focus(LaneFocus::Lane(Lane::Ready))
        .with_selected_lane_item_index(0);
        let effect =
            console_tui::step_tui_runtime(&state, &events, TuiTerminalInput::Confirm, "operator")
                .effect()
                .clone();
        let kind = CommandType::WorkItemSetWorkflowScopeOverrideRequested;
        let (actions, landed) = drive_effects(&mut store, &[effect], kind);
        check(
            (actions) == (["set-workflow-scope-override:wi-refused:citation-only"]),
            "assert_eq failed",
        );
        check((landed) == (1), "assert_eq failed");
    }

    #[test]
    fn the_erroring_port_inherits_the_honest_not_wired_read_default() {
        // The trait's default read is honest not-wired for a port carrying no
        // real read capability — asserted on THIS crate's test port so the
        // inherited default is exercised where it is instantiated.
        let mut port = ErroringWorkItemActionPort;
        let reading = port.read_action(&OrchestratorActionRequest::new("config".to_owned()));
        check(
            (reading) == (Ok(console_application::OrchestratorActionReading::not_wired())),
            "assert_eq failed",
        );
    }

    #[test]
    fn a_refused_valve_persists_its_refusal_into_the_failure_event() {
        // The action-invocation half of the silent-valve defect: the refusal
        // payload the drive surface emits rides the work_item.action.failed
        // event into the store, where the record modal renders it — instead of
        // being discarded at the port boundary.
        let (mut store, events) = store_with_selectable_item();
        let approve = valve_effect(&events, PendingValve::Approve);
        let refusal = concat!(
            r#"{"action_id":"approve:wi-1","domain_error":"invalid-source-state","#,
            r#""status":"failed","summary":"approve requires an effective-manual "#,
            r#"pending-approval item."}"#,
        );
        let mut port = SimulatedWorkItemActionPort::returning(
            OrchestratorActionOutcome::failed_with_refusal(refusal.to_owned()),
        );
        let persisted =
            persist_tui_runtime_effects(&mut store, &[approve], "2026-08-02T00:00:01Z").ok_test();
        check((persisted.len()) == (1), "assert_eq failed");
        let outcomes =
            handle_pending_work_item_commands(&mut store, "2026-08-02T00:00:02Z", &mut port)
                .ok_test();
        check(
            (outcomes[0].command_status()) == ("failed"),
            "assert_eq failed",
        );
        let events = store.list_console_events().ok_test();
        let commands = store.list_commands().ok_test();
        let payload = events
            .iter()
            .find(|event| *event.event_type() == EventType::WorkItemActionFailed)
            .map(ConsoleEvent::payload_json)
            .unwrap_or_default();
        let expected = serde_json::json!({
            "action_id": "approve:wi-1",
            "domain_error": "invalid-source-state",
            "status": "failed",
            "summary": "approve requires an effective-manual pending-approval item."
        })
        .to_string();
        check((payload) == (expected), "assert_eq failed");
        check(
            (commands[0].error_json()) == (Some(payload)),
            "assert_eq failed",
        );
    }

    #[test]
    fn store_backed_once_only_valves_still_dedupe() {
        // The other half of the audit, and the regression this fix must NOT cause.
        // Approve and accept are SEMANTICALLY once-per-item: approving twice is a
        // no-op, and their static per-item key is CORRECT while the original row
        // is not terminal-failed. Widening the retry path must not sweep healthy
        // valves in -- if it did, a double keypress would fire the valve twice.
        let (mut store, events) = store_with_selectable_item();
        let approve = valve_effect(&events, PendingValve::Approve);
        let once = [approve];
        let kind = CommandType::WorkItemApproveRequested;
        let (actions, landed) = drive_effects(&mut store, &once, kind);
        check((actions) == (["approve:wi-1"]), "assert_eq failed");
        check((landed) == (1), "assert_eq failed");

        // The SAME approve again dedupes: no second command, no second action.
        let kind = CommandType::WorkItemApproveRequested;
        let (repeat_actions, still_landed) = drive_effects(&mut store, &once, kind);
        check(repeat_actions.is_empty(), "assert failed");
        check((still_landed) == (1), "assert_eq failed");
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn store_backed_failed_approve_can_be_retried_without_losing_replay_safety() {
        const FIRST_REQUESTED_AT: &str = "2026-07-19T00:00:01Z";
        const FIRST_HANDLED_AT: &str = "2026-07-19T00:00:02Z";
        const RETRY_REQUESTED_AT: &str = "2026-07-19T00:00:03Z";
        const RETRY_HANDLED_AT: &str = "2026-07-19T00:00:04Z";
        const AFTER_SUCCESS_REQUESTED_AT: &str = "2026-07-19T00:00:05Z";

        let (mut store, events) = store_with_selectable_item();
        let approve = valve_effect(&events, PendingValve::Approve);
        let once = [approve];

        let first = persist_tui_runtime_effects(&mut store, &once, FIRST_REQUESTED_AT).ok_test();
        check((first.len()) == (1), "assert_eq failed");
        check(
            (first[0].status()) == (CommandAppendStatus::Inserted),
            "assert_eq failed",
        );

        let mut failing_port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::failed());
        let first_outcomes =
            handle_pending_work_item_commands(&mut store, FIRST_HANDLED_AT, &mut failing_port)
                .ok_test();
        check((first_outcomes.len()) == (1), "assert_eq failed");
        check(
            (first_outcomes[0].command_status()) == ("failed"),
            "assert_eq failed",
        );
        check(
            (failing_port.observed_action_ids) == (["approve:wi-1"]),
            "assert_eq failed",
        );
        let commands_after_failure = store.list_commands().ok_test();
        check((commands_after_failure.len()) == (1), "assert_eq failed");
        check(
            (commands_after_failure[0].status()) == ("failed"),
            "assert_eq failed",
        );
        check(
            (commands_after_failure[0].idempotency_key()) == ("wi-1:work_item.approve_requested"),
            "assert_eq failed",
        );

        let retry = persist_tui_runtime_effects(&mut store, &once, RETRY_REQUESTED_AT).ok_test();
        check((retry.len()) == (1), "assert_eq failed");
        check(
            (retry[0].status()) == (CommandAppendStatus::Inserted),
            "assert_eq failed",
        );
        check(
            (retry[0].command_id()) == ("cmd_work_item_approve_requested_wi-1_1"),
            "assert_eq failed",
        );
        let commands_after_retry = store.list_commands().ok_test();
        check((commands_after_retry.len()) == (2), "assert_eq failed");
        let retry_command = commands_after_retry
            .iter()
            .find(|command| command.command_id() == retry[0].command_id())
            .ok_or(ConsoleRuntimeError::TuiRuntimeFailed)
            .ok_test();
        check(
            (retry_command.idempotency_key()) == ("wi-1:work_item.approve_requested:1"),
            "assert_eq failed",
        );
        check((retry_command.status()) == ("pending"), "assert_eq failed");

        let replay = store
            .append_command(&CommandAppend::new(
                CommandEnvelope::new(
                    retry_command.command_id().to_owned(),
                    CommandType::WorkItemApproveRequested,
                    retry_command.aggregate_id().unwrap_or_default().to_owned(),
                    retry_command.idempotency_key().to_owned(),
                    retry_command.requested_by().to_owned(),
                ),
                RETRY_REQUESTED_AT.to_owned(),
                retry_command.aggregate_id().map(ToOwned::to_owned),
                format!("corr_{}", retry_command.command_id()),
                "{}".to_owned(),
            ))
            .ok_test();
        check(
            (replay.status()) == (CommandAppendStatus::Duplicate),
            "assert_eq failed",
        );

        let mut succeeding_port = SimulatedWorkItemActionPort::default();
        let retry_outcomes =
            handle_pending_work_item_commands(&mut store, RETRY_HANDLED_AT, &mut succeeding_port)
                .ok_test();
        check((retry_outcomes.len()) == (1), "assert_eq failed");
        check(
            (retry_outcomes[0].command_status()) == ("completed"),
            "assert_eq failed",
        );
        check(
            (succeeding_port.observed_action_ids) == (["approve:wi-1"]),
            "assert_eq failed",
        );
        let commands_after_success = store.list_commands().ok_test();
        check(
            (commands_after_success
                .iter()
                .find(|command| command.command_id() == "cmd_work_item_approve_requested_wi-1")
                .map(StoredCommand::status))
                == (Some("failed")),
            "assert_eq failed",
        );
        check(
            (commands_after_success
                .iter()
                .find(|command| command.command_id() == "cmd_work_item_approve_requested_wi-1_1")
                .map(StoredCommand::status))
                == (Some("completed")),
            "assert_eq failed",
        );

        let after_success =
            persist_tui_runtime_effects(&mut store, &once, AFTER_SUCCESS_REQUESTED_AT).ok_test();
        check((after_success.len()) == (1), "assert_eq failed");
        check(
            (after_success[0].status()) == (CommandAppendStatus::Duplicate),
            "assert_eq failed",
        );
        check(
            (store.list_commands().ok_test().len()) == (2),
            "assert_eq failed",
        );
    }

    #[test]
    fn store_backed_repeated_moves_all_land_and_drive_the_move_action() {
        // The regression the gate missed, plus the move idempotency-key fix: the
        // s-move valve → Confirm → effect → command → drive round-trip, exercised
        // for THREE moves of the SAME item (to backlog, back to ready, to backlog
        // AGAIN). The pre-fix static per-item key (`<id>:work_item.move_requested`)
        // deduped every move after the first, so the item could be moved once ever
        // and the third (repeat target) silently no-oped. Folding the monotonic
        // append sequence into the key makes every distinct move a distinct command
        // that lands and drives `move:<id>:<target>`.
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        // Surface a selectable work-item "wi-1" via the needs-attention inbox.
        let na_port = ScriptedNeedsAttentionPort::observing(vec![attention_item_fixture(
            "wi-1",
            "Move wi-1",
        )]);
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");
        ingest_needs_attention(&mut store, &needs_attention, "2026-07-17T00:00:00Z").ok_test();

        // Each staging sees the board record ingestion would have produced by
        // then: ready before the first move, backlog before the second, ready
        // again before the third — the availability check requires the staged
        // `from` to BE the item's lane.
        append_work_item_lane(&mut store, "wi-1", "ready", 1, "2026-07-17T00:00:00Z");
        let move_to_backlog = move_effect(
            &store.list_console_events().ok_test(),
            Lane::Ready,
            Lane::Backlog,
        );
        append_work_item_lane(&mut store, "wi-1", "backlog", 2, "2026-07-17T00:00:00Z");
        let move_back_to_ready = move_effect(
            &store.list_console_events().ok_test(),
            Lane::Backlog,
            Lane::Ready,
        );
        append_work_item_lane(&mut store, "wi-1", "ready", 3, "2026-07-17T00:00:00Z");
        let move_to_backlog_again = move_effect(
            &store.list_console_events().ok_test(),
            Lane::Ready,
            Lane::Backlog,
        );

        let mut factory_port = SimulatedFactoryDrainPort;
        let mut work_item_port = SimulatedWorkItemActionPort::default();
        let decisions = empty_decisions_port();
        let requester = poll_requester();
        let commands = command_requester();
        {
            let mut sink = StoreBackedTuiRuntimeEffectSink::new(
                &mut store,
                "2026-07-17T00:00:01Z",
                &mut factory_port,
                &mut work_item_port,
                &decisions,
                &requester,
                &commands,
            );
            for effect in [
                &move_to_backlog,
                &move_back_to_ready,
                &move_to_backlog_again,
            ] {
                let applied = sink
                    .handle_runtime_effect(effect)
                    .ok()
                    .ok_or(ConsoleRuntimeError::TuiRuntimeFailed)
                    .ok_test();
                check(
                    (applied) == (TuiRuntimeEffectSinkOutcome::Applied),
                    "assert_eq failed",
                );
            }
        }

        // All THREE distinct moves reached the orchestrator port as
        // `move:<id>:<target>` — including the repeat back to a prior target, which
        // the pre-fix static key would have silently deduped.
        check(
            (work_item_port.observed_action_ids)
                == (["move:wi-1:backlog", "move:wi-1:ready", "move:wi-1:backlog"]),
            "assert_eq failed",
        );
        // Three distinct move commands landed (not deduped down to one).
        let move_commands = store
            .list_commands()
            .ok_test()
            .iter()
            .filter(|command| {
                command.command_type() == CommandType::WorkItemMoveRequested.contract_name()
            })
            .count();
        check((move_commands) == (3), "assert_eq failed");
    }

    /// Build the `factory.drain_requested` effect the dispatch menu action
    /// produces, by driving the pure runtime through the registered menu row and
    /// the target readback confirmation — the same menu → Confirm → Confirm →
    /// effect path the interactive loop drives.
    fn drain_effect(events: &[ConsoleEvent]) -> TuiRuntimeEffect {
        let (top, selected) = console_application::action_registry::menu_tree()
            .iter()
            .enumerate()
            .find_map(|(top_index, _top)| {
                console_application::action_registry::menu_actions(top_index)
                    .iter()
                    .position(|spec| spec.id == "dispatch-ready")
                    .map(|action_index| (top_index, action_index))
            })
            .unwrap_or((0, 0));
        let state =
            TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::Menu { top, selected });
        let staged =
            console_tui::step_tui_runtime(&state, events, TuiTerminalInput::Confirm, "operator");
        console_tui::step_tui_runtime(
            staged.state(),
            events,
            TuiTerminalInput::Confirm,
            "operator",
        )
        .effect()
        .clone()
    }

    /// Seed one Ready-lane work-item so the drain policy ACCEPTS the drain — with
    /// an empty Ready lane every drain is policy-rejected before it ever reaches
    /// the Dispatcher port, which would make a repeatability assertion vacuous.
    fn seed_ready_work_item(store: &mut SqliteEventStore) {
        let source = sequenced_work_item_source(&[("wi-ready", Lane::Ready, "ready", 1)]);
        let sources: Vec<SourceAdapterRef<'_>> =
            vec![("orchestrator:livespec-console-beads-fabro", &source)];
        let na_port = empty_needs_attention_port();
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");
        refresh_sources(store, "2026-07-19T00:00:00Z", &sources, &needs_attention).ok_test();
    }

    /// Drive `count` dispatch-menu gestures through a store-backed sink and
    /// report the drains the Dispatcher port observed alongside the drain
    /// commands that actually landed in the store. The two together are what
    /// separates a drain that LANDS from one the static-key dedupe swallowed: a
    /// deduped command neither appends a row nor reaches the port.
    fn drive_drains(store: &mut SqliteEventStore, count: usize) -> (Vec<String>, usize) {
        let events = store.list_console_events().ok_test();
        let effect = drain_effect(&events);
        let mut factory_port = RecordingFactoryDrainPort::default();
        let mut work_item_port = SimulatedWorkItemActionPort::default();
        let decisions = empty_decisions_port();
        let requester = poll_requester();
        let commands = command_requester();
        {
            let mut sink = StoreBackedTuiRuntimeEffectSink::new(
                store,
                "2026-07-19T00:00:01Z",
                &mut factory_port,
                &mut work_item_port,
                &decisions,
                &requester,
                &commands,
            );
            for _gesture in 0..count {
                let applied = sink
                    .handle_runtime_effect(&effect)
                    .ok()
                    .ok_or(ConsoleRuntimeError::TuiRuntimeFailed)
                    .ok_test();
                check(
                    (applied) == (TuiRuntimeEffectSinkOutcome::Applied),
                    "assert_eq failed",
                );
            }
        }
        let drain_commands = store
            .list_commands()
            .ok_test()
            .iter()
            .filter(|command| {
                command.command_type() == CommandType::FactoryDrainRequested.contract_name()
            })
            .count();
        (factory_port.observed_aggregate_ids, drain_commands)
    }

    #[test]
    fn store_backed_repeated_drains_all_land_and_reach_the_drain_port() {
        // The reported bug: the console could perform exactly ONE factory drain
        // per store, EVER. The drain command is payload-less and its aggregate is
        // the FLEET (`fleet:livespec`), so its key
        // (`fleet:livespec:factory.drain_requested:budget=1:parallel=1`) was
        // constant for all time; `insert or ignore` against the `unique`
        // idempotency_key made every later `:drain` a silent no-op that appended
        // no row and never reached the Dispatcher. Folding the monotonic append
        // sequence into the key makes each gesture a distinct command.
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        seed_ready_work_item(&mut store);

        let (observed, drain_commands) = drive_drains(&mut store, 2);

        check(
            (observed) == (["fleet:livespec", "fleet:livespec"]),
            "assert_eq failed",
        );
        check((drain_commands) == (2), "assert_eq failed");
    }

    #[test]
    fn store_backed_drain_lands_despite_a_spent_terminal_drain_row() {
        // The RECOVERY property, and the difference between "fixed going forward"
        // and "this store is still bricked". `find_existing_command_id` matches on
        // idempotency_key with NO status filter, so under the static key a drain
        // already spent at a TERMINAL status (`failed`) blocked every future drain
        // just as firmly as a pending one would — the operator's only signal being
        // to read the SQLite store directly. A sequence-distinguished key cannot
        // collide with that legacy row, so an existing store recovers with no
        // store surgery.
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        seed_ready_work_item(&mut store);
        let spent = CommandEnvelope::new(
            "cmd_factory_drain_requested_budget_1_parallel_1".to_owned(),
            CommandType::FactoryDrainRequested,
            "fleet:livespec".to_owned(),
            "fleet:livespec:factory.drain_requested:budget=1:parallel=1".to_owned(),
            "operator".to_owned(),
        );
        store
            .append_command(&CommandAppend::new(
                spent.clone(),
                "2026-07-18T00:00:00Z".to_owned(),
                Some(spent.aggregate_id().to_owned()),
                "corr_spent_drain".to_owned(),
                "{}".to_owned(),
            ))
            .ok_test();
        let terminal_update = store.update_command_status(
            spent.command_id(),
            "failed",
            "2026-07-18T00:00:01Z",
            Some(r#"{"event_count":3}"#),
            Some("{}"),
        );
        check(terminal_update.is_ok(), "assert failed");

        let (observed, drain_commands) = drive_drains(&mut store, 1);

        // The new drain reached the Dispatcher...
        check((observed) == (["fleet:livespec"]), "assert_eq failed");
        // ...as a SECOND row beside the spent one, which is left untouched.
        check((drain_commands) == (2), "assert_eq failed");
        let spent_status = store
            .list_commands()
            .ok_test()
            .into_iter()
            .find(|command| command.command_id() == spent.command_id())
            .map(|command| command.status().to_owned());
        check(
            (spent_status.as_deref()) == (Some("failed")),
            "assert_eq failed",
        );
    }

    #[test]
    fn distinguish_repeatable_command_distinguishes_drain_and_leaves_healthy_valves_alone() {
        // The pure-function contract, tested directly rather than through the
        // effect pipeline: which actions get a per-append identity, and that an
        // exact re-persist at the SAME sequence still dedupes so replay safety
        // survives.
        let drain = CommandEnvelope::new(
            "cmd_factory_drain_requested_budget_1_parallel_1".to_owned(),
            CommandType::FactoryDrainRequested,
            "fleet:livespec".to_owned(),
            "fleet:livespec:factory.drain_requested:budget=1:parallel=1".to_owned(),
            "operator".to_owned(),
        );
        let distinguished = distinguish_repeatable_command(&drain, 7);
        check(
            (distinguished.command_id()) == ("cmd_factory_drain_requested_budget_1_parallel_1_7"),
            "assert_eq failed",
        );
        check(
            (distinguished.idempotency_key())
                == ("fleet:livespec:factory.drain_requested:budget=1:parallel=1:7"),
            "assert_eq failed",
        );
        // Replay safety: the same command at the same sequence is byte-identical,
        // so an exact re-persist still dedupes against its own earlier row.
        check(
            (distinguish_repeatable_command(&drain, 7).idempotency_key())
                == (distinguished.idempotency_key()),
            "assert_eq failed",
        );

        // The once-per-item valves are NOT unconditionally widened into:
        // approving or accepting an item twice SHOULD be absorbed as an
        // idempotent no-op while the original row is not terminal-failed, so the
        // pure repeatable discriminator keeps both static.
        for (command_id, command_type, idempotency_key) in [
            (
                "cmd_work_item_approve_requested_wi-1",
                CommandType::WorkItemApproveRequested,
                "wi-1:work_item.approve_requested",
            ),
            (
                "cmd_work_item_accept_requested_wi-1",
                CommandType::WorkItemAcceptRequested,
                "wi-1:work_item.accept_requested",
            ),
        ] {
            let once_only = CommandEnvelope::new(
                command_id.to_owned(),
                command_type,
                "wi-1".to_owned(),
                idempotency_key.to_owned(),
                "operator".to_owned(),
            );
            let unchanged = distinguish_repeatable_command(&once_only, 7);
            check((unchanged.command_id()) == (command_id), "assert_eq failed");
            check(
                (unchanged.idempotency_key()) == (idempotency_key),
                "assert_eq failed",
            );
        }

        let move_command = CommandEnvelope::new(
            "cmd_work_item_move_requested_wi-1_ready".to_owned(),
            CommandType::WorkItemMoveRequested,
            "wi-1".to_owned(),
            "wi-1:work_item.move_requested:target_status=ready".to_owned(),
            "operator".to_owned(),
        );
        check(
            !is_failed_once_only_valve_retry(&move_command, &[]),
            "assert failed",
        );
    }

    #[test]
    fn store_backed_tui_session_reports_runner_errors() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let mut runner = ErroringTuiSessionRunner;
        let mut factory_port = SimulatedFactoryDrainPort;
        let mut work_item_port = SimulatedWorkItemActionPort::default();
        let scripted = scripted_source_list();
        let sources = scripted_source_refs(&scripted);
        let na_port = empty_needs_attention_port();
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");

        let outcome = run_store_backed_tui_session(
            &mut store,
            "2026-06-23T00:00:02Z",
            "operator",
            &mut runner,
            &sources,
            &mut factory_port,
            &mut work_item_port,
            &empty_decisions_port(),
            &needs_attention,
            &poll_requester(),
            &command_requester(),
        );

        check(
            format!("{outcome:?}").contains("TuiRuntimeFailed"),
            "assert failed",
        );
        check(
            (store.list_console_events().ok_test().len()) == (6),
            "assert_eq failed",
        );
    }

    #[test]
    fn ingest_needs_attention_diffs_snapshot_into_stream_and_projects_inbox() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();

        // First ingest against an empty prior: both items appear.
        let first_port = ScriptedNeedsAttentionPort::observing(vec![
            attention_item_fixture("wi-approve", "Pending approval"),
            attention_item_fixture("wi-accept", "Acceptance review"),
        ]);
        let first = NeedsAttentionIngest::new(&first_port, "livespec-console-beads-fabro");
        let appeared = ingest_needs_attention(&mut store, &first, "2026-07-07T00:00:00Z").ok_test();
        check((appeared) == (2), "assert_eq failed");
        check(
            (attention_event_count(
                &store.list_console_events().ok_test(),
                EventType::AttentionItemAppeared,
            )) == (2),
            "assert_eq failed",
        );
        check(
            (project_attention(&store.list_console_events().ok_test()).len()) == (2),
            "assert_eq failed",
        );

        // Re-ingest the identical snapshot: idempotent, emits nothing.
        let unchanged =
            ingest_needs_attention(&mut store, &first, "2026-07-07T00:01:00Z").ok_test();
        check((unchanged) == (0), "assert_eq failed");
        check(
            (store.list_console_events().ok_test().len()) == (2),
            "assert_eq failed",
        );

        // Second ingest: wi-approve changes, wi-accept resolves, wi-blocked appears.
        let second_port = ScriptedNeedsAttentionPort::observing(vec![
            attention_item_fixture("wi-approve", "Pending approval (urgent)"),
            attention_item_fixture("wi-blocked", "Blocked: needs-human"),
        ]);
        let second = NeedsAttentionIngest::new(&second_port, "livespec-console-beads-fabro");
        let ingested =
            ingest_needs_attention(&mut store, &second, "2026-07-07T00:02:00Z").ok_test();
        check((ingested) == (3), "assert_eq failed");

        let events = store.list_console_events().ok_test();
        check(
            (attention_event_count(&events, EventType::AttentionItemChanged)) == (1),
            "assert_eq failed",
        );
        check(
            (attention_event_count(&events, EventType::AttentionItemResolved)) == (1),
            "assert_eq failed",
        );
        check(
            (attention_event_count(&events, EventType::AttentionItemAppeared)) == (3),
            "assert_eq failed",
        );

        let inbox = project_attention(&events);
        let ids: Vec<&str> = inbox.iter().map(AttentionItem::id).collect();
        check((ids) == (["wi-approve", "wi-blocked"]), "assert_eq failed");
        let approve = &inbox[0];
        check(
            (approve.title()) == ("Pending approval (urgent)"),
            "assert_eq failed",
        );
        check((approve.source()) == ("human-valve"), "assert_eq failed");
        check(
            (approve.source_reference()) == ("livespec-console-beads-fabro:wi-approve"),
            "assert_eq failed",
        );
    }

    #[test]
    fn ingest_needs_attention_preserves_inbox_when_source_unavailable() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let present = ScriptedNeedsAttentionPort::observing(vec![attention_item_fixture(
            "wi-approve",
            "Pending approval",
        )]);
        let ingest = NeedsAttentionIngest::new(&present, "livespec-console-beads-fabro");
        check(
            (ingest_needs_attention(&mut store, &ingest, "2026-07-07T00:00:00Z").ok_test()) == (1),
            "assert_eq failed",
        );

        // An unavailable read must NOT resolve the inbox from a failed poll.
        let down = ScriptedNeedsAttentionPort::unavailable("needs-attention: binary missing");
        let ingest_down = NeedsAttentionIngest::new(&down, "livespec-console-beads-fabro");
        check(
            (ingest_needs_attention(&mut store, &ingest_down, "2026-07-07T00:01:00Z").ok_test())
                == (0),
            "assert_eq failed",
        );
        check(
            (project_attention(&store.list_console_events().ok_test()).len()) == (1),
            "assert_eq failed",
        );
    }

    fn attention_event_count(events: &[ConsoleEvent], event_type: EventType) -> usize {
        events
            .iter()
            .filter(|event| event.event_type() == &event_type)
            .count()
    }

    #[test]
    fn runtime_error_conversions_keep_source_context() {
        check(
            format!(
                "{:?}",
                ConsoleRuntimeError::from(ApplicationError::FactoryDrainPortFailed)
            )
            .contains("FactoryDrainPortFailed"),
            "assert failed",
        );
        check(
            format!(
                "{:?}",
                ConsoleRuntimeError::from(EventStoreError::InvalidSequence)
            )
            .contains("InvalidSequence"),
            "assert failed",
        );
    }

    #[test]
    fn effect_sink_io_error_formats_debug_context() {
        let error = effect_sink_io_error(EventStoreError::InvalidSequence);

        check(
            (error.kind()) == (std::io::ErrorKind::Other),
            "assert_eq failed",
        );
        check(
            (error.to_string()) == ("InvalidSequence"),
            "assert_eq failed",
        );
    }

    #[test]
    fn sqlite_factory_command_store_forwards_unguarded_status_updates() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let effects = [factory_drain_effect()];
        persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z").ok_test();
        let command_id = store.list_commands().ok_test()[0].command_id().to_owned();

        let result_json = Some(r#"{"event_count":0}"#);
        let updated = FactoryCommandStore::update_command_status(
            &mut store,
            &command_id,
            "completed",
            "2026-06-23T00:00:03Z",
            result_json,
            None,
        );

        check(
            (updated.ok_test().status()) == ("completed"),
            "assert_eq failed",
        );
        check(
            (store.list_commands().ok_test()[0].status()) == ("completed"),
            "assert_eq failed",
        );
    }

    #[test]
    fn command_status_update_runtime_result_maps_success_and_failure() {
        let success = command_status_update_runtime_result(Ok(CommandStatusUpdateOutcome::new(
            "cmd_1".to_owned(),
            "completed".to_owned(),
        )));
        let failure = command_status_update_runtime_result(Err(EventStoreError::InvalidSequence));

        let success = success.ok_test();
        check((success.command_id()) == ("cmd_1"), "assert_eq failed");
        check((success.status()) == ("completed"), "assert_eq failed");
        check(
            format!("{failure:?}").contains("InvalidSequence"),
            "assert failed",
        );
    }

    #[test]
    fn two_factory_executors_claim_one_pending_command_once() {
        struct ReentrantFactoryDrainPort {
            loser_store: Rc<RefCell<SqliteEventStore>>,
            calls: Rc<std::cell::Cell<usize>>,
        }

        impl FactoryDrainPort for ReentrantFactoryDrainPort {
            fn drain_ready_queue(
                &mut self,
                _request: &FactoryDrainRequest,
            ) -> Result<FactoryDrainPortOutcome, ApplicationError> {
                self.calls.set(self.calls.get() + 1);
                let mut nested_port = RecordingFactoryDrainPort::default();
                let nested = handle_pending_factory_commands(
                    &mut *self.loser_store.borrow_mut(),
                    "2026-06-23T00:00:03Z",
                    &mut nested_port,
                );
                check(nested.ok_test().is_empty(), "assert failed");
                check(
                    (nested_port.observed_aggregate_ids) == (Vec::<String>::new()),
                    "assert_eq failed",
                );
                Ok(FactoryDrainPortOutcome::completed(1))
            }
        }

        let path = std::env::temp_dir().join(format!(
            "livespec-console-command-claim-{}-{}.sqlite",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_error| ConsoleRuntimeError::TuiRuntimeFailed)
                .ok_test()
                .as_nanos()
        ));
        let _ignored = fs::remove_file(&path);
        let mut winner_store = SqliteEventStore::open(&path).ok_test();
        let effects = [factory_drain_effect()];
        persist_tui_runtime_effects(&mut winner_store, &effects, "2026-06-23T00:00:02Z").ok_test();
        append_ready_work_item(&mut winner_store, "2026-06-23T00:00:02Z");
        let loser_store = Rc::new(RefCell::new(SqliteEventStore::open(&path).ok_test()));
        let calls = Rc::new(std::cell::Cell::new(0));
        let mut port = ReentrantFactoryDrainPort {
            loser_store,
            calls: Rc::clone(&calls),
        };

        let outcomes =
            handle_pending_factory_commands(&mut winner_store, "2026-06-23T00:00:03Z", &mut port)
                .ok_test();

        check((outcomes.len()) == (1), "assert_eq failed");
        check((calls.get()) == (1), "assert_eq failed");
        check(
            (winner_store.list_commands().ok_test()[0].status()) == ("completed"),
            "assert_eq failed",
        );
        let _ignored = fs::remove_file(&path);
    }

    #[test]
    fn stale_executing_factory_command_is_failed_without_reexecution() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let effects = [factory_drain_effect()];
        persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z").ok_test();
        let command_id = store.list_commands().ok_test()[0].command_id().to_owned();
        check(
            store
                .claim_command(&command_id, "2026-06-22T00:00:00Z")
                .ok_test(),
            "assert failed",
        );
        let mut port = RecordingFactoryDrainPort::default();

        let outcomes =
            handle_pending_factory_commands(&mut store, "2026-06-23T01:00:01Z", &mut port)
                .ok_test();

        check(outcomes.is_empty(), "assert failed");
        check(
            (port.observed_aggregate_ids) == (Vec::<String>::new()),
            "assert_eq failed",
        );
        check(
            (store.list_commands().ok_test()[0].status()) == ("failed"),
            "assert_eq failed",
        );
    }

    #[test]
    fn pending_factory_commands_append_lifecycle_events_and_complete() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let effects = [factory_drain_effect()];
        persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z").ok_test();
        append_ready_work_item(&mut store, "2026-06-23T00:00:02Z");
        let mut port = SimulatedFactoryDrainPort;

        let outcomes =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port)
                .ok_test();
        let commands = store.list_commands().ok_test();
        let events = store.list_console_events().ok_test();

        check((outcomes.len()) == (1), "assert_eq failed");
        // Sequence-distinguished on persist (the drain is repeatable), so the
        // handled command carries the `_0` identity, not the authored static one.
        check(
            (outcomes[0])
                == (super::PendingCommandOutcome::new(
                    "cmd_factory_drain_requested_budget_1_parallel_1_0".to_owned(),
                    "completed".to_owned(),
                    3,
                )),
            "assert_eq failed",
        );
        check(
            (outcomes[0].command_id()) == ("cmd_factory_drain_requested_budget_1_parallel_1_0"),
            "assert_eq failed",
        );
        check(
            (outcomes[0].command_status()) == ("completed"),
            "assert_eq failed",
        );
        check(
            (outcomes[0].appended_event_count()) == (3),
            "assert_eq failed",
        );
        check((commands[0].status()) == ("completed"), "assert_eq failed");
        check(
            (events
                .iter()
                .map(ConsoleEvent::event_type)
                .collect::<Vec<_>>())
                == ([
                    &EventType::WorkItemSnapshotObserved,
                    &EventType::CommandAccepted,
                    &EventType::FactoryDrainStarted,
                    &EventType::FactoryDrainCompleted,
                ]),
            "assert_eq failed",
        );
    }

    #[test]
    fn finalizing_pending_command_counts_only_inserted_events() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        append_ready_work_item(&mut store, "2026-06-23T00:00:02Z");
        persist_tui_runtime_effects(
            &mut store,
            &[factory_drain_effect()],
            "2026-06-23T00:00:03Z",
        )
        .ok_test();
        let stored = store.list_commands().ok_test();
        let command = CommandEnvelope::new(
            stored[0].command_id().to_owned(),
            CommandType::FactoryDrainRequested,
            "fleet:livespec".to_owned(),
            stored[0].idempotency_key().to_owned(),
            stored[0].requested_by().to_owned(),
        );
        let mut preseed_port = SimulatedFactoryDrainPort;
        let outcome =
            handle_factory_drain_command(&command, &FactoryDrainPolicy::new(1), &mut preseed_port)
                .ok_test();
        for event in outcome.events() {
            let append = event_append_from_command_event(event, &command, "2026-06-23T00:00:04Z");
            store.append_event(&append).ok_test();
        }
        let mut port = SimulatedFactoryDrainPort;

        let outcomes =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:04Z", &mut port)
                .ok_test();

        check(
            (outcomes[0].appended_event_count()) == (0),
            "assert_eq failed",
        );
    }

    #[test]
    fn pending_factory_commands_record_failed_port_outcome() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let effects = [factory_drain_effect()];
        persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z").ok_test();
        append_ready_work_item(&mut store, "2026-06-23T00:00:02Z");
        let mut port = FailedFactoryDrainPort;

        let outcomes =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port)
                .ok_test();
        let commands = store.list_commands().ok_test();
        let events = store.list_console_events().ok_test();
        let expected = serde_json::json!({
            "summary": "factory-safety refusal",
            "domain_error": "host-only-refused"
        })
        .to_string();

        check(
            (outcomes[0].command_status()) == ("failed"),
            "assert_eq failed",
        );
        check((commands[0].status()) == ("failed"), "assert_eq failed");
        check(
            (commands[0].error_json()) == (Some(expected.as_str())),
            "assert_eq failed",
        );
        check(
            (events.last().map(ConsoleEvent::event_type)) == (Some(&EventType::FactoryDrainFailed)),
            "assert_eq failed",
        );
        check(
            (events.last().map(ConsoleEvent::payload_json)) == (commands[0].error_json()),
            "assert_eq failed",
        );
    }

    #[test]
    fn pending_factory_commands_record_human_valve_park_without_failure() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let effects = [factory_drain_effect()];
        persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z").ok_test();
        append_ready_work_item(&mut store, "2026-06-23T00:00:02Z");
        let mut port = ParkedFactoryDrainPort;

        let outcomes =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port)
                .ok_test();
        let commands = store.list_commands().ok_test();
        let events = store.list_console_events().ok_test();

        check(
            (outcomes[0].command_status()) == ("parked-awaiting-human"),
            "assert_eq failed",
        );
        check(
            (commands[0].status()) == ("parked-awaiting-human"),
            "assert_eq failed",
        );
        check(commands[0].error_json().is_none(), "assert_eq failed");
        check(
            (events.last().map(ConsoleEvent::event_type))
                == (Some(&EventType::FactoryDrainAwaitingHuman)),
            "assert_eq failed",
        );
    }

    #[test]
    fn pending_factory_commands_return_status_update_errors() {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::StatusUpdateFails);
        let mut port = SimulatedFactoryDrainPort;

        let outcome =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port);

        check(
            format!("{outcome:?}").contains("InvalidSequence"),
            "assert failed",
        );
        check((store.appended_event_count) == (3), "assert_eq failed");
    }

    #[test]
    fn pending_factory_commands_return_list_errors() {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::ListFails);
        let mut port = SimulatedFactoryDrainPort;

        let outcome =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port);

        check(
            format!("{outcome:?}").contains("InvalidSequence"),
            "assert failed",
        );
    }

    #[test]
    fn pending_factory_commands_return_missing_aggregate_errors() {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::MissingAggregate);
        let mut port = SimulatedFactoryDrainPort;

        let outcome =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port);

        check(
            format!("{outcome:?}").contains("cmd_missing_aggregate"),
            "assert failed",
        );
    }

    #[test]
    fn pending_factory_commands_return_port_errors() {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::StatusUpdateFails);
        let mut port = ErroringFactoryDrainPort;

        let outcome =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port);

        check(
            format!("{outcome:?}").contains("FactoryDrainPortFailed"),
            "assert failed",
        );
        check((store.appended_event_count) == (0), "assert_eq failed");
    }

    #[test]
    fn pending_factory_commands_return_append_errors() {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::AppendFails);
        let mut port = SimulatedFactoryDrainPort;

        let outcome =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port);

        check(
            format!("{outcome:?}").contains("InvalidSequence"),
            "assert failed",
        );
    }

    #[test]
    fn scripted_factory_command_store_supports_successful_handling() {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::Completes);
        let mut port = SimulatedFactoryDrainPort;

        let outcomes =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port)
                .ok_test();

        check(
            (outcomes)
                == (vec![PendingCommandOutcome::new(
                    "cmd_factory_drain_requested_budget_1_parallel_1".to_owned(),
                    "completed".to_owned(),
                    3,
                )]),
            "assert_eq failed",
        );
        check((store.appended_event_count) == (3), "assert_eq failed");
    }

    #[test]
    fn pending_factory_command_handler_skips_non_factory_or_non_pending() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let effects = [TuiRuntimeEffect::PersistCommand(CommandEnvelope::new(
            "cmd_factory_drain_requested_budget_1_parallel_1".to_owned(),
            CommandType::FactoryDrainRequested,
            "fleet:livespec".to_owned(),
            "fleet:livespec:factory.drain_requested:budget=1:parallel=1".to_owned(),
            "operator".to_owned(),
        ))];
        persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z").ok_test();
        // The persisted id is sequence-distinguished (`_0`), so drive the status
        // update against THAT id rather than the authored static one.
        let update = store.update_command_status(
            "cmd_factory_drain_requested_budget_1_parallel_1_0",
            "completed",
            "2026-06-23T00:00:03Z",
            Some("{}"),
            None,
        );
        check(update.is_ok(), "assert failed");
        let mut port = SimulatedFactoryDrainPort;

        let outcomes =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port)
                .ok_test();

        check(outcomes.is_empty(), "assert_eq failed");
        check(
            store.list_console_events().ok_test().is_empty(),
            "assert_eq failed",
        );
    }

    #[test]
    fn pending_factory_dispatch_item_command_records_not_wired() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let effects = [TuiRuntimeEffect::PersistCommand(CommandEnvelope::new(
            "cmd_factory_dispatch_item_requested_wi_1".to_owned(),
            CommandType::FactoryDispatchItemRequested,
            "wi-1".to_owned(),
            "wi-1:factory.dispatch_item_requested".to_owned(),
            "operator".to_owned(),
        ))];
        persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z").ok_test();
        let mut port = SimulatedFactoryDrainPort;

        let outcomes =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port)
                .ok_test();

        check(
            (outcomes)
                == (vec![PendingCommandOutcome::new(
                    "cmd_factory_dispatch_item_requested_wi_1_0".to_owned(),
                    "not_wired".to_owned(),
                    2,
                )]),
            "assert_eq failed",
        );
        let events = store.list_console_events().ok_test();
        check(
            (events
                .iter()
                .map(ConsoleEvent::event_type)
                .collect::<Vec<_>>())
                == (vec![
                    &EventType::CommandAccepted,
                    &EventType::FactoryDispatchItemNotWired,
                ]),
            "assert_eq failed",
        );
    }

    #[test]
    fn pending_factory_dispatch_item_command_can_complete_through_wired_port() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let effects = [TuiRuntimeEffect::PersistCommand(CommandEnvelope::new(
            "cmd_factory_dispatch_item_requested_wi_1".to_owned(),
            CommandType::FactoryDispatchItemRequested,
            "wi-1".to_owned(),
            "wi-1:factory.dispatch_item_requested".to_owned(),
            "operator".to_owned(),
        ))];
        check(
            persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z").is_ok(),
            "assert failed",
        );
        let mut drain_port = SimulatedFactoryDrainPort;
        let mut dispatch_item_port = CompletingFactoryDispatchItemPort::default();

        let outcomes = handle_pending_factory_commands_with_dispatch_port(
            &mut store,
            "2026-06-23T00:00:03Z",
            &mut drain_port,
            &mut dispatch_item_port,
        );

        check(outcomes.is_ok(), "assert failed");
        let outcomes = outcomes.unwrap_or_default();
        check(
            (outcomes[0].command_status()) != ("not_wired"),
            "assert_ne failed",
        );
        check(
            (outcomes[0].command_status()) == ("completed"),
            "assert_eq failed",
        );
        check(
            (dispatch_item_port.observed_work_item_ids) == (["wi-1"]),
            "assert_eq failed",
        );
        let events = store.list_console_events().unwrap_or_default();
        check(
            (events
                .iter()
                .map(ConsoleEvent::event_type)
                .collect::<Vec<_>>())
                == (vec![
                    &EventType::CommandAccepted,
                    &EventType::FactoryDispatchItemStarted,
                    &EventType::FactoryDispatchItemCompleted,
                ]),
            "assert_eq failed",
        );
    }

    #[test]
    fn pending_factory_dispatch_item_command_propagates_port_errors() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let effects = [TuiRuntimeEffect::PersistCommand(CommandEnvelope::new(
            "cmd_factory_dispatch_item_requested_wi_1".to_owned(),
            CommandType::FactoryDispatchItemRequested,
            "wi-1".to_owned(),
            "wi-1:factory.dispatch_item_requested".to_owned(),
            "operator".to_owned(),
        ))];
        persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z")
            .map_err(|_error| EventStoreError::InvalidSequence)
            .ok_test();
        let mut drain_port = SimulatedFactoryDrainPort;
        let mut dispatch_item_port = ErroringFactoryDispatchItemPort;

        let outcome = handle_pending_factory_commands_with_dispatch_port(
            &mut store,
            "2026-06-23T00:00:03Z",
            &mut drain_port,
            &mut dispatch_item_port,
        );

        check(
            format!("{outcome:?}").contains("FactoryDispatchItemPortFailed"),
            "assert failed",
        );
    }

    #[test]
    fn serve_report_with_dispatch_port_propagates_dispatch_errors() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let effects = [TuiRuntimeEffect::PersistCommand(CommandEnvelope::new(
            "cmd_factory_dispatch_item_requested_wi_1".to_owned(),
            CommandType::FactoryDispatchItemRequested,
            "wi-1".to_owned(),
            "wi-1:factory.dispatch_item_requested".to_owned(),
            "operator".to_owned(),
        ))];
        persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z")
            .map_err(|_error| EventStoreError::InvalidSequence)
            .ok_test();
        let scripted = scripted_source_list();
        let sources = scripted_source_refs(&scripted);
        let mut factory_port = SimulatedFactoryDrainPort;
        let mut dispatch_item_port = ErroringFactoryDispatchItemPort;
        let mut work_item_port = SimulatedWorkItemActionPort::default();
        let na_port = empty_needs_attention_port();
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");

        let outcome = serve_report_with_dispatch_port(
            &mut store,
            "2026-06-23T00:00:03Z",
            &sources,
            &mut factory_port,
            &mut dispatch_item_port,
            &mut work_item_port,
            &empty_decisions_port(),
            &needs_attention,
        );

        check(
            format!("{outcome:?}").contains("FactoryDispatchItemPortFailed"),
            "assert failed",
        );
    }

    #[test]
    fn per_item_dispatch_request_event_uses_dispatch_item_event_type() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let effects = [TuiRuntimeEffect::PersistCommand(CommandEnvelope::new(
            "cmd_factory_dispatch_item_requested_wi_1".to_owned(),
            CommandType::FactoryDispatchItemRequested,
            "wi-1".to_owned(),
            "wi-1:factory.dispatch_item_requested".to_owned(),
            "operator".to_owned(),
        ))];
        let outcomes =
            persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z").ok_test();

        append_factory_drain_requested_events(&mut store, &outcomes, "2026-06-23T00:00:02Z")
            .ok_test();

        let events = store.list_console_events().ok_test();
        check(
            (events
                .iter()
                .map(ConsoleEvent::event_type)
                .collect::<Vec<_>>())
                == (vec![&EventType::FactoryDispatchItemRequested]),
            "assert_eq failed",
        );
    }

    #[test]
    fn pending_factory_command_handler_ignores_a_lost_claim() {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::FactoryClaimMiss);
        let mut port = RecordingFactoryDrainPort::default();

        let outcomes =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port)
                .ok_test();

        check(outcomes.is_empty(), "assert_eq failed");
        check(
            (port.observed_aggregate_ids) == (Vec::<String>::new()),
            "assert_eq failed",
        );
        check((store.appended_event_count) == (0), "assert_eq failed");
    }

    #[test]
    fn pending_factory_commands_return_stale_recovery_errors() {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::RecoveryFails);
        let mut port = RecordingFactoryDrainPort::default();

        let outcome =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port);

        check(
            format!("{outcome:?}").contains("InvalidSequence"),
            "assert failed",
        );
        check(
            (port.observed_aggregate_ids) == (Vec::<String>::new()),
            "assert_eq failed",
        );
    }

    #[test]
    fn pending_factory_commands_skip_stale_recovery_for_unparseable_time() {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::NonFactoryPending);
        let mut port = RecordingFactoryDrainPort::default();

        let outcomes =
            handle_pending_factory_commands(&mut store, "not-rfc3339", &mut port).ok_test();

        check(outcomes.is_empty(), "assert_eq failed");
        check((store.appended_event_count) == (0), "assert_eq failed");
    }

    #[test]
    fn pending_factory_command_handler_skips_pending_non_factory_commands() {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::NonFactoryPending);
        let mut port = SimulatedFactoryDrainPort;

        let outcomes =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port)
                .ok_test();

        check(outcomes.is_empty(), "assert_eq failed");
        check((store.appended_event_count) == (0), "assert_eq failed");
    }

    #[test]
    fn factory_command_reconstruction_ignores_non_factory_commands() {
        let stored_command = StoredCommand::new(
            "cmd_non_factory".to_owned(),
            "attention".to_owned(),
            "attention.local_only".to_owned(),
            Some("work-item".to_owned()),
            "idem_non_factory".to_owned(),
            "operator".to_owned(),
            "pending".to_owned(),
        );

        check(
            format!("{:?}", factory_command_from_stored(&stored_command)).contains("Ok(None)"),
            "assert failed",
        );
    }

    #[test]
    fn factory_command_reconstruction_requires_aggregate() {
        let stored_command = StoredCommand::new(
            "cmd_1".to_owned(),
            "factory".to_owned(),
            "factory.drain_requested".to_owned(),
            None,
            "idem_1".to_owned(),
            "operator".to_owned(),
            "pending".to_owned(),
        );

        let result = factory_command_from_stored(&stored_command);

        check(format!("{result:?}").contains("cmd_1"), "assert failed");
    }

    #[test]
    fn pending_work_item_commands_ignore_a_lost_claim() {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::WorkItemClaimMiss);
        let mut port = SimulatedWorkItemActionPort::default();

        let outcomes =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port)
                .ok_test();

        check(outcomes.is_empty(), "assert_eq failed");
        check(
            (port.observed_action_ids) == (Vec::<String>::new()),
            "assert_eq failed",
        );
        check((store.appended_event_count) == (0), "assert_eq failed");
    }

    #[test]
    fn pending_config_commands_ignore_a_lost_claim() {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::ConfigClaimMiss);
        let mut port = SimulatedWorkItemActionPort::default();

        let outcomes =
            handle_pending_config_commands(&mut store, "2026-07-11T00:00:01Z", &mut port).ok_test();

        check(outcomes.is_empty(), "assert_eq failed");
        check(
            (port.observed_action_ids) == (Vec::<String>::new()),
            "assert_eq failed",
        );
        check((store.appended_event_count) == (0), "assert_eq failed");
    }

    #[test]
    fn work_item_command_reconstruction_ignores_non_work_item_commands() {
        let stored_command = StoredCommand::new(
            "cmd_factory".to_owned(),
            "factory".to_owned(),
            "factory.drain_requested".to_owned(),
            Some("fleet:livespec".to_owned()),
            "idem_factory".to_owned(),
            "operator".to_owned(),
            "pending".to_owned(),
        );

        let result =
            work_item_command_from_stored(&stored_command).map(|command| command.is_none());

        check(format!("{result:?}").contains("Ok(true)"), "assert failed");
    }

    #[test]
    fn work_item_command_reconstruction_requires_aggregate() {
        let stored_command = StoredCommand::new(
            "cmd_approve".to_owned(),
            "work_item".to_owned(),
            "work_item.approve_requested".to_owned(),
            None,
            "idem_approve".to_owned(),
            "operator".to_owned(),
            "pending".to_owned(),
        );

        let result = work_item_command_from_stored(&stored_command).map(|_command| ());

        check(
            format!("{result:?}").contains("cmd_approve"),
            "assert failed",
        );
    }

    fn config_command_append(payload_json: &str) -> CommandAppend {
        CommandAppend::new(
            CommandEnvelope::new(
                "cmd_setting".to_owned(),
                CommandType::ConfigDispatcherSettingSet,
                "livespec-console-beads-fabro".to_owned(),
                "livespec-console-beads-fabro:config.dispatcher_setting_set".to_owned(),
                "operator".to_owned(),
            ),
            "2026-07-11T00:00:00Z".to_owned(),
            Some("livespec-console-beads-fabro".to_owned()),
            "corr_cmd_setting".to_owned(),
            payload_json.to_owned(),
        )
    }

    #[test]
    fn pending_config_dispatcher_setting_set_effects_through_port_and_is_idempotent() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        store.append_command(&config_command_append(
            r#"{"repo":"livespec-console-beads-fabro","setting":"auto_approve_ready","value":true}"#,
        )).ok_test();
        let mut port = SimulatedWorkItemActionPort::default();

        let outcomes =
            handle_pending_config_commands(&mut store, "2026-07-11T00:00:01Z", &mut port).ok_test();

        check((outcomes.len()) == (1), "assert_eq failed");
        check(
            (outcomes[0].command_status()) == ("completed"),
            "assert_eq failed",
        );
        // The write was effected through the orchestrator's `set-config` action.
        check(
            (port.observed_action_ids) == (["set-config:auto_approve_ready:true"]),
            "assert_eq failed",
        );
        let commands = store.list_commands().ok_test();
        check(
            (commands[0].command_type()) == ("config.dispatcher_setting_set"),
            "assert_eq failed",
        );
        check((commands[0].status()) == ("completed"), "assert_eq failed");
        // The change audit event was persisted.
        let events = store.list_console_events().ok_test();
        check(
            events
                .iter()
                .any(|event| event.event_type() == &EventType::ConfigDispatcherSettingChanged),
            "assert failed",
        );

        // A second pass skips the already-completed command: no re-write.
        let repeat =
            handle_pending_config_commands(&mut store, "2026-07-11T00:00:02Z", &mut port).ok_test();
        check(repeat.is_empty(), "assert failed");
        check(
            (port.observed_action_ids) == (["set-config:auto_approve_ready:true"]),
            "assert_eq failed",
        );
    }

    #[test]
    fn pending_config_commands_ignore_non_config_commands() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        store.append_command(&approve_command_append()).ok_test();
        let mut port = SimulatedWorkItemActionPort::default();

        let outcomes =
            handle_pending_config_commands(&mut store, "2026-07-11T00:00:01Z", &mut port).ok_test();

        // The non-config command is skipped; the settings port is never called.
        check(outcomes.is_empty(), "assert failed");
        check(port.observed_action_ids.is_empty(), "assert failed");
    }

    #[test]
    fn pending_config_commands_surface_a_malformed_payload_as_application_error() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        store
            .append_command(&config_command_append("not json"))
            .ok_test();
        let mut port = SimulatedWorkItemActionPort::default();

        let outcome = handle_pending_config_commands(&mut store, "2026-07-11T00:00:01Z", &mut port);

        check(
            format!("{outcome:?}").contains("InvalidDispatcherSettingPayload"),
            "assert failed",
        );
        // The malformed command never reached the settings port.
        check(port.observed_action_ids.is_empty(), "assert failed");
    }

    #[test]
    fn config_command_reconstruction_requires_aggregate() {
        let stored_command = StoredCommand::new(
            "cmd_setting".to_owned(),
            "configuration".to_owned(),
            "config.dispatcher_setting_set".to_owned(),
            None,
            "idem_setting".to_owned(),
            "operator".to_owned(),
            "pending".to_owned(),
        );

        let result = config_command_from_stored(&stored_command);

        check(
            format!("{result:?}").contains("cmd_setting"),
            "assert failed",
        );
    }

    #[test]
    fn pending_config_commands_propagate_store_append_errors() {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::ConfigAppendFails);
        let mut port = SimulatedWorkItemActionPort::default();

        let outcome = handle_pending_config_commands(&mut store, "2026-07-11T00:00:03Z", &mut port);

        check(
            format!("{outcome:?}").contains("InvalidSequence"),
            "assert failed",
        );
    }

    fn approve_command_append() -> CommandAppend {
        CommandAppend::new(
            CommandEnvelope::new(
                "cmd_approve".to_owned(),
                CommandType::WorkItemApproveRequested,
                "wi-1".to_owned(),
                "wi-1:work_item.approve_requested".to_owned(),
                "operator".to_owned(),
            ),
            "2026-07-10T00:00:00Z".to_owned(),
            Some("wi-1".to_owned()),
            "corr_cmd_approve".to_owned(),
            "{}".to_owned(),
        )
    }

    fn accept_command_append() -> CommandAppend {
        CommandAppend::new(
            CommandEnvelope::new(
                "cmd_accept".to_owned(),
                CommandType::WorkItemAcceptRequested,
                "wi-1".to_owned(),
                "wi-1:work_item.accept_requested".to_owned(),
                "operator".to_owned(),
            ),
            "2026-07-10T00:00:00Z".to_owned(),
            Some("wi-1".to_owned()),
            "corr_cmd_accept".to_owned(),
            "{}".to_owned(),
        )
    }

    fn reject_command_append(payload_json: &str) -> CommandAppend {
        CommandAppend::new(
            CommandEnvelope::new(
                "cmd_reject".to_owned(),
                CommandType::WorkItemRejectRequested,
                "wi-1".to_owned(),
                "wi-1:work_item.reject_requested".to_owned(),
                "operator".to_owned(),
            ),
            "2026-07-10T00:00:00Z".to_owned(),
            Some("wi-1".to_owned()),
            "corr_cmd_reject".to_owned(),
            payload_json.to_owned(),
        )
    }

    fn set_admission_command_append(payload_json: &str) -> CommandAppend {
        CommandAppend::new(
            CommandEnvelope::new(
                "cmd_set_admission".to_owned(),
                CommandType::WorkItemSetAdmissionRequested,
                "wi-1".to_owned(),
                "wi-1:work_item.set_admission_requested".to_owned(),
                "operator".to_owned(),
            ),
            "2026-07-10T00:00:00Z".to_owned(),
            Some("wi-1".to_owned()),
            "corr_cmd_set_admission".to_owned(),
            payload_json.to_owned(),
        )
    }

    fn set_acceptance_command_append(payload_json: &str) -> CommandAppend {
        CommandAppend::new(
            CommandEnvelope::new(
                "cmd_set_acceptance".to_owned(),
                CommandType::WorkItemSetAcceptanceRequested,
                "wi-1".to_owned(),
                "wi-1:work_item.set_acceptance_requested".to_owned(),
                "operator".to_owned(),
            ),
            "2026-07-10T00:00:00Z".to_owned(),
            Some("wi-1".to_owned()),
            "corr_cmd_set_acceptance".to_owned(),
            payload_json.to_owned(),
        )
    }

    #[test]
    fn pending_work_item_accept_dispatches_through_port() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        store.append_command(&accept_command_append()).ok_test();
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcomes =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port)
                .ok_test();

        check((outcomes.len()) == (1), "assert_eq failed");
        check(
            (outcomes[0].command_status()) == ("completed"),
            "assert_eq failed",
        );
        check(
            (port.observed_action_ids) == (["accept:wi-1"]),
            "assert_eq failed",
        );
    }

    #[test]
    fn pending_work_item_reject_routes_mode_from_payload_through_port() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        store
            .append_command(&reject_command_append(r#"{"mode":"regroom"}"#))
            .ok_test();
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcomes =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port)
                .ok_test();

        check((outcomes.len()) == (1), "assert_eq failed");
        check(
            (outcomes[0].command_status()) == ("completed"),
            "assert_eq failed",
        );
        // The mode extracted from the stored payload lands in the action-id.
        check(
            (port.observed_action_ids) == (["reject:wi-1:regroom"]),
            "assert_eq failed",
        );
    }

    #[test]
    fn pending_work_item_reject_surfaces_invalid_mode_as_application_error() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        store
            .append_command(&reject_command_append(r#"{"mode":"bogus"}"#))
            .ok_test();
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port);

        check(
            format!("{outcome:?}").contains("InvalidRejectMode"),
            "assert failed",
        );
        check(
            (port.observed_action_ids) == ([] as [String; 0]),
            "assert_eq failed",
        );
    }

    #[test]
    fn pending_work_item_set_admission_routes_policy_from_payload_through_port() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        store
            .append_command(&set_admission_command_append(r#"{"policy":"auto"}"#))
            .ok_test();
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcomes =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port)
                .ok_test();

        check((outcomes.len()) == (1), "assert_eq failed");
        check(
            (outcomes[0].command_status()) == ("completed"),
            "assert_eq failed",
        );
        // The policy extracted from the stored payload lands in the action-id.
        check(
            (port.observed_action_ids) == (["set-admission:wi-1:auto"]),
            "assert_eq failed",
        );
    }

    #[test]
    fn pending_work_item_set_admission_surfaces_invalid_policy_as_application_error() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        store
            .append_command(&set_admission_command_append(r#"{"policy":"bogus"}"#))
            .ok_test();
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port);

        check(
            format!("{outcome:?}").contains("InvalidAdmissionPolicy"),
            "assert failed",
        );
        check(
            (port.observed_action_ids) == ([] as [String; 0]),
            "assert_eq failed",
        );
    }

    #[test]
    fn pending_work_item_set_acceptance_routes_policy_from_payload_through_port() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        store
            .append_command(&set_acceptance_command_append(r#"{"policy":"ai-only"}"#))
            .ok_test();
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcomes =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port)
                .ok_test();

        check((outcomes.len()) == (1), "assert_eq failed");
        check(
            (outcomes[0].command_status()) == ("completed"),
            "assert_eq failed",
        );
        // The policy extracted from the stored payload lands in the action-id.
        check(
            (port.observed_action_ids) == (["set-acceptance:wi-1:ai-only"]),
            "assert_eq failed",
        );
    }

    #[test]
    fn control_commands_for_other_items_complete_while_factory_drain_is_executing() {
        let path = std::env::temp_dir().join(format!(
            "livespec-console-overlap-{}-{}.sqlite",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_error| ConsoleRuntimeError::TuiRuntimeFailed)
                .ok_test()
                .as_nanos()
        ));
        let _ignored = fs::remove_file(&path);
        let mut setup_store = SqliteEventStore::open(&path).ok_test();
        seed_running_drain_overlap_commands(&mut setup_store);
        drop(setup_store);

        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let worker_path = path.clone();
        let factory_worker = std::thread::spawn(move || {
            let mut store = SqliteEventStore::open(&worker_path).ok_test();
            let mut port = BlockingFactoryDrainPort {
                started_tx,
                release_rx,
            };
            handle_pending_factory_commands(&mut store, "2026-08-21T00:00:02Z", &mut port)
        });
        started_rx
            .recv()
            .map_err(|_error| ConsoleRuntimeError::TuiRuntimeFailed)
            .ok_test();

        let mut control_store = SqliteEventStore::open(&path).ok_test();
        let mut control_port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());
        let control_outcomes =
            handle_control_commands_for_test(&mut control_store, &mut control_port).ok_test();

        let commands_while_drain_runs = control_store.list_commands().ok_test();
        check((control_outcomes.len()) == (1), "assert_eq failed");
        check(
            (control_outcomes[0].command_status()) == ("completed"),
            "assert_eq failed",
        );
        check(
            (control_port.observed_action_ids) == (["set-acceptance:wi-other:ai-then-human"]),
            "assert_eq failed",
        );
        check(
            (commands_while_drain_runs
                .iter()
                .find(|command| command.command_id() == "cmd_drain")
                .map(StoredCommand::status))
                == (Some("executing")),
            "assert_eq failed",
        );
        check(
            (commands_while_drain_runs
                .iter()
                .find(|command| command.command_id() == "cmd_set_acceptance_other")
                .map(StoredCommand::status))
                == (Some("completed")),
            "assert_eq failed",
        );

        release_tx
            .send(())
            .map_err(|_error| ConsoleRuntimeError::TuiRuntimeFailed)
            .ok_test();
        let factory_outcomes = factory_worker
            .join()
            .map_err(|_error| ConsoleRuntimeError::TuiRuntimeFailed)
            .ok_test()
            .ok_test();
        check((factory_outcomes.len()) == (1), "assert_eq failed");

        let _ignored = fs::remove_file(&path);
    }

    struct BlockingFactoryDrainPort {
        started_tx: std::sync::mpsc::Sender<()>,
        release_rx: std::sync::mpsc::Receiver<()>,
    }

    impl FactoryDrainPort for BlockingFactoryDrainPort {
        fn drain_ready_queue(
            &mut self,
            _request: &FactoryDrainRequest,
        ) -> Result<FactoryDrainPortOutcome, ApplicationError> {
            self.started_tx
                .send(())
                .map_err(|_error| ApplicationError::FactoryDrainPortFailed)
                .ok_test();
            self.release_rx
                .recv()
                .map_err(|_error| ApplicationError::FactoryDrainPortFailed)
                .ok_test();
            Ok(FactoryDrainPortOutcome::completed(1))
        }
    }

    fn seed_running_drain_overlap_commands(store: &mut SqliteEventStore) {
        store
            .append_command(&CommandAppend::new(
                CommandEnvelope::new(
                    "cmd_drain".to_owned(),
                    CommandType::FactoryDrainRequested,
                    "fleet:livespec".to_owned(),
                    "fleet:livespec:factory.drain_requested:budget=1:parallel=1".to_owned(),
                    "operator".to_owned(),
                ),
                "2026-08-21T00:00:00Z".to_owned(),
                Some("fleet:livespec".to_owned()),
                "corr_cmd_drain".to_owned(),
                "{}".to_owned(),
            ))
            .ok_test();
        store
            .append_command(&CommandAppend::new(
                CommandEnvelope::new(
                    "cmd_set_acceptance_other".to_owned(),
                    CommandType::WorkItemSetAcceptanceRequested,
                    "wi-other".to_owned(),
                    "wi-other:work_item.set_acceptance_requested".to_owned(),
                    "operator".to_owned(),
                ),
                "2026-08-21T00:00:01Z".to_owned(),
                Some("wi-other".to_owned()),
                "corr_cmd_set_acceptance_other".to_owned(),
                r#"{"policy":"ai-then-human"}"#.to_owned(),
            ))
            .ok_test();
        append_ready_work_item(store, "2026-08-21T00:00:01Z");
    }

    fn handle_control_commands_for_test(
        store: &mut SqliteEventStore,
        port: &mut SimulatedWorkItemActionPort,
    ) -> ConsoleRuntimeResult<Vec<PendingCommandOutcome>> {
        handle_pending_control_commands(store, "2026-08-21T00:00:03Z", port)
    }

    #[test]
    fn control_commands_do_not_overtake_older_same_item_factory_commands() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        store
            .append_command(&CommandAppend::new(
                CommandEnvelope::new(
                    "cmd_dispatch_item".to_owned(),
                    CommandType::FactoryDispatchItemRequested,
                    "wi-1".to_owned(),
                    "wi-1:factory.dispatch_item_requested".to_owned(),
                    "operator".to_owned(),
                ),
                "2026-08-21T00:00:00Z".to_owned(),
                Some("wi-1".to_owned()),
                "corr_cmd_dispatch_item".to_owned(),
                "{}".to_owned(),
            ))
            .ok_test();
        store
            .append_command(&CommandAppend::new(
                CommandEnvelope::new(
                    "cmd_set_acceptance".to_owned(),
                    CommandType::WorkItemSetAcceptanceRequested,
                    "wi-1".to_owned(),
                    "wi-1:work_item.set_acceptance_requested".to_owned(),
                    "operator".to_owned(),
                ),
                "2026-08-21T00:00:01Z".to_owned(),
                Some("wi-1".to_owned()),
                "corr_cmd_set_acceptance".to_owned(),
                r#"{"policy":"ai-then-human"}"#.to_owned(),
            ))
            .ok_test();
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcomes =
            handle_pending_control_commands(&mut store, "2026-08-21T00:00:01Z", &mut port)
                .ok_test();

        check(outcomes.is_empty(), "assert failed");
        check(port.observed_action_ids.is_empty(), "assert failed");
        check(
            (store
                .list_commands()
                .ok_test()
                .iter()
                .map(StoredCommand::status)
                .collect::<Vec<_>>())
                == (["pending", "pending"]),
            "assert_eq failed",
        );
    }

    #[test]
    fn older_factory_command_guard_only_blocks_same_aggregate_control_commands() {
        let older_factory = StoredCommand::new(
            "cmd_dispatch_item".to_owned(),
            "factory".to_owned(),
            "factory.dispatch_item_requested".to_owned(),
            Some("wi-1".to_owned()),
            "wi-1:factory.dispatch_item_requested".to_owned(),
            "operator".to_owned(),
            "executing".to_owned(),
        );
        let same_item_control = StoredCommand::new(
            "cmd_set_acceptance".to_owned(),
            "work_item".to_owned(),
            "work_item.set_acceptance_requested".to_owned(),
            Some("wi-1".to_owned()),
            "wi-1:work_item.set_acceptance_requested".to_owned(),
            "operator".to_owned(),
            "pending".to_owned(),
        );
        let other_item_control = StoredCommand::new(
            "cmd_set_acceptance_other".to_owned(),
            "work_item".to_owned(),
            "work_item.set_acceptance_requested".to_owned(),
            Some("wi-2".to_owned()),
            "wi-2:work_item.set_acceptance_requested".to_owned(),
            "operator".to_owned(),
            "pending".to_owned(),
        );
        let no_aggregate_control = StoredCommand::new(
            "cmd_no_aggregate".to_owned(),
            "work_item".to_owned(),
            "work_item.set_acceptance_requested".to_owned(),
            None,
            "missing:work_item.set_acceptance_requested".to_owned(),
            "operator".to_owned(),
            "pending".to_owned(),
        );

        check(
            older_factory_command_blocks_control_command(
                &same_item_control,
                std::slice::from_ref(&older_factory),
            ),
            "assert failed",
        );
        check(
            !older_factory_command_blocks_control_command(
                &other_item_control,
                std::slice::from_ref(&older_factory),
            ),
            "assert failed",
        );
        check(
            !older_factory_command_blocks_control_command(&no_aggregate_control, &[older_factory]),
            "assert failed",
        );
    }

    #[test]
    fn pending_work_item_set_acceptance_surfaces_invalid_policy_as_application_error() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        store
            .append_command(&set_acceptance_command_append(r#"{"policy":"bogus"}"#))
            .ok_test();
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port);

        check(
            format!("{outcome:?}").contains("InvalidAcceptancePolicy"),
            "assert failed",
        );
        check(
            (port.observed_action_ids) == ([] as [String; 0]),
            "assert_eq failed",
        );
    }

    fn resolve_blocked_command_append(payload_json: &str) -> CommandAppend {
        CommandAppend::new(
            CommandEnvelope::new(
                "cmd_resolve_blocked".to_owned(),
                CommandType::WorkItemResolveBlockedRequested,
                "wi-1".to_owned(),
                "wi-1:work_item.resolve_blocked_requested".to_owned(),
                "operator".to_owned(),
            ),
            "2026-07-10T00:00:00Z".to_owned(),
            Some("wi-1".to_owned()),
            "corr_cmd_resolve_blocked".to_owned(),
            payload_json.to_owned(),
        )
    }

    #[test]
    fn pending_work_item_resolve_blocked_routes_target_from_payload_through_port() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        store
            .append_command(&resolve_blocked_command_append(
                r#"{"target_status":"ready"}"#,
            ))
            .ok_test();
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcomes =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port)
                .ok_test();

        check((outcomes.len()) == (1), "assert_eq failed");
        check(
            (outcomes[0].command_status()) == ("completed"),
            "assert_eq failed",
        );
        // The target extracted from the stored payload lands in the action-id.
        check(
            (port.observed_action_ids) == (["resolve-blocked:wi-1:ready"]),
            "assert_eq failed",
        );
    }

    #[test]
    fn pending_work_item_resolve_blocked_surfaces_invalid_target_as_application_error() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        store
            .append_command(&resolve_blocked_command_append(
                r#"{"target_status":"active"}"#,
            ))
            .ok_test();
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port);

        check(
            format!("{outcome:?}").contains("InvalidResolveBlockedTarget"),
            "assert failed",
        );
        check(
            (port.observed_action_ids) == ([] as [String; 0]),
            "assert_eq failed",
        );
    }

    fn move_command_append(payload_json: &str) -> CommandAppend {
        CommandAppend::new(
            CommandEnvelope::new(
                "cmd_move".to_owned(),
                CommandType::WorkItemMoveRequested,
                "wi-1".to_owned(),
                "wi-1:work_item.move_requested".to_owned(),
                "operator".to_owned(),
            ),
            "2026-07-10T00:00:00Z".to_owned(),
            Some("wi-1".to_owned()),
            "corr_cmd_move".to_owned(),
            payload_json.to_owned(),
        )
    }

    #[test]
    fn pending_work_item_move_routes_target_from_payload_through_port() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        store
            .append_command(&move_command_append(r#"{"target_status":"blocked"}"#))
            .ok_test();
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcomes =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port)
                .ok_test();

        check((outcomes.len()) == (1), "assert_eq failed");
        check(
            (outcomes[0].command_status()) == ("completed"),
            "assert_eq failed",
        );
        // The target extracted from the stored payload lands in the move action-id.
        check(
            (port.observed_action_ids) == (["move:wi-1:blocked"]),
            "assert_eq failed",
        );
    }

    fn set_override_command_append(payload_json: &str) -> CommandAppend {
        CommandAppend::new(
            CommandEnvelope::new(
                "cmd_override".to_owned(),
                CommandType::WorkItemSetDispatcherOverrideRequested,
                "wi-1".to_owned(),
                "wi-1:work_item.set_dispatcher_override_requested".to_owned(),
                "operator".to_owned(),
            ),
            "2026-07-10T00:00:00Z".to_owned(),
            Some("wi-1".to_owned()),
            "corr_cmd_override".to_owned(),
            payload_json.to_owned(),
        )
    }

    #[test]
    fn pending_work_item_set_dispatcher_override_routes_setting_and_value_through_port() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        // A null value clears the per-item override back to inherit-global.
        store
            .append_command(&set_override_command_append(
                r#"{"setting":"review_fix_cap","value":null}"#,
            ))
            .ok_test();
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcomes =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port)
                .ok_test();

        check((outcomes.len()) == (1), "assert_eq failed");
        check(
            (outcomes[0].command_status()) == ("completed"),
            "assert_eq failed",
        );
        check(
            (port.observed_action_ids) == (["set-review-fix-cap:wi-1:clear"]),
            "assert_eq failed",
        );
    }

    #[test]
    fn pending_work_item_set_dispatcher_override_surfaces_bad_setting_as_application_error() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        store
            .append_command(&set_override_command_append(
                r#"{"setting":"wip_cap","value":5}"#,
            ))
            .ok_test();
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port);

        check(
            format!("{outcome:?}").contains("InvalidDispatcherOverrideSetting"),
            "assert failed",
        );
        check(
            (port.observed_action_ids) == ([] as [String; 0]),
            "assert_eq failed",
        );
    }

    #[test]
    fn pending_work_item_approve_dispatches_through_port_and_skips_others() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        store.append_command(&approve_command_append()).ok_test();
        // A pending factory command must be skipped by the work-item handler.
        store
            .append_command(&CommandAppend::new(
                CommandEnvelope::new(
                    "cmd_drain".to_owned(),
                    CommandType::FactoryDrainRequested,
                    "fleet:livespec".to_owned(),
                    "fleet:livespec:factory.drain_requested:budget=1:parallel=1".to_owned(),
                    "operator".to_owned(),
                ),
                "2026-07-10T00:00:00Z".to_owned(),
                Some("fleet:livespec".to_owned()),
                "corr_cmd_drain".to_owned(),
                "{}".to_owned(),
            ))
            .ok_test();
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcomes =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port)
                .ok_test();

        check((outcomes.len()) == (1), "assert_eq failed");
        check(
            (outcomes[0].command_status()) == ("completed"),
            "assert_eq failed",
        );
        check(
            (outcomes[0].appended_event_count()) == (3),
            "assert_eq failed",
        );
        check(
            (port.observed_action_ids) == (["approve:wi-1"]),
            "assert_eq failed",
        );
        // The skipped factory command produces no events, so the store carries
        // exactly the shared work_item outcome family for the approve.
        let events = store.list_console_events().ok_test();
        check(
            (events
                .iter()
                .map(ConsoleEvent::event_type)
                .collect::<Vec<_>>())
                == ([
                    &EventType::CommandAccepted,
                    &EventType::WorkItemActionStarted,
                    &EventType::WorkItemActionCompleted,
                ]),
            "assert_eq failed",
        );

        // Second pass: the approve command is now non-pending and skipped, and
        // the factory command is still not a work-item command, so nothing runs.
        let second =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:02Z", &mut port)
                .ok_test();
        check(second.is_empty(), "assert failed");
    }

    #[test]
    fn pending_work_item_approve_records_not_wired_without_fabricating_start() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        store.append_command(&approve_command_append()).ok_test();
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::not_wired());

        let outcomes =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port)
                .ok_test();

        check(
            (outcomes[0].command_status()) == ("not_wired"),
            "assert_eq failed",
        );
        check(
            (store.list_commands().ok_test()[0].status()) == ("not_wired"),
            "assert_eq failed",
        );
        check(
            (store
                .list_console_events()
                .ok_test()
                .iter()
                .map(ConsoleEvent::event_type)
                .collect::<Vec<_>>())
                == ([
                    &EventType::CommandAccepted,
                    &EventType::WorkItemActionNotWired,
                ]),
            "assert_eq failed",
        );
    }

    #[test]
    fn pending_work_item_commands_propagate_store_errors() {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::WorkItemAppendFails);
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:03Z", &mut port);

        check(
            format!("{outcome:?}").contains("InvalidSequence"),
            "assert failed",
        );
    }

    #[test]
    fn simulated_factory_drain_port_rejects_unbounded_request() {
        let mut port = SimulatedFactoryDrainPort;
        let request = FactoryDrainRequest::new("fleet:livespec".to_owned(), 0, 1);

        let outcome = port.drain_ready_queue(&request);

        check(
            (outcome) == (Err(ApplicationError::FactoryDrainPortFailed)),
            "assert_eq failed",
        );

        let request = FactoryDrainRequest::new("fleet:livespec".to_owned(), 1, 0);

        let outcome = port.drain_ready_queue(&request);

        check(
            (outcome) == (Err(ApplicationError::FactoryDrainPortFailed)),
            "assert_eq failed",
        );
    }

    #[test]
    fn backing_cli_resolution_uses_explicit_root_and_program_overrides() {
        let temp = resolver_temp_root("explicit");
        let explicit = resolver_plugin_root(&temp, "explicit-plugin");
        let repo = resolver_plugin_root(&temp, "repo-plugin");
        let mut env = resolver_empty_env();
        env.insert(
            "LIVESPEC_CONSOLE_ORCHESTRATOR_PLUGIN_ROOT".to_owned(),
            explicit.display().to_string(),
        );
        env.insert(
            "LIVESPEC_CONSOLE_LIST_WORK_ITEMS_PROGRAM".to_owned(),
            "/custom/list".to_owned(),
        );
        env.insert(
            "LIVESPEC_CONSOLE_LIVESPEC_PROGRAM".to_owned(),
            "/custom/livespec".to_owned(),
        );
        env.insert(
            "LIVESPEC_CONSOLE_FABRO_PROGRAM".to_owned(),
            "/custom/fabro".to_owned(),
        );
        env.insert(
            "LIVESPEC_CONSOLE_DRAIN_PROGRAM".to_owned(),
            "/custom/dispatcher".to_owned(),
        );
        env.insert(
            "LIVESPEC_CONSOLE_DRIVE_PROGRAM".to_owned(),
            "/custom/drive".to_owned(),
        );
        env.insert(
            "LIVESPEC_CONSOLE_NEEDS_ATTENTION_PROGRAM".to_owned(),
            "/custom/needs".to_owned(),
        );
        env.insert(
            "LIVESPEC_CONSOLE_GH_PROGRAM".to_owned(),
            "/custom/gh".to_owned(),
        );

        let resolution = BackingCliResolution::resolve(&resolver_inputs(env, repo, None)).ok_test();

        check(
            (resolution.programs().list_work_items()) == ("/custom/list"),
            "assert_eq failed",
        );
        check(
            (resolution.programs().livespec().program()) == ("/custom/livespec"),
            "assert_eq failed",
        );
        check(
            (resolution.programs().livespec().args()) == (["next".to_owned(), "--json".to_owned()]),
            "assert_eq failed",
        );
        check(
            (resolution.programs().fabro()) == ("/custom/fabro"),
            "assert_eq failed",
        );
        check(
            (resolution.programs().dispatcher()) == ("/custom/dispatcher"),
            "assert_eq failed",
        );
        check(
            (resolution.programs().drive()) == ("/custom/drive"),
            "assert_eq failed",
        );
        check(
            (resolution.programs().needs_attention()) == ("/custom/needs"),
            "assert_eq failed",
        );
        check(
            (resolution.programs().github()) == ("/custom/gh"),
            "assert_eq failed",
        );
    }

    #[test]
    fn backing_cli_resolution_uses_selected_repo_checkout() {
        let temp = resolver_temp_root("repo");
        let repo = resolver_plugin_root(&temp, "repo-plugin");

        let resolution = BackingCliResolution::resolve(&resolver_inputs(
            resolver_empty_env(),
            repo.clone(),
            None,
        ))
        .ok_test();

        let bin = repo.join(".claude-plugin/scripts/bin");
        check(
            (resolution.selected_repo_path()) == (repo.as_path()),
            "assert_eq failed",
        );
        check(
            (resolution.programs().list_work_items())
                == (bin.join("list_work_items.py").display().to_string()),
            "assert_eq failed",
        );
        // The livespec source observes the SPEC-side `livespec next` action, NOT
        // the orchestrator plugin's impl-side `next.py` (which ranks work-items).
        check(
            (resolution.programs().livespec().program()) == ("livespec"),
            "assert_eq failed",
        );
        check(
            (resolution.programs().livespec().args()) == (["next".to_owned(), "--json".to_owned()]),
            "assert_eq failed",
        );
        // The github source uses the `gh` CLI, overridable via
        // LIVESPEC_CONSOLE_GH_PROGRAM.
        check(
            (resolution.programs().github()) == ("gh"),
            "assert_eq failed",
        );
        check(
            (resolution.programs().drive()) == (bin.join("drive.py").display().to_string()),
            "assert_eq failed",
        );
        check(
            (resolution.programs().dispatcher())
                == (bin.join("dispatcher.py").display().to_string()),
            "assert_eq failed",
        );
        check(
            (resolution.programs().needs_attention())
                == (bin.join("needs_attention.py").display().to_string()),
            "assert_eq failed",
        );
    }

    #[test]
    fn backing_cli_resolution_accepts_flattened_governed_checkout() {
        // The Claude plugin installer FLATTENS `.claude-plugin/scripts/` to
        // `scripts/`, so a resolved checkout may carry the bin scripts at
        // `<root>/scripts/bin` rather than `<root>/.claude-plugin/scripts/bin`.
        // The resolver MUST detect, validate, and build program paths against
        // the flattened layout too — otherwise the cockpit launch dies here.
        let temp = resolver_temp_root("repo-flattened");
        let repo = resolver_flattened_plugin_root(&temp, "repo-plugin-flattened");

        let resolution = BackingCliResolution::resolve(&resolver_inputs(
            resolver_empty_env(),
            repo.clone(),
            None,
        ))
        .ok_test();

        let bin = repo.join("scripts/bin");
        check(
            (resolution.selected_repo_path()) == (repo.as_path()),
            "assert_eq failed",
        );
        check(
            (resolution.programs().list_work_items())
                == (bin.join("list_work_items.py").display().to_string()),
            "assert_eq failed",
        );
        // Spec-side `livespec next`, not the orchestrator's impl-side `next.py`.
        check(
            (resolution.programs().livespec().program()) == ("livespec"),
            "assert_eq failed",
        );
        check(
            (resolution.programs().livespec().args()) == (["next".to_owned(), "--json".to_owned()]),
            "assert_eq failed",
        );
        check(
            (resolution.programs().drive()) == (bin.join("drive.py").display().to_string()),
            "assert_eq failed",
        );
        check(
            (resolution.programs().dispatcher())
                == (bin.join("dispatcher.py").display().to_string()),
            "assert_eq failed",
        );
        check(
            (resolution.programs().needs_attention())
                == (bin.join("needs_attention.py").display().to_string()),
            "assert_eq failed",
        );
    }

    #[test]
    fn backing_cli_resolution_accepts_flattened_installed_cache() {
        // The real installed orchestrator plugin cache carries the FLATTENED
        // `scripts/bin` layout, so the installed-cache resolution rung must
        // accept it exactly as a governed checkout would.
        let temp = resolver_temp_root("cache-flattened");
        let repo = temp.join("repo-without-plugin");
        fs::create_dir_all(&repo).ok_test();
        let home = temp.join("home");
        let cached = resolver_flattened_plugin_root(&temp, "cached-plugin-flattened");
        let cache_dir = home.join(".claude/plugins");
        fs::create_dir_all(&cache_dir).ok_test();
        let cache = serde_json::json!({
            "plugins": {
                "livespec-orchestrator-beads-fabro@livespec-orchestrator-beads-fabro": [
                    {"installPath": cached.display().to_string()}
                ]
            }
        });
        fs::write(cache_dir.join("installed_plugins.json"), cache.to_string()).ok_test();

        let resolution =
            BackingCliResolution::resolve(&resolver_inputs(resolver_empty_env(), repo, Some(home)))
                .ok_test();

        let bin = cached.join("scripts/bin");
        check(
            (resolution.programs().list_work_items())
                == (bin.join("list_work_items.py").display().to_string()),
            "assert_eq failed",
        );
        check(
            (resolution.programs().needs_attention())
                == (bin.join("needs_attention.py").display().to_string()),
            "assert_eq failed",
        );
    }

    #[test]
    fn backing_cli_resolution_fails_loudly_when_neither_layout_present() {
        // A plugin root that carries NEITHER the source `.claude-plugin/scripts/bin`
        // NOR the flattened `scripts/bin` directory is malformed; the resolver
        // must fail loudly and name both accepted layouts rather than silently
        // degrading to bare-name defaults.
        let temp = resolver_temp_root("neither-layout");
        let explicit_root = temp.join("explicit-plugin-no-bin");
        fs::create_dir_all(&explicit_root).ok_test();
        let mut env = resolver_empty_env();
        env.insert(
            "LIVESPEC_CONSOLE_ORCHESTRATOR_PLUGIN_ROOT".to_owned(),
            explicit_root.display().to_string(),
        );
        check(
            format!(
                "{:?}",
                BackingCliResolution::resolve(&resolver_inputs(env, temp, None))
            )
            .contains("neither .claude-plugin/scripts/bin nor scripts/bin"),
            "assert failed",
        );
    }

    #[test]
    fn backing_cli_resolution_uses_installed_plugin_cache() {
        let temp = resolver_temp_root("cache");
        let repo = temp.join("repo-without-plugin");
        fs::create_dir_all(&repo).ok_test();
        let home = temp.join("home");
        let cached = resolver_plugin_root(&temp, "cached-plugin");
        let cache_dir = home.join(".claude/plugins");
        fs::create_dir_all(&cache_dir).ok_test();
        let cache = serde_json::json!({
            "plugins": {
                "some-other-plugin@github": [
                    {"installPath": temp.join("other").display().to_string()}
                ],
                "livespec-orchestrator-beads-fabro@github": [
                    {"installPath": cached.display().to_string()}
                ]
            }
        });
        fs::write(cache_dir.join("installed_plugins.json"), cache.to_string()).ok_test();

        let resolution =
            BackingCliResolution::resolve(&resolver_inputs(resolver_empty_env(), repo, Some(home)))
                .ok_test();

        let bin = cached.join(".claude-plugin/scripts/bin");
        check(
            (resolution.programs().list_work_items())
                == (bin.join("list_work_items.py").display().to_string()),
            "assert_eq failed",
        );
        check(
            (resolution.programs().needs_attention())
                == (bin.join("needs_attention.py").display().to_string()),
            "assert_eq failed",
        );
    }

    #[test]
    fn backing_cli_resolution_uses_newest_applicable_installed_plugin_record() {
        // Claude's plugin cache is append-only for updates: one plugin key may
        // hold many records with mixed versions. The console must not take the
        // stale first record when a newer record applies to the selected repo.
        let temp = resolver_temp_root("cache-newest-applicable");
        let repo = temp.join("repo-without-plugin");
        fs::create_dir_all(&repo).ok_test();
        let home = temp.join("home");
        let stale = resolver_plugin_root(&temp, "stale-plugin");
        let other_project = resolver_plugin_root(&temp, "other-project-plugin");
        let newest = resolver_plugin_root(&temp, "newest-plugin");
        let cache_dir = home.join(".claude/plugins");
        fs::create_dir_all(&cache_dir).ok_test();
        let cache = serde_json::json!({
            "plugins": {
                "livespec-orchestrator-beads-fabro@livespec-orchestrator-beads-fabro": [
                    {
                        "projectPath": repo.display().to_string(),
                        "installPath": stale.display().to_string(),
                        "version": "stale-first"
                    },
                    {
                        "projectPath": temp.join("other-repo").display().to_string(),
                        "installPath": other_project.display().to_string(),
                        "version": "other-project"
                    },
                    {
                        "projectPath": repo.display().to_string(),
                        "installPath": newest.display().to_string(),
                        "version": "newest-applicable"
                    }
                ]
            }
        });
        fs::write(cache_dir.join("installed_plugins.json"), cache.to_string()).ok_test();

        let resolution =
            BackingCliResolution::resolve(&resolver_inputs(resolver_empty_env(), repo, Some(home)))
                .ok_test();

        let bin = newest.join(".claude-plugin/scripts/bin");
        check(
            (resolution.programs().dispatcher())
                == (bin.join("dispatcher.py").display().to_string()),
            "assert_eq failed",
        );
        check(
            (resolution.plugin_resolution())
                == (&PluginResolution::resolved(
                    "installed Claude plugin cache".to_owned(),
                    newest,
                    Some("newest-applicable".to_owned()),
                )),
            "assert_eq failed",
        );
        check(
            (resolution.plugin_resolution().source()) == ("installed Claude plugin cache"),
            "assert_eq failed",
        );
    }

    #[test]
    fn backing_cli_resolution_is_deterministic_for_a_heterogeneous_cache() {
        let temp = resolver_temp_root("cache-deterministic");
        let repo = temp.join("repo-without-plugin");
        fs::create_dir_all(&repo).ok_test();
        let home = temp.join("home");
        let first = resolver_plugin_root(&temp, "first-plugin");
        let second = resolver_plugin_root(&temp, "second-plugin");
        let cache_dir = home.join(".claude/plugins");
        fs::create_dir_all(&cache_dir).ok_test();
        let cache = serde_json::json!({
            "plugins": {
                "livespec-orchestrator-beads-fabro@livespec-orchestrator-beads-fabro": [
                    {
                        "projectPath": repo.display().to_string(),
                        "installPath": first.display().to_string(),
                        "version": "build-a"
                    },
                    {
                        "projectPath": repo.display().to_string(),
                        "installPath": second.display().to_string(),
                        "version": "build-b"
                    }
                ]
            }
        });
        fs::write(cache_dir.join("installed_plugins.json"), cache.to_string()).ok_test();

        let first_resolution = BackingCliResolution::resolve(&resolver_inputs(
            resolver_empty_env(),
            repo.clone(),
            Some(home.clone()),
        ))
        .ok_test();
        let second_resolution =
            BackingCliResolution::resolve(&resolver_inputs(resolver_empty_env(), repo, Some(home)))
                .ok_test();

        check(
            (first_resolution.plugin_resolution()) == (second_resolution.plugin_resolution()),
            "assert_eq failed",
        );
        check(
            (first_resolution.plugin_resolution().version()) == (Some("build-b")),
            "assert_eq failed",
        );
    }

    #[test]
    fn backing_cli_resolution_degrades_to_defaults_when_plugin_absent() {
        let temp = resolver_temp_root("absent");
        let repo = temp.join("repo-without-plugin");
        let home = temp.join("home");
        let cache_dir = home.join(".claude/plugins");
        fs::create_dir_all(&repo).ok_test();
        fs::create_dir_all(&cache_dir).ok_test();
        fs::write(cache_dir.join("installed_plugins.json"), "{}").ok_test();

        let resolution = BackingCliResolution::resolve(&resolver_inputs(
            resolver_empty_env(),
            repo.clone(),
            Some(home),
        ))
        .ok_test();

        check(
            (resolution.selected_repo_path()) == (repo.as_path()),
            "assert_eq failed",
        );
        check(
            (resolution.programs().list_work_items()) == ("list-work-items"),
            "assert_eq failed",
        );
        check(
            (resolution.programs().needs_attention()) == ("needs-attention"),
            "assert_eq failed",
        );
        check(
            (resolution.programs().dispatcher()) == ("livespec-dispatcher-drain"),
            "assert_eq failed",
        );
        check(
            (resolution.programs().drive()) == ("livespec-orchestrator-drive"),
            "assert_eq failed",
        );
        // No `fabro` under the injected (empty) home, so the bare default is
        // kept — resolution never touches the ambient filesystem.
        check(
            (resolution.programs().fabro()) == ("fabro"),
            "assert_eq failed",
        );
        check(
            (resolution.programs().github()) == ("gh"),
            "assert_eq failed",
        );
        check(
            (resolution.programs().livespec().program()) == ("livespec"),
            "assert_eq failed",
        );
        check(
            (resolution.programs().livespec().args()) == (["next".to_owned(), "--json".to_owned()]),
            "assert_eq failed",
        );
    }

    #[test]
    fn backing_cli_resolution_resolves_fabro_under_the_injected_home() {
        // A `fabro` binary at `~/.local/bin/fabro` resolves to its absolute
        // path so it spawns under the credential wrapper's scrubbed PATH, which
        // does not include `~/.local/bin`. Resolution reads ONLY the injected
        // home, so this is hermetic.
        let temp = resolver_temp_root("fabro-home");
        let repo = temp.join("repo-without-plugin");
        let home = temp.join("home");
        let fabro_dir = home.join(".local/bin");
        fs::create_dir_all(&repo).ok_test();
        fs::create_dir_all(&fabro_dir).ok_test();
        let fabro = fabro_dir.join("fabro");
        fs::write(&fabro, "#!/usr/bin/env bash\n").ok_test();

        let resolution =
            BackingCliResolution::resolve(&resolver_inputs(resolver_empty_env(), repo, Some(home)))
                .ok_test();

        check(
            (resolution.programs().fabro()) == (fabro.display().to_string()),
            "assert_eq failed",
        );
    }

    #[test]
    fn backing_cli_resolution_fabro_env_override_wins_over_home_resolution() {
        // An explicit LIVESPEC_CONSOLE_FABRO_PROGRAM override still wins over the
        // home-relative absolute resolution.
        let temp = resolver_temp_root("fabro-override");
        let repo = temp.join("repo-without-plugin");
        let home = temp.join("home");
        let fabro_dir = home.join(".local/bin");
        fs::create_dir_all(&repo).ok_test();
        fs::create_dir_all(&fabro_dir).ok_test();
        fs::write(fabro_dir.join("fabro"), "#!/usr/bin/env bash\n").ok_test();
        let mut env = resolver_empty_env();
        env.insert(
            "LIVESPEC_CONSOLE_FABRO_PROGRAM".to_owned(),
            "/custom/fabro".to_owned(),
        );

        let resolution =
            BackingCliResolution::resolve(&resolver_inputs(env, repo, Some(home))).ok_test();

        check(
            (resolution.programs().fabro()) == ("/custom/fabro"),
            "assert_eq failed",
        );
    }

    #[test]
    fn dispatcher_journal_path_is_absolute_under_the_selected_repo() {
        // The dispatch source reads an ABSOLUTE journal path under the SELECTED
        // repo, not a working-directory-relative path, so it observes the right
        // tenant's journal regardless of the process working directory.
        let temp = resolver_temp_root("journal");
        let repo = temp.join("selected-repo");
        fs::create_dir_all(&repo).ok_test();

        let resolution = BackingCliResolution::resolve(&resolver_inputs(
            resolver_empty_env(),
            repo.clone(),
            None,
        ))
        .ok_test();

        let journal = resolution.dispatcher_journal_path();
        check(
            (journal)
                == (repo
                    .join("tmp/fabro-dispatch-journal.jsonl")
                    .display()
                    .to_string()),
            "assert_eq failed",
        );
        check(Path::new(&journal).is_absolute(), "assert failed");
    }

    #[test]
    fn backing_cli_resolution_degrades_to_defaults_when_cache_file_absent() {
        // home_dir is present but `~/.claude/plugins/installed_plugins.json`
        // does not exist. The resolver must read the unreadable-cache case as
        // "no installed plugin" and degrade to bare-name program defaults,
        // deterministically — independent of whether the host running the test
        // happens to carry a real installed cache.
        let temp = resolver_temp_root("cache-file-absent");
        let repo = temp.join("repo-without-plugin");
        let home = temp.join("home-without-cache-file");
        fs::create_dir_all(&repo).ok_test();
        // Intentionally do NOT create home/.claude/plugins/installed_plugins.json.

        let resolution = BackingCliResolution::resolve(&resolver_inputs(
            resolver_empty_env(),
            repo.clone(),
            Some(home),
        ))
        .ok_test();

        check(
            (resolution.selected_repo_path()) == (repo.as_path()),
            "assert_eq failed",
        );
        check(
            (resolution.programs().list_work_items()) == ("list-work-items"),
            "assert_eq failed",
        );
        check(
            (resolution.programs().needs_attention()) == ("needs-attention"),
            "assert_eq failed",
        );
    }

    #[test]
    fn backing_cli_resolution_ignores_cache_without_orchestrator_plugin() {
        let temp = resolver_temp_root("other-cache-only");
        let repo = temp.join("repo-without-plugin");
        let home = temp.join("home");
        let cache_dir = home.join(".claude/plugins");
        fs::create_dir_all(&repo).ok_test();
        fs::create_dir_all(&cache_dir).ok_test();
        let cache = serde_json::json!({
            "plugins": {
                "some-other-plugin@github": [
                    {"installPath": temp.join("other").display().to_string()}
                ]
            }
        });
        fs::write(cache_dir.join("installed_plugins.json"), cache.to_string()).ok_test();

        let resolution =
            BackingCliResolution::resolve(&resolver_inputs(resolver_empty_env(), repo, Some(home)))
                .ok_test();

        check(
            (resolution.programs().list_work_items()) == ("list-work-items"),
            "assert_eq failed",
        );
    }

    #[test]
    fn backing_cli_resolution_returns_selected_repo_path_override() {
        let temp = resolver_temp_root("repo-path-env");
        let current_dir = temp.join("current");
        let selected = temp.join("selected");
        fs::create_dir_all(&current_dir).ok_test();
        fs::create_dir_all(&selected).ok_test();
        let mut env = resolver_empty_env();
        env.insert(
            "LIVESPEC_CONSOLE_REPO_PATH".to_owned(),
            selected.display().to_string(),
        );

        let resolution =
            BackingCliResolution::resolve(&resolver_inputs(env, current_dir, None)).ok_test();

        check(
            (resolution.selected_repo_path()) == (selected.as_path()),
            "assert_eq failed",
        );
    }

    #[test]
    fn backing_cli_resolution_drive_repo_arg_is_resolved_path_not_id() {
        let temp = resolver_temp_root("drive-repo-arg");
        let selected = temp.join("selected-repo");
        fs::create_dir_all(&selected).ok_test();
        let mut env = resolver_empty_env();
        env.insert(
            "LIVESPEC_CONSOLE_REPO_PATH".to_owned(),
            selected.display().to_string(),
        );

        let resolution = BackingCliResolution::resolve(&resolver_inputs(env, temp, None)).ok_test();

        // The `drive --repo` argument the console hands the orchestrator MUST be
        // the resolved repo filesystem PATH, not the repo id: the orchestrator's
        // drive.py does `Path(repo_arg)` and errors `--repo does not exist: <id>`
        // when handed the id. So it must equal the resolved path, must not equal
        // the repo id, and must name an existing directory.
        let drive_repo_arg = resolution.drive_repo_arg();
        check(
            (drive_repo_arg) == (selected.display().to_string()),
            "assert_eq failed",
        );
        check(
            (drive_repo_arg) != ("livespec-console-beads-fabro"),
            "assert_ne failed",
        );
        check(
            std::path::Path::new(&drive_repo_arg).is_dir(),
            "assert failed",
        );
    }

    #[test]
    fn backing_cli_resolution_fails_loudly_for_malformed_roots_and_cache() {
        let temp = resolver_temp_root("malformed");
        let explicit_root = temp.join("explicit-plugin");
        fs::create_dir_all(explicit_root.join(".claude-plugin/scripts/bin")).ok_test();
        let mut env = resolver_empty_env();
        env.insert(
            "LIVESPEC_CONSOLE_ORCHESTRATOR_PLUGIN_ROOT".to_owned(),
            explicit_root.display().to_string(),
        );
        check(
            format!(
                "{:?}",
                BackingCliResolution::resolve(&resolver_inputs(env, temp.clone(), None))
            )
            .contains("orchestrator plugin root"),
            "assert failed",
        );

        let repo_root = temp.join("repo-plugin");
        fs::create_dir_all(repo_root.join(".claude-plugin/scripts/bin")).ok_test();
        check(
            format!(
                "{:?}",
                BackingCliResolution::resolve(&resolver_inputs(
                    resolver_empty_env(),
                    repo_root,
                    None
                ))
            )
            .contains("orchestrator plugin root"),
            "assert failed",
        );

        let home = temp.join("home-invalid-json");
        let cache_dir = home.join(".claude/plugins");
        fs::create_dir_all(&cache_dir).ok_test();
        fs::write(cache_dir.join("installed_plugins.json"), "not json").ok_test();
        check(
            format!(
                "{:?}",
                BackingCliResolution::resolve(&resolver_inputs(
                    resolver_empty_env(),
                    temp.join("repo-without-plugin"),
                    Some(home),
                ))
            )
            .contains("invalid Claude plugin cache"),
            "assert failed",
        );
    }

    #[test]
    fn backing_cli_resolution_fails_loudly_for_cached_install_without_path_or_scripts() {
        let temp = resolver_temp_root("cache-invalid");
        let repo = temp.join("repo-without-plugin");
        fs::create_dir_all(&repo).ok_test();
        let home = temp.join("home-missing-install-path");
        let cache_dir = home.join(".claude/plugins");
        fs::create_dir_all(&cache_dir).ok_test();
        let missing_install_path = serde_json::json!({
            "plugins": {
                "livespec-orchestrator-beads-fabro@github": [{}]
            }
        });
        check(
            fs::write(
                cache_dir.join("installed_plugins.json"),
                missing_install_path.to_string(),
            )
            .is_ok(),
            "assert failed",
        );
        check(
            format!(
                "{:?}",
                BackingCliResolution::resolve(&resolver_inputs(
                    resolver_empty_env(),
                    repo.clone(),
                    Some(home),
                ))
            )
            .contains("has no installPath"),
            "assert failed",
        );

        let home = temp.join("home-missing-scripts");
        let cached = temp.join("cached-plugin");
        let cache_dir = home.join(".claude/plugins");
        fs::create_dir_all(cached.join(".claude-plugin/scripts/bin")).ok_test();
        fs::create_dir_all(&cache_dir).ok_test();
        let missing_scripts = serde_json::json!({
            "plugins": {
                "livespec-orchestrator-beads-fabro@github": [
                    {"installPath": cached.display().to_string()}
                ]
            }
        });
        check(
            fs::write(
                cache_dir.join("installed_plugins.json"),
                missing_scripts.to_string(),
            )
            .is_ok(),
            "assert failed",
        );
        check(
            format!(
                "{:?}",
                BackingCliResolution::resolve(&resolver_inputs(
                    resolver_empty_env(),
                    repo,
                    Some(home)
                ))
            )
            .contains("orchestrator plugin root"),
            "assert failed",
        );
    }

    #[test]
    fn backing_cli_resolution_fails_loudly_for_cached_install_list_that_is_not_an_array() {
        let temp = resolver_temp_root("cache-non-array");
        let repo = temp.join("repo-without-plugin");
        fs::create_dir_all(&repo).ok_test();
        let home = temp.join("home-non-array");
        let cache_dir = home.join(".claude/plugins");
        fs::create_dir_all(&cache_dir).ok_test();
        let cache = serde_json::json!({
            "plugins": {
                "livespec-orchestrator-beads-fabro@github": {
                    "installPath": temp.join("cached-plugin").display().to_string()
                }
            }
        });
        fs::write(cache_dir.join("installed_plugins.json"), cache.to_string()).ok_test();

        check(
            format!(
                "{:?}",
                BackingCliResolution::resolve(&resolver_inputs(
                    resolver_empty_env(),
                    repo,
                    Some(home),
                ))
            )
            .contains("is not an array"),
            "assert failed",
        );
    }

    #[test]
    fn backing_cli_resolution_from_process_environment_is_callable() {
        let resolution = BackingCliResolution::from_environment().ok_test();

        check(
            !resolution.selected_repo_path().as_os_str().is_empty(),
            "assert failed",
        );
        check(
            !resolution.programs().list_work_items().is_empty(),
            "assert failed",
        );
    }

    #[test]
    fn python_normalized_invocation_wraps_py_script_through_interpreter() {
        // A resolved `.py` backing CLI (as produced for the installed cache,
        // e.g. `…/scripts/bin/needs_attention.py`) must be invoked as
        // `python3 <script> <args…>` so the script's exec bit is irrelevant —
        // the Finding E fix. The `.py` path becomes the FIRST argument, ahead of
        // the caller's own arguments, and the resolved program is `python3`.
        let script = "/home/user/.claude/plugins/cache/orch/scripts/bin/needs_attention.py";
        let (program, args) = python_normalized_invocation(script, &["--json"]);

        check((program) == ("python3"), "assert_eq failed");
        check((args) == (vec![script, "--json"]), "assert_eq failed");
    }

    #[test]
    fn python_normalized_invocation_leaves_non_py_program_unchanged() {
        // A non-`.py` program — a bare-name default like `needs-attention` or an
        // env-var override pointing at another command — must run directly, so
        // overrides and non-Python programs are never rewritten through python3.
        let (bare_program, bare_args) =
            python_normalized_invocation("needs-attention", &["--json"]);
        check((bare_program) == ("needs-attention"), "assert_eq failed");
        check((bare_args) == (vec!["--json"]), "assert_eq failed");

        let (override_program, override_args) =
            python_normalized_invocation("/usr/local/bin/custom-drive", &["--action", "approve:x"]);
        check(
            (override_program) == ("/usr/local/bin/custom-drive"),
            "assert_eq failed",
        );
        check(
            (override_args) == (vec!["--action", "approve:x"]),
            "assert_eq failed",
        );
    }

    #[test]
    fn live_source_adapters_observe_each_source_through_the_probe() {
        let probe = UnavailableProbe;
        let adapters = live_source_adapters(&probe, "console").ok_test();

        let adapter_ids: Vec<&str> = adapters
            .iter()
            .map(|(adapter_id, _adapter)| adapter_id.as_str())
            .collect();
        check(
            (adapter_ids)
                == ([
                    "orchestrator:console",
                    "dispatcher:console",
                    "fabro:console",
                    "livespec:console",
                    "github:console",
                ]),
            "assert_eq failed",
        );

        // Polling every adapter exercises both probe capabilities (commands and
        // the Dispatcher journal file). The probe reports every source
        // unavailable, so each adapter emits one honest not-observed finding
        // rather than a fabricated snapshot.
        let refs: Vec<SourceAdapterRef<'_>> = adapters
            .iter()
            .map(|(adapter_id, adapter)| (adapter_id.as_str(), adapter as &dyn PullSourcePort))
            .collect();
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        let summaries =
            backfill_source_adapters(&mut store, "2026-06-25T00:00:00Z", &refs).ok_test();

        check((summaries.len()) == (5), "assert_eq failed");
        check(
            (store.list_console_events().ok_test().len()) == (5),
            "assert_eq failed",
        );
        for event in store.list_console_events().ok_test() {
            check(
                (event.event_type().contract_name()) == ("source.not_observed_finding_observed"),
                "assert_eq failed",
            );
        }
    }

    #[test]
    fn live_source_adapters_rejects_empty_repo() {
        let probe = UnavailableProbe;

        let result = live_source_adapters(&probe, "  ").map(|_adapters| ());

        check(format!("{result:?}").contains("EmptyRepo"), "assert failed");
    }

    #[test]
    fn demo_backfill_round_trips_through_event_store() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();

        let outcomes = append_demo_events_to_store(&mut store, "2026-06-23T00:00:00Z").ok_test();
        let events = load_tui_events_from_store(&store).ok_test();

        check((outcomes.len()) == (2), "assert_eq failed");
        check(
            (outcomes[0].status()) == (AppendStatus::Inserted),
            "assert_eq failed",
        );
        check(
            (outcomes[1].status()) == (AppendStatus::Inserted),
            "assert_eq failed",
        );
        check((events) == (persisted_demo_events()), "assert_eq failed");
    }

    #[test]
    fn demo_backfill_reports_event_append_errors() {
        let mut store = EventAppendFailingStore;

        let outcome = append_demo_events_to_store(&mut store, "2026-06-23T00:00:00Z");

        check(
            format!("{outcome:?}").contains("InvalidSequence"),
            "assert failed",
        );
    }

    #[test]
    fn demo_backfill_is_idempotent_by_source_event_id() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();

        let first = append_demo_events_to_store(&mut store, "2026-06-23T00:00:00Z").ok_test();
        let second = append_demo_events_to_store(&mut store, "2026-06-23T00:00:00Z").ok_test();
        let events = load_tui_events_from_store(&store).ok_test();

        check(
            (first[0].status()) == (AppendStatus::Inserted),
            "assert_eq failed",
        );
        check(
            (second[0].status()) == (AppendStatus::Duplicate),
            "assert_eq failed",
        );
        check(
            (second[1].status()) == (AppendStatus::Duplicate),
            "assert_eq failed",
        );
        check((events) == (persisted_demo_events()), "assert_eq failed");
    }

    #[test]
    fn backfilled_work_item_snapshot_rebuilds_into_its_lane() {
        let scripted = scripted_source_list();
        let sources = scripted_source_refs(&scripted);
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        backfill_source_adapters(&mut store, "2026-06-25T00:00:00Z", &sources).ok_test();

        // The lane board rebuilds purely from the persisted snapshot payloads:
        // the seeded work-item is emitted as blocked:needs-human at rank "a1".
        let events = store.list_console_events().ok_test();
        let board = project_lane_board(&events);

        check(
            (board.column(Lane::Blocked).map(LaneColumn::count)) == (Some(1)),
            "assert_eq failed",
        );
        let blocked_items = board
            .column(Lane::Blocked)
            .map(LaneColumn::items)
            .unwrap_or_default();
        check(
            (blocked_items[0].work_item_id()) == ("livespec-console-beads-fabro-y45jhj"),
            "assert_eq failed",
        );
        check((blocked_items[0].rank()) == ("a1"), "assert_eq failed");
        check(
            (blocked_items[0].lane_reason()) == (Some(LaneReason::NeedsHuman)),
            "assert_eq failed",
        );
        check((board.total()) == (1), "assert_eq failed");
    }

    #[test]
    fn normalized_dispatcher_journal_payload_persists_projection_fields() {
        let entries: Vec<DispatcherJournalEntry> = DispatcherJournalEntry::new(
            "console",
            "console-1",
            "dispatch-1",
            DispatcherJournalKind::BacklogBounce,
            3,
        )
        .ok()
        .into_iter()
        .collect();
        check((entries.len()) == (1), "assert_eq failed");
        let entry = entries[0].clone();
        let payload_json = normalized_payload_json(&SourcePayload::DispatcherJournalEntry(entry));

        check(
            payload_json.contains(r#""repo":"console""#),
            "assert failed",
        );
        check(
            payload_json.contains(r#""work_item_id":"console-1""#),
            "assert failed",
        );
        check(
            payload_json.contains(r#""dispatch_id":"dispatch-1""#),
            "assert failed",
        );
        check(
            payload_json.contains(r#""kind":"backlog-bounce""#),
            "assert failed",
        );
    }

    /// The demo events as they are read back from the store, where the load
    /// path re-attaches the persisted (empty) `payload_json` that in-memory
    /// envelopes carry as `None`.
    fn persisted_demo_events() -> Vec<ConsoleEvent> {
        demo_events().into_iter().collect()
    }

    fn append_ready_work_item(store: &mut SqliteEventStore, observed_at: &str) {
        let event = ConsoleEvent::new(
            "evt_ready_work".to_owned(),
            1,
            "factory".to_owned(),
            EventType::WorkItemSnapshotObserved,
            "orchestrator".to_owned(),
            "fleet:livespec:ready-work".to_owned(),
            1,
        )
        .with_payload_json(
            r#"{"repo":"fleet:livespec","work_item_id":"work-ready","lane":"ready","lane_reason":null,"rank":"a0","status":"ready","source_version":1}"#
                .to_owned(),
        );
        store
            .append_event(&EventAppend::new(
                event,
                "fleet:livespec:ready-work".to_owned(),
                observed_at.to_owned(),
                observed_at.to_owned(),
                None,
                "corr_evt_ready_work".to_owned(),
                Some("evt_ready_work".to_owned()),
                r#"{"repo":"fleet:livespec","work_item_id":"work-ready","lane":"ready","lane_reason":null,"rank":"a0","status":"ready","source_version":1}"#
                    .to_owned(),
                "{}".to_owned(),
            ))
            .ok_test();
    }

    /// The seed timestamp the session fixtures share.
    const TS0: &str = "2026-07-13T00:00:00Z";

    /// Append a work-item snapshot so a selectable inbox row resolves to a
    /// full board record — the availability context the registry-checked
    /// valves consult — in the lane that admits the staged valve.
    fn append_work_item_lane(
        store: &mut SqliteEventStore,
        work_item_id: &str,
        lane_label: &str,
        source_version: u64,
        observed_at: &str,
    ) {
        let payload = format!(
            r#"{{"repo":"livespec-console-beads-fabro","work_item_id":"{work_item_id}","lane":"{lane_label}","lane_reason":null,"rank":"a0","status":"{lane_label}","source_version":{source_version}}}"#
        );
        let event_id = format!("evt_{work_item_id}_{source_version}");
        let stream = format!("repo:livespec-console-beads-fabro:{work_item_id}");
        let event = ConsoleEvent::new(
            event_id.clone(),
            1,
            "factory".to_owned(),
            EventType::WorkItemSnapshotObserved,
            "orchestrator".to_owned(),
            stream.clone(),
            source_version,
        )
        .with_payload_json(payload.clone());
        store
            .append_event(&EventAppend::new(
                event,
                stream,
                observed_at.to_owned(),
                observed_at.to_owned(),
                None,
                format!("corr_{event_id}"),
                Some(event_id),
                payload,
                "{}".to_owned(),
            ))
            .ok_test();
    }

    fn factory_drain_effect() -> TuiRuntimeEffect {
        TuiRuntimeEffect::PersistCommand(CommandEnvelope::new(
            "cmd_factory_drain_requested_budget_1_parallel_1".to_owned(),
            CommandType::FactoryDrainRequested,
            "fleet:livespec".to_owned(),
            "fleet:livespec:factory.drain_requested:budget=1:parallel=1".to_owned(),
            "operator".to_owned(),
        ))
    }

    /// A payload-bearing dispatcher-setting write effect, as a Settings-row edit
    /// produces it: a `config.dispatcher_setting_set` command carrying the
    /// `{ repo, setting, value }` payload the Configuration handler reads.
    fn dispatcher_setting_set_effect() -> TuiRuntimeEffect {
        TuiRuntimeEffect::PersistCommandWithPayload {
            command: CommandEnvelope::new(
                "cmd_config_dispatcher_setting_set_livespec-console-beads-fabro_auto_approve_ready_true"
                    .to_owned(),
                CommandType::ConfigDispatcherSettingSet,
                "livespec-console-beads-fabro".to_owned(),
                "livespec-console-beads-fabro:config.dispatcher_setting_set:auto_approve_ready=true"
                    .to_owned(),
                "operator".to_owned(),
            ),
            payload_json:
                r#"{"repo":"livespec-console-beads-fabro","setting":"auto_approve_ready","value":true}"#
                    .to_owned(),
        }
    }

    fn command_args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn real_store_persistence_reports_missing_command_table_errors() {
        let (path, mut store) = file_store("persist-missing-commands");
        corrupt_store(&path, "drop table commands");

        let error = err_eventstore_command_outcomes(persist_tui_runtime_effects(
            &mut store,
            &[factory_drain_effect()],
            "2026-06-23T00:00:02Z",
        ));

        check_event_store_error(error);
        cleanup_store(&path);
    }

    #[test]
    fn real_store_demo_backfill_reports_missing_event_table_errors() {
        let (path, mut store) = file_store("demo-missing-events");
        corrupt_store(&path, "drop table events");

        let append_error = err_eventstore_append_outcomes(append_demo_events_to_store(
            &mut store,
            "2026-06-23T00:00:00Z",
        ));
        let report_error =
            err_eventstore_string(backfill_demo_report(&mut store, "2026-06-23T00:00:00Z"));

        check_event_store_error(append_error);
        check_event_store_error(report_error);
        cleanup_store(&path);
    }

    #[test]
    fn real_store_read_reports_propagate_missing_event_table_errors() {
        for command in ["events", "snapshot", "doctor", "plans"] {
            let (path, store) = file_store(&format!("read-missing-events-{command}"));
            corrupt_store(&path, "drop table events");

            let error = if command == "events" {
                err_eventstore_string(events_tail_report(&store, 20))
            } else if command == "snapshot" {
                err_eventstore_string(snapshot_report(&store))
            } else if command == "doctor" {
                err_eventstore_string(doctor_report(&store))
            } else {
                err_eventstore_string(plan_page_report(&store, "epic-1"))
            };

            check_event_store_error(error);
            cleanup_store(&path);
        }
    }

    #[test]
    fn real_store_snapshot_and_doctor_propagate_missing_command_table_errors() {
        for command in ["snapshot", "doctor"] {
            let (path, store) = file_store(&format!("read-missing-commands-{command}"));
            corrupt_store(&path, "drop table commands");

            let error = if command == "snapshot" {
                err_eventstore_string(snapshot_report(&store))
            } else {
                err_eventstore_string(doctor_report(&store))
            };

            check_event_store_error(error);
            cleanup_store(&path);
        }
    }

    #[test]
    fn real_store_needs_attention_ingest_reports_missing_event_table_errors() {
        let (path, mut store) = file_store("needs-attention-missing-events");
        let port = ScriptedNeedsAttentionPort::observing(vec![attention_item_fixture(
            "wi-approve",
            "Pending approval",
        )]);
        let needs_attention = NeedsAttentionIngest::new(&port, "livespec-console-beads-fabro");
        corrupt_store(&path, "drop table events");

        let error = err_runtime_usize(ingest_needs_attention(
            &mut store,
            &needs_attention,
            "2026-07-07T00:00:00Z",
        ));

        check_runtime_event_store_error(error);
        cleanup_store(&path);
    }

    #[test]
    fn real_store_refresh_sources_reports_needs_attention_store_errors() {
        let (path, mut store) = file_store("refresh-missing-events");
        let port = ScriptedNeedsAttentionPort::observing(vec![attention_item_fixture(
            "wi-approve",
            "Pending approval",
        )]);
        let needs_attention = NeedsAttentionIngest::new(&port, "livespec-console-beads-fabro");
        corrupt_store(&path, "drop table events");

        let error = err_runtime_summaries(refresh_sources(
            &mut store,
            "2026-07-07T00:00:00Z",
            &[],
            &needs_attention,
        ));

        check_runtime_event_store_error(error);
        cleanup_store(&path);
    }

    #[test]
    fn real_store_reflection_reports_missing_command_and_event_table_errors() {
        let audit = AutonomousAudit::new(
            vec![ok_decision(AutonomousDecision::from_auto_disposition(
                "wi-1",
                "auto-approve",
                vec!["auto_approve_ready".to_owned()],
            ))],
            vec![ok_decision(AutonomousDecision::from_auto_disposition(
                "wi-2",
                "cap-exceeded-escalation",
                vec!["acceptance_rework_cap".to_owned()],
            ))],
        );
        let decisions = SimulatedDecisionsPort::returning(audit);

        let (command_path, mut command_store) = file_store("reflection-missing-commands");
        corrupt_store(&command_path, "drop table commands");
        let command_error = err_runtime_usize(observe_and_reflect_autonomous_decisions(
            &mut command_store,
            "2026-07-11T00:00:01Z",
            &decisions,
        ));
        check_runtime_event_store_error(command_error);
        cleanup_store(&command_path);

        let escalation_audit =
            AutonomousAudit::new(Vec::new(), decisions.audit.escalations().to_vec());
        let escalation_decisions = SimulatedDecisionsPort::returning(escalation_audit);
        let (event_path, mut event_store) = file_store("reflection-missing-events");
        corrupt_store(&event_path, "drop table events");
        let event_error = err_runtime_usize(observe_and_reflect_autonomous_decisions(
            &mut event_store,
            "2026-07-11T00:00:01Z",
            &escalation_decisions,
        ));
        check_runtime_event_store_error(event_error);
        cleanup_store(&event_path);
    }

    #[test]
    fn real_store_pending_handlers_report_missing_table_errors() {
        let (factory_event_path, mut factory_event_store) = file_store("factory-missing-events");
        corrupt_store(&factory_event_path, "drop table events");
        let factory_event_error = err_runtime_pending_outcomes(handle_pending_factory_commands(
            &mut factory_event_store,
            "2026-06-23T00:00:03Z",
            &mut SimulatedFactoryDrainPort,
        ));
        check_runtime_event_store_error(factory_event_error);
        cleanup_store(&factory_event_path);

        let (factory_command_path, mut factory_command_store) =
            file_store("factory-missing-commands");
        corrupt_store(&factory_command_path, "drop table commands");
        let factory_command_error = err_runtime_pending_outcomes(handle_pending_factory_commands(
            &mut factory_command_store,
            "2026-06-23T00:00:03Z",
            &mut SimulatedFactoryDrainPort,
        ));
        check_runtime_event_store_error(factory_command_error);
        cleanup_store(&factory_command_path);

        let (work_item_path, mut work_item_store) = file_store("work-item-missing-commands");
        corrupt_store(&work_item_path, "drop table commands");
        let work_item_error = err_runtime_pending_outcomes(handle_pending_work_item_commands(
            &mut work_item_store,
            "2026-06-23T00:00:03Z",
            &mut SimulatedWorkItemActionPort::default(),
        ));
        check_runtime_event_store_error(work_item_error);
        cleanup_store(&work_item_path);

        let (config_path, mut config_store) = file_store("config-missing-commands");
        corrupt_store(&config_path, "drop table commands");
        let config_error = err_runtime_pending_outcomes(handle_pending_config_commands(
            &mut config_store,
            "2026-06-23T00:00:03Z",
            &mut SimulatedWorkItemActionPort::default(),
        ));
        check_runtime_event_store_error(config_error);
        cleanup_store(&config_path);
    }

    #[test]
    fn real_store_tui_session_reports_startup_ingest_store_errors() {
        let (path, mut store) = file_store("tui-session-missing-events");
        let port = ScriptedNeedsAttentionPort::observing(vec![attention_item_fixture(
            "wi-approve",
            "Pending approval",
        )]);
        let needs_attention = NeedsAttentionIngest::new(&port, "livespec-console-beads-fabro");
        let mut runner = ScriptedTuiSessionRunner::new(Vec::new());
        let mut factory_port = SimulatedFactoryDrainPort;
        let mut work_item_port = SimulatedWorkItemActionPort::default();
        let poll_requester = poll_requester();
        let command_requester = command_requester();
        corrupt_store(&path, "drop table events");

        let error = err_runtime_tui_outcome(run_store_backed_tui_session(
            &mut store,
            "2026-07-07T00:00:00Z",
            "operator",
            &mut runner,
            &[],
            &mut factory_port,
            &mut work_item_port,
            &empty_decisions_port(),
            &needs_attention,
            &poll_requester,
            &command_requester,
        ));

        check_runtime_event_store_error(error);
        cleanup_store(&path);
    }

    fn plan_snapshot_event(
        event_id: &str,
        work_item_id: &str,
        rank: &str,
        status: &str,
        detail_json: &str,
    ) -> ConsoleEvent {
        let payload = format!(
            r#"{{"repo":"console","work_item_id":"{work_item_id}","lane":"ready","lane_reason":null,"rank":"{rank}","status":"{status}","source_version":1,"detail":{detail_json}}}"#
        );
        ConsoleEvent::fixture(
            event_id,
            EventType::WorkItemSnapshotObserved,
            "orchestrator",
        )
        .with_payload_json(payload)
    }

    /// Test double standing in for a real Dispatcher port that completes a
    /// drain. Production no longer ships a success-fabricating port (the live
    /// path uses `DispatcherFactoryDrainPort`); this double lets the command and
    /// session machinery still be exercised against a completing outcome.
    struct SimulatedFactoryDrainPort;

    impl FactoryDrainPort for SimulatedFactoryDrainPort {
        fn drain_ready_queue(
            &mut self,
            request: &FactoryDrainRequest,
        ) -> Result<FactoryDrainPortOutcome, ApplicationError> {
            if request.budget() == 0 {
                return Err(ApplicationError::FactoryDrainPortFailed);
            }
            if request.parallel() == 0 {
                return Err(ApplicationError::FactoryDrainPortFailed);
            }
            Ok(FactoryDrainPortOutcome::completed(1))
        }
    }

    /// Test double standing in for the real Dispatcher drain port. Unlike
    /// [`SimulatedFactoryDrainPort`] it RECORDS every drain it was asked to run,
    /// which is what distinguishes a drain that actually reached the Dispatcher
    /// from one the command store silently deduped away before it ever got here.
    #[derive(Default)]
    struct RecordingFactoryDrainPort {
        observed_aggregate_ids: Vec<String>,
    }

    impl FactoryDrainPort for RecordingFactoryDrainPort {
        fn drain_ready_queue(
            &mut self,
            request: &FactoryDrainRequest,
        ) -> Result<FactoryDrainPortOutcome, ApplicationError> {
            self.observed_aggregate_ids
                .push(request.aggregate_id().to_owned());
            Ok(FactoryDrainPortOutcome::completed(1))
        }
    }

    #[derive(Default)]
    struct CompletingFactoryDispatchItemPort {
        observed_work_item_ids: Vec<String>,
    }

    impl FactoryDispatchItemPort for CompletingFactoryDispatchItemPort {
        fn dispatch_item(
            &mut self,
            request: &console_application::FactoryDispatchItemRequest,
        ) -> Result<console_application::FactoryDispatchItemPortOutcome, ApplicationError> {
            self.observed_work_item_ids
                .push(request.work_item_id().to_owned());
            Ok(console_application::FactoryDispatchItemPortOutcome::completed())
        }
    }

    struct ErroringFactoryDispatchItemPort;

    impl FactoryDispatchItemPort for ErroringFactoryDispatchItemPort {
        fn dispatch_item(
            &mut self,
            _request: &console_application::FactoryDispatchItemRequest,
        ) -> Result<console_application::FactoryDispatchItemPortOutcome, ApplicationError> {
            Err(ApplicationError::FactoryDispatchItemPortFailed)
        }
    }

    struct CountingFactoryDrainPort {
        calls: Rc<std::cell::Cell<usize>>,
    }

    impl CountingFactoryDrainPort {
        fn new(calls: Rc<std::cell::Cell<usize>>) -> Self {
            Self { calls }
        }
    }

    impl FactoryDrainPort for CountingFactoryDrainPort {
        fn drain_ready_queue(
            &mut self,
            _request: &FactoryDrainRequest,
        ) -> Result<FactoryDrainPortOutcome, ApplicationError> {
            self.calls.set(self.calls.get() + 1);
            Ok(FactoryDrainPortOutcome::completed(1))
        }
    }

    /// Test double standing in for the real orchestrator-action port. It
    /// records the action-ids it was asked to run and returns a configurable
    /// outcome so the work-item command machinery can be exercised without a
    /// live `drive` binary.
    #[derive(Default)]
    struct SimulatedWorkItemActionPort {
        outcome: Option<OrchestratorActionOutcome>,
        observed_action_ids: Vec<String>,
    }

    impl SimulatedWorkItemActionPort {
        fn returning(outcome: OrchestratorActionOutcome) -> Self {
            Self {
                outcome: Some(outcome),
                observed_action_ids: Vec::new(),
            }
        }
    }

    impl OrchestratorActionPort for SimulatedWorkItemActionPort {
        fn run_action(
            &mut self,
            request: &OrchestratorActionRequest,
        ) -> Result<OrchestratorActionOutcome, ApplicationError> {
            self.observed_action_ids
                .push(request.action_id().to_owned());
            Ok(self
                .outcome
                .clone()
                .unwrap_or(OrchestratorActionOutcome::Completed))
        }
    }

    struct ErroringWorkItemActionPort;

    impl OrchestratorActionPort for ErroringWorkItemActionPort {
        fn run_action(
            &mut self,
            _request: &OrchestratorActionRequest,
        ) -> Result<OrchestratorActionOutcome, ApplicationError> {
            Err(ApplicationError::FactoryDrainPortFailed)
        }
    }

    /// Scriptable autonomous-decisions port double: returns a canned audit so the
    /// observe/reflect path can be driven without a live Dispatcher journal.
    struct SimulatedDecisionsPort {
        audit: AutonomousAudit,
    }

    impl SimulatedDecisionsPort {
        fn returning(audit: AutonomousAudit) -> Self {
            Self { audit }
        }
    }

    impl AutonomousDecisionsPort for SimulatedDecisionsPort {
        fn read_autonomous_decisions(&self) -> AutonomousAudit {
            self.audit.clone()
        }
    }

    /// A decisions port observing an empty audit -- nothing to reflect -- for the
    /// many store-backed tests that exercise other flows.
    fn empty_decisions_port() -> SimulatedDecisionsPort {
        SimulatedDecisionsPort::returning(AutonomousAudit::default())
    }

    /// A poll requester that counts the out-of-band poll requests it received, so
    /// a test can assert `refresh_events(true)` pings the poller while
    /// `refresh_events(false)` does not. Used by every store-backed test (the ones
    /// that do not care simply ignore the count).
    struct RecordingPollRequester {
        polls: std::cell::Cell<usize>,
    }

    impl RecordingPollRequester {
        fn new() -> Self {
            Self {
                polls: std::cell::Cell::new(0),
            }
        }

        fn poll_count(&self) -> usize {
            self.polls.get()
        }
    }

    impl SourcePollRequester for RecordingPollRequester {
        fn request_poll(&self) {
            self.polls.set(self.polls.get() + 1);
        }
    }

    fn poll_requester() -> RecordingPollRequester {
        RecordingPollRequester::new()
    }

    struct RecordingPendingCommandRequester {
        requests: std::cell::Cell<usize>,
        inline: bool,
    }

    impl RecordingPendingCommandRequester {
        const fn new(inline: bool) -> Self {
            Self {
                requests: std::cell::Cell::new(0),
                inline,
            }
        }

        fn request_count(&self) -> usize {
            self.requests.get()
        }
    }

    impl PendingCommandRequester for RecordingPendingCommandRequester {
        fn request_pending_command_handling(&self) {
            self.requests.set(self.requests.get() + 1);
        }

        fn handles_pending_commands_inline(&self) -> bool {
            self.inline
        }
    }

    fn command_requester() -> RecordingPendingCommandRequester {
        RecordingPendingCommandRequester::new(true)
    }

    fn async_command_requester() -> RecordingPendingCommandRequester {
        RecordingPendingCommandRequester::new(false)
    }

    #[test]
    #[should_panic(expected = "check failed")]
    fn check_panics() {
        check(false, "check failed");
    }

    #[test]
    #[should_panic(expected = "check_event_store_error failed")]
    fn check_event_store_error_panics() {
        check_event_store_error(EventStoreError::UnknownEventType(
            "unknown.event".to_owned(),
        ));
    }

    #[test]
    #[should_panic(expected = "check_runtime_event_store_error failed")]
    fn check_runtime_event_store_error_panics() {
        check_runtime_event_store_error(ConsoleRuntimeError::TuiRuntimeFailed);
    }

    #[test]
    #[should_panic(expected = "ok_store failed")]
    fn ok_store_panics() {
        ok_store(Err(EventStoreError::InvalidSequence));
    }

    #[test]
    #[should_panic(expected = "ok_sqlite_connection failed")]
    fn ok_sqlite_connection_panics() {
        ok_sqlite_connection(Err(rusqlite::Error::InvalidQuery));
    }

    #[test]
    #[should_panic(expected = "ok_sqlite_unit failed")]
    fn ok_sqlite_unit_panics() {
        ok_sqlite_unit(Err(rusqlite::Error::InvalidQuery));
    }

    #[test]
    #[should_panic(expected = "ok_io_unit failed")]
    fn ok_io_unit_panics() {
        ok_io_unit(Err(std::io::Error::other("boom")));
    }

    #[test]
    #[should_panic(expected = "ok_duration failed")]
    fn ok_duration_panics() {
        ok_duration(SystemTime::UNIX_EPOCH.duration_since(SystemTime::now()));
    }

    #[test]
    #[should_panic(expected = "ok_decision failed")]
    fn ok_decision_panics() {
        ok_decision(None);
    }

    #[test]
    #[should_panic(expected = "err_eventstore_command_outcomes failed")]
    fn err_eventstore_command_outcomes_panics() {
        err_eventstore_command_outcomes(Ok(Vec::new()));
    }

    #[test]
    #[should_panic(expected = "err_eventstore_append_outcomes failed")]
    fn err_eventstore_append_outcomes_panics() {
        err_eventstore_append_outcomes(Ok(Vec::new()));
    }

    #[test]
    #[should_panic(expected = "err_eventstore_string failed")]
    fn err_eventstore_string_panics() {
        err_eventstore_string(Ok(String::new()));
    }

    #[test]
    #[should_panic(expected = "err_runtime_usize failed")]
    fn err_runtime_usize_panics() {
        err_runtime_usize(Ok(0));
    }

    #[test]
    #[should_panic(expected = "err_runtime_summaries failed")]
    fn err_runtime_summaries_panics() {
        err_runtime_summaries(Ok(Vec::new()));
    }

    #[test]
    #[should_panic(expected = "err_runtime_pending_outcomes failed")]
    fn err_runtime_pending_outcomes_panics() {
        err_runtime_pending_outcomes(Ok(Vec::new()));
    }

    #[test]
    #[should_panic(expected = "err_runtime_tui_outcome failed")]
    fn err_runtime_tui_outcome_panics() {
        err_runtime_tui_outcome(Ok(TuiSessionOutcome::new(0, 0, 0, 0, 0, 0)));
    }

    #[test]
    #[should_panic(expected = "ok_eventstore_append_outcomes failed")]
    fn ok_eventstore_append_outcomes_panics() {
        let result: EventStoreResult<Vec<AppendOutcome>> = Err(EventStoreError::InvalidSequence);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_eventstore_append_outcome failed")]
    fn ok_eventstore_append_outcome_panics() {
        let result: EventStoreResult<AppendOutcome> = Err(EventStoreError::InvalidSequence);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_eventstore_command_outcomes failed")]
    fn ok_eventstore_command_outcomes_panics() {
        let result: EventStoreResult<Vec<CommandAppendOutcome>> =
            Err(EventStoreError::InvalidSequence);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_eventstore_command_outcome failed")]
    fn ok_eventstore_command_outcome_panics() {
        let result: EventStoreResult<CommandAppendOutcome> = Err(EventStoreError::InvalidSequence);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_eventstore_console_events failed")]
    fn ok_eventstore_console_events_panics() {
        let result: EventStoreResult<Vec<ConsoleEvent>> = Err(EventStoreError::InvalidSequence);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_eventstore_commands failed")]
    fn ok_eventstore_commands_panics() {
        let result: EventStoreResult<Vec<StoredCommand>> = Err(EventStoreError::InvalidSequence);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_eventstore_events failed")]
    fn ok_eventstore_events_panics() {
        let result: EventStoreResult<Vec<StoredEvent>> = Err(EventStoreError::InvalidSequence);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_eventstore_optional_string failed")]
    fn ok_eventstore_optional_string_panics() {
        let result: EventStoreResult<Option<String>> = Err(EventStoreError::InvalidSequence);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_eventstore_string failed")]
    fn ok_eventstore_string_panics() {
        let result: EventStoreResult<String> = Err(EventStoreError::InvalidSequence);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_eventstore_bool failed")]
    fn ok_eventstore_bool_panics() {
        let result: EventStoreResult<bool> = Err(EventStoreError::InvalidSequence);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_eventstore_status_update failed")]
    fn ok_eventstore_status_update_panics() {
        let result: EventStoreResult<CommandStatusUpdateOutcome> =
            Err(EventStoreError::InvalidSequence);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_runtime_unit failed")]
    fn ok_runtime_unit_panics() {
        let result: ConsoleRuntimeResult<()> = Err(ConsoleRuntimeError::TuiRuntimeFailed);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_runtime_usize failed")]
    fn ok_runtime_usize_panics() {
        let result: ConsoleRuntimeResult<usize> = Err(ConsoleRuntimeError::TuiRuntimeFailed);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_runtime_string failed")]
    fn ok_runtime_string_panics() {
        let result: ConsoleRuntimeResult<String> = Err(ConsoleRuntimeError::TuiRuntimeFailed);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_runtime_summaries failed")]
    fn ok_runtime_summaries_panics() {
        let result: ConsoleRuntimeResult<Vec<AdapterIngestionSummary>> =
            Err(ConsoleRuntimeError::TuiRuntimeFailed);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_runtime_console_events failed")]
    fn ok_runtime_console_events_panics() {
        let result: ConsoleRuntimeResult<Vec<ConsoleEvent>> =
            Err(ConsoleRuntimeError::TuiRuntimeFailed);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_runtime_pending_outcomes failed")]
    fn ok_runtime_pending_outcomes_panics() {
        let result: ConsoleRuntimeResult<Vec<PendingCommandOutcome>> =
            Err(ConsoleRuntimeError::TuiRuntimeFailed);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_runtime_tui_effects failed")]
    fn ok_runtime_tui_effects_panics() {
        let result: ConsoleRuntimeResult<Vec<TuiRuntimeEffect>> =
            Err(ConsoleRuntimeError::TuiRuntimeFailed);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_runtime_observed_adapters failed")]
    fn ok_runtime_observed_adapters_panics() {
        let result: ConsoleRuntimeResult<Vec<(String, ObservedSourceAdapter<'_>)>> =
            Err(ConsoleRuntimeError::TuiRuntimeFailed);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_runtime_source_polls failed")]
    fn ok_runtime_source_polls_panics() {
        let result: ConsoleRuntimeResult<Vec<(&'static str, AdapterPoll)>> =
            Err(ConsoleRuntimeError::TuiRuntimeFailed);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_runtime_tui_outcome failed")]
    fn ok_runtime_tui_outcome_panics() {
        let result: ConsoleRuntimeResult<TuiSessionOutcome> =
            Err(ConsoleRuntimeError::TuiRuntimeFailed);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_runtime_tui_effect_outcome failed")]
    fn ok_runtime_tui_effect_outcome_panics() {
        let result: ConsoleRuntimeResult<TuiRuntimeEffectSinkOutcome> =
            Err(ConsoleRuntimeError::TuiRuntimeFailed);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_runtime_status_update failed")]
    fn ok_runtime_status_update_panics() {
        let result: ConsoleRuntimeResult<CommandStatusUpdateOutcome> =
            Err(ConsoleRuntimeError::TuiRuntimeFailed);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_runtime_decision failed")]
    fn ok_runtime_decision_panics() {
        let result: ConsoleRuntimeResult<AutonomousDecision> =
            Err(ConsoleRuntimeError::TuiRuntimeFailed);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_runtime_stored_command_ref failed")]
    fn ok_runtime_stored_command_ref_panics() {
        let result: Result<&StoredCommand, ConsoleRuntimeError> =
            Err(ConsoleRuntimeError::TuiRuntimeFailed);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_application_factory_outcome failed")]
    fn ok_application_factory_outcome_panics() {
        let result: Result<FactoryCommandOutcome, ApplicationError> =
            Err(ApplicationError::FactoryDrainPortFailed);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_application_unit failed")]
    fn ok_application_unit_panics() {
        let result: Result<(), ApplicationError> = Err(ApplicationError::FactoryDrainPortFailed);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_adapter_poll failed")]
    fn ok_adapter_poll_panics() {
        let result: Result<AdapterPoll, AdapterError> = Err(AdapterError::EmptyCheckpoint);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_work_item_snapshot failed")]
    fn ok_work_item_snapshot_panics() {
        let result: Result<WorkItemSnapshot, AdapterError> = Err(AdapterError::EmptyWorkItemId);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_runtime_duration failed")]
    fn ok_runtime_duration_panics() {
        let result: Result<std::time::Duration, ConsoleRuntimeError> =
            Err(ConsoleRuntimeError::TuiRuntimeFailed);
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_runtime_pending_outcomes_result failed")]
    fn ok_runtime_pending_outcomes_result_panics() {
        let result: Result<ConsoleRuntimeResult<Vec<PendingCommandOutcome>>, ConsoleRuntimeError> =
            Err(ConsoleRuntimeError::TuiRuntimeFailed);
        let _ = result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_box_unit failed")]
    fn ok_box_unit_panics() {
        let result: Result<(), Box<dyn Error>> = Err(Box::new(std::io::Error::other("boom")));
        result.ok_test();
    }

    #[test]
    #[should_panic(expected = "ok_backing_cli_resolution failed")]
    fn ok_backing_cli_resolution_panics() {
        let result: Result<BackingCliResolution, BackingCliResolutionError> =
            Err(BackingCliResolutionError::new("boom".to_owned()));
        result.ok_test();
    }

    #[test]
    fn helper_success_arms_are_covered() {
        check(true, "helper success");
        ok_io_unit(Ok(()));
        let _count = (Ok(1_usize) as ConsoleRuntimeResult<usize>).ok_test();
        let _summaries =
            (Ok(Vec::new()) as ConsoleRuntimeResult<Vec<AdapterIngestionSummary>>).ok_test();
        let _pending =
            (Ok(Vec::new()) as ConsoleRuntimeResult<Vec<PendingCommandOutcome>>).ok_test();
        let _effects = (Ok(Vec::new()) as ConsoleRuntimeResult<Vec<TuiRuntimeEffect>>).ok_test();
        let _outcome =
            (Ok(TuiSessionOutcome::new(0, 0, 0, 0, 0, 0)) as ConsoleRuntimeResult<_>).ok_test();
        let command = StoredCommand::new(
            "cmd_helper".to_owned(),
            "factory".to_owned(),
            "factory.drain_requested".to_owned(),
            Some("fleet:livespec".to_owned()),
            "idem_helper".to_owned(),
            "operator".to_owned(),
            "pending".to_owned(),
        );
        let _command = (Ok(&command) as Result<&StoredCommand, ConsoleRuntimeError>).ok_test();
        let inner: ConsoleRuntimeResult<Vec<PendingCommandOutcome>> = Ok(Vec::new());
        let _inner = (Ok(inner)
            as Result<ConsoleRuntimeResult<Vec<PendingCommandOutcome>>, ConsoleRuntimeError>)
            .ok_test();
        let _poll = (Ok(AdapterPoll::new("checkpoint", Vec::new()).ok_test())
            as Result<AdapterPoll, AdapterError>)
            .ok_test();
        let _source_polls =
            (Ok(Vec::new()) as ConsoleRuntimeResult<Vec<(&'static str, AdapterPoll)>>).ok_test();
        (Ok(()) as Result<(), ApplicationError>).ok_test();
        (Ok(()) as Result<(), Box<dyn Error>>).ok_test();
    }

    trait TestOk {
        type Output;

        fn ok_test(self) -> Self::Output;
    }

    impl TestOk for EventStoreResult<SqliteEventStore> {
        type Output = SqliteEventStore;

        #[track_caller]
        fn ok_test(self) -> SqliteEventStore {
            ok_store(self)
        }
    }

    impl TestOk for EventStoreResult<Vec<AppendOutcome>> {
        type Output = Vec<AppendOutcome>;

        #[track_caller]
        fn ok_test(self) -> Vec<AppendOutcome> {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_eventstore_append_outcomes failed: {error:?}"),
            }
        }
    }

    impl TestOk for EventStoreResult<AppendOutcome> {
        type Output = AppendOutcome;

        #[track_caller]
        fn ok_test(self) -> AppendOutcome {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_eventstore_append_outcome failed: {error:?}"),
            }
        }
    }

    impl TestOk for EventStoreResult<Vec<CommandAppendOutcome>> {
        type Output = Vec<CommandAppendOutcome>;

        #[track_caller]
        fn ok_test(self) -> Vec<CommandAppendOutcome> {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_eventstore_command_outcomes failed: {error:?}"),
            }
        }
    }

    impl TestOk for EventStoreResult<CommandAppendOutcome> {
        type Output = CommandAppendOutcome;

        #[track_caller]
        fn ok_test(self) -> CommandAppendOutcome {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_eventstore_command_outcome failed: {error:?}"),
            }
        }
    }

    impl TestOk for EventStoreResult<Vec<ConsoleEvent>> {
        type Output = Vec<ConsoleEvent>;

        #[track_caller]
        fn ok_test(self) -> Vec<ConsoleEvent> {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_eventstore_console_events failed: {error:?}"),
            }
        }
    }

    impl TestOk for EventStoreResult<Vec<StoredCommand>> {
        type Output = Vec<StoredCommand>;

        #[track_caller]
        fn ok_test(self) -> Vec<StoredCommand> {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_eventstore_commands failed: {error:?}"),
            }
        }
    }

    impl TestOk for EventStoreResult<Vec<StoredEvent>> {
        type Output = Vec<StoredEvent>;

        #[track_caller]
        fn ok_test(self) -> Vec<StoredEvent> {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_eventstore_events failed: {error:?}"),
            }
        }
    }

    impl TestOk for EventStoreResult<Option<String>> {
        type Output = Option<String>;

        #[track_caller]
        fn ok_test(self) -> Option<String> {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_eventstore_optional_string failed: {error:?}"),
            }
        }
    }

    impl TestOk for EventStoreResult<String> {
        type Output = String;

        #[track_caller]
        fn ok_test(self) -> String {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_eventstore_string failed: {error:?}"),
            }
        }
    }

    impl TestOk for EventStoreResult<bool> {
        type Output = bool;

        #[track_caller]
        fn ok_test(self) -> bool {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_eventstore_bool failed: {error:?}"),
            }
        }
    }

    impl TestOk for EventStoreResult<CommandStatusUpdateOutcome> {
        type Output = CommandStatusUpdateOutcome;

        #[track_caller]
        fn ok_test(self) -> CommandStatusUpdateOutcome {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_eventstore_status_update failed: {error:?}"),
            }
        }
    }

    impl TestOk for ConsoleRuntimeResult<()> {
        type Output = ();

        #[track_caller]
        fn ok_test(self) {
            match self {
                Ok(()) => {}
                Err(error) => panic!("ok_runtime_unit failed: {error:?}"),
            }
        }
    }

    impl TestOk for ConsoleRuntimeResult<usize> {
        type Output = usize;

        #[track_caller]
        fn ok_test(self) -> usize {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_runtime_usize failed: {error:?}"),
            }
        }
    }

    impl TestOk for ConsoleRuntimeResult<String> {
        type Output = String;

        #[track_caller]
        fn ok_test(self) -> String {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_runtime_string failed: {error:?}"),
            }
        }
    }

    impl TestOk for ConsoleRuntimeResult<Vec<AdapterIngestionSummary>> {
        type Output = Vec<AdapterIngestionSummary>;

        #[track_caller]
        fn ok_test(self) -> Vec<AdapterIngestionSummary> {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_runtime_summaries failed: {error:?}"),
            }
        }
    }

    impl TestOk for ConsoleRuntimeResult<Vec<ConsoleEvent>> {
        type Output = Vec<ConsoleEvent>;

        #[track_caller]
        fn ok_test(self) -> Vec<ConsoleEvent> {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_runtime_console_events failed: {error:?}"),
            }
        }
    }

    impl TestOk for ConsoleRuntimeResult<Vec<PendingCommandOutcome>> {
        type Output = Vec<PendingCommandOutcome>;

        #[track_caller]
        fn ok_test(self) -> Vec<PendingCommandOutcome> {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_runtime_pending_outcomes failed: {error:?}"),
            }
        }
    }

    impl TestOk for ConsoleRuntimeResult<Vec<TuiRuntimeEffect>> {
        type Output = Vec<TuiRuntimeEffect>;

        #[track_caller]
        fn ok_test(self) -> Vec<TuiRuntimeEffect> {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_runtime_tui_effects failed: {error:?}"),
            }
        }
    }

    impl<'a> TestOk for ConsoleRuntimeResult<Vec<(String, ObservedSourceAdapter<'a>)>> {
        type Output = Vec<(String, ObservedSourceAdapter<'a>)>;

        #[track_caller]
        fn ok_test(self) -> Vec<(String, ObservedSourceAdapter<'a>)> {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_runtime_observed_adapters failed: {error:?}"),
            }
        }
    }

    impl TestOk for ConsoleRuntimeResult<Vec<(&'static str, AdapterPoll)>> {
        type Output = Vec<(&'static str, AdapterPoll)>;

        #[track_caller]
        fn ok_test(self) -> Vec<(&'static str, AdapterPoll)> {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_runtime_source_polls failed: {error:?}"),
            }
        }
    }

    impl TestOk for ConsoleRuntimeResult<TuiSessionOutcome> {
        type Output = TuiSessionOutcome;

        #[track_caller]
        fn ok_test(self) -> TuiSessionOutcome {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_runtime_tui_outcome failed: {error:?}"),
            }
        }
    }

    impl TestOk for ConsoleRuntimeResult<TuiRuntimeEffectSinkOutcome> {
        type Output = TuiRuntimeEffectSinkOutcome;

        #[track_caller]
        fn ok_test(self) -> TuiRuntimeEffectSinkOutcome {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_runtime_tui_effect_outcome failed: {error:?}"),
            }
        }
    }

    impl TestOk for ConsoleRuntimeResult<CommandStatusUpdateOutcome> {
        type Output = CommandStatusUpdateOutcome;

        #[track_caller]
        fn ok_test(self) -> CommandStatusUpdateOutcome {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_runtime_status_update failed: {error:?}"),
            }
        }
    }

    impl TestOk for ConsoleRuntimeResult<AutonomousDecision> {
        type Output = AutonomousDecision;

        #[track_caller]
        fn ok_test(self) -> AutonomousDecision {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_runtime_decision failed: {error:?}"),
            }
        }
    }

    impl<'a> TestOk for Result<&'a StoredCommand, ConsoleRuntimeError> {
        type Output = &'a StoredCommand;

        #[track_caller]
        fn ok_test(self) -> &'a StoredCommand {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_runtime_stored_command_ref failed: {error:?}"),
            }
        }
    }

    impl TestOk for Result<FactoryCommandOutcome, ApplicationError> {
        type Output = FactoryCommandOutcome;

        #[track_caller]
        fn ok_test(self) -> FactoryCommandOutcome {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_application_factory_outcome failed: {error:?}"),
            }
        }
    }

    impl TestOk for Result<(), ApplicationError> {
        type Output = ();

        #[track_caller]
        fn ok_test(self) {
            match self {
                Ok(()) => {}
                Err(error) => panic!("ok_application_unit failed: {error:?}"),
            }
        }
    }

    impl TestOk for Result<AdapterPoll, AdapterError> {
        type Output = AdapterPoll;

        #[track_caller]
        fn ok_test(self) -> AdapterPoll {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_adapter_poll failed: {error:?}"),
            }
        }
    }

    impl TestOk for Result<WorkItemSnapshot, AdapterError> {
        type Output = WorkItemSnapshot;

        #[track_caller]
        fn ok_test(self) -> WorkItemSnapshot {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_work_item_snapshot failed: {error:?}"),
            }
        }
    }

    impl TestOk for Result<std::time::Duration, ConsoleRuntimeError> {
        type Output = std::time::Duration;

        #[track_caller]
        fn ok_test(self) -> std::time::Duration {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_runtime_duration failed: {error:?}"),
            }
        }
    }

    impl TestOk for Result<ConsoleRuntimeResult<Vec<PendingCommandOutcome>>, ConsoleRuntimeError> {
        type Output = ConsoleRuntimeResult<Vec<PendingCommandOutcome>>;

        #[track_caller]
        fn ok_test(self) -> ConsoleRuntimeResult<Vec<PendingCommandOutcome>> {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_runtime_pending_outcomes_result failed: {error:?}"),
            }
        }
    }

    impl TestOk for Result<(), Box<dyn Error>> {
        type Output = ();

        #[track_caller]
        fn ok_test(self) {
            match self {
                Ok(()) => {}
                Err(error) => panic!("ok_box_unit failed: {error:?}"),
            }
        }
    }

    impl TestOk for Result<BackingCliResolution, BackingCliResolutionError> {
        type Output = BackingCliResolution;

        #[track_caller]
        fn ok_test(self) -> BackingCliResolution {
            match self {
                Ok(value) => value,
                Err(error) => panic!("ok_backing_cli_resolution failed: {error:?}"),
            }
        }
    }

    impl TestOk for Result<std::time::Duration, std::time::SystemTimeError> {
        type Output = std::time::Duration;

        #[track_caller]
        fn ok_test(self) -> std::time::Duration {
            ok_duration(self)
        }
    }

    impl TestOk for Result<(), std::io::Error> {
        type Output = ();

        #[track_caller]
        fn ok_test(self) {
            ok_io_unit(self);
        }
    }

    #[track_caller]
    fn check(condition: bool, context: &str) {
        if !condition {
            panic!("{context}");
        }
    }

    #[track_caller]
    fn check_event_store_error(error: EventStoreError) {
        match error {
            EventStoreError::InvalidSequence | EventStoreError::Sqlite(_) => {}
            other => panic!("check_event_store_error failed: {other:?}"),
        }
    }

    #[track_caller]
    fn check_runtime_event_store_error(error: ConsoleRuntimeError) {
        match error {
            ConsoleRuntimeError::EventStore(error) => check_event_store_error(error),
            other => panic!("check_runtime_event_store_error failed: {other:?}"),
        }
    }

    #[track_caller]
    fn ok_store(result: EventStoreResult<SqliteEventStore>) -> SqliteEventStore {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_store failed: {error:?}"),
        }
    }

    #[track_caller]
    fn ok_sqlite_connection(
        result: Result<rusqlite::Connection, rusqlite::Error>,
    ) -> rusqlite::Connection {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_sqlite_connection failed: {error:?}"),
        }
    }

    #[track_caller]
    fn ok_sqlite_unit(result: Result<(), rusqlite::Error>) {
        match result {
            Ok(()) => {}
            Err(error) => panic!("ok_sqlite_unit failed: {error:?}"),
        }
    }

    #[track_caller]
    fn ok_io_unit(result: Result<(), std::io::Error>) {
        match result {
            Ok(()) => {}
            Err(error) => panic!("ok_io_unit failed: {error:?}"),
        }
    }

    #[track_caller]
    fn ok_duration(
        result: Result<std::time::Duration, std::time::SystemTimeError>,
    ) -> std::time::Duration {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_duration failed: {error:?}"),
        }
    }

    #[track_caller]
    fn ok_decision(result: Option<AutonomousDecision>) -> AutonomousDecision {
        match result {
            Some(value) => value,
            None => panic!("ok_decision failed"),
        }
    }

    #[track_caller]
    fn err_eventstore_command_outcomes(
        result: EventStoreResult<Vec<CommandAppendOutcome>>,
    ) -> EventStoreError {
        match result {
            Ok(_value) => panic!("err_eventstore_command_outcomes failed"),
            Err(error) => error,
        }
    }

    #[track_caller]
    fn err_eventstore_append_outcomes(
        result: EventStoreResult<Vec<AppendOutcome>>,
    ) -> EventStoreError {
        match result {
            Ok(_value) => panic!("err_eventstore_append_outcomes failed"),
            Err(error) => error,
        }
    }

    #[track_caller]
    fn err_eventstore_string(result: EventStoreResult<String>) -> EventStoreError {
        match result {
            Ok(_value) => panic!("err_eventstore_string failed"),
            Err(error) => error,
        }
    }

    #[track_caller]
    fn err_runtime_usize(result: ConsoleRuntimeResult<usize>) -> ConsoleRuntimeError {
        match result {
            Ok(_value) => panic!("err_runtime_usize failed"),
            Err(error) => error,
        }
    }

    #[track_caller]
    fn err_runtime_summaries(
        result: ConsoleRuntimeResult<Vec<AdapterIngestionSummary>>,
    ) -> ConsoleRuntimeError {
        match result {
            Ok(_value) => panic!("err_runtime_summaries failed"),
            Err(error) => error,
        }
    }

    #[track_caller]
    fn err_runtime_pending_outcomes(
        result: ConsoleRuntimeResult<Vec<PendingCommandOutcome>>,
    ) -> ConsoleRuntimeError {
        match result {
            Ok(_value) => panic!("err_runtime_pending_outcomes failed"),
            Err(error) => error,
        }
    }

    #[track_caller]
    fn err_runtime_tui_outcome(
        result: ConsoleRuntimeResult<TuiSessionOutcome>,
    ) -> ConsoleRuntimeError {
        match result {
            Ok(_value) => panic!("err_runtime_tui_outcome failed"),
            Err(error) => error,
        }
    }

    fn file_store(name: &str) -> (PathBuf, SqliteEventStore) {
        let nanos = ok_duration(SystemTime::now().duration_since(UNIX_EPOCH)).as_nanos();
        let path = std::env::temp_dir().join(format!(
            "livespec-console-cli-{name}-{}-{nanos}.sqlite",
            std::process::id()
        ));
        let _ignored = fs::remove_file(&path);
        let store = ok_store(SqliteEventStore::open(&path));
        (path, store)
    }

    fn corrupt_store(path: &Path, sql: &str) {
        let connection = ok_sqlite_connection(rusqlite::Connection::open(path));
        ok_sqlite_unit(connection.execute_batch(sql));
    }

    fn cleanup_store(path: &Path) {
        let _ignored = fs::remove_file(path);
        let wal = path.with_extension("sqlite-wal");
        let shm = path.with_extension("sqlite-shm");
        let _ignored = fs::remove_file(wal);
        let _ignored = fs::remove_file(shm);
    }

    #[test]
    fn observe_and_reflect_resolves_auto_resolutions_and_surfaces_escalations() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        // Seed the inbox with the human-gate valve item the plane will
        // auto-resolve. The cap-exceeded escalation is sourced only from the
        // journal read leg.
        let approve_item = attention_item_fixture("valve:approve:wi-1", "Approve wi-1");
        let port = ScriptedNeedsAttentionPort::observing(vec![approve_item]);
        let needs_attention = NeedsAttentionIngest::new(&port, "fleet");
        ingest_needs_attention(&mut store, &needs_attention, "2026-07-11T00:00:00Z").ok_test();
        check(
            (project_attention(&store.list_console_events().ok_test()).len()) == (1),
            "assert_eq failed",
        );

        // The plane's engine auto-approved wi-1 and escalated wi-2 from the
        // journal read surface.
        let audit = AutonomousAudit::new(
            vec![
                AutonomousDecision::from_auto_disposition(
                    "wi-1",
                    "auto-approve",
                    vec!["auto_approve_ready".to_owned()],
                )
                .ok_or(ConsoleRuntimeError::TuiRuntimeFailed)
                .ok_test(),
            ],
            vec![
                AutonomousDecision::from_auto_disposition(
                    "wi-2",
                    "cap-exceeded-escalation",
                    vec!["acceptance_rework_cap".to_owned()],
                )
                .ok_or(ConsoleRuntimeError::TuiRuntimeFailed)
                .ok_test(),
            ],
        );
        let decisions = SimulatedDecisionsPort::returning(audit);

        let now = "2026-07-11T00:00:01Z";
        let reflected =
            observe_and_reflect_autonomous_decisions(&mut store, now, &decisions).ok_test();

        // The auto-approved item left the inbox; the escalation is surfaced
        // using the needs-human valve identity.
        check((reflected) == (2), "assert_eq failed");
        let remaining: Vec<String> = project_attention(&store.list_console_events().ok_test())
            .iter()
            .map(|item| item.id().to_owned())
            .collect();
        check(
            (remaining) == (["valve:set-admission:wi-2"]),
            "assert_eq failed",
        );

        // The reflection rode a command-plus-outcome-event path: a completed
        // `factory.autonomous_decision_reflected` command plus the resolved event.
        let commands = store.list_commands().ok_test();
        check(
            commands.iter().any(|command| {
                command.command_type() == "factory.autonomous_decision_reflected"
                    && command.status() == "completed"
            }),
            "assert failed",
        );
        check(
            store.list_console_events().ok_test().iter().any(|event| {
                *event.event_type() == EventType::AttentionItemResolved
                    && event.source() == "console:autonomous-reflect"
            }),
            "assert failed",
        );

        // A second run re-observing the same audit reflects nothing new (the
        // append-only journal is idempotent under content-stable command ids).
        let later = "2026-07-11T00:00:02Z";
        let again =
            observe_and_reflect_autonomous_decisions(&mut store, later, &decisions).ok_test();
        check((again) == (0), "assert_eq failed");
        check(
            (project_attention(&store.list_console_events().ok_test()).len()) == (1),
            "assert_eq failed",
        );
    }

    #[test]
    fn observe_and_reflect_ignores_a_lost_reflection_claim() {
        let mut store =
            ScriptedFactoryCommandStore::new(ScriptedStoreMode::AutonomousReflectionClaimMiss);
        let decision = AutonomousDecision::from_auto_disposition(
            "wi-1",
            "auto-approve",
            vec!["auto_approve_ready".to_owned()],
        );
        check(decision.is_some(), "assert failed");
        let audit = AutonomousAudit::new(decision.into_iter().collect(), Vec::new());
        let decisions = SimulatedDecisionsPort::returning(audit);

        let reflected = observe_and_reflect_autonomous_decisions(
            &mut store,
            "2026-07-11T00:00:01Z",
            &decisions,
        );

        check((reflected.ok_test()) == (0), "assert_eq failed");
        check((store.appended_event_count) == (0), "assert_eq failed");
    }

    #[test]
    fn observe_and_reflect_skips_a_decision_with_no_reflectable_item() {
        let mut store = SqliteEventStore::open_in_memory().ok_test();
        // A decision whose gate maps to no needs-attention valve id is skipped --
        // no command is recorded and nothing is fabricated.
        let audit = AutonomousAudit::new(
            vec![AutonomousDecision::new(
                "wi-1",
                "mystery-gate",
                "d",
                "auto-resolved",
            )],
            Vec::new(),
        );
        let decisions = SimulatedDecisionsPort::returning(audit);

        let now = "2026-07-11T00:00:00Z";
        let reflected =
            observe_and_reflect_autonomous_decisions(&mut store, now, &decisions).ok_test();

        check((reflected) == (0), "assert_eq failed");
        check(store.list_commands().ok_test().is_empty(), "assert failed");
    }

    struct FailedFactoryDrainPort;

    impl FactoryDrainPort for FailedFactoryDrainPort {
        fn drain_ready_queue(
            &mut self,
            _request: &FactoryDrainRequest,
        ) -> Result<FactoryDrainPortOutcome, ApplicationError> {
            Ok(FactoryDrainPortOutcome::failed_with_diagnostic(
                r#"{"summary":"factory-safety refusal","domain_error":"host-only-refused"}"#
                    .to_owned(),
            ))
        }
    }

    struct ParkedFactoryDrainPort;

    impl FactoryDrainPort for ParkedFactoryDrainPort {
        fn drain_ready_queue(
            &mut self,
            _request: &FactoryDrainRequest,
        ) -> Result<FactoryDrainPortOutcome, ApplicationError> {
            Ok(FactoryDrainPortOutcome::failed_with_diagnostic(
                "parked in acceptance under acceptance_policy ai-then-human".to_owned(),
            ))
        }
    }

    struct ErroringFactoryDrainPort;

    impl FactoryDrainPort for ErroringFactoryDrainPort {
        fn drain_ready_queue(
            &mut self,
            _request: &FactoryDrainRequest,
        ) -> Result<FactoryDrainPortOutcome, ApplicationError> {
            Err(ApplicationError::FactoryDrainPortFailed)
        }
    }

    struct ScriptedTuiSessionRunner {
        effects: Vec<TuiRuntimeEffect>,
        observed_event_count: usize,
        observed_requested_by: String,
    }

    impl ScriptedTuiSessionRunner {
        fn new(effects: Vec<TuiRuntimeEffect>) -> Self {
            Self {
                effects,
                observed_event_count: 0,
                observed_requested_by: String::new(),
            }
        }

        const fn observed_event_count(&self) -> usize {
            self.observed_event_count
        }

        fn observed_requested_by(&self) -> &str {
            &self.observed_requested_by
        }
    }

    impl TuiSessionRunner for ScriptedTuiSessionRunner {
        fn run_tui(
            &mut self,
            events: &[ConsoleEvent],
            requested_by: &str,
            _session: &mut dyn TuiLiveSession,
        ) -> ConsoleRuntimeResult<Vec<TuiRuntimeEffect>> {
            self.observed_event_count = events.len();
            self.observed_requested_by = requested_by.to_owned();
            Ok(self.effects.clone())
        }
    }

    struct DrainThenInputTuiSessionRunner {
        port_calls: Rc<std::cell::Cell<usize>>,
        port_calls_after_drain_effect: Option<usize>,
        serviced_input_after_drain_effect: bool,
    }

    impl DrainThenInputTuiSessionRunner {
        fn new(port_calls: Rc<std::cell::Cell<usize>>) -> Self {
            Self {
                port_calls,
                port_calls_after_drain_effect: None,
                serviced_input_after_drain_effect: false,
            }
        }
    }

    impl TuiSessionRunner for DrainThenInputTuiSessionRunner {
        fn run_tui(
            &mut self,
            events: &[ConsoleEvent],
            _requested_by: &str,
            session: &mut dyn TuiLiveSession,
        ) -> ConsoleRuntimeResult<Vec<TuiRuntimeEffect>> {
            session
                .handle_runtime_effect(&factory_drain_effect())
                .map_err(|_error| ConsoleRuntimeError::TuiRuntimeFailed)
                .ok_test();
            self.port_calls_after_drain_effect = Some(self.port_calls.get());
            let state = TuiInteractionState::new(
                0,
                TuiOverlay::CommandPalette {
                    query: String::new(),
                },
            );
            let step = console_tui::step_tui_runtime(
                &state,
                events,
                TuiTerminalInput::Interaction(TuiInteraction::TypeChar('x')),
                "operator",
            );
            self.serviced_input_after_drain_effect = matches!(
                step.state().overlay(),
                TuiOverlay::CommandPalette { query } if query == "x"
            );
            Ok(Vec::new())
        }
    }

    struct ImmediateValveTuiSessionRunner;

    impl TuiSessionRunner for ImmediateValveTuiSessionRunner {
        fn run_tui(
            &mut self,
            events: &[ConsoleEvent],
            requested_by: &str,
            session: &mut dyn TuiLiveSession,
        ) -> ConsoleRuntimeResult<Vec<TuiRuntimeEffect>> {
            let state = TuiInteractionState::new(
                0,
                TuiOverlay::ValveConfirm {
                    valve: PendingValve::SetAdmission(AdmissionPolicy::Auto),
                },
            );
            let step = console_tui::step_tui_runtime(
                &state,
                events,
                TuiTerminalInput::Confirm,
                requested_by,
            );
            match session
                .handle_runtime_effect(step.effect())
                .map_err(|_error| ConsoleRuntimeError::TuiRuntimeFailed)
            {
                Ok(_outcome) => {}
                Err(error) => return Err(error),
            }
            Ok(Vec::new())
        }
    }

    struct ErroringTuiSessionRunner;

    impl TuiSessionRunner for ErroringTuiSessionRunner {
        fn run_tui(
            &mut self,
            _events: &[ConsoleEvent],
            _requested_by: &str,
            _session: &mut dyn TuiLiveSession,
        ) -> ConsoleRuntimeResult<Vec<TuiRuntimeEffect>> {
            Err(ConsoleRuntimeError::TuiRuntimeFailed)
        }
    }

    struct CommandAppendFailingStore;

    impl CommandAppendStore for CommandAppendFailingStore {
        fn append_command(
            &mut self,
            _append: &CommandAppend,
        ) -> EventStoreResult<CommandAppendOutcome> {
            Err(EventStoreError::InvalidSequence)
        }

        fn list_commands(&self) -> EventStoreResult<Vec<StoredCommand>> {
            Ok(Vec::new())
        }

        fn command_count(&self) -> EventStoreResult<usize> {
            Ok(0)
        }
    }

    struct EventAppendFailingStore;

    impl EventAppendStore for EventAppendFailingStore {
        fn append_event(&mut self, _append: &EventAppend) -> EventStoreResult<AppendOutcome> {
            Err(EventStoreError::InvalidSequence)
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ScriptedStoreMode {
        AppendFails,
        AutonomousReflectionClaimMiss,
        Completes,
        ConfigAppendFails,
        ConfigClaimMiss,
        FactoryClaimMiss,
        ListFails,
        MissingAggregate,
        NonFactoryPending,
        RecoveryFails,
        StatusUpdateFails,
        WorkItemAppendFails,
        WorkItemClaimMiss,
    }

    struct ScriptedFactoryCommandStore {
        command: StoredCommand,
        appended_event_count: usize,
        mode: ScriptedStoreMode,
    }

    impl ScriptedFactoryCommandStore {
        fn new(mode: ScriptedStoreMode) -> Self {
            Self {
                command: StoredCommand::new(
                    "cmd_factory_drain_requested_budget_1_parallel_1".to_owned(),
                    "factory".to_owned(),
                    "factory.drain_requested".to_owned(),
                    Some("fleet:livespec".to_owned()),
                    "fleet:livespec:factory.drain_requested:budget=1:parallel=1".to_owned(),
                    "operator".to_owned(),
                    "pending".to_owned(),
                ),
                appended_event_count: 0,
                mode,
            }
        }

        fn commands(&self) -> Vec<StoredCommand> {
            if self.mode == ScriptedStoreMode::MissingAggregate {
                return vec![StoredCommand::new(
                    "cmd_missing_aggregate".to_owned(),
                    "factory".to_owned(),
                    "factory.drain_requested".to_owned(),
                    None,
                    "fleet:livespec:factory.drain_requested:budget=1:parallel=1".to_owned(),
                    "operator".to_owned(),
                    "pending".to_owned(),
                )];
            }
            if self.mode == ScriptedStoreMode::NonFactoryPending {
                return vec![StoredCommand::new(
                    "cmd_non_factory".to_owned(),
                    "attention".to_owned(),
                    "attention.local_only".to_owned(),
                    Some("work-item".to_owned()),
                    "idem_non_factory".to_owned(),
                    "operator".to_owned(),
                    "pending".to_owned(),
                )];
            }
            if matches!(
                self.mode,
                ScriptedStoreMode::WorkItemAppendFails | ScriptedStoreMode::WorkItemClaimMiss
            ) {
                return vec![StoredCommand::new(
                    "cmd_approve".to_owned(),
                    "work_item".to_owned(),
                    "work_item.approve_requested".to_owned(),
                    Some("wi-1".to_owned()),
                    "wi-1:work_item.approve_requested".to_owned(),
                    "operator".to_owned(),
                    "pending".to_owned(),
                )];
            }
            if matches!(
                self.mode,
                ScriptedStoreMode::ConfigAppendFails | ScriptedStoreMode::ConfigClaimMiss
            ) {
                return vec![StoredCommand::new(
                    "cmd_setting".to_owned(),
                    "configuration".to_owned(),
                    "config.dispatcher_setting_set".to_owned(),
                    Some("livespec-console-beads-fabro".to_owned()),
                    "livespec-console-beads-fabro:config.dispatcher_setting_set".to_owned(),
                    "operator".to_owned(),
                    "pending".to_owned(),
                )
                .with_payload_json(
                    r#"{"repo":"livespec-console-beads-fabro","setting":"auto_approve_ready","value":true}"#
                        .to_owned(),
                )];
            }
            vec![self.command.clone()]
        }
    }

    impl FactoryCommandStore for ScriptedFactoryCommandStore {
        fn list_commands(&self) -> EventStoreResult<Vec<StoredCommand>> {
            if self.mode == ScriptedStoreMode::ListFails {
                return Err(EventStoreError::InvalidSequence);
            }
            Ok(self.commands())
        }

        fn list_console_events(&self) -> EventStoreResult<Vec<ConsoleEvent>> {
            Ok(vec![ConsoleEvent::new(
                "evt_ready_work".to_owned(),
                1,
                "factory".to_owned(),
                EventType::WorkItemSnapshotObserved,
                "orchestrator".to_owned(),
                "fleet:livespec:ready-work".to_owned(),
                1,
            )
            .with_payload_json(
                r#"{"repo":"fleet:livespec","work_item_id":"work-ready","lane":"ready","lane_reason":null,"rank":"a0","status":"ready","source_version":1}"#
                    .to_owned(),
            )])
        }

        fn append_command(
            &mut self,
            append: &CommandAppend,
        ) -> EventStoreResult<CommandAppendOutcome> {
            Ok(CommandAppendOutcome::new(
                append.command().command_id().to_owned(),
                CommandAppendStatus::Inserted,
            ))
        }

        fn append_event(&mut self, _append: &EventAppend) -> EventStoreResult<AppendOutcome> {
            if matches!(
                self.mode,
                ScriptedStoreMode::AppendFails
                    | ScriptedStoreMode::WorkItemAppendFails
                    | ScriptedStoreMode::ConfigAppendFails
            ) {
                return Err(EventStoreError::InvalidSequence);
            }
            self.appended_event_count += 1;
            Ok(AppendOutcome::new(1, AppendStatus::Inserted))
        }

        fn claim_command(
            &mut self,
            _command_id: &str,
            _claimed_at: &str,
        ) -> EventStoreResult<bool> {
            Ok(!matches!(
                self.mode,
                ScriptedStoreMode::ConfigClaimMiss
                    | ScriptedStoreMode::FactoryClaimMiss
                    | ScriptedStoreMode::AutonomousReflectionClaimMiss
                    | ScriptedStoreMode::WorkItemClaimMiss
            ))
        }

        fn update_command_status(
            &mut self,
            _command_id: &str,
            _status: &str,
            _updated_at: &str,
            _result_json: Option<&str>,
            _error_json: Option<&str>,
        ) -> EventStoreResult<CommandStatusUpdateOutcome> {
            if self.mode == ScriptedStoreMode::StatusUpdateFails {
                return Err(EventStoreError::InvalidSequence);
            }
            Ok(CommandStatusUpdateOutcome::new(
                "cmd_factory_drain_requested_budget_1_parallel_1".to_owned(),
                "completed".to_owned(),
            ))
        }

        fn finalize_executing_command_status(
            &mut self,
            command_id: &str,
            status: &str,
            updated_at: &str,
            result_json: Option<&str>,
            error_json: Option<&str>,
        ) -> EventStoreResult<CommandStatusUpdateOutcome> {
            self.update_command_status(command_id, status, updated_at, result_json, error_json)
        }

        fn fail_stale_executing_commands(
            &mut self,
            _stale_before: &str,
            _recovered_at: &str,
            _error_json: &str,
        ) -> EventStoreResult<usize> {
            if self.mode == ScriptedStoreMode::RecoveryFails {
                return Err(EventStoreError::InvalidSequence);
            }
            Ok(0)
        }
    }
}
