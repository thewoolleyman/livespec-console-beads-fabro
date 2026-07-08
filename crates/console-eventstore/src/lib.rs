//! SQLite-backed append-only event store for the console, persisting `ConsoleEvent`s and command envelopes and reading them back for projections.

#![forbid(unsafe_code)]

use std::num::TryFromIntError;
use std::path::Path;

use console_domain::{CommandEnvelope, ConsoleEvent, EventType};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

const SCHEMA: &str = r"
create table if not exists events (
  global_seq integer primary key,
  event_id text not null unique,
  context text not null,
  aggregate_id text not null,
  stream_id text not null,
  stream_seq integer not null,
  type text not null,
  schema_version integer not null,
  occurred_at text not null,
  observed_at text not null,
  causation_id text null,
  correlation_id text not null,
  source text not null,
  source_event_id text null,
  payload_json text not null,
  metadata_json text not null
);

create unique index if not exists events_source_event_unique
on events(source, source_event_id)
where source_event_id is not null;

create table if not exists commands (
  command_id text primary key,
  context text not null,
  type text not null,
  aggregate_id text null,
  idempotency_key text not null unique,
  requested_by text not null,
  requested_at text not null,
  causation_event_id text null,
  correlation_id text not null,
  status text not null,
  payload_json text not null,
  result_json text null,
  error_json text null,
  updated_at text not null
);

create table if not exists checkpoints (
  adapter_id text primary key,
  checkpoint_json text not null,
  advanced_at text not null
);
";

#[derive(Debug)]
pub enum EventStoreError {
    CommandNotFound(String),
    InvalidSequence,
    Sqlite(rusqlite::Error),
    UnknownEventType(String),
}

impl From<rusqlite::Error> for EventStoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

impl From<TryFromIntError> for EventStoreError {
    fn from(_error: TryFromIntError) -> Self {
        Self::InvalidSequence
    }
}

pub type EventStoreResult<T> = Result<T, EventStoreError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandAppend {
    command: CommandEnvelope,
    requested_at: String,
    causation_event_id: Option<String>,
    correlation_id: String,
    payload_json: String,
}

impl CommandAppend {
    #[must_use]
    pub const fn new(
        command: CommandEnvelope,
        requested_at: String,
        causation_event_id: Option<String>,
        correlation_id: String,
        payload_json: String,
    ) -> Self {
        Self {
            command,
            requested_at,
            causation_event_id,
            correlation_id,
            payload_json,
        }
    }

    #[must_use]
    pub const fn command(&self) -> &CommandEnvelope {
        &self.command
    }

