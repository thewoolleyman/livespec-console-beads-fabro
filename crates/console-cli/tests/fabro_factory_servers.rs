//! `livespec-console-beads-fabro-z4y7kp`: the Fabro source MUST poll the factory
//! servers `.livespec.jsonc` declares, with an explicit `--server`.
//!
//! MEASURED 2026-09-12 on the real store: the Fabro source was built as
//! `fabro ps --json` with no `--server`, so the fabro CLI fell back to its
//! `http://127.0.0.1:32276` default. Nothing listens there on a console host --
//! the factories this repo dispatches to are REMOTE (`dispatcher.factories`:
//! `hp` and `vps`, both on the tailnet) -- so every poll failed with
//! `Connection refused (os error 111)`, and had done continuously since
//! 2026-09-10. The console's run-level view of the factory was not merely stale,
//! it was structurally blind: `fabro ps --server https://hp-...:32276` listed
//! three live runs at the same moment the console's own source saw nothing at
//! all, and every run-derived signal (the active lane's executing/claimed split,
//! orphaned-run detection, run staleness) rested on a source that could never
//! return data.
//!
//! Both tests here drive the PRODUCTION path -- the argv
//! `live_source_adapters_with_programs` actually builds from a real
//! `BackingCliResolution` over a real `.livespec.jsonc` on disk, polled through
//! the real ingestion and projected by the real header tally. A test that
//! asserted on a hand-built plan, or on the config parser alone, would pass with
//! the defect present: the defect lived in the seam between the resolved config
//! and the constructed command.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use console_application::build_tui_model;
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
const HP_SERVER: &str = "https://hp-xubuntu.perch-rudd.ts.net:32276";
const VPS_SERVER: &str = "https://vps.perch-rudd.ts.net:32276";

/// A `.livespec.jsonc` in the shape a governed repo actually carries: JSONC
/// comments, and the dispatcher settings nested under the implementation
/// plugin's own key. The comment beside the factories is deliberate -- a
/// comment stripper that is not string-aware truncates every `https://` server
/// to `https:` and resolves zero factories, which presents exactly like the
/// defect under test.
fn config_text() -> String {
    format!(
        r#"{{
  // The console reads this file; it never writes it.
  "implementation": {{ "plugin": "livespec-orchestrator-beads-fabro" }},
  "livespec-orchestrator-beads-fabro": {{
    "dispatcher": {{
      // Two REMOTE factories, on the tailnet -- neither is localhost.
      "factories": {{
        "hp": {{ "server": "{HP_SERVER}" }},
        "vps": {{ "server": "{VPS_SERVER}" }}
      }},
      "default_factory": "hp"
    }}
  }}
}}
"#
    )
}

/// A scratch checkout under the crate's own `target/`, unique per test process,
/// carrying nothing but the orchestrator config the resolver reads.
fn temp_repo_checkout(label: &str) -> Result<PathBuf, String> {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp-factory-servers");
    let path = base.join(format!("{label}-{}", std::process::id()));
    let _ignored = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path)
        .map_err(|error| format!("create scratch dir {}: {error}", path.display()))?;
    std::fs::write(path.join(".livespec.jsonc"), config_text())
        .map_err(|error| format!("write .livespec.jsonc: {error}"))?;
    Ok(path)
}

