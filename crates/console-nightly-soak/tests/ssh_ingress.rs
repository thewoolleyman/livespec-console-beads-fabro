//! End-to-end coverage of the nightly soak's production write transport.
//!
//! The composition root files findings through the `ci-writer` SSH forced
//! command on the shared Dolt host — the only write surface CI is allowed, per
//! `SPECIFICATION/non-functional-requirements.md` §Quality Gate as ratified in
//! v048. These tests drive the real binary with a FAKE `ssh` first on `PATH`,
//! so they observe exactly what the ingress would observe: one `create`
//! request line per finding, carrying the fingerprint verbatim and
//! base64url-encoded title and body, and nothing else.
//!
//! The remaining two cases pin the fail-loudly obligation. The ingress is
//! reachable only from the tailnet, so a nightly that cannot reach it MUST
//! fail rather than complete while filing nothing: neither a non-zero `ssh`
//! exit nor a missing destination may be reported and then dropped.
//!
//! Each case returns `io::Result` and propagates with `?` rather than
//! unwrapping: the workspace denies `expect_used`, `unwrap_used`, and `panic`
//! in every target, tests included.

use std::io;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The one finding every case files. Its fingerprint and chore fields are the
/// ones the crate's own unit tests pin, so the expected request line below is
/// anchored to values asserted elsewhere.
const FINDINGS_JSON: &str = r#"[
  {
    "type": "surviving_mutant",
    "source_file": "crates/console-domain/src/lib.rs",
    "line": 42,
    "mutation_operator": "replace + with -"
  }
]"#;

/// The exact request line the ci-writer forced command must receive.
///
/// `create <fingerprint> <base64url-title> <base64url-body>`, where the
/// fingerprint is `mutant-<first 32 hex of SHA-256("<file>:<line>:<op>")>` and
/// both fields are RFC 4648 §5 base64url with the padding stripped. Written out
/// literally so an encoding regression cannot hide behind the code that
/// produced it.
const EXPECTED_REQUEST_LINE: &str = concat!(
    "create mutant-7367e080768a2733ac64929a669ded1a ",
    "bmlnaHRseTogc3Vydml2aW5nIG11dGFudCBpbiBjcmF0ZXMvY29uc29sZS1kb21haW4vc3JjL2xpYi5\
     yczo0MiAocmVwbGFjZSArIHdpdGggLSk ",
    "TmlnaHRseSBzb2FrIGZpbmRpbmc6IHN1cnZpdmluZyBtdXRhbnQuCgpzb3VyY2UgZmlsZTogY3JhdGV\
     zL2NvbnNvbGUtZG9tYWluL3NyYy9saWIucnMKbGluZTogNDIKbXV0YXRpb24gb3BlcmF0b3I6IHJlcG\
     xhY2UgKyB3aXRoIC0KClRoZSBmaW5nZXJwcmludCBpcyB0aGUgU0hBLTI1NiBvZiB0aGF0IHRocmVlL\
     XBhcnQgaWRlbnRpdHku",
);

/// The environment variable naming the ci-writer SSH destination.
const DESTINATION_ENV: &str = "BEADS_CI_WRITER_SSH_DESTINATION";

