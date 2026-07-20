//! Settings-surface completeness primitives — the API-to-Settings-to-help-to-doc
//! lockstep gate, per `SPECIFICATION/contracts.md` "Settings-surface
//! completeness" and `scenarios.md` "Settings surface stays in lockstep with the
//! orchestrator's declared keys".
//!
//! Every key the orchestrator declares as API-configurable (its published
//! `config-manifest`) MUST appear, in lockstep, in three console places: a
//! Settings-surface row, that row's inline help, and the console's settings doc
//! (`docs/detailed-usage.md`). This crate is the CONSUMER-side check (No-Circular-Dependency
//! Directive): it reads the orchestrator's PUBLISHED declared-key surface and
//! compares it against the console's own surfaces; nothing here reads orchestrator
//! internals, and the declared-key list is read from the manifest, never hardcoded.
//!
//! The published-key surface is read from a COMMITTED capture of the orchestrator's
//! `config-manifest` (hermetic — `just check`/CI run offline, no live orchestrator).
//! A capture goes stale when its declared key set changes without being refreshed,
//! so the capture is DIGEST-STAMPED with the declared key set. The check FAILS
//! when that stamp differs from the fixture's current declared-key digest. A core
//! pin bump alone does not invalidate the capture; a true key-set change still
//! fails closed until `just refresh-config-manifest` re-captures the live surface.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use console_application::DispatcherSettingRow;
use sha2::{Digest, Sha256};

/// One console Settings-surface row reduced to what the completeness check needs:
/// the orchestrator `dispatcher.*` key it surfaces and its inline help text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsRow {
    key: String,
    help: String,
}

impl SettingsRow {
    #[must_use]
    /// Construct a row surface from its orchestrator key and inline help.
    pub const fn new(key: String, help: String) -> Self {
        Self { key, help }
    }

    #[must_use]
    /// The orchestrator `dispatcher.*` key this row surfaces.
    pub fn key(&self) -> &str {
        &self.key
    }

    #[must_use]
    /// The row's inline / context help text.
    pub fn help(&self) -> &str {
        &self.help
    }
}

/// The console's live Settings surface, derived from the [`DispatcherSettingRow`]
/// enum the TUI renders (its key + inline help per row).
#[must_use]
pub fn console_settings_rows() -> Vec<SettingsRow> {
    DispatcherSettingRow::all()
        .iter()
        .map(|row| SettingsRow::new(row.orchestrator_key().to_owned(), row.help().to_owned()))
        .collect()
}

/// The result of the completeness check: the declared keys missing from each of
/// the three required console surfaces. All-empty means the surfaces are complete.
// The three fields deliberately share the `missing_` prefix — one per surface a
// key must reach — which is the clearest naming; the pedantic same-prefix lint is
// not helpful here.
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompletenessReport {
    missing_settings_row: Vec<String>,
    missing_help: Vec<String>,
    missing_doc: Vec<String>,
}

impl CompletenessReport {
    #[must_use]
    /// Whether every declared key reaches all three surfaces.
    pub const fn is_clean(&self) -> bool {
        self.missing_settings_row.is_empty()
            && self.missing_help.is_empty()
            && self.missing_doc.is_empty()
    }

    #[must_use]
    /// Declared keys with no console Settings row.
    pub fn missing_settings_row(&self) -> &[String] {
        &self.missing_settings_row
    }

    #[must_use]
    /// Declared keys whose Settings row carries no inline help.
    pub fn missing_help(&self) -> &[String] {
        &self.missing_help
    }

    #[must_use]
    /// Declared keys absent from the settings doc.
    pub fn missing_doc(&self) -> &[String] {
        &self.missing_doc
    }

    #[must_use]
    /// One diagnostic line per missing (key, surface) pair, each NAMING the key so
    /// the operator can see exactly which declared key fell out of lockstep.
    pub fn diagnostics(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for key in &self.missing_settings_row {
            lines.push(format!("declared key `{key}` has no console Settings row"));
        }
        for key in &self.missing_help {
            lines.push(format!(
                "declared key `{key}` has no inline help on its row"
            ));
        }
        for key in &self.missing_doc {
            lines.push(format!(
                "declared key `{key}` is not documented in the settings doc"
            ));
        }
        lines
    }
}

