//! `console-arch-check` — architecture conformance checks for the
//! livespec-console-beads-fabro workspace.
//!
//! Crate-graph layering rules are enforced from a structured `cargo metadata`
//! source, and source-level rules are enforced at the Rust AST level via `syn`
//! so comments, strings, and similar identifiers do not produce false matches.
//!
//! ```rust,ignore
//! // Run from the repository root after dependencies are available.
//! std::process::Command::new("console-arch-check").status()?;
//! # Ok::<(), std::io::Error>(())
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::ExitCode;

use cargo_metadata::{DependencyKind, MetadataCommand};
use syn::visit::Visit;

/// Product workspace crates whose source is scanned for the AST-level rules.
///
/// Every workspace package must be present here or in
/// [`SOURCE_RULE_EXCLUDED_CRATES`]. That closed set is asserted from
/// `cargo metadata` so a newly-added product crate cannot silently miss the
/// source-level architecture rules.
const SCANNED_CRATES: &[&str] = &[
    "console-application",
    "console-cli",
    "console-domain",
    "console-eventstore",
    "console-tui",
];

/// Workspace crates deliberately excluded from the product source-level rules,
/// with the reason for each exclusion kept beside the crate name.
const SOURCE_RULE_EXCLUDED_CRATES: &[CrateSourceRuleExclusion] = &[
    CrateSourceRuleExclusion {
        name: "console-arch-check",
        reason: "architecture checker tooling, not console product code",
    },
    CrateSourceRuleExclusion {
        name: "console-ci-parity-check",
        reason: "CI guardrail tooling, not console product code",
    },
    CrateSourceRuleExclusion {
        name: "console-completeness-check",
        reason: "spec coverage checker tooling, not console product code",
    },
    CrateSourceRuleExclusion {
        name: "console-fork-drift-check",
        reason: "repository guardrail tooling, not console product code",
    },
    CrateSourceRuleExclusion {
        name: "console-nightly-soak",
        reason: "nightly quality-gate tooling (fuzz soak + mutation sweep + chore filing), not console product code",
    },
    CrateSourceRuleExclusion {
        name: "console-red-green-replay-check",
        reason: "commit discipline checker tooling, not console product code",
    },
    CrateSourceRuleExclusion {
        name: "console-spec-check",
        reason: "live-spec conformance checker tooling, not console product code",
    },
    CrateSourceRuleExclusion {
        name: "console-upstream-dep-check",
        reason: "upstream-dependency ledger gate tooling, not console product code",
    },
];

struct CrateSourceRuleExclusion {
    name: &'static str,
    reason: &'static str,
}

/// External crates a purity-constrained crate (domain, UI) must never
/// depend on directly: persistence, HTTP, async-runtime, and
/// subprocess/source-system I/O.
const FORBIDDEN_INFRA_DEPENDENCIES: &[&str] = &[
    "rusqlite",
    "libsqlite3-sys",
    "sqlx",
    "reqwest",
    "hyper",
    "axum",
    "tokio",
    "ureq",
    "surf",
    "isahc",
];

fn main() -> ExitCode {
    let root = PathBuf::from(".");
    let findings = run_checks(&root);
    if findings.is_empty() {
        return ExitCode::SUCCESS;
    }
    for finding in &findings {
        eprintln!("{finding}");
    }
    ExitCode::FAILURE
}

/// Run every architecture check against the workspace at `root`,
/// returning a flat list of human-readable findings (empty == pass).
fn run_checks(root: &Path) -> Vec<String> {
    let mut findings = check_crate_graph(root);
    findings.extend(check_source_rule_crate_coverage(root));
    for crate_name in SCANNED_CRATES {
        let crate_dir = root.join("crates").join(crate_name);
        findings.extend(check_crate_sources(crate_name, &crate_dir));
    }
    findings.extend(check_zero_beads_knowledge(root));
    findings.extend(check_tmux_socket_scoping(root));
    findings.extend(check_workspace_rust_version_matches_toolchain(root));
    findings.extend(check_fabro_image_rust_toolchain(root));
    findings.extend(check_first_party_python_import_supply(root));
    findings
}

// ---------------------------------------------------------------------------
// Workspace Rust MSRV/toolchain lockstep.
// ---------------------------------------------------------------------------

fn check_workspace_rust_version_matches_toolchain(root: &Path) -> Vec<String> {
    match workspace_rust_version_matches_toolchain(root) {
        Ok(()) => Vec::new(),
        Err(error) => vec![error],
    }
}

fn workspace_rust_version_matches_toolchain(root: &Path) -> Result<(), String> {
    let rust_version = workspace_rust_version(root)?;
    let toolchain = expected_rust_toolchain(root)?;
    let rust_version = VersionComponents::parse(&rust_version)
        .map_err(|error| format!("Cargo.toml workspace.package.rust-version: {error}"))?;
    let channel = VersionComponents::parse(&toolchain.channel)
        .map_err(|error| format!("rust-toolchain.toml toolchain.channel: {error}"))?;
    if !rust_version.is_component_prefix_of(&channel) {
        return Err(format!(
            "Cargo.toml workspace.package.rust-version `{}` does not agree with \
             rust-toolchain.toml toolchain.channel `{}` by version component",
            rust_version.raw, channel.raw
        ));
    }
    Ok(())
}

fn workspace_rust_version(root: &Path) -> Result<String, String> {
    let path = root.join("Cargo.toml");
    let source = fs::read_to_string(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let value: toml::Value = toml::from_str(&source)
        .map_err(|error| format!("could not parse {}: {error}", path.display()))?;
    let workspace_package = value
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(toml::Value::as_table)
        .ok_or_else(|| format!("{}: missing workspace.package table", path.display()))?;
    let rust_version = workspace_package
        .get("rust-version")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| format!("{}: missing workspace.package.rust-version", path.display()))?;
    Ok(rust_version.to_owned())
}

#[derive(Debug, PartialEq, Eq)]
struct VersionComponents {
    raw: String,
    components: Vec<u64>,
}

