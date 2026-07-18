//! CLI binary composition root for the operator console.
//!
//! It opens the configured `SQLite` store, wires the live source probes, and
//! delegates command behavior to the `livespec_console_beads_fabro` library.
//!
//! ```rust,ignore
//! std::process::Command::new("livespec-console-beads-fabro")
//!     .arg("events")
//!     .arg("tail")
//!     .status()?;
//! # Ok::<(), std::io::Error>(())
//! ```

#![forbid(unsafe_code)]

#[cfg(all(not(test), not(coverage)))]
use std::io::IsTerminal;
#[cfg(all(not(test), not(coverage)))]
use std::path::{Path, PathBuf};
#[cfg(all(not(test), not(coverage)))]
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
#[cfg(all(not(test), not(coverage)))]
use std::time::Duration;

#[cfg(all(not(test), not(coverage)))]
use console_application::source_adapters::{
    ObservedSourceAdapter, ProbeNeedsAttentionPort, PullSourcePort, SourceProbe, SourceProbeOutcome,
};
#[cfg(all(not(test), not(coverage)))]
use console_application::{
    DispatcherFactoryDrainPort, DispatcherOrchestratorActionPort, DispatcherSettingsPort,
    DispatcherSettingsRead, JournalAutonomousDecisionsPort,
};
#[cfg(all(not(test), not(coverage)))]
use console_eventstore::SqliteEventStore;
#[cfg(all(not(test), not(coverage)))]
use livespec_console_beads_fabro::{
    BackingCliResolution, ConsoleRuntimeError, NeedsAttentionIngest, SourceAdapterRef,
    SourcePollRequester, TuiSessionRunner,
};

/// A message to the off-thread source poller: run a source poll now (on demand),
/// or stop.
#[cfg(all(not(test), not(coverage)))]
enum PollMessage {
    /// Re-poll the source adapters at once (sent right after a ledger-mutating
    /// operator effect so the ledger's lane change appears promptly).
    PollNow,
    /// Stop the poller and let it join.
    Shutdown,
}

/// How long the off-thread source poller waits between (slow, CLI-shelling)
/// source re-polls when no on-demand `PollNow` arrives. Short enough that
/// external ledger changes surface promptly; the UI thread never waits on it.
#[cfg(all(not(test), not(coverage)))]
const POLLER_CADENCE: Duration = Duration::from_secs(2);
#[cfg(all(not(test), not(coverage)))]
use time::OffsetDateTime;
#[cfg(all(not(test), not(coverage)))]
use time::format_description::well_known::Rfc3339;

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    #[cfg(all(not(test), not(coverage)))]
    {
        if should_run_interactive_tui(&args) && std::io::stdout().is_terminal() {
            match run_interactive_store_tui() {
                Ok(()) => {
                    std::process::exit(0);
                }
                Err(error) => {
                    eprintln!("tui error: {error}");
                    std::process::exit(1);
                }
            }
        }
        if should_run_store_backed_command(&args) {
            match run_store_backed_command(&args) {
                Ok(output) => {
                    println!("{}", output.message());
                    std::process::exit(output.code());
                }
                Err(error) => {
                    eprintln!("store command error: {error}");
                    std::process::exit(1);
                }
            }
        }
    }
    let output = livespec_console_beads_fabro::run(args);
    println!("{}", output.message());
    std::process::exit(output.code());
}

#[cfg(all(not(test), not(coverage)))]
fn should_run_interactive_tui(args: &[String]) -> bool {
    let command = args.get(1).map(String::as_str);
    let mode = args.get(2).map(String::as_str);
    matches!(command, Some("serve" | "tui")) && mode != Some("--preview")
}

#[cfg(all(not(test), not(coverage)))]
fn should_run_store_backed_command(args: &[String]) -> bool {
    let command = args.get(1).map(String::as_str);
    matches!(
        command,
        Some("serve" | "backfill" | "events" | "snapshot" | "doctor")
    )
}