/// Parse the declared API-configurable keys from the orchestrator's published
/// `config-manifest` output.
///
/// The shape is `{ "manifest": { "keys": [ { "key": ... } ] } }`, as the
/// orchestrator's `drive --action config-manifest --json` emits. The key list is
/// READ from the manifest here, never hardcoded, so a key the orchestrator adds
/// is picked up with no change to this check.
///
/// # Errors
/// Returns an error string when the JSON does not parse, the `manifest.keys`
/// array is absent, a key entry lacks a string `key`, or no keys are declared.
pub fn declared_keys(manifest_json: &str) -> Result<Vec<String>, String> {
    let value: serde_json::Value = serde_json::from_str(manifest_json)
        .map_err(|error| format!("config-manifest is not valid JSON: {error}"))?;
    declared_keys_from_value(&value)
}

fn declared_keys_from_value(value: &serde_json::Value) -> Result<Vec<String>, String> {
    let entries = value
        .get("manifest")
        .and_then(|manifest| manifest.get("keys"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "config-manifest has no `manifest.keys` array".to_owned())?;
    let mut keys = Vec::new();
    for entry in entries {
        let key = entry
            .get("key")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "a config-manifest key entry has no string `key`".to_owned())?;
        keys.push(key.to_owned());
    }
    if keys.is_empty() {
        return Err("config-manifest declares no keys".to_owned());
    }
    Ok(keys)
}

/// The console's settings doc, relative to the repository root.
///
/// The User Documentation Contract (`SPECIFICATION/contracts.md`) names this
/// file: the settings doc MUST be `docs/detailed-usage.md` and MUST NOT be the
/// top-level `README.md`. It superseded the earlier settings-doc-is-the-README
/// anchor, which held only while the console had no `docs/` tree.
pub const SETTINGS_DOC: &str = "docs/detailed-usage.md";

/// The heading that opens the settings doc's Dispatcher-settings section.
/// Substring-matched (level-agnostic) so a heading-level tweak does not silently
/// unscope the check.
const SETTINGS_SECTION_MARKER: &str = "Dispatcher settings";

/// The slice of `settings_doc` that is the Dispatcher-settings section: from its heading
/// line to the next heading of the same-or-higher level (or end of file). Empty
/// when no such heading exists.
///
/// Scoping the doc match to this section is deliberate: the six keys are also
/// named in the keybinding table and prose ELSEWHERE in the settings doc, so an
/// unscoped whole-document substring match would false-pass a key that is mentioned
/// incidentally but never documented as a setting.
#[must_use]
pub fn dispatcher_settings_section(settings_doc: &str) -> &str {
    let mut section_start: Option<usize> = None;
    let mut section_level = 0usize;
    // Walk line by line via byte offsets so the returned slice borrows `settings_doc`.
    for (line_start, line) in line_spans(settings_doc) {
        let level = heading_level(line);
        let Some(start) = section_start else {
            if level > 0 && line.contains(SETTINGS_SECTION_MARKER) {
                section_start = Some(line_start);
                section_level = level;
            }
            continue;
        };
        // In the section: a heading of the same-or-higher level ends it.
        if level > 0 && level <= section_level {
            return &settings_doc[start..line_start];
        }
    }
    section_start.map_or("", |start| &settings_doc[start..])
}

/// The heading level of a line (count of leading `#`), or `0` when it is not an
/// ATX heading (`#`..`######` followed by a space).
fn heading_level(line: &str) -> usize {
    let hashes = line
        .chars()
        .take_while(|character| *character == '#')
        .count();
    if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
        hashes
    } else {
        0
    }
}

/// Yield each line of `source` as `(byte_offset_of_line_start, line_without_newline)`.
fn line_spans(source: &str) -> Vec<(usize, &str)> {
    let mut spans = Vec::new();
    let mut start = 0usize;
    for (index, character) in source.char_indices() {
        if character == '\n' {
            spans.push((start, &source[start..index]));
            start = index + 1;
        }
    }
    if start <= source.len() {
        spans.push((start, &source[start..]));
    }
    spans
}

