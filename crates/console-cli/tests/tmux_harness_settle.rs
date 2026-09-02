//! Deterministic tests for the E2E harness's readiness / settle polling
//! (`support::poll_ready` and `support::poll_settled`), its host-load budget
//! scaling, and its one-console-at-a-time slot.
//!
//! These reproduce the `l7unt3` blank-capture starvation WITHOUT a real `tmux`
//! pane or the console binary: a scripted capture source stands in for the pane,
//! so the flake-hardening logic is covered fast and — critically — the coverage
//! does not itself depend on a fast, unloaded host. The sources advance by CALL
//! COUNT, never wall-clock, so these tests are themselves immune to the host
//! slowness they are hardening the real harness against.
//!
//! The `pis7qu` additions keep that property. The load-scaling policy is a pure
//! function over `/proc` file CONTENTS, so a saturated host can be described in
//! a string literal rather than manufactured; and the console slot is asserted
//! through its own blocking behaviour, never through a sleep long enough to
//! "probably" have finished.

mod support;

use std::cell::Cell;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use support::{
    ConsoleSlot, HarnessResult, MAX_SCALE_PERMILLE, MAX_TIMEOUT, NO_SCALE_PERMILLE,
    io_pressure_hundredths, load_scale_permille, loadavg_hundredths, parse_hundredths, poll_ready,
    poll_settled, scaled_timeout,
};

/// A `/proc/pressure/io` body with the given `some avg10` percentage.
fn io_pressure(avg10: &str) -> String {
    format!("some avg10={avg10} avg60=1.00 avg300=0.50 total=123456\nfull avg10=0.00 total=1\n")
}

/// A capture source that yields `blank_calls` empty frames (a process starved at
/// startup, not yet painted) and then `frame` on every later call.
fn starved_source(
    blank_calls: usize,
    frame: &'static str,
) -> impl FnMut() -> HarnessResult<String> {
    let calls = Cell::new(0usize);
    move || {
        let n = calls.get();
        calls.set(n + 1);
        if n < blank_calls {
            Ok(String::new())
        } else {
            Ok(frame.to_owned())
        }
    }
}

#[test]
fn poll_ready_returns_the_first_non_blank_frame() -> HarnessResult<()> {
    // Two blank captures (slow start), then a painted frame: readiness resolves
    // on the first non-blank capture rather than treating blank as content.
    let source = starved_source(2, "view: Lanes\nrepo: alpha");
    let frame = poll_ready(source, Duration::from_secs(5), "")?;
    assert!(!frame.trim().is_empty());
    assert!(frame.contains("view: Lanes"));
    Ok(())
}

#[test]
fn poll_ready_times_out_with_a_starvation_message_on_permanent_blank() -> HarnessResult<()> {
    // A process that never paints must fail with a message that says WHY (blank
    // == starved, not wrong) and preserves the caller context — never a silent
    // or misleading "content missing".
    let source = || Ok(String::new());
    let Err(err) = poll_ready(
        source,
        Duration::from_millis(250),
        " in tmux session lc_e2e_probe",
    ) else {
        return Err("a never-painting process must time out".to_owned());
    };
    assert!(
        err.contains("first frame"),
        "names the readiness failure: {err}"
    );
    assert!(err.contains("starved"), "explains blank != wrong: {err}");
    assert!(err.contains("lc_e2e_probe"), "preserves context: {err}");
    Ok(())
}

#[test]
fn readiness_gate_then_short_settle_tolerates_a_slow_first_paint() -> HarnessResult<()> {
    // THE FIX. Gate on readiness first (absorbing a slow start), THEN run a SHORT
    // content-settle budget — which succeeds because painting is already done.
    // This mirrors `launch`, which now blocks on the first frame before handing
    // the handle back, so per-assertion budgets are measured against a live TUI.
    let mut source = starved_source(2, "view: Lanes");
    poll_ready(&mut source, Duration::from_secs(5), "")?;
    let frame = poll_settled(&mut source, "view: Lanes", Duration::from_millis(600), "")?;
    assert!(frame.contains("view: Lanes"));
    Ok(())
}

