//! SQLite-backed append-only event store for the operator console.
//!
//! This crate owns the durable schema for canonical console events, operator
//! command envelopes, command status updates, and source adapter checkpoints. It
//! deduplicates events by event id or source-event id and reconstructs domain
//! envelopes for projections.
//!
//! The store persists to a `SQLite` database file, or to an in-memory database for tests, through a single owned connection.
//!
//! ```rust,ignore
//! use console_eventstore::SqliteEventStore;
//!
//! let mut store = SqliteEventStore::open_in_memory()?;
//! let events = store.list_console_events()?;
//! assert!(events.is_empty());
//! # Ok::<(), console_eventstore::EventStoreError>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

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
/// Variants for event store error state or outcome values.
pub enum EventStoreError {
    /// Command not found variant.
    CommandNotFound(String),
    /// Invalid sequence variant.
    InvalidSequence,
    /// Sqlite variant.
    Sqlite(rusqlite::Error),
    /// Unknown event type variant.
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

/// Type alias for event store result values.
pub type EventStoreResult<T> = Result<T, EventStoreError>;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents command append data used by the console.
pub struct CommandAppend {
    command: CommandEnvelope,
    requested_at: String,
    causation_event_id: Option<String>,
    correlation_id: String,
    payload_json: String,
}

impl CommandAppend {
    #[must_use]
    /// Construct a new value from its required fields.
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
    /// Return the wrapped command envelope.
    pub const fn command(&self) -> &CommandEnvelope {
        &self.command
    }

