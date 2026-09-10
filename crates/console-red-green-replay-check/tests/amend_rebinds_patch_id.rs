//! An amend after a rebase re-mints its attestation, against REAL git
//! (livespec-console-beads-fabro-pzbdbo.38, AC2/AC3/AC4).
//!
//! # Why a real repository rather than a fake runner
//!
//! The unit tests queue canned git output, so they pin the checker's control
//! flow but cannot tell whether `HEAD^`, `git diff --cached <rev>` and
//! `git patch-id --stable` actually behave the way this fix assumes on a real
//! rebase — which is exactly where pzbdbo.37's FIRST design (a tree hash) was
//! wrong and its second (a patch-id) was right; both were only settled by
//! running real git. So this drives real commits, a real rebase and a real
//! amend, and compares the recorded trailer against
//! `git show HEAD | git patch-id --stable`, which is the comparison AC2 names.
//!
//! `cargo_test` is the one thing stubbed: the fixture repository is not a
//! cargo workspace, and what is under test is which PARENT the attestation
//! binds to, not whether a suite passes.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use console_red_green_replay_check::{
    CommandOutput, ProcessRunner, Runner, TestScope, check_commit_msg, validate_range_from,
};

/// Real git, stubbed cargo.
struct RealGitStubbedCargo {
    inner: ProcessRunner,
}

impl RealGitStubbedCargo {
    fn new(workdir: &Path) -> Self {
        Self {
            inner: ProcessRunner::with_workdir(workdir),
        }
    }
}

impl Runner for RealGitStubbedCargo {
    fn git(&self, args: &[&str]) -> Result<CommandOutput, String> {
        self.inner.git(args)
    }

    fn cargo_test(&self, _scope: TestScope) -> Result<CommandOutput, String> {
        Ok(CommandOutput::success("stubbed suite: pass"))
    }

    fn patch_id(&self, base: &str, target: &str) -> Result<CommandOutput, String> {
        self.inner.patch_id(base, target)
    }
}

