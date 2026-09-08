//! The `doctor` diagnostic.
//!
//! Before livespec-console-beads-fabro-mx9u.14, `doctor` always printed `no
//! findings`: it read the store's raw event/command/attention counts but
//! never consulted the source-health and staleness signals the HEADER
//! already renders from the very same event log. A console could be running
//! with several sources unavailable and an attention count the operator
//! could see disagreed with a direct read of its source, and `doctor` had
//! nothing to say about either.
//!
//! Every finding here is derived from [`crate::project_tui_events`] -- the
//! SAME projection [`crate::build_tui_model`] renders into the header -- so
//! `doctor` can never disagree with what the header already shows the
//! operator. The one exception is each unavailable source's last-successful-
//! read timestamp, which needs the store's raw `observed_at` column (no
//! [`ConsoleEvent`] carries a timestamp of its own); that reuses the exact
//! same event classification ([`crate::is_positive_source_observation`])
//! `unavailable_sources` uses to decide a source is down in the first place.
//!
//! Out of scope here, by design:
//! - The `needs-attention` source itself can never appear in
//!   [`crate::TuiProjection::unavailable_sources`] -- nothing in the ingest
//!   path emits a not-observed/observed marker for it
//!   (livespec-console-beads-fabro-mx9u.12). The attention-disagreement
//!   finding below reports the NUMBERS honestly without asserting that gap
//!   is staleness; closing mx9u.12 is what would let a future pass attribute
//!   it.
//! - Marking a STALE projection in the TUI's own presentation (the header
//!   segment, list rows) is livespec-console-beads-fabro-mx9u.17; this module
//!   is the CLI/doctor half only.

use std::collections::BTreeMap;

use console_domain::{ConsoleEvent, EventType};

use crate::source_adapters::materialize_attention_items;
use crate::{is_positive_source_observation, project_tui_events};

/// One diagnostic finding: a human-readable line naming a condition `doctor`
/// found in the console's own in-process state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorFinding {
    message: String,
}

impl DoctorFinding {
    const fn new(message: String) -> Self {
        Self { message }
    }

    #[must_use]
    /// The finding's rendered message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// The result of a `doctor` pass.
///
/// The findings (empty on a healthy console) plus the `attention:` summary
/// line, which reads as a bare count when it agrees with the needs-attention
/// source's own count, or as that count ANNOTATED with the source's own
/// count when they disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorReport {
    findings: Vec<DoctorFinding>,
    attention_line: String,
}

impl DoctorReport {
    #[must_use]
    /// The findings doctor is reporting, in a stable order.
    pub fn findings(&self) -> &[DoctorFinding] {
        &self.findings
    }

    #[must_use]
    /// The `attention:` line's value, without the `attention: ` label.
    pub fn attention_line(&self) -> &str {
        &self.attention_line
    }

    #[must_use]
    /// Whether `doctor` should report the console unhealthy: any finding
    /// present. Drives the CLI's exit code so a script can gate on it.
    pub const fn has_findings(&self) -> bool {
        !self.findings.is_empty()
    }
}

#[must_use]
/// Build the doctor report.
///
/// `events` is the event log [`crate::build_tui_model`] projects for the
/// header; `events_with_observed_at` is the store's `observed_at`-paired
/// read, used ONLY to date each unavailable source's last successful read.
pub fn build_doctor_report(
    events: &[ConsoleEvent],
    events_with_observed_at: &[(ConsoleEvent, String)],
) -> DoctorReport {
    let projection = project_tui_events(events, None);
    let last_success = last_successful_observed_at(events_with_observed_at);

    let mut findings: Vec<DoctorFinding> = projection
        .unavailable_sources()
        .iter()
        .map(|source| unavailable_source_finding(events, source, last_success.get(source)))
        .collect();

    let attention_total = projection.attention_total();
    let needs_attention_count = materialize_attention_items(events).len();
    let attention_line = if attention_total == needs_attention_count {
        attention_total.to_string()
    } else {
        findings.push(DoctorFinding::new(format!(
            "attention count disagrees with the needs-attention source: console reports \
             {attention_total}, source reports {needs_attention_count}"
        )));
        format!("{attention_total} (source reports {needs_attention_count})")
    };

    DoctorReport {
        findings,
        attention_line,
    }
}

