//! `console-completeness-check` — the API-to-Settings-to-help-to-doc lockstep
//! gate. Reads the orchestrator's PUBLISHED `config-manifest` (a committed
//! capture — see `tests/fixtures/orchestrator-config-manifest.json`, refreshed
//! by `just refresh-config-manifest`) and asserts every declared API-configurable
//! key reaches the console's Settings surface, its inline help, and the README
//! settings doc. Exits non-zero NAMING any key that fell out of lockstep.
//!
//! Reading a captured copy of the orchestrator's PUBLISHED declared-key surface
//! (never its internals) keeps the check hermetic — `just check`/CI run it
//! offline with no live orchestrator, credential wrapper, or Dolt tenant — while
//! honoring the No-Circular-Dependency Directive. The capture is refreshed from
//! the live `drive --action config-manifest` when the orchestrator's key set
//! changes (part of the orchestrator pin bump).

#![forbid(unsafe_code)]

use std::path::Path;
use std::process::ExitCode;

use console_completeness_check::{console_settings_rows, declared_keys, evaluate};

/// The committed capture of the orchestrator's published `config-manifest`,
/// read relative to the repository root (where `just check` runs).
const MANIFEST_FIXTURE: &str = "tests/fixtures/orchestrator-config-manifest.json";

/// The console's settings doc — the repo `README.md` (there is no `docs/` dir;
/// the README IS the settings doc).
const README: &str = "README.md";

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("console-completeness-check: {message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let manifest = std::fs::read_to_string(Path::new(MANIFEST_FIXTURE))
        .map_err(|error| format!("cannot read {MANIFEST_FIXTURE}: {error}"))?;
    let readme = std::fs::read_to_string(Path::new(README))
        .map_err(|error| format!("cannot read {README}: {error}"))?;

    let declared = declared_keys(&manifest)?;
    let rows = console_settings_rows();
    let report = evaluate(&declared, &rows, &readme);

    if report.is_clean() {
        eprintln!(
            "console-completeness-check: API-to-Settings-to-help-to-doc lockstep clean ({} declared keys)",
            declared.len()
        );
        return Ok(ExitCode::SUCCESS);
    }
    for line in report.diagnostics() {
        eprintln!("console-completeness-check: {line}");
    }
    eprintln!(
        "console-completeness-check: after an orchestrator key change, refresh the capture with \
         `just refresh-config-manifest`, then add the missing Settings row / inline help / README \
         settings-doc entry for the named key(s)."
    );
    Ok(ExitCode::FAILURE)
}
