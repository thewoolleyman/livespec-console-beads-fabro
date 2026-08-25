use console_application::source_adapters::AcceptancePolicy;
use console_application::{DispatcherSettingRow, DispatcherSettings, dispatcher_setting_rows};

#[test]
fn dispatcher_settings_surface_includes_drift_capture_merge_threshold_row() {
    let settings = DispatcherSettings::new(true, false, AcceptancePolicy::AiOnly, 4, 2, 5);
    let rows = dispatcher_setting_rows(&settings);

    let rendered = rows
        .iter()
        .map(|row| (row.label(), row.value().to_owned(), row.dangerous()))
        .collect::<Vec<_>>();
    assert_eq!(
        rendered,
        [
            ("Auto-approve ready", "on".to_owned(), true),
            ("Merge on review cap", "off".to_owned(), true),
            ("Acceptance mode", "ai-only".to_owned(), true),
            ("Review fix cap", "4".to_owned(), false),
            ("Acceptance rework cap", "2".to_owned(), false),
            ("Drift capture merge threshold", "1".to_owned(), false,),
            ("WIP cap", "5".to_owned(), false),
        ]
    );

    let keys = DispatcherSettingRow::all()
        .iter()
        .map(DispatcherSettingRow::orchestrator_key)
        .collect::<Vec<_>>();
    assert_eq!(
        keys,
        [
            "auto_approve_ready",
            "merge_on_review_cap",
            "acceptance_mode",
            "review_fix_cap",
            "acceptance_rework_cap",
            "drift_capture_merge_threshold",
            "wip_cap",
        ]
    );

    assert!(rows.iter().any(|row| {
        row.label() == "Drift capture merge threshold"
            && row.help().contains("merge-only drift detections")
            && row.help().contains("Enter/Space increments")
    }));
}
