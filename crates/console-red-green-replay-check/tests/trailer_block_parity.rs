//! Red-state detection and Green verification must read ONE parsing surface.
//!
//! `head_red_awaiting_green` used to substring-scan the whole commit message
//! for `TDD-Red-Test-File-Checksum:` while `handle_green` read the same
//! information through `git log --pretty=%(trailers:...)`, which parses the
//! FINAL trailer block only. A commit carrying the Red tokens outside that
//! block therefore classified as red-awaiting and then yielded an EMPTY test
//! path, wedging the branch with `cannot read : No such file or directory` --
//! an error naming neither the cause nor the fix
//! (livespec-console-beads-fabro-gwcq2f).
//!
//! These are integration tests on purpose. The functions at fault are private,
//! and `check_commit_msg` is the smallest public surface that exercises the
//! disagreement end to end -- which is also the surface a wedged branch hits.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use console_red_green_replay_check::{CommandOutput, Runner, TestScope, check_commit_msg};

/// A `Runner` that answers from a fixed table, recording the cargo scopes it
/// was asked for so a test can assert WHICH verification path ran.
struct FakeRunner {
    /// Body returned for `git log -1 --format=%B`.
    log_body: String,
    /// Values `git log --pretty=%(trailers:key=K,valueonly)` reports, i.e. the
    /// FINAL trailer block only. A key absent here is a key git would report
    /// as empty.
    trailers: HashMap<String, String>,
    scopes: RefCell<Vec<TestScope>>,
}

impl FakeRunner {
    fn new(log_body: &str, trailers: &[(&str, &str)]) -> Self {
        Self {
            log_body: log_body.to_owned(),
            trailers: trailers
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
            scopes: RefCell::new(Vec::new()),
        }
    }

    fn scopes(&self) -> Vec<TestScope> {
        self.scopes.borrow().clone()
    }
}

impl Runner for FakeRunner {
    fn git(&self, args: &[&str]) -> Result<CommandOutput, String> {
        let joined = args.join(" ");
        let stdout = match joined.as_str() {
            "diff --cached --name-only --diff-filter=d" => {
                "crates/console-eventstore/src/lib.rs\n".to_owned()
            }
            "rev-parse --verify --quiet HEAD" | "rev-parse HEAD" => {
                "0f1e2d3c4b5a69788796a5b4c3d2e1f0deadbeef\n".to_owned()
            }
            "log -1 --format=%B" => self.log_body.clone(),
            other => other
                .strip_prefix("log -1 --pretty=%(trailers:key=")
                .and_then(|rest| rest.split(',').next())
                .and_then(|key| self.trailers.get(key))
                .cloned()
                .unwrap_or_default(),
        };
        Ok(CommandOutput::success(&stdout))
    }

    fn cargo_test(&self, scope: TestScope) -> Result<CommandOutput, String> {
        self.scopes.borrow_mut().push(scope);
        Ok(CommandOutput::success("test result: ok. 1 passed\n"))
    }
}

/// A commit-message file the checker may rewrite in place.
fn message_file(name: &str, body: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "rgr-trailer-block-parity-{}-{name}.msg",
        std::process::id()
    ));
    assert!(
        std::fs::write(&path, body).is_ok(),
        "write the commit-message fixture at {}",
        path.display()
    );
    path
}

/// The defect: Red tokens present in the message but OUTSIDE the final trailer
/// block. `%(trailers:...)` reports nothing for them, so a detector that reads
/// the raw body disagrees with the verifier that reads trailers.
#[test]
fn red_tokens_outside_the_final_trailer_block_do_not_claim_a_red_to_verify() {
    let runner = FakeRunner::new(
        concat!(
            "fix(store): something\n",
            "\n",
            "Quoting the ritual for reference:\n",
            "TDD-Red-Test: crates/x/tests/y.rs\n",
            "TDD-Red-Test-File-Checksum: sha256:a\n",
            "\n",
            "...and then a trailing paragraph, which ends the trailer block.\n",
        ),
        &[],
    );
    let msg = message_file("outside", "fix(store): next change\n");

    let result = check_commit_msg(&runner, &msg);

    let rendered = format!("{result:?}");
    assert!(
        rendered.contains("Ok"),
        "expected the commit to be accepted through the suite-green path, got {rendered}"
    );
    let scopes = format!("{:?}", runner.scopes());
    assert!(
        scopes.contains("Workspace"),
        "expected the full-suite verification to run, got {scopes}"
    );
    assert!(
        !scopes.contains("Integration"),
        "expected NO per-test Green verification, got {scopes}"
    );
    let _ = std::fs::remove_file(&msg);
}

/// Defence in depth: a genuinely red-awaiting HEAD whose trailer block is
/// malformed -- the checksum trailer is present but the test-path trailer is
/// not -- must fail with an error naming the commit and the requirement, not
/// with `cannot read :`.
#[test]
fn a_red_awaiting_head_without_a_readable_test_trailer_fails_actionably() {
    let runner = FakeRunner::new(
        "fix(store): something\n\nTDD-Red-Test-File-Checksum: sha256:a\n",
        &[("TDD-Red-Test-File-Checksum", "sha256:a")],
    );
    let msg = message_file("malformed", "fix(store): next change\n");

    let result = check_commit_msg(&runner, &msg);

    let rendered = format!("{result:?}");
    assert!(
        rendered.contains("red-green-replay-red-test-trailer-missing"),
        "expected a named, actionable failure, got {rendered}"
    );
    assert!(
        rendered.contains("0f1e2d3c4b5a69788796a5b4c3d2e1f0deadbeef"),
        "expected the failure to name the offending HEAD commit, got {rendered}"
    );
    assert!(
        rendered.contains("final trailer block"),
        "expected the failure to state the trailer-block requirement, got {rendered}"
    );
    assert!(
        !rendered.contains("cannot read :"),
        "expected the un-actionable empty-path error to be gone, got {rendered}"
    );
    let _ = std::fs::remove_file(&msg);
}
