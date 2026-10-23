//! A down STANDBY factory must not blind the reconcile-runs source
//! (livespec-console-beads-fabro-rhh56m AC1/AC2).
//!
//! MEASURED 2026-09-12 on the real store: `.livespec.jsonc` declares two
//! factories, `hp` (where every run executes) and `vps` (a deliberate backup
//! whose fabro-server unit is inactive, so its tailnet proxy answers 502). The
//! reconciler surveys both, collects hp's runs successfully, and exits non-zero
//! because the vps leg failed -- so the console lost reconciliation visibility
//! ENTIRELY, permanently, for a factory nobody routes work to. The information
//! the lane needed was in the very invocation that got branded unreachable, and
//! the envelope says in structured form which leg failed and why.
//!
//! The adapter honesty rule's own principle is that a per-ITEM problem must not
//! be promoted into an uninterpretable payload for the whole source. A survey
//! LEG that could not be reached is that same shape one level up: the legs that
//! answered are observations the console genuinely made, and the leg that did
//! not is named rather than erased. Its own source instance (`fabro:vps`) is
//! what carries that factory's unavailability, by name -- which is where an
//! operator reads it.

use console_application::source_adapters::{
    AdapterError, AdapterPollRequest, ObservedSourceAdapter, PullSourcePort, SourceAdapterKind,
    SourceObservationPlan, SourcePayload, SourceProbe, SourceProbeOutcome,
    parse_reconcile_runs_observation,
};
use console_domain::EventType;

/// The envelope the real `dispatcher.py reconcile-runs --dry-run --json`
/// printed on 2026-09-12 beside its non-zero exit, trimmed to the fields the
/// console reads: both factories surveyed, hp's orphan collected, and the vps
/// leg's failure named in `errors[]`.
const TWO_FACTORIES_ONE_DOWN: &str = r#"{
  "dry_run": true,
  "factories_surveyed": 2,
  "reconciled": [
    {
      "run_id": "run-hp-1",
      "factory_name": "hp",
      "status_kind": "running",
      "work_item_id": "livespec-console-beads-fabro-abc123",
      "work_item_status": "in_progress",
      "orphan_reason": "item-closed",
      "termination_route": "terminate"
    }
  ],
  "errors": [
    {
      "factory_name": "vps",
      "factory_server_url": "https://vps.perch-rudd.ts.net:32276",
      "reason": "factory-ps-failed",
      "detail": "fabro ps exited 1"
    }
  ]
}"#;

/// The same survey with EVERY leg failed: nothing was observed anywhere.
const EVERY_FACTORY_DOWN: &str = r#"{
  "dry_run": true,
  "factories_surveyed": 2,
  "reconciled": [],
  "errors": [
    {"factory_name": "hp", "reason": "factory-ps-failed"},
    {"factory_name": "vps", "reason": "factory-ps-failed"}
  ]
}"#;

/// The verbatim diagnostic the real probe attached to that non-zero exit.
const NON_ZERO_EXIT_DIAGNOSTIC: &str =
    "source command exited non-zero (exit status: 1): stderr: fabro ps exited 1";

/// A probe whose command exits NON-ZERO while still printing a readable
/// envelope -- the measured shape, not a synthesized one.
struct FailedExitWithPayloadProbe {
    stdout: String,
}

impl SourceProbe for FailedExitWithPayloadProbe {
    fn run_command(&self, _program: &str, _args: &[&str]) -> SourceProbeOutcome {
        SourceProbeOutcome::observed_failed(&self.stdout, NON_ZERO_EXIT_DIAGNOSTIC)
    }

    fn read_file(&self, _path: &str) -> SourceProbeOutcome {
        SourceProbeOutcome::unavailable("this fixture observes a command, not a file")
    }
}

/// Poll the reconcile-runs adapter over `stdout`, returning each emitted event
/// kind paired with the surfaced reasons its payload carries (empty for the
/// availability markers, which carry none).
fn poll_reconcile_runs(stdout: &str) -> Result<Vec<(EventType, String)>, AdapterError> {
    let probe = FailedExitWithPayloadProbe {
        stdout: stdout.to_owned(),
    };
    let adapter = ObservedSourceAdapter::new(
        &probe,
        SourceAdapterKind::Reconciler,
        "livespec-console-beads-fabro",
        SourceObservationPlan::command("dispatcher", &["reconcile-runs", "--dry-run", "--json"]),
        parse_reconcile_runs_observation,
    )?;
    let request = AdapterPollRequest::new("reconcile-runs:livespec-console-beads-fabro", None, 0)?;
    let poll = adapter.poll(&request)?;
    Ok(poll
        .events()
        .iter()
        .map(|event| {
            (
                *event.event().event_type(),
                match event.payload() {
                    SourcePayload::ReconcileRunsSnapshot(snapshot) => {
                        snapshot.skipped().join(" | ")
                    }
                    _other => String::new(),
                },
            )
        })
        .collect())
}

/// AC1/AC2: with two factories configured and one failing, the reconcile-runs
/// source is NOT reported unavailable -- no
/// `source.not_observed_finding_observed` is emitted at all, and the positive
/// observation marker that clears the header's tally IS.
#[test]
fn a_down_standby_factory_does_not_make_reconcile_runs_unavailable() -> Result<(), AdapterError> {
    let emitted = poll_reconcile_runs(TWO_FACTORIES_ONE_DOWN)?;
    let kinds = emitted
        .iter()
        .map(|(kind, _reason)| *kind)
        .collect::<Vec<_>>();
    assert!(
        !kinds.contains(&EventType::SourceNotObservedFindingObserved),
        "a reachable factory's runs were collected in this very invocation, so \
         reconcile-runs must not read unavailable: {kinds:?}"
    );
    assert!(
        kinds.contains(&EventType::SourceObservedFindingObserved),
        "the partial observation must emit the positive marker that clears the \
         header's unavailability tally: {kinds:?}"
    );
    Ok(())
}

/// AC1: the source reports the reachable factory's runs WITH the unreachable
/// factory named, from the reconciler's own structured output -- never a silent
/// partial lane that reads as a clean one.
#[test]
fn the_unreachable_factory_is_named_in_the_reported_observation() -> Result<(), AdapterError> {
    let emitted = poll_reconcile_runs(TWO_FACTORIES_ONE_DOWN)?;
    let orphans = emitted
        .iter()
        .find(|(kind, _reason)| *kind == EventType::FactoryRunOrphansObserved)
        .map(|(_kind, reason)| reason.clone())
        .unwrap_or_default();
    assert!(
        orphans.contains("vps") && orphans.contains("factory-ps-failed"),
        "the surveyed-but-unreachable factory and its reason must be surfaced \
         with the runs that were collected: {orphans:?}"
    );
    Ok(())
}

/// AC1's boundary, in the OTHER direction: when the envelope reports EVERY
/// surveyed factory as failed, nothing was observed and the source IS
/// unavailable. A partial-availability rule that swallowed a total outage would
/// be the cockpit-blind screen this bug's own defect 1 complains about, with
/// the sign flipped.
#[test]
fn every_factory_failing_is_still_an_unavailable_source() -> Result<(), AdapterError> {
    let kinds = poll_reconcile_runs(EVERY_FACTORY_DOWN)?
        .into_iter()
        .map(|(kind, _reason)| kind)
        .collect::<Vec<_>>();
    assert!(
        kinds.contains(&EventType::SourceNotObservedFindingObserved),
        "no factory answered, so the source observed nothing: {kinds:?}"
    );
    Ok(())
}
