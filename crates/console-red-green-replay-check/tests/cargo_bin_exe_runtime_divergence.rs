//! Regression test for livespec-console-beads-fabro-pzbdbo.36.
//!
//! Reproduces the exact historical divergence — an integration test that
//! resolves `CARGO_BIN_EXE_*` via a RUNTIME-only `std::env::var_os` read,
//! with no compile-time `option_env!` capture — against the standalone
//! fixture crate under `tests/fixtures/cargo-bin-exe-divergence/`, and proves
//! it with REAL `cargo` / `cargo nextest` invocations rather than assertions
//! about what they would do:
//!
//! 1. The fixture genuinely is a case whose outcome differs between
//!    runners: plain `cargo test` FAILS it, `cargo nextest run` PASSES it.
//! 2. `ProcessRunner` — what the commit-msg hook actually calls — agrees
//!    with nextest/CI (step 1's second half), not with plain `cargo test`.
//!    Before this fix, `ProcessRunner` shelled to plain `cargo test` and this
//!    assertion would have failed exactly like step 1's first half.
//!
//! Deliberately NOT `#[ignore]`d: livespec-console-beads-fabro-pzbdbo.36
//! requires this be caught by the gate that runs on push, not only when
//! someone remembers to pass `--ignored`. The fixture is a trivial,
//! dependency-free crate, so the extra two nested builds cost a couple of
//! seconds, not minutes.

use std::path::PathBuf;
use std::process::Command;

use console_red_green_replay_check::{ProcessRunner, Runner, TestScope};

const FIXTURE_PACKAGE: &str = "cargo-bin-exe-divergence-fixture";
const FIXTURE_TARGET: &str = "runtime_only";

fn fixture_manifest() -> Result<PathBuf, String> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cargo-bin-exe-divergence/Cargo.toml");
    if manifest.is_file() {
        Ok(manifest)
    } else {
        Err(format!(
            "fixture manifest missing at {}",
            manifest.display()
        ))
    }
}

#[test]
fn the_fixture_reproduces_the_historical_runner_divergence() -> Result<(), String> {
    let manifest = fixture_manifest()?;

    // 1a. Plain `cargo test` FAILS the fixture.
    let cargo_test = Command::new("cargo")
        .args(["test", "--manifest-path"])
        .arg(&manifest)
        .output()
        .map_err(|err| format!("spawn cargo test over the fixture: {err}"))?;
    assert!(
        !cargo_test.status.success(),
        "expected `cargo test` to FAIL the {FIXTURE_PACKAGE}::{FIXTURE_TARGET} fixture (it \
         reads CARGO_BIN_EXE_* at runtime with no compile-time option_env! capture), but it \
         PASSED — the fixture no longer reproduces the historical divergence; stdout:\n{}\n\
         stderr:\n{}",
        String::from_utf8_lossy(&cargo_test.stdout),
        String::from_utf8_lossy(&cargo_test.stderr)
    );

    // 1b. `cargo nextest run` PASSES the same fixture — this is what CI and
    //     `just check` run.
    let nextest = Command::new("cargo")
        .args(["nextest", "run", "--manifest-path"])
        .arg(&manifest)
        .output()
        .map_err(|err| format!("spawn cargo nextest run over the fixture: {err}"))?;
    assert!(
        nextest.status.success(),
        "expected `cargo nextest run` to PASS the {FIXTURE_PACKAGE}::{FIXTURE_TARGET} fixture \
         (nextest exports CARGO_BIN_EXE_* to the running test process), but it FAILED — this \
         fixture is meant to demonstrate a divergence, not a universal failure; stdout:\n{}\n\
         stderr:\n{}",
        String::from_utf8_lossy(&nextest.stdout),
        String::from_utf8_lossy(&nextest.stderr)
    );

    // 2. The production Runner — what the commit-msg hook actually calls —
    //    MUST agree with nextest/CI (step 1b), not with plain `cargo test`
    //    (step 1a).
    let fixture_dir = manifest
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", manifest.display()))?;
    let runner = ProcessRunner::with_workdir(fixture_dir);
    let result = runner
        .cargo_test(TestScope::Integration {
            package: FIXTURE_PACKAGE.to_owned(),
            target: FIXTURE_TARGET.to_owned(),
        })
        .map_err(|err| format!("ProcessRunner::cargo_test over the fixture: {err}"))?;
    assert_eq!(
        result.code, 0,
        "commit-msg-hook runner DISAGREES with CI/just-check over \
         {FIXTURE_PACKAGE}::{FIXTURE_TARGET}: cargo nextest run (CI, just check) PASSED this \
         test in step 1b above, but ProcessRunner (the commit-msg hook's runner) exited {} — \
         stdout:\n{}\nstderr:\n{}",
        result.code, result.stdout, result.stderr
    );
    Ok(())
}
