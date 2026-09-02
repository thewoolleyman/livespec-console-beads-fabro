//! The store's `SQLite` busy timeout is INJECTABLE, and the injected value —
//! not the product default — is what decides a CONTENDED open.
//!
//! # Why this is an integration test rather than a `--lib` unit test
//!
//! The behaviour only exists against a real database file with a real PEER
//! holding the write lock. An in-memory store has no peer, and a unit test
//! cannot conjure one, so this scene opens a second, raw `rusqlite` connection
//! to the same file, has it take the write lock, and measures what the store
//! does about it.
//!
//! # Why the timeout is INJECTED rather than set in the environment
//!
//! `console_eventstore::BUSY_TIMEOUT_ENV` is the operator-facing knob, but Rust
//! 2024 makes `std::env::set_var` `unsafe` and this workspace forbids unsafe
//! code, so no test may set it. The env value is therefore parsed by a pure
//! function (unit-tested against the `--lib` coverage gate) and handed to
//! `SqliteEventStore::open_with_busy_timeout`, which is what this scene drives.
//!
//! # What makes each leg DISCRIMINATE
//!
//! Neither leg turns on how fast the host is. The peer holds the write lock for
//! a bounded interval measured from the moment the leg's open begins, and both
//! budgets are compared against that same interval — so an injected budget that
//! was ignored in favour of the product default fails leg 2 on ANY host, and an
//! injected budget that took effect passes on any host.
//!
//! # Why it matters (livespec-console-beads-fabro-pis7qu)
//!
//! Measured 2026-09-02 on the shared self-hosted CI pool: the tmux E2E job died
//! with `tui error: EventStore(Sqlite(SqliteFailure(...` because a saturated
//! disk kept a peer's brief write past the five-second product default. The
//! default is right for a healthy host; a saturated one needs to be able to say
//! "wait longer" without a recompile, and the store must honour it.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use console_eventstore::SqliteEventStore;
use rusqlite::Connection;

/// A budget so long that a contended open under it can only end by the PEER
/// RELEASING, never by the timeout elapsing.
const PATIENT: Duration = Duration::from_secs(30);

/// A budget so short that a contended open under it can only end by the TIMEOUT
/// ELAPSING, never by the peer releasing (the peer is released explicitly, and
/// only after this leg has been asserted).
const IMPATIENT: Duration = Duration::from_millis(200);

/// How long the peer keeps the write lock once the patient leg has started.
///
/// Deliberately TWICE the product default, and timed from the instant that
/// leg's open begins. That is what makes the leg discriminate: an open running
/// on the injected [`PATIENT`] budget waits the peer out, while one that fell
/// back to `DEFAULT_BUSY_TIMEOUT` gives up at the halfway mark — a relationship
/// between two constants, not an assumption about how fast the host is.
const HOLD: Duration = console_eventstore::DEFAULT_BUSY_TIMEOUT.saturating_mul(2);

#[test]
fn the_injected_busy_timeout_decides_a_contended_open() -> Result<(), String> {
    let dir = scratch("busy-timeout-contention")?;

    // --- a peer takes the write lock and holds it until told to let go -------
    let store_path = dir.join("contended.sqlite");
    let (ready_tx, ready_rx) = mpsc::channel::<()>();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let peer_path = store_path.clone();
    let peer = thread::spawn(move || hold_write_lock(&peer_path, &ready_tx, &release_rx));
    ready_rx
        .recv()
        .map_err(|error| format!("the peer never took the write lock: {error}"))?;

    // --- leg 1: an IMPATIENT budget gives up, reporting transient contention --
    // Deliberately unbounded in wall-clock terms: the peer is still holding, so
    // this open can only return by exhausting the 200 ms it was handed.
    let Err(error) = SqliteEventStore::open_with_busy_timeout(&store_path, IMPATIENT) else {
        return Err("an impatient open must not win a lock the peer still holds".to_owned());
    };
    assert!(
        error.is_transient_contention(),
        "a budget exhausted against a peer's write lock is TRANSIENT CONTENTION, \
         not a fault: {error:?}"
    );

    // --- leg 2: a PATIENT budget waits the peer out and succeeds -------------
    // THE DISCRIMINATING LEG. The peer lets go after HOLD — twice the product
    // default — so an open that fell back to that default gives up at the
    // halfway mark and this leg fails, while one honouring the injected budget
    // waits and wins. The elapsed check pins that it WAITED rather than raced
    // in ahead of the peer.
    let started = Instant::now();
    let releaser = thread::spawn(move || {
        thread::sleep(HOLD);
        release_tx.send(())
    });
    let waited = SqliteEventStore::open_with_busy_timeout(&store_path, PATIENT)
        .map_err(|error| format!("a patient open must wait the peer out: {error:?}"))?;
    let elapsed = started.elapsed();
    assert!(
        elapsed >= HOLD,
        "the patient open must have BLOCKED on the peer, not raced in ahead of \
         it: returned after {elapsed:?}, peer held for {HOLD:?}"
    );
    drop(waited);

    join(releaser, "releaser")?.map_err(|error| format!("releasing the peer failed: {error}"))?;
    join(peer, "peer")?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// Take the database's write lock, announce it on `ready`, and keep it until
/// `release` fires. A real `CREATE TABLE` (not a bare `BEGIN EXCLUSIVE`) is what
/// makes the lock unambiguously held: the transaction has written, so every
/// other writer must wait or time out.
fn hold_write_lock(path: &Path, ready: &mpsc::Sender<()>, release: &mpsc::Receiver<()>) {
    let Ok(connection) = Connection::open(path) else {
        return;
    };
    // WAL first, so the store under test meets a database whose journal mode is
    // already settled and blocks on the SCHEMA write rather than on a journal
    // mode change — the same shape the live console meets.
    if connection
        .execute_batch("PRAGMA journal_mode=WAL; BEGIN EXCLUSIVE; CREATE TABLE peer_write(x);")
        .is_err()
    {
        return;
    }
    if ready.send(()).is_err() {
        return;
    }
    let _ = release.recv();
    let _ = connection.execute_batch("ROLLBACK;");
}

/// Join a helper thread, turning a panicked thread into a legible error instead
/// of an `unwrap` (which this workspace denies).
fn join<T>(handle: thread::JoinHandle<T>, label: &str) -> Result<T, String> {
    handle
        .join()
        .map_err(|_| format!("the {label} thread panicked"))
}

/// A per-process scratch directory for this scene's database files.
fn scratch(label: &str) -> Result<PathBuf, String> {
    let dir =
        std::env::temp_dir().join(format!("console-eventstore-{label}-{}", std::process::id()));
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("create scratch dir {} failed: {error}", dir.display()))?;
    Ok(dir)
}
