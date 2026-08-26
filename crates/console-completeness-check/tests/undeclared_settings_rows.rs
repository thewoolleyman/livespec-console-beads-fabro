use console_completeness_check::{SettingsRow, evaluate};

fn settings_doc_with_section(keys: &[&str]) -> String {
    format!(
        "## Other\n\n{}\n\n## Dispatcher settings\n\n{}\n\n## Next\n",
        keys.join("\n"),
        keys.iter()
            .map(|key| format!("| `{key}` | documented |"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

#[test]
fn evaluate_names_a_settings_row_absent_from_the_declared_key_set() {
    let declared = vec!["auto_approve_ready".to_owned()];
    let rows = vec![
        SettingsRow::new("auto_approve_ready".to_owned(), "help".to_owned()),
        SettingsRow::new("undeclared_console_row".to_owned(), "help".to_owned()),
    ];
    let settings_doc = settings_doc_with_section(&["auto_approve_ready"]);
    let report = evaluate(&declared, &rows, &settings_doc);
    assert!(!report.is_clean());
    assert!(report.diagnostics().iter().any(|line| {
        line.contains("undeclared_console_row")
            && line.contains("not declared by the config-manifest")
    }));
}
