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
//! # Why hash pins, and not a live diff against a bundle
//!
//! The bundle the verdict is taken against is the one the PINNED release ships,
//! and that is not a thing any host reliably has on disk: a host installs
//! whatever `ensure-plugins` last gave it, and CI installs nothing. Taking the
//! verdict from whatever bundle happened to be present is precisely the defect
//! recorded below. So the fixture carries that bundle's digests, per file, as
//! COMMITTED state — which is also what lets the gate see OUR edits to the fork,
//! something a pins-versus-install comparison never could
//! (`livespec-console-beads-fabro-3r6`).
//!
//! A per-file digest plus a recorded divergence also keeps a deliberate fork
//! legible. The fork's standing policy is zero divergence — every prompt and
//! both graph files are taken verbatim, the sandbox-image line excepted — but
//! that policy is a choice, not a mechanism: a file may earn a divergence by
//! recording one, and only then.
//!
//! So this pins, per file, the digest of the file as the pinned orchestrator
//! release ships it, plus a human reason. The one narrower exception is
//! `workflow.toml`: the sandbox-image line is the fork's reason for existing
//! (`python-rust-agent-`, because the console builds Rust, against upstream's
//! `python-agent-`), and that line also moves on the release fan-out's cadence
//! on BOTH sides, so it is normalized away before hashing. Every other byte in
//! `workflow.toml` is pinned.
//!
//! # The verdict is a function of COMMITTED state
//!
//! - `check_local`: the manifest describes exactly the fork files that exist —
//!   no undeclared file added, none declared-but-missing, every pin explained.
//! - `check_fork_against_pinned_release`: THE VERDICT. The committed fork's
//!   bytes against the digests the fixture records for the release named by its
//!   `upstream_plugin_version`, which is the plugin cache hash and therefore the
//!   release tag's own commit.
//!
//! Neither needs a plugin, a network or a host, so one console commit has ONE
//! verdict everywhere — including CI, which carries no orchestrator plugin and
//! could not be given one without editing `.github/workflows/` (which factory
//! branches must not do).
//!
//! That is a deliberate correction. The gate used to take its verdict from the
//! workflow bundle of whatever orchestrator build the HOST had installed, so
//! every orchestrator release that changed that bundle turned console `master`
//! red on every host that ran `ensure-plugins`, with no console commit at all —
//! refusing every push and failing every post-merge janitor until a human
//! hand-ran a design review plus `just refresh-fork-upstream-pins` through a PR.
//! Three recoveries in three days (R316 / PR 1256, R498 / PR 1260, R620 /
//! PR 1268) earned work-item `livespec-console-beads-fabro-lavbfp` and spec v055
//! Contributor Scenario K. The Dispatcher runs the console's COMMITTED fork and
//! never the installed bundle, so lag behind the newest release is a CURRENCY
//! matter — see [`Currency`], which names both releases and never fails the gate
//! — while the pin advances only through a change that re-syncs the fork and
//! re-takes the pins, so the upstream diff is design-reviewed as that change's
//! review.

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
    /// SHA-256 of this file's bytes AS THE PINNED RELEASE SHIPS THEM, or `None`
    /// when that release is expected NOT to carry this path. The field keeps its
    /// `upstream_` name because the committed fixture's keys are these names.
    pub upstream_sha256: Option<String>,
    /// The same digest after applying this crate's single path-scoped
    /// normalizer. Used only for `workflow.toml`, whose sandbox-image line is
    /// deliberately ignored on both sides.
    pub upstream_sha256_ignoring_docker_pin: Option<String>,
    /// Whether our fork is expected to carry this path. `false` records a
    /// deliberate omission (upstream has it, we do not).
    pub present_in_fork: bool,
    /// Why this file diverges, in one line. Never empty.
    pub reason: String,
    /// Set, and non-blank, when OUR committed copy deliberately differs from the
    /// pinned release's bytes. Without it, any difference between the fork and
    /// the pinned bundle is a finding — which is what keeps a local edit to the
    /// fork visible instead of being absorbed into the prose of `reason`.
    pub fork_divergence: Option<String>,
}

