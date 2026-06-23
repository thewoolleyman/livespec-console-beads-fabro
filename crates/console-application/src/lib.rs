#![forbid(unsafe_code)]

use console_domain::{ConsoleEvent, EventType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttentionItem {
    id: String,
    title: String,
    source: String,
}

impl AttentionItem {
    #[must_use]
    pub const fn new(id: String, title: String, source: String) -> Self {
        Self { id, title, source }
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplicationError {
    EmptyOperatorAction,
}

pub type ApplicationResult<T> = Result<T, ApplicationError>;

#[must_use]
pub fn project_attention(events: &[ConsoleEvent]) -> Vec<AttentionItem> {
    events
        .iter()
        .filter(|event| event.event_type().requires_attention())
        .map(|event| {
            AttentionItem::new(
                event.event_id().to_owned(),
                event.event_type().label().to_owned(),
                event.source().to_owned(),
            )
        })
        .collect()
}

pub fn validate_operator_action(action: &str) -> ApplicationResult<&str> {
    let trimmed = action.trim();
    if trimmed.is_empty() {
        return Err(ApplicationError::EmptyOperatorAction);
    }
    Ok(trimmed)
}

trait AttentionEvent {
    fn requires_attention(&self) -> bool;
    fn label(&self) -> &'static str;
}

impl AttentionEvent for EventType {
    fn requires_attention(&self) -> bool {
        matches!(
            self,
            Self::FabroHumanGateObserved
                | Self::LivespecReviseRequired
                | Self::DispatcherNeedsRegroomObserved
        )
    }

    fn label(&self) -> &'static str {
        match self {
            Self::FabroHumanGateObserved => "Fabro human gate",
            Self::LivespecReviseRequired => "LiveSpec revise required",
            Self::DispatcherNeedsRegroomObserved => "Dispatcher needs-regroom",
            Self::FactoryDrainRequested => "Factory drain requested",
        }
    }
}

#[cfg(test)]
mod tests {
    use console_domain::{ConsoleEvent, EventType};

    use super::{ApplicationError, project_attention, validate_operator_action};

    #[test]
    fn attention_projection_keeps_only_attention_events() {
        let events = [
            ConsoleEvent::fixture("evt_1", EventType::FabroHumanGateObserved, "fabro"),
            ConsoleEvent::fixture("evt_2", EventType::FactoryDrainRequested, "console"),
            ConsoleEvent::fixture("evt_3", EventType::LivespecReviseRequired, "livespec"),
        ];

        let projected = project_attention(&events);

        assert_eq!(projected.len(), 2);
        assert_eq!(projected[0].id(), "evt_1");
        assert_eq!(projected[0].title(), "Fabro human gate");
        assert_eq!(projected[1].source(), "livespec");
    }

    #[test]
    fn operator_action_validation_rejects_empty_input() {
        let result = validate_operator_action("  ");

        assert_eq!(result, Err(ApplicationError::EmptyOperatorAction));
    }

    #[test]
    fn operator_action_validation_trims_valid_input() {
        let result = validate_operator_action("  acknowledge  ");

        assert_eq!(result, Ok("acknowledge"));
    }
}
