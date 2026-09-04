//! Red test for the general upstream-dependency gate
//! (`livespec-console-beads-fabro-pzbdbo.1`): drives the public API with
//! inline ledgers in the exact shape `bd list --status all --json -n 0` emits.

use console_upstream_dep_check::{Finding, Report, run};

fn proxy(id: &str, status: &str) -> String {
    format!(
        r#"{{"id":"{id}","title":"BLOCKED-ON orchestrator bd-ib-ott6: prepare steps","status":"{status}",
            "labels":["origin:freeform","upstream-dep:livespec-orchestrator-beads-fabro"],
            "metadata":{{"upstream_work_item_id":"bd-ib-ott6","plan_ref":"livespec-orchestrator-beads-fabro/console-control-plane-primitives"}},
            "description":"PROXY","dependencies":[]}}"#
    )
}

fn item(id: &str, status: &str, description: &str, depends_on: &[&str]) -> String {
    let deps = depends_on
        .iter()
        .map(|target| {
            format!(r#"{{"issue_id":"{id}","depends_on_id":"{target}","type":"blocks"}}"#)
        })
        .collect::<Vec<_>>()
        .join(",");
    let escaped = description.replace('"', "\\\"").replace('\n', "\\n");
    format!(
        r#"{{"id":"{id}","title":"item {id}","status":"{status}","labels":["origin:freeform"],
            "metadata":{{}},"description":"{escaped}","dependencies":[{deps}]}}"#
    )
}

fn ledger(entries: &[String]) -> String {
    format!("[{}]", entries.join(","))
}

/// Findings, with a parse error surfaced as a sentinel finding so an
/// assertion of "no findings" can never pass on an unreadable ledger.
fn findings(text: &str) -> Vec<Finding> {
    match run(text) {
        Ok(report) => report.findings,
        Err(error) => vec![Finding::DeviationWithoutProxy {
            id: "<ledger-error>".to_owned(),
            evidence: error,
        }],
    }
}

#[test]
fn a_well_formed_ledger_passes_and_reports_the_scan_size() {
    let text = ledger(&[
        proxy("c-p1", "blocked"),
        item(
            "c-held",
            "blocked",
            "deviations: literal prepare steps (bd-ib-ott6)",
            &["c-p1"],
        ),
        item("c-clean", "ready", "deviations: none", &[]),
    ]);
    assert_eq!(
        run(&text),
        Ok(Report {
            scanned: 3,
            findings: Vec::new()
        })
    );
}

#[test]
fn rule_a_a_proxy_that_is_not_blocked_is_refused() {
    let text = ledger(&[proxy("c-p1", "ready")]);
    assert_eq!(
        findings(&text),
        vec![Finding::ProxyNotBlocked {
            id: "c-p1".to_owned(),
            status: "ready".to_owned()
        }]
    );
}

#[test]
fn rule_a_a_closed_proxy_is_out_of_scope() {
    let text = ledger(&[proxy("c-p1", "closed")]);
    assert!(findings(&text).is_empty());
}

#[test]
fn rule_a_a_proxy_missing_plan_ref_is_refused() {
    let text = r#"[{"id":"c-p2","title":"BLOCKED-ON orchestrator bd-ib-6pzg: janitor","status":"blocked",
        "labels":["upstream-dep:livespec-orchestrator-beads-fabro"],
        "metadata":{"upstream_work_item_id":"bd-ib-6pzg"},"description":"","dependencies":[]}]"#;
    assert_eq!(
        findings(text),
        vec![Finding::ProxyMissingMetadata {
            id: "c-p2".to_owned(),
            field: "plan_ref"
        }]
    );
}

#[test]
fn rule_a_a_proxy_title_must_announce_the_block() {
    let text = r#"[{"id":"c-p3","title":"orchestrator bd-ib-6pzg: janitor","status":"blocked",
        "labels":["upstream-dep:livespec-orchestrator-beads-fabro"],
        "metadata":{"upstream_work_item_id":"bd-ib-6pzg","plan_ref":"t/s"},"description":"","dependencies":[]}]"#;
    assert_eq!(
        findings(text),
        vec![Finding::ProxyTitleNotBlockedOn {
            id: "c-p3".to_owned()
        }]
    );
}

