//! Console domain model for canonical [`ConsoleEvent`] envelopes, command
//! envelopes, and event types shared by the operator console crates.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Canonical event envelope emitted by source adapters and command handlers.
///
/// The envelope carries the stable event identity, bounded context, typed
/// contract name, stream position, source, and optional persisted JSON payload
/// that projections can later replay.
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
    /// Build an event envelope from its required identity and stream fields.
    ///
    /// The returned event has no payload until [`Self::with_payload_json`] is
    /// used by the event store or an adapter.
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

    /// Build a deterministic fixture event for tests and demos.
    ///
    /// The fixture uses schema version `1`, the `factory` context, the console
    /// factory stream, and stream sequence `1`.
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

    /// Stable event id, unique in the event store.
    #[must_use]
    pub fn event_id(&self) -> &str {
        &self.event_id
    }

    /// Event schema version carried with the envelope.
    #[must_use]
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    /// Bounded context that owns the event contract.
    #[must_use]
    pub fn context(&self) -> &str {
        &self.context
    }

    /// Typed event contract variant.
    #[must_use]
    pub const fn event_type(&self) -> &EventType {
        &self.event_type
    }

    /// Source system or adapter that observed the event.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Stream id used to order related events.
    #[must_use]
    pub fn stream_id(&self) -> &str {
        &self.stream_id
    }

    /// Sequence number within the stream.
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

/// Canonical event contracts the console persists and projects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    /// Work-item lane snapshot observed from the orchestrator.
    WorkItemSnapshotObserved,
    /// Command accepted into the command log.
    CommandAccepted,
    /// Command rejected by application policy.
    CommandRejected,
    /// Dispatcher backlog bounce observed.
    DispatcherBacklogBounceObserved,
    /// Fabro run reached a human gate.
    FabroHumanGateObserved,
    /// Factory drain completed successfully.
    FactoryDrainCompleted,
    /// Factory drain failed.
    FactoryDrainFailed,
    /// Factory drain command could not reach a wired Dispatcher.
    FactoryDrainNotWired,
    /// Operator requested a factory drain.
    FactoryDrainRequested,
    /// Factory drain started.
    FactoryDrainStarted,
    /// A work-item orchestrator action started. Shared across every
    /// `work_item.*` command; the specific action is keyed by the `action_id`
    /// in the event payload (for example `approve:<work-item-id>`).
    WorkItemActionStarted,
    /// A work-item orchestrator action completed, keyed by `action_id`.
    WorkItemActionCompleted,
    /// A work-item orchestrator action failed, keyed by `action_id`.
    WorkItemActionFailed,
    /// A work-item orchestrator action could not reach a wired action surface,
    /// keyed by `action_id`. The honest outcome a simulated or unimplemented
    /// port emits instead of fabricating success.
    WorkItemActionNotWired,
    /// GitHub pull request snapshot observed.
    GithubPullRequestSnapshotObserved,
    /// `LiveSpec` `next` snapshot observed.
    LivespecNextSnapshotObserved,
    /// `LiveSpec` revise action is required.
    LivespecReviseRequired,
    /// Source completeness finding observed.
    SourceCompletenessFindingObserved,
    /// Source could not be observed honestly.
    SourceNotObservedFindingObserved,
    /// Attention item appeared in the product inbox.
    AttentionItemAppeared,
    /// Attention item changed in the product inbox.
    AttentionItemChanged,
    /// Attention item resolved from the product inbox.
    AttentionItemResolved,
    /// One dispatcher policy setting was changed for a repo -- the durable
    /// Configuration audit fact, carrying `{ repo, setting, previous, new, actor,
    /// occurred_at }`, appended only when the write actually landed through the
    /// orchestrator's published command surface.
    ConfigDispatcherSettingChanged,
    /// A `config.dispatcher_setting_set` write was issued but no real
    /// orchestrator command surface is wired, so no setting was changed. The
    /// honest not-wired outcome, never a fabricated success.
    ConfigDispatcherSettingNotWired,
}

