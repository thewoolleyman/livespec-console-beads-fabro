//! `console-spec-check` — behavioral-coverage primitives (clause -> scenario
//! -> test), per the Behavioral Coverage section of
//! `SPECIFICATION/non-functional-requirements.md`.
//!
//! This library ports livespec's Python plumbing to Rust:
//!
//! - the shared `spec_clauses.py` gap-id primitive — [`extract_rules`] and
//!   [`derive_gap_id`] are byte-identical to the family module the
//!   orchestrator's `detect-impl-gaps` vendors, so the gap-ids derived here
//!   map across the family (pinned by a parity test);
//! - the `behavior_scenario_link.py` clause -> scenario guardrail — a clause
//!   is linked when `tests/heading-coverage.json` binds its gap-id to a live
//!   scenario H2 in the audience-appropriate target;
//! - plus the scenario -> test enforcement dimension — every live scenario H2
//!   must carry a registry entry with a non-empty test.
//!
//! All functions are pure (no I/O, no process exit); the binary shim
//! (`main.rs`) supplies the file reads, the severity lever, and the exit code.

//!
//! ```rust,ignore
//! use console_spec_check::{derive_gap_id, extract_rules};
//!
//! let id = derive_gap_id("spec.md", "Root", "The console MUST render lanes.");
//! let rules = extract_rules("spec.md", "# Root\nThe console MUST render lanes.");
//! assert_eq!(rules[0].gap_id, id);
//! ```
#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};

use serde_json::Value;
use sha2::{Digest, Sha256};

/// The contributor-facing spec file (its own clauses bind to its
/// `## Scenarios` section, not to `scenarios.md`).
pub const NFR_FILE: &str = "non-functional-requirements.md";

/// The environment lever selecting the gate severity (`warn` default, `fail`
/// to enforce). Mirrors livespec's `LIVESPEC_BEHAVIOR_SCENARIO_LINK`.
pub const SEVERITY_ENV: &str = "LIVESPEC_BEHAVIOR_SCENARIO_LINK";

/// The operator-facing clause-bearing spec files (their clauses bind to
/// `scenarios.md`).
pub const OPERATOR_FILES: [&str; 3] = ["spec.md", "contracts.md", "constraints.md"];

const GAP_ID_LEN: usize = 8;

// ---------------------------------------------------------------------------
// gap-id primitive — parity with livespec `spec_clauses.py`.
// ---------------------------------------------------------------------------

/// A single `MUST` / `MUST NOT` / `SHOULD` / `SHOULD NOT` rule detected in a
/// spec file, with its derived gap-id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleMatch {
    /// Spec file field.
    pub spec_file: String,
    /// Heading path field.
    pub heading_path: String,
    /// Line text field.
    pub line_text: String,
    /// Gap id field.
    pub gap_id: String,
}

/// Derive the stable `gap-<8>` id for a single rule.
///
/// The id is `gap-` followed by the first eight lowercase base32 characters of
/// `sha256(spec_file \x1f heading_path \x1f rule_text)`. Pure function of its
/// inputs — byte-identical to the family `spec_clauses.derive_gap_id`.
#[must_use]
pub fn derive_gap_id(spec_file: &str, heading_path: &str, rule_text: &str) -> String {
    let payload = format!("{spec_file}\u{1f}{heading_path}\u{1f}{rule_text}");
    let digest = Sha256::digest(payload.as_bytes());
    let mut id = String::with_capacity("gap-".len() + GAP_ID_LEN);
    id.push_str("gap-");
    id.extend(base32_first8_lower(&digest));
    id
}

/// Lowercase RFC4648 base32 of the first five digest bytes — exactly eight
/// characters, identical to Python `b32encode(...).lower()[:8]` (which depends
/// only on the first five bytes, as base32 encodes in five-byte groups).
fn base32_first8_lower(digest: &[u8]) -> [char; GAP_ID_LEN] {
    const ALPHABET: [char; 32] = [
        'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r',
        's', 't', 'u', 'v', 'w', 'x', 'y', 'z', '2', '3', '4', '5', '6', '7',
    ];
    let bits = (u64::from(digest[0]) << 32)
        | (u64::from(digest[1]) << 24)
        | (u64::from(digest[2]) << 16)
        | (u64::from(digest[3]) << 8)
        | u64::from(digest[4]);
    let mut out = ['a'; GAP_ID_LEN];
    for (index, slot) in out.iter_mut().enumerate() {
        let shift = 35 - index * 5;
        let group = (bits >> shift) & 0x1f;
        *slot = ALPHABET[usize::try_from(group).unwrap_or(0)];
    }
    out
}

