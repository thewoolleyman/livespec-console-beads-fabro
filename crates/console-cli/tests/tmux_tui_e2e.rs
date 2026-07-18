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
use std::time::{Duration, Instant};

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
    // Assert the header's PRIORITY status fields -- the ones the header
    // deliberately preserves at the pinned 112-column width when several sources
    // are down. `mode: tui` / `fleet: livespec` are the low-value constant fields
    // the header intentionally sheds first at that width (see console-application
    // `fit_header_line` and its `header_line_fits_the_pinned_width_and_preserves_the_priority_fields`
    // unit test), so this real-tmux frame must NOT require them.
    assert!(
        screen.contains("view: Attention") && screen.contains("attention:"),
        "expected header priority fields for tenant {}:\n{screen}",
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

/// Scenario 18 / B4 — the navigable, pane-specific modal Help overlay, driven
/// live through a real `tmux` pane against the SHIPPED binary.
///
/// Covers every clause of Scenario 18 against two different panes (Attention,
/// Lanes, Settings): `?` opens auto-focused to the focused pane's section; the
/// modal is a bordered window with a 3-character border on each side and on top
/// and bottom, never wider than the viewport; a LEFT-side section menu beside a
/// RIGHT-side text pane whose content switches as the menu is navigated (and
/// which left/right never scroll); `esc to exit` is printed at the bottom at all
/// times; the modal closes ONLY on `Esc` (a non-Esc key keeps it open and the
/// underlying view neither switches nor scrolls); and Esc returns focus to the
/// pane the operator was on.
#[test]
#[ignore = "real-TUI tmux E2E; run via `just check-e2e-tmux` (needs tmux + release binary)"]
fn tmux_tui_e2e_modal_help_scenario_18() -> HarnessResult<()> {
    let repo = RepoFixture::new("e2e-help", &PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let console = TmuxConsole::launch(&repo)?;

    // The shell renders on the default needs-attention view.
    console.wait_for_settled("view: Attention", RENDER_TIMEOUT)?;

    // --- `?` opens auto-focused to the focused pane's section (Attention) ---
    console.send_keys(&["?"])?;
    let help = console.wait_for_settled("esc to exit", RENDER_TIMEOUT)?;
    assert!(
        help.contains("Global actions"),
        "menu must carry a Global actions section:\n{help}"
    );
    assert!(
        help.contains("> Attention"),
        "help opened from Attention must auto-focus the Attention section:\n{help}"
    );
    for label in ["Spec", "Lanes", "Events", "Repos", "Settings"] {
        assert!(
            help.contains(label),
            "menu section {label:?} missing:\n{help}"
        );
    }

    // --- bordered window: 3-character border on each side and top/bottom ---
    assert_help_border_geometry(&help);

    // --- modal, Esc-only close: a non-Esc key keeps it open and the underlying
    //     view neither switches nor scrolls (`?` no longer toggles it shut) ---
    console.send_keys(&["?"])?;
    let still_open = console.wait_for_settled("esc to exit", RENDER_TIMEOUT)?;
    assert!(
        still_open.contains("view: Attention"),
        "the underlying view must not switch while help is open:\n{still_open}"
    );
    console.send_keys(&["x"])?;
    let still_open = console.wait_for_settled("esc to exit", RENDER_TIMEOUT)?;
    assert!(
        still_open.contains("> Attention"),
        "a plain letter must not dismiss or disturb the modal:\n{still_open}"
    );

    // --- Esc closes and returns focus to the pane the operator was on ---
    console.send_keys(&["Escape"])?;
    let closed = wait_until_absent(&console, "esc to exit", RENDER_TIMEOUT)?;
    assert!(
        closed.contains("view: Attention"),
        "Esc must close the modal and return to the Attention pane:\n{closed}"
    );

    // --- `?` from the Lanes pane opens auto-focused to the Lanes section ---
    console.send_keys(&["Down", "Down"])?; // Attention -> Spec -> Lanes
    console.wait_for_settled("view: Lanes", RENDER_TIMEOUT)?;
    console.send_keys(&["?"])?;
    let lanes = console.wait_for_settled("esc to exit", RENDER_TIMEOUT)?;
    assert!(
        lanes.contains("> Lanes") && lanes.contains("lane board"),
        "help from Lanes must auto-focus the Lanes section:\n{lanes}"
    );
    assert!(
        !lanes.contains("event timeline"),
        "the Lanes section must not show the Events text:\n{lanes}"
    );

    // --- navigating the left menu switches the right pane's content (down) ---
    console.send_keys(&["Down"])?; // section Lanes -> Events
    let events = console.wait_for_settled("event timeline", RENDER_TIMEOUT)?;
    assert!(
        events.contains("> Events"),
        "Down must move the menu selection to Events:\n{events}"
    );
    assert!(
        !events.contains("lane board"),
        "the right pane content must switch away from the Lanes text:\n{events}"
    );

    // --- left / right do NOT scroll or change the right pane ---
    console.send_keys(&["Left"])?;
    let after_left = console.wait_for_settled("> Events", RENDER_TIMEOUT)?;
    assert!(
        after_left.contains("event timeline") && !after_left.contains("lane board"),
        "Left must be inert (no section change, no scroll):\n{after_left}"
    );
    console.send_keys(&["Right"])?;
    let after_right = console.wait_for_settled("> Events", RENDER_TIMEOUT)?;
    assert!(
        after_right.contains("event timeline"),
        "Right must be inert (no section change, no scroll):\n{after_right}"
    );

    // --- up moves the menu selection back (right pane scrolls up/down only) ---
    console.send_keys(&["Up"])?; // section Events -> Lanes
    let back = console.wait_for_settled("> Lanes", RENDER_TIMEOUT)?;
    assert!(
        back.contains("lane board"),
        "Up must move the menu selection back to Lanes:\n{back}"
    );

    // Esc returns to the Lanes pane.
    console.send_keys(&["Escape"])?;
    let closed = wait_until_absent(&console, "esc to exit", RENDER_TIMEOUT)?;
    assert!(
        closed.contains("view: Lanes"),
        "Esc must return focus to the Lanes pane:\n{closed}"
    );

    // --- `?` from the Settings pane opens auto-focused to the Settings section ---
    console.send_keys(&["Down", "Down", "Down"])?; // Lanes -> Events -> Repos -> Settings
    console.wait_for_settled("view: Settings", RENDER_TIMEOUT)?;
    console.send_keys(&["?"])?;
    let settings = console.wait_for_settled("esc to exit", RENDER_TIMEOUT)?;
    assert!(
        settings.contains("> Settings") && settings.contains("auto_approve_ready"),
        "help from Settings must auto-focus the Settings section:\n{settings}"
    );
    console.send_keys(&["Escape"])?;
    wait_until_absent(&console, "esc to exit", RENDER_TIMEOUT)?;

    // Quit cleanly.
    console.send_keys(&["q"])?;
    console.wait_for("TUI_EXIT=0", RENDER_TIMEOUT)?;
    Ok(())
}

/// Assert the modal Help window is inset by a 3-character border on every side
/// of whatever viewport `tmux` gave the pane. `tmux` honors the pinned height
/// but sizes the width to the outer terminal, so the corner columns are derived
/// from the captured frame rather than hardcoded: with a 3-character margin the
/// box's corners sit at column 3 and column `width - 4`, and its top/bottom
/// borders sit 3 rows from the top and bottom of the `height`-row pane.
fn assert_help_border_geometry(frame: &str) {
    let lines: Vec<&str> = frame.lines().collect();
    // The full-width header top border (`┌...┐`, no trailing space) spans the
    // whole pane, so its char count is the pane width; the pane height is the
    // captured row count.
    let width = lines
        .first()
        .map(|line| line.chars().count())
        .unwrap_or_default();
    let height = lines.len();
    assert!(width >= 8 && height >= 8, "viewport too small:\n{frame}");
    let right = width - 4;
    // The char AT a column (one char per cell). The underlying nav/detail boxes
    // live at column 0 and the right edge, so read the modal's inset columns
    // directly rather than the first box character on the row.
    let at = |line: &str, col: usize| line.chars().nth(col);
    let top_row = lines.iter().position(|line| at(line, 3) == Some('┌'));
    assert_eq!(
        top_row,
        Some(3),
        "modal top-border must be inset to column 3 at row 3:\n{frame}"
    );
    assert_eq!(
        at(lines[3], right),
        Some('┐'),
        "modal top-right corner must sit at column {right} (3-col right margin):\n{frame}"
    );
    let bottom_row = lines.iter().position(|line| at(line, 3) == Some('└'));
    assert_eq!(
        bottom_row,
        Some(height - 4),
        "modal bottom-border must leave a 3-row bottom margin:\n{frame}"
    );
    assert_eq!(
        at(lines[height - 4], right),
        Some('┘'),
        "modal bottom-right corner must sit at column {right}:\n{frame}"
    );
    // Never wider than the viewport.
    for line in &lines {
        assert!(
            line.chars().count() <= width,
            "a rendered line exceeds the {width}-column viewport:\n{line}"
        );
    }
}

/// Poll the rendered pane until `needle` is ABSENT, then return that capture.
///
/// The shared harness's [`TmuxConsole::wait_for`] only waits for a token to
/// APPEAR; asserting the modal closed needs the opposite — wait for the
/// always-on `esc to exit` footer to disappear once `Esc` tears the modal down.
fn wait_until_absent(
    console: &TmuxConsole,
    needle: &str,
    timeout: Duration,
) -> HarnessResult<String> {
    let deadline = Instant::now() + timeout;
    loop {
        let capture = console.capture()?;
        if !capture.contains(needle) {
            return Ok(capture);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "timed out after {timeout:?} waiting for {needle:?} to disappear.\n\
                 ---- last capture ----\n{capture}\n---- end capture ----"
            ));
        }
        std::thread::sleep(Duration::from_millis(150));
    }
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

/// One repo fixture rooted at this crate's manifest dir, for the single-repo B1
/// source-availability scenes.
fn one_repo_fixture() -> RepoFixture {
    RepoFixture::new("e2e-b1", &PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

// --- B1 source-availability honesty (Scenario 13) ---------------------------
//
// These real-TUI scenes prove the cockpit-blind-vs-idle distinction end to end:
// every backing CLI stubbed to emit `{}` is a REACHABLE-but-empty source, so the
// header carries NO source-unavailability indicator (and keeps `mode: tui`),
// while a genuinely-unreachable source is counted, NAMED, and its human-readable
// reason is durably persisted with the finding.

#[test]
#[ignore = "real-TUI tmux E2E; run via `just check-e2e-tmux` (needs tmux + release binary)"]
fn tmux_tui_e2e_all_reachable_sources_are_idle_not_unavailable() -> HarnessResult<()> {
    // Every source is stubbed to emit `{}` (reachable-but-empty). The header
    // must show NO source-unavailability indicator -- an idle factory is never
    // dressed as a cockpit-blind screen -- and therefore keeps the low-value
    // `mode: tui` field it would otherwise shed to make room for a phantom
    // `sources: N unavailable` suffix.
    let repo = one_repo_fixture();
    let console = TmuxConsole::launch(&repo)?;

    let header_needle = format!("repo: {}", repo.tenant());
    let screen = console.wait_for_settled(&header_needle, RENDER_TIMEOUT)?;
    assert!(
        screen.contains("mode: tui"),
        "an all-idle header must keep `mode: tui` (no phantom unavailability \
         suffix forcing a shrink):\n{screen}"
    );
    assert!(
        !screen.contains("unavailable") && !screen.contains("sources:"),
        "an all-idle header must carry NO source-unavailability indicator:\n{screen}"
    );

    console.send_keys(&["q"])?;
    console.wait_for("TUI_EXIT=0", RENDER_TIMEOUT)?;

    // The idle sources persisted observed-and-idle markers, and NO source
    // degraded to a not-observed finding.
    let store = SqliteEventStore::open(console.store_path())
        .map_err(|error| format!("open isolated store failed: {error:?}"))?;
    let events = store
        .list_console_events()
        .map_err(|error| format!("read isolated store events failed: {error:?}"))?;
    assert!(
        events
            .iter()
            .any(|event| event.event_type() == &EventType::SourceObservedFindingObserved),
        "expected at least one observed-and-idle marker from a reachable-but-empty source"
    );
    assert!(
        !events
            .iter()
            .any(|event| event.event_type() == &EventType::SourceNotObservedFindingObserved),
        "no source should degrade to a not-observed finding when every source is reachable"
    );
    Ok(())
}

#[test]
#[ignore = "real-TUI tmux E2E; run via `just check-e2e-tmux` (needs tmux + release binary)"]
fn tmux_tui_e2e_unreachable_source_is_counted_named_and_reasoned() -> HarnessResult<()> {
    // Point ONLY the fabro source at a nonexistent binary; every other source
    // stays idle. The header must count the ONE unavailable source and NAME it,
    // and the not-observed finding must carry a human-readable reason that is
    // durably persisted.
    let repo = one_repo_fixture();
    let console = TmuxConsole::launch_with_env(
        &repo,
        &[(
            "LIVESPEC_CONSOLE_FABRO_PROGRAM",
            "/nonexistent/livespec-console-fabro-missing",
        )],
    )?;

    let screen = console.wait_for("sources: 1 unavailable", RENDER_TIMEOUT)?;
    assert!(
        screen.contains("fabro"),
        "the header must NAME the one unavailable source (fabro):\n{screen}"
    );

    console.send_keys(&["q"])?;
    console.wait_for("TUI_EXIT=0", RENDER_TIMEOUT)?;

    // Exactly one source (fabro) degraded to a not-observed finding, and its
    // reason is durably persisted with the finding (not dropped to `{}`).
    let store = SqliteEventStore::open(console.store_path())
        .map_err(|error| format!("open isolated store failed: {error:?}"))?;
    let events = store
        .list_console_events()
        .map_err(|error| format!("read isolated store events failed: {error:?}"))?;
    let fabro_not_observed: Vec<_> = events
        .iter()
        .filter(|event| {
            event.event_type() == &EventType::SourceNotObservedFindingObserved
                && event.source() == "fabro"
        })
        .collect();
    assert!(
        !fabro_not_observed.is_empty(),
        "fabro must degrade to a not-observed finding when its binary is unresolvable"
    );
    for event in &fabro_not_observed {
        assert!(
            event.payload_json().contains("\"reason\""),
            "the not-observed finding must durably persist a human-readable reason, got {}",
            event.payload_json()
        );
    }
    // No OTHER source degraded -- the idle sources stayed idle.
    let other_not_observed = events.iter().any(|event| {
        event.event_type() == &EventType::SourceNotObservedFindingObserved
            && event.source() != "fabro"
    });
    assert!(
        !other_not_observed,
        "only the genuinely-unreachable fabro source should be counted unavailable"
    );
    Ok(())
}