/// A single gate failure. Always names the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// A fork file exists that the manifest does not declare.
    UndeclaredForkFile { path: String },
    /// The manifest declares a fork file that is missing from the tree.
    DeclaredButMissing { path: String },
    /// Our committed copy differs from the pinned release's bytes, and the
    /// fixture records no divergence for it.
    ForkDivergedFromPinnedRelease {
        path: String,
        pinned_release: String,
        pinned: String,
        fork: String,
    },
    /// Our fork carries a path the pinned release does not — the extreme case of
    /// differing from the bundle — with no divergence recorded for it.
    ForkFileAbsentFromPinnedRelease {
        path: String,
        pinned_release: String,
    },
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
            Self::ForkDivergedFromPinnedRelease {
                path,
                pinned_release,
                pinned,
                fork,
            } => format!(
                "{path}: the COMMITTED fork differs from orchestrator release \
                 {pinned_release}, which {PIN_MANIFEST} pins (pinned {}, fork {}), and no \
                 `fork_divergence` is recorded for it. Either restore the pinned release's \
                 bytes, or record the divergence with a reason — and if the intent was to \
                 ADVANCE the pin, re-sync the fork and re-take the pins so the upstream diff \
                 is design-reviewed.",
                &pinned[..pinned.len().min(12)],
                &fork[..fork.len().min(12)],
            ),
            Self::ForkFileAbsentFromPinnedRelease {
                path,
                pinned_release,
            } => format!(
                "{path}: the COMMITTED fork carries this path but orchestrator release \
                 {pinned_release}, which {PIN_MANIFEST} pins, does not, and no \
                 `fork_divergence` is recorded for it. A file upstream never had is the \
                 widest divergence there is; record it with a reason or remove it."
            ),
            Self::MissingReason { path } => {
                format!(
                    "{path}: pin carries an empty `reason`; every divergence must be explained."
                )
            }
        }
    }
}

/// Where the host's installed orchestrator release stands relative to the
/// pinned one.
///
/// CURRENCY ONLY — never a verdict, and never a finding. The Dispatcher runs the
/// console's COMMITTED fork, not the installed bundle, so an install that has
/// moved past the pin says the pin is due a reviewed re-sync; it says nothing
/// about whether the committed fork is correct. Treating it as a failure is the
/// defect this crate was rewritten to remove.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Currency {
    /// The host has the pinned release installed. Nothing to report.
    AtThePin { release: String },
    /// The host has some OTHER release installed. In practice that means a newer
    /// one — `ensure-plugins` only ever moves a host forward — but two plugin
    /// cache hashes cannot be ORDERED, so this records a difference and says so
    /// rather than claiming an ordering it cannot see.
    PastThePin { pinned: String, installed: String },
    /// No installed release could be resolved. Unremarkable: CI carries no
    /// orchestrator plugin, and the verdict no longer needs one.
    Unresolved { pinned: String, reason: String },
}

impl Currency {
    /// One line for the current and unresolved cases; a warning NAMING BOTH
    /// RELEASES on its first line, then the reasoning, when the host has moved
    /// past the pin.
    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Self::AtThePin { release } => format!(
                "console-fork-drift-check: installed orchestrator release {release} is the \
                 pinned release"
            ),
            Self::PastThePin { pinned, installed } => format!(
                "console-fork-drift-check: CURRENCY WARNING — installed orchestrator release \
                 {installed} differs from the pinned release {pinned} (in practice the install \
                 is newer: `ensure-plugins` only moves a host forward)\n  \
                 The verdict above was taken against the PINNED release, and is unaffected: a \
                 Factory dispatch runs this repository's COMMITTED fork, never the installed \
                 bundle.\n  \
                 Advance the pin only through a change that re-syncs the fork against the newer \
                 release and re-takes the pins, so the upstream diff is design-reviewed as that \
                 change's review."
            ),
            Self::Unresolved { pinned, reason } => format!(
                "console-fork-drift-check: no installed orchestrator release resolved — \
                 {reason}. The verdict is unaffected: it is taken against the pinned release \
                 {pinned}."
            ),
        }
    }
}

