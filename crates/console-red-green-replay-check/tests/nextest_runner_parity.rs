//! Parity guard for livespec-console-beads-fabro-pzbdbo.36.
//!
//! Before this fix, the commit-msg hook (`console-red-green-replay-check`'s
//! `ProcessRunner`) ran the workspace suite under plain `cargo test` while
//! `just check` (and therefore CI) ran it under `cargo nextest run` — two
//! runners that do not agree about what environment they hand a test
//! process. Nothing reported the disagreement: CI stayed green while the
//! commit-msg hook refused every commit.
//!
//! The fix makes both layers run the SAME command,
//! [`nextest_command_args`], rather than teaching each layer the invocation
//! separately (which is how they drifted apart the first time). This test is
//! the guard against that drift recurring: it reads the actual `justfile`
//! recipe text and compares it, byte-for-byte, against what
//! `nextest_command_args` produces. If either side changes without the
//! other, the assertion names BOTH sides explicitly, not just one runner's
//! raw output.

use std::path::PathBuf;

use console_red_green_replay_check::{TestScope, nextest_command_args};

fn repo_root() -> PathBuf {
    // This test binary is compiled from
    // crates/console-red-green-replay-check/tests/, two levels below the
    // repository root.
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_justfile() -> Result<String, String> {
    let path = repo_root().join("justfile");
    std::fs::read_to_string(&path).map_err(|err| format!("cannot read {}: {err}", path.display()))
}

/// The trimmed `cargo nextest run ...` invocation line from the `justfile`'s
/// `check-nextest:` recipe body.
///
/// Fails loudly, naming what it found, if the recipe or the invocation line
/// inside it has moved — a structural change there is exactly the kind of
/// drift this test exists to catch, so silently passing through would defeat
/// the point.
fn justfile_check_nextest_invocation(justfile: &str) -> Result<String, String> {
    let mut lines = justfile.lines();
    if !lines.by_ref().any(|line| line == "check-nextest:") {
        return Err(
            "justfile no longer has a `check-nextest:` recipe at column 0 — update this test \
             (and re-verify the commit-msg hook still agrees with whatever replaced it) rather \
             than deleting the parity check"
                .to_owned(),
        );
    }
    let body: Vec<&str> = lines
        .take_while(|line| line.is_empty() || line.starts_with(char::is_whitespace))
        .collect();
    body.iter()
        .map(|line| line.trim())
        .find(|line| line.starts_with("cargo nextest run"))
        .map(str::to_owned)
        .ok_or_else(|| {
            format!(
                "justfile's check-nextest recipe body no longer contains a `cargo nextest run` \
                 line (body was: {body:#?}) — the commit-msg hook's ProcessRunner and this \
                 recipe must run the SAME command; update `nextest_command_args` in \
                 crates/console-red-green-replay-check/src/lib.rs to match whatever replaced it"
            )
        })
}

#[test]
fn commit_hook_and_just_check_run_the_same_nextest_invocation() -> Result<(), String> {
    let justfile = read_justfile()?;
    let recipe_invocation = justfile_check_nextest_invocation(&justfile)?;

    let hook_args = nextest_command_args(&TestScope::Workspace);
    let hook_invocation = format!("cargo {}", hook_args.join(" "));

    assert_eq!(
        recipe_invocation, hook_invocation,
        "commit-msg hook / just-check nextest invocation MISMATCH (livespec-console-beads-fabro-\
         pzbdbo.36): justfile's `check-nextest` recipe runs `{recipe_invocation}`, but \
         console-red-green-replay-check's ProcessRunner (nextest_command_args) would run \
         `{hook_invocation}`. These MUST match byte-for-byte — a commit-msg hook that disagrees \
         with `just check`/CI about which test runner sees which environment can pass CI while \
         blocking every commit, or the reverse, with nothing reporting the disagreement. Update \
         whichever side changed to match the other."
    );
    Ok(())
}