    #[must_use]
    /// Return the causation event id value.
    pub fn causation_event_id(&self) -> Option<&str> {
        self.causation_event_id.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents event append data used by the console.
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
    /// Construct a new value from its required fields.
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
    /// Return the wrapped console event.
    pub const fn event(&self) -> &ConsoleEvent {
        &self.event
    }

    #[must_use]
    /// Return the source event id value.
    pub fn source_event_id(&self) -> Option<&str> {
        self.source_event_id.as_deref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Variants for append status state or outcome values.
pub enum AppendStatus {
    /// Inserted variant.
    Inserted,
    /// Duplicate variant.
    Duplicate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Represents append outcome data used by the console.
pub struct AppendOutcome {
    global_seq: u64,
    status: AppendStatus,
}

impl AppendOutcome {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(global_seq: u64, status: AppendStatus) -> Self {
        Self { global_seq, status }
    }

    #[must_use]
    /// Return the global event-store sequence.
    pub const fn global_seq(&self) -> u64 {
        self.global_seq
    }

    #[must_use]
    /// Return the outcome status.
    pub const fn status(&self) -> AppendStatus {
        self.status
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents stored event data used by the console.
pub struct StoredEvent {
    global_seq: u64,
    event_id: String,
    event_type: String,
    source: String,
    source_event_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents stored command data used by the console.
pub struct StoredCommand {
    command_id: String,
    context: String,
    command_type: String,
    aggregate_id: Option<String>,
    idempotency_key: String,
    requested_by: String,
    status: String,
    requested_at: String,
    updated_at: String,
    /// The persisted `payload_json` column. `None` until re-attached by
    /// [`Self::with_payload_json`] when a command is loaded, so a handler can
    /// read a command's payload (for example a reject command's `mode`).
    payload_json: Option<String>,
    error_json: Option<String>,
}

impl StoredCommand {
    #[must_use]
    /// Construct a new value from its required fields.
    ///
    /// The returned command has no payload until [`Self::with_payload_json`]
    /// re-attaches the persisted `payload_json`.
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
            requested_at: String::new(),
            updated_at: String::new(),
            payload_json: None,
            error_json: None,
        }
    }

    #[must_use]
    /// Re-attach the persisted command lifecycle timestamps to a stored command.
    pub fn with_lifecycle_timestamps(mut self, requested_at: String, updated_at: String) -> Self {
        self.requested_at = requested_at;
        self.updated_at = updated_at;
        self
    }

    #[must_use]
    /// Re-attach the persisted `payload_json` to a stored command, used by the
    /// loader so a handler can read a command's payload.
    pub fn with_payload_json(mut self, payload_json: String) -> Self {
        self.payload_json = Some(payload_json);
        self
    }

    #[must_use]
    /// Re-attach the persisted `error_json` to a stored command, used by the
    /// loader so tests and projections can inspect terminal diagnostics.
    pub fn with_error_json(mut self, error_json: Option<String>) -> Self {
        self.error_json = error_json;
        self
    }

    #[must_use]
    /// Return the command id value.
    pub fn command_id(&self) -> &str {
        &self.command_id
    }

    #[must_use]
    /// Return the context value.
    pub fn context(&self) -> &str {
        &self.context
    }

    #[must_use]
    /// Return the command type value.
    pub fn command_type(&self) -> &str {
        &self.command_type
    }

    #[must_use]
    /// Return the aggregate id value.
    pub fn aggregate_id(&self) -> Option<&str> {
        self.aggregate_id.as_deref()
    }

    #[must_use]
    /// Return the idempotency key value.
    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }

    #[must_use]
    /// Return the requested by value.
    pub fn requested_by(&self) -> &str {
        &self.requested_by
    }

    #[must_use]
    /// Return the status value.
    pub fn status(&self) -> &str {
        &self.status
    }

    #[must_use]
    /// Return the requested-at timestamp value.
    pub fn requested_at(&self) -> &str {
        &self.requested_at
    }

    #[must_use]
    /// Return the updated-at timestamp value.
    pub fn updated_at(&self) -> &str {
        &self.updated_at
    }

    #[must_use]
    /// Return the persisted command payload, defaulting to the empty object
    /// `{}` when no payload was attached.
    pub fn payload_json(&self) -> &str {
        self.payload_json.as_deref().unwrap_or("{}")
    }

    #[must_use]
    /// Return the persisted command error payload, when present.
    pub fn error_json(&self) -> Option<&str> {
        self.error_json.as_deref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Variants for command append status state or outcome values.
pub enum CommandAppendStatus {
    /// Inserted variant.
    Inserted,
    /// Duplicate variant.
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents command append outcome data used by the console.
pub struct CommandAppendOutcome {
    command_id: String,
    status: CommandAppendStatus,
}

impl CommandAppendOutcome {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(command_id: String, status: CommandAppendStatus) -> Self {
        Self { command_id, status }
    }

    #[must_use]
    /// Return the command id value.
    pub fn command_id(&self) -> &str {
        &self.command_id
    }

    #[must_use]
    /// Return the outcome status.
    pub const fn status(&self) -> CommandAppendStatus {
        self.status
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents command status update outcome data used by the console.
pub struct CommandStatusUpdateOutcome {
    command_id: String,
    status: String,
}

impl CommandStatusUpdateOutcome {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(command_id: String, status: String) -> Self {
        Self { command_id, status }
    }

    #[must_use]
    /// Return the command id value.
    pub fn command_id(&self) -> &str {
        &self.command_id
    }

    #[must_use]
    /// Return the status value.
    pub fn status(&self) -> &str {
        &self.status
    }
}

impl StoredEvent {
    #[must_use]
    /// Construct a new value from its required fields.
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
    /// Return the global event-store sequence.
    pub const fn global_seq(&self) -> u64 {
        self.global_seq
    }

    #[must_use]
    /// Return the event id value.
    pub fn event_id(&self) -> &str {
        &self.event_id
    }

    #[must_use]
    /// Return the event type value.
    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    #[must_use]
    /// Return the source value.
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    /// Return the source event id value.
    pub fn source_event_id(&self) -> Option<&str> {
        self.source_event_id.as_deref()
    }
}

/// Represents sqlite event store data used by the console.
pub struct SqliteEventStore {
    connection: Connection,
}

impl SqliteEventStore {
    /// Return the open value.
    pub fn open(path: &Path) -> EventStoreResult<Self> {
        let connection = Connection::open(path)?;
        initialize_connection(&connection)?;
        Ok(Self { connection })
    }

    /// Return the open in memory value.
    pub fn open_in_memory() -> EventStoreResult<Self> {
        let connection = Connection::open_in_memory()?;
        initialize_connection(&connection)?;
        Ok(Self { connection })
    }

    /// Append event to the backing store.
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

    /// Append command to the backing store.
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

    /// List events from the backing store.
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

    /// List console events from the backing store.
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

    /// List commands from the backing store.
    pub fn list_commands(&self) -> EventStoreResult<Vec<StoredCommand>> {
        let sql = r"
            select command_id, context, type, aggregate_id, idempotency_key, requested_by, status,
                   requested_at, updated_at, payload_json, error_json
            from commands
            order by requested_at, command_id
        ";
        let mut statement = self.connection.prepare(sql)?;
        let mut rows = statement.query([])?;
        let mut commands = Vec::new();
        while let Some(row) = rows.next()? {
            commands.push(
                StoredCommand::new(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                )
                .with_lifecycle_timestamps(row.get(7)?, row.get(8)?)
                .with_payload_json(row.get::<_, String>(9)?)
                .with_error_json(row.get(10)?),
            );
        }
        Ok(commands)
    }

    /// Atomically claim a pending command for one consumer.
    ///
    /// Returns `true` only for the consumer that moved the command from
    /// `pending` to `executing`; duplicate consumers receive `false` and must
    /// not execute the command's side effect.
    pub fn claim_command(&mut self, command_id: &str, claimed_at: &str) -> EventStoreResult<bool> {
        let sql = r"
            update commands
            set status = 'executing',
                updated_at = ?2
            where command_id = ?1
              and status = 'pending'
            ";
        let claimed = self
            .connection
            .execute(sql, params![command_id, claimed_at])?;
        Ok(claimed == 1)
    }

    /// Return the update command status value.
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

    /// Finalize a command owned by an executing consumer.
    pub fn finalize_executing_command_status(
        &mut self,
        command_id: &str,
        status: &str,
        updated_at: &str,
        result_json: Option<&str>,
        error_json: Option<&str>,
    ) -> EventStoreResult<CommandStatusUpdateOutcome> {
        let sql = r"
            update commands
            set status = ?2,
                result_json = ?3,
                error_json = ?4,
                updated_at = ?5
            where command_id = ?1
              and status = 'executing'
            ";
        let updated = self.connection.execute(
            sql,
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

    /// Mark stale executing commands as failed for operator-visible recovery.
    pub fn fail_stale_executing_commands(
        &mut self,
        stale_before: &str,
        recovered_at: &str,
        error_json: &str,
    ) -> EventStoreResult<usize> {
        let sql = r"
            update commands
            set status = 'failed',
                error_json = ?3,
                updated_at = ?2
            where status = 'executing'
              and updated_at < ?1
            ";
        let updated = self
            .connection
            .execute(sql, params![stale_before, recovered_at, error_json])?;
        Ok(updated)
    }

    /// Load checkpoint from the backing store.
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

    /// Save checkpoint to the backing store.
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
    // With WAL a reader never blocks a writer, but the two writers the live TUI
    // runs — the UI thread's effect appends and the off-thread source poller —
    // still serialize; wait out a peer's brief write rather than failing SQLITE_BUSY.
    connection.pragma_update(None, "busy_timeout", 5000)?;
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
    #![allow(clippy::manual_assert, clippy::option_if_let_else, clippy::panic)]

    use super::{
        AppendStatus, CommandAppend, CommandAppendStatus, CommandStatusUpdateOutcome, EventAppend,
        EventStoreError, EventStoreResult, SqliteEventStore, StoredCommand, sequence_from_rowid,
    };
    use console_application::{
        build_tui_model,
        source_adapters::{AcceptancePolicy, AdmissionPolicy, Lane, LaneReason},
    };
    use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};
    use rusqlite::{Rows, Statement, Transaction};

    #[test]
    fn opened_store_uses_wal_mode_and_creates_required_tables() {
        let path = std::env::temp_dir().join(format!(
            "livespec-console-eventstore-{}.sqlite",
            std::process::id()
        ));
        let _remove_result = std::fs::remove_file(&path);
        let store = ok_store(SqliteEventStore::open(&path));

        let journal_mode = ok_string(store.connection.query_row(
            "pragma journal_mode",
            [],
            |row| row.get(0),
        ));
        for table_name in ["events", "commands", "checkpoints"] {
            let sql = format!("select count(*) from {table_name}");
            ok_statement(store.connection.prepare(&sql));
        }
        err_statement(store.connection.prepare("select count(*) from projections"));

        check(journal_mode == "wal", "eventstore test assertion");
        let _remove_result = std::fs::remove_file(&path);
    }

    #[test]
    fn append_event_persists_canonical_event_row() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        let append = event_append("evt_1", Some("source-1"));

        let outcome = ok_append_outcome(store.append_event(&append));
        let events = ok_events(store.list_events());

        check(
            outcome.status() == AppendStatus::Inserted,
            "eventstore test assertion",
        );
        check(outcome.global_seq() == 1, "eventstore test assertion");
        check(events.len() == 1, "eventstore test assertion");
        check(events[0].global_seq() == 1, "eventstore test assertion");
        check(events[0].event_id() == "evt_1", "eventstore test assertion");
        check(
            events[0].event_type() == "fabro.human_gate_observed",
            "eventstore test assertion",
        );
        check(events[0].source() == "fabro", "eventstore test assertion");
        check(
            events[0].source_event_id() == Some("source-1"),
            "eventstore test assertion",
        );
        check(
            append.event().event_id() == "evt_1",
            "eventstore test assertion",
        );
        check(
            append.source_event_id() == Some("source-1"),
            "eventstore test assertion",
        );
    }

    #[test]
    fn list_console_events_rebuilds_domain_events() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
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

        ok_append_outcome(store.append_event(&first));
        ok_append_outcome(store.append_event(&second));
        let events = ok_console_events(store.list_console_events());

        check(events.len() == 2, "eventstore test assertion");
        check(events[0].event_id() == "evt_1", "eventstore test assertion");
        check(
            events[0].event_type() == &EventType::FabroHumanGateObserved,
            "eventstore test assertion",
        );
        check(events[0].source() == "fabro", "eventstore test assertion");
        check(events[0].stream_seq() == 1, "eventstore test assertion");
        check(events[1].event_id() == "evt_2", "eventstore test assertion");
        check(
            events[1].event_type() == &EventType::DispatcherBacklogBounceObserved,
            "eventstore test assertion",
        );
        check(
            events[1].context() == "dispatch",
            "eventstore test assertion",
        );
        check(events[1].stream_seq() == 2, "eventstore test assertion");
        check(
            events[1].payload_json() == "{}",
            "eventstore test assertion",
        );
    }

    #[test]
    fn list_console_events_attaches_persisted_payload_json() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
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

        ok_append_outcome(store.append_event(&append));
        let events = ok_console_events(store.list_console_events());

        check(events.len() == 1, "eventstore test assertion");
        check(
            events[0].payload_json() == payload,
            "eventstore test assertion",
        );
    }

    #[test]
    fn work_item_projections_rebuild_identically_after_store_wipe_and_ledger_replay() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
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
        ok_append_outcome(store.append_event(&pending));
        ok_append_outcome(store.append_event(&ready));
        ok_append_outcome(store.append_event(&blocked));

        let command = command_append("cmd_1", "idem_1", CommandType::FactoryDrainRequested);
        ok_command_append_outcome(store.append_command(&command));
        let status_update = ok_status_update(store.update_command_status(
            "cmd_1",
            "completed",
            "2026-06-23T00:00:03Z",
            Some(r#"{"event_count":3}"#),
            None,
        ));
        check(
            status_update.status() == "completed",
            "eventstore test assertion",
        );

        let original_events = ok_console_events(store.list_console_events());
        let original_model = build_tui_model(&original_events, 0);

        let mut rebuilt = ok_store(SqliteEventStore::open_in_memory());
        for event in original_events
            .iter()
            .filter(|event| event.event_type() == &EventType::WorkItemSnapshotObserved)
        {
            ok_append_outcome(rebuilt.append_event(&replayed_work_item_append(event)));
        }
        let rebuilt_events = ok_console_events(rebuilt.list_console_events());
        let rebuilt_model = build_tui_model(&rebuilt_events, 0);

        check(
            ok_commands(rebuilt.list_commands()).is_empty(),
            "eventstore test assertion",
        );
        check(rebuilt_events.len() == 3, "eventstore test assertion");
        check(
            rebuilt_model.lane_board() == original_model.lane_board(),
            "eventstore test assertion",
        );
        check(
            rebuilt_model.attention_items() == original_model.attention_items(),
            "eventstore test assertion",
        );
        check(
            rebuilt_model.detail() == original_model.detail(),
            "eventstore test assertion",
        );
    }

    #[test]
    fn schema_has_no_primary_work_item_lifecycle_state_outside_command_carve_out() {
        let store = ok_store(SqliteEventStore::open_in_memory());

        err_statement(store.connection.prepare("select count(*) from projections"));
        for table_name in ["events", "checkpoints"] {
            for column_name in table_columns(&store, table_name) {
                check(
                    !["lane", "lane_reason", "work_item_status", "status"]
                        .contains(&column_name.as_str()),
                    "eventstore test assertion",
                );
            }
        }
        // `commands.status` is console-local operator-command state, not
        // work-item lifecycle state. It is intentionally excluded from
        // rebuild determinism and must not be event-sourced as a work-item
        // projection.
        check(
            table_columns(&store, "commands").contains(&"status".to_owned()),
            "eventstore test assertion",
        );
    }

    #[test]
    fn list_console_events_rejects_unknown_event_type() {
        let store = ok_store(SqliteEventStore::open_in_memory());

        let inserted = ok_execute_count(store.connection.execute(
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
        ));
        check(inserted == 1, "eventstore test assertion");

        let error = err_console_events(store.list_console_events());

        check_unknown_event_type(error, "unknown.event");
    }

    #[test]
    fn duplicate_source_event_id_returns_existing_sequence() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        let first = event_append("evt_1", Some("source-1"));
        let duplicate = event_append("evt_2", Some("source-1"));

        let first_outcome = ok_append_outcome(store.append_event(&first));
        let duplicate_outcome = ok_append_outcome(store.append_event(&duplicate));
        let events = ok_events(store.list_events());

        check(
            first_outcome.status() == AppendStatus::Inserted,
            "eventstore test assertion",
        );
        check(
            duplicate_outcome.status() == AppendStatus::Duplicate,
            "eventstore test assertion",
        );
        check(
            duplicate_outcome.global_seq() == first_outcome.global_seq(),
            "eventstore test assertion",
        );
        check(events.len() == 1, "eventstore test assertion");
        check(events[0].event_id() == "evt_1", "eventstore test assertion");
    }

    #[test]
    fn duplicate_event_id_without_source_event_id_returns_existing_sequence() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        let first = event_append("evt_1", None);
        let duplicate = event_append("evt_1", None);

        let first_outcome = ok_append_outcome(store.append_event(&first));
        let duplicate_outcome = ok_append_outcome(store.append_event(&duplicate));

        check(
            duplicate_outcome.status() == AppendStatus::Duplicate,
            "eventstore test assertion",
        );
        check(
            duplicate_outcome.global_seq() == first_outcome.global_seq(),
            "eventstore test assertion",
        );
    }

    #[test]
    fn append_command_persists_pending_command_row() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        let append = command_append("cmd_1", "idem_1", CommandType::FactoryDrainRequested);

        let outcome = ok_command_append_outcome(store.append_command(&append));
        let commands = ok_commands(store.list_commands());

        check(
            outcome.status() == CommandAppendStatus::Inserted,
            "eventstore test assertion",
        );
        check(outcome.command_id() == "cmd_1", "eventstore test assertion");
        check(commands.len() == 1, "eventstore test assertion");
        check(
            commands[0].command_id() == "cmd_1",
            "eventstore test assertion",
        );
        check(
            commands[0].context() == "factory",
            "eventstore test assertion",
        );
        check(
            commands[0].command_type() == "factory.drain_requested",
            "eventstore test assertion",
        );
        check(
            commands[0].aggregate_id() == Some("evt_gate"),
            "eventstore test assertion",
        );
        check(
            commands[0].idempotency_key() == "idem_1",
            "eventstore test assertion",
        );
        check(
            commands[0].requested_by() == "operator",
            "eventstore test assertion",
        );
        check(
            commands[0].status() == "pending",
            "eventstore test assertion",
        );
        check(
            append.command().command_id() == "cmd_1",
            "eventstore test assertion",
        );
        check(
            append.causation_event_id() == Some("evt_gate"),
            "eventstore test assertion",
        );
    }

    #[test]
    fn duplicate_command_id_returns_existing_command_id() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        let first = command_append("cmd_1", "idem_1", CommandType::FactoryDrainRequested);
        let duplicate = command_append("cmd_1", "idem_2", CommandType::FactoryDrainRequested);

        let first_outcome = ok_command_append_outcome(store.append_command(&first));
        let duplicate_outcome = ok_command_append_outcome(store.append_command(&duplicate));
        let commands = ok_commands(store.list_commands());

        check(
            first_outcome.status() == CommandAppendStatus::Inserted,
            "eventstore test assertion",
        );
        check(
            duplicate_outcome.status() == CommandAppendStatus::Duplicate,
            "eventstore test assertion",
        );
        check(
            duplicate_outcome.command_id() == "cmd_1",
            "eventstore test assertion",
        );
        check(commands.len() == 1, "eventstore test assertion");
        check(
            commands[0].command_type() == "factory.drain_requested",
            "eventstore test assertion",
        );
    }

    #[test]
    fn duplicate_idempotency_key_returns_existing_command_id() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        let first = command_append("cmd_1", "idem_1", CommandType::FactoryDrainRequested);
        let duplicate = command_append("cmd_2", "idem_1", CommandType::FactoryDrainRequested);

        let first_outcome = ok_command_append_outcome(store.append_command(&first));
        let duplicate_outcome = ok_command_append_outcome(store.append_command(&duplicate));
        let commands = ok_commands(store.list_commands());

        check(
            first_outcome.status() == CommandAppendStatus::Inserted,
            "eventstore test assertion",
        );
        check(
            duplicate_outcome.status() == CommandAppendStatus::Duplicate,
            "eventstore test assertion",
        );
        check(
            duplicate_outcome.command_id() == "cmd_1",
            "eventstore test assertion",
        );
        check(commands.len() == 1, "eventstore test assertion");
        check(
            commands[0].command_id() == "cmd_1",
            "eventstore test assertion",
        );
    }

    #[test]
    fn command_claim_wins_once_and_ignores_duplicate_consumers() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        let append = command_append("cmd_1", "idem_1", CommandType::FactoryDrainRequested);
        ok_command_append_outcome(store.append_command(&append));

        check(
            ok_claimed(store.claim_command("cmd_1", "2026-06-23T00:00:02Z")),
            "eventstore test assertion",
        );
        check(
            !ok_claimed(store.claim_command("cmd_1", "2026-06-23T00:00:03Z")),
            "eventstore test assertion",
        );

        let commands = ok_commands(store.list_commands());
        check(
            commands[0].status() == "executing",
            "eventstore test assertion",
        );
    }

    #[test]
    fn command_claim_updates_only_the_claimed_command_timestamp() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        let first = command_append("cmd_1", "idem_1", CommandType::FactoryDrainRequested);
        let second = command_append("cmd_2", "idem_2", CommandType::FactoryDrainRequested);
        ok_command_append_outcome(store.append_command(&first));
        ok_command_append_outcome(store.append_command(&second));

        check(
            ok_claimed(store.claim_command("cmd_1", "2026-06-23T00:00:03Z")),
            "eventstore test assertion",
        );
        let commands = ok_commands(store.list_commands());
        let claimed = stored_command(
            commands
                .iter()
                .find(|command| command.command_id() == "cmd_1"),
        );
        let untouched = stored_command(
            commands
                .iter()
                .find(|command| command.command_id() == "cmd_2"),
        );

        check(claimed.status() == "executing", "eventstore test assertion");
        check(
            claimed.updated_at() == "2026-06-23T00:00:03Z",
            "eventstore test assertion",
        );
        check(untouched.status() == "pending", "eventstore test assertion");
        check(
            untouched.updated_at() == untouched.requested_at(),
            "eventstore test assertion",
        );
    }

