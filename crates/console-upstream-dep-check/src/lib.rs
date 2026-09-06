//! `console-upstream-dep-check` — the GENERAL upstream-dependency gate over the
//! beads ledger (`livespec-console-beads-fabro-pzbdbo.1`). Not tied to any
//! epic: it applies to every work item in this tenant, epics or not.
//!
//! Input: the JSON array `bd list --status all --json -n 0` emits, for THIS
//! tenant, plus — through `--upstream <path> --now <YYYY-MM-DD>` — the same
//! array for the orchestrator tenant. The crate is pure — no ledger access,
//! no environment, no clock — so it is testable on fixtures; the
//! `gate-upstream-deps` recipe fetches BOTH ledgers under ONE credential-wrapper
//! invocation and fails closed when either read fails or comes back empty.
//! Six rules, five of them refusals:
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
//! Rules D, E, U and the W warning are the CROSS-TENANT lifecycle, evaluated
//! only when an upstream ledger is supplied. They exist because on 2026-09-06
//! four proxies sat `blocked` for one to two days after their orchestrator
//! items had closed and nothing noticed: the console was waiting without
//! knowing the wait had ended.
//!
//! - **D. Upstream closed ⇒ proxy must close.** A non-closed proxy whose
//!   `upstream_work_item_id` is `closed` upstream is refused; the hold it
//!   carries has expired and every item held behind it is waiting on nothing.
//! - **E. Proxy closed while upstream open ⇒ refuse.** A closed proxy whose
//!   upstream item is still open is refused, UNLESS the proxy records a
//!   deliberate release in metadata [`PROXY_RELEASED_REASON`]. Closing a proxy
//!   by hand is exactly the "work around the upstream dependency" move the
//!   guard forbids, so releasing one is allowed only on the record.
//! - **U. Upstream unknown ⇒ refuse.** A proxy whose upstream id is absent
//!   from the upstream ledger names a dependency nobody is tracking. Silence
//!   there is indistinguishable from "not started", so it fails closed.
//! - **W. Stale upstream (WARNS, NEVER refuses).** A non-closed proxy whose
//!   upstream item has not been touched in more than [`STALE_UPSTREAM_DAYS`]
//!   days relative to `--now` is printed on every push. A stalled dependency
//!   must be SEEN rather than silently waited on — but a stall is the
//!   orchestrator's to break, not something this repo's push may be blocked
//!   on, so it never becomes a refusal.
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
/// Metadata naming the upstream item a proxy stands in for.
pub const UPSTREAM_ID_KEY: &str = "upstream_work_item_id";
/// Metadata naming the upstream plan thread the proxy belongs to.
pub const PLAN_REF_KEY: &str = "plan_ref";
/// Metadata every proxy must carry.
pub const REQUIRED_PROXY_METADATA: &[&str] = &[UPSTREAM_ID_KEY, PLAN_REF_KEY];
/// Metadata recording a DELIBERATE release of a proxy ahead of its upstream
/// item — rule E's only exemption, and only when non-empty.
pub const PROXY_RELEASED_REASON: &str = "proxy_released_reason";
/// How long an upstream item may sit untouched before the stale warning fires.
pub const STALE_UPSTREAM_DAYS: i64 = 7;
/// Stable machine-readable name of the never-refusing stale-upstream warning.
pub const UPSTREAM_STALE_WARNING: &str = "upstream-dep-upstream-stale";
/// Stands in for an upstream `updated_at` that is missing or unparseable.
///
/// An upstream item whose freshness cannot be read is warned about rather than
/// passed over: the warning never refuses, so warning loudly costs nothing and
/// staying silent would reintroduce the exact blindness rule W exists to end.
pub const UNKNOWN_UPDATED_AT: &str = "an unreadable date";

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
    /// Rule D: an open proxy whose upstream item has already closed.
    ProxyStaleUpstreamClosed {
        /// The proxy's id.
        id: String,
        /// Its status.
        status: String,
        /// The closed upstream item's id.
        upstream: String,
    },
    /// Rule E: a closed proxy whose upstream item is still open, with no
    /// recorded [`PROXY_RELEASED_REASON`].
    ProxyClosedUpstreamOpen {
        /// The proxy's id.
        id: String,
        /// The still-open upstream item's id.
        upstream: String,
        /// The upstream item's status.
        upstream_status: String,
    },
    /// Rule U: a proxy naming an upstream id the upstream ledger does not hold.
    ProxyUpstreamUnknown {
        /// The proxy's id.
        id: String,
        /// The upstream id nothing upstream answers to.
        upstream: String,
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
            Self::ProxyStaleUpstreamClosed { .. } => "upstream-dep-proxy-stale-upstream-closed",
            Self::ProxyClosedUpstreamOpen { .. } => "upstream-dep-proxy-closed-upstream-open",
            Self::ProxyUpstreamUnknown { .. } => "upstream-dep-proxy-upstream-unknown",
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
            Self::ProxyStaleUpstreamClosed {
                id,
                status,
                upstream,
            } => write!(
                f,
                "{id} is still `{status}` while its upstream item {upstream} is `{CLOSED}`; close the proxy so everything held behind it stops waiting on nothing"
            ),
            Self::ProxyClosedUpstreamOpen {
                id,
                upstream,
                upstream_status,
            } => write!(
                f,
                "{id} is `{CLOSED}` while its upstream item {upstream} is `{upstream_status}`; a proxy closes only when its upstream item closes, so record a `{PROXY_RELEASED_REASON}` if the release was deliberate"
            ),
            Self::ProxyUpstreamUnknown { id, upstream } => write!(
                f,
                "{id} names the upstream item {upstream}, which the upstream ledger does not hold; a dependency nobody tracks cannot be waited on"
            ),
        }
    }
}

