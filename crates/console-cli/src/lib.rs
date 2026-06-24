#![forbid(unsafe_code)]

use console_application::build_tui_model;
use console_domain::{CommandEnvelope, ConsoleEvent, EventType};
use console_eventstore::{CommandAppend, CommandAppendOutcome, EventStoreResult, SqliteEventStore};
use console_tui::TuiRuntimeEffect;

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

pub fn persist_tui_runtime_effects(
    store: &mut SqliteEventStore,
    effects: &[TuiRuntimeEffect],
    requested_at: &str,
) -> EventStoreResult<Vec<CommandAppendOutcome>> {
    let mut outcomes = Vec::new();
    for effect in effects {
        let Some(append) = command_append_from_tui_effect(effect, requested_at) else {
            continue;
        };
        outcomes.push(store.append_command(&append)?);
    }
    Ok(outcomes)
}

fn command_append_from_tui_effect(
    effect: &TuiRuntimeEffect,
    requested_at: &str,
) -> Option<CommandAppend> {
    match effect {
        TuiRuntimeEffect::PersistCommand(command) => Some(CommandAppend::new(
            command.clone(),
            requested_at.to_owned(),
            Some(command.aggregate_id().to_owned()),
            command_correlation_id(command),
            "{}".to_owned(),
        )),
        TuiRuntimeEffect::Render
        | TuiRuntimeEffect::OpenAttachCommand(_)
        | TuiRuntimeEffect::CopyAttachCommand(_)
        | TuiRuntimeEffect::Quit
        | TuiRuntimeEffect::ApplicationError(_) => None,
    }
}

fn command_correlation_id(command: &CommandEnvelope) -> String {
    format!("corr_{}", command.command_id())
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
    let events = demo_events();
    let model = build_tui_model(&events, 0);
    render_tui_preview(&model, 100, 28)
}

#[must_use]
pub fn demo_events() -> [ConsoleEvent; 2] {
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

fn render_tui_preview(
    model: &console_application::TuiScreenModel,
    width: u16,
    height: u16,
) -> String {
    match console_tui::render_to_text(model, width, height) {
        Ok(rendered) => rendered,
        Err(_error) => "TUI render error: empty area".to_owned(),
    }
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
    use console_application::{ApplicationError, build_tui_model};
    use console_domain::{CommandEnvelope, CommandType};
    use console_eventstore::{CommandAppendStatus, EventStoreError, SqliteEventStore};
    use console_tui::TuiRuntimeEffect;

    use super::{persist_tui_runtime_effects, render_tui_preview, run};

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
        assert!(output.message().contains("LiveSpec Console"));
        assert!(output.message().contains("> Attention"));
        assert!(output.message().contains("> Fabro human gate"));
        assert!(
            output
                .message()
                .contains("Repo: livespec-console-beads-fabro")
        );
        assert!(output.message().contains("Fabro run: run_demo_1"));
        assert!(output.message().contains("Attach: fabro attach run_demo_1"));
        assert!(
            output
                .message()
                .contains("Actions: Acknowledge, Snooze, Open Fabro")
        );
        assert!(output.message().contains("attach, Copy Fabro attach"));
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

    #[test]
    fn tui_preview_reports_render_errors() {
        let model = build_tui_model(&[], 0);

        assert_eq!(
            render_tui_preview(&model, 0, 28),
            "TUI render error: empty area"
        );
    }

    #[test]
    fn tui_persistence_stores_command_effects() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let effects = [
            TuiRuntimeEffect::OpenAttachCommand("fabro attach run_1".to_owned()),
            TuiRuntimeEffect::PersistCommand(CommandEnvelope::new(
                "cmd_evt_gate_acknowledge_requested".to_owned(),
                CommandType::AttentionAcknowledgeRequested,
                "evt_gate".to_owned(),
                "evt_gate:attention.acknowledge_requested".to_owned(),
                "operator".to_owned(),
            )),
            TuiRuntimeEffect::CopyAttachCommand("fabro attach run_1".to_owned()),
        ];

        let outcomes = persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z")?;
        let commands = store.list_commands()?;

        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].status(), CommandAppendStatus::Inserted);
        assert_eq!(
            outcomes[0].command_id(),
            "cmd_evt_gate_acknowledge_requested"
        );
        assert_eq!(commands.len(), 1);
        assert_eq!(
            commands[0].command_id(),
            "cmd_evt_gate_acknowledge_requested"
        );
        assert_eq!(
            commands[0].command_type(),
            "attention.acknowledge_requested"
        );
        assert_eq!(commands[0].aggregate_id(), Some("evt_gate"));
        assert_eq!(
            commands[0].idempotency_key(),
            "evt_gate:attention.acknowledge_requested"
        );
        assert_eq!(commands[0].requested_by(), "operator");
        assert_eq!(commands[0].status(), "pending");
        Ok(())
    }

    #[test]
    fn tui_persistence_ignores_local_only_effects() -> Result<(), EventStoreError> {
        let mut store = SqliteEventStore::open_in_memory()?;
        let effects = [
            TuiRuntimeEffect::Render,
            TuiRuntimeEffect::OpenAttachCommand("fabro attach run_1".to_owned()),
            TuiRuntimeEffect::CopyAttachCommand("fabro attach run_1".to_owned()),
            TuiRuntimeEffect::ApplicationError(ApplicationError::NoSelectedOperatorAction),
            TuiRuntimeEffect::Quit,
        ];

        let outcomes = persist_tui_runtime_effects(&mut store, &effects, "2026-06-23T00:00:02Z")?;
        let commands = store.list_commands()?;

        assert_eq!(outcomes, []);
        assert_eq!(commands, []);
        Ok(())
    }
}
