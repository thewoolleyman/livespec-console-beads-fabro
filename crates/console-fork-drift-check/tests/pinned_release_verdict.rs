//! Contributor Scenario K — the fork-drift gate judges the committed workflow
//! fork against the orchestrator release its pin fixture names, at the SHIPPED
//! boundary: `console-fork-drift-check` run in a throwaway repository root,
//! against a throwaway `$HOME` carrying a plugin install.
//!
//! The defect this pins (work-item `livespec-console-beads-fabro-lavbfp`): the
//! gate took its verdict from whatever orchestrator build the HOST had
//! installed, so every orchestrator release that changed the workflow bundle
//! turned console `master` red on every host that ran `ensure-plugins`, with no
//! console commit at all — three hand recoveries through worktree, PR and merge
//! in three days (R316, R498, R620), each one blocking every push and every
//! post-merge janitor until a human re-pinned. The Dispatcher runs the console's
//! COMMITTED fork, never the installed bundle, so lag behind the newest release
//! is a CURRENCY matter and never a correctness one.
//!
//! The exit code is therefore the whole point, and it must be a function of
//! COMMITTED state alone: the pin fixture plus the fork, identical on every
//! host.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use console_fork_drift_check::digest;

/// Where the gate reads its pins, relative to the repository root.
const PIN_MANIFEST: &str = "tests/fixtures/fabro-fork-upstream-pins.json";
/// The forked workflow directory, relative to a repository root AND to a
/// resolved plugin install root.
const FORK_DIR: &str = ".fabro/workflows/implement-work-item";
/// The one forked file these fixtures carry. A real fixture pins eleven; the
/// gate's verdict is per file, so one is enough to pin the verdict's source.
const FORKED_FILE: &str = "prompts/implement.md";

/// The orchestrator release the throwaway fixture pins — the plugin cache hash,
/// which is the release tag's own commit, exactly as the real fixture records it.
const PINNED_RELEASE: &str = "b8eafc6f1d10";
/// A LATER orchestrator release, installed on the host and nowhere in the
/// committed fixture.
const NEWER_RELEASE: &str = "0f1e2d3c4b5a";

/// The forked file as the PINNED release ships it.
const PINNED_BUNDLE_BODY: &str = "the implement prompt, as the pinned release ships it\n";
/// The same file as the NEWER installed release ships it — the bundle change
/// that used to redden `master` on its own.
const NEWER_BUNDLE_BODY: &str = "the implement prompt, as the NEWER release ships it\n";

#[test]
fn the_verdict_is_taken_against_the_pinned_release_not_the_installed_bundle() -> Result<(), String>
{
    let root = repository("ignores-install", PINNED_BUNDLE_BODY)?;
    let home = installed_plugin("ignores-install", &root, NEWER_RELEASE, NEWER_BUNDLE_BODY)?;

    let output = gate(&root, &home)?;

    let rendered = rendered(&output);
    assert!(
        output.status.success(),
        "a fork matching its PINNED release must pass however far the INSTALLED bundle has \
         moved — that is the three-hand-recoveries defect: {rendered}"
    );
    assert!(
        rendered.contains(PINNED_RELEASE),
        "the gate names the orchestrator release it took the verdict against: {rendered}"
    );
    Ok(())
}

#[test]
fn a_newer_installed_release_draws_a_currency_warning_naming_both_releases() -> Result<(), String> {
    let root = repository("currency", PINNED_BUNDLE_BODY)?;
    let home = installed_plugin("currency", &root, NEWER_RELEASE, NEWER_BUNDLE_BODY)?;

    let output = gate(&root, &home)?;

    let rendered = rendered(&output);
    assert!(
        output.status.success(),
        "a newer install is a currency signal, so the gate still exits zero: {rendered}"
    );
    let warning = currency_warning(&rendered).unwrap_or_default();
    assert!(
        warning.contains(PINNED_RELEASE),
        "the currency warning names the PINNED release: {rendered}"
    );
    assert!(
        warning.contains(NEWER_RELEASE),
        "the currency warning names the INSTALLED release: {rendered}"
    );
    Ok(())
}

#[test]
fn an_install_at_the_pinned_release_draws_no_currency_warning() -> Result<(), String> {
    let root = repository("current", PINNED_BUNDLE_BODY)?;
    let home = installed_plugin("current", &root, PINNED_RELEASE, PINNED_BUNDLE_BODY)?;

    let output = gate(&root, &home)?;

    let rendered = rendered(&output);
    assert!(output.status.success(), "{rendered}");
    assert!(
        currency_warning(&rendered).is_none(),
        "a host already at the pinned release has no currency to warn about: {rendered}"
    );
    Ok(())
}

