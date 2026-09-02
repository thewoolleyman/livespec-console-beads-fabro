//! Reusable real-TUI end-to-end harness that drives the console binary through a
//! real `tmux` pane.
//!
//! # Why this exists
//!
//! The interactive TUI entry point (`run_interactive_tui` /
//! `run_interactive_tui_with_effect_sink`) is gated behind
//! `#[cfg(all(not(test), not(coverage)))]`, so it is compiled OUT of every
//! `cargo test` / coverage build. The in-process `scenario_*.rs` tests drive
//! `run_store_backed_tui_session` with scripted `TuiSessionRunner` fakes and
//! never touch a real terminal, so the real keypress -> raw-mode -> render path
//! has zero automated coverage. This harness closes that gap: it launches the
//! shipped binary in a pinned-size `tmux` pane, sends real keystrokes, captures
//! the rendered screen, and asserts on the visible content and on the store
//! side effects the run leaves behind.
//!
//! # Hermeticity
//!
//! The console's live source adapters shell out to backing CLIs
//! (`needs-attention`, `drive`, `dispatcher`, ...) that, on a provisioned host,
//! connect to the Beads/Dolt backend and BLOCK for tens of seconds without the
//! credential wrapper. To stay hermetic and fast (and to run in CI with no
//! secrets), the harness points every backing CLI at a trivial stub via the
//! `LIVESPEC_CONSOLE_*_PROGRAM` overrides `BackingCliResolution` honors, and
//! isolates the event store under a per-run temp dir via
//! `LIVESPEC_CONSOLE_STORE_PATH`. The tenant shown in the header is pinned via
//! `LIVESPEC_CONSOLE_REPO`, so the harness is parameterized by repo and the same
//! driver runs against any number of repos.
//!
//! # Waiting on a host the harness does not control (`pis7qu`)
//!
//! Every ceiling in this harness is DERIVED, never a bare literal, because a
//! bare wall-clock ceiling encodes an assumption the host cannot honour: that
//! the work under it finishes in N seconds regardless of what else the machine
//! is doing. On the shared self-hosted CI pool that assumption breaks — several
//! jobs compile and test at once against one disk — and the test then goes RED
//! for a reason the product is not responsible for. Measured 2026-09-02, all on
//! branches already carrying the `l7unt3` readiness gate: PR #933 run
//! 33596120346 (`timed out after 45s waiting for "TUI_EXIT=0" ... tui error:
//! EventStore(Sqlite(SqliteFailure(...`), PR #931 run 33599307984 (`timed out
//! after 45s waiting for a settled frame containing "repo: e2e-b1"`), PR #927
//! run 33599420432 (4 of 11 failed).
//!
//! Three mechanisms produced those failures, and each has its own answer here:
//!
//! 1. **Four consoles at once.** `RUST_TEST_THREADS=4` (`.cargo/config.toml`)
//!    ran four `tmux_tui_e2e` tests in parallel, each with its own release
//!    binary, `SQLite` store, and `tmux` server on the same work volume — the
//!    job competing with itself. [`ConsoleSlot`] now serializes them: a live
//!    [`TmuxConsole`] holds a process-wide slot, so exactly ONE console runs at
//!    a time no matter how many threads the runner hands the binary. Enforced
//!    by the harness rather than by an invocation flag, so it holds under
//!    `cargo test`, `cargo nextest`, and a bare `--test-threads` override alike.
//!
//! 2. **Ceilings that could not bend.** Two answers, applied in order.
//!    A wait STARTS from a readiness signal of the process under test wherever
//!    one exists — [`TmuxConsole::launch`] blocks on the first painted frame,
//!    so no content-settle budget is spent absorbing startup. The residual
//!    ceilings are then SCALED by a measured host-load signal
//!    ([`load_scale_permille`] over `/proc/pressure/io`, else `/proc/loadavg`
//!    against the core count), so a saturated host makes the suite SLOWER
//!    rather than RED. The product of base and scale is clamped at
//!    [`MAX_TIMEOUT`]: past that a run is not slow, it is stuck, and the merge
//!    gate must still fail rather than hang.
//!
//! 3. **A store budget the harness could not reach.** The console's
//!    `PRAGMA busy_timeout` is a product default the harness inherited, with no
//!    way to say "this host is slow". [`store_busy_timeout`] now sizes it from
//!    the harness's OWN first-frame budget and exports it through
//!    `console_eventstore::BUSY_TIMEOUT_ENV`.
//!
//! The load signal is re-measured on EVERY call rather than sampled once per
//! process: the job's own load is what these budgets must track, and it is at
//! its lowest before the first test starts — exactly when a once-per-process
//! sample would be taken.

#![allow(dead_code)]

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Default pinned pane width used by the operator cockpit E2E scenarios.
pub const DEFAULT_COLS: u16 = 112;
/// Default pinned pane height used by the operator cockpit E2E scenarios.
pub const DEFAULT_ROWS: u16 = 28;

/// How often [`TmuxConsole::wait_for`] re-captures while polling for content.
const POLL_INTERVAL: Duration = Duration::from_millis(150);