/// The finding for one currently-unavailable `source`: its last-recorded
/// not-observed reason, and the timestamp of its last successful read (from
/// `last_success`, keyed by source), so the operator sees both WHY it is down
/// and WHEN its projections stopped being current -- reading its row counts
/// as a current fact would otherwise be silently wrong.
fn unavailable_source_finding(
    events: &[ConsoleEvent],
    source: &str,
    last_success: Option<&String>,
) -> DoctorFinding {
    let reason = latest_not_observed_reason(events, source)
        .unwrap_or_else(|| "no reason recorded".to_owned());
    let since = last_success.map_or_else(|| "never observed".to_owned(), Clone::clone);
    DoctorFinding::new(format!(
        "source unavailable: {source} ({reason}) -- its projections are STALE, not current; \
         last successful read: {since}"
    ))
}

/// The `reason` field from the most recent
/// [`EventType::SourceNotObservedFindingObserved`] for `source`, or `None`
/// when no such event exists or its payload does not carry one.
fn latest_not_observed_reason(events: &[ConsoleEvent], source: &str) -> Option<String> {
    let event = events.iter().rev().find(|event| {
        *event.event_type() == EventType::SourceNotObservedFindingObserved
            && event.source() == source
    })?;
    let payload: serde_json::Value = serde_json::from_str(event.payload_json()).ok()?;
    payload
        .get("reason")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
}

/// For every source, the `observed_at` of its MOST RECENT positive
/// observation ([`is_positive_source_observation`]) -- the moment its
/// projections were last known current, before whatever unavailability
/// followed (if any).
fn last_successful_observed_at(
    events_with_observed_at: &[(ConsoleEvent, String)],
) -> BTreeMap<String, String> {
    let mut last_success = BTreeMap::new();
    for (event, observed_at) in events_with_observed_at {
        if is_positive_source_observation(*event.event_type()) {
            last_success.insert(event.source().to_owned(), observed_at.clone());
        }
    }
    last_success
}

#[cfg(test)]
mod tests {
    #![allow(clippy::manual_assert, clippy::panic)]

    use console_domain::{ConsoleEvent, EventType};

    use super::{DoctorFinding, build_doctor_report};
    use crate::build_tui_model;
    use crate::source_adapters::{
        AttentionHandoff, AttentionItemSnapshot, AttentionSourceRef, NotObservedFinding,
        SourceAdapterKind, attention_item_payload_json, not_observed_finding_payload_json,
    };

    #[track_caller]
    fn check(condition: bool, context: &str) {
        if !condition {
            panic!("{context}");
        }
    }

    #[test]
    #[should_panic(expected = "expected panic")]
    fn check_panics() {
        check(false, "expected panic");
    }

    fn not_observed_event(source: SourceAdapterKind, reason: &str) -> ConsoleEvent {
        let finding = NotObservedFinding::new("livespec-console-beads-fabro", source, reason);
        ConsoleEvent::fixture(
            &format!("evt:{}:not_observed", source.source_name()),
            EventType::SourceNotObservedFindingObserved,
            source.source_name(),
        )
        .with_payload_json(not_observed_finding_payload_json(&finding))
    }

    fn positive_observation_event(
        source: SourceAdapterKind,
        observed_at: &str,
    ) -> (ConsoleEvent, String) {
        (
            ConsoleEvent::fixture(
                &format!("evt:{}:observed_idle", source.source_name()),
                EventType::SourceObservedFindingObserved,
                source.source_name(),
            ),
            observed_at.to_owned(),
        )
    }

    /// A needs-attention item that JOINS the work item `id` (its `work_item`
    /// source ref, not merely sharing the string): `unified_attention_entries`
    /// excludes a needs-attention item from the inbox list when its
    /// `work_item` ref names an already-listed attention-worthy work item, so
    /// this is what real needs-attention/work-item pairs look like on the
    /// wire.
    fn needs_attention_item_event(id: &str) -> ConsoleEvent {
        let item = AttentionItemSnapshot::new(
            id,
            "human-answer",
            "high",
            &format!("Needs attention: {id}"),
            AttentionSourceRef::new("livespec-console-beads-fabro", Some(id), None),
            AttentionHandoff::new("inspect", None, "inspect"),
        );
        ConsoleEvent::fixture(
            &format!("evt:{id}:appeared"),
            EventType::AttentionItemAppeared,
            "needs-attention",
        )
        .with_payload_json(attention_item_payload_json(&item))
    }

