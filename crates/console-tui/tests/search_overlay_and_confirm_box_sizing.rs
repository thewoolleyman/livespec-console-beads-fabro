//! The `/` search overlay reads as a one-line filter, and every confirm box is
//! sized to the lines it actually carries.
//!
//! Dogfooded 2026-09-08 (`livespec-console-beads-fabro-mx9u.4`, from the
//! `pzbdbo.23` / `ohtig5` TUI passes): pressing `/` opened a box titled
//! `Search` seven columns INTO the content pane and twelve empty rows tall
//! under its single `/una` input line, with list fragments (`propo`, `Resol`,
//! `Adopt`, `Repai`) still visible to its left -- so the region read as
//! half-cleared rather than as an overlay. At the same time the header's
//! `attention:` count followed the filter (`80` -> `1` -> `22`), which makes
//! the one number that reports how much work is waiting report instead how
//! much of it the current query happens to match.
//!
//! These tests pin the remedy at the RENDERED TEXT, which is the surface the
//! operator reads: the search overlay is two content rows plus its borders,
//! anchored to the content pane's left edge and spanning to the right edge so
//! no list row survives beneath it; the second row carries the `N of M match`
//! feedback the header no longer distorts; and the Valve / Factory Dispatch
//! confirm boxes are as tall as their content, so they read as dialogs.

use console_application::source_adapters::{AdmissionPolicy, Lane, LaneReason};
use console_application::{
    PendingValve, TuiInteractionState, TuiOverlay, TuiScreenModel, build_tui_model_for_state,
};
use console_domain::{ConsoleEvent, EventType};
use console_tui::render_to_text;

/// A viewport wide and tall enough that nothing here is clipped by the terminal
/// size, so every assertion is about layout rather than about degradation.
const WIDTH: u16 = 96;
const HEIGHT: u16 = 24;

/// A `blocked` / `needs-human` work-item snapshot: the lane that rests on a
/// human step, so the item lands in the needs-attention inbox.
fn blocked_event(event_id: &str, work_item_id: &str) -> ConsoleEvent {
    let payload = format!(
        r#"{{"repo":"console","work_item_id":"{work_item_id}","lane":"{}","lane_reason":"{}","rank":"a0","status":"blocked","detail":{{"title":"Routine lane fixture item"}},"source_version":1}}"#,
        Lane::Blocked.label(),
        LaneReason::NeedsHuman.label()
    );
    ConsoleEvent::fixture(
        event_id,
        EventType::WorkItemSnapshotObserved,
        "orchestrator",
    )
    .with_payload_json(payload)
}

/// A two-item inbox whose ids differ, so a query can match exactly one of them
/// and the filtered count and the inbox total are distinguishable numbers.
fn inbox_events() -> [ConsoleEvent; 2] {
    [
        blocked_event("evt_alpha", "console-alpha"),
        blocked_event("evt_beta", "console-beta"),
    ]
}

fn model(overlay: TuiOverlay) -> TuiScreenModel {
    build_tui_model_for_state(&inbox_events(), &TuiInteractionState::new(0, overlay))
}

fn render(overlay: TuiOverlay) -> Result<String, String> {
    render_to_text(&model(overlay), WIDTH, HEIGHT).map_err(|error| format!("{error:?}"))
}

fn searching(query: &str) -> TuiOverlay {
    TuiOverlay::Search {
        query: query.to_owned(),
    }
}

fn char_at(line: &str, column: usize) -> Option<char> {
    line.chars().nth(column)
}

/// The rendered row from `column` rightwards, as characters rather than bytes:
/// every box-drawing glyph here is multi-byte, so byte slicing would cut one in
/// half.
fn from_column(line: &str, column: usize) -> String {
    line.chars().skip(column).collect()
}

fn row(rendered: &str, index: usize) -> Result<&str, String> {
    rendered
        .lines()
        .nth(index)
        .ok_or_else(|| format!("row {index} is rendered\n{rendered}"))
}

/// The content pane's left edge, derived from an UNOVERLAID render rather than
/// hardcoded: it is the second top-left corner on the body's top border row,
/// the first being the `Views` navigation pane's.
fn content_pane_left_edge() -> Result<usize, String> {
    let rendered = render(TuiOverlay::None)?;
    let border_row = rendered
        .lines()
        .find(|line| line.contains("┌Views"))
        .ok_or_else(|| format!("the body's top border row names the Views pane\n{rendered}"))?;
    border_row
        .chars()
        .enumerate()
        .filter(|(_column, glyph)| *glyph == '┌')
        .map(|(column, _glyph)| column)
        .nth(1)
        .ok_or_else(|| format!("the content pane's corner sits beside the Views pane\n{rendered}"))
}

/// The row index of the box titled `title`, and the column its left border sits
/// in.
fn box_corner(rendered: &str, title: &str) -> Result<(usize, usize), String> {
    let marker = format!("┌{title}");
    let (index, line) = rendered
        .lines()
        .enumerate()
        .find(|(_index, line)| line.contains(&marker))
        .ok_or_else(|| format!("a box titled {title} is drawn\n{rendered}"))?;
    let byte_offset = line
        .find(&marker)
        .ok_or_else(|| format!("the {title} corner is locatable\n{rendered}"))?;
    let column = line
        .get(..byte_offset)
        .ok_or_else(|| format!("the {title} corner sits on a character boundary\n{rendered}"))?
        .chars()
        .count();
    Ok((index, column))
}

