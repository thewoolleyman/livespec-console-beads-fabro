//! `console-spec-check` — the two spec gates this repository ships.
//!
//! Run with no arguments it is the behavioral-coverage gate (clause ->
//! scenario -> test), per the Behavioral Coverage section of
//! `SPECIFICATION/non-functional-requirements.md`: it reads the SPECIFICATION
//! sources and `tests/heading-coverage.json` from the repository root,
//! evaluates the clause -> scenario -> test chain, reports diagnostics, and
//! exits according to `LIVESPEC_BEHAVIOR_SCENARIO_LINK`.
//!
//! Run with `--fenced-block-staleness` it is the fenced-block staleness gate:
//! it asks git what this working tree changed under `SPECIFICATION/`, runs the
//! [`stale_fenced_blocks_in_changes`] detector over each changed file's before
//! and after, and exits according to `LIVESPEC_SPEC_FENCED_BLOCK_STALENESS`.
//! The two are separate `just check` targets so a failure names which gate
//! failed.
//!
//! ```rust,ignore
//! // Run from the repository root so SPECIFICATION/ and tests/ are visible.
//! std::process::Command::new("console-spec-check").status()?;
//! # Ok::<(), std::io::Error>(())
//! ```
#![forbid(unsafe_code)]

use std::path::Path;
use std::process::{Command, ExitCode};

use console_spec_check::{
    Audience, CoverageReport, FENCED_BLOCK_STALENESS_ENV, Mode, NFR_FILE, OPERATOR_FILES,
    SEVERITY_ENV, SpecChange, SpecSource, StaleFencedBlock, evaluate, is_spec_markdown,
    nfr_scenarios, operator_scenarios, parse_registry, resolve_mode,
    stale_fenced_blocks_in_changes, validate_test_registrations,
};

/// Selects the fenced-block staleness gate instead of the coverage gate.
const STALENESS_FLAG: &str = "--fenced-block-staleness";

/// Overrides the ref the staleness gate compares the working tree against.
/// Unset, it walks the candidates below.
const STALENESS_BASE_ENV: &str = "LIVESPEC_SPEC_STALENESS_BASE";

/// Where the branch started, in preference order. `origin/master` first
/// because that is what a branch is measured against everywhere else in this
/// repository (the Red-Green-Replay range check pins the same ref); plain
/// `master` covers a clone with no remote, which is what the gate's own test
/// repository is.
const STALENESS_BASE_CANDIDATES: [&str; 2] = ["origin/master", "master"];

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
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments.first().map(String::as_str) {
        None => run_coverage(),
        Some(STALENESS_FLAG) if arguments.len() == 1 => run_staleness(),
        Some(other) => Err(format!(
            "unrecognized argument `{other}` (expected no arguments, or `{STALENESS_FLAG}`)"
        )),
    }
}

fn run_coverage() -> Result<ExitCode, String> {
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

    let mut report = evaluate(&sources, &registry, &operator, &nfr);
    report.invalid_test_registrations = validate_test_registrations(&registry, Path::new("."));
    let mode = resolve_mode(std::env::var(SEVERITY_ENV).ok().as_deref());
    emit(&report, mode);

    if !report.has_blocking_failures() || mode == Mode::Warn {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::FAILURE)
    }
}

// ---------------------------------------------------------------------------
// The fenced-block staleness gate.
// ---------------------------------------------------------------------------

fn run_staleness() -> Result<ExitCode, String> {
    if !Path::new("SPECIFICATION").is_dir() {
        // No spec tree here — nothing to check, exactly as the coverage lane
        // decides.
        return Ok(ExitCode::SUCCESS);
    }
    let Some(base) = resolve_base()? else {
        // A checkout with neither `origin/master` nor `master` has no "before"
        // to read, so there is no change set and nothing to assert. Say so
        // rather than reporting a silent clean run, because a gate that cannot
        // see the change is not the same as a change with no findings.
        eprintln!(
            "console-spec-check: fenced-block staleness skipped — no comparison base \
             ({}; override with {STALENESS_BASE_ENV})",
            STALENESS_BASE_CANDIDATES.join(", ")
        );
        return Ok(ExitCode::SUCCESS);
    };

    let changes = collect_changes(&base)?;
    let findings = stale_fenced_blocks_in_changes(&changes);
    let mode = resolve_mode(std::env::var(FENCED_BLOCK_STALENESS_ENV).ok().as_deref());
    emit_staleness(&findings, changes.len(), &base, mode);

    if findings.is_empty() || mode == Mode::Warn {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::FAILURE)
    }
}

/// The commit the working tree is measured against: the merge base of the
/// first resolvable candidate ref and `HEAD`, so a branch is compared with
/// where it left master rather than with wherever master has since moved.
fn resolve_base() -> Result<Option<String>, String> {
    let candidates: Vec<String> = match std::env::var(STALENESS_BASE_ENV) {
        Ok(value) if !value.trim().is_empty() => vec![value.trim().to_owned()],
        _ => STALENESS_BASE_CANDIDATES
            .iter()
            .map(|candidate| (*candidate).to_owned())
            .collect(),
    };
    for candidate in candidates {
        let Some(commit) = git(&["rev-parse", "--verify", "--quiet", &candidate])? else {
            continue;
        };
        let commit = commit.trim().to_owned();
        let merge_base = git(&["merge-base", &commit, "HEAD"])?;
        return Ok(Some(
            merge_base.map_or(commit, |base| base.trim().to_owned()),
        ));
    }
    Ok(None)
}

