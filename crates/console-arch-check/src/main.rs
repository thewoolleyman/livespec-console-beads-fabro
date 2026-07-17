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

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cargo_metadata::{DependencyKind, MetadataCommand};
use syn::visit::Visit;

/// Workspace crates whose source is scanned for the AST-level rules.
const SCANNED_CRATES: &[&str] = &[
    "console-application",
    "console-cli",
    "console-domain",
    "console-eventstore",
    "console-tui",
];

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
    for crate_name in SCANNED_CRATES {
        let crate_dir = root.join("crates").join(crate_name);
        findings.extend(check_crate_sources(crate_name, &crate_dir));
    }
    findings
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
    check_layering(&nodes)
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
    }
    findings
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
    use super::{
        CrateNode, check_adapter_isolation, check_forbid_unsafe, check_layering,
        check_type_placement, check_unwrap_expect,
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
}
