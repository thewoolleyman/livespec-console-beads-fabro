//! Scenario 10 -- Autonomous mode resolves the decidable and escalates the rest
//! (`SPECIFICATION/scenarios.md`).
//!
//! Drives the shipped `serve` run loop end to end: with a repo in autonomous
//! mode, the orchestrator plane's engine auto-resolves a decidable needs-attention
//! item and escalates a truly-unresolvable one, journaling both on its published
//! per-decision audit. The console reads that audit, reflects the auto-resolution
//! through its own command-plus-outcome-event path so the item leaves the inbox
//! (case 1), and LEAVES the escalation as a needs-attention item -- neither
//! dropping nor fabricating it (case 2).

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
use livespec_console_beads_fabro::{ConsoleRuntimeError, NeedsAttentionIngest, serve_report};

/// The needs-attention surface: two human-gate valve items, keyed exactly as the
/// orchestrator plane keys them (`valve:<verb>:<work-item-id>`).
struct TwoValveNeedsAttentionPort;

impl NeedsAttentionSnapshotPort for TwoValveNeedsAttentionPort {
    fn read_snapshot(&self) -> NeedsAttentionReadOutcome {
        NeedsAttentionReadOutcome::Observed(vec![
            valve_item("valve:approve:wi-1", "wi-1", "approve"),
            valve_item("valve:accept:wi-2", "wi-2", "accept"),
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

/// A probe whose `read_file` returns the orchestrator plane's Dispatcher journal:
/// wi-1's approve gate auto-resolved, wi-2's acceptance escalated.
struct JournalProbe;

impl SourceProbe for JournalProbe {
    fn run_command(&self, program: &str, _args: &[&str]) -> SourceProbeOutcome {
        SourceProbeOutcome::unavailable(&format!("{program}: not wired in this test"))
    }

    fn read_file(&self, _path: &str) -> SourceProbeOutcome {
        let journal = [
            r#"{"stage":"autonomous-decision","work_item_id":"wi-1","gate":"approve","decision":"auto-approve routine manual admission","disposition":"auto-resolved"}"#,
            r#"{"stage":"autonomous-decision","work_item_id":"wi-2","gate":"acceptance","decision":"needs human sign-off","disposition":"escalated"}"#,
        ]
        .join("\n");
        SourceProbeOutcome::observed(&journal, true)
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
fn scenario_10_autonomous_run_reflects_the_decidable_and_escalates_the_rest()
-> Result<(), ConsoleRuntimeError> {
    let mut store = SqliteEventStore::open_in_memory()?;
    let na_port = TwoValveNeedsAttentionPort;
    let needs_attention = NeedsAttentionIngest::new(&na_port, "fleet");
    let probe = JournalProbe;
    let decisions = JournalAutonomousDecisionsPort::new(&probe, "tmp/dispatcher-journal.jsonl");
    let mut drain = NoDrainPort;
    let mut work_item = NoWorkItemActionPort;

    // The shipped run loop: ingest the needs-attention surface, then observe the
    // plane's published per-decision audit and reflect it.
    let _report = serve_report(
        &mut store,
        "2026-07-11T00:00:00Z",
        &[],
        &mut drain,
        &mut work_item,
        &decisions,
        &needs_attention,
    )?;

    let inbox: Vec<String> = project_attention(&store.list_console_events()?)
        .iter()
        .map(|item| item.id().to_owned())
        .collect();

    // Case 1 -- the decidable item (wi-1's approve) was resolved by the plane and
    // reflected, so it left the needs-attention inbox.
    // Case 2 -- the truly-unresolvable item (wi-2's acceptance) still needs a
    // human, so it stays: neither dropped nor fabricated.
    assert_eq!(inbox, ["valve:accept:wi-2"]);

    // The reflection rode the console's own command-plus-outcome-event path.
    let commands = store.list_commands()?;
    assert!(commands.iter().any(|command| {
        command.command_type() == "factory.autonomous_decision_reflected"
            && command.status() == "completed"
    }));

    Ok(())
}