fn run(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(repo)
        .env("GIT_EDITOR", "true")
        .args(args)
        .output()
        .map_err(|err| format!("spawn git {args:?}: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Like [`run`], but hands back whether the command succeeded instead of
/// failing the test — for the rebase, which may legitimately conflict.
fn try_run(repo: &Path, args: &[&str]) -> Result<bool, String> {
    let output = Command::new("git")
        .current_dir(repo)
        .env("GIT_EDITOR", "true")
        .args(args)
        .output()
        .map_err(|err| format!("spawn git {args:?}: {err}"))?;
    Ok(output.status.success())
}

fn write(repo: &Path, rel: &str, body: &str) -> Result<(), String> {
    let path = repo.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create {}: {err}", parent.display()))?;
    }
    std::fs::write(&path, body).map_err(|err| format!("write {}: {err}", path.display()))
}

/// `git show <rev> | git patch-id --stable`, the comparison AC2 names.
fn actual_patch_id(repo: &Path, rev: &str) -> Result<String, String> {
    let show = Command::new("git")
        .current_dir(repo)
        .args(["show", rev])
        .output()
        .map_err(|err| format!("spawn git show {rev}: {err}"))?;
    let mut child = Command::new("git")
        .current_dir(repo)
        .args(["patch-id", "--stable"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|err| format!("spawn git patch-id: {err}"))?;
    {
        use std::io::Write as _;
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| "git patch-id stdin was not piped".to_owned())?;
        stdin
            .write_all(&show.stdout)
            .map_err(|err| format!("feed git patch-id: {err}"))?;
    }
    let out = child
        .wait_with_output()
        .map_err(|err| format!("git patch-id output: {err}"))?;
    Ok(String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned())
}

fn recorded_patch_id(message: &str) -> String {
    message
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix("TDD-Verified-Patch-Id: "))
        .unwrap_or_default()
        .to_owned()
}

fn scratch_repo(label: &str) -> Result<PathBuf, String> {
    let repo = std::env::temp_dir().join(format!("rgr-amend-{}-{label}", std::process::id()));
    let _ignored = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(&repo).map_err(|err| format!("create {}: {err}", repo.display()))?;
    run(&repo, &["init", "-q", "-b", "master"])?;
    run(&repo, &["config", "user.email", "probe@example.invalid"])?;
    run(&repo, &["config", "user.name", "probe"])?;
    run(&repo, &["config", "commit.gpgsign", "false"])?;
    Ok(repo)
}

/// AC2, on a real rebase: a branch commit's attestation goes stale when the
/// rebase moves its diff, and re-running the hook while amending re-mints a
/// trailer matching the amended commit's ACTUAL patch-id.
///
/// AC3 and AC4 ride the same scene: before the amend the carried trailer is
/// refused and the refusal names the remedy; after it, the range check accepts.
#[test]
fn an_amend_after_a_rebase_rebinds_the_attestation_to_the_amended_commit() -> Result<(), String> {
    let repo = scratch_repo("rebind")?;
    write(&repo, "crates/x/src/lib.rs", "pub fn one() {}\n")?;
    run(&repo, &["add", "-A"])?;
    run(&repo, &["commit", "-q", "-m", "chore: base"])?;
    run(&repo, &["checkout", "-q", "-b", "feature"])?;

    // The branch commit, attested exactly the way the hook attests one.
    write(
        &repo,
        "crates/x/src/lib.rs",
        "pub fn one() {}\npub fn feature() {}\n",
    )?;
    run(&repo, &["add", "-A"])?;
    let msg_path = repo.join("msg.txt");
    std::fs::write(&msg_path, "feat: the feature\n")
        .map_err(|err| format!("write message: {err}"))?;
    let runner = RealGitStubbedCargo::new(&repo);
    check_commit_msg(&runner, &msg_path)?;
    run(
        &repo,
        &["commit", "-q", "-F", &msg_path.display().to_string()],
    )?;
    let fresh = run(&repo, &["log", "-1", "--format=%B"])?;
    assert_eq!(
        recorded_patch_id(&fresh),
        actual_patch_id(&repo, "HEAD")?,
        "a freshly attested commit's trailer must match its own patch-id"
    );

    // Master gains a change to the SAME lines, so the rebase genuinely moves
    // this commit's patch-id. (An unrelated commit elsewhere would NOT move
    // it — that is the property pzbdbo.37 chose a patch-id for.)
    run(&repo, &["checkout", "-q", "master"])?;
    write(
        &repo,
        "crates/x/src/lib.rs",
        "pub fn one() {}\npub fn landed_on_master() {}\n",
    )?;
    run(&repo, &["add", "-A"])?;
    run(&repo, &["commit", "-q", "-m", "chore: master moves"])?;
    run(&repo, &["checkout", "-q", "feature"])?;
    if !try_run(&repo, &["rebase", "master"])? {
        // A conflicted rebase is a perfectly good version of this scene:
        // resolve it the way an author would, and continue.
        write(
            &repo,
            "crates/x/src/lib.rs",
            "pub fn one() {}\npub fn landed_on_master() {}\npub fn feature() {}\n",
        )?;
        run(&repo, &["add", "-A"])?;
        run(&repo, &["rebase", "--continue"])?;
    }

    // AC3, first direction: the carried-forward trailer no longer describes
    // this commit's diff, and the gate says so.
    let carried = run(&repo, &["log", "-1", "--format=%B"])?;
    assert_ne!(
        recorded_patch_id(&carried),
        actual_patch_id(&repo, "HEAD")?,
        "the rebase must have moved this commit's patch-id, or the scene proves nothing"
    );
    let refusal = validate_range_from(&runner, "master")
        .err()
        .ok_or_else(|| "a stale attestation must be refused".to_owned())?;
    assert!(
        refusal.contains("red-green-replay-range-missing-trailers"),
        "unexpected refusal: {refusal}"
    );
    // AC4: the refusal names the remedy rather than leaving the author to find
    // it by reading the checker.
    assert!(
        refusal.contains("git reset --soft HEAD~1"),
        "the refusal must name the remedy: {refusal}"
    );

    // The fix: re-run the hook while amending. The message is the commit's own
    // — trailers and all — which is what tells the checker this is an amend, so
    // it binds to HEAD^ rather than to the commit being replaced.
    std::fs::write(&msg_path, &carried).map_err(|err| format!("reuse the message: {err}"))?;
    check_commit_msg(&runner, &msg_path)?;
    run(
        &repo,
        &[
            "commit",
            "-q",
            "--amend",
            "-F",
            &msg_path.display().to_string(),
        ],
    )?;

    let amended = run(&repo, &["log", "-1", "--format=%B"])?;
    assert_eq!(
        recorded_patch_id(&amended),
        actual_patch_id(&repo, "HEAD")?,
        "the amended commit's trailer must match its own patch-id"
    );
    assert_eq!(
        amended.matches("TDD-Verified-Patch-Id:").count(),
        1,
        "one attestation per commit, rewritten in place: {amended}"
    );
    // AC3, second direction: with a matching trailer the range check accepts.
    assert_eq!(validate_range_from(&runner, "master"), Ok(()));
    let _ignored = std::fs::remove_dir_all(&repo);
    Ok(())
}

/// A message whose PROSE quotes a trailer name is a FRESH commit, not an amend.
/// Without this, a commit describing the ritual — the one that introduced this
/// very fix, for instance — would bind to `HEAD^` and be refused at push time
/// for a reason with nothing to do with its content.
#[test]
fn prose_quoting_a_trailer_name_does_not_make_a_commit_look_like_an_amend() -> Result<(), String> {
    let repo = scratch_repo("prose")?;
    write(&repo, "crates/x/src/lib.rs", "pub fn one() {}\n")?;
    run(&repo, &["add", "-A"])?;
    run(&repo, &["commit", "-q", "-m", "chore: base"])?;

    write(
        &repo,
        "crates/x/src/lib.rs",
        "pub fn one() {}\npub fn two() {}\n",
    )?;
    run(&repo, &["add", "-A"])?;
    let msg_path = repo.join("msg.txt");
    std::fs::write(
        &msg_path,
        "feat: describe the ritual\n\n\
         The hook records a TDD-Verified-Patch-Id: trailer at the moment\n\
         verification succeeds, and the range check recomputes it.\n\n\
         Co-Authored-By: Somebody <nobody@example.invalid>\n",
    )
    .map_err(|err| format!("write message: {err}"))?;
    let runner = RealGitStubbedCargo::new(&repo);
    check_commit_msg(&runner, &msg_path)?;
    run(
        &repo,
        &["commit", "-q", "-F", &msg_path.display().to_string()],
    )?;

    let message = run(&repo, &["log", "-1", "--format=%B"])?;
    assert_eq!(
        recorded_patch_id(&message),
        actual_patch_id(&repo, "HEAD")?,
        "a fresh commit whose prose mentions the trailer must still bind to HEAD"
    );
    let _ignored = std::fs::remove_dir_all(&repo);
    Ok(())
}
