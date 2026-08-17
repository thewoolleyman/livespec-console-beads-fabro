//! Fork-drift gate for `.fabro/workflows/implement-work-item/`.
//!
//! That directory is a COMMITTED FORK of the orchestrator plugin's bundled
//! workflow. It is not an accident: `_dispatcher_paths.py::workflow_toml()`
//! resolves the dispatch target's own committed `workflow.toml` in preference to
//! the plugin's, precisely so "a Rust repo needing the `python-rust-agent-`
//! layer, against the orchestrator's Python-only pin" governs its own execution
//! substrate. The override granularity is the WHOLE DIRECTORY, so owning one
//! image-pin line means owning the graph and every prompt with it.
//!
//! The cost of that is silent drift, and it has already been paid once: upstream
//! fixed the pr-stage publish leg (fetch + rebase before the push, work-item
//! `bd-ib-qq7f`, merged 2026-07-24 as `231e9a48`) and our fork never received it,
//! because the file is ours. Nobody noticed for three weeks. NOTHING ASSERTED THE
//! LEG WAS PRESENT — `.fabro/` was covered by no gate at all. This crate is that
//! missing assertion.
//!
//! # Why a hash pin and not byte-equality-modulo-allowlist
//!
//! The obvious design — assert the fork equals upstream except for a small
//! allowlist — does NOT work here, and adopting it would recreate the very
//! failure it is meant to catch. Six of the seven forked files diverge
//! deliberately and substantially: our review gate is advisory/ship-on-cap by
//! recorded decision (`workflow.fabro`, work-item `bd-ib-egms32`) while
//! upstream's is blocking, and upstream has since grown a whole `disposition`
//! stage we do not run. An allowlist wide enough to tolerate that would swallow
//! nearly everything and the gate would pass on anything.
//!
//! So this pins, per file, the UPSTREAM digest last reconciled against, plus a
//! human reason for the divergence. The gate fires when UPSTREAM MOVES, naming
//! the file, which forces a conscious sync-or-re-pin. It is deliberately
//! indifferent to how far our copy has diverged — that is the point of a fork —
//! and it is immune to our own `workflow.toml` being rewritten by the pin-bump
//! automation, because it pins upstream's bytes, never ours.
//!
//! # Two lanes, and an honest gap
//!
//! - `check_local`: hermetic. Asserts the manifest describes exactly the fork
//!   files that exist — no undeclared file added, none declared-but-missing.
//!   Runs everywhere, including CI, with no plugin installed.
//! - `check_upstream`: needs the installed orchestrator plugin. Compares live
//!   upstream digests against the pinned ones.
//!
//! `check_upstream` CANNOT run in CI today: the orchestrator plugin is not
//! installed there, and provisioning it would mean editing
//! `.github/workflows/`, which factory branches must not do (that is what
//! `just check-no-workflow-edits` exists to enforce). Rather than pretend
//! otherwise, an unresolvable plugin is reported as an explicit, loud SKIP naming
//! the reason — never a silent pass. The lane where it does run is the one that
//! matters most in practice: every `just check` on a machine that dispatches,
//! including the `lefthook` pre-push hook, which is where a fork edit is made and
//! where a dispatch is launched.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// The committed pin manifest, read relative to the repository root (where
/// `just check` runs).
pub const PIN_MANIFEST: &str = "tests/fixtures/fabro-fork-upstream-pins.json";

/// The forked workflow directory, relative to the repository root.
pub const FORK_DIR: &str = ".fabro/workflows/implement-work-item";

/// One pinned file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    /// Path relative to [`FORK_DIR`].
    pub path: String,
    /// SHA-256 of the UPSTREAM file's bytes at the last reconciliation, or
    /// `None` when upstream is expected NOT to carry this path.
    pub upstream_sha256: Option<String>,
    /// Whether our fork is expected to carry this path. `false` records a
    /// deliberate omission (upstream has it, we do not).
    pub present_in_fork: bool,
    /// Why this file diverges, in one line. Never empty.
    pub reason: String,
}