#[test]
fn settle_alone_starves_when_its_budget_is_shorter_than_startup() -> HarnessResult<()> {
    // THE BUG (l7unt3). With NO readiness gate, a settle budget measured from
    // launch that is shorter than the startup-blank window times out with a BLANK
    // last capture — the exact "starved, not wrong" signature seen in CI.
    let source = starved_source(5, "view: Lanes");
    let Err(err) = poll_settled(source, "view: Lanes", Duration::from_millis(250), "") else {
        return Err("a from-launch budget shorter than startup must time out".to_owned());
    };
    assert!(
        err.contains("last capture"),
        "the last capture is attached: {err}"
    );
    let after = err
        .split("---- last capture ----")
        .nth(1)
        .and_then(|tail| tail.split("---- end capture ----").next())
        .unwrap_or_default();
    assert!(
        after.trim().is_empty(),
        "the attached capture was blank (starved): {after:?}"
    );
    Ok(())
}

#[test]
fn poll_settled_requires_two_identical_frames_before_returning() -> HarnessResult<()> {
    // A partially-painted frame that CONTAINS the needle but changes between
    // captures must not settle until two consecutive captures are byte-identical,
    // so a multi-token assertion never reads a half-drawn screen.
    let frames = [
        "view: Lanes (loading)",
        "view: Lanes ready",
        "view: Lanes ready",
    ];
    let calls = Cell::new(0usize);
    let source = || {
        let i = calls.get();
        calls.set(i + 1);
        Ok(frames[i.min(frames.len() - 1)].to_owned())
    };
    let frame = poll_settled(source, "view: Lanes", Duration::from_secs(5), "")?;
    assert_eq!(frame, "view: Lanes ready");
    Ok(())
}

// --- pis7qu: host-load-scaled budgets ----------------------------------------

#[test]
fn fixed_point_host_load_figures_parse_to_hundredths() {
    // Both /proc signals report two-place decimals; parsing straight to an
    // integer is what keeps the whole scaling path free of float casts.
    assert_eq!(parse_hundredths("12.34"), Some(1_234));
    assert_eq!(parse_hundredths("12.3"), Some(1_230));
    assert_eq!(parse_hundredths("12"), Some(1_200));
    assert_eq!(parse_hundredths("  0.00  "), Some(0));
}

#[test]
fn an_unreadable_host_load_figure_is_absent_rather_than_zero() {
    // The distinction is load-bearing: `None` means "this signal is
    // unavailable" and leaves the budget to the other signal, whereas zero
    // would assert an IDLE host and could narrow a budget on a saturated one.
    for raw in ["", "n/a", "-1.00", "1.2x", "avg10="] {
        assert_eq!(parse_hundredths(raw), None, "{raw:?} must not parse");
    }
}

#[test]
fn the_io_pressure_signal_is_read_from_the_some_avg10_field() {
    // PSI's `some` line is the share of the last ten seconds in which at least
    // one task was stalled on I/O — the field that tracks the disk saturation
    // these tests kept dying on. The `full` line must not be mistaken for it.
    assert_eq!(io_pressure_hundredths(&io_pressure("37.50")), Some(3_750));
    assert_eq!(io_pressure_hundredths(&io_pressure("0.00")), Some(0));
    assert_eq!(io_pressure_hundredths("full avg10=99.00 total=1\n"), None);
    assert_eq!(io_pressure_hundredths(""), None);
}

#[test]
fn the_loadavg_signal_is_read_from_the_one_minute_field() {
    assert_eq!(loadavg_hundredths("3.75 2.10 1.90 1/234 5678\n"), Some(375));
    assert_eq!(loadavg_hundredths(""), None);
}

#[test]
fn an_idle_host_leaves_the_base_budgets_exactly_as_they_are() {
    // The tight budget is what makes a REAL regression fail fast, so scaling
    // must be inert on a healthy host rather than generously slack everywhere.
    let scale = load_scale_permille(Some(&io_pressure("0.00")), Some("0.10 0.10 0.10 1/1 2"), 8);

    assert_eq!(scale, NO_SCALE_PERMILLE);
}

#[test]
fn a_host_stalled_on_io_widens_the_budget_toward_the_cap() {
    // 100% stalled is the top of the PSI range and maps to the cap; half-stalled
    // lands halfway, because the budget being scaled is a WAIT and doubling the
    // stalled share roughly doubles how long that wait takes to clear.
    let saturated = load_scale_permille(Some(&io_pressure("100.00")), None, 8);
    let half = load_scale_permille(Some(&io_pressure("50.00")), None, 8);

    assert_eq!(saturated, MAX_SCALE_PERMILLE);
    assert_eq!(half, 3_500);
}

