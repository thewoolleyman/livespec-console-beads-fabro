//! `console-upstream-dep-check` — the GENERAL upstream-dependency gate over the
//! beads ledger (`livespec-console-beads-fabro-pzbdbo.1`). Not tied to any
//! epic: it applies to every work item in this tenant, epics or not.
//!
//! Input: the JSON array `bd list --status all --json -n 0` emits. The crate
//! is pure — no ledger access, no environment — so it is testable on
//! fixtures; the `gate-upstream-deps` recipe fetches the ledger under the
//! credential wrapper and fails closed when it cannot. Three rules, each a
//! refusal:
//!
//! - **A. Proxy shape.** A non-closed item carrying a label
//!   `upstream-dep:<tenant>` is a PROXY for an upstream orchestrator
//!   dependency. It MUST be `blocked`, be titled `BLOCKED-ON …`, and carry
//!   metadata `upstream_work_item_id` and `plan_ref`. A proxy closes only
//!   when its upstream item closes.
//! - **B. Deviation ⇒ proxy.** An ADMITTED item (any status except `closed`,
//!   `backlog`, `open` — a filing is not a shipped deviation) whose
//!   description records a deviation MUST depend, through a `blocks` edge, on
//!   at least one proxy. A deviation is a `deviations:` line whose value is
//!   not `none`, or a marker phrase together with an upstream reference. The
//!   guard paragraph every child of the retire-overseer epic carries is
//!   stripped first, so the rule's own wording never trips it.
//! - **C. Held ⇒ not dispatchable.** A non-closed item that depends on an
//!   OPEN proxy MUST NOT be `ready` or `active`. The dispatcher already
//!   refuses this; the gate re-asserts it so a hand-moved status is caught
//!   at push.
//!
//! Venue: the pre-push hook on the host. A sandbox checkout
//! (`livespec.sandboxExempt`) has no ledger by design; there the dispatcher's
//! pre-dispatch refusal is the gate. See the recipe for the fail-closed shape.

#![forbid(unsafe_code)]

use std::fmt;

use serde_json::Value;

/// Label prefix that marks an item as a proxy for an upstream dependency.
pub const UPSTREAM_DEP_LABEL_PREFIX: &str = "upstream-dep:";
/// Every proxy title announces the block with this prefix.
pub const PROXY_TITLE_PREFIX: &str = "BLOCKED-ON";
/// First line of the guard paragraph prepended to governed items; stripped
/// before deviation scanning so the rule's own wording never trips it.
pub const GUARD_HEAD: &str = "⛔ NEVER WORK AROUND AN UPSTREAM ORCHESTRATOR DEPENDENCY";
/// Phrases that record a workaround. Case-insensitive; a hit counts only
/// beside an upstream reference (see [`UPSTREAM_REFERENCES`]).
pub const DEVIATION_MARKERS: &[&str] = &[
    "hand-bridge",
    "hand bridge",
    "because pinned",
    "workaround",
    "work around",
    "worked around",
    "working around",
    "in place of a projection",
    "literal prepare",
    "literal value",
];
/// Substrings that tie a marker phrase to the orchestrator.
pub const UPSTREAM_REFERENCES: &[&str] = &["bd-ib-", "orchestrator", "upstream"];
/// Metadata every proxy must carry.
pub const REQUIRED_PROXY_METADATA: &[&str] = &["upstream_work_item_id", "plan_ref"];

const DEPENDS_EDGE: &str = "blocks";
const CLOSED: &str = "closed";
const BLOCKED: &str = "blocked";
const UNADMITTED: &[&str] = &[CLOSED, "backlog", "open"];
const DISPATCHABLE: &[&str] = &["ready", "active"];

