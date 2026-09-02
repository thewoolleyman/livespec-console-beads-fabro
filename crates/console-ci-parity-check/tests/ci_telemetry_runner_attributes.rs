//! Every `ci.job.*` span the CI telemetry exporter emits must carry runner
//! identity.
//!
//! # Why this gate exists
//!
//! `.github/scripts/export-ci-telemetry.sh` used to emit per-job spans with no
//! runner identity at all — no name, no labels, no hosted/self-hosted flag. The
//! console's CI has run on both GitHub-hosted `ubuntu-latest` and the
//! self-hosted `poweredge` ARC pool, and which one a given run used is decided
//! at dispatch by the `CI_RUNNER_LABELS` repo variable. With no attribute
//! recording it, the only way to split a self-hosted window from a hosted one in
//! Honeycomb was to *guess the switch date* from a step-change in job duration
//! (see `plan/optimize-console-builds/research/008-pre-raid10-poweredge-disk-baseline.md`).
//! That makes every before/after comparison — a disk upgrade, a runner resize —
//! a date guess rather than a filter.
//!
//! # Why it runs the script instead of restating its logic
//!
//! The derivation under test is three lines of `jq`/`bash` inside the exporter.
//! A Rust reimplementation of "labels contain `ubuntu-latest` ⇒ hosted" would
//! assert that the test agrees with itself and would stay green if the exporter
//! stopped emitting the attributes entirely. So the exporter is executed for
//! real over committed fixtures, with `gh` and `curl` replaced by stubs, and the
//! assertions are made against the OTLP payload it actually POSTs.
//!
//! `gh` is stubbed rather than mocked at the HTTP layer because the exporter
//! reaches GitHub twice for different shapes: `gh run view --json jobs` (gh's
//! own projection, which cannot express `runner_name`) and the raw jobs API
//! (which can). Both fixtures are the real payload shapes.
//!
//! `jq` itself is NOT stubbed — it is the language the exporter is written in.
//! It is pinned in `.mise.toml` for exactly this reason; see the comment there.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Error, ErrorKind, Result};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// The exporter under test, relative to the repo root.
const EXPORTER: &str = ".github/scripts/export-ci-telemetry.sh";

/// Replays `gh run view --json ...` from the committed run fixture, and applies
/// the exporter's own `--jq` filter to the committed jobs-API fixture the way
/// `gh api --jq` would. Any other subcommand is a signal that the exporter grew
/// a GitHub call this harness has not been taught about, so it is a hard error
/// rather than a silent empty result.
const GH_STUB: &str = r#"#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  run)
    cat "$STUB_RUN_VIEW"
    ;;
  api)
    filter="."
    while [ "$#" -gt 1 ]; do
      if [ "$1" = "--jq" ]; then filter="$2"; fi
      shift
    done
    jq -c "$filter" "$STUB_JOBS_API"
    ;;
  *)
    echo "stub gh: unexpected invocation: $*" >&2
    exit 64
    ;;
esac
"#;

/// Captures the OTLP payload the exporter POSTs and answers with the status and
/// rejected-span count the test asked for. Writes the status to stdout because
/// that is what the exporter's `-w '%{http_code}'` reads.
const CURL_STUB: &str = r#"#!/usr/bin/env bash
set -euo pipefail
out=""
payload=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) out="$2"; shift 2 ;;
    --data-binary) payload="${2#@}"; shift 2 ;;
    *) shift ;;
  esac
done
cp "$payload" "$STUB_CAPTURE"
printf '{"partialSuccess":{"rejectedSpans":%s}}' "$STUB_REJECTED" >"$out"
printf '%s' "$STUB_HTTP"
"#;

fn repo_root() -> Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn invalid_data(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidData, message.into())
}

/// A scratch directory holding the `gh`/`curl` stubs and the captured payload.
struct Harness {
    root: PathBuf,
}

impl Harness {
    fn new(name: &str) -> Result<Self> {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "console-ci-telemetry-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let bin = root.join("bin");
        fs::create_dir_all(&bin)?;
        write_executable(&bin.join("gh"), GH_STUB)?;
        write_executable(&bin.join("curl"), CURL_STUB)?;
        Ok(Self { root })
    }

    fn capture_path(&self) -> PathBuf {
        self.root.join("otlp-payload.json")
    }

    /// Run the exporter with the stubs ahead of the inherited `PATH`, answering
    /// its ingest POST with `http`/`rejected`.
    fn export(&self, http: &str, rejected: &str) -> Result<std::process::Output> {
        let root = repo_root()?;
        let inherited = std::env::var("PATH").unwrap_or_default();
        let path = format!("{}:{inherited}", self.root.join("bin").display());
        Command::new("bash")
            .arg(root.join(EXPORTER))
            .current_dir(&root)
            .env("PATH", path)
            .env("REPO", "thewoolleyman/livespec-console-beads-fabro")
            .env("RUN_ID", "4242424242")
            .env("HONEYCOMB_GITHUB_CI_INGEST_KEY_LIVESPEC", "test-ingest-key")
            .env("STUB_RUN_VIEW", fixture("ci-telemetry-run-view.json"))
            .env("STUB_JOBS_API", fixture("ci-telemetry-jobs-api.json"))
            .env("STUB_CAPTURE", self.capture_path())
            .env("STUB_HTTP", http)
            .env("STUB_REJECTED", rejected)
            .output()
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).ok();
    }
}

