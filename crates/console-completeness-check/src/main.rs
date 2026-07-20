//! `console-completeness-check` — the API-to-Settings-to-help-to-doc lockstep
//! gate. Reads the orchestrator's PUBLISHED `config-manifest` (a committed
//! capture — see `tests/fixtures/orchestrator-config-manifest.json`, refreshed
//! by `just refresh-config-manifest`) and asserts every declared API-configurable
//! key reaches the console's Settings surface, its inline help, and the
//! settings doc. Exits non-zero NAMING any key that fell out of lockstep.
//!
//! Reading a captured copy of the orchestrator's PUBLISHED declared-key surface
//! (never its internals) keeps the check hermetic — `just check`/CI run it
//! offline with no live orchestrator, credential wrapper, or Dolt tenant — while
//! honoring the No-Circular-Dependency Directive.
//!
//! The capture is DIGEST-STAMPED (`captured_key_set_digest`) with the
//! orchestrator's declared key set. Before the lockstep check, the gate FAILS
//! when that stamp differs from the fixture's current declared-key digest. A core
//! pin bump alone does not invalidate the capture; a true key-set change still
//! fails closed until `just refresh-config-manifest` re-captures the live surface.
//!
//! `console-completeness-check --refresh` reads a fresh `config-manifest` output
//! on STDIN, stamps its declared-key digest into it, and writes the committed
//! fixture. `just refresh-config-manifest` drives it.

#![forbid(unsafe_code)]

use std::io::Read;
use std::path::Path;
use std::process::ExitCode;

use console_completeness_check::{
    SETTINGS_DOC, check_key_set_digest, console_settings_rows, declared_keys, evaluate,
    stamp_manifest,
};

/// The committed capture of the orchestrator's published `config-manifest`,
/// read relative to the repository root (where `just check` runs).
const MANIFEST_FIXTURE: &str = "tests/fixtures/orchestrator-config-manifest.json";

fn main() -> ExitCode {
    let refresh = std::env::args().any(|arg| arg == "--refresh");
    let result = if refresh { refresh_fixture() } else { run() };
    match result {
        Ok(code) => code,
        Err(message) => {
            eprintln!("console-completeness-check: {message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let manifest = read_file(MANIFEST_FIXTURE)?;
    let settings_doc = read_file(SETTINGS_DOC)?;
    // Staleness gate FIRST: a fixture whose declared keys no longer match its
    // stamp cannot be trusted as a refreshed capture.
    if let Some(mismatch) = check_key_set_digest(&manifest)? {
        eprintln!("console-completeness-check: {}", mismatch.diagnostic());
        return Ok(ExitCode::FAILURE);
    }

    let declared = declared_keys(&manifest)?;
    let rows = console_settings_rows();
    let report = evaluate(&declared, &rows, &settings_doc);

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
        "console-completeness-check: add the missing Settings row / inline help / settings-doc \
         entry for the named key(s)."
    );
    Ok(ExitCode::FAILURE)
}

/// Re-capture the fixture: stamp the declared-key digest into a fresh
/// `config-manifest` output read from STDIN and write the committed fixture.
fn refresh_fixture() -> Result<ExitCode, String> {
    let mut drive_output = String::new();
    std::io::stdin()
        .read_to_string(&mut drive_output)
        .map_err(|error| format!("cannot read config-manifest output from stdin: {error}"))?;
    let stamped = stamp_manifest(&drive_output)?;
    std::fs::write(Path::new(MANIFEST_FIXTURE), format!("{stamped}\n"))
        .map_err(|error| format!("cannot write {MANIFEST_FIXTURE}: {error}"))?;
    eprintln!("console-completeness-check: refreshed {MANIFEST_FIXTURE} (captured_key_set_digest)");
    Ok(ExitCode::SUCCESS)
}

fn read_file(path: &str) -> Result<String, String> {
    std::fs::read_to_string(Path::new(path)).map_err(|error| format!("cannot read {path}: {error}"))
}
