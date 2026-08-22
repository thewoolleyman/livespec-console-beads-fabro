//! Rust Red-Green-Replay checker.
//!
//! Content is the trigger. Subject prefixes never exempt product Rust from the
//! ritual; a commit staging no product Rust already passes without a special
//! docs/chore rule.

#![forbid(unsafe_code)]

use std::fmt::Write as _;
use std::path::Path;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use sha2::{Digest, Sha256};

pub const PORTED_FROM_UPSTREAM: &str = concat!(
    "livespec-dev-tooling@87fd400dae07f537df9a200d54b2f4dc44c42971 ",
    "livespec_dev_tooling/checks/red_green_replay.py; ",
    "livespec_dev_tooling/checks/_red_green_replay_modes.py; ",
    "livespec_dev_tooling/checks/_red_green_replay_trailers.py"
);

pub const RANGE_BASE: &str = "origin/master";

const RED_TEST_KEY: &str = "TDD-Red-Test";
const RED_CHECKSUM_KEY: &str = "TDD-Red-Test-File-Checksum";
const RED_TRAILER_TOKEN: &str = "TDD-Red-Test-File-Checksum:";
const GREEN_TRAILER_TOKEN: &str = "TDD-Green-Verified-At:";
const SUITE_TRAILER_TOKEN: &str = "TDD-Suite-Green-Captured-At:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    #[must_use]
    pub fn success(stdout: &str) -> Self {
        Self {
            code: 0,
            stdout: stdout.to_owned(),
            stderr: String::new(),
        }
    }

    #[must_use]
    pub fn failure(stderr: &str) -> Self {
        Self {
            code: 1,
            stdout: String::new(),
            stderr: stderr.to_owned(),
        }
    }
}

pub trait Runner {
    fn git(&self, args: &[&str]) -> Result<CommandOutput, String>;
    fn cargo_test(&self, scope: TestScope) -> Result<CommandOutput, String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Pass,
    Red,
    TestPassedAtRedReject,
    Green,
    SuiteGreen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestScope {
    Workspace,
    Integration { package: String, target: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Buckets {
    pub tests: Vec<String>,
    pub impls: Vec<String>,
}

#[must_use]
pub fn classify(paths: &[String]) -> Buckets {
    let tests = paths
        .iter()
        .filter(|path| is_rust(path) && is_integration_test_path(path))
        .cloned()
        .collect();
    let impls = paths
        .iter()
        .filter(|path| {
            is_rust(path) && is_product_impl_path(path) && !is_integration_test_path(path)
        })
        .cloned()
        .collect();
    Buckets { tests, impls }
}

#[must_use]
pub fn decide(
    subject: &str,
    buckets: &Buckets,
    head_red_awaiting_green: bool,
    tests_pass: bool,
) -> Decision {
    if buckets.tests.is_empty() && buckets.impls.is_empty() {
        return Decision::Pass;
    }
    if buckets.impls.is_empty() {
        if declares_red_intent(subject) {
            return if tests_pass {
                Decision::TestPassedAtRedReject
            } else {
                Decision::Red
            };
        }
        return if tests_pass {
            Decision::SuiteGreen
        } else {
            Decision::Red
        };
    }
    if head_red_awaiting_green {
        Decision::Green
    } else {
        Decision::SuiteGreen
    }
}

pub fn check_commit_msg(runner: &impl Runner, msg_path: &Path) -> Result<(), String> {
    let message = std::fs::read_to_string(msg_path)
        .map_err(|err| format!("cannot read {}: {err}", msg_path.display()))?;
    let subject = message.lines().next().unwrap_or_default();
    let staged = staged_files(runner)?;
    let buckets = classify(&staged);
    let tests_pass = if buckets.impls.is_empty() && !buckets.tests.is_empty() {
        runner.cargo_test(scope_for_tests(&buckets.tests))?.code == 0
    } else {
        false
    };
    let awaiting = if buckets.impls.is_empty() {
        false
    } else {
        head_red_awaiting_green(runner)?
    };

    match decide(subject, &buckets, awaiting, tests_pass) {
        Decision::Pass => Ok(()),
        Decision::Red => handle_red(runner, msg_path, &buckets.tests),
        Decision::TestPassedAtRedReject => Err("red-green-replay-test-passed-at-red: Red mode requires the staged Rust test to fail first".to_owned()),
        Decision::Green => handle_green(runner, msg_path),
        Decision::SuiteGreen => handle_suite_green(runner, msg_path),
    }
}

pub fn validate_default_range(runner: &impl Runner) -> Result<(), String> {
    let base = runner.git(&["rev-parse", "--verify", "--quiet", RANGE_BASE])?;
    if base.code != 0 {
        return Err(format!(
            "red-green-replay-range-base-unresolvable: fetch {RANGE_BASE} before validating {RANGE_BASE}..HEAD"
        ));
    }
    let shas = git_stdout_lines(
        runner,
        &["rev-list", "--no-merges", &format!("{RANGE_BASE}..HEAD")],
    )?;
    let mut violating = Vec::new();
    for sha in &shas {
        if commit_violates(runner, sha)? {
            violating.push(sha.clone());
        }
    }
    if violating.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "red-green-replay-range-missing-trailers: commits touch product Rust without TDD trailer shape: {}",
            violating.join(", ")
        ))
    }
}

