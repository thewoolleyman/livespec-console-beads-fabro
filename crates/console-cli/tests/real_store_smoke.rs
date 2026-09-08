//! `livespec-console-beads-fabro-mx9u.15` — a smoke gate starts the console
//! against a store seeded from REAL captured event history and fails if it
//! cannot stay up.
//!
//! # Why this exists
//!
//! On 2026-09-08, `dd50c09` (`livespec-console-beads-fabro-mx9u.11`) merged
//! with all 20 CI checks green and a green post-merge janitor, and made the
//! console exit 1 shortly after startup against the maintainer's real store
//! (`tui error: AttentionResolveDuplicate("...hygiene:idle-factory...
//! :resolved:3124974868900000571")`). Nothing in this repo's gates ever
//! started the compiled binary against a store holding real accumulated
//! event history, so a defect that depends on EXISTING STORE STATE was
//! invisible to every gate — including `check-e2e-tmux`, which drives a real
//! `tmux` pane but always against a freshly-created, empty store.
//!
//! This file closes that gap with two layers:
//!
//! - A FAST, non-`#[ignore]`d layer (runs in the default `cargo test` /
//!   `check-nextest` matrix) that proves, at the `ingest_needs_attention`
//!   level, that the real fixture reproduces the exact real collision named
//!   in the crash log, and that it fails closed exactly the way `dd50c09`'s
//!   unmitigated `AttentionResolveDuplicate` propagation did before
//!   `livespec-console-beads-fabro-mx9u.11-crash` (PR #1106) taught
//!   `refresh_sources` to downgrade it to a diagnostic instead.
//! - An `#[ignore]`d, `tmux`-driven layer — mirroring `tmux_tui_e2e.rs` —
//!   that builds the RELEASE binary, seeds a store from the fixture, and
//!   asserts a real rendered frame appears (never merely a zero exit code:
//!   `should_run_interactive_tui` in `console-cli/src/main.rs` only enters the
//!   TUI path when stdout is a TTY, so a redirected-stdout invocation of this
//!   binary exits 0 having served no terminal at all and proves nothing).
//!   Also proves the DETECTION MECHANISM itself fails closed against a
//!   reproduction of `dd50c09`'s literal crash shape (exits non-zero before
//!   any frame renders), without needing to check out that commit.
//!
//! Run via `just check-real-store-smoke`, which builds the release binary,
//! points the harness at it, and runs this file's `#[ignore]`d tests
//! explicitly — wired into the `check` aggregate and therefore the post-merge
//! janitor.
//!
//! # The fixture
//!
//! `tests/fixtures/real_store_smoke/idle_factory_toggle.json` is a literal,
//! byte-for-byte export of nine rows from the maintainer's real
//! `tmp/livespec-console.sqlite`, captured 2026-09-08 — not synthesised. See
//! `support::real_store_seed` for the full provenance and why exactly these
//! nine rows reproduce the real collision deterministically: row 9
//! (`appeared`) carries content byte-identical to row 7 (`changed`), and row
//! 8 (`resolved`) already claims the identity that resolving row 9's
//! occurrence recomputes.

mod support;

use std::os::unix::fs::PermissionsExt as _;
use std::time::Duration;

use console_application::source_adapters::{
    AttentionItemSnapshot, NeedsAttentionReadOutcome, NeedsAttentionSnapshotPort,
};
use console_domain::{ConsoleEvent, EventType};
use console_eventstore::{
    AppendOutcome, AppendStatus, CommandAppend, CommandAppendOutcome, CommandStatusUpdateOutcome,
    EventAppend, EventStoreResult, SqliteEventStore, StoredCommand,
};
use livespec_console_beads_fabro::{
    ConsoleRuntimeError, FactoryCommandStore, NeedsAttentionIngest, ingest_needs_attention,
};
use support::real_store_seed::{self, FixtureEvent};
use support::{HarnessResult, RepoFixture, TmuxConsole, real_store_smoke_ready_timeout};