    #[must_use]
    pub fn causation_event_id(&self) -> Option<&str> {
        self.causation_event_id.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventAppend {
    event: ConsoleEvent,
    aggregate_id: String,
    occurred_at: String,
    observed_at: String,
    causation_id: Option<String>,
    correlation_id: String,
    source_event_id: Option<String>,
    payload_json: String,
    metadata_json: String,
}

impl EventAppend {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        event: ConsoleEvent,
        aggregate_id: String,
        occurred_at: String,
        observed_at: String,
        causation_id: Option<String>,
        correlation_id: String,
        source_event_id: Option<String>,
        payload_json: String,
        metadata_json: String,
    ) -> Self {
        Self {
            event,
            aggregate_id,
            occurred_at,
            observed_at,
            causation_id,
            correlation_id,
            source_event_id,
            payload_json,
            metadata_json,
        }
    }

    #[must_use]
    pub const fn event(&self) -> &ConsoleEvent {
        &self.event
    }

    #[must_use]
    pub fn source_event_id(&self) -> Option<&str> {
        self.source_event_id.as_deref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppendStatus {
    Inserted,
    Duplicate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppendOutcome {
    global_seq: u64,
    status: AppendStatus,
}

impl AppendOutcome {
    #[must_use]
    pub const fn new(global_seq: u64, status: AppendStatus) -> Self {
        Self { global_seq, status }
    }

    #[must_use]
    pub const fn global_seq(&self) -> u64 {
        self.global_seq
    }

    #[must_use]
    pub const fn status(&self) -> AppendStatus {
        self.status
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredEvent {
    global_seq: u64,
    event_id: String,
    event_type: String,
    source: String,
    source_event_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCommand {
    command_id: String,
    context: String,
    command_type: String,
    aggregate_id: Option<String>,
    idempotency_key: String,
    requested_by: String,
    status: String,
}

impl StoredCommand {
    #[must_use]
    pub const fn new(
        command_id: String,
        context: String,
        command_type: String,
        aggregate_id: Option<String>,
        idempotency_key: String,
        requested_by: String,
        status: String,
    ) -> Self {
        Self {
            command_id,
            context,
            command_type,
            aggregate_id,
            idempotency_key,
            requested_by,
            status,
        }
    }

    #[must_use]
    pub fn command_id(&self) -> &str {
        &self.command_id
    }

    #[must_use]
    pub fn context(&self) -> &str {
        &self.context
    }

    #[must_use]
    pub fn command_type(&self) -> &str {
        &self.command_type
    }

    #[must_use]
    pub fn aggregate_id(&self) -> Option<&str> {
        self.aggregate_id.as_deref()
    }

    #[must_use]
    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }

    #[must_use]
    pub fn requested_by(&self) -> &str {
        &self.requested_by
    }

    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandAppendStatus {
    Inserted,
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandAppendOutcome {
    command_id: String,
    status: CommandAppendStatus,
}

impl CommandAppendOutcome {
    #[must_use]
    pub const fn new(command_id: String, status: CommandAppendStatus) -> Self {
        Self { command_id, status }
    }

    #[must_use]
    pub fn command_id(&self) -> &str {
        &self.command_id
    }

    #[must_use]
    pub const fn status(&self) -> CommandAppendStatus {
        self.status
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandStatusUpdateOutcome {
    command_id: String,
    status: String,
}

impl CommandStatusUpdateOutcome {
    #[must_use]
    pub const fn new(command_id: String, status: String) -> Self {
        Self { command_id, status }
    }

    #[must_use]
    pub fn command_id(&self) -> &str {
        &self.command_id
    }

    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }
}

impl StoredEvent {
    #[must_use]
    pub const fn new(
        global_seq: u64,
        event_id: String,
        event_type: String,
        source: String,
        source_event_id: Option<String>,
    ) -> Self {
        Self {
            global_seq,
            event_id,
            event_type,
            source,
            source_event_id,
        }
    }

    #[must_use]
    pub const fn global_seq(&self) -> u64 {
        self.global_seq
    }

    #[must_use]
    pub fn event_id(&self) -> &str {
        &self.event_id
    }

    #[must_use]
    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub fn source_event_id(&self) -> Option<&str> {
        self.source_event_id.as_deref()
    }
}

pub struct SqliteEventStore {
    connection: Connection,
}

impl SqliteEventStore {
    pub fn open(path: &Path) -> EventStoreResult<Self> {
        let connection = Connection::open(path)?;
        initialize_connection(&connection)?;
        Ok(Self { connection })
    }

    pub fn open_in_memory() -> EventStoreResult<Self> {
        let connection = Connection::open_in_memory()?;
        initialize_connection(&connection)?;
        Ok(Self { connection })
    }

    pub fn append_event(&mut self, append: &EventAppend) -> EventStoreResult<AppendOutcome> {
        let transaction = self.connection.transaction()?;
        let inserted = transaction.execute(
            r"
            insert or ignore into events (
              event_id,
              context,
              aggregate_id,
              stream_id,
              stream_seq,
              type,
              schema_version,
              occurred_at,
              observed_at,
              causation_id,
              correlation_id,
              source,
              source_event_id,
              payload_json,
              metadata_json
            ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            ",
            params![
                append.event.event_id(),
                append.event.context(),
                append.aggregate_id,
                append.event.stream_id(),
                append.event.stream_seq(),
                append.event.event_type().contract_name(),
                append.event.schema_version(),
                append.occurred_at,
                append.observed_at,
                append.causation_id,
                append.correlation_id,
                append.event.source(),
                append.source_event_id,
                append.payload_json,
                append.metadata_json,
            ],
        )?;
        let outcome = if inserted == 0 {
            AppendOutcome::new(
                find_existing_sequence(&transaction, append)?,
                AppendStatus::Duplicate,
            )
        } else {
            AppendOutcome::new(
                sequence_from_rowid(transaction.last_insert_rowid())?,
                AppendStatus::Inserted,
            )
        };
        transaction.commit()?;
        Ok(outcome)
    }

    pub fn append_command(
        &mut self,
        append: &CommandAppend,
    ) -> EventStoreResult<CommandAppendOutcome> {
        let transaction = self.connection.transaction()?;
        let inserted = transaction.execute(
            r"
            insert or ignore into commands (
              command_id,
              context,
              type,
              aggregate_id,
              idempotency_key,
              requested_by,
              requested_at,
              causation_event_id,
              correlation_id,
              status,
              payload_json,
              updated_at
            ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'pending', ?10, ?7)
            ",
            params![
                append.command.command_id(),
                append.command.command_type().context(),
                append.command.command_type().contract_name(),
                append.command.aggregate_id(),
                append.command.idempotency_key(),
                append.command.requested_by(),
                append.requested_at,
                append.causation_event_id,
                append.correlation_id,
                append.payload_json,
            ],
        )?;
        let outcome = if inserted == 0 {
            CommandAppendOutcome::new(
                find_existing_command_id(&transaction, append)?,
                CommandAppendStatus::Duplicate,
            )
        } else {
            CommandAppendOutcome::new(
                append.command.command_id().to_owned(),
                CommandAppendStatus::Inserted,
            )
        };
        transaction.commit()?;
        Ok(outcome)
    }

    pub fn list_events(&self) -> EventStoreResult<Vec<StoredEvent>> {
        let sql = "select global_seq, event_id, type, source, source_event_id from events order by global_seq";
        let mut statement = self.connection.prepare(sql)?;
        let mut rows = statement.query([])?;
        let mut events = Vec::new();
        while let Some(row) = rows.next()? {
            events.push(StoredEvent::new(
                sequence_from_rowid(row.get::<_, i64>(0)?)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ));
        }
        Ok(events)
    }

    pub fn list_console_events(&self) -> EventStoreResult<Vec<ConsoleEvent>> {
        let sql = r"
            select event_id, schema_version, context, type, source, stream_id, stream_seq,
                   payload_json
            from events
            order by global_seq
        ";
        let mut statement = self.connection.prepare(sql)?;
        let mut rows = statement.query([])?;
        let mut events = Vec::new();
        while let Some(row) = rows.next()? {
            let event_type_name = row.get::<_, String>(3)?;
            let Some(event_type) = EventType::from_contract_name(&event_type_name) else {
                return Err(EventStoreError::UnknownEventType(event_type_name));
            };
            events.push(
                ConsoleEvent::new(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    event_type,
                    row.get(4)?,
                    row.get(5)?,
                    sequence_from_rowid(row.get::<_, i64>(6)?)?,
                )
                .with_payload_json(row.get::<_, String>(7)?),
            );
        }
        Ok(events)
    }

    pub fn list_commands(&self) -> EventStoreResult<Vec<StoredCommand>> {
        let sql = r"
            select command_id, context, type, aggregate_id, idempotency_key, requested_by, status
            from commands
            order by requested_at, command_id
        ";
        let mut statement = self.connection.prepare(sql)?;
        let mut rows = statement.query([])?;
        let mut commands = Vec::new();
        while let Some(row) = rows.next()? {
            commands.push(StoredCommand::new(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ));
        }
        Ok(commands)
    }

    pub fn update_command_status(
        &mut self,
        command_id: &str,
        status: &str,
        updated_at: &str,
        result_json: Option<&str>,
        error_json: Option<&str>,
    ) -> EventStoreResult<CommandStatusUpdateOutcome> {
        let updated = self.connection.execute(
            r"
            update commands
            set status = ?2,
                result_json = ?3,
                error_json = ?4,
                updated_at = ?5
            where command_id = ?1
            ",
            params![command_id, status, result_json, error_json, updated_at],
        )?;
        if updated == 0 {
            return Err(EventStoreError::CommandNotFound(command_id.to_owned()));
        }
        Ok(CommandStatusUpdateOutcome::new(
            command_id.to_owned(),
            status.to_owned(),
        ))
    }

    pub fn load_checkpoint(&self, adapter_id: &str) -> EventStoreResult<Option<String>> {
        let checkpoint = self
            .connection
            .query_row(
                "select checkpoint_json from checkpoints where adapter_id = ?1",
                params![adapter_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(checkpoint)
    }

    pub fn save_checkpoint(
        &mut self,
        adapter_id: &str,
        checkpoint_json: &str,
        advanced_at: &str,
    ) -> EventStoreResult<()> {
        self.connection.execute(
            r"
            insert into checkpoints (adapter_id, checkpoint_json, advanced_at)
            values (?1, ?2, ?3)
            on conflict(adapter_id) do update set
              checkpoint_json = excluded.checkpoint_json,
              advanced_at = excluded.advanced_at
            ",
            params![adapter_id, checkpoint_json, advanced_at],
        )?;
        Ok(())
    }
}

fn initialize_connection(connection: &Connection) -> EventStoreResult<()> {
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.execute_batch(SCHEMA)?;
    Ok(())
}

fn find_existing_sequence(
    transaction: &Transaction<'_>,
    append: &EventAppend,
) -> EventStoreResult<u64> {
    let sequence = match append.source_event_id.as_deref() {
        Some(source_event_id) => transaction
            .query_row(
                r"
                select global_seq
                from events
                where source = ?1 and source_event_id = ?2
                ",
                params![append.event.source(), source_event_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?,
        None => transaction
            .query_row(
                "select global_seq from events where event_id = ?1",
                params![append.event.event_id()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?,
    };
    sequence_from_rowid(sequence.ok_or(rusqlite::Error::QueryReturnedNoRows)?)
}

fn find_existing_command_id(
    transaction: &Transaction<'_>,
    append: &CommandAppend,
) -> EventStoreResult<String> {
    let command_id = transaction
        .query_row(
            r"
            select command_id
            from commands
            where idempotency_key = ?1
            ",
            params![append.command.idempotency_key()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    match command_id {
        Some(command_id) => Ok(command_id),
        None => Ok(transaction.query_row(
            r"
            select command_id
            from commands
            where command_id = ?1
            ",
            params![append.command.command_id()],
            |row| row.get::<_, String>(0),
        )?),
    }
}

fn sequence_from_rowid(value: i64) -> EventStoreResult<u64> {
    Ok(u64::try_from(value)?)
}

#[cfg(test)]
mod tests {
    use super::{
        AppendStatus, CommandAppend, CommandAppendStatus, CommandStatusUpdateOutcome, EventAppend,
        EventStoreError, EventStoreResult, SqliteEventStore, StoredCommand, sequence_from_rowid,
    };
    use console_application::{
        build_tui_model,
        source_adapters::{AcceptancePolicy, AdmissionPolicy, Lane, LaneReason},
    };
    use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};

    #[test]
    fn opened_store_uses_wal_mode_and_creates_required_tables() -> Result<(), EventStoreError> {
        let path = std::env::temp_dir().join(format!(
            "livespec-console-eventstore-{}.sqlite",
            std::process::id()
        ));
        let _remove_result = std::fs::remove_file(&path);
        let store = SqliteEventStore::open(&path)?;

        let journal_mode: String =
            store
                .connection
                .query_row("pragma journal_mode", [], |row| row.get(0))?;
        for table_name in ["events", "commands", "checkpoints"] {
            let sql = format!("select count(*) from {table_name}");
            assert!(store.connection.prepare(&sql).is_ok());
        }
        assert!(!table_select_prepares(&store, "projections"));

        assert_eq!(journal_mode, "wal");
        let _remove_result = std::fs::remove_file(&path);
        Ok(())
    }

    #[test]
    fn append_event_persists_canonical_event_row() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let append = event_append("evt_1", Some("source-1"));

        let outcome = store.append_event(&append)?;
        let events = store.list_events()?;

        assert_eq!(outcome.status(), AppendStatus::Inserted);
        assert_eq!(outcome.global_seq(), 1);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].global_seq(), 1);
        assert_eq!(events[0].event_id(), "evt_1");
        assert_eq!(events[0].event_type(), "fabro.human_gate_observed");
        assert_eq!(events[0].source(), "fabro");
        assert_eq!(events[0].source_event_id(), Some("source-1"));
        assert_eq!(append.event().event_id(), "evt_1");
        assert_eq!(append.source_event_id(), Some("source-1"));
        Ok(())
    }

    #[test]
    fn list_console_events_rebuilds_domain_events() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let first = event_append("evt_1", Some("source-1"));
        let second = EventAppend::new(
            ConsoleEvent::new(
                "evt_2".to_owned(),
                1,
                "dispatch".to_owned(),
                EventType::DispatcherBacklogBounceObserved,
                "dispatcher".to_owned(),
                "repo:livespec-console-beads-fabro".to_owned(),
                2,
            ),
            "repo:livespec-console-beads-fabro".to_owned(),
            "2026-06-23T00:00:00Z".to_owned(),
            "2026-06-23T00:00:01Z".to_owned(),
            None,
            "corr_1".to_owned(),
            Some("source-2".to_owned()),
            "{}".to_owned(),
            "{}".to_owned(),
        );

        store.append_event(&first)?;
        store.append_event(&second)?;
        let events = store.list_console_events()?;

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_id(), "evt_1");
        assert_eq!(events[0].event_type(), &EventType::FabroHumanGateObserved);
        assert_eq!(events[0].source(), "fabro");
        assert_eq!(events[0].stream_seq(), 1);
        assert_eq!(events[1].event_id(), "evt_2");
        assert_eq!(
            events[1].event_type(),
            &EventType::DispatcherBacklogBounceObserved
        );
        assert_eq!(events[1].context(), "dispatch");
        assert_eq!(events[1].stream_seq(), 2);
        assert_eq!(events[1].payload_json(), "{}");
        Ok(())
    }

    #[test]
    fn list_console_events_attaches_persisted_payload_json() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let payload = r#"{"repo":"console","work_item_id":"console-1","lane":"ready"}"#;
        let append = EventAppend::new(
            ConsoleEvent::fixture(
                "evt_snap",
                EventType::WorkItemSnapshotObserved,
                "orchestrator",
            ),
            "repo:console".to_owned(),
            "2026-06-29T00:00:00Z".to_owned(),
            "2026-06-29T00:00:01Z".to_owned(),
            None,
            "corr_snap".to_owned(),
            Some("source-snap".to_owned()),
            payload.to_owned(),
            "{}".to_owned(),
        );

        store.append_event(&append)?;
        let events = store.list_console_events()?;

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload_json(), payload);
        Ok(())
    }

    #[test]
    fn work_item_projections_rebuild_identically_after_store_wipe_and_ledger_replay()
    -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let pending = work_item_append(
            "evt_console_1_pending",
            "console-1",
            Lane::PendingApproval,
            None,
            "a1",
            "pending-approval",
            1,
        );
        let ready = work_item_append(
            "evt_console_2_ready",
            "console-2",
            Lane::Ready,
            None,
            "a0",
            "ready",
            2,
        );
        let blocked = work_item_append(
            "evt_console_1_blocked",
            "console-1",
            Lane::Blocked,
            Some(LaneReason::NeedsHuman),
            "a2",
            "blocked",
            3,
        );
        store.append_event(&pending)?;
        store.append_event(&ready)?;
        store.append_event(&blocked)?;

        let command = command_append("cmd_1", "idem_1", CommandType::FactoryDrainRequested);
        store.append_command(&command)?;
        let status_update = store.update_command_status(
            "cmd_1",
            "completed",
            "2026-06-23T00:00:03Z",
            Some(r#"{"event_count":3}"#),
            None,
        );
        assert!(matches!(
            status_update,
            Ok(CommandStatusUpdateOutcome { .. })
        ));

        let original_events = store.list_console_events()?;
        let original_model = build_tui_model(&original_events, 0);

        let mut rebuilt = SqliteEventStore::open_in_memory()?;
        for event in original_events
            .iter()
            .filter(|event| event.event_type() == &EventType::WorkItemSnapshotObserved)
        {
            rebuilt.append_event(&replayed_work_item_append(event))?;
        }
        let rebuilt_events = rebuilt.list_console_events()?;
        let rebuilt_model = build_tui_model(&rebuilt_events, 0);

        assert_eq!(rebuilt.list_commands()?, []);
        assert_eq!(rebuilt_events.len(), 3);
        assert_eq!(rebuilt_model.lane_board(), original_model.lane_board());
        assert_eq!(
            rebuilt_model.attention_items(),
            original_model.attention_items()
        );
        assert_eq!(rebuilt_model.detail(), original_model.detail());
        Ok(())
    }

    #[test]
    fn schema_has_no_primary_work_item_lifecycle_state_outside_command_carve_out()
    -> Result<(), EventStoreError> {
        let store = SqliteEventStore::open_in_memory()?;

        assert!(!table_select_prepares(&store, "projections"));
        for table_name in ["events", "checkpoints"] {
            for column_name in table_columns(&store, table_name)? {
                assert!(!matches!(
                    column_name.as_str(),
                    "lane" | "lane_reason" | "work_item_status" | "status"
                ));
            }
        }
        // `commands.status` is console-local operator-command state, not
        // work-item lifecycle state. It is intentionally excluded from
        // rebuild determinism and must not be event-sourced as a work-item
        // projection.
        assert!(table_columns(&store, "commands")?.contains(&"status".to_owned()));
        Ok(())
    }

    #[test]
    fn list_console_events_rejects_unknown_event_type() -> Result<(), EventStoreError> {
        let store = SqliteEventStore::open_in_memory()?;

        let inserted = store.connection.execute(
            r"
            insert into events (
              event_id,
              context,
              aggregate_id,
              stream_id,
              stream_seq,
              type,
              schema_version,
              occurred_at,
              observed_at,
              correlation_id,
              source,
              payload_json,
              metadata_json
            ) values ('evt_bad', 'factory', 'repo:livespec', 'repo:livespec', 1,
              'unknown.event', 1, '2026-06-23T00:00:00Z',
              '2026-06-23T00:00:01Z', 'corr_1', 'test', '{}', '{}')
            ",
            [],
        );
        assert!(matches!(inserted, Ok(1)));

        let result = store.list_console_events();

        assert!(matches!(
            result,
            Err(EventStoreError::UnknownEventType(event_type)) if event_type == "unknown.event"
        ));
        Ok(())
    }

    #[test]
    fn duplicate_source_event_id_returns_existing_sequence() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let first = event_append("evt_1", Some("source-1"));
        let duplicate = event_append("evt_2", Some("source-1"));

        let first_outcome = store.append_event(&first)?;
        let duplicate_outcome = store.append_event(&duplicate)?;
        let events = store.list_events()?;

        assert_eq!(first_outcome.status(), AppendStatus::Inserted);
        assert_eq!(duplicate_outcome.status(), AppendStatus::Duplicate);
        assert_eq!(duplicate_outcome.global_seq(), first_outcome.global_seq());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_id(), "evt_1");
        Ok(())
    }

    #[test]
    fn duplicate_event_id_without_source_event_id_returns_existing_sequence()
    -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let first = event_append("evt_1", None);
        let duplicate = event_append("evt_1", None);

        let first_outcome = store.append_event(&first)?;
        let duplicate_outcome = store.append_event(&duplicate)?;

        assert_eq!(duplicate_outcome.status(), AppendStatus::Duplicate);
        assert_eq!(duplicate_outcome.global_seq(), first_outcome.global_seq());
        Ok(())
    }

    #[test]
    fn append_command_persists_pending_command_row() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let append = command_append("cmd_1", "idem_1", CommandType::FactoryDrainRequested);

        let outcome = store.append_command(&append)?;
        let commands = store.list_commands()?;

        assert_eq!(outcome.status(), CommandAppendStatus::Inserted);
        assert_eq!(outcome.command_id(), "cmd_1");
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command_id(), "cmd_1");
        assert_eq!(commands[0].context(), "factory");
        assert_eq!(commands[0].command_type(), "factory.drain_requested");
        assert_eq!(commands[0].aggregate_id(), Some("evt_gate"));
        assert_eq!(commands[0].idempotency_key(), "idem_1");
        assert_eq!(commands[0].requested_by(), "operator");
        assert_eq!(commands[0].status(), "pending");
        assert_eq!(append.command().command_id(), "cmd_1");
        assert_eq!(append.causation_event_id(), Some("evt_gate"));
        Ok(())
    }

    #[test]
    fn duplicate_command_id_returns_existing_command_id() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let first = command_append("cmd_1", "idem_1", CommandType::FactoryDrainRequested);
        let duplicate = command_append("cmd_1", "idem_2", CommandType::FactoryDrainRequested);

        let first_outcome = store.append_command(&first)?;
        let duplicate_outcome = store.append_command(&duplicate)?;
        let commands = store.list_commands()?;

        assert_eq!(first_outcome.status(), CommandAppendStatus::Inserted);
        assert_eq!(duplicate_outcome.status(), CommandAppendStatus::Duplicate);
        assert_eq!(duplicate_outcome.command_id(), "cmd_1");
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command_type(), "factory.drain_requested");
        Ok(())
    }

    #[test]
    fn duplicate_idempotency_key_returns_existing_command_id() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let first = command_append("cmd_1", "idem_1", CommandType::FactoryDrainRequested);
        let duplicate = command_append("cmd_2", "idem_1", CommandType::FactoryDrainRequested);

        let first_outcome = store.append_command(&first)?;
        let duplicate_outcome = store.append_command(&duplicate)?;
        let commands = store.list_commands()?;

        assert_eq!(first_outcome.status(), CommandAppendStatus::Inserted);
        assert_eq!(duplicate_outcome.status(), CommandAppendStatus::Duplicate);
        assert_eq!(duplicate_outcome.command_id(), "cmd_1");
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command_id(), "cmd_1");
        Ok(())
    }

    #[test]
    fn command_status_update_marks_existing_command() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let append = command_append("cmd_1", "idem_1", CommandType::FactoryDrainRequested);
        store.append_command(&append)?;

        let outcome = store.update_command_status(
            "cmd_1",
            "completed",
            "2026-06-23T00:00:03Z",
            Some(r#"{"event_count":3}"#),
            None,
        );
        let commands = store.list_commands()?;

        assert!(matches!(
            outcome.as_ref().map(CommandStatusUpdateOutcome::command_id),
            Ok("cmd_1")
        ));
        assert!(matches!(
            outcome.as_ref().map(CommandStatusUpdateOutcome::status),
            Ok("completed")
        ));
        assert_eq!(commands[0].status(), "completed");
        Ok(())
    }

    #[test]
    fn command_status_update_rejects_unknown_command() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;

        let outcome = store.update_command_status(
            "cmd_missing",
            "completed",
            "2026-06-23T00:00:03Z",
            None,
            None,
        );

        assert!(matches!(
            outcome,
            Err(EventStoreError::CommandNotFound(command_id)) if command_id == "cmd_missing"
        ));
        Ok(())
    }

