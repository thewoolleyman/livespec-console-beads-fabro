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
use std::path::{Path, PathBuf};

/// Where each dispatching host renders the fleet factory registry.
pub const HOST_FACTORY_REGISTRY_PATH: &str = "/etc/livespec/fabro-factories.json";

/// The factory-name → server map the host rendered, as the console read it.
///
/// The path it was read FROM is kept so an unresolvable factory can be reported
/// against the file that failed to bind it -- an operator who sees
/// `fabro:legacy` go dark needs to know WHICH registry did not name it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FactoryRegistry {
    path: PathBuf,
    servers: BTreeMap<String, String>,
}

impl FactoryRegistry {
    #[must_use]
    /// Read the registry rendered at `path`.
    ///
    /// An absent, unreadable, or uninterpretable file yields a registry that
    /// binds NOTHING rather than an error: the console must still start and
    /// poll every other source on a host that renders no registry at all. What
    /// it must NOT do is quietly substitute an address for a name the registry
    /// does not bind -- [`Self::resolve`] refuses such a name, and
    /// `crate::factory_servers` turns that refusal into an unresolvable factory
    /// rather than a poll at the fabro CLI's localhost default.
    pub fn read_at(path: &Path) -> Self {
        let servers = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .as_ref()
            .and_then(rendered_servers)
            .unwrap_or_default();
        Self {
            path: path.to_path_buf(),
            servers,
        }
    }

    /// Return the server the registry binds to `name`.
    ///
    /// # Errors
    /// Returns the honest reason the name is unresolvable -- naming both the
    /// factory and the registry file that did not bind it -- when the registry
    /// renders no usable server for it.
    pub fn resolve(&self, name: &str) -> Result<&str, String> {
        self.servers.get(name).map(String::as_str).ok_or_else(|| {
            format!(
                "factory \"{name}\" is registry-backed and the host factory registry at {} \
                 binds no server for that name",
                self.path.display()
            )
        })
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