const REPO: &str = "livespec-console-beads-fabro";
const IDLE_FACTORY_ID: &str = "hygiene:idle-factory:livespec-console-beads-fabro";
const FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/real_store_smoke/idle_factory_toggle.json"
);
/// The real, already-claimed identity from the crash log this gate exists to
/// catch (`livespec-console-beads-fabro-mx9u.15`'s description), and the
/// identity row 9's occurrence recomputes when it resolves.
const COLLIDING_IDENTITY: &str = "resolved:3124974868900000571";

fn load_fixture() -> Result<Vec<FixtureEvent>, String> {
    real_store_seed::load_fixture(std::path::Path::new(FIXTURE_PATH))
}

fn seeded_in_memory_store(fixtures: &[FixtureEvent]) -> Result<SqliteEventStore, String> {
    let mut store = SqliteEventStore::open_in_memory()
        .map_err(|error| format!("open in-memory store failed: {error:?}"))?;
    real_store_seed::seed_store(&mut store, fixtures)?;
    Ok(store)
}

// --- fixture sanity: the export really is what this file's docs claim ------

#[test]
fn the_fixture_holds_the_nine_real_rows_the_collision_needs() -> Result<(), String> {
    let fixtures = load_fixture()?;
    assert_eq!(
        fixtures.len(),
        9,
        "the exported real-history fixture should carry exactly the 9 rows documented in \
         support::real_store_seed"
    );
    let row7 = &fixtures[6];
    let row8 = &fixtures[7];
    let row9 = &fixtures[8];
    assert_eq!(row7.event_type(), &EventType::AttentionItemChanged);
    assert_eq!(row8.event_type(), &EventType::AttentionItemResolved);
    assert_eq!(row9.event_type(), &EventType::AttentionItemAppeared);
    assert_eq!(
        row7.payload_json(),
        row9.payload_json(),
        "row 9 must carry BYTE-IDENTICAL content to row 7 — this real coincidence is what \
         makes resolving row 9 recompute row 8's already-claimed identity"
    );
    let Some(row8_id) = row8.source_event_id() else {
        return Err("row 8 (resolved) must carry a source_event_id".to_owned());
    };
    assert!(
        row8_id.ends_with(COLLIDING_IDENTITY),
        "row 8 should already claim the identity named in the crash log, got {row8_id:?}"
    );
    Ok(())
}

// --- ingest-level proof: the real fixture reproduces the real defect -------

struct StubNeedsAttentionPort {
    snapshot: Vec<AttentionItemSnapshot>,
}

impl NeedsAttentionSnapshotPort for StubNeedsAttentionPort {
    fn read_snapshot(&self) -> NeedsAttentionReadOutcome {
        NeedsAttentionReadOutcome::Observed(self.snapshot.clone())
    }
}

/// A store decorator that always reports a resolved-event append (INCLUDING
/// `ingest_needs_attention`'s disambiguated retry, since its rebuilt event
/// still carries `EventType::AttentionItemResolved`) as `Duplicate`.
///
/// This reproduces, deterministically and without checking out any commit,
/// the exact failure mode `dd50c09` shipped before
/// `livespec-console-beads-fabro-mx9u.11-crash` (PR #1106) added the
/// idempotent retry AND taught `refresh_sources` to downgrade the residual
/// case: an ingest that returns `Err(AttentionResolveDuplicate)` for an
/// already-resolved occurrence, with nothing left to catch it before it
/// would reach `main`'s fatal `std::process::exit(1)`.
struct AlwaysDuplicateOnResolveStore<'a> {
    inner: &'a mut SqliteEventStore,
}

