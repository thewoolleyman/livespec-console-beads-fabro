//! `console-fork-drift-check` — the gate over the committed
//! `.fabro/workflows/implement-work-item/` fork.
//!
//! Run from the repository root (where `just check` runs). Exits non-zero NAMING
//! every file that drifted. See the crate docs for why this pins upstream digests
//! rather than asserting byte-equality against an allowlist.
//!
//! `--refresh` re-reads the live upstream and rewrites the pin manifest,
//! preserving each pin's `reason`. Driven by `just refresh-fork-upstream-pins`.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use console_fork_drift_check::{
    FORK_DIR, PIN_MANIFEST, check_fork_against_pinned_release, check_local, currency, digest,
    digest_for_pin, parse_pinned_release, parse_pins, resolve_upstream_dir,
};

fn main() -> ExitCode {
    let refresh = std::env::args().any(|arg| arg == "--refresh");
    let repo_root = PathBuf::from(".");
    let Ok(home) = std::env::var("HOME") else {
        eprintln!("console-fork-drift-check: HOME is unset; cannot resolve the plugin cache");
        return ExitCode::FAILURE;
    };
    let home = PathBuf::from(home);
    let result = if refresh {
        refresh_pins(&repo_root, &home)
    } else {
        run(&repo_root, &home)
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

/// The PRIMARY checkout's absolute path, for matching `projectPath`.
///
/// `installed_plugins.json` keys plugin installs on the primary checkout, never
/// on a linked worktree — and all tracked-file work here happens in worktrees
/// under `~/.worktrees/`, so matching the cwd would miss every time. `git
/// rev-parse --git-common-dir` resolves to the primary's `.git` from inside any
/// worktree; its parent is the primary checkout.
fn absolute_root(repo_root: &Path) -> PathBuf {
    let fallback = || std::fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .current_dir(repo_root)
        .output()
    else {
        return fallback();
    };
    if !output.status.success() {
        return fallback();
    }
    let Ok(text) = String::from_utf8(output.stdout) else {
        return fallback();
    };
    Path::new(text.trim())
        .parent()
        .map_or_else(fallback, |primary| {
            std::fs::canonicalize(primary).unwrap_or_else(|_| primary.to_path_buf())
        })
}

fn run(repo_root: &Path, home: &Path) -> Result<(), String> {
    let manifest_path = repo_root.join(PIN_MANIFEST);
    let manifest = std::fs::read_to_string(&manifest_path)
        .map_err(|err| format!("cannot read {}: {err}", manifest_path.display()))?;
    let pins = parse_pins(&manifest)?;
    let pinned_release = parse_pinned_release(&manifest)?;
    let fork_dir = repo_root.join(FORK_DIR);

    let mut findings = check_local(&pins, &fork_dir);
    findings.extend(check_fork_against_pinned_release(
        &pins,
        &fork_dir,
        &pinned_release,
    ));

    // The verdict names the release it was taken against, and that release comes
    // from the COMMITTED fixture — never from whatever the host installed. The
    // installed build is reported beside it as currency, never as a verdict:
    // a Factory dispatch runs the console's committed fork, so an install that
    // has moved past the pin is a currency signal and never a correctness one.
    println!(
        "console-fork-drift-check: {} pinned file(s) judged against orchestrator release \
         {pinned_release}, as pinned in {PIN_MANIFEST}",
        pins.len()
    );
    println!(
        "{}",
        currency(
            &pinned_release,
            &resolve_upstream_dir(home, &absolute_root(repo_root))
        )
        .render()
    );

    if findings.is_empty() {
        println!("console-fork-drift-check: fork in lockstep with its pins");
        return Ok(());
    }
    let mut report = format!(
        "console-fork-drift-check: the committed .fabro fork does not match orchestrator \
         release {pinned_release}, which {PIN_MANIFEST} pins\n"
    );
    for finding in &findings {
        report.push_str("  - ");
        report.push_str(&finding.render());
        report.push('\n');
    }
    report.push_str(
        "\nThis gate exists because upstream once fixed the pr-stage publish leg and our\n\
         fork silently kept the broken one for three weeks (bd-ib-qq7f / PR #905).\n\
         It judges COMMITTED state only, so this verdict is the same on every host:\n\
         an orchestrator release that has moved past the pin is reported above as\n\
         currency and is never this failure.\n",
    );
    Err(report)
}

fn refresh_pins(repo_root: &Path, home: &Path) -> Result<(), String> {
    let manifest_path = repo_root.join(PIN_MANIFEST);
    let existing = std::fs::read_to_string(&manifest_path).unwrap_or_default();
    let previous = parse_pins(&existing).unwrap_or_default();
    let (upstream_dir, version) = resolve_upstream_dir(home, &absolute_root(repo_root))?;
    let fork_dir = repo_root.join(FORK_DIR);

    // The union of both sides, so a file present in only one is still recorded.
    let mut paths: Vec<String> = walk(&upstream_dir);
    for path in walk(&fork_dir) {
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths.sort();

    let entries: Vec<String> = paths
        .iter()
        .map(|path| {
            let upstream_sha = std::fs::read(upstream_dir.join(path))
                .ok()
                .map(|bytes| upstream_sha_field(path, &bytes));
            let present_in_fork = fork_dir.join(path).is_file();
            let recorded = previous.iter().find(|pin| &pin.path == path);
            let reason = recorded.map_or_else(
                || "TODO: explain this divergence".to_owned(),
                |pin| pin.reason.clone(),
            );
            // A recorded divergence is carried FORWARD. Dropping it on a refresh
            // would turn the next run red on a difference someone had already
            // explained, which is how a gate teaches people to bypass it.
            let divergence = recorded
                .and_then(|pin| pin.fork_divergence.as_ref())
                .map_or_else(String::new, |divergence| {
                    format!(
                        "      \"fork_divergence\": {},\n",
                        serde_json::to_string(divergence).unwrap_or_else(|_| "\"\"".to_owned())
                    )
                });
            let (sha_key, sha_field) = upstream_sha.map_or_else(
                || ("upstream_sha256", "null".to_owned()),
                |(key, sha)| (key, format!("\"{sha}\"")),
            );
            format!(
                "    {{\n      \"path\": \"{path}\",\n      \"{sha_key}\": {sha_field},\n{divergence}      \"present_in_fork\": {present_in_fork},\n      \"reason\": {}\n    }}",
                serde_json::to_string(&reason).unwrap_or_else(|_| "\"\"".to_owned())
            )
        })
        .collect();

    let rendered = format!(
        "{{\n  \"_comment\": \"Upstream digests for the committed .fabro fork. Regenerate with `just refresh-fork-upstream-pins`; every entry MUST carry a reason. See crates/console-fork-drift-check/src/lib.rs.\",\n  \"upstream_plugin_version\": \"{version}\",\n  \"files\": [\n{}\n  ]\n}}\n",
        entries.join(",\n")
    );
    std::fs::write(&manifest_path, rendered)
        .map_err(|err| format!("cannot write {}: {err}", manifest_path.display()))?;
    println!(
        "console-fork-drift-check: re-pinned {} file(s) against plugin {version}",
        paths.len()
    );
    Ok(())
}

fn walk(dir: &Path) -> Vec<String> {
    fn recurse(base: &Path, current: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(current) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                recurse(base, &path, out);
            } else if let Ok(relative) = path.strip_prefix(base) {
                out.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = Vec::new();
    recurse(dir, dir, &mut out);
    out.sort();
    out
}

fn upstream_sha_field(path: &str, bytes: &[u8]) -> (&'static str, String) {
    if path == "workflow.toml" {
        return (
            "upstream_sha256_ignoring_docker_pin",
            digest_for_pin(path, bytes),
        );
    }
    ("upstream_sha256", digest(bytes))
}
