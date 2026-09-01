//! Resolve host backing CLIs for live console source ports.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const ORCHESTRATOR_PLUGIN_NAME: &str = "livespec-orchestrator-beads-fabro";
const PLUGIN_ROOT_ENV: &str = "LIVESPEC_CONSOLE_ORCHESTRATOR_PLUGIN_ROOT";
const SELECTED_REPO_PATH_ENV: &str = "LIVESPEC_CONSOLE_REPO_PATH";
const LIST_WORK_ITEMS_PROGRAM_ENV: &str = "LIVESPEC_CONSOLE_LIST_WORK_ITEMS_PROGRAM";
const LIVESPEC_PROGRAM_ENV: &str = "LIVESPEC_CONSOLE_LIVESPEC_PROGRAM";
const FABRO_PROGRAM_ENV: &str = "LIVESPEC_CONSOLE_FABRO_PROGRAM";
const DRAIN_PROGRAM_ENV: &str = "LIVESPEC_CONSOLE_DRAIN_PROGRAM";
const DRIVE_PROGRAM_ENV: &str = "LIVESPEC_CONSOLE_DRIVE_PROGRAM";
const NEEDS_ATTENTION_PROGRAM_ENV: &str = "LIVESPEC_CONSOLE_NEEDS_ATTENTION_PROGRAM";
const RECONCILE_RUNS_PROGRAM_ENV: &str = "LIVESPEC_CONSOLE_RECONCILE_RUNS_PROGRAM";
const GH_PROGRAM_ENV: &str = "LIVESPEC_CONSOLE_GH_PROGRAM";
const INVOKER_ENV: &str = "LIVESPEC_INVOKER";

/// Home-relative install locations probed for the `fabro` binary, in order,
/// when it is not overridden by [`FABRO_PROGRAM_ENV`]. The cockpit runs under
/// the credential wrapper, whose scrubbed `PATH` does NOT include `~/.local/bin`,
/// so a bare `fabro` fails to spawn and the fabro source silently degrades to
/// not-observed. Resolving to the absolute home-relative install path makes it
/// reachable under the wrapper. These are expanded against the injected home
/// directory ONLY (never the ambient filesystem), so resolution stays hermetic:
/// with no injected home, or no `fabro` under it, the bare `fabro` default is
/// kept and any other host provides its path via [`FABRO_PROGRAM_ENV`].
const FABRO_HOME_RELATIVE_CANDIDATES: [&str; 2] = [".local/bin/fabro", ".fabro/bin/fabro"];

#[derive(Debug, Clone, PartialEq, Eq)]
/// Resolved backing CLI programs and argument shapes.
pub struct BackingCliPrograms {
    list_work_items: String,
    livespec: CommandShape,
    fabro: String,
    dispatcher: String,
    drive: String,
    needs_attention: String,
    reconcile_runs: String,
    github: String,
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
            reconcile_runs: "reconcile-runs".to_owned(),
            github: "gh".to_owned(),
        }
    }
}

impl BackingCliPrograms {
    #[must_use]
    /// Return the list-work-items program path.
    pub fn list_work_items(&self) -> &str {
        &self.list_work_items
    }

    #[must_use]
    /// Return the livespec command shape.
    pub const fn livespec(&self) -> &CommandShape {
        &self.livespec
    }

    #[must_use]
    /// Return the Fabro program path.
    pub fn fabro(&self) -> &str {
        &self.fabro
    }

    #[must_use]
    /// Return the Dispatcher drain program path.
    pub fn dispatcher(&self) -> &str {
        &self.dispatcher
    }

    #[must_use]
    /// Return the orchestrator drive program path.
    pub fn drive(&self) -> &str {
        &self.drive
    }

    #[must_use]
    /// Return the needs-attention program path.
    pub fn needs_attention(&self) -> &str {
        &self.needs_attention
    }

    #[must_use]
    /// Return the reconcile-runs program path.
    pub fn reconcile_runs(&self) -> &str {
        &self.reconcile_runs
    }

