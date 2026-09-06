//! Red test for the general upstream-dependency gate
//! (`livespec-console-beads-fabro-pzbdbo.1`): drives the public API with
//! inline ledgers in the exact shape `bd list --status all --json -n 0` emits.

use console_upstream_dep_check::{Finding, Report, Upstream, Warning, parse_args, run};

fn proxy(id: &str, status: &str) -> String {
    format!(
        r#"{{"id":"{id}","title":"BLOCKED-ON orchestrator bd-ib-ott6: prepare steps","status":"{status}",
            "labels":["origin:freeform","upstream-dep:livespec-orchestrator-beads-fabro"],
            "metadata":{{"upstream_work_item_id":"bd-ib-ott6","plan_ref":"livespec-orchestrator-beads-fabro/console-control-plane-primitives"}},
            "description":"PROXY","dependencies":[]}}"#
    )
}

/// One upstream-tenant row, in the shape the orchestrator tenant's
/// `bd list --status all --json -n 0` emits.
fn upstream_item(id: &str, status: &str, updated_at: &str) -> String {
    format!(
        r#"[{{"id":"{id}","title":"upstream {id}","status":"{status}","updated_at":"{updated_at}",
            "labels":[],"metadata":{{}},"description":"","dependencies":[]}}]"#
    )
}

/// The upstream ledger holding `bd-ib-ott6`, judged on [`NOW`]. `None` on a
/// broken fixture: the cross-tenant rules are then skipped, so every test
/// expecting a cross-tenant finding fails rather than passing blind. That the
/// fixture DOES parse is asserted on its own in
/// [`the_upstream_ledger_parses_and_resolves_each_proxy_by_its_metadata_id`].
fn upstream(status: &str, updated_at: &str) -> Option<Upstream> {
    Upstream::parse(&upstream_item("bd-ib-ott6", status, updated_at), NOW).ok()
}

/// The day rule D was earned, and the date every fixture is judged against.
const NOW: &str = "2026-09-06";
/// An upstream `updated_at` eight days before [`NOW`] — past the seven-day band.
const STALE: &str = "2026-08-29T09:15:00Z";
/// An upstream `updated_at` inside the seven-day band.
const FRESH: &str = "2026-09-05T09:15:00Z";

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
    findings_against(text, None)
}

/// As [`findings`], judged against an upstream ledger.
fn findings_against(text: &str, upstream: Option<&Upstream>) -> Vec<Finding> {
    match run(text, upstream) {
        Ok(report) => report.findings,
        Err(error) => vec![Finding::DeviationWithoutProxy {
            id: "<ledger-error>".to_owned(),
            evidence: error,
        }],
    }
}

