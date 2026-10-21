//! Read the host-rendered Fabro factory registry.
//!
//! A committed `dispatcher.factories` entry is REGISTRY-BACKED: it declares
//! `"registry": true` and carries no `server` and no `docker_host`, so the
//! committed configuration names no machine (coordinator rulings R318/R320;
//! `SPECIFICATION/constraints.md` §"Factory routing"). Which Fabro server
//! answers for a factory NAME is bound only in the fleet gitops inventory's
//! factory registry, which each dispatching host renders at
//! [`FACTORY_REGISTRY_PATH`] as a JSON object mapping each factory name to
//! `{server, optional docker_host}` (livespec-dev-tooling
//! `non-functional-requirements.md` §"Fabro factory registry").
//!
//! Reading that published file CONSUMES the dev-tooling contract; it does not
//! re-implement an orchestrator primitive. The orchestrator publishes no CLI
//! surface returning resolved factory addresses -- its resolution is internal
//! to `commands/_factory_registry.py` -- so there is no upstream surface to
//! consume instead. If one is published later, this module is what gets
//! repointed at it.
//!
//! This module is READ-ONLY over the registry; the console never renders or
//! writes it.

use std::collections::BTreeMap;
use std::path::Path;

/// Where each dispatching host renders the fleet factory registry.
pub const FACTORY_REGISTRY_PATH: &str = "/etc/livespec/fabro-factories.json";

/// The host's factory registry: each factory NAME mapped to the Fabro server
/// that answers for it.
///
/// The registry's `docker_host` field addresses the host's container runtime,
/// which the console never speaks to -- only `server` is kept.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FactoryRegistry {
    servers: BTreeMap<String, String>,
}

impl FactoryRegistry {
    #[must_use]
    /// Read the registry the host rendered at `path`.
    ///
    /// An absent or unreadable file yields an EMPTY registry rather than an
    /// error: a console host that dispatches nothing renders no registry, and
    /// the console must still start and poll every other source there. What
    /// the console must never do is treat an unresolved factory as absent --
    /// that is what sends the Fabro source back to the CLI's localhost
    /// default -- and that is decided by the caller, not here.
    pub fn read_at(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .map(|document| Self::parse(&document))
            .unwrap_or_default()
    }

    #[must_use]
    /// Parse a rendered registry document.
    ///
    /// A document that is not a JSON object, and an entry carrying no
    /// non-empty `server` string, each yield no mapping -- a name the registry
    /// cannot address is a name it does not map.
    pub fn parse(document: &str) -> Self {
        let Ok(serde_json::Value::Object(entries)) =
            serde_json::from_str::<serde_json::Value>(document)
        else {
            return Self::default();
        };
        Self {
            servers: entries
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
        }
    }

    #[must_use]
    /// Return the Fabro server this registry maps `name` to, if any.
    pub fn server(&self, name: &str) -> Option<&str> {
        self.servers.get(name).map(String::as_str)
    }
}
