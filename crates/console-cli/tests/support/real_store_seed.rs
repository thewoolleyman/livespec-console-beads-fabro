//! Replay a JSON export of REAL captured event history into a fresh
//! `SqliteEventStore`, used to seed the `real_store_smoke` gate's fixture
//! (`livespec-console-beads-fabro-mx9u.15`).
//!
//! # Why a replay, not a copied binary `.sqlite` file
//!
//! The fixture under `tests/fixtures/real_store_smoke/` is a literal export of
//! nine rows from the maintainer's own live store
//! (`tmp/livespec-console.sqlite`, captured 2026-09-08) — every `event_id`,
//! `source_event_id`, `occurred_at`/`observed_at` timestamp, and `payload_json`
//! byte is copied verbatim from that real ledger, not authored. A JSON export
//! is reviewable in a diff the way a binary `.sqlite` blob is not, and this
//! module is the thin, deterministic replay that turns it back into a real
//! event-store file at test time via the SAME production `append_event` API
//! the console itself uses — so the seeded store's rows are indistinguishable
//! from ones the console wrote directly.
//!
//! # Why THESE nine rows
//!
//! They are the full real history of one attention occurrence stream
//! (`attention_item:livespec-console-beads-fabro:hygiene:idle-factory:livespec-console-beads-fabro`)
//! up through the exact moment that reproduces the crash named in
//! `livespec-console-beads-fabro-mx9u.15`'s description
//! (`AttentionResolveDuplicate("...hygiene:idle-factory...:resolved:3124974868900000571")`):
//!
//! - Row 7 (`changed`) and row 9 (`appeared`) carry BYTE-IDENTICAL
//!   `payload_json` — the finding disappeared and reappeared with the exact
//!   same content (a real, unremarkable coincidence in the live ledger: the
//!   dispatcher's admission-eligible count happened to repeat).
//! - Row 8 (`resolved`) is the REAL resolution of row 7's occurrence, and
//!   already claims the identity `...resolved:3124974868900000571` — computed
//!   purely from that occurrence's content
//!   (`attention_item_resolved_event` in `console-application`).
//! - Row 9 is left OPEN in the real ledger (never resolved as of the export).
//!   Resolving it recomputes the IDENTICAL identity row 8 already claims,
//!   because the resolved event's identity folds in only the resolved item's
//!   own content — see `crates/console-application/src/source_adapters.rs`.
//!
//! So seeding exactly these nine rows and then resolving the still-open row-9
//! occurrence (see `real_store_smoke.rs`) reproduces the real duplicate-resolve
//! collision deterministically, from real bytes, without needing to get lucky
//! waiting for a live toggle.

use std::path::Path;

use console_domain::{ConsoleEvent, EventType};
use console_eventstore::{EventAppend, SqliteEventStore};

/// One fixture row, parsed from the exported JSON array.
pub struct FixtureEvent {
    event_id: String,
    context: String,
    aggregate_id: String,
    stream_id: String,
    event_type: EventType,
    schema_version: u16,
    occurred_at: String,
    observed_at: String,
    causation_id: Option<String>,
    correlation_id: String,
    source: String,
    source_event_id: Option<String>,
    payload_json: String,
    metadata_json: String,
}

impl FixtureEvent {
    /// The event's own `source_event_id`, the identity the store's
    /// `UNIQUE(source, source_event_id)` index dedupes on.
    #[must_use]
    pub fn source_event_id(&self) -> Option<&str> {
        self.source_event_id.as_deref()
    }

    /// The canonical persisted `payload_json` for this row.
    #[must_use]
    pub fn payload_json(&self) -> &str {
        &self.payload_json
    }

    /// The event-type contract this row carries.
    #[must_use]
    pub const fn event_type(&self) -> &EventType {
        &self.event_type
    }
}

/// Parse a required string field out of a JSON object, by key.
fn required_str(value: &serde_json::Value, key: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("fixture row missing required string field {key:?}: {value}"))
}

