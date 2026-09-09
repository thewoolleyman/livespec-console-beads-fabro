//! Ready-lane projection freshness (`livespec-console-beads-fabro-v8un`).
//!
//! The defect this grades, measured live by plan 04's campaign walks #3 and #5:
//! a ready-lane row serves PRE-REFRESH values — `rank ~`, no title, no
//! `acceptance_policy` — indefinitely, and no operator action reliably clears
//! it.
//!
//! The mechanism, reproduced end-to-end below. A work-item snapshot's event id
//! is CONTENT-ADDRESSED (`evt:orchestrator:<repo>:<id>:<content-hash>:snapshot`)
//! and the event store appends `insert or ignore` on that id, while the lane
//! projection is last-observation-wins in append order. So an item observed
//! GOOD, then observed DEGRADED, and then observed GOOD again re-derives the
//! FIRST observation's id, is rejected as a duplicate, never lands, and the
//! projection stays pinned on the DEGRADED observation for the life of the
//! store. Only a change to some OTHER field — which yields a fresh hash — ever
//! shakes it loose, which is exactly why walk #5's `set-acceptance` against the
//! item itself completed and left the row stale while walk #3's arm of a
//! different item appeared to fix it.
//!
//! `rank` and `acceptance_policy` degrade TOGETHER here because they are pinned
//! by the same swallowed observation, not because two feeds disagree.

use console_application::project_lane_board;
use console_application::source_adapters::{
    Lane, NeedsAttentionReadOutcome, NeedsAttentionSnapshotPort, ObservedSourceAdapter,
    PullSourcePort, SourceAdapterKind, SourceObservationPlan, SourceProbe, SourceProbeOutcome,
    parse_orchestrator_observation,
};
use console_application::writer_identity::WriterIdentity;
use console_eventstore::SqliteEventStore;
use livespec_console_beads_fabro::{ConsoleRuntimeError, NeedsAttentionIngest, refresh_sources};

fn test_writer_identity() -> WriterIdentity {
    WriterIdentity::new(
        4242,
        "/opt/console/test-binary",
        "/data/projects/repo",
        "test1234",
    )
}

const REPO: &str = "livespec-console-beads-fabro";

/// The orchestrator's own `list-work-items --json` output for a ranked, titled,
/// explicitly-armed ready item — the state the ledger and the ranker hold.
const GOOD: &str = concat!(
    r#"[{"id":"wi-1","lane":"ready","rank":"a0","status":"ready","#,
    r#""title":"Adopt the charter gate in livespec-console-beads-fabro","#,
    r#""acceptance_policy":"ai-only"}]"#,
);

/// The same item observed through a DEGRADED read that carried neither `rank`
/// nor `title` nor `acceptance_policy` — the shape a stale row displays.
const DEGRADED: &str = r#"[{"id":"wi-1","lane":"ready","status":"ready"}]"#;

/// A probe replaying a scripted sequence of `list-work-items` stdouts, one per
/// successive poll, repeating the last once exhausted.
struct SequencedProbe {
    stdouts: Vec<&'static str>,
    cursor: std::cell::Cell<usize>,
}

impl SourceProbe for SequencedProbe {
    fn run_command(&self, _program: &str, _args: &[&str]) -> SourceProbeOutcome {
        let index = self.cursor.get().min(self.stdouts.len().saturating_sub(1));
        self.cursor.set(index.saturating_add(1));
        SourceProbeOutcome::observed(self.stdouts.get(index).copied().unwrap_or_default(), true)
    }

    fn read_file(&self, _path: &str) -> SourceProbeOutcome {
        SourceProbeOutcome::Unavailable {
            reason: "this adapter observes a command, not a file".to_owned(),
        }
    }
}

/// A needs-attention port holding nothing, so the ready lane under test is fed
/// by the work-item snapshot adapter alone.
struct EmptyNeedsAttentionPort;

impl NeedsAttentionSnapshotPort for EmptyNeedsAttentionPort {
    fn read_snapshot(&self) -> NeedsAttentionReadOutcome {
        NeedsAttentionReadOutcome::Observed(Vec::new())
    }
}