#[test]
fn an_oversubscribed_host_widens_the_budget_by_runnable_threads_per_core() {
    // Load average alone cannot say what the threads are waiting for, but it
    // still separates an idle host from an oversubscribed one — which is why it
    // is the fallback for a kernel built without PSI.
    assert_eq!(
        load_scale_permille(None, Some("12.00 4.0 4.0 1/9 2"), 4),
        3_000
    );
    assert_eq!(
        load_scale_permille(None, Some("4.00 4.0 4.0 1/9 2"), 4),
        NO_SCALE_PERMILLE
    );
    assert_eq!(
        load_scale_permille(None, Some("1.00 1.0 1.0 1/9 2"), 4),
        NO_SCALE_PERMILLE
    );
}

#[test]
fn the_larger_of_the_two_signals_wins() {
    // They measure different scarcities and a run can be starved by either, so
    // averaging them would let a quiet CPU mask a saturated disk.
    let quiet_cpu_busy_disk =
        load_scale_permille(Some(&io_pressure("100.00")), Some("0.10 0.1 0.1 1/9 2"), 8);
    let quiet_disk_busy_cpu =
        load_scale_permille(Some(&io_pressure("0.00")), Some("48.00 4.0 4.0 1/9 2"), 8);

    assert_eq!(quiet_cpu_busy_disk, MAX_SCALE_PERMILLE);
    assert_eq!(quiet_disk_busy_cpu, MAX_SCALE_PERMILLE);
}

#[test]
fn a_host_with_no_readable_load_signal_keeps_the_base_budgets() {
    // A non-Linux host, or one with neither file readable, must behave exactly
    // as the harness did before this change — never worse.
    assert_eq!(load_scale_permille(None, None, 8), NO_SCALE_PERMILLE);
    assert_eq!(
        load_scale_permille(Some("garbage"), Some("garbage"), 8),
        NO_SCALE_PERMILLE
    );
}

#[test]
fn a_scaled_budget_widens_but_stays_bounded() {
    let base = Duration::from_secs(45);

    assert_eq!(scaled_timeout(base, NO_SCALE_PERMILLE), base);
    assert_eq!(scaled_timeout(base, 2_000), Duration::from_secs(90));
    // The clamp is what keeps a stuck run from hanging the merge gate: past
    // MAX_TIMEOUT the run is not slow, it is wedged, and it must still fail
    // with a captured frame.
    assert_eq!(scaled_timeout(base, MAX_SCALE_PERMILLE), MAX_TIMEOUT);
    assert_eq!(
        scaled_timeout(Duration::from_secs(600), NO_SCALE_PERMILLE),
        MAX_TIMEOUT
    );
}

// --- pis7qu: one console at a time -------------------------------------------

#[test]
fn a_second_console_slot_waits_for_the_first_to_be_released() -> HarnessResult<()> {
    // THE FIX for four consoles per job. The slot is what makes "one console at
    // a time" a property of the harness rather than of an invocation flag, so
    // it holds under cargo test, nextest, and any --test-threads value.
    let held = ConsoleSlot::acquire()?;
    let (acquired_tx, acquired_rx) = mpsc::channel::<()>();
    let waiter = thread::spawn(move || {
        let slot = ConsoleSlot::acquire();
        let sent = acquired_tx.send(());
        drop(slot);
        sent
    });

    // Not "probably still waiting": the slot is genuinely occupied, so the
    // waiter CANNOT proceed and this leg is decided by the lock, not the clock.
    assert!(
        acquired_rx
            .recv_timeout(Duration::from_millis(250))
            .is_err(),
        "a second slot must not be handed out while the first is alive"
    );

    drop(held);
    acquired_rx
        .recv_timeout(Duration::from_secs(30))
        .map_err(|error| format!("releasing the slot must admit the waiter: {error}"))?;
    waiter
        .join()
        .map_err(|_| "the waiter thread panicked".to_owned())?
        .map_err(|error| format!("the waiter could not report back: {error}"))
}

#[test]
fn a_second_console_slot_on_the_same_thread_fails_instead_of_deadlocking() -> HarnessResult<()> {
    // A test that launched a second console before dropping its first would
    // otherwise block on itself and sit until the CI timeout with nothing
    // saying why. This turns that mistake into an immediate, self-describing
    // failure that names the remedy.
    let held = ConsoleSlot::acquire()?;

    let Err(error) = ConsoleSlot::acquire() else {
        return Err("a same-thread second slot must be refused, not granted".to_owned());
    };
    assert!(
        error.contains("ONE") && error.contains("dropped"),
        "the refusal must name the rule and the remedy: {error}"
    );

    // And the refusal does not corrupt the slot: releasing still admits a
    // later claim on the same thread.
    drop(held);
    let _reclaimed = ConsoleSlot::acquire()?;
    Ok(())
}