/// Parse an optional (possibly `null` or absent) string field by key.
fn optional_str(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

/// Load the fixture JSON array at `path` into ordered [`FixtureEvent`] rows.
///
/// # Errors
/// Returns a message when the file cannot be read, is not a JSON array of
/// objects, a row is missing a required field, or a row's `type` is not a
/// recognized event contract name.
pub fn load_fixture(path: &Path) -> Result<Vec<FixtureEvent>, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|error| format!("read fixture {} failed: {error}", path.display()))?;
    let rows: Vec<serde_json::Value> = serde_json::from_str(&raw)
        .map_err(|error| format!("fixture {} is not a JSON array: {error}", path.display()))?;
    rows.iter()
        .map(|row| {
            let type_name = required_str(row, "type")?;
            let event_type = EventType::from_contract_name(&type_name).ok_or_else(|| {
                format!("fixture row carries an unrecognized event type {type_name:?}")
            })?;
            let schema_version = row
                .get("schema_version")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| "fixture row missing schema_version".to_owned())?;
            Ok(FixtureEvent {
                event_id: required_str(row, "event_id")?,
                context: required_str(row, "context")?,
                aggregate_id: required_str(row, "aggregate_id")?,
                stream_id: required_str(row, "stream_id")?,
                event_type,
                schema_version: u16::try_from(schema_version)
                    .map_err(|error| format!("schema_version out of range: {error}"))?,
                occurred_at: required_str(row, "occurred_at")?,
                observed_at: required_str(row, "observed_at")?,
                causation_id: optional_str(row, "causation_id"),
                correlation_id: required_str(row, "correlation_id")?,
                source: required_str(row, "source")?,
                source_event_id: optional_str(row, "source_event_id"),
                payload_json: required_str(row, "payload_json")?,
                metadata_json: required_str(row, "metadata_json")?,
            })
        })
        .collect()
}

/// Replay `fixtures` into `store` IN ORDER via the production `append_event`
/// path, preserving every identity byte-for-byte.
///
/// The store assigns its own `stream_seq` from each stream's current length
/// (see `SqliteEventStore::append_event`), so replaying in the fixture's
/// original order reproduces the original sequence numbers exactly, as long as
/// every row shares one stream (true of this fixture).
///
/// # Errors
/// Returns a message if the underlying store append fails, or if a fixture row
/// unexpectedly collides with an earlier one in the same replay (the export is
/// real, already-deduplicated history, so a collision here means the fixture
/// itself is malformed, not a defect under test).
pub fn seed_store(store: &mut SqliteEventStore, fixtures: &[FixtureEvent]) -> Result<(), String> {
    for fixture in fixtures {
        let event = ConsoleEvent::new(
            fixture.event_id.clone(),
            fixture.schema_version,
            fixture.context.clone(),
            fixture.event_type,
            fixture.source.clone(),
            fixture.stream_id.clone(),
            0, // ignored by append_event; the store assigns its own stream_seq
        );
        let append = EventAppend::new(
            event,
            fixture.aggregate_id.clone(),
            fixture.occurred_at.clone(),
            fixture.observed_at.clone(),
            fixture.causation_id.clone(),
            fixture.correlation_id.clone(),
            fixture.source_event_id.clone(),
            fixture.payload_json.clone(),
            fixture.metadata_json.clone(),
        );
        let outcome = store.append_event(&append).map_err(|error| {
            format!(
                "seeding fixture row {:?} failed: {error:?}",
                fixture.event_id
            )
        })?;
        if outcome.status() != console_eventstore::AppendStatus::Inserted {
            return Err(format!(
                "fixture row {:?} collided while seeding a fresh store — the exported \
                 fixture is internally inconsistent",
                fixture.event_id
            ));
        }
    }
    Ok(())
}

/// Open (creating if absent) the store at `path` and [`seed_store`] it from
/// `fixtures`, then close it — the caller (typically a `tmux` launcher) opens
/// its own connection afterward.
///
/// # Errors
/// Returns a message if the store cannot be opened or seeding fails.
pub fn seed_store_file(path: &Path, fixtures: &[FixtureEvent]) -> Result<(), String> {
    let mut store = SqliteEventStore::open(path)
        .map_err(|error| format!("open seed store {} failed: {error:?}", path.display()))?;
    seed_store(&mut store, fixtures)
}