/// A scratch directory holding the fake `ssh`, its argv log, and the findings
/// file. Removed when the case ends.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    /// Build a sandbox whose fake `ssh` appends its argv to `ssh-argv.log`, one
    /// argument per line, and then exits with `exit_code`.
    fn new(label: &str, exit_code: i32) -> io::Result<Self> {
        let root = std::env::temp_dir().join(format!(
            "console-nightly-soak-{}-{label}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root)?;

        let ssh = root.join("ssh");
        std::fs::write(
            &ssh,
            format!(
                "#!/bin/sh\n\
                 printf '%s\\n' \"$@\" >> \"$NIGHTLY_SOAK_FAKE_SSH_LOG\"\n\
                 if [ {exit_code} -ne 0 ]; then\n\
                 \x20 echo 'ssh: connect to host: Network is unreachable' >&2\n\
                 fi\n\
                 exit {exit_code}\n"
            ),
        )?;
        std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755))?;
        std::fs::write(root.join("findings.json"), FINDINGS_JSON)?;

        Ok(Self { root })
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    /// The soak binary, pointed at this sandbox's findings file, with the fake
    /// `ssh` first on `PATH` and the production (non-dry-run) transport wired.
    ///
    /// Uses `std::env::var` rather than `env!` because `CARGO_BIN_EXE_*` is set
    /// by cargo at test-run time, not at clippy check time.
    fn soak_command(&self) -> io::Result<Command> {
        let binary =
            std::env::var("CARGO_BIN_EXE_console-nightly-soak").map_err(io::Error::other)?;
        let mut command = Command::new(binary);
        command
            .arg(self.path("findings.json"))
            .env("PATH", prepended_path(&self.root))
            .env("NIGHTLY_SOAK_FAKE_SSH_LOG", self.path("ssh-argv.log"))
            .env_remove("NIGHTLY_SOAK_DRY_RUN");
        Ok(command)
    }

    fn run_soak(&self) -> io::Result<Output> {
        self.soak_command()?
            .env(DESTINATION_ENV, "ci-writer@dolt-server")
            .output()
    }

    /// Every argument the fake `ssh` was invoked with, in order.
    fn ssh_argv(&self) -> Vec<String> {
        std::fs::read_to_string(self.path("ssh-argv.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// `PATH` with `dir` in front, so `Command::new("ssh")` resolves to the fake.
fn prepended_path(dir: &Path) -> String {
    let inherited = std::env::var("PATH").unwrap_or_default();
    format!("{}:{inherited}", dir.display())
}

/// Case 1 — a finding reaches the ingress as one exact `create` request line.
#[test]
fn the_nightly_files_a_finding_as_one_create_line_on_the_ssh_ingress() -> io::Result<()> {
    let sandbox = Sandbox::new("files-one-create-line", 0)?;

    let output = sandbox.run_soak()?;

    assert!(
        output.status.success(),
        "a filed finding must not fail the canonical branch; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let argv = sandbox.ssh_argv();
    assert_eq!(
        argv.last().map(String::as_str),
        Some(EXPECTED_REQUEST_LINE),
        "the remote command must be the exact single-line create request"
    );
    assert!(
        argv.contains(&"ci-writer@dolt-server".to_owned()),
        "the configured ci-writer destination must be the ssh target; argv: {argv:?}"
    );
    assert_eq!(
        argv.iter().filter(|arg| arg.starts_with("create ")).count(),
        1,
        "one finding files exactly one create request; argv: {argv:?}"
    );
    Ok(())
}

/// Case 2 — a non-zero `ssh` exit fails the nightly instead of being swallowed.
#[test]
fn an_unreachable_ssh_ingress_fails_the_nightly_loudly() -> io::Result<()> {
    let sandbox = Sandbox::new("unreachable-ingress-fails", 255)?;

    let output = sandbox.run_soak()?;

    assert!(
        !output.status.success(),
        "an ingress the nightly cannot reach must fail loudly, not file nothing quietly"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("mutant-7367e080768a2733ac64929a669ded1a"),
        "the failure must name the finding it could not file; stderr: {stderr}"
    );
    Ok(())
}

/// Case 3 — a nightly with no configured destination fails before sending.
#[test]
fn an_unconfigured_ingress_destination_fails_the_nightly_loudly() -> io::Result<()> {
    let sandbox = Sandbox::new("unconfigured-destination", 0)?;

    let output = sandbox
        .soak_command()?
        .env_remove(DESTINATION_ENV)
        .output()?;

    assert!(
        !output.status.success(),
        "a nightly with nowhere to file must fail rather than complete silently"
    );
    assert!(
        sandbox.ssh_argv().is_empty(),
        "no request may be attempted without a configured destination"
    );
    Ok(())
}
