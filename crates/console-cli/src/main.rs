#![forbid(unsafe_code)]

#[cfg(all(not(test), not(coverage)))]
use std::io::IsTerminal;
#[cfg(all(not(test), not(coverage)))]
use std::path::{Path, PathBuf};

#[cfg(all(not(test), not(coverage)))]
use console_eventstore::SqliteEventStore;
#[cfg(all(not(test), not(coverage)))]
use livespec_console_beads_fabro::{ConsoleRuntimeError, TuiSessionRunner};
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
    Ok(livespec_console_beads_fabro::run_with_store(
        args,
        &mut store,
        &observed_at,
    ))
}

#[cfg(all(not(test), not(coverage)))]
fn run_interactive_store_tui() -> Result<(), String> {
    let path = console_store_path();
    create_store_parent(&path)?;
    let mut store = SqliteEventStore::open(&path).map_err(|error| format!("{error:?}"))?;
    let observed_at = current_requested_at()?;
    let mut runner = InteractiveTuiRunner;
    let mut factory_port = livespec_console_beads_fabro::SimulatedFactoryDrainPort;
    livespec_console_beads_fabro::run_store_backed_tui_session(
        &mut store,
        &observed_at,
        "operator",
        &mut runner,
        &mut factory_port,
    )
    .map_err(|error| format!("{error:?}"))?;
    Ok(())
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