/// Enumerate the `MUST` / `MUST NOT` / `SHOULD` / `SHOULD NOT` rule lines in a
/// single spec file's content, in document order.
///
/// Lines inside fenced code blocks are skipped; markdown headings build the
/// ` > `-joined `heading_path`. Byte-identical extraction to the family
/// `spec_clauses.extract_rules_from_file`.
#[must_use]
pub fn extract_rules(spec_file: &str, content: &str) -> Vec<RuleMatch> {
    let mut rules = Vec::new();
    let mut heading_stack: Vec<String> = Vec::new();
    let mut in_code_fence = false;
    for raw_line in content.split('\n') {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.trim_start().starts_with("```") {
            in_code_fence = !in_code_fence;
            continue;
        }
        if in_code_fence {
            continue;
        }
        if let Some((level, title)) = parse_heading(line) {
            push_heading(&mut heading_stack, level, title);
            continue;
        }
        if !has_rule_keyword(line) {
            continue;
        }
        let line_text = line.trim().to_string();
        let heading_path = if heading_stack.is_empty() {
            "(top)".to_string()
        } else {
            heading_stack.join(" > ")
        };
        let gap_id = derive_gap_id(spec_file, &heading_path, &line_text);
        rules.push(RuleMatch {
            spec_file: spec_file.to_string(),
            heading_path,
            line_text,
            gap_id,
        });
    }
    rules
}

/// Parse a markdown ATX heading line.
///
/// Matches the family regex `^(#{1,6})\s+(.+?)\s*$`: one to six leading `#`,
/// then required whitespace, then a non-empty title (surrounding whitespace
/// stripped).
fn parse_heading(line: &str) -> Option<(usize, String)> {
    let hashes = line.bytes().take_while(|&byte| byte == b'#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &line[hashes..];
    let after_whitespace = rest.trim_start();
    if after_whitespace.len() == rest.len() {
        return None;
    }
    let title = after_whitespace.trim_end();
    if title.is_empty() {
        return None;
    }
    Some((hashes, title.to_string()))
}

/// Update the heading-breadcrumb stack for a heading at `level`.
///
/// Matches the family `_push_heading`: pop to above `level`, pad missing
/// intermediate levels with empty crumbs, then push the title.
fn push_heading(stack: &mut Vec<String>, level: usize, title: String) {
    while stack.len() >= level {
        let _ = stack.pop();
    }
    while stack.len() + 1 < level {
        stack.push(String::new());
    }
    stack.push(title);
}

/// Whether a line carries a `MUST` / `SHOULD` rule keyword (case-sensitive,
/// whole word). `MUST NOT` / `SHOULD NOT` are detected by their `MUST` /
/// `SHOULD` prefix, so this is equivalent to the family regex alternation.
fn has_rule_keyword(line: &str) -> bool {
    contains_whole_word(line, "MUST") || contains_whole_word(line, "SHOULD")
}

/// Whether `word` (ASCII) occurs in `haystack` as a whole word — bounded on
/// both sides by a non-word character or a string edge, matching `\bword\b`.
fn contains_whole_word(haystack: &str, word: &str) -> bool {
    let mut search_from = 0;
    while let Some(relative) = haystack[search_from..].find(word) {
        let start = search_from + relative;
        let before = haystack[..start].chars().next_back();
        let after = haystack[start + word.len()..].chars().next();
        if before.is_none_or(|character| !is_word_char(character))
            && after.is_none_or(|character| !is_word_char(character))
        {
            return true;
        }
        search_from = start + 1;
    }
    false
}

/// A regex `\w` word character: Unicode alphanumeric or underscore.
fn is_word_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

// ---------------------------------------------------------------------------
// Link registry — the `tests/heading-coverage.json` `clauses[]` shape.
// ---------------------------------------------------------------------------

/// One clause -> scenario link inside a registry entry's `clauses[]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClauseLink {
    /// Gap id field.
    pub gap_id: String,
    /// Scenario field.
    pub scenario: String,
}