#[cfg(all(not(test), not(coverage)))]
fn run_store_backed_command(
    args: &[String],
) -> Result<livespec_console_beads_fabro::RunOutput, String> {
    let path = console_store_path();
    create_store_parent(&path)?;
    let mut store = SqliteEventStore::open(&path).map_err(|error| format!("{error:?}"))?;
    let observed_at = current_requested_at()?;
    let probe = SystemSourceProbe;
    let repo = console_repo();
    let resolution = BackingCliResolution::from_environment().map_err(|error| error.to_string())?;
    let journal_path = resolution.dispatcher_journal_path();
    let adapters = livespec_console_beads_fabro::live_source_adapters_with_programs(
        &probe,
        &repo,
        resolution.programs(),
        &journal_path,
    )
    .map_err(|error| format!("{error:?}"))?;
    let sources = source_refs(&adapters);
    let needs_attention_port =
        ProbeNeedsAttentionPort::new(&probe, resolution.programs().needs_attention(), &["--json"]);
    let needs_attention = NeedsAttentionIngest::new(&needs_attention_port, &repo);
    let repo_path = resolution.drive_repo_arg();
    let mut drain = DispatcherFactoryDrainPort::new(
        &probe,
        resolution.programs().dispatcher(),
        &["loop", "--repo", repo_path.as_str()],
    );
    let mut drive = DispatcherOrchestratorActionPort::new(
        &probe,
        resolution.programs().drive(),
        &["--repo", repo_path.as_str(), "--json"],
    );
    let decisions = JournalAutonomousDecisionsPort::new(&probe, journal_path.as_str());
    Ok(livespec_console_beads_fabro::run_with_store(
        args,
        &mut store,
        &observed_at,
        &sources,
        &mut drain,
        &mut drive,
        &decisions,
        &needs_attention,
    ))
}

#[cfg(all(not(test), not(coverage)))]
fn run_interactive_store_tui() -> Result<(), String> {
    let path = console_store_path();
    create_store_parent(&path)?;
    let mut store = SqliteEventStore::open(&path).map_err(|error| format!("{error:?}"))?;
    let observed_at = current_requested_at()?;
    let probe = SystemSourceProbe;
    let repo = console_repo();
    let resolution = BackingCliResolution::from_environment().map_err(|error| error.to_string())?;
    let journal_path = resolution.dispatcher_journal_path();
    let adapters = livespec_console_beads_fabro::live_source_adapters_with_programs(
        &probe,
        &repo,
        resolution.programs(),
        &journal_path,
    )
    .map_err(|error| format!("{error:?}"))?;
    let sources = source_refs(&adapters);
    let needs_attention_port =
        ProbeNeedsAttentionPort::new(&probe, resolution.programs().needs_attention(), &["--json"]);
    let needs_attention = NeedsAttentionIngest::new(&needs_attention_port, &repo);
    let repo_path = resolution.drive_repo_arg();
    let mut drain = DispatcherFactoryDrainPort::new(
        &probe,
        resolution.programs().dispatcher(),
        &["loop", "--repo", repo_path.as_str()],
    );
    let mut drive = DispatcherOrchestratorActionPort::new(
        &probe,
        resolution.programs().drive(),
        &["--repo", repo_path.as_str(), "--json"],
    );
    let dispatcher_settings = DispatcherSettingsPort::new(&mut drive)
        .read_settings()
        .unwrap_or(DispatcherSettingsRead::NotObserved);
    let decisions = JournalAutonomousDecisionsPort::new(&probe, journal_path.as_str());
    let mut runner = InteractiveTuiRunner {
        selected_repo: repo.clone(),
        dispatcher_settings,
    };
    // Move the SLOW CLI-shelling source polls onto a background thread so the UI
    // thread never blocks on them (dropped keystrokes were the move-doesn't-land
    // symptom). The poller is fully self-contained — it re-resolves its own
    // adapters + probe and opens its OWN store connection (SqliteEventStore is
    // WAL, so a second connection is safe) — so nothing non-`Send` crosses the
    // thread boundary. The UI thread pings it (via `ChannelPollRequester`) after a
    // ledger-mutating effect, and the channel doubles as the shutdown signal.
    let (poll_tx, poll_rx) = std::sync::mpsc::channel::<PollMessage>();
    let poller = std::thread::spawn(move || poller_loop(&poll_rx));
    let requester = ChannelPollRequester {
        tx: poll_tx.clone(),
    };
    let session_result = livespec_console_beads_fabro::run_store_backed_tui_session(
        &mut store,
        &observed_at,
        "operator",
        &mut runner,
        &sources,
        &mut drain,
        &mut drive,
        &decisions,
        &needs_attention,
        &requester,
    );
    // Stop the poller (wake it if it is mid-`recv_timeout`) and join before
    // returning, so no source poll outlives the session.
    let _ = poll_tx.send(PollMessage::Shutdown);
    let _ = poller.join();
    session_result.map_err(|error| format!("{error:?}"))?;
    Ok(())
}