/// Evaluate the API-to-Settings-to-help-to-doc lockstep.
///
/// For EACH declared key, require a console Settings row, non-empty inline help
/// on that row, and a mention in the settings doc's Dispatcher-settings
/// section (see [`dispatcher_settings_section`]). A key missing any surface is
/// named in the returned report.
#[must_use]
pub fn evaluate(
    declared: &[String],
    rows: &[SettingsRow],
    settings_doc: &str,
) -> CompletenessReport {
    let mut report = CompletenessReport::default();
    let section = dispatcher_settings_section(settings_doc);
    for key in declared {
        match rows.iter().find(|row| row.key() == key) {
            None => report.missing_settings_row.push(key.clone()),
            Some(row) if row.help().trim().is_empty() => report.missing_help.push(key.clone()),
            Some(_row) => {}
        }
        if !section.contains(key.as_str()) {
            report.missing_doc.push(key.clone());
        }
    }
    report
}

/// A stale-capture finding.
///
/// The key-set digest stamped into the config-manifest fixture (`found`) differs
/// from the digest of the key set the fixture currently declares (`expected`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeySetDigestMismatch {
    expected: String,
    found: String,
}

impl KeySetDigestMismatch {
    #[must_use]
    /// The digest of the key set the fixture currently declares.
    pub fn expected(&self) -> &str {
        &self.expected
    }

    #[must_use]
    /// The key-set digest stamped into the fixture capture.
    pub fn found(&self) -> &str {
        &self.found
    }

    #[must_use]
    /// The operator-facing diagnostic naming both digests and the remediation.
    pub fn diagnostic(&self) -> String {
        format!(
            "the config-manifest capture was stamped with key-set digest `{}` but \
             the fixture now declares key-set digest `{}` -- the capture is stale; run \
             `just refresh-config-manifest`",
            self.found, self.expected
        )
    }
}

