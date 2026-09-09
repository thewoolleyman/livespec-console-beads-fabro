//! `livespec-console-beads-fabro-mx9u.22`: a source that recovers with data
//! IDENTICAL to what the store already holds must clear the header's
//! unavailable-sources tally.
//!
//! `ObservedSourceAdapter`'s data-bearing success branch computed the
//! observed-availability transition and then discarded it (`_transition_epoch`),
//! emitting only the parsed data events. Those events are CONTENT-ADDRESSED
//! (`console-eventstore`'s `insert or ignore` on `(source, source_event_id)`),
//! so when a previously-failing source recovers and reports the exact state the
//! store already recorded, the recovery poll inserts nothing at all. Nothing new
//! ever lands to clear the earlier `source.not_observed_finding_observed`
//! marker, so the header brands the source unavailable forever, in violation of
//! the adapter's own documented contract ("a transient failure is never branded
//! permanently").
//!
//! This test exercises the REAL failure mode end to end: a real
//! `ObservedSourceAdapter` poll sequence (succeed, fail, recover to the SAME
//! content) over the REAL `SqliteEventStore` dedup (not a hand-built event
//! fixture list), asserting the projected header actually clears on recovery.
//! A test that only asserts the marker event's SHAPE, or that folds a
//! hand-assembled event list, would pass even with the bug present -- the bug
//! lives in whether the marker is emitted at all when the data path dedupes
//! away.

use console_application::build_tui_model;
use console_application::source_adapters::{
    NeedsAttentionReadOutcome, NeedsAttentionSnapshotPort, ObservedSourceAdapter, PullSourcePort,
    SourceAdapterKind, SourceObservationPlan, SourceProbe, SourceProbeOutcome,
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

/// The orchestrator's `list-work-items --json` output for one ready item --
/// stable across polls 1 and 3 so poll 3 re-derives poll 1's content-addressed
/// event id and dedupes at the store.
const GOOD: &str = concat!(
    r#"[{"id":"wi-1","lane":"ready","rank":"a0","status":"ready","#,
    r#""title":"Recover after a transient failure","acceptance_policy":"ai-only"}]"#,
);

/// A probe replaying a scripted sequence of outcomes, one per successive poll,
/// repeating the last once exhausted.
struct SequencedProbe {
    outcomes: Vec<SourceProbeOutcome>,
    cursor: std::cell::Cell<usize>,
}

impl SourceProbe for SequencedProbe {
    fn run_command(&self, _program: &str, _args: &[&str]) -> SourceProbeOutcome {
        let index = self.cursor.get().min(self.outcomes.len().saturating_sub(1));
        self.cursor.set(index.saturating_add(1));
        self.outcomes
            .get(index)
            .cloned()
            .unwrap_or_else(|| SourceProbeOutcome::Unavailable {
                reason: "sequenced probe exhausted".to_owned(),
            })
    }

    fn read_file(&self, _path: &str) -> SourceProbeOutcome {
        SourceProbeOutcome::Unavailable {
            reason: "this adapter observes a command, not a file".to_owned(),
        }
    }
}

/// A needs-attention port holding nothing -- this test is entirely about the
/// orchestrator work-item adapter's own availability marker.
struct EmptyNeedsAttentionPort;

impl NeedsAttentionSnapshotPort for EmptyNeedsAttentionPort {
    fn read_snapshot(&self) -> NeedsAttentionReadOutcome {
        NeedsAttentionReadOutcome::Observed(Vec::new())
    }
}

#[test]
fn unavailable_tally_clears_when_a_recovered_read_dedupes_to_no_new_data_event()
-> Result<(), ConsoleRuntimeError> {
    let probe = SequencedProbe {
        outcomes: vec![
            SourceProbeOutcome::observed(GOOD, true),
            SourceProbeOutcome::Unavailable {
                reason: "orchestrator ledger unreachable".to_owned(),
            },
            // The recovery: identical content to poll 1, so the normalized
            // data event re-derives poll 1's content-addressed id and the
            // store dedupes it away as a Duplicate -- nothing new is inserted
            // on the data path alone.
            SourceProbeOutcome::observed(GOOD, true),
        ],
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

    // Poll 1 -- observed, holding data. Never unavailable.
    refresh_sources(
        &mut store,
        "2026-09-08T00:00:01Z",
        &sources,
        &needs_attention,
        &test_writer_identity(),
    )?;
    assert!(
        build_tui_model(&store.list_console_events()?, 0)
            .unavailable_sources()
            .is_empty(),
        "a source that only ever succeeded must never be branded unavailable"
    );

    // Poll 2 -- the source fails. The header brands it unavailable.
    refresh_sources(
        &mut store,
        "2026-09-08T00:00:02Z",
        &sources,
        &needs_attention,
        &test_writer_identity(),
    )?;
    assert_eq!(
        build_tui_model(&store.list_console_events()?, 0).unavailable_sources(),
        ["orchestrator".to_owned()],
        "a failed poll must brand its source unavailable"
    );

    // Poll 3 -- THE RECOVERY. The source returns the SAME data the store
    // already holds from poll 1, so the data event alone dedupes to nothing.
    // Before the fix, nothing at all lands on this poll and the source stays
    // branded unavailable forever, contradicting the adapter's own documented
    // contract that a transient failure is never branded permanently.
    refresh_sources(
        &mut store,
        "2026-09-08T00:00:03Z",
        &sources,
        &needs_attention,
        &test_writer_identity(),
    )?;
    assert!(
        build_tui_model(&store.list_console_events()?, 0)
            .unavailable_sources()
            .is_empty(),
        "a source recovering to data identical to what the store already holds \
         must still clear the unavailable tally -- the dedup-swallowed data \
         event must not be the only signal a recovery relies on"
    );

    Ok(())
}
