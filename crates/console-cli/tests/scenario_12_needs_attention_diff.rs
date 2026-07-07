//! Scenario 12 — needs-attention snapshot diffed at ingest into
//! `attention_item` events (and the re-cast Scenario 1 "Mixed source signals"
//! journey).
//!
//! Top-of-pyramid acceptance/integration test (the Scenario 7 precedent): it
//! drives the real ingestion path end-to-end against a live event store — the
//! dedicated snapshot-source port, the diff-at-ingest adapter, and the
//! re-sourced needs-attention projection — proving appeared/changed/resolved
//! keyed by stable id, idempotence, and the projected inbox.

use console_application::source_adapters::{
    AttentionHandoff, AttentionItemSnapshot, AttentionSourceRef, NeedsAttentionReadOutcome,
    NeedsAttentionSnapshotPort,
};
use console_application::{AttentionItem, project_attention};
use console_domain::{ConsoleEvent, EventType};
use console_eventstore::SqliteEventStore;
use livespec_console_beads_fabro::{
    ConsoleRuntimeError, NeedsAttentionIngest, ingest_needs_attention,
};

/// A needs-attention snapshot-source port that returns a canned snapshot, so the
/// diff-at-ingest can be driven without a live orchestrator `needs-attention`
/// CLI.
struct StubNeedsAttentionPort {
    snapshot: Vec<AttentionItemSnapshot>,
}

impl NeedsAttentionSnapshotPort for StubNeedsAttentionPort {
    fn read_snapshot(&self) -> NeedsAttentionReadOutcome {
        NeedsAttentionReadOutcome::Observed(self.snapshot.clone())
    }
}

fn attention_item(
    id: &str,
    kind: &str,
    urgency: &str,
    summary: &str,
    command: &str,
) -> AttentionItemSnapshot {
    AttentionItemSnapshot::new(
        id,
        kind,
        urgency,
        summary,
        AttentionSourceRef::new("livespec-console-beads-fabro", Some(id), None),
        AttentionHandoff::new(kind, Some(command), command),
    )
}

fn ingest(
    store: &mut SqliteEventStore,
    snapshot: Vec<AttentionItemSnapshot>,
    observed_at: &str,
) -> Result<usize, ConsoleRuntimeError> {
    let port = StubNeedsAttentionPort { snapshot };
    let needs_attention = NeedsAttentionIngest::new(&port, "livespec-console-beads-fabro");
    ingest_needs_attention(store, &needs_attention, observed_at)
}

fn count(events: &[ConsoleEvent], event_type: EventType) -> usize {
    events
        .iter()
        .filter(|event| event.event_type() == &event_type)
        .count()
}

#[test]
fn scenario_12_needs_attention_snapshot_diffed_at_ingest_into_attention_item_events()
-> Result<(), ConsoleRuntimeError> {
    let mut store = SqliteEventStore::open_in_memory()?;

    // Mixed source signals compose one product needs-attention snapshot: a
    // blocked Fabro run with a human gate, pending proposed changes requiring
    // revise, and a non-converging item bounced to `backlog` for re-grooming.
    let first_snapshot = vec![
        attention_item(
            "fabro-gate",
            "human-valve",
            "high",
            "Blocked Fabro run awaiting human gate",
            "approve:fabro-gate",
        ),
        attention_item(
            "spec-revise",
            "spec",
            "medium",
            "Pending proposed changes requiring revise",
            "livespec:revise",
        ),
        attention_item(
            "regroom",
            "plan",
            "medium",
            "Non-converging item bounced to backlog for re-grooming",
            "groom:regroom",
        ),
    ];

    // First ingest against an empty prior: every item appears.
    let appeared = ingest(&mut store, first_snapshot.clone(), "2026-07-07T00:00:00Z")?;
    assert_eq!(appeared, 3);

    let events = store.list_console_events()?;
    assert_eq!(count(&events, EventType::AttentionItemAppeared), 3);
    assert_eq!(count(&events, EventType::AttentionItemChanged), 0);
    assert_eq!(count(&events, EventType::AttentionItemResolved), 0);

    // The projection lists all three items from the attention_item stream, each
    // carrying a source reference and its next operator action (the handoff).
    let inbox = project_attention(&events);
    let ids: Vec<&str> = inbox.iter().map(AttentionItem::id).collect();
    assert_eq!(ids, ["fabro-gate", "regroom", "spec-revise"]);
    for item in &inbox {
        assert!(
            item.source_reference()
                .starts_with("livespec-console-beads-fabro:")
        );
    }

    // Re-ingesting the identical snapshot is idempotent: an unchanged id emits
    // nothing.
    let unchanged = ingest(&mut store, first_snapshot, "2026-07-07T00:01:00Z")?;
    assert_eq!(unchanged, 0);
    assert_eq!(store.list_console_events()?.len(), 3);

    // A second snapshot: one item changed (fabro-gate escalates), one removed
    // (spec-revise resolved), one added (a new hygiene finding); the plan item
    // is unchanged and must emit nothing.
    let second_snapshot = vec![
        attention_item(
            "fabro-gate",
            "human-valve",
            "critical",
            "Blocked Fabro run awaiting human gate (escalated)",
            "approve:fabro-gate",
        ),
        attention_item(
            "regroom",
            "plan",
            "medium",
            "Non-converging item bounced to backlog for re-grooming",
            "groom:regroom",
        ),
        attention_item(
            "hygiene-stale",
            "hygiene",
            "low",
            "Stale worktree needs reaping",
            "reap:hygiene-stale",
        ),
    ];

    let ingested = ingest(&mut store, second_snapshot, "2026-07-07T00:02:00Z")?;
    assert_eq!(ingested, 3);

    let events = store.list_console_events()?;
    // Exactly one changed, one resolved, one new appeared this poll (three
    // appeared total: the two survivors plus the new hygiene item).
    assert_eq!(count(&events, EventType::AttentionItemChanged), 1);
    assert_eq!(count(&events, EventType::AttentionItemResolved), 1);
    assert_eq!(count(&events, EventType::AttentionItemAppeared), 4);

    // Every emitted event is keyed by the item's stable id (its per-item stream).
    for event in events
        .iter()
        .filter(|event| event.source() == "needs-attention")
    {
        assert!(
            event
                .stream_id()
                .starts_with("attention_item:livespec-console-beads-fabro:")
        );
    }

    // The rebuilt inbox reflects the diff: fabro-gate (changed) and regroom
    // (unchanged) survive, hygiene-stale appeared, spec-revise resolved out.
    let inbox = project_attention(&events);
    let ids: Vec<&str> = inbox.iter().map(AttentionItem::id).collect();
    assert_eq!(ids, ["fabro-gate", "hygiene-stale", "regroom"]);
    assert_eq!(
        inbox[0].title(),
        "Blocked Fabro run awaiting human gate (escalated)"
    );
    assert_eq!(inbox[0].source(), "human-valve");
    Ok(())
}