/// One never-refusing observation: an upstream item this repo waits on has
/// gone quiet. Printed on every push so a stalled dependency is seen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// The waiting proxy's id.
    pub proxy: String,
    /// The upstream item it waits on.
    pub upstream: String,
    /// The upstream item's `updated_at`, or [`UNKNOWN_UPDATED_AT`].
    pub updated_at: String,
}

impl Warning {
    /// Stable machine-readable name of the warning.
    #[must_use]
    pub const fn warning_mode(&self) -> &'static str {
        UPSTREAM_STALE_WARNING
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            proxy,
            upstream,
            updated_at,
        } = self;
        write!(
            f,
            "{proxy} waits on upstream {upstream}, last updated {updated_at} — more than {STALE_UPSTREAM_DAYS} days ago; the dependency may be stalled"
        )
    }
}

/// The outcome of one scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Number of ledger items examined.
    pub scanned: usize,
    /// Every refusal, in ledger order then rule order.
    pub findings: Vec<Finding>,
    /// Every never-refusing observation, in ledger order.
    pub warnings: Vec<Warning>,
}

/// The orchestrator tenant's ledger, plus the date staleness is judged
/// against. Both arrive from the caller — the crate never reads a clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upstream {
    items: Vec<Value>,
    today: i64,
}

impl Upstream {
    /// Build from the upstream ledger text and a `YYYY-MM-DD` date.
    ///
    /// # Errors
    /// Returns a message when the text is not a JSON array (see
    /// [`parse_ledger`]) or when `now` is not a `YYYY-MM-DD` date.
    pub fn parse(text: &str, now: &str) -> Result<Self, String> {
        let items = parse_ledger(text)?;
        let today = parse_date(now)
            .ok_or_else(|| format!("--now must be a YYYY-MM-DD date (got `{now}`)"))?;
        Ok(Self { items, today })
    }

    fn find(&self, id: &str) -> Option<&Value> {
        self.items.iter().find(|item| field(item, "id") == id)
    }
}

/// The command line the gate binary understands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Args {
    /// The console ledger's path; `None` reads stdin.
    pub ledger: Option<String>,
    /// `--upstream <path>`: the orchestrator tenant's ledger.
    pub upstream: Option<String>,
    /// `--now <YYYY-MM-DD>`: the date rule W is judged against.
    pub now: Option<String>,
}

