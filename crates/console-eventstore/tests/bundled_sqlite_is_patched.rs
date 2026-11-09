//! The `SQLite` this store links is at or past 3.54.0, and the file-backed store
//! still runs in WAL mode and round-trips events.
//!
//! RUSTSEC-2026-0459 (CVE-2026-119375): `SQLite` before 3.54.0 can return
//! uninitialized page-cache memory when a database file is opened beside a
//! `-wal` sibling whose page size differs. The store opens WAL databases by
//! design, so the bundled library version is part of its contract, and
//! `just check-deps` refuses the push while an older one is locked.

use std::path::Path;

use console_domain::{ConsoleEvent, EventType};
use console_eventstore::{EventAppend, SqliteEventStore};
use rusqlite::Connection;

const MINIMUM_PATCHED: (u32, u32) = (3, 54);

#[test]
fn the_bundled_sqlite_is_at_least_3_54_0() -> Result<(), String> {
    let connection = Connection::open_in_memory().map_err(|error| format!("open: {error}"))?;
    let version = connection
        .query_row("select sqlite_version()", [], |row| row.get::<_, String>(0))
        .map_err(|error| format!("version query: {error}"))?;
    let mut parts = version.split('.').map(str::parse::<u32>);
    let (Some(Ok(major)), Some(Ok(minor))) = (parts.next(), parts.next()) else {
        return Err(format!("unparseable sqlite_version() {version:?}"));
    };
    assert!(
        (major, minor) >= MINIMUM_PATCHED,
        "sqlite_version() is {version}, below the 3.54.0 that RUSTSEC-2026-0459 requires"
    );
    Ok(())
}

#[test]
fn the_file_backed_store_opens_in_wal_mode_and_reads_back_its_events() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!(
        "livespec-console-eventstore-patched-sqlite-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|error| format!("mkdir: {error}"))?;

    let result = round_trip(&dir.join("events.sqlite"));
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn round_trip(path: &Path) -> Result<(), String> {
    let mut store =
        SqliteEventStore::open(path).map_err(|error| format!("open failed: {error:?}"))?;
    for (suffix, event_type) in [
        ("one", EventType::CommandAccepted),
        ("two", EventType::WorkItemActionStarted),
    ] {
        let append = EventAppend::new(
            ConsoleEvent::new(
                format!("evt_patched_{suffix}"),
                1,
                "command".to_owned(),
                event_type,
                "console:test".to_owned(),
                "stream-patched".to_owned(),
                0,
            ),
            "stream-patched".to_owned(),
            "2026-11-09T20:00:00Z".to_owned(),
            "2026-11-09T20:00:00Z".to_owned(),
            None,
            "corr_patched".to_owned(),
            None,
            "{}".to_owned(),
            "{}".to_owned(),
        );
        store
            .append_event(&append)
            .map_err(|error| format!("append failed: {error:?}"))?;
    }

    let positions: Vec<u64> = store
        .list_console_events()
        .map_err(|error| format!("list failed: {error:?}"))?
        .iter()
        .map(ConsoleEvent::stream_seq)
        .collect();
    assert_eq!(positions, vec![1, 2]);

    let raw = Connection::open(path).map_err(|error| format!("raw open failed: {error}"))?;
    let journal_mode = raw
        .query_row("pragma journal_mode", [], |row| row.get::<_, String>(0))
        .map_err(|error| format!("journal_mode query: {error}"))?;
    assert_eq!(journal_mode, "wal");
    Ok(())
}
