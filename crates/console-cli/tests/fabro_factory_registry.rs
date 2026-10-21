//! `livespec-console-beads-fabro-x6o6pl`: a REGISTRY-BACKED factory entry
//! resolves its server from the host factory registry, not from the committed
//! config and not from the fabro CLI's localhost default.
//!
//! Coordinator rulings R318/R320 (spec v054 `## Factory routing`, Scenario 34)
//! require every committed `dispatcher.factories` entry in `.livespec.jsonc` to
//! declare `"registry": true` and name NO machine. Which Fabro server answers
//! to a factory name is bound only in the fleet gitops inventory's factory
//! registry, which each dispatching host renders as
//! `/etc/livespec/fabro-factories.json` (livespec-dev-tooling
//! `non-functional-requirements.md` §"Fabro factory registry": a JSON object
//! mapping each factory name to `{server, optional docker_host}`).
//!
//! Before this fix the console kept ONLY entries carrying a non-empty committed
//! `server`, so the moment the committed table went registry-backed the console
//! would resolve ZERO factories, fall back to the single unqualified `fabro ps
//! --json` on `http://127.0.0.1:32276`, and go structurally blind to every
//! factory run -- the exact cockpit blindness
//! `livespec-console-beads-fabro-z4y7kp` already fixed once.
//!
//! These tests drive the PRODUCTION path end to end: a real
//! `BackingCliResolution` over a real `.livespec.jsonc` and a real rendered
//! registry file on disk, polled through the real ingestion and read back off
//! the real event log. A test over the config parser alone would pass with the
//! defect present -- the defect lives in the seam between the resolved config
//! and the constructed command.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use console_application::source_adapters::{
    NeedsAttentionReadOutcome, NeedsAttentionSnapshotPort, PullSourcePort, SourceProbe,
    SourceProbeOutcome,
};
use console_application::writer_identity::WriterIdentity;
use console_eventstore::SqliteEventStore;
use livespec_console_beads_fabro::{
    BackingCliResolution, NeedsAttentionIngest, ResolveInputs, live_source_adapters_with_programs,
    refresh_sources,
};

type TestResult = Result<(), String>;

const REPO: &str = "livespec-console-beads-fabro";
/// The server the RENDERED REGISTRY maps `legacy` to. It appears nowhere in the
/// committed config, so a poll that reaches it can only have resolved by name.
const LEGACY_SERVER: &str = "https://legacy-host.example:32276";
/// The env override naming where the host rendered its factory registry. The
/// production default is the absolute `/etc/livespec/fabro-factories.json`,
/// which a test must never write.
const REGISTRY_PATH_ENV: &str = "LIVESPEC_CONSOLE_FABRO_FACTORY_REGISTRY";

/// A committed `.livespec.jsonc` in the shape ruling R318/R320 requires: the
/// factory entry declares `"registry": true` and carries NO `server` and NO
/// `docker_host`, so the committed table names no machine.
const REGISTRY_BACKED_CONFIG: &str = r#"{
  // The console reads this file; it never writes it.
  "implementation": { "plugin": "livespec-orchestrator-beads-fabro" },
  "livespec-orchestrator-beads-fabro": {
    "dispatcher": {
      // Registry-backed: no server, no docker_host, no machine named here.
      "factories": {
        "legacy": { "registry": true }
      },
      "default_factory": "legacy"
    }
  }
}
"#;

/// The host-rendered registry, in the dev-tooling contract's shape: a JSON
/// object mapping each factory NAME to `{server, optional docker_host}`.
const RENDERED_REGISTRY: &str = r#"{
  "legacy": {
    "server": "https://legacy-host.example:32276",
    "docker_host": "ssh://legacy-host.example"
  },
  "spare": { "server": "https://spare-host.example:32276" }
}
"#;

/// A scratch checkout under the crate's own `target/`, unique per test case,
/// carrying the committed config and -- when `registry` is `Some` -- a rendered
/// registry file beside it.
fn temp_case(label: &str, registry: Option<&str>) -> Result<(PathBuf, PathBuf), String> {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp-factory-registry");
    let case = base.join(format!("{label}-{}", std::process::id()));
    let _ignored = std::fs::remove_dir_all(&case);
    let repo_path = case.join("repo");
    std::fs::create_dir_all(&repo_path)
        .map_err(|error| format!("create scratch dir {}: {error}", repo_path.display()))?;
    std::fs::write(repo_path.join(".livespec.jsonc"), REGISTRY_BACKED_CONFIG)
        .map_err(|error| format!("write .livespec.jsonc: {error}"))?;
    let registry_path = case.join("fabro-factories.json");
    if let Some(contents) = registry {
        std::fs::write(&registry_path, contents)
            .map_err(|error| format!("write the rendered registry: {error}"))?;
    }
    Ok((repo_path, registry_path))
}

fn resolution_for(repo_path: &Path, registry_path: &Path) -> Result<BackingCliResolution, String> {
    let mut env = BTreeMap::new();
    env.insert(
        "LIVESPEC_CONSOLE_REPO_PATH".to_owned(),
        repo_path.display().to_string(),
    );
    env.insert(
        REGISTRY_PATH_ENV.to_owned(),
        registry_path.display().to_string(),
    );
    BackingCliResolution::resolve(&ResolveInputs {
        env,
        current_dir: repo_path.to_path_buf(),
        home_dir: None,
    })
    .map_err(|error| format!("resolve backing CLIs: {error}"))
}

/// One observed fabro run, in the envelope shape `fabro ps --json` reports.
fn fabro_run_stdout(run_id: &str, work_item_id: &str) -> String {
    format!(
        r#"[{{"run_id":"{run_id}","work_item_id":"{work_item_id}","status":{{"kind":"running"}}}}]"#
    )
}

