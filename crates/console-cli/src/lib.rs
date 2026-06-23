#![forbid(unsafe_code)]

use console_application::{
    AttentionDetail, OperatorAction, TimelineEntry, TuiScreenModel, build_tui_model,
};
use console_domain::{ConsoleEvent, EventType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutput {
    code: i32,
    message: String,
}

impl RunOutput {
    #[must_use]
    pub const fn new(code: i32, message: String) -> Self {
        Self { code, message }
    }

    #[must_use]
    pub const fn code(&self) -> i32 {
        self.code
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

pub fn run<I>(args: I) -> RunOutput
where
    I: IntoIterator,
    I::Item: Into<String>,
{
    let mut values = args.into_iter().map(Into::into);
    let _binary_name = values.next();
    let command = values.next();
    match command.as_deref() {
        None | Some("help" | "--help" | "-h") => RunOutput::new(0, help_text()),
        Some("tui") => RunOutput::new(0, tui_preview()),
        Some("serve") => RunOutput::new(0, "serve mode bootstrap: not yet wired".to_owned()),
        Some("backfill") => RunOutput::new(0, "backfill mode bootstrap: not yet wired".to_owned()),
        Some("events") => {
            let subcommand = values.next();
            run_events(subcommand.as_deref())
        }
        Some("snapshot") => RunOutput::new(0, "snapshot mode bootstrap: not yet wired".to_owned()),
        Some("doctor") => RunOutput::new(0, "doctor bootstrap: no findings".to_owned()),
        Some("arch-check") => RunOutput::new(
            0,
            "run `just check-arch` for architecture enforcement".to_owned(),
        ),
        Some(other) => RunOutput::new(2, format!("unknown command: {other}\n\n{}", help_text())),
    }
}

fn run_events(subcommand: Option<&str>) -> RunOutput {
    match subcommand {
        Some("tail") => RunOutput::new(0, "events tail bootstrap: not yet wired".to_owned()),
        _ => RunOutput::new(
            2,
            "usage: livespec-console-beads-fabro events tail".to_owned(),
        ),
    }
}

fn tui_preview() -> String {
    let events = [
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
    ];
    format_tui_model(&build_tui_model(&events, 0))
}

fn format_tui_model(model: &TuiScreenModel) -> String {
    let mut lines = vec![
        model.header().to_owned(),
        format!("view: {}", model.active_view().label()),
        format!(
            "navigation: {}",
            model
                .navigation()
                .iter()
                .map(console_application::TuiView::label)
                .collect::<Vec<_>>()
                .join(" | ")
        ),
        "attention:".to_owned(),
    ];
    lines.extend(
        model
            .attention_items()
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let marker = if Some(index) == model.selected_attention_index() {
                    ">"
                } else {
                    " "
                };
                format!(
                    "{marker} {} [{}] next: {}",
                    item.title(),
                    item.source_reference(),
                    item.next_action().label()
                )
            }),
    );
    if let Some(detail) = model.detail() {
        lines.extend(format_detail(detail));
    }
    lines.push(model.footer().to_owned());
    lines.join("\n")
}

fn format_detail(detail: &AttentionDetail) -> Vec<String> {
    let mut lines = vec![
        "detail:".to_owned(),
        format!("repo: {}", detail.repo()),
        format!("work item: {}", detail.work_item()),
        format!("fabro run: {}", detail.fabro_run()),
        format!("attach: {}", detail.attach_command()),
        format!("actions: {}", format_actions(detail.actions())),
        "timeline:".to_owned(),
    ];
    lines.extend(detail.timeline().iter().map(format_timeline_entry));
    lines
}

fn format_actions(actions: &[OperatorAction]) -> String {
    actions
        .iter()
        .map(OperatorAction::label)
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_timeline_entry(entry: &TimelineEntry) -> String {
    format!(
        "- {} [{}] {}",
        entry.event_id(),
        entry.source(),
        entry.label()
    )
}

fn help_text() -> String {
    [
        "livespec-console-beads-fabro",
        "",
        "Commands:",
        "  tui",
        "  serve",
        "  backfill",
        "  events tail",
        "  snapshot",
        "  doctor",
        "  arch-check",
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn help_lists_specified_command_shape() {
        let output = run(["bin", "help"]);

        assert_eq!(output.code(), 0);
        assert!(output.message().contains("events tail"));
        assert!(output.message().contains("arch-check"));
    }

    #[test]
    fn tui_command_projects_demo_attention_items() {
        let output = run(["bin", "tui"]);

        assert_eq!(output.code(), 0);
        assert!(output.message().contains("view: Attention"));
        assert!(output.message().contains("> Fabro human gate"));
        assert!(
            output
                .message()
                .contains("repo: livespec-console-beads-fabro")
        );
        assert!(output.message().contains("fabro run: run_demo_1"));
        assert!(output.message().contains("attach: fabro attach run_demo_1"));
        assert!(
            output
                .message()
                .contains("actions: Acknowledge, Snooze, Open Fabro attach, Copy Fabro attach")
        );
    }

    #[test]
    fn unknown_command_is_usage_error() {
        let output = run(["bin", "bogus"]);

        assert_eq!(output.code(), 2);
        assert!(output.message().contains("unknown command: bogus"));
    }

    #[test]
    fn no_command_prints_help() {
        let output = run(["bin"]);

        assert_eq!(output.code(), 0);
        assert!(output.message().contains("Commands:"));
    }

    #[test]
    fn bootstrap_commands_report_placeholder_modes() {
        for (command, expected) in [
            ("serve", "serve mode bootstrap: not yet wired"),
            ("backfill", "backfill mode bootstrap: not yet wired"),
            ("snapshot", "snapshot mode bootstrap: not yet wired"),
            ("doctor", "doctor bootstrap: no findings"),
            (
                "arch-check",
                "run `just check-arch` for architecture enforcement",
            ),
        ] {
            let output = run(["bin", command]);

            assert_eq!(output.code(), 0);
            assert_eq!(output.message(), expected);
        }
    }

    #[test]
    fn events_tail_reports_placeholder_mode() {
        let output = run(["bin", "events", "tail"]);

        assert_eq!(output.code(), 0);
        assert_eq!(output.message(), "events tail bootstrap: not yet wired");
    }

    #[test]
    fn events_without_tail_is_usage_error() {
        let output = run(["bin", "events"]);

        assert_eq!(output.code(), 2);
        assert_eq!(
            output.message(),
            "usage: livespec-console-beads-fabro events tail"
        );
    }
}
