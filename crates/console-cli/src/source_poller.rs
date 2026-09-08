//! Cadence pacing for the off-thread source poller.
//!
//! The binary's poller thread runs the SLOW CLI-shelling source polls
//! ([`crate::refresh_sources`]) off the UI thread — six backing CLIs plus the
//! orchestrator's `needs_attention` snapshot per sweep. What it lacked was a
//! FLOOR on how often it may re-enter them. Its wait was a plain channel
//! `recv_timeout`, so every on-demand re-poll request that a ledger-mutating
//! keystroke queued short-circuited the cadence and started a fresh sweep at
//! once: N queued requests drained as N back-to-back sweeps with no gap
//! between them. Dogfooded 2026-09-08 as a console pegged at ~53% CPU
//! respawning `needs_attention.py` continuously
//! (livespec-console-beads-fabro-pzbdbo.25).
//!
//! This module holds that pacing decision — and only that decision — behind a
//! [`SourcePollHost`] seam, so it is testable: the thread, the channel, the
//! clock, and the store all stay in the binary's composition root, which is
//! `#[cfg]`-excluded from every test and coverage build. Three properties come
//! out of the loop:
//!
//! * **At most one invocation is in flight.** The loop is sequential — it never
//!   starts a poll while one is running, so no source is ever shelled twice
//!   concurrently.
//! * **A request never bypasses the cadence.** Requests arriving inside the
//!   window COALESCE into the single poll that runs once the window closes, so
//!   a burst of operator actions costs one sweep rather than one sweep each.
//! * **A stop is observed BETWEEN polls, always.** The host is asked for a wake
//!   on every turn, including when the window has already closed (a zero
//!   timeout, i.e. a non-blocking check), so a quitting session is never made
//!   to wait out a cadence.
//!
//! The window is measured from the END of the previous poll, which is the
//! stricter of the two readings of "one poll per cadence": it also guarantees
//! an idle gap after a sweep that itself ran longer than the cadence, which is
//! the shape that had the CPU pegged.

use std::time::{Duration, Instant};

/// What woke the poller from its cadence wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourcePollWake {
    /// An on-demand re-poll request arrived (the operator mutated the ledger).
    Requested,
    /// The wait elapsed with no request.
    Elapsed,
    /// The session asked the poller to stop.
    Stopped,
}

/// The host a paced poll loop drives: it runs the poll, waits for the next
/// wake, and reads the clock.
///
/// Every side effect the poller has lives behind this trait, so
/// [`run_paced_source_poll_loop`] stays pure control flow.
pub trait SourcePollHost {
    /// Run ONE source poll to completion.
    ///
    /// Called only when the cadence window has closed, and never re-entered
    /// while a previous call is still running — that sequencing is what bounds
    /// the backing CLI to one in-flight invocation per source.
    fn poll_sources(&mut self);

    /// Wait up to `timeout` for the next wake.
    ///
    /// A ZERO `timeout` is a non-blocking check for a pending stop or request,
    /// not a spin: the loop uses it to observe a shutdown before starting the
    /// poll whose window has already opened.
    fn wait(&mut self, timeout: Duration) -> SourcePollWake;

    /// Read the clock. Injected rather than taken from [`Instant::now`]
    /// directly so the pacing is measurable without sleeping through it.
    fn now(&self) -> Instant;
}

/// Drive `host`'s source polls, one per `cadence`, until it reports
/// [`SourcePollWake::Stopped`].
///
/// The clock starts as though a poll had just finished, because one has: the
/// launch path runs a synchronous [`crate::ingest_and_reflect`] before the
/// session's UI loop begins, and the poller thread is spawned alongside it. A
/// first poll fired immediately would simply double that startup sweep — two
/// concurrent invocations of every backing CLI, which is precisely what this
/// loop exists to prevent.
pub fn run_paced_source_poll_loop(host: &mut dyn SourcePollHost, cadence: Duration) {
    let mut last_finished = host.now();
    loop {
        let remaining = wait_before_next_poll(cadence, last_finished, host.now());
        if host.wait(remaining) == SourcePollWake::Stopped {
            return;
        }
        if !remaining.is_zero() {
            // The window is still open. Whatever woke us — a queued re-poll
            // request, or a wait cut short — is COALESCED into the one poll
            // that runs when the remaining time reaches zero.
            continue;
        }
        host.poll_sources();
        last_finished = host.now();
    }
}

/// How much of the cadence window is left, given when the previous poll
/// finished. Zero once the window has closed, including when `now` predates
/// `last_finished`, which a non-monotonic reading could otherwise turn into a
/// wait of nearly forever.
fn wait_before_next_poll(cadence: Duration, last_finished: Instant, now: Instant) -> Duration {
    cadence.saturating_sub(now.saturating_duration_since(last_finished))
}

#[cfg(test)]
mod tests {
    use super::{
        SourcePollHost, SourcePollWake, run_paced_source_poll_loop, wait_before_next_poll,
    };
    use std::cell::Cell;
    use std::time::{Duration, Instant};

    const CADENCE: Duration = Duration::from_secs(2);

