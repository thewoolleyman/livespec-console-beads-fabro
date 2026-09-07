//! Binary composition root for the nightly soak driver.
//!
//! Reads a JSON findings file (path from argv[1]) produced by the
//! `just nightly-soak` recipe and processes each finding through
//! [`NightlySoakFiler`], wiring the production transport: the `ci-writer` SSH
//! forced command the shared Dolt host exposes on the tailnet.
//!
//! Two exit rules, and they are not in tension. A FINDING never fails the
//! canonical branch — that is the whole point of filing a chore instead — so a
//! malformed findings file or an unreadable crash artifact is reported and
//! skipped at exit 0. An unreachable INGRESS is a different failure: the write
//! surface is reachable only from the tailnet, and the ratified v048 clause
//! requires a nightly scheduled without a tailnet identity to "fail loudly
//! rather than complete while filing nothing". So every ingress failure —
//! including no configured destination at all — exits non-zero and names what
//! could not be filed.
//!
//! CI holds no work-items database credential here: it authenticates with the
//! `ci-writer` SSH key alone, and the host runs its own pinned `bd` behind the
//! forced command. Nothing in this binary shells out to `bd`, reads the ledger,
//! or needs the family secret wrapper.

#![forbid(unsafe_code)]

use std::io::Read;
use std::process::{Command, ExitCode};

use console_nightly_soak::{
    FindingIngress, FindingKind, IngressError, NightlySoakFiler, RecordingIngress,
    ingress_request_line,
};

// ---------------------------------------------------------------------------
// The tailnet SSH write ingress
// ---------------------------------------------------------------------------

/// Names the `user@host` the ci-writer forced command answers on.
///
/// It is deliberately configuration rather than a baked-in literal: the
/// workflow supplies it beside the `BEADS_CI_WRITER_SSH_KEY` secret and the
/// `BEADS_CI_WRITER_KNOWN_HOSTS` variable, and an unset value is a fail-loudly
/// condition rather than a default worth guessing.
const SSH_DESTINATION_ENV: &str = "BEADS_CI_WRITER_SSH_DESTINATION";

/// The production [`FindingIngress`]: one `ssh` invocation per finding,
/// carrying the request line as the remote command the forced command reads.
struct SshIngress {
    destination: String,
}

impl SshIngress {
    /// Resolve the ingress destination from the environment.
    fn from_env() -> Result<Self, IngressError> {
        std::env::var(SSH_DESTINATION_ENV).map_or_else(
            |_| {
                Err(IngressError(format!(
                    "{SSH_DESTINATION_ENV} is unset: the nightly files through the on-tailnet \
                     ci-writer SSH ingress and has no other write path"
                )))
            },
            |destination| Ok(Self { destination }),
        )
    }
}

impl FindingIngress for SshIngress {
    fn create(
        &self,
        fingerprint: &str,
        title: &str,
        body: Option<&str>,
    ) -> Result<(), IngressError> {
        let request = ingress_request_line(fingerprint, title, body);
        // BatchMode refuses to sit at a prompt on a scheduled runner, and
        // strict host-key checking makes an unknown host a failure rather than
        // a trust-on-first-use: both turn "cannot reach the ingress" into an
        // immediate non-zero exit instead of a hang or a silent misfile.
        let output = Command::new("ssh")
            .args(["-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=yes"])
            .arg(&self.destination)
            .arg(&request)
            .output()
            .map_err(|err| {
                IngressError(format!(
                    "cannot run ssh for create {fingerprint} to {}: {err}",
                    self.destination
                ))
            })?;
        if output.status.success() {
            return Ok(());
        }
        Err(IngressError(format!(
            "ci-writer ssh ingress rejected create {fingerprint} to {} ({}): {}",
            self.destination,
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
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
        return process_all(&inputs, &double);
    }
    match SshIngress::from_env() {
        Ok(ingress) => process_all(&inputs, &ingress),
        Err(err) => {
            eprintln!("nightly-soak: {err}");
            ExitCode::FAILURE
        }
    }
}

/// File every finding, returning FAILURE if the ingress refused any of them.
///
/// A finding the soak cannot even describe (an unreadable crash artifact) is
/// skipped at SUCCESS — that is a soak-input problem, not a filing failure.
/// An ingress refusal is the fail-loudly case: reporting it and exiting 0 is
/// exactly the "complete while filing nothing" outcome the clause forbids.
fn process_all(inputs: &[FindingInput], ingress: &dyn FindingIngress) -> ExitCode {
    let filer = NightlySoakFiler::new(ingress);
    let mut refused = 0_usize;
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
                refused += 1;
                eprintln!("nightly-soak: {err}");
            }
        }
    }
    if refused == 0 {
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "nightly-soak: {refused} finding(s) could not be filed through the write ingress"
        );
        ExitCode::FAILURE
    }
}
