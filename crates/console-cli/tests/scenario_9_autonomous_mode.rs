//! Scenario 9 -- Enabling full autonomous mode is guarded and audited
//! (SPECIFICATION/scenarios.md). Autonomous mode defaults to disabled; enabling
//! it MUST be confirmed, MUST arm the orchestrator plane's single permission key
//! (`livespec-orchestrator-beads-fabro.dispatcher.autonomous_mode`) through that
//! plane's published arming surface, and MUST append a durable
//! `config.autonomous_mode.enabled` audit event -- while an unconfirmed enable is
//! rejected with no effect (no key write, no audit event).
//!
//! Both gherkin cases drive the real `LivespecJsoncArmingPort` against an
//! in-memory `.livespec.jsonc` document so the guarded-enable path is exercised
//! end to end: the audit event and the issued factory command are observed in the
//! event store, and the orchestrator permission key is observed actually written
//! (or, for the rejection, observed unchanged). The type-to-confirm modal itself
//! is the TUI surface (a later slice); this scenario covers the command,
//! key-write, and audit contract the modal ultimately submits.

use std::cell::RefCell;

use console_application::source_adapters::{SourceProbe, SourceProbeOutcome};
use console_application::{LivespecJsoncArmingPort, read_autonomous_mode_from_jsonc};
use console_domain::{CommandEnvelope, CommandType, EventType};
use console_eventstore::{CommandAppend, SqliteEventStore};
use livespec_console_beads_fabro::{ConsoleRuntimeError, handle_pending_config_commands};

/// A `.livespec.jsonc` for a registered repo whose autonomous mode is disabled
/// by default: the orchestrator block carries no `dispatcher.autonomous_mode`
/// key, and comments and other members must survive an arming write.
const CONFIG_DISABLED_BY_DEFAULT: &str = r#"{
  "template": "livespec-with-diagrams",
  // The orchestrator block, with no dispatcher.autonomous_mode key yet.
  "livespec-orchestrator-beads-fabro": {
    "format": "beads",
    "connection": { "tenant": "livespec-console-beads-fabro" }
  }
}"#;

const SCRATCH_PATH: &str = "/scratch/.livespec.jsonc";

/// A `SourceProbe` backed by an in-memory `.livespec.jsonc` document: reads
/// return the current contents and writes replace them, so the real arming port
/// is driven end to end without touching the filesystem.
struct InMemoryConfigProbe {
    contents: RefCell<String>,
}

impl InMemoryConfigProbe {
    fn new(initial: &str) -> Self {
        Self {
            contents: RefCell::new(initial.to_owned()),
        }
    }

    fn contents(&self) -> String {
        self.contents.borrow().clone()
    }
}

impl SourceProbe for InMemoryConfigProbe {
    fn run_command(&self, _program: &str, _args: &[&str]) -> SourceProbeOutcome {
        SourceProbeOutcome::unavailable("run_command unused by the arming port")
    }

    fn read_file(&self, _path: &str) -> SourceProbeOutcome {
        SourceProbeOutcome::observed(&self.contents.borrow(), true)
    }

    fn write_file(&self, _path: &str, contents: &str) -> SourceProbeOutcome {
        self.contents.replace(contents.to_owned());
        SourceProbeOutcome::observed("", true)
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

/// Enabling autonomous mode is confirmed, armed, and audited: a
/// `config.autonomous_mode_set` with `confirmed` true completes, appends the
/// `config.autonomous_mode.enabled` audit event, issues
/// `factory.autonomous_mode_enable_requested` through the orchestrator's arming
/// surface, and the orchestrator permission key ends up actually written.
#[test]
fn scenario_9_enabling_autonomous_mode_is_confirmed_armed_and_audited()
-> Result<(), ConsoleRuntimeError> {
    let mut store = SqliteEventStore::open_in_memory()?;

    // Given a registered repo whose autonomous mode is disabled by default.
    let probe = InMemoryConfigProbe::new(CONFIG_DISABLED_BY_DEFAULT);
    assert!(!read_autonomous_mode_from_jsonc(&probe.contents()));

    // When the operator submits config.autonomous_mode_set with confirmed true.
    store.append_command(&autonomous_mode_set_command(
        r#"{"repo":"livespec-console-beads-fabro","enabled":true,"confirmed":true}"#,
    ))?;
    let mut port = LivespecJsoncArmingPort::new(&probe, SCRATCH_PATH);
    let outcomes = handle_pending_config_commands(&mut store, "2026-07-11T00:00:01Z", &mut port)?;

    // Then the command completes.
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].command_status(), "completed");

    let events = store.list_console_events()?;
    // And appends a config.autonomous_mode.enabled audit event (configuration).
    let audit = events
        .iter()
        .find(|event| event.event_type() == &EventType::ConfigAutonomousModeEnabled);
    assert!(matches!(audit, Some(event) if event.context() == "configuration"));
    // And issues factory.autonomous_mode_enable_requested to the orchestrator
    // through its published arming surface (factory context).
    let issued = events
        .iter()
        .find(|event| event.event_type() == &EventType::FactoryAutonomousModeEnableRequested);
    assert!(matches!(issued, Some(event) if event.context() == "factory"));

    // And the orchestrator permission key is actually armed in the config.
    assert!(read_autonomous_mode_from_jsonc(&probe.contents()));
    Ok(())
}

/// An unconfirmed enable is rejected with no effect: the Configuration context
/// rejects the command, no setting is written (the permission key stays
/// disabled), and no audit event is appended.
#[test]
fn scenario_9_an_unconfirmed_enable_is_rejected_with_no_effect() -> Result<(), ConsoleRuntimeError>
{
    let mut store = SqliteEventStore::open_in_memory()?;

    // Given a registered repo whose autonomous mode is disabled.
    let probe = InMemoryConfigProbe::new(CONFIG_DISABLED_BY_DEFAULT);

    // When a config.autonomous_mode_set with enabled true arrives without
    // confirmed true.
    store.append_command(&autonomous_mode_set_command(
        r#"{"repo":"livespec-console-beads-fabro","enabled":true,"confirmed":false}"#,
    ))?;
    let mut port = LivespecJsoncArmingPort::new(&probe, SCRATCH_PATH);
    let outcomes = handle_pending_config_commands(&mut store, "2026-07-11T00:00:01Z", &mut port)?;

    // Then the Configuration context rejects the command.
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].command_status(), "rejected");

    // And no setting is written: the permission key stays disabled.
    assert!(!read_autonomous_mode_from_jsonc(&probe.contents()));
    assert_eq!(probe.contents(), CONFIG_DISABLED_BY_DEFAULT);

    // And no audit event is appended.
    let events = store.list_console_events()?;
    assert!(
        !events
            .iter()
            .any(|event| event.event_type() == &EventType::ConfigAutonomousModeEnabled)
    );
    Ok(())
}