/// Where the host stands, from the pinned release and whatever install
/// [`resolve_upstream_dir`] could resolve.
#[must_use]
pub fn currency(pinned: &str, installed: &Result<(PathBuf, String), String>) -> Currency {
    match installed {
        Ok((_bundle, installed)) if installed == pinned => Currency::AtThePin {
            release: installed.clone(),
        },
        Ok((_bundle, installed)) => Currency::PastThePin {
            pinned: pinned.to_owned(),
            installed: installed.clone(),
        },
        Err(reason) => Currency::Unresolved {
            pinned: pinned.to_owned(),
            reason: reason.clone(),
        },
    }
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

/// The orchestrator release the manifest pins, from `upstream_plugin_version`.
///
/// That value is the plugin cache hash, which is the orchestrator release tag's
/// own commit (`v0.231.4` is `b8eafc6f1d10`), so the release the fixture names
/// is identified by COMMITTED state and the gate's verdict can be a function of
/// committed state alone — the same on every host, whatever each has installed.
///
/// # Errors
/// Returns a message when the JSON is malformed, or when the key is absent or
/// blank: a fixture that names no release can judge nothing host-independently,
/// and silently judging against the host's install instead is the very defect
/// this crate was rewritten to remove.
pub fn parse_pinned_release(manifest_json: &str) -> Result<String, String> {
    let root: serde_json::Value =
        serde_json::from_str(manifest_json).map_err(|err| format!("{PIN_MANIFEST}: {err}"))?;
    let release = root
        .get("upstream_plugin_version")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim();
    if release.is_empty() {
        return Err(format!(
            "{PIN_MANIFEST}: carries no `upstream_plugin_version`. The verdict is taken against \
             the orchestrator release the fixture PINS, so a fixture naming no release cannot \
             be judged against anything."
        ));
    }
    Ok(release.to_owned())
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
            let upstream_sha256_ignoring_docker_pin = entry
                .get("upstream_sha256_ignoring_docker_pin")
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
            let fork_divergence = entry
                .get("fork_divergence")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            Ok(Pin {
                path,
                upstream_sha256,
                upstream_sha256_ignoring_docker_pin,
                present_in_fork,
                reason,
                fork_divergence,
            })
        })
        .collect()
}

/// Every file under `dir`, as paths relative to it, sorted.
fn walk_relative(dir: &Path) -> Vec<String> {
    fn recurse(current: &Path, relative_prefix: &str, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(current) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let relative = if relative_prefix.is_empty() {
                name.into_owned()
            } else {
                format!("{relative_prefix}/{name}")
            };
            if path.is_dir() {
                recurse(&path, &relative, out);
            } else {
                out.push(relative);
            }
        }
    }
    let mut out = Vec::new();
    recurse(dir, "", &mut out);
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

const WORKFLOW_TOML: &str = "workflow.toml";
/// The sandbox-image line, up to the tag. Deliberately stops BEFORE the image
/// flavour: upstream's is `python-agent-`, the fork's is `python-rust-agent-`
/// because the console builds Rust, and that one value is `workflow.toml`'s
/// whole reason for being forked. Normalizing it on BOTH sides is what lets the
/// fork be compared against the pinned release at all.
const DOCKER_IMAGE_LINE_PREFIX: &str = r#"docker = "ghcr.io/thewoolleyman/livespec-fabro-sandbox:"#;
/// The canonical form every sandbox-image line normalizes to.
///
/// These BYTES are load-bearing and must not be tidied: the committed
/// `upstream_sha256_ignoring_docker_pin` digests were taken under exactly this
/// replacement, so changing it — even to something more neutral-looking than a
/// `python-agent-` flavour the fork never uses — invalidates every pin in the
/// fixture and can only be re-taken against a real orchestrator bundle.
const NORMALIZED_DOCKER_IMAGE_LINE: &str =
    r#"docker = "ghcr.io/thewoolleyman/livespec-fabro-sandbox:python-agent-<ignored>""#;

/// The digest a pin is compared on: raw bytes, except `workflow.toml`, whose
/// sandbox-image line is normalized away first.
#[must_use]
pub fn digest_for_pin(path: &str, bytes: &[u8]) -> String {
    if path == WORKFLOW_TOML {
        return digest(&normalize_workflow_docker_pin(bytes));
    }
    digest(bytes)
}

fn pinned_digest(pin: &Pin) -> Option<&String> {
    pin.upstream_sha256_ignoring_docker_pin
        .as_ref()
        .or(pin.upstream_sha256.as_ref())
}

