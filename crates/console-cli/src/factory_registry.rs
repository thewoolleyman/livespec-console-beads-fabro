//! Read the host's rendered Fabro factory registry.
//!
//! A committed `dispatcher.factories` entry that declares `"registry": true`
//! names NO machine (spec `constraints.md` §"Factory routing", Scenario 34,
//! coordinator rulings R318/R320). Which Fabro server answers to a factory
//! NAME is bound only in the fleet gitops inventory's factory registry, which
//! every dispatching host renders to [`HOST_FACTORY_REGISTRY_PATH`] --
//! livespec-dev-tooling `non-functional-requirements.md` §"Fabro factory
//! registry": a JSON object mapping each factory name to
//! `{server, optional docker_host}`.
//!
//! Reading that published file is CONSUMING the dev-tooling contract, not
//! re-implementing an orchestrator primitive: the orchestrator publishes no CLI
//! surface returning resolved factory addresses (the resolution is internal to
//! its `commands/_factory_registry.py`). If such a surface is ever published,
//! this module is what gets repointed at it.
//!
//! READ-ONLY, like every other console-side host read: the console never writes
//! the registry, and never writes `.livespec.jsonc`.

use std::collections::BTreeMap;
use std::path::Path;

/// Where each dispatching host renders the fleet factory registry.
pub const HOST_FACTORY_REGISTRY_PATH: &str = "/etc/livespec/fabro-factories.json";

/// The factory-name → server map the host rendered, as the console read it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FactoryRegistry {
    servers: BTreeMap<String, String>,
}

impl FactoryRegistry {
    #[must_use]
    /// Read the registry rendered at `path`.
    ///
    /// An absent, unreadable, or uninterpretable file yields an EMPTY registry
    /// rather than an error: the console must still start and poll every other
    /// source on a host that renders no registry at all. What it must NOT do
    /// is quietly substitute an address for a name the registry does not bind:
    /// an unbound name resolves to nothing here, and
    /// `crate::factory_servers` decides what the console says about it.
    pub fn read_at(path: &Path) -> Self {
        let servers = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .as_ref()
            .and_then(rendered_servers)
            .unwrap_or_default();
        Self { servers }
    }

    #[must_use]
    /// Return the server the registry binds to `name`, if it binds one.
    pub fn server_for(&self, name: &str) -> Option<&str> {
        self.servers.get(name).map(String::as_str)
    }
}

/// Collect the `name -> server` bindings out of a rendered registry document.
///
/// A document that is not a JSON object binds nothing. An entry carrying no
/// usable `server` string is omitted rather than bound to a blank address: a
/// name the console cannot address is not one it can poll, and reporting it
/// unresolvable is the honest outcome.
fn rendered_servers(document: &serde_json::Value) -> Option<BTreeMap<String, String>> {
    Some(
        document
            .as_object()?
            .iter()
            .filter_map(|(name, entry)| {
                entry
                    .get("server")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|server| !server.is_empty())
                    .map(|server| (name.clone(), server.to_owned()))
            })
            .collect(),
    )
}