impl VersionComponents {
    fn parse(raw: &str) -> Result<Self, String> {
        let components = raw
            .split('.')
            .map(|component| {
                if component.is_empty() {
                    return Err(format!("version `{raw}` contains an empty component"));
                }
                component.parse::<u64>().map_err(|error| {
                    format!("version `{raw}` contains non-numeric component `{component}`: {error}")
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if components.is_empty() {
            return Err("version is empty".to_owned());
        }
        Ok(Self {
            raw: raw.to_owned(),
            components,
        })
    }

    fn is_component_prefix_of(&self, other: &Self) -> bool {
        other.components.starts_with(&self.components)
    }
}

// ---------------------------------------------------------------------------
// Fabro sandbox image lockstep (actual image probe).
// ---------------------------------------------------------------------------

/// The committed Fabro workflow whose Docker image must carry the same Rust
/// toolchain this repo declares.
const FABRO_IMPLEMENT_WORKFLOW: &str = ".fabro/workflows/implement-work-item/workflow.toml";

fn check_fabro_image_rust_toolchain(root: &Path) -> Vec<String> {
    check_fabro_image_rust_toolchain_with_probe(root, probe_image_rust_toolchain)
}

fn check_fabro_image_rust_toolchain_with_probe(
    root: &Path,
    probe: impl FnOnce(&str) -> Result<String, String>,
) -> Vec<String> {
    let mut findings = Vec::new();
    let expected = match expected_rust_toolchain(root) {
        Ok(expected) => expected,
        Err(error) => {
            findings.push(error);
            return findings;
        }
    };
    let image = match fabro_python_rust_image(root) {
        Ok(image) => image,
        Err(error) => {
            findings.push(error);
            return findings;
        }
    };
    let probe_output = match probe(&image) {
        Ok(output) => output,
        Err(error) => {
            findings.push(format!(
                "Fabro sandbox Rust probe could not read image `{image}` — refusing \
                 to pass without interrogating the actual configured image: {error}"
            ));
            return findings;
        }
    };
    let actual = match observed_rust_toolchain(&probe_output) {
        Ok(actual) => actual,
        Err(error) => {
            findings.push(format!(
                "Fabro sandbox Rust probe for image `{image}` produced no usable \
                 Rust evidence — refusing to pass without rustc/clippy/rustfmt \
                 output: {error}"
            ));
            return findings;
        }
    };
    if actual.rustc_version != expected.channel {
        findings.push(format!(
            "Fabro sandbox image `{image}` bakes rustc {}, but rust-toolchain.toml \
             pins channel {}",
            actual.rustc_version, expected.channel
        ));
    }
    for component in &expected.components {
        if !actual.components.contains(component) {
            findings.push(format!(
                "Fabro sandbox image `{image}` does not prove required \
                 rust-toolchain.toml component `{component}` is installed"
            ));
        }
    }
    findings
}

#[derive(Debug, PartialEq, Eq)]
struct ExpectedRustToolchain {
    channel: String,
    components: BTreeSet<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct ObservedRustToolchain {
    rustc_version: String,
    components: BTreeSet<String>,
}

fn expected_rust_toolchain(root: &Path) -> Result<ExpectedRustToolchain, String> {
    let path = root.join("rust-toolchain.toml");
    let source = fs::read_to_string(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let value: toml::Value = toml::from_str(&source)
        .map_err(|error| format!("could not parse {}: {error}", path.display()))?;
    let toolchain = value
        .get("toolchain")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| format!("{}: missing [toolchain] table", path.display()))?;
    let channel = toolchain
        .get("channel")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| format!("{}: missing toolchain.channel", path.display()))?;
    let components = toolchain
        .get("components")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| format!("{}: missing toolchain.components", path.display()))?
        .iter()
        .map(|component| {
            component.as_str().map(str::to_owned).ok_or_else(|| {
                format!(
                    "{}: every toolchain.components entry must be a string",
                    path.display()
                )
            })
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    Ok(ExpectedRustToolchain {
        channel: channel.to_owned(),
        components,
    })
}

fn fabro_python_rust_image(root: &Path) -> Result<String, String> {
    let path = root.join(FABRO_IMPLEMENT_WORKFLOW);
    let source = fs::read_to_string(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let value: toml::Value = toml::from_str(&source)
        .map_err(|error| format!("could not parse {}: {error}", path.display()))?;
    let image = value
        .get("environments")
        .and_then(|environments| environments.get("livespec-ci"))
        .and_then(|environment| environment.get("image"))
        .and_then(|image| image.get("docker"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| {
            format!(
                "{}: missing environments.livespec-ci.image.docker",
                path.display()
            )
        })?;
    if !image.contains("livespec-fabro-sandbox:python-rust-agent-") {
        return Err(format!(
            "{}: configured Fabro sandbox image `{image}` is not the \
             python-rust-agent image whose baked Rust this guard covers",
            path.display()
        ));
    }
    Ok(image.to_owned())
}

fn probe_image_rust_toolchain(image: &str) -> Result<String, String> {
    let output = Command::new("docker")
        .args([
            "run",
            "--rm",
            image,
            "sh",
            "-eu",
            "-c",
            "rustc --version && cargo clippy --version && rustfmt --version",
        ])
        .output();
    let output = match output {
        Ok(output) => output,
        Err(error) => {
            return probe_image_config_history(image).map_err(|fallback_error| {
                format!(
                    "could not execute `docker run`: {error}; OCI image-config fallback \
                     also failed: {fallback_error}"
                )
            });
        }
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return probe_image_config_history(image).map_err(|fallback_error| {
            format!(
                "`docker run --rm {image} ...` exited with {}. stdout: {} stderr: {}; \
                 OCI image-config fallback also failed: {fallback_error}",
                output.status,
                stdout.trim(),
                stderr.trim()
            )
        });
    }
    String::from_utf8(output.stdout).map_err(|error| format!("probe output was not UTF-8: {error}"))
}

fn probe_image_config_history(image: &str) -> Result<String, String> {
    let reference = GhcrImageReference::parse(image)?;
    let token_url = format!(
        "https://ghcr.io/token?scope=repository:{}:pull&service=ghcr.io",
        reference.repository
    );
    let token_json = curl(&token_url, &[])?;
    let token = json_field(&token_json, "token")?;
    let manifest_url = format!(
        "https://ghcr.io/v2/{}/manifests/{}",
        reference.repository, reference.tag
    );
    let manifest_json = curl(
        &manifest_url,
        &[
            "-H",
            &format!("Authorization: Bearer {token}"),
            "-H",
            "Accept: application/vnd.oci.image.manifest.v1+json,application/vnd.docker.distribution.manifest.v2+json",
        ],
    )?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_json)
        .map_err(|error| format!("could not parse image manifest JSON: {error}"))?;
    let config_digest = manifest
        .get("config")
        .and_then(|config| config.get("digest"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "image manifest did not carry a config digest".to_owned())?;
    let config_url = format!(
        "https://ghcr.io/v2/{}/blobs/{config_digest}",
        reference.repository
    );
    let config_json = curl(
        &config_url,
        &["-H", &format!("Authorization: Bearer {token}")],
    )?;
    let config: serde_json::Value = serde_json::from_str(&config_json)
        .map_err(|error| format!("could not parse image config JSON: {error}"))?;
    observed_rust_toolchain_from_image_config(&config)
}

struct GhcrImageReference {
    repository: String,
    tag: String,
}

impl GhcrImageReference {
    fn parse(image: &str) -> Result<Self, String> {
        let Some(rest) = image.strip_prefix("ghcr.io/") else {
            return Err(format!("unsupported image registry in `{image}`"));
        };
        let Some((repository, tag)) = rest.rsplit_once(':') else {
            return Err(format!("image `{image}` has no tag"));
        };
        if repository.is_empty() || tag.is_empty() {
            return Err(format!("image `{image}` must include repository and tag"));
        }
        Ok(Self {
            repository: repository.to_owned(),
            tag: tag.to_owned(),
        })
    }
}

fn curl(url: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new("curl")
        .args(["-fsSL", "--retry", "3"])
        .args(args)
        .arg(url)
        .output()
        .map_err(|error| format!("could not execute `curl`: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "`curl` exited with {} while reading {url}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|error| format!("curl output was not UTF-8: {error}"))
}

fn json_field(json: &str, field: &str) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|error| format!("could not parse JSON: {error}"))?;
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("JSON response did not carry string field `{field}`"))
}

fn observed_rust_toolchain_from_image_config(config: &serde_json::Value) -> Result<String, String> {
    let mut rustc_version = None;
    let mut components = BTreeSet::new();
    let Some(history) = config.get("history").and_then(serde_json::Value::as_array) else {
        return Err("image config did not carry history entries".to_owned());
    };
    for created_by in history
        .iter()
        .filter_map(|entry| entry.get("created_by").and_then(serde_json::Value::as_str))
    {
        if rustc_version.is_none() {
            rustc_version = rust_version_from_history(created_by);
        }
        components.extend(components_from_history(created_by));
    }
    let version = rustc_version.ok_or_else(|| {
        "image config history did not prove which Rust version was baked".to_owned()
    })?;
    if components.is_empty() {
        return Err("image config history did not prove any Rust components were baked".to_owned());
    }
    let mut output = format!("rustc {version} (from image config history)\n");
    for component in components {
        output.push_str(&component);
        output.push_str(" (from image config history)\n");
    }
    Ok(output)
}

fn rust_version_from_history(created_by: &str) -> Option<String> {
    created_by.split_whitespace().find_map(|token| {
        token
            .strip_prefix("RUST_VERSION=")
            .filter(|version| !version.is_empty())
            .map(str::to_owned)
    })
}

fn components_from_history(created_by: &str) -> BTreeSet<String> {
    let mut components = BTreeSet::new();
    let mut tokens = created_by.split_whitespace();
    while let Some(token) = tokens.next() {
        let component_list = token
            .strip_prefix("--component=")
            .or_else(|| (token == "--component").then(|| tokens.next()).flatten());
        if let Some(component_list) = component_list {
            for component in component_list.split(',') {
                if matches!(component, "clippy" | "rustfmt") {
                    components.insert(component.to_owned());
                }
            }
        }
    }
    components
}

fn observed_rust_toolchain(output: &str) -> Result<ObservedRustToolchain, String> {
    let mut rustc_version = None;
    let mut components = BTreeSet::new();
    for line in output.lines() {
        let line = line.trim();
        if let Some(version) = line.strip_prefix("rustc ") {
            rustc_version = Some(
                version
                    .split_whitespace()
                    .next()
                    .ok_or_else(|| format!("malformed rustc version line `{line}`"))?
                    .to_owned(),
            );
        } else if line.starts_with("clippy ") {
            components.insert("clippy".to_owned());
        } else if line.starts_with("rustfmt ") {
            components.insert("rustfmt".to_owned());
        }
    }
    let rustc_version = rustc_version.ok_or_else(|| "missing `rustc --version` line".to_owned())?;
    Ok(ObservedRustToolchain {
        rustc_version,
        components,
    })
}

// ---------------------------------------------------------------------------
// Crate-graph rules (structured, from `cargo metadata`).
// ---------------------------------------------------------------------------

/// A workspace crate reduced to the bits the layering rules need: its
/// name, its workspace-member dependencies, and its external (registry)
/// dependencies. Dev-dependencies are excluded — layering constrains the
/// production dependency direction, not test-only edges.
struct CrateNode {
    name: String,
    workspace_deps: Vec<String>,
    external_deps: Vec<String>,
}

fn check_crate_graph(root: &Path) -> Vec<String> {
    let manifest = root.join("Cargo.toml");
    let metadata = match MetadataCommand::new().manifest_path(&manifest).exec() {
        Ok(metadata) => metadata,
        Err(error) => return vec![format!("could not load cargo metadata: {error}")],
    };
    let member_names: BTreeSet<String> = metadata
        .workspace_packages()
        .iter()
        .map(|package| package.name.to_string())
        .collect();
    let nodes: Vec<CrateNode> = metadata
        .workspace_packages()
        .iter()
        .map(|package| {
            let mut workspace_deps = Vec::new();
            let mut external_deps = Vec::new();
            for dependency in &package.dependencies {
                if dependency.kind == DependencyKind::Development {
                    continue;
                }
                if member_names.contains(dependency.name.as_str()) {
                    workspace_deps.push(dependency.name.clone());
                } else {
                    external_deps.push(dependency.name.clone());
                }
            }
            CrateNode {
                name: package.name.to_string(),
                workspace_deps,
                external_deps,
            }
        })
        .collect();
    let mut findings = check_crate_graph_non_vacuity(&nodes);
    if findings.is_empty() {
        findings.extend(check_layering(&nodes));
    }
    findings
}

fn check_crate_graph_non_vacuity(nodes: &[CrateNode]) -> Vec<String> {
    if nodes.is_empty() {
        return vec![
            "crate graph check read zero workspace packages from cargo metadata — refusing to \
             pass without a non-empty workspace package graph"
                .to_owned(),
        ];
    }
    Vec::new()
}

/// Pure layering rule set over the reduced crate graph.
fn check_layering(nodes: &[CrateNode]) -> Vec<String> {
    let mut findings = Vec::new();
    for node in nodes {
        let allowed = allowed_workspace_deps(&node.name);
        for dependency in &node.workspace_deps {
            if !allowed.contains(&dependency.as_str()) {
                findings.push(format!(
                    "crate `{}` must not depend on workspace crate `{dependency}` \
                     (forbidden layering direction)",
                    node.name
                ));
            }
        }
        if is_purity_constrained(&node.name) {
            for dependency in &node.external_deps {
                if FORBIDDEN_INFRA_DEPENDENCIES.contains(&dependency.as_str()) {
                    findings.push(format!(
                        "crate `{}` must not depend on infrastructure crate `{dependency}` \
                         (domain/UI purity)",
                        node.name
                    ));
                }
            }
        }
    }
    findings
}

/// The workspace crates a given crate is allowed to depend on, keyed by
/// cargo **package** name (not directory). The allow-list encodes the
/// DDD layering invariants: domain depends on nothing, application only
/// on domain, the outer crates on the inner ones, the composition-root
/// binary on all product crates, and nobody on `console-arch-check`.
///
/// `livespec-console-beads-fabro` is the package name of the cli binary
/// (its directory is `crates/console-cli`); it is the composition root.
fn allowed_workspace_deps(crate_name: &str) -> &'static [&'static str] {
    match crate_name {
        "console-application" => &["console-domain"],
        "console-eventstore"
        | "console-tui"
        | "console-arch-check"
        | "console-completeness-check" => &["console-domain", "console-application"],
        "livespec-console-beads-fabro" => &[
            "console-domain",
            "console-application",
            "console-eventstore",
            "console-tui",
        ],
        // console-domain (and any unrecognized crate) may depend on no
        // workspace crate.
        _ => &[],
    }
}

/// Domain and UI crates are constrained to use no infrastructure
/// dependency directly (the event store, HTTP, async runtimes, or
/// source-system I/O).
fn is_purity_constrained(crate_name: &str) -> bool {
    matches!(crate_name, "console-domain" | "console-tui")
}

// ---------------------------------------------------------------------------
// Source-level rules (AST, via `syn`).
// ---------------------------------------------------------------------------

fn check_crate_sources(crate_name: &str, crate_dir: &Path) -> Vec<String> {
    let mut findings = Vec::new();
    for path in rust_files(crate_dir) {
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => {
                findings.push(format!("could not read {}: {error}", path.display()));
                continue;
            }
        };
        let file = match syn::parse_file(&source) {
            Ok(file) => file,
            Err(error) => {
                findings.push(format!("could not parse {}: {error}", path.display()));
                continue;
            }
        };
        let display = path.display().to_string();
        if is_entrypoint(&path) {
            findings.extend(check_forbid_unsafe(&file, &display));
        }
        findings.extend(check_unwrap_expect(&file, &display));
        findings.extend(check_type_placement(crate_name, &file, &display));
        findings.extend(check_adapter_isolation(&file, &display));
        findings.extend(check_registry_bypass(crate_name, &file, &display));
    }
    findings
}

fn check_source_rule_crate_coverage(root: &Path) -> Vec<String> {
    let manifest = root.join("Cargo.toml");
    let metadata = match MetadataCommand::new().manifest_path(&manifest).exec() {
        Ok(metadata) => metadata,
        Err(error) => return vec![format!("could not load cargo metadata: {error}")],
    };
    let mut findings = Vec::new();
    let mut workspace_crates = BTreeSet::new();
    for package in metadata.workspace_packages() {
        let Some(crate_dir) = package
            .manifest_path
            .parent()
            .and_then(|path| path.file_name())
        else {
            findings.push(format!(
                "could not derive source-rule crate directory for workspace package `{}` \
                 from manifest path `{}`",
                package.name, package.manifest_path
            ));
            continue;
        };
        workspace_crates.insert(crate_dir.to_owned());
    }
    findings.extend(check_source_rule_crate_coverage_for_names(
        &workspace_crates,
    ));
    findings
}

fn check_source_rule_crate_coverage_for_names(workspace_crates: &BTreeSet<String>) -> Vec<String> {
    let mut findings = Vec::new();
    if workspace_crates.is_empty() {
        findings.push(
            "source-rule crate coverage check read zero workspace packages from cargo metadata — \
             refusing to pass without a non-empty workspace package set"
                .to_owned(),
        );
        return findings;
    }

    let scanned: BTreeSet<&str> = SCANNED_CRATES.iter().copied().collect();
    let excluded: BTreeSet<&str> = SOURCE_RULE_EXCLUDED_CRATES
        .iter()
        .map(|exclusion| exclusion.name)
        .collect();

    for crate_name in workspace_crates {
        if !scanned.contains(crate_name.as_str()) && !excluded.contains(crate_name.as_str()) {
            findings.push(format!(
                "workspace crate `{crate_name}` is not covered by source-level architecture \
                 rules: add it to SCANNED_CRATES or declare it in SOURCE_RULE_EXCLUDED_CRATES \
                 with a reason"
            ));
        }
    }

    for crate_name in &scanned {
        if !workspace_crates.contains(*crate_name) {
            findings.push(format!(
                "SCANNED_CRATES names `{crate_name}`, but cargo metadata has no such workspace \
                 package"
            ));
        }
    }

    for exclusion in SOURCE_RULE_EXCLUDED_CRATES {
        if exclusion.reason.trim().is_empty() {
            findings.push(format!(
                "SOURCE_RULE_EXCLUDED_CRATES entry `{}` must carry a non-empty reason",
                exclusion.name
            ));
        }
        if !workspace_crates.contains(exclusion.name) {
            findings.push(format!(
                "SOURCE_RULE_EXCLUDED_CRATES names `{}`, but cargo metadata has no such \
                 workspace package",
                exclusion.name
            ));
        }
    }

    let duplicate_names = duplicate_source_rule_members();
    findings.extend(duplicate_names.into_iter().map(|crate_name| {
        format!(
            "workspace crate `{crate_name}` appears in both SCANNED_CRATES and \
             SOURCE_RULE_EXCLUDED_CRATES"
        )
    }));
    findings
}

fn duplicate_source_rule_members() -> Vec<&'static str> {
    let excluded: BTreeSet<&str> = SOURCE_RULE_EXCLUDED_CRATES
        .iter()
        .map(|exclusion| exclusion.name)
        .collect();
    SCANNED_CRATES
        .iter()
        .copied()
        .filter(|crate_name| excluded.contains(crate_name))
        .collect()
}

/// Rule: every `tmux` invocation in the workspace must run on a PRIVATE socket
/// — `TMUX_TMPDIR=<per-run-scratch>` plus `-L <private-label>` among the server
/// options that precede the tmux sub-command.
///
/// The rule is SUSPECT-BY-DEFAULT, and deliberately so. An earlier revision
/// peeled three CLOSED allow-lists (a six-entry sub-command list, a
/// literal-`"tmux"` program test, a single scanned directory) and inspected one
/// argument position, so anything that displaced the hazard off an enumerated
/// shape — an unlisted sub-command, a renamed binding, a non-chained builder, a
/// moved directory — silently disabled the rule instead of tripping it. Here a
/// tmux invocation needs scope unless it is PROVABLY harmless, an unresolvable
/// tmux-shaped builder is reported rather than dismissed, argument VALUES are
/// validated rather than merely counted, and a walk that turns up no Rust files
/// fails instead of passing vacuously.
fn check_tmux_socket_scoping(root: &Path) -> Vec<String> {
    let (paths, mut findings) = rust_files_for_tmux_scan(root);
    if paths.is_empty() {
        findings.push(format!(
            "tmux socket-scoping scan found no Rust files under {} — the scan root moved \
             or the walk is broken; refusing to pass without having read anything",
            root.display()
        ));
        return findings;
    }
    for path in paths {
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => {
                findings.push(format!("could not read {}: {error}", path.display()));
                continue;
            }
        };
        findings.extend(check_tmux_socket_scoping_source(
            &path.display().to_string(),
            &source,
        ));
    }
    findings
}

/// Directory names the tmux scan never descends into: `target` holds build
/// artifacts and vendored third-party sources, `.git` holds object storage, and
/// `tmp` is maintainer-owned scratch that may hold unrelated checkouts. The
/// list is a skip-list rather than a scan-list on purpose — a NEW source
/// directory is covered by default instead of falling outside an enumeration.
/// `.venv` joins the skip-list now that an unreadable or symlinked path is
/// REPORTED rather than silently passed: it is a `uv`-managed virtualenv holding
/// no first-party Rust, it is gitignored, and it is materialized DURING
/// `just check` (by `check-baseline`), so it legitimately carries interpreter
/// symlinks (`bin/python3`, `lib64`) that would otherwise be reported on every
/// run after the first.
const TMUX_SCAN_SKIPPED_DIRS: &[&str] = &["target", ".git", "tmp", ".venv"];

/// The symlink's target, if and only if it resolves INSIDE `root`.
///
/// Separates the two kinds of symlink the tmux scan meets. An in-tree link
/// (`CLAUDE.md -> AGENTS.md`) cannot leave the repository, so the walk FOLLOWS
/// it to the returned path and scans the content. A link pointing outside the
/// root genuinely hides content from the scan and must stay a finding.
///
/// Returning the target rather than a bool is what lets the caller follow it.
/// An earlier revision skipped in-tree links silently on the reasoning that the
/// walk reaches their targets anyway under the real path — true for
/// `CLAUDE.md`, FALSE in general: a link into a skip-listed directory
/// (`target/`, `.git/`, `tmp/`, `.venv/`) has no other route in, so skipping it
/// left content unscanned with no finding. That is the same vacuity this rule
/// exists to cure.
///
/// Fails CLOSED on every uncertainty. `canonicalize` resolves the whole chain
/// and errors on a dangling link, so an unresolvable target answers `None` and
/// is reported. `root` is canonicalized too, because a `root` reached through a
/// symlinked parent would otherwise never prefix-match its own resolved children.
fn symlink_target_within(path: &Path, root: &Path) -> Option<PathBuf> {
    let (Ok(target), Ok(resolved_root)) = (fs::canonicalize(path), fs::canonicalize(root)) else {
        return None;
    };
    target.starts_with(&resolved_root).then_some(target)
}

/// Collect the Rust files the tmux rule scans, plus a finding for every part of
/// the tree the walk could NOT read.
///
/// The skips themselves are correct — following symlinks risks a cycle or an
/// escape from the repository, and an unreadable directory cannot be walked —
/// but an earlier revision performed them SILENTLY. That fails open: symlink
/// `crates/console-cli/tests` and the whole governed harness leaves the scan with
/// no finding at all, while the zero-file guard stays quiet because the rest of
/// the repository still yields `.rs` files. Reporting the skip keeps the rule
/// suspect-by-default: coverage that cannot be established is a finding, not a
/// pass.
fn rust_files_for_tmux_scan(root: &Path) -> (Vec<PathBuf>, Vec<String>) {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    let mut findings = Vec::new();
    // Canonical paths already enqueued. Following in-tree symlinks makes the
    // walk a graph rather than a tree, so this is what keeps a link cycle from
    // looping forever and stops an already-walked subtree being re-reported.
    let mut visited = std::collections::HashSet::new();
    while let Some(path) = pending.pop() {
        // `symlink_metadata` does not follow links, so a symlinked directory
        // can never send the walk round a cycle or out of the repository.
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            // A missing ROOT is already reported, more clearly, by the zero-files
            // guard in the caller; reporting it twice is noise. Anything else that
            // cannot be stat'd is a genuine hole in the walk's coverage.
            if path != root {
                findings.push(format!(
                    "tmux socket-scoping scan could not stat {} — the walk cannot \
                     establish whether it holds tmux invocations",
                    path.display()
                ));
            }
            continue;
        };
        if metadata.is_symlink() {
            // The hazard this arm guards is stated in its own finding: following
            // the link could LEAVE THE REPOSITORY. A link resolving back INSIDE
            // the root cannot, so it is FOLLOWED rather than refused —
            // `CLAUDE.md -> AGENTS.md` is the in-tree case that proved the
            // blanket refusal wrong, landing after this rule was written and
            // failing a scan that had nothing to say about it.
            //
            // Following, not skipping, is the point. Skipping in-tree links
            // would leave a link into a skip-listed directory unscanned and
            // unreported — the vacuity this rule exists to cure. `visited`
            // makes that safe: canonical paths dedupe, so a link cycle
            // terminates and a link to an already-walked subtree costs nothing.
            match symlink_target_within(&path, root) {
                Some(target) => pending.push(target),
                None => findings.push(format!(
                    "tmux socket-scoping scan skipped the symlink {} — its target \
                     does not resolve inside the repository, so its contents are \
                     UNSCANNED; move the real directory into the tree or add it to \
                     the skip-list deliberately",
                    path.display()
                )),
            }
            continue;
        }
        // Dedupe REAL paths only, and only after the symlink arm. Canonicalizing
        // a symlink yields its TARGET, so checking here first would mark the
        // target visited on behalf of the link and then skip the target itself —
        // silently unscanning exactly the file the arm just went to the trouble
        // of following.
        if let Ok(canonical) = fs::canonicalize(&path)
            && !visited.insert(canonical)
        {
            continue;
        }
        if metadata.is_dir() {
            let skipped = path != root
                && path
                    .file_name()
                    .is_some_and(|name| TMUX_SCAN_SKIPPED_DIRS.iter().any(|entry| name == *entry));
            if skipped {
                continue;
            }
            let Ok(entries) = fs::read_dir(&path) else {
                findings.push(format!(
                    "tmux socket-scoping scan could not read the directory {} — its \
                     contents are UNSCANNED, so the rule cannot claim coverage of them",
                    path.display()
                ));
                continue;
            };
            for entry in entries.flatten() {
                pending.push(entry.path());
            }
            continue;
        }
        if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    (files, findings)
}

fn check_tmux_socket_scoping_source(display: &str, source: &str) -> Vec<String> {
    let file = match syn::parse_file(source) {
        Ok(file) => file,
        Err(error) => return vec![format!("could not parse {display}: {error}")],
    };
    let mut visitor = TmuxSocketScopeVisitor {
        display,
        findings: Vec::new(),
        scopes: Vec::new(),
    };
    visitor.visit_file(&file);
    visitor.findings
}

struct TmuxSocketScopeVisitor<'a> {
    display: &'a str,
    findings: Vec<String>,
    /// Lexical scopes of `let` bindings holding a `Command` builder, so the
    /// ordinary non-chained idiom (`let mut cmd = Command::new(tmux);
    /// cmd.args(...); cmd.status();`) is analyzed exactly like the chained form
    /// rather than being invisible to the check.
    scopes: Vec<BTreeMap<String, TmuxCommandInvocation>>,
}

