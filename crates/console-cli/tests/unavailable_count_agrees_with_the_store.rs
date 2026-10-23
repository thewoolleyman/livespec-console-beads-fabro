//! The header's unavailable COUNT equals what the store records as
//! not-observed at the latest poll (livespec-console-beads-fabro-rhh56m
//! AC5/AC6), and each per-factory source carries its OWN last-successful read
//! (that item's 2026-09-29 sharpening of AC3).
//!
//! MEASURED 2026-09-12T19:04Z on the real store: the header said THREE
//! unavailable while the store recorded exactly TWO not-observed sources.
//! `livespec-console-beads-fabro-25rvmd` had already fixed a dedup defect in
//! this same tally, so the number is demonstrably easy to get wrong in more
//! than one way -- which is why this gate measures the count against an
//! INDEPENDENT record of the same fact rather than against a literal.
//!
//! The independent record is the CHECKPOINT table. Every poll writes one
//! checkpoint per adapter carrying that poll's availability verdict
//! (`checkpoint_reflects_a_successful_poll`), while the tally the header
//! renders is folded from the EVENT LOG. Two separate stores of the same
//! outcome, written by the same poll: if the header's count ever disagrees
//! with the checkpoint table's, one of them is lying about what the console
//! just observed.
//!
//! The 2026-09-29 root-cause finding is gated here too.
//! `checkpoint_source_last_success` reduced an adapter id by splitting on the
//! FIRST colon, which is the pre-z4y7kp `<source>:<repo>` shape; z4y7kp made
//! the ids `<source>:<factory>:<repo>`, so `fabro:hp:<repo>` and
//! `fabro:vps:<repo>` BOTH collapsed onto the bare family name `fabro`. On the
//! real store that credited hp's successful poll to a RETIRED `fabro` row --
//! whose roster line then showed "last successful read: 2026-09-29T08:38:40Z",
//! a timestamp belonging to a different source -- and left both per-factory
//! sources with no checkpoint-derived last-success of their own.

use console_application::build_tui_model_for_state;
use console_application::source_adapters::{
    NeedsAttentionReadOutcome, NeedsAttentionSnapshotPort, ObservedSourceAdapter, PullSourcePort,
    SourceAdapterKind, SourceObservationPlan, SourceProbe, SourceProbeOutcome,
    checkpoint_reflects_a_successful_poll, parse_orchestrator_observation,
};
use console_application::writer_identity::WriterIdentity;
use console_application::{TuiInteractionState, TuiOverlay};
use console_eventstore::SqliteEventStore;
use livespec_console_beads_fabro::{
    ConsoleRuntimeError, NeedsAttentionIngest, adapter_id_source_name,
    checkpoint_source_last_success, refresh_sources,
};

const REPO: &str = "livespec-console-beads-fabro";

fn test_writer_identity() -> WriterIdentity {
    WriterIdentity::new(
        4242,
        "/opt/console/test-binary",
        "/data/projects/repo",
        "test1234",
    )
}

/// One ready work-item, varied per successful poll so each recovery lands a
/// genuinely new data event rather than deduping away (the mx9u.22 case is
/// already gated by its own test; this one is about the COUNT).
fn work_items(title: &str) -> String {
    format!(
        concat!(
            r#"[{{"id":"wi-1","lane":"ready","rank":"a0","status":"ready","#,
            r#""title":"{}","acceptance_policy":"ai-only"}}]"#
        ),
        title
    )
}

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

struct EmptyNeedsAttentionPort;

impl NeedsAttentionSnapshotPort for EmptyNeedsAttentionPort {
    fn read_snapshot(&self) -> NeedsAttentionReadOutcome {
        NeedsAttentionReadOutcome::Observed(Vec::new())
    }
}