impl FactoryCommandStore for AlwaysDuplicateOnResolveStore<'_> {
    fn list_commands(&self) -> EventStoreResult<Vec<StoredCommand>> {
        self.inner.list_commands()
    }

    fn list_console_events(&self) -> EventStoreResult<Vec<ConsoleEvent>> {
        self.inner.list_console_events()
    }

    fn append_command(&mut self, append: &CommandAppend) -> EventStoreResult<CommandAppendOutcome> {
        self.inner.append_command(append)
    }

    fn append_event(&mut self, append: &EventAppend) -> EventStoreResult<AppendOutcome> {
        if append.event().event_type() == &EventType::AttentionItemResolved {
            return Ok(AppendOutcome::new(0, AppendStatus::Duplicate));
        }
        self.inner.append_event(append)
    }

    fn claim_command(&mut self, command_id: &str, claimed_at: &str) -> EventStoreResult<bool> {
        self.inner.claim_command(command_id, claimed_at)
    }

    fn update_command_status(
        &mut self,
        command_id: &str,
        status: &str,
        updated_at: &str,
        result_json: Option<&str>,
        error_json: Option<&str>,
    ) -> EventStoreResult<CommandStatusUpdateOutcome> {
        self.inner
            .update_command_status(command_id, status, updated_at, result_json, error_json)
    }

    fn finalize_executing_command_status(
        &mut self,
        command_id: &str,
        status: &str,
        updated_at: &str,
        result_json: Option<&str>,
        error_json: Option<&str>,
    ) -> EventStoreResult<CommandStatusUpdateOutcome> {
        self.inner.finalize_executing_command_status(
            command_id,
            status,
            updated_at,
            result_json,
            error_json,
        )
    }

    fn fail_stale_executing_commands(
        &mut self,
        stale_before: &str,
        recovered_at: &str,
        error_json: &str,
    ) -> EventStoreResult<usize> {
        self.inner
            .fail_stale_executing_commands(stale_before, recovered_at, error_json)
    }
}

/// AC2: proves the real fixture reproduces the real defect this gate exists
/// to catch — an ingest that returns `Err` for an already-resolved
/// occurrence — reproducing `dd50c09`'s unmitigated propagation shape.
#[test]
fn the_real_fixture_reproduces_an_unmitigated_duplicate_resolve_error() -> Result<(), String> {
    let fixtures = load_fixture()?;
    let mut store = seeded_in_memory_store(&fixtures)?;
    let mut duplicating = AlwaysDuplicateOnResolveStore { inner: &mut store };
    let port = StubNeedsAttentionPort { snapshot: vec![] };
    let needs_attention = NeedsAttentionIngest::new(&port, REPO);

    // Row 9's occurrence is open (it was never resolved in the real export);
    // an empty live snapshot means it just disappeared, so ingest attempts to
    // resolve it.
    let result = ingest_needs_attention(&mut duplicating, &needs_attention, "smoke-t0");

    let Err(error) = result else {
        return Err(format!(
            "expected the real fixture's toggle to fail closed under an unmitigated duplicate \
             resolve, got {result:?}"
        ));
    };
    let ConsoleRuntimeError::AttentionResolveDuplicate(detail) = &error else {
        return Err(format!("expected AttentionResolveDuplicate, got {error:?}"));
    };
    assert!(
        detail.contains(COLLIDING_IDENTITY),
        "the failure should name the real colliding identity from the crash log, got {detail:?}"
    );
    assert!(
        detail.contains(REPO),
        "diagnostic should name the repo: {detail:?}"
    );
    Ok(())
}

/// AC1's "passes otherwise": the SAME real fixture, run through the CURRENT,
/// UNDECORATED ingest path (the idempotent retry `ingest_needs_attention`
/// itself performs), lands cleanly.
#[test]
fn the_real_fixture_resolves_cleanly_under_the_current_ingest_path() -> Result<(), String> {
    let fixtures = load_fixture()?;
    let mut store = seeded_in_memory_store(&fixtures)?;
    let port = StubNeedsAttentionPort { snapshot: vec![] };
    let needs_attention = NeedsAttentionIngest::new(&port, REPO);

    let result = ingest_needs_attention(&mut store, &needs_attention, "smoke-t0");

    assert!(
        result.is_ok(),
        "the current ingest path must retire the real fixture's toggle without error, got \
         {result:?}"
    );
    Ok(())
}

// --- tmux-level proof: the smoke check's own detection mechanism -----------

fn idle_factory_repo_fixture() -> RepoFixture {
    RepoFixture::new(
        "real-store-smoke",
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")),
    )
}

