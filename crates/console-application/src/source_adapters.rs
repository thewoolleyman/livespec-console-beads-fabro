use console_domain::{ConsoleEvent, EventType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceAdapterKind {
    Beads,
    LiveSpec,
}

impl SourceAdapterKind {
    #[must_use]
    pub const fn source_name(&self) -> &'static str {
        match self {
            Self::Beads => "beads",
            Self::LiveSpec => "livespec",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterError {
    EmptyAdapterId,
    EmptyCheckpoint,
    EmptyRepo,
    EmptyWorkItemId,
    InvalidSourceVersion,
}

pub type AdapterResult<T> = Result<T, AdapterError>;

pub trait PullSourcePort {
    fn poll(&self, request: &AdapterPollRequest) -> AdapterResult<AdapterPoll>;
}

pub trait SourceCheckpointPort {
    fn load_checkpoint(&self, adapter_id: &str) -> AdapterResult<Option<String>>;
    fn save_checkpoint(&self, adapter_id: &str, checkpoint: &str) -> AdapterResult<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterPollRequest {
    adapter_id: String,
    checkpoint: Option<String>,
    safety_window: u64,
}

impl AdapterPollRequest {
    pub fn new(
        adapter_id: &str,
        checkpoint: Option<&str>,
        safety_window: u64,
    ) -> AdapterResult<Self> {
        Ok(Self {
            adapter_id: required_text(adapter_id, AdapterError::EmptyAdapterId)?,
            checkpoint: optional_text(checkpoint, AdapterError::EmptyCheckpoint)?,
            safety_window,
        })
    }

    #[must_use]
    pub fn adapter_id(&self) -> &str {
        &self.adapter_id
    }

    #[must_use]
    pub fn checkpoint(&self) -> Option<&str> {
        self.checkpoint.as_deref()
    }

    #[must_use]
    pub const fn safety_window(&self) -> u64 {
        self.safety_window
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterPoll {
    checkpoint: String,
    events: Vec<NormalizedSourceEvent>,
}

impl AdapterPoll {
    pub fn new(checkpoint: &str, events: Vec<NormalizedSourceEvent>) -> AdapterResult<Self> {
        Ok(Self {
            checkpoint: required_text(checkpoint, AdapterError::EmptyCheckpoint)?,
            events,
        })
    }

    #[must_use]
    pub fn checkpoint(&self) -> &str {
        &self.checkpoint
    }

    #[must_use]
    pub fn events(&self) -> &[NormalizedSourceEvent] {
        &self.events
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeadsWorkItemStatus {
    Ready,
    Closed,
    NeedsRegroom,
    Manual,
}

impl BeadsWorkItemStatus {
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Closed => "closed",
            Self::NeedsRegroom => "needs-regroom",
            Self::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeadsWorkItemSnapshot {
    repo: String,
    work_item_id: String,
    status: BeadsWorkItemStatus,
    source_version: u64,
}

impl BeadsWorkItemSnapshot {
    pub fn new(
        repo: &str,
        work_item_id: &str,
        status: BeadsWorkItemStatus,
        source_version: u64,
    ) -> AdapterResult<Self> {
        if source_version == 0 {
            return Err(AdapterError::InvalidSourceVersion);
        }
        Ok(Self {
            repo: required_text(repo, AdapterError::EmptyRepo)?,
            work_item_id: required_text(work_item_id, AdapterError::EmptyWorkItemId)?,
            status,
            source_version,
        })
    }

    #[must_use]
    pub fn repo(&self) -> &str {
        &self.repo
    }

    #[must_use]
    pub fn work_item_id(&self) -> &str {
        &self.work_item_id
    }

    #[must_use]
    pub const fn status(&self) -> BeadsWorkItemStatus {
        self.status
    }

    #[must_use]
    pub const fn source_version(&self) -> u64 {
        self.source_version
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LivespecNextAction {
    None,
    Critique,
    Revise,
}

impl LivespecNextAction {
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Critique => "critique",
            Self::Revise => "revise",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LivespecNextSnapshot {
    repo: String,
    action: LivespecNextAction,
    source_version: u64,
}

impl LivespecNextSnapshot {
    pub fn new(repo: &str, action: LivespecNextAction, source_version: u64) -> AdapterResult<Self> {
        if source_version == 0 {
            return Err(AdapterError::InvalidSourceVersion);
        }
        Ok(Self {
            repo: required_text(repo, AdapterError::EmptyRepo)?,
            action,
            source_version,
        })
    }

    #[must_use]
    pub fn repo(&self) -> &str {
        &self.repo
    }

    #[must_use]
    pub const fn action(&self) -> LivespecNextAction {
        self.action
    }

    #[must_use]
    pub const fn source_version(&self) -> u64 {
        self.source_version
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletenessFinding {
    repo: String,
    source: SourceAdapterKind,
    message: String,
}

impl CompletenessFinding {
    pub fn new(repo: &str, source: SourceAdapterKind, message: &str) -> AdapterResult<Self> {
        Ok(Self {
            repo: required_text(repo, AdapterError::EmptyRepo)?,
            source,
            message: required_text(message, AdapterError::EmptyCheckpoint)?,
        })
    }

    #[must_use]
    pub fn repo(&self) -> &str {
        &self.repo
    }

    #[must_use]
    pub const fn source(&self) -> SourceAdapterKind {
        self.source
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourcePayload {
    BeadsWorkItemSnapshot(BeadsWorkItemSnapshot),
    CompletenessFinding(CompletenessFinding),
    LivespecNextSnapshot(LivespecNextSnapshot),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedSourceEvent {
    event: ConsoleEvent,
    source_event_id: String,
    payload: SourcePayload,
}

impl NormalizedSourceEvent {
    #[must_use]
    pub const fn new(event: ConsoleEvent, source_event_id: String, payload: SourcePayload) -> Self {
        Self {
            event,
            source_event_id,
            payload,
        }
    }

    #[must_use]
    pub const fn event(&self) -> &ConsoleEvent {
        &self.event
    }

    #[must_use]
    pub fn source_event_id(&self) -> &str {
        &self.source_event_id
    }

    #[must_use]
    pub const fn payload(&self) -> &SourcePayload {
        &self.payload
    }
}

pub fn normalize_beads_snapshot(snapshot: &BeadsWorkItemSnapshot) -> AdapterResult<AdapterPoll> {
    let checkpoint = snapshot.source_version().to_string();
    let snapshot_event = beads_snapshot_event(snapshot);
    let finding_event = beads_completeness_finding_event(snapshot);
    AdapterPoll::new(&checkpoint, vec![snapshot_event, finding_event])
}

pub fn normalize_livespec_next_snapshot(
    snapshot: LivespecNextSnapshot,
) -> AdapterResult<AdapterPoll> {
    let checkpoint = snapshot.source_version().to_string();
    let event = livespec_next_event(snapshot);
    AdapterPoll::new(&checkpoint, vec![event])
}

fn beads_snapshot_event(snapshot: &BeadsWorkItemSnapshot) -> NormalizedSourceEvent {
    NormalizedSourceEvent::new(
        ConsoleEvent::new(
            format!(
                "evt:beads:{}:{}:{}:snapshot",
                snapshot.repo(),
                snapshot.work_item_id(),
                snapshot.source_version()
            ),
            1,
            "factory".to_owned(),
            EventType::BeadsWorkItemSnapshotObserved,
            SourceAdapterKind::Beads.source_name().to_owned(),
            repo_stream(snapshot.repo()),
            snapshot.source_version(),
        ),
        format!(
            "beads:{}:{}:{}:snapshot",
            snapshot.repo(),
            snapshot.work_item_id(),
            snapshot.source_version()
        ),
        SourcePayload::BeadsWorkItemSnapshot(snapshot.clone()),
    )
}

fn beads_completeness_finding_event(snapshot: &BeadsWorkItemSnapshot) -> NormalizedSourceEvent {
    let finding = CompletenessFinding {
        repo: snapshot.repo().to_owned(),
        source: SourceAdapterKind::Beads,
        message: "Beads current-state snapshot cannot prove full transition history".to_owned(),
    };
    NormalizedSourceEvent::new(
        ConsoleEvent::new(
            format!(
                "evt:beads:{}:{}:completeness",
                snapshot.repo(),
                snapshot.source_version()
            ),
            1,
            "source".to_owned(),
            EventType::SourceCompletenessFindingObserved,
            SourceAdapterKind::Beads.source_name().to_owned(),
            repo_stream(snapshot.repo()),
            snapshot.source_version(),
        ),
        format!(
            "beads:{}:{}:completeness",
            snapshot.repo(),
            snapshot.source_version()
        ),
        SourcePayload::CompletenessFinding(finding),
    )
}

fn livespec_next_event(snapshot: LivespecNextSnapshot) -> NormalizedSourceEvent {
    let event_type = match snapshot.action() {
        LivespecNextAction::Revise => EventType::LivespecReviseRequired,
        LivespecNextAction::None | LivespecNextAction::Critique => {
            EventType::LivespecNextSnapshotObserved
        }
    };
    NormalizedSourceEvent::new(
        ConsoleEvent::new(
            format!(
                "evt:livespec:{}:{}:next",
                snapshot.repo(),
                snapshot.source_version()
            ),
            1,
            "spec".to_owned(),
            event_type,
            SourceAdapterKind::LiveSpec.source_name().to_owned(),
            repo_stream(snapshot.repo()),
            snapshot.source_version(),
        ),
        format!(
            "livespec:{}:{}:next",
            snapshot.repo(),
            snapshot.source_version()
        ),
        SourcePayload::LivespecNextSnapshot(snapshot),
    )
}

fn repo_stream(repo: &str) -> String {
    format!("repo:{repo}")
}

fn required_text(value: &str, error: AdapterError) -> AdapterResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(error);
    }
    Ok(trimmed.to_owned())
}

fn optional_text(value: Option<&str>, error: AdapterError) -> AdapterResult<Option<String>> {
    value.map_or(Ok(None), |text| required_text(text, error).map(Some))
}

#[cfg(test)]
mod tests {
    use console_domain::EventType;

    use super::{
        AdapterError, AdapterPoll, AdapterPollRequest, BeadsWorkItemSnapshot, BeadsWorkItemStatus,
        CompletenessFinding, LivespecNextAction, LivespecNextSnapshot, NormalizedSourceEvent,
        SourceAdapterKind, SourcePayload, normalize_beads_snapshot,
        normalize_livespec_next_snapshot,
    };

    #[test]
    fn poll_request_keeps_checkpoint_window() {
        let request = AdapterPollRequest::new("  beads:repo  ", Some(" 42 "), 3);

        assert_eq!(
            request.as_ref().map(AdapterPollRequest::adapter_id),
            Ok("beads:repo")
        );
        assert_eq!(
            request.as_ref().map(AdapterPollRequest::checkpoint),
            Ok(Some("42"))
        );
        assert_eq!(
            request.as_ref().map(AdapterPollRequest::safety_window),
            Ok(3)
        );
    }

    #[test]
    fn poll_request_rejects_empty_inputs() {
        assert_eq!(
            AdapterPollRequest::new(" ", Some("42"), 3),
            Err(AdapterError::EmptyAdapterId)
        );
        assert_eq!(
            AdapterPollRequest::new("beads", Some(" "), 3),
            Err(AdapterError::EmptyCheckpoint)
        );
    }

    #[test]
    fn source_kind_and_snapshot_labels_are_stable() {
        assert_eq!(SourceAdapterKind::Beads.source_name(), "beads");
        assert_eq!(SourceAdapterKind::LiveSpec.source_name(), "livespec");
        assert_eq!(BeadsWorkItemStatus::Ready.label(), "ready");
        assert_eq!(BeadsWorkItemStatus::Closed.label(), "closed");
        assert_eq!(BeadsWorkItemStatus::NeedsRegroom.label(), "needs-regroom");
        assert_eq!(BeadsWorkItemStatus::Manual.label(), "manual");
        assert_eq!(LivespecNextAction::None.label(), "none");
        assert_eq!(LivespecNextAction::Critique.label(), "critique");
        assert_eq!(LivespecNextAction::Revise.label(), "revise");
    }

    #[test]
    fn beads_snapshot_validates_source_identity() {
        let snapshot =
            BeadsWorkItemSnapshot::new(" repo ", " item ", BeadsWorkItemStatus::Manual, 3);
        assert_eq!(
            snapshot.as_ref().map(BeadsWorkItemSnapshot::repo),
            Ok("repo")
        );
        assert_eq!(
            snapshot.as_ref().map(BeadsWorkItemSnapshot::work_item_id),
            Ok("item")
        );
        assert_eq!(
            snapshot.as_ref().map(BeadsWorkItemSnapshot::status),
            Ok(BeadsWorkItemStatus::Manual)
        );
        assert_eq!(
            snapshot.as_ref().map(BeadsWorkItemSnapshot::source_version),
            Ok(3)
        );
        assert_eq!(
            BeadsWorkItemSnapshot::new(" ", "item", BeadsWorkItemStatus::Ready, 1),
            Err(AdapterError::EmptyRepo)
        );
        assert_eq!(
            BeadsWorkItemSnapshot::new("repo", " ", BeadsWorkItemStatus::Ready, 1),
            Err(AdapterError::EmptyWorkItemId)
        );
        assert_eq!(
            BeadsWorkItemSnapshot::new("repo", "item", BeadsWorkItemStatus::Ready, 0),
            Err(AdapterError::InvalidSourceVersion)
        );
    }

    #[test]
    fn livespec_snapshot_validates_source_identity() {
        let snapshot = LivespecNextSnapshot::new(" repo ", LivespecNextAction::Critique, 4);
        assert_eq!(
            snapshot.as_ref().map(LivespecNextSnapshot::repo),
            Ok("repo")
        );
        assert_eq!(
            snapshot.as_ref().map(LivespecNextSnapshot::action),
            Ok(LivespecNextAction::Critique)
        );
        assert_eq!(
            snapshot.as_ref().map(LivespecNextSnapshot::source_version),
            Ok(4)
        );
        assert_eq!(
            LivespecNextSnapshot::new(" ", LivespecNextAction::Revise, 1),
            Err(AdapterError::EmptyRepo)
        );
        assert_eq!(
            LivespecNextSnapshot::new("repo", LivespecNextAction::Revise, 0),
            Err(AdapterError::InvalidSourceVersion)
        );
    }

    #[test]
    fn completeness_finding_keeps_caveat_fields() {
        let finding =
            CompletenessFinding::new(" repo ", SourceAdapterKind::Beads, " snapshot only ");

        assert_eq!(
            finding,
            Ok(CompletenessFinding {
                repo: "repo".to_owned(),
                source: SourceAdapterKind::Beads,
                message: "snapshot only".to_owned(),
            })
        );
        assert_eq!(finding.as_ref().map(CompletenessFinding::repo), Ok("repo"));
        assert_eq!(
            finding.as_ref().map(CompletenessFinding::source),
            Ok(SourceAdapterKind::Beads)
        );
        assert_eq!(
            finding.as_ref().map(CompletenessFinding::message),
            Ok("snapshot only")
        );
        assert_eq!(
            CompletenessFinding::new(" ", SourceAdapterKind::Beads, "snapshot only"),
            Err(AdapterError::EmptyRepo)
        );
        assert_eq!(
            CompletenessFinding::new("repo", SourceAdapterKind::Beads, " "),
            Err(AdapterError::EmptyCheckpoint)
        );
    }

    #[test]
    fn beads_snapshot_normalizes_to_snapshot_and_completeness_events() {
        let snapshot = beads_snapshot_fixture();
        let poll = normalize_beads_snapshot(&snapshot);

        assert_eq!(poll.as_ref().map(AdapterPoll::checkpoint), Ok("7"));
        assert_eq!(poll.as_ref().map(|value| value.events().len()), Ok(2));
        assert_eq!(
            poll.as_ref().map(|value| &value.events()[0]),
            Ok(&beads_snapshot_event_fixture())
        );
        assert_eq!(
            poll.as_ref().map(|value| &value.events()[1]),
            Ok(&beads_completeness_event_fixture())
        );
        assert_eq!(
            poll.as_ref()
                .map(|value| value.events()[0].source_event_id()),
            Ok("beads:livespec-console-beads-fabro:livespec-console-beads-fabro-y45jhj:7:snapshot")
        );
        assert_eq!(
            poll.as_ref().map(|value| value.events()[0].payload()),
            Ok(&SourcePayload::BeadsWorkItemSnapshot(
                beads_snapshot_fixture()
            ))
        );
    }

    fn beads_snapshot_fixture() -> BeadsWorkItemSnapshot {
        BeadsWorkItemSnapshot {
            repo: "livespec-console-beads-fabro".to_owned(),
            work_item_id: "livespec-console-beads-fabro-y45jhj".to_owned(),
            status: BeadsWorkItemStatus::NeedsRegroom,
            source_version: 7,
        }
    }

    fn beads_snapshot_event_fixture() -> NormalizedSourceEvent {
        NormalizedSourceEvent::new(
            console_domain::ConsoleEvent::new(
                "evt:beads:livespec-console-beads-fabro:livespec-console-beads-fabro-y45jhj:7:snapshot"
                    .to_owned(),
                1,
                "factory".to_owned(),
                EventType::BeadsWorkItemSnapshotObserved,
                "beads".to_owned(),
                "repo:livespec-console-beads-fabro".to_owned(),
                7,
            ),
            "beads:livespec-console-beads-fabro:livespec-console-beads-fabro-y45jhj:7:snapshot"
                .to_owned(),
            SourcePayload::BeadsWorkItemSnapshot(beads_snapshot_fixture()),
        )
    }

    fn beads_completeness_event_fixture() -> NormalizedSourceEvent {
        NormalizedSourceEvent::new(
            console_domain::ConsoleEvent::new(
                "evt:beads:livespec-console-beads-fabro:7:completeness".to_owned(),
                1,
                "source".to_owned(),
                EventType::SourceCompletenessFindingObserved,
                "beads".to_owned(),
                "repo:livespec-console-beads-fabro".to_owned(),
                7,
            ),
            "beads:livespec-console-beads-fabro:7:completeness".to_owned(),
            SourcePayload::CompletenessFinding(CompletenessFinding {
                repo: "livespec-console-beads-fabro".to_owned(),
                source: SourceAdapterKind::Beads,
                message: "Beads current-state snapshot cannot prove full transition history"
                    .to_owned(),
            }),
        )
    }

    #[test]
    fn livespec_revise_snapshot_normalizes_to_attention_event() {
        let snapshot = livespec_snapshot_fixture(LivespecNextAction::Revise);

        let poll = normalize_livespec_next_snapshot(snapshot);

        assert_eq!(poll.as_ref().map(AdapterPoll::checkpoint), Ok("5"));
        assert_eq!(poll.as_ref().map(|value| value.events().len()), Ok(1));
        assert_eq!(
            poll.as_ref().map(|value| &value.events()[0]),
            Ok(&livespec_event_fixture(
                LivespecNextAction::Revise,
                EventType::LivespecReviseRequired
            ))
        );
    }

    #[test]
    fn non_revise_livespec_snapshots_keep_snapshot_event_type() {
        for action in [LivespecNextAction::None, LivespecNextAction::Critique] {
            let snapshot = LivespecNextSnapshot {
                repo: "repo".to_owned(),
                action,
                source_version: 2,
            };

            let poll = normalize_livespec_next_snapshot(snapshot);

            assert_eq!(
                poll.as_ref()
                    .map(|value| value.events()[0].event().event_type()),
                Ok(&EventType::LivespecNextSnapshotObserved)
            );
        }
    }

    fn livespec_snapshot_fixture(action: LivespecNextAction) -> LivespecNextSnapshot {
        LivespecNextSnapshot {
            repo: "livespec-console-beads-fabro".to_owned(),
            action,
            source_version: 5,
        }
    }

    fn livespec_event_fixture(
        action: LivespecNextAction,
        event_type: EventType,
    ) -> NormalizedSourceEvent {
        NormalizedSourceEvent::new(
            console_domain::ConsoleEvent::new(
                "evt:livespec:livespec-console-beads-fabro:5:next".to_owned(),
                1,
                "spec".to_owned(),
                event_type,
                "livespec".to_owned(),
                "repo:livespec-console-beads-fabro".to_owned(),
                5,
            ),
            "livespec:livespec-console-beads-fabro:5:next".to_owned(),
            SourcePayload::LivespecNextSnapshot(livespec_snapshot_fixture(action)),
        )
    }

    #[test]
    fn adapter_poll_rejects_empty_checkpoint() {
        let poll = AdapterPoll::new(" ", Vec::new());

        assert_eq!(poll, Err(AdapterError::EmptyCheckpoint));
    }
}
