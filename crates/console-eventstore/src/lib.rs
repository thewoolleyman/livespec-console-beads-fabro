#![forbid(unsafe_code)]

use std::num::TryFromIntError;
use std::path::Path;

use console_domain::{CommandEnvelope, ConsoleEvent};
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

create table if not exists projections (
  name text not null,
  version integer not null,
  checkpoint_seq integer not null,
  state_json text not null,
  primary key (name, version)
);
";

#[derive(Debug)]
pub enum EventStoreError {
    InvalidSequence,
    Sqlite(rusqlite::Error),
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
        AppendStatus, CommandAppend, CommandAppendStatus, EventAppend, EventStoreError,
        SqliteEventStore, StoredCommand, sequence_from_rowid,
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
        for table_name in ["events", "commands", "checkpoints", "projections"] {
            let sql = format!("select count(*) from {table_name}");
            assert!(store.connection.prepare(&sql).is_ok());
        }

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
        let append = command_append(
            "cmd_1",
            "idem_1",
            CommandType::AttentionAcknowledgeRequested,
        );

        let outcome = store.append_command(&append)?;
        let commands = store.list_commands()?;

        assert_eq!(outcome.status(), CommandAppendStatus::Inserted);
        assert_eq!(outcome.command_id(), "cmd_1");
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command_id(), "cmd_1");
        assert_eq!(commands[0].context(), "attention");
        assert_eq!(
            commands[0].command_type(),
            "attention.acknowledge_requested"
        );
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
        let first = command_append(
            "cmd_1",
            "idem_1",
            CommandType::AttentionAcknowledgeRequested,
        );
        let duplicate = command_append("cmd_1", "idem_2", CommandType::AttentionSnoozeRequested);

        let first_outcome = store.append_command(&first)?;
        let duplicate_outcome = store.append_command(&duplicate)?;
        let commands = store.list_commands()?;

        assert_eq!(first_outcome.status(), CommandAppendStatus::Inserted);
        assert_eq!(duplicate_outcome.status(), CommandAppendStatus::Duplicate);
        assert_eq!(duplicate_outcome.command_id(), "cmd_1");
        assert_eq!(commands.len(), 1);
        assert_eq!(
            commands[0].command_type(),
            "attention.acknowledge_requested"
        );
        Ok(())
    }

    #[test]
    fn duplicate_idempotency_key_returns_existing_command_id() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let first = command_append(
            "cmd_1",
            "idem_1",
            CommandType::AttentionAcknowledgeRequested,
        );
        let duplicate = command_append("cmd_2", "idem_1", CommandType::AttentionSnoozeRequested);

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
