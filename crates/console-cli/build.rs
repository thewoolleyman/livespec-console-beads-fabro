//! Stamps the running binary's build identity at COMPILE time.
//!
//! `livespec-console-beads-fabro-mx9u.13`: a dogfood pass graded fixes that
//! were not in the running binary, because nothing on screen said which
//! commit or moment it was built from. The fix reads the build host's own
//! `git`/`date` here, once, and hands the result to the crate as two
//! `env!()`-embedded constants (`console_cli::build_identity::BUILD_GIT_SHA`
//! and `BUILD_TIMESTAMP`) -- never as a runtime read of the working tree, so
//! a binary copied to another host, or simply left running while its
//! checkout moves on underneath it, keeps reporting the commit it was
//! actually compiled from.
//!
//! `rerun-if-changed` is pointed at the resolved git dir's `HEAD` file (and,
//! when `HEAD` is a symbolic ref, the ref file it points at), so an ordinary
//! `cargo build` after a fresh commit or checkout picks up the new sha
//! without needing an unrelated source edit to trigger a rebuild.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    for watch in git_head_watch_paths() {
        println!("cargo:rerun-if-changed={}", watch.display());
    }

    let sha = git_short_sha().unwrap_or_else(|| "unknown".to_owned());
    let timestamp = build_timestamp();
    println!("cargo:rustc-env=CONSOLE_BUILD_GIT_SHA={sha}");
    println!("cargo:rustc-env=CONSOLE_BUILD_TIMESTAMP={timestamp}");
}

/// The short commit sha `HEAD` resolves to on the build host, or `None` when
/// `git` is unavailable or the checkout is not a git repository at all (a
/// source tarball, for instance).
fn git_short_sha() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8(output.stdout).ok()?;
    let sha = sha.trim();
    (!sha.is_empty()).then(|| sha.to_owned())
}

/// The build host's wall-clock time, UTC, ISO-8601. Shells `date` rather than
/// pulling in a time-formatting dependency purely for a build script that
/// runs once per compile on a host that already has `date`.
fn build_timestamp() -> String {
    Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map_or_else(|| "unknown".to_owned(), |stdout| stdout.trim().to_owned())
}

/// The git-internal files whose change should trigger a rebuild of this
/// crate: the resolved git dir's `HEAD`, and -- when `HEAD` is a symbolic ref
/// (the ordinary case, `ref: refs/heads/<branch>`) -- the ref file it points
/// at, since that is what actually moves on an ordinary commit.
///
/// Resolved via `git rev-parse --git-dir` rather than a hard-coded `.git`, so
/// this also works from a linked worktree, whose git dir lives under the
/// primary checkout's `.git/worktrees/<name>` rather than beside the
/// worktree itself.
fn git_head_watch_paths() -> Vec<PathBuf> {
    let Some(git_dir) = git_dir() else {
        return Vec::new();
    };
    let head_path = git_dir.join("HEAD");
    let mut watches = vec![head_path.clone()];
    if let Ok(head_contents) = std::fs::read_to_string(&head_path)
        && let Some(ref_path) = head_contents.trim().strip_prefix("ref: ")
    {
        watches.push(git_dir.join(ref_path));
    }
    watches
}

fn git_dir() -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?;
    let path = path.trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}
