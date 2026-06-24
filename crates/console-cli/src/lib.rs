#![forbid(unsafe_code)]

use std::cell::RefCell;
use std::rc::Rc;

use console_application::{
    ApplicationError, FactoryDrainPort, FactoryDrainPortOutcome, FactoryDrainRequest,
    build_tui_model, handle_factory_drain_command, project_attention,
    source_adapters::{
        AdapterError, AdapterIngestionSummary, AdapterPoll, AdapterPollRequest,
        BeadsWorkItemSnapshot, BeadsWorkItemStatus, DispatcherJournalEntry, DispatcherJournalKind,
        FabroRunSnapshot, FabroRunState, GithubPullRequestSnapshot, GithubPullRequestState,
        LivespecNextAction, LivespecNextSnapshot, NormalizedSourceEvent, PullSourcePort,
        SourceCheckpointPort, SourceEventAppendPort, normalize_beads_snapshot,
        normalize_dispatcher_journal_entry, normalize_fabro_run_snapshot,
        normalize_github_pull_request_snapshot, normalize_livespec_next_snapshot, run_adapter_poll,
    },
};
use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};
use console_eventstore::{
    AppendOutcome, AppendStatus, CommandAppend, CommandAppendOutcome, CommandStatusUpdateOutcome,
    EventAppend, EventStoreError, EventStoreResult, SqliteEventStore, StoredCommand,
};
use console_tui::TuiRuntimeEffect;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutput {
    code: i32,
    message: String,
}

impl RunOutput {
    #[must_use]
    pub const fn new(code: i32, message: String) -> Self {
        Self { code, message }
    }

