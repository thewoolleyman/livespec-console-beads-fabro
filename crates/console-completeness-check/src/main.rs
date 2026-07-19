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
//! The capture is PIN-STAMPED (its `captured_at_pin`) with the orchestrator
//! release it was taken at. Before the lockstep check, the gate FAILS when that
//! stamp differs from `.livespec.jsonc` `compat.pinned` — so the auto-merging
//! pin-bump PR goes RED the moment the pin advances, forcing a
//! `just refresh-config-manifest` (which re-captures the manifest and thereby
//! surfaces any newly-declared key) before the bump can merge.
//!
//! `console-completeness-check --refresh` reads a fresh `config-manifest` output
//! on STDIN, stamps the current `.livespec.jsonc` pin into it, and writes the
//! committed fixture. `just refresh-config-manifest` drives it.

#![forbid(unsafe_code)]

use std::io::Read;
use std::path::Path;
use std::process::ExitCode;

use console_completeness_check::{
    SETTINGS_DOC, check_pin, console_settings_rows, declared_keys, evaluate, read_pinned,
    stamp_manifest,
};

/// The committed capture of the orchestrator's published `config-manifest`,
/// read relative to the repository root (where `just check` runs).
const MANIFEST_FIXTURE: &str = "tests/fixtures/orchestrator-config-manifest.json";

/// The project config carrying the orchestrator pin (`compat.pinned`).
const LIVESPEC_JSONC: &str = ".livespec.jsonc";

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
    let livespec_jsonc = read_file(LIVESPEC_JSONC)?;

    // Staleness gate FIRST: a capture taken at a different pin than the project now
    // pins cannot be trusted to declare the current key set.
    if let Some(mismatch) = check_pin(&livespec_jsonc, &manifest)? {
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

/// Re-capture the fixture: stamp the current `.livespec.jsonc` pin into a fresh
/// `config-manifest` output read from STDIN and write the committed fixture.
fn refresh_fixture() -> Result<ExitCode, String> {
    let livespec_jsonc = read_file(LIVESPEC_JSONC)?;
    let pinned = read_pinned(&livespec_jsonc)?;
    let mut drive_output = String::new();
    std::io::stdin()
        .read_to_string(&mut drive_output)
        .map_err(|error| format!("cannot read config-manifest output from stdin: {error}"))?;
    let stamped = stamp_manifest(&drive_output, &pinned)?;
    std::fs::write(Path::new(MANIFEST_FIXTURE), format!("{stamped}\n"))
        .map_err(|error| format!("cannot write {MANIFEST_FIXTURE}: {error}"))?;
    eprintln!(
        "console-completeness-check: refreshed {MANIFEST_FIXTURE} (captured_at_pin `{pinned}`)"
    );
    Ok(ExitCode::SUCCESS)
}

fn read_file(path: &str) -> Result<String, String> {
    std::fs::read_to_string(Path::new(path)).map_err(|error| format!("cannot read {path}: {error}"))
}
