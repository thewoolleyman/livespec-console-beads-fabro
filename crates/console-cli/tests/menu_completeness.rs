//! EVERY operator-reachable behaviour must be drivable via a menu path, or be
//! explicitly and reasonably excluded.
//!
//! # Why this gate quantifies over BEHAVIOURS and not registry rows
//!
//! Menus are GENERATED from the action registry. A completeness test written
//! against "every registered action" would therefore quantify over its own
//! input and pass by construction — it could never fail, and this repo's
//! standing rule is that a verifier must be able to fail.
//!
//! So the population is what the operator can actually DO: every key arm
//! handled in `key_event_to_terminal_input`. Each must be either
//!
//!   * a REGISTRY hotkey (and so carries a `menu_path` by construction), or
//!   * listed in `tests/fixtures/menu-completeness-carveout.json` with a
//!     mandatory reason, or
//!   * inert — an arm the handler maps to `None`, which has no behaviour to
//!     cover.
//!
//! Anything else is a RED BUILD. **Fail-closed is the point**: a structural key
//! added tomorrow escapes the gate silently otherwise, which is the `-2ckgiy`
//! mechanism (a shipped verb with no prose) wearing different clothes.
//!
//! Maintainer ruling 2026-08-03: "every operator-reachable behaviour", with a
//! REGISTER-BY-DEFAULT posture — exclusion is the exception and must be argued.
//! See `plan/02-menu-shell-primacy/handoff.md` § 3b.

use std::path::{Path, PathBuf};

use console_application::action_registry::ACTION_REGISTRY;

/// The key handler whose arms define the behaviour population.
const KEY_HANDLER: &str = "crates/console-tui/src/lib.rs";
/// The argued exclusions.
const CARVEOUT: &str = "tests/fixtures/menu-completeness-carveout.json";

fn repo_root() -> std::io::Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
}

fn read(relative: &str) -> std::io::Result<String> {
    std::fs::read_to_string(repo_root()?.join(relative))
}

/// The ONE tokenizer both scanners use.
///
/// # Why this is shared rather than written twice
///
/// It used to be written twice, and the two copies disagreed. The handled
/// scanner kept `Char(...)` whole but truncated `F(_)` to `F`; the inert
/// scanner kept the parens and produced `F(_)`. The tokens therefore never
/// compared equal for any parenthesised arm, so every inert `F(_)`, `Media(_)`
/// and `Modifier(_)` surfaced as an uncovered behaviour — three false
/// positives in a gate whose entire value is that its red output is
/// trustworthy.
///
/// Carving those three out in the fixture would have papered over the defect
/// AND weakened the fixture, since they are not behaviours at all. One
/// tokenizer is the fix: two encodings of "what a key arm is called" is the
/// same second-encoding defect this arc exists to retire, in miniature.
///
/// `Char('X')` must survive whole: X is frequently a punctuation key
/// (`/`, `:`, `?`), so an alphanumeric-only scan silently truncates the very
/// arms this gate exists to name.
fn key_token(raw: &str) -> String {
    let split = raw
        .char_indices()
        .find(|(_, c)| !(c.is_alphanumeric() || *c == '_'))
        .map_or(raw.len(), |(index, _)| index);
    let (name, rest) = raw.split_at(split);
    rest.strip_prefix('(').map_or_else(
        || name.to_owned(),
        |inner| {
            inner.find(')').map_or_else(
                || name.to_owned(),
                |end| format!("{name}({})", &inner[..end]),
            )
        },
    )
}

/// Every `KeyCode::…` arm of `key_event_to_terminal_input`, in source order.
///
/// Parsed from the match rather than hand-listed: a hand-listed population is
/// the same second-encoding defect this arc exists to retire, and it would go
/// stale the moment a key is added — which is exactly the case the gate must
/// catch.
fn handled_key_arms(source: &str) -> Vec<String> {
    let body = source
        .split_once("fn key_event_to_terminal_input")
        .map_or(source, |(_, rest)| rest);
    // The match ends at the function's closing brace at column 0.
    let body = body.split_once("\n}\n").map_or(body, |(head, _)| head);
    let mut arms = Vec::new();
    for raw in body.split("KeyCode::").skip(1) {
        let token = key_token(raw);
        if token.is_empty() {
            continue;
        }
        if !arms.contains(&token) {
            arms.push(token);
        }
    }
    arms
}