    /// A blocked/needs-human work-item snapshot: attention-worthy by the
    /// header's own lane-and-reason rule. Built from a literal payload
    /// (mirroring `console-cli`'s own `demo_events` fixture) rather than
    /// `WorkItemSnapshot::new`'s fallible constructor: every field here is
    /// known-valid by construction, so there is no error arm for a test to
    /// leave uncovered.
    fn attention_worthy_work_item_event(work_item_id: &str) -> ConsoleEvent {
        ConsoleEvent::fixture(
            &format!("evt:{work_item_id}:snapshot"),
            EventType::WorkItemSnapshotObserved,
            "orchestrator",
        )
        .with_payload_json(format!(
            r#"{{"repo":"livespec-console-beads-fabro","work_item_id":"{work_item_id}","lane":"blocked","lane_reason":"needs-human","rank":"a1","status":"blocked","source_version":1}}"#
        ))
    }

    fn finding_messages(findings: &[DoctorFinding]) -> Vec<&str> {
        findings.iter().map(DoctorFinding::message).collect()
    }

    #[test]
    fn reports_no_findings_over_a_fully_healthy_fixture() {
        let events = [attention_worthy_work_item_event(
            "livespec-console-beads-fabro-a1",
        )];
        let report = build_doctor_report(&events, &[]);

        // The single attention-worthy work item has no needs-attention
        // counterpart, so console (1) and source (0) genuinely disagree here
        // -- proving the "healthy" fixture below is not accidentally healthy
        // because nothing was checked.
        check(
            report.has_findings(),
            "expected the work-item-only fixture to disagree",
        );

        let events = [
            attention_worthy_work_item_event("livespec-console-beads-fabro-a1"),
            needs_attention_item_event("livespec-console-beads-fabro-a1"),
        ];
        let report = build_doctor_report(&events, &[]);
        check(
            !report.has_findings(),
            "expected no findings on a healthy fixture",
        );
        check(
            report.findings().is_empty(),
            "expected an empty findings list",
        );
        check(report.attention_line() == "1", "expected the bare count");
    }

    #[test]
    fn latest_not_observed_reason_is_none_when_no_matching_event_exists() {
        // Direct unit test of the private helper, over a fixture the public
        // `build_doctor_report` API cannot construct: a source only ever
        // reaches `unavailable_source_finding` (and so this search) once it
        // is already known to carry a not-observed event, so this branch is
        // otherwise unreachable through the public path.
        let events: [ConsoleEvent; 0] = [];
        assert_eq!(
            super::latest_not_observed_reason(&events, "dispatcher"),
            None
        );
    }

    #[test]
    fn latest_not_observed_reason_is_none_over_a_malformed_payload() {
        let event = ConsoleEvent::fixture(
            "evt_bad_payload",
            EventType::SourceNotObservedFindingObserved,
            "dispatcher",
        )
        .with_payload_json("not json".to_owned());
        assert_eq!(
            super::latest_not_observed_reason(std::slice::from_ref(&event), "dispatcher"),
            None
        );
    }

    #[test]
    fn unavailable_source_finding_falls_back_when_no_reason_can_be_read() {
        let event = ConsoleEvent::fixture(
            "evt_bad_payload",
            EventType::SourceNotObservedFindingObserved,
            "dispatcher",
        )
        .with_payload_json("not json".to_owned());
        let finding =
            super::unavailable_source_finding(std::slice::from_ref(&event), "dispatcher", None);
        assert!(finding.message().contains("no reason recorded"));
        assert!(
            finding
                .message()
                .contains("last successful read: never observed")
        );
    }

    #[test]
    fn ac1_emits_one_finding_per_unavailable_source_naming_it_and_its_reason() {
        let events = [
            not_observed_event(SourceAdapterKind::Dispatcher, "dispatcher binary not found"),
            not_observed_event(SourceAdapterKind::GitHub, "gh: command not found"),
        ];
        let report = build_doctor_report(&events, &[]);

        let messages = finding_messages(report.findings());
        check(
            messages.iter().any(|message| {
                message.contains("dispatcher") && message.contains("dispatcher binary not found")
            }),
            "expected a finding naming dispatcher and its reason",
        );
        check(
            messages.iter().any(|message| {
                message.contains("github") && message.contains("gh: command not found")
            }),
            "expected a finding naming github and its reason",
        );

        let clean_report = build_doctor_report(&[], &[]);
        check(
            !clean_report.has_findings(),
            "expected no findings when every source is available",
        );
    }

