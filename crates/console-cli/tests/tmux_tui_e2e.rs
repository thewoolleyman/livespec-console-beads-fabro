//! DELIVERABLE #0 — real-TUI end-to-end smoke, driven through a real `tmux` pane.
//!
//! This is the top tier of the console test pyramid: unlike the in-process
//! `scenario_*.rs` tests (which drive `run_store_backed_tui_session` with
//! scripted `TuiSessionRunner` fakes and never touch a terminal), this test
//! launches the SHIPPED binary in a pinned-size `tmux` pane, sends real
//! keystrokes, captures the rendered screen, and asserts on both the visible
//! content and the store side effects — the first automated coverage of the
//! `run_interactive_tui` raw-mode/render path, which every other test compiles
//! out via `#[cfg(all(not(test), not(coverage)))]`.
//!
//! # Tiering
//!
//! Marked `#[ignore]` so the default `cargo test` / `cargo nextest` matrix stays
//! green and free of a `tmux` dependency. The dedicated `just check-e2e-tmux`
//! target builds the RELEASE binary, points the harness at it, and runs this
//! test with `--ignored`, so it is a first-class always-run gate — just in its
//! own invocation (and its own CI job, which the CI image must provision `tmux`
//! for).
//!
//! # Two-repo parameterization
//!
//! The cross-cutting acceptance for this program is "verified against TWO
//! DIFFERENT REPOS". The harness is parameterized by [`support::RepoFixture`],
//! and this smoke drives the full launch/keypress/capture/quit cycle against two
//! distinct repo fixtures in one run, asserting each renders its own tenant.

mod support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use console_domain::EventType;
use console_eventstore::SqliteEventStore;
use support::{HarnessResult, RepoFixture, TmuxConsole};

/// Generous ceiling for a single render/keypress to settle. The render itself is
/// sub-second; the slack absorbs a cold binary and a busy CI host.
const RENDER_TIMEOUT: Duration = Duration::from_secs(20);

#[test]
#[ignore = "real-TUI tmux E2E; run via `just check-e2e-tmux` (needs tmux + release binary)"]
fn tmux_tui_e2e_smoke_two_repos() -> HarnessResult<()> {
    for repo in two_repo_fixtures() {
        drive_one_repo(&repo)?;
    }
    Ok(())
}

/// The whole real-TUI scene for one repo: launch, assert the shell renders with
/// this repo's tenant and the navigation menu, drive a keypress that changes the
/// view, quit cleanly, and confirm the isolated store took a write.
fn drive_one_repo(repo: &RepoFixture) -> HarnessResult<()> {
    let console = TmuxConsole::launch(repo)?;

    // The shell renders: header title, this repo's tenant, and the nav menu.
    // Wait for a SETTLED frame (two identical consecutive captures) so the
    // several substring assertions below never race a partially painted frame.
    let header_needle = format!("repo: {}", repo.tenant());
    let screen = console.wait_for_settled(&header_needle, RENDER_TIMEOUT)?;
    assert!(
        screen.contains("LiveSpec Console"),
        "header title missing for tenant {}:\n{screen}",
        repo.tenant()
    );
    assert!(
        screen.contains("mode: tui") && screen.contains("view: Attention"),
        "expected header status fields for tenant {}:\n{screen}",
        repo.tenant()
    );
    for label in ["Attention", "Spec", "Lanes", "Events", "Repos", "Settings"] {
        assert!(
            screen.contains(label),
            "navigation label {label:?} missing for tenant {}:\n{screen}",
            repo.tenant()
        );
    }

    // A real keypress changes what renders: move the nav selection down to
    // `Lanes` (Attention -> Spec -> Lanes) and switch the view to it.
    console.send_keys(&["Down"])?;
    console.send_keys(&["Down"])?;
    console.send_keys(&["Enter"])?;
    let lanes = console.wait_for_settled("view: Lanes", RENDER_TIMEOUT)?;
    assert!(
        lanes.contains(&header_needle),
        "tenant must persist after navigation for {}:\n{lanes}",
        repo.tenant()
    );
    for lane in ["backlog", "ready", "active", "done"] {
        assert!(
            lanes.contains(lane),
            "lane column {lane:?} missing after switching to Lanes for tenant {}:\n{lanes}",
            repo.tenant()
        );
    }

    // Quit cleanly; the launcher prints the exit code once the TUI tears down.
    console.send_keys(&["q"])?;
    console.wait_for("TUI_EXIT=0", RENDER_TIMEOUT)?;

    // Side effect + hermeticity: the session wrote real events into its isolated
    // store, and NONE of them is a live GitHub observation.
    assert_store_side_effects(console.store_path())?;
    Ok(())
}

/// Assert the run's store side effects, reusing the production event-store reader
/// rather than a raw `SQLite` probe:
///
/// 1. At least one console event was persisted — proving the launch's
///    source-ingest pass ran against a writable isolated store. (This is NOT
///    evidence that a keypress produced a write; the keypress evidence is the
///    render change asserted above, e.g. `view: Attention` -> `view: Lanes`.)
/// 2. HERMETICITY: no `pr.snapshot_observed` from the github source. That event
///    only lands when the hardcoded `gh pr list` reaches the real authenticated
///    GitHub API; the harness's `gh` stub (front of PATH) must degrade the
///    github source to a not-observed finding instead, so a leak here means the
///    stub / PATH shadow stopped taking effect.
fn assert_store_side_effects(store_path: &Path) -> HarnessResult<()> {
    assert!(
        store_path.is_file(),
        "isolated store was never created at {}",
        store_path.display()
    );
    let store = SqliteEventStore::open(store_path)
        .map_err(|error| format!("open isolated store failed: {error:?}"))?;
    let events = store
        .list_console_events()
        .map_err(|error| format!("read isolated store events failed: {error:?}"))?;
    assert!(
        !events.is_empty(),
        "expected the session to persist at least one event into {}",
        store_path.display()
    );
    let github_leak = events.iter().any(|event| {
        event.source() == "github"
            && event.event_type() == &EventType::GithubPullRequestSnapshotObserved
    });
    assert!(
        !github_leak,
        "hermeticity leak: a live github pr.snapshot_observed event landed in {} — \
         the gh stub / PATH shadow is not taking effect",
        store_path.display()
    );
    Ok(())
}

/// Two distinct repo fixtures — different tenant labels AND different checkout
/// paths — so the smoke proves the same harness drives two different repos.
/// Paths resolve from this crate's manifest dir, so the test is host-independent
/// (works from a CI checkout as well as a local clone).
fn two_repo_fixtures() -> Vec<RepoFixture> {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = crate_dir
        .ancestors()
        .nth(2)
        .map_or_else(|| crate_dir.clone(), Path::to_path_buf);
    vec![
        RepoFixture::new("e2e-alpha", &crate_dir),
        RepoFixture::new("e2e-beta", &workspace_root),
    ]
}