fn normalize_workflow_docker_pin(bytes: &[u8]) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return bytes.to_vec();
    };
    let mut normalized = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let (body, newline) = line
            .strip_suffix('\n')
            .map_or((line, ""), |body| (body, "\n"));
        let body = body.strip_suffix('\r').unwrap_or(body);
        let carriage_return = if line.ends_with("\r\n") { "\r" } else { "" };
        if body.starts_with(DOCKER_IMAGE_LINE_PREFIX) && body.ends_with('"') {
            normalized.push_str(NORMALIZED_DOCKER_IMAGE_LINE);
        } else {
            normalized.push_str(body);
        }
        normalized.push_str(carriage_return);
        normalized.push_str(newline);
    }
    normalized.into_bytes()
}

/// THE VERDICT: the committed fork, judged against the workflow bundle of the
/// orchestrator release the fixture pins.
///
/// The fixture records that bundle BY DIGEST, per file, which is why this needs
/// no plugin, no network and no host: it is a function of committed state alone,
/// so one console commit has one verdict everywhere. A file whose bytes differ
/// from the pinned release's is a finding unless the fixture records a
/// `fork_divergence` for it — which is the one thing a fork is allowed to be.
#[must_use]
pub fn check_fork_against_pinned_release(
    pins: &[Pin],
    fork_dir: &Path,
    pinned_release: &str,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for pin in pins {
        if !pin.present_in_fork || records_a_divergence(pin) {
            continue;
        }
        // An absent file is `check_local`'s `DeclaredButMissing`, not a
        // divergence; reporting it twice says nothing new.
        let Ok(bytes) = std::fs::read(fork_dir.join(&pin.path)) else {
            continue;
        };
        let fork = digest_for_pin(&pin.path, &bytes);
        match pinned_digest(pin) {
            Some(pinned) if *pinned == fork => {}
            Some(pinned) => findings.push(Finding::ForkDivergedFromPinnedRelease {
                path: pin.path.clone(),
                pinned_release: pinned_release.to_owned(),
                pinned: pinned.clone(),
                fork,
            }),
            None => findings.push(Finding::ForkFileAbsentFromPinnedRelease {
                path: pin.path.clone(),
                pinned_release: pinned_release.to_owned(),
            }),
        }
    }
    findings
}

