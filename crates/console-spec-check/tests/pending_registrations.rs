use console_spec_check::{CoverageEntry, NFR_FILE, PendingTestRegistration, SpecSource, evaluate};

#[test]
fn reasoned_todo_registrations_are_reported_as_pending_not_clean() {
    let sources: [SpecSource; 0] = [];
    let operator = vec!["Pending Op".to_string()];
    let nfr = vec!["Pending Nfr".to_string()];
    let registry = vec![
        CoverageEntry {
            scenario: "Pending Op".to_string(),
            scenario_file: "scenarios.md".to_string(),
            tests: vec!["TODO".to_string()],
            reason: "Test tier: a top-of-pyramid acceptance test will cover this scenario."
                .to_string(),
            clauses: Vec::new(),
        },
        CoverageEntry {
            scenario: "Pending Nfr".to_string(),
            scenario_file: NFR_FILE.to_string(),
            tests: vec!["TODO".to_string()],
            reason: "Test tier: integration coverage will land with the implementation slice."
                .to_string(),
            clauses: Vec::new(),
        },
    ];

    let report = evaluate(&sources, &registry, &operator, &nfr);

    assert!(report.untested_scenarios.is_empty());
    assert_eq!(
        report.pending_test_registrations,
        vec![
            PendingTestRegistration {
                scenario_file: "scenarios.md".to_string(),
                scenario: "Pending Op".to_string(),
                test: "TODO".to_string(),
                reason: "Test tier: a top-of-pyramid acceptance test will cover this scenario."
                    .to_string(),
            },
            PendingTestRegistration {
                scenario_file: NFR_FILE.to_string(),
                scenario: "Pending Nfr".to_string(),
                test: "TODO".to_string(),
                reason: "Test tier: integration coverage will land with the implementation slice."
                    .to_string(),
            },
        ]
    );
    assert!(!report.has_blocking_failures());
    assert!(!report.is_clean());
}
