//! Resolve the factory servers the Fabro source polls.
//!
//! The factories this repo dispatches to are REMOTE and declared in the
//! orchestrator's `.livespec.jsonc` under `dispatcher.factories`, each with its
//! own `server` URL. `fabro`'s CLI defaults to `http://127.0.0.1:32276` when no
//! `--server` is passed, and nothing listens there on a console host -- so a
//! Fabro source built without an explicit `--server` polls a port that will
//! never answer and the console is structurally blind to every factory run
//! (measured 2026-09-12: `Connection refused (os error 111)` on every poll since
//! 2026-09-10, while `fabro ps --server https://hp-...:32276` listed three live
//! runs).
//!
//! This module reads those declarations so the source can poll the servers the
//! dispatcher actually uses. It is READ-ONLY over the config file; the console
//! never writes `.livespec.jsonc` (every setting change goes through the
//! orchestrator's published `drive` action surface).
//!
//! A committed entry no longer has to CARRY its server. The spec's
//! `constraints.md` (v054, the factory-routing clauses, with Scenario 34 in
//! `scenarios.md`) requires every committed entry to be
//! REGISTRY-BACKED -- `"registry": true`, no `server`, no `docker_host` -- so
//! the committed table names no machine; such an entry resolves BY NAME through
//! the host's rendered registry ([`crate::factory_registry`]). The
//! committed-server path stays live for an entry that still carries one.

use std::path::Path;

use crate::factory_registry::FactoryRegistry;

/// The orchestrator's per-repo configuration file, relative to the repo root.
const CONFIG_FILE: &str = ".livespec.jsonc";

/// The plugin key `dispatcher` is nested under when `.livespec.jsonc` does not
/// declare it at the top level.
///
/// A governed repo names its implementation plugin in `implementation.plugin`
/// and carries that plugin's settings under a key of the same name, so this is
/// only the fallback for a config that declares no plugin at all.
const DEFAULT_IMPLEMENTATION_PLUGIN: &str = "livespec-orchestrator-beads-fabro";

/// Where a declared factory resolved to, or why it did not.
///
/// An unresolvable factory is NOT dropped. Dropping it empties the factory
/// table, and an empty table puts the Fabro source back on the single
/// unqualified `fabro ps --json` at the CLI's `http://127.0.0.1:32276`
/// default -- a silent poll of an endpoint nobody configured, which reads at
/// the cockpit exactly like a healthy factory with no runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactoryAddress {
    /// The server the Fabro source polls this factory at, passed verbatim as
    /// `--server`.
    Server(String),
    /// The honest reason this factory has no address, reported against its
    /// own named source instance instead of being polled anywhere.
    Unresolvable(String),
}

/// One factory the Fabro source knows about, as `dispatcher.factories`
/// declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactoryServer {
    name: String,
    address: FactoryAddress,
}

impl FactoryServer {
    #[must_use]
    /// Build a factory addressed at `server`.
    pub fn new(name: &str, server: &str) -> Self {
        Self {
            name: name.to_owned(),
            address: FactoryAddress::Server(server.to_owned()),
        }
    }

    #[must_use]
    /// Build a factory that has no address, carrying the reason why.
    pub fn unresolvable(name: &str, reason: &str) -> Self {
        Self {
            name: name.to_owned(),
            address: FactoryAddress::Unresolvable(reason.to_owned()),
        }
    }

    #[must_use]
    /// Return the factory's configured NAME (`hp`, `vps`, `legacy`), which
    /// names the source instance the runs it reports are attributed to.
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    /// Return where this factory resolved to, or why it did not.
    pub const fn address(&self) -> &FactoryAddress {
        &self.address
    }
}

/// Read the factory servers declared by the selected repo checkout's
/// `.livespec.jsonc`.
///
/// An unreadable or uninterpretable config yields an EMPTY list rather than an
/// error: the console must still start and poll every other source on a checkout
/// that carries no orchestrator config at all (a sandbox, a test fixture, a
/// non-governed repo). The Fabro source then falls back to the CLI's own default
/// endpoint, which is correct for a host that genuinely runs a local fabro
/// server.
///
/// `registry` is the host's rendered factory registry, which binds the servers
/// of every registry-backed entry.
#[must_use]
pub fn factory_servers_at(repo_path: &Path, registry: &FactoryRegistry) -> Vec<FactoryServer> {
    std::fs::read_to_string(repo_path.join(CONFIG_FILE))
        .map(|config| parse_factory_servers(&config, registry))
        .unwrap_or_default()
}