/// A single gate failure. Always names the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// A fork file exists that the manifest does not declare.
    UndeclaredForkFile { path: String },
    /// The manifest declares a fork file that is missing from the tree.
    DeclaredButMissing { path: String },
    /// Upstream's bytes changed since the pin was taken.
    UpstreamMoved {
        path: String,
        pinned: String,
        live: String,
        reason: String,
    },
    /// Upstream no longer carries a path we pinned a digest for.
    UpstreamGone { path: String, pinned: String },
    /// Upstream grew a path the manifest never anticipated.
    UpstreamAdded { path: String },
    /// A pin carries no reason. A divergence nobody explained is how the last
    /// one survived.
    MissingReason { path: String },
}

impl Finding {
    /// One human line, always leading with the file it concerns.
    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Self::UndeclaredForkFile { path } => format!(
                "{path}: present in the fork but absent from {PIN_MANIFEST}. \
                 Every forked file must be pinned and explained; add it via \
                 `just refresh-fork-upstream-pins`."
            ),
            Self::DeclaredButMissing { path } => {
                format!("{path}: pinned in {PIN_MANIFEST} but missing from {FORK_DIR}.")
            }
            Self::UpstreamMoved {
                path,
                pinned,
                live,
                reason,
            } => format!(
                "{path}: UPSTREAM MOVED since this pin was taken \
                 (pinned {}, live {}). Recorded divergence: {reason}. \
                 Review upstream's change and either port it into the fork or \
                 re-pin with an updated reason via `just refresh-fork-upstream-pins`.",
                &pinned[..pinned.len().min(12)],
                &live[..live.len().min(12)],
            ),
            Self::UpstreamGone { path, pinned } => format!(
                "{path}: pinned at upstream {} but upstream no longer carries \
                 this path.",
                &pinned[..pinned.len().min(12)],
            ),
            Self::UpstreamAdded { path } => format!(
                "{path}: upstream carries this path but {PIN_MANIFEST} does not \
                 mention it — a whole stage or prompt may have been added upstream."
            ),
            Self::MissingReason { path } => {
                format!(
                    "{path}: pin carries an empty `reason`; every divergence must be explained."
                )
            }
        }
    }
}

/// Outcome of the upstream lane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpstreamLane {
    /// Upstream was resolved and compared.
    Compared { plugin_version: String },
    /// Upstream could not be resolved; the lane did not run.
    Skipped { reason: String },
}

/// SHA-256 of some bytes, lowercase hex.
#[must_use]
pub fn digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut acc, byte| {
            let _ = write!(acc, "{byte:02x}");
            acc
        })
}

/// Parse the pin manifest.
///
/// # Errors
/// Returns a message when the JSON is malformed or a pin is not an object with
/// the expected shape.
pub fn parse_pins(manifest_json: &str) -> Result<Vec<Pin>, String> {
    let root: serde_json::Value =
        serde_json::from_str(manifest_json).map_err(|err| format!("{PIN_MANIFEST}: {err}"))?;
    let files = root
        .get("files")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("{PIN_MANIFEST}: missing a `files` array"))?;
    files
        .iter()
        .map(|entry| {
            let path = entry
                .get("path")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("{PIN_MANIFEST}: a file entry has no string `path`"))?
                .to_owned();
            let upstream_sha256 = entry
                .get("upstream_sha256")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            let present_in_fork = entry
                .get("present_in_fork")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true);
            let reason = entry
                .get("reason")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned();
            Ok(Pin {
                path,
                upstream_sha256,
                present_in_fork,
                reason,
            })
        })
        .collect()
}