/// BASE ceiling for a content frame to settle, once the TUI is known to be
/// live. The render itself is sub-second; this slack absorbs poll granularity
/// plus a busy host. It is a BASE, not the ceiling: [`render_timeout`] widens it
/// by the measured host load, and `LIVESPEC_CONSOLE_E2E_RENDER_TIMEOUT_SECS`
/// overrides the base itself — the wall clock is not the thing under test.
const DEFAULT_RENDER_TIMEOUT_SECS: u64 = 45;

/// BASE ceiling for the TUI's FIRST painted frame after launch, overridable via
/// `LIVESPEC_CONSOLE_E2E_READY_TIMEOUT_SECS` and widened by the measured host
/// load in [`ready_timeout`]. This is a SEPARATE budget from the render/settle
/// one because startup on a saturated host (disk at ~100% util, load far above
/// core count) can starve the process for many seconds before it paints
/// anything at all — and, since `pis7qu`, because it is also what the console's
/// contended store open is sized against (see [`store_busy_timeout`]). A blank
/// capture in that window is "not started yet", not "rendered wrong", so it
/// earns its own slack instead of eating a content-settle budget measured from
/// launch.
const DEFAULT_READY_TIMEOUT_SECS: u64 = 45;

/// Hard ceiling on any derived budget, applied AFTER load scaling.
///
/// A scaled budget must stay bounded: past this point the run is not slow, it
/// is stuck, and the merge gate has to fail with a captured frame rather than
/// hang. This is the one number in the harness that is deliberately absolute.
pub const MAX_TIMEOUT: Duration = Duration::from_secs(240);

/// The neutral load scale (1.0x), in per-mille.
///
/// The scale is carried in per-mille INTEGERS rather than floats: every value
/// it multiplies is an integer `Duration`, so floating point would buy nothing
/// and only add precision-loss casts to a workspace that denies them.
pub const NO_SCALE_PERMILLE: u32 = 1_000;

/// The largest factor a measured host-load signal may apply, in per-mille.
///
/// Six times the base budget corresponds to a host stalled on I/O essentially
/// all of the time, or oversubscribed six runnable threads per core. Beyond
/// that the signal stops discriminating — everything is slow — so widening
/// further would only delay a failure that [`MAX_TIMEOUT`] is going to impose
/// anyway.
pub const MAX_SCALE_PERMILLE: u32 = 6_000;

/// Resolve a `Duration` from a `u64`-seconds env override, falling back to
/// `default_secs`. A malformed or empty value falls back rather than failing —
/// the override exists to WIDEN a budget on a slow host, never to break the gate.
fn env_timeout(var: &str, default_secs: u64) -> Duration {
    let secs = std::env::var(var)
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(default_secs);
    Duration::from_secs(secs)
}

/// Parse a fixed-point decimal (`"12"`, `"12.3"`, `"12.34"`) into HUNDREDTHS.
///
/// Both host-load files report fixed-point decimals with two places, so parsing
/// straight to an integer keeps the whole scaling path in integer arithmetic.
/// Anything unparseable yields `None`, which the caller reads as "this signal
/// is unavailable" — never as zero load, which would silently narrow a budget.
#[must_use]
pub fn parse_hundredths(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    let (whole, fraction) = trimmed.split_once('.').unwrap_or((trimmed, ""));
    let whole: u64 = whole.parse().ok()?;
    if !fraction.chars().all(|digit| digit.is_ascii_digit()) {
        return None;
    }
    let mut digits = fraction.chars();
    let tenths = digits
        .next()
        .and_then(|digit| digit.to_digit(10))
        .unwrap_or(0);
    let hundredths = digits
        .next()
        .and_then(|digit| digit.to_digit(10))
        .unwrap_or(0);
    whole
        .checked_mul(100)?
        .checked_add(u64::from(tenths) * 10 + u64::from(hundredths))
}

/// The `some avg10=` figure from `/proc/pressure/io`, in hundredths of a percent.
///
/// PSI's `some` line is the share of the last ten seconds in which at least one
/// task was stalled waiting on I/O — the most direct measure there is of the
/// thing that reddened these tests, and far better targeted than load average,
/// which cannot tell a busy CPU from a saturated disk.
#[must_use]
pub fn io_pressure_hundredths(contents: &str) -> Option<u64> {
    contents
        .lines()
        .find_map(|line| line.strip_prefix("some "))
        .and_then(|rest| {
            rest.split_whitespace()
                .find_map(|field| field.strip_prefix("avg10="))
        })
        .and_then(parse_hundredths)
}

/// The one-minute load average from `/proc/loadavg`, in hundredths.
///
/// The fallback signal, for a kernel built without PSI. Coarser — it counts
/// runnable threads without saying what they are waiting for — but it still
/// separates an idle host from an oversubscribed one.
#[must_use]
pub fn loadavg_hundredths(contents: &str) -> Option<u64> {
    contents
        .split_whitespace()
        .next()
        .and_then(parse_hundredths)
}