/// The sources the store records as not-observed at the LATEST poll, read from
/// the CHECKPOINT table rather than from the event log the header folds.
fn not_observed_at_the_latest_poll(
    store: &SqliteEventStore,
) -> Result<Vec<String>, ConsoleRuntimeError> {
    let mut sources = store
        .list_checkpoints()?
        .into_iter()
        .filter(|(_adapter_id, checkpoint_json, _advanced_at)| {
            !checkpoint_reflects_a_successful_poll(checkpoint_json)
        })
        .map(|(adapter_id, _checkpoint_json, _advanced_at)| {
            adapter_id_source_name(&adapter_id).to_owned()
        })
        .collect::<Vec<_>>();
    sources.sort();
    sources.dedup();
    Ok(sources)
}

/// AC5/AC6: across a down -> up -> down-again sequence, the header's count and
/// names equal what the checkpoint table records for the latest poll, every
/// time. A source that recovers and fails again is the case a tally with any
/// memory of its own gets wrong.
#[test]
fn the_header_count_tracks_the_store_through_a_recovery_and_a_second_outage()
-> Result<(), ConsoleRuntimeError> {
    let probe = SequencedProbe {
        outcomes: vec![
            SourceProbeOutcome::Unavailable {
                reason: "orchestrator ledger unreachable".to_owned(),
            },
            SourceProbeOutcome::observed(&work_items("recovered"), true),
            SourceProbeOutcome::Unavailable {
                reason: "orchestrator ledger unreachable again".to_owned(),
            },
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
    let state = TuiInteractionState::new(0, TuiOverlay::None)
        .with_current_source_names(vec!["orchestrator".to_owned()]);

    for (poll, expected) in [
        (1_u8, vec!["orchestrator".to_owned()]),
        (2, Vec::new()),
        (3, vec!["orchestrator".to_owned()]),
    ] {
        refresh_sources(
            &mut store,
            &format!("2026-09-12T19:0{poll}:00Z"),
            &sources,
            &needs_attention,
            &test_writer_identity(),
        )?;
        let recorded = not_observed_at_the_latest_poll(&store)?;
        assert_eq!(
            recorded, expected,
            "poll {poll}: the checkpoint table must record the outcome this \
             poll actually had"
        );
        let model = build_tui_model_for_state(&store.list_console_events()?, &state);
        assert_eq!(
            model.unavailable_sources(),
            recorded.as_slice(),
            "poll {poll}: the header's tally must equal what the store records \
             as not-observed at the latest poll"
        );
    }

    Ok(())
}

/// The 2026-09-29 sharpening of AC3: a stale `fabro:<repo>` checkpoint beside
/// live `fabro:hp:<repo>` and `fabro:vps:<repo>` rows. Each source carries its
/// OWN last-successful read, and hp's is not credited to the retired row.
#[test]
fn each_factory_carries_its_own_last_successful_read() -> Result<(), ConsoleRuntimeError> {
    let mut store = SqliteEventStore::open_in_memory()?;
    let observed = r#"{"schema_version":1,"source_checkpoint":"observed_idle","availability":"observed","transition_epoch":1}"#;
    let not_observed = r#"{"schema_version":1,"source_checkpoint":"not_observed","availability":"not_observed","transition_epoch":8}"#;

    // The RETIRED row: written by the pre-z4y7kp build, which polled an
    // unqualified `fabro` source. Nothing has advanced it since.
    store.save_checkpoint(
        "fabro:livespec-console-beads-fabro",
        observed,
        "2026-09-11T00:21:00Z",
    )?;
    // The sources the current build polls: hp healthy, vps the down standby.
    store.save_checkpoint(
        "fabro:hp:livespec-console-beads-fabro",
        observed,
        "2026-09-29T08:41:01Z",
    )?;
    store.save_checkpoint(
        "fabro:vps:livespec-console-beads-fabro",
        not_observed,
        "2026-09-29T08:41:02Z",
    )?;

    let last_success = checkpoint_source_last_success(&store)?;

    assert_eq!(
        last_success,
        vec![
            ("fabro".to_owned(), "2026-09-11T00:21:00Z".to_owned()),
            ("fabro:hp".to_owned(), "2026-09-29T08:41:01Z".to_owned()),
        ],
        "each adapter id must reduce to its OWN source instance name: hp's \
         successful poll belongs to `fabro:hp`, never to the retired `fabro`"
    );

    Ok(())
}