/// The background source poller: it owns its own store connection and adapters
/// (re-resolved from the environment) and runs the SLOW CLI-shelling source polls
/// ([`refresh_sources`]) on a cadence and on demand, appending to the store. The
/// UI thread never runs these polls, so its `event::poll`/`read` stays responsive
/// and keystrokes are never dropped. Terminal-adjacent + thread-bound, so
/// `#[cfg]`-excluded from tests; the polling logic it drives (`refresh_sources`)
/// is exercised directly.
#[cfg(all(not(test), not(coverage)))]
fn poller_loop(poll_rx: &Receiver<PollMessage>) {
    let Ok(resolution) = BackingCliResolution::from_environment() else {
        return;
    };
    let path = console_store_path();
    let Ok(mut store) = SqliteEventStore::open(&path) else {
        return;
    };
    let probe = SystemSourceProbe;
    let repo = console_repo();
    let journal_path = resolution.dispatcher_journal_path();
    let Ok(adapters) = livespec_console_beads_fabro::live_source_adapters_with_programs(
        &probe,
        &repo,
        resolution.programs(),
        &journal_path,
    ) else {
        return;
    };
    let sources = source_refs(&adapters);
    let needs_attention_port =
        ProbeNeedsAttentionPort::new(&probe, resolution.programs().needs_attention(), &["--json"]);
    let needs_attention = NeedsAttentionIngest::new(&needs_attention_port, &repo);
    loop {
        // A source poll failure (transient CLI/store hiccup) must NEVER crash the
        // poller — ignore it and try again next cycle.
        if let Ok(observed_at) = current_requested_at() {
            let _ = livespec_console_beads_fabro::refresh_sources(
                &mut store,
                &observed_at,
                &sources,
                &needs_attention,
            );
        }
        match poll_rx.recv_timeout(POLLER_CADENCE) {
            Ok(PollMessage::PollNow) | Err(RecvTimeoutError::Timeout) => {}
            Ok(PollMessage::Shutdown) | Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Backs [`SourcePollRequester`] with the channel to the poller thread: a
/// non-blocking `PollNow` send that is dropped if the poller has already stopped.
#[cfg(all(not(test), not(coverage)))]
struct ChannelPollRequester {
    tx: Sender<PollMessage>,
}

#[cfg(all(not(test), not(coverage)))]
impl SourcePollRequester for ChannelPollRequester {
    fn request_poll(&self) {
        let _ = self.tx.send(PollMessage::PollNow);
    }
}

#[cfg(all(not(test), not(coverage)))]
fn source_refs<'a>(
    adapters: &'a [(String, ObservedSourceAdapter<'a>)],
) -> Vec<SourceAdapterRef<'a>> {
    adapters
        .iter()
        .map(|(adapter_id, adapter)| (adapter_id.as_str(), adapter as &dyn PullSourcePort))
        .collect()
}

/// Host-backed probe: run a stable CLI or read a file. The honest source of all
/// live observations; unreachable sources degrade to not-observed findings.
#[cfg(all(not(test), not(coverage)))]
struct SystemSourceProbe;

