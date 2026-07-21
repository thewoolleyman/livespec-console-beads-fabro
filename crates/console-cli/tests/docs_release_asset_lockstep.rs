//! The download commands in `docs/installing.md` must name assets the release
//! workflow actually publishes.
//!
//! # Why this gate exists
//!
//! `docs/installing.md` tells a user to fetch the binary with a `gh release
//! download --pattern` glob. That glob is prose: nothing connects it to
//! `release-binary.yml`, which is where the asset name is really constructed.
//! Rename the target triple, or the binary, or move the tag into a different
//! position, and the documented command silently stops matching — the user gets
//! `no assets match the file pattern` and no test ever complained.
//!
//! This is the same lockstep idea as `docs_status_hint_lockstep` (which binds
//! documented Status-line hints to the literals that render them), applied to
//! the release asset: every download glob the install doc quotes must match the
//! name the release workflow would upload.
//!
//! # Why this is hermetic
//!
//! It would be tempting to assert against the live GitHub Releases API — that
//! is, after all, where the real asset lives. It is deliberately NOT done: a
//! network call would make every CI run depend on GitHub availability and on
//! a release already existing, turning an unrelated outage into a red build.
//! Both halves of the binding are in-repo, so the drift that actually breaks
//! users — doc and workflow disagreeing — is catchable without leaving the
//! working tree.
//!
//! The B8 acceptance run verified the other half once, live: the globs below
//! matched the published `v0.2.0` assets when downloaded with the documented
//! commands.

use std::path::{Path, PathBuf};

/// The doc carrying the `gh release download` commands.
const INSTALL_DOC: &str = "docs/installing.md";
/// The workflow that builds and uploads the release assets.
const RELEASE_WORKFLOW: &str = ".github/workflows/release-binary.yml";

fn repo_root() -> std::io::Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
}

fn read(relative: &str) -> std::io::Result<String> {
    std::fs::read_to_string(repo_root()?.join(relative))
}

/// Every `--pattern '<glob>'` argument quoted in the install doc.
fn documented_patterns(doc: &str) -> Vec<String> {
    let mut patterns = Vec::new();
    for line in doc.lines() {
        let Some(after) = line.split_once("--pattern").map(|(_, rest)| rest) else {
            continue;
        };
        let trimmed = after.trim_start();
        let Some(quoted) = trimmed.strip_prefix('\'') else {
            continue;
        };
        let Some(close) = quoted.find('\'') else {
            continue;
        };
        patterns.push(quoted[..close].to_string());
    }
    patterns
}

/// The asset name the workflow uploads, reconstructed from its shell.
///
/// `release-binary.yml` builds the name from two assignments:
/// `target="…"` and `asset="…-${TAG}-${target}"`. This resolves them rather
/// than hard-coding the result, so renaming either one moves this expectation
/// with it — and the doc, which does NOT move, is what fails.
fn workflow_asset_name(workflow: &str, tag: &str) -> Option<String> {
    let assignment = |key: &str| -> Option<String> {
        workflow.lines().find_map(|line| {
            let rest = line.trim().strip_prefix(key)?.strip_prefix("=\"")?;
            let close = rest.find('"')?;
            Some(rest[..close].to_string())
        })
    };

    // Named `triple`, not `target`: a `"${target}"` literal alongside an
    // in-scope `target` binding trips `literal_string_with_formatting_args`.
    let triple = assignment("target")?;
    let asset = assignment("asset")?;
    Some(asset.replace("${TAG}", tag).replace("${target}", &triple))
}

/// Does `candidate` match a glob whose only metacharacter is `*`?
fn glob_matches(pattern: &str, candidate: &str) -> bool {
    let mut rest = candidate;
    let mut segments = pattern.split('*').peekable();

    let Some(first) = segments.next() else {
        return false;
    };
    let Some(stripped) = rest.strip_prefix(first) else {
        return false;
    };
    rest = stripped;

    // No `*` at all: the pattern is an equality test, so the whole candidate
    // must have been consumed by that single literal segment.
    if segments.peek().is_none() {
        return rest.is_empty();
    }

    while let Some(segment) = segments.next() {
        if segments.peek().is_none() {
            // Final segment must anchor to the end.
            return rest.ends_with(segment);
        }
        if segment.is_empty() {
            continue;
        }
        match rest.find(segment) {
            Some(at) => rest = &rest[at + segment.len()..],
            None => return false,
        }
    }
    true
}

