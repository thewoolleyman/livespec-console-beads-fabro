//! Scenario 9 -- Operator sets a dispatcher policy setting from the console
//! (SPECIFICATION/scenarios.md). The console holds no dispatcher-setting state of
//! its own: a setting change is a single per-setting write that MUST be effected
//! THROUGH the orchestrator's published command surface (`drive.py --action
//! set-config:<key>:<value>`) and MUST append the audit fact -- rather than the
//! console writing the orchestrator's `.livespec.jsonc` itself.
//!
//! This drives the console command pump end to end over the REAL
//! `DispatcherOrchestratorActionPort` wired to a recording `SourceProbe`, so the
//! full path is exercised: the stored `config.autonomous_mode_set` command (the
//! transitional bridge onto the six-setting surface, retired with the arming
//! surface by the Settings surface) is handled, the orchestrator `set-config`
//! action is observed issued through `drive.py`, the audit event lands in the
//! event store, and the probe observes NO `.livespec.jsonc` write. An unconfirmed
//! dangerous enable is still rejected with no effect in this transitional slice.

use std::cell::RefCell;

use console_application::DispatcherOrchestratorActionPort;
use console_application::source_adapters::{SourceProbe, SourceProbeOutcome};
use console_domain::{CommandEnvelope, CommandType, EventType};
use console_eventstore::{CommandAppend, SqliteEventStore};
use livespec_console_beads_fabro::{ConsoleRuntimeError, handle_pending_config_commands};

/// A read-only `SourceProbe` that returns a scripted `run_command` outcome and
/// records every command argv. It stands in for a live `drive.py`, so the
/// settings write is driven end to end; the probe (like the shipped one) exposes
/// no write capability, so the console cannot write a file even in principle.
struct RecordingDriveProbe {
    run_outcome: SourceProbeOutcome,
    run_calls: RefCell<Vec<Vec<String>>>,
}

impl RecordingDriveProbe {
    const fn returning(run_outcome: SourceProbeOutcome) -> Self {
        Self {
            run_outcome,
            run_calls: RefCell::new(Vec::new()),
        }
    }
}

impl SourceProbe for RecordingDriveProbe {
    fn run_command(&self, program: &str, args: &[&str]) -> SourceProbeOutcome {
        let mut call = vec![program.to_owned()];
        call.extend(args.iter().map(|arg| (*arg).to_owned()));
        self.run_calls.borrow_mut().push(call);
        self.run_outcome.clone()
    }

    fn read_file(&self, _path: &str) -> SourceProbeOutcome {
        SourceProbeOutcome::unavailable("read_file unused by the settings port")
    }
}

fn autonomous_mode_set_command(payload_json: &str) -> CommandAppend {
    CommandAppend::new(
        CommandEnvelope::new(
            "cmd_autonomous".to_owned(),
            CommandType::ConfigAutonomousModeSet,
            "livespec-console-beads-fabro".to_owned(),
            "livespec-console-beads-fabro:config.autonomous_mode_set".to_owned(),
            "operator".to_owned(),
        ),
        "2026-07-11T00:00:00Z".to_owned(),
        Some("livespec-console-beads-fabro".to_owned()),
        "corr_cmd_autonomous".to_owned(),
        payload_json.to_owned(),
    )
}

/// Setting a dispatcher policy is commanded THROUGH the orchestrator API: a
/// confirmed `config.autonomous_mode_set` completes, issues the orchestrator's
/// published `set-config` action through `drive.py`, appends the audit event, and
/// writes NO `.livespec.jsonc` on disk.
#[test]
fn scenario_9_setting_a_dispatcher_policy_is_commanded_through_the_orchestrator_api()
-> Result<(), ConsoleRuntimeError> {
    let mut store = SqliteEventStore::open_in_memory()?;

    // Given a registered repo. When the operator submits a confirmed setting
    // change through the console command pump.
    store.append_command(&autonomous_mode_set_command(
        r#"{"repo":"livespec-console-beads-fabro","enabled":true,"confirmed":true}"#,
    ))?;
    let probe = RecordingDriveProbe::returning(SourceProbeOutcome::observed("{}", true));
    let mut drive =
        DispatcherOrchestratorActionPort::new(&probe, "drive.py", &["--repo", "/orch", "--json"]);
    let outcomes = handle_pending_config_commands(&mut store, "2026-07-11T00:00:01Z", &mut drive)?;

    // Then the command completes.
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].command_status(), "completed");

    // And the setting was effected through the orchestrator's published
    // `set-config` action -- never by the console writing `.livespec.jsonc`.
    assert_eq!(
        probe.run_calls.borrow().as_slice(),
        [vec![
            "drive.py".to_owned(),
            "--repo".to_owned(),
            "/orch".to_owned(),
            "--json".to_owned(),
            "--action".to_owned(),
            "set-config:auto_approve_ready:true".to_owned(),
        ]]
    );

    // And the durable audit event lands in the configuration context.
    let events = store.list_console_events()?;
    let audit = events
        .iter()
        .find(|event| event.event_type() == &EventType::ConfigAutonomousModeEnabled);
    assert!(matches!(audit, Some(event) if event.context() == "configuration"));
    Ok(())
}

/// The transitional dangerous-enable guard: an unconfirmed enable is rejected
/// with no effect -- no orchestrator action issued, no `.livespec.jsonc` write,
/// and no audit event.
#[test]
fn scenario_9_an_unconfirmed_enable_is_rejected_with_no_effect() -> Result<(), ConsoleRuntimeError>
{
    let mut store = SqliteEventStore::open_in_memory()?;

    store.append_command(&autonomous_mode_set_command(
        r#"{"repo":"livespec-console-beads-fabro","enabled":true,"confirmed":false}"#,
    ))?;
    let probe = RecordingDriveProbe::returning(SourceProbeOutcome::observed("{}", true));
    let mut drive =
        DispatcherOrchestratorActionPort::new(&probe, "drive.py", &["--repo", "/orch", "--json"]);
    let outcomes = handle_pending_config_commands(&mut store, "2026-07-11T00:00:01Z", &mut drive)?;

    // Then the command is rejected with no effect.
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].command_status(), "rejected");

    // And no orchestrator action was issued.
    assert!(probe.run_calls.borrow().is_empty());

    // And no audit event is appended.
    let events = store.list_console_events()?;
    assert!(
        !events
            .iter()
            .any(|event| event.event_type() == &EventType::ConfigAutonomousModeEnabled)
    );
    Ok(())
}