fn write_executable(path: &Path, body: &str) -> Result<()> {
    fs::write(path, body)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

/// Fail early and legibly when `jq` is missing, rather than through a stub whose
/// error would read like an exporter bug.
fn require_jq() -> Result<()> {
    const HINT: &str = "jq is required to run the CI telemetry exporter and is pinned in \
                        .mise.toml; run this test under mise (`mise exec -- just check-nextest`)";
    Command::new("jq")
        .arg("--version")
        .output()
        .map_err(|err| invalid_data(format!("{HINT}: {err}")))?;
    Ok(())
}

/// The `ci.job.*` spans of an OTLP payload, keyed by `ci.job.name`, each mapped
/// to its own string attributes.
fn job_span_attributes(payload: &Value) -> Result<BTreeMap<String, BTreeMap<String, String>>> {
    let spans = payload
        .pointer("/resourceSpans/0/scopeSpans/0/spans")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_data(format!("no OTLP spans in payload: {payload}")))?;
    let mut by_job = BTreeMap::new();
    for span in spans {
        let name = span.get("name").and_then(Value::as_str).unwrap_or_default();
        if !name.starts_with("ci.job.") {
            continue;
        }
        let mut attributes = BTreeMap::new();
        let listed = span
            .get("attributes")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid_data(format!("span {name} has no attributes")))?;
        for attribute in listed {
            let (Some(key), Some(value)) = (
                attribute.get("key").and_then(Value::as_str),
                attribute
                    .pointer("/value/stringValue")
                    .and_then(Value::as_str),
            ) else {
                continue;
            };
            attributes.insert(key.to_owned(), value.to_owned());
        }
        by_job.insert(name.to_owned(), attributes);
    }
    Ok(by_job)
}

fn captured_payload(harness: &Harness) -> Result<Value> {
    let raw = fs::read_to_string(harness.capture_path())?;
    serde_json::from_str(&raw).map_err(|err| invalid_data(format!("captured payload: {err}")))
}

fn describe(output: &std::process::Output) -> String {
    format!(
        "status={:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn every_ci_job_span_carries_the_runner_identity_of_its_job() -> Result<()> {
    require_jq()?;
    let harness = Harness::new("runner-identity")?;

    let output = harness.export("200", "0")?;
    assert!(output.status.success(), "{}", describe(&output));

    let payload = captured_payload(&harness)?;
    let by_job = job_span_attributes(&payload)?;
    assert_eq!(
        by_job.keys().collect::<Vec<_>>(),
        vec!["ci.job.check-deps", "ci.job.check-format"],
        "both fixture jobs must produce a ci.job.* span: {payload}"
    );

    // The hosted job: `ubuntu-latest` in its labels.
    let hosted = by_job
        .get("ci.job.check-format")
        .ok_or_else(|| invalid_data("missing hosted job span"))?;
    assert_eq!(
        hosted.get("ci.runner.name").map(String::as_str),
        Some("GitHub Actions 3"),
        "{hosted:?}"
    );
    assert_eq!(
        hosted.get("ci.runner.labels").map(String::as_str),
        Some("ubuntu-latest"),
        "{hosted:?}"
    );
    assert_eq!(
        hosted.get("ci.runner.kind").map(String::as_str),
        Some("hosted"),
        "{hosted:?}"
    );

    // The self-hosted job: an ARC pod on the poweredge pool.
    let self_hosted = by_job
        .get("ci.job.check-deps")
        .ok_or_else(|| invalid_data("missing self-hosted job span"))?;
    assert_eq!(
        self_hosted.get("ci.runner.name").map(String::as_str),
        Some("livespec-console-beads-k3s-nx8fj-runner-9wqcv"),
        "{self_hosted:?}"
    );
    assert_eq!(
        self_hosted.get("ci.runner.labels").map(String::as_str),
        Some("livespec-console-beads-k3s"),
        "{self_hosted:?}"
    );
    assert_eq!(
        self_hosted.get("ci.runner.kind").map(String::as_str),
        Some("self-hosted"),
        "{self_hosted:?}"
    );

    Ok(())
}

/// The closed-loop property the exporter exists for: a rejected payload must
/// redden the job rather than pass quietly. Adding attributes to the span shape
/// is exactly the kind of change that can start tripping Honeycomb's ingest, so
/// the fail-hard leg is pinned alongside them.
#[test]
fn a_rejected_payload_still_fails_the_export() -> Result<()> {
    require_jq()?;
    let harness = Harness::new("ingest-rejected")?;

    let output = harness.export("400", "2")?;

    assert!(!output.status.success(), "{}", describe(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("CI telemetry export to Honeycomb FAILED"),
        "{}",
        describe(&output)
    );
    Ok(())
}
