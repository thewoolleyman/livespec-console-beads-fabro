//! `console-red-green-replay-check` — standalone Red-Green-Replay checker.

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::Command;
use std::process::ExitCode;

use console_red_green_replay_check::{
    CommandOutput, Runner, TestScope, check_commit_msg, validate_default_range,
};

struct ProcessRunner;

impl Runner for ProcessRunner {
    fn git(&self, args: &[&str]) -> Result<CommandOutput, String> {
        let output = Command::new("git")
            .args(args)
            .output()
            .map_err(|err| format!("git {}: {err}", args.join(" ")))?;
        Ok(CommandOutput {
            code: output.status.code().unwrap_or(1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    fn cargo_test(&self, scope: TestScope) -> Result<CommandOutput, String> {
        let mut command = Command::new("cargo");
        command.args(["test", "--workspace", "--all-features"]);
        if let TestScope::Integration { package, target } = scope {
            command.args(["--package", &package, "--test", &target]);
        }
        let output = command
            .output()
            .map_err(|err| format!("cargo test: {err}"))?;
        Ok(CommandOutput {
            code: output.status.code().unwrap_or(1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

fn main() -> ExitCode {
    let runner = ProcessRunner;
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