    #[test]
    fn executing_command_finalization_requires_an_owned_claim() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        let append = command_append("cmd_1", "idem_1", CommandType::FactoryDrainRequested);
        ok_command_append_outcome(store.append_command(&append));

        let unclaimed = err_status_update(store.finalize_executing_command_status(
            "cmd_1",
            "completed",
            "2026-06-23T00:00:03Z",
            Some(r#"{"event_count":0}"#),
            None,
        ));
        check_command_not_found(unclaimed, "cmd_1");

        check(
            ok_claimed(store.claim_command("cmd_1", "2026-06-23T00:00:02Z")),
            "eventstore test assertion",
        );
        let result_json = Some(r#"{"event_count":0}"#);
        let claimed = ok_status_update(store.finalize_executing_command_status(
            "cmd_1",
            "completed",
            "2026-06-23T00:00:03Z",
            result_json,
            None,
        ));

        check(claimed.status() == "completed", "eventstore test assertion");
        check(
            ok_commands(store.list_commands())[0].status() == "completed",
            "eventstore test assertion",
        );
    }

    #[test]
    fn stale_executing_commands_fail_only_before_the_cutoff() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        let stale = command_append(
            "cmd_stale",
            "idem_stale",
            CommandType::FactoryDrainRequested,
        );
        let fresh = command_append(
            "cmd_fresh",
            "idem_fresh",
            CommandType::FactoryDrainRequested,
        );
        ok_command_append_outcome(store.append_command(&stale));
        ok_command_append_outcome(store.append_command(&fresh));
        check(
            ok_claimed(store.claim_command("cmd_stale", "2026-06-22T00:00:00Z")),
            "eventstore test assertion",
        );
        check(
            ok_claimed(store.claim_command("cmd_fresh", "2026-06-23T00:00:00Z")),
            "eventstore test assertion",
        );

        let cutoff = "2026-06-22T12:00:00Z";
        let recovered_at = "2026-06-23T12:00:00Z";
        let error_json = r#"{"reason":"stale"}"#;
        let recovered = ok_recovered_count(store.fail_stale_executing_commands(
            cutoff,
            recovered_at,
            error_json,
        ));
        let commands = ok_commands(store.list_commands());

        let stale_status = commands
            .iter()
            .find(|command| command.command_id() == "cmd_stale")
            .map(StoredCommand::status);
        let fresh_status = commands
            .iter()
            .find(|command| command.command_id() == "cmd_fresh")
            .map(StoredCommand::status);

        check(recovered == 1, "eventstore test assertion");
        check(stale_status == Some("failed"), "eventstore test assertion");
        check(
            fresh_status == Some("executing"),
            "eventstore test assertion",
        );
    }

