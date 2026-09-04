//! `console-upstream-dep-check` — the general upstream-dependency gate over the
//! beads ledger (`livespec-console-beads-fabro-pzbdbo.1`). API surface only;
//! the rules land through the Red→Green ritual.

#![forbid(unsafe_code)]

use std::fmt;

use serde_json::Value;

/// Label prefix that marks an item as a proxy for an upstream dependency.
pub const UPSTREAM_DEP_LABEL_PREFIX: &str = "upstream-dep:";
/// Every proxy title announces the block with this prefix.
pub const PROXY_TITLE_PREFIX: &str = "BLOCKED-ON";
/// First line of the guard paragraph prepended to governed items.
pub const GUARD_HEAD: &str = "⛔ NEVER WORK AROUND AN UPSTREAM ORCHESTRATOR DEPENDENCY";
/// Phrases that record a workaround.
pub const DEVIATION_MARKERS: &[&str] = &[];
/// Substrings that tie a marker phrase to the orchestrator.
pub const UPSTREAM_REFERENCES: &[&str] = &[];
/// Metadata every proxy must carry.
pub const REQUIRED_PROXY_METADATA: &[&str] = &["upstream_work_item_id", "plan_ref"];

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
        "upstream-dep-unimplemented"
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
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
/// Always, until the gate lands.
pub fn parse_ledger(text: &str) -> Result<Vec<Value>, String> {
    Err(format!("gate not implemented ({} bytes)", text.len()))
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
pub const fn check(items: &[Value]) -> Report {
    Report {
        scanned: items.len(),
        findings: Vec::new(),
    }
}

/// The evidence line or phrase when a description records a deviation.
#[must_use]
pub const fn deviation_evidence(_description: &str) -> Option<String> {
    None
}

/// The description with every guard paragraph removed.
#[must_use]
pub fn strip_guard(description: &str) -> String {
    description.to_owned()
}
