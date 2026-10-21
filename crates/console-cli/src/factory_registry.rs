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
///
/// A registry that could not be read is a DISTINCT state from one that was
/// read and maps nothing, and both are distinct from absent: the console must
/// be able to tell an operator WHY a factory has no address, so `origin` and
/// `unreadable` are carried here rather than collapsed into an empty map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactoryRegistry {
    origin: String,
    servers: BTreeMap<String, String>,
    unreadable: Option<String>,
}

impl FactoryRegistry {
    #[must_use]
    /// Read the registry the host rendered at `path`.
    ///
    /// An absent or unreadable file is not an error -- a console host that
    /// dispatches nothing renders no registry, and the console must still
    /// start and poll every other source there. It is recorded, so a factory
    /// that goes unresolved can say so in the operator's own terms rather than
    /// vanishing from the table.
    pub fn read_at(path: &Path) -> Self {
        let origin = path.display().to_string();
        match std::fs::read_to_string(path) {
            Ok(document) => Self::parse(&origin, &document),
            Err(error) => Self {
                origin,
                servers: BTreeMap::new(),
                unreadable: Some(error.to_string()),
            },
        }
    }

    #[must_use]
    /// Parse a registry document rendered at `origin`.
    ///
    /// A document that is not a JSON object is UNREADABLE, not empty: the host
    /// rendered something the published contract does not describe, which an
    /// operator needs told. An entry carrying no non-empty `server` string
    /// simply yields no mapping -- a name the registry cannot address is a
    /// name it does not map.
    pub fn parse(origin: &str, document: &str) -> Self {
        let Ok(serde_json::Value::Object(entries)) =
            serde_json::from_str::<serde_json::Value>(document)
        else {
            return Self {
                origin: origin.to_owned(),
                servers: BTreeMap::new(),
                unreadable: Some(
                    "it is not a JSON object mapping factory names to {server, docker_host}"
                        .to_owned(),
                ),
            };
        };
        Self {
            origin: origin.to_owned(),
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
            unreadable: None,
        }
    }

    /// Resolve a registry-backed factory NAME to the Fabro server that answers
    /// for it.
    ///
    /// # Errors
    /// Returns the reason the name has no address -- naming the factory and
    /// the registry that failed to answer for it -- so the console can report
    /// the factory as unresolvable instead of dropping it.
    pub fn resolve(&self, name: &str) -> Result<&str, String> {
        if let Some(detail) = &self.unreadable {
            return Err(format!(
                "factory `{name}` names no machine, and the host factory registry {} \
                 cannot be read: {detail}",
                self.origin
            ));
        }
        self.servers.get(name).map(String::as_str).ok_or_else(|| {
            format!(
                "factory `{name}` names no machine, and the host factory registry {} \
                 maps no server to that name",
                self.origin
            )
        })
    }
}
