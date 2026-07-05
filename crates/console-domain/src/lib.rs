#![forbid(unsafe_code)]

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleEvent {
    event_id: String,
    schema_version: u16,
    context: String,
    event_type: EventType,
    source: String,
    stream_id: String,
    stream_seq: u64,
    /// Canonical event payload as stored JSON. `None` for an envelope built
    /// in code that carries no payload; the store re-attaches the persisted
    /// `payload_json` column when an event is loaded, so a projection can read
    /// the snapshot an observation captured. The accessor normalizes `None`
    /// to the empty-object literal `{}`.
    payload_json: Option<String>,
}

impl ConsoleEvent {
    #[must_use]
    pub const fn new(
        event_id: String,
        schema_version: u16,
        context: String,
        event_type: EventType,
        source: String,
        stream_id: String,
        stream_seq: u64,
    ) -> Self {
        Self {
            event_id,
            schema_version,
            context,
            event_type,
            source,
            stream_id,
            stream_seq,
            payload_json: None,
        }
    }

    /// Re-attach the persisted `payload_json` to an envelope, used by the
    /// event store when it loads an event so downstream projections can read
    /// the captured payload.
    #[must_use]
    pub fn with_payload_json(mut self, payload_json: String) -> Self {
        self.payload_json = Some(payload_json);
        self
    }

    #[must_use]
    pub fn fixture(event_id: &str, event_type: EventType, source: &str) -> Self {
        Self::new(
            event_id.to_owned(),
            1,
            "factory".to_owned(),
            event_type,
            source.to_owned(),
            "factory:livespec-console-beads-fabro".to_owned(),
            1,
        )
    }

    #[must_use]
    pub fn event_id(&self) -> &str {
        &self.event_id
    }

    #[must_use]
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    #[must_use]
    pub fn context(&self) -> &str {
        &self.context
    }

