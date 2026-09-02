//! Deterministic tests for the E2E harness's readiness / settle polling
//! (`support::poll_ready` and `support::poll_settled`).
//!
//! These reproduce the `l7unt3` blank-capture starvation WITHOUT a real `tmux`
//! pane or the console binary: a scripted capture source stands in for the pane,
//! so the flake-hardening logic is covered fast and — critically — the coverage
//! does not itself depend on a fast, unloaded host. The sources advance by CALL
//! COUNT, never wall-clock, so these tests are themselves immune to the host
//! slowness they are hardening the real harness against.

mod support;

use std::cell::Cell;
use std::time::Duration;

use support::{HarnessResult, poll_ready, poll_settled};

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