    #[test]
    fn command_status_update_marks_existing_command() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        let append = command_append("cmd_1", "idem_1", CommandType::FactoryDrainRequested);
        ok_command_append_outcome(store.append_command(&append));

        let outcome = ok_status_update(store.update_command_status(
            "cmd_1",
            "completed",
            "2026-06-23T00:00:03Z",
            Some(r#"{"event_count":3}"#),
            None,
        ));
        let commands = ok_commands(store.list_commands());

        check(outcome.command_id() == "cmd_1", "eventstore test assertion");
        check(outcome.status() == "completed", "eventstore test assertion");
        check(
            commands[0].status() == "completed",
            "eventstore test assertion",
        );
    }

    #[test]
    fn executing_command_finalization_reports_sqlite_failure() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        ok_sqlite_unit(store.connection.execute_batch("drop table commands"));

        let outcome = err_status_update(store.finalize_executing_command_status(
            "cmd_1",
            "completed",
            "2026-06-23T00:00:03Z",
            Some(r#"{"event_count":0}"#),
            None,
        ));

        check_sqlite_error(outcome);
    }

    #[test]
    fn command_status_update_rejects_unknown_command() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());

        let outcome = err_status_update(store.update_command_status(
            "cmd_missing",
            "completed",
            "2026-06-23T00:00:03Z",
            None,
            None,
        ));