/// One refusal. Each variant names a stable `failure_mode` for logs and tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// Rule A: a proxy whose status is not `blocked`.
    ProxyNotBlocked {
        /// The proxy's id.
        id: String,
        /// Its actual status.
        status: String,
    },
    /// Rule A: a proxy whose title does not start with `BLOCKED-ON`.
    ProxyTitleNotBlockedOn {
        /// The proxy's id.
        id: String,
    },
    /// Rule A: a proxy missing one of [`REQUIRED_PROXY_METADATA`].
    ProxyMissingMetadata {
        /// The proxy's id.
        id: String,
        /// The missing metadata key.
        field: &'static str,
    },
    /// Rule B: an admitted item records a deviation and depends on no proxy.
    DeviationWithoutProxy {
        /// The item's id.
        id: String,
        /// The line or phrase that recorded the deviation.
        evidence: String,
    },
    /// Rule C: an item held on an open proxy sits at a dispatchable status.
    HeldItemDispatchable {
        /// The item's id.
        id: String,
        /// Its status (`ready` or `active`).
        status: String,
        /// The open proxy it depends on.
        proxy: String,
    },
}

impl Finding {
    /// Stable machine-readable name of the refusal.
    #[must_use]
    pub const fn failure_mode(&self) -> &'static str {
        match self {
            Self::ProxyNotBlocked { .. } => "upstream-dep-proxy-not-blocked",
            Self::ProxyTitleNotBlockedOn { .. } => "upstream-dep-proxy-title",
            Self::ProxyMissingMetadata { .. } => "upstream-dep-proxy-metadata-missing",
            Self::DeviationWithoutProxy { .. } => "upstream-dep-deviation-without-proxy",
            Self::HeldItemDispatchable { .. } => "upstream-dep-held-item-dispatchable",
        }
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProxyNotBlocked { id, status } => write!(
                f,
                "{id} carries an `{UPSTREAM_DEP_LABEL_PREFIX}` label but is `{status}`, not `{BLOCKED}`; a proxy closes only when its upstream item closes"
            ),
            Self::ProxyTitleNotBlockedOn { id } => write!(
                f,
                "{id} is a proxy whose title does not start with `{PROXY_TITLE_PREFIX}`"
            ),
            Self::ProxyMissingMetadata { id, field } => {
                let required = REQUIRED_PROXY_METADATA.join(" + ");
                write!(
                    f,
                    "{id} is a proxy with no `{field}` metadata (needs {required})"
                )
            }
            Self::DeviationWithoutProxy { id, evidence } => write!(
                f,
                "{id} records a deviation ({evidence}) but depends on no `{UPSTREAM_DEP_LABEL_PREFIX}` proxy; a workaround cannot exist without the upstream item it stands in for"
            ),
            Self::HeldItemDispatchable { id, status, proxy } => write!(
                f,
                "{id} is `{status}` while it depends on the open proxy {proxy}; a held item must not be dispatchable"
            ),
        }
    }
}

/// The outcome of one scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Number of ledger items examined.
    pub scanned: usize,
    /// Every refusal, in ledger order then rule order.
    pub findings: Vec<Finding>,
}

/// Parse the `bd list --json` text into its items.
///
/// # Errors
/// Returns a message when the text is not JSON or not a JSON array — an
/// unreadable ledger is a refusal, never a pass.
pub fn parse_ledger(text: &str) -> Result<Vec<Value>, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|err| format!("ledger is not JSON: {err}"))?;
    match value {
        Value::Array(items) => Ok(items),
        other => Err(format!(
            "ledger is not a JSON array (got {}); expected the output of `bd list --status all --json -n 0`",
            kind(&other)
        )),
    }
}

/// Parse then check.
///
/// # Errors
/// See [`parse_ledger`].
pub fn run(text: &str) -> Result<Report, String> {
    parse_ledger(text).map(|items| check(&items))
}

