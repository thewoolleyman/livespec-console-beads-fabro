//! Cross-repo parity: the action registry accounts for the orchestrator's
//! PUBLISHED human action surface.
//!
//! The fixture (`tests/fixtures/drive-human-action-surface.json`) is a
//! hand-reviewed capture of the orchestrator's drive action surface — its
//! `contracts.md` "#### drive" grammar and per-state operator verb
//! vocabulary, cross-checked against the shipped `is_human_valve_action`
//! prefix tuple — because the orchestrator publishes no machine-readable
//! manifest for actions (only config keys have one). Every captured action
//! must either bind to a registry action id or carry a mandatory non-empty
//! reason, and the check is BIDIRECTIONAL: a registry entry claiming a drive
//! verb must appear in the capture too, so neither side can drift silently.
//! A gate demonstrated red in one direction is evidence about that direction
//! only — this one was BORN RED against the shipped-but-unpublished
//! `set-workflow-scope-override` before the console bound it, and that red
//! run is its red demonstration.

use std::path::{Path, PathBuf};

use console_application::action_registry::{ACTION_REGISTRY, action_for_id};

const FIXTURE: &str = "tests/fixtures/drive-human-action-surface.json";

fn repo_root() -> std::io::Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
}

fn fixture() -> std::io::Result<serde_json::Value> {
    let raw = std::fs::read_to_string(repo_root()?.join(FIXTURE))?;
    serde_json::from_str(&raw).map_err(std::io::Error::other)
}

fn entries<'a>(
    doc: &'a serde_json::Value,
    key: &str,
) -> Vec<&'a serde_json::Map<String, serde_json::Value>> {
    doc.get(key)
        .and_then(serde_json::Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(serde_json::Value::as_object)
                .collect()
        })
        .unwrap_or_default()
}

fn str_field<'a>(entry: &'a serde_json::Map<String, serde_json::Value>, key: &str) -> &'a str {
    entry
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
}

/// Forward direction: every captured human action binds to a REGISTERED
/// action id, or carries a recorded omission with a mandatory reason.
#[test]
fn every_captured_human_action_is_registered_or_deliberately_omitted() -> std::io::Result<()> {
    let doc = fixture()?;
    let actions = entries(&doc, "actions");
    assert!(
        actions.len() >= 11,
        "the capture must carry the eleven human valve prefixes"
    );
    let mut missing: Vec<String> = Vec::new();
    for entry in &actions {
        let action = str_field(entry, "action");
        let binding = str_field(entry, "console_binding");
        assert!(
            !action.is_empty(),
            "an actions entry is missing its action id"
        );
        assert!(
            !binding.is_empty(),
            "captured action `{action}` must carry a console_binding or move to a recorded-omission section with a reason"
        );
        if action_for_id(binding).is_none() {
            missing.push(format!("{action} -> {binding}"));
        }
    }
    assert!(
        missing.is_empty(),
        "captured human actions bind to NO registered action (the registry does not account \
         for the orchestrator's published surface):\n{}",
        missing.join("\n")
    );
    Ok(())
}

/// Every recorded omission carries a mandatory non-empty reason — an
/// allowlist says "ignore this"; a reason says "here is why".
#[test]
fn every_recorded_omission_carries_a_reason() -> std::io::Result<()> {
    let doc = fixture()?;
    for section in [
        "published_but_unimplemented",
        "non_operator_actions",
        "console_local_actions",
    ] {
        let listed = entries(&doc, section);
        assert!(
            !listed.is_empty(),
            "section `{section}` must exist and be non-empty"
        );
        for entry in listed {
            let action = str_field(entry, "action");
            assert!(!action.is_empty());
            assert!(
                !str_field(entry, "reason").trim().is_empty(),
                "`{action}` in `{section}` carries no reason"
            );
        }
    }
    Ok(())
}

/// Reverse direction: every registry entry appears in the capture — as a
/// `console_binding` or a recorded console-local action — so a registry action
/// claiming a drive verb the capture never reviewed is a red build, not a
/// silent gap (the fork-drift gate's measured one-way blind spot, not
/// inherited here).
#[test]
fn every_registry_action_is_accounted_for_in_the_capture() -> std::io::Result<()> {
    let doc = fixture()?;
    let bound: Vec<&str> = entries(&doc, "actions")
        .iter()
        .map(|entry| str_field(entry, "console_binding"))
        .collect();
    let local: Vec<&str> = entries(&doc, "console_local_actions")
        .iter()
        .map(|entry| str_field(entry, "action"))
        .collect();
    let unaccounted: Vec<&str> = ACTION_REGISTRY
        .iter()
        .map(|spec| spec.id)
        .filter(|id| !bound.contains(id) && !local.contains(id))
        .collect();
    assert!(
        unaccounted.is_empty(),
        "registry actions the capture never reviewed: {unaccounted:?}"
    );
    Ok(())
}
