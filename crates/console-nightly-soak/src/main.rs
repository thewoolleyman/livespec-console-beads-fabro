//! Binary composition root for the nightly soak driver.
//!
//! Reads a JSON findings file (path from argv[1]) produced by the
//! `just nightly-soak` recipe and processes each finding through
//! [`NightlySoakFiler`]. Always exits 0: findings never fail master.
//!
//! The production transport — the SSH forced command on the shared Dolt host,
//! reachable only from the tailnet — is wired in the second slice of this
//! change (`livespec-console-beads-fabro-4jb3kl.4`). Until it lands, the
//! non-dry-run path is an UNWIRED ingress that refuses every request and says
//! so, which the loop below reports per finding. That is deliberately louder
//! than the surface it replaces: the previous composition root read the
//! work-items ledger directly and filed through the orchestrator capture
//! surface, which the ratified v048 ingress clause forbids CI from doing at
//! all (`CI MUST NOT hold the work-items database credential`), and whose
//! filings the ingress would in any case have rejected for their pre-v048
//! fingerprints.

#![forbid(unsafe_code)]

use std::io::Read;
use std::process::ExitCode;

use console_nightly_soak::{
    FindingIngress, FindingKind, IngressError, NightlySoakFiler, RecordingIngress,
};

// ---------------------------------------------------------------------------
// Placeholder transport until the tailnet SSH ingress lands
// ---------------------------------------------------------------------------

struct UnwiredIngress;

impl FindingIngress for UnwiredIngress {
    fn create(
        &self,
        fingerprint: &str,
        title: &str,
        _body: Option<&str>,
    ) -> Result<(), IngressError> {
        Err(IngressError(format!(
            "tailnet SSH write ingress not wired yet \
             (livespec-console-beads-fabro-4jb3kl.4); dropped create {fingerprint} ({title})"
        )))
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

    // Prefer the recording double in test/dry-run mode (NIGHTLY_SOAK_DRY_RUN=1).
    let dry_run = std::env::var("NIGHTLY_SOAK_DRY_RUN")
        .map(|v| v == "1")
        .unwrap_or(false);

    if dry_run {
        let double = RecordingIngress::default();
        process_all(&inputs, &double);
    } else {
        process_all(&inputs, &UnwiredIngress);
    }

    ExitCode::SUCCESS
}

fn process_all(inputs: &[FindingInput], ingress: &dyn FindingIngress) {
    let filer = NightlySoakFiler::new(ingress);
    for input in inputs {
        let kind = match input_to_kind(input) {
            Ok(kind) => kind,
            Err(err) => {
                eprintln!("nightly-soak: skipping finding ({err})");
                continue;
            }
        };
        match filer.process_finding(&kind) {
            Ok(fingerprint) => {
                // Accepted, not necessarily created: the ingress suppresses a
                // fingerprint that already carries an open chore, host-side and
                // indistinguishably from here.
                println!("nightly-soak: submitted finding {}", fingerprint.as_str());
            }
            Err(err) => {
                eprintln!("nightly-soak: ingress error (non-fatal): {err}");
            }
        }
    }
}