/// Every file under `dir`, as paths relative to it, sorted.
fn walk_relative(dir: &Path) -> Vec<String> {
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

/// The hermetic lane: the manifest must describe exactly the fork that exists,
/// and every pin must carry a reason.
#[must_use]
pub fn check_local(pins: &[Pin], fork_dir: &Path) -> Vec<Finding> {
    let declared: BTreeMap<&str, &Pin> = pins.iter().map(|pin| (pin.path.as_str(), pin)).collect();
    let mut findings = Vec::new();

    for present in walk_relative(fork_dir) {
        if !declared.contains_key(present.as_str()) {
            findings.push(Finding::UndeclaredForkFile { path: present });
        }
    }
    for pin in pins {
        if pin.reason.trim().is_empty() {
            findings.push(Finding::MissingReason {
                path: pin.path.clone(),
            });
        }
        if pin.present_in_fork && !fork_dir.join(&pin.path).is_file() {
            findings.push(Finding::DeclaredButMissing {
                path: pin.path.clone(),
            });
        }
    }
    findings
}

/// The upstream lane: compare live upstream digests against the pins.
#[must_use]
pub fn check_upstream(pins: &[Pin], upstream_dir: &Path) -> Vec<Finding> {
    let declared: BTreeMap<&str, &Pin> = pins.iter().map(|pin| (pin.path.as_str(), pin)).collect();
    let mut findings = Vec::new();

    for pin in pins {
        let path = upstream_dir.join(&pin.path);
        let live = std::fs::read(&path).ok().map(|bytes| digest(&bytes));
        match (&pin.upstream_sha256, live) {
            (Some(pinned), Some(live)) if *pinned != live => {
                findings.push(Finding::UpstreamMoved {
                    path: pin.path.clone(),
                    pinned: pinned.clone(),
                    live,
                    reason: pin.reason.clone(),
                });
            }
            (Some(pinned), None) => findings.push(Finding::UpstreamGone {
                path: pin.path.clone(),
                pinned: pinned.clone(),
            }),
            _ => {}
        }
    }
    for present in walk_relative(upstream_dir) {
        if !declared.contains_key(present.as_str()) {
            findings.push(Finding::UpstreamAdded { path: present });
        }
    }
    findings
}

/// Resolve the installed orchestrator plugin's bundled workflow directory.
///
/// Reads `~/.claude/plugins/installed_plugins.json` and picks the entry whose
/// `projectPath` is this repository — the same absolute-path discipline the
/// dispatcher itself is invoked under, because a session can hold DIFFERENT
/// plugin pins for different surfaces at once, and `PATH` here is inert.
///
/// # Errors
/// Returns a human reason when the manifest, the entry, or the directory is
/// absent. Callers report it as a loud skip, never a silent pass.
pub fn resolve_upstream_dir(home: &Path, project_root: &Path) -> Result<(PathBuf, String), String> {
    let manifest = home.join(".claude/plugins/installed_plugins.json");
    let text = std::fs::read_to_string(&manifest)
        .map_err(|err| format!("cannot read {}: {err}", manifest.display()))?;
    let root: serde_json::Value =
        serde_json::from_str(&text).map_err(|err| format!("{}: {err}", manifest.display()))?;
    let installs = root
        .get("plugins")
        .and_then(|plugins| {
            plugins.get("livespec-orchestrator-beads-fabro@livespec-orchestrator-beads-fabro")
        })
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            "no `livespec-orchestrator-beads-fabro` install recorded in installed_plugins.json"
                .to_owned()
        })?;
    let wanted = project_root.to_string_lossy();
    let install = installs
        .iter()
        .enumerate()
        .filter(|(_index, entry)| {
            entry
                .get("projectPath")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|path| path == wanted)
        })
        .max_by_key(|(index, _entry)| *index)
        .map(|(_index, entry)| entry)
        .ok_or_else(|| format!("no orchestrator install recorded for projectPath {wanted}"))?;
    let install_path = install
        .get("installPath")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "orchestrator install entry has no `installPath`".to_owned())?;
    let version = install
        .get("version")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("<unknown>")
        .to_owned();
    let dir = Path::new(install_path).join(FORK_DIR);
    if !dir.is_dir() {
        return Err(format!("resolved plugin {version} has no {FORK_DIR}"));
    }
    Ok((dir, version))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(path: &str, sha: Option<&str>, present: bool, reason: &str) -> Pin {
        Pin {
            path: path.to_owned(),
            upstream_sha256: sha.map(str::to_owned),
            present_in_fork: present,
            reason: reason.to_owned(),
        }
    }

    /// A unique scratch directory. Filesystem errors are deliberately ignored:
    /// the assertions below fail loudly if setup did not take, and the workspace
    /// denies `unwrap`/`expect`/`panic` in tests as well as in production.
    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("console-fork-drift-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(dir.join("prompts"));
        dir
    }

    #[test]
    fn digest_is_stable_lowercase_hex() {
        assert_eq!(
            digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn parse_pins_reads_every_field() {
        let parsed = parse_pins(
            r#"{"files":[{"path":"prompts/pr.md","upstream_sha256":"aa","present_in_fork":true,"reason":"synced"}]}"#,
        )
        .unwrap_or_default();
        assert_eq!(
            parsed,
            vec![pin("prompts/pr.md", Some("aa"), true, "synced")]
        );
    }

    #[test]
    fn parse_pins_rejects_a_missing_files_array() {
        assert!(parse_pins("{}").is_err());
    }

    #[test]
    fn upstream_moved_is_reported_with_the_file_named() {
        let dir = scratch("moved");
        let _ = std::fs::write(dir.join("prompts/pr.md"), b"NEW UPSTREAM");
        let findings = check_upstream(
            &[pin("prompts/pr.md", Some(&digest(b"OLD")), true, "forked")],
            &dir,
        );
        assert!(matches!(
            findings.as_slice(),
            [Finding::UpstreamMoved { path, .. }] if path == "prompts/pr.md"
        ));
        let rendered = findings.first().map(Finding::render).unwrap_or_default();
        assert!(rendered.contains("prompts/pr.md"), "{rendered}");
        assert!(rendered.contains("UPSTREAM MOVED"), "{rendered}");
    }

    #[test]
    fn an_unchanged_upstream_is_clean() {
        let dir = scratch("clean");
        let _ = std::fs::write(dir.join("prompts/pr.md"), b"SAME");
        let findings = check_upstream(
            &[pin("prompts/pr.md", Some(&digest(b"SAME")), true, "forked")],
            &dir,
        );
        assert_eq!(findings, vec![]);
    }

    #[test]
    fn a_new_upstream_file_is_reported() {
        let dir = scratch("added");
        let _ = std::fs::write(dir.join("prompts/disposition.md"), b"NEW STAGE");
        let findings = check_upstream(&[], &dir);
        assert!(matches!(
            findings.as_slice(),
            [Finding::UpstreamAdded { path }] if path == "prompts/disposition.md"
        ));
    }

    #[test]
    fn an_upstream_file_that_vanished_is_reported() {
        let dir = scratch("gone");
        let findings = check_upstream(&[pin("prompts/pr.md", Some("aa"), true, "forked")], &dir);
        assert!(matches!(
            findings.as_slice(),
            [Finding::UpstreamGone { path, .. }] if path == "prompts/pr.md"
        ));
    }

    #[test]
    fn an_undeclared_fork_file_is_reported() {
        let dir = scratch("undeclared");
        let _ = std::fs::write(dir.join("prompts/sneaky.md"), b"x");
        let findings = check_local(&[], &dir);
        assert!(matches!(
            findings.as_slice(),
            [Finding::UndeclaredForkFile { path }] if path == "prompts/sneaky.md"
        ));
    }

    #[test]
    fn a_declared_but_missing_fork_file_is_reported() {
        let dir = scratch("missing");
        let findings = check_local(&[pin("prompts/pr.md", Some("aa"), true, "forked")], &dir);
        assert!(matches!(
            findings.as_slice(),
            [Finding::DeclaredButMissing { path }] if path == "prompts/pr.md"
        ));
    }

    #[test]
    fn a_deliberate_absence_is_not_reported_as_missing() {
        let dir = scratch("absent-ok");
        let findings = check_local(
            &[pin(
                "prompts/disposition.md",
                Some("aa"),
                false,
                "upstream-only stage we do not run",
            )],
            &dir,
        );
        assert_eq!(findings, vec![]);
    }

    #[test]
    fn a_pin_without_a_reason_is_reported() {
        let dir = scratch("no-reason");
        let findings = check_local(&[pin("prompts/pr.md", Some("aa"), false, "  ")], &dir);
        assert!(matches!(
            findings.as_slice(),
            [Finding::MissingReason { path }] if path == "prompts/pr.md"
        ));
    }

    /// Every `Finding` renders, and every rendering LEADS WITH THE FILE — the
    /// property that makes a red build actionable instead of a puzzle.
    #[test]
    fn every_finding_renders_and_names_its_file() {
        let cases = vec![
            Finding::UndeclaredForkFile {
                path: "a.md".to_owned(),
            },
            Finding::DeclaredButMissing {
                path: "b.md".to_owned(),
            },
            Finding::UpstreamMoved {
                path: "c.md".to_owned(),
                pinned: "0".repeat(64),
                live: "1".repeat(64),
                reason: "forked".to_owned(),
            },
            Finding::UpstreamGone {
                path: "d.md".to_owned(),
                pinned: "2".repeat(64),
            },
            Finding::UpstreamAdded {
                path: "e.md".to_owned(),
            },
            Finding::MissingReason {
                path: "f.md".to_owned(),
            },
        ];
        for case in cases {
            let rendered = case.render();
            let named = match &case {
                Finding::UndeclaredForkFile { path }
                | Finding::DeclaredButMissing { path }
                | Finding::UpstreamMoved { path, .. }
                | Finding::UpstreamGone { path, .. }
                | Finding::UpstreamAdded { path }
                | Finding::MissingReason { path } => path.clone(),
            };
            assert!(rendered.starts_with(&named), "{rendered}");
        }
    }

    /// Short digests must not panic the truncating renderer.
    #[test]
    fn render_tolerates_a_short_digest() {
        let rendered = Finding::UpstreamMoved {
            path: "c.md".to_owned(),
            pinned: "ab".to_owned(),
            live: "cd".to_owned(),
            reason: "r".to_owned(),
        }
        .render();
        assert!(rendered.contains("c.md"), "{rendered}");
    }

    #[test]
    fn parse_pins_rejects_an_entry_without_a_string_path() {
        assert!(parse_pins(r#"{"files":[{"upstream_sha256":"aa"}]}"#).is_err());
    }

    #[test]
    fn parse_pins_rejects_malformed_json() {
        assert!(parse_pins("{not json").is_err());
    }

    #[test]
    fn parse_pins_defaults_present_in_fork_and_reason() {
        let parsed = parse_pins(r#"{"files":[{"path":"a.md"}]}"#).unwrap_or_default();
        assert_eq!(parsed, vec![pin("a.md", None, true, "")]);
    }

    /// Build a fake `$HOME` carrying an `installed_plugins.json`.
    fn fake_home(tag: &str, body: &str) -> PathBuf {
        let home = scratch(tag);
        let dir = home.join(".claude/plugins");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("installed_plugins.json"), body);
        home
    }

    fn install_json(project_path: &str, install_path: &str) -> String {
        format!(
            r#"{{"plugins":{{"livespec-orchestrator-beads-fabro@livespec-orchestrator-beads-fabro":[{{"projectPath":"{project_path}","installPath":"{install_path}","version":"cafe1234"}}]}}}}"#
        )
    }

    #[test]
    fn resolve_upstream_dir_finds_the_install_for_this_project() {
        let install = scratch("resolve-ok-install");
        let _ = std::fs::create_dir_all(install.join(FORK_DIR));
        let home = fake_home(
            "resolve-ok",
            &install_json("/repo", &install.to_string_lossy()),
        );
        // `unwrap_or_default` rather than `if let`: a non-taken `else` arm would
        // itself be an uncovered branch, and the empty defaults fail the asserts
        // just as loudly as a panic would.
        let (dir, version) = resolve_upstream_dir(&home, Path::new("/repo")).unwrap_or_default();
        assert_eq!(dir, install.join(FORK_DIR));
        assert_eq!(version, "cafe1234");
    }

    #[test]
    fn resolve_upstream_dir_uses_newest_install_for_this_project() {
        let stale = scratch("resolve-newest-stale-install");
        let newest = scratch("resolve-newest-current-install");
        let other = scratch("resolve-newest-other-install");
        let _ = std::fs::create_dir_all(stale.join(FORK_DIR));
        let _ = std::fs::create_dir_all(newest.join(FORK_DIR));
        let _ = std::fs::create_dir_all(other.join(FORK_DIR));
        let home = fake_home(
            "resolve-newest",
            &format!(
                r#"{{"plugins":{{"livespec-orchestrator-beads-fabro@livespec-orchestrator-beads-fabro":[
                    {{"projectPath":"/repo","installPath":"{}","version":"stale-first"}},
                    {{"projectPath":"/other","installPath":"{}","version":"other-project"}},
                    {{"projectPath":"/repo","installPath":"{}","version":"newest-applicable"}}
                ]}}}}"#,
                stale.to_string_lossy(),
                other.to_string_lossy(),
                newest.to_string_lossy()
            ),
        );

        let (dir, version) = resolve_upstream_dir(&home, Path::new("/repo")).unwrap_or_default();

        assert_eq!(dir, newest.join(FORK_DIR));
        assert_eq!(version, "newest-applicable");
    }

    #[test]
    fn resolve_upstream_dir_reports_an_absent_manifest() {
        let home = scratch("resolve-no-manifest");
        assert!(resolve_upstream_dir(&home, Path::new("/repo")).is_err());
    }

    #[test]
    fn resolve_upstream_dir_reports_malformed_json() {
        let home = fake_home("resolve-bad-json", "{not json");
        assert!(resolve_upstream_dir(&home, Path::new("/repo")).is_err());
    }

    #[test]
    fn resolve_upstream_dir_reports_a_missing_plugin_entry() {
        let home = fake_home("resolve-no-plugin", r#"{"plugins":{}}"#);
        assert!(resolve_upstream_dir(&home, Path::new("/repo")).is_err());
    }

    #[test]
    fn resolve_upstream_dir_reports_no_install_for_this_project() {
        let home = fake_home(
            "resolve-other-project",
            &install_json("/some/other/repo", "/tmp/whatever"),
        );
        assert!(resolve_upstream_dir(&home, Path::new("/repo")).is_err());
    }

    #[test]
    fn resolve_upstream_dir_reports_an_entry_without_an_install_path() {
        let home = fake_home(
            "resolve-no-install-path",
            r#"{"plugins":{"livespec-orchestrator-beads-fabro@livespec-orchestrator-beads-fabro":[{"projectPath":"/repo"}]}}"#,
        );
        assert!(resolve_upstream_dir(&home, Path::new("/repo")).is_err());
    }

    /// A resolved install whose workflow directory is absent is an error, not a
    /// silent empty comparison — an empty upstream would otherwise read as
    /// "nothing moved".
    #[test]
    fn resolve_upstream_dir_reports_a_plugin_without_the_workflow_dir() {
        let install = scratch("resolve-no-forkdir-install");
        let home = fake_home(
            "resolve-no-forkdir",
            &install_json("/repo", &install.to_string_lossy()),
        );
        let resolved = resolve_upstream_dir(&home, Path::new("/repo"));
        assert!(resolved.is_err(), "{resolved:?}");
    }

    #[test]
    fn resolve_upstream_dir_defaults_an_absent_version() {
        let install = scratch("resolve-no-version-install");
        let _ = std::fs::create_dir_all(install.join(FORK_DIR));
        let home = fake_home(
            "resolve-no-version",
            &format!(
                r#"{{"plugins":{{"livespec-orchestrator-beads-fabro@livespec-orchestrator-beads-fabro":[{{"projectPath":"/repo","installPath":"{}"}}]}}}}"#,
                install.to_string_lossy()
            ),
        );
        let (_, version) = resolve_upstream_dir(&home, Path::new("/repo")).unwrap_or_default();
        assert_eq!(version, "<unknown>");
    }

    /// `walk_relative` must recurse, so a nested prompt cannot hide from the gate.
    #[test]
    fn nested_fork_files_are_seen() {
        let dir = scratch("nested");
        let _ = std::fs::create_dir_all(dir.join("prompts/deep"));
        let _ = std::fs::write(dir.join("prompts/deep/buried.md"), b"x");
        let findings = check_local(&[], &dir);
        assert!(matches!(
            findings.as_slice(),
            [Finding::UndeclaredForkFile { path }] if path == "prompts/deep/buried.md"
        ));
    }

    /// An unreadable directory yields no paths rather than propagating an error.
    #[test]
    fn a_missing_directory_walks_to_nothing() {
        let findings = check_local(&[], Path::new("/nonexistent-fork-dir-for-test"));
        assert_eq!(findings, vec![]);
    }
}
