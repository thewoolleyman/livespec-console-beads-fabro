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
use std::rc::Rc;

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
    ApplicationError, FactoryDrainPolicy, FactoryDrainPort, OrchestratorActionPort,
    build_tui_model, handle_factory_drain_command, handle_work_item_accept_command,
    handle_work_item_approve_command, handle_work_item_reject_command,
    handle_work_item_set_acceptance_command, handle_work_item_set_admission_command,
    project_attention,
    source_adapters::{
        AdapterError, AdapterIngestionSummary, NeedsAttentionReadOutcome,
        NeedsAttentionSnapshotPort, NormalizeObservation, NormalizedSourceEvent,
        ObservedSourceAdapter, PullSourcePort, SourceAdapterKind, SourceCheckpointPort,
        SourceEventAppendPort, SourceObservationPlan, SourcePayload, SourceProbe,
        attention_item_payload_json, attention_resolved_payload_json, diff_needs_attention,
        materialize_attention_items, parse_dispatcher_observation, parse_fabro_observation,
        parse_github_observation, parse_livespec_observation, parse_orchestrator_observation,
        run_adapter_poll, work_item_snapshot_payload_json,
    },
};
use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};
use console_eventstore::{
    AppendOutcome, AppendStatus, CommandAppend, CommandAppendOutcome, CommandStatusUpdateOutcome,
    EventAppend, EventStoreError, EventStoreResult, SqliteEventStore, StoredCommand,
};
use console_tui::TuiRuntimeEffect;

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
pub fn run_with_store(
    args: &[String],
    store: &mut SqliteEventStore,
    observed_at: &str,
    sources: &[SourceAdapterRef<'_>],
    factory_port: &mut dyn FactoryDrainPort,
    work_item_port: &mut dyn OrchestratorActionPort,
    needs_attention: &NeedsAttentionIngest<'_>,
) -> RunOutput {
    match command_name(args) {
        Some("serve") => run_runtime_result(
            serve_report(
                store,
                observed_at,
                sources,
                factory_port,
                work_item_port,
                needs_attention,
            ),
            "serve",
        ),
        Some("backfill") => run_runtime_result(
            backfill_source_report(store, observed_at, sources, needs_attention),
            "backfill",
        ),
        Some("events") => run_events_with_store(args, store),
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
        Some("snapshot") => RunOutput::new(0, "snapshot mode bootstrap: not yet wired".to_owned()),
        Some("doctor") => RunOutput::new(0, "doctor bootstrap: no findings".to_owned()),
        Some("arch-check") => RunOutput::new(
            0,
            "run `just check-arch` for architecture enforcement".to_owned(),
        ),
        Some(other) => RunOutput::new(2, format!("unknown command: {other}\n\n{}", help_text())),
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
}

impl CommandAppendStore for SqliteEventStore {
    fn append_command(&mut self, append: &CommandAppend) -> EventStoreResult<CommandAppendOutcome> {
        Self::append_command(self, append)
    }
}

/// Return the persist tui runtime effects value.
pub fn persist_tui_runtime_effects(
    store: &mut dyn CommandAppendStore,
    effects: &[TuiRuntimeEffect],
    requested_at: &str,
) -> EventStoreResult<Vec<CommandAppendOutcome>> {
    let mut outcomes = Vec::new();
    for effect in effects {
        let Some(append) = command_append_from_tui_effect(effect, requested_at) else {
            continue;
        };
        outcomes.push(store.append_command(&append)?);
    }
    Ok(outcomes)
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
    ) -> ConsoleRuntimeResult<Vec<TuiRuntimeEffect>>;
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
    needs_attention: &NeedsAttentionIngest<'_>,
) -> ConsoleRuntimeResult<TuiSessionOutcome> {
    let existing_events = store.list_console_events()?;
    let ingestion = if existing_events.is_empty() {
        backfill_source_adapters(store, observed_at, sources)?
    } else {
        Vec::new()
    };
    let _attention_ingested = ingest_needs_attention(store, needs_attention, observed_at)?;
    let presented_events = store.list_console_events()?;
    let effects = runner.run_tui(&presented_events, requested_by)?;
    let persisted = persist_tui_runtime_effects(store, &effects, observed_at)?;
    let handled = handle_pending_factory_commands(store, observed_at, factory_port)?;
    let _work_item_handled = handle_pending_work_item_commands(store, observed_at, work_item_port)?;
    let final_events = store.list_console_events()?;
    let attention_count = project_attention(&final_events).len();
    let backfilled_event_count = ingestion
        .iter()
        .map(AdapterIngestionSummary::appended_event_count)
        .sum();
    Ok(TuiSessionOutcome::new(
        backfilled_event_count,
        presented_events.len(),
        persisted.len(),
        handled.len(),
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
    Ok(format!(
        "backfill source adapters: adapters {}, events {event_count}",
        summaries.len()
    ))
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

const DISPATCHER_JOURNAL_PATH: &str = "tmp/dispatcher-journal.jsonl";

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
    let specs: [(
        &str,
        SourceAdapterKind,
        SourceObservationPlan,
        NormalizeObservation,
    ); 5] = [
        (
            "orchestrator",
            SourceAdapterKind::Orchestrator,
            SourceObservationPlan::command("list-work-items", &["--json"]),
            parse_orchestrator_observation,
        ),
        (
            "dispatcher",
            SourceAdapterKind::Dispatcher,
            SourceObservationPlan::file(DISPATCHER_JOURNAL_PATH),
            parse_dispatcher_observation,
        ),
        (
            "fabro",
            SourceAdapterKind::Fabro,
            SourceObservationPlan::command("fabro", &["ps", "--json"]),
            parse_fabro_observation,
        ),
        (
            "livespec",
            SourceAdapterKind::LiveSpec,
            SourceObservationPlan::command("livespec", &["next", "--json"]),
            parse_livespec_observation,
        ),
        (
            "github",
            SourceAdapterKind::GitHub,
            SourceObservationPlan::command(
                "gh",
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
    let prior = materialize_attention_items(&existing);
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

/// Return the serve report value.
pub fn serve_report(
    store: &mut SqliteEventStore,
    observed_at: &str,
    sources: &[SourceAdapterRef<'_>],
    factory_port: &mut dyn FactoryDrainPort,
    work_item_port: &mut dyn OrchestratorActionPort,
    needs_attention: &NeedsAttentionIngest<'_>,
) -> ConsoleRuntimeResult<String> {
    let events = store.list_console_events()?;
    let ingestion = if events.is_empty() {
        backfill_source_adapters(store, observed_at, sources)?
    } else {
        Vec::new()
    };
    let _attention_ingested = ingest_needs_attention(store, needs_attention, observed_at)?;
    let handled = handle_pending_factory_commands(store, observed_at, factory_port)?;
    let work_item_handled = handle_pending_work_item_commands(store, observed_at, work_item_port)?;
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

    /// Append a command-handling event and return the append outcome.
    fn append_event(&mut self, append: &EventAppend) -> EventStoreResult<AppendOutcome>;

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
}

impl FactoryCommandStore for SqliteEventStore {
    fn list_commands(&self) -> EventStoreResult<Vec<StoredCommand>> {
        Self::list_commands(self)
    }

    fn list_console_events(&self) -> EventStoreResult<Vec<ConsoleEvent>> {
        Self::list_console_events(self)
    }

    fn append_event(&mut self, append: &EventAppend) -> EventStoreResult<AppendOutcome> {
        Self::append_event(self, append)
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
}

/// Handle pending factory commands.
pub fn handle_pending_factory_commands(
    store: &mut dyn FactoryCommandStore,
    handled_at: &str,
    port: &mut dyn FactoryDrainPort,
) -> ConsoleRuntimeResult<Vec<PendingCommandOutcome>> {
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
        let command_outcome = handle_factory_drain_command(&command, &policy, port)?;
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
    let mut outcomes = Vec::new();
    for stored_command in store.list_commands()? {
        if stored_command.status() != "pending" {
            continue;
        }
        let Some(pending) = work_item_command_from_stored(&stored_command)? else {
            continue;
        };
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
    let error_json = if matches!(command_status, "failed" | "rejected") {
        Some("{}")
    } else {
        None
    };
    let status_update = store.update_command_status(
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
) -> Option<CommandAppend> {
    match effect {
        TuiRuntimeEffect::PersistCommand(command) => Some(CommandAppend::new(
            command.clone(),
            requested_at.to_owned(),
            Some(command.aggregate_id().to_owned()),
            command_correlation_id(command),
            "{}".to_owned(),
        )),
        TuiRuntimeEffect::Render
        | TuiRuntimeEffect::OpenAttachCommand(_)
        | TuiRuntimeEffect::CopyAttachCommand(_)
        | TuiRuntimeEffect::Quit
        | TuiRuntimeEffect::ApplicationError(_) => None,
    }
}

fn command_correlation_id(command: &CommandEnvelope) -> String {
    format!("corr_{}", command.command_id())
}

fn factory_command_from_stored(
    stored_command: &StoredCommand,
) -> ConsoleRuntimeResult<Option<CommandEnvelope>> {
    if stored_command.command_type() != CommandType::FactoryDrainRequested.contract_name() {
        return Ok(None);
    }
    let Some(aggregate_id) = stored_command.aggregate_id() else {
        return Err(ConsoleRuntimeError::MissingCommandAggregate(
            stored_command.command_id().to_owned(),
        ));
    };
    Ok(Some(CommandEnvelope::new(
        stored_command.command_id().to_owned(),
        CommandType::FactoryDrainRequested,
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
}

impl PendingWorkItemCommand {
    /// The wrapped command envelope, shared by every dispatch outcome.
    const fn command(&self) -> &CommandEnvelope {
        match self {
            Self::Approve(command)
            | Self::Accept(command)
            | Self::Reject { command, .. }
            | Self::SetAdmission { command, .. }
            | Self::SetAcceptance { command, .. } => command,
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
    if !(is_approve || is_accept || is_reject || is_set_admission || is_set_acceptance) {
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
    } else {
        // Set-acceptance is the acceptance policy dial: it surfaces the stored
        // `payload_json` (the `{"policy": ...}` object) for the application
        // handler to parse.
        PendingWorkItemCommand::SetAcceptance {
            command: rebuild(CommandType::WorkItemSetAcceptanceRequested),
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
/// snapshots are serialized in full so the lane board can rebuild from them;
/// other source payloads carry no projection state yet and persist as `{}`.
fn normalized_payload_json(payload: &SourcePayload) -> String {
    match payload {
        SourcePayload::WorkItemSnapshot(snapshot) => work_item_snapshot_payload_json(snapshot),
        SourcePayload::AttentionItemAppeared(item) | SourcePayload::AttentionItemChanged(item) => {
            attention_item_payload_json(item)
        }
        SourcePayload::AttentionItemResolved(id) => attention_resolved_payload_json(id),
        SourcePayload::CompletenessFinding(_)
        | SourcePayload::DispatcherJournalEntry(_)
        | SourcePayload::FabroRunSnapshot(_)
        | SourcePayload::GithubPullRequestSnapshot(_)
        | SourcePayload::LivespecNextSnapshot(_)
        | SourcePayload::NotObservedFinding(_) => "{}".to_owned(),
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
            .map_err(|_error| AdapterError::AppendFailed)
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
        "  snapshot",
        "  doctor",
        "  arch-check",
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use console_application::{
        ApplicationError, AttentionItem, FactoryDrainPort, FactoryDrainPortOutcome,
        FactoryDrainRequest, LaneColumn, OrchestratorActionOutcome, OrchestratorActionPort,
        OrchestratorActionRequest, build_tui_model, project_attention, project_lane_board,
        source_adapters::{
            AcceptancePolicy, AdapterError, AdmissionPolicy, AttentionHandoff,
            AttentionItemSnapshot, AttentionSourceRef, Lane, LaneReason, NeedsAttentionReadOutcome,
            NeedsAttentionSnapshotPort, PullSourcePort, SourceProbe, SourceProbeOutcome,
            WorkItemSnapshot, normalize_work_item_snapshot,
        },
    };
    use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};
    use console_eventstore::{
        AppendOutcome, AppendStatus, CommandAppend, CommandAppendOutcome, CommandAppendStatus,
        CommandStatusUpdateOutcome, EventAppend, EventStoreError, EventStoreResult,
        SqliteEventStore, StoredCommand,
    };
    use console_tui::TuiRuntimeEffect;

    use super::{
        CommandAppendStore, ConsoleRuntimeError, ConsoleRuntimeResult, EventAppendStore,
        FactoryCommandStore, InitialSourceSeed, NeedsAttentionIngest, PendingCommandOutcome,
        ScriptedSource, SourceAdapterRef, TuiSessionOutcome, TuiSessionRunner,
        append_demo_events_to_store, backfill_demo_report, backfill_source_adapters,
        backfill_source_report, command_status_update_runtime_result, demo_events, doctor_report,
        events_tail_report, factory_command_from_stored, handle_pending_factory_commands,
        handle_pending_work_item_commands, ingest_needs_attention, initial_source_seed,
        live_source_adapters, load_tui_events_from_store, persist_tui_runtime_effects,
        render_tui_preview, run, run_store_backed_tui_session, run_with_store, serve_report,
        snapshot_report, source_polls_from_seed, work_item_command_from_stored,
    };

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
            .unwrap_or_default()
            .into_iter()
            .map(|(adapter_id, poll)| (adapter_id.to_owned(), ScriptedSource::new(poll)))
            .collect()
    }

    fn scripted_source_list_with_ready_work() -> Vec<(String, ScriptedSource)> {
        let mut sources = scripted_source_list();
        if let Ok(snapshot) = WorkItemSnapshot::new(
            "livespec-console-beads-fabro",
            "livespec-console-beads-fabro-ready",
            Lane::Ready,
            None,
            "a0",
            "ready",
            AdmissionPolicy::Manual,
            AcceptancePolicy::AiThenHuman,
            7,
        ) {
            sources.push((
                "orchestrator-ready:livespec-console-beads-fabro".to_owned(),
                ScriptedSource::new(normalize_work_item_snapshot(&snapshot)),
            ));
        }
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
            &needs_attention,
        )
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

    #[test]
    fn help_lists_specified_command_shape() {
        let output = run(["bin", "help"]);

        assert_eq!(output.code(), 0);
        assert!(output.message().contains("events tail"));
        assert!(output.message().contains("arch-check"));
    }

    #[test]
    fn tui_command_projects_demo_attention_items() {
        let output = run(["bin", "tui"]);

        assert_eq!(output.code(), 0);
        assert!(output.message().contains("LiveSpec Console"));
        assert!(output.message().contains("> Attention"));
        assert!(output.message().contains("> Blocked: needs-human"));
        assert!(output.message().contains("Repo: console"));
        assert!(output.message().contains("Fabro run: evt_demo_1"));
        assert!(output.message().contains("Attach: fabro attach evt_demo_1"));
        assert!(!output.message().contains("Actions:"));
    }

    #[test]
    fn unknown_command_is_usage_error() {
        let output = run(["bin", "bogus"]);

        assert_eq!(output.code(), 2);
        assert!(output.message().contains("unknown command: bogus"));
    }

    #[test]
    fn no_command_prints_help() {
        let output = run(["bin"]);

        assert_eq!(output.code(), 0);
        assert!(output.message().contains("Commands:"));
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

            assert_eq!(output.code(), 0);
            assert_eq!(output.message(), expected);
        }
    }

    #[test]
    fn events_tail_reports_placeholder_mode() {
        let output = run(["bin", "events", "tail"]);

        assert_eq!(output.code(), 0);
        assert_eq!(output.message(), "events tail bootstrap: not yet wired");
    }

    #[test]
    fn events_without_tail_is_usage_error() {
        let output = run(["bin", "events"]);

        assert_eq!(output.code(), 2);
        assert_eq!(
            output.message(),
            "usage: livespec-console-beads-fabro events tail"
        );
    }

    #[test]
    fn store_backed_backfill_command_reports_source_adapter_counts() -> Result<(), EventStoreError>
    {
        let mut store = SqliteEventStore::open_in_memory()?;

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

        assert_eq!(first.code(), 0);
        assert_eq!(
            first.message(),
            "backfill source adapters: adapters 5, events 6"
        );
        assert_eq!(second.code(), 0);
        assert_eq!(
            second.message(),
            "backfill source adapters: adapters 5, events 6"
        );
        assert_eq!(store.list_console_events()?.len(), 6);
        assert_eq!(
            store.load_checkpoint("orchestrator:livespec-console-beads-fabro")?,
            Some("1".to_owned())
        );
        assert_eq!(
            store.load_checkpoint("dispatcher:livespec-console-beads-fabro")?,
            Some("2".to_owned())
        );
        assert_eq!(
            store.load_checkpoint("fabro:livespec-console-beads-fabro")?,
            Some("3".to_owned())
        );
        assert_eq!(
            store.load_checkpoint("livespec:livespec-console-beads-fabro")?,
            Some("4".to_owned())
        );
        assert_eq!(
            store.load_checkpoint("github:livespec-console-beads-fabro")?,
            Some("5".to_owned())
        );
        Ok(())
    }

    #[test]
    fn source_backfill_rejects_empty_observed_at() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let scripted = scripted_source_list();
        let sources = scripted_source_refs(&scripted);
        let na_port = empty_needs_attention_port();
        let needs_attention = NeedsAttentionIngest::new(&na_port, "livespec-console-beads-fabro");

        let result = backfill_source_report(&mut store, "", &sources, &needs_attention);

        assert!(matches!(
            result,
            Err(ConsoleRuntimeError::Adapter(AdapterError::EmptyObservedAt))
        ));
        assert_eq!(store.list_console_events()?.len(), 0);
        Ok(())
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

            assert!(matches!(
                result,
                Err(ConsoleRuntimeError::Adapter(error)) if error == expected_error
            ));
        }
    }

    #[test]
    fn demo_backfill_report_counts_inserted_and_duplicate_events() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;

        let first = backfill_demo_report(&mut store, "2026-06-23T00:00:00Z")?;
        let second = backfill_demo_report(&mut store, "2026-06-23T00:00:01Z")?;

        assert_eq!(first, "backfill demo events: inserted 2, duplicate 0");
        assert_eq!(second, "backfill demo events: inserted 0, duplicate 2");
        Ok(())
    }

    #[test]
    fn store_backed_events_tail_reports_persisted_events() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        append_demo_events_to_store(&mut store, "2026-06-23T00:00:00Z")?;

        let output = run_with_store_scripted(
            &command_args(&["bin", "events", "tail"]),
            &mut store,
            "unused",
        );

        assert_eq!(output.code(), 0);
        assert!(output.message().contains("events tail"));
        assert!(output.message().contains("evt_demo_1"));
        assert!(output.message().contains("work_item.snapshot_observed"));
        assert!(output.message().contains("evt_demo_2"));
        Ok(())
    }

    #[test]
    fn store_backed_serve_bootstraps_empty_store() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;

        let output = run_with_store_scripted(
            &command_args(&["bin", "serve"]),
            &mut store,
            "2026-06-23T00:00:00Z",
        );

        assert_eq!(output.code(), 0);
        assert_eq!(
            output.message(),
            "serve: store ready\nbackfill events: 6\nevents: 6\nattention: 0\ncommands: 0\npending: 0\nfactory commands handled: 0\nwork-item commands handled: 0"
        );
        assert_eq!(store.list_console_events()?.len(), 6);
        assert_eq!(
            store.load_checkpoint("github:livespec-console-beads-fabro")?,
            Some("5".to_owned())
        );
        Ok(())
    }

    #[test]
    fn store_backed_serve_threads_injected_drain_port() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let persistence = persist_tui_runtime_effects(
            &mut store,
            &[factory_drain_effect()],
            "2026-06-23T00:00:01Z",
        );
        assert!(persistence.is_ok());

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
            &needs_attention,
        );

        assert_eq!(output.code(), 0);
        assert_eq!(
            output.message(),
            "serve: store ready\nbackfill events: 8\nevents: 11\nattention: 0\ncommands: 1\npending: 0\nfactory commands handled: 1\nwork-item commands handled: 0"
        );
        assert_eq!(store.list_commands()?[0].status(), "completed");
        Ok(())
    }

    #[test]
    fn store_backed_serve_does_not_backfill_non_empty_store() -> Result<(), ConsoleRuntimeError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        append_demo_events_to_store(&mut store, "2026-06-23T00:00:00Z")?;
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
            &needs_attention,
        );

        assert_eq!(
            report?,
            "serve: store ready\nbackfill events: 0\nevents: 2\nattention: 0\ncommands: 0\npending: 0\nfactory commands handled: 0\nwork-item commands handled: 0"
        );
        Ok(())
    }

    #[test]
    fn store_backed_events_tail_reports_empty_store() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;

        let output = run_with_store_scripted(
            &command_args(&["bin", "events", "tail"]),
            &mut store,
            "unused",
        );

        assert_eq!(output.code(), 0);
        assert_eq!(output.message(), "events tail: no events");
        Ok(())
    }

    #[test]
    fn store_backed_events_usage_keeps_error_code() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;

        let output =
            run_with_store_scripted(&command_args(&["bin", "events"]), &mut store, "unused");

        assert_eq!(output.code(), 2);
        assert_eq!(
            output.message(),
            "usage: livespec-console-beads-fabro events tail"
        );
        Ok(())
    }

    #[test]
    fn store_backed_runner_falls_back_to_static_commands() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;

        let output = run_with_store_scripted(&command_args(&["bin", "help"]), &mut store, "unused");

        assert_eq!(output.code(), 0);
        assert!(output.message().contains("Commands:"));
        Ok(())
    }

    #[test]
    fn store_result_reports_event_store_errors() {
        let output = super::run_store_result(Err(EventStoreError::InvalidSequence), "snapshot");

        assert_eq!(output.code(), 1);
        assert_eq!(output.message(), "snapshot error: InvalidSequence");
    }

    #[test]
    fn runtime_result_reports_console_runtime_errors() {
        let output = super::run_runtime_result(
            Err(ConsoleRuntimeError::Application(
                ApplicationError::FactoryDrainPortFailed,
            )),
            "serve",
        );

        assert_eq!(output.code(), 1);
        assert_eq!(
            output.message(),
            "serve error: Application(FactoryDrainPortFailed)"
        );
    }

    #[test]
    fn store_backed_snapshot_reports_projection_counts() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        append_demo_events_to_store(&mut store, "2026-06-23T00:00:00Z")?;
        let persistence = persist_tui_runtime_effects(
            &mut store,
            &[factory_drain_effect()],
            "2026-06-23T00:00:01Z",
        );
        assert!(persistence.is_ok());

        let output =
            run_with_store_scripted(&command_args(&["bin", "snapshot"]), &mut store, "unused");

        assert_eq!(output.code(), 0);
        assert_eq!(
            output.message(),
            "snapshot: events 2, attention 0, commands 1, pending 1"
        );
        Ok(())
    }

    #[test]
    fn store_backed_doctor_reports_no_findings_with_store_counts() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        append_demo_events_to_store(&mut store, "2026-06-23T00:00:00Z")?;

        let output =
            run_with_store_scripted(&command_args(&["bin", "doctor"]), &mut store, "unused");

        assert_eq!(output.code(), 0);
        assert_eq!(
            output.message(),
            "doctor: no findings\nstore events: 2\ncommands: 0\nattention: 0"
        );
        Ok(())
    }

    #[test]
    fn store_report_helpers_match_command_output() -> Result<(), ConsoleRuntimeError> {
        let mut store = SqliteEventStore::open_in_memory()?;
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
        assert_eq!(backfill?, "backfill source adapters: adapters 5, events 6");
        assert!(events_tail_report(&store, 1)?.contains("pr.snapshot_observed"));
        // Attention is now sourced from the `attention_item.*` stream; a store of
        // work-item snapshots alone carries no attention items until the
        // needs-attention snapshot is ingested (Scenario 12).
        assert_eq!(
            snapshot_report(&store)?,
            "snapshot: events 6, attention 0, commands 0, pending 0"
        );
        assert_eq!(
            doctor_report(&store)?,
            "doctor: no findings\nstore events: 6\ncommands: 0\nattention: 0"
        );
        Ok(())
    }

    #[test]
    fn tui_preview_reports_render_errors() {
        let model = build_tui_model(&[], 0);

        assert_eq!(
            render_tui_preview(&model, 0, 28),
            "TUI render error: empty area"
        );
    }

    #[test]
    fn tui_persistence_stores_command_effects() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
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

        let outcomes = persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z")?;
        let commands = store.list_commands()?;

        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].status(), CommandAppendStatus::Inserted);
        assert_eq!(
            outcomes[0].command_id(),
            "cmd_factory_drain_requested_budget_1_parallel_1"
        );
        assert_eq!(commands.len(), 1);
        assert_eq!(
            commands[0].command_id(),
            "cmd_factory_drain_requested_budget_1_parallel_1"
        );
        assert_eq!(commands[0].command_type(), "factory.drain_requested");
        assert_eq!(commands[0].aggregate_id(), Some("fleet:livespec"));
        assert_eq!(
            commands[0].idempotency_key(),
            "fleet:livespec:factory.drain_requested:budget=1:parallel=1"
        );
        assert_eq!(commands[0].requested_by(), "operator");
        assert_eq!(commands[0].status(), "pending");
        Ok(())
    }

    #[test]
    fn tui_persistence_ignores_local_only_effects() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let effects = [
            TuiRuntimeEffect::Render,
            TuiRuntimeEffect::OpenAttachCommand("fabro attach run_1".to_owned()),
            TuiRuntimeEffect::CopyAttachCommand("fabro attach run_1".to_owned()),
            TuiRuntimeEffect::ApplicationError(ApplicationError::NoSelectedOperatorAction),
            TuiRuntimeEffect::Quit,
        ];

        let outcomes = persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z")?;
        let commands = store.list_commands()?;

        assert_eq!(outcomes, []);
        assert_eq!(commands, []);
        Ok(())
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

        assert!(matches!(outcome, Err(EventStoreError::InvalidSequence)));
    }

    #[test]
    fn store_backed_tui_session_backfills_runs_tui_and_handles_factory_command()
    -> Result<(), ConsoleRuntimeError> {
        let mut store = SqliteEventStore::open_in_memory()?;
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
            &needs_attention,
        );
        let commands = store.list_commands()?;

        assert!(matches!(
            outcome,
            Ok(ref value) if value == &TuiSessionOutcome::new(8, 8, 1, 1, 11, 0)
        ));
        assert!(matches!(
            outcome
                .as_ref()
                .map(TuiSessionOutcome::backfilled_event_count),
            Ok(8)
        ));
        assert!(matches!(
            outcome
                .as_ref()
                .map(TuiSessionOutcome::presented_event_count),
            Ok(8)
        ));
        assert!(matches!(
            outcome
                .as_ref()
                .map(TuiSessionOutcome::persisted_command_count),
            Ok(1)
        ));
        assert!(matches!(
            outcome
                .as_ref()
                .map(TuiSessionOutcome::handled_command_count),
            Ok(1)
        ));
        assert!(matches!(
            outcome.as_ref().map(TuiSessionOutcome::final_event_count),
            Ok(11)
        ));
        assert!(matches!(
            outcome.as_ref().map(TuiSessionOutcome::attention_count),
            Ok(0)
        ));
        assert_eq!(runner.observed_event_count(), 8);
        assert_eq!(runner.observed_requested_by(), "operator");
        assert_eq!(commands[0].status(), "completed");
        Ok(())
    }

    #[test]
    fn store_backed_tui_session_uses_existing_events_without_backfill()
    -> Result<(), ConsoleRuntimeError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        append_demo_events_to_store(&mut store, "2026-06-23T00:00:00Z")?;
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
            &needs_attention,
        );

        assert!(matches!(
            outcome,
            Ok(ref value) if value == &TuiSessionOutcome::new(0, 2, 0, 0, 2, 0)
        ));
        assert_eq!(runner.observed_event_count(), 2);
        assert_eq!(store.list_console_events()?.len(), 2);
        Ok(())
    }

    #[test]
    fn store_backed_tui_session_reports_runner_errors() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
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
            &needs_attention,
        );

        assert!(matches!(
            outcome,
            Err(ConsoleRuntimeError::TuiRuntimeFailed)
        ));
        assert_eq!(store.list_console_events()?.len(), 6);
        Ok(())
    }

    #[test]
    fn ingest_needs_attention_diffs_snapshot_into_stream_and_projects_inbox()
    -> Result<(), ConsoleRuntimeError> {
        let mut store = SqliteEventStore::open_in_memory()?;

        // First ingest against an empty prior: both items appear.
        let first_port = ScriptedNeedsAttentionPort::observing(vec![
            attention_item_fixture("wi-approve", "Pending approval"),
            attention_item_fixture("wi-accept", "Acceptance review"),
        ]);
        let first = NeedsAttentionIngest::new(&first_port, "livespec-console-beads-fabro");
        let appeared = ingest_needs_attention(&mut store, &first, "2026-07-07T00:00:00Z")?;
        assert_eq!(appeared, 2);
        assert_eq!(
            attention_event_count(
                &store.list_console_events()?,
                EventType::AttentionItemAppeared
            ),
            2
        );
        assert_eq!(project_attention(&store.list_console_events()?).len(), 2);

        // Re-ingest the identical snapshot: idempotent, emits nothing.
        let unchanged = ingest_needs_attention(&mut store, &first, "2026-07-07T00:01:00Z")?;
        assert_eq!(unchanged, 0);
        assert_eq!(store.list_console_events()?.len(), 2);

        // Second ingest: wi-approve changes, wi-accept resolves, wi-blocked appears.
        let second_port = ScriptedNeedsAttentionPort::observing(vec![
            attention_item_fixture("wi-approve", "Pending approval (urgent)"),
            attention_item_fixture("wi-blocked", "Blocked: needs-human"),
        ]);
        let second = NeedsAttentionIngest::new(&second_port, "livespec-console-beads-fabro");
        let ingested = ingest_needs_attention(&mut store, &second, "2026-07-07T00:02:00Z")?;
        assert_eq!(ingested, 3);

        let events = store.list_console_events()?;
        assert_eq!(
            attention_event_count(&events, EventType::AttentionItemChanged),
            1
        );
        assert_eq!(
            attention_event_count(&events, EventType::AttentionItemResolved),
            1
        );
        assert_eq!(
            attention_event_count(&events, EventType::AttentionItemAppeared),
            3
        );

        let inbox = project_attention(&events);
        let ids: Vec<&str> = inbox.iter().map(AttentionItem::id).collect();
        assert_eq!(ids, ["wi-approve", "wi-blocked"]);
        let approve = &inbox[0];
        assert_eq!(approve.title(), "Pending approval (urgent)");
        assert_eq!(approve.source(), "human-valve");
        assert_eq!(
            approve.source_reference(),
            "livespec-console-beads-fabro:wi-approve"
        );
        Ok(())
    }

    #[test]
    fn ingest_needs_attention_preserves_inbox_when_source_unavailable()
    -> Result<(), ConsoleRuntimeError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let present = ScriptedNeedsAttentionPort::observing(vec![attention_item_fixture(
            "wi-approve",
            "Pending approval",
        )]);
        let ingest = NeedsAttentionIngest::new(&present, "livespec-console-beads-fabro");
        assert_eq!(
            ingest_needs_attention(&mut store, &ingest, "2026-07-07T00:00:00Z")?,
            1
        );

        // An unavailable read must NOT resolve the inbox from a failed poll.
        let down = ScriptedNeedsAttentionPort::unavailable("needs-attention: binary missing");
        let ingest_down = NeedsAttentionIngest::new(&down, "livespec-console-beads-fabro");
        assert_eq!(
            ingest_needs_attention(&mut store, &ingest_down, "2026-07-07T00:01:00Z")?,
            0
        );
        assert_eq!(project_attention(&store.list_console_events()?).len(), 1);
        Ok(())
    }

    fn attention_event_count(events: &[ConsoleEvent], event_type: EventType) -> usize {
        events
            .iter()
            .filter(|event| event.event_type() == &event_type)
            .count()
    }

    #[test]
    fn runtime_error_conversions_keep_source_context() {
        assert!(matches!(
            ConsoleRuntimeError::from(ApplicationError::FactoryDrainPortFailed),
            ConsoleRuntimeError::Application(ApplicationError::FactoryDrainPortFailed)
        ));
        assert!(matches!(
            ConsoleRuntimeError::from(EventStoreError::InvalidSequence),
            ConsoleRuntimeError::EventStore(EventStoreError::InvalidSequence)
        ));
    }

    #[test]
    fn command_status_update_runtime_result_maps_success_and_failure() {
        let success = command_status_update_runtime_result(Ok(CommandStatusUpdateOutcome::new(
            "cmd_1".to_owned(),
            "completed".to_owned(),
        )));
        let failure = command_status_update_runtime_result(Err(EventStoreError::InvalidSequence));

        assert!(matches!(
            success,
            Ok(outcome)
                if outcome.command_id() == "cmd_1" && outcome.status() == "completed"
        ));
        assert!(matches!(
            failure,
            Err(ConsoleRuntimeError::EventStore(
                EventStoreError::InvalidSequence
            ))
        ));
    }

    #[test]
    fn pending_factory_commands_append_lifecycle_events_and_complete()
    -> Result<(), ConsoleRuntimeError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let effects = [factory_drain_effect()];
        persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z")?;
        append_ready_work_item(&mut store, "2026-06-23T00:00:02Z")?;
        let mut port = SimulatedFactoryDrainPort;

        let outcomes =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port)?;
        let commands = store.list_commands()?;
        let events = store.list_console_events()?;

        assert_eq!(outcomes.len(), 1);
        assert_eq!(
            outcomes[0],
            super::PendingCommandOutcome::new(
                "cmd_factory_drain_requested_budget_1_parallel_1".to_owned(),
                "completed".to_owned(),
                3,
            )
        );
        assert_eq!(
            outcomes[0].command_id(),
            "cmd_factory_drain_requested_budget_1_parallel_1"
        );
        assert_eq!(outcomes[0].command_status(), "completed");
        assert_eq!(outcomes[0].appended_event_count(), 3);
        assert_eq!(commands[0].status(), "completed");
        assert_eq!(
            events
                .iter()
                .map(ConsoleEvent::event_type)
                .collect::<Vec<_>>(),
            [
                &EventType::WorkItemSnapshotObserved,
                &EventType::CommandAccepted,
                &EventType::FactoryDrainStarted,
                &EventType::FactoryDrainCompleted
            ]
        );
        Ok(())
    }

    #[test]
    fn pending_factory_commands_record_failed_port_outcome() -> Result<(), ConsoleRuntimeError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let effects = [factory_drain_effect()];
        persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z")?;
        append_ready_work_item(&mut store, "2026-06-23T00:00:02Z")?;
        let mut port = FailedFactoryDrainPort;

        let outcomes =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port)?;
        let commands = store.list_commands()?;
        let events = store.list_console_events()?;

        assert_eq!(outcomes[0].command_status(), "failed");
        assert_eq!(commands[0].status(), "failed");
        assert_eq!(
            events.last().map(ConsoleEvent::event_type),
            Some(&EventType::FactoryDrainFailed)
        );
        Ok(())
    }

    #[test]
    fn pending_factory_commands_return_status_update_errors() {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::StatusUpdateFails);
        let mut port = SimulatedFactoryDrainPort;

        let outcome =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port);

        assert!(matches!(
            outcome,
            Err(ConsoleRuntimeError::EventStore(
                EventStoreError::InvalidSequence
            ))
        ));
        assert_eq!(store.appended_event_count, 3);
    }

    #[test]
    fn pending_factory_commands_return_list_errors() {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::ListFails);
        let mut port = SimulatedFactoryDrainPort;

        let outcome =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port);

        assert!(matches!(
            outcome,
            Err(ConsoleRuntimeError::EventStore(
                EventStoreError::InvalidSequence
            ))
        ));
    }

    #[test]
    fn pending_factory_commands_return_missing_aggregate_errors() {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::MissingAggregate);
        let mut port = SimulatedFactoryDrainPort;

        let outcome =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port);

        assert!(matches!(
            outcome,
            Err(ConsoleRuntimeError::MissingCommandAggregate(command_id))
                if command_id == "cmd_missing_aggregate"
        ));
    }

    #[test]
    fn pending_factory_commands_return_port_errors() {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::StatusUpdateFails);
        let mut port = ErroringFactoryDrainPort;

        let outcome =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port);

        assert!(matches!(
            outcome,
            Err(ConsoleRuntimeError::Application(
                ApplicationError::FactoryDrainPortFailed
            ))
        ));
        assert_eq!(store.appended_event_count, 0);
    }

    #[test]
    fn pending_factory_commands_return_append_errors() {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::AppendFails);
        let mut port = SimulatedFactoryDrainPort;

        let outcome =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port);

        assert!(matches!(
            outcome,
            Err(ConsoleRuntimeError::EventStore(
                EventStoreError::InvalidSequence
            ))
        ));
    }

    #[test]
    fn scripted_factory_command_store_supports_successful_handling()
    -> Result<(), ConsoleRuntimeError> {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::Completes);
        let mut port = SimulatedFactoryDrainPort;

        let outcomes =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port)?;

        assert_eq!(
            outcomes,
            vec![PendingCommandOutcome::new(
                "cmd_factory_drain_requested_budget_1_parallel_1".to_owned(),
                "completed".to_owned(),
                3,
            )]
        );
        assert_eq!(store.appended_event_count, 3);
        Ok(())
    }

    #[test]
    fn pending_factory_command_handler_skips_non_factory_or_non_pending()
    -> Result<(), ConsoleRuntimeError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let effects = [TuiRuntimeEffect::PersistCommand(CommandEnvelope::new(
            "cmd_factory_drain_requested_budget_1_parallel_1".to_owned(),
            CommandType::FactoryDrainRequested,
            "fleet:livespec".to_owned(),
            "fleet:livespec:factory.drain_requested:budget=1:parallel=1".to_owned(),
            "operator".to_owned(),
        ))];
        persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z")?;
        let update = store.update_command_status(
            "cmd_factory_drain_requested_budget_1_parallel_1",
            "completed",
            "2026-06-23T00:00:03Z",
            Some("{}"),
            None,
        );
        assert!(update.is_ok());
        let mut port = SimulatedFactoryDrainPort;

        let outcomes =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port)?;

        assert_eq!(outcomes, []);
        assert_eq!(store.list_console_events()?, []);
        Ok(())
    }

    #[test]
    fn pending_factory_command_handler_skips_pending_non_factory_commands()
    -> Result<(), ConsoleRuntimeError> {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::NonFactoryPending);
        let mut port = SimulatedFactoryDrainPort;

        let outcomes =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port)?;

        assert_eq!(outcomes, []);
        assert_eq!(store.appended_event_count, 0);
        Ok(())
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

        assert!(matches!(
            factory_command_from_stored(&stored_command),
            Ok(None)
        ));
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

        assert!(matches!(
            result,
            Err(ConsoleRuntimeError::MissingCommandAggregate(command_id)) if command_id == "cmd_1"
        ));
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

        assert!(matches!(
            work_item_command_from_stored(&stored_command),
            Ok(None)
        ));
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

        let result = work_item_command_from_stored(&stored_command);

        assert!(matches!(
            result,
            Err(ConsoleRuntimeError::MissingCommandAggregate(command_id)) if command_id == "cmd_approve"
        ));
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
    fn pending_work_item_accept_dispatches_through_port() -> Result<(), ConsoleRuntimeError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        store.append_command(&accept_command_append())?;
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcomes =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port)?;

        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].command_status(), "completed");
        assert_eq!(port.observed_action_ids, ["accept:wi-1"]);
        Ok(())
    }

    #[test]
    fn pending_work_item_reject_routes_mode_from_payload_through_port()
    -> Result<(), ConsoleRuntimeError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        store.append_command(&reject_command_append(r#"{"mode":"regroom"}"#))?;
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcomes =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port)?;

        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].command_status(), "completed");
        // The mode extracted from the stored payload lands in the action-id.
        assert_eq!(port.observed_action_ids, ["reject:wi-1:regroom"]);
        Ok(())
    }

    #[test]
    fn pending_work_item_reject_surfaces_invalid_mode_as_application_error()
    -> Result<(), ConsoleRuntimeError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        store.append_command(&reject_command_append(r#"{"mode":"bogus"}"#))?;
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port);

        assert!(matches!(
            outcome,
            Err(ConsoleRuntimeError::Application(
                ApplicationError::InvalidRejectMode
            ))
        ));
        assert_eq!(port.observed_action_ids, [] as [String; 0]);
        Ok(())
    }

    #[test]
    fn pending_work_item_set_admission_routes_policy_from_payload_through_port()
    -> Result<(), ConsoleRuntimeError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        store.append_command(&set_admission_command_append(r#"{"policy":"auto"}"#))?;
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcomes =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port)?;

        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].command_status(), "completed");
        // The policy extracted from the stored payload lands in the action-id.
        assert_eq!(port.observed_action_ids, ["set-admission:wi-1:auto"]);
        Ok(())
    }

    #[test]
    fn pending_work_item_set_admission_surfaces_invalid_policy_as_application_error()
    -> Result<(), ConsoleRuntimeError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        store.append_command(&set_admission_command_append(r#"{"policy":"bogus"}"#))?;
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port);

        assert!(matches!(
            outcome,
            Err(ConsoleRuntimeError::Application(
                ApplicationError::InvalidAdmissionPolicy
            ))
        ));
        assert_eq!(port.observed_action_ids, [] as [String; 0]);
        Ok(())
    }

    #[test]
    fn pending_work_item_set_acceptance_routes_policy_from_payload_through_port()
    -> Result<(), ConsoleRuntimeError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        store.append_command(&set_acceptance_command_append(r#"{"policy":"ai-only"}"#))?;
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcomes =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port)?;

        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].command_status(), "completed");
        // The policy extracted from the stored payload lands in the action-id.
        assert_eq!(port.observed_action_ids, ["set-acceptance:wi-1:ai-only"]);
        Ok(())
    }

    #[test]
    fn pending_work_item_set_acceptance_surfaces_invalid_policy_as_application_error()
    -> Result<(), ConsoleRuntimeError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        store.append_command(&set_acceptance_command_append(r#"{"policy":"bogus"}"#))?;
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port);

        assert!(matches!(
            outcome,
            Err(ConsoleRuntimeError::Application(
                ApplicationError::InvalidAcceptancePolicy
            ))
        ));
        assert_eq!(port.observed_action_ids, [] as [String; 0]);
        Ok(())
    }

    #[test]
    fn pending_work_item_approve_dispatches_through_port_and_skips_others()
    -> Result<(), ConsoleRuntimeError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        store.append_command(&approve_command_append())?;
        // A pending factory command must be skipped by the work-item handler.
        store.append_command(&CommandAppend::new(
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
        ))?;
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcomes =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port)?;

        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].command_status(), "completed");
        assert_eq!(outcomes[0].appended_event_count(), 3);
        assert_eq!(port.observed_action_ids, ["approve:wi-1"]);
        // The skipped factory command produces no events, so the store carries
        // exactly the shared work_item outcome family for the approve.
        let events = store.list_console_events()?;
        assert_eq!(
            events
                .iter()
                .map(ConsoleEvent::event_type)
                .collect::<Vec<_>>(),
            [
                &EventType::CommandAccepted,
                &EventType::WorkItemActionStarted,
                &EventType::WorkItemActionCompleted,
            ]
        );

        // Second pass: the approve command is now non-pending and skipped, and
        // the factory command is still not a work-item command, so nothing runs.
        let second =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:02Z", &mut port)?;
        assert!(second.is_empty());
        Ok(())
    }

    #[test]
    fn pending_work_item_approve_records_not_wired_without_fabricating_start()
    -> Result<(), ConsoleRuntimeError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        store.append_command(&approve_command_append())?;
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::not_wired());

        let outcomes =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:01Z", &mut port)?;

        assert_eq!(outcomes[0].command_status(), "not_wired");
        assert_eq!(store.list_commands()?[0].status(), "not_wired");
        assert_eq!(
            store
                .list_console_events()?
                .iter()
                .map(ConsoleEvent::event_type)
                .collect::<Vec<_>>(),
            [
                &EventType::CommandAccepted,
                &EventType::WorkItemActionNotWired
            ]
        );
        Ok(())
    }

    #[test]
    fn pending_work_item_commands_propagate_store_errors() {
        let mut store = ScriptedFactoryCommandStore::new(ScriptedStoreMode::WorkItemAppendFails);
        let mut port =
            SimulatedWorkItemActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome =
            handle_pending_work_item_commands(&mut store, "2026-07-10T00:00:03Z", &mut port);

        assert!(matches!(
            outcome,
            Err(ConsoleRuntimeError::EventStore(
                EventStoreError::InvalidSequence
            ))
        ));
    }

    #[test]
    fn simulated_factory_drain_port_rejects_unbounded_request() {
        let mut port = SimulatedFactoryDrainPort;
        let request = FactoryDrainRequest::new("fleet:livespec".to_owned(), 0, 1);

        let outcome = port.drain_ready_queue(&request);

        assert_eq!(outcome, Err(ApplicationError::FactoryDrainPortFailed));

        let request = FactoryDrainRequest::new("fleet:livespec".to_owned(), 1, 0);

        let outcome = port.drain_ready_queue(&request);

        assert_eq!(outcome, Err(ApplicationError::FactoryDrainPortFailed));
    }

    #[test]
    fn live_source_adapters_observe_each_source_through_the_probe()
    -> Result<(), ConsoleRuntimeError> {
        let probe = UnavailableProbe;
        let adapters = live_source_adapters(&probe, "console")?;

        let adapter_ids: Vec<&str> = adapters
            .iter()
            .map(|(adapter_id, _adapter)| adapter_id.as_str())
            .collect();
        assert_eq!(
            adapter_ids,
            [
                "orchestrator:console",
                "dispatcher:console",
                "fabro:console",
                "livespec:console",
                "github:console",
            ]
        );

        // Polling every adapter exercises both probe capabilities (commands and
        // the Dispatcher journal file). The probe reports every source
        // unavailable, so each adapter emits one honest not-observed finding
        // rather than a fabricated snapshot.
        let refs: Vec<SourceAdapterRef<'_>> = adapters
            .iter()
            .map(|(adapter_id, adapter)| (adapter_id.as_str(), adapter as &dyn PullSourcePort))
            .collect();
        let mut store = SqliteEventStore::open_in_memory()?;
        let summaries = backfill_source_adapters(&mut store, "2026-06-25T00:00:00Z", &refs)?;

        assert_eq!(summaries.len(), 5);
        assert_eq!(store.list_console_events()?.len(), 5);
        for event in store.list_console_events()? {
            assert_eq!(
                event.event_type().contract_name(),
                "source.not_observed_finding_observed"
            );
        }
        Ok(())
    }

    #[test]
    fn live_source_adapters_rejects_empty_repo() {
        let probe = UnavailableProbe;

        let result = live_source_adapters(&probe, "  ");

        assert!(matches!(
            result,
            Err(ConsoleRuntimeError::Adapter(AdapterError::EmptyRepo))
        ));
    }

    #[test]
    fn demo_backfill_round_trips_through_event_store() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;

        let outcomes = append_demo_events_to_store(&mut store, "2026-06-23T00:00:00Z")?;
        let events = load_tui_events_from_store(&store)?;

        assert_eq!(outcomes.len(), 2);
        assert_eq!(outcomes[0].status(), AppendStatus::Inserted);
        assert_eq!(outcomes[1].status(), AppendStatus::Inserted);
        assert_eq!(events, persisted_demo_events());
        Ok(())
    }

    #[test]
    fn demo_backfill_reports_event_append_errors() {
        let mut store = EventAppendFailingStore;

        let outcome = append_demo_events_to_store(&mut store, "2026-06-23T00:00:00Z");

        assert!(matches!(outcome, Err(EventStoreError::InvalidSequence)));
    }

    #[test]
    fn demo_backfill_is_idempotent_by_source_event_id() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;

        let first = append_demo_events_to_store(&mut store, "2026-06-23T00:00:00Z")?;
        let second = append_demo_events_to_store(&mut store, "2026-06-23T00:00:00Z")?;
        let events = load_tui_events_from_store(&store)?;

        assert_eq!(first[0].status(), AppendStatus::Inserted);
        assert_eq!(second[0].status(), AppendStatus::Duplicate);
        assert_eq!(second[1].status(), AppendStatus::Duplicate);
        assert_eq!(events, persisted_demo_events());
        Ok(())
    }

    #[test]
    fn backfilled_work_item_snapshot_rebuilds_into_its_lane() -> Result<(), ConsoleRuntimeError> {
        let scripted = scripted_source_list();
        let sources = scripted_source_refs(&scripted);
        let mut store = SqliteEventStore::open_in_memory()?;
        backfill_source_adapters(&mut store, "2026-06-25T00:00:00Z", &sources)?;

        // The lane board rebuilds purely from the persisted snapshot payloads:
        // the seeded work-item is emitted as blocked:needs-human at rank "a1".
        let events = store.list_console_events()?;
        let board = project_lane_board(&events);

        assert_eq!(board.column(Lane::Blocked).map(LaneColumn::count), Some(1));
        let blocked_items = board
            .column(Lane::Blocked)
            .map(LaneColumn::items)
            .unwrap_or_default();
        assert_eq!(
            blocked_items[0].work_item_id(),
            "livespec-console-beads-fabro-y45jhj"
        );
        assert_eq!(blocked_items[0].rank(), "a1");
        assert_eq!(blocked_items[0].lane_reason(), Some(LaneReason::NeedsHuman));
        assert_eq!(board.total(), 1);
        Ok(())
    }

    /// The demo events as they are read back from the store, where the load
    /// path re-attaches the persisted (empty) `payload_json` that in-memory
    /// envelopes carry as `None`.
    fn persisted_demo_events() -> Vec<ConsoleEvent> {
        demo_events().into_iter().collect()
    }

    fn append_ready_work_item(
        store: &mut SqliteEventStore,
        observed_at: &str,
    ) -> Result<(), EventStoreError> {
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
        store.append_event(&EventAppend::new(
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
        ))?;
        Ok(())
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

    fn command_args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
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

    struct FailedFactoryDrainPort;

    impl FactoryDrainPort for FailedFactoryDrainPort {
        fn drain_ready_queue(
            &mut self,
            _request: &FactoryDrainRequest,
        ) -> Result<FactoryDrainPortOutcome, ApplicationError> {
            Ok(FactoryDrainPortOutcome::failed())
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
        ) -> ConsoleRuntimeResult<Vec<TuiRuntimeEffect>> {
            self.observed_event_count = events.len();
            self.observed_requested_by = requested_by.to_owned();
            Ok(self.effects.clone())
        }
    }

    struct ErroringTuiSessionRunner;

    impl TuiSessionRunner for ErroringTuiSessionRunner {
        fn run_tui(
            &mut self,
            _events: &[ConsoleEvent],
            _requested_by: &str,
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
        Completes,
        ListFails,
        MissingAggregate,
        NonFactoryPending,
        StatusUpdateFails,
        WorkItemAppendFails,
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
            if self.mode == ScriptedStoreMode::WorkItemAppendFails {
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

        fn append_event(&mut self, _append: &EventAppend) -> EventStoreResult<AppendOutcome> {
            if matches!(
                self.mode,
                ScriptedStoreMode::AppendFails | ScriptedStoreMode::WorkItemAppendFails
            ) {
                return Err(EventStoreError::InvalidSequence);
            }
            self.appended_event_count += 1;
            Ok(AppendOutcome::new(1, AppendStatus::Inserted))
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
    }
}