impl TmuxSocketScopeVisitor<'_> {
    fn lookup(&self, name: &str) -> Option<&TmuxCommandInvocation> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    /// Rebuild the `Command` an expression denotes, following both an inline
    /// `Command::new(...)` chain and a chain rooted at a `let`-bound builder.
    /// `None` means "this is not a command builder I can follow".
    fn resolve(&self, expr: &syn::Expr) -> Option<TmuxCommandInvocation> {
        match strip_wrappers(expr) {
            syn::Expr::MethodCall(method_call) => {
                let mut invocation = self.resolve(&method_call.receiver)?;
                invocation.record_method_call(method_call);
                Some(invocation)
            }
            syn::Expr::Call(call) if is_command_new_call(call) => {
                Some(TmuxCommandInvocation::new(call.args.first()))
            }
            other => bare_ident(other).and_then(|name| self.lookup(&name).cloned()),
        }
    }

    /// Fold a statement-level builder chain (`cmd.args(...);`) back into the
    /// binding it mutates, so a launcher called on that binding later in the
    /// block sees the accumulated arguments. Chains that END in a launcher are
    /// left alone — `visit_expr_method_call` evaluates those against the
    /// binding as it stands.
    fn apply_builder_statement(&mut self, expr: &syn::Expr) {
        let mut chain = Vec::new();
        let mut cursor = strip_wrappers(expr);
        while let syn::Expr::MethodCall(method_call) = cursor {
            chain.push(method_call);
            cursor = strip_wrappers(&method_call.receiver);
        }
        if chain
            .first()
            .is_some_and(|outermost| is_launcher(&outermost.method.to_string()))
        {
            return;
        }
        let Some(name) = bare_ident(cursor) else {
            return;
        };
        // `chain` runs outermost-first; replay it in source order.
        for method_call in chain.iter().rev() {
            if let Some(scope) = self
                .scopes
                .iter_mut()
                .rev()
                .find(|scope| scope.contains_key(&name))
                && let Some(invocation) = scope.get_mut(&name)
            {
                invocation.record_method_call(method_call);
            }
        }
    }
}

impl<'ast> Visit<'ast> for TmuxSocketScopeVisitor<'_> {
    fn visit_block(&mut self, node: &'ast syn::Block) {
        self.scopes.push(BTreeMap::new());
        for statement in &node.stmts {
            match statement {
                syn::Stmt::Local(local) => {
                    if let Some(init) = &local.init {
                        self.apply_builder_statement(&init.expr);
                        syn::visit::visit_stmt(self, statement);
                        // Bind AFTER visiting, so `let cmd = cmd.arg(..)` reads
                        // the old binding on the right-hand side first.
                        if let Some(name) = local_binding_ident(local)
                            && let Some(invocation) = self.resolve(&init.expr)
                            && let Some(scope) = self.scopes.last_mut()
                        {
                            scope.insert(name, invocation);
                        }
                        continue;
                    }
                }
                syn::Stmt::Expr(expr, _) => self.apply_builder_statement(expr),
                syn::Stmt::Item(_) | syn::Stmt::Macro(_) => {}
            }
            syn::visit::visit_stmt(self, statement);
        }
        self.scopes.pop();
    }

    /// Descend into MACRO BODIES, which `syn` leaves as an opaque token stream.
    ///
    /// Without this the rule has a hole exactly where it is least affordable: the
    /// file it governs is a TEST file, and wrapping a command in `assert!(...)` is
    /// the default idiom there, so
    /// `assert!(Command::new(&tmux).args(["-L", "default", "kill-server"]).status()?.success())`
    /// would compile, hit the shared server, and be scanned by nothing. Re-parse
    /// the tokens as a comma-separated expression list and visit each; a body that
    /// mentions tmux but cannot be parsed is REPORTED rather than skipped, so an
    /// unparsable macro cannot become a new way to hide.
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        let parsed = node.parse_body_with(
            syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated,
        );
        match parsed {
            Ok(arguments) => {
                for argument in &arguments {
                    self.apply_builder_statement(argument);
                    syn::visit::visit_expr(self, argument);
                }
            }
            Err(_) => {
                if node.tokens.to_string().contains("tmux") {
                    self.findings.push(format!(
                        "{}: a macro body mentions tmux but does not parse as an \
                         expression list, so its socket scoping cannot be verified — \
                         build the command outside the macro where the check can see it",
                        self.display
                    ));
                }
            }
        }
        syn::visit::visit_macro(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if is_launcher(&node.method.to_string()) {
            match self.resolve(&node.receiver) {
                Some(invocation) => {
                    if let Some(reason) = invocation.socket_scope_violation() {
                        self.findings.push(format!("{}: {reason}", self.display));
                    }
                }
                // An unfollowable builder is only interesting when it NAMES
                // tmux; that keeps a renamed helper (`tmux_command().status()`)
                // suspect without dragging in every unrelated subprocess.
                None => {
                    if expr_names_tmux(&node.receiver) {
                        self.findings.push(format!(
                            "{}: tmux-shaped command builder cannot be resolved to a \
                             `Command::new(...)` chain, so its socket scoping cannot be \
                             verified — build the command where the check can see it",
                            self.display
                        ));
                    }
                }
            }
        }
        syn::visit::visit_expr_method_call(self, node);
    }
}

/// The standard ways to launch a built `Command`. `spawn` matters as much as
/// `output`/`status`: it is the natural choice for a long-lived tmux server.
/// `exec` (`std::os::unix::process::CommandExt`) replaces the current process
/// image and never returns, so it launches just as surely as the others — an
/// earlier revision omitted it, leaving a real launcher uninspected.
fn is_launcher(method: &str) -> bool {
    matches!(method, "output" | "status" | "spawn" | "exec")
}

/// What `Command::new(...)` was handed.
#[derive(Clone, Copy)]
enum ProgramKind {
    /// Definitely tmux — a literal naming it, or an expression whose name says
    /// so (`tmux_bin`, `resolve_tmux()`, `paths.tmux_path`).
    Tmux,
    /// Definitely something else: a literal naming another program.
    Other,
    /// An expression this check cannot read. Suspect as soon as the arguments
    /// look tmux-shaped, rather than assumed innocent.
    Unknown,
}

/// How `TMUX_TMPDIR` was set on the command under analysis.
#[derive(Clone)]
enum TmuxTmpdir {
    /// Never set, so the socket file lands in the shared default namespace.
    Unset,
    /// Set from an expression this check cannot read — which is exactly what a
    /// genuine per-run scratch path looks like in source, so it is accepted.
    Runtime,
    /// Set from a literal, whose VALUE is validated.
    Literal(String),
}

#[derive(Clone)]
struct TmuxCommandInvocation {
    program: ProgramKind,
    tmux_tmpdir: TmuxTmpdir,
    args: Vec<Option<String>>,
}

impl TmuxCommandInvocation {
    fn new(program: Option<&syn::Expr>) -> Self {
        Self {
            program: program.map_or(ProgramKind::Unknown, classify_program),
            tmux_tmpdir: TmuxTmpdir::Unset,
            args: Vec::new(),
        }
    }

    fn record_method_call(&mut self, method_call: &syn::ExprMethodCall) {
        let method = method_call.method.to_string();
        match method.as_str() {
            "arg" => {
                if let Some(argument) = method_call.args.first() {
                    self.args.push(string_literal(argument));
                }
            }
            "args" => {
                if let Some(argument) = method_call.args.first() {
                    self.args.extend(string_literals(argument));
                }
            }
            "env" => {
                if names_tmux_tmpdir(method_call.args.first()) {
                    self.tmux_tmpdir = method_call
                        .args
                        .get(1)
                        .and_then(string_literal)
                        .map_or(TmuxTmpdir::Runtime, TmuxTmpdir::Literal);
                }
            }
            "env_remove" => {
                if names_tmux_tmpdir(method_call.args.first()) {
                    self.tmux_tmpdir = TmuxTmpdir::Unset;
                }
            }
            "env_clear" => self.tmux_tmpdir = TmuxTmpdir::Unset,
            _ => {}
        }
    }

    /// Why this invocation breaks the private-socket rule, or `None` when it is
    /// in the clear. Every reason is reported together so one finding names
    /// everything wrong with the invocation.
    fn socket_scope_violation(&self) -> Option<String> {
        if !self.needs_socket_scope() {
            return None;
        }
        let reasons: Vec<String> = [self.tmux_tmpdir_violation(), self.socket_label_violation()]
            .into_iter()
            .flatten()
            .collect();
        if reasons.is_empty() {
            return None;
        }
        Some(format!(
            "tmux invocation must run on a private socket: {}",
            reasons.join("; ")
        ))
    }

    /// Whether the rule applies. A definite tmux program ALWAYS needs scope
    /// unless the whole invocation is provably harmless — an unrecognized
    /// sub-command can never mean "the rule does not apply". An unresolved
    /// program needs scope the moment its arguments look tmux-shaped.
    fn needs_socket_scope(&self) -> bool {
        if self.is_provably_safe_query() {
            return false;
        }
        match self.program {
            ProgramKind::Tmux => true,
            // A program that is definitively NOT tmux still reaches tmux when it
            // is an interpreter and tmux is buried in an argument
            // (`sh -c "tmux kill-server"`). An earlier revision exempted this arm
            // unconditionally, so a resolved-but-wrong program was trusted MORE
            // than an unresolvable one — the shape-shift this rule exists to deny.
            ProgramKind::Other => self.args_mention_tmux(),
            ProgramKind::Unknown => self.args_look_tmux_shaped() || self.args_mention_tmux(),
        }
    }

    /// Whether any argument LITERAL mentions tmux at all.
    ///
    /// Deliberately broader than [`args_look_tmux_shaped`](Self::args_look_tmux_shaped),
    /// which recognizes tmux's own argument grammar: a shell wrapper's payload is
    /// one opaque string (`"tmux kill-server"`) that matches no sub-command and no
    /// `-L`/`-S` flag, yet is exactly the hazard. Substring matching is the point —
    /// anything naming tmux in an argument must prove itself scoped.
    fn args_mention_tmux(&self) -> bool {
        self.args
            .iter()
            .flatten()
            .any(|argument| argument.contains("tmux"))
    }

    /// `tmux -V` and `tmux -h` interrogate the binary itself and contact no
    /// server, so they need no socket. This is the ONLY exemption, and it is
    /// shaped as a deny-list of provably-safe forms: EVERY argument must be a
    /// literal drawn from the safe set, so nothing can exempt itself by being
    /// unrecognized.
    fn is_provably_safe_query(&self) -> bool {
        !self.args.is_empty()
            && self.args.iter().all(|argument| {
                argument
                    .as_deref()
                    .is_some_and(|value| matches!(value, "-V" | "--version" | "-h" | "--help"))
            })
    }

    /// Evidence that an otherwise-unreadable program is tmux: a known tmux
    /// sub-command, a socket flag, or a `TMUX_TMPDIR` override. Keyed on
    /// EXPLICIT tmux tokens only — a merely non-literal argument is not
    /// evidence, so an ordinary `Command::new(program).args(&args)` is left
    /// alone.
    fn args_look_tmux_shaped(&self) -> bool {
        !matches!(self.tmux_tmpdir, TmuxTmpdir::Unset)
            || self.args.iter().flatten().any(|argument| {
                is_known_tmux_subcommand(argument) || argument == "-L" || argument == "-S"
            })
    }

    fn tmux_tmpdir_violation(&self) -> Option<String> {
        match &self.tmux_tmpdir {
            TmuxTmpdir::Unset => Some(
                "`TMUX_TMPDIR` is not set to a per-run scratch directory, so the socket \
                 file lands in the shared default tmux namespace"
                    .to_owned(),
            ),
            // The check reads source only, so for a runtime value it can say no
            // more than "it is set".
            TmuxTmpdir::Runtime => None,
            TmuxTmpdir::Literal(value) => (!is_private_tmux_tmpdir(value)).then(|| {
                format!("`TMUX_TMPDIR={value}` resolves into the shared default tmux namespace")
            }),
        }
    }

    fn socket_label_violation(&self) -> Option<String> {
        let section_end = self.server_option_section_end();
        let Some(flag_index) = self.args[..section_end]
            .iter()
            .position(|argument| argument.as_deref() == Some("-L"))
        else {
            return Some(
                "no `-L <private-socket>` appears among the server options preceding the \
                 tmux sub-command"
                    .to_owned(),
            );
        };
        match self.args.get(flag_index + 1) {
            None => Some("`-L` is not followed by a socket name".to_owned()),
            // A non-literal label is the per-run generated name this rule wants.
            Some(None) => None,
            Some(Some(label)) if is_private_socket_label(label) => None,
            Some(Some(label)) => Some(format!(
                "`-L {label}` selects the shared default tmux socket"
            )),
        }
    }

    /// A tmux command line is `tmux [server-options] <sub-command> [...]`, and
    /// `-L` only scopes the socket while it sits in that LEADING option
    /// section. Walk the section and return the index just past it. Anything
    /// not shaped like a flag ends it, so the answer never depends on
    /// recognizing which sub-command follows.
    fn server_option_section_end(&self) -> usize {
        let mut index = 0;
        while index < self.args.len() {
            // A non-literal argument could be anything, the sub-command
            // included, so the option section is treated as over.
            let Some(argument) = self.args[index].as_deref() else {
                return index;
            };
            if !argument.starts_with('-') {
                return index;
            }
            index += 1;
            if TMUX_VALUE_TAKING_SERVER_FLAGS.contains(&argument) {
                index += 1;
            }
        }
        index
    }
}

/// tmux server options that consume the argument after them, so the walk over
/// the leading option section does not mistake a flag's VALUE for the
/// sub-command.
const TMUX_VALUE_TAKING_SERVER_FLAGS: &[&str] = &["-L", "-S", "-f", "-c", "-T"];

fn names_tmux_tmpdir(argument: Option<&syn::Expr>) -> bool {
    argument
        .and_then(string_literal)
        .is_some_and(|name| name == "TMUX_TMPDIR")
}