/// Parse the argument list (already without `argv[0]`).
///
/// # Errors
/// Returns a message for an unknown flag, a flag with no value, a second
/// positional argument, or `--upstream` and `--now` given apart: the
/// cross-tenant rules need the ledger AND the date, and half of that pair
/// would silently skip them.
pub fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut rest = argv.iter();
    while let Some(argument) = rest.next() {
        let slot = match argument.as_str() {
            "--upstream" => &mut parsed.upstream,
            "--now" => &mut parsed.now,
            flag if flag.starts_with('-') => return Err(format!("unknown flag `{flag}`")),
            _ if parsed.ledger.is_some() => {
                return Err(format!("unexpected second ledger path `{argument}`"));
            }
            _ => {
                parsed.ledger = Some(argument.clone());
                continue;
            }
        };
        *slot = Some(
            rest.next()
                .ok_or_else(|| format!("{argument} needs a value"))?
                .clone(),
        );
    }
    if parsed.upstream.is_some() != parsed.now.is_some() {
        return Err(
            "--upstream and --now go together; the cross-tenant rules need both".to_owned(),
        );
    }
    Ok(parsed)
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
pub fn run(text: &str, upstream: Option<&Upstream>) -> Result<Report, String> {
    parse_ledger(text).map(|items| check(&items, upstream))
}