/// Records every command argv the console runs, and answers a fabro poll only
/// at the registry-mapped server. Every other source reads unavailable: these
/// tests are about the fabro source alone.
struct RegistryProbe {
    commands: RefCell<Vec<(String, Vec<String>)>>,
    answers: BTreeMap<String, SourceProbeOutcome>,
}

impl RegistryProbe {
    const fn new(answers: BTreeMap<String, SourceProbeOutcome>) -> Self {
        Self {
            commands: RefCell::new(Vec::new()),
            answers,
        }
    }

    /// The argv of every command whose program was the fabro binary.
    fn fabro_commands(&self) -> Vec<Vec<String>> {
        self.commands
            .borrow()
            .iter()
            .filter(|(program, _args)| program.ends_with("fabro"))
            .map(|(_program, args)| args.clone())
            .collect()
    }
}

impl SourceProbe for RegistryProbe {
    fn run_command(&self, program: &str, args: &[&str]) -> SourceProbeOutcome {
        let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
        self.commands
            .borrow_mut()
            .push((program.to_owned(), args.clone()));
        args.windows(2)
            .find(|pair| pair[0] == "--server")
            .and_then(|pair| self.answers.get(&pair[1]))
            .cloned()
            .unwrap_or_else(|| SourceProbeOutcome::unavailable("test probe: not a factory poll"))
    }

    fn read_file(&self, _path: &str) -> SourceProbeOutcome {
        SourceProbeOutcome::unavailable("test probe: no file sources")
    }
}

struct EmptyNeedsAttentionPort;

impl NeedsAttentionSnapshotPort for EmptyNeedsAttentionPort {
    fn read_snapshot(&self) -> NeedsAttentionReadOutcome {
        NeedsAttentionReadOutcome::Observed(Vec::new())
    }
}

fn test_writer_identity() -> WriterIdentity {
    WriterIdentity::new(
        4242,
        "/opt/console/test-binary",
        "/data/projects/repo",
        "test1234",
    )
}

/// Poll every live source once through the real ingestion path, returning the
/// adapter ids that were built and the store the poll wrote into.
fn poll_once(
    probe: &RegistryProbe,
    resolution: &BackingCliResolution,
) -> Result<(Vec<String>, SqliteEventStore), String> {
    let adapters = live_source_adapters_with_programs(
        probe,
        REPO,
        resolution.programs(),
        &resolution.dispatcher_journal_path(),
    )
    .map_err(|error| format!("build the live source adapters: {error:?}"))?;
    let adapter_ids: Vec<String> = adapters
        .iter()
        .map(|(adapter_id, _adapter)| adapter_id.clone())
        .collect();
    let sources: Vec<(&str, &dyn PullSourcePort)> = adapters
        .iter()
        .map(|(adapter_id, adapter)| (adapter_id.as_str(), adapter as &dyn PullSourcePort))
        .collect();
    let port = EmptyNeedsAttentionPort;
    let needs_attention = NeedsAttentionIngest::new(&port, REPO);
    let mut store =
        SqliteEventStore::open_in_memory().map_err(|error| format!("open store: {error:?}"))?;
    refresh_sources(
        &mut store,
        "2026-10-21T12:00:00Z",
        &sources,
        &needs_attention,
        &test_writer_identity(),
    )
    .map_err(|error| format!("refresh every live source: {error:?}"))?;
    Ok((adapter_ids, store))
}

fn observed_run_sources(store: &SqliteEventStore) -> Result<Vec<String>, String> {
    Ok(store
        .list_console_events()
        .map_err(|error| format!("read the event log: {error:?}"))?
        .iter()
        .filter(|event| event.event_type().contract_name() == "fabro.run_observed")
        .map(|event| event.source().to_owned())
        .collect())
}

/// A committed entry that declares `"registry": true` and names no machine is
/// polled at the server the host-rendered registry maps its NAME to.
#[test]
fn a_registry_backed_factory_is_polled_at_the_server_the_registry_maps_its_name_to() -> TestResult {
    let (repo_path, registry_path) = temp_case("resolves-by-name", Some(RENDERED_REGISTRY))?;
    let resolution = resolution_for(&repo_path, &registry_path)?;
    let answers = BTreeMap::from([(
        LEGACY_SERVER.to_owned(),
        SourceProbeOutcome::observed(
            &fabro_run_stdout("01M5LEGACYRUNAAAAAAAAAAAAA", "wi-legacy"),
            true,
        ),
    )]);
    let probe = RegistryProbe::new(answers);

    let (adapter_ids, store) = poll_once(&probe, &resolution)?;

    // The registry-backed factory still gets its own named source instance.
    assert!(
        adapter_ids.contains(&format!("fabro:legacy:{REPO}")),
        "a registry-backed factory needs its own source adapter, named for the \
         factory; got {adapter_ids:?}"
    );
    assert!(
        !adapter_ids.contains(&format!("fabro:{REPO}")),
        "resolving the registry-backed factory must not leave the unqualified \
         localhost-default fabro source in play; got {adapter_ids:?}"
    );

    // THE DEFECT: the constructed command must carry the server the REGISTRY
    // maps `legacy` to. That address appears nowhere in the committed config,
    // so reaching it proves resolution happened by name.
    assert_eq!(
        probe.fabro_commands(),
        vec![vec![
            "ps".to_owned(),
            "--json".to_owned(),
            "--server".to_owned(),
            LEGACY_SERVER.to_owned(),
        ]],
        "the registry-backed factory must be polled exactly once, at the \
         server the rendered registry maps its name to"
    );

    // The run lands, attributed to the factory NAME the registry resolved.
    assert_eq!(observed_run_sources(&store)?, ["fabro:legacy".to_owned()]);

    Ok(())
}