/// A stub `needs-attention` program that reports the idle-factory occurrence
/// as ABSENT (an explicit empty `attention[]` array, not the default hermetic
/// `{}` stub — which the port reads as "unavailable" and never diffs). This is
/// what drives the seeded store's still-open row-9 occurrence into a
/// resolution attempt at startup, the same way the live source did when the
/// real crash happened.
fn write_absent_needs_attention_stub(dir: &std::path::Path) -> HarnessResult<std::path::PathBuf> {
    let path = dir.join("needs-attention-absent.sh");
    std::fs::write(
        &path,
        "#!/usr/bin/env bash\nprintf '{\"attention\":[]}\\n'\nexit 0\n",
    )
    .map_err(|error| format!("write stub failed: {error}"))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .map_err(|error| format!("chmod stub failed: {error}"))?;
    Ok(path)
}

/// A "known-bad" fake binary reproducing `dd50c09`'s literal crash shape: it
/// exits 1 immediately, printing the exact `tui error: ...` diagnostic the
/// real crash printed, WITHOUT ever rendering a frame. Used to prove this
/// gate's OWN detection mechanism (a `tmux`-driven wait for a real rendered
/// frame, per AC5) fails closed against that shape — without checking out
/// `dd50c09`, whose propagation this fake reproduces byte-for-byte.
fn write_known_bad_binary(dir: &std::path::Path) -> HarnessResult<std::path::PathBuf> {
    let path = dir.join("known-bad-console.sh");
    let body = format!(
        "#!/usr/bin/env bash\n\
         echo 'tui error: AttentionResolveDuplicate(\"{REPO}:needs-attention:{REPO}:{IDLE_FACTORY_ID}:{COLLIDING_IDENTITY}\")' >&2\n\
         exit 1\n"
    );
    std::fs::write(&path, body)
        .map_err(|error| format!("write known-bad binary failed: {error}"))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .map_err(|error| format!("chmod known-bad binary failed: {error}"))?;
    Ok(path)
}

