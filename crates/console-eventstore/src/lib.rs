#![forbid(unsafe_code)]

use std::num::TryFromIntError;
use std::path::Path;

use console_domain::ConsoleEvent;
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

fn sequence_from_rowid(value: i64) -> EventStoreResult<u64> {
    Ok(u64::try_from(value)?)
}

#[cfg(test)]
mod tests {
    use super::{
        AppendStatus, EventAppend, EventStoreError, SqliteEventStore, sequence_from_rowid,
    };
    use console_domain::{ConsoleEvent, EventType};

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
}
