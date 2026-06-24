#![forbid(unsafe_code)]

use console_application::{
    ApplicationError, FactoryDrainPort, FactoryDrainPortOutcome, FactoryDrainRequest,
    build_tui_model, handle_factory_drain_command, project_attention,
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
        Some("backfill") => run_store_result(backfill_demo_report(store, observed_at), "backfill"),
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

fn count_commands_with_status(commands: &[StoredCommand], status: &str) -> usize {
    commands
        .iter()
        .filter(|command| command.status() == status)
        .count()
}

#[derive(Debug)]
pub enum ConsoleRuntimeError {
    Application(ApplicationError),
    EventStore(EventStoreError),
    MissingCommandAggregate(String),
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

pub struct SimulatedFactoryDrainPort;

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
        build_tui_model,
    };
    use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};
    use console_eventstore::{
        AppendOutcome, AppendStatus, CommandAppend, CommandAppendOutcome, CommandAppendStatus,
        CommandStatusUpdateOutcome, EventAppend, EventStoreError, EventStoreResult,
        SqliteEventStore, StoredCommand,
    };
    use console_tui::TuiRuntimeEffect;

    use super::{
        CommandAppendStore, ConsoleRuntimeError, EventAppendStore, FactoryCommandHandlingOutcome,
        FactoryCommandStore, SimulatedFactoryDrainPort, append_demo_events_to_store,
        backfill_demo_report, command_status_update_runtime_result, demo_events, doctor_report,
        events_tail_report, factory_command_from_stored, handle_pending_factory_commands,
        load_tui_events_from_store, persist_tui_runtime_effects, render_tui_preview, run,
        run_with_store, snapshot_report,
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
    fn store_backed_backfill_command_reports_insert_and_duplicate_counts()
    -> Result<(), EventStoreError> {
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
            "backfill demo events: inserted 2, duplicate 0"
        );
        assert_eq!(second.code(), 0);
        assert_eq!(
            second.message(),
            "backfill demo events: inserted 0, duplicate 2"
        );
        assert_eq!(store.list_console_events()?.len(), 2);
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
    fn store_report_helpers_match_command_output() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;

        assert_eq!(
            backfill_demo_report(&mut store, "2026-06-23T00:00:00Z")?,
            "backfill demo events: inserted 2, duplicate 0"
        );
        assert!(events_tail_report(&store, 1)?.contains("evt_demo_2"));
        assert_eq!(
            snapshot_report(&store)?,
            "snapshot: events 2, attention 2, commands 0, pending 0"
        );
        assert_eq!(
            doctor_report(&store)?,
            "doctor: no findings\nstore events: 2\ncommands: 0\nattention: 2"
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