/// Apply rules A, B and C to the items, and — when an upstream ledger is
/// supplied — the cross-tenant lifecycle rules D, E, U and the W warning.
#[must_use]
pub fn check(items: &[Value], upstream: Option<&Upstream>) -> Report {
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
    let mut warnings = Vec::new();
    for item in items {
        let id = field(item, "id");
        let status = field(item, "status");
        if is_proxy(item) {
            // Rules D, E and U judge a CLOSED proxy too — rule E is precisely
            // the case where the proxy closed and the upstream item did not —
            // so the closed-item skip below cannot cover proxies.
            if status != CLOSED {
                findings.extend(check_proxy_shape(item, id, status));
            }
            if let Some(upstream) = upstream {
                let (finding, warning) = check_cross_tenant(item, id, status, upstream);
                findings.extend(finding);
                warnings.extend(warning);
            }
            continue;
        }
        if status == CLOSED {
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
        warnings,
    }
}

/// Rules D, E, U and the W warning for one proxy. A proxy with no upstream id
/// is left to rule A, which already refuses it for the missing metadata.
fn check_cross_tenant(
    item: &Value,
    id: &str,
    status: &str,
    upstream: &Upstream,
) -> (Option<Finding>, Option<Warning>) {
    let Some(upstream_id) = metadata_field(item, UPSTREAM_ID_KEY)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    else {
        return (None, None);
    };
    let Some(target) = upstream.find(&upstream_id) else {
        return (
            Some(Finding::ProxyUpstreamUnknown {
                id: id.to_owned(),
                upstream: upstream_id,
            }),
            None,
        );
    };
    let upstream_status = field(target, "status");
    if status == CLOSED {
        let released = metadata_field(item, PROXY_RELEASED_REASON)
            .is_some_and(|reason| !reason.trim().is_empty());
        let finding =
            (upstream_status != CLOSED && !released).then(|| Finding::ProxyClosedUpstreamOpen {
                id: id.to_owned(),
                upstream: upstream_id,
                upstream_status: upstream_status.to_owned(),
            });
        return (finding, None);
    }
    if upstream_status == CLOSED {
        return (
            Some(Finding::ProxyStaleUpstreamClosed {
                id: id.to_owned(),
                status: status.to_owned(),
                upstream: upstream_id,
            }),
            None,
        );
    }
    let updated_at = field(target, "updated_at");
    let warning = stale(updated_at, upstream.today).then(|| Warning {
        proxy: id.to_owned(),
        upstream: upstream_id,
        updated_at: if updated_at.is_empty() {
            UNKNOWN_UPDATED_AT.to_owned()
        } else {
            updated_at.to_owned()
        },
    });
    (None, warning)
}

/// Whether an upstream `updated_at` is more than [`STALE_UPSTREAM_DAYS`] days
/// before `today`. A date that will not parse counts as stale — see
/// [`UNKNOWN_UPDATED_AT`].
fn stale(updated_at: &str, today: i64) -> bool {
    parse_date(updated_at).is_none_or(|day| today - day > STALE_UPSTREAM_DAYS)
}

/// `YYYY-MM-DD`, or any text whose first ten characters are one (an RFC 3339
/// timestamp), as days since 1970-01-01.
fn parse_date(text: &str) -> Option<i64> {
    // Matched as a slice rather than pulled off the iterator one `?` at a
    // time: `split` always yields at least one element, so the FIRST such `?`
    // would be an error arm no input can reach.
    let parts = text.get(..10)?.split('-').collect::<Vec<_>>();
    let [year, month, day] = parts.as_slice() else {
        return None;
    };
    let year = year.parse::<i64>().ok()?;
    let month = month.parse::<i64>().ok()?;
    let day = day.parse::<i64>().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

/// Howard Hinnant's `days_from_civil`: a proleptic Gregorian date as days
/// since 1970-01-01, using an era of 400 years (146097 days).
const fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let shifted_year = year - if month <= 2 { 1 } else { 0 };
    let era = shifted_year.div_euclid(400);
    let year_of_era = shifted_year - era * 400;
    let shifted_month = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
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
            run("[]", None),
            Ok(Report {
                scanned: 0,
                findings: Vec::new(),
                warnings: Vec::new(),
            })
        );
    }

    #[test]
    fn closed_items_are_skipped_entirely_even_when_malformed() {
        let mut bad = proxy("p", "closed");
        bad["title"] = json!("no prefix");
        bad["metadata"] = json!({});
        let held = item("h", "closed", "deviations: x (bd-ib-y)", &[]);
        assert!(check(&[bad, held], None).findings.is_empty());
    }

    #[test]
    fn proxy_shape_reports_every_defect_at_once() {
        let mut bad = proxy("p", "ready");
        bad["title"] = json!("wrong");
        bad["metadata"] = json!({"upstream_work_item_id": "  "});
        let findings = check(&[bad], None).findings;
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
        assert!(check(&[ok.clone()], None).findings.is_empty());
        ok["metadata"] = json!("not json");
        assert_eq!(check(&[ok.clone()], None).findings.len(), 2);
        ok["metadata"] = json!(null);
        assert_eq!(check(&[ok], None).findings.len(), 2);
        let absent = json!({
            "id": "q", "status": "blocked",
            "title": "BLOCKED-ON orchestrator bd-ib-x: y",
            "labels": ["upstream-dep:t"], "description": "", "dependencies": []
        });
        assert_eq!(check(&[absent], None).findings.len(), 2);
    }

    #[test]
    fn proxies_are_never_scanned_for_deviations() {
        let mut p = proxy("p", "blocked");
        p["description"] = json!("the workaround this proxy retires (orchestrator)");
        assert!(check(&[p], None).findings.is_empty());
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
            assert!(check(&[filing], None).findings.is_empty());
        }
        for status in [
            "ready",
            "active",
            "blocked",
            "pending-approval",
            "acceptance",
        ] {
            let admitted = item("a", status, "deviations: x (bd-ib-y)", &[]);
            let findings = check(&[admitted], None).findings;
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
        assert!(check(&[closed, dev], None).findings.is_empty());
    }

    #[test]
    fn held_items_are_refused_only_at_dispatchable_statuses_and_per_open_proxy() {
        let p1 = proxy("p1", "blocked");
        let p2 = proxy("p2", "blocked");
        let closed = proxy("p3", "closed");
        for status in ["ready", "active"] {
            let held = item("h", status, "", &["p1", "p2", "p3"]);
            let findings = check(&[p1.clone(), p2.clone(), closed.clone(), held], None).findings;
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
            assert!(check(&[p1.clone(), held], None).findings.is_empty());
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

    /// An upstream ledger holding one item, judged on 2026-09-06.
    fn upstream(status: &str, updated_at: &str) -> Upstream {
        let mut target = json!({"id": "bd-ib-ott6", "status": status});
        if !updated_at.is_empty() {
            target["updated_at"] = json!(updated_at);
        }
        Upstream {
            items: vec![json!({"id": "bd-ib-other", "status": "open"}), target],
            today: parse_date("2026-09-06").unwrap_or_default(),
        }
    }

    /// A proxy carrying an extra metadata key.
    fn proxy_with(id: &str, status: &str, key: &str, value: &str) -> Value {
        let mut item = proxy(id, status);
        item["metadata"][key] = json!(value);
        item
    }

    #[test]
    fn parse_date_reads_a_bare_date_or_an_rfc3339_prefix_and_rejects_the_rest() {
        assert_eq!(parse_date("1970-01-01"), Some(0));
        assert_eq!(parse_date("1970-01-02"), Some(1));
        assert_eq!(parse_date("1970-03-01"), Some(59));
        assert_eq!(parse_date("2026-09-06T04:37:17Z"), Some(20_702));
        // Both civil-date branches, an era before 1970, and a leap day.
        assert_eq!(parse_date("0001-01-01"), Some(-719_162));
        assert_eq!(parse_date("2024-02-29"), Some(19_782));
        for rejected in [
            "2026-09",           // too short to hold a date
            "20260906xx",        // no separators at all
            "1234567890",        // a year, then nothing
            "123456-890",        // a year and a month, then nothing
            "2026-9x-06",        // a month that will not parse
            "2026-09-xx",        // a day that will not parse
            "1-2-3-4-56789",     // more than three components
            "2026-13-01",        // month out of range
            "2026-00-01",        // month out of range
            "2026-09-32",        // day out of range
            "2026-09-00",        // day out of range
            "20\u{e9}6-09-06ab", // not a character boundary at ten bytes
        ] {
            assert_eq!(parse_date(rejected), None);
        }
    }

    #[test]
    fn staleness_is_measured_in_whole_days_and_an_unreadable_date_counts_as_stale() {
        let today = parse_date("2026-09-06").unwrap_or_default();
        // Exactly seven days is inside the band; eight is not; and an absent
        // date is not a fresh one.
        assert!(!stale("2026-08-30T23:59:59Z", today));
        assert!(stale("2026-08-29T00:00:01Z", today));
        assert!(stale("", today));
    }

    #[test]
    fn upstream_parse_rejects_a_non_array_ledger_and_a_non_date_now() {
        assert!(
            matches!(Upstream::parse("{}", "2026-09-06"), Err(error) if error.contains("array"))
        );
        assert!(
            matches!(Upstream::parse("[]", "the sixth"), Err(error) if error.contains("YYYY-MM-DD"))
        );
        let parsed = Upstream::parse(r#"[{"id":"bd-ib-ott6"}]"#, "2026-09-06");
        assert!(matches!(&parsed, Ok(up) if up.find("bd-ib-ott6").is_some()));
        assert!(matches!(&parsed, Ok(up) if up.find("bd-ib-absent").is_none()));
    }

    #[test]
    fn parse_args_reads_the_ledger_path_and_the_cross_tenant_pair() {
        let line = ["l.json", "--upstream", "o.json", "--now", "2026-09-06"]
            .map(str::to_owned)
            .to_vec();
        let parsed = parse_args(&line);
        assert!(matches!(&parsed, Ok(read) if read.ledger.as_deref() == Some("l.json")));
        assert!(matches!(&parsed, Ok(read) if read.upstream.as_deref() == Some("o.json")));
        assert!(matches!(&parsed, Ok(read) if read.now.as_deref() == Some("2026-09-06")));
        let bare = parse_args(&[]);
        assert!(matches!(&bare, Ok(read) if read == &Args::default()));
    }

    #[test]
    fn parse_args_refuses_every_malformed_command_line() {
        for (words, expected) in [
            (vec!["-x"], "unknown flag"),
            (vec!["--upstream"], "needs a value"),
            (vec!["a.json", "b.json"], "second ledger path"),
            (vec!["--upstream", "o.json"], "go together"),
            (vec!["--now", "2026-09-06"], "go together"),
        ] {
            let line = words.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert!(matches!(parse_args(&line), Err(error) if error.contains(expected)));
        }
    }

    #[test]
    fn cross_tenant_rules_are_skipped_without_an_upstream_ledger_or_an_upstream_id() {
        let blank = proxy_with("p", "blocked", UPSTREAM_ID_KEY, "  ");
        assert!(check(&[proxy("p", "blocked")], None).findings.is_empty());
        let report = check(&[blank], Some(&upstream("closed", "2026-09-05")));
        assert_eq!(
            report.findings,
            vec![Finding::ProxyMissingMetadata {
                id: "p".to_owned(),
                field: UPSTREAM_ID_KEY
            }]
        );
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn rule_u_names_both_ids_when_the_upstream_ledger_does_not_hold_the_target() {
        let stranger = proxy_with("p", "blocked", UPSTREAM_ID_KEY, " bd-ib-gone ");
        let finding = Finding::ProxyUpstreamUnknown {
            id: "p".to_owned(),
            upstream: "bd-ib-gone".to_owned(),
        };
        assert_eq!(
            check(&[stranger], Some(&upstream("open", "2026-09-05"))).findings,
            vec![finding.clone()]
        );
        let text = finding.to_string();
        assert!(text.contains("p names") && text.contains("bd-ib-gone"));
        assert_eq!(
            finding.failure_mode(),
            "upstream-dep-proxy-upstream-unknown"
        );
    }

    #[test]
    fn rule_d_refuses_a_still_open_proxy_whose_upstream_item_has_closed() {
        let finding = Finding::ProxyStaleUpstreamClosed {
            id: "p".to_owned(),
            status: "blocked".to_owned(),
            upstream: "bd-ib-ott6".to_owned(),
        };
        assert_eq!(
            check(
                &[proxy("p", "blocked")],
                Some(&upstream(CLOSED, "2026-09-05"))
            )
            .findings,
            vec![finding.clone()]
        );
        let text = finding.to_string();
        assert!(text.contains("still `blocked`") && text.contains("bd-ib-ott6"));
        assert_eq!(
            finding.failure_mode(),
            "upstream-dep-proxy-stale-upstream-closed"
        );
    }

    #[test]
    fn rule_e_refuses_a_closed_proxy_unless_the_release_is_recorded() {
        let finding = Finding::ProxyClosedUpstreamOpen {
            id: "p".to_owned(),
            upstream: "bd-ib-ott6".to_owned(),
            upstream_status: "in_progress".to_owned(),
        };
        let open = upstream("in_progress", "2026-09-05");
        assert_eq!(
            check(&[proxy("p", CLOSED)], Some(&open)).findings,
            vec![finding.clone()]
        );
        let blank = proxy_with("p", CLOSED, PROXY_RELEASED_REASON, "   ");
        assert_eq!(check(&[blank], Some(&open)).findings, vec![finding.clone()]);
        let released = proxy_with("p", CLOSED, PROXY_RELEASED_REASON, "superseded, recorded");
        assert!(check(&[released], Some(&open)).findings.is_empty());
        // A closed proxy whose upstream item also closed is the normal end.
        assert!(
            check(&[proxy("p", CLOSED)], Some(&upstream(CLOSED, "2026-09-05")))
                .findings
                .is_empty()
        );
        let text = finding.to_string();
        assert!(text.contains("`in_progress`") && text.contains(PROXY_RELEASED_REASON));
        assert_eq!(
            finding.failure_mode(),
            "upstream-dep-proxy-closed-upstream-open"
        );
    }

    #[test]
    fn warning_w_fires_past_the_band_never_refuses_and_names_an_unreadable_date() {
        let fresh = check(
            &[proxy("p", "blocked")],
            Some(&upstream("open", "2026-09-05")),
        );
        assert!(fresh.findings.is_empty() && fresh.warnings.is_empty());
        let stalled = check(
            &[proxy("p", "blocked")],
            Some(&upstream("open", "2026-08-01T00:00:00Z")),
        );
        assert!(stalled.findings.is_empty());

        assert_eq!(
            stalled.warnings,
            vec![Warning {
                proxy: "p".to_owned(),
                upstream: "bd-ib-ott6".to_owned(),
                updated_at: "2026-08-01T00:00:00Z".to_owned(),
            }]
        );
        let undated = check(&[proxy("p", "blocked")], Some(&upstream("open", "")));
        assert_eq!(
            undated.warnings,
            vec![Warning {
                proxy: "p".to_owned(),
                upstream: "bd-ib-ott6".to_owned(),
                updated_at: UNKNOWN_UPDATED_AT.to_owned(),
            }]
        );
        let warning = &undated.warnings[0];
        let text = warning.to_string();
        assert!(text.contains("p waits on upstream bd-ib-ott6"));
        assert!(text.contains(UNKNOWN_UPDATED_AT) && text.contains("7 days"));
        assert_eq!(warning.warning_mode(), UPSTREAM_STALE_WARNING);
    }
}