/// tmux's own default socket is literally named `default`, so `-L default`
/// lands on exactly the shared server this rule exists to protect. An empty
/// label is rejected for the same reason: tmux falls back to the default.
fn is_private_socket_label(label: &str) -> bool {
    let label = label.trim();
    !label.is_empty() && label != "default"
}

/// Whether a LITERAL `TMUX_TMPDIR` value points somewhere private.
///
/// tmux puts its sockets in `$TMUX_TMPDIR/tmux-<uid>/`, defaulting
/// `TMUX_TMPDIR` to `/tmp`. So `TMUX_TMPDIR=/tmp` reproduces the shared default
/// namespace exactly, and a value already pointing INTO a `tmux-<uid>`
/// directory joins one. The test is purely lexical — the check never touches
/// the filesystem — so a value it cannot place (relative, or empty) is rejected
/// as unverifiable rather than assumed safe.
fn is_private_tmux_tmpdir(value: &str) -> bool {
    let Some(segments) = normalized_absolute_segments(value) else {
        return false;
    };
    if segments.is_empty() || segments.iter().any(|segment| segment.starts_with("tmux-")) {
        return false;
    }
    !matches!(
        segments.as_slice(),
        ["tmp"] | ["var", "tmp"] | ["dev", "shm"]
    )
}

/// Lexically normalize an ABSOLUTE path into its segments, resolving `.` and
/// `..` without consulting the filesystem. `None` for a relative path, which
/// this check cannot place.
fn normalized_absolute_segments(value: &str) -> Option<Vec<&str>> {
    if !value.starts_with('/') {
        return None;
    }
    let mut segments: Vec<&str> = Vec::new();
    for segment in value.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    Some(segments)
}

fn is_command_new_call(call: &syn::ExprCall) -> bool {
    let syn::Expr::Path(path) = call.func.as_ref() else {
        return false;
    };
    let mut segments = path
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string());
    let Some(last) = segments.next_back() else {
        return false;
    };
    let Some(previous) = segments.next_back() else {
        return false;
    };
    last == "new" && previous == "Command"
}

/// Classify the expression handed to `Command::new(...)`.
///
/// A rename or an indirection must never silently disable the rule, so anything
/// whose NAME says tmux counts as tmux, and anything unreadable is `Unknown`
/// (suspect once its arguments look tmux-shaped) rather than dismissed.
fn classify_program(expr: &syn::Expr) -> ProgramKind {
    match strip_wrappers(expr) {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(literal),
            ..
        }) => {
            let value = literal.value();
            let basename = value.rsplit('/').next().unwrap_or(value.as_str());
            if name_mentions_tmux(basename) {
                ProgramKind::Tmux
            } else {
                ProgramKind::Other
            }
        }
        syn::Expr::Path(path) => path
            .path
            .segments
            .last()
            .map_or(ProgramKind::Unknown, |segment| {
                tmux_or_unknown(&segment.ident.to_string())
            }),
        syn::Expr::Field(field) => match &field.member {
            syn::Member::Named(ident) => tmux_or_unknown(&ident.to_string()),
            syn::Member::Unnamed(_) => ProgramKind::Unknown,
        },
        syn::Expr::MethodCall(method_call) => {
            if name_mentions_tmux(&method_call.method.to_string()) {
                ProgramKind::Tmux
            } else {
                classify_program(&method_call.receiver)
            }
        }
        syn::Expr::Call(call) => classify_program(&call.func),
        syn::Expr::Index(index) => classify_program(&index.expr),
        _ => ProgramKind::Unknown,
    }
}

fn tmux_or_unknown(name: &str) -> ProgramKind {
    if name_mentions_tmux(name) {
        ProgramKind::Tmux
    } else {
        ProgramKind::Unknown
    }
}

/// Case-insensitive `tmux` substring test over a name, so `tmux`, `tmux_bin`,
/// `resolve_tmux`, and `TMUX_PATH` all read as tmux.
fn name_mentions_tmux(name: &str) -> bool {
    name.to_ascii_lowercase().contains("tmux")
}

/// Whether an expression NAMES tmux anywhere — in any identifier or string
/// literal it contains. Decides that an unfollowable command builder is suspect
/// rather than ignorable.
fn expr_names_tmux(expr: &syn::Expr) -> bool {
    let mut visitor = TmuxMentionVisitor { found: false };
    visitor.visit_expr(expr);
    visitor.found
}

struct TmuxMentionVisitor {
    found: bool,
}

impl<'ast> Visit<'ast> for TmuxMentionVisitor {
    fn visit_ident(&mut self, node: &'ast syn::Ident) {
        if name_mentions_tmux(&node.to_string()) {
            self.found = true;
        }
    }

    fn visit_lit_str(&mut self, node: &'ast syn::LitStr) {
        if name_mentions_tmux(&node.value()) {
            self.found = true;
        }
    }
}

/// Peel the wrappers that do not change which command an expression denotes, so
/// `(&mut cmd)`, `cmd.status()?`, and their combinations resolve like the bare
/// form.
fn strip_wrappers(expr: &syn::Expr) -> &syn::Expr {
    match expr {
        syn::Expr::Paren(paren) => strip_wrappers(&paren.expr),
        syn::Expr::Group(group) => strip_wrappers(&group.expr),
        syn::Expr::Reference(reference) => strip_wrappers(&reference.expr),
        syn::Expr::Try(try_expr) => strip_wrappers(&try_expr.expr),
        other => other,
    }
}

/// The name of a single-segment path expression — that is, a plain local
/// binding such as `cmd`.
fn bare_ident(expr: &syn::Expr) -> Option<String> {
    let syn::Expr::Path(path) = strip_wrappers(expr) else {
        return None;
    };
    if path.qself.is_some() || path.path.segments.len() != 1 {
        return None;
    }
    path.path
        .segments
        .first()
        .map(|segment| segment.ident.to_string())
}

fn local_binding_ident(local: &syn::Local) -> Option<String> {
    match &local.pat {
        syn::Pat::Ident(pattern) => Some(pattern.ident.to_string()),
        syn::Pat::Type(pattern) => match pattern.pat.as_ref() {
            syn::Pat::Ident(inner) => Some(inner.ident.to_string()),
            _ => None,
        },
        _ => None,
    }
}

fn string_literals(expr: &syn::Expr) -> Vec<Option<String>> {
    match strip_wrappers(expr) {
        syn::Expr::Array(array) => array.elems.iter().map(string_literal).collect(),
        syn::Expr::Tuple(tuple) => tuple.elems.iter().map(string_literal).collect(),
        other => vec![string_literal(other)],
    }
}

fn string_literal(expr: &syn::Expr) -> Option<String> {
    match strip_wrappers(expr) {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(literal),
            ..
        }) => Some(literal.value()),
        _ => None,
    }
}

fn is_known_tmux_subcommand(argument: &str) -> bool {
    KNOWN_TMUX_SUBCOMMANDS.contains(&argument)
}

/// A sample of tmux sub-commands, used ONLY as positive evidence that an
/// otherwise-unreadable program is tmux. tmux ships roughly 170 of these, so
/// this list is necessarily incomplete — which is now harmless, because nothing
/// keys "the rule does not apply" off it. Adding an entry can only widen
/// coverage; omitting one can no longer create a bypass.
const KNOWN_TMUX_SUBCOMMANDS: &[&str] = &[
    "attach-session",
    "capture-pane",
    "display-message",
    "has-session",
    "kill-pane",
    "kill-server",
    "kill-session",
    "kill-window",
    "list-panes",
    "list-sessions",
    "list-windows",
    "new-session",
    "new-window",
    "run-shell",
    "select-pane",
    "send-keys",
    "set-option",
    "show-options",
    "source-file",
    "split-window",
];

// ---------------------------------------------------------------------------
// First-party Python import supply rule.
// ---------------------------------------------------------------------------

/// Third-party Python import roots this repo's own first-party Python may use,
/// mapped to the distribution that must be declared by this repo or vendored
/// under an in-repo `_vendor` tree.
///
/// This is deliberately a CLOSED allow-list. The static promise here is limited
/// and honest: parsing import statements can prove that a source file asks for
/// `returns`, and parsing `pyproject.toml` can prove this repo declares the
/// `returns` distribution, but it cannot prove that the runtime interpreter will
/// resolve that exact distribution instead of an ambient host package.
const ALLOWED_PYTHON_IMPORT_DISTRIBUTIONS: &[(&str, &str)] = &[
    ("livespec_runtime", "livespec-runtime"),
    ("returns", "returns"),
];

/// Python import roots treated as stdlib by the source-level supply check. Any
/// non-relative import root outside this set and outside the closed third-party
/// allow-list is suspect by default.
const PYTHON_STDLIB_IMPORT_ROOTS: &[&str] = &[
    "__future__",
    "collections",
    "dataclasses",
    "datetime",
    "enum",
    "functools",
    "itertools",
    "json",
    "os",
    "pathlib",
    "re",
    "shutil",
    "subprocess",
    "sys",
    "tempfile",
    "textwrap",
    "typing",
    "unittest",
];

fn check_first_party_python_import_supply(root: &Path) -> Vec<String> {
    let paths = first_party_python_files(root);
    check_first_party_python_import_supply_paths(root, &paths)
}

fn check_first_party_python_import_supply_paths(root: &Path, paths: &[PathBuf]) -> Vec<String> {
    let mut findings = Vec::new();
    if paths.is_empty() {
        findings.push(format!(
            "first-party Python import-supply scan found no Python files under {} — \
             the scan root moved or the walk is broken; refusing to pass without \
             having read anything",
            root.display()
        ));
        return findings;
    }
    let declared = match declared_python_distributions(root) {
        Ok(declared) => declared,
        Err(error) => {
            findings.push(error);
            BTreeSet::new()
        }
    };
    let vendored = vendored_python_import_roots(root);
    for path in paths {
        let display = path.display().to_string();
        let source = match fs::read_to_string(path) {
            Ok(source) => source,
            Err(error) => {
                findings.push(format!("could not read {}: {error}", path.display()));
                continue;
            }
        };
        for import in python_import_roots(&source) {
            if PYTHON_STDLIB_IMPORT_ROOTS.contains(&import.as_str()) || vendored.contains(&import) {
                continue;
            }
            let Some(distribution) = allowed_python_distribution_for_import(&import) else {
                findings.push(format!(
                    "{display}: Python import `{import}` is not in the closed first-party \
                     Python third-party import allow-list; declare the owning distribution \
                     deliberately or vendor it under `_vendor`"
                ));
                continue;
            };
            if !declared.contains(distribution) {
                findings.push(format!(
                    "{display}: Python import `{import}` maps to distribution \
                     `{distribution}`, but that distribution is not declared by this repo \
                     and was not found under `_vendor`"
                ));
            }
        }
    }
    findings
}

fn allowed_python_distribution_for_import(import: &str) -> Option<&'static str> {
    ALLOWED_PYTHON_IMPORT_DISTRIBUTIONS
        .iter()
        .find_map(|(root, distribution)| (*root == import).then_some(*distribution))
}

fn first_party_python_files(root: &Path) -> Vec<PathBuf> {
    git_ls_files_python(root).unwrap_or_else(|_| walk_first_party_python_files(root))
}

fn git_ls_files_python(root: &Path) -> Result<Vec<PathBuf>, String> {
    let output = Command::new("git")
        .args(["-C"])
        .arg(root)
        .args(["ls-files", "*.py"])
        .output()
        .map_err(|error| format!("could not execute git ls-files: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "`git ls-files '*.py'` exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| format!("git ls-files output was not UTF-8: {error}"))?;
    Ok(stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| root.join(line))
        .collect())
}

fn walk_first_party_python_files(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(path) = pending.pop() {
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            if path != root
                && path.file_name().is_some_and(|name| {
                    ["target", ".git", "tmp", ".venv"]
                        .iter()
                        .any(|entry| name == *entry)
                })
            {
                continue;
            }
            let Ok(entries) = fs::read_dir(&path) else {
                continue;
            };
            for entry in entries.flatten() {
                pending.push(entry.path());
            }
            continue;
        }
        if path.extension().is_some_and(|extension| extension == "py") {
            files.push(path);
        }
    }
    files
}

fn declared_python_distributions(root: &Path) -> Result<BTreeSet<String>, String> {
    let path = root.join("pyproject.toml");
    let source = fs::read_to_string(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let value: toml::Value = toml::from_str(&source)
        .map_err(|error| format!("could not parse {}: {error}", path.display()))?;
    let mut declared = BTreeSet::new();
    collect_dependency_array(
        value
            .get("project")
            .and_then(|project| project.get("dependencies")),
        &mut declared,
    );
    if let Some(groups) = value
        .get("dependency-groups")
        .and_then(toml::Value::as_table)
    {
        for group in groups.values() {
            collect_dependency_array(Some(group), &mut declared);
        }
    }
    if let Some(optional) = value
        .get("project")
        .and_then(|project| project.get("optional-dependencies"))
        .and_then(toml::Value::as_table)
    {
        for group in optional.values() {
            collect_dependency_array(Some(group), &mut declared);
        }
    }
    Ok(declared)
}

fn collect_dependency_array(value: Option<&toml::Value>, declared: &mut BTreeSet<String>) {
    let Some(array) = value.and_then(toml::Value::as_array) else {
        return;
    };
    for entry in array {
        if let Some(requirement) = entry.as_str()
            && let Some(distribution) = requirement_distribution(requirement)
        {
            declared.insert(distribution);
        }
    }
}

fn requirement_distribution(requirement: &str) -> Option<String> {
    let name: String = requirement
        .trim()
        .chars()
        .take_while(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
        .collect();
    (!name.is_empty()).then(|| normalize_distribution_name(&name))
}

fn normalize_distribution_name(name: &str) -> String {
    name.to_ascii_lowercase().replace(['_', '.'], "-")
}

fn vendored_python_import_roots(root: &Path) -> BTreeSet<String> {
    let mut pending = vec![root.to_path_buf()];
    let mut vendored = BTreeSet::new();
    while let Some(path) = pending.pop() {
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if !metadata.is_dir() {
            continue;
        }
        if path.file_name().is_some_and(|name| name == "_vendor") {
            if let Ok(entries) = fs::read_dir(&path) {
                for entry in entries.flatten() {
                    let child = entry.path();
                    if child.is_dir()
                        && let Some(name) = child.file_name().and_then(|name| name.to_str())
                    {
                        vendored.insert(name.to_owned());
                    }
                }
            }
            continue;
        }
        if path != root
            && path.file_name().is_some_and(|name| {
                ["target", ".git", "tmp", ".venv"]
                    .iter()
                    .any(|entry| name == *entry)
            })
        {
            continue;
        }
        if let Ok(entries) = fs::read_dir(&path) {
            for entry in entries.flatten() {
                pending.push(entry.path());
            }
        }
    }
    vendored
}

fn python_import_roots(source: &str) -> BTreeSet<String> {
    let mut imports = BTreeSet::new();
    for line in source.lines() {
        let line = line.trim_start();
        if line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("import ") {
            for part in rest.split(',') {
                if let Some(root) = python_import_root(part) {
                    imports.insert(root);
                }
            }
        } else if let Some(rest) = line.strip_prefix("from ") {
            if rest.starts_with('.') {
                continue;
            }
            if let Some((module, _)) = rest.split_once(" import ")
                && let Some(root) = python_import_root(module)
            {
                imports.insert(root);
            }
        }
    }
    imports
}

fn python_import_root(import: &str) -> Option<String> {
    let root = import
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .split('.')
        .next()
        .unwrap_or_default()
        .trim();
    (!root.is_empty()
        && root
            .chars()
            .all(|character| character == '_' || character.is_ascii_alphanumeric()))
    .then(|| root.to_owned())
}

// ---------------------------------------------------------------------------
// Zero-Beads-knowledge rule.
// ---------------------------------------------------------------------------

/// The source file that owns the console's compiled-in backing CLI defaults.
const BACKING_CLI_SOURCE: &str = "crates/console-cli/src/backing_cli.rs";

/// Programs and default command tokens the console is permitted to resolve from
/// compiled-in backing CLI defaults.
///
/// This is deliberately a CLOSED allow-list: the console's work-item boundary is
/// the orchestrator CLI, not Beads itself. The static promise here is limited
/// and honest: `BackingCliResolution::from_environment` applies runtime program
/// overrides, so an environment variable can still swap a program after compile
/// time. This guard proves the compiled-in defaults contain no Beads-native
/// program, and the resolvable default set cannot be widened without editing
/// the watched backing CLI file and making a deliberate allow-list decision.
const ALLOWED_BACKING_CLI_DEFAULT_TOKENS: &[&str] = &[
    "--json",
    "dispatcher.py",
    "drive.py",
    "fabro",
    "gh",
    "list-work-items",
    "list_work_items.py",
    "livespec",
    "livespec-dispatcher-drain",
    "livespec-orchestrator-drive",
    "needs-attention",
    "needs_attention.py",
    "next",
];

fn check_zero_beads_knowledge(root: &Path) -> Vec<String> {
    let mut findings = check_backing_cli_default_tokens(root);
    findings.extend(check_zero_beads_source_paths(root));
    findings
}

fn check_zero_beads_source_paths(root: &Path) -> Vec<String> {
    let mut findings = Vec::new();
    let mut paths = Vec::new();
    for crate_name in SCANNED_CRATES {
        paths.extend(rust_files(&root.join("crates").join(crate_name)));
    }
    if paths.is_empty() {
        findings.push(format!(
            "zero-Beads-knowledge scan found no Rust files under {} — the scan root moved \
             or the walk is broken; refusing to pass without having read anything",
            root.display()
        ));
        return findings;
    }
    for path in paths {
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => {
                findings.push(format!("could not read {}: {error}", path.display()));
                continue;
            }
        };
        let file = match syn::parse_file(&source) {
            Ok(file) => file,
            Err(error) => {
                findings.push(format!("could not parse {}: {error}", path.display()));
                continue;
            }
        };
        findings.extend(check_beads_native_source_paths(
            &file,
            &path.display().to_string(),
        ));
    }
    findings
}

fn check_backing_cli_default_tokens(root: &Path) -> Vec<String> {
    let path = root.join(BACKING_CLI_SOURCE);
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => return vec![format!("could not read {}: {error}", path.display())],
    };
    let file = match syn::parse_file(&source) {
        Ok(file) => file,
        Err(error) => return vec![format!("could not parse {}: {error}", path.display())],
    };
    let mut visitor = BackingCliDefaultVisitor {
        tokens: BTreeSet::new(),
        struct_literal_count: 0,
        backing_cli_impl_depth: 0,
    };
    visitor.visit_file(&file);
    if visitor.struct_literal_count == 0 || visitor.tokens.is_empty() {
        return vec![format!(
            "{}: no `BackingCliPrograms` default struct literals were parsed — refusing \
             to pass a vacuous zero-Beads-knowledge backing CLI check",
            path.display()
        )];
    }
    visitor
        .tokens
        .into_iter()
        .filter(|token| !ALLOWED_BACKING_CLI_DEFAULT_TOKENS.contains(&token.as_str()))
        .map(|token| {
            format!(
                "{}: backing CLI default token `{token}` is not in the explicit \
                 zero-Beads-knowledge allow-list",
                path.display()
            )
        })
        .collect()
}