fn resolution_for(repo_path: &Path) -> Result<BackingCliResolution, String> {
    let mut env = BTreeMap::new();
    env.insert(
        "LIVESPEC_CONSOLE_REPO_PATH".to_owned(),
        repo_path.display().to_string(),
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

/// Records every command argv the console runs, and answers the fabro polls per
/// `--server` value. Every other source is reported unavailable: these tests are
/// about the fabro source alone.
struct FactoryProbe {
    commands: RefCell<Vec<(String, Vec<String>)>>,
    answers: BTreeMap<String, SourceProbeOutcome>,
}

impl FactoryProbe {
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

impl SourceProbe for FactoryProbe {
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
    probe: &FactoryProbe,
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
        "2026-09-12T12:00:00Z",
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

/// Every configured factory is polled, at an EXPLICIT `--server` derived from
/// the config, and the run each one reports is attributed to that factory by
/// name.
#[test]
fn the_fabro_source_polls_every_configured_factory_at_an_explicit_server() -> TestResult {
    let repo_path = temp_repo_checkout("polls-every-factory")?;
    let resolution = resolution_for(&repo_path)?;
    let answers = BTreeMap::from([
        (
            HP_SERVER.to_owned(),
            SourceProbeOutcome::observed(
                &fabro_run_stdout("01M2HPRUNAAAAAAAAAAAAAAAAA", "wi-hp"),
                true,
            ),
        ),
        (
            VPS_SERVER.to_owned(),
            SourceProbeOutcome::observed(
                &fabro_run_stdout("01M2VPSRUNBBBBBBBBBBBBBBBB", "wi-vps"),
                true,
            ),
        ),
    ]);
    let probe = FactoryProbe::new(answers);

    let (adapter_ids, store) = poll_once(&probe, &resolution)?;

    // One adapter per configured factory, each named for the factory it polls.
    assert!(
        adapter_ids.contains(&format!("fabro:hp:{REPO}"))
            && adapter_ids.contains(&format!("fabro:vps:{REPO}")),
        "every configured factory needs its own source adapter; got {adapter_ids:?}"
    );

    // THE DEFECT: the constructed command must carry an explicit `--server`.
    // Without it the fabro CLI silently polls its 127.0.0.1 default.
    let fabro_commands = probe.fabro_commands();
    assert_eq!(
        fabro_commands.len(),
        2,
        "each configured factory must be polled exactly once; got {fabro_commands:?}"
    );
    for argv in &fabro_commands {
        assert!(
            argv.iter().any(|arg| arg == "--server"),
            "a fabro poll without an explicit --server falls back to the CLI's \
             127.0.0.1 default, which no console host listens on; got {argv:?}"
        );
    }
    for server in [HP_SERVER, VPS_SERVER] {
        assert!(
            fabro_commands.contains(&vec![
                "ps".to_owned(),
                "--json".to_owned(),
                "--server".to_owned(),
                server.to_owned(),
            ]),
            "each factory must be polled at its CONFIGURED server ({server}); \
             got {fabro_commands:?}"
        );
    }

    // Each factory's run lands, attributed to the factory it came from.
    let run_sources = observed_run_sources(&store)?;
    assert!(
        run_sources.iter().any(|source| source == "fabro:hp")
            && run_sources.iter().any(|source| source == "fabro:vps"),
        "a run must name the factory it was observed at; got {run_sources:?}"
    );

    Ok(())
}

/// One unreachable factory must NOT brand the whole Fabro source unavailable
/// while another factory is answering, and the unreachable one is reported BY
/// NAME.
#[test]
fn an_unreachable_factory_is_named_without_branding_the_reachable_one() -> TestResult {
    let repo_path = temp_repo_checkout("one-factory-down")?;
    let resolution = resolution_for(&repo_path)?;
    // vps is absent from the answer table, so the probe reports it unavailable --
    // the `Connection refused` case, at one endpoint only.
    let answers = BTreeMap::from([(
        HP_SERVER.to_owned(),
        SourceProbeOutcome::observed(
            &fabro_run_stdout("01M2HPRUNCCCCCCCCCCCCCCCCC", "wi-hp"),
            true,
        ),
    )]);
    let probe = FactoryProbe::new(answers);

    let (_adapter_ids, store) = poll_once(&probe, &resolution)?;

    let events = store
        .list_console_events()
        .map_err(|error| format!("read the event log: {error:?}"))?;
    let unavailable = build_tui_model(&events, 0).unavailable_sources().to_vec();

    assert!(
        unavailable.iter().any(|source| source == "fabro:vps"),
        "the unreachable factory must be reported by name; got {unavailable:?}"
    );
    assert!(
        !unavailable.iter().any(|source| source == "fabro:hp"),
        "a reachable factory must not be branded unavailable; got {unavailable:?}"
    );
    assert!(
        !unavailable.iter().any(|source| source == "fabro"),
        "one factory being down must not brand the WHOLE fabro source \
         unavailable -- that is the tally row an operator reads as \
         cockpit-blind; got {unavailable:?}"
    );
    // The reachable factory's run still landed, so the blindness is genuinely
    // scoped to the endpoint that is down.
    assert_eq!(observed_run_sources(&store)?, ["fabro:hp".to_owned()]);

    Ok(())
}