fn handle_red(runner: &impl Runner, msg_path: &Path, tests: &[String]) -> Result<(), String> {
    if tests.len() > 1 {
        return Err(
            "red-green-replay-multi-test-file: stage exactly one Rust test file per Red commit"
                .to_owned(),
        );
    }
    let Some(test_path) = tests.first() else {
        return Err(
            "red-green-replay-red-without-test: Red mode requires one staged Rust test file"
                .to_owned(),
        );
    };
    let checksum = file_checksum(Path::new(test_path))?;
    let result = runner.cargo_test(scope_for_tests(tests))?;
    if result.code == 0 {
        return Err("red-green-replay-test-passed-at-red: Red mode requires the staged Rust test to fail first".to_owned());
    }
    write_trailers(
        msg_path,
        &[
            (RED_TEST_KEY, test_path),
            ("TDD-Red-Failure-Reason", &summary(&result.combined())),
            (RED_CHECKSUM_KEY, &checksum),
            (
                "TDD-Red-Output-Checksum",
                &text_checksum(&result.combined()),
            ),
            ("TDD-Red-Captured-At", &utc_timestamp()),
        ],
    )
}

fn handle_green(runner: &impl Runner, msg_path: &Path) -> Result<(), String> {
    let test_path = head_trailer_value(runner, RED_TEST_KEY)?;
    let recorded_checksum = head_trailer_value(runner, RED_CHECKSUM_KEY)?;
    let current_checksum = file_checksum(Path::new(&test_path))?;
    if current_checksum != recorded_checksum {
        return Err(
            "red-green-replay-checksum-mismatch: test file changed between Red and Green"
                .to_owned(),
        );
    }
    let result = runner.cargo_test(scope_for_tests(std::slice::from_ref(&test_path)))?;
    if result.code != 0 {
        return Err(
            "red-green-replay-test-still-failing: Green mode requires the Red test to pass"
                .to_owned(),
        );
    }
    let parent = current_head_sha(runner)?;
    write_trailers(
        msg_path,
        &[
            ("TDD-Green-Verified-At", &utc_timestamp()),
            ("TDD-Green-Parent-Reflog", &parent),
        ],
    )
}

fn handle_suite_green(runner: &impl Runner, msg_path: &Path) -> Result<(), String> {
    let result = runner.cargo_test(TestScope::Workspace)?;
    if result.code != 0 {
        return Err(format!(
            "red-green-replay-suite-red: full cargo test suite failed: {}",
            summary(&result.combined())
        ));
    }
    write_trailers(
        msg_path,
        &[
            ("TDD-Suite-Green-Scope", "full-suite"),
            (
                "TDD-Suite-Green-Output-Checksum",
                &text_checksum(&result.combined()),
            ),
            ("TDD-Suite-Green-Captured-At", &utc_timestamp()),
        ],
    )
}

fn staged_files(runner: &impl Runner) -> Result<Vec<String>, String> {
    git_stdout_lines(
        runner,
        &["diff", "--cached", "--name-only", "--diff-filter=d"],
    )
}

fn head_red_awaiting_green(runner: &impl Runner) -> Result<bool, String> {
    let resolved = runner.git(&["rev-parse", "--verify", "--quiet", "HEAD"])?;
    if resolved.code == 1 {
        return Ok(false);
    }
    if resolved.code != 0 {
        return Err(format!(
            "red-green-replay-git-command-failed: git rev-parse --verify --quiet HEAD: {}",
            summary(&resolved.stderr)
        ));
    }
    let message = git_stdout(runner, &["log", "-1", "--format=%B"])?;
    Ok(message.contains(RED_TRAILER_TOKEN) && !message.contains(GREEN_TRAILER_TOKEN))
}

fn head_trailer_value(runner: &impl Runner, key: &str) -> Result<String, String> {
    git_stdout(
        runner,
        &[
            "log",
            "-1",
            &format!("--pretty=%(trailers:key={key},valueonly)"),
        ],
    )
}

fn current_head_sha(runner: &impl Runner) -> Result<String, String> {
    git_stdout(runner, &["rev-parse", "HEAD"])
}

fn git_stdout(runner: &impl Runner, args: &[&str]) -> Result<String, String> {
    let result = runner.git(args)?;
    if result.code == 0 {
        Ok(result.stdout.trim().to_owned())
    } else {
        Err(format!(
            "red-green-replay-git-command-failed: git {}: {}",
            args.join(" "),
            summary(&result.stderr)
        ))
    }
}

