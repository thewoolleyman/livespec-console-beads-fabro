//! `console-ci-parity-check` — gate `just check` target names against CI jobs.

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

use console_ci_parity_check::{
    CI_WORKFLOW_PATH, FIXTURE_PATH, JUSTFILE_PATH, ParityInput, check_parity, parse_ci_jobs,
    parse_fixture, parse_just_check_targets, parse_just_recipes,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    justfile: PathBuf,
    ci: PathBuf,
    fixture: PathBuf,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            justfile: PathBuf::from(JUSTFILE_PATH),
            ci: PathBuf::from(CI_WORKFLOW_PATH),
            fixture: PathBuf::from(FIXTURE_PATH),
        }
    }
}

fn main() -> ExitCode {
    match run(&parse_args(std::env::args().skip(1))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Args {
    let mut parsed = Args::default();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--justfile" => {
                if let Some(path) = iter.next() {
                    parsed.justfile = PathBuf::from(path);
                }
            }
            "--ci" => {
                if let Some(path) = iter.next() {
                    parsed.ci = PathBuf::from(path);
                }
            }
            "--fixture" => {
                if let Some(path) = iter.next() {
                    parsed.fixture = PathBuf::from(path);
                }
            }
            _ => {}
        }
    }
    parsed
}

fn run(args: &Args) -> Result<(), String> {
    let justfile = std::fs::read_to_string(&args.justfile)
        .map_err(|err| format!("cannot read {}: {err}", args.justfile.display()))?;
    let ci = std::fs::read_to_string(&args.ci)
        .map_err(|err| format!("cannot read {}: {err}", args.ci.display()))?;
    let fixture = std::fs::read_to_string(&args.fixture)
        .map_err(|err| format!("cannot read {}: {err}", args.fixture.display()))?;

    let input = ParityInput {
        just_targets: parse_just_check_targets(&justfile)?,
        just_recipes: parse_just_recipes(&justfile),
        ci_jobs: parse_ci_jobs(&ci)?,
        fixture_entries: parse_fixture(&fixture)?,
    };
    let findings = check_parity(&input);
    if findings.is_empty() {
        println!(
            "console-ci-parity-check: {} `just check` target(s) and {} CI job(s) accounted for",
            input.just_targets.len(),
            input.ci_jobs.len()
        );
        return Ok(());
    }

    let mut report =
        String::from("console-ci-parity-check: `just check` targets and CI jobs diverged\n");
    for finding in &findings {
        report.push_str("  - ");
        report.push_str(&finding.render());
        report.push('\n');
    }
    report.push_str(
        "\nThis is intentionally bidirectional. See livespec-console-beads-fabro-3r6 \
         for the one-way drift blind spot this gate must not copy.\n",
    );
    Err(report)
}
