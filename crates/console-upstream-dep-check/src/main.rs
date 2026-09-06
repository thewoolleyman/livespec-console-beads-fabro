//! `console-upstream-dep-check [<ledger.json>] [--upstream <orch.json> --now
//! <YYYY-MM-DD>]` — reads the `bd list --status all --json -n 0` array from the
//! path (or stdin when absent) and refuses on any finding. `--upstream` and
//! `--now` add the cross-tenant lifecycle rules D, E, U and the never-refusing
//! stale-upstream warning. The `gate-upstream-deps` recipe supplies both
//! ledgers under ONE credential-wrapper invocation and fails closed when it
//! cannot read either.

#![forbid(unsafe_code)]

use std::io::Read as _;
use std::process::ExitCode;

use console_upstream_dep_check::{Upstream, parse_args, run};

fn main() -> ExitCode {
    let argv = std::env::args().skip(1).collect::<Vec<_>>();
    match report(&argv) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(message) => {
            eprintln!("console-upstream-dep-check: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Run the gate, printing everything it found. `Ok(false)` is a refusal;
/// warnings print but never change the answer.
fn report(argv: &[String]) -> Result<bool, String> {
    let parsed = parse_args(argv)?;
    let text = read_input(parsed.ledger.as_deref())?;
    let upstream = match (&parsed.upstream, &parsed.now) {
        (Some(path), Some(now)) => Some(Upstream::parse(&read_file(path)?, now)?),
        _ => None,
    };
    let report = run(&text, upstream.as_ref())?;
    for warning in &report.warnings {
        eprintln!(
            "console-upstream-dep-check: {}: {warning}",
            warning.warning_mode()
        );
    }
    if report.findings.is_empty() {
        println!(
            "console-upstream-dep-check: {} item(s) scanned; {} warning(s); no findings",
            report.scanned,
            report.warnings.len()
        );
        return Ok(true);
    }
    for finding in &report.findings {
        eprintln!(
            "console-upstream-dep-check: {}: {finding}",
            finding.failure_mode()
        );
    }
    eprintln!(
        "console-upstream-dep-check: {} finding(s) over {} item(s) — refusing",
        report.findings.len(),
        report.scanned
    );
    Ok(false)
}

fn read_input(path: Option<&str>) -> Result<String, String> {
    path.map_or_else(read_stdin, read_file)
}

fn read_file(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|err| format!("cannot read {path}: {err}"))
}

fn read_stdin() -> Result<String, String> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|err| format!("cannot read stdin: {err}"))?;
    Ok(text)
}
