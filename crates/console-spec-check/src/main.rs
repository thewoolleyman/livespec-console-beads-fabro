//! `console-spec-check` — the behavioral-coverage gate (clause -> scenario ->
//! test), per the Behavioral Coverage section of
//! `SPECIFICATION/non-functional-requirements.md`.
//!
//! The binary reads the SPECIFICATION sources and `tests/heading-coverage.json`
//! from the repository root, evaluates the clause -> scenario -> test chain,
//! reports diagnostics, and exits according to `LIVESPEC_BEHAVIOR_SCENARIO_LINK`.
//!
//! ```rust,ignore
//! // Run from the repository root so SPECIFICATION/ and tests/ are visible.
//! std::process::Command::new("console-spec-check").status()?;
//! # Ok::<(), std::io::Error>(())
//! ```
#![forbid(unsafe_code)]

use std::path::Path;
use std::process::ExitCode;

use console_spec_check::{
    Audience, CoverageReport, Mode, NFR_FILE, OPERATOR_FILES, SEVERITY_ENV, SpecSource, evaluate,
    nfr_scenarios, operator_scenarios, parse_registry, resolve_mode,
};

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("console-spec-check: {message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let spec_root = Path::new("SPECIFICATION");
    if !spec_root.is_dir() {
        // No spec tree here — nothing to check (parity with the Python guard).
        return Ok(ExitCode::SUCCESS);
    }

    // The clause-bearing sources, partitioned by audience: operator-facing
    // clauses (spec/contracts/constraints) bind to `scenarios.md`; this
    // document's own contributor-facing clauses bind to the NFR `## Scenarios`.
    let mut owned: Vec<(String, Audience, String)> = Vec::new();
    for name in OPERATOR_FILES {
        if let Some(text) = read_optional(&spec_root.join(name))? {
            owned.push((name.to_string(), Audience::Operator, text));
        }
    }
    let nfr_text = read_optional(&spec_root.join(NFR_FILE))?;
    if let Some(text) = &nfr_text {
        owned.push((NFR_FILE.to_string(), Audience::Contributor, text.clone()));
    }
    let sources: Vec<SpecSource> = owned
        .iter()
        .map(|(file, audience, text)| SpecSource {
            spec_file: file.as_str(),
            content: text.as_str(),
            audience: *audience,
        })
        .collect();

    // Live scenario sections.
    let scenarios_text = read_optional(&spec_root.join("scenarios.md"))?.unwrap_or_default();
    let operator = operator_scenarios(&scenarios_text);
    let nfr = nfr_text.as_deref().map(nfr_scenarios).unwrap_or_default();

    // The link registry (absent -> empty).
    let registry_text = read_optional(Path::new("tests/heading-coverage.json"))?;
    let registry = match &registry_text {
        Some(json) => parse_registry(json)?,
        None => Vec::new(),
    };

    let report = evaluate(&sources, &registry, &operator, &nfr);
    let mode = resolve_mode(std::env::var(SEVERITY_ENV).ok().as_deref());
    emit(&report, mode);

    if report.is_clean() || mode == Mode::Warn {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::FAILURE)
    }
}

fn read_optional(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("failed to read {}: {error}", path.display())),
    }
}

fn emit(report: &CoverageReport, mode: Mode) {
    let label = match mode {
        Mode::Warn => "warn",
        Mode::Fail => "error",
    };
    for clause in &report.unlinked_clauses {
        eprintln!(
            "{label}: clause not linked to a scenario [{}] {} > {} :: {}",
            clause.gap_id, clause.spec_file, clause.heading_path, clause.clause
        );
    }
    for scenario in &report.untested_scenarios {
        eprintln!(
            "{label}: scenario has no registered test [{}] {}",
            scenario.scenario_file, scenario.scenario
        );
    }
    let unlinked = report.unlinked_clauses.len();
    let untested = report.untested_scenarios.len();
    if unlinked == 0 && untested == 0 {
        eprintln!("console-spec-check: behavioral coverage clean (0 unlinked, 0 untested)");
    } else {
        eprintln!(
            "{label}: behavioral-coverage: {unlinked} unlinked clause(s), {untested} untested \
             scenario(s) (lever {SEVERITY_ENV}; default `warn`, set to `fail` to enforce)"
        );
    }
}