    #[must_use]
    pub const fn event_type(&self) -> &EventType {
        &self.event_type
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub fn stream_id(&self) -> &str {
        &self.stream_id
    }

    #[must_use]
    pub const fn stream_seq(&self) -> u64 {
        self.stream_seq
    }

    /// The stored event payload as JSON, defaulting to the empty object `{}`
    /// when the envelope carries none.
    #[must_use]
    pub fn payload_json(&self) -> &str {
        self.payload_json.as_deref().unwrap_or("{}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    WorkItemSnapshotObserved,
    CommandAccepted,
    CommandRejected,
    DispatcherBacklogBounceObserved,
    FabroHumanGateObserved,
    FactoryDrainCompleted,
    FactoryDrainFailed,
    FactoryDrainNotWired,
    FactoryDrainRequested,
    FactoryDrainStarted,
    GithubPullRequestSnapshotObserved,
    LivespecNextSnapshotObserved,
    LivespecReviseRequired,
    SourceCompletenessFindingObserved,
    SourceNotObservedFindingObserved,
}

impl EventType {
    #[must_use]
    pub const fn contract_name(&self) -> &'static str {
        match self {
            Self::WorkItemSnapshotObserved => "work_item.snapshot_observed",
            Self::CommandAccepted => "command.accepted",
            Self::CommandRejected => "command.rejected",
            Self::DispatcherBacklogBounceObserved => "dispatch.backlog_bounce_observed",
            Self::FabroHumanGateObserved => "fabro.human_gate_observed",
            Self::FactoryDrainCompleted => "factory.drain.completed",
            Self::FactoryDrainFailed => "factory.drain.failed",
            Self::FactoryDrainNotWired => "factory.drain.not_wired",
            Self::FactoryDrainRequested => "factory.drain_requested",
            Self::FactoryDrainStarted => "factory.drain.started",
            Self::GithubPullRequestSnapshotObserved => "pr.snapshot_observed",
            Self::LivespecNextSnapshotObserved => "spec.next_snapshot_observed",
            Self::LivespecReviseRequired => "spec.revise_required",
            Self::SourceCompletenessFindingObserved => "source.completeness_finding_observed",
            Self::SourceNotObservedFindingObserved => "source.not_observed_finding_observed",
        }
    }

    #[must_use]
    pub fn from_contract_name(value: &str) -> Option<Self> {
        match value {
            "work_item.snapshot_observed" => Some(Self::WorkItemSnapshotObserved),
            "command.accepted" => Some(Self::CommandAccepted),
            "command.rejected" => Some(Self::CommandRejected),
            "dispatch.backlog_bounce_observed" => Some(Self::DispatcherBacklogBounceObserved),
            "fabro.human_gate_observed" => Some(Self::FabroHumanGateObserved),
            "factory.drain.completed" => Some(Self::FactoryDrainCompleted),
            "factory.drain.failed" => Some(Self::FactoryDrainFailed),
            "factory.drain.not_wired" => Some(Self::FactoryDrainNotWired),
            "factory.drain_requested" => Some(Self::FactoryDrainRequested),
            "factory.drain.started" => Some(Self::FactoryDrainStarted),
            "pr.snapshot_observed" => Some(Self::GithubPullRequestSnapshotObserved),
            "spec.next_snapshot_observed" => Some(Self::LivespecNextSnapshotObserved),
            "spec.revise_required" => Some(Self::LivespecReviseRequired),
            "source.completeness_finding_observed" => Some(Self::SourceCompletenessFindingObserved),
            "source.not_observed_finding_observed" => Some(Self::SourceNotObservedFindingObserved),
            _unknown => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandEnvelope {
    command_id: String,
    command_type: CommandType,
    aggregate_id: String,
    idempotency_key: String,
    requested_by: String,
}

impl CommandEnvelope {
    #[must_use]
    pub const fn new(
        command_id: String,
        command_type: CommandType,
        aggregate_id: String,
        idempotency_key: String,
        requested_by: String,
    ) -> Self {
        Self {
            command_id,
            command_type,
            aggregate_id,
            idempotency_key,
            requested_by,
        }
    }

    #[must_use]
    pub fn command_id(&self) -> &str {
        &self.command_id
    }

    #[must_use]
    pub const fn command_type(&self) -> &CommandType {
        &self.command_type
    }

    #[must_use]
    pub fn aggregate_id(&self) -> &str {
        &self.aggregate_id
    }

    #[must_use]
    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }

    #[must_use]
    pub fn requested_by(&self) -> &str {
        &self.requested_by
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandType {
    FactoryDrainRequested,
}

impl CommandType {
    #[must_use]
    pub const fn contract_name(&self) -> &'static str {
        match self {
            Self::FactoryDrainRequested => "factory.drain_requested",
        }
    }

    #[must_use]
    pub const fn context(&self) -> &'static str {
        match self {
            Self::FactoryDrainRequested => "factory",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    EmptyIdentifier,
    InvalidSequence,
}

pub type DomainResult<T> = Result<T, DomainError>;

pub fn validate_non_empty_identifier(value: &str) -> DomainResult<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(DomainError::EmptyIdentifier);
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use proptest::{prop_assert_eq, proptest};

    use super::{
        CommandEnvelope, CommandType, ConsoleEvent, DomainError, EventType,
        validate_non_empty_identifier,
    };

    #[test]
    fn event_envelope_keeps_contract_fields() {
        let event = ConsoleEvent::new(
            "evt_1".to_owned(),
            1,
            "factory".to_owned(),
            EventType::FactoryDrainRequested,
            "console".to_owned(),
            "factory:repo".to_owned(),
            12,
        );

        assert_eq!(event.event_id(), "evt_1");
        assert_eq!(event.schema_version(), 1);
        assert_eq!(event.context(), "factory");
        assert_eq!(event.event_type(), &EventType::FactoryDrainRequested);
        assert_eq!(event.source(), "console");
        assert_eq!(event.stream_id(), "factory:repo");
        assert_eq!(event.stream_seq(), 12);
        assert_eq!(event.payload_json(), "{}");
    }

    #[test]
    fn event_payload_defaults_to_empty_object_and_round_trips() {
        let envelope =
            ConsoleEvent::fixture("evt_1", EventType::WorkItemSnapshotObserved, "source");
        assert_eq!(envelope.payload_json(), "{}");

        let with_payload = envelope.with_payload_json(r#"{"lane":"ready"}"#.to_owned());
        assert_eq!(with_payload.payload_json(), r#"{"lane":"ready"}"#);
    }

    #[test]
    fn event_type_contract_names_are_stable() {
        assert_eq!(
            EventType::WorkItemSnapshotObserved.contract_name(),
            "work_item.snapshot_observed"
        );
        assert_eq!(
            EventType::CommandAccepted.contract_name(),
            "command.accepted"
        );
        assert_eq!(
            EventType::CommandRejected.contract_name(),
            "command.rejected"
        );
        assert_eq!(
            EventType::DispatcherBacklogBounceObserved.contract_name(),
            "dispatch.backlog_bounce_observed"
        );
        assert_eq!(
            EventType::FabroHumanGateObserved.contract_name(),
            "fabro.human_gate_observed"
        );
        assert_eq!(
            EventType::FactoryDrainCompleted.contract_name(),
            "factory.drain.completed"
        );
        assert_eq!(
            EventType::FactoryDrainFailed.contract_name(),
            "factory.drain.failed"
        );
        assert_eq!(
            EventType::FactoryDrainNotWired.contract_name(),
            "factory.drain.not_wired"
        );
        assert_eq!(
            EventType::FactoryDrainRequested.contract_name(),
            "factory.drain_requested"
        );
        assert_eq!(
            EventType::FactoryDrainStarted.contract_name(),
            "factory.drain.started"
        );
        assert_eq!(
            EventType::GithubPullRequestSnapshotObserved.contract_name(),
            "pr.snapshot_observed"
        );
        assert_eq!(
            EventType::LivespecNextSnapshotObserved.contract_name(),
            "spec.next_snapshot_observed"
        );
        assert_eq!(
            EventType::LivespecReviseRequired.contract_name(),
            "spec.revise_required"
        );
        assert_eq!(
            EventType::SourceCompletenessFindingObserved.contract_name(),
            "source.completeness_finding_observed"
        );
        assert_eq!(
            EventType::SourceNotObservedFindingObserved.contract_name(),
            "source.not_observed_finding_observed"
        );
    }

    #[test]
    fn event_type_contract_names_round_trip() {
        for event_type in [
            EventType::WorkItemSnapshotObserved,
            EventType::CommandAccepted,
            EventType::CommandRejected,
            EventType::DispatcherBacklogBounceObserved,
            EventType::FabroHumanGateObserved,
            EventType::FactoryDrainCompleted,
            EventType::FactoryDrainFailed,
            EventType::FactoryDrainNotWired,
            EventType::FactoryDrainRequested,
            EventType::FactoryDrainStarted,
            EventType::GithubPullRequestSnapshotObserved,
            EventType::LivespecNextSnapshotObserved,
            EventType::LivespecReviseRequired,
            EventType::SourceCompletenessFindingObserved,
            EventType::SourceNotObservedFindingObserved,
        ] {
            assert_eq!(
                EventType::from_contract_name(event_type.contract_name()),
                Some(event_type)
            );
        }
        assert_eq!(EventType::from_contract_name("unknown.event"), None);
    }

    #[test]
    fn command_envelope_keeps_intention_fields() {
        let command = CommandEnvelope::new(
            "cmd_1".to_owned(),
            CommandType::FactoryDrainRequested,
            "repo:livespec".to_owned(),
            "repo:livespec:drain:1".to_owned(),
            "operator".to_owned(),
        );

        assert_eq!(command.command_id(), "cmd_1");
        assert_eq!(command.command_type(), &CommandType::FactoryDrainRequested);
        assert_eq!(command.aggregate_id(), "repo:livespec");
        assert_eq!(command.idempotency_key(), "repo:livespec:drain:1");
        assert_eq!(command.requested_by(), "operator");
    }

    #[test]
    fn command_type_contract_names_are_stable() {
        assert_eq!(
            CommandType::FactoryDrainRequested.contract_name(),
            "factory.drain_requested"
        );
    }

    #[test]
    fn command_type_contexts_are_bounded_context_names() {
        assert_eq!(CommandType::FactoryDrainRequested.context(), "factory");
    }

    #[test]
    fn identifier_validation_rejects_blank_values() {
        let result = validate_non_empty_identifier("  ");

        assert_eq!(result, Err(DomainError::EmptyIdentifier));
    }

    #[test]
    fn identifier_validation_trims_valid_values() {
        let result = validate_non_empty_identifier("  evt_1  ");

        assert_eq!(result, Ok("evt_1"));
    }

    proptest! {
        #[test]
        fn identifier_validation_accepts_every_string_with_visible_content(
            leading in "\\s*",
            value in "[[:graph:]]+",
            trailing in "\\s*",
        ) {
            let candidate = format!("{leading}{value}{trailing}");
            let result = validate_non_empty_identifier(&candidate);

            prop_assert_eq!(result, Ok(value.as_str()));
        }

        #[test]
        fn identifier_validation_rejects_every_whitespace_only_string(
            candidate in "\\s*",
        ) {
            let result = validate_non_empty_identifier(&candidate);

            prop_assert_eq!(result, Err(DomainError::EmptyIdentifier));
        }
    }
}
