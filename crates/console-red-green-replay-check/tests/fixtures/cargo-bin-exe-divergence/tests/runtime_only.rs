//! The historical bug, reproduced deliberately: resolves
//! `CARGO_BIN_EXE_cargo-bin-exe-divergence-fixture` via a RUNTIME-only
//! `std::env::var_os` read, with no compile-time `option_env!` capture.
//!
//! Cargo defines `CARGO_BIN_EXE_<bin>` for an integration test only at
//! COMPILE time. `cargo nextest run` additionally exports it to the running
//! test PROCESS; plain `cargo test` does not. So this test PASSES under
//! nextest and FAILS under `cargo test` — the same shape that let a broken
//! commit hook block every commit in livespec-console-beads-fabro while CI
//! (which runs nextest) stayed green. See
//! `crates/console-red-green-replay-check/tests/cargo_bin_exe_runtime_divergence.rs`,
//! which runs both real toolchains against this fixture to prove it, and
//! `console-arch-check`'s `check_cargo_bin_exe_compile_time_resolution`,
//! which flags this exact pattern (a runtime-only read with no `option_env!`
//! in the same file) so a future occurrence is caught on push regardless of
//! which test runner is in play. DO NOT "fix" this file to use `option_env!`
//! — that would defeat the fixture's purpose.
#[test]
fn resolves_bin_exe_path() {
    let value = std::env::var_os("CARGO_BIN_EXE_cargo-bin-exe-divergence-fixture");
    assert!(
        value.is_some(),
        "CARGO_BIN_EXE_cargo-bin-exe-divergence-fixture was not set in the running test \
         process — expected under `cargo test` (compile-time-only variable, not exported \
         to the runtime env), NOT expected under `cargo nextest run`"
    );
}