/// Whether the fixture records a deliberate divergence for this file. A blank
/// value records nothing — the whole point is that someone wrote down why.
fn records_a_divergence(pin: &Pin) -> bool {
    pin.fork_divergence
        .as_ref()
        .is_some_and(|divergence| !divergence.trim().is_empty())
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

    /// The release every fixture in these tests pins.
    const PINNED_RELEASE: &str = "b8eafc6f1d10";

    fn pin(path: &str, sha: Option<&str>, present: bool, reason: &str) -> Pin {
        Pin {
            path: path.to_owned(),
            upstream_sha256: sha.map(str::to_owned),
            upstream_sha256_ignoring_docker_pin: None,
            present_in_fork: present,
            reason: reason.to_owned(),
            fork_divergence: None,
        }
    }

    fn workflow_pin(sha: &str) -> Pin {
        Pin {
            path: WORKFLOW_TOML.to_owned(),
            upstream_sha256: None,
            upstream_sha256_ignoring_docker_pin: Some(sha.to_owned()),
            present_in_fork: true,
            reason: "Rust fork keeps its own sandbox image".to_owned(),
            fork_divergence: None,
        }
    }

    /// The verdict over a fork directory, against [`PINNED_RELEASE`].
    fn verdict(pins: &[Pin], fork_dir: &Path) -> Vec<Finding> {
        check_fork_against_pinned_release(pins, fork_dir, PINNED_RELEASE)
    }

    /// Region-coverage note for the a2 slices: keep assertion failures routed
    /// through this single monomorphic site, and cover the panic arm once. Do
    /// not replace this with a generic result/assert helper; generic helpers
    /// monomorphize into multiple uncovered failure sites.
    #[track_caller]
    #[allow(clippy::manual_assert, clippy::panic)]
    fn check(condition: bool, context: &str) {
        if !condition {
            panic!("check failed: {context}");
        }
    }

    #[test]
    #[should_panic(expected = "check failed")]
    fn check_reports_failure() {
        check(false, "deliberate failure");
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
            r#"{"files":[{"path":"prompts/pr.md","upstream_sha256":"aa","upstream_sha256_ignoring_docker_pin":"bb","present_in_fork":true,"reason":"synced"}]}"#,
        )
        .unwrap_or_default();
        let mut expected = pin("prompts/pr.md", Some("aa"), true, "synced");
        expected.upstream_sha256_ignoring_docker_pin = Some("bb".to_owned());
        assert_eq!(parsed, vec![expected]);
    }

    #[test]
    fn parse_pinned_release_reads_the_release_the_fixture_names() {
        assert_eq!(
            parse_pinned_release(r#"{"upstream_plugin_version":"b8eafc6f1d10","files":[]}"#),
            Ok("b8eafc6f1d10".to_owned())
        );
    }

    #[test]
    fn parse_pinned_release_rejects_a_fixture_that_names_no_release() {
        check(
            parse_pinned_release(r#"{"files":[]}"#).is_err(),
            "a fixture naming no release is rejected",
        );
        check(
            parse_pinned_release(r#"{"upstream_plugin_version":"  ","files":[]}"#).is_err(),
            "a blank release is rejected",
        );
    }

    #[test]
    fn parse_pinned_release_rejects_malformed_json() {
        check(
            parse_pinned_release("{not json").is_err(),
            "malformed JSON is rejected",
        );
    }

    #[test]
    fn currency_at_the_pin_is_reported_without_a_warning() {
        let at_the_pin = currency(
            "b8eafc6f1d10",
            &Ok((PathBuf::from("/install"), "b8eafc6f1d10".to_owned())),
        );
        assert_eq!(
            at_the_pin,
            Currency::AtThePin {
                release: "b8eafc6f1d10".to_owned()
            }
        );
        let rendered = at_the_pin.render();
        check(!rendered.contains("CURRENCY WARNING"), &rendered);
        check(rendered.contains("b8eafc6f1d10"), &rendered);
    }

    #[test]
    fn currency_past_the_pin_warns_naming_both_releases_on_one_line() {
        let past_the_pin = currency(
            "b8eafc6f1d10",
            &Ok((PathBuf::from("/install"), "0f1e2d3c4b5a".to_owned())),
        );
        assert_eq!(
            past_the_pin,
            Currency::PastThePin {
                pinned: "b8eafc6f1d10".to_owned(),
                installed: "0f1e2d3c4b5a".to_owned()
            }
        );
        let rendered = past_the_pin.render();
        let warning = rendered
            .lines()
            .find(|line| line.contains("CURRENCY WARNING"))
            .unwrap_or_default();
        check(warning.contains("b8eafc6f1d10"), &rendered);
        check(warning.contains("0f1e2d3c4b5a"), &rendered);
    }

    #[test]
    fn currency_with_no_resolvable_install_says_the_verdict_is_unaffected() {
        let unresolved = currency("b8eafc6f1d10", &Err("no plugin installed".to_owned()));
        assert_eq!(
            unresolved,
            Currency::Unresolved {
                pinned: "b8eafc6f1d10".to_owned(),
                reason: "no plugin installed".to_owned()
            }
        );
        let rendered = unresolved.render();
        check(!rendered.contains("CURRENCY WARNING"), &rendered);
        check(rendered.contains("no plugin installed"), &rendered);
        check(rendered.contains("unaffected"), &rendered);
    }

    #[test]
    fn parse_pins_rejects_a_missing_files_array() {
        check(parse_pins("{}").is_err(), "missing files array is rejected");
    }

    #[test]
    fn a_fork_file_differing_from_the_pinned_release_is_reported_with_the_file_named() {
        let dir = scratch("diverged");
        let _ = std::fs::write(dir.join("prompts/pr.md"), b"OUR UNRECORDED EDIT");
        let findings = verdict(
            &[pin(
                "prompts/pr.md",
                Some(&digest(b"AS THE PINNED RELEASE SHIPS IT")),
                true,
                "Taken verbatim.",
            )],
            &dir,
        );
        check(
            matches!(
                findings.as_slice(),
                [Finding::ForkDivergedFromPinnedRelease { path, pinned_release, .. }]
                    if path == "prompts/pr.md" && pinned_release == PINNED_RELEASE
            ),
            "an unrecorded fork divergence is reported for prompts/pr.md",
        );
        let rendered = findings.first().map(Finding::render).unwrap_or_default();
        check(rendered.contains("prompts/pr.md"), &rendered);
        check(rendered.contains(PINNED_RELEASE), &rendered);
    }

    #[test]
    fn a_fork_matching_the_pinned_release_is_clean() {
        let dir = scratch("matches");
        let _ = std::fs::write(dir.join("prompts/pr.md"), b"SAME");
        let findings = verdict(
            &[pin(
                "prompts/pr.md",
                Some(&digest(b"SAME")),
                true,
                "Taken verbatim.",
            )],
            &dir,
        );
        assert_eq!(findings, vec![]);
    }

    /// A divergence the fixture RECORDS is what a fork is; the gate judges the
    /// ones nobody wrote down.
    #[test]
    fn a_recorded_fork_divergence_is_clean() {
        let dir = scratch("recorded-divergence");
        let _ = std::fs::write(dir.join("prompts/pr.md"), b"OURS, DELIBERATELY");
        let mut recorded = pin(
            "prompts/pr.md",
            Some(&digest(b"UPSTREAM")),
            true,
            "Taken verbatim except as recorded.",
        );
        recorded.fork_divergence = Some("Divergence: the console builds Rust.".to_owned());
        assert_eq!(verdict(&[recorded], &dir), vec![]);
    }

    /// A BLANK `fork_divergence` records nothing, so it exempts nothing.
    #[test]
    fn a_blank_fork_divergence_exempts_nothing() {
        let dir = scratch("blank-divergence");
        let _ = std::fs::write(dir.join("prompts/pr.md"), b"OURS");
        let mut blank = pin(
            "prompts/pr.md",
            Some(&digest(b"UPSTREAM")),
            true,
            "Taken verbatim.",
        );
        blank.fork_divergence = Some("   ".to_owned());
        check(
            matches!(
                verdict(&[blank], &dir).as_slice(),
                [Finding::ForkDivergedFromPinnedRelease { .. }]
            ),
            "a blank divergence record exempts nothing",
        );
    }

    /// The fork carries a file the pinned release does not, and nobody recorded
    /// why — the widest divergence there is.
    #[test]
    fn a_fork_file_the_pinned_release_never_carried_is_reported() {
        let dir = scratch("fork-only");
        let _ = std::fs::write(dir.join("prompts/ours-only.md"), b"OURS ALONE");
        let findings = verdict(
            &[pin(
                "prompts/ours-only.md",
                None,
                true,
                "TODO: explain this",
            )],
            &dir,
        );
        check(
            matches!(
                findings.as_slice(),
                [Finding::ForkFileAbsentFromPinnedRelease { path, .. }]
                    if path == "prompts/ours-only.md"
            ),
            "a fork-only file with no recorded divergence is reported",
        );
    }

    /// A file the manifest declares ABSENT from the fork is `check_local`'s
    /// business; the verdict never reads it.
    #[test]
    fn a_pin_not_present_in_the_fork_is_not_judged() {
        let dir = scratch("absent-from-fork");
        let findings = verdict(
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

    /// A declared file missing from the tree is reported ONCE, by `check_local`.
    #[test]
    fn a_declared_but_missing_fork_file_is_not_also_a_divergence() {
        let dir = scratch("missing-not-diverged");
        let findings = verdict(
            &[pin("prompts/pr.md", Some("aa"), true, "Taken verbatim.")],
            &dir,
        );
        assert_eq!(findings, vec![]);
    }

    /// `workflow.toml`'s sandbox-image line is the fork's reason for existing —
    /// `python-rust-agent-` against upstream's `python-agent-` — AND it moves on
    /// the release fan-out's cadence on both sides. Normalizing it on both sides
    /// is what makes the rest of the file comparable.
    #[test]
    fn the_workflow_sandbox_image_line_is_normalized_on_both_sides() {
        let upstream = b"steps = []\ndocker = \"ghcr.io/thewoolleyman/livespec-fabro-sandbox:python-agent-v1.0.0\"\ncommit_timeout = \"10m\"\n";
        let fork = b"steps = []\ndocker = \"ghcr.io/thewoolleyman/livespec-fabro-sandbox:python-rust-agent-v1.126.0\"\ncommit_timeout = \"10m\"\n";
        let dir = scratch("workflow-image-clean");
        let _ = std::fs::write(dir.join(WORKFLOW_TOML), fork);
        let findings = verdict(
            &[workflow_pin(&digest_for_pin(WORKFLOW_TOML, upstream))],
            &dir,
        );
        assert_eq!(findings, vec![]);
    }

    #[test]
    fn workflow_differences_outside_the_image_line_are_still_reported() {
        let upstream = b"steps = []\ndocker = \"ghcr.io/thewoolleyman/livespec-fabro-sandbox:python-agent-v1.0.0\"\ncommit_timeout = \"10m\"\n";
        let fork = b"steps = []\ndocker = \"ghcr.io/thewoolleyman/livespec-fabro-sandbox:python-rust-agent-v1.126.0\"\ncommit_timeout = \"15m\"\n";
        let dir = scratch("workflow-real-drift");
        let _ = std::fs::write(dir.join(WORKFLOW_TOML), fork);
        let findings = verdict(
            &[workflow_pin(&digest_for_pin(WORKFLOW_TOML, upstream))],
            &dir,
        );
        check(
            matches!(
                findings.as_slice(),
                [Finding::ForkDivergedFromPinnedRelease { path, .. }] if path == WORKFLOW_TOML
            ),
            "a workflow difference outside the image line is reported",
        );
    }

    /// Only the exact `docker = "<registry>:` form normalizes. A rewritten key or
    /// changed spacing is a real difference, not an image-tag bump.
    #[test]
    fn a_workflow_image_line_with_different_spacing_is_still_reported() {
        let upstream =
            b"docker = \"ghcr.io/thewoolleyman/livespec-fabro-sandbox:python-agent-v1.0.0\"\n";
        let fork =
            b"docker=\"ghcr.io/thewoolleyman/livespec-fabro-sandbox:python-agent-v1.99.0\"\n";
        let dir = scratch("workflow-spacing-drift");
        let _ = std::fs::write(dir.join(WORKFLOW_TOML), fork);
        let findings = verdict(
            &[workflow_pin(&digest_for_pin(WORKFLOW_TOML, upstream))],
            &dir,
        );
        check(
            matches!(
                findings.as_slice(),
                [Finding::ForkDivergedFromPinnedRelease { path, .. }] if path == WORKFLOW_TOML
            ),
            "a rewritten image key or spacing is reported",
        );
    }

    #[test]
    fn workflow_digest_falls_back_to_raw_bytes_for_non_utf8() {
        let bytes =
            b"docker = \"ghcr.io/thewoolleyman/livespec-fabro-sandbox:python-agent-v1.0.0\"\xff\n";
        assert_eq!(digest_for_pin(WORKFLOW_TOML, bytes), digest(bytes));
    }

    #[test]
    fn workflow_digest_preserves_crlf_when_normalizing_docker_pin() {
        let old =
            b"docker = \"ghcr.io/thewoolleyman/livespec-fabro-sandbox:python-agent-v1.0.0\"\r\n";
        let new =
            b"docker = \"ghcr.io/thewoolleyman/livespec-fabro-sandbox:python-agent-v2.0.0\"\r\n";
        assert_eq!(
            digest_for_pin(WORKFLOW_TOML, old),
            digest_for_pin(WORKFLOW_TOML, new)
        );
    }

    #[test]
    fn an_undeclared_fork_file_is_reported() {
        let dir = scratch("undeclared");
        let _ = std::fs::write(dir.join("prompts/sneaky.md"), b"x");
        let findings = check_local(&[], &dir);
        check(
            matches!(
                findings.as_slice(),
                [Finding::UndeclaredForkFile { path }] if path == "prompts/sneaky.md"
            ),
            "undeclared fork file is reported",
        );
    }

    #[test]
    fn a_declared_fork_file_is_not_reported_as_undeclared() {
        let dir = scratch("declared-present");
        let _ = std::fs::write(dir.join("prompts/pr.md"), b"x");
        let findings = check_local(&[pin("prompts/pr.md", None, true, "forked")], &dir);
        check(
            findings.is_empty(),
            "declared present fork file should be clean",
        );
    }

    #[test]
    fn a_declared_but_missing_fork_file_is_reported() {
        let dir = scratch("missing");
        let findings = check_local(&[pin("prompts/pr.md", Some("aa"), true, "forked")], &dir);
        check(
            matches!(
                findings.as_slice(),
                [Finding::DeclaredButMissing { path }] if path == "prompts/pr.md"
            ),
            "declared missing fork file is reported",
        );
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
        check(
            matches!(
                findings.as_slice(),
                [Finding::MissingReason { path }] if path == "prompts/pr.md"
            ),
            "pin without a reason is reported",
        );
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
            Finding::ForkDivergedFromPinnedRelease {
                path: "c.md".to_owned(),
                pinned_release: PINNED_RELEASE.to_owned(),
                pinned: "0".repeat(64),
                fork: "1".repeat(64),
            },
            Finding::ForkFileAbsentFromPinnedRelease {
                path: "d.md".to_owned(),
                pinned_release: PINNED_RELEASE.to_owned(),
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
                | Finding::ForkDivergedFromPinnedRelease { path, .. }
                | Finding::ForkFileAbsentFromPinnedRelease { path, .. }
                | Finding::MissingReason { path } => path.clone(),
            };
            check(rendered.starts_with(&named), &rendered);
        }
    }

    /// Short digests must not panic the truncating renderer.
    #[test]
    fn render_tolerates_a_short_digest() {
        let rendered = Finding::ForkDivergedFromPinnedRelease {
            path: "c.md".to_owned(),
            pinned_release: PINNED_RELEASE.to_owned(),
            pinned: "ab".to_owned(),
            fork: "cd".to_owned(),
        }
        .render();
        check(rendered.contains("c.md"), &rendered);
    }

    #[test]
    fn parse_pins_rejects_an_entry_without_a_string_path() {
        check(
            parse_pins(r#"{"files":[{"upstream_sha256":"aa"}]}"#).is_err(),
            "entry without string path is rejected",
        );
    }

    #[test]
    fn parse_pins_rejects_malformed_json() {
        check(
            parse_pins("{not json").is_err(),
            "malformed JSON is rejected",
        );
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
        check(
            resolve_upstream_dir(&home, Path::new("/repo")).is_err(),
            "absent plugin manifest is reported",
        );
    }

    #[test]
    fn resolve_upstream_dir_reports_malformed_json() {
        let home = fake_home("resolve-bad-json", "{not json");
        check(
            resolve_upstream_dir(&home, Path::new("/repo")).is_err(),
            "malformed plugin manifest is reported",
        );
    }

    #[test]
    fn resolve_upstream_dir_reports_a_missing_plugin_entry() {
        let home = fake_home("resolve-no-plugin", r#"{"plugins":{}}"#);
        check(
            resolve_upstream_dir(&home, Path::new("/repo")).is_err(),
            "missing plugin entry is reported",
        );
    }

    #[test]
    fn resolve_upstream_dir_reports_no_install_for_this_project() {
        let home = fake_home(
            "resolve-other-project",
            &install_json("/some/other/repo", "/tmp/whatever"),
        );
        check(
            resolve_upstream_dir(&home, Path::new("/repo")).is_err(),
            "install for another project is reported",
        );
    }

    #[test]
    fn resolve_upstream_dir_reports_an_entry_without_an_install_path() {
        let home = fake_home(
            "resolve-no-install-path",
            r#"{"plugins":{"livespec-orchestrator-beads-fabro@livespec-orchestrator-beads-fabro":[{"projectPath":"/repo"}]}}"#,
        );
        check(
            resolve_upstream_dir(&home, Path::new("/repo")).is_err(),
            "missing install path is reported",
        );
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
        check(resolved.is_err(), &format!("{resolved:?}"));
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
        check(
            matches!(
                findings.as_slice(),
                [Finding::UndeclaredForkFile { path }] if path == "prompts/deep/buried.md"
            ),
            "nested fork file is reported",
        );
    }

    /// An unreadable directory yields no paths rather than propagating an error.
    #[test]
    fn a_missing_directory_walks_to_nothing() {
        let findings = check_local(&[], Path::new("/nonexistent-fork-dir-for-test"));
        assert_eq!(findings, vec![]);
    }
}