/// The full height of the box titled `title`, borders included.
fn box_height(rendered: &str, title: &str) -> Result<usize, String> {
    let (top, left) = box_corner(rendered, title)?;
    let bottom = rendered
        .lines()
        .enumerate()
        .skip(top + 1)
        .find(|(_index, line)| char_at(line, left) == Some('└'))
        .map(|(index, _line)| index)
        .ok_or_else(|| format!("the box titled {title} is closed\n{rendered}"))?;
    Ok(bottom - top + 1)
}

#[test]
fn the_search_overlay_is_two_content_rows_plus_borders_anchored_to_the_content_pane()
-> Result<(), String> {
    let left = content_pane_left_edge()?;
    let rendered = render(searching("alpha"))?;
    let (top, box_left) = box_corner(&rendered, "Search")?;

    // Anchored to the content pane's left edge -- not inset into it, which is
    // what left the list fragments visible beside the old box.
    assert_eq!(box_left, left, "\n{rendered}");
    // Exactly two content rows (the input and the match feedback) between the
    // two border rows.
    assert_eq!(box_height(&rendered, "Search")?, 4, "\n{rendered}");

    // Each row of the overlay carries the overlay ALONE: it spans from the
    // content pane's left edge to the viewport's right edge, so no row of the
    // underlying list is partially visible on these rows.
    let inner = usize::from(WIDTH) - left - 2;
    assert_eq!(
        from_column(row(&rendered, top)?, left),
        format!("┌Search{}┐", "─".repeat(inner - "Search".len())),
        "\n{rendered}"
    );
    assert_eq!(
        from_column(row(&rendered, top + 1)?, left),
        format!("│{:<inner$}│", "/alpha"),
        "\n{rendered}"
    );
    assert_eq!(
        from_column(row(&rendered, top + 2)?, left),
        format!("│{:<inner$}│", "1 of 2 match"),
        "\n{rendered}"
    );
    assert_eq!(
        from_column(row(&rendered, top + 3)?, left),
        format!("└{}┘", "─".repeat(inner)),
        "\n{rendered}"
    );
    Ok(())
}

#[test]
fn the_header_attention_count_stays_the_inbox_total_while_a_search_filters_the_list()
-> Result<(), String> {
    let unfiltered = model(TuiOverlay::None);
    assert_eq!(unfiltered.attention_items().len(), 2);
    assert!(
        unfiltered.header().contains("attention: 2"),
        "{}",
        unfiltered.header()
    );

    let filtered = model(searching("alpha"));

    // The list narrows to the match...
    assert_eq!(filtered.attention_items().len(), 1);
    // ...and the header keeps reporting the INBOX, not the match count.
    assert!(
        filtered.header().contains("attention: 2"),
        "{}",
        filtered.header()
    );
    assert!(
        filtered.header_line(300).contains("attention: 2"),
        "{}",
        filtered.header_line(300)
    );

    // The match count is feedback the overlay owns.
    let rendered = render(searching("alpha"))?;
    assert!(rendered.contains("1 of 2 match"), "\n{rendered}");
    assert!(!rendered.contains("attention: 1"), "\n{rendered}");
    Ok(())
}

#[test]
fn an_empty_query_reports_the_whole_inbox_as_matching() -> Result<(), String> {
    let rendered = render(searching(""))?;

    assert!(rendered.contains("2 of 2 match"), "\n{rendered}");
    assert_eq!(box_height(&rendered, "Search")?, 4, "\n{rendered}");
    Ok(())
}

#[test]
fn the_factory_dispatch_confirm_box_is_its_four_content_lines_plus_borders() -> Result<(), String> {
    let rendered = render(TuiOverlay::FactoryDispatchItemConfirm {
        work_item_id: "console-alpha".to_owned(),
    })?;

    assert_eq!(
        box_height(&rendered, "Factory Dispatch")?,
        6,
        "\n{rendered}"
    );
    for line in [
        "Dispatch selected work-item",
        "Target: console-alpha",
        "Uses Dispatcher loop --budget 1 --parallel 1 --item",
        "Enter to dispatch | Esc to cancel",
    ] {
        assert!(rendered.contains(line), "{line}\n{rendered}");
    }
    Ok(())
}

#[test]
fn the_valve_confirm_box_is_its_four_content_lines_plus_borders() -> Result<(), String> {
    let rendered = render(TuiOverlay::ValveConfirm {
        valve: PendingValve::SetAdmission(AdmissionPolicy::Manual),
        answer: String::new(),
    })?;

    assert_eq!(box_height(&rendered, "Valve")?, 6, "\n{rendered}");
    for line in [
        "Target: console-alpha",
        "Policy/mode: manual",
        "Enter to confirm | Esc to cancel",
    ] {
        assert!(rendered.contains(line), "{line}\n{rendered}");
    }
    Ok(())
}
