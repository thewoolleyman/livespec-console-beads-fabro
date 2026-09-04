//! `console-upstream-dep-check` — scaffold entry point; refuses until the gate lands.

#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("console-upstream-dep-check: gate not implemented yet");
    ExitCode::FAILURE
}
