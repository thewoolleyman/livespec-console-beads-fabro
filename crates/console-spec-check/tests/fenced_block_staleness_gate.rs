//! The fenced-block staleness gate at the SHIPPED boundary.
//!
//! Slice A proved the detector as a pure function. This asserts the check the
//! `just check` aggregate actually runs: `console-spec-check
//! --fenced-block-staleness`, over a throwaway git repository whose COMMITTED
//! spec carries the v048 amendment's pre-rewrite prose and whose WORKING TREE
//! carries the rewrite. That is precisely the state a reviewer had to sweep 24
//! fenced blocks by hand to catch, so the exit code is the whole point: it must
//! be non-zero while a block still says the old thing, and zero once the
//! amendment co-edited its blocks.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PREVIOUS: &str = include_str!("data/staleness/v048-nightly-filing-previous.md");
const CURRENT: &str = include_str!("data/staleness/v048-nightly-filing-current.md");

/// The spec path the fixture stands in for. The fixture lives under
/// `tests/data/` and is copied to this path INSIDE the throwaway repository —
/// it never enters this repository's own counted spec tree.
const SPEC_PATH: &str = "SPECIFICATION/non-functional-requirements.md";

/// The stale quality-gate pyramid node, from the slice-A fixtures.
const STALE_PYRAMID_NODE: &str = "finding -> top-ranked chore work-item; never fail master";
/// The stale Contributor Scenario C Gherkin filing step.
const STALE_GHERKIN_STEP: &str =
    "And a chore work-item is filed at the top of the rank order in the";

fn checker() -> Result<PathBuf, String> {
    std::env::var_os("CARGO_BIN_EXE_console-spec-check")
        .map(PathBuf::from)
        .ok_or_else(|| "CARGO_BIN_EXE_console-spec-check must be set by cargo test".to_string())
}

#[test]
fn the_gate_fails_and_names_the_file_the_block_range_and_the_removed_term() -> Result<(), String> {
    let root = repository("stale", CURRENT)?;

    let output = gate(&root)?;

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        !output.status.success(),
        "a stale fenced block must fail the gate; stderr: {stderr}"
    );
    assert!(
        stderr.contains(SPEC_PATH),
        "the finding names the offending file: {stderr}"
    );
    for term in ["top rank", "rank order", "capture surface"] {
        assert!(
            stderr.contains(&format!(":: {term}")),
            "the finding names the removed term `{term}` that matched: {stderr}"
        );
    }
    for (start, end) in reported_ranges(&stderr) {
        assert!(
            is_fence_line(CURRENT, start) && is_fence_line(CURRENT, end),
            "lines {start}-{end} are the block's opening and closing fences"
        );
    }
    assert!(
        !reported_ranges(&stderr).is_empty(),
        "at least one block range is reported: {stderr}"
    );
    Ok(())
}

#[test]
fn the_gate_passes_when_the_amendment_co_edited_its_blocks() -> Result<(), String> {
    let root = repository("clean", &co_edited())?;

    let output = gate(&root)?;

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "an amendment that co-edited its blocks is clean; stderr: {stderr}"
    );
    assert!(
        stderr.contains("fenced-block staleness clean"),
        "the clean run says so: {stderr}"
    );
    Ok(())
}

/// The v048 amendment as it was finally ratified: the same prose rewrite, with
/// the pyramid node and the Gherkin filing step rewritten alongside it.
fn co_edited() -> String {
    CURRENT
        .replace(
            STALE_PYRAMID_NODE,
            "finding -> chore filed unranked at beads open; never fail master",
        )
        .replace(
            STALE_GHERKIN_STEP,
            "And a chore work-item is filed in the livespec-console-beads-fabro",
        )
        .replace(
            "      livespec-console-beads-fabro tenant through the orchestrator's\n      capture surface",
            "      tenant through the tailnet beads write ingress",
        )
}

/// A throwaway git repository whose committed spec is [`PREVIOUS`] and whose
/// working tree carries `working_tree`.
fn repository(label: &str, working_tree: &str) -> Result<PathBuf, String> {
    let root = temp_root(label).map_err(|error| format!("cannot create the temp root: {error}"))?;
    let spec = root.join(SPEC_PATH);
    let parent = spec
        .parent()
        .ok_or_else(|| format!("{SPEC_PATH} has no parent directory"))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    write(&spec, PREVIOUS)?;

    git(&root, &["init", "--quiet", "--initial-branch=master"])?;
    git(&root, &["add", "--all"])?;
    git(
        &root,
        &[
            "-c",
            "user.name=fenced-block gate",
            "-c",
            "user.email=gate@example.invalid",
            "commit",
            "--quiet",
            "--message",
            "the spec before the amendment",
        ],
    )?;

    write(&spec, working_tree)?;
    Ok(root)
}

fn write(path: &Path, content: &str) -> Result<(), String> {
    fs::write(path, content).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

/// Run the shipped check in `root`, in its fenced-block staleness mode.
fn gate(root: &Path) -> Result<Output, String> {
    Command::new(checker()?)
        .arg("--fenced-block-staleness")
        .current_dir(root)
        .output()
        .map_err(|error| format!("cannot run the checker: {error}"))
}

fn git(root: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|error| format!("cannot run git {}: {error}", args.join(" ")))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    ))
}

/// Every `lines <start>-<end>` range the diagnostics report.
fn reported_ranges(stderr: &str) -> Vec<(usize, usize)> {
    stderr
        .lines()
        .filter_map(|line| line.split_once("] lines "))
        .filter_map(|(_, rest)| rest.split_once(" :: "))
        .filter_map(|(range, _)| range.split_once('-'))
        .filter_map(|(start, end)| Some((start.parse().ok()?, end.parse().ok()?)))
        .collect()
}

/// Whether 1-based `number` in `text` is a fence line.
fn is_fence_line(text: &str, number: usize) -> bool {
    text.split('\n')
        .nth(number.wrapping_sub(1))
        .is_some_and(|line| line.trim_start().starts_with("```"))
}

fn temp_root(label: &str) -> std::io::Result<PathBuf> {
    let root = std::env::temp_dir().join(format!(
        "console-spec-check-gate-{label}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root)?;
    Ok(root)
}