/// AC2 + AC3, at the level this gate actually operates: proves the smoke
/// check's OWN detection mechanism reports FAILURE — with the exit status and
/// last terminal output captured — against a process reproducing `dd50c09`'s
/// crash shape (exits non-zero, before any frame renders, printing the exact
/// real diagnostic).
#[test]
#[ignore = "real-TUI tmux smoke; run via `just check-real-store-smoke` (needs tmux)"]
fn the_smoke_check_fails_closed_on_a_reproduction_of_the_known_bad_crash() -> HarnessResult<()> {
    let scratch =
        std::env::temp_dir().join(format!("lc-real-store-known-bad-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)
        .map_err(|error| format!("create scratch dir failed: {error}"))?;
    let known_bad = write_known_bad_binary(&scratch)?;

    let repo = idle_factory_repo_fixture();
    let fixtures = load_fixture()?;
    let launched = TmuxConsole::launch_seeded_with_binary(
        &repo,
        Some(&known_bad),
        &fixtures,
        &[],
        Duration::from_secs(20),
    );
    // The known-bad script's OWN crash text makes the pane non-blank almost
    // immediately, so [`TmuxConsole::launch_seeded_with_binary`]'s internal
    // readiness gate (which only distinguishes "nothing painted yet" from
    // "something painted") is satisfied and returns `Ok`. The gate this test
    // proves is the SAME one `real_store_smoke_survives_the_real_captured_
    // duplicate_resolve_toggle` uses to prove success: waiting for the real
    // rendered header. A crash must fail THAT wait, not the readiness gate.
    let console = launched?;
    let result = console.wait_for_settled("LiveSpec Console", Duration::from_secs(10));
    let _ = std::fs::remove_dir_all(&scratch);

    let Err(error) = result else {
        return Err(
            "expected the known-bad reproduction to never render the real console header, but \
             the wait for it succeeded"
                .to_owned(),
        );
    };
    assert!(
        error.contains("AttentionResolveDuplicate"),
        "failure diagnostic should carry the process's own crash output: {error}"
    );
    assert!(
        error.contains("TUI_EXIT=1"),
        "failure diagnostic should carry the exit status the launcher observed: {error}"
    );
    Ok(())
}

/// AC5: a non-TTY invocation of this binary is NOT mistaken for success —
/// `should_run_interactive_tui` in `console-cli/src/main.rs` only enters the
/// TUI path when stdout is a real terminal, so redirecting stdout makes it
/// skip straight to `livespec_console_beads_fabro::run`, which exits 0
/// having served no terminal at all. This gate never relies on that signal:
/// [`TmuxConsole::launch_seeded`] always drives a real `tmux` pty and this
/// test's sibling asserts on RENDERED CONTENT, never on exit code alone.
#[test]
#[ignore = "requires the built release binary; run via `just check-real-store-smoke`"]
fn a_redirected_stdout_invocation_exits_zero_and_proves_nothing() -> HarnessResult<()> {
    let binary = std::env::var("LIVESPEC_CONSOLE_E2E_BIN").map_err(|_error| {
        "LIVESPEC_CONSOLE_E2E_BIN must be set (run via just check-real-store-smoke)".to_owned()
    })?;
    let scratch = std::env::temp_dir().join(format!("lc-non-tty-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)
        .map_err(|error| format!("create scratch dir failed: {error}"))?;
    let store_path = scratch.join("store.sqlite");
    let fixtures = load_fixture()?;
    real_store_seed::seed_store_file(&store_path, &fixtures)?;
    let log_path = scratch.join("redirected.log");
    let log =
        std::fs::File::create(&log_path).map_err(|error| format!("create log failed: {error}"))?;
    let status = std::process::Command::new(&binary)
        .arg("serve")
        .env("LIVESPEC_CONSOLE_STORE_PATH", &store_path)
        .env("LIVESPEC_CONSOLE_REPO", "real-store-smoke-non-tty")
        .stdout(log)
        .status()
        .map_err(|error| format!("spawn {binary} failed: {error}"))?;
    let _ = std::fs::remove_dir_all(&scratch);
    assert!(
        status.success(),
        "documents the pitfall this gate must not fall into: a redirected-stdout invocation \
         is expected to exit 0 ({status}) without ever serving a terminal — this gate must \
         never treat that alone as a pass"
    );
    Ok(())
}

/// AC1 + AC4: the actual gate. Builds against the REAL release binary
/// (`LIVESPEC_CONSOLE_E2E_BIN`, set by `just check-real-store-smoke`), seeds a
/// store from the real captured fixture, and asserts the console renders a
/// real frame and stays up — bounded by
/// [`real_store_smoke_ready_timeout`] (never unbounded).
#[test]
#[ignore = "real-TUI tmux E2E; run via `just check-real-store-smoke` (needs tmux + release binary)"]
fn real_store_smoke_survives_the_real_captured_duplicate_resolve_toggle() -> HarnessResult<()> {
    let repo = idle_factory_repo_fixture();
    let fixtures = load_fixture()?;
    let scratch =
        std::env::temp_dir().join(format!("lc-real-store-absent-stub-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)
        .map_err(|error| format!("create scratch dir failed: {error}"))?;
    let absent_stub = write_absent_needs_attention_stub(&scratch)?;
    let absent_stub_str = absent_stub.display().to_string();

    let console = TmuxConsole::launch_seeded(
        &repo,
        &fixtures,
        &[("LIVESPEC_CONSOLE_NEEDS_ATTENTION_PROGRAM", &absent_stub_str)],
        real_store_smoke_ready_timeout(),
    )?;

    let screen = console.wait_for_settled("LiveSpec Console", real_store_smoke_ready_timeout())?;
    assert!(
        screen.contains(&format!("repo: {}", repo.tenant())),
        "header must render this run's tenant:\n{screen}"
    );
    assert!(
        screen.contains("view: Attention"),
        "expected the Attention view to render (proves this is a real painted frame, not a \
         crash captured mid-scroll):\n{screen}"
    );

    console.send_keys(&["q"])?;
    console.wait_for("TUI_EXIT=0", real_store_smoke_ready_timeout())?;

    let _ = std::fs::remove_dir_all(&scratch);
    Ok(())
}
