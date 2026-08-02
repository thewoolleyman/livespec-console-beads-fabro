//! The Status-line hints documented in `docs/detailed-usage.md` must be the
//! hints the console actually renders.
//!
//! # Why this gate exists
//!
//! The B6 docs tree documented the Status hints correctly against the binary of
//! the day. Within twenty-four hours a behavior change made the hints
//! state-dependent — they stopped advertising keys that would do nothing — and
//! the doc silently became wrong in four places: the single "Lanes" row was
//! really three (lane overview / drilled-in with a selection / drilled-in and
//! empty), the Attention row was two, `enter drill` became `enter item` inside a
//! lane, and a new work-item record overlay arrived with a hint of its own.
//! Nothing failed, because prose is not executable.
//!
//! This is the same lockstep idea as `console-completeness-check` (which binds
//! the orchestrator's declared settings keys to the settings doc), applied to
//! the hint strings: every hint the doc quotes must exist VERBATIM as a string
//! literal in the module that produces them. Changing a hint without updating
//! the doc now fails here rather than in a reader's terminal.
//!
//! The value check is deliberately one-directional: it asserts doc ⊆ source, not
//! equality. The table's selected-work-item contexts are checked separately for
//! completeness because an omitted row cannot be caught by comparing only the
//! hint strings that the doc chose to quote.
//!
//! Since the hints became a registry DERIVATION rather than string literals, a
//! third arm binds every selected-work-item row to the availability context it
//! documents and asserts the quoted hint EQUALS the derivation for that
//! context — bidirectional per documented row, which the grep arm cannot be.
//! (The grep arm still guards the non-item rows, whose strings remain source
//! literals.)

use std::path::{Path, PathBuf};

use console_application::action_registry::{ActionContext, ActionSurface, selected_item_hint};
use console_application::source_adapters::{AcceptancePolicy, AdmissionPolicy, Lane};

/// Where the hints are produced (`footer_hint` / `pane_footer_hint`).
const HINT_SOURCE: &str = "crates/console-application/src/lib.rs";
/// The doc carrying the Status-line table.
const SETTINGS_DOC: &str = "docs/detailed-usage.md";

fn repo_root() -> std::io::Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
}

fn read(relative: &str) -> std::io::Result<String> {
    std::fs::read_to_string(repo_root()?.join(relative))
}

/// Every hint quoted in the doc's Status-line table.
///
/// The table renders each hint in backticks with markdown-escaped pipes
/// (`\|`); this recovers the literal the source must contain. Only rows of the
/// table are considered — a row is a line starting with `|` whose second cell is
/// a single backticked span.
fn documented_hints(doc: &str) -> Vec<String> {
    let mut hints = Vec::new();
    for line in doc.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let Some(open) = line.find('`') else { continue };
        let rest = &line[open + 1..];
        let Some(close) = rest.find('`') else {
            continue;
        };
        let span = &rest[..close];
        // A hint always offers at least one key/action pair separated by `|`.
        if !span.contains("\\|") {
            continue;
        }
        hints.push(span.replace("\\|", "|"));
    }
    hints
}

/// Every run of whitespace collapsed to a single space, so a comparison is
/// insensitive to how a string literal was wrapped in the source.
fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn documented_contexts(doc: &str) -> Vec<String> {
    let mut contexts = Vec::new();
    for line in doc.lines() {
        let line = line.trim();
        if !line.starts_with('|') || line.starts_with("|---") {
            continue;
        }
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        if cells.len() < 3 || cells[1] == "Context" {
            continue;
        }
        contexts.push(cells[1].to_owned());
    }
    contexts
}

