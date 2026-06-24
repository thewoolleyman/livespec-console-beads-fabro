use console_domain::{ConsoleEvent, EventType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceAdapterKind {
    Beads,
    Dispatcher,
    Fabro,
    GitHub,
    LiveSpec,
}

impl SourceAdapterKind {
    #[must_use]
    pub const fn source_name(&self) -> &'static str {
        match self {
            Self::Beads => "beads",
            Self::Dispatcher => "dispatcher",
            Self::Fabro => "fabro",
            Self::GitHub => "github",
            Self::LiveSpec => "livespec",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterError {
    AppendFailed,
    CheckpointLoadFailed,
    CheckpointSaveFailed,
    EmptyAdapterId,
    EmptyCheckpoint,
    EmptyDispatchId,
    EmptyObservedAt,
    EmptyRepo,
    EmptyRunId,
    EmptyWorkItemId,
    InvalidPullRequestNumber,
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

pub trait SourceEventAppendPort {
    fn append_normalized_event(
        &mut self,
        event: &NormalizedSourceEvent,
        observed_at: &str,
    ) -> AdapterResult<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterIngestionSummary {
    adapter_id: String,
    previous_checkpoint: Option<String>,
    checkpoint: String,
    appended_event_count: usize,
}

impl AdapterIngestionSummary {
    #[must_use]
    pub const fn new(
        adapter_id: String,
        previous_checkpoint: Option<String>,
        checkpoint: String,
        appended_event_count: usize,
    ) -> Self {
        Self {
            adapter_id,
            previous_checkpoint,
            checkpoint,
            appended_event_count,
        }
    }

    #[must_use]
    pub fn adapter_id(&self) -> &str {
        &self.adapter_id
    }

    #[must_use]
    pub fn previous_checkpoint(&self) -> Option<&str> {
        self.previous_checkpoint.as_deref()
    }

    #[must_use]
    pub fn checkpoint(&self) -> &str {
        &self.checkpoint
    }

    #[must_use]
    pub const fn appended_event_count(&self) -> usize {
        self.appended_event_count
    }
}

pub fn run_adapter_poll(
    adapter_id: &str,
    safety_window: u64,
    observed_at: &str,
    source: &impl PullSourcePort,
    checkpoints: &mut impl SourceCheckpointPort,
    event_log: &mut impl SourceEventAppendPort,
) -> AdapterResult<AdapterIngestionSummary> {
    let adapter_id = required_text(adapter_id, AdapterError::EmptyAdapterId)?;
    let observed_at = required_text(observed_at, AdapterError::EmptyObservedAt)?;
    let previous_checkpoint = checkpoints.load_checkpoint(&adapter_id)?;
    let request =
        AdapterPollRequest::new(&adapter_id, previous_checkpoint.as_deref(), safety_window)?;
    let poll = source.poll(&request)?;
    for event in poll.events() {
        event_log.append_normalized_event(event, &observed_at)?;
    }
    checkpoints.save_checkpoint(&adapter_id, poll.checkpoint())?;
    Ok(AdapterIngestionSummary::new(
        adapter_id,
        previous_checkpoint,
        poll.checkpoint().to_owned(),
        poll.events().len(),
    ))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatcherJournalKind {
    NeedsRegroom,
}

impl DispatcherJournalKind {
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::NeedsRegroom => "needs-regroom",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatcherJournalEntry {
    repo: String,
    work_item_id: String,
    dispatch_id: String,
    kind: DispatcherJournalKind,
    source_version: u64,
}

impl DispatcherJournalEntry {
    pub fn new(
        repo: &str,
        work_item_id: &str,
        dispatch_id: &str,
        kind: DispatcherJournalKind,
        source_version: u64,
    ) -> AdapterResult<Self> {
        if source_version == 0 {
            return Err(AdapterError::InvalidSourceVersion);
        }
        Ok(Self {
            repo: required_text(repo, AdapterError::EmptyRepo)?,
            work_item_id: required_text(work_item_id, AdapterError::EmptyWorkItemId)?,
            dispatch_id: required_text(dispatch_id, AdapterError::EmptyDispatchId)?,
            kind,
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
    pub fn dispatch_id(&self) -> &str {
        &self.dispatch_id
    }

    #[must_use]
    pub const fn kind(&self) -> DispatcherJournalKind {
        self.kind
    }

    #[must_use]
    pub const fn source_version(&self) -> u64 {
        self.source_version
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FabroRunState {
    HumanGate,
}

impl FabroRunState {
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::HumanGate => "human-gate",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FabroRunSnapshot {
    repo: String,
    work_item_id: String,
    run_id: String,
    state: FabroRunState,
    source_version: u64,
}

impl FabroRunSnapshot {
    pub fn new(
        repo: &str,
        work_item_id: &str,
        run_id: &str,
        state: FabroRunState,
        source_version: u64,
    ) -> AdapterResult<Self> {
        if source_version == 0 {
            return Err(AdapterError::InvalidSourceVersion);
        }
        Ok(Self {
            repo: required_text(repo, AdapterError::EmptyRepo)?,
            work_item_id: required_text(work_item_id, AdapterError::EmptyWorkItemId)?,
            run_id: required_text(run_id, AdapterError::EmptyRunId)?,
            state,
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
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    #[must_use]
    pub const fn state(&self) -> FabroRunState {
        self.state
    }

    #[must_use]
    pub const fn source_version(&self) -> u64 {
        self.source_version
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GithubPullRequestState {
    Open,
    ChecksPassing,
    ChecksFailing,
    Merged,
}

impl GithubPullRequestState {
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::ChecksPassing => "checks-passing",
            Self::ChecksFailing => "checks-failing",
            Self::Merged => "merged",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubPullRequestSnapshot {
    repo: String,
    pr_number: u64,
    state: GithubPullRequestState,
    source_version: u64,
}

impl GithubPullRequestSnapshot {
    pub fn new(
        repo: &str,
        pr_number: u64,
        state: GithubPullRequestState,
        source_version: u64,
    ) -> AdapterResult<Self> {
        if pr_number == 0 {
            return Err(AdapterError::InvalidPullRequestNumber);
        }
        if source_version == 0 {
            return Err(AdapterError::InvalidSourceVersion);
        }
        Ok(Self {
            repo: required_text(repo, AdapterError::EmptyRepo)?,
            pr_number,
            state,
            source_version,
        })
    }

    #[must_use]
    pub fn repo(&self) -> &str {
        &self.repo
    }

    #[must_use]
    pub const fn pr_number(&self) -> u64 {
        self.pr_number
    }

    #[must_use]
    pub const fn state(&self) -> GithubPullRequestState {
        self.state
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
    DispatcherJournalEntry(DispatcherJournalEntry),
    FabroRunSnapshot(FabroRunSnapshot),
    GithubPullRequestSnapshot(GithubPullRequestSnapshot),
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

#[must_use]
pub fn normalize_beads_snapshot(snapshot: &BeadsWorkItemSnapshot) -> AdapterPoll {
    let snapshot_event = beads_snapshot_event(snapshot);
    let finding_event = beads_completeness_finding_event(snapshot);
    poll_from_source_version(
        snapshot.source_version(),
        vec![snapshot_event, finding_event],
    )
}

#[must_use]
pub fn normalize_livespec_next_snapshot(snapshot: LivespecNextSnapshot) -> AdapterPoll {
    let source_version = snapshot.source_version();
    let event = livespec_next_event(snapshot);
    poll_from_source_version(source_version, vec![event])
}

#[must_use]
pub fn normalize_dispatcher_journal_entry(entry: DispatcherJournalEntry) -> AdapterPoll {
    let source_version = entry.source_version();
    let event = dispatcher_journal_event(entry);
    poll_from_source_version(source_version, vec![event])
}

#[must_use]
pub fn normalize_fabro_run_snapshot(snapshot: FabroRunSnapshot) -> AdapterPoll {
    let source_version = snapshot.source_version();
    let event = fabro_run_event(snapshot);
    poll_from_source_version(source_version, vec![event])
}

#[must_use]
pub fn normalize_github_pull_request_snapshot(snapshot: GithubPullRequestSnapshot) -> AdapterPoll {
    let source_version = snapshot.source_version();
    let event = github_pull_request_event(snapshot);
    poll_from_source_version(source_version, vec![event])
}

fn poll_from_source_version(
    source_version: u64,
    events: Vec<NormalizedSourceEvent>,
) -> AdapterPoll {
    AdapterPoll {
        checkpoint: source_version.to_string(),
        events,
    }
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

fn dispatcher_journal_event(entry: DispatcherJournalEntry) -> NormalizedSourceEvent {
    NormalizedSourceEvent::new(
        ConsoleEvent::new(
            format!(
                "evt:dispatcher:{}:{}:{}:{}",
                entry.repo(),
                entry.work_item_id(),
                entry.dispatch_id(),
                entry.source_version()
            ),
            1,
            "factory".to_owned(),
            EventType::DispatcherNeedsRegroomObserved,
            SourceAdapterKind::Dispatcher.source_name().to_owned(),
            repo_stream(entry.repo()),
            entry.source_version(),
        ),
        format!(
            "dispatcher:{}:{}:{}:{}",
            entry.repo(),
            entry.work_item_id(),
            entry.dispatch_id(),
            entry.source_version()
        ),
        SourcePayload::DispatcherJournalEntry(entry),
    )
}

fn fabro_run_event(snapshot: FabroRunSnapshot) -> NormalizedSourceEvent {
    NormalizedSourceEvent::new(
        ConsoleEvent::new(
            format!(
                "evt:fabro:{}:{}:{}:{}",
                snapshot.repo(),
                snapshot.work_item_id(),
                snapshot.run_id(),
                snapshot.source_version()
            ),
            1,
            "factory".to_owned(),
            EventType::FabroHumanGateObserved,
            SourceAdapterKind::Fabro.source_name().to_owned(),
            repo_stream(snapshot.repo()),
            snapshot.source_version(),
        ),
        format!(
            "fabro:{}:{}:{}:{}",
            snapshot.repo(),
            snapshot.work_item_id(),
            snapshot.run_id(),
            snapshot.source_version()
        ),
        SourcePayload::FabroRunSnapshot(snapshot),
    )
}

fn github_pull_request_event(snapshot: GithubPullRequestSnapshot) -> NormalizedSourceEvent {
    NormalizedSourceEvent::new(
        ConsoleEvent::new(
            format!(
                "evt:github:{}:pr:{}:{}",
                snapshot.repo(),
                snapshot.pr_number(),
                snapshot.source_version()
            ),
            1,
            "source".to_owned(),
            EventType::GithubPullRequestSnapshotObserved,
            SourceAdapterKind::GitHub.source_name().to_owned(),
            repo_stream(snapshot.repo()),
            snapshot.source_version(),
        ),
        format!(
            "github:{}:pr:{}:{}",
            snapshot.repo(),
            snapshot.pr_number(),
            snapshot.source_version()
        ),
        SourcePayload::GithubPullRequestSnapshot(snapshot),
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
    use std::cell::RefCell;
    use std::rc::Rc;

    use console_domain::EventType;

    use super::{
        AdapterError, AdapterIngestionSummary, AdapterPoll, AdapterPollRequest, AdapterResult,
        BeadsWorkItemSnapshot, BeadsWorkItemStatus, CompletenessFinding, DispatcherJournalEntry,
        DispatcherJournalKind, FabroRunSnapshot, FabroRunState, GithubPullRequestSnapshot,
        GithubPullRequestState, LivespecNextAction, LivespecNextSnapshot, NormalizedSourceEvent,
        PullSourcePort, SourceAdapterKind, SourceCheckpointPort, SourceEventAppendPort,
        SourcePayload, normalize_beads_snapshot, normalize_dispatcher_journal_entry,
        normalize_fabro_run_snapshot, normalize_github_pull_request_snapshot,
        normalize_livespec_next_snapshot, run_adapter_poll,
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
    fn adapter_ingestion_appends_events_before_advancing_checkpoint() {
        let trace = Trace::new();
        let source = ScriptedSource::new(
            trace.clone(),
            AdapterPoll::new("8", vec![beads_snapshot_event_fixture()]),
        );
        let mut checkpoints = MemoryCheckpoints::new(trace.clone(), Some("7"));
        let mut event_log = MemoryEventLog::new(trace.clone(), None);

        let summary = run_adapter_poll(
            " beads:repo ",
            3,
            " 2026-06-24T00:00:00Z ",
            &source,
            &mut checkpoints,
            &mut event_log,
        );

        assert_eq!(
            summary.as_ref().map(AdapterIngestionSummary::adapter_id),
            Ok("beads:repo")
        );
        assert_eq!(
            summary
                .as_ref()
                .map(AdapterIngestionSummary::previous_checkpoint),
            Ok(Some("7"))
        );
        assert_eq!(
            summary.as_ref().map(AdapterIngestionSummary::checkpoint),
            Ok("8")
        );
        assert_eq!(
            summary
                .as_ref()
                .map(AdapterIngestionSummary::appended_event_count),
            Ok(1)
        );
        assert_eq!(
            trace.entries(),
            vec![
                "load:beads:repo".to_owned(),
                "poll:beads:repo:7:3".to_owned(),
                "append:evt:beads:livespec-console-beads-fabro:livespec-console-beads-fabro-y45jhj:7:snapshot:2026-06-24T00:00:00Z"
                    .to_owned(),
                "save:beads:repo:8".to_owned(),
            ]
        );
        assert_eq!(checkpoints.saved(), vec!["beads:repo:8".to_owned()]);
        assert_eq!(
            event_log.appended,
            vec![
                "evt:beads:livespec-console-beads-fabro:livespec-console-beads-fabro-y45jhj:7:snapshot"
                    .to_owned()
            ]
        );
    }

    #[test]
    fn adapter_ingestion_uses_empty_starting_checkpoint() {
        let trace = Trace::new();
        let source = ScriptedSource::new(trace.clone(), AdapterPoll::new("1", Vec::new()));
        let mut checkpoints = MemoryCheckpoints::new(trace.clone(), None);
        let mut event_log = MemoryEventLog::new(trace.clone(), None);

        let summary = run_adapter_poll(
            "github:repo",
            5,
            "2026-06-24T00:00:00Z",
            &source,
            &mut checkpoints,
            &mut event_log,
        );

        assert_eq!(
            summary
                .as_ref()
                .map(AdapterIngestionSummary::previous_checkpoint),
            Ok(None)
        );
        assert_eq!(
            summary
                .as_ref()
                .map(AdapterIngestionSummary::appended_event_count),
            Ok(0)
        );
        assert_eq!(
            trace.entries(),
            vec![
                "load:github:repo".to_owned(),
                "poll:github:repo:none:5".to_owned(),
                "save:github:repo:1".to_owned(),
            ]
        );
    }

    #[test]
    fn adapter_ingestion_does_not_advance_checkpoint_after_append_failure() {
        let trace = Trace::new();
        let source = ScriptedSource::new(
            trace.clone(),
            AdapterPoll::new("8", vec![beads_snapshot_event_fixture()]),
        );
        let mut checkpoints = MemoryCheckpoints::new(trace.clone(), Some("7"));
        let mut event_log = MemoryEventLog::new(trace.clone(), Some(0));

        let summary = run_adapter_poll(
            "beads:repo",
            3,
            "2026-06-24T00:00:00Z",
            &source,
            &mut checkpoints,
            &mut event_log,
        );

        assert_eq!(summary, Err(AdapterError::InvalidSourceVersion));
        assert_eq!(
            trace.entries(),
            vec![
                "load:beads:repo".to_owned(),
                "poll:beads:repo:7:3".to_owned(),
                "append-failed:evt:beads:livespec-console-beads-fabro:livespec-console-beads-fabro-y45jhj:7:snapshot"
                    .to_owned(),
            ]
        );
        assert_eq!(checkpoints.saved(), Vec::<String>::new());
    }

    #[test]
    fn adapter_ingestion_rejects_empty_runner_inputs() {
        let trace = Trace::new();
        let source = ScriptedSource::new(trace.clone(), AdapterPoll::new("1", Vec::new()));
        let mut checkpoints = MemoryCheckpoints::new(trace.clone(), None);
        let mut event_log = MemoryEventLog::new(trace, None);

        assert_eq!(
            run_adapter_poll(
                " ",
                1,
                "2026-06-24T00:00:00Z",
                &source,
                &mut checkpoints,
                &mut event_log,
            ),
            Err(AdapterError::EmptyAdapterId)
        );
        assert_eq!(
            run_adapter_poll(
                "beads:repo",
                1,
                " ",
                &source,
                &mut checkpoints,
                &mut event_log,
            ),
            Err(AdapterError::EmptyObservedAt)
        );
    }

    #[test]
    fn source_kind_and_snapshot_labels_are_stable() {
        assert_eq!(SourceAdapterKind::Beads.source_name(), "beads");
        assert_eq!(SourceAdapterKind::Dispatcher.source_name(), "dispatcher");
        assert_eq!(SourceAdapterKind::Fabro.source_name(), "fabro");
        assert_eq!(SourceAdapterKind::GitHub.source_name(), "github");
        assert_eq!(SourceAdapterKind::LiveSpec.source_name(), "livespec");
        assert_eq!(BeadsWorkItemStatus::Ready.label(), "ready");
        assert_eq!(BeadsWorkItemStatus::Closed.label(), "closed");
        assert_eq!(BeadsWorkItemStatus::NeedsRegroom.label(), "needs-regroom");
        assert_eq!(BeadsWorkItemStatus::Manual.label(), "manual");
        assert_eq!(DispatcherJournalKind::NeedsRegroom.label(), "needs-regroom");
        assert_eq!(FabroRunState::HumanGate.label(), "human-gate");
        assert_eq!(GithubPullRequestState::Open.label(), "open");
        assert_eq!(
            GithubPullRequestState::ChecksPassing.label(),
            "checks-passing"
        );
        assert_eq!(
            GithubPullRequestState::ChecksFailing.label(),
            "checks-failing"
        );
        assert_eq!(GithubPullRequestState::Merged.label(), "merged");
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
    fn dispatcher_entry_validates_source_identity() {
        let entry = DispatcherJournalEntry::new(
            " repo ",
            " item ",
            " dispatch ",
            DispatcherJournalKind::NeedsRegroom,
            9,
        );

        assert_eq!(entry.as_ref().map(DispatcherJournalEntry::repo), Ok("repo"));
        assert_eq!(
            entry.as_ref().map(DispatcherJournalEntry::work_item_id),
            Ok("item")
        );
        assert_eq!(
            entry.as_ref().map(DispatcherJournalEntry::dispatch_id),
            Ok("dispatch")
        );
        assert_eq!(
            entry.as_ref().map(DispatcherJournalEntry::kind),
            Ok(DispatcherJournalKind::NeedsRegroom)
        );
        assert_eq!(
            entry.as_ref().map(DispatcherJournalEntry::source_version),
            Ok(9)
        );
        assert_eq!(
            DispatcherJournalEntry::new(
                " ",
                "item",
                "dispatch",
                DispatcherJournalKind::NeedsRegroom,
                1
            ),
            Err(AdapterError::EmptyRepo)
        );
        assert_eq!(
            DispatcherJournalEntry::new(
                "repo",
                " ",
                "dispatch",
                DispatcherJournalKind::NeedsRegroom,
                1
            ),
            Err(AdapterError::EmptyWorkItemId)
        );
        assert_eq!(
            DispatcherJournalEntry::new(
                "repo",
                "item",
                " ",
                DispatcherJournalKind::NeedsRegroom,
                1
            ),
            Err(AdapterError::EmptyDispatchId)
        );
        assert_eq!(
            DispatcherJournalEntry::new(
                "repo",
                "item",
                "dispatch",
                DispatcherJournalKind::NeedsRegroom,
                0
            ),
            Err(AdapterError::InvalidSourceVersion)
        );
    }

    #[test]
    fn fabro_snapshot_validates_source_identity() {
        let snapshot =
            FabroRunSnapshot::new(" repo ", " item ", " run ", FabroRunState::HumanGate, 11);

        assert_eq!(snapshot.as_ref().map(FabroRunSnapshot::repo), Ok("repo"));
        assert_eq!(
            snapshot.as_ref().map(FabroRunSnapshot::work_item_id),
            Ok("item")
        );
        assert_eq!(snapshot.as_ref().map(FabroRunSnapshot::run_id), Ok("run"));
        assert_eq!(
            snapshot.as_ref().map(FabroRunSnapshot::state),
            Ok(FabroRunState::HumanGate)
        );
        assert_eq!(
            snapshot.as_ref().map(FabroRunSnapshot::source_version),
            Ok(11)
        );
        assert_eq!(
            FabroRunSnapshot::new(" ", "item", "run", FabroRunState::HumanGate, 1),
            Err(AdapterError::EmptyRepo)
        );
        assert_eq!(
            FabroRunSnapshot::new("repo", " ", "run", FabroRunState::HumanGate, 1),
            Err(AdapterError::EmptyWorkItemId)
        );
        assert_eq!(
            FabroRunSnapshot::new("repo", "item", " ", FabroRunState::HumanGate, 1),
            Err(AdapterError::EmptyRunId)
        );
        assert_eq!(
            FabroRunSnapshot::new("repo", "item", "run", FabroRunState::HumanGate, 0),
            Err(AdapterError::InvalidSourceVersion)
        );
    }

    #[test]
    fn github_snapshot_validates_source_identity() {
        let snapshot =
            GithubPullRequestSnapshot::new(" repo ", 42, GithubPullRequestState::ChecksPassing, 13);

        assert_eq!(
            snapshot.as_ref().map(GithubPullRequestSnapshot::repo),
            Ok("repo")
        );
        assert_eq!(
            snapshot.as_ref().map(GithubPullRequestSnapshot::pr_number),
            Ok(42)
        );
        assert_eq!(
            snapshot.as_ref().map(GithubPullRequestSnapshot::state),
            Ok(GithubPullRequestState::ChecksPassing)
        );
        assert_eq!(
            snapshot
                .as_ref()
                .map(GithubPullRequestSnapshot::source_version),
            Ok(13)
        );
        assert_eq!(
            GithubPullRequestSnapshot::new(" ", 42, GithubPullRequestState::Open, 1),
            Err(AdapterError::EmptyRepo)
        );
        assert_eq!(
            GithubPullRequestSnapshot::new("repo", 0, GithubPullRequestState::Open, 1),
            Err(AdapterError::InvalidPullRequestNumber)
        );
        assert_eq!(
            GithubPullRequestSnapshot::new("repo", 42, GithubPullRequestState::Open, 0),
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

        assert_eq!(poll.checkpoint(), "7");
        assert_eq!(poll.events().len(), 2);
        assert_eq!(&poll.events()[0], &beads_snapshot_event_fixture());
        assert_eq!(&poll.events()[1], &beads_completeness_event_fixture());
        assert_eq!(
            poll.events()[0].source_event_id(),
            "beads:livespec-console-beads-fabro:livespec-console-beads-fabro-y45jhj:7:snapshot"
        );
        assert_eq!(
            poll.events()[0].payload(),
            &SourcePayload::BeadsWorkItemSnapshot(beads_snapshot_fixture())
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

        assert_eq!(poll.checkpoint(), "5");
        assert_eq!(poll.events().len(), 1);
        assert_eq!(
            &poll.events()[0],
            &livespec_event_fixture(
                LivespecNextAction::Revise,
                EventType::LivespecReviseRequired
            )
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
                poll.events()[0].event().event_type(),
                &EventType::LivespecNextSnapshotObserved
            );
        }
    }

    #[test]
    fn dispatcher_entry_normalizes_to_needs_regroom_event() {
        let entry = dispatcher_entry_fixture();
        let poll = normalize_dispatcher_journal_entry(entry);

        assert_eq!(poll.checkpoint(), "8");
        assert_eq!(poll.events().len(), 1);
        assert_eq!(&poll.events()[0], &dispatcher_event_fixture());
        assert_eq!(
            poll.events()[0].source_event_id(),
            "dispatcher:livespec-console-beads-fabro:livespec-console-beads-fabro-y45jhj:dispatch_1:8"
        );
        assert_eq!(
            poll.events()[0].payload(),
            &SourcePayload::DispatcherJournalEntry(dispatcher_entry_fixture())
        );
    }

    #[test]
    fn fabro_snapshot_normalizes_to_human_gate_event() {
        let snapshot = fabro_snapshot_fixture();
        let poll = normalize_fabro_run_snapshot(snapshot);

        assert_eq!(poll.checkpoint(), "10");
        assert_eq!(poll.events().len(), 1);
        assert_eq!(&poll.events()[0], &fabro_event_fixture());
        assert_eq!(
            poll.events()[0].source_event_id(),
            "fabro:livespec-console-beads-fabro:livespec-console-beads-fabro-y45jhj:run_1:10"
        );
        assert_eq!(
            poll.events()[0].payload(),
            &SourcePayload::FabroRunSnapshot(fabro_snapshot_fixture())
        );
    }

    #[test]
    fn github_snapshot_normalizes_to_pr_event() {
        let snapshot = github_snapshot_fixture();
        let poll = normalize_github_pull_request_snapshot(snapshot);

        assert_eq!(poll.checkpoint(), "12");
        assert_eq!(poll.events().len(), 1);
        assert_eq!(&poll.events()[0], &github_event_fixture());
        assert_eq!(
            poll.events()[0].source_event_id(),
            "github:livespec-console-beads-fabro:pr:22:12"
        );
        assert_eq!(
            poll.events()[0].payload(),
            &SourcePayload::GithubPullRequestSnapshot(github_snapshot_fixture())
        );
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

    fn dispatcher_entry_fixture() -> DispatcherJournalEntry {
        DispatcherJournalEntry {
            repo: "livespec-console-beads-fabro".to_owned(),
            work_item_id: "livespec-console-beads-fabro-y45jhj".to_owned(),
            dispatch_id: "dispatch_1".to_owned(),
            kind: DispatcherJournalKind::NeedsRegroom,
            source_version: 8,
        }
    }

    fn dispatcher_event_fixture() -> NormalizedSourceEvent {
        NormalizedSourceEvent::new(
            console_domain::ConsoleEvent::new(
                "evt:dispatcher:livespec-console-beads-fabro:livespec-console-beads-fabro-y45jhj:dispatch_1:8"
                    .to_owned(),
                1,
                "factory".to_owned(),
                EventType::DispatcherNeedsRegroomObserved,
                "dispatcher".to_owned(),
                "repo:livespec-console-beads-fabro".to_owned(),
                8,
            ),
            "dispatcher:livespec-console-beads-fabro:livespec-console-beads-fabro-y45jhj:dispatch_1:8"
                .to_owned(),
            SourcePayload::DispatcherJournalEntry(dispatcher_entry_fixture()),
        )
    }

    fn fabro_snapshot_fixture() -> FabroRunSnapshot {
        FabroRunSnapshot {
            repo: "livespec-console-beads-fabro".to_owned(),
            work_item_id: "livespec-console-beads-fabro-y45jhj".to_owned(),
            run_id: "run_1".to_owned(),
            state: FabroRunState::HumanGate,
            source_version: 10,
        }
    }

    fn fabro_event_fixture() -> NormalizedSourceEvent {
        NormalizedSourceEvent::new(
            console_domain::ConsoleEvent::new(
                "evt:fabro:livespec-console-beads-fabro:livespec-console-beads-fabro-y45jhj:run_1:10"
                    .to_owned(),
                1,
                "factory".to_owned(),
                EventType::FabroHumanGateObserved,
                "fabro".to_owned(),
                "repo:livespec-console-beads-fabro".to_owned(),
                10,
            ),
            "fabro:livespec-console-beads-fabro:livespec-console-beads-fabro-y45jhj:run_1:10"
                .to_owned(),
            SourcePayload::FabroRunSnapshot(fabro_snapshot_fixture()),
        )
    }

    fn github_snapshot_fixture() -> GithubPullRequestSnapshot {
        GithubPullRequestSnapshot {
            repo: "livespec-console-beads-fabro".to_owned(),
            pr_number: 22,
            state: GithubPullRequestState::ChecksPassing,
            source_version: 12,
        }
    }

    fn github_event_fixture() -> NormalizedSourceEvent {
        NormalizedSourceEvent::new(
            console_domain::ConsoleEvent::new(
                "evt:github:livespec-console-beads-fabro:pr:22:12".to_owned(),
                1,
                "source".to_owned(),
                EventType::GithubPullRequestSnapshotObserved,
                "github".to_owned(),
                "repo:livespec-console-beads-fabro".to_owned(),
                12,
            ),
            "github:livespec-console-beads-fabro:pr:22:12".to_owned(),
            SourcePayload::GithubPullRequestSnapshot(github_snapshot_fixture()),
        )
    }

    #[derive(Clone)]
    struct Trace {
        entries: Rc<RefCell<Vec<String>>>,
    }

    impl Trace {
        fn new() -> Self {
            Self {
                entries: Rc::new(RefCell::new(Vec::new())),
            }
        }

        fn push(&self, entry: String) {
            self.entries.borrow_mut().push(entry);
        }

        fn entries(&self) -> Vec<String> {
            self.entries.borrow().clone()
        }
    }

    struct ScriptedSource {
        trace: Trace,
        poll: AdapterResult<AdapterPoll>,
    }

    impl ScriptedSource {
        fn new(trace: Trace, poll: AdapterResult<AdapterPoll>) -> Self {
            Self { trace, poll }
        }
    }

    impl PullSourcePort for ScriptedSource {
        fn poll(&self, request: &AdapterPollRequest) -> AdapterResult<AdapterPoll> {
            let checkpoint = request
                .checkpoint()
                .map_or_else(|| "none".to_owned(), str::to_owned);
            self.trace.push(format!(
                "poll:{}:{}:{}",
                request.adapter_id(),
                checkpoint,
                request.safety_window()
            ));
            self.poll.clone()
        }
    }

    struct MemoryCheckpoints {
        trace: Trace,
        checkpoint: Option<String>,
        saved: Rc<RefCell<Vec<String>>>,
    }

    impl MemoryCheckpoints {
        fn new(trace: Trace, checkpoint: Option<&str>) -> Self {
            Self {
                trace,
                checkpoint: checkpoint.map(str::to_owned),
                saved: Rc::new(RefCell::new(Vec::new())),
            }
        }

        fn saved(&self) -> Vec<String> {
            self.saved.borrow().clone()
        }
    }

    impl SourceCheckpointPort for MemoryCheckpoints {
        fn load_checkpoint(&self, adapter_id: &str) -> AdapterResult<Option<String>> {
            self.trace.push(format!("load:{adapter_id}"));
            Ok(self.checkpoint.clone())
        }

        fn save_checkpoint(&self, adapter_id: &str, checkpoint: &str) -> AdapterResult<()> {
            self.trace.push(format!("save:{adapter_id}:{checkpoint}"));
            self.saved
                .borrow_mut()
                .push(format!("{adapter_id}:{checkpoint}"));
            Ok(())
        }
    }

    struct MemoryEventLog {
        trace: Trace,
        fail_after: Option<usize>,
        appended: Vec<String>,
    }

    impl MemoryEventLog {
        fn new(trace: Trace, fail_after: Option<usize>) -> Self {
            Self {
                trace,
                fail_after,
                appended: Vec::new(),
            }
        }
    }

    impl SourceEventAppendPort for MemoryEventLog {
        fn append_normalized_event(
            &mut self,
            event: &NormalizedSourceEvent,
            observed_at: &str,
        ) -> AdapterResult<()> {
            if self.fail_after == Some(self.appended.len()) {
                self.trace
                    .push(format!("append-failed:{}", event.event().event_id()));
                return Err(AdapterError::InvalidSourceVersion);
            }
            self.trace
                .push(format!("append:{}:{observed_at}", event.event().event_id()));
            self.appended.push(event.event().event_id().to_owned());
            Ok(())
        }
    }

    #[test]
    fn adapter_poll_rejects_empty_checkpoint() {
        let poll = AdapterPoll::new(" ", Vec::new());

        assert_eq!(poll, Err(AdapterError::EmptyCheckpoint));
    }
}
