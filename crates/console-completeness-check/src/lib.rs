//! Settings-surface completeness primitives — the API-to-Settings-to-help-to-doc
//! lockstep gate, per `SPECIFICATION/contracts.md` "Settings-surface
//! completeness" and `scenarios.md` "Settings surface stays in lockstep with the
//! orchestrator's declared keys".
//!
//! Every key the orchestrator declares as API-configurable (its published
//! `config-manifest`) MUST appear, in lockstep, in three console places: a
//! Settings-surface row, that row's inline help, and the console's settings doc
//! (the `README.md`). This crate is the CONSUMER-side check (No-Circular-Dependency
//! Directive): it reads the orchestrator's PUBLISHED declared-key surface and
//! compares it against the console's own surfaces; nothing here reads orchestrator
//! internals, and the declared-key list is read from the manifest, never hardcoded.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use console_application::DispatcherSettingRow;

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
    /// Declared keys absent from the README settings doc.
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
                "declared key `{key}` is not documented in the README settings doc"
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

/// Evaluate the API-to-Settings-to-help-to-doc lockstep.
///
/// For EACH declared key, require a console Settings row, non-empty inline help
/// on that row, and a mention in the README settings doc. A key missing any
/// surface is named in the returned report.
#[must_use]
pub fn evaluate(declared: &[String], rows: &[SettingsRow], readme: &str) -> CompletenessReport {
    let mut report = CompletenessReport::default();
    for key in declared {
        match rows.iter().find(|row| row.key() == key) {
            None => report.missing_settings_row.push(key.clone()),
            Some(row) if row.help().trim().is_empty() => report.missing_help.push(key.clone()),
            Some(_row) => {}
        }
        if !readme.contains(key.as_str()) {
            report.missing_doc.push(key.clone());
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::{CompletenessReport, SettingsRow, console_settings_rows, declared_keys, evaluate};

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
        // The real console surface is itself in lockstep: every row has a key and
        // non-empty help.
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
        // Not JSON.
        assert!(declared_keys("{not json").is_err());
        // No manifest.keys array.
        assert!(declared_keys("{\"manifest\": {}}").is_err());
        // A key entry without a string `key`.
        assert!(declared_keys("{\"manifest\": {\"keys\": [{\"nope\": 1}]}}").is_err());
        // An empty key list is a degenerate manifest and is refused.
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
        let readme = "## Dispatcher settings\n`auto_approve_ready` ... `wip_cap` ...";
        let report = evaluate(&declared, &rows, readme);
        assert!(report.is_clean());
        assert!(report.diagnostics().is_empty());
    }

    #[test]
    fn evaluate_names_a_key_missing_from_the_settings_surface() {
        // The check reads the declared list from the manifest (not hardcoded): a
        // declared key with no Settings row is named as missing.
        let declared = vec![
            "auto_approve_ready".to_owned(),
            "new_upstream_key".to_owned(),
        ];
        let rows = vec![row("auto_approve_ready", "help")];
        let readme = "`auto_approve_ready` `new_upstream_key`";
        let report = evaluate(&declared, &rows, readme);
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
        let readme = "`wip_cap`";
        let report = evaluate(&declared, &rows, readme);
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
    fn evaluate_names_a_key_missing_from_the_readme_settings_doc() {
        let declared = vec!["wip_cap".to_owned()];
        let rows = vec![row("wip_cap", "help")];
        let readme = "## Dispatcher settings — no keys documented here";
        let report = evaluate(&declared, &rows, readme);
        assert_eq!(report.missing_doc(), ["wip_cap".to_owned()]);
        assert!(report.missing_settings_row().is_empty());
        assert!(report.missing_help().is_empty());
        assert!(
            report
                .diagnostics()
                .iter()
                .any(|line| line.contains("wip_cap") && line.contains("README settings doc"))
        );
    }

    #[test]
    fn completeness_report_default_is_clean() {
        let report = CompletenessReport::default();
        assert!(report.is_clean());
    }
}
