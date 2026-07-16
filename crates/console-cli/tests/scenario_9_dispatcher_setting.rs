//! Scenario 9 -- Operator sets a dispatcher policy setting from the console
//! (SPECIFICATION/scenarios.md). The console holds no dispatcher-setting state of
//! its own: a setting change is a single per-setting write that MUST be effected
//! THROUGH the orchestrator's published command surface (`drive.py --action
//! set-config:<key>:<value>`) and MUST append the `config.dispatcher_setting.changed`
//! audit fact -- rather than the console writing the orchestrator's
//! `.livespec.jsonc` itself. Enabling a dangerous setting is an ORDINARY recorded
//! write: there is no type-the-repo-name arming ceremony.
//!
//! This drives the console command pump end to end over the REAL
//! `DispatcherOrchestratorActionPort` wired to a recording `SourceProbe`, so the
//! full path is exercised: the stored `config.dispatcher_setting_set` command is
//! handled, the orchestrator `set-config` action is observed issued through
//! `drive.py`, the audit event lands in the event store, and the probe observes
//! NO `.livespec.jsonc` write. A simulated port surfaces the honest not-wired
//! outcome rather than fabricating success.

use std::cell::RefCell;

use console_application::DispatcherOrchestratorActionPort;
use console_application::source_adapters::{SourceProbe, SourceProbeOutcome};
use console_domain::{CommandEnvelope, CommandType, EventType};
use console_eventstore::{CommandAppend, SqliteEventStore};
use livespec_console_beads_fabro::{ConsoleRuntimeError, handle_pending_config_commands};

/// A `config` read payload as the orchestrator emits it under `--json`, with
/// `auto_approve_ready` currently off (so enabling it records a `false -> true`
/// change).
const CONFIG_READ_JSON: &str = r#"{
  "settings": [
    { "key": "auto_approve_ready", "value": false, "source": "default" },
    { "key": "merge_on_review_cap", "value": false, "source": "default" },
    { "key": "acceptance_mode", "value": "ai-then-human", "source": "default" },
    { "key": "review_fix_cap", "value": 3, "source": "default" },
    { "key": "acceptance_rework_cap", "value": 2, "source": "default" },
    { "key": "wip_cap", "value": 5, "source": "default" }
  ]
}"#;

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

fn dispatcher_setting_set_command(payload_json: &str) -> CommandAppend {
    CommandAppend::new(
        CommandEnvelope::new(
            "cmd_setting".to_owned(),
            CommandType::ConfigDispatcherSettingSet,
            "livespec-console-beads-fabro".to_owned(),
            "livespec-console-beads-fabro:config.dispatcher_setting_set".to_owned(),
            "operator".to_owned(),
        ),
        "2026-07-11T00:00:00Z".to_owned(),
        Some("livespec-console-beads-fabro".to_owned()),
        "corr_cmd_setting".to_owned(),
        payload_json.to_owned(),
    )
}

/// Whether `call` is the recorded `drive.py` invocation carrying the given
/// `--action` id.
fn is_action_call(call: &[String], action_id: &str) -> bool {
    call.windows(2)
        .any(|pair| pair[0] == "--action" && pair[1] == action_id)
}

/// Setting a dispatcher policy is commanded THROUGH the orchestrator API: a
/// `config.dispatcher_setting_set` write completes, issues the orchestrator's
/// published `set-config` action through `drive.py`, appends the
/// `config.dispatcher_setting.changed` audit event carrying the previous and new
/// values, and writes NO `.livespec.jsonc` on disk. There is no arming ceremony.
#[test]
fn scenario_9_setting_a_dispatcher_policy_is_commanded_through_the_orchestrator_api()
-> Result<(), ConsoleRuntimeError> {
    let mut store = SqliteEventStore::open_in_memory()?;

    // Given a registered repo whose settings the console reads from the
    // orchestrator. When the operator edits the Auto-approve ready row.
    store.append_command(&dispatcher_setting_set_command(
        r#"{"repo":"livespec-console-beads-fabro","setting":"auto_approve_ready","value":true}"#,
    ))?;
    let probe =
        RecordingDriveProbe::returning(SourceProbeOutcome::observed(CONFIG_READ_JSON, true));
    let mut drive =
        DispatcherOrchestratorActionPort::new(&probe, "drive.py", &["--repo", "/orch", "--json"]);
    let outcomes = handle_pending_config_commands(&mut store, "2026-07-11T00:00:01Z", &mut drive)?;

    // Then the command completes with no arming ceremony.
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].command_status(), "completed");

    // And the setting was effected through the orchestrator's published
    // `set-config` action -- never by the console writing `.livespec.jsonc`.
    let calls = probe.run_calls.borrow();
    assert!(
        calls
            .iter()
            .any(|call| is_action_call(call, "set-config:auto_approve_ready:true")),
        "expected a set-config write, got {calls:?}"
    );

    // And the durable change audit event lands in the configuration context,
    // carrying the previous (false) and new (true) values.
    let events = store.list_console_events()?;
    let audits: Vec<(&str, &str)> = events
        .iter()
        .filter(|event| event.event_type() == &EventType::ConfigDispatcherSettingChanged)
        .map(|event| (event.context(), event.payload_json()))
        .collect();
    assert_eq!(
        audits.len(),
        1,
        "expected one change audit event, got {audits:?}"
    );
    let (context, payload_json) = audits[0];
    assert_eq!(context, "configuration");
    let payload: serde_json::Value = serde_json::from_str(payload_json).unwrap_or_default();
    assert_eq!(payload["setting"], "auto_approve_ready");
    assert_eq!(payload["previous"], serde_json::json!(false));
    assert_eq!(payload["new"], serde_json::json!(true));
    Ok(())
}

/// A simulated / unimplemented orchestrator port surfaces a not-wired outcome
/// rather than fabricating success: the command is recorded not-wired and no
/// `config.dispatcher_setting.changed` event is appended.
#[test]
fn scenario_9_a_simulated_port_surfaces_not_wired_rather_than_fabricating_success()
-> Result<(), ConsoleRuntimeError> {
    let mut store = SqliteEventStore::open_in_memory()?;

    store.append_command(&dispatcher_setting_set_command(
        r#"{"repo":"livespec-console-beads-fabro","setting":"auto_approve_ready","value":true}"#,
    ))?;
    // The port is unreachable, so both the read and the write are not-wired.
    let probe = RecordingDriveProbe::returning(SourceProbeOutcome::unavailable("no drive surface"));
    let mut drive =
        DispatcherOrchestratorActionPort::new(&probe, "drive.py", &["--repo", "/orch", "--json"]);
    let outcomes = handle_pending_config_commands(&mut store, "2026-07-11T00:00:01Z", &mut drive)?;

    // Then the command is recorded not-wired.
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].command_status(), "not_wired");

    // And no change event is appended (no fabricated success).
    let events = store.list_console_events()?;
    assert!(
        !events
            .iter()
            .any(|event| event.event_type() == &EventType::ConfigDispatcherSettingChanged)
    );
    assert!(
        events
            .iter()
            .any(|event| event.event_type() == &EventType::ConfigDispatcherSettingNotWired)
    );
    Ok(())
}
