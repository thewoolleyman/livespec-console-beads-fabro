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

#[derive(Debug, Clone, PartialEq, Eq)]
/// Resolved backing CLI programs and argument shapes.
pub struct BackingCliPrograms {
    list_work_items: String,
    livespec: CommandShape,
    fabro: String,
    dispatcher: String,
    drive: String,
    needs_attention: String,
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
        let plugin_root = resolve_plugin_root(inputs, &selected_repo_path)?;
        let mut programs = plugin_root
            .as_deref()
            .map(programs_from_plugin_root)
            .unwrap_or_default();
        apply_program_overrides(&inputs.env, &mut programs);
        Ok(Self {
            selected_repo_path,
            programs,
        })
    }

    #[must_use]
    /// Return the selected repo filesystem path used by repo-scoped backing CLIs.
    pub fn selected_repo_path(&self) -> &Path {
        &self.selected_repo_path
    }

    #[must_use]
    /// Return resolved backing CLI programs.
    pub const fn programs(&self) -> &BackingCliPrograms {
        &self.programs
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
) -> Result<Option<PathBuf>, BackingCliResolutionError> {
    if let Some(root) = inputs.env.get(PLUGIN_ROOT_ENV).map(PathBuf::from) {
        validate_plugin_root(&root)?;
        return Ok(Some(root));
    }

    if selected_repo_path
        .join(".claude-plugin/scripts/bin")
        .is_dir()
    {
        validate_plugin_root(selected_repo_path)?;
        return Ok(Some(selected_repo_path.to_path_buf()));
    }

    let Some(root) = installed_plugin_root(inputs)? else {
        return Ok(None);
    };
    validate_plugin_root(&root)?;
    Ok(Some(root))
}

fn installed_plugin_root(
    inputs: &ResolveInputs,
) -> Result<Option<PathBuf>, BackingCliResolutionError> {
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
    for (name, installs) in plugins {
        if !name.starts_with(&format!("{ORCHESTRATOR_PLUGIN_NAME}@")) {
            continue;
        }
        let Some(install_path) = installs
            .as_array()
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("installPath"))
            .and_then(serde_json::Value::as_str)
        else {
            return Err(BackingCliResolutionError::new(format!(
                "Claude plugin cache entry {name} has no installPath"
            )));
        };
        return Ok(Some(PathBuf::from(install_path)));
    }
    Ok(None)
}

fn validate_plugin_root(root: &Path) -> Result<(), BackingCliResolutionError> {
    let scripts = root.join(".claude-plugin/scripts/bin");
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

fn programs_from_plugin_root(root: &Path) -> BackingCliPrograms {
    let bin = root.join(".claude-plugin/scripts/bin");
    BackingCliPrograms {
        list_work_items: bin.join("list_work_items.py").display().to_string(),
        livespec: CommandShape {
            program: bin.join("next.py").display().to_string(),
            args: vec!["--json".to_owned()],
        },
        fabro: "fabro".to_owned(),
        dispatcher: bin.join("dispatcher.py").display().to_string(),
        drive: bin.join("drive.py").display().to_string(),
        needs_attention: bin.join("needs_attention.py").display().to_string(),
    }
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
}