    #[must_use]
    pub const fn code(&self) -> i32 {
        self.code
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

pub fn run<I>(args: I) -> RunOutput
where
    I: IntoIterator,
    I::Item: Into<String>,
{
    let values = args.into_iter().map(Into::into).collect::<Vec<_>>();
    run_static(&values)
}

pub fn run_with_store(
    args: &[String],
    store: &mut SqliteEventStore,
    observed_at: &str,
) -> RunOutput {
    match command_name(args) {
        Some("serve") => run_runtime_result(serve_report(store, observed_at), "serve"),
        Some("backfill") => {
            run_runtime_result(backfill_source_report(store, observed_at), "backfill")
        }
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

pub trait CommandAppendStore {
    fn append_command(&mut self, append: &CommandAppend) -> EventStoreResult<CommandAppendOutcome>;
}

impl CommandAppendStore for SqliteEventStore {
    fn append_command(&mut self, append: &CommandAppend) -> EventStoreResult<CommandAppendOutcome> {
        Self::append_command(self, append)
    }
}

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

pub trait EventAppendStore {
    fn append_event(&mut self, append: &EventAppend) -> EventStoreResult<AppendOutcome>;
}

impl EventAppendStore for SqliteEventStore {
    fn append_event(&mut self, append: &EventAppend) -> EventStoreResult<AppendOutcome> {
        Self::append_event(self, append)
    }
}

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

pub fn load_tui_events_from_store(store: &SqliteEventStore) -> EventStoreResult<Vec<ConsoleEvent>> {
    store.list_console_events()
}

pub trait TuiSessionRunner {
    fn run_tui(
        &mut self,
        events: &[ConsoleEvent],
        requested_by: &str,
    ) -> ConsoleRuntimeResult<Vec<TuiRuntimeEffect>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    pub const fn backfilled_event_count(&self) -> usize {
        self.backfilled_events
    }

    #[must_use]
    pub const fn presented_event_count(&self) -> usize {
        self.presented_events
    }

    #[must_use]
    pub const fn persisted_command_count(&self) -> usize {
        self.persisted_commands
    }

    #[must_use]
    pub const fn handled_command_count(&self) -> usize {
        self.handled_commands
    }

    #[must_use]
    pub const fn final_event_count(&self) -> usize {
        self.final_events
    }

    #[must_use]
    pub const fn attention_count(&self) -> usize {
        self.attention_items
    }
}

pub fn run_store_backed_tui_session(
    store: &mut SqliteEventStore,
    observed_at: &str,
    requested_by: &str,
    runner: &mut dyn TuiSessionRunner,
    factory_port: &mut dyn FactoryDrainPort,
) -> ConsoleRuntimeResult<TuiSessionOutcome> {
    let existing_events = store.list_console_events()?;
    let ingestion = if existing_events.is_empty() {
        backfill_source_adapters(store, observed_at)?
    } else {
        Vec::new()
    };
    let presented_events = store.list_console_events()?;
    let effects = runner.run_tui(&presented_events, requested_by)?;
    let persisted = persist_tui_runtime_effects(store, &effects, observed_at)?;
    let handled = handle_pending_factory_commands(store, observed_at, factory_port)?;
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

pub fn backfill_source_report(
    store: &mut SqliteEventStore,
    observed_at: &str,
) -> ConsoleRuntimeResult<String> {
    let summaries = backfill_source_adapters(store, observed_at)?;
    let event_count: usize = summaries
        .iter()
        .map(AdapterIngestionSummary::appended_event_count)
        .sum();
    Ok(format!(
        "backfill source adapters: adapters {}, events {event_count}",
        summaries.len()
    ))
}

fn backfill_source_adapters(
    store: &mut SqliteEventStore,
    observed_at: &str,
) -> ConsoleRuntimeResult<Vec<AdapterIngestionSummary>> {
    let shared = SharedSqliteStore::new(store);
    let mut summaries = Vec::new();
    for (adapter_id, poll) in initial_source_polls()? {
        let source = ScriptedSource::new(poll);
        let mut checkpoints = SqliteCheckpointPort::new(shared.clone(), observed_at);
        let mut event_log = SqliteSourceEventLog::new(shared.clone());
        summaries.push(run_adapter_poll(
            adapter_id,
            1,
            observed_at,
            &source,
            &mut checkpoints,
            &mut event_log,
        )?);
    }
    Ok(summaries)
}

fn initial_source_polls() -> ConsoleRuntimeResult<Vec<(&'static str, AdapterPoll)>> {
    source_polls_from_seed(&initial_source_seed())
}

fn source_polls_from_seed(
    seed: &InitialSourceSeed<'_>,
) -> ConsoleRuntimeResult<Vec<(&'static str, AdapterPoll)>> {
    let beads_snapshot = BeadsWorkItemSnapshot::new(
        seed.repo,
        seed.work_item_id,
        BeadsWorkItemStatus::NeedsRegroom,
        1,
    )?;
    let dispatcher_entry = DispatcherJournalEntry::new(
        seed.repo,
        seed.work_item_id,
        seed.dispatch_id,
        DispatcherJournalKind::NeedsRegroom,
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
            "beads:livespec-console-beads-fabro",
            normalize_beads_snapshot(&beads_snapshot),
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

#[derive(Clone)]
struct InitialSourceSeed<'a> {
    repo: &'a str,
    work_item_id: &'a str,
    dispatch_id: &'a str,
    run_id: &'a str,
}

const fn initial_source_seed() -> InitialSourceSeed<'static> {
    InitialSourceSeed {
        repo: "livespec-console-beads-fabro",
        work_item_id: "livespec-console-beads-fabro-y45jhj",
        dispatch_id: "dispatch_1",
        run_id: "run_1",
    }
}

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

pub fn serve_report(
    store: &mut SqliteEventStore,
    observed_at: &str,
) -> ConsoleRuntimeResult<String> {
    let events = store.list_console_events()?;
    let ingestion = if events.is_empty() {
        backfill_source_adapters(store, observed_at)?
    } else {
        Vec::new()
    };
    let mut factory_port = NotWiredFactoryDrainPort;
    let handled = handle_pending_factory_commands(store, observed_at, &mut factory_port)?;
    let events = store.list_console_events()?;
    let commands = store.list_commands()?;
    let attention_count = project_attention(&events).len();
    let pending_count = count_commands_with_status(&commands, "pending");
    let backfill_event_count: usize = ingestion
        .iter()
        .map(AdapterIngestionSummary::appended_event_count)
        .sum();
    Ok(format!(
        "serve: store ready\nbackfill events: {backfill_event_count}\nevents: {}\nattention: {attention_count}\ncommands: {}\npending: {pending_count}\nfactory commands handled: {}",
        events.len(),
        commands.len(),
        handled.len()
    ))
}

fn count_commands_with_status(commands: &[StoredCommand], status: &str) -> usize {
    commands
        .iter()
        .filter(|command| command.status() == status)
        .count()
}

#[derive(Debug)]
pub enum ConsoleRuntimeError {
    Adapter(AdapterError),
    Application(ApplicationError),
    EventStore(EventStoreError),
    MissingCommandAggregate(String),
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

pub type ConsoleRuntimeResult<T> = Result<T, ConsoleRuntimeError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactoryCommandHandlingOutcome {
    command_id: String,
    command_status: String,
    appended_event_count: usize,
}

impl FactoryCommandHandlingOutcome {
    #[must_use]
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
    pub fn command_id(&self) -> &str {
        &self.command_id
    }

    #[must_use]
    pub fn command_status(&self) -> &str {
        &self.command_status
    }

    #[must_use]
    pub const fn appended_event_count(&self) -> usize {
        self.appended_event_count
    }
}

/// Honest factory-drain port for the live serve/tui path.
///
/// Used until a real Dispatcher-invoking port lands. No real Dispatcher is
/// wired, so this never fabricates a drain outcome: it reports `NotWired` so
/// the command handler records an honest not-wired event instead of a
/// fictitious success.
pub struct NotWiredFactoryDrainPort;

impl FactoryDrainPort for NotWiredFactoryDrainPort {
    fn drain_ready_queue(
        &mut self,
        _request: &FactoryDrainRequest,
    ) -> Result<FactoryDrainPortOutcome, ApplicationError> {
        Ok(FactoryDrainPortOutcome::not_wired())
    }
}

pub trait FactoryCommandStore {
    fn list_commands(&self) -> EventStoreResult<Vec<StoredCommand>>;

    fn append_event(&mut self, append: &EventAppend) -> EventStoreResult<AppendOutcome>;

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

pub fn handle_pending_factory_commands(
    store: &mut dyn FactoryCommandStore,
    handled_at: &str,
    port: &mut dyn FactoryDrainPort,
) -> ConsoleRuntimeResult<Vec<FactoryCommandHandlingOutcome>> {
    let mut outcomes = Vec::new();
    for stored_command in store.list_commands()? {
        if stored_command.status() != "pending" {
            continue;
        }
        let Some(command) = factory_command_from_stored(&stored_command)? else {
            continue;
        };
        let command_outcome = handle_factory_drain_command(&command, port)?;
        for event in command_outcome.events() {
            let append = event_append_from_command_event(event, &command, handled_at);
            store.append_event(&append)?;
        }
        let result_json = format!(r#"{{"event_count":{}}}"#, command_outcome.events().len());
        let error_json = if command_outcome.command_status() == "failed" {
            Some("{}")
        } else {
            None
        };
        let status_update = store.update_command_status(
            command.command_id(),
            command_outcome.command_status(),
            handled_at,
            Some(&result_json),
            error_json,
        );
        let status_outcome = command_status_update_runtime_result(status_update)?;
        outcomes.push(FactoryCommandHandlingOutcome::new(
            status_outcome.command_id().to_owned(),
            status_outcome.status().to_owned(),
            command_outcome.events().len(),
        ));
    }
    Ok(outcomes)
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
        "{}".to_owned(),
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
        "{}".to_owned(),
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
        "{}".to_owned(),
        "{}".to_owned(),
    )
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

struct ScriptedSource {
    poll: AdapterPoll,
}

impl ScriptedSource {
    const fn new(poll: AdapterPoll) -> Self {
        Self { poll }
    }
}

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
pub fn demo_events() -> [ConsoleEvent; 2] {
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
        ApplicationError, FactoryDrainPort, FactoryDrainPortOutcome, FactoryDrainRequest,
        build_tui_model, source_adapters::AdapterError,
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
        FactoryCommandHandlingOutcome, FactoryCommandStore, InitialSourceSeed,
        NotWiredFactoryDrainPort, TuiSessionOutcome, TuiSessionRunner, append_demo_events_to_store,
        backfill_demo_report, backfill_source_report, command_status_update_runtime_result,
        demo_events, doctor_report, events_tail_report, factory_command_from_stored,
        handle_pending_factory_commands, initial_source_seed, load_tui_events_from_store,
        persist_tui_runtime_effects, render_tui_preview, run, run_store_backed_tui_session,
        run_with_store, serve_report, snapshot_report, source_polls_from_seed,
    };

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
        assert!(output.message().contains("> Fabro human gate"));
        assert!(
            output
                .message()
                .contains("Repo: livespec-console-beads-fabro")
        );
        assert!(output.message().contains("Fabro run: run_demo_1"));
        assert!(output.message().contains("Attach: fabro attach run_demo_1"));
        assert!(
            output
                .message()
                .contains("Actions: Acknowledge, Snooze, Open Fabro")
        );
        assert!(output.message().contains("attach, Copy Fabro attach"));
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

        let first = run_with_store(
            &command_args(&["bin", "backfill"]),
            &mut store,
            "2026-06-23T00:00:00Z",
        );
        let second = run_with_store(
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
            store.load_checkpoint("beads:livespec-console-beads-fabro")?,
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

        let result = backfill_source_report(&mut store, "");

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

        let output = run_with_store(
            &command_args(&["bin", "events", "tail"]),
            &mut store,
            "unused",
        );

        assert_eq!(output.code(), 0);
        assert!(output.message().contains("events tail"));
        assert!(output.message().contains("evt_demo_1"));
        assert!(output.message().contains("fabro.human_gate_observed"));
        assert!(output.message().contains("evt_demo_2"));
        Ok(())
    }

    #[test]
    fn store_backed_serve_bootstraps_empty_store() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;

        let output = run_with_store(
            &command_args(&["bin", "serve"]),
            &mut store,
            "2026-06-23T00:00:00Z",
        );

        assert_eq!(output.code(), 0);
        assert_eq!(
            output.message(),
            "serve: store ready\nbackfill events: 6\nevents: 6\nattention: 3\ncommands: 0\npending: 0\nfactory commands handled: 0"
        );
        assert_eq!(store.list_console_events()?.len(), 6);
        assert_eq!(
            store.load_checkpoint("github:livespec-console-beads-fabro")?,
            Some("5".to_owned())
        );
        Ok(())
    }

    #[test]
    fn store_backed_serve_handles_pending_factory_commands_honestly_not_wired()
    -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let persistence = persist_tui_runtime_effects(
            &mut store,
            &[factory_drain_effect()],
            "2026-06-23T00:00:01Z",
        );
        assert!(persistence.is_ok());

        let output = run_with_store(
            &command_args(&["bin", "serve"]),
            &mut store,
            "2026-06-23T00:00:02Z",
        );

        // The live serve path has no real Dispatcher port wired, so the pending
        // drain is handled honestly: accepted + not_wired (two events, no
        // fabricated start/completion), and the command lands in `not_wired`.
        assert_eq!(output.code(), 0);
        assert_eq!(
            output.message(),
            "serve: store ready\nbackfill events: 6\nevents: 8\nattention: 3\ncommands: 1\npending: 0\nfactory commands handled: 1"
        );
        assert_eq!(store.list_commands()?[0].status(), "not_wired");
        Ok(())
    }

    #[test]
    fn store_backed_serve_does_not_backfill_non_empty_store() -> Result<(), ConsoleRuntimeError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        append_demo_events_to_store(&mut store, "2026-06-23T00:00:00Z")?;

        let report = serve_report(&mut store, "2026-06-23T00:00:01Z")?;

        assert_eq!(
            report,
            "serve: store ready\nbackfill events: 0\nevents: 2\nattention: 2\ncommands: 0\npending: 0\nfactory commands handled: 0"
        );
        Ok(())
    }

    #[test]
    fn store_backed_events_tail_reports_empty_store() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;

        let output = run_with_store(
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

        let output = run_with_store(&command_args(&["bin", "events"]), &mut store, "unused");

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

        let output = run_with_store(&command_args(&["bin", "help"]), &mut store, "unused");

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

        let output = run_with_store(&command_args(&["bin", "snapshot"]), &mut store, "unused");

        assert_eq!(output.code(), 0);
        assert_eq!(
            output.message(),
            "snapshot: events 2, attention 2, commands 1, pending 1"
        );
        Ok(())
    }

    #[test]
    fn store_backed_doctor_reports_no_findings_with_store_counts() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        append_demo_events_to_store(&mut store, "2026-06-23T00:00:00Z")?;

        let output = run_with_store(&command_args(&["bin", "doctor"]), &mut store, "unused");

        assert_eq!(output.code(), 0);
        assert_eq!(
            output.message(),
            "doctor: no findings\nstore events: 2\ncommands: 0\nattention: 2"
        );
        Ok(())
    }

    #[test]
    fn store_report_helpers_match_command_output() -> Result<(), ConsoleRuntimeError> {
        let mut store = SqliteEventStore::open_in_memory()?;

        assert_eq!(
            backfill_source_report(&mut store, "2026-06-23T00:00:00Z")?,
            "backfill source adapters: adapters 5, events 6"
        );
        assert!(events_tail_report(&store, 1)?.contains("pr.snapshot_observed"));
        assert_eq!(
            snapshot_report(&store)?,
            "snapshot: events 6, attention 3, commands 0, pending 0"
        );
        assert_eq!(
            doctor_report(&store)?,
            "doctor: no findings\nstore events: 6\ncommands: 0\nattention: 3"
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
                "cmd_evt_gate_acknowledge_requested".to_owned(),
                CommandType::AttentionAcknowledgeRequested,
                "evt_gate".to_owned(),
                "evt_gate:attention.acknowledge_requested".to_owned(),
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
            "cmd_evt_gate_acknowledge_requested"
        );
        assert_eq!(commands.len(), 1);
        assert_eq!(
            commands[0].command_id(),
            "cmd_evt_gate_acknowledge_requested"
        );
        assert_eq!(
            commands[0].command_type(),
            "attention.acknowledge_requested"
        );
        assert_eq!(commands[0].aggregate_id(), Some("evt_gate"));
        assert_eq!(
            commands[0].idempotency_key(),
            "evt_gate:attention.acknowledge_requested"
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
            "cmd_evt_gate_acknowledge_requested".to_owned(),
            CommandType::AttentionAcknowledgeRequested,
            "evt_gate".to_owned(),
            "evt_gate:attention.acknowledge_requested".to_owned(),
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

        let outcome = run_store_backed_tui_session(
            &mut store,
            "2026-06-23T00:00:02Z",
            "operator",
            &mut runner,
            &mut factory_port,
        );
        let commands = store.list_commands()?;

        assert!(matches!(
            outcome,
            Ok(ref value) if value == &TuiSessionOutcome::new(6, 6, 1, 1, 9, 3)
        ));
        assert!(matches!(
            outcome
                .as_ref()
                .map(TuiSessionOutcome::backfilled_event_count),
            Ok(6)
        ));
        assert!(matches!(
            outcome
                .as_ref()
                .map(TuiSessionOutcome::presented_event_count),
            Ok(6)
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
            Ok(9)
        ));
        assert!(matches!(
            outcome.as_ref().map(TuiSessionOutcome::attention_count),
            Ok(3)
        ));
        assert_eq!(runner.observed_event_count(), 6);
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

        let outcome = run_store_backed_tui_session(
            &mut store,
            "2026-06-23T00:00:02Z",
            "operator",
            &mut runner,
            &mut factory_port,
        );

        assert!(matches!(
            outcome,
            Ok(ref value) if value == &TuiSessionOutcome::new(0, 2, 0, 0, 2, 2)
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

        let outcome = run_store_backed_tui_session(
            &mut store,
            "2026-06-23T00:00:02Z",
            "operator",
            &mut runner,
            &mut factory_port,
        );

        assert!(matches!(
            outcome,
            Err(ConsoleRuntimeError::TuiRuntimeFailed)
        ));
        assert_eq!(store.list_console_events()?.len(), 6);
        Ok(())
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
        let mut port = SimulatedFactoryDrainPort;

        let outcomes =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port)?;
        let commands = store.list_commands()?;
        let events = store.list_console_events()?;

        assert_eq!(outcomes.len(), 1);
        assert_eq!(
            outcomes[0],
            super::FactoryCommandHandlingOutcome::new(
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
            vec![FactoryCommandHandlingOutcome::new(
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
            "cmd_evt_gate_acknowledge_requested".to_owned(),
            CommandType::AttentionAcknowledgeRequested,
            "evt_gate".to_owned(),
            "evt_gate:attention.acknowledge_requested".to_owned(),
            "operator".to_owned(),
        ))];
        persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z")?;
        let update = store.update_command_status(
            "cmd_evt_gate_acknowledge_requested",
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
    fn pending_factory_command_handler_skips_pending_non_factory_command()
    -> Result<(), ConsoleRuntimeError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let effects = [TuiRuntimeEffect::PersistCommand(CommandEnvelope::new(
            "cmd_evt_gate_acknowledge_requested".to_owned(),
            CommandType::AttentionAcknowledgeRequested,
            "evt_gate".to_owned(),
            "evt_gate:attention.acknowledge_requested".to_owned(),
            "operator".to_owned(),
        ))];
        persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z")?;
        let mut port = SimulatedFactoryDrainPort;

        let outcomes =
            handle_pending_factory_commands(&mut store, "2026-06-23T00:00:03Z", &mut port)?;

        assert_eq!(outcomes, []);
        assert_eq!(store.list_console_events()?, []);
        Ok(())
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
    fn not_wired_factory_drain_port_reports_not_wired_without_fabricating_success() {
        let mut port = NotWiredFactoryDrainPort;
        let request = FactoryDrainRequest::new("fleet:livespec".to_owned(), 1, 1);

        let outcome = port.drain_ready_queue(&request);

        assert_eq!(outcome, Ok(FactoryDrainPortOutcome::not_wired()));
    }

    #[test]
    fn demo_backfill_round_trips_through_event_store() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;

        let outcomes = append_demo_events_to_store(&mut store, "2026-06-23T00:00:00Z")?;
        let events = load_tui_events_from_store(&store)?;

        assert_eq!(outcomes.len(), 2);
        assert_eq!(outcomes[0].status(), AppendStatus::Inserted);
        assert_eq!(outcomes[1].status(), AppendStatus::Inserted);
        assert_eq!(events, demo_events());
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
        assert_eq!(events, demo_events());
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
    /// path uses `NotWiredFactoryDrainPort`); this double lets the command and
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
        StatusUpdateFails,
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

        fn append_event(&mut self, _append: &EventAppend) -> EventStoreResult<AppendOutcome> {
            if self.mode == ScriptedStoreMode::AppendFails {
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