/// One registry entry: a scenario H2 (`scenario` in `scenario_file`), its
/// top-of-pyramid `test`, and the clauses linked to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageEntry {
    /// Scenario field.
    pub scenario: String,
    /// Scenario file field.
    pub scenario_file: String,
    /// Test field.
    pub test: String,
    /// Clauses field.
    pub clauses: Vec<ClauseLink>,
}

/// Parse the `tests/heading-coverage.json` registry.
///
/// Malformed entries are skipped (mirroring the family's defensive parse);
/// only a non-array top level or invalid JSON is a hard error.
///
/// # Errors
/// Returns an error when the text is not valid JSON or not a JSON array.
pub fn parse_registry(json: &str) -> Result<Vec<CoverageEntry>, String> {
    let value: Value =
        serde_json::from_str(json).map_err(|error| format!("invalid registry JSON: {error}"))?;
    let array = value
        .as_array()
        .ok_or_else(|| "registry must be a JSON array".to_string())?;
    Ok(array.iter().filter_map(parse_entry).collect())
}

fn parse_entry(item: &Value) -> Option<CoverageEntry> {
    let object = item.as_object()?;
    let scenario = object.get("scenario")?.as_str()?.to_string();
    let scenario_file = object.get("scenario_file")?.as_str()?.to_string();
    let test = object
        .get("test")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let clauses = object.get("clauses").map(parse_clauses).unwrap_or_default();
    Some(CoverageEntry {
        scenario,
        scenario_file,
        test,
        clauses,
    })
}

fn parse_clauses(value: &Value) -> Vec<ClauseLink> {
    value
        .as_array()
        .map(|array| array.iter().filter_map(parse_clause).collect())
        .unwrap_or_default()
}

fn parse_clause(item: &Value) -> Option<ClauseLink> {
    let object = item.as_object()?;
    let gap_id = object.get("gap_id")?.as_str()?.to_string();
    let scenario = object.get("scenario")?.as_str()?.to_string();
    Some(ClauseLink { gap_id, scenario })
}

// ---------------------------------------------------------------------------
// Scenario sections — the link targets.
// ---------------------------------------------------------------------------

/// The operator-facing scenario H2 section names in `scenarios.md`.
#[must_use]
pub fn operator_scenarios(scenarios_md: &str) -> Vec<String> {
    h2_sections(scenarios_md)
}

/// The contributor-facing scenario H2 section names — the `## ` H2 headings
/// that appear after the `## Scenarios` marker in `non-functional-requirements.md`.
/// Empty until those scenarios are authored.
#[must_use]
pub fn nfr_scenarios(nfr_md: &str) -> Vec<String> {
    let mut sections = Vec::new();
    let mut after_marker = false;
    for raw_line in nfr_md.split('\n') {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if let Some(name) = h2_name(line) {
            if after_marker {
                sections.push(name);
            } else if name == "Scenarios" {
                after_marker = true;
            }
        }
    }
    sections
}

fn h2_sections(text: &str) -> Vec<String> {
    text.split('\n')
        .filter_map(|raw_line| h2_name(raw_line.strip_suffix('\r').unwrap_or(raw_line)))
        .collect()
}

fn h2_name(line: &str) -> Option<String> {
    match parse_heading(line) {
        Some((2, title)) => Some(title),
        _ => None,
    }
}

/// Normalize a scenario reference for matching: trim, drop any leading `#`,
/// trim again — matching the family `_normalize_scenario`.
fn normalize_scenario(value: &str) -> String {
    value.trim().trim_start_matches('#').trim().to_string()
}

// ---------------------------------------------------------------------------
// Evaluation — clause -> scenario and scenario -> test.
// ---------------------------------------------------------------------------

/// Which scenario target a clause's source file binds to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    /// Operator-facing source (`spec.md` / `contracts.md` / `constraints.md`)
    /// — clauses bind to `scenarios.md`.
    Operator,
    /// Contributor-facing source (`non-functional-requirements.md`) — clauses
    /// bind to the NFR `## Scenarios` section.
    Contributor,
}

/// A clause-bearing spec source plus the audience that selects its scenario
/// target set.
#[derive(Debug, Clone, Copy)]
pub struct SpecSource<'a> {
    /// Spec file field.
    pub spec_file: &'a str,
    /// Content field.
    pub content: &'a str,
    /// Audience field.
    pub audience: Audience,
}

