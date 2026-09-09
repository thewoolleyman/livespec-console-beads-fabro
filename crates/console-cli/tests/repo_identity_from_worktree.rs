//! `resolve_console_repo` must name the REPOSITORY, not the working directory.
//!
//! Regression coverage for `livespec-console-beads-fabro-mx9u.21`: the maintainer
//! ran the console from inside a linked worktree
//! (`~/.worktrees/livespec-console-beads-fabro/<branch>`) and the "Repos
//! observed" pane minted a SECOND, phantom repo named after the worktree's
//! branch directory. `resolve_console_repo` fell back to the cwd's plain path
//! basename, which is the branch directory name for a linked worktree, not the
//! repository name.
//!
//! The fix reuses the pattern already proven in
//! `console-fork-drift-check::absolute_root`: `git rev-parse --path-format
//! =absolute --git-common-dir` resolves a linked worktree straight to the
//! PRIMARY checkout's `.git` directory, so the primary checkout's own
//! directory name is recoverable from any worktree of it. These tests build a
//! REAL git repository and a REAL linked worktree with the actual `git`
//! binary — a fixture-shaped fabricated path could not exercise the git
//! plumbing the fix depends on.

use std::path::{Path, PathBuf};
use std::process::Command;

use livespec_console_beads_fabro::resolve_console_repo;

type TestResult = Result<(), String>;

/// A scratch directory under the crate's own `target/`, unique per test
/// process, so parallel test binaries never collide on the same repo.
fn make_scratch_dir(label: &str) -> Result<PathBuf, String> {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp-repo-identity");
    let dir = base.join(format!("{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("create scratch dir {}: {error}", dir.display()))?;
    Ok(dir)
}

fn git(dir: &Path, args: &[&str]) -> TestResult {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .output()
        .map_err(|error| format!("failed to run git {args:?}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// Build a real primary checkout named `repo_name` under a scratch root, with
/// one commit (a repo with no commits has no valid HEAD to branch a worktree
/// from), and a real LINKED worktree checked out under it at
/// `<root>/.worktrees/<repo_name>/<branch>` -- the exact family layout
/// documented in this repo's own `CLAUDE.md`. Returns the primary checkout
/// path and the linked worktree path.
fn primary_and_worktree(
    root: &Path,
    repo_name: &str,
    branch: &str,
) -> Result<(PathBuf, PathBuf), String> {
    let primary = root.join(repo_name);
    std::fs::create_dir_all(&primary)
        .map_err(|error| format!("create primary checkout dir {}: {error}", primary.display()))?;
    git(&primary, &["init", "--quiet", "--initial-branch=main"])?;
    let readme = primary.join("README.md");
    std::fs::write(&readme, "scratch fixture\n")
        .map_err(|error| format!("write {}: {error}", readme.display()))?;
    git(&primary, &["add", "README.md"])?;
    git(&primary, &["commit", "--quiet", "-m", "initial"])?;

    let worktree = root.join(".worktrees").join(repo_name).join(branch);
    let worktree_parent = worktree
        .parent()
        .ok_or_else(|| format!("worktree path {} has no parent", worktree.display()))?;
    std::fs::create_dir_all(worktree_parent)
        .map_err(|error| format!("create {}: {error}", worktree_parent.display()))?;
    let worktree_str = worktree
        .to_str()
        .ok_or_else(|| format!("worktree path {} is not valid UTF-8", worktree.display()))?;
    git(&primary, &["worktree", "add", "-b", branch, worktree_str])?;

    Ok((primary, worktree))
}

#[test]
fn running_from_a_worktree_directory_resolves_the_repository_name() -> TestResult {
    let root = make_scratch_dir("worktree-identity")?;
    let (_primary, worktree) =
        primary_and_worktree(&root, "livespec-console-beads-fabro", "fix/mx9u21-example")?;

    // The worktree directory's own basename is the BRANCH's leaf segment, not
    // the repository name -- exactly the shape that used to mint a phantom
    // second "repo".
    assert_eq!(
        worktree.file_name().and_then(|name| name.to_str()),
        Some("mx9u21-example"),
        "fixture sanity: the worktree leaf must NOT already equal the repo name"
    );

    let resolved = resolve_console_repo(None, Some(&worktree));

    assert_eq!(
        resolved, "livespec-console-beads-fabro",
        "identity must be derived from the repository the worktree belongs to, \
         not the worktree's own directory name"
    );
    Ok(())
}

#[test]
fn a_worktree_of_a_differently_named_repo_still_resolves_to_that_repos_name() -> TestResult {
    // A second, differently-named repository, so the assertion above cannot be
    // satisfied by coincidence (e.g. a stray fallback that always returns the
    // fixed default `livespec-console-beads-fabro`).
    let root = make_scratch_dir("worktree-identity-other")?;
    let (_primary, worktree) = primary_and_worktree(&root, "some-other-repo", "chore/tidy")?;

    let resolved = resolve_console_repo(None, Some(&worktree));

    assert_eq!(resolved, "some-other-repo");
    Ok(())
}

#[test]
fn the_explicit_env_override_still_wins_over_a_worktree_path() -> TestResult {
    let root = make_scratch_dir("worktree-identity-override")?;
    let (_primary, worktree) =
        primary_and_worktree(&root, "livespec-console-beads-fabro", "fix/mx9u21-override")?;

    let resolved = resolve_console_repo(Some("explicit-override"), Some(&worktree));

    assert_eq!(resolved, "explicit-override");
    Ok(())
}

#[test]
fn a_non_git_directory_still_falls_back_to_the_plain_basename() -> TestResult {
    // Not every caller runs inside a git checkout at all (a packaged install, a
    // container with no `.git`). The git-aware resolution must degrade to the
    // pre-existing plain-basename behaviour rather than erroring or blanking
    // out the identity.
    let root = make_scratch_dir("non-git-fallback")?;
    let plain = root.join("livespec-console-beads-fabro");
    std::fs::create_dir_all(&plain)
        .map_err(|error| format!("create {}: {error}", plain.display()))?;

    let resolved = resolve_console_repo(None, Some(&plain));

    assert_eq!(resolved, "livespec-console-beads-fabro");
    Ok(())
}

#[test]
fn running_from_the_primary_checkout_itself_still_resolves_its_own_name() -> TestResult {
    // The primary checkout is its own git-common-dir parent, so the git-aware
    // path must be a no-op for the common case, not just for worktrees.
    let root = make_scratch_dir("primary-checkout-identity")?;
    let (primary, _worktree) =
        primary_and_worktree(&root, "livespec-console-beads-fabro", "fix/unused")?;

    let resolved = resolve_console_repo(None, Some(&primary));

    assert_eq!(resolved, "livespec-console-beads-fabro");
    Ok(())
}
