//! The off-thread source poller's cadence floor, asserted over a REAL fake
//! backing CLI that records its own spawn timestamps
//! (livespec-console-beads-fabro-pzbdbo.25).
//!
//! Dogfooding found the console sitting at ~53% CPU respawning the
//! orchestrator's `needs_attention.py` back to back. The poller's wait was a
//! plain channel `recv_timeout`, so every on-demand re-poll request a
//! ledger-mutating keystroke queued short-circuited that wait: N queued
//! requests drained as N full source sweeps with NO gap between them, each one
//! shelling six backing CLIs plus `needs_attention`.
//!
//! This is a top-of-pyramid assertion of the two properties that fixes it, and
//! it measures them where the operator felt them — in the PROCESSES the poller
//! actually spawned, timestamped by the spawned program itself rather than by
//! the caller:
//!
//! 1. At most one invocation is in flight per source: the log's `start` and
//!    `end` markers strictly alternate, so no invocation ever overlapped
//!    another.
//! 2. A new invocation never starts before the configured cadence has elapsed,
//!    however many re-poll requests are queued — including the FIRST one, which
//!    must not double the synchronous source poll the launch path has already
//!    run.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use livespec_console_beads_fabro::{SourcePollHost, SourcePollWake, run_paced_source_poll_loop};

/// Result alias mirroring the repo's other harnesses: a failure is a message,
/// never a panic-by-`unwrap`.
type TestResult<T> = Result<T, String>;

/// The cadence under test. Short enough to keep the suite fast, long enough
/// that the fake CLI's own runtime cannot be mistaken for the floor.
const CADENCE: Duration = Duration::from_millis(300);

/// How many re-poll requests are already queued when the loop starts — the
/// dogfooded shape, where several ledger-mutating keystrokes each queued one.
const QUEUED_REQUESTS: usize = 5;

/// How many polls to observe before stopping the loop.
const OBSERVED_POLLS: usize = 3;

/// A poll host that shells a real fake backing CLI and models the binary's
/// channel: a burst of already-queued requests, then a blocking wait.
struct FakeBackingCliHost {
    program: PathBuf,
    log: PathBuf,
    queued_requests: usize,
    polls: usize,
    stop_after_polls: usize,
}

impl SourcePollHost for FakeBackingCliHost {
    fn poll_sources(&mut self) {
        self.polls += 1;
        // A spawn failure leaves the log short, which the assertions name; the
        // status itself is deliberately not consulted (the CLI's own record of
        // the spawn is the evidence under test).
        let _status = Command::new(&self.program)
            .arg(&self.log)
            .stdout(std::process::Stdio::null())
            .status();
    }