    #[must_use]
    /// Return the GitHub CLI program path (the `gh pr list` github source).
    pub fn github(&self) -> &str {
        &self.github
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A command program plus its default arguments.
pub struct CommandShape {
    program: String,
    args: Vec<String>,
}

impl CommandShape {
    fn new(program: &str, args: &[&str]) -> Self {
        Self {
            program: program.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        }
    }

    #[must_use]
    /// Return the command program.
    pub fn program(&self) -> &str {
        &self.program
    }

    #[must_use]
    /// Return the command arguments.
    pub fn args(&self) -> &[String] {
        &self.args
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Resolved backing CLI configuration for one console run.
pub struct BackingCliResolution {
    selected_repo_path: PathBuf,
    programs: BackingCliPrograms,
    plugin_resolution: PluginResolution,
}

impl BackingCliResolution {
    /// Resolve backing CLIs from process environment and filesystem state.
    ///
    /// # Errors
    /// Returns an error when an explicitly selected plugin root or discovered
    /// plugin cache entry is malformed.
    pub fn from_environment() -> Result<Self, BackingCliResolutionError> {
        let env = std::env::vars().collect::<BTreeMap<_, _>>();
        let current_dir = std::env::current_dir().unwrap_or_default();
        let home_dir = std::env::var("HOME").ok().map(PathBuf::from);
        Self::resolve(&ResolveInputs {
            env,
            current_dir,
            home_dir,
        })
    }

    /// Resolve backing CLIs from injectable inputs.
    ///
    /// # Errors
    /// Returns an error when an explicitly selected plugin root or discovered
    /// plugin cache entry is malformed.
    pub fn resolve(inputs: &ResolveInputs) -> Result<Self, BackingCliResolutionError> {
        let selected_repo_path = selected_repo_path(inputs);
        let plugin_resolution = resolve_plugin_root(inputs, &selected_repo_path)?;
        let mut programs = plugin_resolution
            .root()
            .and_then(plugin_bin_dir)
            .as_deref()
            .map(programs_from_plugin_bin)
            .unwrap_or_default();
        // Resolve the bare `fabro` default to an absolute install path so it
        // spawns under the credential wrapper's scrubbed PATH. An explicit
        // `LIVESPEC_CONSOLE_FABRO_PROGRAM` override (applied next) still wins.
        if let Some(resolved) = resolve_fabro_program(inputs.home_dir.as_deref()) {
            programs.fabro = resolved;
        }
        apply_program_overrides(&inputs.env, &mut programs);
        Ok(Self {
            selected_repo_path,
            programs,
            plugin_resolution,
        })
    }

    #[must_use]
    /// Return the selected repo filesystem path used by repo-scoped backing CLIs.
    pub fn selected_repo_path(&self) -> &Path {
        &self.selected_repo_path
    }

    #[must_use]
    /// Return the `--repo` argument for the orchestrator drive/drain ports: the
    /// resolved repo filesystem PATH (NOT the repo id), so the orchestrator's
    /// path-expecting `--repo` handling resolves the selected repo checkout
    /// instead of erroring `--repo does not exist: <id>`.
    pub fn drive_repo_arg(&self) -> String {
        self.selected_repo_path.display().to_string()
    }

    #[must_use]
    /// Return resolved backing CLI programs.
    pub const fn programs(&self) -> &BackingCliPrograms {
        &self.programs
    }

    #[must_use]
    /// Return the resolved orchestrator plugin root/build shown to operators.
    pub const fn plugin_resolution(&self) -> &PluginResolution {
        &self.plugin_resolution
    }

    #[must_use]
    /// Return the ABSOLUTE Dispatcher journal path the dispatch source reads:
    /// the selected repo checkout joined with the repo-relative journal location
    /// ([`crate::DISPATCHER_JOURNAL_PATH`]). Resolving it against the selected
    /// repo (not the process working directory) keeps the source reading the
    /// right tenant's journal even when the console is launched from another
    /// directory.
    pub fn dispatcher_journal_path(&self) -> String {
        self.selected_repo_path
            .join(crate::DISPATCHER_JOURNAL_PATH)
            .display()
            .to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// The orchestrator plugin root/build selected for backing CLI programs.
pub struct PluginResolution {
    source: String,
    root: Option<PathBuf>,
    version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// The principal the console asserts for actions it records or forwards.
pub struct ConsoleInvokerResolution {
    principal: String,
    source: String,
}

impl ConsoleInvokerResolution {
    #[must_use]
    /// Build an invoker resolution.
    pub const fn new(principal: String, source: String) -> Self {
        Self { principal, source }
    }

    #[must_use]
    /// Return the console-asserted principal (`console:<principal>`).
    pub fn principal(&self) -> &str {
        &self.principal
    }

    #[must_use]
    /// Return where the principal came from: `flag`, `env`, or `fallback`.
    pub fn source(&self) -> &str {
        &self.source
    }
}

impl PluginResolution {
    #[must_use]
    /// Build a resolved plugin summary.
    pub const fn resolved(source: String, root: PathBuf, version: Option<String>) -> Self {
        Self {
            source,
            root: Some(root),
            version,
        }
    }

    #[must_use]
    /// Build an unresolved plugin summary.
    pub const fn unresolved(source: String) -> Self {
        Self {
            source,
            root: None,
            version: None,
        }
    }

    #[must_use]
    /// Return the resolution source label.
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    /// Return the resolved plugin root, if any.
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    #[must_use]
    /// Return the resolved plugin build/version, if known.
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Injectable resolver inputs used by unit tests.
pub struct ResolveInputs {
    /// Environment variables visible to the resolver.
    pub env: BTreeMap<String, String>,
    /// Current working directory for repo checkout discovery.
    pub current_dir: PathBuf,
    /// Home directory for the installed Claude plugin cache.
    pub home_dir: Option<PathBuf>,
}

#[must_use]
/// Resolve the console invoker using the orchestrator contract:
/// `--invoker`, then non-empty `LIVESPEC_INVOKER`, then an unattributed
/// `<os-user>@<hostname>` mark.
pub fn resolve_console_invoker(
    args: &[String],
    env: &BTreeMap<String, String>,
    os_user: &str,
    hostname: &str,
) -> ConsoleInvokerResolution {
    if let Some(value) = flag_value(args, "--invoker").filter(|value| !value.is_empty()) {
        return ConsoleInvokerResolution::new(format!("console:{value}"), "flag".to_owned());
    }
    if let Some(value) = env.get(INVOKER_ENV).filter(|value| !value.is_empty()) {
        return ConsoleInvokerResolution::new(format!("console:{value}"), "env".to_owned());
    }
    ConsoleInvokerResolution::new(
        format!("console:unattributed:{os_user}@{hostname}"),
        "fallback".to_owned(),
    )
}

fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].as_str())
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Resolver validation failure.
pub struct BackingCliResolutionError {
    message: String,
}

impl BackingCliResolutionError {
    pub(crate) const fn new(message: String) -> Self {
        Self { message }
    }
}

impl std::fmt::Display for BackingCliResolutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for BackingCliResolutionError {}

fn selected_repo_path(inputs: &ResolveInputs) -> PathBuf {
    inputs
        .env
        .get(SELECTED_REPO_PATH_ENV)
        .map_or_else(|| inputs.current_dir.clone(), PathBuf::from)
}

fn resolve_plugin_root(
    inputs: &ResolveInputs,
    selected_repo_path: &Path,
) -> Result<PluginResolution, BackingCliResolutionError> {
    if let Some(root) = inputs.env.get(PLUGIN_ROOT_ENV).map(PathBuf::from) {
        validate_plugin_root(&root)?;
        return Ok(PluginResolution::resolved(
            PLUGIN_ROOT_ENV.to_owned(),
            root,
            None,
        ));
    }

    if plugin_bin_dir(selected_repo_path).is_some() {
        validate_plugin_root(selected_repo_path)?;
        return Ok(PluginResolution::resolved(
            "selected repo checkout".to_owned(),
            selected_repo_path.to_path_buf(),
            None,
        ));
    }

    let Some((root, version)) = installed_plugin_root(inputs, selected_repo_path)? else {
        return Ok(PluginResolution::unresolved("not resolved".to_owned()));
    };
    validate_plugin_root(&root)?;
    Ok(PluginResolution::resolved(
        "installed Claude plugin cache".to_owned(),
        root,
        version,
    ))
}

fn installed_plugin_root(
    inputs: &ResolveInputs,
    selected_repo_path: &Path,
) -> Result<Option<(PathBuf, Option<String>)>, BackingCliResolutionError> {
    let Some(home_dir) = &inputs.home_dir else {
        return Ok(None);
    };
    let path = home_dir.join(".claude/plugins/installed_plugins.json");
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    let value: serde_json::Value = serde_json::from_str(&contents).map_err(|error| {
        BackingCliResolutionError::new(format!(
            "invalid Claude plugin cache {}: {error}",
            path.display()
        ))
    })?;
    let Some(plugins) = value.get("plugins").and_then(serde_json::Value::as_object) else {
        return Ok(None);
    };
    let selected_repo = selected_repo_path.to_string_lossy();
    let mut selected: Option<InstalledPluginCandidate> = None;
    for (name, installs) in plugins {
        if !name.starts_with(&format!("{ORCHESTRATOR_PLUGIN_NAME}@")) {
            continue;
        }
        let Some(entries) = installs.as_array() else {
            return Err(BackingCliResolutionError::new(format!(
                "Claude plugin cache entry {name} is not an array"
            )));
        };
        for (index, entry) in entries.iter().enumerate() {
            let applicability = installed_plugin_applicability(entry, &selected_repo);
            let candidate = InstalledPluginCandidate {
                name,
                index,
                applicability,
                entry,
            };
            if selected
                .as_ref()
                .is_none_or(|current| candidate.is_newer_than(current))
            {
                selected = Some(candidate);
            }
        }
    }
    let Some(candidate) = selected else {
        return Ok(None);
    };
    let Some(install_path) = candidate
        .entry
        .get("installPath")
        .and_then(serde_json::Value::as_str)
    else {
        return Err(BackingCliResolutionError::new(format!(
            "Claude plugin cache entry {}[{}] has no installPath",
            candidate.name, candidate.index
        )));
    };
    let version = candidate
        .entry
        .get("version")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    Ok(Some((PathBuf::from(install_path), version)))
}

struct InstalledPluginCandidate<'a> {
    name: &'a str,
    index: usize,
    applicability: InstalledPluginApplicability,
    entry: &'a serde_json::Value,
}

impl InstalledPluginCandidate<'_> {
    fn is_newer_than(&self, other: &Self) -> bool {
        (self.applicability, self.index) > (other.applicability, other.index)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum InstalledPluginApplicability {
    AnyProject,
    Global,
    SelectedProject,
}

fn installed_plugin_applicability(
    entry: &serde_json::Value,
    selected_repo: &str,
) -> InstalledPluginApplicability {
    match entry.get("projectPath").and_then(serde_json::Value::as_str) {
        Some(path) if path == selected_repo => InstalledPluginApplicability::SelectedProject,
        None => InstalledPluginApplicability::Global,
        Some(_other) => InstalledPluginApplicability::AnyProject,
    }
}

/// Return the backing-CLI `bin` directory under a plugin root, accepting BOTH
/// the SOURCE layout (`<root>/.claude-plugin/scripts/bin`, what a governed spec
/// checkout carries) and the FLATTENED installed-marketplace-cache layout
/// (`<root>/scripts/bin`). The Claude plugin installer collapses
/// `.claude-plugin/scripts/` to `scripts/`, so a resolver that accepts only the
/// source layout rejects every real installed cache. Source is tried first.
fn plugin_bin_dir(root: &Path) -> Option<PathBuf> {
    let source = root.join(".claude-plugin/scripts/bin");
    if source.is_dir() {
        return Some(source);
    }
    let flattened = root.join("scripts/bin");
    if flattened.is_dir() {
        return Some(flattened);
    }
    None
}

fn validate_plugin_root(root: &Path) -> Result<(), BackingCliResolutionError> {
    let Some(scripts) = plugin_bin_dir(root) else {
        return Err(BackingCliResolutionError::new(format!(
            "orchestrator plugin root {} is missing a scripts/bin directory \
             (neither .claude-plugin/scripts/bin nor scripts/bin)",
            root.display()
        )));
    };
    let expected = [
        "needs_attention.py",
        "list_work_items.py",
        "drive.py",
        "dispatcher.py",
        "next.py",
    ];
    for script in expected {
        let path = scripts.join(script);
        if !path.is_file() {
            return Err(BackingCliResolutionError::new(format!(
                "orchestrator plugin root {} is missing {}",
                root.display(),
                path.display()
            )));
        }
    }
    Ok(())
}

fn programs_from_plugin_bin(bin: &Path) -> BackingCliPrograms {
    BackingCliPrograms {
        list_work_items: bin.join("list_work_items.py").display().to_string(),
        // The livespec source observes the SPEC-side `livespec next` action
        // (revise / critique / none), NOT the orchestrator's impl-side
        // `next.py` (which ranks work-items). Resolving it from the orchestrator
        // plugin bin wired it to the wrong CLI, whose work-item-ranking output
        // never parses as a spec-next action and so degraded the source. Keep the
        // spec-side `livespec next --json` command; the
        // `LIVESPEC_CONSOLE_LIVESPEC_PROGRAM` override points it at a concrete
        // spec-side next CLI (for example core's `next.py`) where one is present.
        livespec: CommandShape::new("livespec", &["next", "--json"]),
        fabro: "fabro".to_owned(),
        dispatcher: bin.join("dispatcher.py").display().to_string(),
        drive: bin.join("drive.py").display().to_string(),
        needs_attention: bin.join("needs_attention.py").display().to_string(),
        reconcile_runs: bin.join("reconcile_runs.py").display().to_string(),
        github: "gh".to_owned(),
    }
}

/// Normalize a resolved backing-CLI invocation so a `.py` script is run through
/// the Python interpreter rather than exec'd directly.
///
/// Several backing CLIs resolve to `.py` script paths in the installed
/// marketplace cache (for example `…/scripts/bin/needs_attention.py`). The
/// Claude plugin installer does NOT uniformly mark those scripts executable —
/// on a real host `needs_attention.py` and `drive.py` ship non-executable while
/// their siblings ship `+x` — so exec-ing the path directly fails with
/// "Permission denied" and the source silently degrades to unavailable
/// (cockpit attention reads 0; the `drive` valves fail). Invoking through
/// `python3` makes the script's exec bit irrelevant, matching the documented
/// plugin convention (invoke plugin scripts as `python3 "<path>"`, never rely
/// on the exec bit).
///
/// When `program` ends in `.py`, this returns `("python3", [program, …args])`.
/// A non-`.py` program — a bare-name default (`needs-attention`), or an
/// environment override pointing at another command — is returned unchanged so
/// overrides and non-Python programs keep working. `python3` is resolved from
/// PATH (the cockpit runs under the credential wrapper where it is present);
/// no absolute interpreter path is hard-coded.
#[must_use]
pub fn python_normalized_invocation<'a>(
    program: &'a str,
    args: &[&'a str],
) -> (&'a str, Vec<&'a str>) {
    let is_python_script = Path::new(program)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("py"));
    if is_python_script {
        let mut invocation = Vec::with_capacity(args.len() + 1);
        invocation.push(program);
        invocation.extend_from_slice(args);
        return ("python3", invocation);
    }
    (program, args.to_vec())
}

/// Resolve `fabro` to the first existing home-relative install path, or `None`
/// to keep the bare `fabro` default. Probes ONLY paths under `home_dir` (never
/// the ambient filesystem), so a caller that injects no home — or a home with no
/// `fabro` — deterministically keeps the bare default.
fn resolve_fabro_program(home_dir: Option<&Path>) -> Option<String> {
    let home = home_dir?;
    for candidate in FABRO_HOME_RELATIVE_CANDIDATES {
        let path = home.join(candidate);
        if path.is_file() {
            return Some(path.display().to_string());
        }
    }
    None
}

fn apply_program_overrides(env: &BTreeMap<String, String>, programs: &mut BackingCliPrograms) {
    if let Some(value) = env.get(LIST_WORK_ITEMS_PROGRAM_ENV) {
        programs.list_work_items.clone_from(value);
    }
    if let Some(value) = env.get(LIVESPEC_PROGRAM_ENV) {
        programs.livespec = CommandShape::new(value, &["next", "--json"]);
    }
    if let Some(value) = env.get(FABRO_PROGRAM_ENV) {
        programs.fabro.clone_from(value);
    }
    if let Some(value) = env.get(DRAIN_PROGRAM_ENV) {
        programs.dispatcher.clone_from(value);
    }
    if let Some(value) = env.get(DRIVE_PROGRAM_ENV) {
        programs.drive.clone_from(value);
    }
    if let Some(value) = env.get(NEEDS_ATTENTION_PROGRAM_ENV) {
        programs.needs_attention.clone_from(value);
    }
    if let Some(value) = env.get(RECONCILE_RUNS_PROGRAM_ENV) {
        programs.reconcile_runs.clone_from(value);
    }
    if let Some(value) = env.get(GH_PROGRAM_ENV) {
        programs.github.clone_from(value);
    }
}
