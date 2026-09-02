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
//! Since the hints became a registry DERIVATION rather than string literals,
//! extra arms bind documented rows to the availability/model context they
//! describe and assert the quoted hint EQUALS the rendered derivation for that
//! context — bidirectional per documented row, which the grep arm cannot be.

use std::path::{Path, PathBuf};

use console_application::action_registry::{
    ActionContext, ActionSurface, global_status_hint_tokens,
    operator_key_action_reference_markdown, selected_item_hint,
};
use console_application::source_adapters::{AcceptancePolicy, AdmissionPolicy, Lane};
use console_application::{
    FocusPane, LaneFocus, PendingValve, TuiInteractionState, TuiOverlay, TuiView,
    build_tui_model_for_state,
};

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

fn documented_model_hint(label: &str) -> Option<String> {
    let state = match label {
        "Header focused" => {
            TuiInteractionState::new(0, TuiOverlay::None).with_focus(FocusPane::Header)
        }
        "Attention, no work-item selected" => TuiInteractionState::new(0, TuiOverlay::None),
        "Lanes, lane overview" => {
            TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
        }
        "Lanes, drilled into an empty lane" => {
            TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
                .with_lane_focus(LaneFocus::Lane(Lane::Backlog))
        }
        "Settings" => TuiInteractionState::for_view(TuiView::Settings, 0, TuiOverlay::None),
        "Spec, Events, Repos" => TuiInteractionState::for_view(TuiView::Spec, 0, TuiOverlay::None),
        "Search open" => TuiInteractionState::new(
            0,
            TuiOverlay::Search {
                query: String::new(),
            },
        ),
        "Command palette open" => TuiInteractionState::new(
            0,
            TuiOverlay::CommandPalette {
                query: String::new(),
            },
        ),
        "Action invoker open" => {
            TuiInteractionState::new(0, TuiOverlay::ActionInvoker { selected_action: 0 })
        }
        "Command modal open" => TuiInteractionState::new(
            0,
            TuiOverlay::CommandModal {
                selected_action_index: 0,
            },
        ),
        "Valve confirm open" => TuiInteractionState::new(
            0,
            TuiOverlay::ValveConfirm {
                valve: PendingValve::Approve,
            },
        ),
        "Work-item record open" => TuiInteractionState::new(
            0,
            TuiOverlay::WorkItemDetail {
                work_item_id: String::new(),
                scroll: 0,
            },
        ),
        "Help open" => TuiInteractionState::new(
            0,
            TuiOverlay::Help {
                focus: console_application::HelpFocus::Menu,
                selected_section: 0,
                scroll: 0,
            },
        ),
        _other => return None,
    };
    Some(build_tui_model_for_state(&[], &state).footer().into_owned())
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
        "Lanes, drilled into a factory-unsafe ready item",
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
    let missing: Vec<String> = documented_context_hints(&doc)
        .into_iter()
        .filter(|(_context, hint)| hint.contains('|'))
        .filter(|(context, hint)| {
            !folded.contains(&collapse_whitespace(hint))
                && documented_model_hint(context).is_none_or(|rendered| rendered != *hint)
        })
        .map(|(_context, hint)| hint)
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
        // A backlog item ALWAYS claims the driver-handoff (groom) verb, and the
        // claim is a property of the RECORD, not of the view hosting it
        // (Scenario 31), so the inbox row carries `h handoff` exactly as the
        // drilled-in row below does.
        "Attention, backlog work-item selected" => (
            ActionSurface::Attention,
            Lane::Backlog,
            AdmissionPolicy::Manual,
            true,
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
        "Lanes, drilled into a factory-unsafe ready item" => (
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
        // The documented contexts describe items NOT awaiting an override, so
        // the scope-override action contributes no hint token to any of them.
        // It carries an empty `hint_token` anyway (it is menu/invoker-only),
        // so this cannot change a documented row either way — stated rather
        // than left for a reader to re-derive.
        awaits_scope_override: false,
        ready_work_item_count: 1,
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

#[test]
fn generated_reference_carries_the_global_status_hint_derivation() {
    let generated = operator_key_action_reference_markdown();
    let expected = format!("`{}`", global_status_hint_tokens().join(" | "));
    assert!(
        generated.contains(&expected),
        "the generated key/action reference must carry the registry-derived global Status hint: {expected}"
    );
}

#[test]
fn driver_handoff_behavior_is_documented() -> std::io::Result<()> {
    let doc = read(SETTINGS_DOC)?;
    let doc = collapse_whitespace(&doc);
    for required in [
        "### Driver handoff",
        "`h` opens the full-width **Driver Handoff** overlay",
        // The verb is claimed by the RECORD, not by the surface hosting it
        // (Scenario 31), so the documented phrases no longer say "drilled-in".
        "`backlog` item renders the groom invocation",
        "`ready` item with a non-null `factory_safety` marking",
        "renders the driver-implement invocation",
        "suppressed everywhere else",
        "carries only the selected work-item id",
        "does not write a prompt file, execute the driver, spawn a process, monitor a driver session, or wait for one",
        "`copy sent to terminal`",
        "MUST NOT claim an unobservable result such as `Copied!`",
    ] {
        assert!(
            doc.contains(required),
            "{SETTINGS_DOC} must document the driver-handoff behavior phrase:\n  {required}"
        );
    }
    Ok(())
}
