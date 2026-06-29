use console_domain::{ConsoleEvent, EventType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceAdapterKind {
    Orchestrator,
    Dispatcher,
    Fabro,
    GitHub,
    LiveSpec,
}

impl SourceAdapterKind {
    #[must_use]
    pub const fn source_name(&self) -> &'static str {
        match self {
            Self::Orchestrator => "orchestrator",
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
    source: &(impl PullSourcePort + ?Sized),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lane {
    Backlog,
    PendingApproval,
    Ready,
    Active,
    Acceptance,
    Blocked,
    Done,
}

impl Lane {
    /// The seven lanes in canonical lifecycle order (backlog → done); the
    /// lane board renders its columns in this order.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Backlog,
            Self::PendingApproval,
            Self::Ready,
            Self::Active,
            Self::Acceptance,
            Self::Blocked,
            Self::Done,
        ]
    }

    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Backlog => "backlog",
            Self::PendingApproval => "pending-approval",
            Self::Ready => "ready",
            Self::Active => "active",
            Self::Acceptance => "acceptance",
            Self::Blocked => "blocked",
            Self::Done => "done",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LaneReason {
    NeedsHuman,
    InfraExternal,
    Dependency,
}

impl LaneReason {
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::NeedsHuman => "needs-human",
            Self::InfraExternal => "infra-external",
            Self::Dependency => "dependency",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkItemSnapshot {
    repo: String,
    work_item_id: String,
    lane: Lane,
    lane_reason: Option<LaneReason>,
    rank: String,
    status: String,
    source_version: u64,
}

impl WorkItemSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo: &str,
        work_item_id: &str,
        lane: Lane,
        lane_reason: Option<LaneReason>,
        rank: &str,
        status: &str,
        source_version: u64,
    ) -> AdapterResult<Self> {
        if source_version == 0 {
            return Err(AdapterError::InvalidSourceVersion);
        }
        Ok(Self {
            repo: required_text(repo, AdapterError::EmptyRepo)?,
            work_item_id: required_text(work_item_id, AdapterError::EmptyWorkItemId)?,
            lane,
            lane_reason,
            rank: rank.to_owned(),
            status: status.to_owned(),
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
    pub const fn lane(&self) -> Lane {
        self.lane
    }

    #[must_use]
    pub const fn lane_reason(&self) -> Option<LaneReason> {
        self.lane_reason
    }

    /// The first-class fractional `rank` emitted by the orchestrator (a
    /// lexicographically-ordered key; the bottom sentinel `~` sorts last).
    /// The lane board orders each lane's items by this key.
    #[must_use]
    pub fn rank(&self) -> &str {
        &self.rank
    }

    /// The stored 7-state lifecycle status emitted alongside the derived
    /// `lane` (carried verbatim; never used to re-derive a lane).
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    #[must_use]
    pub const fn source_version(&self) -> u64 {
        self.source_version
    }
}

/// Bottom-of-list sentinel for a missing `rank`, matching the orchestrator's
/// own fractional-indexing bottom key so a rank-less item sorts last.
fn rank_bottom_sentinel() -> String {
    "~".to_owned()
}

/// The persisted JSON shape a work-item snapshot observation reads back as.
///
/// Owned here so the wire format ingestion writes into `payload_json` is the
/// exact shape a projection reads back. Robust to an absent `rank` / `status`
/// (defaulting to the bottom sentinel / empty) so a leaner emission still
/// round-trips.
#[derive(serde::Deserialize)]
struct WorkItemSnapshotPayload {
    repo: String,
    work_item_id: String,
    lane: Lane,
    #[serde(default)]
    lane_reason: Option<LaneReason>,
    #[serde(default = "rank_bottom_sentinel")]
    rank: String,
    #[serde(default)]
    status: String,
    source_version: u64,
}

/// Serialize a work-item snapshot into its canonical persisted `payload_json`.
///
/// Written with the observation event so a projection can rebuild from it.
/// Built directly as a [`serde_json::Value`] — whose `to_string` is the
/// infallible `Display` — over the same field names and kebab-case lane
/// encodings the typed [`WorkItemSnapshotPayload`] reads back, so the
/// round-trip is total and carries no unreachable failure arm.
#[must_use]
pub fn work_item_snapshot_payload_json(snapshot: &WorkItemSnapshot) -> String {
    let lane_reason = snapshot
        .lane_reason
        .map_or(serde_json::Value::Null, |reason| {
            serde_json::Value::String(reason.label().to_owned())
        });
    let mut object = serde_json::Map::new();
    object.insert("repo".to_owned(), snapshot.repo.clone().into());
    object.insert(
        "work_item_id".to_owned(),
        snapshot.work_item_id.clone().into(),
    );
    object.insert("lane".to_owned(), snapshot.lane.label().into());
    object.insert("lane_reason".to_owned(), lane_reason);
    object.insert("rank".to_owned(), snapshot.rank.clone().into());
    object.insert("status".to_owned(), snapshot.status.clone().into());
    object.insert("source_version".to_owned(), snapshot.source_version.into());
    serde_json::Value::Object(object).to_string()
}

/// Rebuild a work-item snapshot from a persisted `payload_json`.
///
/// Returns `None` for any payload that is not a valid, complete snapshot (an
/// empty object, a different event's payload, or a corrupt cache row) so the
/// lane reduction skips it instead of failing.
#[must_use]
pub fn work_item_snapshot_from_payload_json(payload_json: &str) -> Option<WorkItemSnapshot> {
    let payload: WorkItemSnapshotPayload = serde_json::from_str(payload_json).ok()?;
    WorkItemSnapshot::new(
        &payload.repo,
        &payload.work_item_id,
        payload.lane,
        payload.lane_reason,
        &payload.rank,
        &payload.status,
        payload.source_version,
    )
    .ok()
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
    WorkItemSnapshot(WorkItemSnapshot),
    CompletenessFinding(CompletenessFinding),
    DispatcherJournalEntry(DispatcherJournalEntry),
    FabroRunSnapshot(FabroRunSnapshot),
    GithubPullRequestSnapshot(GithubPullRequestSnapshot),
    LivespecNextSnapshot(LivespecNextSnapshot),
    NotObservedFinding(NotObservedFinding),
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
pub fn normalize_work_item_snapshot(snapshot: &WorkItemSnapshot) -> AdapterPoll {
    let snapshot_event = work_item_snapshot_event(snapshot);
    let finding_event = work_item_completeness_finding_event(snapshot);
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

fn work_item_snapshot_event(snapshot: &WorkItemSnapshot) -> NormalizedSourceEvent {
    NormalizedSourceEvent::new(
        ConsoleEvent::new(
            format!(
                "evt:orchestrator:{}:{}:{}:snapshot",
                snapshot.repo(),
                snapshot.work_item_id(),
                snapshot.source_version()
            ),
            1,
            "factory".to_owned(),
            EventType::WorkItemSnapshotObserved,
            SourceAdapterKind::Orchestrator.source_name().to_owned(),
            repo_stream(snapshot.repo()),
            snapshot.source_version(),
        ),
        format!(
            "orchestrator:{}:{}:{}:snapshot",
            snapshot.repo(),
            snapshot.work_item_id(),
            snapshot.source_version()
        ),
        SourcePayload::WorkItemSnapshot(snapshot.clone()),
    )
}

fn work_item_completeness_finding_event(snapshot: &WorkItemSnapshot) -> NormalizedSourceEvent {
    let finding = CompletenessFinding {
        repo: snapshot.repo().to_owned(),
        source: SourceAdapterKind::Orchestrator,
        message: "Work-item current-state snapshot cannot prove full transition history".to_owned(),
    };
    NormalizedSourceEvent::new(
        ConsoleEvent::new(
            format!(
                "evt:orchestrator:{}:{}:completeness",
                snapshot.repo(),
                snapshot.source_version()
            ),
            1,
            "source".to_owned(),
            EventType::SourceCompletenessFindingObserved,
            SourceAdapterKind::Orchestrator.source_name().to_owned(),
            repo_stream(snapshot.repo()),
            snapshot.source_version(),
        ),
        format!(
            "orchestrator:{}:{}:completeness",
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

/// Checkpoint stored for a source that could not be observed on a cold start
/// (no previous checkpoint to carry forward). It does not advance any real
/// source position; it only records that the adapter ran and observed nothing.
const NOT_OBSERVED_CHECKPOINT: &str = "not_observed";

/// Outcome of probing a real source instance.
///
/// `Observed` carries the raw payload (CLI stdout or file contents) and whether
/// the probe reported success; `Unavailable` carries an honest reason the source
/// could not be reached (binary missing, spawn error, file absent).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceProbeOutcome {
    Observed { stdout: String, success: bool },
    Unavailable { reason: String },
}

impl SourceProbeOutcome {
    #[must_use]
    pub fn observed(stdout: &str, success: bool) -> Self {
        Self::Observed {
            stdout: stdout.to_owned(),
            success,
        }
    }

    #[must_use]
    pub fn unavailable(reason: &str) -> Self {
        Self::Unavailable {
            reason: reason.to_owned(),
        }
    }
}

/// Capability for observing a real source through the host.
///
/// Runs a stable CLI or reads a file. UI/domain code must never call sources
/// directly; adapters reach them only through this port. The concrete
/// host-backed implementation lives in the binary (`console-cli` `main.rs`),
/// outside the covered library surface.
pub trait SourceProbe {
    fn run_command(&self, program: &str, args: &[&str]) -> SourceProbeOutcome;
    fn read_file(&self, path: &str) -> SourceProbeOutcome;
}

/// How a given adapter observes its source instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceObservationPlan {
    Command { program: String, args: Vec<String> },
    File { path: String },
}

impl SourceObservationPlan {
    #[must_use]
    pub fn command(program: &str, args: &[&str]) -> Self {
        Self::Command {
            program: program.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        }
    }

    #[must_use]
    pub fn file(path: &str) -> Self {
        Self::File {
            path: path.to_owned(),
        }
    }
}

/// A successful raw observation handed to a source-specific normalizer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedSource {
    source: SourceAdapterKind,
    repo: String,
    stdout: String,
}

impl ObservedSource {
    #[must_use]
    pub fn new(source: SourceAdapterKind, repo: &str, stdout: &str) -> Self {
        Self {
            source,
            repo: repo.to_owned(),
            stdout: stdout.to_owned(),
        }
    }

    #[must_use]
    pub const fn source(&self) -> SourceAdapterKind {
        self.source
    }

    #[must_use]
    pub fn repo(&self) -> &str {
        &self.repo
    }

    #[must_use]
    pub fn stdout(&self) -> &str {
        &self.stdout
    }
}

/// Result of normalizing a real observation into canonical events plus the
/// checkpoint that identifies the observed state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedObservation {
    checkpoint: String,
    events: Vec<NormalizedSourceEvent>,
}

impl ParsedObservation {
    #[must_use]
    pub fn new(checkpoint: &str, events: Vec<NormalizedSourceEvent>) -> Self {
        Self {
            checkpoint: checkpoint.to_owned(),
            events,
        }
    }
}

/// Source-specific normalizer.
///
/// Turns a raw observation into canonical events, or returns an honest reason
/// the observation could not be interpreted (which the adapter records as a
/// not-observed finding rather than fabricating data).
pub type NormalizeObservation = fn(&ObservedSource) -> Result<ParsedObservation, String>;

/// A pull adapter that observes a real source through a [`SourceProbe`].
///
/// On a successful, interpretable observation it emits the normalized events.
/// On an unavailable source, a non-zero probe, or an uninterpretable payload it
/// emits an honest `source.not_observed_finding_observed` event and carries the
/// previous checkpoint forward instead of advancing it or fabricating a
/// snapshot.
pub struct ObservedSourceAdapter<'a> {
    probe: &'a dyn SourceProbe,
    source: SourceAdapterKind,
    repo: String,
    plan: SourceObservationPlan,
    normalize: NormalizeObservation,
}

impl<'a> ObservedSourceAdapter<'a> {
    pub fn new(
        probe: &'a dyn SourceProbe,
        source: SourceAdapterKind,
        repo: &str,
        plan: SourceObservationPlan,
        normalize: NormalizeObservation,
    ) -> AdapterResult<Self> {
        Ok(Self {
            probe,
            source,
            repo: required_text(repo, AdapterError::EmptyRepo)?,
            plan,
            normalize,
        })
    }

    fn observe(&self) -> SourceProbeOutcome {
        match &self.plan {
            SourceObservationPlan::Command { program, args } => {
                let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
                self.probe.run_command(program, &arg_refs)
            }
            SourceObservationPlan::File { path } => self.probe.read_file(path),
        }
    }

    fn not_observed_poll(&self, previous_checkpoint: Option<&str>, reason: &str) -> AdapterPoll {
        let checkpoint = previous_checkpoint
            .map_or_else(|| NOT_OBSERVED_CHECKPOINT.to_owned(), ToOwned::to_owned);
        AdapterPoll {
            checkpoint,
            events: vec![not_observed_event(self.source, &self.repo, reason)],
        }
    }
}

impl PullSourcePort for ObservedSourceAdapter<'_> {
    fn poll(&self, request: &AdapterPollRequest) -> AdapterResult<AdapterPoll> {
        let previous = request.checkpoint();
        match self.observe() {
            SourceProbeOutcome::Observed {
                stdout,
                success: true,
            } => {
                let observed = ObservedSource::new(self.source, &self.repo, &stdout);
                match (self.normalize)(&observed) {
                    Ok(parsed) if !parsed.events.is_empty() => {
                        AdapterPoll::new(&parsed.checkpoint, parsed.events)
                    }
                    Ok(_empty) => {
                        Ok(self.not_observed_poll(previous, "source produced no records"))
                    }
                    Err(reason) => Ok(self.not_observed_poll(previous, &reason)),
                }
            }
            SourceProbeOutcome::Observed { success: false, .. } => {
                Ok(self.not_observed_poll(previous, "source command exited non-zero"))
            }
            SourceProbeOutcome::Unavailable { reason } => {
                Ok(self.not_observed_poll(previous, &reason))
            }
        }
    }
}

/// Honest finding that a source could not be observed this poll.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotObservedFinding {
    repo: String,
    source: SourceAdapterKind,
    reason: String,
}

impl NotObservedFinding {
    #[must_use]
    pub fn new(repo: &str, source: SourceAdapterKind, reason: &str) -> Self {
        Self {
            repo: repo.to_owned(),
            source,
            reason: reason.to_owned(),
        }
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
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

fn not_observed_event(
    source: SourceAdapterKind,
    repo: &str,
    reason: &str,
) -> NormalizedSourceEvent {
    let finding = NotObservedFinding::new(repo, source, reason);
    NormalizedSourceEvent::new(
        ConsoleEvent::new(
            format!("evt:{}:{repo}:not_observed", source.source_name()),
            1,
            "source".to_owned(),
            EventType::SourceNotObservedFindingObserved,
            source.source_name().to_owned(),
            repo_stream(repo),
            1,
        ),
        format!("{}:{repo}:not_observed", source.source_name()),
        SourcePayload::NotObservedFinding(finding),
    )
}

// --- Real source normalizers ------------------------------------------------
//
// Each normalizer interprets the raw payload from one source's stable CLI/file
// into canonical snapshot events. Inputs come from the orchestrator's
// `list-work-items`, `gh`, the Dispatcher journal, `fabro`, and `livespec`; an
// uninterpretable payload
// returns an honest reason so the adapter records a not-observed finding
// instead of fabricating a snapshot. JSON is read with minimal flat-field
// extraction rather than a dependency, since only a few identifying fields are
// needed and any malformed shape degrades to a not-observed finding.

/// Extract the first `"key": "value"` string value from flat JSON-ish text.
fn first_json_string(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let start = text.find(&needle)? + needle.len();
    let after_colon = text[start..].trim_start().strip_prefix(':')?.trim_start();
    let body = after_colon.strip_prefix('"')?;
    let end = body.find('"')?;
    Some(body[..end].to_owned())
}

/// Extract the first `"key": <number>` unsigned value from flat JSON-ish text.
fn first_json_u64(text: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{key}\"");
    let start = text.find(&needle)? + needle.len();
    let after_colon = text[start..].trim_start().strip_prefix(':')?.trim_start();
    let digits: String = after_colon
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// Stable, non-zero version token for an observed state, so re-observing the
/// same state yields the same source-event identity (idempotent) while a real
/// change yields a new one. FNV-1a over the identifying fields; no dependency.
fn stable_version(parts: &[&str]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for byte in part.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= u64::from(b'\x1f');
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash | 1
}

/// Normalize real orchestrator `list-work-items --json` output into one
/// work-item snapshot per item, consuming each item's emitted `lane` and
/// `lane_reason` directly rather than re-deriving a lane.
pub fn parse_orchestrator_observation(
    observed: &ObservedSource,
) -> Result<ParsedObservation, String> {
    #[derive(serde::Deserialize)]
    struct WorkItemRecord {
        id: String,
        lane: Lane,
        #[serde(default)]
        lane_reason: Option<LaneReason>,
        #[serde(default = "rank_bottom_sentinel")]
        rank: String,
        #[serde(default)]
        status: String,
    }

    let items: Vec<WorkItemRecord> = serde_json::from_str(observed.stdout())
        .map_err(|_error| "orchestrator list-work-items output is not a JSON array".to_owned())?;
    if items.is_empty() {
        return Err("no work-items observed".to_owned());
    }
    let mut events = Vec::new();
    let mut versions = Vec::new();
    for item in items {
        // rank and status join lane/lane_reason in the identity hash so a
        // re-rank or status transition appends a fresh observation the lane
        // board can pick up.
        let version = stable_version(&[
            observed.repo(),
            &item.id,
            item.lane.label(),
            item.lane_reason.map_or("", |reason| reason.label()),
            &item.rank,
            &item.status,
        ]);
        let snapshot = WorkItemSnapshot::new(
            observed.repo(),
            &item.id,
            item.lane,
            item.lane_reason,
            &item.rank,
            &item.status,
            version,
        )
        .map_err(|_error| "invalid work-item".to_owned())?;
        events.extend(normalize_work_item_snapshot(&snapshot).events().to_vec());
        versions.push(version.to_string());
    }
    let checkpoint =
        stable_version(&versions.iter().map(String::as_str).collect::<Vec<_>>()).to_string();
    Ok(ParsedObservation::new(&checkpoint, events))
}

/// Normalize real `gh pr list --json ...` output into a GitHub PR snapshot.
pub fn parse_github_observation(observed: &ObservedSource) -> Result<ParsedObservation, String> {
    let number = first_json_u64(observed.stdout(), "number")
        .ok_or_else(|| "no pull request observed".to_owned())?;
    let state_text = first_json_string(observed.stdout(), "state").unwrap_or_default();
    let state = match state_text.as_str() {
        "MERGED" => GithubPullRequestState::Merged,
        "CLOSED" => GithubPullRequestState::ChecksFailing,
        _other => GithubPullRequestState::Open,
    };
    let version = stable_version(&[observed.repo(), &number.to_string(), state.label()]);
    let snapshot = GithubPullRequestSnapshot::new(observed.repo(), number, state, version)
        .map_err(|_error| "invalid pull request".to_owned())?;
    let poll = normalize_github_pull_request_snapshot(snapshot);
    Ok(ParsedObservation::new(
        &version.to_string(),
        poll.events().to_vec(),
    ))
}

/// Normalize the last real Dispatcher journal JSONL line into a dispatch event.
pub fn parse_dispatcher_observation(
    observed: &ObservedSource,
) -> Result<ParsedObservation, String> {
    let line = observed
        .stdout()
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| "empty dispatcher journal".to_owned())?;
    let work_item_id = first_json_string(line, "work_item_id")
        .ok_or_else(|| "no work-item in journal entry".to_owned())?;
    let dispatch_id = first_json_string(line, "dispatch_id")
        .ok_or_else(|| "no dispatch id in journal entry".to_owned())?;
    let version = stable_version(&[&work_item_id, &dispatch_id]);
    let entry = DispatcherJournalEntry::new(
        observed.repo(),
        &work_item_id,
        &dispatch_id,
        DispatcherJournalKind::NeedsRegroom,
        version,
    )
    .map_err(|_error| "invalid journal entry".to_owned())?;
    let poll = normalize_dispatcher_journal_entry(entry);
    Ok(ParsedObservation::new(
        &version.to_string(),
        poll.events().to_vec(),
    ))
}

/// Normalize real `fabro ps`/run output into a Fabro run snapshot.
pub fn parse_fabro_observation(observed: &ObservedSource) -> Result<ParsedObservation, String> {
    let run_id = first_json_string(observed.stdout(), "run_id")
        .or_else(|| first_json_string(observed.stdout(), "id"))
        .ok_or_else(|| "no fabro run observed".to_owned())?;
    let work_item_id =
        first_json_string(observed.stdout(), "work_item_id").unwrap_or_else(|| run_id.clone());
    let version = stable_version(&[&run_id, &work_item_id]);
    let snapshot = FabroRunSnapshot::new(
        observed.repo(),
        &work_item_id,
        &run_id,
        FabroRunState::HumanGate,
        version,
    )
    .map_err(|_error| "invalid fabro run".to_owned())?;
    let poll = normalize_fabro_run_snapshot(snapshot);
    Ok(ParsedObservation::new(
        &version.to_string(),
        poll.events().to_vec(),
    ))
}

/// Normalize real `livespec next` output into a `LivespecNextSnapshot`.
pub fn parse_livespec_observation(observed: &ObservedSource) -> Result<ParsedObservation, String> {
    let action_text = first_json_string(observed.stdout(), "action")
        .or_else(|| first_json_string(observed.stdout(), "next"))
        .ok_or_else(|| "no livespec next action observed".to_owned())?;
    let action = match action_text.as_str() {
        "revise" => LivespecNextAction::Revise,
        "critique" => LivespecNextAction::Critique,
        _other => LivespecNextAction::None,
    };
    let version = stable_version(&[observed.repo(), action.label()]);
    let snapshot = LivespecNextSnapshot::new(observed.repo(), action, version)
        .map_err(|_error| "invalid livespec snapshot".to_owned())?;
    let poll = normalize_livespec_next_snapshot(snapshot);
    Ok(ParsedObservation::new(
        &version.to_string(),
        poll.events().to_vec(),
    ))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use console_domain::EventType;

    use super::{
        AdapterError, AdapterIngestionSummary, AdapterPoll, AdapterPollRequest, AdapterResult,
        CompletenessFinding, DispatcherJournalEntry, DispatcherJournalKind, FabroRunSnapshot,
        FabroRunState, GithubPullRequestSnapshot, GithubPullRequestState, Lane, LaneReason,
        LivespecNextAction, LivespecNextSnapshot, NormalizedSourceEvent, NotObservedFinding,
        ObservedSource, ObservedSourceAdapter, ParsedObservation, PullSourcePort,
        SourceAdapterKind, SourceCheckpointPort, SourceEventAppendPort, SourceObservationPlan,
        SourcePayload, SourceProbe, SourceProbeOutcome, WorkItemSnapshot,
        normalize_dispatcher_journal_entry, normalize_fabro_run_snapshot,
        normalize_github_pull_request_snapshot, normalize_livespec_next_snapshot,
        normalize_work_item_snapshot, parse_dispatcher_observation, parse_fabro_observation,
        parse_github_observation, parse_livespec_observation, parse_orchestrator_observation,
        run_adapter_poll, work_item_snapshot_from_payload_json, work_item_snapshot_payload_json,
    };

    #[test]
    fn poll_request_keeps_checkpoint_window() {
        let request = AdapterPollRequest::new("  orchestrator:repo  ", Some(" 42 "), 3);

        assert_eq!(
            request.as_ref().map(AdapterPollRequest::adapter_id),
            Ok("orchestrator:repo")
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
            AdapterPollRequest::new("orchestrator", Some(" "), 3),
            Err(AdapterError::EmptyCheckpoint)
        );
    }

    struct StubProbe {
        command_outcome: SourceProbeOutcome,
        file_outcome: SourceProbeOutcome,
        calls: RefCell<Vec<String>>,
    }

    impl StubProbe {
        fn command(outcome: SourceProbeOutcome) -> Self {
            Self {
                command_outcome: outcome,
                file_outcome: SourceProbeOutcome::unavailable("no file plan in this stub"),
                calls: RefCell::new(Vec::new()),
            }
        }

        fn file(outcome: SourceProbeOutcome) -> Self {
            Self {
                command_outcome: SourceProbeOutcome::unavailable("no command plan in this stub"),
                file_outcome: outcome,
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl SourceProbe for StubProbe {
        fn run_command(&self, program: &str, args: &[&str]) -> SourceProbeOutcome {
            self.calls
                .borrow_mut()
                .push(format!("cmd:{program} {}", args.join(" ")));
            self.command_outcome.clone()
        }

        fn read_file(&self, path: &str) -> SourceProbeOutcome {
            self.calls.borrow_mut().push(format!("file:{path}"));
            self.file_outcome.clone()
        }
    }

    // Test normalizer: a non-empty payload normalizes into a work-item snapshot
    // poll; the literal "empty" yields zero events; blank input is an error.
    fn stub_normalize(observed: &ObservedSource) -> Result<ParsedObservation, String> {
        let trimmed = observed.stdout().trim();
        if trimmed.is_empty() {
            return Err("blank observation".to_owned());
        }
        if trimmed == "empty" {
            return Ok(ParsedObservation::new("v-empty", Vec::new()));
        }
        // "broken" drives the builder-error branch (empty work-item id).
        let work_item_id = if trimmed == "broken" { "" } else { trimmed };
        let snapshot = WorkItemSnapshot::new(
            observed.repo(),
            work_item_id,
            Lane::Ready,
            None,
            "a0",
            "ready",
            1,
        )
        .map_err(|_error| "snapshot build failed".to_owned())?;
        let poll = normalize_work_item_snapshot(&snapshot);
        Ok(ParsedObservation::new(
            "ck-observed",
            poll.events().to_vec(),
        ))
    }

    fn orchestrator_command_adapter(probe: &StubProbe) -> AdapterResult<ObservedSourceAdapter<'_>> {
        ObservedSourceAdapter::new(
            probe,
            SourceAdapterKind::Orchestrator,
            "console",
            SourceObservationPlan::command("list-work-items", &["--json"]),
            stub_normalize,
        )
    }

    fn dispatcher_file_adapter(probe: &StubProbe) -> AdapterResult<ObservedSourceAdapter<'_>> {
        ObservedSourceAdapter::new(
            probe,
            SourceAdapterKind::Dispatcher,
            "console",
            SourceObservationPlan::file("/var/log/dispatcher.jsonl"),
            stub_normalize,
        )
    }

    fn cold_request() -> AdapterResult<AdapterPollRequest> {
        AdapterPollRequest::new("orchestrator:console", None, 1)
    }

    #[test]
    fn observed_source_exposes_fields() {
        let observed = ObservedSource::new(SourceAdapterKind::Orchestrator, "console", "work-1");

        assert_eq!(observed.source(), SourceAdapterKind::Orchestrator);
        assert_eq!(observed.repo(), "console");
        assert_eq!(observed.stdout(), "work-1");
    }

    #[test]
    fn observation_plan_constructors_capture_inputs() {
        assert_eq!(
            SourceObservationPlan::command("list-work-items", &["--json"]),
            SourceObservationPlan::Command {
                program: "list-work-items".to_owned(),
                args: vec!["--json".to_owned()],
            }
        );
        assert_eq!(
            SourceObservationPlan::file("/tmp/journal.jsonl"),
            SourceObservationPlan::File {
                path: "/tmp/journal.jsonl".to_owned(),
            }
        );
    }

    #[test]
    fn not_observed_finding_exposes_fields() {
        let finding = NotObservedFinding::new("console", SourceAdapterKind::Fabro, "fabro absent");

        assert_eq!(finding.repo(), "console");
        assert_eq!(finding.source(), SourceAdapterKind::Fabro);
        assert_eq!(finding.reason(), "fabro absent");
    }

    #[test]
    fn observed_source_adapter_rejects_empty_repo() {
        let probe = StubProbe::command(SourceProbeOutcome::observed("work-1", true));

        let adapter = ObservedSourceAdapter::new(
            &probe,
            SourceAdapterKind::Orchestrator,
            "  ",
            SourceObservationPlan::command("list-work-items", &["--json"]),
            stub_normalize,
        );

        assert!(matches!(adapter, Err(AdapterError::EmptyRepo)));
    }

    #[test]
    fn observed_source_adapter_emits_parsed_events_on_success() -> AdapterResult<()> {
        let probe = StubProbe::command(SourceProbeOutcome::observed("work-1", true));
        let adapter = orchestrator_command_adapter(&probe)?;

        let poll = adapter.poll(&cold_request()?)?;

        assert_eq!(poll.checkpoint(), "ck-observed");
        assert_eq!(poll.events().len(), 2);
        assert_eq!(
            poll.events()[0].event().event_type(),
            &EventType::WorkItemSnapshotObserved
        );
        assert_eq!(
            probe.calls.borrow().as_slice(),
            ["cmd:list-work-items --json"]
        );
        Ok(())
    }

    #[test]
    fn observed_source_adapter_reads_file_plan() -> AdapterResult<()> {
        let probe = StubProbe::file(SourceProbeOutcome::observed("work-1", true));
        let adapter = dispatcher_file_adapter(&probe)?;

        let poll = adapter.poll(&cold_request()?)?;

        assert_eq!(poll.checkpoint(), "ck-observed");
        assert_eq!(
            probe.calls.borrow().as_slice(),
            ["file:/var/log/dispatcher.jsonl"]
        );
        Ok(())
    }

    fn assert_not_observed(poll: &AdapterPoll, expected_reason: &str) {
        assert_eq!(poll.events().len(), 1);
        let event = &poll.events()[0];
        assert_eq!(
            event.event().event_type(),
            &EventType::SourceNotObservedFindingObserved
        );
        assert_eq!(
            event.payload(),
            &SourcePayload::NotObservedFinding(NotObservedFinding::new(
                "console",
                SourceAdapterKind::Orchestrator,
                expected_reason,
            ))
        );
    }

    #[test]
    fn observed_source_adapter_emits_not_observed_when_unavailable() -> AdapterResult<()> {
        let probe = StubProbe::command(SourceProbeOutcome::unavailable("orchestrator not found"));
        let adapter = orchestrator_command_adapter(&probe)?;

        let poll = adapter.poll(&cold_request()?)?;

        assert_eq!(poll.checkpoint(), "not_observed");
        assert_not_observed(&poll, "orchestrator not found");
        Ok(())
    }

    #[test]
    fn observed_source_adapter_carries_previous_checkpoint_on_not_observed() -> AdapterResult<()> {
        let probe = StubProbe::command(SourceProbeOutcome::unavailable("orchestrator not found"));
        let adapter = orchestrator_command_adapter(&probe)?;
        let request = AdapterPollRequest::new("orchestrator:console", Some("prior-checkpoint"), 1)?;

        let poll = adapter.poll(&request)?;

        assert_eq!(poll.checkpoint(), "prior-checkpoint");
        assert_not_observed(&poll, "orchestrator not found");
        Ok(())
    }

    #[test]
    fn observed_source_adapter_emits_not_observed_on_non_zero_exit() -> AdapterResult<()> {
        let probe = StubProbe::command(SourceProbeOutcome::observed("ignored", false));
        let adapter = orchestrator_command_adapter(&probe)?;

        let poll = adapter.poll(&cold_request()?)?;

        assert_not_observed(&poll, "source command exited non-zero");
        Ok(())
    }

    #[test]
    fn observed_source_adapter_emits_not_observed_on_empty_parse() -> AdapterResult<()> {
        let probe = StubProbe::command(SourceProbeOutcome::observed("empty", true));
        let adapter = orchestrator_command_adapter(&probe)?;

        let poll = adapter.poll(&cold_request()?)?;

        assert_not_observed(&poll, "source produced no records");
        Ok(())
    }

    #[test]
    fn observed_source_adapter_emits_not_observed_on_parse_error() -> AdapterResult<()> {
        let probe = StubProbe::command(SourceProbeOutcome::observed("   ", true));
        let adapter = orchestrator_command_adapter(&probe)?;

        let poll = adapter.poll(&cold_request()?)?;

        assert_not_observed(&poll, "blank observation");
        Ok(())
    }

    #[test]
    fn observed_source_adapter_emits_not_observed_on_builder_error() -> AdapterResult<()> {
        let probe = StubProbe::command(SourceProbeOutcome::observed("broken", true));
        let adapter = orchestrator_command_adapter(&probe)?;

        let poll = adapter.poll(&cold_request()?)?;

        assert_not_observed(&poll, "snapshot build failed");
        Ok(())
    }

    #[test]
    fn adapter_ingestion_appends_events_before_advancing_checkpoint() {
        let trace = Trace::new();
        let source = ScriptedSource::new(
            trace.clone(),
            AdapterPoll::new("8", vec![work_item_snapshot_event_fixture()]),
        );
        let mut checkpoints = MemoryCheckpoints::new(trace.clone(), Some("7"));
        let mut event_log = MemoryEventLog::new(trace.clone(), None);

        let summary = run_adapter_poll(
            " orchestrator:repo ",
            3,
            " 2026-06-24T00:00:00Z ",
            &source,
            &mut checkpoints,
            &mut event_log,
        );

        assert_eq!(
            summary.as_ref().map(AdapterIngestionSummary::adapter_id),
            Ok("orchestrator:repo")
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
                "load:orchestrator:repo".to_owned(),
                "poll:orchestrator:repo:7:3".to_owned(),
                "append:evt:orchestrator:livespec-console-beads-fabro:livespec-console-beads-fabro-y45jhj:7:snapshot:2026-06-24T00:00:00Z"
                    .to_owned(),
                "save:orchestrator:repo:8".to_owned(),
            ]
        );
        assert_eq!(checkpoints.saved(), vec!["orchestrator:repo:8".to_owned()]);
        assert_eq!(
            event_log.appended,
            vec![
                "evt:orchestrator:livespec-console-beads-fabro:livespec-console-beads-fabro-y45jhj:7:snapshot"
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
            AdapterPoll::new("8", vec![work_item_snapshot_event_fixture()]),
        );
        let mut checkpoints = MemoryCheckpoints::new(trace.clone(), Some("7"));
        let mut event_log = MemoryEventLog::new(trace.clone(), Some(0));

        let summary = run_adapter_poll(
            "orchestrator:repo",
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
                "load:orchestrator:repo".to_owned(),
                "poll:orchestrator:repo:7:3".to_owned(),
                "append-failed:evt:orchestrator:livespec-console-beads-fabro:livespec-console-beads-fabro-y45jhj:7:snapshot"
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
                "orchestrator:repo",
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
        assert_eq!(
            SourceAdapterKind::Orchestrator.source_name(),
            "orchestrator"
        );
        assert_eq!(SourceAdapterKind::Dispatcher.source_name(), "dispatcher");
        assert_eq!(SourceAdapterKind::Fabro.source_name(), "fabro");
        assert_eq!(SourceAdapterKind::GitHub.source_name(), "github");
        assert_eq!(SourceAdapterKind::LiveSpec.source_name(), "livespec");
        assert_eq!(Lane::Backlog.label(), "backlog");
        assert_eq!(Lane::PendingApproval.label(), "pending-approval");
        assert_eq!(Lane::Ready.label(), "ready");
        assert_eq!(Lane::Active.label(), "active");
        assert_eq!(Lane::Acceptance.label(), "acceptance");
        assert_eq!(Lane::Blocked.label(), "blocked");
        assert_eq!(Lane::Done.label(), "done");
        assert_eq!(LaneReason::NeedsHuman.label(), "needs-human");
        assert_eq!(LaneReason::InfraExternal.label(), "infra-external");
        assert_eq!(LaneReason::Dependency.label(), "dependency");
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
    fn work_item_snapshot_validates_source_identity() {
        let snapshot = WorkItemSnapshot::new(
            " repo ",
            " item ",
            Lane::Blocked,
            Some(LaneReason::NeedsHuman),
            "a5",
            "blocked",
            3,
        );
        assert_eq!(snapshot.as_ref().map(WorkItemSnapshot::repo), Ok("repo"));
        assert_eq!(
            snapshot.as_ref().map(WorkItemSnapshot::work_item_id),
            Ok("item")
        );
        assert_eq!(
            snapshot.as_ref().map(WorkItemSnapshot::lane),
            Ok(Lane::Blocked)
        );
        assert_eq!(
            snapshot.as_ref().map(WorkItemSnapshot::lane_reason),
            Ok(Some(LaneReason::NeedsHuman))
        );
        assert_eq!(snapshot.as_ref().map(WorkItemSnapshot::rank), Ok("a5"));
        assert_eq!(
            snapshot.as_ref().map(WorkItemSnapshot::status),
            Ok("blocked")
        );
        assert_eq!(
            snapshot.as_ref().map(WorkItemSnapshot::source_version),
            Ok(3)
        );
        assert_eq!(
            WorkItemSnapshot::new(" ", "item", Lane::Ready, None, "a0", "ready", 1),
            Err(AdapterError::EmptyRepo)
        );
        assert_eq!(
            WorkItemSnapshot::new("repo", " ", Lane::Ready, None, "a0", "ready", 1),
            Err(AdapterError::EmptyWorkItemId)
        );
        assert_eq!(
            WorkItemSnapshot::new("repo", "item", Lane::Ready, None, "a0", "ready", 0),
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
            CompletenessFinding::new(" repo ", SourceAdapterKind::Orchestrator, " snapshot only ");

        assert_eq!(
            finding,
            Ok(CompletenessFinding {
                repo: "repo".to_owned(),
                source: SourceAdapterKind::Orchestrator,
                message: "snapshot only".to_owned(),
            })
        );
        assert_eq!(finding.as_ref().map(CompletenessFinding::repo), Ok("repo"));
        assert_eq!(
            finding.as_ref().map(CompletenessFinding::source),
            Ok(SourceAdapterKind::Orchestrator)
        );
        assert_eq!(
            finding.as_ref().map(CompletenessFinding::message),
            Ok("snapshot only")
        );
        assert_eq!(
            CompletenessFinding::new(" ", SourceAdapterKind::Orchestrator, "snapshot only"),
            Err(AdapterError::EmptyRepo)
        );
        assert_eq!(
            CompletenessFinding::new("repo", SourceAdapterKind::Orchestrator, " "),
            Err(AdapterError::EmptyCheckpoint)
        );
    }

    #[test]
    fn work_item_snapshot_normalizes_to_snapshot_and_completeness_events() {
        let snapshot = work_item_snapshot_fixture();
        let poll = normalize_work_item_snapshot(&snapshot);

        assert_eq!(poll.checkpoint(), "7");
        assert_eq!(poll.events().len(), 2);
        assert_eq!(&poll.events()[0], &work_item_snapshot_event_fixture());
        assert_eq!(&poll.events()[1], &work_item_completeness_event_fixture());
        assert_eq!(
            poll.events()[0].source_event_id(),
            "orchestrator:livespec-console-beads-fabro:livespec-console-beads-fabro-y45jhj:7:snapshot"
        );
        assert_eq!(
            poll.events()[0].payload(),
            &SourcePayload::WorkItemSnapshot(work_item_snapshot_fixture())
        );
    }

    fn work_item_snapshot_fixture() -> WorkItemSnapshot {
        WorkItemSnapshot {
            repo: "livespec-console-beads-fabro".to_owned(),
            work_item_id: "livespec-console-beads-fabro-y45jhj".to_owned(),
            lane: Lane::Blocked,
            lane_reason: Some(LaneReason::NeedsHuman),
            rank: "a8".to_owned(),
            status: "blocked".to_owned(),
            source_version: 7,
        }
    }

    fn work_item_snapshot_event_fixture() -> NormalizedSourceEvent {
        NormalizedSourceEvent::new(
            console_domain::ConsoleEvent::new(
                "evt:orchestrator:livespec-console-beads-fabro:livespec-console-beads-fabro-y45jhj:7:snapshot"
                    .to_owned(),
                1,
                "factory".to_owned(),
                EventType::WorkItemSnapshotObserved,
                "orchestrator".to_owned(),
                "repo:livespec-console-beads-fabro".to_owned(),
                7,
            ),
            "orchestrator:livespec-console-beads-fabro:livespec-console-beads-fabro-y45jhj:7:snapshot"
                .to_owned(),
            SourcePayload::WorkItemSnapshot(work_item_snapshot_fixture()),
        )
    }

    fn work_item_completeness_event_fixture() -> NormalizedSourceEvent {
        NormalizedSourceEvent::new(
            console_domain::ConsoleEvent::new(
                "evt:orchestrator:livespec-console-beads-fabro:7:completeness".to_owned(),
                1,
                "source".to_owned(),
                EventType::SourceCompletenessFindingObserved,
                "orchestrator".to_owned(),
                "repo:livespec-console-beads-fabro".to_owned(),
                7,
            ),
            "orchestrator:livespec-console-beads-fabro:7:completeness".to_owned(),
            SourcePayload::CompletenessFinding(CompletenessFinding {
                repo: "livespec-console-beads-fabro".to_owned(),
                source: SourceAdapterKind::Orchestrator,
                message: "Work-item current-state snapshot cannot prove full transition history"
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

    // --- Real normalizer tests ----------------------------------------------

    fn observed_for(source: SourceAdapterKind, repo: &str, stdout: &str) -> ObservedSource {
        ObservedSource::new(source, repo, stdout)
    }

    fn first_payload(parsed: &ParsedObservation) -> &SourcePayload {
        parsed.events[0].payload()
    }

    #[test]
    fn parse_orchestrator_consumes_emitted_lanes() -> Result<(), String> {
        let stdout = "[{\"id\":\"livespec-console-beads-fabro-a1\",\"lane\":\"ready\",\"lane_reason\":null},\
                      {\"id\":\"livespec-console-beads-fabro-b2\",\"lane\":\"blocked\",\"lane_reason\":\"needs-human\"}]";
        let parsed = parse_orchestrator_observation(&observed_for(
            SourceAdapterKind::Orchestrator,
            "livespec-console-beads-fabro",
            stdout,
        ))?;

        // The emitted lane/lane_reason are consumed directly into the snapshot
        // payloads (never re-derived from any other field).
        let snapshots: Vec<&WorkItemSnapshot> = parsed
            .events
            .iter()
            .filter_map(|event| match event.payload() {
                SourcePayload::WorkItemSnapshot(snapshot) => Some(snapshot),
                _other => None,
            })
            .collect();

        // One observed snapshot event per work-item.
        assert_eq!(snapshots.len(), 2);
        assert_eq!(
            snapshots[0].work_item_id(),
            "livespec-console-beads-fabro-a1"
        );
        assert_eq!(snapshots[0].lane(), Lane::Ready);
        assert_eq!(snapshots[0].lane_reason(), None);
        assert_eq!(
            snapshots[1].work_item_id(),
            "livespec-console-beads-fabro-b2"
        );
        assert_eq!(snapshots[1].lane(), Lane::Blocked);
        assert_eq!(snapshots[1].lane_reason(), Some(LaneReason::NeedsHuman));
        for snapshot_event in parsed.events.iter().step_by(2) {
            assert_eq!(
                snapshot_event.event().event_type(),
                &EventType::WorkItemSnapshotObserved
            );
        }
        Ok(())
    }

    #[test]
    fn parse_orchestrator_reports_empty_and_malformed() {
        assert_eq!(
            parse_orchestrator_observation(&observed_for(
                SourceAdapterKind::Orchestrator,
                "console",
                "[]"
            )),
            Err("no work-items observed".to_owned())
        );
        assert_eq!(
            parse_orchestrator_observation(&observed_for(
                SourceAdapterKind::Orchestrator,
                "console",
                "not json at all",
            )),
            Err("orchestrator list-work-items output is not a JSON array".to_owned())
        );
        assert_eq!(
            parse_orchestrator_observation(&observed_for(
                SourceAdapterKind::Orchestrator,
                "console",
                r#"[{"id":"","lane":"ready","lane_reason":null}]"#,
            )),
            Err("invalid work-item".to_owned())
        );
    }

    #[test]
    fn parse_orchestrator_carries_rank_and_status() -> Result<(), String> {
        let stdout = r#"[{"id":"console-1","lane":"active","lane_reason":null,"rank":"a3","status":"active"},
                         {"id":"console-2","lane":"ready"}]"#;
        let parsed = parse_orchestrator_observation(&observed_for(
            SourceAdapterKind::Orchestrator,
            "console",
            stdout,
        ))?;
        let snapshots: Vec<&WorkItemSnapshot> = parsed
            .events
            .iter()
            .filter_map(|event| match event.payload() {
                SourcePayload::WorkItemSnapshot(snapshot) => Some(snapshot),
                _other => None,
            })
            .collect();

        // The emitted rank/status are carried verbatim.
        assert_eq!(snapshots[0].rank(), "a3");
        assert_eq!(snapshots[0].status(), "active");
        // An item that omits rank/status defaults to the bottom sentinel and
        // an empty status rather than failing to parse.
        assert_eq!(snapshots[1].rank(), "~");
        assert_eq!(snapshots[1].status(), "");
        Ok(())
    }

    #[test]
    fn work_item_snapshot_payload_round_trips() {
        let snapshot = work_item_snapshot_fixture();

        let payload_json = work_item_snapshot_payload_json(&snapshot);
        let rebuilt = work_item_snapshot_from_payload_json(&payload_json);

        assert_eq!(rebuilt.as_ref(), Some(&snapshot));
    }

    #[test]
    fn work_item_snapshot_payload_defaults_absent_rank_and_status() {
        // A leaner payload (no rank/status) still rebuilds, defaulting to the
        // bottom sentinel and an empty status.
        let rebuilt = work_item_snapshot_from_payload_json(
            r#"{"repo":"console","work_item_id":"console-1","lane":"ready","source_version":3}"#,
        );

        assert_eq!(rebuilt.as_ref().map(WorkItemSnapshot::rank), Some("~"));
        assert_eq!(rebuilt.as_ref().map(WorkItemSnapshot::status), Some(""));
        assert_eq!(
            rebuilt.as_ref().map(WorkItemSnapshot::lane),
            Some(Lane::Ready)
        );
    }

    #[test]
    fn work_item_snapshot_payload_rejects_non_snapshot_json() {
        // The empty object, an unrelated payload, and malformed JSON all
        // decline to rebuild rather than fabricating a lane row.
        assert_eq!(work_item_snapshot_from_payload_json("{}"), None);
        assert_eq!(
            work_item_snapshot_from_payload_json(
                r#"{"repo":"","work_item_id":"x","lane":"ready","source_version":1}"#
            ),
            None
        );
        assert_eq!(work_item_snapshot_from_payload_json("not json"), None);
    }

    #[test]
    fn parse_github_maps_real_states_into_snapshots() -> Result<(), String> {
        for (raw, expected) in [
            ("MERGED", GithubPullRequestState::Merged),
            ("CLOSED", GithubPullRequestState::ChecksFailing),
            ("OPEN", GithubPullRequestState::Open),
        ] {
            let stdout = format!("[{{\"number\": 24, \"state\": \"{raw}\"}}]");
            let parsed = parse_github_observation(&observed_for(
                SourceAdapterKind::GitHub,
                "console",
                &stdout,
            ))?;
            let version = super::stable_version(&["console", "24", expected.label()]);

            assert_eq!(
                first_payload(&parsed),
                &SourcePayload::GithubPullRequestSnapshot(GithubPullRequestSnapshot {
                    repo: "console".to_owned(),
                    pr_number: 24,
                    state: expected,
                    source_version: version,
                })
            );
        }
        Ok(())
    }

    #[test]
    fn parse_github_reports_missing_and_invalid_records() {
        assert_eq!(
            parse_github_observation(&observed_for(SourceAdapterKind::GitHub, "console", "[]")),
            Err("no pull request observed".to_owned())
        );
        assert_eq!(
            parse_github_observation(&observed_for(
                SourceAdapterKind::GitHub,
                "console",
                "[{\"number\": 0, \"state\": \"OPEN\"}]"
            )),
            Err("invalid pull request".to_owned())
        );
    }

    #[test]
    fn parse_dispatcher_reads_last_journal_entry() -> Result<(), String> {
        let stdout = "\n{\"work_item_id\": \"console-1\", \"dispatch_id\": \"dispatch_9\"}\n";
        let parsed = parse_dispatcher_observation(&observed_for(
            SourceAdapterKind::Dispatcher,
            "console",
            stdout,
        ))?;
        let version = super::stable_version(&["console-1", "dispatch_9"]);

        assert_eq!(
            first_payload(&parsed),
            &SourcePayload::DispatcherJournalEntry(DispatcherJournalEntry {
                repo: "console".to_owned(),
                work_item_id: "console-1".to_owned(),
                dispatch_id: "dispatch_9".to_owned(),
                kind: DispatcherJournalKind::NeedsRegroom,
                source_version: version,
            })
        );
        Ok(())
    }

    #[test]
    fn parse_dispatcher_reports_missing_and_invalid_records() {
        assert_eq!(
            parse_dispatcher_observation(&observed_for(
                SourceAdapterKind::Dispatcher,
                "console",
                "   \n  "
            )),
            Err("empty dispatcher journal".to_owned())
        );
        assert_eq!(
            parse_dispatcher_observation(&observed_for(
                SourceAdapterKind::Dispatcher,
                "console",
                "{\"dispatch_id\": \"dispatch_9\"}"
            )),
            Err("no work-item in journal entry".to_owned())
        );
        assert_eq!(
            parse_dispatcher_observation(&observed_for(
                SourceAdapterKind::Dispatcher,
                "console",
                "{\"work_item_id\": \"console-1\"}"
            )),
            Err("no dispatch id in journal entry".to_owned())
        );
        assert_eq!(
            parse_dispatcher_observation(&observed_for(
                SourceAdapterKind::Dispatcher,
                "",
                "{\"work_item_id\": \"console-1\", \"dispatch_id\": \"dispatch_9\"}"
            )),
            Err("invalid journal entry".to_owned())
        );
    }

    #[test]
    fn parse_fabro_reads_run_and_falls_back_to_id() -> Result<(), String> {
        let with_run = parse_fabro_observation(&observed_for(
            SourceAdapterKind::Fabro,
            "console",
            "{\"run_id\": \"run_7\", \"work_item_id\": \"console-1\"}",
        ))?;
        let version = super::stable_version(&["run_7", "console-1"]);
        assert_eq!(
            first_payload(&with_run),
            &SourcePayload::FabroRunSnapshot(FabroRunSnapshot {
                repo: "console".to_owned(),
                work_item_id: "console-1".to_owned(),
                run_id: "run_7".to_owned(),
                state: FabroRunState::HumanGate,
                source_version: version,
            })
        );

        // No run_id: fall back to id, and default work_item_id to the run id.
        let fallback = parse_fabro_observation(&observed_for(
            SourceAdapterKind::Fabro,
            "console",
            "{\"id\": \"run_8\"}",
        ))?;
        let fallback_version = super::stable_version(&["run_8", "run_8"]);
        assert_eq!(
            first_payload(&fallback),
            &SourcePayload::FabroRunSnapshot(FabroRunSnapshot {
                repo: "console".to_owned(),
                work_item_id: "run_8".to_owned(),
                run_id: "run_8".to_owned(),
                state: FabroRunState::HumanGate,
                source_version: fallback_version,
            })
        );
        Ok(())
    }

    #[test]
    fn parse_fabro_reports_missing_and_invalid_records() {
        assert_eq!(
            parse_fabro_observation(&observed_for(
                SourceAdapterKind::Fabro,
                "console",
                "{\"state\": \"human-gate\"}"
            )),
            Err("no fabro run observed".to_owned())
        );
        assert_eq!(
            parse_fabro_observation(&observed_for(
                SourceAdapterKind::Fabro,
                "",
                "{\"run_id\": \"run_7\"}"
            )),
            Err("invalid fabro run".to_owned())
        );
    }

    #[test]
    fn parse_livespec_maps_real_actions() -> Result<(), String> {
        for (raw, expected) in [
            ("revise", LivespecNextAction::Revise),
            ("critique", LivespecNextAction::Critique),
            ("none", LivespecNextAction::None),
        ] {
            let stdout = format!("{{\"action\": \"{raw}\"}}");
            let parsed = parse_livespec_observation(&observed_for(
                SourceAdapterKind::LiveSpec,
                "console",
                &stdout,
            ))?;
            let version = super::stable_version(&["console", expected.label()]);

            assert_eq!(
                first_payload(&parsed),
                &SourcePayload::LivespecNextSnapshot(LivespecNextSnapshot {
                    repo: "console".to_owned(),
                    action: expected,
                    source_version: version,
                })
            );
        }
        Ok(())
    }

    #[test]
    fn parse_livespec_falls_back_to_next_key_and_reports_errors() -> Result<(), String> {
        let parsed = parse_livespec_observation(&observed_for(
            SourceAdapterKind::LiveSpec,
            "console",
            "{\"next\": \"revise\"}",
        ))?;
        assert_eq!(
            first_payload(&parsed),
            &SourcePayload::LivespecNextSnapshot(LivespecNextSnapshot {
                repo: "console".to_owned(),
                action: LivespecNextAction::Revise,
                source_version: super::stable_version(&["console", "revise"]),
            })
        );
        assert_eq!(
            parse_livespec_observation(&observed_for(
                SourceAdapterKind::LiveSpec,
                "console",
                "{\"status\": \"clean\"}"
            )),
            Err("no livespec next action observed".to_owned())
        );
        assert_eq!(
            parse_livespec_observation(&observed_for(
                SourceAdapterKind::LiveSpec,
                "",
                "{\"action\": \"revise\"}"
            )),
            Err("invalid livespec snapshot".to_owned())
        );
        Ok(())
    }

    #[test]
    fn json_field_helpers_handle_absent_and_malformed_fields() {
        assert_eq!(
            super::first_json_string("{\"a\": \"b\"}", "a").as_deref(),
            Some("b")
        );
        assert_eq!(super::first_json_string("{\"a\": \"b\"}", "z"), None);
        assert_eq!(super::first_json_string("{\"a\" \"b\"}", "a"), None);
        assert_eq!(super::first_json_string("{\"a\": bare}", "a"), None);
        assert_eq!(super::first_json_string("{\"a\": \"b", "a"), None);
        assert_eq!(super::first_json_u64("{\"n\": 42}", "n"), Some(42));
        assert_eq!(super::first_json_u64("{\"n\": 42}", "z"), None);
        assert_eq!(super::first_json_u64("{\"n\" 42}", "n"), None);
        assert_eq!(super::first_json_u64("{\"n\": x}", "n"), None);
        assert!(super::stable_version(&["a"]) != 0);
    }
}
