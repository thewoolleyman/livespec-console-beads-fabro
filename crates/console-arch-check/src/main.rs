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
    findings.extend(check_tmux_socket_scoping(root));
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
            findings.push(format!(
                "tmux socket-scoping scan skipped the symlink {} — following it \
                 could leave the repository, so its contents are UNSCANNED; move \
                 the real directory into the tree or add it to the skip-list \
                 deliberately",
                path.display()
            ));
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
    use std::path::Path;

    use super::{
        CrateNode, check_adapter_isolation, check_forbid_unsafe, check_layering,
        check_tmux_socket_scoping, check_tmux_socket_scoping_source, check_type_placement,
        check_unwrap_expect,
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
}