/// A normative clause with no resolving scenario link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnlinkedClause {
    /// Spec file field.
    pub spec_file: String,
    /// Heading path field.
    pub heading_path: String,
    /// Gap id field.
    pub gap_id: String,
    /// Clause field.
    pub clause: String,
}

/// A live scenario H2 with no registered test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UntestedScenario {
    /// Scenario file field.
    pub scenario_file: String,
    /// Scenario field.
    pub scenario: String,
}

/// The outcome of an evaluation: the clauses lacking a scenario link and the
/// scenarios lacking a test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageReport {
    /// Unlinked clauses field.
    pub unlinked_clauses: Vec<UnlinkedClause>,
    /// Untested scenarios field.
    pub untested_scenarios: Vec<UntestedScenario>,
}

impl CoverageReport {
    /// Whether every clause is linked and every scenario tested.
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.unlinked_clauses.is_empty() && self.untested_scenarios.is_empty()
    }
}

/// The gate severity: report-only or enforcing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Warn variant.
    Warn,
    /// Fail variant.
    Fail,
}

/// Resolve the severity lever from its raw environment value; unset or
/// unrecognized defaults to [`Mode::Warn`] (only `fail`, case-insensitively,
/// selects [`Mode::Fail`]).
#[must_use]
pub fn resolve_mode(raw: Option<&str>) -> Mode {
    match raw {
        Some(value) if value.trim().eq_ignore_ascii_case("fail") => Mode::Fail,
        _ => Mode::Warn,
    }
}

/// Evaluate the clause -> scenario -> test chain over the spec `sources`, the
/// link `registry`, and the live operator / NFR scenario section names.
///
/// A clause is linked when the registry binds its gap-id to a scenario that
/// resolves to a live H2 in the set its [`Audience`] selects. A live scenario
/// is tested when the registry carries an entry for it (matched by file and
/// name) with a non-empty test.
#[must_use]
pub fn evaluate(
    sources: &[SpecSource],
    registry: &[CoverageEntry],
    operator_scenario_sections: &[String],
    nfr_scenario_sections: &[String],
) -> CoverageReport {
    let operator_live = normalized_set(operator_scenario_sections);
    let nfr_live = normalized_set(nfr_scenario_sections);

    let mut links: HashMap<String, Vec<String>> = HashMap::new();
    for entry in registry {
        for clause in &entry.clauses {
            links
                .entry(clause.gap_id.clone())
                .or_default()
                .push(normalize_scenario(&clause.scenario));
        }
    }

    let mut unlinked_clauses = Vec::new();
    for source in sources {
        let live = match source.audience {
            Audience::Operator => &operator_live,
            Audience::Contributor => &nfr_live,
        };
        for rule in extract_rules(source.spec_file, source.content) {
            let linked = links
                .get(&rule.gap_id)
                .is_some_and(|names| names.iter().any(|name| live.contains(name)));
            if !linked {
                unlinked_clauses.push(UnlinkedClause {
                    spec_file: rule.spec_file,
                    heading_path: rule.heading_path,
                    gap_id: rule.gap_id,
                    clause: rule.line_text,
                });
            }
        }
    }

    let mut untested_scenarios =
        missing_tests("scenarios.md", operator_scenario_sections, registry);
    untested_scenarios.extend(missing_tests(NFR_FILE, nfr_scenario_sections, registry));

    CoverageReport {
        unlinked_clauses,
        untested_scenarios,
    }
}

fn normalized_set(sections: &[String]) -> HashSet<String> {
    sections
        .iter()
        .map(|section| normalize_scenario(section))
        .collect()
}

fn missing_tests(
    scenario_file: &str,
    live: &[String],
    registry: &[CoverageEntry],
) -> Vec<UntestedScenario> {
    let tested: HashSet<String> = registry
        .iter()
        .filter(|entry| entry.scenario_file == scenario_file && !entry.test.trim().is_empty())
        .map(|entry| normalize_scenario(&entry.scenario))
        .collect();
    live.iter()
        .map(|section| normalize_scenario(section))
        .filter(|name| !tested.contains(name))
        .map(|name| UntestedScenario {
            scenario_file: scenario_file.to_string(),
            scenario: name,
        })
        .collect()
}

#[cfg(test)]
mod tests;