/// Warnings, with a parse error surfaced as a sentinel warning for the same
/// reason [`findings`] surfaces one.
fn warnings(text: &str, upstream: Option<&Upstream>) -> Vec<Warning> {
    match run(text, upstream) {
        Ok(report) => report.warnings,
        Err(error) => vec![Warning {
            proxy: "<ledger-error>".to_owned(),
            upstream: error,
            updated_at: String::new(),
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
        run(&text, None),
        Ok(Report {
            scanned: 3,
            findings: Vec::new(),
            warnings: Vec::new(),
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
    assert!(matches!(run(r#"{"issues":[]}"#, None), Err(error) if error.contains("array")));
}

#[test]
fn the_upstream_ledger_parses_and_resolves_each_proxy_by_its_metadata_id() {
    assert!(
        Upstream::parse(&upstream_item("bd-ib-ott6", "open", FRESH), NOW).is_ok(),
        "the upstream fixture every cross-tenant test leans on must parse"
    );
    assert!(matches!(
        Upstream::parse("[]", "the sixth"),
        Err(error) if error.contains("YYYY-MM-DD")
    ));
    // `c-p1`'s metadata names `bd-ib-ott6`, which the upstream ledger holds;
    // `c-p9`'s names an id it does not.
    let known = ledger(&[proxy("c-p1", "blocked")]);
    assert!(findings_against(&known, upstream("open", FRESH).as_ref()).is_empty());
    let stranger = known.replace("bd-ib-ott6", "bd-ib-nope");
    assert!(matches!(
        findings_against(&stranger, upstream("open", FRESH).as_ref()).as_slice(),
        [Finding::ProxyUpstreamUnknown { upstream, .. }] if upstream == "bd-ib-nope"
    ));
}

#[test]
fn rule_d_a_blocked_proxy_whose_upstream_item_closed_is_refused() {
    let text = ledger(&[proxy("c-p1", "blocked")]);
    let found = findings_against(&text, upstream("closed", FRESH).as_ref());
    assert_eq!(
        found,
        vec![Finding::ProxyStaleUpstreamClosed {
            id: "c-p1".to_owned(),
            status: "blocked".to_owned(),
            upstream: "bd-ib-ott6".to_owned(),
        }]
    );
    let text = found[0].to_string();
    assert!(
        text.contains("c-p1") && text.contains("bd-ib-ott6"),
        "{text}"
    );
    assert_eq!(
        found[0].failure_mode(),
        "upstream-dep-proxy-stale-upstream-closed"
    );
}

#[test]
fn rule_e_a_closed_proxy_whose_upstream_is_still_open_is_refused() {
    let text = ledger(&[proxy("c-p1", "closed")]);
    let found = findings_against(&text, upstream("in_progress", FRESH).as_ref());
    assert_eq!(
        found,
        vec![Finding::ProxyClosedUpstreamOpen {
            id: "c-p1".to_owned(),
            upstream: "bd-ib-ott6".to_owned(),
            upstream_status: "in_progress".to_owned(),
        }]
    );
    assert_eq!(
        found[0].failure_mode(),
        "upstream-dep-proxy-closed-upstream-open"
    );
}

#[test]
fn rule_e_a_recorded_proxy_released_reason_makes_the_release_deliberate() {
    let released = proxy("c-p1", "closed").replace(
        r#""plan_ref":"#,
        r#""proxy_released_reason":"superseded by the console-side projection (maintainer, 2026-09-06)","plan_ref":"#,
    );
    let text = ledger(&[released]);
    assert!(findings_against(&text, upstream("in_progress", FRESH).as_ref()).is_empty());
}

#[test]
fn rule_u_an_upstream_id_the_upstream_ledger_does_not_hold_is_refused() {
    let text = ledger(&[proxy("c-p1", "blocked")]);
    let elsewhere = Upstream::parse(&upstream_item("bd-ib-other", "open", FRESH), NOW).ok();
    let found = findings_against(&text, elsewhere.as_ref());
    assert_eq!(
        found,
        vec![Finding::ProxyUpstreamUnknown {
            id: "c-p1".to_owned(),
            upstream: "bd-ib-ott6".to_owned(),
        }]
    );
    let text = found[0].to_string();
    assert!(
        text.contains("c-p1") && text.contains("bd-ib-ott6"),
        "{text}"
    );
    assert_eq!(
        found[0].failure_mode(),
        "upstream-dep-proxy-upstream-unknown"
    );
}

#[test]
fn warning_w_a_stalled_upstream_warns_and_the_run_still_passes() {
    let text = ledger(&[proxy("c-p1", "blocked")]);
    let stalled = upstream("open", STALE);
    assert!(findings_against(&text, stalled.as_ref()).is_empty());
    let warned = warnings(&text, stalled.as_ref());
    assert_eq!(
        warned,
        vec![Warning {
            proxy: "c-p1".to_owned(),
            upstream: "bd-ib-ott6".to_owned(),
            updated_at: STALE.to_owned(),
        }]
    );
    let rendered = warned[0].to_string();
    assert!(
        rendered.contains("c-p1") && rendered.contains("bd-ib-ott6"),
        "{rendered}"
    );
    assert_eq!(warned[0].warning_mode(), "upstream-dep-upstream-stale");
}

#[test]
fn warning_w_an_upstream_touched_inside_the_band_is_silent() {
    let text = ledger(&[proxy("c-p1", "blocked")]);
    assert!(warnings(&text, upstream("open", FRESH).as_ref()).is_empty());
}

#[test]
fn the_gate_binary_accepts_an_upstream_path_and_a_now_date() {
    let argv = ["ledger.json", "--upstream", "orch.json", "--now", NOW]
        .map(str::to_owned)
        .to_vec();
    let parsed = parse_args(&argv);
    assert_eq!(
        parsed,
        Ok(console_upstream_dep_check::Args {
            ledger: Some("ledger.json".to_owned()),
            upstream: Some("orch.json".to_owned()),
            now: Some(NOW.to_owned()),
        })
    );
    let half = ["--upstream".to_owned(), "orch.json".to_owned()];
    assert!(matches!(parse_args(&half), Err(error) if error.contains("go together")));
}

#[test]
fn every_finding_names_a_failure_mode() {
    let all = [
        Finding::ProxyStaleUpstreamClosed {
            id: "x".to_owned(),
            status: "blocked".to_owned(),
            upstream: "u".to_owned(),
        },
        Finding::ProxyClosedUpstreamOpen {
            id: "x".to_owned(),
            upstream: "u".to_owned(),
            upstream_status: "open".to_owned(),
        },
        Finding::ProxyUpstreamUnknown {
            id: "x".to_owned(),
            upstream: "u".to_owned(),
        },
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
            "upstream-dep-proxy-stale-upstream-closed",
            "upstream-dep-proxy-closed-upstream-open",
            "upstream-dep-proxy-upstream-unknown",
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
