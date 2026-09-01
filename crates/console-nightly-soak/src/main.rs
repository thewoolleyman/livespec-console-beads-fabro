//! Binary composition root for the nightly soak driver.
//!
//! Reads a JSON findings file (path from argv[1]) produced by the
//! `just nightly-soak` recipe, wires the production `BeadsLedger` (which
//! shells out to `bd list` and the orchestrator capture surface), and
//! processes each finding through [`NightlySoakFiler`]. Always exits 0:
//! findings never fail master.
//!
//! Run under the 1Password environment wrapper so `BEADS_DOLT_PASSWORD` is
//! injected; see `AGENTS.md` for the Beads runtime prerequisites.

#![forbid(unsafe_code)]

use std::io::Read;
use std::process::{Command, ExitCode};

use console_nightly_soak::{
    BeadsDouble, FilingResult, FindingKind, LedgerChore, LedgerError, LedgerPort, NightlySoakFiler,
};

// ---------------------------------------------------------------------------
// Production ledger implementation (shells out to bd + orchestrator CLI)
// ---------------------------------------------------------------------------

const BD_PATH: &str = "/usr/local/bin/bd";

struct BeadsLedger {
    bd: String,
    capture_cli: String,
}

impl BeadsLedger {
    fn new() -> Self {
        let bd = std::env::var("LIVESPEC_BD_PATH").unwrap_or_else(|_| BD_PATH.to_owned());
        let capture_cli = std::env::var("LIVESPEC_ORCHESTRATOR_DRIVE")
            .unwrap_or_else(|_| "livespec-orchestrator-drive".to_owned());
        Self { bd, capture_cli }
    }
}

impl LedgerPort for BeadsLedger {
    fn list_open_chores(&self) -> Result<Vec<LedgerChore>, LedgerError> {
        let output = Command::new(&self.bd)
            .args(["list", "--status", "all", "--json", "-n", "0"])
            .output()
            .map_err(|err| LedgerError(format!("bd list failed to start: {err}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(LedgerError(format!("bd list exited non-zero: {stderr}")));
        }
        let json_str = String::from_utf8_lossy(&output.stdout);
        let items: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|err| LedgerError(format!("bd list JSON parse failed: {err}")))?;
        let chores = items
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter(|item| {
                        let status = item
                            .get("status")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("");
                        status != "done" && status != "closed"
                    })
                    .map(|item| LedgerChore {
                        description: item
                            .get("description")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(chores)
    }

    fn file_chore(
        &self,
        title: &str,
        description: &str,
        _signature: &str,
    ) -> Result<(), LedgerError> {
        let status = Command::new(&self.capture_cli)
            .args([
                "--action",
                "capture-work-item",
                "--json",
                "--type",
                "chore",
                "--rank",
                "a0",
                "--title",
                title,
                "--description",
                description,
            ])
            .status()
            .map_err(|err| LedgerError(format!("capture-work-item failed to start: {err}")))?;
        if !status.success() {
            return Err(LedgerError(format!(
                "capture-work-item exited with: {status}"
            )));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Finding deserialization
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum FindingInput {
    FuzzCrash {
        target: String,
        artifact_path: String,
    },
    SurvivingMutant {
        source_file: String,
        line: u32,
        mutation_operator: String,
    },
}

fn parse_findings(json_str: &str) -> Result<Vec<FindingInput>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json_str).map_err(|err| format!("JSON parse error: {err}"))?;
    let arr = value
        .as_array()
        .ok_or_else(|| "findings JSON must be an array".to_owned())?;
    let mut findings = Vec::new();
    for item in arr {
        let kind = item
            .get("type")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "finding missing 'type' field".to_owned())?;
        match kind {
            "fuzz_crash" => {
                let target = item
                    .get("target")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| "fuzz_crash missing 'target'".to_owned())?
                    .to_owned();
                let artifact_path = item
                    .get("artifact_path")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| "fuzz_crash missing 'artifact_path'".to_owned())?
                    .to_owned();
                findings.push(FindingInput::FuzzCrash {
                    target,
                    artifact_path,
                });
            }
            "surviving_mutant" => {
                let source_file = item
                    .get("source_file")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| "surviving_mutant missing 'source_file'".to_owned())?
                    .to_owned();
                let line = item
                    .get("line")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| "surviving_mutant missing valid 'line'".to_owned())?;
                let mutation_operator = item
                    .get("mutation_operator")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| "surviving_mutant missing 'mutation_operator'".to_owned())?
                    .to_owned();
                findings.push(FindingInput::SurvivingMutant {
                    source_file,
                    line,
                    mutation_operator,
                });
            }
            other => {
                return Err(format!("unknown finding type: {other}"));
            }
        }
    }
    Ok(findings)
}

fn input_to_kind(input: &FindingInput) -> Result<FindingKind, String> {
    match input {
        FindingInput::FuzzCrash {
            target,
            artifact_path,
        } => {
            let reproducing_input = std::fs::read(artifact_path)
                .map_err(|err| format!("cannot read artifact {artifact_path}: {err}"))?;
            Ok(FindingKind::FuzzCrash {
                target: target.clone(),
                reproducing_input,
            })
        }
        FindingInput::SurvivingMutant {
            source_file,
            line,
            mutation_operator,
        } => Ok(FindingKind::SurvivingMutant {
            source_file: source_file.clone(),
            line: *line,
            mutation_operator: mutation_operator.clone(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let json_str = if args.len() > 1 && args[1] != "-" {
        match std::fs::read_to_string(&args[1]) {
            Ok(s) => s,
            Err(err) => {
                eprintln!("nightly-soak: cannot read findings file {}: {err}", args[1]);
                return ExitCode::SUCCESS;
            }
        }
    } else {
        let mut buf = String::new();
        if let Err(err) = std::io::stdin().read_to_string(&mut buf) {
            eprintln!("nightly-soak: cannot read stdin: {err}");
            return ExitCode::SUCCESS;
        }
        buf
    };

    if json_str.trim().is_empty() || json_str.trim() == "[]" {
        println!("nightly-soak: no findings to process");
        return ExitCode::SUCCESS;
    }

    let inputs = match parse_findings(&json_str) {
        Ok(inputs) => inputs,
        Err(err) => {
            eprintln!("nightly-soak: findings parse error: {err}");
            return ExitCode::SUCCESS;
        }
    };

    // Prefer BeadsDouble in test/dry-run mode (env NIGHTLY_SOAK_DRY_RUN=1).
    let dry_run = std::env::var("NIGHTLY_SOAK_DRY_RUN")
        .map(|v| v == "1")
        .unwrap_or(false);

    if dry_run {
        let double = BeadsDouble::new(vec![]);
        process_all(&inputs, &double);
    } else {
        let ledger = BeadsLedger::new();
        process_all(&inputs, &ledger);
    }

    ExitCode::SUCCESS
}

fn process_all(inputs: &[FindingInput], ledger: &dyn LedgerPort) {
    let filer = NightlySoakFiler::new(ledger);
    for input in inputs {
        let kind = match input_to_kind(input) {
            Ok(kind) => kind,
            Err(err) => {
                eprintln!("nightly-soak: skipping finding ({err})");
                continue;
            }
        };
        match filer.process_finding(&kind) {
            Ok(FilingResult::Filed { signature }) => {
                println!("nightly-soak: filed chore for {}", signature.as_str());
            }
            Ok(FilingResult::AlreadyOpen { signature }) => {
                println!(
                    "nightly-soak: open chore already exists for {}",
                    signature.as_str()
                );
            }
            Err(err) => {
                eprintln!("nightly-soak: ledger error (non-fatal): {err}");
            }
        }
    }
}
