//! `console-red-green-replay-check` — standalone Red-Green-Replay checker.

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

use console_red_green_replay_check::{ProcessRunner, check_commit_msg, validate_default_range};

fn main() -> ExitCode {
    // `ProcessRunner` runs the suite under `cargo nextest run` — the same
    // runner `just check`'s `check-nextest` recipe uses — so this hook and CI
    // cannot silently disagree about whether a test passes. See
    // livespec-console-beads-fabro-pzbdbo.36 and the doc comment on
    // `ProcessRunner` in lib.rs.
    let runner = ProcessRunner::new();
    let result = std::env::args_os().nth(1).map_or_else(
        || validate_default_range(&runner),
        |path| check_commit_msg(&runner, &PathBuf::from(path)),
    );

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
