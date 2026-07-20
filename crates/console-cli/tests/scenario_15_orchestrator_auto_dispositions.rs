//! Scenario 15 -- Orchestrator auto-dispositions and escalations reach the operator
//! (`SPECIFICATION/scenarios.md`).
//!
//! Drives the shipped `serve` run loop end to end against a fixture generated
//! from the orchestrator producer's published auto-disposition journal builder.
//! The console reads that journal, reflects auto-dispositions through its own
//! command-plus-outcome-event path, attributes them to the governing settings,
//! and surfaces cap-exceeded escalations as needs-attention items.

use std::cell::RefCell;
use std::fmt::Write as _;

use console_application::source_adapters::{
    AttentionHandoff, AttentionItemSnapshot, AttentionSourceRef, NeedsAttentionReadOutcome,
    NeedsAttentionSnapshotPort, SourceProbe, SourceProbeOutcome,
};
use console_application::{
    ApplicationError, FactoryDrainPort, FactoryDrainPortOutcome, FactoryDrainRequest,
    JournalAutonomousDecisionsPort, OrchestratorActionOutcome, OrchestratorActionPort,
    OrchestratorActionRequest, project_attention,
};
use console_eventstore::SqliteEventStore;
use livespec_console_beads_fabro::{
    ConsoleRuntimeError, DISPATCHER_JOURNAL_PATH, NeedsAttentionIngest, serve_report,
};
use sha2::{Digest, Sha256};

const PRODUCER_FIXTURE_SHA256: &str =
    "ebbd867d419348986db81ab220ddbea14c32aa0891c3241bd21538dfc1b42fd4";
const PRODUCER_FIXTURE: &str = include_str!("fixtures/orchestrator-auto-disposition-journal.jsonl");

/// The needs-attention surface: the four human-gate valve items that the
/// orchestrator's auto-dispositions resolve. The cap-exceeded escalation is not
/// pre-seeded; Scenario 15 requires the journal read leg to surface it.
struct AutoDispositionNeedsAttentionPort;

impl NeedsAttentionSnapshotPort for AutoDispositionNeedsAttentionPort {
    fn read_snapshot(&self) -> NeedsAttentionReadOutcome {
        NeedsAttentionReadOutcome::Observed(vec![
            valve_item("valve:approve:bd-ib-1", "bd-ib-1", "approve"),
            valve_item("valve:accept:bd-ib-2", "bd-ib-2", "accept"),
            valve_item("valve:accept:bd-ib-3", "bd-ib-3", "accept"),
            valve_item("valve:accept:bd-ib-4", "bd-ib-4", "accept"),
        ])
    }
}

fn valve_item(id: &str, work_item: &str, verb: &str) -> AttentionItemSnapshot {
    AttentionItemSnapshot::new(
        id,
        "human-valve",
        "high",
        &format!("{verb} completed work-item {work_item}"),
        AttentionSourceRef::new("fleet", Some(work_item), None),
        AttentionHandoff::new(
            "drive",
            Some(&format!("{verb}:{work_item}")),
            &format!("drive --action {verb}:{work_item}"),
        ),
    )
}

/// A probe whose `read_file` returns the orchestrator plane's Dispatcher journal
/// fixture and records which path the port asked for.
struct JournalProbe {
    observed_path: RefCell<Option<String>>,
}

impl JournalProbe {
    const fn new() -> Self {
        Self {
            observed_path: RefCell::new(None),
        }
    }
}

impl SourceProbe for JournalProbe {
    fn run_command(&self, program: &str, _args: &[&str]) -> SourceProbeOutcome {
        SourceProbeOutcome::unavailable(&format!("{program}: not wired in this test"))
    }

    fn read_file(&self, path: &str) -> SourceProbeOutcome {
        let _old = self.observed_path.replace(Some(path.to_owned()));
        SourceProbeOutcome::observed(PRODUCER_FIXTURE, true)
    }
}

struct NoDrainPort;

impl FactoryDrainPort for NoDrainPort {
    fn drain_ready_queue(
        &mut self,
        _request: &FactoryDrainRequest,
    ) -> Result<FactoryDrainPortOutcome, ApplicationError> {
        Ok(FactoryDrainPortOutcome::not_wired())
    }
}

struct NoWorkItemActionPort;

impl OrchestratorActionPort for NoWorkItemActionPort {
    fn run_action(
        &mut self,
        _request: &OrchestratorActionRequest,
    ) -> Result<OrchestratorActionOutcome, ApplicationError> {
        Ok(OrchestratorActionOutcome::not_wired())
    }
}

#[test]
fn scenario_15_orchestrator_auto_dispositions_reflect_and_surface_escalations()
-> Result<(), ConsoleRuntimeError> {
    assert_eq!(sha256_hex(PRODUCER_FIXTURE), PRODUCER_FIXTURE_SHA256);

    let mut store = SqliteEventStore::open_in_memory()?;
    let na_port = AutoDispositionNeedsAttentionPort;
    let needs_attention = NeedsAttentionIngest::new(&na_port, "fleet");
    let probe = JournalProbe::new();
    let decisions = JournalAutonomousDecisionsPort::new(&probe, DISPATCHER_JOURNAL_PATH);
    let mut drain = NoDrainPort;
    let mut work_item = NoWorkItemActionPort;

    let _report = serve_report(
        &mut store,
        "2026-07-20T00:00:00Z",
        &[],
        &mut drain,
        &mut work_item,
        &decisions,
        &needs_attention,
    )?;

    assert_eq!(
        probe.observed_path.borrow().as_deref(),
        Some("tmp/fabro-dispatch-journal.jsonl")
    );

    let inbox: Vec<String> = project_attention(&store.list_console_events()?)
        .iter()
        .map(|item| item.id().to_owned())
        .collect();

    assert_eq!(inbox, ["valve:set-admission:bd-ib-5"]);

    let _second_report = serve_report(
        &mut store,
        "2026-07-20T00:00:01Z",
        &[],
        &mut drain,
        &mut work_item,
        &decisions,
        &needs_attention,
    )?;
    let second_inbox: Vec<String> = project_attention(&store.list_console_events()?)
        .iter()
        .map(|item| item.id().to_owned())
        .collect();
    assert_eq!(second_inbox, ["valve:set-admission:bd-ib-5"]);

    let commands = store.list_commands()?;
    let reflection_commands = commands
        .iter()
        .filter(|command| {
            command.command_type() == "factory.autonomous_decision_reflected"
                && command.status() == "completed"
        })
        .count();
    assert_eq!(reflection_commands, 4);
    assert!(commands.iter().any(|command| {
        command.payload_json().contains("ai-fail-auto-rework")
            && command.payload_json().contains("acceptance_rework_cap")
    }));

    Ok(())
}

fn sha256_hex(text: &str) -> String {
    let mut hex = String::new();
    for byte in Sha256::digest(text.as_bytes()) {
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}