    fn wait(&mut self, timeout: Duration) -> SourcePollWake {
        if self.polls >= self.stop_after_polls {
            return SourcePollWake::Stopped;
        }
        if self.queued_requests > 0 {
            self.queued_requests -= 1;
            return SourcePollWake::Requested;
        }
        std::thread::sleep(timeout);
        SourcePollWake::Elapsed
    }

    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// One line the fake backing CLI wrote: a marker and the nanosecond clock the
/// SPAWNED PROCESS read.
struct SpawnRecord {
    marker: String,
    at_nanos: u128,
}

#[test]
fn source_poller_never_respawns_the_backing_cli_before_the_cadence_elapses() -> TestResult<()> {
    let scratch = scratch_dir()?;
    let program = write_fake_backing_cli(&scratch)?;
    let log = scratch.join("spawns.log");

    let mut host = FakeBackingCliHost {
        program,
        log: log.clone(),
        queued_requests: QUEUED_REQUESTS,
        polls: 0,
        stop_after_polls: OBSERVED_POLLS,
    };
    let started = Instant::now();
    run_paced_source_poll_loop(&mut host, CADENCE);
    let ran_for = started.elapsed();

    let records = read_spawn_records(&log)?;
    let _cleanup = std::fs::remove_dir_all(&scratch);

    // Every queued request coalesced into the pacing rather than starting a
    // sweep of its own: exactly the polls the loop was allowed to run.
    let starts = markers(&records, "start");
    let ends = markers(&records, "end");
    assert_eq!(
        starts.len(),
        OBSERVED_POLLS,
        "expected exactly {OBSERVED_POLLS} spawns of the backing CLI, got {}",
        starts.len()
    );
    assert_eq!(ends.len(), starts.len(), "a spawn never recorded its end");

    // At most one invocation in flight: start/end strictly alternate.
    let alternating = records
        .iter()
        .enumerate()
        .all(|(index, record)| record.marker == if index % 2 == 0 { "start" } else { "end" });
    assert!(
        alternating,
        "backing-CLI invocations overlapped; markers were {:?}",
        records
            .iter()
            .map(|record| record.marker.as_str())
            .collect::<Vec<_>>()
    );

    // The FIRST background poll waits a full cadence: the launch path has
    // already run one synchronous source poll, and doubling it is precisely the
    // back-to-back spawn this item is about.
    assert!(
        ran_for >= CADENCE,
        "the first spawn came before the cadence elapsed (loop ran for {ran_for:?})"
    );

    // And no LATER poll starts before the cadence has elapsed either, however
    // many requests were queued.
    for pair in starts.windows(2) {
        let gap = Duration::from_nanos(u64::try_from(pair[1] - pair[0]).unwrap_or(u64::MAX));
        assert!(
            gap >= CADENCE,
            "backing CLI respawned after only {gap:?}, under the {CADENCE:?} cadence"
        );
    }
    Ok(())
}

/// The nanosecond stamps carried by every record with `marker`, in file order.
fn markers(records: &[SpawnRecord], marker: &str) -> Vec<u128> {
    records
        .iter()
        .filter(|record| record.marker == marker)
        .map(|record| record.at_nanos)
        .collect()
}

/// A per-process scratch directory for the fake CLI and its log.
fn scratch_dir() -> TestResult<PathBuf> {
    let scratch = std::env::temp_dir().join(format!("lc-poller-cadence-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)
        .map_err(|error| format!("create {} failed: {error}", scratch.display()))?;
    Ok(scratch)
}

/// Write the fake backing CLI: it timestamps its own start, does a little work,
/// timestamps its own end, and prints the empty JSON object a backing CLI is
/// expected to emit.
fn write_fake_backing_cli(scratch: &Path) -> TestResult<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    let program = scratch.join("fake-needs-attention.sh");
    let body = "#!/usr/bin/env bash\n\
                log=\"$1\"\n\
                printf 'start %s\\n' \"$(date +%s%N)\" >>\"$log\"\n\
                sleep 0.05\n\
                printf 'end %s\\n' \"$(date +%s%N)\" >>\"$log\"\n\
                printf '{}\\n'\n";
    std::fs::write(&program, body)
        .map_err(|error| format!("write {} failed: {error}", program.display()))?;
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
        .map_err(|error| format!("chmod {} failed: {error}", program.display()))?;
    Ok(program)
}

/// Parse the fake CLI's log into records, in the order the spawned processes
/// wrote them.
fn read_spawn_records(log: &Path) -> TestResult<Vec<SpawnRecord>> {
    let contents = std::fs::read_to_string(log)
        .map_err(|error| format!("read {} failed: {error}", log.display()))?;
    contents
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (marker, stamp) = line
                .split_once(' ')
                .ok_or_else(|| format!("unparseable spawn record {line:?}"))?;
            let at_nanos = stamp
                .trim()
                .parse::<u128>()
                .map_err(|error| format!("unparseable spawn timestamp {stamp:?}: {error}"))?;
            Ok(SpawnRecord {
                marker: marker.to_owned(),
                at_nanos,
            })
        })
        .collect()
}
