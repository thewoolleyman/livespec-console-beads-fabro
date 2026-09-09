//! `livespec-console-beads-fabro-mx9u.23` AC5 — reconstructs the measured
//! eleven-hour double-write end to end and proves the fix bounds it.
//!
//! The postmortem: a second console process wrote to the SAME store as a
//! first one for roughly eleven hours, producing 1,828
//! `source.not_observed_finding_observed` markers for the SAME source
//! (`livespec`) — about 20% of a 9,039-event store, growing UNBOUNDED with
//! every poll cycle. Dedupe on `source_event_id` (shaped
//! `<source>:<repo>:not_observed:<epoch>`, where the epoch only advances on
//! an availability TRANSITION) proved it was genuinely TWO writers, not one
//! flapping process: 858 markers with 858 DISTINCT ids meant 858 separate
//! not-observed -> observed -> not-observed round trips, which only makes
//! sense if ONE process could resolve `livespec` (minting the recovery
//! flip) while ANOTHER, concurrently, could not (minting the next
//! not-observed).
//!
//! This test drives exactly that shape through the REAL `refresh_sources`
//! entry point against ONE shared store: a writer that always resolves the
//! source (`WRITER_RESOLVING`) alternating with a writer that never can
//! (`WRITER_FAILING`), both against the identical adapter id and source, for
//! many cycles. Before AC3/AC4's lease, both writers would race the shared
//! checkpoint and mint a fresh transition epoch — and therefore a fresh,
//! distinct marker — on every alternation, exactly reproducing the unbounded
//! growth. With the lease, only whichever writer acquires it first ever
//! actually polls; the other is held read-only for the entire run, so the
//! marker count is bounded by the ACTIVE writer's own steady state, not by
//! the number of cycles.

use console_application::source_adapters::{
    NeedsAttentionReadOutcome, NeedsAttentionSnapshotPort, ObservedSourceAdapter, PullSourcePort,
    SourceAdapterKind, SourceObservationPlan, SourceProbe, SourceProbeOutcome,
    parse_livespec_observation,
};
use console_application::writer_identity::WriterIdentity;
use console_domain::EventType;
use console_eventstore::SqliteEventStore;
use livespec_console_beads_fabro::{ConsoleRuntimeError, NeedsAttentionIngest, refresh_sources};

const REPO: &str = "livespec-console-beads-fabro";
const ADAPTER_ID: &str = "livespec:livespec-console-beads-fabro";

/// A needs-attention port holding nothing — this test is entirely about the
/// livespec source's own availability marker.
struct EmptyNeedsAttentionPort;

impl NeedsAttentionSnapshotPort for EmptyNeedsAttentionPort {
    fn read_snapshot(&self) -> NeedsAttentionReadOutcome {
        NeedsAttentionReadOutcome::Observed(Vec::new())
    }
}

/// A probe that ALWAYS resolves the source with a parseable payload —
/// the writer whose build could reach `livespec` at all.
struct AlwaysResolvingProbe;

impl SourceProbe for AlwaysResolvingProbe {
    fn run_command(&self, _program: &str, _args: &[&str]) -> SourceProbeOutcome {
        SourceProbeOutcome::observed(r#"{"action":"revise"}"#, true)
    }

    fn read_file(&self, _path: &str) -> SourceProbeOutcome {
        SourceProbeOutcome::Unavailable {
            reason: "this adapter observes a command, not a file".to_owned(),
        }
    }
}

/// A probe that can NEVER resolve the source — the bare-name spawn failure
/// the postmortem pinned as the second writer's own vintage.
struct NeverResolvingProbe;

impl SourceProbe for NeverResolvingProbe {
    fn run_command(&self, _program: &str, _args: &[&str]) -> SourceProbeOutcome {
        SourceProbeOutcome::Unavailable {
            reason: "livespec: command not found".to_owned(),
        }
    }

