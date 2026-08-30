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
use std::time::Duration;

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

impl EventStoreError {
    /// Is this a TRANSIENT contention failure rather than a real fault?
    ///
    /// `SQLite` returns `SQLITE_BUSY` / `SQLITE_LOCKED` when a peer connection
    /// held the write lock longer than the busy timeout. Nothing was computed
    /// wrongly and nothing was committed — the same call may simply succeed a
    /// moment later. The live TUI runs several writer connections (the UI
    /// thread's effect appends, the off-thread source poller, and a fresh
    /// connection per command-lane invocation), so this is a reachable outcome
    /// rather than a theoretical one.
    ///
    /// Deliberately keyed on the `SQLite` PRIMARY code, never on the message
    /// text: the rendered string is a diagnostic, not a contract.
    #[must_use]
    pub const fn is_transient_contention(&self) -> bool {
        match self {
            Self::Sqlite(rusqlite::Error::SqliteFailure(error, _)) => matches!(
                error.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            ),
            _ => false,
        }
    }
}

/// How many times a store OPEN is attempted before the failure is reported.
///
/// Each failed attempt has ALREADY waited out `SQLite`'s 5 s busy timeout
/// (rusqlite arms `sqlite3_busy_timeout(db, 5000)` inside `Connection::open`,
/// before any user pragma runs), so the bound is deliberately small: three
/// attempts is at most ~15 s, which stays inside the e2e harness's 20 s
/// first-frame budget and therefore still lets an exhausted open print its
/// cause where the harness captures it.
pub const STORE_OPEN_ATTEMPTS: u32 = 3;

/// The pause before the next open attempt.
///
/// Deliberately SHORT and NOT a substitute for the busy timeout. The attempt
/// that just failed already spent five seconds waiting for the peer, so a long
/// sleep here buys nothing and only eats the startup budget.
#[must_use]
pub fn open_retry_backoff(attempt: u32) -> Duration {
    Duration::from_millis(50 * u64::from(attempt))
}

/// Should another open attempt follow `error` on attempt `attempt` of `attempts`?
///
/// Only TRANSIENT contention is retried, and only inside the bound. A real
/// fault is returned on its first attempt so a retry loop can never mask
/// corruption.
#[must_use]
pub const fn should_retry_open(error: &EventStoreError, attempt: u32, attempts: u32) -> bool {
    error.is_transient_contention() && attempt < attempts
}

/// Open the store, tolerating TRANSIENT contention up to `attempts` times.
///
/// This is the STARTUP counterpart to the running loop's contention tolerance,
/// and it deliberately makes the OPPOSITE call (livespec-console-beads-fabro-bss4rq).
/// The loop refuses a hidden retry because there is a live frame to report NOT
/// APPLIED onto and an operator whose keystroke is the natural retry. At startup
/// none of that holds — there is no frame yet and no gesture to inherit, so the
/// choice is retry or die — and an open is IDEMPOTENT, so retrying it cannot
/// double-apply anything the way re-issuing a write could.
///
/// `open` and `pause` are taken as `dyn` callbacks rather than generics so the
/// real filesystem open and the real sleep stay in the composition root, where
/// this bounded loop can be exercised against scripted failures instead of a
/// contrived race.
pub fn open_tolerating_contention(
    attempts: u32,
    open: &mut dyn FnMut() -> EventStoreResult<SqliteEventStore>,
    pause: &mut dyn FnMut(u32),
) -> EventStoreResult<SqliteEventStore> {
    let mut attempt: u32 = 1;
    loop {
        let error = match open() {
            Ok(store) => return Ok(store),
            Err(error) => error,
        };
        if !should_retry_open(&error, attempt, attempts) {
            return Err(error);
        }
        pause(attempt);
        attempt = attempt.saturating_add(1);
    }
}

