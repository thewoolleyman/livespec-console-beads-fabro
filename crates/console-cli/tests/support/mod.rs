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

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// Default pinned pane width used by the operator cockpit E2E scenarios.
pub const DEFAULT_COLS: u16 = 112;
/// Default pinned pane height used by the operator cockpit E2E scenarios.
pub const DEFAULT_ROWS: u16 = 28;

/// How often [`TmuxConsole::wait_for`] re-captures while polling for content.
const POLL_INTERVAL: Duration = Duration::from_millis(150);

/// Monotonic suffix so concurrently-running harness instances never collide on
/// a `tmux` session name or temp dir.
static NONCE: AtomicU32 = AtomicU32::new(0);

/// Every fallible harness operation surfaces its failure as a legible message
/// the test propagates with `?`, so a broken launch or a render regression
/// fails the test loudly instead of panicking inside the harness.
pub type HarnessResult<T> = Result<T, String>;

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
pub struct TmuxConsole {
    tmux: PathBuf,
    session: String,
    scratch: PathBuf,
    store_path: PathBuf,
}

impl TmuxConsole {
    /// Launch the console for `repo` at the default pinned size.
    pub fn launch(repo: &RepoFixture) -> HarnessResult<Self> {
        Self::launch_sized(repo, DEFAULT_COLS, DEFAULT_ROWS)
    }

    /// Launch the console for `repo` at an explicit pane size.
    pub fn launch_sized(repo: &RepoFixture, cols: u16, rows: u16) -> HarnessResult<Self> {
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
        // Best-effort clear of any stale session with this name before launch.
        run_tmux(&tmux, &["kill-session", "-t", &session]);

        let status = Command::new(&tmux)
            .args([
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

        Ok(Self {
            tmux,
            session,
            scratch,
            store_path,
        })
    }

    /// Return the current rendered pane contents.
    pub fn capture(&self) -> HarnessResult<String> {
        let output = Command::new(&self.tmux)
            .args(["capture-pane", "-p", "-t", &self.session])
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
            .args(["send-keys", "-t", &self.session])
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
        let deadline = Instant::now() + timeout;
        let mut previous: Option<String> = None;
        loop {
            let capture = self.capture()?;
            if capture.contains(needle) && previous.as_deref() == Some(capture.as_str()) {
                return Ok(capture);
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "timed out after {timeout:?} waiting for a settled frame containing \
                     {needle:?} in tmux session {session}.\n---- last capture ----\n{capture}\n\
                     ---- end capture ----",
                    session = self.session
                ));
            }
            previous = Some(capture);
            std::thread::sleep(POLL_INTERVAL);
        }
    }

    /// The isolated event-store path this run wrote, for side-effect assertions.
    #[must_use]
    pub fn store_path(&self) -> &Path {
        &self.store_path
    }
}

impl Drop for TmuxConsole {
    fn drop(&mut self) {
        run_tmux(&self.tmux, &["kill-session", "-t", &self.session]);
        let _ = std::fs::remove_dir_all(&self.scratch);
    }
}

/// Run a `tmux` sub-command best-effort, ignoring the outcome.
fn run_tmux(tmux: &Path, args: &[&str]) {
    let _ = Command::new(tmux).args(args).output();
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

/// Write the pane launcher script and return its path. It prepends the scratch
/// dir to PATH (so the `gh` stub shadows the hardcoded github backing CLI),
/// exports the isolated store, the pinned tenant, and the six backing-CLI stub
/// overrides, execs the binary's `serve` (interactive TUI), then keeps the pane
/// alive so a captured error survives inspection. The harness's `Drop` kills the
/// session long before the keep-alive elapses.
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
         export PATH={scratch_dir}:\"$PATH\"\n\
         export LIVESPEC_CONSOLE_STORE_PATH={store}\n\
         export LIVESPEC_CONSOLE_REPO={tenant}\n\
         export LIVESPEC_CONSOLE_REPO_PATH={repo_path}\n\
         export LIVESPEC_CONSOLE_LIST_WORK_ITEMS_PROGRAM={stub}\n\
         export LIVESPEC_CONSOLE_LIVESPEC_PROGRAM={stub}\n\
         export LIVESPEC_CONSOLE_FABRO_PROGRAM={stub}\n\
         export LIVESPEC_CONSOLE_DRAIN_PROGRAM={stub}\n\
         export LIVESPEC_CONSOLE_DRIVE_PROGRAM={stub}\n\
         export LIVESPEC_CONSOLE_NEEDS_ATTENTION_PROGRAM={stub}\n\
         {binary} serve\n\
         printf 'TUI_EXIT=%s\\n' \"$?\"\n\
         sleep 300\n",
        repo_path = shell_quote(&repo.repo_path.display().to_string()),
        scratch_dir = shell_quote(&scratch.display().to_string()),
        store = shell_quote(&store_path.display().to_string()),
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