/// Strip JSONC comments (`//` line and `/* */` block), preserving comment-like
/// text inside string literals, so the result parses as strict JSON.
#[must_use]
pub fn strip_jsonc_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(character) = chars.next() {
        if in_string {
            out.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        match character {
            '"' => {
                in_string = true;
                out.push('"');
            }
            '/' if chars.peek() == Some(&'/') => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                let _asterisk = chars.next();
                let mut prev = '\0';
                for next in chars.by_ref() {
                    if prev == '*' && next == '/' {
                        break;
                    }
                    prev = next;
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// Compute the stable digest for an orchestrator config-manifest's declared key
/// set.
///
/// The digest is order-independent because it represents the set of declared
/// keys, not the manifest's emission order. Each key is length-framed before
/// hashing to avoid ambiguity between adjacent strings.
///
/// # Errors
/// Returns an error string when the manifest's declared keys cannot be read.
pub fn declared_key_set_digest(manifest_json: &str) -> Result<String, String> {
    let keys = declared_keys(manifest_json)?;
    Ok(digest_declared_keys(keys))
}

fn digest_declared_keys(mut keys: Vec<String>) -> String {
    keys.sort_unstable();

    let mut hasher = Sha256::new();
    for key in keys {
        hasher.update(key.len().to_be_bytes());
        hasher.update(key.as_bytes());
    }
    let digest = hasher.finalize();
    hex_lower(&digest)
}

/// Read the declared-key digest a config-manifest fixture was captured with from
/// its top-level `captured_key_set_digest` field.
///
/// # Errors
/// Returns an error string when the JSON does not parse or the field is absent --
/// an unstamped fixture is treated as stale (refresh it).
pub fn captured_key_set_digest(manifest_json: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(manifest_json)
        .map_err(|error| format!("config-manifest is not valid JSON: {error}"))?;
    value
        .get("captured_key_set_digest")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            "config-manifest capture has no `captured_key_set_digest` field; run \
             `just refresh-config-manifest`"
                .to_owned()
        })
}

/// Compare the fixture's stamped key-set digest against its declared key set.
///
/// `Ok(None)` when they match, `Ok(Some(mismatch))` when the capture is stale.
///
/// # Errors
/// Returns an error string when either digest cannot be read.
pub fn check_key_set_digest(manifest_json: &str) -> Result<Option<KeySetDigestMismatch>, String> {
    let expected = declared_key_set_digest(manifest_json)?;
    let found = captured_key_set_digest(manifest_json)?;
    if expected == found {
        Ok(None)
    } else {
        Ok(Some(KeySetDigestMismatch { expected, found }))
    }
}

/// Stamp the declared-key digest into a fresh `config-manifest` output.
///
/// Inserts a top-level `captured_key_set_digest` field and returns the
/// pretty-printed JSON to write as the committed fixture. Used by
/// `just refresh-config-manifest`.
///
/// # Errors
/// Returns an error string when the drive output is not valid JSON or is not a
/// JSON object, or when its declared keys cannot be read.
pub fn stamp_manifest(drive_output_json: &str) -> Result<String, String> {
    let mut value: serde_json::Value = serde_json::from_str(drive_output_json)
        .map_err(|error| format!("config-manifest output is not valid JSON: {error}"))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "config-manifest output is not a JSON object".to_owned())?;
    let digest = digest_declared_keys(declared_keys_from_value(&serde_json::Value::Object(
        object.clone(),
    ))?);
    let _previous = object.insert(
        "captured_key_set_digest".to_owned(),
        serde_json::Value::String(digest),
    );
    // Serializing a `serde_json::Value` back to text is infallible in practice;
    // fall back to an empty string rather than carry an untestable error branch.
    Ok(serde_json::to_string_pretty(&value).unwrap_or_default())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        CompletenessReport, KeySetDigestMismatch, SettingsRow, captured_key_set_digest,
        check_key_set_digest, console_settings_rows, declared_key_set_digest, declared_keys,
        dispatcher_settings_section, evaluate, stamp_manifest, strip_jsonc_comments,
    };

    fn manifest(keys: &[&str]) -> String {
        let entries: Vec<String> = keys
            .iter()
            .map(|key| format!("{{\"key\": \"{key}\"}}"))
            .collect();
        format!("{{\"manifest\": {{\"keys\": [{}]}}}}", entries.join(", "))
    }

    fn row(key: &str, help: &str) -> SettingsRow {
        SettingsRow::new(key.to_owned(), help.to_owned())
    }

    /// A settings doc whose Dispatcher-settings section documents the given keys, with a
    /// trailing non-settings section so the section slice is bounded.
    fn settings_doc_with_section(keys: &[&str]) -> String {
        let mut documented = String::new();
        for key in keys {
            documented.push('`');
            documented.push_str(key);
            documented.push_str("` ");
        }
        format!("### Dispatcher settings\n\n{documented}\n\n### Acting on work\n\nunrelated\n")
    }

    #[test]
    fn settings_row_exposes_its_key_and_help() {
        let row = row("wip_cap", "the per-repo ceiling");
        assert_eq!(row.key(), "wip_cap");
        assert_eq!(row.help(), "the per-repo ceiling");
    }

    #[test]
    fn console_settings_rows_surface_every_dispatcher_row_with_help() {
        let rows = console_settings_rows();
        assert_eq!(rows.len(), 6);
        for row in &rows {
            assert!(!row.key().is_empty());
            assert!(!row.help().trim().is_empty());
        }
        let keys: Vec<&str> = rows.iter().map(SettingsRow::key).collect();
        assert!(keys.contains(&"auto_approve_ready"));
        assert!(keys.contains(&"wip_cap"));
    }

    #[test]
    fn declared_keys_reads_the_manifest_key_list() {
        let keys = declared_keys(&manifest(&["auto_approve_ready", "wip_cap"]));
        assert_eq!(
            keys,
            Ok(vec!["auto_approve_ready".to_owned(), "wip_cap".to_owned()])
        );
    }

    #[test]
    fn declared_keys_rejects_malformed_manifests() {
        assert!(declared_keys("{not json").is_err());
        assert!(declared_keys("{\"manifest\": {}}").is_err());
        assert!(declared_keys("{\"manifest\": {\"keys\": [{\"nope\": 1}]}}").is_err());
        assert_eq!(
            declared_keys("{\"manifest\": {\"keys\": []}}"),
            Err("config-manifest declares no keys".to_owned())
        );
    }

    #[test]
    fn evaluate_is_clean_when_every_key_reaches_all_three_surfaces() {
        let declared = vec!["auto_approve_ready".to_owned(), "wip_cap".to_owned()];
        let rows = vec![
            row("auto_approve_ready", "auto-approve help"),
            row("wip_cap", "wip help"),
        ];
        let settings_doc = settings_doc_with_section(&["auto_approve_ready", "wip_cap"]);
        let report = evaluate(&declared, &rows, &settings_doc);
        assert!(report.is_clean());
        assert!(report.diagnostics().is_empty());
    }

    #[test]
    fn evaluate_names_a_key_missing_from_the_settings_surface() {
        let declared = vec![
            "auto_approve_ready".to_owned(),
            "new_upstream_key".to_owned(),
        ];
        let rows = vec![row("auto_approve_ready", "help")];
        let settings_doc = settings_doc_with_section(&["auto_approve_ready", "new_upstream_key"]);
        let report = evaluate(&declared, &rows, &settings_doc);
        assert!(!report.is_clean());
        assert_eq!(
            report.missing_settings_row(),
            ["new_upstream_key".to_owned()]
        );
        assert!(report.missing_help().is_empty());
        assert!(report.missing_doc().is_empty());
        assert!(
            report
                .diagnostics()
                .iter()
                .any(|line| line.contains("new_upstream_key")
                    && line.contains("no console Settings row"))
        );
    }

    #[test]
    fn evaluate_names_a_key_whose_row_has_no_inline_help() {
        let declared = vec!["wip_cap".to_owned()];
        let rows = vec![row("wip_cap", "   ")];
        let settings_doc = settings_doc_with_section(&["wip_cap"]);
        let report = evaluate(&declared, &rows, &settings_doc);
        assert_eq!(report.missing_help(), ["wip_cap".to_owned()]);
        assert!(report.missing_settings_row().is_empty());
        assert!(
            report
                .diagnostics()
                .iter()
                .any(|line| line.contains("wip_cap") && line.contains("no inline help"))
        );
    }

    #[test]
    fn evaluate_names_a_key_missing_from_the_settings_doc() {
        let declared = vec!["wip_cap".to_owned()];
        let rows = vec![row("wip_cap", "help")];
        let settings_doc = "### Dispatcher settings\n\nno keys documented here\n";
        let report = evaluate(&declared, &rows, settings_doc);
        assert_eq!(report.missing_doc(), ["wip_cap".to_owned()]);
        assert!(report.missing_settings_row().is_empty());
        assert!(report.missing_help().is_empty());
        assert!(
            report
                .diagnostics()
                .iter()
                .any(|line| line.contains("wip_cap") && line.contains("settings doc"))
        );
    }

    #[test]
    fn evaluate_scopes_the_doc_match_to_the_dispatcher_settings_section() {
        // A key mentioned ONLY outside the Dispatcher-settings section (e.g. the
        // keybinding table) is NOT documented as a setting and must be named
        // missing from the doc.
        let declared = vec!["wip_cap".to_owned()];
        let rows = vec![row("wip_cap", "help")];
        let settings_doc = "### Keys\n\n`wip_cap` is mentioned here\n\n### Dispatcher settings\n\nno setting keys here\n";
        let report = evaluate(&declared, &rows, settings_doc);
        assert_eq!(report.missing_doc(), ["wip_cap".to_owned()]);
    }

    #[test]
    fn dispatcher_settings_section_slices_to_the_next_same_level_heading() {
        let settings_doc = "### Dispatcher settings\n\nbody keys\n\n### Next\n\nafter\n";
        let section = dispatcher_settings_section(settings_doc);
        assert!(section.contains("body keys"));
        assert!(!section.contains("after"));
    }

    #[test]
    fn dispatcher_settings_section_runs_to_end_of_file_and_is_empty_when_absent() {
        let to_end = dispatcher_settings_section("## Dispatcher settings\n\nlast section\n");
        assert!(to_end.contains("last section"));
        assert_eq!(
            dispatcher_settings_section("no settings heading here\n"),
            ""
        );
    }

    #[test]
    fn completeness_report_default_is_clean() {
        let report = CompletenessReport::default();
        assert!(report.is_clean());
    }

    #[test]
    fn strip_jsonc_comments_drops_comments_but_keeps_comment_like_strings() {
        let source = "{\n  // line\n  \"url\": \"http://x\", /* block */ \"n\": 1\n}";
        let stripped = strip_jsonc_comments(source);
        let value: serde_json::Value = serde_json::from_str(&stripped).unwrap_or_default();
        assert_eq!(
            value.get("url").and_then(serde_json::Value::as_str),
            Some("http://x")
        );
        assert_eq!(value.get("n").and_then(serde_json::Value::as_u64), Some(1));
    }

    #[test]
    fn strip_jsonc_comments_respects_escaped_chars_in_strings() {
        // An escaped backslash and an escaped quote inside a string must be
        // preserved: the `\"` must NOT end the string, and `//` inside a string is
        // not a comment.
        let source = "{\"path\": \"a\\\\b // not a comment\", \"q\": \"x\\\"y\"}";
        let value: serde_json::Value =
            serde_json::from_str(&strip_jsonc_comments(source)).unwrap_or_default();
        assert_eq!(
            value.get("path").and_then(serde_json::Value::as_str),
            Some("a\\b // not a comment")
        );
        assert_eq!(
            value.get("q").and_then(serde_json::Value::as_str),
            Some("x\"y")
        );
    }

    #[test]
    fn declared_key_set_digest_is_order_independent() {
        assert_eq!(
            declared_key_set_digest(&manifest(&["auto_approve_ready", "wip_cap"])),
            declared_key_set_digest(&manifest(&["wip_cap", "auto_approve_ready"]))
        );
        assert_ne!(
            declared_key_set_digest(&manifest(&["auto_approve_ready", "wip_cap"])),
            declared_key_set_digest(&manifest(&["auto_approve_ready", "new_key"]))
        );
    }

    #[test]
    fn captured_key_set_digest_reads_the_stamp_or_errors_when_absent() {
        assert_eq!(
            captured_key_set_digest("{\"captured_key_set_digest\": \"abc123\"}"),
            Ok("abc123".to_owned())
        );
        assert!(captured_key_set_digest("{\"manifest\": {}}").is_err());
        assert!(captured_key_set_digest("{not json").is_err());
    }

    #[test]
    fn check_key_set_digest_ignores_core_pin_only_changes() {
        let drive_output = manifest(&["auto_approve_ready", "wip_cap"]);
        let stamped = stamp_manifest(&drive_output).unwrap_or_default();
        assert_eq!(check_key_set_digest(&stamped), Ok(None));

        let old_pin =
            "{\"livespec-orchestrator-beads-fabro\": {\"compat\": {\"pinned\": \"v0.16.0\"}}}";
        let new_pin =
            "{\"livespec-orchestrator-beads-fabro\": {\"compat\": {\"pinned\": \"v0.17.0\"}}}";
        assert!(strip_jsonc_comments(old_pin).contains("v0.16.0"));
        assert!(strip_jsonc_comments(new_pin).contains("v0.17.0"));
        assert_eq!(check_key_set_digest(&stamped), Ok(None));
    }

    #[test]
    fn check_key_set_digest_fails_closed_on_genuine_key_set_change() {
        let stamped = stamp_manifest(&manifest(&["auto_approve_ready", "wip_cap"]))
            .unwrap_or_default()
            .replace("wip_cap", "new_upstream_key");
        let drift = check_key_set_digest(&stamped);
        let mismatch = drift.unwrap_or(None);
        let mismatch = mismatch.unwrap_or(KeySetDigestMismatch {
            expected: String::new(),
            found: String::new(),
        });
        assert_ne!(mismatch.expected(), mismatch.found());
        assert!(
            mismatch.diagnostic().contains("key-set digest")
                && mismatch
                    .diagnostic()
                    .contains("just refresh-config-manifest")
        );
        // A read error propagates.
        assert!(check_key_set_digest("{bad").is_err());
    }

    #[test]
    fn stamp_manifest_inserts_the_key_set_digest_and_rejects_non_objects() {
        let drive_output =
            "{\"kind\": \"config-manifest\", \"manifest\": {\"keys\": [{\"key\": \"wip_cap\"}]}}";
        let stamped = stamp_manifest(drive_output);
        let text = stamped.unwrap_or_default();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        let captured = value
            .get("captured_key_set_digest")
            .and_then(serde_json::Value::as_str);
        assert_eq!(captured.map(str::len), Some(64));
        assert_eq!(
            value.get("kind").and_then(serde_json::Value::as_str),
            Some("config-manifest")
        );
        assert!(stamp_manifest("[1, 2]").is_err());
        assert!(stamp_manifest("{not json").is_err());
    }
}
