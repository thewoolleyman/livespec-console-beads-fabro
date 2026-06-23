#![forbid(unsafe_code)]

use console_application::{AttentionDetail, AttentionItem, TimelineEntry, TuiScreenModel};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Widget, Wrap};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiRenderError {
    EmptyArea,
}

pub type TuiRenderResult<T> = Result<T, TuiRenderError>;

pub fn render_to_text(model: &TuiScreenModel, width: u16, height: u16) -> TuiRenderResult<String> {
    if width == 0 || height == 0 {
        return Err(TuiRenderError::EmptyArea);
    }
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    render_model(model, area, &mut buffer);
    Ok(buffer_to_text(&buffer, area))
}

pub fn render_model(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    if area.is_empty() {
        return;
    }
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(area);
    render_header(model, vertical[0], buffer);
    render_body(model, vertical[1], buffer);
    render_footer(model, vertical[2], buffer);
}

fn render_header(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    Paragraph::new(model.header())
        .block(Block::new().borders(Borders::ALL).title("LiveSpec Console"))
        .render(area, buffer);
}

fn render_body(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(18),
            Constraint::Percentage(38),
            Constraint::Percentage(62),
        ])
        .split(area);
    render_navigation(model, horizontal[0], buffer);
    render_attention(model, horizontal[1], buffer);
    render_detail(model.detail(), horizontal[2], buffer);
}

fn render_footer(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    Paragraph::new(model.footer())
        .block(Block::new().borders(Borders::ALL).title("Status"))
        .render(area, buffer);
}

fn render_navigation(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    let items = model.navigation().iter().map(|view| {
        let label = if *view == model.active_view() {
            format!("> {}", view.label())
        } else {
            format!("  {}", view.label())
        };
        ListItem::new(label)
    });
    List::new(items)
        .block(Block::new().borders(Borders::ALL).title("Views"))
        .render(area, buffer);
}

fn render_attention(model: &TuiScreenModel, area: Rect, buffer: &mut Buffer) {
    let items = model
        .attention_items()
        .iter()
        .enumerate()
        .map(|(index, item)| attention_item_line(model, index, item))
        .collect::<Vec<_>>();
    List::new(items)
        .block(Block::new().borders(Borders::ALL).title("Attention"))
        .render(area, buffer);
}

fn attention_item_line(
    model: &TuiScreenModel,
    index: usize,
    item: &AttentionItem,
) -> ListItem<'static> {
    let marker = if Some(index) == model.selected_attention_index() {
        ">"
    } else {
        " "
    };
    ListItem::new(format!(
        "{marker} {} [{}]",
        item.title(),
        item.next_action().label()
    ))
    .style(if Some(index) == model.selected_attention_index() {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    })
}

fn render_detail(detail: Option<&AttentionDetail>, area: Rect, buffer: &mut Buffer) {
    let lines = detail.map_or_else(
        || vec![Line::from("No attention item selected")],
        detail_lines,
    );
    Paragraph::new(lines)
        .block(Block::new().borders(Borders::ALL).title("Detail"))
        .wrap(Wrap { trim: true })
        .render(area, buffer);
}

fn detail_lines(detail: &AttentionDetail) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(format!("Repo: {}", detail.repo())),
        Line::from(format!("Work item: {}", detail.work_item())),
        Line::from(format!("Fabro run: {}", detail.fabro_run())),
        Line::from(format!("Attach: {}", detail.attach_command())),
        Line::from(format!(
            "Actions: {}",
            detail
                .actions()
                .iter()
                .map(console_application::OperatorAction::label)
                .collect::<Vec<_>>()
                .join(", ")
        )),
        Line::from("Timeline:"),
    ];
    lines.extend(detail.timeline().iter().map(timeline_line));
    lines
}

fn timeline_line(entry: &TimelineEntry) -> Line<'static> {
    Line::from(format!(
        "- {} [{}] {}",
        entry.event_id(),
        entry.source(),
        entry.label()
    ))
}

fn buffer_to_text(buffer: &Buffer, area: Rect) -> String {
    let mut rows = Vec::new();
    for y in area.top()..area.bottom() {
        let mut row = String::new();
        for x in area.left()..area.right() {
            if let Some(cell) = buffer.cell((x, y)) {
                row.push_str(cell.symbol());
            }
        }
        rows.push(row.trim_end().to_owned());
    }
    rows.join("\n")
}

#[cfg(test)]
mod tests {
    use console_application::build_tui_model;
    use console_domain::{ConsoleEvent, EventType};
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    use super::{TuiRenderError, buffer_to_text, render_model, render_to_text};

    #[test]
    fn render_to_text_draws_required_tui_regions() {
        let model = build_tui_model(&demo_events(), 0);

        let output = render_to_text(&model, 96, 24);

        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("LiveSpec Console")),
            Ok(true)
        );
        assert_eq!(
            output.as_ref().map(|rendered| rendered.contains("Views")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("> Attention")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("Fabro human gate")),
            Ok(true)
        );
        assert_eq!(
            output.as_ref().map(|rendered| rendered.contains("Detail")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("Repo: livespec-console-beads-fabro")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("Actions: Acknowledge, Snooze, Open Fabro")),
            Ok(true)
        );
        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("attach, Copy Fabro attach")),
            Ok(true)
        );
        assert_eq!(
            output.as_ref().map(|rendered| rendered.contains("Status")),
            Ok(true)
        );
    }

    #[test]
    fn render_to_text_rejects_empty_area() {
        let model = build_tui_model(&[], 0);

        assert_eq!(
            render_to_text(&model, 0, 24),
            Err(TuiRenderError::EmptyArea)
        );
        assert_eq!(
            render_to_text(&model, 80, 0),
            Err(TuiRenderError::EmptyArea)
        );
    }

    #[test]
    fn render_model_leaves_empty_area_untouched() {
        let model = build_tui_model(&demo_events(), 0);
        let area = Rect::new(0, 0, 20, 5);
        let mut buffer = Buffer::empty(area);
        let before = buffer_to_text(&buffer, area);

        render_model(&model, Rect::new(0, 0, 0, 0), &mut buffer);

        assert_eq!(buffer_to_text(&buffer, area), before);
    }

    #[test]
    fn render_to_text_handles_empty_attention_list() {
        let model = build_tui_model(&[], 0);

        let output = render_to_text(&model, 80, 16);

        assert_eq!(
            output
                .as_ref()
                .map(|rendered| rendered.contains("No attention item selected")),
            Ok(true)
        );
    }

    fn demo_events() -> [ConsoleEvent; 2] {
        [
            ConsoleEvent::new(
                "evt_demo_1".to_owned(),
                1,
                "factory".to_owned(),
                EventType::FabroHumanGateObserved,
                "fabro:run_demo_1".to_owned(),
                "repo:livespec-console-beads-fabro".to_owned(),
                1,
            ),
            ConsoleEvent::new(
                "evt_demo_2".to_owned(),
                1,
                "factory".to_owned(),
                EventType::DispatcherNeedsRegroomObserved,
                "dispatcher".to_owned(),
                "repo:livespec-console-beads-fabro".to_owned(),
                2,
            ),
        ]
    }
}