fn git_stdout_lines(runner: &impl Runner, args: &[&str]) -> Result<Vec<String>, String> {
    Ok(git_stdout(runner, args)?
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

fn commit_violates(runner: &impl Runner, sha: &str) -> Result<bool, String> {
    let touched = git_stdout_lines(
        runner,
        &[
            "diff-tree",
            "--no-commit-id",
            "--name-only",
            "-r",
            "--root",
            "--diff-filter=d",
            sha,
        ],
    )?;
    if !touched.iter().any(|path| is_product_impl_path(path)) {
        return Ok(false);
    }
    let message = git_stdout(runner, &["log", "-1", "--format=%B", sha])?;
    let has_pair = message.contains(RED_TRAILER_TOKEN) && message.contains(GREEN_TRAILER_TOKEN);
    let has_suite = message.contains(SUITE_TRAILER_TOKEN);
    Ok(!(has_pair || has_suite))
}

fn write_trailers(msg_path: &Path, trailers: &[(&str, &str)]) -> Result<(), String> {
    let original = std::fs::read_to_string(msg_path)
        .map_err(|err| format!("cannot read {}: {err}", msg_path.display()))?;
    let keys = trailers.iter().map(|(key, _)| *key).collect::<Vec<_>>();
    let kept = original
        .lines()
        .filter(|line| {
            let head = line.split_once(':').map_or(*line, |(head, _)| head);
            !keys.contains(&head)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut next = kept.trim_end().to_owned();
    next.push_str("\n\n");
    for (key, value) in trailers {
        next.push_str(key);
        next.push_str(": ");
        next.push_str(value);
        next.push('\n');
    }
    std::fs::write(msg_path, next)
        .map_err(|err| format!("cannot write {}: {err}", msg_path.display()))
}

fn scope_for_tests(tests: &[String]) -> TestScope {
    let Some(path) = tests.first() else {
        return TestScope::Workspace;
    };
    integration_scope(path).unwrap_or(TestScope::Workspace)
}

fn integration_scope(path: &str) -> Option<TestScope> {
    let parts = path.split('/').collect::<Vec<_>>();
    if !(parts.len() == 4 && parts[0] == "crates" && parts[2] == "tests" && is_rust(parts[3])) {
        return None;
    }
    let target = &parts[3][..parts[3].len() - 3];
    Some(TestScope::Integration {
        package: parts[1].to_owned(),
        target: target.to_owned(),
    })
}

fn is_rust(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("rs"))
}

fn is_integration_test_path(path: &str) -> bool {
    path.starts_with("tests/") || integration_scope(path).is_some()
}

fn is_product_impl_path(path: &str) -> bool {
    path.starts_with("crates/") && path.contains("/src/") && is_rust(path)
}

fn declares_red_intent(subject: &str) -> bool {
    subject.starts_with("feat:")
        || subject.starts_with("fix:")
        || subject.starts_with("feat(")
        || subject.starts_with("fix(")
        || subject.starts_with("feat!:")
        || subject.starts_with("fix!:")
}

fn file_checksum(path: &Path) -> Result<String, String> {
    let bytes =
        std::fs::read(path).map_err(|err| format!("cannot read {}: {err}", path.display()))?;
    Ok(bytes_checksum(&bytes))
}

fn text_checksum(text: &str) -> String {
    bytes_checksum(text.as_bytes())
}

fn bytes_checksum(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let hex = digest
        .iter()
        .fold(String::with_capacity(64), |mut rendered, byte| {
            let _ = write!(rendered, "{byte:02x}");
            rendered
        });
    format!("sha256:{hex}")
}

fn utc_timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let (year, month, day, hour, minute, second) = unix_seconds_to_utc(seconds);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn unix_seconds_to_utc(seconds: u64) -> (i32, u32, u32, u32, u32, u32) {
    let days = i64::try_from(seconds / 86_400).unwrap_or(i64::MAX);
    let day_seconds = seconds % 86_400;
    let hour = u32::try_from(day_seconds / 3_600).unwrap_or(0);
    let minute = u32::try_from((day_seconds % 3_600) / 60).unwrap_or(0);
    let second = u32::try_from(day_seconds % 60).unwrap_or(0);
    let (year, month, day) = civil_from_days(days);
    (year, month, day, hour, minute, second)
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i32, u32, u32) {
    let days = days_since_unix_epoch + 719_468;
    let era = days / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (
        i32::try_from(year).unwrap_or(i32::MAX),
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

fn summary(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(400)
        .collect()
}

impl CommandOutput {
    fn combined(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::fs;
    use std::path::PathBuf;

    #[derive(Debug)]
    struct FakeRunner {
        git: RefCell<VecDeque<CommandOutput>>,
        cargo: RefCell<VecDeque<CommandOutput>>,
    }

    impl FakeRunner {
        fn new(git: Vec<CommandOutput>, cargo: Vec<CommandOutput>) -> Self {
            Self {
                git: RefCell::new(git.into()),
                cargo: RefCell::new(cargo.into()),
            }
        }
    }

    impl Runner for FakeRunner {
        fn git(&self, _args: &[&str]) -> Result<CommandOutput, String> {
            self.git
                .borrow_mut()
                .pop_front()
                .ok_or_else(|| "missing fake git output".to_owned())
        }

        fn cargo_test(&self, _scope: TestScope) -> Result<CommandOutput, String> {
            self.cargo
                .borrow_mut()
                .pop_front()
                .ok_or_else(|| "missing fake cargo output".to_owned())
        }
    }

    #[track_caller]
    fn check(condition: bool, context: &str) {
        assert!(condition, "{context}: condition was false");
    }

    fn temp_file(name: &str, text: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("console-rgr-{}-{name}", std::process::id()));
        let result = fs::write(&path, text);
        check(result.is_ok(), &format!("temp write failed: {result:?}"));
        path
    }

    fn temp_workspace_file(rel_path: &str, text: &str) -> PathBuf {
        let path = PathBuf::from(rel_path);
        let write = fs::write(&path, text);
        check(
            write.is_ok(),
            &format!("workspace temp write failed: {write:?}"),
        );
        path
    }

    #[test]
    #[should_panic(expected = "condition was false")]
    fn check_panics_with_context_when_condition_is_false() {
        check(false, "expected panic");
    }

    #[test]
    fn parity_fixture_names_upstream_pin_and_decision_vectors() {
        let parsed: Result<serde_json::Value, serde_json::Error> = serde_json::from_str(
            include_str!("../../../tests/fixtures/red-green-replay-parity-vectors.json"),
        );
        check(parsed.is_ok(), &format!("fixture must parse: {parsed:?}"));
        let fixture = parsed.unwrap_or(serde_json::Value::Null);
        assert_eq!(
            fixture["ported_from_upstream_commit"],
            "87fd400dae07f537df9a200d54b2f4dc44c42971"
        );
        assert!(PORTED_FROM_UPSTREAM.contains("red_green_replay.py"));
        assert_eq!(
            fixture["decision_vectors"].as_array().map(Vec::len),
            Some(6)
        );
    }

    #[test]
    fn classify_splits_rust_tests_and_product_impl() {
        let buckets = classify(&[
            "crates/console-cli/tests/flow.rs".to_owned(),
            "crates/console-cli/src/lib.rs".to_owned(),
            "crates/console-cli/src/tests/helpers.rs".to_owned(),
            "docs/example.rs".to_owned(),
        ]);
        assert_eq!(buckets.tests, ["crates/console-cli/tests/flow.rs"]);
        assert_eq!(
            buckets.impls,
            [
                "crates/console-cli/src/lib.rs",
                "crates/console-cli/src/tests/helpers.rs"
            ]
        );
    }

    #[test]
    fn decision_tree_covers_pass_red_reject_green_and_suite_green() {
        let empty = Buckets {
            tests: Vec::new(),
            impls: Vec::new(),
        };
        let tests = Buckets {
            tests: vec!["crates/x/tests/red.rs".to_owned()],
            impls: Vec::new(),
        };
        let impls = Buckets {
            tests: Vec::new(),
            impls: vec!["crates/x/src/lib.rs".to_owned()],
        };
        assert_eq!(decide("docs: x", &empty, false, false), Decision::Pass);
        assert_eq!(decide("chore: x", &tests, false, false), Decision::Red);
        assert_eq!(
            decide("feat: x", &tests, false, true),
            Decision::TestPassedAtRedReject
        );
        assert_eq!(decide("feat: x", &tests, false, false), Decision::Red);
        assert_eq!(decide("test: x", &tests, false, true), Decision::SuiteGreen);
        assert_eq!(decide("feat: x", &impls, true, false), Decision::Green);
        assert_eq!(
            decide("feat: x", &impls, false, false),
            Decision::SuiteGreen
        );
    }

    #[test]
    fn red_leg_writes_red_trailers_when_targeted_cargo_test_fails() {
        let test_path = temp_file("red.rs", "fn x() {}\n");
        let msg_path = temp_file("msg-red", "feat: x\n");
        let test_rel = test_path.to_string_lossy().into_owned();
        let runner = FakeRunner::new(Vec::new(), vec![CommandOutput::failure("failed")]);
        let result = handle_red(&runner, &msg_path, std::slice::from_ref(&test_rel));
        check(result.is_ok(), &format!("red leg should pass: {result:?}"));
        let msg = fs::read_to_string(&msg_path);
        assert!(
            msg.as_ref()
                .is_ok_and(|text| text.contains("TDD-Red-Test:"))
        );
        assert!(msg.as_ref().is_ok_and(|text| text.contains(&test_rel)));
        let captured_at = msg
            .as_ref()
            .ok()
            .and_then(|text| trailer_value(text, "TDD-Red-Captured-At"));
        assert!(captured_at.is_some_and(|value| value.ends_with('Z') && value != "now"));
        let _ = fs::remove_file(test_path);
        let _ = fs::remove_file(msg_path);
    }

    #[test]
    fn red_leg_rejects_multiple_tests_and_passing_red_test() {
        let msg_path = temp_file("msg-red-reject", "feat: x\n");
        let runner = FakeRunner::new(Vec::new(), vec![CommandOutput::success("ok")]);
        let multi = handle_red(&runner, &msg_path, &["a.rs".to_owned(), "b.rs".to_owned()]);
        assert!(multi.is_err_and(|err| err.contains("multi-test-file")));
        let test_path = temp_file("passing.rs", "fn x() {}\n");
        let passing = handle_red(
            &runner,
            &msg_path,
            &[test_path.to_string_lossy().into_owned()],
        );
        assert!(passing.is_err_and(|err| err.contains("test-passed-at-red")));
        let none = handle_red(&runner, &msg_path, &[]);
        assert!(none.is_err_and(|err| err.contains("without-test")));
        let _ = fs::remove_file(test_path);
        let _ = fs::remove_file(msg_path);
    }

    #[test]
    fn red_leg_propagates_targeted_cargo_runner_failure() {
        let test_path = temp_file("red-cargo-error.rs", "fn x() {}\n");
        let msg_path = temp_file("msg-red-cargo-error", "feat: x\n");
        let runner = FakeRunner::new(Vec::new(), Vec::new());
        let result = handle_red(
            &runner,
            &msg_path,
            &[test_path.to_string_lossy().into_owned()],
        );
        assert!(result.is_err_and(|err| err.contains("missing fake cargo output")));
        let _ = fs::remove_file(test_path);
        let _ = fs::remove_file(msg_path);
    }

    #[test]
    fn green_leg_checks_same_test_then_writes_green_trailers() {
        let test_path = temp_file("green.rs", "fn x() {}\n");
        let msg_path = temp_file("msg-green", "fix: x\n");
        let checksum = file_checksum(&test_path);
        check(
            checksum.is_ok(),
            &format!("checksum should compute: {checksum:?}"),
        );
        let runner = FakeRunner::new(
            vec![
                CommandOutput::success(&test_path.to_string_lossy()),
                CommandOutput::success(&checksum.unwrap_or_default()),
                CommandOutput::success("abc123"),
            ],
            vec![CommandOutput::success("pass")],
        );
        let result = handle_green(&runner, &msg_path);
        check(
            result.is_ok(),
            &format!("green leg should pass: {result:?}"),
        );
        let msg = fs::read_to_string(&msg_path);
        assert!(
            msg.as_ref()
                .is_ok_and(|text| text.contains("TDD-Green-Verified-At:"))
        );
        let verified_at = msg
            .as_ref()
            .ok()
            .and_then(|text| trailer_value(text, "TDD-Green-Verified-At"));
        assert!(verified_at.is_some_and(|value| value.ends_with('Z') && value != "now"));
        let _ = fs::remove_file(test_path);
        let _ = fs::remove_file(msg_path);
    }

    #[test]
    fn green_leg_rejects_checksum_mismatch_and_still_failing_test() {
        let test_path = temp_file("green-reject.rs", "fn x() {}\n");
        let msg_path = temp_file("msg-green-reject", "fix: x\n");
        let mismatch = FakeRunner::new(
            vec![
                CommandOutput::success(&test_path.to_string_lossy()),
                CommandOutput::success("sha256:not-it"),
            ],
            Vec::new(),
        );
        assert!(handle_green(&mismatch, &msg_path).is_err_and(|err| err.contains("checksum")));
        let checksum = file_checksum(&test_path);
        check(
            checksum.is_ok(),
            &format!("checksum should compute: {checksum:?}"),
        );
        let failing = FakeRunner::new(
            vec![
                CommandOutput::success(&test_path.to_string_lossy()),
                CommandOutput::success(&checksum.unwrap_or_default()),
            ],
            vec![CommandOutput::failure("still red")],
        );
        assert!(handle_green(&failing, &msg_path).is_err_and(|err| err.contains("still-failing")));
        let _ = fs::remove_file(test_path);
        let _ = fs::remove_file(msg_path);
    }

    #[test]
    fn green_leg_propagates_runner_failures_for_trailers_cargo_and_parent() {
        let test_path = temp_file("green-runner-failures.rs", "fn x() {}\n");
        let msg_path = temp_file("msg-green-runner-failures", "fix: x\n");
        let checksum = file_checksum(&test_path).unwrap_or_default();

        let missing_test_trailer = FakeRunner::new(Vec::new(), Vec::new());
        assert!(
            handle_green(&missing_test_trailer, &msg_path)
                .is_err_and(|err| err.contains("missing fake git output"))
        );

        let missing_checksum_trailer = FakeRunner::new(
            vec![CommandOutput::success(&test_path.to_string_lossy())],
            Vec::new(),
        );
        assert!(
            handle_green(&missing_checksum_trailer, &msg_path)
                .is_err_and(|err| err.contains("missing fake git output"))
        );

        let cargo_error = FakeRunner::new(
            vec![
                CommandOutput::success(&test_path.to_string_lossy()),
                CommandOutput::success(&checksum),
            ],
            Vec::new(),
        );
        assert!(
            handle_green(&cargo_error, &msg_path)
                .is_err_and(|err| err.contains("missing fake cargo output"))
        );

        let parent_error = FakeRunner::new(
            vec![
                CommandOutput::success(&test_path.to_string_lossy()),
                CommandOutput::success(&checksum),
            ],
            vec![CommandOutput::success("pass")],
        );
        assert!(
            handle_green(&parent_error, &msg_path)
                .is_err_and(|err| err.contains("missing fake git output"))
        );

        let _ = fs::remove_file(test_path);
        let _ = fs::remove_file(msg_path);
    }

    #[test]
    fn green_leg_reports_missing_current_test_file() {
        let missing_test_path = std::env::temp_dir().join(format!(
            "console-rgr-{}-missing-green.rs",
            std::process::id()
        ));
        let msg_path = temp_file("msg-green-missing-file", "fix: x\n");
        let runner = FakeRunner::new(
            vec![
                CommandOutput::success(&missing_test_path.to_string_lossy()),
                CommandOutput::success("sha256:not-read"),
            ],
            Vec::new(),
        );
        assert!(handle_green(&runner, &msg_path).is_err_and(|err| err.contains("cannot read")));
        let _ = fs::remove_file(msg_path);
    }

    #[test]
    fn suite_green_writes_suite_trailers_only_when_full_cargo_test_passes() {
        let msg_path = temp_file("msg-suite", "chore: x\n");
        let green = FakeRunner::new(Vec::new(), vec![CommandOutput::success("pass")]);
        assert!(handle_suite_green(&green, &msg_path).is_ok());
        let msg = fs::read_to_string(&msg_path);
        assert!(
            msg.as_ref()
                .is_ok_and(|text| text.contains("TDD-Suite-Green-Captured-At:"))
        );
        let captured_at = msg
            .as_ref()
            .ok()
            .and_then(|text| trailer_value(text, "TDD-Suite-Green-Captured-At"));
        assert!(captured_at.is_some_and(|value| value.ends_with('Z') && value != "now"));
        let red = FakeRunner::new(Vec::new(), vec![CommandOutput::failure("fail")]);
        assert!(handle_suite_green(&red, &msg_path).is_err_and(|err| err.contains("suite-red")));
        let _ = fs::remove_file(msg_path);
    }

    #[test]
    fn suite_green_propagates_workspace_cargo_runner_failure() {
        let msg_path = temp_file("msg-suite-cargo-error", "chore: x\n");
        let runner = FakeRunner::new(Vec::new(), Vec::new());
        assert!(
            handle_suite_green(&runner, &msg_path)
                .is_err_and(|err| err.contains("missing fake cargo output"))
        );
        let _ = fs::remove_file(msg_path);
    }

    #[test]
    fn no_arg_range_rejects_unresolvable_base_and_missing_trailer_shape() {
        let missing_base = FakeRunner::new(vec![CommandOutput::failure("fatal")], Vec::new());
        assert!(validate_default_range(&missing_base).is_err_and(|err| err.contains("base")));
        let missing_trailers = FakeRunner::new(
            vec![
                CommandOutput::success("origin/master\n"),
                CommandOutput::success("abc\n"),
                CommandOutput::success("crates/x/src/lib.rs\n"),
                CommandOutput::success("feat: x\n"),
            ],
            Vec::new(),
        );
        assert!(
            validate_default_range(&missing_trailers)
                .is_err_and(|err| err.contains("missing-trailers"))
        );
    }

    #[test]
    fn no_arg_range_propagates_missing_commit_message_lookup() {
        let runner = FakeRunner::new(
            vec![
                CommandOutput::success("origin/master\n"),
                CommandOutput::success("abc\n"),
                CommandOutput::success("crates/x/src/lib.rs\n"),
            ],
            Vec::new(),
        );
        assert!(
            validate_default_range(&runner)
                .is_err_and(|err| err.contains("missing fake git output"))
        );
    }

    #[test]
    fn no_arg_range_accepts_pair_suite_and_non_product_commits() {
        let runner = FakeRunner::new(
            vec![
                CommandOutput::success("origin/master\n"),
                CommandOutput::success("pair\nsuite\nother\n"),
                CommandOutput::success("crates/x/src/lib.rs\n"),
                CommandOutput::success(
                    "fix\n\nTDD-Red-Test-File-Checksum: sha256:a\nTDD-Green-Verified-At: now\n",
                ),
                CommandOutput::success("crates/x/src/lib.rs\n"),
                CommandOutput::success("chore\n\nTDD-Suite-Green-Captured-At: now\n"),
                CommandOutput::success("docs/readme.md\n"),
            ],
            Vec::new(),
        );
        assert!(validate_default_range(&runner).is_ok());
    }

    #[test]
    fn head_state_distinguishes_unborn_not_red_red_and_completed_pair() {
        let unborn = FakeRunner::new(
            vec![CommandOutput {
                code: 1,
                stdout: String::new(),
                stderr: String::new(),
            }],
            Vec::new(),
        );
        assert_eq!(head_red_awaiting_green(&unborn), Ok(false));
        let failed_probe = FakeRunner::new(
            vec![CommandOutput {
                code: 128,
                stdout: String::new(),
                stderr: "fatal".to_owned(),
            }],
            Vec::new(),
        );
        assert!(
            head_red_awaiting_green(&failed_probe)
                .is_err_and(|err| err.contains("git-command-failed"))
        );
        let not_red = FakeRunner::new(
            vec![
                CommandOutput::success("head"),
                CommandOutput::success("message"),
            ],
            Vec::new(),
        );
        assert_eq!(head_red_awaiting_green(&not_red), Ok(false));
        let red = FakeRunner::new(
            vec![
                CommandOutput::success("head"),
                CommandOutput::success("TDD-Red-Test-File-Checksum: sha256:a\n"),
            ],
            Vec::new(),
        );
        assert_eq!(head_red_awaiting_green(&red), Ok(true));
        let pair = FakeRunner::new(
            vec![
                CommandOutput::success("head"),
                CommandOutput::success(
                    "TDD-Red-Test-File-Checksum: sha256:a\nTDD-Green-Verified-At: now\n",
                ),
            ],
            Vec::new(),
        );
        assert_eq!(head_red_awaiting_green(&pair), Ok(false));
    }

    #[test]
    fn commit_msg_propagates_runner_failures_after_staged_file_classification() {
        let msg_path = temp_file("msg-dispatch-runner-failures", "feat: x\n");
        let staged_files_error = FakeRunner::new(Vec::new(), Vec::new());
        assert!(
            check_commit_msg(&staged_files_error, &msg_path)
                .is_err_and(|err| err.contains("missing fake git output"))
        );

        let awaiting_error = FakeRunner::new(
            vec![CommandOutput::success("crates/x/src/lib.rs\n")],
            Vec::new(),
        );
        assert!(
            check_commit_msg(&awaiting_error, &msg_path)
                .is_err_and(|err| err.contains("missing fake git output"))
        );

        let suite_error = FakeRunner::new(
            vec![
                CommandOutput::success("crates/x/src/lib.rs\n"),
                CommandOutput::success("head\n"),
                CommandOutput::success("message\n"),
            ],
            Vec::new(),
        );
        assert!(
            check_commit_msg(&suite_error, &msg_path)
                .is_err_and(|err| err.contains("missing fake cargo output"))
        );
        let _ = fs::remove_file(msg_path);
    }

    #[test]
    fn commit_msg_mode_routes_all_outer_branches() {
        let msg_path = temp_file("msg-dispatch", "feat: x\n");
        let pass = FakeRunner::new(vec![CommandOutput::success("docs/readme.md\n")], Vec::new());
        assert!(check_commit_msg(&pass, &msg_path).is_ok());
        let red_path = format!("tests/dispatch-red-{}.rs", std::process::id());
        let _ = fs::create_dir_all("tests");
        let red_test = temp_workspace_file(&red_path, "fn x() {}\n");
        let red = FakeRunner::new(
            vec![CommandOutput::success(&format!("{red_path}\n"))],
            vec![
                CommandOutput::failure("red"),
                CommandOutput::failure("red again"),
            ],
        );
        assert!(check_commit_msg(&red, &msg_path).is_ok());
        let reject = FakeRunner::new(
            vec![CommandOutput::success("crates/x/tests/new.rs\n")],
            vec![CommandOutput::success("pass")],
        );
        assert!(
            check_commit_msg(&reject, &msg_path).is_err_and(|err| err.contains("passed-at-red"))
        );
        let suite = FakeRunner::new(
            vec![
                CommandOutput::success("crates/x/src/lib.rs\n"),
                CommandOutput::success("head\n"),
                CommandOutput::success("message\n"),
            ],
            vec![CommandOutput::success("pass")],
        );
        assert!(check_commit_msg(&suite, &msg_path).is_ok());
        let green_test = temp_file("dispatch-green.rs", "fn x() {}\n");
        let checksum = file_checksum(&green_test);
        check(
            checksum.is_ok(),
            &format!("checksum should compute: {checksum:?}"),
        );
        let green = FakeRunner::new(
            vec![
                CommandOutput::success("crates/x/src/lib.rs\n"),
                CommandOutput::success("head\n"),
                CommandOutput::success("TDD-Red-Test-File-Checksum: sha256:a\n"),
                CommandOutput::success(&green_test.to_string_lossy()),
                CommandOutput::success(&checksum.unwrap_or_default()),
                CommandOutput::success("parent"),
            ],
            vec![CommandOutput::success("pass")],
        );
        assert!(check_commit_msg(&green, &msg_path).is_ok());
        let git_failure = FakeRunner::new(
            vec![CommandOutput {
                code: 128,
                stdout: String::new(),
                stderr: "fatal".to_owned(),
            }],
            Vec::new(),
        );
        assert!(git_stdout(&git_failure, &["status"]).is_err_and(|err| err.contains("status")));
        assert_eq!(scope_for_tests(&[]), TestScope::Workspace);
        let _ = fs::remove_file(red_test);
        let _ = fs::remove_file(green_test);
        let _ = fs::remove_file(msg_path);
    }

    #[test]
    fn scope_for_integration_tests_targets_the_package_test_binary() {
        assert_eq!(
            scope_for_tests(&["crates/console-cli/tests/flow.rs".to_owned()]),
            TestScope::Integration {
                package: "console-cli".to_owned(),
                target: "flow".to_owned()
            }
        );
        assert_eq!(
            scope_for_tests(&["tests/root.rs".to_owned()]),
            TestScope::Workspace
        );
        assert_eq!(
            scope_for_tests(&["crates/console-cli/src/tests/helpers.rs".to_owned()]),
            TestScope::Workspace
        );
    }

    #[test]
    fn unix_timestamp_conversion_covers_january_february_month_mapping() {
        assert_eq!(unix_seconds_to_utc(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(unix_seconds_to_utc(2_678_400), (1970, 2, 1, 0, 0, 0));
    }

    fn trailer_value(text: &str, key: &str) -> Option<String> {
        text.lines()
            .find_map(|line| line.strip_prefix(&format!("{key}: ")))
            .map(str::to_owned)
    }

    #[test]
    fn failure_paths_are_fail_closed_and_actionable() {
        let missing_msg =
            std::env::temp_dir().join(format!("console-rgr-{}-missing-msg", std::process::id()));
        assert!(
            check_commit_msg(&FakeRunner::new(Vec::new(), Vec::new()), &missing_msg)
                .is_err_and(|err| err.contains("cannot read"))
        );

        let msg_path = temp_file("msg-failures", "feat: x\n");
        assert!(
            staged_files(&FakeRunner::new(Vec::new(), Vec::new()))
                .is_err_and(|err| err.contains("missing fake git output"))
        );
        let diff_failed = FakeRunner::new(
            vec![CommandOutput {
                code: 128,
                stdout: "crates/x/src/lib.rs\n".to_owned(),
                stderr: "fatal diff".to_owned(),
            }],
            Vec::new(),
        );
        assert!(
            staged_files(&diff_failed)
                .is_err_and(|err| err.contains("git diff --cached --name-only"))
        );
        assert!(
            validate_default_range(&FakeRunner::new(Vec::new(), Vec::new()))
                .is_err_and(|err| err.contains("missing fake git output"))
        );

        let rev_list_fails =
            FakeRunner::new(vec![CommandOutput::success("origin/master\n")], Vec::new());
        assert!(
            validate_default_range(&rev_list_fails)
                .is_err_and(|err| err.contains("missing fake git output"))
        );
        let rev_list_nonzero = FakeRunner::new(
            vec![
                CommandOutput::success("origin/master\n"),
                CommandOutput {
                    code: 128,
                    stdout: "abc\n".to_owned(),
                    stderr: "fatal rev-list".to_owned(),
                },
            ],
            Vec::new(),
        );
        assert!(
            validate_default_range(&rev_list_nonzero)
                .is_err_and(|err| err.contains("git rev-list --no-merges"))
        );
        let diff_tree_nonzero = FakeRunner::new(
            vec![
                CommandOutput::success("origin/master\n"),
                CommandOutput::success("abc\n"),
                CommandOutput {
                    code: 128,
                    stdout: "crates/x/src/lib.rs\n".to_owned(),
                    stderr: "fatal diff-tree".to_owned(),
                },
            ],
            Vec::new(),
        );
        assert!(
            validate_default_range(&diff_tree_nonzero)
                .is_err_and(|err| err.contains("git diff-tree --no-commit-id"))
        );

        let cargo_fails = FakeRunner::new(
            vec![CommandOutput::success("crates/x/tests/red.rs\n")],
            Vec::new(),
        );
        assert!(
            check_commit_msg(&cargo_fails, &msg_path)
                .is_err_and(|err| err.contains("missing fake cargo output"))
        );

        let head_log_fails = FakeRunner::new(vec![CommandOutput::success("head\n")], Vec::new());
        assert!(
            head_red_awaiting_green(&head_log_fails)
                .is_err_and(|err| err.contains("missing fake git output"))
        );

        assert!(
            handle_red(
                &FakeRunner::new(Vec::new(), Vec::new()),
                &msg_path,
                &["/definitely/missing/tests/red.rs".to_owned()]
            )
            .is_err_and(|err| err.contains("cannot read"))
        );
        assert!(
            write_trailers(&missing_msg, &[("TDD-Suite-Green-Scope", "full-suite")])
                .is_err_and(|err| err.contains("cannot read"))
        );
        assert!(
            write_trailers(
                Path::new("/proc/version"),
                &[("TDD-Suite-Green-Scope", "full-suite")]
            )
            .is_err_and(|err| err.contains("cannot write"))
        );
        assert_eq!(integration_scope("crates/console-cli/tests/flow"), None);
        let _ = fs::remove_file(msg_path);
    }
}
