//! Attention conformance slice (`livespec-console-beads-fabro-cddfxl`):
//! the v042 Initial-Adapters exclusivity clauses and the re-sourced
//! Scenario 5 inspection, graded end-to-end.
//!
//! - The needs-attention adapter's `attention_item.*` emissions are the ONLY
//!   events this source produces: an `impl:` attention row must NOT synthesize
//!   work-item lane state (gap-hi6r2ue4).
//! - A needs-attention row referencing a work item the console genuinely
//!   ingested joins that record's detail by id (gap-etdhc5zx).
//! - A referenced work-item record the console never ingested renders as
//!   explicitly absent rather than be fabricated (gap-2saxvsp7).

use console_application::build_tui_model;
use console_application::source_adapters::{
    AcceptancePolicy, AdmissionPolicy, AttentionHandoff, AttentionItemSnapshot, AttentionSourceRef,
    Lane, NeedsAttentionReadOutcome, NeedsAttentionSnapshotPort, WorkItemSnapshot,
    attention_item_payload_json, work_item_snapshot_payload_json,
};
use console_domain::{ConsoleEvent, EventType};
use console_eventstore::SqliteEventStore;
use livespec_console_beads_fabro::{
    ConsoleRuntimeError, NeedsAttentionIngest, ingest_needs_attention,
};

/// A needs-attention snapshot-source port that returns a canned snapshot.
struct StubNeedsAttentionPort {
    snapshot: Vec<AttentionItemSnapshot>,
}

impl NeedsAttentionSnapshotPort for StubNeedsAttentionPort {
    fn read_snapshot(&self) -> NeedsAttentionReadOutcome {
        NeedsAttentionReadOutcome::Observed(self.snapshot.clone())
    }
}

/// An `impl:` needs-attention row referencing `work_item`, shaped like the
/// orchestrator's ready-implementation-work composition class.
fn impl_attention_row(work_item: &str) -> AttentionItemSnapshot {
    AttentionItemSnapshot::new(
        &format!("impl:{work_item}"),
        "impl-ready",
        "normal",
        &format!("Ready implementation work: {work_item}"),
        AttentionSourceRef::new("livespec-console-beads-fabro", Some(work_item), None),
        AttentionHandoff::new(
            "drive",
            Some(&format!("impl:{work_item}")),
            &format!("drive --action impl:{work_item}"),
        ),
    )
}

/// One `attention_item.appeared` console event carrying `item`'s payload.
fn attention_appeared_event(event_id: &str, item: &AttentionItemSnapshot) -> ConsoleEvent {
    ConsoleEvent::new(
        event_id.to_owned(),
        1,
        "factory".to_owned(),
        EventType::AttentionItemAppeared,
        "needs-attention".to_owned(),
        format!("attention_item:livespec-console-beads-fabro:{}", item.id()),
        1,
    )
    .with_payload_json(attention_item_payload_json(item))
}

/// One genuinely-ingested `work_item.snapshot.observed` console event for
/// `work_item_id` resting in the Ready lane (which draws no attention of its
/// own, so an attention row referencing it renders as a needs-attention entry).
fn genuine_ready_snapshot_event(
    event_id: &str,
    work_item_id: &str,
) -> Result<ConsoleEvent, ConsoleRuntimeError> {
    let snapshot = WorkItemSnapshot::new(
        "livespec-console-beads-fabro",
        work_item_id,
        Lane::Ready,
        None,
        "a0",
        "ready",
        AdmissionPolicy::Auto,
        AcceptancePolicy::AiOnly,
        1,
    )?;
    Ok(ConsoleEvent::new(
        event_id.to_owned(),
        1,
        "factory".to_owned(),
        EventType::WorkItemSnapshotObserved,
        "orchestrator".to_owned(),
        format!("livespec-console-beads-fabro:{work_item_id}"),
        1,
    )
    .with_payload_json(work_item_snapshot_payload_json(&snapshot)))
}

#[test]
fn impl_attention_row_creates_only_attention_item_events() -> Result<(), ConsoleRuntimeError> {
    let mut store = SqliteEventStore::open_in_memory()?;
    let port = StubNeedsAttentionPort {
        snapshot: vec![impl_attention_row("wi-slice")],
    };
    let ingest = NeedsAttentionIngest::new(&port, "livespec-console-beads-fabro");
    ingest_needs_attention(&mut store, &ingest, "2026-08-25T00:00:00Z")?;

    let events = store.list_console_events()?;
    assert!(!events.is_empty(), "the ingest must produce events");
    for event in &events {
        assert!(
            matches!(
                event.event_type(),
                EventType::AttentionItemAppeared
                    | EventType::AttentionItemChanged
                    | EventType::AttentionItemResolved
            ),
            "an impl: attention row must create ONLY attention_item.* events; found {:?}",
            event.event_type()
        );
    }
    Ok(())
}

#[test]
fn needs_attention_detail_joins_genuinely_ingested_work_item_by_id()
-> Result<(), ConsoleRuntimeError> {
    let events = [
        genuine_ready_snapshot_event("evt_ready", "wi-joined")?,
        attention_appeared_event("evt_attn_join", &impl_attention_row("wi-joined")),
    ];

    let model = build_tui_model(&events, 0);
    assert_eq!(
        model.attention_items().len(),
        1,
        "one needs-attention entry expected"
    );
    assert!(
        model.detail().is_some(),
        "a selected row renders a detail pane"
    );
    if let Some(detail) = model.detail() {
        assert!(
            !detail.timeline().is_empty(),
            "detail must join the genuinely-ingested work item's timeline by id"
        );
        assert_eq!(
            detail.attach_command(),
            Some("drive --action impl:wi-joined"),
            "the handoff command stays the operator action"
        );
    }
    Ok(())
}

#[test]
fn never_ingested_record_renders_explicitly_absent() {
    let events = [attention_appeared_event(
        "evt_attn_ghost",
        &impl_attention_row("wi-ghost"),
    )];

    let model = build_tui_model(&events, 0);
    assert_eq!(
        model.attention_items().len(),
        1,
        "one needs-attention entry expected"
    );
    assert!(
        model.detail().is_some(),
        "a selected row renders a detail pane"
    );
    if let Some(detail) = model.detail() {
        assert!(
            detail.work_item().contains("record not ingested"),
            "a never-ingested referenced record must render explicitly absent; got {:?}",
            detail.work_item()
        );
        assert!(
            detail.timeline().is_empty() && detail.actions().is_empty(),
            "no lifecycle state may be synthesized for a never-ingested record"
        );
    }
}