/// Every spec file this working tree MODIFIED since `base`, with both sides.
///
/// `git diff <base>` reads the working tree, so an uncommitted edit and a
/// commit already on the branch are both change-set entries — the gate does
/// not wait for a commit to see a stale block. Added and deleted files are
/// filtered out because a one-sided change has no removed prose.
fn collect_changes(base: &str) -> Result<Vec<SpecChange>, String> {
    let listing = git(&[
        "diff",
        "--name-only",
        "--diff-filter=M",
        base,
        "--",
        "SPECIFICATION",
    ])?
    .ok_or_else(|| format!("failed to diff the working tree against {base}"))?;

    let mut changes = Vec::new();
    for path in listing
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if !is_spec_markdown(path) {
            continue;
        }
        let Some(previous) = git(&["show", &format!("{base}:{path}")])? else {
            continue;
        };
        let current = std::fs::read_to_string(path)
            .map_err(|error| format!("failed to read {path}: {error}"))?;
        changes.push(SpecChange {
            spec_file: path.to_owned(),
            previous,
            current,
        });
    }
    Ok(changes)
}

/// Run git and return its stdout, or `None` when git itself reports failure —
/// an unresolvable ref, a path absent at the base. A git that cannot be RUN is
/// an error, not an absence: this gate is not silently skippable.
fn git(arguments: &[&str]) -> Result<Option<String>, String> {
    let output = Command::new("git")
        .args(arguments)
        .output()
        .map_err(|error| format!("failed to run `git {}`: {error}", arguments.join(" ")))?;
    if !output.status.success() {
        return Ok(None);
    }
    String::from_utf8(output.stdout).map(Some).map_err(|error| {
        format!(
            "`git {}` produced non-UTF-8 output: {error}",
            arguments.join(" ")
        )
    })
}

fn emit_staleness(findings: &[StaleFencedBlock], changed: usize, base: &str, mode: Mode) {
    let label = match mode {
        Mode::Warn => "warn",
        Mode::Fail => "error",
    };
    for finding in findings {
        eprintln!("{}", finding.render(label));
    }
    if findings.is_empty() {
        eprintln!(
            "console-spec-check: fenced-block staleness clean (0 stale blocks across {changed} \
             changed spec file(s) since {base})"
        );
    } else {
        // One finding per (block, term) pair, so the two counts differ: a block
        // that contradicts three removed sentences is one block to go and fix.
        let mut blocks: Vec<(&str, usize, usize)> = findings
            .iter()
            .map(|finding| {
                (
                    finding.spec_file.as_str(),
                    finding.start_line,
                    finding.end_line,
                )
            })
            .collect();
        blocks.sort_unstable();
        blocks.dedup();
        eprintln!(
            "{label}: fenced-block staleness: {} finding(s) across {} fenced block(s) in {changed} \
             changed spec file(s) since {base}. Amend the block alongside the prose, or state the \
             term again in the prose if it was not meant to be removed (lever \
             {FENCED_BLOCK_STALENESS_ENV}; default `fail`, set to `warn` to report only)",
            findings.len(),
            blocks.len()
        );
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
    for registration in &report.pending_test_registrations {
        eprintln!(
            "pending: scenario has explicit pending test registration [{}] {} :: {} ({})",
            registration.scenario_file,
            registration.scenario,
            registration.test,
            registration.reason
        );
    }
    for registration in &report.invalid_test_registrations {
        match &registration.function {
            Some(function) => eprintln!(
                "{label}: invalid registered test [{}] {} :: {} -> {}::{} ({})",
                registration.scenario_file,
                registration.scenario,
                registration.test,
                registration.file,
                function,
                registration.reason
            ),
            None => eprintln!(
                "{label}: invalid registered test [{}] {} :: {} -> {} ({})",
                registration.scenario_file,
                registration.scenario,
                registration.test,
                registration.file,
                registration.reason
            ),
        }
    }
    let unlinked = report.unlinked_clauses.len();
    let untested = report.untested_scenarios.len();
    let pending = report.pending_test_registrations.len();
    let invalid = report.invalid_test_registrations.len();
    if unlinked == 0 && untested == 0 && pending == 0 && invalid == 0 {
        eprintln!(
            "console-spec-check: behavioral coverage clean (0 unlinked, 0 untested, 0 invalid test registrations)"
        );
    } else {
        let summary_label = if report.has_blocking_failures() {
            label
        } else {
            "pending"
        };
        eprintln!(
            "{summary_label}: behavioral-coverage: {unlinked} unlinked clause(s), {untested} untested \
             scenario(s), {pending} pending test registration(s), {invalid} invalid test \
             registration(s) (lever {SEVERITY_ENV}; default `fail`, set to `warn` to report only)"
        );
    }
}
