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
    DispatcherFactoryDispatchItemPort, DispatcherFactoryDrainPort,
    DispatcherOrchestratorActionPort, DispatcherSettingsPort, DispatcherSettingsRead,
    JournalAutonomousDecisionsPort, PluginResolution as TuiPluginResolution,
};
#[cfg(all(not(test), not(coverage)))]
use console_eventstore::{
    STORE_OPEN_ATTEMPTS, SqliteEventStore, open_retry_backoff, open_tolerating_contention,
    render_open_failure,
};
#[cfg(all(not(test), not(coverage)))]
use livespec_console_beads_fabro::{
    BackingCliResolution, ConsoleLane, ConsoleRuntimeError, LaneStartupStage, NeedsAttentionIngest,
    PendingCommandRequester, SourceAdapterRef, SourcePollRequester, TuiSessionRunner,
    append_lane_diagnostic, lane_diagnostics_path, lane_open_failure_line,
    lane_startup_failure_line, resolve_console_invoker,
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

/// A message to the off-thread pending-command worker.
#[cfg(all(not(test), not(coverage)))]
enum CommandMessage {
    /// Claim and handle pending command rows at once.
    HandleNow,
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
            match run_interactive_store_tui(&args) {
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
        Some("serve" | "backfill" | "events" | "snapshot" | "doctor" | "plans")
    )
}

#[cfg(all(not(test), not(coverage)))]
fn run_store_backed_command(
    args: &[String],
) -> Result<livespec_console_beads_fabro::RunOutput, String> {
    let path = console_store_path();
    create_store_parent(&path)?;
    let mut store = open_console_store(&path)?;
    let observed_at = current_requested_at()?;
    let repo = console_repo();
    let resolution = BackingCliResolution::from_environment().map_err(|error| error.to_string())?;
    let probe = SystemSourceProbe::new(resolution.selected_repo_path());
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
    let mut dispatch_item = DispatcherFactoryDispatchItemPort::new(
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
    let filtered_args = strip_invoker_args(args);
    Ok(
        livespec_console_beads_fabro::run_with_store_and_dispatch_port(
            &filtered_args,
            &mut store,
            &observed_at,
            &sources,
            &mut drain,
            &mut dispatch_item,
            &mut drive,
            &decisions,
            &needs_attention,
        ),
    )
}

/// Report a lane step that failed and cannot usefully be retried, then give up.
///
/// The companion to `open_lane_store`, and deliberately a DIFFERENT remedy. The
/// store open races, so it is retried; these steps read configuration, the
/// environment and the clock, where a failure is deterministic and a retry wins
/// nothing. What they share is the part that was missing: a lane that gives up
/// must say so.
///
/// This exists because the store-open report alone made the lane log a
/// REASSURING signal wired to one of seven failure paths. An empty log read as
/// "the lanes are fine" while six ways of dying still wrote nothing — and a
/// guard that manufactures confidence is worse than no guard.
#[cfg(all(not(test), not(coverage)))]
fn report_lane_startup_failure(lane: ConsoleLane, stage: LaneStartupStage, detail: &str) {
    let path = console_store_path();
    let at = current_requested_at().unwrap_or_else(|_error| "unknown-time".to_owned());
    let report = lane_startup_failure_line(lane, stage, detail, &at);
    let _ = append_lane_diagnostic(&lane_diagnostics_path(&path), &report);
}

/// Open the store for an off-thread LANE, tolerating transient contention and
/// reporting an exhausted open instead of vanishing.
///
/// livespec-console-beads-fabro-k9vt2m. All three lanes previously wrote
/// `let Ok(mut store) = SqliteEventStore::open(&path) else { return; }` — a
/// failed open produced no store, no message, and no thread. The poller case is
/// the sharp one: it is spawned ONCE, so a lost race left the session silently
/// non-refreshing for its whole life, rendering a stale view that looks entirely
/// normal.
///
/// RETRY IS ADOPTED HERE, and the choice is made against both prior decisions
/// rather than copied from either:
///
/// - `bss4rq` retries at startup because an open is idempotent and there is no
///   rendered surface yet. The idempotence half transfers exactly — this is the
///   same `SqliteEventStore::open`, and re-running it cannot double-apply
///   anything. The surface half does NOT: the session around these lanes is live
///   and rendering.
/// - `ddfbcx.1` refuses to retry in the running loop because there IS a rendered
///   surface and an operator keystroke to inherit as the retry. Neither holds
///   for a lane: an operator cannot re-trigger the poller, and a command lane's
///   invocation is already spent.
///
/// So these lanes get BOTH — a bounded retry, because nothing else will retry
/// for them, and a report on exhaustion, because a lane that gives up silently
/// is the defect itself. The report is a sibling file rather than stderr:
/// stderr during a live TUI would scribble on the alternate screen.
///
/// A failure to write the report is itself swallowed, and that is deliberate —
/// there is no further surface to escalate to, and a lane must not die trying to
/// announce that it is dying.
#[cfg(all(not(test), not(coverage)))]
fn open_lane_store(lane: ConsoleLane, path: &Path) -> Option<SqliteEventStore> {
    let opened = open_tolerating_contention(
        STORE_OPEN_ATTEMPTS,
        &mut || SqliteEventStore::open(path),
        &mut |attempt| std::thread::sleep(open_retry_backoff(attempt)),
    );
    match opened {
        Ok(store) => Some(store),
        Err(error) => {
            let at = current_requested_at().unwrap_or_else(|_error| "unknown-time".to_owned());
            let report = lane_open_failure_line(lane, STORE_OPEN_ATTEMPTS, &error, &at);
            let _ = append_lane_diagnostic(&lane_diagnostics_path(path), &report);
            None
        }
    }
}

/// Open the console store for a STARTUP sequence, tolerating transient contention.
///
/// livespec-console-beads-fabro-bss4rq. Every open also runs
/// `execute_batch(SCHEMA)`, which is itself a write transaction, and one
/// walkthrough opens EIGHT connections across SIX threads per console process
/// (see the writer-count note in `run_interactive_store_tui`). A bare `?` here
/// therefore killed the session on a momentary `SQLITE_BUSY` — before the first
/// frame, so the operator saw nothing at all.
///
/// The loop, the bound, and the rendered failure live in `console_eventstore`
/// where they are tested against scripted failures; this composition root holds
/// only the two effects that cannot be: the real open and the real sleep.
#[cfg(all(not(test), not(coverage)))]
fn open_console_store(path: &Path) -> Result<SqliteEventStore, String> {
    open_tolerating_contention(
        STORE_OPEN_ATTEMPTS,
        &mut || SqliteEventStore::open(path),
        &mut |attempt| std::thread::sleep(open_retry_backoff(attempt)),
    )
    .map_err(|error| render_open_failure(&error, STORE_OPEN_ATTEMPTS))
}

#[cfg(all(not(test), not(coverage)))]
fn run_interactive_store_tui(args: &[String]) -> Result<(), String> {
    let path = console_store_path();
    create_store_parent(&path)?;
    let mut store = open_console_store(&path)?;
    let observed_at = current_requested_at()?;
    let repo = console_repo();
    let resolution = BackingCliResolution::from_environment().map_err(|error| error.to_string())?;
    let probe = SystemSourceProbe::new(resolution.selected_repo_path());
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
    let invoker = console_invoker(args);
    let mut runner = InteractiveTuiRunner {
        selected_repo: repo.clone(),
        dispatcher_settings,
        plugin_resolution: plugin_resolution_for_tui(resolution.plugin_resolution()),
    };
    // Move the SLOW CLI-shelling source polls onto a background thread so the UI
    // thread never blocks on them (dropped keystrokes were the move-doesn't-land
    // symptom). The poller is fully self-contained — it re-resolves its own
    // adapters + probe and opens its OWN store connection (SqliteEventStore is
    // WAL, so a second connection is safe) — so nothing non-`Send` crosses the
    // thread boundary.
    //
    // MEASURED WRITER COUNT, because an earlier version of this comment said
    // "TWO writers" and that understates it fourfold: one walkthrough opens
    // EIGHT store connections across SIX threads per console process. The UI
    // thread and this poller are only two of them — the control-command lane
    // (`handle_pending_control_command_lane`) and the factory lane
    // (`handle_pending_factory_command_lane`, spawned and deliberately unjoined)
    // each open a FRESH `SqliteEventStore` per invocation, and every open also
    // runs `execute_batch(SCHEMA)`, which is itself a write transaction.
    //
    // That is why a transient `SQLITE_BUSY` is a reachable outcome rather than a
    // theoretical one, and why the effect sink tolerates it instead of letting
    // it end the session (livespec-console-beads-fabro-ddfbcx.1). The UI thread pings it (via `ChannelPollRequester`) after a
    // ledger-mutating effect, and the channel doubles as the shutdown signal.
    let (poll_tx, poll_rx) = std::sync::mpsc::channel::<PollMessage>();
    let poller = std::thread::spawn(move || poller_loop(&poll_rx));
    let requester = ChannelPollRequester {
        tx: poll_tx.clone(),
    };
    let (command_tx, command_rx) = std::sync::mpsc::channel::<CommandMessage>();
    let command_worker = std::thread::spawn(move || command_worker_loop(&command_rx));
    let command_requester = ChannelCommandRequester {
        tx: command_tx.clone(),
    };
    let session_result = livespec_console_beads_fabro::run_store_backed_tui_session(
        &mut store,
        &observed_at,
        invoker.principal(),
        &mut runner,
        &sources,
        &mut drain,
        &mut drive,
        &decisions,
        &needs_attention,
        &requester,
        &command_requester,
    );
    // Stop the poller (wake it if it is mid-`recv_timeout`) and join before
    // returning, so no source poll outlives the session.
    let _ = poll_tx.send(PollMessage::Shutdown);
    let _ = poller.join();
    drop(command_tx);
    if command_worker.is_finished() {
        let _ = command_worker.join();
    } else {
        // Do not join an in-flight mutating command: a factory drain may be
        // running for hours, and graceful cockpit quit must not wait on it.
    }
    let outcome = session_result.map_err(|error| format!("{error:?}"))?;
    // A shutdown-time lock convoy DEGRADES rather than killing the process
    // (livespec-console-beads-fabro-aidncj): the session and the operator's
    // actions already completed, so the epilogue reports and the process still
    // exits 0. stderr is the right surface by now — `run_tui` has left the
    // alternate screen and restored the terminal, so this cannot scribble on a
    // live TUI the way an in-session `eprintln!` would.
    if let Some(warning) = outcome.store_warning() {
        eprintln!("tui warning: {warning}");
    }
    Ok(())
}

#[cfg(all(not(test), not(coverage)))]
fn console_invoker(args: &[String]) -> livespec_console_beads_fabro::ConsoleInvokerResolution {
    let env = std::env::vars().collect::<std::collections::BTreeMap<_, _>>();
    resolve_console_invoker(args, &env, &os_user(), &hostname())
}

#[cfg(all(not(test), not(coverage)))]
fn strip_invoker_args(args: &[String]) -> Vec<String> {
    let mut filtered = Vec::with_capacity(args.len());
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--invoker" {
            index += 2;
            continue;
        }
        filtered.push(args[index].clone());
        index += 1;
    }
    filtered
}

