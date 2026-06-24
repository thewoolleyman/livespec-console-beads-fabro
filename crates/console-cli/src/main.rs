#![forbid(unsafe_code)]

#[cfg(all(not(test), not(coverage)))]
use std::io::IsTerminal;
#[cfg(all(not(test), not(coverage)))]
use std::path::{Path, PathBuf};

#[cfg(all(not(test), not(coverage)))]
use console_domain::ConsoleEvent;
#[cfg(all(not(test), not(coverage)))]
use console_eventstore::SqliteEventStore;
#[cfg(all(not(test), not(coverage)))]
use console_tui::TuiRuntimeEffect;
#[cfg(all(not(test), not(coverage)))]
use time::OffsetDateTime;
#[cfg(all(not(test), not(coverage)))]
use time::format_description::well_known::Rfc3339;

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    #[cfg(all(not(test), not(coverage)))]
    {
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
        if should_run_interactive_tui(&args) && std::io::stdout().is_terminal() {
            let events = match load_interactive_tui_events() {
                Ok(events) => events,
                Err(error) => {
                    eprintln!("tui source error: {error}");
                    std::process::exit(1);
                }
            };
            match console_tui::run_interactive_tui(&events, "operator") {
                Ok(effects) => match persist_tui_effects(&effects) {
                    Ok(()) => {
                        std::process::exit(0);
                    }
                    Err(error) => {
                        eprintln!("tui persistence error: {error}");
                        std::process::exit(1);
                    }
                },
                Err(error) => {
                    eprintln!("tui error: {error}");
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
    command == Some("tui") && mode != Some("--preview")
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
fn load_interactive_tui_events() -> Result<Vec<ConsoleEvent>, String> {
    let path = console_store_path();
    if !path.exists() {
        return Ok(livespec_console_beads_fabro::demo_events().to_vec());
    }
    let store = SqliteEventStore::open(&path).map_err(|error| format!("{error:?}"))?;
    let events = livespec_console_beads_fabro::load_tui_events_from_store(&store)
        .map_err(|error| format!("{error:?}"))?;
    if events.is_empty() {
        return Ok(livespec_console_beads_fabro::demo_events().to_vec());
    }
    Ok(events)
}

#[cfg(all(not(test), not(coverage)))]
fn persist_tui_effects(effects: &[TuiRuntimeEffect]) -> Result<(), String> {
    if !effects
        .iter()
        .any(|effect| matches!(effect, TuiRuntimeEffect::PersistCommand(_)))
    {
        return Ok(());
    }
    let path = console_store_path();
    create_store_parent(&path)?;
    let mut store = SqliteEventStore::open(&path).map_err(|error| format!("{error:?}"))?;
    let requested_at = current_requested_at()?;
    livespec_console_beads_fabro::persist_tui_runtime_effects(&mut store, effects, &requested_at)
        .map_err(|error| format!("{error:?}"))?;
    let mut factory_port = livespec_console_beads_fabro::SimulatedFactoryDrainPort;
    livespec_console_beads_fabro::handle_pending_factory_commands(
        &mut store,
        &requested_at,
        &mut factory_port,
    )
    .map_err(|error| format!("{error:?}"))?;
    Ok(())
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
