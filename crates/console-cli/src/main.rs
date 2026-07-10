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
use console_application::source_adapters::{
    ObservedSourceAdapter, ProbeNeedsAttentionPort, PullSourcePort, SourceProbe, SourceProbeOutcome,
};
#[cfg(all(not(test), not(coverage)))]
use console_application::{DispatcherFactoryDrainPort, DispatcherOrchestratorActionPort};
#[cfg(all(not(test), not(coverage)))]
use console_eventstore::SqliteEventStore;
#[cfg(all(not(test), not(coverage)))]
use livespec_console_beads_fabro::{
    ConsoleRuntimeError, NeedsAttentionIngest, SourceAdapterRef, TuiSessionRunner,
};
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
    let adapters = livespec_console_beads_fabro::live_source_adapters(&probe, &repo)
        .map_err(|error| format!("{error:?}"))?;
    let sources = source_refs(&adapters);
    let needs_attention_port =
        ProbeNeedsAttentionPort::new(&probe, &needs_attention_program(), &["--json"]);
    let needs_attention = NeedsAttentionIngest::new(&needs_attention_port, &repo);
    let drain_program = drain_program();
    let mut drain = DispatcherFactoryDrainPort::new(&probe, &drain_program, &["drain"]);
    let drive_program = drive_program();
    let mut drive =
        DispatcherOrchestratorActionPort::new(&probe, &drive_program, &["--repo", repo.as_str()]);
    Ok(livespec_console_beads_fabro::run_with_store(
        args,
        &mut store,
        &observed_at,
        &sources,
        &mut drain,
        &mut drive,
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
    let adapters = livespec_console_beads_fabro::live_source_adapters(&probe, &repo)
        .map_err(|error| format!("{error:?}"))?;
    let sources = source_refs(&adapters);
    let needs_attention_port =
        ProbeNeedsAttentionPort::new(&probe, &needs_attention_program(), &["--json"]);
    let needs_attention = NeedsAttentionIngest::new(&needs_attention_port, &repo);
    let drain_program = drain_program();
    let mut drain = DispatcherFactoryDrainPort::new(&probe, &drain_program, &["drain"]);
    let drive_program = drive_program();
    let mut drive =
        DispatcherOrchestratorActionPort::new(&probe, &drive_program, &["--repo", repo.as_str()]);
    let mut runner = InteractiveTuiRunner;
    livespec_console_beads_fabro::run_store_backed_tui_session(
        &mut store,
        &observed_at,
        "operator",
        &mut runner,
        &sources,
        &mut drain,
        &mut drive,
        &needs_attention,
    )
    .map_err(|error| format!("{error:?}"))?;
    Ok(())
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
        match std::process::Command::new(program).args(args).output() {
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
            Err(error) => SourceProbeOutcome::unavailable(&format!("{path}: {error}")),
        }
    }
}

#[cfg(all(not(test), not(coverage)))]
fn console_repo() -> String {
    std::env::var("LIVESPEC_CONSOLE_REPO")
        .unwrap_or_else(|_error| "livespec-console-beads-fabro".to_owned())
}

#[cfg(all(not(test), not(coverage)))]
fn drain_program() -> String {
    std::env::var("LIVESPEC_CONSOLE_DRAIN_PROGRAM")
        .unwrap_or_else(|_error| "livespec-dispatcher-drain".to_owned())
}

#[cfg(all(not(test), not(coverage)))]
fn drive_program() -> String {
    std::env::var("LIVESPEC_CONSOLE_DRIVE_PROGRAM")
        .unwrap_or_else(|_error| "livespec-orchestrator-drive".to_owned())
}

#[cfg(all(not(test), not(coverage)))]
fn needs_attention_program() -> String {
    std::env::var("LIVESPEC_CONSOLE_NEEDS_ATTENTION_PROGRAM")
        .unwrap_or_else(|_error| "needs-attention".to_owned())
}

#[cfg(all(not(test), not(coverage)))]
struct InteractiveTuiRunner;

#[cfg(all(not(test), not(coverage)))]
impl TuiSessionRunner for InteractiveTuiRunner {
    fn run_tui(
        &mut self,
        events: &[console_domain::ConsoleEvent],
        requested_by: &str,
    ) -> Result<Vec<console_tui::TuiRuntimeEffect>, ConsoleRuntimeError> {
        console_tui::run_interactive_tui(events, requested_by)
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