struct BackingCliDefaultVisitor {
    tokens: BTreeSet<String>,
    struct_literal_count: usize,
    backing_cli_impl_depth: usize,
}

impl BackingCliDefaultVisitor {
    fn collect_expr_tokens(&mut self, expr: &syn::Expr) {
        let mut visitor = StringLiteralCollector {
            tokens: &mut self.tokens,
        };
        visitor.visit_expr(expr);
    }
}

impl<'ast> Visit<'ast> for BackingCliDefaultVisitor {
    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let is_backing_cli_impl = matches!(
            node.self_ty.as_ref(),
            syn::Type::Path(type_path) if path_ends_with(&type_path.path, "BackingCliPrograms")
        );
        if is_backing_cli_impl {
            self.backing_cli_impl_depth += 1;
        }
        syn::visit::visit_item_impl(self, node);
        if is_backing_cli_impl {
            self.backing_cli_impl_depth -= 1;
        }
    }

    fn visit_expr_struct(&mut self, node: &'ast syn::ExprStruct) {
        if path_ends_with(&node.path, "BackingCliPrograms")
            || (self.backing_cli_impl_depth > 0 && path_ends_with(&node.path, "Self"))
        {
            self.struct_literal_count += 1;
            for field in &node.fields {
                self.collect_expr_tokens(&field.expr);
            }
        }
        syn::visit::visit_expr_struct(self, node);
    }
}

struct StringLiteralCollector<'a> {
    tokens: &'a mut BTreeSet<String>,
}

impl<'ast> Visit<'ast> for StringLiteralCollector<'_> {
    fn visit_lit_str(&mut self, node: &'ast syn::LitStr) {
        let value = node.value();
        if let Some(token) = backing_cli_token(&value) {
            self.tokens.insert(token);
        }
    }
}

fn backing_cli_token(value: &str) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    if value.starts_with('-') {
        return Some(value.to_owned());
    }
    Path::new(value)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

fn check_beads_native_source_paths(file: &syn::File, display: &str) -> Vec<String> {
    let mut visitor = BeadsNativePathVisitor {
        findings: Vec::new(),
        display,
    };
    visitor.visit_file(file);
    visitor.findings
}

struct BeadsNativePathVisitor<'a> {
    findings: Vec<String>,
    display: &'a str,
}

impl<'ast> Visit<'ast> for BeadsNativePathVisitor<'_> {
    fn visit_attribute(&mut self, _node: &'ast syn::Attribute) {}

    fn visit_expr_lit(&mut self, node: &'ast syn::ExprLit) {
        if let syn::Lit::Str(literal) = &node.lit {
            let value = literal.value();
            if is_beads_native_source_path(&value) {
                self.findings.push(format!(
                    "{}: string literal `{value}` embeds a Beads-native store path; \
                     the console must reach work-items through orchestrator CLI ports, \
                     not `.beads`, Dolt, or Beads SQLite storage",
                    self.display
                ));
            }
        }
        syn::visit::visit_expr_lit(self, node);
    }
}

fn is_beads_native_source_path(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains(".beads")
        || (lower.contains("beads") && (lower.contains("dolt") || lower.contains("sqlite")))
}

fn path_ends_with(path: &syn::Path, name: &str) -> bool {
    path.segments
        .last()
        .is_some_and(|segment| segment.ident == name)
}

/// A crate entrypoint is its `src/lib.rs` or `src/main.rs`.
fn is_entrypoint(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name == "lib.rs" || name == "main.rs")
        && path.parent().is_some_and(|parent| parent.ends_with("src"))
}

/// Rule: each scanned crate's entrypoint declares
/// `#![forbid(unsafe_code)]`.
fn check_forbid_unsafe(file: &syn::File, display: &str) -> Vec<String> {
    if file.attrs.iter().any(is_forbid_unsafe_attr) {
        Vec::new()
    } else {
        vec![format!("{display}: must declare `#![forbid(unsafe_code)]`")]
    }
}

fn is_forbid_unsafe_attr(attr: &syn::Attribute) -> bool {
    if !attr.path().is_ident("forbid") {
        return false;
    }
    match &attr.meta {
        syn::Meta::List(list) => list.tokens.to_string().contains("unsafe_code"),
        syn::Meta::Path(_) | syn::Meta::NameValue(_) => false,
    }
}

/// Rule: no real `.unwrap()` / `.expect()` method call outside test code.
/// AST-based, so `unwrap_or`, comments, and string literals never match.
fn check_unwrap_expect(file: &syn::File, display: &str) -> Vec<String> {
    let mut visitor = UnwrapExpectVisitor {
        findings: Vec::new(),
        display,
    };
    visitor.visit_file(file);
    visitor.findings
}

struct UnwrapExpectVisitor<'a> {
    findings: Vec<String>,
    display: &'a str,
}

impl<'ast> Visit<'ast> for UnwrapExpectVisitor<'_> {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if has_cfg_test(&node.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if is_test_fn(&node.attrs) {
            return;
        }
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        let method = node.method.to_string();
        if method == "unwrap" || method == "expect" {
            self.findings.push(format!(
                "{}: forbidden `.{method}()` call — use typed error handling",
                self.display
            ));
        }
        syn::visit::visit_expr_method_call(self, node);
    }
}

/// Rule: no key handler stages an operator action around the action registry.
///
/// In `console-tui` production code the ONLY function that may construct the
/// action-staging interactions (`TuiInteraction::OpenValveConfirm` and
/// `TuiInteraction::OpenDriverHandoff`) is `registry_action_input`, the one
/// path that consults the registry's availability derivation. A hand-written
/// key arm that stages either interaction directly would reintroduce the
/// second-encoding defect the registry exists to retire, so it is flagged
/// here rather than waiting for a hint/behavior divergence to be noticed.
fn check_registry_bypass(crate_name: &str, file: &syn::File, display: &str) -> Vec<String> {
    if crate_name != "console-tui" {
        return Vec::new();
    }
    let mut visitor = RegistryBypassVisitor {
        findings: Vec::new(),
        display,
        allowed_depth: 0,
    };
    visitor.visit_file(file);
    visitor.findings
}

/// The `console-tui` functions allowed to construct the staging
/// interactions: the registry-consulting key path, the invoker's confirm, the
/// command explainer continuation, and the generated menu's confirm — all stage
/// exclusively through
/// `action_registry`'s `staged_without_selection` / `stage_action`.
///
/// The menu joined this list rather than being exempted from the rule: a menu
/// that staged its own way would be a THIRD encoding of invocation, which is
/// precisely what this check exists to prevent.
const REGISTRY_STAGING_FNS: [&str; 4] = [
    "registry_action_input",
    "invoker_confirm_step",
    "staged_action_step",
    "menu_confirm_step",
];

/// The staging interactions only the registry path may construct.
const STAGING_INTERACTIONS: [&str; 2] = ["OpenValveConfirm", "OpenDriverHandoff"];

struct RegistryBypassVisitor<'a> {
    findings: Vec<String>,
    display: &'a str,
    /// Non-zero while visiting the body of the allowed staging function.
    allowed_depth: usize,
}

impl<'ast> Visit<'ast> for RegistryBypassVisitor<'_> {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if has_cfg_test(&node.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if is_test_fn(&node.attrs) {
            return;
        }
        let allowed = REGISTRY_STAGING_FNS
            .iter()
            .any(|name| node.sig.ident == name);
        if allowed {
            self.allowed_depth += 1;
        }
        syn::visit::visit_item_fn(self, node);
        if allowed {
            self.allowed_depth -= 1;
        }
    }

    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        if self.allowed_depth == 0 {
            let segments: Vec<String> = node
                .path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect();
            if let [.., parent, variant] = segments.as_slice()
                && parent == "TuiInteraction"
                && STAGING_INTERACTIONS
                    .iter()
                    .any(|staging| variant == staging)
            {
                self.findings.push(format!(
                    "{}: `TuiInteraction::{variant}` may only be constructed by a \
                     registry-consulting staging path ({REGISTRY_STAGING_FNS:?}); a direct \
                     staging bypasses the registry availability derivation",
                    self.display
                ));
            }
        }
        syn::visit::visit_expr_path(self, node);
    }
}

/// Rule: the `EventType` / `CommandType` enums are defined only in
/// `console-domain` (the bounded-context contract home), never in
/// adapter or other outer crates.
fn check_type_placement(crate_name: &str, file: &syn::File, display: &str) -> Vec<String> {
    if crate_name == "console-domain" {
        return Vec::new();
    }
    let mut visitor = TypePlacementVisitor {
        findings: Vec::new(),
        display,
    };
    visitor.visit_file(file);
    visitor.findings
}

struct TypePlacementVisitor<'a> {
    findings: Vec<String>,
    display: &'a str,
}

impl<'ast> Visit<'ast> for TypePlacementVisitor<'_> {
    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        let name = node.ident.to_string();
        if name == "EventType" || name == "CommandType" {
            self.findings.push(format!(
                "{}: enum `{name}` must be defined in console-domain, not here",
                self.display
            ));
        }
        syn::visit::visit_item_enum(self, node);
    }
}

/// Rule: when source adapters are realized as sibling modules, no
/// adapter module may reference another adapter module's items (the
/// per-source isolation invariant). Enforced at whatever module
/// granularity is in use: with the current single flat `source_adapters`
/// module there are no siblings, so the rule holds by construction; it
/// activates the moment adapters are split into sibling modules.
fn check_adapter_isolation(file: &syn::File, display: &str) -> Vec<String> {
    if !display.ends_with("source_adapters.rs") {
        return Vec::new();
    }
    let siblings: BTreeSet<String> = file
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Mod(item_mod) if !has_cfg_test(&item_mod.attrs) => {
                Some(item_mod.ident.to_string())
            }
            _ => None,
        })
        .collect();
    if siblings.len() < 2 {
        return Vec::new();
    }
    let mut findings = Vec::new();
    for item in &file.items {
        if let syn::Item::Mod(item_mod) = item {
            let current = item_mod.ident.to_string();
            if !siblings.contains(&current) {
                continue;
            }
            let mut visitor = SiblingRefVisitor {
                siblings: &siblings,
                current: &current,
                display,
                findings: Vec::new(),
            };
            visitor.visit_item_mod(item_mod);
            findings.extend(visitor.findings);
        }
    }
    findings
}

struct SiblingRefVisitor<'a> {
    siblings: &'a BTreeSet<String>,
    current: &'a str,
    display: &'a str,
    findings: Vec<String>,
}

impl<'ast> Visit<'ast> for SiblingRefVisitor<'_> {
    fn visit_path(&mut self, node: &'ast syn::Path) {
        for segment in &node.segments {
            let name = segment.ident.to_string();
            if name != self.current && self.siblings.contains(&name) {
                self.findings.push(format!(
                    "{}: adapter module `{}` must not reference sibling adapter module `{name}`",
                    self.display, self.current
                ));
            }
        }
        syn::visit::visit_path(self, node);
    }
}

// ---------------------------------------------------------------------------
// Shared AST + filesystem helpers.
// ---------------------------------------------------------------------------

fn has_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && match &attr.meta {
                syn::Meta::List(list) => list.tokens.to_string().contains("test"),
                syn::Meta::Path(_) | syn::Meta::NameValue(_) => false,
            }
    })
}

fn is_test_fn(attrs: &[syn::Attribute]) -> bool {
    has_cfg_test(attrs) || attrs.iter().any(|attr| attr.path().is_ident("test"))
}

fn rust_files(crate_dir: &Path) -> Vec<PathBuf> {
    let mut pending = vec![crate_dir.join("src")];
    rust_files_from(&mut pending)
}