    fn read_file(&self, _path: &str) -> SourceProbeOutcome {
        SourceProbeOutcome::Unavailable {
            reason: "this adapter observes a command, not a file".to_owned(),
        }
    }
}

fn writer_identity(pid: u32, exe_path: &str, build_sha: &str) -> WriterIdentity {
    WriterIdentity::new(pid, exe_path, "/data/projects/repo", build_sha)
}

/// How many markers of `event_type` the store carries for the `livespec`
/// source.
fn livespec_marker_count(
    store: &SqliteEventStore,
    event_type: EventType,
) -> Result<usize, ConsoleRuntimeError> {
    Ok(store
        .list_console_events()?
        .iter()
        .filter(|event| event.source() == SourceAdapterKind::LiveSpec.source_name())
        .filter(|event| event.event_type() == &event_type)
        .count())
}

#[test]
fn two_writers_with_different_behaviour_cannot_grow_the_marker_stream_unbounded()
-> Result<(), ConsoleRuntimeError> {
    let mut store = SqliteEventStore::open_in_memory()?;
    let port = EmptyNeedsAttentionPort;
    let needs_attention = NeedsAttentionIngest::new(&port, REPO);

    let resolving_probe = AlwaysResolvingProbe;
    let resolving_adapter = ObservedSourceAdapter::new(
        &resolving_probe,
        SourceAdapterKind::LiveSpec,
        REPO,
        SourceObservationPlan::command("livespec", &["next", "--json"]),
        parse_livespec_observation,
    )?;
    let resolving_sources: Vec<(&str, &dyn PullSourcePort)> =
        vec![(ADAPTER_ID, &resolving_adapter)];
    let resolving_identity = writer_identity(1_111, "/opt/console/build-old", "aaaaaaa");

    let failing_probe = NeverResolvingProbe;
    let failing_adapter = ObservedSourceAdapter::new(
        &failing_probe,
        SourceAdapterKind::LiveSpec,
        REPO,
        SourceObservationPlan::command("livespec", &["next", "--json"]),
        parse_livespec_observation,
    )?;
    let failing_sources: Vec<(&str, &dyn PullSourcePort)> = vec![(ADAPTER_ID, &failing_adapter)];
    let failing_identity = writer_identity(2_222, "/opt/console/build-new", "bbbbbbb");

    // 12 cycles, alternating writers, well inside the lease TTL (30s) at one
    // second apart -- the SAME cadence the real poller runs on, just without
    // sleeping through it. The FAILING writer polls first, so it is the one
    // that acquires the lease fresh; the RESOLVING writer polls every other
    // cycle and must be held off for every one of them.
    for cycle in 0..12u32 {
        let observed_at = format!("2026-09-08T14:00:{cycle:02}Z");
        if cycle % 2 == 0 {
            refresh_sources(
                &mut store,
                &observed_at,
                &failing_sources,
                &needs_attention,
                &failing_identity,
            )?;
        } else {
            refresh_sources(
                &mut store,
                &observed_at,
                &resolving_sources,
                &needs_attention,
                &resolving_identity,
            )?;
        }
    }

    // THE ASSERTION AC5 REQUIRES: the marker count, not merely that a lease
    // was taken. Before the fix this would be 6 -- one fresh not-observed
    // marker per failing-writer cycle, each re-derived against a checkpoint
    // the OTHER writer's interleaved polls kept disturbing. With the lease,
    // the failing writer holds it for the whole run (it polled first and
    // renews every one of its own cycles inside the TTL), so its own
    // not-observed marker is minted ONCE and every later poll of the SAME
    // failure dedupes against it -- not because the source recovered, but
    // because nothing about ITS OWN observation ever changed.
    let not_observed_count =
        livespec_marker_count(&store, EventType::SourceNotObservedFindingObserved)?;
    assert_eq!(
        not_observed_count, 1,
        "the not-observed marker count must be bounded by the ACTIVE writer's \
         own steady state, not by the number of poll cycles -- got {not_observed_count} \
         after 12 cycles"
    );

    // The read-only writer must never have gotten a single poll through: no
    // positive observed marker, and no data snapshot from its side.
    let observed_count = livespec_marker_count(&store, EventType::SourceObservedFindingObserved)?;
    assert_eq!(
        observed_count, 0,
        "the resolving writer was held read-only for the entire run; a \
         positive observed marker would mean it wrote anyway"
    );
    let snapshot_count = livespec_marker_count(&store, EventType::LivespecNextSnapshotObserved)?;
    assert_eq!(
        snapshot_count, 0,
        "the resolving writer's data snapshot must never land while it holds \
         no lease"
    );

    // The lease itself still names the writer that actually ran, confirming
    // the bound above is the LEASE working, not an unrelated coincidence.
    let lease = store.read_writer_lease()?;
    assert!(lease.is_some(), "a lease must exist after 12 polls");
    if let Some((holder, _renewed_at)) = lease {
        assert_eq!(holder.pid(), 2_222);
        assert_eq!(holder.build_sha(), "bbbbbbb");
    }

    Ok(())
}
