//! The by-focus key table's `Enter` cell must name every view that actually
//! binds `Enter`.
//!
//! # Why this gate exists
//!
//! This is the single most-drifted claim in `docs/`, and it is not a close
//! call — it rotted TWICE in one day:
//!
//! 1. `e724b9c` made `Enter` open a work-item record inside a drilled-in lane.
//!    The table still said "open the item's command modal". Fixed by an audit.
//! 2. Hours later `2cd1f28` bound `Enter` in the Attention view to the same
//!    record modal and made the command modal early-return when it has no
//!    actions. The table went stale again, in the same shape.
//!
//! The second one is the instructive one. `2cd1f28` DID update the Status-line
//! hints in `docs/detailed-usage.md` — because `docs_status_hint_lockstep`
//! fails the build otherwise — and did NOT update this table three sections
//! away, which nothing gated. Same file, same commit, same author. The gated
//! half moved and the ungated half rotted, which is about as clean a natural
//! experiment as this repo is going to produce.
//!
//! # What it asserts
//!
//! `enter_content_input` in the TUI is the one place `Enter` is resolved for
//! the content pane. Every `TuiView` variant it mentions has some `Enter`
//! behavior worth a reader knowing about, so every one of them must be NAMED
//! in the table's `Enter` cell.
//!
//! One-directional on purpose, matching `docs_status_hint_lockstep`: source ⊆
//! doc. It fires when a view GAINS `Enter` behavior that the table does not
//! mention — the exact drift that happened twice. It cannot catch a cell that
//! describes a named view's behavior *incorrectly*, because prose is not
//! executable; keeping the cell honest about WHAT each view does still needs a
//! human. What it removes is the failure mode where a whole view silently
//! appears or disappears from the binding.

use std::path::{Path, PathBuf};

/// Where `Enter` is resolved for the content pane.
const ENTER_SOURCE: &str = "crates/console-tui/src/lib.rs";
/// The doc carrying the by-focus key table.
const KEY_TABLE_DOC: &str = "docs/detailed-usage.md";
/// The function that owns the binding.
const ENTER_FN: &str = "fn enter_content_input";

fn repo_root() -> std::io::Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
}

fn read(relative: &str) -> std::io::Result<String> {
    std::fs::read_to_string(repo_root()?.join(relative))
}

/// The body of `enter_content_input`, from its signature to the first
/// column-zero `}`.
fn enter_fn_body(source: &str) -> Option<String> {
    let start = source.find(ENTER_FN)?;
    let rest = &source[start..];
    let end = rest.find("\n}")?;
    Some(rest[..end].to_string())
}

/// Every `TuiView` variant named in that body, deduplicated and sorted.
fn views_with_enter_behavior(body: &str) -> Vec<String> {
    let mut views: Vec<String> = body
        .match_indices("TuiView::")
        .map(|(at, marker)| {
            body[at + marker.len()..]
                .chars()
                .take_while(char::is_ascii_alphabetic)
                .collect::<String>()
        })
        .filter(|name| !name.is_empty())
        .collect();
    views.sort();
    views.dedup();
    views
}

/// The `Enter` cell of the by-focus table's `content` row.
///
/// Columns are `Focus | up/down | left | right | Enter | Esc`, so the cell is
/// the fifth after the row's leading pipe.
fn content_row_enter_cell(doc: &str) -> Option<String> {
    let row = doc
        .lines()
        .find(|line| line.trim_start().starts_with("| content "))?;
    let cells: Vec<&str> = row.split('|').collect();
    cells.get(5).map(|cell| cell.trim().to_string())
}

#[test]
fn every_view_binding_enter_is_named_in_the_key_table() -> std::io::Result<()> {
    let source = read(ENTER_SOURCE)?;
    let doc = read(KEY_TABLE_DOC)?;

    let body = enter_fn_body(&source).unwrap_or_default();
    assert!(
        !body.is_empty(),
        "could not locate `{ENTER_FN}` in {ENTER_SOURCE}; if it was renamed, point this gate \
         at its replacement rather than deleting the gate"
    );

    let views = views_with_enter_behavior(&body);
    assert!(
        views.len() >= 2,
        "expected `{ENTER_FN}` to branch on at least two TuiView variants, found {views:?}; \
         the extraction this gate relies on has probably stopped working"
    );

    let cell = content_row_enter_cell(&doc).unwrap_or_default();
    assert!(
        !cell.is_empty(),
        "could not find the `content` row of the by-focus key table in {KEY_TABLE_DOC}"
    );

    let missing: Vec<&String> = views.iter().filter(|view| !cell.contains(*view)).collect();

    assert!(
        missing.is_empty(),
        "{KEY_TABLE_DOC}'s by-focus `Enter` cell does not mention every view that binds \
         `Enter`.\nA reader consulting the table in one of these views is told the wrong \
         thing.\n\nViews handled by `{ENTER_FN}`: {views:?}\nNot named in the table: \
         {missing:?}\n\nThe cell currently reads:\n  {cell}\n"
    );
    Ok(())
}

#[cfg(test)]
mod extraction {
    use super::{content_row_enter_cell, views_with_enter_behavior};

    const BODY: &str = r"
        if model.active_view() == TuiView::Lanes { return drill(); }
        if model.active_view() == TuiView::Attention { return record(); }
        if model.active_view() == TuiView::Settings { return edit(); }
    ";

    #[test]
    fn collects_each_view_once_and_sorted() {
        assert_eq!(
            views_with_enter_behavior(BODY),
            vec![
                "Attention".to_string(),
                "Lanes".to_string(),
                "Settings".to_string()
            ]
        );
    }

    #[test]
    fn a_repeated_view_is_not_double_counted() {
        let repeated = "TuiView::Lanes ... TuiView::Lanes";
        assert_eq!(
            views_with_enter_behavior(repeated),
            vec!["Lanes".to_string()]
        );
    }

    /// The exact drift `2cd1f28` introduced: source gained `Attention`, the
    /// table still named only the other two.
    #[test]
    fn the_real_regression_is_detectable() {
        let stale_cell = "in Lanes: drill into a lane; in Settings: edit the row";
        let views = views_with_enter_behavior(BODY);
        let missing: Vec<&String> = views
            .iter()
            .filter(|view| !stale_cell.contains(*view))
            .collect();
        assert_eq!(missing, vec![&"Attention".to_string()]);
    }

    #[test]
    fn reads_the_fifth_cell_of_the_content_row() {
        let doc = "\
| Focus | up | left | right | Enter | Esc |
|---|---|---|---|---|---|
| Views | a | b | c | d | e |
| content | move | back | detail | in Lanes: drill | as left |
";
        assert_eq!(
            content_row_enter_cell(doc).as_deref(),
            Some("in Lanes: drill")
        );
    }

    #[test]
    fn returns_none_when_the_content_row_is_absent() {
        assert_eq!(
            content_row_enter_cell("| Views | a | b | c | d | e |"),
            None
        );
    }
}