#[test]
fn rule_b_a_recorded_deviation_with_no_proxy_is_refused() {
    let text = ledger(&[item(
        "c-dev",
        "ready",
        "acceptance:\ndeviations: literal prepare steps because pinned fabro cannot render them (bd-ib-ott6)",
        &[],
    )]);
    let found = findings(&text);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(matches!(&found[0], Finding::DeviationWithoutProxy { id, .. } if id == "c-dev"));
}

#[test]
fn rule_b_a_deviation_linked_to_a_proxy_passes() {
    let text = ledger(&[
        proxy("c-p1", "blocked"),
        item(
            "c-dev",
            "blocked",
            "deviations: literal prepare steps (bd-ib-ott6)",
            &["c-p1"],
        ),
    ]);
    assert!(findings(&text).is_empty());
}

#[test]
fn rule_b_a_marker_phrase_needs_an_upstream_reference_to_count() {
    let text = ledger(&[item(
        "c-env",
        "ready",
        "The documented in-wrapper env form is the workaround for the scrubbed variable.",
        &[],
    )]);
    assert!(findings(&text).is_empty());
}

#[test]
fn rule_b_a_marker_phrase_with_an_upstream_reference_is_refused() {
    let text = ledger(&[item(
        "c-hb",
        "active",
        "We hand-bridged the pack because the orchestrator janitor never installs it.",
        &[],
    )]);
    assert!(matches!(
        findings(&text).as_slice(),
        [Finding::DeviationWithoutProxy { id, .. }] if id == "c-hb"
    ));
}

#[test]
fn rule_b_the_guard_paragraph_never_counts_as_a_deviation() {
    let guard = "⛔ NEVER WORK AROUND AN UPSTREAM ORCHESTRATOR DEPENDENCY — never again. It never hand-bridges or writes a literal in place of an orchestrator primitive. workaround.";
    let text = ledger(&[item(
        "c-guard",
        "ready",
        &format!("{guard}\n\nDo the work."),
        &[],
    )]);
    assert!(findings(&text).is_empty());
}

#[test]
fn rule_b_a_backlog_filing_is_not_a_shipped_deviation() {
    let text = ledger(&[item(
        "c-bl",
        "backlog",
        "deviations: workaround for bd-ib-x",
        &[],
    )]);
    assert!(findings(&text).is_empty());
}

#[test]
fn rule_c_a_held_item_at_ready_is_refused() {
    let text = ledger(&[
        proxy("c-p1", "blocked"),
        item("c-held", "ready", "deviations: none", &["c-p1"]),
    ]);
    assert_eq!(
        findings(&text),
        vec![Finding::HeldItemDispatchable {
            id: "c-held".to_owned(),
            status: "ready".to_owned(),
            proxy: "c-p1".to_owned()
        }]
    );
}

#[test]
fn rule_c_a_closed_proxy_releases_the_hold() {
    let text = ledger(&[
        proxy("c-p1", "closed"),
        item("c-held", "ready", "deviations: none", &["c-p1"]),
    ]);
    assert!(findings(&text).is_empty());
}

#[test]
fn a_ledger_that_is_not_an_array_is_an_error_not_a_pass() {
    assert!(matches!(run(r#"{"issues":[]}"#), Err(error) if error.contains("array")));
}

#[test]
fn every_finding_names_a_failure_mode() {
    let all = [
        Finding::ProxyNotBlocked {
            id: "x".to_owned(),
            status: "ready".to_owned(),
        },
        Finding::ProxyTitleNotBlockedOn { id: "x".to_owned() },
        Finding::ProxyMissingMetadata {
            id: "x".to_owned(),
            field: "plan_ref",
        },
        Finding::DeviationWithoutProxy {
            id: "x".to_owned(),
            evidence: "e".to_owned(),
        },
        Finding::HeldItemDispatchable {
            id: "x".to_owned(),
            status: "ready".to_owned(),
            proxy: "p".to_owned(),
        },
    ];
    let modes = all.iter().map(Finding::failure_mode).collect::<Vec<_>>();
    assert_eq!(
        modes,
        [
            "upstream-dep-proxy-not-blocked",
            "upstream-dep-proxy-title",
            "upstream-dep-proxy-metadata-missing",
            "upstream-dep-deviation-without-proxy",
            "upstream-dep-held-item-dispatchable",
        ]
    );
    for finding in &all {
        assert!(finding.to_string().contains('x'), "{finding}");
    }
}