/// The factor to widen a budget by, in per-mille, from whichever host-load
/// signals are readable.
///
/// The signals are combined by taking the LARGER: they measure different kinds
/// of scarcity (blocked-on-I/O time versus runnable threads per core), a run
/// can be starved by either, and averaging them would let a quiet CPU mask a
/// saturated disk. With neither readable — a non-Linux host, or a kernel with
/// no PSI and an unreadable `/proc` — the scale is neutral and the base budgets
/// stand exactly as they do today.
#[must_use]
pub fn load_scale_permille(io_pressure: Option<&str>, loadavg: Option<&str>, cpus: u64) -> u32 {
    let span = u64::from(MAX_SCALE_PERMILLE - NO_SCALE_PERMILLE);
    // 0% stalled -> 1.0x, 100% stalled -> the cap, linear between: the budget
    // being scaled is a WAIT, and doubling the share of time the process spends
    // blocked roughly doubles how long that wait takes to clear.
    let from_pressure = io_pressure
        .and_then(io_pressure_hundredths)
        .map(|hundredths| u64::from(NO_SCALE_PERMILLE) + hundredths.min(10_000) * span / 10_000);
    // Runnable threads per core. At or below one per core the host is not
    // oversubscribed and the tight budget stands; above it, a wait is competing
    // with that many peers for the same core.
    let from_loadavg = loadavg
        .and_then(loadavg_hundredths)
        .map(|hundredths| hundredths * 10 / cpus.max(1));
    let scale = from_pressure
        .into_iter()
        .chain(from_loadavg)
        .max()
        .unwrap_or_else(|| u64::from(NO_SCALE_PERMILLE))
        .clamp(u64::from(NO_SCALE_PERMILLE), u64::from(MAX_SCALE_PERMILLE));
    u32::try_from(scale).unwrap_or(MAX_SCALE_PERMILLE)
}

/// Widen `base` by a per-mille scale, bounded by [`MAX_TIMEOUT`].
#[must_use]
pub fn scaled_timeout(base: Duration, scale_permille: u32) -> Duration {
    (base.saturating_mul(scale_permille) / NO_SCALE_PERMILLE).min(MAX_TIMEOUT)
}

/// Measure the host's current load as a per-mille scale factor.
///
/// The composition root for the two `/proc` reads: everything above this line
/// is a pure function over their contents, so the scaling policy is testable
/// without a saturated machine to test it on.
fn host_load_scale_permille() -> u32 {
    let io_pressure = std::fs::read_to_string("/proc/pressure/io").ok();
    let loadavg = std::fs::read_to_string("/proc/loadavg").ok();
    let cpus = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    load_scale_permille(
        io_pressure.as_deref(),
        loadavg.as_deref(),
        u64::try_from(cpus).unwrap_or(1),
    )
}

/// The content-settle ceiling used across the E2E scenarios: a generous base,
/// widenable via `LIVESPEC_CONSOLE_E2E_RENDER_TIMEOUT_SECS` and then scaled by
/// the measured host load. Prefer this over a hard-coded literal so every
/// content wait bends with the host in one place.
#[must_use]
pub fn render_timeout() -> Duration {
    scaled_timeout(
        env_timeout(
            "LIVESPEC_CONSOLE_E2E_RENDER_TIMEOUT_SECS",
            DEFAULT_RENDER_TIMEOUT_SECS,
        ),
        host_load_scale_permille(),
    )
}

/// The first-frame readiness ceiling, widenable via
/// `LIVESPEC_CONSOLE_E2E_READY_TIMEOUT_SECS` and scaled by the measured host
/// load. The env override sets the BASE, so an operator's widening and the
/// measured widening compose rather than one silently replacing the other.
#[must_use]
pub fn ready_timeout() -> Duration {
    scaled_timeout(
        env_timeout(
            "LIVESPEC_CONSOLE_E2E_READY_TIMEOUT_SECS",
            DEFAULT_READY_TIMEOUT_SECS,
        ),
        host_load_scale_permille(),
    )
}

/// The `SQLite` busy timeout the harness arms in the console under test.
///
/// Derived from the harness's OWN first-frame budget rather than picked: the
/// console retries a contended open `STORE_OPEN_ATTEMPTS` times, each attempt
/// waiting out this timeout, so sizing it at that fraction of the first-frame
/// budget makes the whole bounded retry fit inside the window the harness is
/// prepared to wait — the store waits exactly as long as the harness does, and
/// an exhausted open still gets to print its cause on a frame the harness
/// captures. Floored at the product default so this can only ever widen it.
#[must_use]
pub fn store_busy_timeout() -> Duration {
    ready_timeout()
        .checked_div(console_eventstore::STORE_OPEN_ATTEMPTS)
        .unwrap_or(console_eventstore::DEFAULT_BUSY_TIMEOUT)
        .max(console_eventstore::DEFAULT_BUSY_TIMEOUT)
}

/// Whether a console currently occupies the process-wide slot.
///
/// See mechanism 1 in this module's header. A process-wide flag is enough
/// because the tmux E2E tests all run in ONE test binary, so serializing within
/// the process serializes the whole job however many threads the runner
/// supplies.
///
/// The occupancy is a `Mutex<bool>` guarded by a `Condvar` rather than a held
/// `MutexGuard`, so a waiting thread sleeps instead of spinning AND the guard
/// never outlives `acquire`. A [`TmuxConsole`] that carried a lock guard for its
/// whole lifetime would make every console binding in every scenario a
/// long-lived lock guard, which is both harder to reason about and exactly what
/// `clippy::significant_drop_tightening` objects to.
///
/// A poisoned slot is taken anyway (`PoisonError::into_inner`): poisoning means
/// a test panicked while holding a console, which its own failure already
/// reports — wedging every remaining test behind that would replace one legible
/// failure with ten misleading ones.
static CONSOLE_SLOT_OCCUPIED: Mutex<bool> = Mutex::new(false);