    /// A host over a SCRIPTED clock, so a whole cadence is exercised without
    /// sleeping through it.
    ///
    /// Each `now` reading consumes the next scripted offset, and the loop reads
    /// the clock at fixed points: once before the first turn, once per turn to
    /// size the wait, and once more after each poll. The scripts below are
    /// written against that sequence. Both scripts repeat their last entry once
    /// exhausted, so a test only spells out the readings it cares about.
    struct ScriptedHost {
        origin: Instant,
        /// Offsets from `origin`, in milliseconds, handed to successive `now`
        /// calls.
        clock_offsets_millis: Vec<u64>,
        /// `now` takes `&self` — a clock reading is not a mutation — so the
        /// script position is carried in a `Cell`.
        clock_reads: Cell<usize>,
        /// Wakes handed to successive `wait` calls.
        wakes: Vec<SourcePollWake>,
        wait_calls: usize,
        /// Every timeout `wait` was asked for, in call order.
        observed_timeouts: Vec<Duration>,
        polls: usize,
    }

    impl ScriptedHost {
        fn new(clock_offsets_millis: &[u64], wakes: &[SourcePollWake]) -> Self {
            Self {
                origin: Instant::now(),
                clock_offsets_millis: clock_offsets_millis.to_vec(),
                clock_reads: Cell::new(0),
                wakes: wakes.to_vec(),
                wait_calls: 0,
                observed_timeouts: Vec::new(),
                polls: 0,
            }
        }

        fn run(mut self, cadence: Duration) -> Self {
            run_paced_source_poll_loop(&mut self, cadence);
            self
        }

        fn script_step<T: Copy>(script: &[T], index: usize, fallback: T) -> T {
            script
                .get(index)
                .or_else(|| script.last())
                .copied()
                .unwrap_or(fallback)
        }
    }

    impl SourcePollHost for ScriptedHost {
        fn poll_sources(&mut self) {
            self.polls += 1;
        }

        fn wait(&mut self, timeout: Duration) -> SourcePollWake {
            self.observed_timeouts.push(timeout);
            let wake = Self::script_step(&self.wakes, self.wait_calls, SourcePollWake::Stopped);
            self.wait_calls += 1;
            wake
        }

        fn now(&self) -> Instant {
            let index = self.clock_reads.get();
            self.clock_reads.set(index + 1);
            let offset = Self::script_step(&self.clock_offsets_millis, index, 0);
            self.origin + Duration::from_millis(offset)
        }
    }

    #[test]
    fn the_first_poll_waits_a_full_cadence_because_launch_already_polled_once() {
        // The clock has not moved since the loop started, so the whole cadence
        // is still outstanding and no poll is due.
        let finished = ScriptedHost::new(&[0], &[SourcePollWake::Stopped]).run(CADENCE);

        assert_eq!(finished.polls, 0);
        assert_eq!(finished.observed_timeouts, vec![CADENCE]);
    }

    #[test]
    fn a_queued_request_inside_the_window_coalesces_instead_of_polling() {
        // Turn 1: 500 ms in, a request arrives -> 1.5 s still to wait, no poll.
        // Turn 2: 1 s in, another request -> 1 s still to wait, still no poll.
        // Turn 3: the window has closed, and the wake is a stop.
        let finished = ScriptedHost::new(
            &[0, 500, 1_000, 2_000],
            &[
                SourcePollWake::Requested,
                SourcePollWake::Requested,
                SourcePollWake::Stopped,
            ],
        )
        .run(CADENCE);

        // No poll at all: a queued request may not start one early.
        assert_eq!(finished.polls, 0);
        // Each coalesced request SHORTENS the outstanding wait; none ends it.
        let expected_timeouts = vec![
            Duration::from_millis(1_500),
            Duration::from_millis(1_000),
            Duration::ZERO,
        ];
        assert_eq!(finished.observed_timeouts, expected_timeouts);
    }

    #[test]
    fn a_poll_runs_once_the_window_closes_and_reopens_it() {
        // Turn 1: the window has closed (2 s in) -> poll, whose end (3 s in)
        // reopens the window. Turn 2: still 3 s in, so a full cadence is
        // outstanding again; the wake is a stop.
        let finished = ScriptedHost::new(
            &[0, 2_000, 3_000, 3_000],
            &[SourcePollWake::Elapsed, SourcePollWake::Stopped],
        )
        .run(CADENCE);

        assert_eq!(finished.polls, 1);
        // The closed window is checked non-blockingly, then re-armed in full.
        assert_eq!(finished.observed_timeouts, vec![Duration::ZERO, CADENCE]);
    }

    #[test]
    fn a_stop_is_observed_before_a_due_poll_is_started() {
        // The window is closed, so a poll is due — and the non-blocking check
        // still runs first, which is what lets a quitting session leave without
        // waiting out another sweep.
        let finished = ScriptedHost::new(&[0, 5_000], &[SourcePollWake::Stopped]).run(CADENCE);

        assert_eq!(finished.polls, 0);
        assert_eq!(finished.observed_timeouts, vec![Duration::ZERO]);
    }

    #[test]
    fn the_remaining_window_saturates_at_both_ends() {
        let origin = Instant::now();

        let part_way = wait_before_next_poll(CADENCE, origin, origin + Duration::from_millis(500));
        assert_eq!(part_way, Duration::from_millis(1_500));

        // An overrun window is CLOSED, never negative.
        let overrun = wait_before_next_poll(CADENCE, origin, origin + Duration::from_secs(9));
        assert_eq!(overrun, Duration::ZERO);

        // And a backwards reading waits the cadence, never nearly forever.
        let backwards = wait_before_next_poll(CADENCE, origin + Duration::from_secs(9), origin);
        assert_eq!(backwards, CADENCE);
    }
}