#[test]
fn every_documented_download_pattern_matches_a_published_asset_name() -> std::io::Result<()> {
    let doc = read(INSTALL_DOC)?;
    let workflow = read(RELEASE_WORKFLOW)?;

    let tag = "v0.2.0";
    let binary = workflow_asset_name(&workflow, tag).unwrap_or_default();
    assert!(
        !binary.is_empty(),
        "could not recover the asset name from {RELEASE_WORKFLOW}; the `target=` / `asset=` \
         shell assignments this gate reads must still be present"
    );
    // The workflow uploads the binary and `${asset}.sha256` alongside it.
    let published = [binary.clone(), format!("{binary}.sha256")];

    let patterns = documented_patterns(&doc);
    assert!(
        patterns.len() >= 2,
        "expected {INSTALL_DOC} to document at least the binary and checksum download patterns, \
         got {patterns:#?}"
    );

    let unmatched: Vec<&String> = patterns
        .iter()
        .filter(|pattern| !published.iter().any(|name| glob_matches(pattern, name)))
        .collect();

    assert!(
        unmatched.is_empty(),
        "{INSTALL_DOC} documents `gh release download` patterns that match nothing \
         {RELEASE_WORKFLOW} publishes.\nA user following the doc would get \
         `no assets match the file pattern`.\nPublished for {tag}:\n{}\nUnmatched patterns:\n{}",
        published
            .iter()
            .map(|name| format!("  - {name}"))
            .collect::<Vec<_>>()
            .join("\n"),
        unmatched
            .iter()
            .map(|pattern| format!("  - {pattern}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    Ok(())
}

/// The checksum pattern must not also swallow the binary.
///
/// `gh release download` takes the union of its patterns, so a glob that
/// matched both assets would still "work" — right up until a reader copies the
/// binary-only command and silently gets the `.sha256` too.
#[test]
fn the_binary_pattern_does_not_match_the_checksum_asset() -> std::io::Result<()> {
    let doc = read(INSTALL_DOC)?;
    let workflow = read(RELEASE_WORKFLOW)?;

    let binary = workflow_asset_name(&workflow, "v0.2.0").unwrap_or_default();
    assert!(
        !binary.is_empty(),
        "asset name must be recoverable from {RELEASE_WORKFLOW}"
    );
    let checksum = format!("{binary}.sha256");

    let binary_patterns: Vec<String> = documented_patterns(&doc)
        .into_iter()
        .filter(|pattern| !pattern.ends_with(".sha256"))
        .collect();
    assert!(
        !binary_patterns.is_empty(),
        "{INSTALL_DOC} must document a binary download pattern"
    );

    for pattern in binary_patterns {
        assert!(
            glob_matches(&pattern, &binary),
            "the documented binary pattern `{pattern}` does not match `{binary}`"
        );
        assert!(
            !glob_matches(&pattern, &checksum),
            "the documented binary pattern `{pattern}` also matches `{checksum}`, so the \
             documented download command would fetch the checksum file as well"
        );
    }
    Ok(())
}

#[cfg(test)]
mod glob {
    use super::glob_matches;

    /// The negative tests: the drift this gate exists to catch.
    #[test]
    fn a_renamed_target_triple_stops_matching() {
        let documented = "livespec-console-beads-fabro-*-x86_64-unknown-linux-gnu";
        assert!(glob_matches(
            documented,
            "livespec-console-beads-fabro-v0.2.0-x86_64-unknown-linux-gnu"
        ));
        // The exact drift: the workflow moves to a musl baseline.
        assert!(!glob_matches(
            documented,
            "livespec-console-beads-fabro-v0.2.0-x86_64-unknown-linux-musl"
        ));
        // Or the binary is renamed.
        assert!(!glob_matches(
            documented,
            "livespec-console-v0.2.0-x86_64-unknown-linux-gnu"
        ));
    }

    #[test]
    fn a_suffix_glob_anchors_to_the_end() {
        assert!(glob_matches("a-*-gnu", "a-v1-gnu"));
        assert!(!glob_matches("a-*-gnu", "a-v1-gnu.sha256"));
        assert!(glob_matches("a-*-gnu.sha256", "a-v1-gnu.sha256"));
    }

    #[test]
    fn a_pattern_without_metacharacters_is_an_equality_test() {
        assert!(glob_matches("exact", "exact"));
        assert!(!glob_matches("exact", "exact-plus"));
    }
}