#[cfg(all(not(test), not(coverage)))]
impl SourceProbe for SystemSourceProbe {
    fn run_command(&self, program: &str, args: &[&str]) -> SourceProbeOutcome {
        // Normalize `.py` backing CLIs through the Python interpreter so an
        // exec-bit inconsistency in the installed marketplace cache (Finding E:
        // needs_attention.py / drive.py ship non-executable) stops mattering.
        // Non-`.py` programs (env overrides, bare-name defaults) run directly.
        let (resolved_program, resolved_args) =
            livespec_console_beads_fabro::python_normalized_invocation(program, args);
        // Explicitly null the child's stdin so a shelled source CLI can never
        // steal the TUI's PTY stdin (belt-and-suspenders — `.output()` already
        // nulls stdin rather than inheriting it).
        match std::process::Command::new(resolved_program)
            .args(&resolved_args)
            .stdin(std::process::Stdio::null())
            .output()
        {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                SourceProbeOutcome::observed(&stdout, output.status.success())
            }
            Err(error) => SourceProbeOutcome::unavailable(&format!("{program}: {error}")),
        }
    }

    fn read_file(&self, path: &str) -> SourceProbeOutcome {
        match std::fs::read_to_string(path) {
            Ok(contents) => SourceProbeOutcome::observed(&contents, true),
            // An ABSENT expected file is observed-and-idle, not unreachable: a
            // factory that has not yet written its dispatch journal reads as an
            // empty observation, so the source is idle rather than cockpit-blind
            // (scenarios.md Scenario 13). A present-but-unreadable file (a real
            // permission or I/O fault) is genuinely unreachable.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                SourceProbeOutcome::observed("", true)
            }
            Err(error) => SourceProbeOutcome::unavailable(&format!("{path}: {error}")),
        }
    }
}

/// The observed tenant repo the cockpit is watching.
///
/// Derived from the process working directory's basename so it matches the
/// `source_ref.repo` the orchestrator's `needs-attention` surface composes (which
/// uses its own `project_root.name`); launched from the orchestrator cwd this
/// resolves to the true observed tenant instead of the console's own name. The
/// `LIVESPEC_CONSOLE_REPO` override still wins. See
/// [`livespec_console_beads_fabro::resolve_console_repo`].
#[cfg(all(not(test), not(coverage)))]
fn console_repo() -> String {
    let env_override = std::env::var("LIVESPEC_CONSOLE_REPO").ok();
    let current_dir = std::env::current_dir().ok();
    livespec_console_beads_fabro::resolve_console_repo(
        env_override.as_deref(),
        current_dir.as_deref(),
    )
}

#[cfg(all(not(test), not(coverage)))]
struct InteractiveTuiRunner {
    selected_repo: String,
    dispatcher_settings: DispatcherSettingsRead,
}

#[cfg(all(not(test), not(coverage)))]
impl TuiSessionRunner for InteractiveTuiRunner {
    fn run_tui(
        &mut self,
        events: &[console_domain::ConsoleEvent],
        requested_by: &str,
        session: &mut dyn console_tui::TuiLiveSession,
    ) -> Result<Vec<console_tui::TuiRuntimeEffect>, ConsoleRuntimeError> {
        console_tui::run_interactive_tui_with_effect_sink(
            events,
            requested_by,
            &self.selected_repo,
            self.dispatcher_settings.clone(),
            session,
        )
        .map_err(|_error| ConsoleRuntimeError::TuiRuntimeFailed)
    }
}

#[cfg(all(not(test), not(coverage)))]
fn console_store_path() -> PathBuf {
    std::env::var_os("LIVESPEC_CONSOLE_STORE_PATH").map_or_else(
        || PathBuf::from("tmp/livespec-console.sqlite"),
        PathBuf::from,
    )
}

#[cfg(all(not(test), not(coverage)))]
fn create_store_parent(path: &Path) -> Result<(), String> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    if parent.as_os_str().is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())
}

#[cfg(all(not(test), not(coverage)))]
fn current_requested_at() -> Result<String, String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| error.to_string())
}