/// Signalled whenever [`CONSOLE_SLOT_OCCUPIED`] returns to `false`.
static CONSOLE_SLOT_FREED: Condvar = Condvar::new();

thread_local! {
    /// Whether THIS thread already occupies the console slot.
    ///
    /// Without it, a test that launched a second console before dropping its
    /// first would block on itself and the job would sit until the CI timeout
    /// with no explanation. With it, that mistake fails immediately and says
    /// what to do about it.
    static SLOT_HELD_BY_THIS_THREAD: Cell<bool> = const { Cell::new(false) };
}

/// A held claim on the process-wide console slot, released on drop.
pub struct ConsoleSlot {
    /// Private so the only way to hold a claim is through [`Self::acquire`],
    /// which is what keeps the occupancy flag and the claim in step.
    _private: (),
}

impl ConsoleSlot {
    /// Block until this thread occupies the console slot.
    ///
    /// Errors — rather than blocking — when the calling thread already holds
    /// it, because that can only be a same-thread deadlock in the making.
    pub fn acquire() -> HarnessResult<Self> {
        if SLOT_HELD_BY_THIS_THREAD.with(Cell::get) {
            return Err(
                "this thread already holds the console slot: the harness runs exactly ONE \
                 console at a time, so an earlier TmuxConsole must be dropped (scope it in \
                 a block) before launching the next"
                    .to_owned(),
            );
        }
        let occupied = CONSOLE_SLOT_OCCUPIED
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut occupied = CONSOLE_SLOT_FREED
            .wait_while(occupied, |occupied| *occupied)
            .unwrap_or_else(PoisonError::into_inner);
        *occupied = true;
        drop(occupied);
        SLOT_HELD_BY_THIS_THREAD.with(|held| held.set(true));
        Ok(Self { _private: () })
    }
}

impl Drop for ConsoleSlot {
    fn drop(&mut self) {
        let mut occupied = CONSOLE_SLOT_OCCUPIED
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *occupied = false;
        drop(occupied);
        CONSOLE_SLOT_FREED.notify_one();
        SLOT_HELD_BY_THIS_THREAD.with(|held| held.set(false));
    }
}