#[cfg(all(not(test), not(coverage)))]
fn os_user() -> String {
    std::env::var("USER")
        .or_else(|_error| std::env::var("LOGNAME"))
        .unwrap_or_else(|_error| "unknown".to_owned())
}

#[cfg(all(not(test), not(coverage)))]
fn hostname() -> String {
    std::env::var("HOSTNAME").unwrap_or_else(|_error| "unknown".to_owned())
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
    let resolution = match BackingCliResolution::from_environment() {
        Ok(resolution) => resolution,
        Err(error) => {
            report_lane_startup_failure(
                ConsoleLane::SourcePoller,
                LaneStartupStage::BackingCliResolution,
                &error.to_string(),
            );
            return;
        }
    };
    let path = console_store_path();
    let Some(mut store) = open_lane_store(ConsoleLane::SourcePoller, &path) else {
        return;
    };
    let repo = console_repo();
    let probe = SystemSourceProbe::new(resolution.selected_repo_path());
    let journal_path = resolution.dispatcher_journal_path();
    let adapters = match livespec_console_beads_fabro::live_source_adapters_with_programs(
        &probe,
        &repo,
        resolution.programs(),
        &journal_path,
    ) {
        Ok(adapters) => adapters,
        Err(error) => {
            report_lane_startup_failure(
                ConsoleLane::SourcePoller,
                LaneStartupStage::SourceAdapters,
                &format!("{error:?}"),
            );
            return;
        }
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

/// Backs [`PendingCommandRequester`] with the channel to the command worker.
#[cfg(all(not(test), not(coverage)))]
struct ChannelCommandRequester {
    tx: Sender<CommandMessage>,
}

#[cfg(all(not(test), not(coverage)))]
impl PendingCommandRequester for ChannelCommandRequester {
    fn request_pending_command_handling(&self) {
        let _ = self.tx.send(CommandMessage::HandleNow);
    }
}

#[cfg(all(not(test), not(coverage)))]
fn command_worker_loop(command_rx: &Receiver<CommandMessage>) {
    while matches!(command_rx.recv(), Ok(CommandMessage::HandleNow)) {
        spawn_factory_command_worker();
        handle_pending_control_command_lane();
    }
}

#[cfg(all(not(test), not(coverage)))]
fn spawn_factory_command_worker() {
    let _ = std::thread::spawn(handle_pending_factory_command_lane);
}

#[cfg(all(not(test), not(coverage)))]
fn handle_pending_factory_command_lane() {
    let resolution = match BackingCliResolution::from_environment() {
        Ok(resolution) => resolution,
        Err(error) => {
            report_lane_startup_failure(
                ConsoleLane::FactoryCommand,
                LaneStartupStage::BackingCliResolution,
                &error.to_string(),
            );
            return;
        }
    };
    let path = console_store_path();
    let Some(mut store) = open_lane_store(ConsoleLane::FactoryCommand, &path) else {
        return;
    };
    let observed_at = match current_requested_at() {
        Ok(observed_at) => observed_at,
        Err(error) => {
            report_lane_startup_failure(
                ConsoleLane::FactoryCommand,
                LaneStartupStage::ObservationClock,
                &error,
            );
            return;
        }
    };
    let repo_path = resolution.drive_repo_arg();
    let probe = SystemSourceProbe::new(resolution.selected_repo_path());
    let mut drain = DispatcherFactoryDrainPort::new(
        &probe,
        resolution.programs().dispatcher(),
        &["loop", "--repo", repo_path.as_str()],
    );
    let mut dispatch_item = DispatcherFactoryDispatchItemPort::new(
        &probe,
        resolution.programs().dispatcher(),
        &["loop", "--repo", repo_path.as_str()],
    );
    let mut drive = DispatcherOrchestratorActionPort::new(
        &probe,
        resolution.programs().drive(),
        &["--repo", repo_path.as_str(), "--json"],
    );
    let _ = livespec_console_beads_fabro::handle_pending_factory_commands_with_dispatch_port(
        &mut store,
        &observed_at,
        &mut drain,
        &mut dispatch_item,
    );
    let _ = livespec_console_beads_fabro::handle_pending_control_commands(
        &mut store,
        &observed_at,
        &mut drive,
    );
}

#[cfg(all(not(test), not(coverage)))]
fn handle_pending_control_command_lane() {
    let resolution = match BackingCliResolution::from_environment() {
        Ok(resolution) => resolution,
        Err(error) => {
            report_lane_startup_failure(
                ConsoleLane::ControlCommand,
                LaneStartupStage::BackingCliResolution,
                &error.to_string(),
            );
            return;
        }
    };
    let path = console_store_path();
    let Some(mut store) = open_lane_store(ConsoleLane::ControlCommand, &path) else {
        return;
    };
    let observed_at = match current_requested_at() {
        Ok(observed_at) => observed_at,
        Err(error) => {
            report_lane_startup_failure(
                ConsoleLane::ControlCommand,
                LaneStartupStage::ObservationClock,
                &error,
            );
            return;
        }
    };
    let repo_path = resolution.drive_repo_arg();
    let probe = SystemSourceProbe::new(resolution.selected_repo_path());
    let mut drive = DispatcherOrchestratorActionPort::new(
        &probe,
        resolution.programs().drive(),
        &["--repo", repo_path.as_str(), "--json"],
    );
    let _ = livespec_console_beads_fabro::handle_pending_control_commands(
        &mut store,
        &observed_at,
        &mut drive,
    );
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
struct SystemSourceProbe {
    cwd: PathBuf,
}

#[cfg(all(not(test), not(coverage)))]
impl SystemSourceProbe {
    fn new(cwd: &Path) -> Self {
        Self {
            cwd: cwd.to_path_buf(),
        }
    }
}

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
            .current_dir(&self.cwd)
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
    plugin_resolution: TuiPluginResolution,
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
            self.plugin_resolution.clone(),
            session,
        )
        .map_err(ConsoleRuntimeError::tui_runtime_io_failed)
    }
}

#[cfg(all(not(test), not(coverage)))]
fn plugin_resolution_for_tui(
    resolution: &livespec_console_beads_fabro::PluginResolution,
) -> TuiPluginResolution {
    TuiPluginResolution::resolved(
        resolution.source().to_owned(),
        resolution.root().map_or_else(
            || "not resolved".to_owned(),
            |root| root.display().to_string(),
        ),
        resolution.version().map(str::to_owned),
    )
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
