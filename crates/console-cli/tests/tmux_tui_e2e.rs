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
use support::attention_rows::{PathBackedAttentionFixture, ROW_SUMMARY};
use support::lifecycle::{ITEM_ID, LifecycleFixture};
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

/// Scenario 19 / B2 — the context-specific Status-line shortcut hints, driven
/// live through a real `tmux` pane against the SHIPPED binary.
///
/// Covers every clause of Scenario 19: (1) a focused pane renders a NON-EMPTY
/// hint line showing that pane's actions; (2) switching focus to a different
/// pane (Lanes -> Settings) changes the hints AND the two panes' hints DIFFER;
/// (3) opening an overlay (`?` Help) replaces the pane's hints with the
/// overlay's, and closing it (Esc) restores the focused pane's hints; and (4) no
/// context in which shortcut actions are available shows an empty hint line.
///
/// The Status line sits at the very bottom of the pane, BELOW the Help modal's
/// 3-row bottom margin, so it stays visible and tmux-capturable while the modal
/// is open — which is how case (3) observes the overlay's hints replacing the
/// pane's. The tokens asserted are each footer-only on their screen: `enter drill`
/// (Lanes hint, hyphenated — distinct from the modal body's "move ... to a
/// status"), `edit row` (Settings hint), and `close help` (Help-overlay hint).
#[test]
#[ignore = "real-TUI tmux E2E; run via `just check-e2e-tmux` (needs tmux + release binary)"]
fn tmux_tui_e2e_status_line_context_hints() -> HarnessResult<()> {
    let repo = RepoFixture::new("e2e-hints", &PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let console = TmuxConsole::launch(&repo)?;

    // --- case 1: the default focused pane (Attention) shows non-empty hints ---
    let attention = console.wait_for_settled("view: Attention", RENDER_TIMEOUT)?;
    assert!(
        attention.contains("Status"),
        "the Status line box must be present:\n{attention}"
    );
    assert!(
        attention.contains("? help") && attention.contains("q quit"),
        "the Attention pane must render its non-empty, context-specific hints:\n{attention}"
    );
    // This fixture's inbox is EMPTY, so NO work-item is selected: the per-item
    // valve keys and the record drill-in alike act on nothing. Proving they are
    // ABSENT here is the real-TUI check on the hint-honesty rule -- a key that
    // cannot act must not be advertised.
    //
    // `enter open` belongs in that list. It was asserted PRESENT here until
    // `2cd1f28` bound Enter to the selected row's work-item record, which an
    // empty inbox does not have; the hint correctly stopped advertising it and
    // this assertion was not updated, which is what made `check-e2e-tmux` red
    // on master. Asserting its ABSENCE is both the current truth and the
    // stronger check.
    assert!(
        !attention.contains("approve/accept/reject"),
        "an empty Attention inbox must not advertise its per-item valve keys:\n{attention}"
    );
    assert!(
        !attention.contains("enter open"),
        "an empty Attention inbox has no selected work-item, so it must not advertise the \
         record drill-in:\n{attention}"
    );

    // --- case 2 (part A): switch focus to the Lanes pane; its hints appear ---
    // Down x2 moves the nav selection Attention -> Spec -> Lanes (no Enter needed;
    // moving the selection switches the active view).
    console.send_keys(&["Down", "Down"])?;
    let lanes = console.wait_for_settled("view: Lanes", RENDER_TIMEOUT)?;
    // `enter drill` rather than `move-status`: this screen is the lane
    // OVERVIEW, which selects a LANE and not a work-item, so every per-item key
    // (move-status, the valves, the policy dials) is inert here and the hint
    // line correctly does not advertise any of them.
    assert!(
        lanes.contains("enter drill"),
        "the Lanes pane hints must surface its distinctive drill key:\n{lanes}"
    );
    assert!(
        !lanes.contains("edit row"),
        "the Lanes hints must not carry the Settings edit key:\n{lanes}"
    );

    // --- case 3: opening the `?` Help overlay swaps the hints to the overlay's ---
    console.send_keys(&["?"])?;
    let help = console.wait_for_settled("esc to exit", RENDER_TIMEOUT)?;
    assert!(
        help.contains("close help"),
        "an open overlay must replace the pane hints with the overlay's hints:\n{help}"
    );
    assert!(
        !help.contains("enter drill"),
        "the Lanes pane hints must be gone while the Help overlay owns the Status line:\n{help}"
    );

    // --- case 3 (cont.): closing the overlay (Esc) restores the pane's hints ---
    console.send_keys(&["Escape"])?;
    let restored = wait_until_absent(&console, "esc to exit", RENDER_TIMEOUT)?;
    assert!(
        restored.contains("enter drill") && !restored.contains("close help"),
        "closing the overlay must restore the Lanes pane's hints:\n{restored}"
    );

    // --- case 2 (part B): switch to the Settings pane; hints DIFFER from Lanes ---
    // Down x3 moves the nav selection Lanes -> Events -> Repos -> Settings.
    console.send_keys(&["Down", "Down", "Down"])?;
    let settings = console.wait_for_settled("view: Settings", RENDER_TIMEOUT)?;
    assert!(
        settings.contains("edit row"),
        "the Settings pane must render its own edit-key hints:\n{settings}"
    );
    assert!(
        !settings.contains("enter drill"),
        "the Settings hints must differ from the Lanes hints (no lane drill):\n{settings}"
    );

    // Quit cleanly.
    console.send_keys(&["q"])?;
    console.wait_for("TUI_EXIT=0", RENDER_TIMEOUT)?;
    Ok(())
}

/// Scenario 20 / B3 — the focusable, horizontally scrollable top/header pane,
/// driven live through a real `tmux` pane against the SHIPPED binary.
///
/// Covers every clause of Scenario 20. At a NARROW pinned width the header
/// content is clipped, so the shrink-to-fit default drops its low-value left
/// field `fleet: livespec` to preserve the priority fields. Focusing the pane
/// (via `Tab`, which cycles focus across every pane including the top/header
/// pane) switches to the FULL, un-degraded header line at offset 0 — so
/// `fleet: livespec` reappears at the left while the right-hand `attention:`
/// field is now clipped off the right edge. Scrolling right reveals that clipped
/// content (and pushes `fleet: livespec` off the left), scrolling left returns
/// to the left edge, and moving focus away snaps the pane back to its
/// shrink-to-fit, left-justified default. A separate wide-viewport launch proves
/// no horizontal scroll is needed when the whole header already fits.
///
/// The focus indicator is the `[focus]` tag on the header block title
/// (`LiveSpec Console [focus]`), the same marker every other focused pane
/// carries. Column arithmetic is avoided: the right/left presses saturate the
/// scroll clamp, so the assertions turn only on which named field is visible at
/// the two extremes, not on an exact offset.
#[test]
#[ignore = "real-TUI tmux E2E; run via `just check-e2e-tmux` (needs tmux + release binary)"]
fn tmux_tui_e2e_top_pane_focus_hscroll() -> HarnessResult<()> {
    let repo = RepoFixture::new("e2e-top-pane", &PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    // A NARROW pane so the header content is clipped and must be scrolled: at 56
    // columns the fit header drops `fleet: livespec` (a low-value left field) and
    // the full line's `attention:` field sits off the right edge.
    let console = TmuxConsole::launch_sized(&repo, NARROW_COLS, support::DEFAULT_ROWS)?;

    // Default (blurred): the shrink-to-fit, left-justified header. It has dropped
    // the low-value `fleet: livespec` field to fit the narrow width, and its
    // block title carries NO focus marker.
    let header_needle = format!("repo: {}", repo.tenant());
    let start = console.wait_for_settled(&header_needle, RENDER_TIMEOUT)?;
    assert!(
        !start.contains("LiveSpec Console [focus]"),
        "the top/header pane must not be focused by default:\n{start}"
    );
    assert!(
        !start.contains("fleet: livespec"),
        "the narrow shrink-to-fit header must drop the low-value fleet field:\n{start}"
    );

    // --- case 1: the top/header pane joins the focus cycle ---
    // Tab cycles focus Nav -> Content -> Detail -> Header (the Attention view has
    // a Detail pane, so the top/header pane is the third Tab from the default
    // Views nav). Focusing it marks the block title `[focus]` and switches to the
    // FULL header line at offset 0: `fleet: livespec` is visible at the left,
    // while the right-hand `attention:` field is clipped off the right edge.
    console.send_keys(&["Tab", "Tab", "Tab"])?;
    let focused = console.wait_for_settled("LiveSpec Console [focus]", RENDER_TIMEOUT)?;
    assert!(
        focused.contains("fleet: livespec"),
        "the focused header shows the full, un-degraded line (fleet at the left):\n{focused}"
    );
    assert!(
        !focused.contains("attention:"),
        "at the left edge the right-hand header fields are clipped off-screen:\n{focused}"
    );

    // --- case 2: horizontal scroll reveals content clipped at the current width ---
    // Many Right presses saturate the scroll clamp, panning to the right edge:
    // the previously-clipped `attention:` field becomes visible, and the left
    // `fleet: livespec` field scrolls off the left (content beyond the width is
    // reachable by scrolling).
    console.send_keys(&[
        "Right", "Right", "Right", "Right", "Right", "Right", "Right", "Right",
    ])?;
    let scrolled = console.wait_for_settled("attention:", RENDER_TIMEOUT)?;
    assert!(
        scrolled.contains("LiveSpec Console [focus]"),
        "the top/header pane stays focused while scrolling:\n{scrolled}"
    );
    assert!(
        scrolled.contains("attention:") && !scrolled.contains("fleet: livespec"),
        "scrolling right reveals the clipped `attention:` field and pans past `fleet`:\n{scrolled}"
    );

    // Scrolling left returns to the left edge (content reachable both directions).
    console.send_keys(&[
        "Left", "Left", "Left", "Left", "Left", "Left", "Left", "Left",
    ])?;
    let back_left = console.wait_for_settled("fleet: livespec", RENDER_TIMEOUT)?;
    assert!(
        back_left.contains("fleet: livespec") && !back_left.contains("attention:"),
        "scrolling left returns to the left edge (fleet visible, attention clipped):\n{back_left}"
    );

    // --- case 3: moving focus away returns the pane to its left-justified default ---
    // Tab moves focus off the header (Header -> Nav); the pane snaps back to the
    // shrink-to-fit default (fleet dropped again) and loses its `[focus]` marker.
    console.send_keys(&["Tab"])?;
    let blurred = wait_until_absent(&console, "LiveSpec Console [focus]", RENDER_TIMEOUT)?;
    assert!(
        !blurred.contains("fleet: livespec"),
        "on blur the pane returns to its shrink-to-fit default (fleet dropped):\n{blurred}"
    );
    assert!(
        blurred.contains(&header_needle),
        "the repo tenant persists after blur:\n{blurred}"
    );

    // Quit cleanly.
    console.send_keys(&["q"])?;
    console.wait_for("TUI_EXIT=0", RENDER_TIMEOUT)?;

    // --- case 4: a wide-enough viewport needs no horizontal scroll ---
    // At the default wide width the whole header fits, so the scroll clamp is
    // zero: focusing the pane shows every field at once (both the left `fleet`
    // and the right `attention:`), and a Right press cannot pan past a header
    // that is already fully visible.
    let wide_repo = RepoFixture::new("e2e-top-wide", &PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let wide = TmuxConsole::launch_sized(&wide_repo, support::DEFAULT_COLS, support::DEFAULT_ROWS)?;
    wide.wait_for_settled(&format!("repo: {}", wide_repo.tenant()), RENDER_TIMEOUT)?;
    wide.send_keys(&["Tab", "Tab", "Tab"])?;
    let wide_focused = wide.wait_for_settled("LiveSpec Console [focus]", RENDER_TIMEOUT)?;
    assert!(
        wide_focused.contains("fleet: livespec") && wide_focused.contains("attention:"),
        "a wide viewport shows the whole header at once, no scrolling needed:\n{wide_focused}"
    );
    wide.send_keys(&["Right"])?;
    let wide_still = wide.wait_for_settled("LiveSpec Console [focus]", RENDER_TIMEOUT)?;
    assert!(
        wide_still.contains("fleet: livespec") && wide_still.contains("attention:"),
        "Right cannot pan a header that already fits (scroll clamp is zero):\n{wide_still}"
    );
    wide.send_keys(&["q"])?;
    wide.wait_for("TUI_EXIT=0", RENDER_TIMEOUT)?;

    Ok(())
}

/// Scenario 21 / B5 — pane bodies render operational content only, with no
/// baked-in documentation prose, driven live through a real `tmux` pane against
/// the SHIPPED binary.
///
/// Covers every clause of Scenario 21: (1) a focused pane renders its
/// operational content — the live counts/state the operator acts on — without an
/// explanatory documentation sentence; (2) a SWEEP of every view finds none of
/// the removed documentation sentences in any pane body — specifically the two
/// named sentences "Spec lifecycle status is projected from `LiveSpec` adapter
/// observations." and "Revise-required events stay visible in the Spec view
/// until resolved.", plus the swept "The event log is the canonical source for
/// projections."; and (3) the operational content stays present (the spec
/// counts, the stored-event count, the repo roster).
///
/// The absence assertions turn on short, distinctive fragments unique to the
/// removed sentences (see [`assert_no_baked_doc_prose`]) so a re-added sentence
/// is caught even if the detail pane wraps it across lines. This test never
/// opens an overlay, so the three KEPT operational-help surfaces (the
/// Status-line hints, the modal Help overlay, the Settings per-row help) are out
/// of frame and correctly untouched by the sweep.
#[test]
#[ignore = "real-TUI tmux E2E; run via `just check-e2e-tmux` (needs tmux + release binary)"]
fn tmux_tui_e2e_panes_operational_content_only_scenario_21() -> HarnessResult<()> {
    let repo = RepoFixture::new("e2e-panes-b5", &PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let console = TmuxConsole::launch(&repo)?;

    // The default Attention view: swept, and its own operational projection (the
    // ranked attention list) confirmed present via its header field.
    let attention = console.wait_for_settled("view: Attention", RENDER_TIMEOUT)?;
    assert_no_baked_doc_prose(&attention, "Attention");

    // --- case 1: the Spec pane renders its operational counts, no doc prose ---
    // From the default Attention view, one Down moves the nav selection to Spec
    // and switches the active view to it (no Enter needed).
    console.send_keys(&["Down"])?;
    let spec = console.wait_for_settled("view: Spec", RENDER_TIMEOUT)?;
    assert!(
        spec.contains("LiveSpec next snapshots:") && spec.contains("Revise required:"),
        "the Spec pane must render its operational counts:\n{spec}"
    );
    assert_no_baked_doc_prose(&spec, "Spec");

    // --- sweep: the Lanes pane (its own lane-board projection), no doc prose ---
    // One Down moves the nav selection Spec -> Lanes.
    console.send_keys(&["Down"])?;
    let lanes = console.wait_for_settled("view: Lanes", RENDER_TIMEOUT)?;
    assert_no_baked_doc_prose(&lanes, "Lanes");

    // --- case 2: the Events pane renders its operational count, no doc prose ---
    // One Down moves the nav selection Lanes -> Events.
    console.send_keys(&["Down"])?;
    let events = console.wait_for_settled("view: Events", RENDER_TIMEOUT)?;
    assert!(
        events.contains("Stored events:"),
        "the Events pane must render its operational stored-event count:\n{events}"
    );
    assert_no_baked_doc_prose(&events, "Events");

    // --- case 3: the Repos pane renders its operational roster, no doc prose ---
    // One Down moves the nav selection Events -> Repos.
    console.send_keys(&["Down"])?;
    let repos = console.wait_for_settled("view: Repos", RENDER_TIMEOUT)?;
    assert!(
        repos.contains("Repos observed:"),
        "the Repos pane must render its operational repo roster:\n{repos}"
    );
    assert_no_baked_doc_prose(&repos, "Repos");

    // --- sweep: the Settings pane (its own dispatcher-settings rows), no prose ---
    // One Down moves the nav selection Repos -> Settings.
    console.send_keys(&["Down"])?;
    let settings = console.wait_for_settled("view: Settings", RENDER_TIMEOUT)?;
    assert_no_baked_doc_prose(&settings, "Settings");

    // Quit cleanly.
    console.send_keys(&["q"])?;
    console.wait_for("TUI_EXIT=0", RENDER_TIMEOUT)?;
    Ok(())
}

/// Assert a rendered frame's pane bodies carry NONE of the documentation
/// sentences B5 removed. Keyed on short, distinctive fragments unique to those
/// sentences — `is projected from` (from "Spec lifecycle status is projected
/// from `LiveSpec` adapter observations."), `stay visible` (from "Revise-required
/// events stay visible in the Spec view until resolved."), and `canonical
/// source` (from "The event log is the canonical source for projections.").
/// Each fragment renders on a single line if its sentence were present, so a
/// re-added sentence is caught even when the detail pane wraps it; and none of
/// the three fragments renders in any KEPT surface, so a match is an
/// unambiguous B5 regression.
fn assert_no_baked_doc_prose(frame: &str, view: &str) {
    for fragment in ["is projected from", "stay visible", "canonical source"] {
        assert!(
            !frame.contains(fragment),
            "the {view} pane body must carry no baked-in documentation prose (found {fragment:?}):\n{frame}"
        );
    }
}

/// The pinned pane width for the narrow top/header-pane scroll scenes: small
/// enough that the header's `attention:` field is clipped off the right edge and
/// the shrink-to-fit default drops the low-value `fleet: livespec` field.
const NARROW_COLS: u16 = 56;

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

// --- B7 key-by-key lifecycle walkthrough -----------------------------------
//
// The acceptance for the walkthrough doc (B7 in
// `plan/cockpit-ux-docs-release/handoff.md`): an agent walks the DOCUMENTED
// steps on a dummy work-item, driving the REAL TUI in a tmux pane, end to end,
// for TWO repos, with no doc/behavior mismatch. This test IS that agent — every
// assertion below quotes what `docs/lifecycle-walkthrough.md` tells an operator
// they will see, so the doc cannot drift from the binary without failing CI.
//
// This asserts EXISTING behavior (the valves of Scenarios 11 and 17, the lanes
// of Scenario 5) against the documentation of it; it introduces no new
// normative clause, so it carries no scenario of its own. The walkthrough page
// itself is permitted by the User Documentation Contract, which lets the tree
// carry further sub-documents beyond the four it names.
//
// Unlike every other scene in this file, these steps need a work-item to exist,
// so they swap the shared `{}` stub for the stateful `LifecycleFixture` (see
// `support/lifecycle.rs`): `{}` is not even a legal `list-work-items --json`
// payload, so the default harness renders an empty board by construction.

/// Step 4 of the walkthrough: `p` on the selected item opens the approve valve.
const APPROVE_MODAL_TITLE: &str = "Approve work-item";
/// Step 8: `c` opens the accept valve.
const ACCEPT_MODAL_TITLE: &str = "Accept work-item";
/// Both valve modals close on this exact affordance line.
const VALVE_CONFIRM_LINE: &str = "Enter to confirm | Esc to cancel";

#[test]
#[ignore = "requires tmux and a release binary; run via `just check-e2e-tmux`"]
fn tmux_tui_e2e_lifecycle_walkthrough_two_repos() -> HarnessResult<()> {
    for (index, repo) in two_repo_fixtures().iter().enumerate() {
        walk_documented_lifecycle(repo, index)?;
    }
    Ok(())
}

/// Walk `docs/lifecycle-walkthrough.md` end to end against one repo.
///
/// The item starts in `pending-approval` so the walk crosses BOTH human valves
/// the lifecycle has — approve (admission) and accept (ship) — rather than only
/// the operator-driven `move` steps between them.
fn walk_documented_lifecycle(repo: &RepoFixture, index: usize) -> HarnessResult<()> {
    let tenant = repo.tenant();
    let fixture = LifecycleFixture::new(&format!("walk{index}"), "pending-approval")?;
    let env = fixture.env();
    let borrowed: Vec<(&str, &str)> = env
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect();
    let console = TmuxConsole::launch_with_env(repo, &borrowed)?;

    // --- Step 1: the item is waiting, and the header counts it ---------------
    console.wait_for("LiveSpec Console", RENDER_TIMEOUT)?;
    let inbox = console.wait_for_settled("Pending approval", RENDER_TIMEOUT)?;
    assert!(
        inbox.contains("attention: 1"),
        "step 1: the header must count the waiting item for {tenant}:\n{inbox}"
    );
    assert!(
        inbox.contains(ITEM_ID),
        "step 1: the detail pane must name the work-item for {tenant}:\n{inbox}"
    );

    // --- Step 2: Enter moves focus from the Views menu into the list ---------
    console.send_keys(&["Enter"])?;
    let focused = console.wait_for_settled("Attention [focus]", RENDER_TIMEOUT)?;
    assert!(
        focused.contains("p/c/r approve/accept/reject"),
        "step 2: the Status line must offer the valve keys for {tenant}:\n{focused}"
    );

    // --- Steps 3-4: `p` opens the approve valve, Enter confirms it -----------
    console.send_keys(&["p"])?;
    let modal = console.wait_for_settled(APPROVE_MODAL_TITLE, RENDER_TIMEOUT)?;
    assert!(
        modal.contains(VALVE_CONFIRM_LINE),
        "step 3: the approve modal must show its confirm affordance for {tenant}:\n{modal}"
    );
    assert!(
        modal.contains("up/down change | enter confirm | esc cancel"),
        "step 3: the Status line must switch to the modal's hints for {tenant}:\n{modal}"
    );

    console.send_keys(&["Enter"])?;
    let approved = console.wait_for_settled("attention: 0", RENDER_TIMEOUT)?;
    assert!(
        approved.contains("No attention item selected"),
        "step 4: approving must empty the inbox for {tenant}:\n{approved}"
    );
    assert_eq!(
        fixture.lane()?,
        "ready",
        "step 4: approving must admit the item to `ready` for {tenant}"
    );

    // --- Step 5: the FACTORY advances the item; the operator cannot ----------
    // `move` refuses `acceptance` (the ship-guard), so this transition is not
    // an operator keystroke and the doc must not pretend otherwise.
    fixture.factory_move("acceptance")?;
    let review = console.wait_for_settled("Acceptance review", RENDER_TIMEOUT)?;
    assert!(
        review.contains("attention: 1"),
        "step 5: the finished item must re-enter the inbox for {tenant}:\n{review}"
    );

    // --- Steps 6-7: `c` opens the accept valve, Enter ships the item ---------
    console.send_keys(&["c"])?;
    let accept = console.wait_for_settled(ACCEPT_MODAL_TITLE, RENDER_TIMEOUT)?;
    assert!(
        accept.contains(VALVE_CONFIRM_LINE),
        "step 6: the accept modal must show its confirm affordance for {tenant}:\n{accept}"
    );

    console.send_keys(&["Enter"])?;
    let shipped = console.wait_for_settled("attention: 0", RENDER_TIMEOUT)?;
    assert!(
        shipped.contains("No attention item selected"),
        "step 7: accepting must empty the inbox for {tenant}:\n{shipped}"
    );
    assert_eq!(
        fixture.lane()?,
        "done",
        "step 7: accepting must ship the item to `done` for {tenant}"
    );

    // --- Step 8: the board shows the item in `done` --------------------------
    console.send_keys(&["Escape"])?;
    console.wait_for_settled("Views [focus]", RENDER_TIMEOUT)?;
    console.send_keys(&["Down", "Down"])?;
    let board = console.wait_for_settled("view: Lanes", RENDER_TIMEOUT)?;
    assert!(
        board.contains("done (1)"),
        "step 8: the board must show the shipped item in `done` for {tenant}:\n{board}"
    );
    assert!(
        board.contains("pending-approval (0)"),
        "step 8: the item must have LEFT its starting lane for {tenant}:\n{board}"
    );

    // --- The drive actions the walk issued, in order -------------------------
    // Asserting the ACTION IDS (not just the screen) proves the documented
    // keystrokes reach the orchestrator as the documented verbs.
    let actions = fixture.actions()?;
    assert_eq!(
        actions,
        vec![format!("approve:{ITEM_ID}"), format!("accept:{ITEM_ID}")],
        "the walk must issue exactly the documented approve/accept actions for {tenant}"
    );
    Ok(())
}

/// B2 hint-honesty on a POPULATED inbox whose selected row names no work-item.
///
/// # The gap this closes
///
/// Scenario 19's rule is that the Status line never advertises a key that would
/// do nothing. Every other tmux scene exercises it against an EMPTY inbox,
/// because the harness's default stubs emit `{}`. That leaves the more
/// interesting half untested: a POPULATED inbox sitting on a row that carries no
/// work-item — a plan thread, a hygiene finding, a spec-revise row — where the
/// per-item valves and the record drill-in are equally inert.
///
/// That state is not hypothetical. The orchestrator inbox observed during the B8
/// acceptance run held 21 items, most of them of exactly this kind, so it is the
/// state an operator meets FIRST on a real repo.
///
/// It is also the state whose DESCRIPTION rotted: `6262f66` moved the flag from
/// the always-present detail projection to `AttentionItem::work_item_id`,
/// broadening "no work-item" from "empty inbox" to "empty inbox OR a row without
/// one" — and every prose description of the condition went stale while
/// `docs_status_hint_lockstep` stayed green, because the hint STRINGS never
/// moved. A gate on the value cannot catch a change in the condition; this scene
/// is the gate on the condition.
///
/// # Why the populated-inbox assertions come FIRST
///
/// An empty inbox produces the SAME hint line as the state under test. If the
/// stub's JSON were malformed the row would not render, the inbox would be
/// empty, and every hint assertion below would pass for entirely the wrong
/// reason. Asserting the row IS on screen, and that the header counts it, is
/// what makes the rest of this test mean anything.
#[test]
#[ignore = "real-TUI tmux E2E; run via `just check-e2e-tmux` (needs tmux + release binary)"]
fn tmux_tui_e2e_hint_honesty_on_a_row_carrying_no_work_item() -> HarnessResult<()> {
    let fixture = PathBackedAttentionFixture::new("hint-honesty")?;
    let repo = RepoFixture::new(
        "e2e-hint-honesty",
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")),
    );
    let env = fixture.env();
    let borrowed: Vec<(&str, &str)> = env
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect();
    let console = TmuxConsole::launch_with_env(&repo, &borrowed)?;

    let screen = console.wait_for_settled("view: Attention", RENDER_TIMEOUT)?;

    // --- the inbox is genuinely POPULATED (see the doc comment) --------------
    assert!(
        screen.contains("attention: 1"),
        "the header must count the path-backed row -- without it every hint \
         assertion below would pass for the empty-inbox reason:\n{screen}"
    );
    assert!(
        screen.contains(ROW_SUMMARY),
        "the path-backed row must RENDER in the Attention pane:\n{screen}"
    );

    // --- and the row carries no work-item, so the per-item keys stay hidden ---
    assert!(
        !screen.contains("approve/accept/reject"),
        "a row naming no work-item must not advertise the per-item valves, even \
         though the inbox is populated:\n{screen}"
    );
    assert!(
        !screen.contains("m/n set-admission/acceptance"),
        "a row naming no work-item must not advertise the policy dials:\n{screen}"
    );
    assert!(
        !screen.contains("enter open"),
        "a row naming no work-item has no record to open, so `enter open` must \
         be absent:\n{screen}"
    );

    // --- the honest remainder is still offered ------------------------------
    assert!(
        screen.contains("? help") && screen.contains("q quit"),
        "the always-available keys must still be advertised:\n{screen}"
    );

    // --- the Detail pane, needs-attention half of the documented split ------
    // `docs/detailed-usage.md` splits the Detail pane by ROW KIND, and this is
    // the kind that keeps `Attach:`. Paired with
    // `tmux_tui_e2e_work_item_row_detail_has_no_attach_without_a_fabro_run`,
    // which asserts the OTHER half; the two together are what make the doc's
    // case-split executable. Asserting only one half is what let the claim
    // ship wrong twice — see that test's comment.
    assert!(
        screen.contains("Fabro run: -"),
        "a needs-attention row always reads `Fabro run: -`:\n{screen}"
    );
    assert!(
        screen.contains("Attach:"),
        "a needs-attention row always carries `Attach:` (its handoff command), \
         even with no Fabro run:\n{screen}"
    );
    Ok(())
}

/// The work-item half of the Detail-pane split documented in
/// `docs/detailed-usage.md`.
///
/// # Why this test exists at all
///
/// The `Attach:` claim shipped WRONG TWICE. First it was documented as
/// unconditional; then, after a plan-thread row was rendered and seen to carry
/// `Attach:`, it was "corrected" to say every needs-attention row has one — and
/// that correction overwrote a walkthrough note which had been right, because a
/// `valve:approve:<id>` row on a `manual` `pending-approval` item does NOT
/// behave like a plan-thread row.
///
/// The reason is `unified_attention_entries`: the inbox merges work-item rows
/// with needs-attention rows and DE-DUPLICATES, dropping a needs-attention row
/// whose work-item a work-item row already claims. So the row kind is not
/// decided by which source emitted it, and no single screen reveals the split.
/// Rendering one case and generalizing is what failed — twice — so both cases
/// are now pinned.
///
/// This half asserts the ABSENCE of `Attach:`, which is only meaningful
/// alongside proof that the row rendered at all; an empty inbox would satisfy
/// the absence trivially.
#[test]
#[ignore = "real-TUI tmux E2E; run via `just check-e2e-tmux` (needs tmux + release binary)"]
fn tmux_tui_e2e_work_item_row_detail_has_no_attach_without_a_fabro_run() -> HarnessResult<()> {
    let fixture = LifecycleFixture::new("detail-split", "pending-approval")?;
    let repo = RepoFixture::new(
        "e2e-detail-split",
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")),
    );
    let env = fixture.env();
    let borrowed: Vec<(&str, &str)> = env
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect();
    let console = TmuxConsole::launch_with_env(&repo, &borrowed)?;

    let screen = console.wait_for_settled("view: Attention", RENDER_TIMEOUT)?;

    // --- the row is really there (see the doc comment) ----------------------
    assert!(
        screen.contains("attention: 1"),
        "the pending-approval work-item must reach the inbox -- without it the \
         `Attach:` absence below proves nothing:\n{screen}"
    );
    assert!(
        screen.contains(&format!("Work item: {ITEM_ID}")),
        "the Detail pane must name the work-item behind the row:\n{screen}"
    );

    // --- and it is the WORK-ITEM projection, not the needs-attention row ----
    assert!(
        screen.contains("Fabro run: -"),
        "no Fabro run is observed for this item, so the line reads `-`:\n{screen}"
    );
    assert!(
        !screen.contains("Attach:"),
        "the work-item projection shows `Attach:` only for a real Fabro attach, \
         so a pending-approval item with no run must have NO such line -- this \
         is the assertion whose absence let the docs ship wrong twice:\n{screen}"
    );
    Ok(())
}
