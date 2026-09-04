//! `console-upstream-dep-check [<ledger.json>]` — reads the `bd list --status
//! all --json -n 0` array from the path (or stdin when absent) and refuses on
//! any finding. The `gate-upstream-deps` recipe supplies the ledger under the
//! credential wrapper and fails closed when it cannot.

#![forbid(unsafe_code)]

use std::io::Read as _;
use std::process::ExitCode;

fn main() -> ExitCode {
    let path = std::env::args().nth(1);
    let Ok(text) = read_input(path.as_deref()).inspect_err(|message| {
        eprintln!("console-upstream-dep-check: {message}");
    }) else {
        return ExitCode::FAILURE;
    };
    match console_upstream_dep_check::run(&text) {
        Ok(report) if report.findings.is_empty() => {
            println!(
                "console-upstream-dep-check: {} item(s) scanned; no findings",
                report.scanned
            );
            ExitCode::SUCCESS
        }
        Ok(report) => {
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
            ExitCode::FAILURE
        }
        Err(message) => {
            eprintln!("console-upstream-dep-check: {message}");
            ExitCode::FAILURE
        }
    }
}

fn read_input(path: Option<&str>) -> Result<String, String> {
    path.map_or_else(read_stdin, |path| {
        std::fs::read_to_string(path).map_err(|err| format!("cannot read {path}: {err}"))
    })
}

fn read_stdin() -> Result<String, String> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|err| format!("cannot read stdin: {err}"))?;
    Ok(text)
}