/// Apply rules A, B and C to the items.
#[must_use]
pub fn check(items: &[Value]) -> Report {
    let open_proxies = items
        .iter()
        .filter(|item| is_proxy(item) && field(item, "status") != CLOSED)
        .map(|item| field(item, "id").to_owned())
        .collect::<Vec<_>>();
    let all_proxies = items
        .iter()
        .filter(|item| is_proxy(item))
        .map(|item| field(item, "id").to_owned())
        .collect::<Vec<_>>();
    let mut findings = Vec::new();
    for item in items {
        let id = field(item, "id");
        let status = field(item, "status");
        if status == CLOSED {
            continue;
        }
        if is_proxy(item) {
            findings.extend(check_proxy_shape(item, id, status));
            continue;
        }
        let depends_on = depends_on_ids(item, id);
        if !UNADMITTED.contains(&status)
            && !depends_on.iter().any(|dep| all_proxies.contains(dep))
            && let Some(evidence) = deviation_evidence(field(item, "description"))
        {
            findings.push(Finding::DeviationWithoutProxy {
                id: id.to_owned(),
                evidence,
            });
        }
        if DISPATCHABLE.contains(&status) {
            for proxy in depends_on.iter().filter(|dep| open_proxies.contains(dep)) {
                findings.push(Finding::HeldItemDispatchable {
                    id: id.to_owned(),
                    status: status.to_owned(),
                    proxy: proxy.clone(),
                });
            }
        }
    }
    Report {
        scanned: items.len(),
        findings,
    }
}

fn check_proxy_shape(item: &Value, id: &str, status: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    if status != BLOCKED {
        findings.push(Finding::ProxyNotBlocked {
            id: id.to_owned(),
            status: status.to_owned(),
        });
    }
    if !field(item, "title").starts_with(PROXY_TITLE_PREFIX) {
        findings.push(Finding::ProxyTitleNotBlockedOn { id: id.to_owned() });
    }
    for &key in REQUIRED_PROXY_METADATA {
        if metadata_field(item, key).is_none_or(|value| value.trim().is_empty()) {
            findings.push(Finding::ProxyMissingMetadata {
                id: id.to_owned(),
                field: key,
            });
        }
    }
    findings
}

/// The evidence line or phrase when a description records a deviation.
#[must_use]
pub fn deviation_evidence(description: &str) -> Option<String> {
    let body = strip_guard(description);
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(rest) = strip_prefix_ignore_case(trimmed, "deviations:")
            && !rest.trim().eq_ignore_ascii_case("none")
        {
            return Some(trimmed.to_owned());
        }
    }
    let lower = body.to_lowercase();
    let referenced = UPSTREAM_REFERENCES
        .iter()
        .any(|reference| lower.contains(reference));
    if !referenced {
        return None;
    }
    DEVIATION_MARKERS
        .iter()
        .find(|marker| lower.contains(*marker))
        .map(|marker| format!("phrase `{marker}` beside an upstream reference"))
}

/// The description with every guard paragraph removed.
#[must_use]
pub fn strip_guard(description: &str) -> String {
    description
        .split("\n\n")
        .filter(|paragraph| !paragraph.trim_start().starts_with(GUARD_HEAD))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| text.get(prefix.len()..))
        .flatten()
}

fn is_proxy(item: &Value) -> bool {
    item.get("labels")
        .and_then(Value::as_array)
        .is_some_and(|labels| {
            labels
                .iter()
                .filter_map(Value::as_str)
                .any(|label| label.starts_with(UPSTREAM_DEP_LABEL_PREFIX))
        })
}

fn field<'a>(item: &'a Value, key: &str) -> &'a str {
    item.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// A metadata value, whether the ledger serialized `metadata` as an object or