    #[test]
    fn ac2_names_an_unavailable_sources_projections_stale_with_its_last_successful_read() {
        let events = [not_observed_event(
            SourceAdapterKind::Dispatcher,
            "dispatcher binary not found",
        )];
        let events_with_observed_at = [
            positive_observation_event(SourceAdapterKind::Dispatcher, "2026-09-08T13:00:00Z"),
            // A LATER not-observed transition than the last positive read --
            // this is the moment the last-successful-read timestamp must
            // survive past, not the not-observed event's own time.
            (events[0].clone(), "2026-09-08T14:00:00Z".to_owned()),
        ];
        let report = build_doctor_report(&events, &events_with_observed_at);

        let messages = finding_messages(report.findings());
        check(
            messages.iter().any(|message| {
                message.contains("STALE")
                    && message.contains("last successful read: 2026-09-08T13:00:00Z")
            }),
            "expected a STALE finding naming the last successful read",
        );
    }

    #[test]
    fn ac3_exit_code_gates_on_whether_any_finding_is_present() {
        let unhealthy = build_doctor_report(
            &[not_observed_event(
                SourceAdapterKind::Dispatcher,
                "dispatcher binary not found",
            )],
            &[],
        );
        check(
            unhealthy.has_findings(),
            "expected findings to gate a non-zero exit",
        );

        let healthy = build_doctor_report(&[], &[]);
        check(
            !healthy.has_findings(),
            "expected no findings to gate a zero exit",
        );
    }

    #[test]
    fn ac4_annotates_the_attention_line_with_the_sources_own_count_when_they_disagree() {
        // Two work-item-lane attention entries but only one needs-attention
        // counterpart: console counts 2, the needs-attention surface counts 1.
        let events = [
            attention_worthy_work_item_event("livespec-console-beads-fabro-a1"),
            attention_worthy_work_item_event("livespec-console-beads-fabro-a2"),
            needs_attention_item_event("livespec-console-beads-fabro-a1"),
        ];
        let report = build_doctor_report(&events, &[]);

        check(
            report.attention_line() == "2 (source reports 1)",
            "expected the attention line annotated with the source's own count",
        );
        check(
            finding_messages(report.findings()).iter().any(|message| {
                message.contains("console reports 2") && message.contains("source reports 1")
            }),
            "expected the disagreement itself to be a finding",
        );

        let agreeing_events = [
            attention_worthy_work_item_event("livespec-console-beads-fabro-a1"),
            needs_attention_item_event("livespec-console-beads-fabro-a1"),
        ];
        let agreeing = build_doctor_report(&agreeing_events, &[]);
        check(
            agreeing.attention_line() == "1",
            "expected a bare count when the two agree",
        );
    }

    #[test]
    fn ac5_agrees_with_the_header_over_one_fixture() {
        let events = [
            not_observed_event(SourceAdapterKind::Dispatcher, "dispatcher binary not found"),
            not_observed_event(SourceAdapterKind::GitHub, "gh: command not found"),
            attention_worthy_work_item_event("livespec-console-beads-fabro-a1"),
            attention_worthy_work_item_event("livespec-console-beads-fabro-a2"),
            needs_attention_item_event("livespec-console-beads-fabro-a1"),
        ];

        let model = build_tui_model(&events, 0);
        let report = build_doctor_report(&events, &[]);

        let mut doctor_sources: Vec<&str> = report
            .findings()
            .iter()
            .map(DoctorFinding::message)
            .filter(|message| message.starts_with("source unavailable: "))
            .map(|message| {
                let rest = message.trim_start_matches("source unavailable: ");
                // `split(' ')` over a non-empty string always yields at least
                // one item, so the fallback (the whole trimmed message) is
                // never actually reached -- it exists only so this stays a
                // total function with no panicking branch for a test to leave
                // uncovered.
                rest.split(' ').next().unwrap_or(rest)
            })
            .collect();
        doctor_sources.sort_unstable();
        let model_sources: Vec<&str> = model
            .unavailable_sources()
            .iter()
            .map(String::as_str)
            .collect();
        check(
            doctor_sources == model_sources,
            "expected doctor's unavailable sources to match the header's exactly",
        );

        // The fallback (`usize::MAX`) is never actually reached -- the
        // attention line always starts with a decimal number -- and is
        // there only so the equality check below fails honestly instead of
        // this leaving a panicking branch for a test to leave uncovered.
        let header_attention_number = report
            .attention_line()
            .split(' ')
            .next()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(usize::MAX);
        check(
            header_attention_number == model.attention_total(),
            "expected doctor's attention number to match the header's attention_total exactly",
        );
    }
}