/// Poll `capture` until it yields a NON-BLANK frame, then return it.
///
/// This is the readiness gate: it distinguishes "the process has not painted
/// yet" (a blank capture — startup starvation on a loaded host) from any
/// content assertion. Running it once before content-settle budgets begin means
/// a slow first paint no longer silently consumes a settle budget measured from
/// launch, which is the `l7unt3` blank-capture flake. `context` is appended to
/// the timeout message (e.g. the tmux session) and may be empty.
pub fn poll_ready<F>(mut capture: F, timeout: Duration, context: &str) -> HarnessResult<String>
where
    F: FnMut() -> HarnessResult<String>,
{
    let deadline = Instant::now() + timeout;
    loop {
        let frame = capture()?;
        if !frame.trim().is_empty() {
            return Ok(frame);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "timed out after {timeout:?} waiting for the console to paint its first \
                 frame{context} (blank capture — the process was starved at startup, not \
                 wrong; widen LIVESPEC_CONSOLE_E2E_READY_TIMEOUT_SECS on a loaded host)"
            ));
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Poll `capture` until a SETTLED frame containing `needle` appears.
///
/// A frame is settled when a capture both contains `needle` AND is byte-identical
/// to the immediately preceding capture, so a partially painted frame is never
/// returned for multi-token assertions. `context` is appended to the timeout
/// message and may be empty. Extracted from [`TmuxConsole::wait_for_settled`] so
/// the polling logic is unit-testable against a scripted capture source without a
/// real tmux pane.
pub fn poll_settled<F>(
    mut capture: F,
    needle: &str,
    timeout: Duration,
    context: &str,
) -> HarnessResult<String>
where
    F: FnMut() -> HarnessResult<String>,
{
    let deadline = Instant::now() + timeout;
    let mut previous: Option<String> = None;
    loop {
        let frame = capture()?;
        if frame.contains(needle) && previous.as_deref() == Some(frame.as_str()) {
            return Ok(frame);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "timed out after {timeout:?} waiting for a settled frame containing \
                 {needle:?}{context}.\n---- last capture ----\n{frame}\n---- end capture ----"
            ));
        }
        previous = Some(frame);
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Monotonic suffix so concurrently-running harness instances never collide on
/// a `tmux` session name or temp dir.
static NONCE: AtomicU32 = AtomicU32::new(0);

/// Every fallible harness operation surfaces its failure as a legible message
/// the test propagates with `?`, so a broken launch or a render regression
/// fails the test loudly instead of panicking inside the harness.
pub type HarnessResult<T> = Result<T, String>;

/// A stateful backing-CLI fixture for lifecycle scenarios (B7). The `{}` stub
/// this module installs by default cannot serve a work-item, so the walkthrough
/// scenarios use `lifecycle::LifecycleFixture` to supply one.
pub mod attention_rows;
pub mod lifecycle;

/// Identifies the repo/tenant a harness run observes.
///
/// `tenant` is what the header renders after `repo:`; `repo_path` becomes the
/// process working directory and `LIVESPEC_CONSOLE_REPO_PATH`, so repo-scoped
/// resolution matches a real launch. Parameterizing by `RepoFixture` is what
/// lets a single scenario run against two different repos.
#[derive(Debug, Clone)]
pub struct RepoFixture {
    tenant: String,
    repo_path: PathBuf,
}

impl RepoFixture {
    /// Build a fixture from a tenant label and the repo checkout path.
    #[must_use]
    pub fn new(tenant: &str, repo_path: &Path) -> Self {
        Self {
            tenant: tenant.to_owned(),
            repo_path: repo_path.to_path_buf(),
        }
    }

    /// The tenant label rendered in the header (`repo: <tenant>`).
    #[must_use]
    pub fn tenant(&self) -> &str {
        &self.tenant
    }

    /// The repo checkout path used as the process working directory.
    #[must_use]
    pub fn repo_path(&self) -> &Path {
        &self.repo_path
    }
}

/// A live console TUI running in a dedicated `tmux` session.
///
/// Dropping the handle kills the `tmux` session and removes the per-run temp
/// dir, so a failed assertion never leaks a session or scratch store.
///
/// A live handle also holds the process-wide [`ConsoleSlot`], so no second
/// console can start while this one is alive. The slot field is declared LAST
/// on purpose: `Drop for TmuxConsole` runs before any field is dropped, so the
/// `tmux` server is already killed by the time the next console is admitted.
pub struct TmuxConsole {
    tmux: PathBuf,
    session: String,
    socket: String,
    scratch: PathBuf,
    store_path: PathBuf,
    _slot: ConsoleSlot,
}

impl TmuxConsole {
    /// Launch the console for `repo` at the default pinned size.
    pub fn launch(repo: &RepoFixture) -> HarnessResult<Self> {
        Self::launch_sized(repo, DEFAULT_COLS, DEFAULT_ROWS)
    }

    /// Launch the console for `repo` at an explicit pane size.
    pub fn launch_sized(repo: &RepoFixture, cols: u16, rows: u16) -> HarnessResult<Self> {
        // Claimed BEFORE anything is spawned, so a queued test contributes no
        // load of its own while it waits its turn.
        let slot = ConsoleSlot::acquire()?;
        let tmux = resolve_tmux()?;
        let binary = resolve_binary();
        if !binary.is_file() {
            return Err(format!(
                "console binary not found at {}; run `just check-e2e-tmux` (which builds \
                 the release binary and sets LIVESPEC_CONSOLE_E2E_BIN)",
                binary.display()
            ));
        }

        let nonce = NONCE.fetch_add(1, Ordering::Relaxed);
        let unique = format!("{}-{nonce}", std::process::id());
        let scratch = std::env::temp_dir().join(format!("lc-e2e-{unique}"));
        std::fs::create_dir_all(&scratch)
            .map_err(|error| format!("create scratch dir {} failed: {error}", scratch.display()))?;
        let store_path = scratch.join("store.sqlite");

        let stub = write_named_stub(&scratch, "stub-backing-cli.sh")?;
        // Shadow the ONE backing CLI the six *_PROGRAM overrides do NOT cover: the
        // github source runs a literal `gh pr list` on the synchronous startup
        // path (crates/console-cli/src/lib.rs), which otherwise hits the real
        // authenticated GitHub API and lands a live `pr.snapshot_observed` event.
        // A `gh` stub on the front of PATH (see `write_launcher`) keeps the run
        // hermetic: no live network, no real github event.
        write_named_stub(&scratch, "gh")?;
        let launcher = write_launcher(&scratch, &binary, repo, &store_path, &stub)?;

        let session = format!("lc_e2e_{unique}");
        // A DEDICATED per-test tmux socket (never the maintainer-owned default
        // socket): it isolates the pane from every other client on the host so
        // the pinned `-x`/`-y` size is honored deterministically, and lets Drop
        // kill-server the whole isolated server safely. TMUX_TMPDIR keeps the
        // socket file itself inside the per-run scratch dir, so even host runs do
        // not add entries to the maintainer-owned /tmp/tmux-<uid> directory.
        let socket = session.clone();
        // Best-effort clear of any stale session with this name before launch.
        run_tmux(&tmux, &socket, &scratch, &["kill-session", "-t", &session]);

        let status = Command::new(&tmux)
            .env("TMUX_TMPDIR", &scratch)
            .args([
                "-L",
                &socket,
                "new-session",
                "-d",
                "-s",
                &session,
                "-x",
                &cols.to_string(),
                "-y",
                &rows.to_string(),
            ])
            .arg(&launcher)
            .status()
            .map_err(|error| format!("spawn tmux new-session failed: {error}"))?;
        if !status.success() {
            return Err(format!("tmux new-session exited unsuccessfully: {status}"));
        }

        let console = Self {
            tmux,
            session,
            socket,
            scratch,
            store_path,
            _slot: slot,
        };
        // Readiness gate: block until the TUI has painted its FIRST frame before
        // handing the handle back, so every subsequent content-settle budget is
        // spent on rendering, not on absorbing startup latency on a loaded host.
        // This is what makes the harness robust to the `l7unt3` blank-capture
        // starvation (settle clocks that used to start at launch).
        let ready_context = format!(" in tmux session {}", console.session);
        poll_ready(|| console.capture(), ready_timeout(), &ready_context)?;
        Ok(console)
    }

    /// Return the current rendered pane contents.
    pub fn capture(&self) -> HarnessResult<String> {
        let output = Command::new(&self.tmux)
            .env("TMUX_TMPDIR", &self.scratch)
            .args([
                "-L",
                &self.socket,
                "capture-pane",
                "-p",
                "-t",
                &self.session,
            ])
            .output()
            .map_err(|error| format!("tmux capture-pane failed: {error}"))?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Send one or more `tmux` key names / literal strings to the pane.
    ///
    /// Each element is passed as a distinct `send-keys` argument, so `"Down"`,
    /// `"Enter"`, and `"q"` are interpreted as the corresponding keys.
    pub fn send_keys(&self, keys: &[&str]) -> HarnessResult<()> {
        let status = Command::new(&self.tmux)
            .env("TMUX_TMPDIR", &self.scratch)
            .args(["-L", &self.socket, "send-keys", "-t", &self.session])
            .args(keys)
            .status()
            .map_err(|error| format!("tmux send-keys failed: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("tmux send-keys exited unsuccessfully: {status}"))
        }
    }

    /// Poll the rendered pane until `needle` appears, then return the capture.
    ///
    /// Returns an error with the last capture attached if `needle` never appears
    /// within `timeout`, so a render regression fails legibly.
    pub fn wait_for(&self, needle: &str, timeout: Duration) -> HarnessResult<String> {
        let deadline = Instant::now() + timeout;
        loop {
            let capture = self.capture()?;
            if capture.contains(needle) {
                return Ok(capture);
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "timed out after {timeout:?} waiting for {needle:?} in tmux session \
                     {session}.\n---- last capture ----\n{capture}\n---- end capture ----",
                    session = self.session
                ));
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }

    /// Poll until a SETTLED frame containing `needle` appears, and return it.
    ///
    /// A frame is settled when a capture both contains `needle` AND is
    /// byte-identical to the immediately preceding capture — so a partially
    /// painted frame (upper rows drawn, lower rows not yet) is never handed back
    /// for multi-token assertions. Use this before asserting several substrings
    /// against one screen; use [`Self::wait_for`] for a single token. Returns an
    /// error with the last capture attached if no settled frame appears in time.
    pub fn wait_for_settled(&self, needle: &str, timeout: Duration) -> HarnessResult<String> {
        let context = format!(" in tmux session {}", self.session);
        poll_settled(|| self.capture(), needle, timeout, &context)
    }

    /// Block until the TUI paints its first non-blank frame, or fail with a
    /// starvation-aware message. Called by `launch`; exposed for scenarios that
    /// re-assert readiness after a disruptive action.
    pub fn wait_until_ready(&self, timeout: Duration) -> HarnessResult<String> {
        let context = format!(" in tmux session {}", self.session);
        poll_ready(|| self.capture(), timeout, &context)
    }

    /// The isolated event-store path this run wrote, for side-effect assertions.
    #[must_use]
    pub fn store_path(&self) -> &Path {
        &self.store_path
    }
}

impl Drop for TmuxConsole {
    fn drop(&mut self) {
        // The socket is dedicated to this instance, so kill-server tears the
        // whole isolated tmux server down (never the default socket).
        run_tmux(&self.tmux, &self.socket, &self.scratch, &["kill-server"]);
        let _ = std::fs::remove_dir_all(&self.scratch);
    }
}

/// Run a `tmux` sub-command on the instance's dedicated socket best-effort,
/// ignoring the outcome.
fn run_tmux(tmux: &Path, socket: &str, tmux_tmpdir: &Path, args: &[&str]) {
    let _ = Command::new(tmux)
        .env("TMUX_TMPDIR", tmux_tmpdir)
        .arg("-L")
        .arg(socket)
        .args(args)
        .output();
}

/// Resolve the `tmux` binary: `LIVESPEC_CONSOLE_E2E_TMUX` override, then the
/// usual install locations. Fails loudly when absent — the gate REQUIRES `tmux`
/// (add it to the CI image), it must never silently pass.
fn resolve_tmux() -> HarnessResult<PathBuf> {
    if let Some(path) = std::env::var_os("LIVESPEC_CONSOLE_E2E_TMUX") {
        let candidate = PathBuf::from(path);
        if candidate.is_file() {
            return Ok(candidate);
        }
        return Err(format!(
            "LIVESPEC_CONSOLE_E2E_TMUX points at a missing file: {}",
            candidate.display()
        ));
    }
    for candidate in [
        "/usr/bin/tmux",
        "/usr/local/bin/tmux",
        "/opt/homebrew/bin/tmux",
    ] {
        let path = PathBuf::from(candidate);
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(
        "tmux not found (checked /usr/bin, /usr/local/bin, /opt/homebrew/bin). The \
         real-TUI E2E gate requires tmux; add it to the CI image or set \
         LIVESPEC_CONSOLE_E2E_TMUX."
            .to_owned(),
    )
}

/// Resolve the console binary under test: `LIVESPEC_CONSOLE_E2E_BIN` override
/// (set by `just check-e2e-tmux` to the RELEASE binary), else the profile-built
/// binary of this package.
fn resolve_binary() -> PathBuf {
    std::env::var_os("LIVESPEC_CONSOLE_E2E_BIN").map_or_else(
        || PathBuf::from(env!("CARGO_BIN_EXE_livespec-console-beads-fabro")),
        PathBuf::from,
    )
}

/// Write a fast `{}`-emitting stub named `name` into the scratch dir and return
/// its path. The stub prints an empty JSON object and exits 0, so any backing CLI
/// pointed at it resolves instantly with no Beads/Dolt backend and no credential
/// wrapper — turning that source into a deterministic not-observed finding.
fn write_named_stub(scratch: &Path, name: &str) -> HarnessResult<PathBuf> {
    let stub = scratch.join(name);
    let body = "#!/usr/bin/env bash\nprintf '{}\\n'\nexit 0\n";
    std::fs::write(&stub, body)
        .map_err(|error| format!("write stub {} failed: {error}", stub.display()))?;
    make_executable(&stub)?;
    Ok(stub)
}

/// Write the pane launcher script and return its path. It sets a HERMETIC PATH
/// (the scratch dir front, then only the coreutils dirs — NOT the ambient PATH),
/// so the `gh` stub shadows the github backing CLI AND no source can silently
/// resolve a real backing CLI (fabro, `livespec`, ...) further down an inherited
/// PATH. That determinism is load-bearing: with the ambient PATH inherited, a
/// reachable-but-empty source could resolve a REAL binary locally and be
/// classified observed, masking a misclassification that a CI runner (whose base
/// PATH lacks those binaries) would surface. It also exports the isolated store,
/// the store's raised busy timeout (see [`store_busy_timeout`]), the pinned
/// tenant, and the six backing-CLI stub overrides, execs the binary's `serve`
/// (interactive TUI), then keeps the pane alive so a captured error survives
/// inspection. The harness's `Drop` kills the session long before the keep-alive
/// elapses.
fn write_launcher(
    scratch: &Path,
    binary: &Path,
    repo: &RepoFixture,
    store_path: &Path,
    stub: &Path,
) -> HarnessResult<PathBuf> {
    let launcher = scratch.join("launch.sh");
    let stub = shell_quote(&stub.display().to_string());
    let body = format!(
        "#!/usr/bin/env bash\n\
         cd {repo_path} || exit 97\n\
         export PATH={scratch_dir}:/usr/local/bin:/usr/bin:/bin\n\
         export LIVESPEC_CONSOLE_STORE_PATH={store}\n\
         export {busy_timeout_env}={busy_timeout_ms}\n\
         export LIVESPEC_CONSOLE_REPO={tenant}\n\
         export LIVESPEC_CONSOLE_REPO_PATH={repo_path}\n\
         export LIVESPEC_CONSOLE_LIST_WORK_ITEMS_PROGRAM={stub}\n\
         export LIVESPEC_CONSOLE_LIVESPEC_PROGRAM={stub}\n\
         export LIVESPEC_CONSOLE_FABRO_PROGRAM={stub}\n\
         export LIVESPEC_CONSOLE_DRAIN_PROGRAM={stub}\n\
         export LIVESPEC_CONSOLE_DRIVE_PROGRAM={stub}\n\
         export LIVESPEC_CONSOLE_NEEDS_ATTENTION_PROGRAM={stub}\n\
         export LIVESPEC_CONSOLE_GH_PROGRAM={stub}\n\
         {binary} serve\n\
         printf 'TUI_EXIT=%s\\n' \"$?\"\n\
         sleep 300\n",
        repo_path = shell_quote(&repo.repo_path.display().to_string()),
        scratch_dir = shell_quote(&scratch.display().to_string()),
        store = shell_quote(&store_path.display().to_string()),
        busy_timeout_env = console_eventstore::BUSY_TIMEOUT_ENV,
        busy_timeout_ms = store_busy_timeout().as_millis(),
        tenant = shell_quote(repo.tenant()),
        binary = shell_quote(&binary.display().to_string()),
    );
    std::fs::write(&launcher, body)
        .map_err(|error| format!("write launcher {} failed: {error}", launcher.display()))?;
    make_executable(&launcher)?;
    Ok(launcher)
}

/// Single-quote a value for safe interpolation into the generated bash script.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Mark a generated helper script executable (0o755).
fn make_executable(path: &Path) -> HarnessResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let permissions = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(path, permissions)
        .map_err(|error| format!("chmod {} failed: {error}", path.display()))
}

// --- B1 source-availability extension (append-only) --------------------------
//
// The default launcher points every backing CLI at the `{}`-emitting stub, so a
// hermetic run resolves EVERY source as observed-and-idle. To exercise a
// GENUINELY-unreachable source (Scenario 13's "named with a reason"), the B1
// tests need to point exactly one `*_PROGRAM` at a bad program while leaving the
// rest idle. This extension launches with EXTRA environment exports appended
// after the default stub exports, so a caller's override wins (later bash
// `export` wins).

impl TmuxConsole {
    /// Launch like [`Self::launch`], but append `extra_env` exports AFTER the
    /// default `{}`-stub `*_PROGRAM` exports so a caller can repoint one backing
    /// source (for example at a nonexistent binary) while the rest stay idle.
    pub fn launch_with_env(repo: &RepoFixture, extra_env: &[(&str, &str)]) -> HarnessResult<Self> {
        // Claimed BEFORE anything is spawned, so a queued test contributes no
        // load of its own while it waits its turn.
        let slot = ConsoleSlot::acquire()?;
        let tmux = resolve_tmux()?;
        let binary = resolve_binary();
        if !binary.is_file() {
            return Err(format!(
                "console binary not found at {}; run `just check-e2e-tmux` (which builds \
                 the release binary and sets LIVESPEC_CONSOLE_E2E_BIN)",
                binary.display()
            ));
        }

        let nonce = NONCE.fetch_add(1, Ordering::Relaxed);
        let unique = format!("{}-{nonce}", std::process::id());
        let scratch = std::env::temp_dir().join(format!("lc-e2e-{unique}"));
        std::fs::create_dir_all(&scratch)
            .map_err(|error| format!("create scratch dir {} failed: {error}", scratch.display()))?;
        let store_path = scratch.join("store.sqlite");

        let stub = write_named_stub(&scratch, "stub-backing-cli.sh")?;
        write_named_stub(&scratch, "gh")?;
        let launcher =
            write_launcher_with_env(&scratch, &binary, repo, &store_path, &stub, extra_env)?;

        let session = format!("lc_e2e_{unique}");
        let socket = session.clone();
        run_tmux(&tmux, &socket, &scratch, &["kill-session", "-t", &session]);

        let status = Command::new(&tmux)
            .env("TMUX_TMPDIR", &scratch)
            .args([
                "-L",
                &socket,
                "new-session",
                "-d",
                "-s",
                &session,
                "-x",
                &DEFAULT_COLS.to_string(),
                "-y",
                &DEFAULT_ROWS.to_string(),
            ])
            .arg(&launcher)
            .status()
            .map_err(|error| format!("spawn tmux new-session failed: {error}"))?;
        if !status.success() {
            return Err(format!("tmux new-session exited unsuccessfully: {status}"));
        }

        let console = Self {
            tmux,
            session,
            socket,
            scratch,
            store_path,
            _slot: slot,
        };
        // Readiness gate: block until the TUI has painted its FIRST frame before
        // handing the handle back, so every subsequent content-settle budget is
        // spent on rendering, not on absorbing startup latency on a loaded host.
        // This is what makes the harness robust to the `l7unt3` blank-capture
        // starvation (settle clocks that used to start at launch).
        let ready_context = format!(" in tmux session {}", console.session);
        poll_ready(|| console.capture(), ready_timeout(), &ready_context)?;
        Ok(console)
    }
}

/// Like [`write_launcher`], but append `extra_env` exports after the default
/// stub `*_PROGRAM` exports so a caller's override wins.
///
/// That ordering covers the raised store busy timeout too: it is exported with
/// the defaults, so a scenario that needs a different budget — or a different
/// store path — can still say so and be obeyed.
fn write_launcher_with_env(
    scratch: &Path,
    binary: &Path,
    repo: &RepoFixture,
    store_path: &Path,
    stub: &Path,
    extra_env: &[(&str, &str)],
) -> HarnessResult<PathBuf> {
    use std::fmt::Write as _;
    let launcher = scratch.join("launch.sh");
    let stub = shell_quote(&stub.display().to_string());
    let mut extra = String::new();
    for (key, value) in extra_env {
        // Writing to a String is infallible; the Result is discarded.
        let _ = writeln!(extra, "export {key}={}", shell_quote(value));
    }
    let body = format!(
        "#!/usr/bin/env bash\n\
         cd {repo_path} || exit 97\n\
         export PATH={scratch_dir}:/usr/local/bin:/usr/bin:/bin\n\
         export LIVESPEC_CONSOLE_STORE_PATH={store}\n\
         export {busy_timeout_env}={busy_timeout_ms}\n\
         export LIVESPEC_CONSOLE_REPO={tenant}\n\
         export LIVESPEC_CONSOLE_REPO_PATH={repo_path}\n\
         export LIVESPEC_CONSOLE_LIST_WORK_ITEMS_PROGRAM={stub}\n\
         export LIVESPEC_CONSOLE_LIVESPEC_PROGRAM={stub}\n\
         export LIVESPEC_CONSOLE_FABRO_PROGRAM={stub}\n\
         export LIVESPEC_CONSOLE_DRAIN_PROGRAM={stub}\n\
         export LIVESPEC_CONSOLE_DRIVE_PROGRAM={stub}\n\
         export LIVESPEC_CONSOLE_NEEDS_ATTENTION_PROGRAM={stub}\n\
         export LIVESPEC_CONSOLE_GH_PROGRAM={stub}\n\
         {extra}\
         {binary} serve\n\
         printf 'TUI_EXIT=%s\\n' \"$?\"\n\
         sleep 300\n",
        repo_path = shell_quote(&repo.repo_path().display().to_string()),
        scratch_dir = shell_quote(&scratch.display().to_string()),
        store = shell_quote(&store_path.display().to_string()),
        busy_timeout_env = console_eventstore::BUSY_TIMEOUT_ENV,
        busy_timeout_ms = store_busy_timeout().as_millis(),
        tenant = shell_quote(repo.tenant()),
        binary = shell_quote(&binary.display().to_string()),
    );
    std::fs::write(&launcher, body)
        .map_err(|error| format!("write launcher {} failed: {error}", launcher.display()))?;
    make_executable(&launcher)?;
    Ok(launcher)
}