    #[test]
    fn command_status_update_reports_sqlite_failure() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        store.connection.execute_batch("drop table commands")?;

        let outcome =
            store.update_command_status("cmd_1", "completed", "2026-06-23T00:00:03Z", None, None);

        assert!(matches!(outcome, Err(EventStoreError::Sqlite(_error))));
        Ok(())
    }

    #[test]
    fn missing_checkpoint_loads_as_none() -> Result<(), EventStoreError> {
        let store = SqliteEventStore::open_in_memory()?;

        let checkpoint = store.load_checkpoint("orchestrator:repo")?;

        assert_eq!(checkpoint, None);
        Ok(())
    }

    #[test]
    fn checkpoint_save_and_load_round_trips_latest_value() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;

        let key = "orchestrator:repo";
        store.save_checkpoint(key, r#"{"version":1}"#, "2026-06-24T00:00:00Z")?;
        store.save_checkpoint(key, r#"{"version":2}"#, "2026-06-24T00:00:01Z")?;
        let second_adapter_save = store.save_checkpoint(
            "fabro:repo",
            r#"{"cursor":"run_1"}"#,
            "2026-06-24T00:00:02Z",
        );

        assert!(matches!(second_adapter_save, Ok(())));
        assert_eq!(
            store.load_checkpoint("orchestrator:repo")?,
            Some(r#"{"version":2}"#.to_owned())
        );
        assert_eq!(
            store.load_checkpoint("fabro:repo")?,
            Some(r#"{"cursor":"run_1"}"#.to_owned())
        );
        Ok(())
    }

    #[test]
    fn checkpoint_save_reports_sqlite_failure() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        store.connection.execute_batch("drop table checkpoints")?;

        let result = store.save_checkpoint(
            "orchestrator:repo",
            r#"{"version":1}"#,
            "2026-06-24T00:00:00Z",
        );

        assert!(matches!(result, Err(EventStoreError::Sqlite(_error))));
        Ok(())
    }

    #[test]
    fn missing_duplicate_command_lookup_returns_sqlite_error() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let append = command_append(
            "cmd_missing",
            "idem_missing",
            CommandType::FactoryDrainRequested,
        );
        let transaction = store.connection.transaction()?;
        let result = super::find_existing_command_id(&transaction, &append);

        assert!(matches!(result, Err(EventStoreError::Sqlite(_error))));
        Ok(())
    }

    #[test]
    fn stored_command_exposes_nullable_aggregate_id() {
        let command = StoredCommand::new(
            "cmd_1".to_owned(),
            "factory".to_owned(),
            "factory.drain_requested".to_owned(),
            None,
            "idem_1".to_owned(),
            "operator".to_owned(),
            "pending".to_owned(),
        );

        assert_eq!(command.command_id(), "cmd_1");
        assert_eq!(command.context(), "factory");
        assert_eq!(command.command_type(), "factory.drain_requested");
        assert_eq!(command.aggregate_id(), None);
        assert_eq!(command.idempotency_key(), "idem_1");
        assert_eq!(command.requested_by(), "operator");
        assert_eq!(command.status(), "pending");
    }

    #[test]
    fn negative_rowid_is_invalid_sequence() {
        let result = sequence_from_rowid(-1);

        assert!(matches!(result, Err(EventStoreError::InvalidSequence)));
    }

    #[test]
    fn sqlite_errors_convert_to_event_store_errors() {
        let result = EventStoreError::from(rusqlite::Error::InvalidQuery);

        assert!(matches!(result, EventStoreError::Sqlite(_error)));
    }

    fn table_select_prepares(store: &SqliteEventStore, table_name: &str) -> bool {
        store
            .connection
            .prepare(&format!("select count(*) from {table_name}"))
            .is_ok()
    }

    fn table_columns(store: &SqliteEventStore, table_name: &str) -> EventStoreResult<Vec<String>> {
        let mut statement = store
            .connection
            .prepare(&format!("pragma table_info({table_name})"))?;
        let mut rows = statement.query([])?;
        let mut columns = Vec::new();
        while let Some(row) = rows.next()? {
            columns.push(row.get(1)?);
        }
        Ok(columns)
    }

    #[allow(clippy::too_many_arguments)]
    fn work_item_append(
        event_id: &str,
        work_item_id: &str,
        lane: Lane,
        lane_reason: Option<LaneReason>,
        rank: &str,
        status: &str,
        source_version: u64,
    ) -> EventAppend {
        let event = ConsoleEvent::new(
            event_id.to_owned(),
            1,
            "factory".to_owned(),
            EventType::WorkItemSnapshotObserved,
            "orchestrator".to_owned(),
            "repo:console".to_owned(),
            source_version,
        );
        EventAppend::new(
            event,
            "repo:console".to_owned(),
            format!("2026-06-29T00:00:0{source_version}Z"),
            format!("2026-06-29T00:00:1{source_version}Z"),
            None,
            "corr_work_items".to_owned(),
            Some(format!("source-{event_id}")),
            work_item_payload_json(
                work_item_id,
                lane,
                lane_reason,
                rank,
                status,
                source_version,
            ),
            "{}".to_owned(),
        )
    }

    fn work_item_payload_json(
        work_item_id: &str,
        lane: Lane,
        lane_reason: Option<LaneReason>,
        rank: &str,
        status: &str,
        source_version: u64,
    ) -> String {
        let reason_json = lane_reason.map_or_else(
            || "null".to_owned(),
            |reason| format!("\"{}\"", reason.label()),
        );
        format!(
            r#"{{"repo":"console","work_item_id":"{work_item_id}","lane":"{}","lane_reason":{reason_json},"rank":"{rank}","status":"{status}","admission_policy":"{}","acceptance_policy":"{}","source_version":{source_version}}}"#,
            lane.label(),
            AdmissionPolicy::Manual.label(),
            AcceptancePolicy::AiThenHuman.label()
        )
    }

    fn replayed_work_item_append(event: &ConsoleEvent) -> EventAppend {
        EventAppend::new(
            event.clone(),
            event.stream_id().to_owned(),
            "2026-06-29T00:01:00Z".to_owned(),
            "2026-06-29T00:01:01Z".to_owned(),
            None,
            "corr_rebuild".to_owned(),
            Some(format!("replay:{}", event.event_id())),
            event.payload_json().to_owned(),
            "{}".to_owned(),
        )
    }

    fn event_append(event_id: &str, source_event_id: Option<&str>) -> EventAppend {
        EventAppend::new(
            ConsoleEvent::fixture(event_id, EventType::FabroHumanGateObserved, "fabro"),
            "repo:livespec".to_owned(),
            "2026-06-23T00:00:00Z".to_owned(),
            "2026-06-23T00:00:01Z".to_owned(),
            None,
            "corr_1".to_owned(),
            source_event_id.map(str::to_owned),
            "{}".to_owned(),
            "{}".to_owned(),
        )
    }

    fn command_append(
        command_id: &str,
        idempotency_key: &str,
        command_type: CommandType,
    ) -> CommandAppend {
        CommandAppend::new(
            CommandEnvelope::new(
                command_id.to_owned(),
                command_type,
                "evt_gate".to_owned(),
                idempotency_key.to_owned(),
                "operator".to_owned(),
            ),
            "2026-06-23T00:00:02Z".to_owned(),
            Some("evt_gate".to_owned()),
            "corr_1".to_owned(),
            "{}".to_owned(),
        )
    }
}