fn rust_files_from(pending: &mut Vec<PathBuf>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    while let Some(path) = pending.pop() {
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            let Ok(entries) = fs::read_dir(&path) else {
                continue;
            };
            for entry in entries.flatten() {
                pending.push(entry.path());
            }
            continue;
        }
        if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::Path;

    use super::{
        CrateNode, ObservedRustToolchain, check_adapter_isolation,
        check_backing_cli_default_tokens, check_beads_native_source_paths,
        check_crate_graph_non_vacuity, check_fabro_image_rust_toolchain_with_probe,
        check_forbid_unsafe, check_layering, check_registry_bypass,
        check_source_rule_crate_coverage_for_names, check_tmux_socket_scoping,
        check_tmux_socket_scoping_source, check_type_placement, check_unwrap_expect,
        check_workspace_rust_version_matches_toolchain, check_zero_beads_source_paths,
        fabro_python_rust_image, observed_rust_toolchain,
        observed_rust_toolchain_from_image_config, rust_files_for_tmux_scan,
    };

    fn node(name: &str, workspace_deps: &[&str], external_deps: &[&str]) -> CrateNode {
        CrateNode {
            name: name.to_owned(),
            workspace_deps: workspace_deps.iter().map(|dep| (*dep).to_owned()).collect(),
            external_deps: external_deps.iter().map(|dep| (*dep).to_owned()).collect(),
        }
    }

    #[test]
    fn layering_flags_a_forbidden_reverse_edge() {
        let nodes = [node("console-domain", &["console-tui"], &[])];
        let findings = check_layering(&nodes);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("console-domain"));
        assert!(findings[0].contains("console-tui"));
    }

    #[test]
    fn layering_allows_the_canonical_direction() {
        let nodes = [
            node("console-domain", &[], &[]),
            node("console-application", &["console-domain"], &[]),
            node("console-eventstore", &["console-domain"], &["rusqlite"]),
            node(
                "console-tui",
                &["console-application", "console-domain"],
                &["ratatui", "crossterm"],
            ),
        ];
        assert!(check_layering(&nodes).is_empty());
    }

    #[test]
    fn layering_flags_infra_dependency_in_domain() {
        let nodes = [node("console-domain", &[], &["tokio"])];
        let findings = check_layering(&nodes);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("tokio"));
    }

    #[test]
    fn layering_flags_infra_dependency_in_ui() {
        let nodes = [node("console-tui", &["console-application"], &["reqwest"])];
        let findings = check_layering(&nodes);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("reqwest"));
    }

    #[test]
    fn layering_does_not_constrain_eventstore_infra() {
        // The event store is infrastructure; rusqlite is expected there.
        let nodes = [node(
            "console-eventstore",
            &["console-domain"],
            &["rusqlite"],
        )];
        assert!(check_layering(&nodes).is_empty());
    }

    #[test]
    fn crate_graph_that_reads_no_workspace_packages_is_flagged() {
        let findings = check_crate_graph_non_vacuity(&[]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings[0].contains("zero workspace packages"),
            "{findings:?}"
        );
    }

    #[test]
    fn source_rule_coverage_allows_the_current_declared_workspace_set() {
        let workspace_crates = workspace_crates(&[
            "console-application",
            "console-arch-check",
            "console-ci-parity-check",
            "console-cli",
            "console-completeness-check",
            "console-domain",
            "console-eventstore",
            "console-fork-drift-check",
            "console-nightly-soak",
            "console-red-green-replay-check",
            "console-spec-check",
            "console-tui",
            "console-upstream-dep-check",
        ]);

        let findings = check_source_rule_crate_coverage_for_names(&workspace_crates);

        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn source_rule_coverage_flags_an_undeclared_workspace_member() {
        let workspace_crates = workspace_crates(&[
            "console-application",
            "console-arch-check",
            "console-ci-parity-check",
            "console-cli",
            "console-completeness-check",
            "console-domain",
            "console-eventstore",
            "console-fork-drift-check",
            "console-nightly-soak",
            "console-red-green-replay-check",
            "console-spec-check",
            "console-tui",
            "console-upstream-dep-check",
            "console-new-product",
        ]);

        let findings = check_source_rule_crate_coverage_for_names(&workspace_crates);

        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("console-new-product"), "{findings:?}");
        assert!(findings[0].contains("SCANNED_CRATES"), "{findings:?}");
        assert!(
            findings[0].contains("SOURCE_RULE_EXCLUDED_CRATES"),
            "{findings:?}"
        );
    }

    #[test]
    fn source_rule_coverage_that_reads_no_workspace_packages_is_flagged() {
        let findings = check_source_rule_crate_coverage_for_names(&BTreeSet::new());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings[0].contains("zero workspace packages"),
            "{findings:?}"
        );
    }

    #[test]
    fn unwrap_call_is_flagged() -> Result<(), syn::Error> {
        let file = syn::parse_file("fn handler() { let value = source().unwrap(); }")?;
        let findings = check_unwrap_expect(&file, "x.rs");
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("unwrap"));
        Ok(())
    }

    #[test]
    fn expect_call_is_flagged() -> Result<(), syn::Error> {
        let file = syn::parse_file("fn handler() { let value = source().expect(\"x\"); }")?;
        let findings = check_unwrap_expect(&file, "x.rs");
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("expect"));
        Ok(())
    }

    #[test]
    fn unwrap_or_is_not_flagged() -> Result<(), syn::Error> {
        let file = syn::parse_file("fn handler() { let value = source().unwrap_or(0); }")?;
        assert!(check_unwrap_expect(&file, "x.rs").is_empty());
        Ok(())
    }

    #[test]
    fn unwrap_in_a_string_literal_is_not_flagged() -> Result<(), syn::Error> {
        let file = syn::parse_file("fn handler() { let note = \".unwrap() in prose\"; }")?;
        assert!(check_unwrap_expect(&file, "x.rs").is_empty());
        Ok(())
    }

    #[test]
    fn unwrap_inside_a_cfg_test_module_is_not_flagged() -> Result<(), syn::Error> {
        let file = syn::parse_file(
            "#[cfg(test)] mod tests { fn t() { let value = source().unwrap(); } }",
        )?;
        assert!(check_unwrap_expect(&file, "x.rs").is_empty());
        Ok(())
    }

    #[test]
    fn event_type_enum_outside_domain_is_flagged() -> Result<(), syn::Error> {
        let file = syn::parse_file("pub enum EventType { Accepted }")?;
        let findings = check_type_placement("console-application", &file, "adapters.rs");
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("EventType"));
        Ok(())
    }

    #[test]
    fn type_enums_in_domain_are_allowed() -> Result<(), syn::Error> {
        let file =
            syn::parse_file("pub enum EventType { Accepted } pub enum CommandType { Drain }")?;
        assert!(check_type_placement("console-domain", &file, "lib.rs").is_empty());
        Ok(())
    }

    #[test]
    fn forbid_unsafe_present_passes() -> Result<(), syn::Error> {
        let file = syn::parse_file("#![forbid(unsafe_code)]\nfn main() {}")?;
        assert!(check_forbid_unsafe(&file, "main.rs").is_empty());
        Ok(())
    }

    #[test]
    fn forbid_unsafe_absent_is_flagged() -> Result<(), syn::Error> {
        let file = syn::parse_file("fn main() {}")?;
        assert_eq!(check_forbid_unsafe(&file, "main.rs").len(), 1);
        Ok(())
    }

    #[test]
    fn adapter_isolation_flags_a_cross_module_reference() -> Result<(), syn::Error> {
        let file = syn::parse_file(
            "mod fabro { pub fn id() -> u8 { 1 } } \
             mod alpha { pub fn other() -> u8 { super::fabro::id() } }",
        )?;
        let findings = check_adapter_isolation(&file, "crates/x/src/source_adapters.rs");
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("alpha"));
        assert!(findings[0].contains("fabro"));
        Ok(())
    }

    #[test]
    fn adapter_isolation_allows_independent_modules() -> Result<(), syn::Error> {
        let file = syn::parse_file(
            "mod fabro { pub fn id() -> u8 { 1 } } \
             mod alpha { pub fn other() -> u8 { 2 } }",
        )?;
        assert!(check_adapter_isolation(&file, "crates/x/src/source_adapters.rs").is_empty());
        Ok(())
    }

    #[test]
    fn adapter_isolation_ignores_non_adapter_files() -> Result<(), syn::Error> {
        let file = syn::parse_file("mod fabro { } mod alpha { fn x() { super::fabro::y(); } }")?;
        assert!(check_adapter_isolation(&file, "crates/x/src/lib.rs").is_empty());
        Ok(())
    }

    #[test]
    fn backing_cli_default_bd_program_is_flagged() -> std::io::Result<()> {
        let root = temp_scan_root("zero-beads-bd-default")?;
        write_backing_cli_source(
            &root,
            r#"
                struct BackingCliPrograms { list_work_items: String }
                impl Default for BackingCliPrograms {
                    fn default() -> Self {
                        Self { list_work_items: "bd".to_owned() }
                    }
                }
            "#,
        )?;

        let findings = check_backing_cli_default_tokens(&root);

        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("`bd`"), "{findings:?}");
        fs::remove_dir_all(&root).ok();
        Ok(())
    }

    #[test]
    fn backing_cli_current_default_program_set_is_allowed() -> std::io::Result<()> {
        let root = temp_scan_root("zero-beads-allowed-defaults")?;
        write_backing_cli_source(
            &root,
            r#"
                struct BackingCliPrograms {
                    list_work_items: String,
                    livespec: CommandShape,
                    fabro: String,
                    dispatcher: String,
                    drive: String,
                    needs_attention: String,
                    github: String,
                }
                struct CommandShape;
                impl CommandShape {
                    fn new(_program: &str, _args: &[&str]) -> Self { Self }
                }
                impl Default for BackingCliPrograms {
                    fn default() -> Self {
                        Self {
                            list_work_items: "list-work-items".to_owned(),
                            livespec: CommandShape::new("livespec", &["next", "--json"]),
                            fabro: "fabro".to_owned(),
                            dispatcher: "livespec-dispatcher-drain".to_owned(),
                            drive: "livespec-orchestrator-drive".to_owned(),
                            needs_attention: "needs-attention".to_owned(),
                            github: "gh".to_owned(),
                        }
                    }
                }
                fn programs_from_plugin_bin(bin: &std::path::Path) -> BackingCliPrograms {
                    BackingCliPrograms {
                        list_work_items: bin.join("list_work_items.py").display().to_string(),
                        livespec: CommandShape::new("livespec", &["next", "--json"]),
                        fabro: "fabro".to_owned(),
                        dispatcher: bin.join("dispatcher.py").display().to_string(),
                        drive: bin.join("drive.py").display().to_string(),
                        needs_attention: bin.join("needs_attention.py").display().to_string(),
                        github: "gh".to_owned(),
                    }
                }
            "#,
        )?;

        let findings = check_backing_cli_default_tokens(&root);

        assert!(findings.is_empty(), "{findings:?}");
        fs::remove_dir_all(&root).ok();
        Ok(())
    }

    #[test]
    fn backing_cli_unparseable_source_is_flagged() -> std::io::Result<()> {
        let root = temp_scan_root("zero-beads-unparseable-backing-cli")?;
        write_backing_cli_source(&root, "impl Default for BackingCliPrograms {")?;

        let findings = check_backing_cli_default_tokens(&root);

        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("could not parse"), "{findings:?}");
        fs::remove_dir_all(&root).ok();
        Ok(())
    }

    #[test]
    fn beads_native_dot_beads_path_is_flagged() -> Result<(), syn::Error> {
        let file = syn::parse_file(
            r#"fn read_store(root: &std::path::Path) { let _path = root.join(".beads"); }"#,
        )?;

        let findings = check_beads_native_source_paths(&file, "crates/console-cli/src/main.rs");

        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains(".beads"), "{findings:?}");
        Ok(())
    }

    #[test]
    fn beads_native_doc_comment_is_not_flagged() -> Result<(), syn::Error> {
        let file = syn::parse_file(
            r"
            /// The tenant is deliberately NOT read from `.beads`.
            fn reads_nothing() {}
            ",
        )?;

        let findings = check_beads_native_source_paths(&file, "crates/console-cli/src/lib.rs");

        assert!(findings.is_empty(), "{findings:?}");
        Ok(())
    }

    #[test]
    fn zero_beads_scan_that_reads_no_rust_files_is_flagged() -> std::io::Result<()> {
        let root = temp_scan_root("zero-beads-no-rust-files")?;

        let findings = check_zero_beads_source_paths(&root);

        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("no Rust files"), "{findings:?}");
        fs::remove_dir_all(&root).ok();
        Ok(())
    }

    #[test]
    fn tmux_invocation_without_private_socket_is_flagged() {
        let source = r#"
            fn launch(tmux: &std::path::Path) {
                let _ = std::process::Command::new(tmux)
                    .args(["new-session", "-d", "-s", "session"])
                    .status();
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("-L"));
    }

    #[test]
    fn tmux_invocation_with_private_socket_before_command_is_allowed() {
        let source = r#"
            fn launch(tmux: &std::path::Path, scratch: &std::path::Path, socket: &str) {
                let _ = std::process::Command::new(tmux)
                    .env("TMUX_TMPDIR", scratch)
                    .args(["-L", socket, "new-session", "-d", "-s", "session"])
                    .status();
            }
        "#;
        assert!(check_tmux_socket_scoping_source("support/mod.rs", source).is_empty());
    }

    #[test]
    fn tmux_socket_without_private_tmpdir_is_flagged() {
        let source = r#"
            fn launch(tmux: &std::path::Path, socket: &str) {
                let _ = std::process::Command::new(tmux)
                    .args(["-L", socket, "new-session", "-d", "-s", "session"])
                    .status();
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("TMUX_TMPDIR"));
    }

    #[test]
    fn tmux_arg_subcommand_without_private_socket_is_flagged() {
        let source = r#"
            fn launch(tmux: &std::path::Path) {
                let _ = std::process::Command::new(tmux)
                    .arg("new-session")
                    .arg("-d")
                    .status();
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert_eq!(findings.len(), 1, "{findings:?}");
    }

    #[test]
    fn tmux_forwarded_args_without_private_socket_are_flagged() {
        let source = r"
            fn run_tmux(tmux: &std::path::Path, args: &[&str]) {
                let _ = std::process::Command::new(tmux).args(args).output();
            }
        ";
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert_eq!(findings.len(), 1, "{findings:?}");
    }

    #[test]
    fn tmux_socket_after_subcommand_is_flagged() {
        let source = r#"
            fn launch(tmux: &std::path::Path, scratch: &std::path::Path, socket: &str) {
                let _ = std::process::Command::new(tmux)
                    .env("TMUX_TMPDIR", scratch)
                    .args(["new-session", "-L", socket, "-d", "-s", "session"])
                    .status();
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert_eq!(findings.len(), 1, "{findings:?}");
    }

    // -----------------------------------------------------------------------
    // Suspect-by-default regressions.
    //
    // Each case below passed the earlier allow-list-driven check clean. They
    // are paired: a form that MUST be flagged, and the corresponding correct
    // form that MUST NOT be, so tightening the rule cannot drift into flagging
    // code that is already right.
    // -----------------------------------------------------------------------

    /// The live harness shape, pinned. `crates/console-cli/tests/support/mod.rs`
    /// is CORRECT, and every tightening here must leave it unflagged.
    #[test]
    fn the_real_harness_invocation_shape_is_not_flagged() {
        let source = r#"
            fn run_tmux(tmux: &Path, socket: &str, tmux_tmpdir: &Path, args: &[&str]) {
                let _ = Command::new(tmux)
                    .env("TMUX_TMPDIR", tmux_tmpdir)
                    .arg("-L")
                    .arg(socket)
                    .args(args)
                    .output();
            }
            fn launch(tmux: &Path, scratch: &Path, socket: &str, session: &str) {
                let _ = Command::new(&tmux)
                    .env("TMUX_TMPDIR", &scratch)
                    .args(["-L", socket, "new-session", "-d", "-s", session])
                    .arg("launcher")
                    .status();
            }
        "#;
        assert!(check_tmux_socket_scoping_source("support/mod.rs", source).is_empty());
    }

    // Defect 1 — an unenumerated sub-command used to disable the rule entirely.

    #[test]
    fn run_shell_subcommand_without_private_socket_is_flagged() {
        // The original bypass: `run-shell` was outside the six-entry
        // sub-command list, so this all-literal command was never checked at
        // all — and it kills the host's shared server.
        let source = r#"
            fn launch(tmux: &std::path::Path) {
                let _ = std::process::Command::new(tmux)
                    .args(["run-shell", "tmux kill-server"])
                    .status();
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("-L"));
    }

    #[test]
    fn a_subcommand_this_check_has_never_heard_of_is_still_flagged() {
        // `choose-tree` is in no list anywhere in this file. An unrecognized
        // sub-command must mean "still checked", not "rule does not apply".
        let source = r#"
            fn launch(tmux: &std::path::Path) {
                let _ = std::process::Command::new(tmux).args(["choose-tree"]).status();
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert_eq!(findings.len(), 1, "{findings:?}");
    }

    #[test]
    fn an_unenumerated_subcommand_with_a_private_socket_is_allowed() {
        let source = r#"
            fn launch(tmux: &std::path::Path, scratch: &std::path::Path, socket: &str) {
                let _ = std::process::Command::new(tmux)
                    .env("TMUX_TMPDIR", scratch)
                    .args(["-L", socket, "run-shell", "echo hi"])
                    .status();
            }
        "#;
        assert!(check_tmux_socket_scoping_source("support/mod.rs", source).is_empty());
    }

    #[test]
    fn a_tmux_version_query_needs_no_socket() {
        let source = r#"
            fn version(tmux: &std::path::Path) {
                let _ = std::process::Command::new(tmux).arg("-V").output();
            }
        "#;
        assert!(check_tmux_socket_scoping_source("support/mod.rs", source).is_empty());
    }

    // Defect 2 — argument VALUES were never validated, only key names and
    // positions.

    #[test]
    fn the_default_socket_label_is_flagged() {
        // The exact shape of the original incident: both the key name and the
        // `-L` position were satisfied, so the old check passed it clean, yet
        // it resolves to /tmp/tmux-<uid>/default — the shared server.
        let source = r#"
            fn launch(tmux: &std::path::Path) {
                let _ = std::process::Command::new(tmux)
                    .env("TMUX_TMPDIR", "/tmp")
                    .args(["-L", "default", "kill-server"])
                    .status();
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("default"), "{findings:?}");
        assert!(findings[0].contains("TMUX_TMPDIR"), "{findings:?}");
    }

    #[test]
    fn a_tmux_tmpdir_of_tmp_is_flagged_even_with_a_private_label() {
        let source = r#"
            fn launch(tmux: &std::path::Path) {
                let _ = std::process::Command::new(tmux)
                    .env("TMUX_TMPDIR", "/tmp")
                    .args(["-L", "lc_e2e_7", "kill-server"])
                    .status();
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("TMUX_TMPDIR"), "{findings:?}");
    }

    #[test]
    fn a_tmux_tmpdir_pointing_into_a_default_namespace_is_flagged() {
        let source = r#"
            fn launch(tmux: &std::path::Path) {
                let _ = std::process::Command::new(tmux)
                    .env("TMUX_TMPDIR", "/tmp/tmux-1000")
                    .args(["-L", "lc_e2e_7", "kill-server"])
                    .status();
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert_eq!(findings.len(), 1, "{findings:?}");
    }

    #[test]
    fn a_tmux_tmpdir_that_traverses_back_to_tmp_is_flagged() {
        let source = r#"
            fn launch(tmux: &std::path::Path) {
                let _ = std::process::Command::new(tmux)
                    .env("TMUX_TMPDIR", "/tmp/scratch/..")
                    .args(["-L", "lc_e2e_7", "kill-server"])
                    .status();
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert_eq!(findings.len(), 1, "{findings:?}");
    }

    #[test]
    fn literal_private_scratch_and_label_values_are_allowed() {
        let source = r#"
            fn launch(tmux: &std::path::Path) {
                let _ = std::process::Command::new(tmux)
                    .env("TMUX_TMPDIR", "/tmp/lc-e2e-4242")
                    .args(["-L", "lc_e2e_4242", "kill-server"])
                    .status();
            }
        "#;
        assert!(check_tmux_socket_scoping_source("support/mod.rs", source).is_empty());
    }

    #[test]
    fn clearing_the_environment_after_setting_tmux_tmpdir_is_flagged() {
        let source = r#"
            fn launch(tmux: &std::path::Path, scratch: &std::path::Path, socket: &str) {
                let _ = std::process::Command::new(tmux)
                    .env("TMUX_TMPDIR", scratch)
                    .env_clear()
                    .args(["-L", socket, "kill-server"])
                    .status();
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("TMUX_TMPDIR"), "{findings:?}");
    }

    #[test]
    fn an_s_flag_socket_path_does_not_satisfy_the_rule() {
        // `-S` names a socket PATH; pointing it at the default namespace is the
        // same hazard, and it is not the `-L` private label the rule requires.
        let source = r#"
            fn launch(tmux: &std::path::Path, scratch: &std::path::Path) {
                let _ = std::process::Command::new(tmux)
                    .env("TMUX_TMPDIR", scratch)
                    .args(["-S", "/tmp/tmux-1000/default", "kill-server"])
                    .status();
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("-L"), "{findings:?}");
    }

    // Defect 3 — detection failed open on a rename or an indirection.

    #[test]
    fn a_renamed_tmux_binding_is_flagged() {
        let source = r#"
            fn launch(state: &State) {
                let _ = std::process::Command::new(&state.tmux_bin)
                    .args(["kill-server"])
                    .status();
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert_eq!(findings.len(), 1, "{findings:?}");
    }

    #[test]
    fn a_resolver_call_returning_tmux_is_flagged() {
        let source = r#"
            fn launch() {
                let _ = std::process::Command::new(resolve_tmux())
                    .args(["kill-server"])
                    .status();
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert_eq!(findings.len(), 1, "{findings:?}");
    }

    #[test]
    fn a_tmux_path_field_is_flagged() {
        let source = r#"
            fn launch(paths: &Paths) {
                let _ = std::process::Command::new(paths.tmux_path)
                    .args(["kill-server"])
                    .status();
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert_eq!(findings.len(), 1, "{findings:?}");
    }

    #[test]
    fn a_renamed_tmux_binding_with_a_private_socket_is_allowed() {
        let source = r#"
            fn launch(state: &State, scratch: &std::path::Path, socket: &str) {
                let _ = std::process::Command::new(&state.tmux_bin)
                    .env("TMUX_TMPDIR", scratch)
                    .args(["-L", socket, "kill-server"])
                    .status();
            }
        "#;
        assert!(check_tmux_socket_scoping_source("support/mod.rs", source).is_empty());
    }

    // ---------------------------------------------------------------------
    // The four bypasses an independent adversarial review found AFTER the
    // suspect-by-default pass. Each is paired must-flag / must-not-flag: the
    // evading shape is caught, and the ordinary shape it resembles is not.
    // ---------------------------------------------------------------------

    #[test]
    fn a_tmux_command_wrapped_in_a_macro_is_flagged() {
        // THE ONE MOST LIKELY TO HAPPEN BY ACCIDENT. `syn` leaves macro bodies as
        // an opaque token stream, and the governed file is a TEST file where
        // wrapping a command in `assert!` is the default idiom — so this compiled,
        // hit the SHARED server, and was scanned by nothing.
        let source = r#"
            fn launch(tmux: &str) {
                assert!(
                    std::process::Command::new(tmux)
                        .args(["-L", "default", "kill-server"])
                        .status()
                        .is_ok()
                );
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert!(!findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn a_scoped_tmux_command_inside_a_macro_is_not_flagged() {
        let source = r#"
            fn launch(tmux: &str, socket: &str, scratch: &Path) {
                assert!(
                    std::process::Command::new(tmux)
                        .env("TMUX_TMPDIR", scratch)
                        .args(["-L", socket, "kill-server"])
                        .status()
                        .is_ok()
                );
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn a_shell_wrapped_tmux_invocation_is_flagged() {
        // A resolved, definitely-not-tmux program with tmux buried in an argument.
        // The previous revision exempted this arm unconditionally, so it trusted a
        // resolved-but-wrong program MORE than an unresolvable one.
        let source = r#"
            fn nuke() {
                let _ = std::process::Command::new("sh")
                    .arg("-c")
                    .arg("tmux kill-server")
                    .status();
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert!(!findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn a_shell_command_that_never_mentions_tmux_is_not_flagged() {
        let source = r#"
            fn list() {
                let _ = std::process::Command::new("sh")
                    .arg("-c")
                    .arg("gh pr list --json number")
                    .status();
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn an_unscoped_tmux_launched_via_exec_is_flagged() {
        // `CommandExt::exec` replaces the process image and never returns — as real
        // a launcher as `status`, and previously uninspected.
        let source = r#"
            fn nuke(tmux: &str) {
                let _ = std::process::Command::new(tmux)
                    .args(["kill-server"])
                    .exec();
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert!(!findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn an_unparsable_macro_body_mentioning_tmux_is_flagged() {
        // An unparsable macro body must not become a NEW way to hide: if it names
        // tmux and cannot be read, that is a finding, not a pass.
        let source = r"
            fn launch() {
                some_macro! { this is not => an expression list tmux kill-server }
            }
        ";
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert!(!findings.is_empty(), "{findings:?}");
        assert!(findings[0].contains("macro body"), "{findings:?}");
    }

    #[test]
    fn an_unresolvable_tmux_shaped_builder_is_flagged() {
        let source = r"
            fn launch(harness: &Harness) {
                let _ = harness.tmux_command().status();
            }
        ";
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("cannot be resolved"), "{findings:?}");
    }

    #[test]
    fn ordinary_non_tmux_commands_are_not_flagged() {
        // These mirror the real non-tmux call sites in the workspace
        // (`crates/console-cli/tests/finding_e_python_exec.rs` and the backing
        // CLI spawn in `crates/console-cli/src/main.rs`). An unreadable program
        // expression with no tmux evidence must stay clean, or the check trains
        // people to work around it.
        let source = r#"
            fn run(script: &str, program: &str, args: &[&str], builder: &Builder) {
                let _ = std::process::Command::new(script).arg("--json").output();
                let _ = std::process::Command::new(program).args(args).output();
                let _ = std::process::Command::new("gh").args(["pr", "list"]).output();
                let _ = builder.git_command().status();
            }
        "#;
        assert!(check_tmux_socket_scoping_source("support/mod.rs", source).is_empty());
    }

    #[test]
    fn an_unreadable_program_with_a_tmux_subcommand_is_flagged() {
        let source = r#"
            fn launch(program: &str) {
                let _ = std::process::Command::new(program)
                    .args(["kill-server"])
                    .status();
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert_eq!(findings.len(), 1, "{findings:?}");
    }

    // Defect 4 — the standard non-chained builder idiom was never analyzed.

    #[test]
    fn a_non_chained_command_builder_is_flagged() {
        let source = r#"
            fn launch(tmux: &std::path::Path) {
                let mut command = std::process::Command::new(tmux);
                command.args(["-L", "default", "kill-server"]);
                let _ = command.status();
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("default"), "{findings:?}");
    }

    #[test]
    fn a_non_chained_command_builder_with_a_private_socket_is_allowed() {
        let source = r#"
            fn launch(tmux: &std::path::Path, scratch: &std::path::Path, socket: &str) {
                let mut command = std::process::Command::new(tmux);
                command.env("TMUX_TMPDIR", scratch);
                command.args(["-L", socket]);
                command.args(["kill-server"]);
                let _ = command.status();
            }
        "#;
        assert!(check_tmux_socket_scoping_source("support/mod.rs", source).is_empty());
    }

    // Defect 5 — `.spawn()` was not inspected.

    #[test]
    fn a_spawned_tmux_invocation_is_flagged() {
        let source = r#"
            fn launch(tmux: &std::path::Path) {
                let _ = std::process::Command::new(tmux)
                    .args(["new-session", "-d", "-s", "session"])
                    .spawn();
            }
        "#;
        let findings = check_tmux_socket_scoping_source("support/mod.rs", source);
        assert_eq!(findings.len(), 1, "{findings:?}");
    }

    #[test]
    fn a_spawned_tmux_invocation_with_a_private_socket_is_allowed() {
        let source = r#"
            fn launch(tmux: &std::path::Path, scratch: &std::path::Path, socket: &str) {
                let _ = std::process::Command::new(tmux)
                    .env("TMUX_TMPDIR", scratch)
                    .args(["-L", socket, "new-session", "-d"])
                    .spawn();
            }
        "#;
        assert!(check_tmux_socket_scoping_source("support/mod.rs", source).is_empty());
    }

    // Defect 6 — the walk failed open when its directory moved.

    #[test]
    fn a_scan_that_reads_no_rust_files_is_flagged() {
        // Renaming the scanned directory used to green the check while it read
        // zero files. This mirrors the justfile's zero-test guard on the E2E
        // suite: having read nothing is a failure, never a pass.
        let findings =
            check_tmux_socket_scoping(Path::new("/nonexistent/console-arch-check/moved-scan-root"));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("no Rust files"), "{findings:?}");
    }

    #[test]
    fn a_scan_of_a_real_root_reads_files_and_passes() {
        // The positive control for the guard above: a real root yields Rust
        // files (so the zero-file finding does NOT fire) and this crate's own
        // sources are clean.
        let findings = check_tmux_socket_scoping(Path::new(env!("CARGO_MANIFEST_DIR")));
        assert!(findings.is_empty(), "{findings:?}");
    }

    // Defect 6b — the symlink arm refused links that could not leave the tree,
    // then skipped them silently, which left in-tree targets unscanned.

    #[test]
    fn a_symlink_resolving_inside_the_scan_root_is_followed() -> std::io::Result<()> {
        // The must-NOT-flag half. `CLAUDE.md -> AGENTS.md` is the real case:
        // following it cannot leave the repository, so it is scanned, not refused.
        let temp = temp_scan_root("symlink-inside")?;
        fs::write(temp.join("real.rs"), "fn f() {}\n")?;
        std::os::unix::fs::symlink("real.rs", temp.join("link.rs"))?;

        let (paths, findings) = rust_files_for_tmux_scan(&temp);

        assert!(findings.is_empty(), "{findings:?}");
        assert!(
            paths.iter().any(|path| path.ends_with("real.rs")),
            "the target must be scanned: {paths:?}"
        );
        fs::remove_dir_all(&temp).ok();
        Ok(())
    }

    #[test]
    fn a_symlink_into_a_skipped_directory_is_still_scanned() -> std::io::Result<()> {
        // The reason in-tree links are FOLLOWED rather than skipped. `target/`
        // is on the skip-list, so the walk has no other route to this file. An
        // earlier revision skipped the link silently on the reasoning that the
        // real path is walked anyway — leaving the content unscanned AND
        // unreported, which is the vacuity this rule exists to cure.
        let temp = temp_scan_root("symlink-into-skipped")?;
        fs::create_dir_all(temp.join("target"))?;
        fs::write(temp.join("target/generated.rs"), "fn f() {}\n")?;
        std::os::unix::fs::symlink("target/generated.rs", temp.join("link.rs"))?;

        let (paths, findings) = rust_files_for_tmux_scan(&temp);

        assert!(findings.is_empty(), "{findings:?}");
        assert!(
            paths.iter().any(|path| path.ends_with("generated.rs")),
            "a link is the only route to this file, so it must be scanned: {paths:?}"
        );
        fs::remove_dir_all(&temp).ok();
        Ok(())
    }

    #[test]
    fn a_symlink_cycle_terminates_and_reports_nothing() -> std::io::Result<()> {
        // Following in-tree links makes the walk a graph, so the visited set is
        // load-bearing: without it this test hangs rather than fails.
        let temp = temp_scan_root("symlink-cycle")?;
        fs::create_dir_all(temp.join("inner"))?;
        fs::write(temp.join("inner/real.rs"), "fn f() {}\n")?;
        std::os::unix::fs::symlink(temp.join("inner"), temp.join("inner/loop"))?;

        let (paths, findings) = rust_files_for_tmux_scan(&temp);

        assert!(findings.is_empty(), "{findings:?}");
        assert_eq!(
            paths
                .iter()
                .filter(|path| path.ends_with("real.rs"))
                .count(),
            1,
            "the cycle must not re-enqueue the subtree: {paths:?}"
        );
        fs::remove_dir_all(&temp).ok();
        Ok(())
    }

    #[test]
    fn a_symlink_escaping_the_scan_root_is_flagged() -> std::io::Result<()> {
        // The must-flag half, and the reason the arm exists: this link really
        // does hide content from the scan, so it stays suspect-by-default.
        let outside = temp_scan_root("symlink-outside-target")?;
        fs::write(outside.join("hidden.rs"), "fn f() {}\n")?;
        let temp = temp_scan_root("symlink-outside")?;
        std::os::unix::fs::symlink(outside.join("hidden.rs"), temp.join("link.rs"))?;

        let (_paths, findings) = rust_files_for_tmux_scan(&temp);

        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("skipped the symlink"), "{findings:?}");
        fs::remove_dir_all(&temp).ok();
        fs::remove_dir_all(&outside).ok();
        Ok(())
    }

    #[test]
    fn a_dangling_symlink_is_flagged() -> std::io::Result<()> {
        // Resolution fails CLOSED: an unresolvable target is reported, never
        // silently treated as in-tree.
        let temp = temp_scan_root("symlink-dangling")?;
        fs::write(temp.join("real.rs"), "fn f() {}\n")?;
        std::os::unix::fs::symlink("nowhere.rs", temp.join("link.rs"))?;

        let (_paths, findings) = rust_files_for_tmux_scan(&temp);

        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("skipped the symlink"), "{findings:?}");
        fs::remove_dir_all(&temp).ok();
        Ok(())
    }

    /// A fresh empty scratch directory named for one test.
    ///
    /// Deliberately a STABLE per-test name rather than a random one: a failed
    /// run leaves its tree inspectable, and the leading remove makes each run
    /// independent of the last one's leftovers. `CARGO_TARGET_TMPDIR` is not
    /// available here — cargo defines it for integration tests, not for a bin
    /// target's unit tests — so this roots under the system temp dir instead.
    fn temp_scan_root(name: &str) -> std::io::Result<std::path::PathBuf> {
        let path = std::env::temp_dir().join(format!("console-arch-check-{name}"));
        fs::remove_dir_all(&path).ok();
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    fn workspace_crates(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    fn write_backing_cli_source(root: &Path, source: &str) -> std::io::Result<()> {
        let path = root.join("crates/console-cli/src/backing_cli.rs");
        fs::create_dir_all(path.parent().unwrap_or(root))?;
        fs::write(path, source)
    }

    fn temp_fabro_toolchain_root(
        name: &str,
        channel: &str,
        components: &[&str],
        image: &str,
    ) -> std::io::Result<std::path::PathBuf> {
        let root = temp_scan_root(name)?;
        fs::create_dir_all(root.join(".fabro/workflows/implement-work-item"))?;
        let component_list = components
            .iter()
            .map(|component| format!("\"{component}\""))
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(
            root.join("rust-toolchain.toml"),
            format!("[toolchain]\nchannel = \"{channel}\"\ncomponents = [{component_list}]\n"),
        )?;
        fs::write(
            root.join(".fabro/workflows/implement-work-item/workflow.toml"),
            format!("[environments.livespec-ci.image]\ndocker = \"{image}\"\n"),
        )?;
        Ok(root)
    }

    fn temp_workspace_toolchain_root(
        name: &str,
        rust_version: &str,
        channel: &str,
    ) -> std::io::Result<std::path::PathBuf> {
        let root = temp_scan_root(name)?;
        fs::write(
            root.join("Cargo.toml"),
            format!(
                "[workspace.package]\nrust-version = \"{rust_version}\"\n\
                 [workspace]\nmembers = []\n"
            ),
        )?;
        fs::write(
            root.join("rust-toolchain.toml"),
            format!("[toolchain]\nchannel = \"{channel}\"\ncomponents = []\n"),
        )?;
        Ok(root)
    }

    #[test]
    fn workspace_rust_version_two_component_prefix_matches_toolchain() -> std::io::Result<()> {
        let root = temp_workspace_toolchain_root("workspace-rust-msrv-match", "1.92", "1.92.0")?;
        let findings = check_workspace_rust_version_matches_toolchain(&root);
        assert!(findings.is_empty(), "{findings:?}");
        fs::remove_dir_all(&root).ok();
        Ok(())
    }

    #[test]
    fn workspace_rust_version_component_mismatch_is_flagged() -> std::io::Result<()> {
        let root = temp_workspace_toolchain_root("workspace-rust-msrv-mismatch", "1.91", "1.92.0")?;
        let findings = check_workspace_rust_version_matches_toolchain(&root);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("1.91"), "{findings:?}");
        assert!(findings[0].contains("1.92.0"), "{findings:?}");
        fs::remove_dir_all(&root).ok();
        Ok(())
    }

    #[test]
    fn workspace_rust_version_uses_component_prefix_not_string_prefix() -> std::io::Result<()> {
        let root =
            temp_workspace_toolchain_root("workspace-rust-msrv-string-prefix", "1.92", "1.920.0")?;
        let findings = check_workspace_rust_version_matches_toolchain(&root);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("1.92"), "{findings:?}");
        assert!(findings[0].contains("1.920.0"), "{findings:?}");
        fs::remove_dir_all(&root).ok();
        Ok(())
    }

    #[test]
    fn missing_workspace_rust_version_is_flagged() -> std::io::Result<()> {
        let root = temp_scan_root("workspace-rust-msrv-missing-key")?;
        fs::write(
            root.join("Cargo.toml"),
            "[workspace.package]\nversion = \"0.0.0\"\n",
        )?;
        fs::write(
            root.join("rust-toolchain.toml"),
            "[toolchain]\nchannel = \"1.92.0\"\ncomponents = []\n",
        )?;
        let findings = check_workspace_rust_version_matches_toolchain(&root);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings[0].contains("missing workspace.package.rust-version"),
            "{findings:?}"
        );
        fs::remove_dir_all(&root).ok();
        Ok(())
    }

    #[test]
    fn missing_workspace_manifest_file_is_flagged() -> std::io::Result<()> {
        let root = temp_scan_root("workspace-rust-msrv-missing-file")?;
        fs::write(
            root.join("rust-toolchain.toml"),
            "[toolchain]\nchannel = \"1.92.0\"\ncomponents = []\n",
        )?;
        let findings = check_workspace_rust_version_matches_toolchain(&root);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("could not read"), "{findings:?}");
        assert!(findings[0].contains("Cargo.toml"), "{findings:?}");
        fs::remove_dir_all(&root).ok();
        Ok(())
    }

    #[test]
    fn missing_rust_toolchain_file_is_flagged() -> std::io::Result<()> {
        let root = temp_scan_root("workspace-rust-toolchain-missing-file")?;
        fs::write(
            root.join("Cargo.toml"),
            "[workspace.package]\nrust-version = \"1.92\"\n",
        )?;
        let findings = check_workspace_rust_version_matches_toolchain(&root);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("could not read"), "{findings:?}");
        assert!(findings[0].contains("rust-toolchain.toml"), "{findings:?}");
        fs::remove_dir_all(&root).ok();
        Ok(())
    }

    #[test]
    fn missing_rust_toolchain_channel_is_flagged() -> std::io::Result<()> {
        let root = temp_scan_root("workspace-rust-toolchain-missing-channel")?;
        fs::write(
            root.join("Cargo.toml"),
            "[workspace.package]\nrust-version = \"1.92\"\n",
        )?;
        fs::write(
            root.join("rust-toolchain.toml"),
            "[toolchain]\ncomponents = []\n",
        )?;
        let findings = check_workspace_rust_version_matches_toolchain(&root);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings[0].contains("missing toolchain.channel"),
            "{findings:?}"
        );
        fs::remove_dir_all(&root).ok();
        Ok(())
    }

    #[test]
    fn fabro_image_rust_toolchain_match_is_allowed() -> std::io::Result<()> {
        let image = "ghcr.io/thewoolleyman/livespec-fabro-sandbox:python-rust-agent-v1.31.1";
        let root =
            temp_fabro_toolchain_root("fabro-rust-match", "1.92.0", &["clippy", "rustfmt"], image)?;
        let findings = check_fabro_image_rust_toolchain_with_probe(&root, |probed_image| {
            assert_eq!(probed_image, image);
            Ok("rustc 1.92.0 (abc 2026-01-01)\nclippy 0.1.92\nrustfmt 1.92.0\n".to_owned())
        });
        assert!(findings.is_empty(), "{findings:?}");
        fs::remove_dir_all(&root).ok();
        Ok(())
    }

    #[test]
    fn fabro_image_rust_toolchain_mismatch_is_flagged() -> std::io::Result<()> {
        let image = "ghcr.io/thewoolleyman/livespec-fabro-sandbox:python-rust-agent-v1.31.1";
        let root = temp_fabro_toolchain_root(
            "fabro-rust-mismatch",
            "1.92.0",
            &["clippy", "rustfmt"],
            image,
        )?;
        let findings = check_fabro_image_rust_toolchain_with_probe(&root, |_| {
            Ok("rustc 1.91.0 (abc 2026-01-01)\nclippy 0.1.91\nrustfmt 1.91.0\n".to_owned())
        });
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("rustc 1.91.0"), "{findings:?}");
        assert!(findings[0].contains("1.92.0"), "{findings:?}");
        fs::remove_dir_all(&root).ok();
        Ok(())
    }

    #[test]
    fn unreadable_fabro_image_probe_is_flagged() -> std::io::Result<()> {
        let image = "ghcr.io/thewoolleyman/livespec-fabro-sandbox:python-rust-agent-v1.31.1";
        let root = temp_fabro_toolchain_root(
            "fabro-rust-unreadable",
            "1.92.0",
            &["clippy", "rustfmt"],
            image,
        )?;
        let findings = check_fabro_image_rust_toolchain_with_probe(&root, |_| {
            Err("docker daemon unavailable".to_owned())
        });
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("could not read image"), "{findings:?}");
        assert!(
            findings[0].contains("docker daemon unavailable"),
            "{findings:?}"
        );
        fs::remove_dir_all(&root).ok();
        Ok(())
    }

    #[test]
    fn vacuous_fabro_image_probe_output_is_flagged() -> std::io::Result<()> {
        let image = "ghcr.io/thewoolleyman/livespec-fabro-sandbox:python-rust-agent-v1.31.1";
        let root = temp_fabro_toolchain_root(
            "fabro-rust-vacuous",
            "1.92.0",
            &["clippy", "rustfmt"],
            image,
        )?;
        let findings = check_fabro_image_rust_toolchain_with_probe(&root, |_| Ok(String::new()));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings[0].contains("no usable Rust evidence"),
            "{findings:?}"
        );
        fs::remove_dir_all(&root).ok();
        Ok(())
    }

    #[test]
    fn missing_required_fabro_image_component_is_flagged() -> std::io::Result<()> {
        let image = "ghcr.io/thewoolleyman/livespec-fabro-sandbox:python-rust-agent-v1.31.1";
        let root = temp_fabro_toolchain_root(
            "fabro-rust-component",
            "1.92.0",
            &["clippy", "rustfmt"],
            image,
        )?;
        let findings = check_fabro_image_rust_toolchain_with_probe(&root, |_| {
            Ok("rustc 1.92.0 (abc 2026-01-01)\nclippy 0.1.92\n".to_owned())
        });
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("rustfmt"), "{findings:?}");
        fs::remove_dir_all(&root).ok();
        Ok(())
    }

    #[test]
    fn fabro_image_pin_must_stay_on_python_rust_agent_layer() -> std::io::Result<()> {
        let root = temp_fabro_toolchain_root(
            "fabro-rust-wrong-image",
            "1.92.0",
            &["clippy", "rustfmt"],
            "ghcr.io/thewoolleyman/livespec-fabro-sandbox:python-agent-v1.31.1",
        )?;
        let result = fabro_python_rust_image(&root);
        assert!(result.is_err(), "wrong image must fail");
        let error = result.err().unwrap_or_default();
        assert!(error.contains("python-rust-agent"), "{error}");
        fs::remove_dir_all(&root).ok();
        Ok(())
    }

    #[test]
    fn observed_rust_toolchain_requires_rustc_evidence() {
        let result = observed_rust_toolchain("clippy 0.1.92\nrustfmt 1.92.0\n");
        assert!(result.is_err(), "missing rustc must fail");
        let error = result.err().unwrap_or_default();
        assert!(error.contains("rustc"), "{error}");
    }

    #[test]
    fn image_config_history_rust_toolchain_evidence_is_usable() -> serde_json::Result<()> {
        let config: serde_json::Value = serde_json::from_str(
            r#"{
              "history": [
                {"created_by": "ARG RUST_VERSION=1.92.0"},
                {"created_by": "RUN |1 RUST_VERSION=1.92.0 /bin/sh -c rustup --default-toolchain ${RUST_VERSION} --component clippy,rustfmt # buildkit"}
              ]
            }"#,
        )?;
        let output = observed_rust_toolchain_from_image_config(&config).unwrap_or_default();
        let observed = observed_rust_toolchain(&output).unwrap_or(ObservedRustToolchain {
            rustc_version: String::new(),
            components: BTreeSet::new(),
        });
        assert_eq!(observed.rustc_version, "1.92.0");
        assert!(observed.components.contains("clippy"));
        assert!(observed.components.contains("rustfmt"));
        Ok(())
    }

    #[test]
    fn image_config_history_without_rust_evidence_is_flagged() -> serde_json::Result<()> {
        let config: serde_json::Value = serde_json::from_str(
            r#"{
              "history": [
                {"created_by": "RUN /bin/sh -c echo no rust evidence"}
              ]
            }"#,
        )?;
        let result = observed_rust_toolchain_from_image_config(&config);
        assert!(result.is_err(), "missing image evidence must fail");
        let error = result.err().unwrap_or_default();
        assert!(error.contains("Rust version"), "{error}");
        Ok(())
    }

    #[test]
    fn a_direct_valve_staging_outside_the_registry_path_is_flagged() -> syn::Result<()> {
        let source = r"
            fn sneaky_key_arm() -> TuiInteraction {
                TuiInteraction::OpenValveConfirm(PendingValve::Approve)
            }
        ";
        let findings = check_registry_bypass("console-tui", &syn::parse_file(source)?, "lib.rs");
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("OpenValveConfirm"));
        Ok(())
    }

    #[test]
    fn a_direct_driver_handoff_staging_outside_the_registry_path_is_flagged() -> syn::Result<()> {
        let source = r"
            fn sneaky_key_arm() -> TuiInteraction {
                TuiInteraction::OpenDriverHandoff
            }
        ";
        let findings = check_registry_bypass("console-tui", &syn::parse_file(source)?, "lib.rs");
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("OpenDriverHandoff"));
        Ok(())
    }

    #[test]
    fn the_registry_staging_path_is_not_flagged() -> syn::Result<()> {
        let source = r"
            fn registry_action_input() -> TuiInteraction {
                TuiInteraction::OpenValveConfirm(PendingValve::Approve)
            }
        ";
        let file = syn::parse_file(source)?;
        assert!(check_registry_bypass("console-tui", &file, "lib.rs").is_empty());
        Ok(())
    }

    #[test]
    fn test_code_staging_is_not_flagged() -> syn::Result<()> {
        let source = r"
            #[cfg(test)]
            mod tests {
                fn helper() -> TuiInteraction {
                    TuiInteraction::OpenValveConfirm(PendingValve::Approve)
                }
            }
        ";
        let file = syn::parse_file(source)?;
        assert!(check_registry_bypass("console-tui", &file, "lib.rs").is_empty());
        Ok(())
    }

    #[test]
    fn non_tui_crates_are_not_scanned_for_registry_bypass() -> syn::Result<()> {
        let source = r"
            fn reducer_arm() -> TuiInteraction {
                TuiInteraction::OpenValveConfirm(PendingValve::Approve)
            }
        ";
        let file = syn::parse_file(source)?;
        assert!(check_registry_bypass("console-application", &file, "lib.rs").is_empty());
        Ok(())
    }
}