impl EventType {
    /// Contract string persisted in the event store.
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
            Self::WorkItemActionStarted => "work_item.action.started",
            Self::WorkItemActionCompleted => "work_item.action.completed",
            Self::WorkItemActionFailed => "work_item.action.failed",
            Self::WorkItemActionNotWired => "work_item.action.not_wired",
            Self::GithubPullRequestSnapshotObserved => "pr.snapshot_observed",
            Self::LivespecNextSnapshotObserved => "spec.next_snapshot_observed",
            Self::LivespecReviseRequired => "spec.revise_required",
            Self::SourceCompletenessFindingObserved => "source.completeness_finding_observed",
            Self::SourceNotObservedFindingObserved => "source.not_observed_finding_observed",
            Self::AttentionItemAppeared => "attention_item.appeared",
            Self::AttentionItemChanged => "attention_item.changed",
            Self::AttentionItemResolved => "attention_item.resolved",
            Self::ConfigDispatcherSettingChanged => "config.dispatcher_setting.changed",
            Self::ConfigDispatcherSettingNotWired => "config.dispatcher_setting.not_wired",
        }
    }

    /// Parse a persisted contract string into an event variant.
    ///
    /// Returns `None` when `value` is not part of the console event vocabulary.
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
            "work_item.action.started" => Some(Self::WorkItemActionStarted),
            "work_item.action.completed" => Some(Self::WorkItemActionCompleted),
            "work_item.action.failed" => Some(Self::WorkItemActionFailed),
            "work_item.action.not_wired" => Some(Self::WorkItemActionNotWired),
            "pr.snapshot_observed" => Some(Self::GithubPullRequestSnapshotObserved),
            "spec.next_snapshot_observed" => Some(Self::LivespecNextSnapshotObserved),
            "spec.revise_required" => Some(Self::LivespecReviseRequired),
            "source.completeness_finding_observed" => Some(Self::SourceCompletenessFindingObserved),
            "source.not_observed_finding_observed" => Some(Self::SourceNotObservedFindingObserved),
            "attention_item.appeared" => Some(Self::AttentionItemAppeared),
            "attention_item.changed" => Some(Self::AttentionItemChanged),
            "attention_item.resolved" => Some(Self::AttentionItemResolved),
            "config.dispatcher_setting.changed" => Some(Self::ConfigDispatcherSettingChanged),
            "config.dispatcher_setting.not_wired" => Some(Self::ConfigDispatcherSettingNotWired),
            _unknown => None,
        }
    }
}

/// Canonical command envelope accepted from operator actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandEnvelope {
    command_id: String,
    command_type: CommandType,
    aggregate_id: String,
    idempotency_key: String,
    requested_by: String,
}

impl CommandEnvelope {
    /// Build a command envelope from its required routing and idempotency data.
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

    /// Stable command id.
    #[must_use]
    pub fn command_id(&self) -> &str {
        &self.command_id
    }

    /// Typed command contract.
    #[must_use]
    pub const fn command_type(&self) -> &CommandType {
        &self.command_type
    }

    /// Aggregate id the command targets.
    #[must_use]
    pub fn aggregate_id(&self) -> &str {
        &self.aggregate_id
    }

    /// Idempotency key used to deduplicate command appends.
    #[must_use]
    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }

    /// Operator or system principal that requested the command.
    #[must_use]
    pub fn requested_by(&self) -> &str {
        &self.requested_by
    }
}

/// Canonical command contracts the console accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandType {
    /// Request to drain ready factory work through the Dispatcher.
    FactoryDrainRequested,
    /// Request to approve a `pending-approval` work-item -- the human approval
    /// act that maps onto the orchestrator's `approve:<work-item-id>` action.
    WorkItemApproveRequested,
    /// Request to accept an `acceptance` work-item -- the human acceptance act
    /// that maps onto the orchestrator's `accept:<work-item-id>` action.
    WorkItemAcceptRequested,
    /// Request to reject a work-item back for rework or regrooming -- maps onto
    /// the orchestrator's `reject:<work-item-id>:<mode>` action, where the
    /// command payload carries `mode` in {rework, regroom}.
    WorkItemRejectRequested,
    /// Request to set a work-item's admission policy dial -- maps onto the
    /// orchestrator's `set-admission:<work-item-id>:<policy>` action, where the
    /// command payload carries `policy` in {auto, manual}. A policy edit never
    /// moves the item between lifecycle states.
    WorkItemSetAdmissionRequested,
    /// Request to set a work-item's acceptance policy dial -- maps onto the
    /// orchestrator's `set-acceptance:<work-item-id>:<policy>` action, where the
    /// command payload carries `policy` in {ai-only, human-only, ai-then-human}.
    /// A policy edit never moves the item between lifecycle states.
    WorkItemSetAcceptanceRequested,
    /// Request to set ONE dispatcher policy setting's global default -- the
    /// Configuration context's per-setting write, whose payload carries
    /// `{ repo, setting, value }`. A single command MUST change exactly one
    /// setting; there is no arming command that flips several at once.
    ConfigDispatcherSettingSet,
    /// The command the console records to reflect one auto-resolution the
    /// orchestrator plane's engine made under full autonomous mode, observed
    /// from that plane's published per-decision audit. Its outcome resolves the
    /// reflected work-item's needs-attention item so it leaves the inbox; the
    /// payload carries `{ work_item_id, gate, decision }`.
    FactoryAutonomousDecisionReflected,
}