#[test]
fn selected_work_item_status_contexts_are_documented() -> std::io::Result<()> {
    let doc = read(SETTINGS_DOC)?;
    let contexts = documented_contexts(&doc);
    let required = [
        "Attention, backlog work-item selected",
        "Attention, pending-approval work-item selected",
        "Attention, dispatcher-admitted pending-approval work-item selected",
        "Attention, acceptance work-item selected",
        "Attention, blocked work-item selected",
        "Lanes, drilled into a backlog item",
        "Lanes, drilled into a pending-approval item",
        "Lanes, drilled into a dispatcher-admitted pending-approval item",
        "Lanes, drilled into a ready item",
        "Lanes, drilled into a host-only-refused ready item",
        "Lanes, drilled into an active item",
        "Lanes, drilled into an acceptance item",
        "Lanes, drilled into a blocked item",
        "Lanes, drilled into a done item",
    ];
    let missing: Vec<&str> = required
        .into_iter()
        .filter(|context| !contexts.iter().any(|documented| documented == context))
        .collect();

    assert!(
        missing.is_empty(),
        "{SETTINGS_DOC} is missing reachable selected-work-item Status contexts:\n{}",
        missing
            .iter()
            .map(|context| format!("  - {context}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    Ok(())
}

#[test]
fn every_documented_status_hint_exists_in_the_source() -> std::io::Result<()> {
    let doc = read(SETTINGS_DOC)?;
    let source = read(HINT_SOURCE)?;

    let hints = documented_hints(&doc);
    assert!(
        hints.len() >= 10,
        "expected the Status-line table to yield at least ten hints, got {}: {hints:#?}",
        hints.len()
    );

    // Rust wraps long string literals across lines with a trailing `\`, which
    // swallows the newline but leaves the next line's indentation in the
    // literal — the source text and the rendered hint differ only in runs of
    // whitespace. Collapse every whitespace run on both sides so the comparison
    // is about the WORDS, not about how the literal happens to be wrapped.
    let folded = collapse_whitespace(&source.replace("\\\n", " "));
    let missing: Vec<String> = hints
        .iter()
        .filter(|hint| !folded.contains(&collapse_whitespace(hint)))
        .cloned()
        .collect();

    assert!(
        missing.is_empty(),
        "{SETTINGS_DOC} documents Status-line hints that {HINT_SOURCE} no longer renders.\n\
         Either the hint changed and the doc was not updated, or the doc quotes a hint that \
         never existed.\nMissing:\n{}",
        missing
            .iter()
            .map(|hint| format!("  - {hint}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    Ok(())
}

/// The lane-overview hint must NOT advertise the per-item valves.
///
/// This is the specific regression the drift introduced: documenting the valve
/// keys on the lane overview tells an operator to press keys that do nothing
/// there, because the overview selects a lane rather than a work-item.
#[test]
fn the_lane_overview_hint_advertises_no_per_item_valve() -> std::io::Result<()> {
    let doc = read(SETTINGS_DOC)?;

    let row = doc
        .lines()
        .find(|line| line.contains("Lanes, lane overview"))
        .unwrap_or_default();
    assert!(
        !row.is_empty(),
        "{SETTINGS_DOC} must document the lane-overview Status hint"
    );
    for valve in ["p/c/r", "m/n", "s move-status"] {
        assert!(
            !row.contains(valve),
            "the lane-overview hint must not advertise `{valve}`: the overview selects a lane, \
             not a work-item, so every per-item key is inert there.\nRow: {row}"
        );
    }
    Ok(())
}

/// Every `(context, hint)` pair quoted in the doc's Status-line table.
fn documented_context_hints(doc: &str) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    for line in doc.lines() {
        let line = line.trim();
        if !line.starts_with('|') || line.starts_with("|---") {
            continue;
        }
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        if cells.len() < 3 || cells[1] == "Context" {
            continue;
        }
        let Some(open) = line.find('`') else { continue };
        let rest = &line[open + 1..];
        let Some(close) = rest.find('`') else {
            continue;
        };
        rows.push((cells[1].to_owned(), rest[..close].replace("\\|", "|")));
    }
    rows
}

/// The availability context a selected-work-item table row documents, or
/// `None` for the non-item rows (header, overview, overlays, ...), which the
/// grep arm guards.
fn context_binding(label: &str) -> Option<ActionContext> {
    let (surface, lane, admission, handoff) = match label {
        "Attention, backlog work-item selected" => (
            ActionSurface::Attention,
            Lane::Backlog,
            AdmissionPolicy::Manual,
            false,
        ),
        "Attention, pending-approval work-item selected" => (
            ActionSurface::Attention,
            Lane::PendingApproval,
            AdmissionPolicy::Manual,
            false,
        ),
        "Attention, dispatcher-admitted pending-approval work-item selected" => (
            ActionSurface::Attention,
            Lane::PendingApproval,
            AdmissionPolicy::Auto,
            false,
        ),
        "Attention, acceptance work-item selected" => (
            ActionSurface::Attention,
            Lane::Acceptance,
            AdmissionPolicy::Manual,
            false,
        ),
        "Attention, blocked work-item selected" => (
            ActionSurface::Attention,
            Lane::Blocked,
            AdmissionPolicy::Manual,
            false,
        ),
        // A drilled-in backlog item ALWAYS claims the driver-handoff (groom)
        // verb, so its documented row carries `h handoff`.
        "Lanes, drilled into a backlog item" => (
            ActionSurface::LaneDrill,
            Lane::Backlog,
            AdmissionPolicy::Manual,
            true,
        ),
        "Lanes, drilled into a pending-approval item" => (
            ActionSurface::LaneDrill,
            Lane::PendingApproval,
            AdmissionPolicy::Manual,
            false,
        ),
        "Lanes, drilled into a dispatcher-admitted pending-approval item" => (
            ActionSurface::LaneDrill,
            Lane::PendingApproval,
            AdmissionPolicy::Auto,
            false,
        ),
        "Lanes, drilled into a ready item" => (
            ActionSurface::LaneDrill,
            Lane::Ready,
            AdmissionPolicy::Manual,
            false,
        ),
        "Lanes, drilled into a host-only-refused ready item" => (
            ActionSurface::LaneDrill,
            Lane::Ready,
            AdmissionPolicy::Manual,
            true,
        ),
        "Lanes, drilled into an active item" => (
            ActionSurface::LaneDrill,
            Lane::Active,
            AdmissionPolicy::Manual,
            false,
        ),
        "Lanes, drilled into an acceptance item" => (
            ActionSurface::LaneDrill,
            Lane::Acceptance,
            AdmissionPolicy::Manual,
            false,
        ),
        "Lanes, drilled into a blocked item" => (
            ActionSurface::LaneDrill,
            Lane::Blocked,
            AdmissionPolicy::Manual,
            false,
        ),
        "Lanes, drilled into a done item" => (
            ActionSurface::LaneDrill,
            Lane::Done,
            AdmissionPolicy::Manual,
            false,
        ),
        _other => return None,
    };
    Some(ActionContext {
        lane,
        admission_policy: admission,
        acceptance_policy: AcceptancePolicy::AiThenHuman,
        has_driver_handoff: handoff,
        surface,
    })
}

/// Every selected-work-item row's quoted hint EQUALS the registry derivation
/// for the context it documents.
#[test]
fn every_documented_selected_item_hint_equals_the_rendered_derivation() -> std::io::Result<()> {
    let doc = read(SETTINGS_DOC)?;
    let mut bound = 0;
    for (label, hint) in documented_context_hints(&doc) {
        let Some(ctx) = context_binding(&label) else {
            continue;
        };
        bound += 1;
        assert_eq!(
            hint,
            selected_item_hint(&ctx),
            "the documented hint for context `{label}` must equal the rendered derivation"
        );
    }
    assert!(
        bound >= 14,
        "expected at least fourteen bound selected-item rows, got {bound}"
    );
    Ok(())
}