/// Render the operator-facing line for a store open that could not proceed.
///
/// The cause is carried VERBATIM in every case: livespec-console-beads-fabro-4vsy7u
/// bought that diagnosability after ten days of an undiagnosable flake, and an
/// exhausted-retry message must not spend it. Contention additionally names
/// itself and the bound it exhausted, so "the store stayed busy" is
/// distinguishable from "the store is broken".
#[must_use]
pub fn render_open_failure(error: &EventStoreError, attempts: u32) -> String {
    if error.is_transient_contention() {
        format!(
            "store busy: {attempts} open attempts each waited out SQLite's busy timeout \
             and the write lock never cleared: {error:?}"
        )
    } else {
        format!("{error:?}")
    }
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
        // Route through `open` so the connection-creation and initialization
        // error arms live in ONE place — covered by `open`'s failure tests —
        // rather than duplicated here, where an in-memory open cannot be made
        // to fail and its `?` arms would be permanently unreachable. SQLite
        // treats the `:memory:` filename as a private in-memory database,
        // identical to `Connection::open_in_memory`.
        Self::open(Path::new(":memory:"))
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
        // `raw_query` binds nothing and returns rows infallibly for this
        // parameterless statement, so it avoids a permanently-unreachable `?`
        // arm that `query([])` would introduce; step and row-decode errors
        // still surface through `rows.next()?` below.
        let mut rows = statement.raw_query();
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
        // `raw_query` binds nothing and returns rows infallibly for this
        // parameterless statement, so it avoids a permanently-unreachable `?`
        // arm that `query([])` would introduce; step and row-decode errors
        // still surface through `rows.next()?` below.
        let mut rows = statement.raw_query();
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
        // `raw_query` binds nothing and returns rows infallibly for this
        // parameterless statement, so it avoids a permanently-unreachable `?`
        // arm that `query([])` would introduce; step and row-decode errors
        // still surface through `rows.next()?` below.
        let mut rows = statement.raw_query();
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
    // One batched pragma statement instead of three separate `pragma_update`
    // calls: the three settings then share a SINGLE fallible call site, so the
    // failure arm is reachable by one test — a read-only connection rejects the
    // journal_mode change — instead of requiring three distinct connection
    // states that make each pragma fail in turn (foreign_keys and busy_timeout
    // effectively never fail on their own). The applied settings are identical.
    //
    // With WAL a reader never blocks a writer, but the two writers the live TUI
    // runs — the UI thread's effect appends and the off-thread source poller —
    // still serialize; wait out a peer's brief write rather than failing SQLITE_BUSY.
    connection.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;",
    )?;
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
        EventStoreError, EventStoreResult, STORE_OPEN_ATTEMPTS, SqliteEventStore, StoredCommand,
        open_retry_backoff, open_tolerating_contention, render_open_failure, sequence_from_rowid,
    };
    use console_application::{
        build_tui_model,
        source_adapters::{AcceptancePolicy, AdmissionPolicy, Lane, LaneReason},
    };
    use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};
    use rusqlite::{Connection, OpenFlags, Rows, Statement, Transaction};

    #[test]
    fn transient_contention_is_keyed_on_the_sqlite_code_not_the_message() {
        // livespec-console-beads-fabro-ddfbcx.1. A momentary lock wait must be
        // distinguishable from a real fault WITHOUT sniffing the rendered
        // string, which is a diagnostic and not a contract.
        let busy = EventStoreError::Sqlite(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(5),
            Some("database is locked".to_owned()),
        ));
        let locked = EventStoreError::Sqlite(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(6),
            None,
        ));
        // A BUSY carrying no message at all is still transient — the code, not
        // the text, decides.
        let busy_without_message = EventStoreError::Sqlite(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(5),
            None,
        ));

        check(busy.is_transient_contention(), "SQLITE_BUSY is transient");
        check(
            locked.is_transient_contention(),
            "SQLITE_LOCKED is transient",
        );
        check(
            busy_without_message.is_transient_contention(),
            "a BUSY with no message text is still transient",
        );

        // NEGATIVE CONTROLS: real faults must NOT be absorbed as contention.
        let corrupt = EventStoreError::Sqlite(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(11),
            Some("database disk image is malformed".to_owned()),
        ));
        check(
            !corrupt.is_transient_contention(),
            "corruption is NOT transient contention",
        );
        check(
            !EventStoreError::InvalidSequence.is_transient_contention(),
            "a non-sqlite variant is NOT transient contention",
        );
        check(
            !EventStoreError::UnknownEventType("x".to_owned()).is_transient_contention(),
            "an unknown event type is NOT transient contention",
        );
    }

    #[test]
    fn startup_open_retries_transient_contention_within_its_bound() {
        // livespec-console-beads-fabro-bss4rq. Startup contention used to kill
        // the session before the first frame: `SqliteEventStore::open` runs
        // `execute_batch(SCHEMA)`, itself a write, and one walkthrough opens
        // EIGHT connections across SIX threads. An open is IDEMPOTENT and there
        // is no rendered surface yet on which to say NOT APPLIED, so the call
        // here is the OPPOSITE of ddfbcx.1's refusal-to-retry: retry, bounded.
        let (attempts_made, paused_before, opened) =
            drive_bounded_open(3, &|attempt| (attempt < 3).then(busy_error));

        check(opened, "a store that frees up within the bound opens");
        check(
            attempts_made == 3,
            "the opener is called once per attempt until it succeeds",
        );
        // Rendered, not matched: a `matches!` arm — or a `let ... else` panic —
        // is a branch no passing run takes, and the coverage gate refuses it.
        check(
            format!("{paused_before:?}") == "[1, 2]",
            "each retry pauses once, and the successful attempt does not",
        );
    }

    #[test]
    fn startup_open_gives_up_at_the_bound_without_exceeding_it() {
        let (attempts_made, paused_before, opened) =
            drive_bounded_open(STORE_OPEN_ATTEMPTS, &|_attempt| Some(busy_error()));

        check(
            !opened,
            "a store busy for every attempt must not report success",
        );
        check(
            attempts_made == STORE_OPEN_ATTEMPTS,
            "the bound is exhausted exactly, never exceeded",
        );
        check(
            format!("{paused_before:?}") == "[1, 2]",
            "the exhausted attempt does not pause on its way out",
        );
    }

    #[test]
    fn startup_open_does_not_retry_a_real_fault() {
        // NEGATIVE CONTROL. Without this the fix would mask corruption behind a
        // retry loop and a contention-flavoured message.
        let (attempts_made, paused_before, opened) =
            drive_bounded_open(STORE_OPEN_ATTEMPTS, &|_attempt| Some(corrupt_error()));

        check(!opened, "a corrupt database must not report success");
        check(
            attempts_made == 1,
            "a real fault is returned on its first attempt, never retried",
        );
        check(
            paused_before.is_empty(),
            "a real fault never pauses for a retry",
        );
    }

    #[test]
    fn exhausted_open_names_the_contention_and_still_carries_the_cause() {
        // 4vsy7u bought this diagnosability after ten days of an undiagnosable
        // flake; an exhausted-retry message must not spend it.
        let contention = render_open_failure(&busy_error(), STORE_OPEN_ATTEMPTS);
        check(
            contention.contains("DatabaseBusy"),
            "the exhausted line carries the sqlite cause verbatim",
        );
        check(
            contention.contains("store busy"),
            "the exhausted line names the contention rather than a bare failure",
        );

        // NEGATIVE CONTROL on the rendering itself: a real fault must not be
        // dressed up as contention.
        let fault = render_open_failure(&corrupt_error(), STORE_OPEN_ATTEMPTS);
        check(
            fault.contains("malformed"),
            "a real fault still carries its cause",
        );
        check(
            !fault.contains("store busy"),
            "a real fault is NOT reported as contention",
        );
    }

    #[test]
    fn open_retry_backoff_stays_short_and_grows() {
        // The failed attempt ALREADY waited out SQLite's 5 s busy timeout, so
        // the peer has had its wait. A long sleep here only eats the startup
        // budget the e2e harness measures against.
        let first = open_retry_backoff(1);
        let second = open_retry_backoff(2);

        check(first < second, "the backoff grows with the attempt");
        check(
            second <= std::time::Duration::from_millis(500),
            "the backoff stays far below the busy timeout already paid",
        );
    }

    /// Drive `open_tolerating_contention` against a scripted opener.
    ///
    /// `script` decides, per 1-based attempt, whether that open fails and with
    /// what. Returning `None` opens a real in-memory store. Every test above
    /// shares this one body so the callback lines it needs are exercised by the
    /// case that actually retries, rather than each test contributing its own
    /// never-taken copy.
    fn drive_bounded_open(
        attempts: u32,
        script: &dyn Fn(u32) -> Option<EventStoreError>,
    ) -> (u32, Vec<u32>, bool) {
        let mut attempts_made = 0_u32;
        let mut paused_before = Vec::new();
        let outcome = open_tolerating_contention(
            attempts,
            &mut || {
                attempts_made = attempts_made.saturating_add(1);
                script(attempts_made).map_or_else(SqliteEventStore::open_in_memory, Err)
            },
            &mut |attempt| paused_before.push(attempt),
        );
        (attempts_made, paused_before, outcome.is_ok())
    }

    fn busy_error() -> EventStoreError {
        EventStoreError::Sqlite(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(5),
            Some("database is locked".to_owned()),
        ))
    }

    fn corrupt_error() -> EventStoreError {
        EventStoreError::Sqlite(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(11),
            Some("database disk image is malformed".to_owned()),
        ))
    }

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
    fn opening_store_at_directory_reports_sqlite_failure() {
        let path = std::env::temp_dir().join(format!(
            "livespec-console-eventstore-dir-{}",
            std::process::id()
        ));
        let _ignored = std::fs::remove_dir_all(&path);
        ok_unit(std::fs::create_dir_all(&path));

        let error = err_store(SqliteEventStore::open(&path));

        check_sqlite_error(error);
        let _ignored = std::fs::remove_dir_all(&path);
    }

    #[test]
    fn opening_store_reports_schema_initialization_failure() {
        let path = std::env::temp_dir().join(format!(
            "livespec-console-eventstore-bad-schema-{}.sqlite",
            std::process::id()
        ));
        let _ignored = std::fs::remove_file(&path);
        let connection = ok_connection(Connection::open(&path));
        ok_sqlite_unit(
            connection.execute_batch("create table events (global_seq integer primary key);"),
        );
        drop(connection);

        let error = err_store(SqliteEventStore::open(&path));

        check_sqlite_error(error);
        let _ignored = std::fs::remove_file(&path);
    }

    #[test]
    fn append_event_reports_transaction_start_failure() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        ok_sqlite_unit(store.connection.execute_batch("begin immediate"));

        let error = err_append_outcome(store.append_event(&event_append("evt_tx", None)));

        check_sqlite_error(error);
        ok_sqlite_unit(store.connection.execute_batch("rollback"));
    }

    #[test]
    fn append_command_reports_transaction_start_failure() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        ok_sqlite_unit(store.connection.execute_batch("begin immediate"));

        let error = err_command_append_outcome(store.append_command(&command_append(
            "cmd_tx",
            "idem_tx",
            CommandType::FactoryDrainRequested,
        )));

        check_sqlite_error(error);
        ok_sqlite_unit(store.connection.execute_batch("rollback"));
    }

    #[test]
    fn append_event_reports_insert_sqlite_failure() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        ok_sqlite_unit(store.connection.execute_batch("drop table events"));

        let error = err_append_outcome(store.append_event(&event_append("evt_insert", None)));

        check_sqlite_error(error);
    }

    #[test]
    fn append_command_reports_insert_sqlite_failure() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        ok_sqlite_unit(store.connection.execute_batch("drop table commands"));

        let error = err_command_append_outcome(store.append_command(&command_append(
            "cmd_insert",
            "idem_insert",
            CommandType::FactoryDrainRequested,
        )));

        check_sqlite_error(error);
    }

    #[test]
    fn store_read_methods_report_missing_table_prepare_failures() {
        let store = ok_store(SqliteEventStore::open_in_memory());
        ok_sqlite_unit(store.connection.execute_batch("drop table events"));
        check_sqlite_error(err_events(store.list_events()));
        check_sqlite_error(err_console_events(store.list_console_events()));

        let store = ok_store(SqliteEventStore::open_in_memory());
        ok_sqlite_unit(store.connection.execute_batch("drop table commands"));
        check_sqlite_error(err_commands(store.list_commands()));

        let store = ok_store(SqliteEventStore::open_in_memory());
        ok_sqlite_unit(store.connection.execute_batch("drop table checkpoints"));
        check_sqlite_error(err_checkpoint(store.load_checkpoint("orchestrator:repo")));
    }

    #[test]
    fn command_mutators_report_missing_table_failures() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        ok_sqlite_unit(store.connection.execute_batch("drop table commands"));
        check_sqlite_error(err_claimed(
            store.claim_command("cmd_1", "2026-06-23T00:00:02Z"),
        ));

        let mut store = ok_store(SqliteEventStore::open_in_memory());
        ok_sqlite_unit(store.connection.execute_batch("drop table commands"));
        check_sqlite_error(err_recovered_count(store.fail_stale_executing_commands(
            "2026-06-22T00:00:00Z",
            "2026-06-23T00:00:00Z",
            "{}",
        )));
    }

    #[test]
    fn duplicate_lookup_helpers_report_missing_sequence_tables() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        let append = event_append("evt_missing_source", Some("source-missing"));
        ok_sqlite_unit(store.connection.execute_batch("drop table events"));
        let transaction = ok_transaction(store.connection.transaction());

        let error = err_sequence(super::find_existing_sequence(&transaction, &append));

        check_sqlite_error(error);
    }

    #[test]
    fn duplicate_lookup_helpers_report_missing_event_id_tables() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        let append = event_append("evt_missing_id", None);
        ok_sqlite_unit(store.connection.execute_batch("drop table events"));
        let transaction = ok_transaction(store.connection.transaction());

        let error = err_sequence(super::find_existing_sequence(&transaction, &append));

        check_sqlite_error(error);
    }

    #[test]
    fn list_events_reports_bad_row_values() {
        let store = ok_store(SqliteEventStore::open_in_memory());
        for column_name in [
            "global_seq",
            "event_id",
            "type",
            "source",
            "source_event_id",
        ] {
            replace_events_for_list_events(&store, column_name);

            let error = err_events(store.list_events());

            check_event_store_error(error);
        }
    }

    #[test]
    fn list_console_events_reports_bad_row_values() {
        let store = ok_store(SqliteEventStore::open_in_memory());
        for column_name in [
            "event_id",
            "schema_version",
            "context",
            "source",
            "stream_id",
            "stream_seq",
            "payload_json",
        ] {
            replace_events_for_list_console_events(&store, column_name);

            let error = err_console_events(store.list_console_events());

            check_event_store_error(error);
        }
    }

    #[test]
    fn list_commands_reports_bad_row_values() {
        let store = ok_store(SqliteEventStore::open_in_memory());
        for column_name in [
            "command_id",
            "context",
            "type",
            "aggregate_id",
            "idempotency_key",
            "requested_by",
            "status",
            "requested_at",
            "updated_at",
            "payload_json",
            "error_json",
        ] {
            replace_commands_for_list_commands(&store, column_name);

            let error = err_commands(store.list_commands());

            check_sqlite_error(error);
        }
    }

    // livespec-console-beads-fabro-txtzn5.14: the residual production `?`-arm
    // regions that drop-table/bad-row injection did not reach — the duplicate
    // and insert sequence-conversion arms, the transaction commit arms, the
    // inner column-decode arms, the missing-row lookup arm, and the read-step
    // corruption arm. The pragma-initialization, in-memory-open, and
    // parameterless-query arms were closed by restructuring the production code
    // (see `initialize_connection`, `open_in_memory`, and the `raw_query` reads).

    const EVENTS_SCHEMA_COLUMNS: &str = "global_seq integer primary key, \
         event_id text not null unique, context text not null, aggregate_id text not null, \
         stream_id text not null, stream_seq integer not null, type text not null, \
         schema_version integer not null, occurred_at text not null, observed_at text not null, \
         causation_id text null, correlation_id text not null, source text not null, \
         source_event_id text null, payload_json text not null, metadata_json text not null";

    const COMMANDS_SCHEMA_COLUMNS: &str = "command_id text primary key, context text not null, \
         type text not null, aggregate_id text null, idempotency_key text not null unique, \
         requested_by text not null, requested_at text not null, causation_event_id text null, \
         correlation_id text not null, status text not null, payload_json text not null, \
         result_json text null, error_json text null, updated_at text not null";

    #[test]
    fn append_event_duplicate_with_negative_stored_sequence_reports_invalid_sequence() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        // Seed a row carrying a NEGATIVE global_seq under a source-event id. An
        // append on that same source-event id is a no-op insert, so the
        // duplicate branch looks the stored sequence up and its conversion to a
        // non-negative sequence fails inside `find_existing_sequence`.
        ok_sqlite_unit(store.connection.execute_batch(
            "insert into events (global_seq, event_id, context, aggregate_id, stream_id, \
             stream_seq, type, schema_version, occurred_at, observed_at, causation_id, \
             correlation_id, source, source_event_id, payload_json, metadata_json) values \
             (-1, 'evt_seed', 'fabro', 'agg', 'st', 1, 'fabro.human_gate_observed', 1, 't', 't', \
             null, 'corr', 'fabro', 'sev_dup', '{}', '{}');",
        ));

        let error =
            err_append_outcome(store.append_event(&event_append("evt_dup", Some("sev_dup"))));

        check_invalid_sequence(error);
    }

    #[test]
    fn append_event_insert_with_negative_rowid_reports_invalid_sequence() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        // Seed a negative primary key so the NEXT auto-assigned rowid is also
        // negative; the fresh insert then reports a negative last_insert_rowid
        // whose conversion to a sequence fails.
        ok_sqlite_unit(store.connection.execute_batch(
            "insert into events (global_seq, event_id, context, aggregate_id, stream_id, \
             stream_seq, type, schema_version, occurred_at, observed_at, causation_id, \
             correlation_id, source, source_event_id, payload_json, metadata_json) values \
             (-9, 'evt_seed', 'fabro', 'agg', 'st', 1, 'fabro.human_gate_observed', 1, 't', 't', \
             null, 'corr', 'fabro', null, '{}', '{}');",
        ));

        let error = err_append_outcome(store.append_event(&event_append("evt_distinct", None)));

        check_invalid_sequence(error);
    }

    #[test]
    fn append_event_reports_commit_failure() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        // Recreate `events` with a deferred foreign key the appended row
        // violates (no parent row exists): the insert defers the check and the
        // failure surfaces at `transaction.commit()`.
        ok_sqlite_unit(store.connection.execute_batch(&format!(
            "drop table events; create table parent(id text primary key); \
             create table events ({EVENTS_SCHEMA_COLUMNS}, \
             foreign key(aggregate_id) references parent(id) deferrable initially deferred);"
        )));

        let error = err_append_outcome(store.append_event(&event_append("evt_commit", None)));

        check_sqlite_error(error);
    }

    #[test]
    fn append_command_reports_commit_failure() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        // Same deferred-foreign-key technique for the command insert path: the
        // insert defers and `transaction.commit()` fails.
        ok_sqlite_unit(store.connection.execute_batch(&format!(
            "drop table commands; create table parent(id text primary key); \
             create table commands ({COMMANDS_SCHEMA_COLUMNS}, \
             foreign key(aggregate_id) references parent(id) deferrable initially deferred);"
        )));

        let error = err_command_append_outcome(store.append_command(&command_append(
            "cmd_commit",
            "idem_commit",
            CommandType::FactoryDrainRequested,
        )));

        check_sqlite_error(error);
    }

    #[test]
    fn append_command_duplicate_with_blob_command_id_reports_sqlite_failure() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        // Seed a command whose command_id is stored as a BLOB under a known
        // idempotency key. Appending a command with that same idempotency key
        // takes the duplicate branch, where reading the blob command_id back as
        // text fails — exercising the `.optional()?` propagation and the
        // duplicate-lookup `?` in `append_command`.
        ok_sqlite_unit(store.connection.execute_batch(
            "insert into commands (command_id, context, type, aggregate_id, idempotency_key, \
             requested_by, requested_at, causation_event_id, correlation_id, status, payload_json, \
             updated_at) values (x'01', 'factory', 'factory.drain_requested', 'agg', 'idem_dup', \
             'operator', '2026-06-23T00:00:02Z', null, 'corr', 'pending', '{}', \
             '2026-06-23T00:00:02Z');",
        ));

        let error = err_command_append_outcome(store.append_command(&command_append(
            "cmd_new",
            "idem_dup",
            CommandType::FactoryDrainRequested,
        )));

        check_sqlite_error(error);
    }

    #[test]
    fn list_events_reports_blob_sequence_column_decode_failure() {
        let store = ok_store(SqliteEventStore::open_in_memory());
        // A BLOB in the global_seq column fails the inner `row.get::<_, i64>`
        // decode, distinct from the negative-integer case that drives the outer
        // sequence-conversion arm.
        ok_sqlite_unit(store.connection.execute_batch(
            "drop table events; \
             create table events (global_seq, event_id, type, source, source_event_id); \
             insert into events values (x'01', 'evt_1', 'fabro.human_gate_observed', 'fabro', null);",
        ));

        check_sqlite_error(err_events(store.list_events()));
    }

    #[test]
    fn list_console_events_reports_blob_type_column_decode_failure() {
        let store = ok_store(SqliteEventStore::open_in_memory());
        // A BLOB in the type column fails the `row.get::<_, String>(3)` decode.
        ok_sqlite_unit(store.connection.execute_batch(
            "drop table events; \
             create table events (global_seq integer, event_id, schema_version, context, type, \
             source, stream_id, stream_seq, payload_json); \
             insert into events values (1, 'evt_1', 1, 'ctx', x'01', 'src', 'st', 1, '{}');",
        ));

        check_sqlite_error(err_console_events(store.list_console_events()));
    }

    #[test]
    fn list_console_events_reports_blob_stream_seq_column_decode_failure() {
        let store = ok_store(SqliteEventStore::open_in_memory());
        // A BLOB in the stream_seq column fails the inner `row.get::<_, i64>(6)`
        // decode, distinct from the negative-integer case that drives the outer
        // sequence-conversion arm.
        ok_sqlite_unit(store.connection.execute_batch(
            "drop table events; \
             create table events (global_seq integer, event_id, schema_version, context, type, \
             source, stream_id, stream_seq, payload_json); \
             insert into events values (1, 'evt_1', 1, 'ctx', 'fabro.human_gate_observed', 'src', \
             'st', x'01', '{}');",
        ));

        check_sqlite_error(err_console_events(store.list_console_events()));
    }

    #[test]
    fn find_existing_sequence_reports_missing_row() {
        let mut store = ok_store(SqliteEventStore::open_in_memory());
        // The lookup query succeeds but returns no row, so the `ok_or` arm that
        // guards the sequence conversion fires.
        let append = event_append("evt_absent", Some("sev_absent"));
        let transaction = ok_transaction(store.connection.transaction());

        let error = err_sequence(super::find_existing_sequence(&transaction, &append));

        check_sqlite_error(error);
    }

    #[test]
    fn list_events_reports_row_step_corruption() {
        // Persist rows to a file, then corrupt every page after the intact
        // schema page: `prepare` still reads the schema page, but stepping into
        // the corrupted table b-tree fails at `rows.next()`.
        let dir = std::env::temp_dir().join(format!(
            "livespec-console-eventstore-corrupt-step-{}",
            std::process::id()
        ));
        let _ignored = std::fs::remove_dir_all(&dir);
        ok_unit(std::fs::create_dir_all(&dir));
        let path = dir.join("db.sqlite");
        {
            let mut store = ok_store(SqliteEventStore::open(&path));
            for index in 0..200 {
                let _outcome = ok_append_outcome(
                    store.append_event(&event_append(&format!("evt_{index}"), None)),
                );
            }
            // Fold the WAL back into the main file so the corruption below is the
            // only copy of the table data the reopened store can read.
            ok_sqlite_unit(
                store
                    .connection
                    .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);"),
            );
        }
        let _ignored = std::fs::remove_file(dir.join("db.sqlite-wal"));
        let _ignored = std::fs::remove_file(dir.join("db.sqlite-shm"));
        let mut bytes = ok_bytes(std::fs::read(&path));
        let page_size = ((usize::from(bytes[16]) << 8) | usize::from(bytes[17])).max(512);
        for byte in bytes.iter_mut().skip(page_size) {
            *byte = 0xAA;
        }
        ok_unit(std::fs::write(&path, &bytes));

        let store = ok_store(SqliteEventStore::open(&path));

        check_sqlite_error(err_events(store.list_events()));
        let _ignored = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn initialize_connection_reports_pragma_failure_on_readonly_connection() {
        // A read-only connection rejects the WAL journal_mode change, so the
        // batched pragma initialization fails — the single fallible call site
        // that replaced the three separate pragma updates.
        let dir = std::env::temp_dir().join(format!(
            "livespec-console-eventstore-readonly-pragma-{}",
            std::process::id()
        ));
        let _ignored = std::fs::remove_dir_all(&dir);
        ok_unit(std::fs::create_dir_all(&dir));
        let path = dir.join("db.sqlite");
        {
            let seed = ok_connection(Connection::open(&path));
            ok_sqlite_unit(seed.execute_batch("create table t(x);"));
        }
        let connection = ok_connection(Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        ));

        let error = err_eventstore_unit(super::initialize_connection(&connection));

        check_sqlite_error(error);
        let _ignored = std::fs::remove_dir_all(&dir);
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
    #[should_panic(expected = "check_event_store_error failed")]
    fn check_event_store_error_panics() {
        check_event_store_error(EventStoreError::UnknownEventType(
            "unknown.event".to_owned(),
        ));
    }

    #[test]
    #[should_panic(expected = "ok_unit failed")]
    fn ok_unit_panics() {
        ok_unit(Err(std::io::Error::other("boom")));
    }

    #[test]
    #[should_panic(expected = "ok_bytes failed")]
    fn ok_bytes_panics() {
        let _bytes = ok_bytes(Err(std::io::Error::other("boom")));
    }

    #[test]
    #[should_panic(expected = "ok_store failed")]
    fn ok_store_panics() {
        ok_store(Err(EventStoreError::InvalidSequence));
    }

    #[test]
    #[should_panic(expected = "ok_connection failed")]
    fn ok_connection_panics() {
        ok_connection(Err(rusqlite::Error::InvalidQuery));
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

    #[test]
    #[should_panic(expected = "err_store failed")]
    fn err_store_panics() {
        err_store(Ok(ok_store(SqliteEventStore::open_in_memory())));
    }

    #[test]
    #[should_panic(expected = "err_append_outcome failed")]
    fn err_append_outcome_panics() {
        err_append_outcome(Ok(super::AppendOutcome::new(1, AppendStatus::Inserted)));
    }

    #[test]
    #[should_panic(expected = "err_command_append_outcome failed")]
    fn err_command_append_outcome_panics() {
        err_command_append_outcome(Ok(super::CommandAppendOutcome::new(
            "cmd_1".to_owned(),
            CommandAppendStatus::Inserted,
        )));
    }

    #[test]
    #[should_panic(expected = "err_events failed")]
    fn err_events_panics() {
        err_events(Ok(Vec::new()));
    }

    #[test]
    #[should_panic(expected = "err_commands failed")]
    fn err_commands_panics() {
        err_commands(Ok(Vec::new()));
    }

    #[test]
    #[should_panic(expected = "err_claimed failed")]
    fn err_claimed_panics() {
        err_claimed(Ok(false));
    }

    #[test]
    #[should_panic(expected = "err_recovered_count failed")]
    fn err_recovered_count_panics() {
        err_recovered_count(Ok(0));
    }

    #[test]
    #[should_panic(expected = "err_checkpoint failed")]
    fn err_checkpoint_panics() {
        err_checkpoint(Ok(None));
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
    fn check_event_store_error(error: EventStoreError) {
        match error {
            EventStoreError::InvalidSequence | EventStoreError::Sqlite(_) => {}
            other => panic!("check_event_store_error failed: {other:?}"),
        }
    }

    #[track_caller]
    fn ok_unit(result: Result<(), std::io::Error>) {
        match result {
            Ok(()) => {}
            Err(error) => panic!("ok_unit failed: {error:?}"),
        }
    }

    #[track_caller]
    fn ok_bytes(result: Result<Vec<u8>, std::io::Error>) -> Vec<u8> {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_bytes failed: {error:?}"),
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
    fn ok_connection(result: Result<Connection, rusqlite::Error>) -> Connection {
        match result {
            Ok(value) => value,
            Err(error) => panic!("ok_connection failed: {error:?}"),
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
    fn err_store(result: EventStoreResult<SqliteEventStore>) -> EventStoreError {
        match result {
            Ok(_value) => panic!("err_store failed"),
            Err(error) => error,
        }
    }

    #[track_caller]
    fn err_append_outcome(result: EventStoreResult<super::AppendOutcome>) -> EventStoreError {
        match result {
            Ok(_value) => panic!("err_append_outcome failed"),
            Err(error) => error,
        }
    }

    #[track_caller]
    fn err_command_append_outcome(
        result: EventStoreResult<super::CommandAppendOutcome>,
    ) -> EventStoreError {
        match result {
            Ok(_value) => panic!("err_command_append_outcome failed"),
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
    fn err_events(result: EventStoreResult<Vec<super::StoredEvent>>) -> EventStoreError {
        match result {
            Ok(_value) => panic!("err_events failed"),
            Err(error) => error,
        }
    }

    #[track_caller]
    fn err_commands(result: EventStoreResult<Vec<StoredCommand>>) -> EventStoreError {
        match result {
            Ok(_value) => panic!("err_commands failed"),
            Err(error) => error,
        }
    }

    #[track_caller]
    fn err_claimed(result: EventStoreResult<bool>) -> EventStoreError {
        match result {
            Ok(_value) => panic!("err_claimed failed"),
            Err(error) => error,
        }
    }

    #[track_caller]
    fn err_recovered_count(result: EventStoreResult<usize>) -> EventStoreError {
        match result {
            Ok(_value) => panic!("err_recovered_count failed"),
            Err(error) => error,
        }
    }

    #[track_caller]
    fn err_checkpoint(result: EventStoreResult<Option<String>>) -> EventStoreError {
        match result {
            Ok(_value) => panic!("err_checkpoint failed"),
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

    fn replace_events_for_list_events(store: &SqliteEventStore, bad_column: &str) {
        let global_seq = if bad_column == "global_seq" {
            "-1"
        } else {
            "1"
        };
        let event_id = sql_text_or_blob("evt_1", bad_column == "event_id");
        let event_type = sql_text_or_blob("fabro.human_gate_observed", bad_column == "type");
        let source = sql_text_or_blob("fabro", bad_column == "source");
        let source_event_id = if bad_column == "source_event_id" {
            "x'01'".to_owned()
        } else {
            "'source-1'".to_owned()
        };
        ok_sqlite_unit(store.connection.execute_batch(&format!(
            "drop table events;
             create table events (
               global_seq integer,
               event_id,
               type,
               source,
               source_event_id
             );
             insert into events values (
               {global_seq}, {event_id}, {event_type}, {source}, {source_event_id}
             );"
        )));
    }

    fn replace_events_for_list_console_events(store: &SqliteEventStore, bad_column: &str) {
        let event_id = sql_text_or_blob("evt_1", bad_column == "event_id");
        let schema_version = if bad_column == "schema_version" {
            "'bad'".to_owned()
        } else {
            "1".to_owned()
        };
        let context = sql_text_or_blob("factory", bad_column == "context");
        let source = sql_text_or_blob("fabro", bad_column == "source");
        let stream_id = sql_text_or_blob("stream-1", bad_column == "stream_id");
        let stream_seq = if bad_column == "stream_seq" {
            "-1".to_owned()
        } else {
            "1".to_owned()
        };
        let payload_json = sql_text_or_blob("{}", bad_column == "payload_json");
        ok_sqlite_unit(store.connection.execute_batch(&format!(
            "drop table events;
             create table events (
               global_seq integer,
               event_id,
               schema_version,
               context,
               type,
               source,
               stream_id,
               stream_seq,
               payload_json
             );
             insert into events values (
               1, {event_id}, {schema_version}, {context},
               'fabro.human_gate_observed', {source}, {stream_id}, {stream_seq}, {payload_json}
             );"
        )));
    }

    fn replace_commands_for_list_commands(store: &SqliteEventStore, bad_column: &str) {
        let value = |column: &str, text: &str| sql_text_or_blob(text, bad_column == column);
        ok_sqlite_unit(store.connection.execute_batch(&format!(
            "drop table commands;
             create table commands (
               command_id,
               context,
               type,
               aggregate_id,
               idempotency_key,
               requested_by,
               status,
               requested_at,
               updated_at,
               payload_json,
               error_json
             );
             insert into commands values (
               {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}
             );",
            value("command_id", "cmd_1"),
            value("context", "factory"),
            value("type", "factory.drain_requested"),
            value("aggregate_id", "evt_gate"),
            value("idempotency_key", "idem_1"),
            value("requested_by", "operator"),
            value("status", "pending"),
            value("requested_at", "2026-06-23T00:00:02Z"),
            value("updated_at", "2026-06-23T00:00:02Z"),
            value("payload_json", "{}"),
            if bad_column == "error_json" {
                "x'01'".to_owned()
            } else {
                "null".to_owned()
            }
        )));
    }

    fn sql_text_or_blob(value: &str, blob: bool) -> String {
        if blob {
            "x'01'".to_owned()
        } else {
            format!("'{}'", value.replace('\'', "''"))
        }
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
