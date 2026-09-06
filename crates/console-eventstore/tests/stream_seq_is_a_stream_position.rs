//! `events.stream_seq` is a PER-STREAM POSITION assigned by the STORE, not a
//! per-command ordinal handed in by whoever built the envelope.
//!
//! # The defect this pins (livespec-console-beads-fabro-1d5f)
//!
//! Measured 2026-08-20 against the live event store, on stream
//! `livespec-console-beads-fabro-erb2ud`:
//!
//! ```text
//! 2887 command.accepted                stream_seq 1   <- set-acceptance command
//! 2888 work_item.action.started        stream_seq 2
//! 2889 work_item.action.completed      stream_seq 3
//! 2892 factory.drain_requested         stream_seq 0   <- dispatch-item command
//! 2893 command.accepted                stream_seq 1      DUPLICATE
//! 2894 factory.dispatch_item.not_wired stream_seq 2      DUPLICATE
//! ```
//!
//! Every command handler passes a literal ordinal that restarts per COMMAND, so
//! one stream carried many events at the same position, store-wide (stream
//! `fleet:livespec` held `stream_seq 1` thirteen times). Any reader that treats
//! the field as a stream position — ordering, optimistic concurrency,
//! replay-from-position, gap detection — was reading a value that repeats.
//!
//! # Why this is an integration test rather than a `--lib` unit test
//!
//! The two legs below are the WHOLE-STORE contract seen from outside: leg 1
//! drives only the public append/read API, exactly as a command lane does, and
//! leg 2 asks whether a database written past that API is refused. Both would
//! be weaker asserted from inside the crate, where a test can reach the
//! connection and could accidentally pin the implementation instead of the
//! behaviour. The per-branch coverage of the assignment and the one-time
//! renumbering migration lives in the crate's `--lib` unit tests.

use std::path::PathBuf;

use console_domain::{ConsoleEvent, EventType};
use console_eventstore::{EventAppend, SqliteEventStore};
use rusqlite::Connection;

/// The work-item stream the measurement above was taken from.
const STREAM: &str = "livespec-console-beads-fabro-erb2ud";

#[test]
fn two_commands_on_one_stream_yield_a_strictly_increasing_sequence() -> Result<(), String> {
    let mut store =
        SqliteEventStore::open_in_memory().map_err(|error| format!("open failed: {error:?}"))?;

    // Command 1 — set-acceptance. Its handler numbers its own events 1, 2, 3.
    for append in command_event_set(
        "cmd_set_acceptance",
        &[
            (EventType::CommandAccepted, "accepted", 1),
            (EventType::WorkItemActionStarted, "started", 2),
            (EventType::WorkItemActionCompleted, "completed", 3),
        ],
    ) {
        store
            .append_event(&append)
            .map_err(|error| format!("append failed: {error:?}"))?;
    }

    // Command 2 — dispatch-item, four minutes later on the SAME stream. Its
    // handler restarts the ordinal at 0, so on the defective store these three
    // events land on positions the stream already holds.
    for append in command_event_set(
        "cmd_dispatch_item",
        &[
            (EventType::FactoryDrainRequested, "requested", 0),
            (EventType::CommandAccepted, "accepted", 1),
            (EventType::FactoryDispatchItemNotWired, "not_wired", 2),
        ],
    ) {
        store
            .append_event(&append)
            .map_err(|error| format!("append failed: {error:?}"))?;
    }

    let positions = stream_positions(&store)?;
    assert_eq!(
        positions,
        vec![1, 2, 3, 4, 5, 6],
        "the store must number a stream by its own length, so two commands' \
         event sets continue one sequence instead of restarting it"
    );
    for window in positions.windows(2) {
        assert!(
            window[0] < window[1],
            "stream positions must strictly increase across the whole stream, \
             got {positions:?}"
        );
    }
    Ok(())
}

#[test]
fn a_repeated_position_on_one_stream_is_refused_by_the_schema() -> Result<(), String> {
    let dir = scratch("stream-seq-unique")?;
    let path = dir.join("events.sqlite");
    {
        let mut store =
            SqliteEventStore::open(&path).map_err(|error| format!("open failed: {error:?}"))?;
        for append in command_event_set(
            "cmd_set_acceptance",
            &[
                (EventType::CommandAccepted, "accepted", 1),
                (EventType::WorkItemActionStarted, "started", 2),
            ],
        ) {
            store
                .append_event(&append)
                .map_err(|error| format!("append failed: {error:?}"))?;
        }
    }

    // Forge a row PAST the store's append path — a regression in a handler, a
    // hand-written repair, a future writer — that repeats a position the stream
    // already holds. The forged row carries no `source_event_id`, so the only
    // constraint that can refuse it is the one this item asks for.
    let raw = Connection::open(&path).map_err(|error| format!("raw open failed: {error}"))?;
    let forged = raw.execute(
        "insert into events (
           event_id, context, aggregate_id, stream_id, stream_seq, type,
           schema_version, occurred_at, observed_at, correlation_id, source,
           payload_json, metadata_json
         )
         select 'evt_forged', context, aggregate_id, stream_id, stream_seq, type,
                schema_version, occurred_at, observed_at, correlation_id, source,
                payload_json, metadata_json
         from events where stream_id = ?1 order by global_seq limit 1",
        [STREAM],
    );

    let outcome = format!("{forged:?}");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        forged.is_err(),
        "a second event at a position the stream already holds must FAIL CLOSED \
         on a unique index over (stream_id, stream_seq), not be stored silently; \
         the forged insert returned {outcome}"
    );
    Ok(())
}

/// One command's event set, shaped exactly as a handler emits it: a literal
/// per-COMMAND ordinal in `stream_seq`, all on one stream.
fn command_event_set(command_id: &str, events: &[(EventType, &str, u64)]) -> Vec<EventAppend> {
    events
        .iter()
        .map(|(event_type, suffix, ordinal)| {
            let event_id = format!("evt_{command_id}_{suffix}");
            EventAppend::new(
                ConsoleEvent::new(
                    event_id,
                    1,
                    "command".to_owned(),
                    *event_type,
                    "console:factory-command-handler".to_owned(),
                    STREAM.to_owned(),
                    *ordinal,
                ),
                STREAM.to_owned(),
                "2026-08-20T22:48:29.528869692Z".to_owned(),
                "2026-08-20T22:48:29.528869692Z".to_owned(),
                Some(command_id.to_owned()),
                format!("corr_{command_id}"),
                None,
                "{}".to_owned(),
                "{}".to_owned(),
            )
        })
        .collect()
}

/// Every stored position on [`STREAM`], in append order.
fn stream_positions(store: &SqliteEventStore) -> Result<Vec<u64>, String> {
    Ok(store
        .list_console_events()
        .map_err(|error| format!("list failed: {error:?}"))?
        .iter()
        .filter(|event| event.stream_id() == STREAM)
        .map(ConsoleEvent::stream_seq)
        .collect())
}

/// A private scratch directory for the file-backed leg.
fn scratch(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!(
        "livespec-console-eventstore-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    Ok(dir)
}