/// The gate's currency-warning line, if it printed one. A currency signal that
/// does not name BOTH releases on its own line leaves the reader to guess which
/// of the two the verdict came from, which is the whole question.
fn currency_warning(rendered: &str) -> Option<&str> {
    rendered
        .lines()
        .find(|line| line.contains("CURRENCY WARNING"))
}

/// A throwaway repository root: the pin fixture naming [`PINNED_RELEASE`], and a
/// committed fork whose one file carries `fork_body`.
fn repository(label: &str, fork_body: &str) -> Result<PathBuf, String> {
    let root = scratch(&format!("repo-{label}"))?;
    write(
        &root.join(PIN_MANIFEST),
        &format!(
            r#"{{
  "upstream_plugin_version": "{PINNED_RELEASE}",
  "files": [
    {{
      "path": "{FORKED_FILE}",
      "upstream_sha256": "{}",
      "present_in_fork": true,
      "reason": "Taken VERBATIM from the pinned release."
    }}
  ]
}}
"#,
            digest(PINNED_BUNDLE_BODY.as_bytes())
        ),
    )?;
    write(&root.join(FORK_DIR).join(FORKED_FILE), fork_body)?;
    Ok(root)
}

/// A throwaway `$HOME` whose `installed_plugins.json` records an orchestrator
/// install for `root`, at `version`, whose bundle carries `bundle_body`.
fn installed_plugin(
    label: &str,
    root: &Path,
    version: &str,
    bundle_body: &str,
) -> Result<PathBuf, String> {
    let home = scratch(&format!("home-{label}"))?;
    let install = home.join("install");
    write(&install.join(FORK_DIR).join(FORKED_FILE), bundle_body)?;
    // A release that moved the bundle usually ADDS prompts too; the gate must be
    // indifferent to that as well, not merely to a changed digest.
    write(
        &install.join(FORK_DIR).join("prompts/brand-new-stage.md"),
        "a stage the pinned release never carried\n",
    )?;
    let project_path = canonical(root)?;
    let install_path = canonical(&install)?;
    write(
        &home.join(".claude/plugins/installed_plugins.json"),
        &format!(
            r#"{{"plugins":{{"livespec-orchestrator-beads-fabro@livespec-orchestrator-beads-fabro":[{{"projectPath":"{}","installPath":"{}","version":"{version}"}}]}}}}"#,
            project_path.display(),
            install_path.display()
        ),
    )?;
    Ok(home)
}

/// Run the shipped gate in `root`, with `home` as its `$HOME`.
fn gate(root: &Path, home: &Path) -> Result<Output, String> {
    Command::new(checker()?)
        .current_dir(root)
        .env("HOME", home)
        .output()
        .map_err(|error| format!("cannot run the gate: {error}"))
}

/// Everything the gate said, on either stream — the verdict is on stdout and the
/// findings are on stderr, and a diagnostic that names neither is no diagnostic.
fn rendered(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Resolves `CARGO_BIN_EXE_console-fork-drift-check` at COMPILE time via
/// `option_env!` first, then falls back to the process environment. Cargo
/// defines the variable while compiling this package's integration tests but
/// plain `cargo test` does not export it to the running test process, so a
/// runtime-only lookup diverges between runners; `option_env!` compiles to
/// `None` in contexts that never set it (`cargo clippy --all-targets`), which is
/// what the runtime fallback is for.
fn checker() -> Result<PathBuf, String> {
    option_env!("CARGO_BIN_EXE_console-fork-drift-check").map_or_else(
        || {
            std::env::var("CARGO_BIN_EXE_console-fork-drift-check")
                .map(PathBuf::from)
                .map_err(|_| {
                    "CARGO_BIN_EXE_console-fork-drift-check is neither compiled in nor set in \
                     the environment; run this suite through cargo test or cargo nextest"
                        .to_owned()
                })
        },
        |path| Ok(PathBuf::from(path)),
    )
}

fn scratch(label: &str) -> Result<PathBuf, String> {
    let root = std::env::temp_dir().join(format!(
        "console-fork-drift-gate-{label}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root)
        .map_err(|error| format!("cannot create {}: {error}", root.display()))?;
    Ok(root)
}

fn write(path: &Path, body: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    fs::write(path, body).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

fn canonical(path: &Path) -> Result<PathBuf, String> {
    fs::canonicalize(path)
        .map_err(|error| format!("cannot canonicalize {}: {error}", path.display()))
}