impl CommandType {
    /// Contract string persisted in the command log.
    #[must_use]
    pub const fn contract_name(&self) -> &'static str {
        match self {
            Self::FactoryDrainRequested => "factory.drain_requested",
            Self::WorkItemApproveRequested => "work_item.approve_requested",
            Self::WorkItemAcceptRequested => "work_item.accept_requested",
            Self::WorkItemRejectRequested => "work_item.reject_requested",
            Self::WorkItemSetAdmissionRequested => "work_item.set_admission_requested",
            Self::WorkItemSetAcceptanceRequested => "work_item.set_acceptance_requested",
            Self::ConfigDispatcherSettingSet => "config.dispatcher_setting_set",
            Self::FactoryAutonomousDecisionReflected => "factory.autonomous_decision_reflected",
        }
    }

    /// Bounded context that owns the command.
    #[must_use]
    pub const fn context(&self) -> &'static str {
        match self {
            Self::FactoryDrainRequested | Self::FactoryAutonomousDecisionReflected => "factory",
            Self::WorkItemApproveRequested
            | Self::WorkItemAcceptRequested
            | Self::WorkItemRejectRequested
            | Self::WorkItemSetAdmissionRequested
            | Self::WorkItemSetAcceptanceRequested => "work_item",
            Self::ConfigDispatcherSettingSet => "configuration",
        }
    }
}

/// Domain validation failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    /// Required identifier text was empty or whitespace.
    EmptyIdentifier,
    /// Sequence value could not be represented in the expected range.
    InvalidSequence,
}

/// Result alias for domain validation operations.
pub type DomainResult<T> = Result<T, DomainError>;

/// Validate that an identifier contains non-whitespace text.
///
/// Returns the trimmed identifier on success.
///
/// # Errors
/// Returns [`DomainError::EmptyIdentifier`] when `value` is empty after trimming.
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
            EventType::WorkItemActionStarted.contract_name(),
            "work_item.action.started"
        );
        assert_eq!(
            EventType::WorkItemActionCompleted.contract_name(),
            "work_item.action.completed"
        );
        assert_eq!(
            EventType::WorkItemActionFailed.contract_name(),
            "work_item.action.failed"
        );
        assert_eq!(
            EventType::WorkItemActionNotWired.contract_name(),
            "work_item.action.not_wired"
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
        assert_eq!(
            EventType::AttentionItemAppeared.contract_name(),
            "attention_item.appeared"
        );
        assert_eq!(
            EventType::AttentionItemChanged.contract_name(),
            "attention_item.changed"
        );
        assert_eq!(
            EventType::AttentionItemResolved.contract_name(),
            "attention_item.resolved"
        );
    }

    #[test]
    fn dispatcher_setting_event_contract_names_are_stable() {
        assert_eq!(
            EventType::ConfigDispatcherSettingChanged.contract_name(),
            "config.dispatcher_setting.changed"
        );
        assert_eq!(
            EventType::ConfigDispatcherSettingNotWired.contract_name(),
            "config.dispatcher_setting.not_wired"
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
            EventType::WorkItemActionStarted,
            EventType::WorkItemActionCompleted,
            EventType::WorkItemActionFailed,
            EventType::WorkItemActionNotWired,
            EventType::GithubPullRequestSnapshotObserved,
            EventType::LivespecNextSnapshotObserved,
            EventType::LivespecReviseRequired,
            EventType::SourceCompletenessFindingObserved,
            EventType::SourceNotObservedFindingObserved,
            EventType::AttentionItemAppeared,
            EventType::AttentionItemChanged,
            EventType::AttentionItemResolved,
            EventType::ConfigDispatcherSettingChanged,
            EventType::ConfigDispatcherSettingNotWired,
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
        assert_eq!(
            CommandType::WorkItemApproveRequested.contract_name(),
            "work_item.approve_requested"
        );
        assert_eq!(
            CommandType::WorkItemAcceptRequested.contract_name(),
            "work_item.accept_requested"
        );
        assert_eq!(
            CommandType::WorkItemRejectRequested.contract_name(),
            "work_item.reject_requested"
        );
        assert_eq!(
            CommandType::WorkItemSetAdmissionRequested.contract_name(),
            "work_item.set_admission_requested"
        );
        assert_eq!(
            CommandType::WorkItemSetAcceptanceRequested.contract_name(),
            "work_item.set_acceptance_requested"
        );
        assert_eq!(
            CommandType::ConfigDispatcherSettingSet.contract_name(),
            "config.dispatcher_setting_set"
        );
        assert_eq!(
            CommandType::FactoryAutonomousDecisionReflected.contract_name(),
            "factory.autonomous_decision_reflected"
        );
    }

    #[test]
    fn command_type_contexts_are_bounded_context_names() {
        assert_eq!(CommandType::FactoryDrainRequested.context(), "factory");
        assert_eq!(CommandType::WorkItemApproveRequested.context(), "work_item");
        assert_eq!(CommandType::WorkItemAcceptRequested.context(), "work_item");
        assert_eq!(CommandType::WorkItemRejectRequested.context(), "work_item");
        assert_eq!(
            CommandType::WorkItemSetAdmissionRequested.context(),
            "work_item"
        );
        assert_eq!(
            CommandType::WorkItemSetAcceptanceRequested.context(),
            "work_item"
        );
        assert_eq!(
            CommandType::ConfigDispatcherSettingSet.context(),
            "configuration"
        );
        assert_eq!(
            CommandType::FactoryAutonomousDecisionReflected.context(),
            "factory"
        );
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