/// as a JSON string.
fn metadata_field(item: &Value, key: &str) -> Option<String> {
    let metadata = item.get("metadata")?;
    let object = match metadata {
        Value::String(text) => serde_json::from_str::<Value>(text).ok()?,
        other => other.clone(),
    };
    object.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// Targets of the item's own `blocks` edges.
fn depends_on_ids(item: &Value, id: &str) -> Vec<String> {
    item.get("dependencies")
        .and_then(Value::as_array)
        .map_or_else(Vec::new, |edges| {
            edges
                .iter()
                .filter(|edge| field(edge, "type") == DEPENDS_EDGE)
                .filter(|edge| {
                    let owner = field(edge, "issue_id");
                    owner.is_empty() || owner == id
                })
                .map(|edge| field(edge, "depends_on_id").to_owned())
                .filter(|target| !target.is_empty())
                .collect()
        })
}

const fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn proxy(id: &str, status: &str) -> Value {
        json!({
            "id": id, "status": status,
            "title": "BLOCKED-ON orchestrator bd-ib-ott6: prepare steps",
            "labels": ["upstream-dep:livespec-orchestrator-beads-fabro"],
            "metadata": {"upstream_work_item_id": "bd-ib-ott6", "plan_ref": "t/s"},
            "description": "", "dependencies": []
        })
    }

    fn item(id: &str, status: &str, description: &str, deps: &[&str]) -> Value {
        let edges = deps
            .iter()
            .map(|dep| json!({"issue_id": id, "depends_on_id": dep, "type": "blocks"}))
            .collect::<Vec<_>>();
        json!({"id": id, "status": status, "title": "t", "labels": [], "metadata": {},
               "description": description, "dependencies": edges})
    }

    fn deviation_ids(findings: &[Finding]) -> Vec<&str> {
        findings
            .iter()
            .filter_map(|finding| match finding {
                Finding::DeviationWithoutProxy { id, .. } => Some(id.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn parse_rejects_non_json_and_non_arrays_and_names_the_shape() {
        assert!(matches!(parse_ledger("nope"), Err(error) if error.contains("not JSON")));
        for (text, shape) in [
            ("null", "null"),
            ("true", "a boolean"),
            ("1", "a number"),
            ("\"s\"", "a string"),
            ("{}", "an object"),
        ] {
            assert!(matches!(parse_ledger(text), Err(error) if error.contains(shape)));
        }
        assert_eq!(kind(&json!([])), "an array");
        assert_eq!(parse_ledger("[]"), Ok(Vec::new()));
    }

    #[test]
    fn run_reports_scan_size_and_no_findings_on_an_empty_ledger() {
        assert_eq!(
            run("[]"),
            Ok(Report {
                scanned: 0,
                findings: Vec::new()
            })
        );
    }

    #[test]
    fn closed_items_are_skipped_entirely_even_when_malformed() {
        let mut bad = proxy("p", "closed");
        bad["title"] = json!("no prefix");
        bad["metadata"] = json!({});
        let held = item("h", "closed", "deviations: x (bd-ib-y)", &[]);
        assert!(check(&[bad, held]).findings.is_empty());
    }

    #[test]
    fn proxy_shape_reports_every_defect_at_once() {
        let mut bad = proxy("p", "ready");
        bad["title"] = json!("wrong");
        bad["metadata"] = json!({"upstream_work_item_id": "  "});
        let findings = check(&[bad]).findings;
        assert_eq!(
            findings,
            vec![
                Finding::ProxyNotBlocked {
                    id: "p".to_owned(),
                    status: "ready".to_owned()
                },
                Finding::ProxyTitleNotBlockedOn { id: "p".to_owned() },
                Finding::ProxyMissingMetadata {
                    id: "p".to_owned(),
                    field: "upstream_work_item_id"
                },
                Finding::ProxyMissingMetadata {
                    id: "p".to_owned(),
                    field: "plan_ref"
                },
            ]
        );
    }

    #[test]
    fn proxy_metadata_may_be_serialized_as_a_json_string() {
        let mut ok = proxy("p", "blocked");
        ok["metadata"] = json!("{\"upstream_work_item_id\":\"bd-ib-x\",\"plan_ref\":\"t/s\"}");
        assert!(check(&[ok.clone()]).findings.is_empty());
        ok["metadata"] = json!("not json");
        assert_eq!(check(&[ok.clone()]).findings.len(), 2);
        ok["metadata"] = json!(null);
        assert_eq!(check(&[ok]).findings.len(), 2);
        let absent = json!({
            "id": "q", "status": "blocked",
            "title": "BLOCKED-ON orchestrator bd-ib-x: y",
            "labels": ["upstream-dep:t"], "description": "", "dependencies": []
        });
        assert_eq!(check(&[absent]).findings.len(), 2);
    }

    #[test]
    fn proxies_are_never_scanned_for_deviations() {
        let mut p = proxy("p", "blocked");
        p["description"] = json!("the workaround this proxy retires (orchestrator)");
        assert!(check(&[p]).findings.is_empty());
    }

    #[test]
    fn deviation_line_is_case_insensitive_and_none_is_clean() {
        assert_eq!(
            deviation_evidence("DEVIATIONS: literal prepare steps"),
            Some("DEVIATIONS: literal prepare steps".to_owned())
        );
        assert_eq!(deviation_evidence("Deviations:   NONE  "), None);
        assert_eq!(
            deviation_evidence("deviations:"),
            Some("deviations:".to_owned())
        );
        assert_eq!(deviation_evidence("unrelated"), None);
    }

    #[test]
    fn a_deviations_prefix_must_be_at_line_start_not_inside_a_word() {
        assert_eq!(deviation_evidence("no deviations: none here"), None);
        assert_eq!(deviation_evidence("dev"), None);
    }

    #[test]
    fn phrase_hits_need_an_upstream_reference_and_name_the_marker() {
        assert_eq!(deviation_evidence("a workaround, no reference"), None);
        assert_eq!(
            deviation_evidence("a Workaround for the Orchestrator"),
            Some("phrase `workaround` beside an upstream reference".to_owned())
        );
        for marker in DEVIATION_MARKERS {
            assert!(deviation_evidence(&format!("{marker} bd-ib-1")).is_some());
        }
        for reference in UPSTREAM_REFERENCES {
            assert!(deviation_evidence(&format!("we hand-bridge {reference}")).is_some());
        }
        assert_eq!(deviation_evidence("bd-ib-1 with nothing recorded"), None);
    }

    #[test]
    fn the_guard_paragraph_is_stripped_wherever_it_sits() {
        let guard = format!("{GUARD_HEAD} — hand-bridge workaround orchestrator.");
        let text = format!("first\n\n  {guard}\n\nlast");
        assert_eq!(strip_guard(&text), "first\n\nlast");
        assert_eq!(strip_guard(&guard), "");
        assert_eq!(deviation_evidence(&format!("{guard}\n\nclean")), None);
        assert_eq!(
            deviation_evidence(&format!("{guard}\n\ndeviations: x")),
            Some("deviations: x".to_owned())
        );
    }

    #[test]
    fn strip_prefix_ignore_case_handles_short_and_matching_text() {
        assert_eq!(strip_prefix_ignore_case("ab", "abc"), None);
        assert_eq!(strip_prefix_ignore_case("ABC:rest", "abc:"), Some("rest"));
        assert_eq!(strip_prefix_ignore_case("xbc:", "abc:"), None);
        assert_eq!(strip_prefix_ignore_case("abc:", "abc:"), Some(""));
    }

    #[test]
    fn deviation_rule_applies_only_to_admitted_items() {
        for status in ["backlog", "open"] {
            let filing = item("f", status, "deviations: x (bd-ib-y)", &[]);
            assert!(check(&[filing]).findings.is_empty());
        }
        for status in [
            "ready",
            "active",
            "blocked",
            "pending-approval",
            "acceptance",
        ] {
            let admitted = item("a", status, "deviations: x (bd-ib-y)", &[]);
            let findings = check(&[admitted]).findings;
            assert_eq!(deviation_ids(&findings), ["a"]);
            let other = Finding::ProxyTitleNotBlockedOn {
                id: status.to_owned(),
            };
            assert!(deviation_ids(&[other]).is_empty());
            assert!(findings.iter().any(|finding| matches!(
                finding,
                Finding::DeviationWithoutProxy { evidence, .. } if evidence == "deviations: x (bd-ib-y)"
            )));
        }
    }

    #[test]
    fn a_deviation_linked_to_any_proxy_even_a_closed_one_passes_rule_b() {
        let closed = proxy("p", "closed");
        let dev = item("a", "blocked", "deviations: x", &["p"]);
        assert!(check(&[closed, dev]).findings.is_empty());
    }

    #[test]
    fn held_items_are_refused_only_at_dispatchable_statuses_and_per_open_proxy() {
        let p1 = proxy("p1", "blocked");
        let p2 = proxy("p2", "blocked");
        let closed = proxy("p3", "closed");
        for status in ["ready", "active"] {
            let held = item("h", status, "", &["p1", "p2", "p3"]);
            let findings = check(&[p1.clone(), p2.clone(), closed.clone(), held]).findings;
            assert_eq!(
                findings,
                vec![
                    Finding::HeldItemDispatchable {
                        id: "h".to_owned(),
                        status: status.to_owned(),
                        proxy: "p1".to_owned()
                    },
                    Finding::HeldItemDispatchable {
                        id: "h".to_owned(),
                        status: status.to_owned(),
                        proxy: "p2".to_owned()
                    },
                ]
            );
        }
        for status in ["blocked", "pending-approval", "backlog"] {
            let held = item("h", status, "", &["p1"]);
            assert!(check(&[p1.clone(), held]).findings.is_empty());
        }
    }

    #[test]
    fn dependency_edges_are_filtered_by_type_and_owner() {
        let edges = json!({"id": "h", "status": "ready", "title": "t", "labels": [], "metadata": {},
        "description": "", "dependencies": [
            {"issue_id": "h", "depends_on_id": "p", "type": "blocks"},
            {"issue_id": "h", "depends_on_id": "parent", "type": "parent-child"},
            {"issue_id": "other", "depends_on_id": "p", "type": "blocks"},
            {"depends_on_id": "p", "type": "blocks"},
            {"issue_id": "h", "depends_on_id": "", "type": "blocks"},
            {"issue_id": "h", "type": "blocks"}
        ]});
        assert_eq!(
            depends_on_ids(&edges, "h"),
            vec!["p".to_owned(), "p".to_owned()]
        );
        let none = json!({"id": "h"});
        assert!(depends_on_ids(&none, "h").is_empty());
        let not_array = json!({"id": "h", "dependencies": "x"});
        assert!(depends_on_ids(&not_array, "h").is_empty());
    }

    #[test]
    fn label_detection_tolerates_missing_or_non_string_labels() {
        assert!(!is_proxy(&json!({"id": "x"})));
        assert!(!is_proxy(&json!({"labels": "upstream-dep:t"})));
        assert!(!is_proxy(&json!({"labels": [1, "other"]})));
        assert!(is_proxy(&json!({"labels": [1, "upstream-dep:t"]})));
    }

    #[test]
    fn missing_string_fields_read_as_empty() {
        assert_eq!(field(&json!({"id": 5}), "id"), "");
        assert_eq!(field(&json!({}), "id"), "");
    }

    #[test]
    fn every_variant_names_its_own_failure_mode() {
        let modes = [
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
        ]
        .iter()
        .map(Finding::failure_mode)
        .collect::<Vec<_>>();
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
    }

    #[test]
    fn display_carries_the_id_and_the_failure_mode_is_stable() {
        let finding = Finding::ProxyMissingMetadata {
            id: "abc".to_owned(),
            field: "plan_ref",
        };
        let text = finding.to_string();
        assert!(text.contains("abc") && text.contains("plan_ref"));
        assert!(text.contains("upstream_work_item_id"));
        assert_eq!(
            finding.failure_mode(),
            "upstream-dep-proxy-metadata-missing"
        );
        let held = Finding::HeldItemDispatchable {
            id: "h".to_owned(),
            status: "active".to_owned(),
            proxy: "p".to_owned(),
        };
        let held_text = held.to_string();
        assert!(held_text.contains("`active`") && held_text.contains(" p;"));
        let dev = Finding::DeviationWithoutProxy {
            id: "d".to_owned(),
            evidence: "deviations: q".to_owned(),
        };
        assert!(dev.to_string().contains("(deviations: q)"));
        let not_blocked = Finding::ProxyNotBlocked {
            id: "n".to_owned(),
            status: "ready".to_owned(),
        };
        assert!(not_blocked.to_string().contains("`ready`, not `blocked`"));
        let title = Finding::ProxyTitleNotBlockedOn { id: "t".to_owned() };
        assert!(title.to_string().contains("BLOCKED-ON"));
    }
}
