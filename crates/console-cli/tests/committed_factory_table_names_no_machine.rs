//! `livespec-console-beads-fabro-wzvd2s`: this repository's COMMITTED factory
//! table names no machine.
//!
//! Coordinator rulings R318/R320 keep assertion 6 of the orchestrator plan
//! `petri-factory-on-gmktec` whole: no committed factory entry names a machine
//! hostname, and a factory is bound to a machine only in the fleet gitops
//! inventory. This console ratified that as `SPECIFICATION/constraints.md`
//! §"Factory routing" (spec v054), carried by `SPECIFICATION/scenarios.md`
//! §"Scenario 34 -- A dispatch resolves its factory through the host's factory
//! registry", whose FIRST scenario is exactly what this file proves: the
//! committed `dispatcher.factories` table declares registry-backed entries
//! named `legacy` and `petri`, no entry carries a `server` or `docker_host`
//! value, and `dispatcher.default_factory` names `petri` (spec v056,
//! coordinator ruling R668, the Petri-era cutover).
//! `legacy` was the default through spec v055.
//!
//! Ground truth against the REAL committed `.livespec.jsonc`, not a fixture.
//! `console-cli`'s parser fixtures already cover both entry shapes, and
//! `crates/console-cli/tests/fabro_factory_registry.rs` already proves a
//! registry-backed entry is polled at the server the host registry maps its
//! name to -- but neither can say what this repository actually commits, which
//! is the whole of the clause.
//!
//! The reading is split deliberately. The STRUCTURAL half -- exactly one
//! entries, named `legacy` and `petri`, each registry-backed -- goes through the production
//! parser `parse_factory_servers`, so this test never re-implements the JSONC
//! reader that module exists to own. The BYTE-LEVEL half -- that the table
//! commits no `server` and no `docker_host` VALUE -- is read off the committed
//! text, because that is precisely what the clause claims: the parser
//! short-circuits a registry-backed entry and would IGNORE a committed server
//! rather than report it, so no parse result can witness its absence.
//!
//! Scenario 34's SECOND scenario -- a dispatch with no explicit factory landing
//! on the server the host-rendered `/etc/livespec/fabro-factories.json` maps
//! `petri` to -- is a host-captured proof no factory sandbox can take, and
//! that scenario's heading-coverage row stays pending with
//! `livespec-console-beads-fabro-y9hc`.

use std::path::{Path, PathBuf};

use livespec_console_beads_fabro::{
    FactoryAddress, FactoryRegistry, FactoryServer, parse_factory_servers,
};

type TestResult = Result<(), String>;

/// The orchestrator's per-repo configuration file, relative to the repo root.
const CONFIG_FILE: &str = ".livespec.jsonc";

/// The fork-era factory name that rulings R318/R320 left in the committed
/// table. It stays declared so recorded pins finish where they started.
const LEGACY: &str = "legacy";

/// The Petri-era factory name, and since spec v056 the committed default.
const PETRI: &str = "petri";

/// This repository's root, two levels up from the `console-cli` manifest.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The committed configuration text, read verbatim with its comments intact.
fn committed_config() -> Result<String, String> {
    let path = repo_root().join(CONFIG_FILE);
    std::fs::read_to_string(&path).map_err(|error| format!("read {}: {error}", path.display()))
}

/// The text of the committed `dispatcher.factories` object, brace-matched out
/// of the document so the byte-level assertions are scoped to the factory
/// table and cannot be satisfied -- or broken -- by an unrelated setting
/// elsewhere in the file that happens to carry a key of the same name.
fn committed_factories_declaration(committed: &str) -> Result<&str, String> {
    let key = committed
        .find("\"factories\"")
        .ok_or_else(|| "the committed config declares no dispatcher.factories table".to_owned())?;
    let open = committed
        .get(key..)
        .and_then(|rest| rest.find('{'))
        .map(|offset| key.saturating_add(offset))
        .ok_or_else(|| "the committed factories key opens no object".to_owned())?;
    let rest = committed
        .get(open..)
        .ok_or_else(|| "the committed factories object starts past the document".to_owned())?;
    let mut depth = 0_usize;
    for (offset, character) in rest.char_indices() {
        if character == '{' {
            depth = depth.saturating_add(1);
        } else if character == '}' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return committed
                    .get(open..=open.saturating_add(offset))
                    .ok_or_else(|| {
                        "the committed factories object ends past the document".to_owned()
                    });
            }
        }
    }
    Err("the committed factories object is never closed".to_owned())
}

/// The committed `dispatcher.factories` table declares exactly the entries
/// `legacy` and `petri`, each registry-backed, and names no machine.
#[test]
fn the_committed_factory_table_declares_registry_backed_legacy_and_petri() -> TestResult {
    let committed = committed_config()?;

    // A registry that binds NOTHING, on purpose. A registry-backed entry then
    // resolves Unresolvable BY NAME while an entry carrying a committed server
    // resolves to that server, so the two shapes stay distinguishable and this
    // reads the COMMITTED declaration rather than whatever registry the host
    // running the suite happens to render.
    let factories = parse_factory_servers(&committed, &FactoryRegistry::default());

    let names: Vec<&str> = factories.iter().map(FactoryServer::name).collect();
    assert_eq!(
        names,
        [LEGACY, PETRI],
        "the committed dispatcher.factories table must declare exactly the \
         entries {LEGACY} and {PETRI}"
    );
    for factory in &factories {
        let address = factory.address();
        assert!(
            matches!(address, FactoryAddress::Unresolvable(_reason)),
            "every committed entry must be registry-backed, so it resolves by \
             NAME through the host factory registry and not from a committed \
             address; {} got {address:?}",
            factory.name()
        );
    }

    let declaration = committed_factories_declaration(&committed)?;
    for machine_naming_key in ["\"server\"", "\"docker_host\""] {
        assert!(
            !declaration.contains(machine_naming_key),
            "the committed factory table must carry no {machine_naming_key} \
             value, so that no committed entry names a machine; got \
             {declaration}"
        );
    }
    Ok(())
}

/// The committed `dispatcher.default_factory` names `petri`, so a dispatch
/// with no explicit factory resolves through the host's factory registry.
#[test]
fn the_committed_default_factory_names_petri() -> TestResult {
    let committed = committed_config()?;

    let declared: Vec<String> = committed
        .lines()
        .filter_map(|line| line.trim().strip_prefix("\"default_factory\":"))
        .map(|value| value.trim().trim_end_matches(',').trim().to_owned())
        .collect();

    assert_eq!(
        declared,
        [format!("\"{PETRI}\"")],
        "the committed dispatcher.default_factory must name {PETRI}"
    );
    Ok(())
}