/// The values the ready-lane row carries into the rendered surface: its rank,
/// its title, and the `acceptance_policy` the orchestrator emitted for it.
#[derive(Debug, PartialEq, Eq)]
struct ReadyRow {
    rank: String,
    title: Option<String>,
    acceptance_policy: Option<String>,
}

/// The single ready-lane row the projection currently serves, or `None` when
/// the lane holds no row at all — compared as an `Option` so a vanished row
/// fails the assertion instead of unwrapping.
fn ready_row(store: &SqliteEventStore) -> Result<Option<ReadyRow>, ConsoleRuntimeError> {
    let events = store.list_console_events()?;
    let board = project_lane_board(&events);
    Ok(board
        .column(Lane::Ready)
        .and_then(|column| column.items().first())
        .map(|item| ReadyRow {
            rank: item.rank().to_owned(),
            title: item.detail().title.clone(),
            acceptance_policy: item.detail().acceptance_policy.clone(),
        }))
}

/// The row as the ledger and the ranker hold it.
fn good_row() -> ReadyRow {
    ReadyRow {
        rank: "a0".to_owned(),
        title: Some("Adopt the charter gate in livespec-console-beads-fabro".to_owned()),
        acceptance_policy: Some("ai-only".to_owned()),
    }
}

/// The row as a degraded read leaves it: the bottom-of-list rank sentinel, no
/// title, and no policy the console can report.
fn degraded_row() -> ReadyRow {
    ReadyRow {
        rank: "~".to_owned(),
        title: None,
        acceptance_policy: None,
    }
}

#[test]
fn a_re_observed_prior_state_refreshes_the_ready_row_instead_of_pinning_it()
-> Result<(), ConsoleRuntimeError> {
    let probe = SequencedProbe {
        stdouts: vec![GOOD, DEGRADED, GOOD, GOOD],
        cursor: std::cell::Cell::new(0),
    };
    let adapter = ObservedSourceAdapter::new(
        &probe,
        SourceAdapterKind::Orchestrator,
        REPO,
        SourceObservationPlan::command("list-work-items", &["--json"]),
        parse_orchestrator_observation,
    )?;
    let sources: Vec<(&str, &dyn PullSourcePort)> =
        vec![("orchestrator:livespec-console-beads-fabro", &adapter)];
    let port = EmptyNeedsAttentionPort;
    let needs_attention = NeedsAttentionIngest::new(&port, REPO);
    let mut store = SqliteEventStore::open_in_memory()?;

    // Poll 1 — the row carries the ledger's rank, title and armed policy.
    refresh_sources(
        &mut store,
        "2026-09-08T00:00:01Z",
        &sources,
        &needs_attention,
        &test_writer_identity(),
    )?;
    assert_eq!(ready_row(&store)?, Some(good_row()));

    // Poll 2 — the degraded read is observed honestly: the row goes stale, and
    // rank / title / acceptance_policy degrade TOGETHER, exactly as walk #5
    // measured against the ledger.
    refresh_sources(
        &mut store,
        "2026-09-08T00:00:02Z",
        &sources,
        &needs_attention,
        &test_writer_identity(),
    )?;
    assert_eq!(ready_row(&store)?, Some(degraded_row()));

    // Poll 3 — THE REFRESH BOUNDARY. The source recovers to the state it held
    // at poll 1. The row must follow it back on the NEXT poll, with no operator
    // action and no unrelated command forcing anything. Before the fix the
    // recovered observation re-derived poll 1's content-addressed event id, was
    // swallowed as a duplicate, and this row stayed `rank ~` forever.
    refresh_sources(
        &mut store,
        "2026-09-08T00:00:03Z",
        &sources,
        &needs_attention,
        &test_writer_identity(),
    )?;
    assert_eq!(ready_row(&store)?, Some(good_row()));

    // Poll 4 — re-observing the SAME state again is still idempotent: the row
    // holds, and the fix must not turn every poll into a fresh append.
    let before = store.list_console_events()?.len();
    refresh_sources(
        &mut store,
        "2026-09-08T00:00:04Z",
        &sources,
        &needs_attention,
        &test_writer_identity(),
    )?;
    assert_eq!(ready_row(&store)?, Some(good_row()));
    assert_eq!(
        before,
        store.list_console_events()?.len(),
        "an unchanged re-observation must append nothing"
    );
    Ok(())
}