        check_command_not_found(outcome, "cmd_missing");
    }

    #[test]
    fn command_status_update_reports_sqlite_failure() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        ok_sqlite_unit(store.connection.execute_batch("drop table commands"));

        let outcome = err_status_update(store.update_command_status(
            "cmd_1",
            "completed",
            "2026-06-23T00:00:03Z",
            None,
            None,
        ));

        check_sqlite_error(outcome);
    }

    #[test]
    fn missing_checkpoint_loads_as_none() {
        let store = ok_store(SqliteEventStore::open_in_memory());

        let checkpoint = ok_checkpoint(store.load_checkpoint("orchestrator:repo"));

        check(checkpoint.is_none(), "eventstore test assertion");
    }

    #[test]
    fn checkpoint_save_and_load_round_trips_latest_value() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());

        let key = "orchestrator:repo";
        ok_eventstore_unit(store.save_checkpoint(key, r#"{"version":1}"#, "2026-06-24T00:00:00Z"));
        ok_eventstore_unit(store.save_checkpoint(key, r#"{"version":2}"#, "2026-06-24T00:00:01Z"));
        ok_eventstore_unit(store.save_checkpoint(
            "fabro:repo",
            r#"{"cursor":"run_1"}"#,
            "2026-06-24T00:00:02Z",
        ));

        check(
            ok_checkpoint(store.load_checkpoint("orchestrator:repo"))
                == Some(r#"{"version":2}"#.to_owned()),
            "eventstore test assertion",
        );
        check(
            ok_checkpoint(store.load_checkpoint("fabro:repo"))
                == Some(r#"{"cursor":"run_1"}"#.to_owned()),
            "eventstore test assertion",
        );
    }

    #[test]
    fn checkpoint_save_reports_sqlite_failure() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        ok_sqlite_unit(store.connection.execute_batch("drop table checkpoints"));

        let result = err_eventstore_unit(store.save_checkpoint(
            "orchestrator:repo",
            r#"{"version":1}"#,
            "2026-06-24T00:00:00Z",
        ));

        check_sqlite_error(result);
    }

    #[test]
    fn missing_duplicate_command_lookup_returns_sqlite_error() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        let append = command_append(
            "cmd_missing",
            "idem_missing",
            CommandType::FactoryDrainRequested,
        );
        let transaction = ok_transaction(store.connection.transaction());
        let result = err_command_id(super::find_existing_command_id(&transaction, &append));

        check_sqlite_error(result);
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

        check(command.command_id() == "cmd_1", "eventstore test assertion");
        check(command.context() == "factory", "eventstore test assertion");
        check(
            command.command_type() == "factory.drain_requested",
            "eventstore test assertion",
        );
        check(
            command.aggregate_id().is_none(),
            "eventstore test assertion",
        );
        check(
            command.idempotency_key() == "idem_1",
            "eventstore test assertion",
        );
        check(
            command.requested_by() == "operator",
            "eventstore test assertion",
        );
        check(command.status() == "pending", "eventstore test assertion");
        // A command loaded without a re-attached payload defaults to `{}`.
        check(command.payload_json() == "{}", "eventstore test assertion");
    }

    #[test]
    fn list_commands_surfaces_the_persisted_command_payload() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        // A reject command carries its `mode` beyond the aggregate id, so the
        // loader must surface the persisted `payload_json` for the handler.
        ok_command_append_outcome(store.append_command(&CommandAppend::new(
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
            r#"{"mode":"regroom"}"#.to_owned(),
        )));

        let commands = ok_commands(store.list_commands());

        check(commands.len() == 1, "eventstore test assertion");
        check(
            commands[0].command_type() == "work_item.reject_requested",
            "eventstore test assertion",
        );
        check(
            commands[0].payload_json() == r#"{"mode":"regroom"}"#,
            "eventstore test assertion",
        );
    }

    #[test]
    fn negative_rowid_is_invalid_sequence() {
        let result = err_sequence(sequence_from_rowid(-1));

        check_invalid_sequence(result);
    }

    #[test]
    fn sqlite_errors_convert_to_event_store_errors() {
        let result = EventStoreError::from(rusqlite::Error::InvalidQuery);

        check_sqlite_error(result);
    }

    #[test]
    #[should_panic(expected = "check failed")]
    fn check_false_panics() {
        check(false, "check failed");
    }

    #[test]
    #[should_panic(expected = "check_unknown_event_type failed")]
    fn check_unknown_event_type_panics() {
        check_unknown_event_type(EventStoreError::InvalidSequence, "unknown.event");
    }

    #[test]
    #[should_panic(expected = "check_command_not_found failed")]
    fn check_command_not_found_panics() {
        check_command_not_found(EventStoreError::InvalidSequence, "cmd_1");
    }

    #[test]
    #[should_panic(expected = "check_sqlite_error failed")]
    fn check_sqlite_error_panics() {
        check_sqlite_error(EventStoreError::InvalidSequence);
    }

    #[test]
    #[should_panic(expected = "check_invalid_sequence failed")]
    fn check_invalid_sequence_panics() {
        check_invalid_sequence(EventStoreError::Sqlite(rusqlite::Error::InvalidQuery));
    }

    #[test]
    #[should_panic(expected = "ok_store failed")]
    fn ok_store_panics() {
        ok_store(Err(EventStoreError::InvalidSequence));
    }

    #[test]
    #[should_panic(expected = "ok_append_outcome failed")]
    fn ok_append_outcome_panics() {
        ok_append_outcome(Err(EventStoreError::InvalidSequence));
    }

    #[test]
    #[should_panic(expected = "ok_events failed")]
    fn ok_events_panics() {
        ok_events(Err(EventStoreError::InvalidSequence));
    }

    #[test]
    #[should_panic(expected = "ok_console_events failed")]
    fn ok_console_events_panics() {
        ok_console_events(Err(EventStoreError::InvalidSequence));
    }

    #[test]
    #[should_panic(expected = "ok_command_append_outcome failed")]
    fn ok_command_append_outcome_panics() {
        ok_command_append_outcome(Err(EventStoreError::InvalidSequence));
    }

    #[test]
    #[should_panic(expected = "ok_claimed failed")]
    fn ok_claimed_panics() {
        ok_claimed(Err(EventStoreError::InvalidSequence));
    }

    #[test]
    #[should_panic(expected = "ok_commands failed")]
    fn ok_commands_panics() {
        ok_commands(Err(EventStoreError::InvalidSequence));
    }

    #[test]
    #[should_panic(expected = "ok_status_update failed")]
    fn ok_status_update_panics() {
        ok_status_update(Err(EventStoreError::InvalidSequence));
    }

    #[test]
    #[should_panic(expected = "ok_recovered_count failed")]
    fn ok_recovered_count_panics() {
        ok_recovered_count(Err(EventStoreError::InvalidSequence));
    }

    #[test]
    #[should_panic(expected = "ok_checkpoint failed")]
    fn ok_checkpoint_panics() {
        ok_checkpoint(Err(EventStoreError::InvalidSequence));
    }

    #[test]
    #[should_panic(expected = "ok_eventstore_unit failed")]
    fn ok_eventstore_unit_panics() {
        ok_eventstore_unit(Err(EventStoreError::InvalidSequence));
    }

    #[test]
    #[should_panic(expected = "ok_string failed")]
    fn ok_string_panics() {
        ok_string(Err(rusqlite::Error::InvalidQuery));
    }

    #[test]
    #[should_panic(expected = "ok_execute_count failed")]
    fn ok_execute_count_panics() {
        ok_execute_count(Err(rusqlite::Error::InvalidQuery));
    }

    #[test]
    #[should_panic(expected = "ok_sqlite_unit failed")]
    fn ok_sqlite_unit_panics() {
        ok_sqlite_unit(Err(rusqlite::Error::InvalidQuery));
    }

    #[test]
    #[should_panic(expected = "ok_transaction failed")]
    fn ok_transaction_panics() {
        ok_transaction(Err(rusqlite::Error::InvalidQuery));
    }

    #[test]
    #[should_panic(expected = "ok_statement failed")]
    fn ok_statement_panics() {
        ok_statement(Err(rusqlite::Error::InvalidQuery));
    }

    #[test]
    #[should_panic(expected = "ok_rows failed")]
    fn ok_rows_panics() {
        let _rows = ok_rows(Err(rusqlite::Error::InvalidQuery));
    }

    #[test]
    #[should_panic(expected = "ok_next_row failed")]
    fn ok_next_row_panics() {
        ok_next_row(Err(rusqlite::Error::InvalidQuery));
    }

    #[test]
    #[should_panic(expected = "stored_command failed")]
    fn stored_command_panics() {
        stored_command(None);
    }

    #[test]
    #[should_panic(expected = "err_statement failed")]
    fn err_statement_panics() {
        let store = ok_store(SqliteEventStore::open_in_memory());
        err_statement(store.connection.prepare("select count(*) from events"));
    }

    #[test]
    #[should_panic(expected = "err_console_events failed")]
    fn err_console_events_panics() {
        err_console_events(Ok(Vec::new()));
    }

    #[test]
    #[should_panic(expected = "err_status_update failed")]
    fn err_status_update_panics() {
        err_status_update(Ok(CommandStatusUpdateOutcome::new(
            "cmd_1".to_owned(),
            "completed".to_owned(),
        )));
    }

    #[test]
    #[should_panic(expected = "err_eventstore_unit failed")]
    fn err_eventstore_unit_panics() {
        err_eventstore_unit(Ok(()));
    }

    #[test]
    #[should_panic(expected = "err_command_id failed")]
    fn err_command_id_panics() {
        err_command_id(Ok("cmd_1".to_owned()));
    }

    #[test]
    #[should_panic(expected = "err_sequence failed")]
    fn err_sequence_panics() {
        err_sequence(Ok(1));
    }

    #[track_caller]
    fn check(condition: bool, context: &str) {
        if !condition {
            panic!("{context}");
        }
    }

    #[track_caller]
    fn check_unknown_event_type(error: EventStoreError, expected: &str) {
        match error {
            EventStoreError::UnknownEventType(event_type) if event_type == expected => {}
            other => panic!("check_unknown_event_type failed: {other:?}"),
        }
    }

    #[track_caller]
    fn check_command_not_found(error: EventStoreError, expected: &str) {
        match error {
            EventStoreError::CommandNotFound(command_id) if command_id == expected => {}
            other => panic!("check_command_not_found failed: {other:?}"),
        }
    }

    #[track_caller]
    fn check_sqlite_error(error: EventStoreError) {
        match error {
            EventStoreError::Sqlite(_error) => {}
            other => panic!("check_sqlite_error failed: {other:?}"),
        }
    }

    #[track_caller]
    fn check_invalid_sequence(error: EventStoreError) {
        match error {
            EventStoreError::InvalidSequence => {}
            other => panic!("check_invalid_sequence failed: {other:?}"),
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
    fn ok_append_outcome(result: EventStoreResult<super::AppendOutcome>) -> super::AppendOutcome {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_append_outcome failed: {error:?}"),
        }
    }

    #[track_caller]
    fn ok_events(result: EventStoreResult<Vec<super::StoredEvent>>) -> Vec<super::StoredEvent> {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_events failed: {error:?}"),
        }
    }

    #[track_caller]
    fn ok_console_events(result: EventStoreResult<Vec<ConsoleEvent>>) -> Vec<ConsoleEvent> {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_console_events failed: {error:?}"),
        }
    }

    #[track_caller]
    fn ok_command_append_outcome(
        result: EventStoreResult<super::CommandAppendOutcome>,
    ) -> super::CommandAppendOutcome {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_command_append_outcome failed: {error:?}"),
        }
    }

    #[track_caller]
    fn ok_claimed(result: EventStoreResult<bool>) -> bool {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_claimed failed: {error:?}"),
        }
    }

    #[track_caller]
    fn ok_commands(result: EventStoreResult<Vec<StoredCommand>>) -> Vec<StoredCommand> {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_commands failed: {error:?}"),
        }
    }

    #[track_caller]
    fn ok_status_update(
        result: EventStoreResult<CommandStatusUpdateOutcome>,
    ) -> CommandStatusUpdateOutcome {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_status_update failed: {error:?}"),
        }
    }

    #[track_caller]
    fn ok_recovered_count(result: EventStoreResult<usize>) -> usize {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_recovered_count failed: {error:?}"),
        }
    }

    #[track_caller]
    fn ok_checkpoint(result: EventStoreResult<Option<String>>) -> Option<String> {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_checkpoint failed: {error:?}"),
        }
    }

    #[track_caller]
    fn ok_eventstore_unit(result: EventStoreResult<()>) {
        match result {
            Ok(()) => {}
            Err(error) => panic!("ok_eventstore_unit failed: {error:?}"),
        }
    }

    #[track_caller]
    fn ok_string(result: Result<String, rusqlite::Error>) -> String {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_string failed: {error:?}"),
        }
    }

    #[track_caller]
    fn ok_execute_count(result: Result<usize, rusqlite::Error>) -> usize {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_execute_count failed: {error:?}"),
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
    fn ok_transaction(result: Result<Transaction<'_>, rusqlite::Error>) -> Transaction<'_> {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_transaction failed: {error:?}"),
        }
    }

    #[track_caller]
    fn ok_statement(result: Result<Statement<'_>, rusqlite::Error>) -> Statement<'_> {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_statement failed: {error:?}"),
        }
    }

    #[track_caller]
    fn ok_rows(result: Result<Rows<'_>, rusqlite::Error>) -> Rows<'_> {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_rows failed: {error:?}"),
        }
    }

    #[track_caller]
    fn ok_next_row<'row>(
        result: Result<Option<&'row rusqlite::Row<'row>>, rusqlite::Error>,
    ) -> Option<&'row rusqlite::Row<'row>> {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_next_row failed: {error:?}"),
        }
    }

    #[track_caller]
    fn stored_command(result: Option<&StoredCommand>) -> &StoredCommand {
        match result {
            Some(value) => value,
            None => panic!("stored_command failed"),
        }
    }

    #[track_caller]
    fn err_statement(result: Result<Statement<'_>, rusqlite::Error>) -> rusqlite::Error {
        match result {
            Ok(_value) => panic!("err_statement failed"),
            Err(error) => error,
        }
    }

    #[track_caller]
    fn err_console_events(result: EventStoreResult<Vec<ConsoleEvent>>) -> EventStoreError {
        match result {
            Ok(_value) => panic!("err_console_events failed"),
            Err(error) => error,
        }
    }

    #[track_caller]
    fn err_status_update(result: EventStoreResult<CommandStatusUpdateOutcome>) -> EventStoreError {
        match result {
            Ok(_value) => panic!("err_status_update failed"),
            Err(error) => error,
        }
    }

    #[track_caller]
    fn err_eventstore_unit(result: EventStoreResult<()>) -> EventStoreError {
        match result {
            Ok(()) => panic!("err_eventstore_unit failed"),
            Err(error) => error,
        }
    }

    #[track_caller]
    fn err_command_id(result: EventStoreResult<String>) -> EventStoreError {
        match result {
            Ok(_value) => panic!("err_command_id failed"),
            Err(error) => error,
        }
    }

    #[track_caller]
    fn err_sequence(result: EventStoreResult<u64>) -> EventStoreError {
        match result {
            Ok(_value) => panic!("err_sequence failed"),
            Err(error) => error,
        }
    }

    fn table_columns(store: &SqliteEventStore, table_name: &str) -> Vec<String> {
        let mut statement = ok_statement(
            store
                .connection
                .prepare(&format!("pragma table_info({table_name})")),
        );
        let mut rows = ok_rows(statement.query([]));
        let mut columns = Vec::new();
        while let Some(row) = ok_next_row(rows.next()) {
            columns.push(ok_string(row.get(1)));
        }
        columns
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