/// Parse `dispatcher.factories` out of a `.livespec.jsonc` document.
///
/// The factories are looked for under the implementation plugin's key first
/// (`<implementation.plugin>.dispatcher.factories`, where a governed repo
/// actually declares them) and then at the top level, so a config written either
/// way resolves. Entries are returned in the config's key order (`serde_json`'s
/// object map is ordered by key).
///
/// An entry declaring `"registry": true` is REGISTRY-BACKED and resolves BY
/// NAME through `registry`; a name the registry does not bind yields an
/// [`FactoryAddress::Unresolvable`] factory, reported by name rather than
/// dropped. Any other entry resolves from its own committed `server`, and one
/// carrying no non-empty `server` string IS dropped -- it is not registry-backed
/// and names no machine, so there is nothing to report it against.
#[must_use]
pub fn parse_factory_servers(config: &str, registry: &FactoryRegistry) -> Vec<FactoryServer> {
    let Ok(document) = serde_json::from_str::<serde_json::Value>(&strip_jsonc_comments(config))
    else {
        return Vec::new();
    };
    let plugin = document
        .get("implementation")
        .and_then(|implementation| implementation.get("plugin"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(DEFAULT_IMPLEMENTATION_PLUGIN);
    let Some(factories) = document
        .get(plugin)
        .and_then(|settings| factories_in(settings))
        .or_else(|| factories_in(&document))
    else {
        return Vec::new();
    };
    factories
        .iter()
        .filter_map(|(name, declaration)| resolve_factory(name, declaration, registry))
        .collect()
}

/// Resolve one `dispatcher.factories` entry to the server the Fabro source
/// polls it at.
fn resolve_factory(
    name: &str,
    declaration: &serde_json::Value,
    registry: &FactoryRegistry,
) -> Option<FactoryServer> {
    if is_registry_backed(declaration) {
        return Some(registry.resolve(name).map_or_else(
            |reason| FactoryServer::unresolvable(name, &reason),
            |server| FactoryServer::new(name, server),
        ));
    }
    committed_server(declaration).map(|server| FactoryServer::new(name, server))
}

/// Whether the entry declares itself registry-backed (`"registry": true`), the
/// shape the spec's `constraints.md` requires of every committed entry.
fn is_registry_backed(declaration: &serde_json::Value) -> bool {
    declaration
        .get("registry")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

/// The non-empty `server` an entry commits, if it commits one.
fn committed_server(declaration: &serde_json::Value) -> Option<&str> {
    declaration
        .get("server")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|server| !server.is_empty())
}

fn factories_in(
    settings: &serde_json::Value,
) -> Option<&serde_json::Map<String, serde_json::Value>> {
    settings
        .get("dispatcher")
        .and_then(|dispatcher| dispatcher.get("factories"))
        .and_then(serde_json::Value::as_object)
}

/// Strip JSONC comments so `serde_json` can read a commented config.
///
/// String-aware by construction: `.livespec.jsonc`'s factory declarations are
/// `https://` URLs, whose `//` is NOT a comment, and a stripper that did not
/// track string state would truncate every server URL to `https:` and silently
/// resolve zero factories -- the same blindness this module exists to end.
/// Both `//` line comments and `/* ... */` block comments are removed; a line
/// comment's terminating newline is kept so line numbers in a parse error still
/// line up with the file.
fn strip_jsonc_comments(config: &str) -> String {
    let mut stripped = String::with_capacity(config.len());
    let mut characters = config.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(character) = characters.next() {
        if in_string {
            stripped.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        if character == '"' {
            in_string = true;
            stripped.push(character);
            continue;
        }
        if character == '/' {
            match characters.peek().copied() {
                Some('/') => {
                    for next in characters.by_ref() {
                        if next == '\n' {
                            stripped.push('\n');
                            break;
                        }
                    }
                    continue;
                }
                Some('*') => {
                    characters.next();
                    let mut previous = '\0';
                    for next in characters.by_ref() {
                        if previous == '*' && next == '/' {
                            break;
                        }
                        previous = next;
                    }
                    continue;
                }
                _other => {}
            }
        }
        stripped.push(character);
    }
    stripped
}