/// Arms the handler resolves to `None` — inert, no behaviour to cover.
fn inert_arms(source: &str) -> Vec<String> {
    let Some((_, rest)) = source.split_once("fn key_event_to_terminal_input") else {
        return Vec::new();
    };
    // The trailing catch-all group: `KeyCode::A | KeyCode::B | … => None,`
    let Some((head, _)) = rest.split_once("=> None,") else {
        return Vec::new();
    };
    let tail_start = head.rfind("KeyCode::Home").unwrap_or(head.len());
    head[tail_start..]
        .split("KeyCode::")
        .filter_map(|raw| {
            let token = key_token(raw);
            (!token.is_empty()).then_some(token)
        })
        .collect()
}

fn carveout_keys(fixture: &str) -> Vec<(String, String)> {
    // Unparseable or malformed fixture yields NO exclusions, so the gate fails
    // closed rather than silently excusing everything.
    let parsed: serde_json::Value = serde_json::from_str(fixture).unwrap_or_default();
    let Some(entries) = parsed.get("excluded").and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    entries
        .iter()
        .map(|entry| {
            (
                entry["key"].as_str().unwrap_or_default().to_owned(),
                entry["reason"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

/// Every excluded key states WHY. A blank reason is an undocumented exclusion,
/// which is the thing this fixture exists to prevent.
#[test]
fn every_carved_out_behaviour_states_its_reason() -> std::io::Result<()> {
    let entries = carveout_keys(&read(CARVEOUT)?);
    assert!(
        !entries.is_empty(),
        "{CARVEOUT} carries no exclusions; if that is genuinely true, delete the fixture \
         rather than leaving an empty one that reads as 'nothing was checked'."
    );
    let unreasoned: Vec<&String> = entries
        .iter()
        .filter(|(_, reason)| reason.trim().len() < 20)
        .map(|(key, _)| key)
        .collect();
    assert!(
        unreasoned.is_empty(),
        "{CARVEOUT} excludes behaviours without a stated reason: {unreasoned:?}\n\
         An exclusion a reader cannot distinguish from an oversight is not an exclusion."
    );
    Ok(())
}

/// THE COMPLETENESS GATE.
///
/// Every handled key arm is a registry hotkey, an argued exclusion, or inert.
#[test]
fn every_operator_reachable_behaviour_is_menu_drivable_or_argued() -> std::io::Result<()> {
    let source = read(KEY_HANDLER)?;
    let handled = handled_key_arms(&source);
    assert!(
        handled.len() > 10,
        "expected to parse the key handler's arms, got {handled:?} — the parse is stale, \
         which would make this gate vacuous"
    );

    let inert = inert_arms(&source);
    let carved: Vec<String> = carveout_keys(&read(CARVEOUT)?)
        .into_iter()
        .map(|(key, _)| key)
        .collect();

    let uncovered: Vec<&String> = handled
        .iter()
        .filter(|arm| {
            // `KeyCode::Char(value)` is the registry dispatch arm itself: every
            // key it resolves carries a menu_path by construction.
            if arm.as_str() == "Char(value)" {
                return false;
            }
            if inert.iter().any(|dead| dead == *arm) {
                return false;
            }
            carved.iter().all(|allowed| allowed != *arm)
        })
        .collect();

    assert!(
        uncovered.is_empty(),
        "Operator-reachable behaviours with NO menu path and NO argued exclusion:\n{}\n\n\
         Register them in the action registry with a `menu_path` (the default posture), or \
         argue each in {CARVEOUT} with a reason. Menus are the PRIMARY navigation surface; a \
         behaviour reachable only by keystroke contradicts that.\n\
         Registry currently holds {} entries.",
        uncovered
            .iter()
            .map(|arm| format!("  - KeyCode::{arm}"))
            .collect::<Vec<_>>()
            .join("\n"),
        ACTION_REGISTRY.len()
    );
    Ok(())
}
