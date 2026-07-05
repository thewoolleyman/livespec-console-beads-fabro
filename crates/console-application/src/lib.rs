#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};

pub mod source_adapters;

use source_adapters::{
    AcceptancePolicy, AdmissionPolicy, Lane, LaneReason, SourceProbe, SourceProbeOutcome,
    WorkItemSnapshot, work_item_snapshot_from_payload_json,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttentionItem {
    id: String,
    title: String,
    source: String,
    source_reference: String,
    next_action: Option<OperatorAction>,
}

impl AttentionItem {
    #[must_use]
    pub const fn new(
        id: String,
        title: String,
        source: String,
        source_reference: String,
        next_action: Option<OperatorAction>,
    ) -> Self {
        Self {
            id,
            title,
            source,
            source_reference,
            next_action,
        }
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

    #[must_use]
    pub fn source_reference(&self) -> &str {
        &self.source_reference
    }

    #[must_use]
    pub const fn next_action(&self) -> Option<OperatorAction> {
        self.next_action
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiView {
    Attention,
    Spec,
    Lanes,
    Events,
    Repos,
}

impl TuiView {
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Attention,
            Self::Spec,
            Self::Lanes,
            Self::Events,
            Self::Repos,
        ]
    }

    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Attention => "Attention",
            Self::Spec => "Spec",
            Self::Lanes => "Lanes",
            Self::Events => "Events",
            Self::Repos => "Repos",
        }
    }
}

/// Which lane sub-view the `Lanes` view is showing: the cross-lane overview
/// home, or a single lane drilled into for its full rank-ordered list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneFocus {
    Overview,
    Lane(Lane),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorAction {
    OpenFabroAttach,
    CopyFabroAttach,
}

impl OperatorAction {
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::OpenFabroAttach => "Open Fabro attach",
            Self::CopyFabroAttach => "Copy Fabro attach",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiOverlay {
    None,
    Search { query: String },
    CommandPalette { query: String },
    CommandModal { selected_action_index: usize },
}

impl TuiOverlay {
    #[must_use]
    pub const fn is_open(&self) -> bool {
        !matches!(self, Self::None)
    }

    #[must_use]
    pub fn query(&self) -> Option<&str> {
        match self {
            Self::Search { query } | Self::CommandPalette { query } => Some(query),
            Self::None | Self::CommandModal { .. } => None,
        }
    }

    #[must_use]
    pub const fn selected_action_index(&self) -> Option<usize> {
        match self {
            Self::CommandModal {
                selected_action_index,
            } => Some(*selected_action_index),
            Self::None | Self::Search { .. } | Self::CommandPalette { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiInteraction {
    SelectNext,
    SelectPrevious,
    OpenSearch,
    OpenCommandPalette,
    OpenCommandModal,
    CloseOverlay,
    SelectNextView,
    SelectPreviousView,
    TypeChar(char),
    Backspace,
    SelectNextAction,
    SelectPreviousAction,
    DrillIntoLane,
    ReturnToLaneOverview,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiInteractionState {
    active_view: TuiView,
    selected_attention_index: usize,
    lane_focus: LaneFocus,
    selected_lane_index: usize,
    overlay: TuiOverlay,
}

impl TuiInteractionState {
    #[must_use]
    pub const fn new(selected_attention_index: usize, overlay: TuiOverlay) -> Self {
        Self {
            active_view: TuiView::Attention,
            selected_attention_index,
            lane_focus: LaneFocus::Overview,
            selected_lane_index: 0,
            overlay,
        }
    }

    #[must_use]
    pub const fn for_view(
        active_view: TuiView,
        selected_attention_index: usize,
        overlay: TuiOverlay,
    ) -> Self {
        Self {
            active_view,
            selected_attention_index,
            lane_focus: LaneFocus::Overview,
            selected_lane_index: 0,
            overlay,
        }
    }

    /// Replace the active view, preserving every other field. Used by the
    /// interaction reducer to keep state changes single-field and readable.
    #[must_use]
    pub const fn with_active_view(mut self, active_view: TuiView) -> Self {
        self.active_view = active_view;
        self
    }

    #[must_use]
    pub const fn with_selected_attention_index(mut self, selected_attention_index: usize) -> Self {
        self.selected_attention_index = selected_attention_index;
        self
    }

    #[must_use]
    pub const fn with_lane_focus(mut self, lane_focus: LaneFocus) -> Self {
        self.lane_focus = lane_focus;
        self
    }

    #[must_use]
    pub const fn with_selected_lane_index(mut self, selected_lane_index: usize) -> Self {
        self.selected_lane_index = selected_lane_index;
        self
    }

    #[must_use]
    pub fn with_overlay(mut self, overlay: TuiOverlay) -> Self {
        self.overlay = overlay;
        self
    }

    #[must_use]
    pub const fn active_view(&self) -> TuiView {
        self.active_view
    }

    #[must_use]
    pub const fn selected_attention_index(&self) -> usize {
        self.selected_attention_index
    }

    #[must_use]
    pub const fn lane_focus(&self) -> LaneFocus {
        self.lane_focus
    }

    #[must_use]
    pub const fn selected_lane_index(&self) -> usize {
        self.selected_lane_index
    }

    #[must_use]
    pub const fn overlay(&self) -> &TuiOverlay {
        &self.overlay
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineEntry {
    event_id: String,
    label: String,
    source: String,
}

impl TimelineEntry {
    #[must_use]
    pub const fn new(event_id: String, label: String, source: String) -> Self {
        Self {
            event_id,
            label,
            source,
        }
    }

    #[must_use]
    pub fn event_id(&self) -> &str {
        &self.event_id
    }

    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttentionDetail {
    repo: String,
    work_item: String,
    fabro_run: String,
    attach_command: String,
    timeline: Vec<TimelineEntry>,
    actions: Vec<OperatorAction>,
}

impl AttentionDetail {
    #[must_use]
    pub const fn new(
        repo: String,
        work_item: String,
        fabro_run: String,
        attach_command: String,
        timeline: Vec<TimelineEntry>,
        actions: Vec<OperatorAction>,
    ) -> Self {
        Self {
            repo,
            work_item,
            fabro_run,
            attach_command,
            timeline,
            actions,
        }
    }

    #[must_use]
    pub fn repo(&self) -> &str {
        &self.repo
    }

    #[must_use]
    pub fn work_item(&self) -> &str {
        &self.work_item
    }

    #[must_use]
    pub fn fabro_run(&self) -> &str {
        &self.fabro_run
    }

    #[must_use]
    pub fn attach_command(&self) -> &str {
        &self.attach_command
    }

    #[must_use]
    pub fn timeline(&self) -> &[TimelineEntry] {
        &self.timeline
    }

    #[must_use]
    pub fn actions(&self) -> &[OperatorAction] {
        &self.actions
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiScreenModel {
    active_view: TuiView,
    navigation: Vec<TuiView>,
    attention_items: Vec<AttentionItem>,
    selected_attention_index: Option<usize>,
    detail: Option<AttentionDetail>,
    view_items: Vec<ViewSummaryItem>,
    lane_board: LaneBoard,
    lane_focus: LaneFocus,
    selected_lane_index: Option<usize>,
    overlay: TuiOverlay,
    header: String,
    footer: String,
}

impl TuiScreenModel {
    #[must_use]
    pub const fn active_view(&self) -> TuiView {
        self.active_view
    }

    #[must_use]
    pub fn navigation(&self) -> &[TuiView] {
        &self.navigation
    }

    #[must_use]
    pub fn attention_items(&self) -> &[AttentionItem] {
        &self.attention_items
    }

    #[must_use]
    pub const fn selected_attention_index(&self) -> Option<usize> {
        self.selected_attention_index
    }

    #[must_use]
    pub const fn detail(&self) -> Option<&AttentionDetail> {
        self.detail.as_ref()
    }

    #[must_use]
    pub fn view_items(&self) -> &[ViewSummaryItem] {
        &self.view_items
    }

    /// The seven-lane board projected from the work-item snapshot observations,
    /// rendered by the `Lanes` view's overview and per-lane drill-in.
    #[must_use]
    pub const fn lane_board(&self) -> &LaneBoard {
        &self.lane_board
    }

    /// Which lane sub-view the `Lanes` view is showing (overview or a drilled-in
    /// lane).
    #[must_use]
    pub const fn lane_focus(&self) -> LaneFocus {
        self.lane_focus
    }

    /// The selected lane row in the lane overview, present only while the
    /// `Lanes` view shows its overview home; `None` otherwise.
    #[must_use]
    pub const fn selected_lane_index(&self) -> Option<usize> {
        self.selected_lane_index
    }

    #[must_use]
    pub const fn overlay(&self) -> &TuiOverlay {
        &self.overlay
    }

    #[must_use]
    pub fn selected_operator_action(&self) -> Option<OperatorAction> {
        let action_index = self.overlay.selected_action_index()?;
        self.detail()?.actions().get(action_index).copied()
    }

    #[must_use]
    pub fn header(&self) -> &str {
        &self.header
    }

    #[must_use]
    pub fn footer(&self) -> &str {
        &self.footer
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewSummaryItem {
    title: String,
    detail: String,
}

impl ViewSummaryItem {
    #[must_use]
    pub const fn new(title: String, detail: String) -> Self {
        Self { title, detail }
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplicationError {
    EmptyOperatorAction,
    FactoryDrainPortFailed,
    NoSelectedAttentionItem,
    NoSelectedOperatorAction,
    UnknownCommandPaletteAction,
}

pub type ApplicationResult<T> = Result<T, ApplicationError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperatorActionOutcome {
    PersistCommand(CommandEnvelope),
    OpenAttachCommand(String),
    CopyAttachCommand(String),
}

impl OperatorActionOutcome {
    #[must_use]
    pub const fn command(&self) -> Option<&CommandEnvelope> {
        match self {
            Self::PersistCommand(command) => Some(command),
            Self::OpenAttachCommand(_) | Self::CopyAttachCommand(_) => None,
        }
    }

    #[must_use]
    pub fn attach_command(&self) -> Option<&str> {
        match self {
            Self::OpenAttachCommand(command) | Self::CopyAttachCommand(command) => Some(command),
            Self::PersistCommand(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactoryDrainRequest {
    aggregate_id: String,
    budget: u16,
    parallel: u16,
}

impl FactoryDrainRequest {
    #[must_use]
    pub const fn new(aggregate_id: String, budget: u16, parallel: u16) -> Self {
        Self {
            aggregate_id,
            budget,
            parallel,
        }
    }

    #[must_use]
    pub fn aggregate_id(&self) -> &str {
        &self.aggregate_id
    }

    #[must_use]
    pub const fn budget(&self) -> u16 {
        self.budget
    }

    #[must_use]
    pub const fn parallel(&self) -> u16 {
        self.parallel
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactoryDrainPortOutcome {
    Completed {
        dispatched_items: u16,
    },
    Failed,
    /// The drain was requested but no real Dispatcher port is wired, so no
    /// drain was attempted. Reported honestly instead of fabricating success.
    NotWired,
}

impl FactoryDrainPortOutcome {
    #[must_use]
    pub const fn completed(dispatched_items: u16) -> Self {
        Self::Completed { dispatched_items }
    }

    #[must_use]
    pub const fn failed() -> Self {
        Self::Failed
    }

    #[must_use]
    pub const fn not_wired() -> Self {
        Self::NotWired
    }
}

pub trait FactoryDrainPort {
    fn drain_ready_queue(
        &mut self,
        request: &FactoryDrainRequest,
    ) -> ApplicationResult<FactoryDrainPortOutcome>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FactoryDrainPolicy {
    ready_work_item_count: usize,
}

impl FactoryDrainPolicy {
    #[must_use]
    pub const fn new(ready_work_item_count: usize) -> Self {
        Self {
            ready_work_item_count,
        }
    }

    #[must_use]
    pub fn from_events(events: &[ConsoleEvent]) -> Self {
        let ready_work_item_count = project_lane_board(events)
            .column(Lane::Ready)
            .map_or(0, LaneColumn::count);
        Self::new(ready_work_item_count)
    }

    #[must_use]
    pub const fn rejection_reason(&self) -> Option<&'static str> {
        if self.ready_work_item_count == 0 {
            Some("no ready implementation work")
        } else {
            None
        }
    }
}

/// Real factory-drain port that invokes the Dispatcher through a [`SourceProbe`].
///
/// It reflects the Dispatcher's actual outcome rather than fabricating success:
/// a successful run completes with the dispatched-item count it reports, a
/// non-zero run fails, and an unavailable Dispatcher binary yields a not-wired
/// outcome. The host-backed probe is supplied by the binary, so the live drain
/// never claims an action that did not happen.
pub struct DispatcherFactoryDrainPort<'a> {
    probe: &'a dyn SourceProbe,
    program: String,
    args: Vec<String>,
}

impl<'a> DispatcherFactoryDrainPort<'a> {
    #[must_use]
    pub fn new(probe: &'a dyn SourceProbe, program: &str, args: &[&str]) -> Self {
        Self {
            probe,
            program: program.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        }
    }
}

impl FactoryDrainPort for DispatcherFactoryDrainPort<'_> {
    fn drain_ready_queue(
        &mut self,
        _request: &FactoryDrainRequest,
    ) -> ApplicationResult<FactoryDrainPortOutcome> {
        let arg_refs: Vec<&str> = self.args.iter().map(String::as_str).collect();
        Ok(match self.probe.run_command(&self.program, &arg_refs) {
            SourceProbeOutcome::Observed {
                stdout,
                success: true,
            } => FactoryDrainPortOutcome::completed(dispatched_item_count(&stdout)),
            SourceProbeOutcome::Observed { success: false, .. } => {
                FactoryDrainPortOutcome::failed()
            }
            SourceProbeOutcome::Unavailable { .. } => FactoryDrainPortOutcome::not_wired(),
        })
    }
}

/// First run of digits in the Dispatcher's drain output, as the dispatched-item
/// count. A report without a count is honestly treated as zero dispatched.
fn dispatched_item_count(stdout: &str) -> u16 {
    let digits: String = stdout
        .chars()
        .skip_while(|character| !character.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse::<u16>().unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactoryCommandOutcome {
    command_status: String,
    events: Vec<ConsoleEvent>,
}

impl FactoryCommandOutcome {
    #[must_use]
    pub const fn new(command_status: String, events: Vec<ConsoleEvent>) -> Self {
        Self {
            command_status,
            events,
        }
    }

    #[must_use]
    pub fn command_status(&self) -> &str {
        &self.command_status
    }

    #[must_use]
    pub fn events(&self) -> &[ConsoleEvent] {
        &self.events
    }
}

#[must_use]
pub fn project_attention(events: &[ConsoleEvent]) -> Vec<AttentionItem> {
    project_attention_from_snapshots(attention_snapshots(events))
}

/// One work-item as it lands in a lane, carrying the fields the lane board
/// renders. Built purely by reducing the persisted work-item snapshot
/// observations — never stored as primary state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaneWorkItem {
    work_item_id: String,
    repo: String,
    lane: Lane,
    lane_reason: Option<LaneReason>,
    rank: String,
    status: String,
}

impl LaneWorkItem {
    fn from_snapshot(snapshot: &WorkItemSnapshot) -> Self {
        Self {
            work_item_id: snapshot.work_item_id().to_owned(),
            repo: snapshot.repo().to_owned(),
            lane: snapshot.lane(),
            lane_reason: snapshot.lane_reason(),
            rank: snapshot.rank().to_owned(),
            status: snapshot.status().to_owned(),
        }
    }

    #[must_use]
    pub fn work_item_id(&self) -> &str {
        &self.work_item_id
    }

    #[must_use]
    pub fn repo(&self) -> &str {
        &self.repo
    }

    #[must_use]
    pub const fn lane(&self) -> Lane {
        self.lane
    }

    #[must_use]
    pub const fn lane_reason(&self) -> Option<LaneReason> {
        self.lane_reason
    }

    #[must_use]
    pub fn rank(&self) -> &str {
        &self.rank
    }

    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }
}

/// One lane column of the board: the lane and its rank-ordered items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaneColumn {
    lane: Lane,
    items: Vec<LaneWorkItem>,
}

impl LaneColumn {
    #[must_use]
    pub const fn lane(&self) -> Lane {
        self.lane
    }

    #[must_use]
    pub fn items(&self) -> &[LaneWorkItem] {
        &self.items
    }

    #[must_use]
    pub const fn count(&self) -> usize {
        self.items.len()
    }
}

/// The seven-lane board: every lane with its rank-ordered items.
///
/// A pure derivation of the work-item snapshot observations, so it is
/// rebuildable from the ledger and never persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaneBoard {
    columns: Vec<LaneColumn>,
}

impl LaneBoard {
    #[must_use]
    pub fn columns(&self) -> &[LaneColumn] {
        &self.columns
    }

    /// The column for a given lane. Present for every lane because the board
    /// always carries all seven, so this never returns `None` for a real lane.
    #[must_use]
    pub fn column(&self, lane: Lane) -> Option<&LaneColumn> {
        self.columns.iter().find(|column| column.lane() == lane)
    }

    /// Total work-items across all lanes.
    #[must_use]
    pub fn total(&self) -> usize {
        self.columns.iter().map(LaneColumn::count).sum()
    }
}

/// Project the seven-lane board by reducing the work-item snapshot observations.
///
/// The latest observation per work-item wins (later events supersede earlier
/// ones), each item lands in its emitted `lane`, and every lane is ordered by
/// the fractional `rank` (ties broken by id). Events whose payload is not a
/// complete snapshot are skipped.
#[must_use]
pub fn project_lane_board(events: &[ConsoleEvent]) -> LaneBoard {
    let mut latest: BTreeMap<String, LaneWorkItem> = BTreeMap::new();
    for event in events {
        if *event.event_type() != EventType::WorkItemSnapshotObserved {
            continue;
        }
        let Some(snapshot) = work_item_snapshot_from_payload_json(event.payload_json()) else {
            continue;
        };
        latest.insert(
            snapshot.work_item_id().to_owned(),
            LaneWorkItem::from_snapshot(&snapshot),
        );
    }
    let columns = Lane::all()
        .iter()
        .map(|lane| {
            let mut items: Vec<LaneWorkItem> = latest
                .values()
                .filter(|item| item.lane() == *lane)
                .cloned()
                .collect();
            items.sort_by(|left, right| {
                left.rank()
                    .cmp(right.rank())
                    .then_with(|| left.work_item_id().cmp(right.work_item_id()))
            });
            LaneColumn { lane: *lane, items }
        })
        .collect();
    LaneBoard { columns }
}

#[must_use]
pub fn build_tui_model(events: &[ConsoleEvent], requested_selection: usize) -> TuiScreenModel {
    let state = TuiInteractionState::new(requested_selection, TuiOverlay::None);
    build_tui_model_for_state(events, &state)
}

#[must_use]
pub fn build_tui_model_for_state(
    events: &[ConsoleEvent],
    state: &TuiInteractionState,
) -> TuiScreenModel {
    let search_query = search_query(state.overlay());
    let attention_snapshots = attention_snapshots_matching(events, search_query);
    let attention_items = project_attention_from_snapshots(attention_snapshots.clone());
    let selected_attention_index =
        selected_index(attention_items.len(), state.selected_attention_index());
    let detail = selected_attention_index
        .map(|index| build_attention_detail(&attention_snapshots[index], events));
    let overlay = normalize_overlay(state.overlay(), detail.as_ref());
    let active_view = state.active_view();
    let lane_board = project_lane_board(events);
    let lane_focus = state.lane_focus();
    let selected_lane_index = match (active_view, lane_focus) {
        (TuiView::Lanes, LaneFocus::Overview) => {
            Some(state.selected_lane_index().min(Lane::all().len() - 1))
        }
        _ => None,
    };
    TuiScreenModel {
        active_view,
        navigation: TuiView::all().to_vec(),
        attention_items,
        selected_attention_index,
        detail,
        view_items: view_summary_items(active_view, events),
        lane_board,
        lane_focus,
        selected_lane_index,
        overlay,
        header: format!(
            "fleet: livespec | mode: tui | view: {} | attention: {}",
            active_view.label(),
            attention_snapshots.len()
        ),
        footer: "shortcuts: up/down select | left/right views | enter details | / search | : command palette"
            .to_owned(),
    }
}

#[must_use]
pub fn reduce_tui_interaction(
    state: &TuiInteractionState,
    events: &[ConsoleEvent],
    interaction: TuiInteraction,
) -> TuiInteractionState {
    let model = build_tui_model_for_state(events, state);
    match interaction {
        TuiInteraction::SelectNext => select_next(state, &model),
        TuiInteraction::SelectPrevious => select_previous(state),
        TuiInteraction::SelectNextView => state
            .clone()
            .with_active_view(move_view_down(state.active_view())),
        TuiInteraction::SelectPreviousView => state
            .clone()
            .with_active_view(move_view_up(state.active_view())),
        TuiInteraction::OpenSearch => state.clone().with_overlay(TuiOverlay::Search {
            query: String::new(),
        }),
        TuiInteraction::OpenCommandPalette => {
            state.clone().with_overlay(TuiOverlay::CommandPalette {
                query: String::new(),
            })
        }
        TuiInteraction::OpenCommandModal => state.clone().with_overlay(TuiOverlay::CommandModal {
            selected_action_index: 0,
        }),
        TuiInteraction::CloseOverlay => state.clone().with_overlay(TuiOverlay::None),
        TuiInteraction::TypeChar(value) => state
            .clone()
            .with_overlay(type_overlay_char(state.overlay(), value)),
        TuiInteraction::Backspace => state
            .clone()
            .with_overlay(backspace_overlay_query(state.overlay())),
        TuiInteraction::SelectNextAction => state
            .clone()
            .with_overlay(move_action_down(state.overlay(), model.detail())),
        TuiInteraction::SelectPreviousAction => {
            state.clone().with_overlay(move_action_up(state.overlay()))
        }
        TuiInteraction::DrillIntoLane => drill_into_lane(state),
        TuiInteraction::ReturnToLaneOverview => state.clone().with_lane_focus(LaneFocus::Overview),
    }
}

/// Whether the `Lanes` view is showing its cross-lane overview home, where
/// up/down moves the selected lane row rather than the attention selection.
fn is_lane_overview(state: &TuiInteractionState) -> bool {
    state.active_view() == TuiView::Lanes && state.lane_focus() == LaneFocus::Overview
}

/// Move the selection down, routed to the lane overview row when the lane
/// overview is active, else to the attention list.
fn select_next(state: &TuiInteractionState, model: &TuiScreenModel) -> TuiInteractionState {
    if is_lane_overview(state) {
        state.clone().with_selected_lane_index(move_selection_down(
            Lane::all().len(),
            state.selected_lane_index(),
        ))
    } else {
        state
            .clone()
            .with_selected_attention_index(move_selection_down(
                model.attention_items().len(),
                state.selected_attention_index(),
            ))
    }
}

/// Move the selection up, routed to the lane overview row when the lane
/// overview is active, else to the attention list.
fn select_previous(state: &TuiInteractionState) -> TuiInteractionState {
    if is_lane_overview(state) {
        state
            .clone()
            .with_selected_lane_index(move_selection_up(state.selected_lane_index()))
    } else {
        state
            .clone()
            .with_selected_attention_index(move_selection_up(state.selected_attention_index()))
    }
}

/// Drill the lane overview's selected lane into a full per-lane list.
fn drill_into_lane(state: &TuiInteractionState) -> TuiInteractionState {
    let lane = Lane::all()[state.selected_lane_index().min(Lane::all().len() - 1)];
    state.clone().with_lane_focus(LaneFocus::Lane(lane))
}

pub fn validate_operator_action(action: &str) -> ApplicationResult<&str> {
    let trimmed = action.trim();
    if trimmed.is_empty() {
        return Err(ApplicationError::EmptyOperatorAction);
    }
    Ok(trimmed)
}

pub fn resolve_selected_operator_action(
    model: &TuiScreenModel,
    requested_by: &str,
) -> ApplicationResult<OperatorActionOutcome> {
    validate_operator_action(requested_by)?;
    let detail = model
        .detail()
        .ok_or(ApplicationError::NoSelectedAttentionItem)?;
    let action = model
        .selected_operator_action()
        .ok_or(ApplicationError::NoSelectedOperatorAction)?;
    Ok(match action {
        OperatorAction::OpenFabroAttach => {
            OperatorActionOutcome::OpenAttachCommand(detail.attach_command().to_owned())
        }
        OperatorAction::CopyFabroAttach => {
            OperatorActionOutcome::CopyAttachCommand(detail.attach_command().to_owned())
        }
    })
}

pub fn resolve_command_palette_action(
    model: &TuiScreenModel,
    requested_by: &str,
) -> ApplicationResult<OperatorActionOutcome> {
    let requested_by = validate_operator_action(requested_by)?;
    let TuiOverlay::CommandPalette { query } = model.overlay() else {
        return Err(ApplicationError::NoSelectedOperatorAction);
    };
    if command_palette_query_matches_drain(query) {
        return Ok(OperatorActionOutcome::PersistCommand(
            factory_drain_command(requested_by),
        ));
    }
    Err(ApplicationError::UnknownCommandPaletteAction)
}

fn command_palette_query_matches_drain(query: &str) -> bool {
    let normalized = query.trim().to_lowercase();
    normalized == "drain" || normalized == "drain ready queue"
}

fn factory_drain_command(requested_by: &str) -> CommandEnvelope {
    CommandEnvelope::new(
        "cmd_factory_drain_requested_budget_1_parallel_1".to_owned(),
        CommandType::FactoryDrainRequested,
        "fleet:livespec".to_owned(),
        "fleet:livespec:factory.drain_requested:budget=1:parallel=1".to_owned(),
        requested_by.to_owned(),
    )
}

pub fn handle_factory_drain_command(
    command: &CommandEnvelope,
    policy: &FactoryDrainPolicy,
    port: &mut dyn FactoryDrainPort,
) -> ApplicationResult<FactoryCommandOutcome> {
    if let Some(reason) = policy.rejection_reason() {
        return Ok(FactoryCommandOutcome::new(
            "rejected".to_owned(),
            vec![rejected_factory_command_event(command, reason)],
        ));
    }
    let request = FactoryDrainRequest::new(command.aggregate_id().to_owned(), 1, 1);
    let port_outcome = port.drain_ready_queue(&request)?;
    let mut events = vec![factory_command_event(
        command,
        EventType::CommandAccepted,
        "accepted",
        1,
    )];
    let command_status = match port_outcome {
        FactoryDrainPortOutcome::Completed {
            dispatched_items: _dispatched_items,
        } => {
            events.push(factory_command_event(
                command,
                EventType::FactoryDrainStarted,
                "started",
                2,
            ));
            events.push(factory_command_event(
                command,
                EventType::FactoryDrainCompleted,
                "completed",
                3,
            ));
            "completed"
        }
        FactoryDrainPortOutcome::Failed => {
            events.push(factory_command_event(
                command,
                EventType::FactoryDrainStarted,
                "started",
                2,
            ));
            events.push(factory_command_event(
                command,
                EventType::FactoryDrainFailed,
                "failed",
                3,
            ));
            "failed"
        }
        FactoryDrainPortOutcome::NotWired => {
            // No real Dispatcher port is wired, so the drain never started.
            // Emit an honest not-wired outcome rather than a fabricated
            // start/completion.
            events.push(factory_command_event(
                command,
                EventType::FactoryDrainNotWired,
                "not_wired",
                2,
            ));
            "not_wired"
        }
    };
    Ok(FactoryCommandOutcome::new(
        command_status.to_owned(),
        events,
    ))
}

fn rejected_factory_command_event(command: &CommandEnvelope, reason: &str) -> ConsoleEvent {
    factory_command_event(command, EventType::CommandRejected, "rejected", 1).with_payload_json(
        serde_json::json!({
            "reason": reason,
        })
        .to_string(),
    )
}

fn factory_command_event(
    command: &CommandEnvelope,
    event_type: EventType,
    suffix: &str,
    stream_seq: u64,
) -> ConsoleEvent {
    ConsoleEvent::new(
        format!("evt_{}_{}", command.command_id(), suffix),
        1,
        factory_command_event_context(event_type).to_owned(),
        event_type,
        "console:factory-command-handler".to_owned(),
        command.aggregate_id().to_owned(),
        stream_seq,
    )
}

const fn factory_command_event_context(event_type: EventType) -> &'static str {
    match event_type {
        EventType::CommandAccepted | EventType::CommandRejected => "command",
        EventType::FactoryDrainCompleted
        | EventType::FactoryDrainFailed
        | EventType::FactoryDrainNotWired
        | EventType::FactoryDrainRequested
        | EventType::FactoryDrainStarted => "factory",
        EventType::WorkItemSnapshotObserved
        | EventType::DispatcherNeedsRegroomObserved
        | EventType::FabroHumanGateObserved
        | EventType::GithubPullRequestSnapshotObserved
        | EventType::LivespecNextSnapshotObserved
        | EventType::LivespecReviseRequired
        | EventType::SourceCompletenessFindingObserved
        | EventType::SourceNotObservedFindingObserved => "source",
    }
}

#[derive(Debug, Clone)]
struct AttentionSnapshot {
    event: ConsoleEvent,
    snapshot: WorkItemSnapshot,
}

fn attention_snapshots(events: &[ConsoleEvent]) -> Vec<AttentionSnapshot> {
    let mut latest: BTreeMap<String, AttentionSnapshot> = BTreeMap::new();
    for event in events {
        if *event.event_type() != EventType::WorkItemSnapshotObserved {
            continue;
        }
        let Some(snapshot) = work_item_snapshot_from_payload_json(event.payload_json()) else {
            continue;
        };
        latest.insert(
            snapshot.work_item_id().to_owned(),
            AttentionSnapshot {
                event: event.clone(),
                snapshot,
            },
        );
    }
    let mut snapshots: Vec<AttentionSnapshot> = latest
        .into_values()
        .filter(|entry| requires_attention(&entry.snapshot))
        .collect();
    snapshots.sort_by(|left, right| {
        left.snapshot
            .rank()
            .cmp(right.snapshot.rank())
            .then_with(|| {
                left.snapshot
                    .work_item_id()
                    .cmp(right.snapshot.work_item_id())
            })
    });
    snapshots
}

fn attention_snapshots_matching(
    events: &[ConsoleEvent],
    search_query: Option<&str>,
) -> Vec<AttentionSnapshot> {
    attention_snapshots(events)
        .into_iter()
        .filter(|entry| attention_snapshot_matches(entry, search_query))
        .collect()
}

fn attention_snapshot_matches(entry: &AttentionSnapshot, search_query: Option<&str>) -> bool {
    search_query.is_none_or(|query| {
        let snapshot = &entry.snapshot;
        query.is_empty()
            || attention_title(snapshot)
                .to_lowercase()
                .contains(&query.to_lowercase())
            || snapshot
                .repo()
                .to_lowercase()
                .contains(&query.to_lowercase())
            || snapshot
                .work_item_id()
                .to_lowercase()
                .contains(&query.to_lowercase())
            || entry
                .event
                .source()
                .to_lowercase()
                .contains(&query.to_lowercase())
    })
}

fn project_attention_from_snapshots(snapshots: Vec<AttentionSnapshot>) -> Vec<AttentionItem> {
    snapshots
        .into_iter()
        .map(|entry| {
            AttentionItem::new(
                entry.snapshot.work_item_id().to_owned(),
                attention_title(&entry.snapshot),
                entry.event.source().to_owned(),
                entry.snapshot.repo().to_owned(),
                None,
            )
        })
        .collect()
}

#[must_use]
const fn requires_attention(snapshot: &WorkItemSnapshot) -> bool {
    requires_attention_from_lane(
        snapshot.lane(),
        snapshot.lane_reason(),
        snapshot.admission_policy(),
        snapshot.acceptance_policy(),
    )
}

#[must_use]
const fn requires_attention_from_lane(
    lane: Lane,
    lane_reason: Option<LaneReason>,
    admission_policy: AdmissionPolicy,
    acceptance_policy: AcceptancePolicy,
) -> bool {
    matches!(
        (lane, lane_reason, admission_policy, acceptance_policy),
        (Lane::PendingApproval, _, AdmissionPolicy::Manual, _)
            | (Lane::Acceptance, _, _, AcceptancePolicy::AiThenHuman)
            | (Lane::Blocked, Some(LaneReason::NeedsHuman), _, _)
    )
}

fn attention_title(snapshot: &WorkItemSnapshot) -> String {
    match (snapshot.lane(), snapshot.lane_reason()) {
        (Lane::PendingApproval, _) => "Pending approval".to_owned(),
        (Lane::Acceptance, _) => "Acceptance review".to_owned(),
        (Lane::Blocked, Some(reason)) => format!("Blocked: {}", reason.label()),
        (lane, _) => lane.label().to_owned(),
    }
}

fn search_query(overlay: &TuiOverlay) -> Option<&str> {
    match overlay {
        TuiOverlay::Search { query } => Some(query),
        TuiOverlay::None | TuiOverlay::CommandPalette { .. } | TuiOverlay::CommandModal { .. } => {
            None
        }
    }
}

fn normalize_overlay(overlay: &TuiOverlay, detail: Option<&AttentionDetail>) -> TuiOverlay {
    match overlay {
        TuiOverlay::CommandModal {
            selected_action_index,
        } => TuiOverlay::CommandModal {
            selected_action_index: clamp_action_index(detail, *selected_action_index),
        },
        TuiOverlay::None | TuiOverlay::Search { .. } | TuiOverlay::CommandPalette { .. } => {
            overlay.clone()
        }
    }
}

fn selected_index(item_count: usize, requested_selection: usize) -> Option<usize> {
    (item_count > 0).then(|| requested_selection.min(item_count - 1))
}

fn move_selection_down(item_count: usize, selected_index: usize) -> usize {
    if item_count == 0 {
        return 0;
    }
    (selected_index + 1).min(item_count - 1)
}

const fn move_selection_up(selected_index: usize) -> usize {
    selected_index.saturating_sub(1)
}

fn move_view_down(active_view: TuiView) -> TuiView {
    let views = TuiView::all();
    let index = view_index(active_view);
    views[(index + 1).min(views.len() - 1)]
}

fn move_view_up(active_view: TuiView) -> TuiView {
    let views = TuiView::all();
    let index = view_index(active_view);
    views[index.saturating_sub(1)]
}

fn view_index(active_view: TuiView) -> usize {
    TuiView::all()
        .iter()
        .position(|view| *view == active_view)
        .unwrap_or_default()
}

fn type_overlay_char(overlay: &TuiOverlay, value: char) -> TuiOverlay {
    match overlay {
        TuiOverlay::Search { query } => TuiOverlay::Search {
            query: format!("{query}{value}"),
        },
        TuiOverlay::CommandPalette { query } => TuiOverlay::CommandPalette {
            query: format!("{query}{value}"),
        },
        TuiOverlay::None | TuiOverlay::CommandModal { .. } => overlay.clone(),
    }
}

fn backspace_overlay_query(overlay: &TuiOverlay) -> TuiOverlay {
    match overlay {
        TuiOverlay::Search { query } => TuiOverlay::Search {
            query: query
                .char_indices()
                .next_back()
                .map_or_else(String::new, |(index, _value)| query[..index].to_owned()),
        },
        TuiOverlay::CommandPalette { query } => TuiOverlay::CommandPalette {
            query: query
                .char_indices()
                .next_back()
                .map_or_else(String::new, |(index, _value)| query[..index].to_owned()),
        },
        TuiOverlay::None | TuiOverlay::CommandModal { .. } => overlay.clone(),
    }
}

fn move_action_down(overlay: &TuiOverlay, detail: Option<&AttentionDetail>) -> TuiOverlay {
    match overlay {
        TuiOverlay::CommandModal {
            selected_action_index,
        } => TuiOverlay::CommandModal {
            selected_action_index: clamp_action_index(detail, selected_action_index + 1),
        },
        TuiOverlay::None | TuiOverlay::Search { .. } | TuiOverlay::CommandPalette { .. } => {
            overlay.clone()
        }
    }
}

fn move_action_up(overlay: &TuiOverlay) -> TuiOverlay {
    match overlay {
        TuiOverlay::CommandModal {
            selected_action_index,
        } => TuiOverlay::CommandModal {
            selected_action_index: selected_action_index.saturating_sub(1),
        },
        TuiOverlay::None | TuiOverlay::Search { .. } | TuiOverlay::CommandPalette { .. } => {
            overlay.clone()
        }
    }
}

fn clamp_action_index(detail: Option<&AttentionDetail>, requested_index: usize) -> usize {
    detail
        .and_then(|detail| selected_index(detail.actions().len(), requested_index))
        .unwrap_or_default()
}

fn build_attention_detail(entry: &AttentionSnapshot, events: &[ConsoleEvent]) -> AttentionDetail {
    let event = &entry.event;
    let fabro_run = fabro_run_id(event);
    AttentionDetail::new(
        entry.snapshot.repo().to_owned(),
        entry.snapshot.work_item_id().to_owned(),
        fabro_run.clone(),
        format!("fabro attach {fabro_run}"),
        latest_timeline(events, event.stream_id(), 3),
        Vec::new(),
    )
}

fn view_summary_items(active_view: TuiView, events: &[ConsoleEvent]) -> Vec<ViewSummaryItem> {
    match active_view {
        TuiView::Spec => spec_view_items(events),
        TuiView::Events => events_view_items(events),
        TuiView::Repos => repos_view_items(events),
        // The Attention and Lanes views render their own projections (the
        // attention list / detail and the lane board), not summary rows.
        TuiView::Attention | TuiView::Lanes => Vec::new(),
    }
}

fn spec_view_items(events: &[ConsoleEvent]) -> Vec<ViewSummaryItem> {
    vec![
        ViewSummaryItem::new(
            format!(
                "LiveSpec next snapshots: {}",
                count_events(events, EventType::LivespecNextSnapshotObserved)
            ),
            "Spec lifecycle status is projected from LiveSpec adapter observations.".to_owned(),
        ),
        ViewSummaryItem::new(
            format!(
                "Revise required: {}",
                count_events(events, EventType::LivespecReviseRequired)
            ),
            "Revise-required events stay visible in the Spec view until resolved.".to_owned(),
        ),
    ]
}

fn events_view_items(events: &[ConsoleEvent]) -> Vec<ViewSummaryItem> {
    let latest = events
        .last()
        .map_or_else(|| "none".to_owned(), latest_event_summary);
    vec![
        ViewSummaryItem::new(
            format!("Stored events: {}", events.len()),
            "The event log is the canonical source for projections.".to_owned(),
        ),
        ViewSummaryItem::new("Latest event".to_owned(), latest),
    ]
}

fn repos_view_items(events: &[ConsoleEvent]) -> Vec<ViewSummaryItem> {
    let mut repos = events.iter().map(repo_id).collect::<Vec<_>>();
    repos.sort();
    repos.dedup();
    vec![ViewSummaryItem::new(
        format!("Repos observed: {}", repos.len()),
        repos.join(", "),
    )]
}

fn latest_event_summary(event: &ConsoleEvent) -> String {
    format!(
        "{} from {} on {}",
        event.event_type().label(),
        event.source(),
        event.stream_id()
    )
}

fn count_events(events: &[ConsoleEvent], event_type: EventType) -> usize {
    events
        .iter()
        .filter(|event| event.event_type() == &event_type)
        .count()
}

fn repo_id(event: &ConsoleEvent) -> String {
    if let Some((_, repo)) = event.stream_id().rsplit_once(':') {
        return repo.to_owned();
    }
    event.stream_id().to_owned()
}

fn fabro_run_id(event: &ConsoleEvent) -> String {
    event
        .source()
        .strip_prefix("fabro:")
        .map_or_else(|| event.event_id().to_owned(), str::to_owned)
}

fn latest_timeline(
    events: &[ConsoleEvent],
    selected_stream_id: &str,
    requested_count: usize,
) -> Vec<TimelineEntry> {
    let mut matching_events = Vec::new();
    for event in events {
        if event.stream_id() == selected_stream_id {
            matching_events.push(event.clone());
        }
    }
    matching_events.sort_by_key(ConsoleEvent::stream_seq);

    let mut timeline = Vec::new();
    for event in matching_events.iter().rev().take(requested_count) {
        timeline.push(TimelineEntry::new(
            event.event_id().to_owned(),
            event.event_type().label().to_owned(),
            event.source().to_owned(),
        ));
    }
    timeline
}

trait AttentionEvent {
    fn label(&self) -> &'static str;
}

impl AttentionEvent for EventType {
    fn label(&self) -> &'static str {
        match self {
            Self::WorkItemSnapshotObserved => "Work-item snapshot",
            Self::CommandAccepted => "Command accepted",
            Self::CommandRejected => "Command rejected",
            Self::FabroHumanGateObserved => "Fabro human gate",
            Self::FactoryDrainCompleted => "Factory drain completed",
            Self::FactoryDrainFailed => "Factory drain failed",
            Self::FactoryDrainNotWired => "Factory drain not wired",
            Self::GithubPullRequestSnapshotObserved => "GitHub pull request snapshot",
            Self::LivespecNextSnapshotObserved => "LiveSpec next snapshot",
            Self::LivespecReviseRequired => "LiveSpec revise required",
            Self::DispatcherNeedsRegroomObserved => "Dispatcher needs-regroom",
            Self::FactoryDrainRequested => "Factory drain requested",
            Self::FactoryDrainStarted => "Factory drain started",
            Self::SourceCompletenessFindingObserved => "Source completeness finding",
            Self::SourceNotObservedFindingObserved => "Source not-observed finding",
        }
    }
}

#[cfg(test)]
mod tests {
    use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};
    use proptest::proptest;

    use super::source_adapters::{
        AcceptancePolicy, AdmissionPolicy, Lane, LaneReason, SourceProbe, SourceProbeOutcome,
        WorkItemSnapshot,
    };
    use super::{
        ApplicationError, AttentionDetail, AttentionEvent, DispatcherFactoryDrainPort,
        FactoryDrainPolicy, FactoryDrainPort, FactoryDrainPortOutcome, FactoryDrainRequest,
        LaneFocus, OperatorAction, OperatorActionOutcome, TuiInteraction, TuiInteractionState,
        TuiOverlay, TuiScreenModel, TuiView, build_tui_model, build_tui_model_for_state,
        handle_factory_drain_command, project_attention, project_lane_board,
        reduce_tui_interaction, resolve_command_palette_action, resolve_selected_operator_action,
        validate_operator_action,
    };

    #[test]
    fn attention_projection_is_derived_from_latest_work_item_lanes() {
        let events = [
            lane_event(
                "evt_ready",
                "console-ready",
                Lane::Ready,
                None,
                "a0",
                "ready",
            ),
            lane_event(
                "evt_blocked",
                "console-blocked",
                Lane::Blocked,
                Some(LaneReason::NeedsHuman),
                "a2",
                "blocked",
            ),
            lane_event(
                "evt_pending",
                "console-pending",
                Lane::PendingApproval,
                None,
                "a0",
                "pending-approval",
            ),
            lane_event(
                "evt_accept",
                "console-accept",
                Lane::Acceptance,
                None,
                "a1",
                "acceptance",
            ),
            ConsoleEvent::fixture("evt_revise", EventType::LivespecReviseRequired, "livespec"),
        ];

        let projected = project_attention(&events);

        assert_eq!(projected.len(), 3);
        assert_eq!(projected[0].id(), "console-pending");
        assert_eq!(projected[0].title(), "Pending approval");
        assert_eq!(projected[0].source(), "orchestrator");
        assert_eq!(projected[0].source_reference(), "console");
        assert_eq!(projected[0].next_action(), None);
        assert_eq!(projected[1].id(), "console-accept");
        assert_eq!(projected[1].title(), "Acceptance review");
        assert_eq!(projected[2].id(), "console-blocked");
        assert_eq!(projected[2].title(), "Blocked: needs-human");
    }

    #[test]
    fn attention_projection_orders_same_rank_by_work_item_id() {
        let events = [
            lane_event(
                "evt_b",
                "console-b",
                Lane::Blocked,
                Some(LaneReason::NeedsHuman),
                "a0",
                "blocked",
            ),
            lane_event(
                "evt_a",
                "console-a",
                Lane::Blocked,
                Some(LaneReason::NeedsHuman),
                "a0",
                "blocked",
            ),
        ];

        let projected = project_attention(&events);

        assert_eq!(projected[0].id(), "console-a");
        assert_eq!(projected[1].id(), "console-b");
    }

    #[test]
    fn attention_title_falls_back_to_lane_label_for_non_attention_lanes() {
        let snapshot = WorkItemSnapshot::new(
            "console",
            "console-ready",
            Lane::Ready,
            None,
            "a0",
            "ready",
            AdmissionPolicy::Manual,
            AcceptancePolicy::AiThenHuman,
            1,
        );

        assert_eq!(
            snapshot.as_ref().map(super::attention_title),
            Ok("ready".to_owned())
        );
    }

    #[test]
    fn attention_projection_uses_the_latest_snapshot_per_work_item() {
        let events = [
            lane_event(
                "evt_old",
                "console-1",
                Lane::Blocked,
                Some(LaneReason::NeedsHuman),
                "a0",
                "blocked",
            ),
            lane_event("evt_new", "console-1", Lane::Ready, None, "a0", "ready"),
        ];

        assert_eq!(project_attention(&events), []);
    }

    #[test]
    fn requires_attention_truth_table_is_lane_policy_derived() {
        for (lane, lane_reason, admission_policy, acceptance_policy, expected) in [
            (
                Lane::PendingApproval,
                None,
                AdmissionPolicy::Manual,
                AcceptancePolicy::AiThenHuman,
                true,
            ),
            (
                Lane::PendingApproval,
                None,
                AdmissionPolicy::Auto,
                AcceptancePolicy::AiThenHuman,
                false,
            ),
            (
                Lane::Acceptance,
                None,
                AdmissionPolicy::Auto,
                AcceptancePolicy::AiThenHuman,
                true,
            ),
            (
                Lane::Blocked,
                Some(LaneReason::NeedsHuman),
                AdmissionPolicy::Auto,
                AcceptancePolicy::AiThenHuman,
                true,
            ),
            (
                Lane::Blocked,
                Some(LaneReason::Dependency),
                AdmissionPolicy::Manual,
                AcceptancePolicy::AiThenHuman,
                false,
            ),
            (
                Lane::Blocked,
                None,
                AdmissionPolicy::Manual,
                AcceptancePolicy::AiThenHuman,
                false,
            ),
            (
                Lane::Backlog,
                None,
                AdmissionPolicy::Manual,
                AcceptancePolicy::AiThenHuman,
                false,
            ),
            (
                Lane::Ready,
                None,
                AdmissionPolicy::Manual,
                AcceptancePolicy::AiThenHuman,
                false,
            ),
            (
                Lane::Active,
                None,
                AdmissionPolicy::Manual,
                AcceptancePolicy::AiThenHuman,
                false,
            ),
            (
                Lane::Done,
                None,
                AdmissionPolicy::Manual,
                AcceptancePolicy::AiThenHuman,
                false,
            ),
        ] {
            assert_eq!(
                super::requires_attention_from_lane(
                    lane,
                    lane_reason,
                    admission_policy,
                    acceptance_policy,
                ),
                expected
            );
        }
    }

    // Build a snapshot-observation event by writing the canonical `payload_json`
    // directly, so the projection exercises the real deserialization path
    // without a fallible constructor in the test.
    fn lane_event(
        event_id: &str,
        work_item_id: &str,
        lane: Lane,
        lane_reason: Option<LaneReason>,
        rank: &str,
        status: &str,
    ) -> ConsoleEvent {
        lane_event_with_policies(
            event_id,
            work_item_id,
            lane,
            lane_reason,
            rank,
            status,
            AdmissionPolicy::Manual,
            AcceptancePolicy::AiThenHuman,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn lane_event_with_policies(
        event_id: &str,
        work_item_id: &str,
        lane: Lane,
        lane_reason: Option<LaneReason>,
        rank: &str,
        status: &str,
        admission_policy: AdmissionPolicy,
        acceptance_policy: AcceptancePolicy,
    ) -> ConsoleEvent {
        let reason_json = lane_reason.map_or_else(
            || "null".to_owned(),
            |reason| format!("\"{}\"", reason.label()),
        );
        let payload = format!(
            r#"{{"repo":"console","work_item_id":"{work_item_id}","lane":"{}","lane_reason":{reason_json},"rank":"{rank}","status":"{status}","admission_policy":"{}","acceptance_policy":"{}","source_version":1}}"#,
            lane.label(),
            admission_policy.label(),
            acceptance_policy.label()
        );
        ConsoleEvent::fixture(
            event_id,
            EventType::WorkItemSnapshotObserved,
            "orchestrator",
        )
        .with_payload_json(payload)
    }

    fn ready_work_item_ids(column: &super::LaneColumn) -> Vec<String> {
        column
            .items()
            .iter()
            .map(|item| item.work_item_id().to_owned())
            .collect()
    }

    #[test]
    fn lane_board_has_all_seven_lanes_in_canonical_order_when_empty() {
        let board = project_lane_board(&[]);

        let lanes: Vec<Lane> = board
            .columns()
            .iter()
            .map(super::LaneColumn::lane)
            .collect();
        assert_eq!(lanes, Lane::all().to_vec());
        assert_eq!(board.total(), 0);
        assert_eq!(
            board.column(Lane::Ready).map(super::LaneColumn::count),
            Some(0)
        );
    }

    #[test]
    fn lane_board_groups_items_and_orders_each_lane_by_rank_then_id() {
        let events = [
            lane_event("evt_a", "console-a", Lane::Ready, None, "a3", "ready"),
            lane_event("evt_b", "console-b", Lane::Ready, None, "a1", "ready"),
            // Same rank as console-b: the id breaks the tie.
            lane_event("evt_c", "console-c", Lane::Ready, None, "a1", "ready"),
            lane_event(
                "evt_d",
                "console-d",
                Lane::Blocked,
                Some(LaneReason::NeedsHuman),
                "a2",
                "blocked",
            ),
        ];

        let board = project_lane_board(&events);

        let ready = board.column(Lane::Ready);
        // Ordered by rank ("a1" < "a3") then id ("console-b" < "console-c").
        assert_eq!(
            ready.map(ready_work_item_ids),
            Some(vec![
                "console-b".to_owned(),
                "console-c".to_owned(),
                "console-a".to_owned(),
            ])
        );
        let first = &ready.map(super::LaneColumn::items).unwrap_or_default()[0];
        assert_eq!(first.rank(), "a1");
        assert_eq!(first.repo(), "console");
        assert_eq!(first.status(), "ready");
        assert_eq!(first.lane(), Lane::Ready);
        assert_eq!(first.lane_reason(), None);

        let blocked = board.column(Lane::Blocked);
        assert_eq!(blocked.map(super::LaneColumn::count), Some(1));
        let blocked_first = &blocked.map(super::LaneColumn::items).unwrap_or_default()[0];
        assert_eq!(blocked_first.lane_reason(), Some(LaneReason::NeedsHuman));
        assert_eq!(board.total(), 4);
    }

    #[test]
    fn lane_board_keeps_only_the_latest_observation_per_work_item() {
        let events = [
            // The same work-item moves ready → active; the later observation wins.
            lane_event("evt_1", "console-1", Lane::Ready, None, "a5", "ready"),
            lane_event("evt_2", "console-1", Lane::Active, None, "a5", "active"),
        ];

        let board = project_lane_board(&events);

        assert_eq!(
            board.column(Lane::Ready).map(super::LaneColumn::count),
            Some(0)
        );
        let active = board.column(Lane::Active);
        assert_eq!(active.map(super::LaneColumn::count), Some(1));
        assert_eq!(
            active
                .map(super::LaneColumn::items)
                .unwrap_or_default()
                .first()
                .map(super::LaneWorkItem::status),
            Some("active")
        );
        assert_eq!(board.total(), 1);
    }

    #[test]
    fn lane_board_skips_non_snapshot_and_unparseable_payloads() {
        let events = [
            // A different event type is not a lane source.
            ConsoleEvent::fixture("evt_gate", EventType::FabroHumanGateObserved, "fabro"),
            // A snapshot event whose payload is the empty object does not rebuild.
            ConsoleEvent::fixture(
                "evt_empty",
                EventType::WorkItemSnapshotObserved,
                "orchestrator",
            ),
            lane_event("evt_ok", "console-1", Lane::Backlog, None, "a0", "backlog"),
        ];

        let board = project_lane_board(&events);

        assert_eq!(board.total(), 1);
        assert_eq!(
            board.column(Lane::Backlog).map(super::LaneColumn::count),
            Some(1)
        );
    }

    #[test]
    fn tui_model_defaults_to_attention_with_required_navigation() {
        let model = build_tui_model(&[], 0);

        assert_eq!(model.active_view(), TuiView::Attention);
        assert_eq!(model.navigation(), TuiView::all());
        assert_eq!(model.attention_items(), []);
        assert_eq!(model.selected_attention_index(), None);
        assert_eq!(model.detail(), None);
        // The Attention view renders its attention list, not summary rows, so
        // it carries no view-summary items; the lane board is always present
        // (all seven lanes) but no lane row is selected outside the Lanes view.
        assert!(model.view_items().is_empty());
        assert_eq!(model.lane_board().columns().len(), Lane::all().len());
        assert_eq!(model.lane_focus(), super::LaneFocus::Overview);
        assert_eq!(model.selected_lane_index(), None);
        assert_eq!(model.overlay(), &TuiOverlay::None);
        assert_eq!(model.selected_operator_action(), None);
        assert_eq!(
            model.header(),
            "fleet: livespec | mode: tui | view: Attention | attention: 0"
        );
        assert_eq!(
            model.footer(),
            "shortcuts: up/down select | left/right views | enter details | / search | : command palette"
        );
    }

    #[test]
    fn tui_model_shows_lane_derived_attention_detail() {
        let model = build_tui_model(&fabro_gate_events(), 0);

        assert_eq!(model.selected_attention_index(), Some(0));
        assert_eq!(model.attention_items().len(), 3);
        assert_lane_attention_detail(&model);
        assert_lane_attention_timeline(&model);
    }

    #[test]
    fn tui_interaction_moves_attention_selection_with_arrows() {
        let events = fabro_gate_events();
        let state = TuiInteractionState::new(0, TuiOverlay::None);

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectNext);
        let model = build_tui_model_for_state(&events, &state);

        assert_eq!(state.selected_attention_index(), 1);
        assert_eq!(model.selected_attention_index(), Some(1));
        assert_eq!(
            model.detail().map(super::AttentionDetail::work_item),
            Some("console-accept")
        );

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectPrevious);
        let model = build_tui_model_for_state(&events, &state);

        assert_eq!(state.selected_attention_index(), 0);
        assert_eq!(model.selected_attention_index(), Some(0));
        assert_lane_attention_detail(&model);
    }

    #[test]
    fn tui_interaction_moves_between_required_views() {
        let events = fabro_gate_events();
        let state = TuiInteractionState::new(1, TuiOverlay::None);

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectNextView);
        let model = build_tui_model_for_state(&events, &state);

        assert_eq!(state.active_view(), TuiView::Spec);
        assert_eq!(state.selected_attention_index(), 1);
        assert_eq!(model.active_view(), TuiView::Spec);
        assert_eq!(model.view_items()[0].title(), "LiveSpec next snapshots: 0");

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectPreviousView);
        let model = build_tui_model_for_state(&events, &state);

        assert_eq!(state.active_view(), TuiView::Attention);
        assert_eq!(model.active_view(), TuiView::Attention);

        let state = TuiInteractionState::for_view(TuiView::Repos, 0, TuiOverlay::None);
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectNextView);

        assert_eq!(state.active_view(), TuiView::Repos);
    }

    #[test]
    fn tui_interaction_preserves_active_view_across_overlays() {
        let events = fabro_gate_events();
        let state = TuiInteractionState::for_view(TuiView::Events, 1, TuiOverlay::None);

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::OpenSearch);
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::TypeChar('g'));
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::Backspace);
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::CloseOverlay);
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::OpenCommandModal);
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectNextAction);
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectPreviousAction);

        assert_eq!(state.active_view(), TuiView::Events);
        assert_eq!(state.selected_attention_index(), 1);
    }

    #[test]
    fn tui_non_attention_views_project_event_summaries() {
        let events = view_summary_events();

        for (view, expected_title, expected_detail) in [
            (
                TuiView::Spec,
                "LiveSpec next snapshots: 1",
                "Spec lifecycle status is projected from LiveSpec adapter observations.",
            ),
            (
                TuiView::Events,
                "Stored events: 8",
                "The event log is the canonical source for projections.",
            ),
            (
                TuiView::Repos,
                "Repos observed: 2",
                "livespec-console-beads-fabro, other-repo",
            ),
        ] {
            let state = TuiInteractionState::for_view(view, 0, TuiOverlay::None);
            let model = build_tui_model_for_state(&events, &state);

            assert_eq!(model.active_view(), view);
            assert_eq!(model.view_items()[0].title(), expected_title);
            assert_eq!(model.view_items()[0].detail(), expected_detail);
        }
    }

    #[test]
    fn tui_lanes_view_opens_on_the_overview_home_with_the_full_board() {
        let events = [
            lane_event("evt_r", "console-r", Lane::Ready, None, "a0", "ready"),
            lane_event("evt_a", "console-a", Lane::Active, None, "a0", "active"),
        ];
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None);

        let model = build_tui_model_for_state(&events, &state);

        assert_eq!(model.active_view(), TuiView::Lanes);
        assert_eq!(model.lane_focus(), LaneFocus::Overview);
        assert_eq!(model.selected_lane_index(), Some(0));
        assert_eq!(model.lane_board().columns().len(), Lane::all().len());
        assert_eq!(model.lane_board().total(), 2);
        // The Lanes view renders the board, not summary rows.
        assert!(model.view_items().is_empty());
    }

    #[test]
    fn tui_lanes_overview_arrows_move_the_selected_lane_not_the_attention_list() {
        let events = fabro_gate_events();
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None);

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectNext);
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectNext);
        let model = build_tui_model_for_state(&events, &state);

        assert_eq!(state.selected_lane_index(), 2);
        assert_eq!(model.selected_lane_index(), Some(2));
        // The attention selection is untouched while the lane overview drives
        // the arrows.
        assert_eq!(state.selected_attention_index(), 0);

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectPrevious);

        assert_eq!(state.selected_lane_index(), 1);
    }

    #[test]
    fn tui_lanes_overview_clamps_the_selected_lane_at_the_last_lane() {
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None);

        let state = (0..20).fold(state, |state, _step| {
            reduce_tui_interaction(&state, &[], TuiInteraction::SelectNext)
        });
        let model = build_tui_model_for_state(&[], &state);

        assert_eq!(state.selected_lane_index(), Lane::all().len() - 1);
        assert_eq!(model.selected_lane_index(), Some(Lane::all().len() - 1));
    }

    #[test]
    fn tui_lanes_drill_into_selected_lane_and_return_to_overview() {
        let events = [lane_event(
            "evt_ready",
            "console-ready",
            Lane::Ready,
            None,
            "a0",
            "ready",
        )];
        // Move the selection to the third lane (Ready) and drill into it.
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None);
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectNext);
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectNext);
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::DrillIntoLane);
        let model = build_tui_model_for_state(&events, &state);

        assert_eq!(state.lane_focus(), LaneFocus::Lane(Lane::Ready));
        assert_eq!(model.lane_focus(), LaneFocus::Lane(Lane::Ready));
        // No lane row is highlighted while a lane is drilled in.
        assert_eq!(model.selected_lane_index(), None);

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::ReturnToLaneOverview);
        let model = build_tui_model_for_state(&events, &state);

        assert_eq!(state.lane_focus(), LaneFocus::Overview);
        // The overview returns to the lane it drilled in from.
        assert_eq!(model.selected_lane_index(), Some(2));
    }

    #[test]
    fn tui_events_view_reports_empty_and_latest_event_detail() {
        let state = TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None);
        let empty_model = build_tui_model_for_state(&[], &state);

        assert_eq!(empty_model.view_items()[1].detail(), "none");

        let events = view_summary_events();
        let model = build_tui_model_for_state(&events, &state);

        assert_eq!(model.view_items()[1].title(), "Latest event");
        assert_eq!(
            model.view_items()[1].detail(),
            "Factory drain failed from console:factory-command-handler on factory:livespec-console-beads-fabro"
        );
    }

    #[test]
    fn tui_interaction_clamps_selection_at_list_bounds() {
        let events = fabro_gate_events();
        let state = TuiInteractionState::new(99, TuiOverlay::None);

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectNext);
        let model = build_tui_model_for_state(&events, &state);

        assert_eq!(state.selected_attention_index(), 2);
        assert_eq!(model.selected_attention_index(), Some(2));

        let state = TuiInteractionState::new(0, TuiOverlay::None);
        let state = reduce_tui_interaction(&state, &[], TuiInteraction::SelectNext);

        assert_eq!(state.selected_attention_index(), 0);
    }

    #[test]
    fn tui_search_overlay_filters_attention_items() {
        let events = fabro_gate_events();
        let state = reduce_tui_interaction(
            &TuiInteractionState::new(0, TuiOverlay::None),
            &events,
            TuiInteraction::OpenSearch,
        );
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::TypeChar('a'));
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::TypeChar('c'));
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::TypeChar('c'));
        let model = build_tui_model_for_state(&events, &state);

        assert!(state.overlay().is_open());
        assert_eq!(state.overlay().query(), Some("acc"));
        assert_eq!(
            model
                .attention_items()
                .iter()
                .map(super::AttentionItem::id)
                .collect::<Vec<_>>(),
            ["console-accept"]
        );
        assert_eq!(
            model.detail().map(super::AttentionDetail::work_item),
            Some("console-accept")
        );
        assert_eq!(
            model.overlay(),
            &TuiOverlay::Search {
                query: "acc".to_owned()
            }
        );

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::Backspace);
        let model = build_tui_model_for_state(&events, &state);

        assert_eq!(state.overlay().query(), Some("ac"));
        assert_eq!(model.attention_items().len(), 1);
    }

    #[test]
    fn tui_search_matches_attention_title_and_work_item() {
        let events = fabro_gate_events();
        let source_state = TuiInteractionState::new(
            0,
            TuiOverlay::Search {
                query: "accept".to_owned(),
            },
        );
        let stream_state = TuiInteractionState::new(
            0,
            TuiOverlay::Search {
                query: "blocked".to_owned(),
            },
        );

        assert_eq!(
            build_tui_model_for_state(&events, &source_state)
                .attention_items()
                .len(),
            1
        );
        assert_eq!(
            build_tui_model_for_state(&events, &stream_state)
                .attention_items()
                .len(),
            1
        );
    }

    #[test]
    fn tui_command_palette_accepts_editable_query() {
        let events = fabro_gate_events();
        let state = reduce_tui_interaction(
            &TuiInteractionState::new(1, TuiOverlay::None),
            &events,
            TuiInteraction::OpenCommandPalette,
        );
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::TypeChar('d'));
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::TypeChar('r'));
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::Backspace);

        assert_eq!(state.selected_attention_index(), 1);
        assert_eq!(state.overlay().query(), Some("d"));
        assert_eq!(
            state.overlay(),
            &TuiOverlay::CommandPalette {
                query: "d".to_owned()
            }
        );
    }

    #[test]
    fn tui_command_modal_selects_attention_action() {
        let events = fabro_gate_events();
        let state = reduce_tui_interaction(
            &TuiInteractionState::new(0, TuiOverlay::None),
            &events,
            TuiInteraction::OpenCommandModal,
        );
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectNextAction);
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectNextAction);
        let model = build_tui_model_for_state(&events, &state);

        assert_eq!(state.overlay().selected_action_index(), Some(0));
        assert_eq!(model.selected_operator_action(), None);

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectPreviousAction);
        let model = build_tui_model_for_state(&events, &state);

        assert_eq!(state.overlay().selected_action_index(), Some(0));
        assert_eq!(model.selected_operator_action(), None);
    }

    #[test]
    fn tui_command_modal_clamps_to_available_actions() {
        let events = fabro_gate_events();
        let state = TuiInteractionState::new(
            1,
            TuiOverlay::CommandModal {
                selected_action_index: 99,
            },
        );
        let model = build_tui_model_for_state(&events, &state);

        assert_eq!(
            model.overlay(),
            &TuiOverlay::CommandModal {
                selected_action_index: 0
            }
        );
        assert_eq!(model.selected_operator_action(), None);
    }

    #[test]
    fn command_palette_drain_resolves_to_factory_command() {
        for query in ["drain", "Drain ready queue", "  drain  "] {
            let state = TuiInteractionState::new(
                0,
                TuiOverlay::CommandPalette {
                    query: query.to_owned(),
                },
            );
            let model = build_tui_model_for_state(&fabro_gate_events(), &state);

            let outcome = resolve_command_palette_action(&model, "operator");
            let command = outcome
                .as_ref()
                .ok()
                .and_then(super::OperatorActionOutcome::command);

            assert_eq!(
                command.map(console_domain::CommandEnvelope::command_type),
                Some(&CommandType::FactoryDrainRequested)
            );
            assert_eq!(
                command.map(console_domain::CommandEnvelope::aggregate_id),
                Some("fleet:livespec")
            );
            assert_eq!(
                command.map(console_domain::CommandEnvelope::idempotency_key),
                Some("fleet:livespec:factory.drain_requested:budget=1:parallel=1")
            );
            assert_eq!(
                command.map(console_domain::CommandEnvelope::requested_by),
                Some("operator")
            );
        }
    }

    #[test]
    fn command_palette_rejects_unknown_action() {
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::CommandPalette {
                query: "launch".to_owned(),
            },
        );
        let model = build_tui_model_for_state(&fabro_gate_events(), &state);

        let outcome = resolve_command_palette_action(&model, "operator");

        assert_eq!(outcome, Err(ApplicationError::UnknownCommandPaletteAction));
    }

    #[test]
    fn command_palette_resolution_requires_command_palette_overlay() {
        let model = build_tui_model(&fabro_gate_events(), 0);

        let outcome = resolve_command_palette_action(&model, "operator");

        assert_eq!(outcome, Err(ApplicationError::NoSelectedOperatorAction));
    }

    #[test]
    fn command_palette_resolution_rejects_blank_requester() {
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::CommandPalette {
                query: "drain".to_owned(),
            },
        );
        let model = build_tui_model_for_state(&fabro_gate_events(), &state);

        let outcome = resolve_command_palette_action(&model, " ");

        assert_eq!(outcome, Err(ApplicationError::EmptyOperatorAction));
    }

    #[test]
    fn selected_operator_action_returns_none_without_detail() {
        let model = super::TuiScreenModel {
            active_view: TuiView::Attention,
            navigation: vec![TuiView::Attention],
            attention_items: Vec::new(),
            selected_attention_index: None,
            detail: None,
            view_items: Vec::new(),
            lane_board: project_lane_board(&[]),
            lane_focus: super::LaneFocus::Overview,
            selected_lane_index: None,
            overlay: TuiOverlay::CommandModal {
                selected_action_index: 0,
            },
            header: "LiveSpec Console".to_owned(),
            footer: String::new(),
        };

        assert_eq!(model.selected_operator_action(), None);
    }

    #[test]
    fn factory_drain_handler_accepts_starts_and_completes_command() {
        let command = factory_drain_test_command();
        let mut port = CompletingDrainPort::default();

        let outcome =
            handle_factory_drain_command(&command, &ready_factory_drain_policy(), &mut port);

        assert_eq!(
            outcome
                .as_ref()
                .map(super::FactoryCommandOutcome::command_status),
            Ok("completed")
        );
        assert_eq!(
            outcome
                .as_ref()
                .map(super::FactoryCommandOutcome::events)
                .map(|events| events
                    .iter()
                    .map(ConsoleEvent::event_type)
                    .collect::<Vec<_>>()),
            Ok(vec![
                &EventType::CommandAccepted,
                &EventType::FactoryDrainStarted,
                &EventType::FactoryDrainCompleted,
            ])
        );
        assert_eq!(
            outcome
                .as_ref()
                .map(super::FactoryCommandOutcome::events)
                .map(|events| events.iter().map(ConsoleEvent::context).collect::<Vec<_>>()),
            Ok(vec!["command", "factory", "factory"])
        );
        assert_eq!(
            outcome
                .as_ref()
                .map(super::FactoryCommandOutcome::events)
                .map(|events| events[0].event_id()),
            Ok("evt_cmd_drain_accepted")
        );
        assert_eq!(
            outcome
                .as_ref()
                .map(super::FactoryCommandOutcome::events)
                .map(|events| events[2].stream_seq()),
            Ok(3)
        );
        assert_eq!(port.requests.len(), 1);
        assert_eq!(port.requests[0].aggregate_id(), "fleet:livespec");
        assert_eq!(port.requests[0].budget(), 1);
        assert_eq!(port.requests[0].parallel(), 1);
    }

    #[test]
    fn factory_drain_handler_records_not_wired_outcome_without_fabricating_start() {
        let command = factory_drain_test_command();
        let mut port = NotWiringDrainPort;

        let outcome =
            handle_factory_drain_command(&command, &ready_factory_drain_policy(), &mut port);

        assert_eq!(
            outcome
                .as_ref()
                .map(super::FactoryCommandOutcome::command_status),
            Ok("not_wired")
        );
        // An honest not-wired drain never started, so no FactoryDrainStarted
        // event is fabricated: only acceptance and the not-wired outcome.
        assert_eq!(
            outcome
                .as_ref()
                .map(super::FactoryCommandOutcome::events)
                .map(|events| events
                    .iter()
                    .map(ConsoleEvent::event_type)
                    .collect::<Vec<_>>()),
            Ok(vec![
                &EventType::CommandAccepted,
                &EventType::FactoryDrainNotWired,
            ])
        );
        assert_eq!(
            outcome
                .as_ref()
                .map(super::FactoryCommandOutcome::events)
                .map(<[ConsoleEvent]>::len),
            Ok(2)
        );
    }

    #[test]
    fn factory_command_event_context_falls_back_to_source_context() {
        assert_eq!(
            super::factory_command_event_context(EventType::SourceCompletenessFindingObserved),
            "source"
        );
    }

    #[test]
    fn factory_drain_handler_rejects_policy_invalid_command_without_invoking_port() {
        let command = factory_drain_test_command();
        let mut port = CompletingDrainPort::default();

        let outcome =
            handle_factory_drain_command(&command, &FactoryDrainPolicy::new(0), &mut port);

        assert_eq!(
            outcome
                .as_ref()
                .map(super::FactoryCommandOutcome::command_status),
            Ok("rejected")
        );
        assert_eq!(port.requests, []);
        assert_eq!(
            outcome
                .as_ref()
                .map(super::FactoryCommandOutcome::events)
                .map(|events| events
                    .iter()
                    .map(ConsoleEvent::event_type)
                    .collect::<Vec<_>>()),
            Ok(vec![&EventType::CommandRejected])
        );
        assert_eq!(
            outcome
                .as_ref()
                .map(super::FactoryCommandOutcome::events)
                .map(|events| events[0].payload_json()),
            Ok(r#"{"reason":"no ready implementation work"}"#)
        );
    }

    #[test]
    fn factory_drain_handler_records_failed_terminal_outcome() {
        let command = factory_drain_test_command();
        let mut port = FailingDrainPort;

        let outcome =
            handle_factory_drain_command(&command, &ready_factory_drain_policy(), &mut port);

        assert_eq!(
            outcome
                .as_ref()
                .map(super::FactoryCommandOutcome::command_status),
            Ok("failed")
        );
        assert_eq!(
            outcome
                .as_ref()
                .map(super::FactoryCommandOutcome::events)
                .and_then(|events| {
                    events
                        .last()
                        .map(ConsoleEvent::event_type)
                        .ok_or(&ApplicationError::NoSelectedAttentionItem)
                }),
            Ok(&EventType::FactoryDrainFailed)
        );
    }

    #[test]
    fn factory_drain_handler_propagates_port_error() {
        let command = factory_drain_test_command();
        let mut port = ErrorDrainPort;

        let outcome =
            handle_factory_drain_command(&command, &ready_factory_drain_policy(), &mut port);

        assert_eq!(outcome, Err(ApplicationError::FactoryDrainPortFailed));
    }

    #[test]
    fn operator_action_resolution_requires_selection_action_and_requester() {
        let empty_model = build_tui_model(&[], 0);
        let base_model = build_tui_model(&fabro_gate_events(), 0);

        assert_eq!(
            resolve_selected_operator_action(&empty_model, "operator"),
            Err(ApplicationError::NoSelectedAttentionItem)
        );
        assert_eq!(
            resolve_selected_operator_action(&base_model, "operator"),
            Err(ApplicationError::NoSelectedOperatorAction)
        );
        assert_eq!(
            resolve_selected_operator_action(&base_model, "  "),
            Err(ApplicationError::EmptyOperatorAction)
        );
    }

    #[test]
    fn operator_action_resolution_keeps_attach_actions_local() {
        let model = TuiScreenModel {
            active_view: TuiView::Attention,
            navigation: TuiView::all().to_vec(),
            attention_items: vec![],
            selected_attention_index: Some(0),
            detail: Some(AttentionDetail::new(
                "repo".to_owned(),
                "work-item".to_owned(),
                "run".to_owned(),
                "fabro attach run".to_owned(),
                vec![],
                vec![
                    OperatorAction::OpenFabroAttach,
                    OperatorAction::CopyFabroAttach,
                ],
            )),
            view_items: vec![],
            lane_board: project_lane_board(&[]),
            lane_focus: LaneFocus::Overview,
            selected_lane_index: Some(0),
            overlay: TuiOverlay::CommandModal {
                selected_action_index: 0,
            },
            header: String::new(),
            footer: String::new(),
        };

        let open = resolve_selected_operator_action(&model, "operator");
        let copy = resolve_selected_operator_action(
            &TuiScreenModel {
                overlay: TuiOverlay::CommandModal {
                    selected_action_index: 1,
                },
                ..model
            },
            "operator",
        );

        assert_eq!(
            open,
            Ok(OperatorActionOutcome::OpenAttachCommand(
                "fabro attach run".to_owned()
            ))
        );
        assert_eq!(
            copy,
            Ok(OperatorActionOutcome::CopyAttachCommand(
                "fabro attach run".to_owned()
            ))
        );
        assert_eq!(
            open.as_ref().ok().and_then(OperatorActionOutcome::command),
            None
        );
        assert_eq!(
            copy.as_ref()
                .ok()
                .and_then(OperatorActionOutcome::attach_command),
            Some("fabro attach run")
        );
        assert_eq!(
            OperatorActionOutcome::PersistCommand(factory_drain_test_command()).attach_command(),
            None
        );
    }

    #[test]
    fn tui_interaction_closes_overlay_and_ignores_text_outside_queries() {
        let events = fabro_gate_events();
        let state = TuiInteractionState::new(0, TuiOverlay::None);

        assert_eq!(state.overlay().query(), None);
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::TypeChar('x'));
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::Backspace);
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectNextAction);
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectPreviousAction);
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::OpenCommandModal);
        assert_eq!(state.overlay().query(), None);
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::TypeChar('x'));
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::Backspace);
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::CloseOverlay);

        assert_eq!(state.overlay(), &TuiOverlay::None);
    }

    fn fabro_gate_events() -> [ConsoleEvent; 4] {
        [
            ConsoleEvent::new(
                "evt_old".to_owned(),
                1,
                "factory".to_owned(),
                EventType::FactoryDrainRequested,
                "console".to_owned(),
                "repo:console".to_owned(),
                1,
            ),
            lane_event(
                "evt_pending",
                "console-pending",
                Lane::PendingApproval,
                None,
                "a0",
                "pending-approval",
            ),
            lane_event(
                "evt_accept",
                "console-accept",
                Lane::Acceptance,
                None,
                "a1",
                "acceptance",
            ),
            lane_event(
                "evt_blocked",
                "console-blocked",
                Lane::Blocked,
                Some(LaneReason::NeedsHuman),
                "a2",
                "blocked",
            ),
        ]
    }

    fn view_summary_events() -> [ConsoleEvent; 8] {
        [
            ConsoleEvent::new(
                "evt_gate".to_owned(),
                1,
                "factory".to_owned(),
                EventType::FabroHumanGateObserved,
                "fabro:run_17".to_owned(),
                "factory:livespec-console-beads-fabro".to_owned(),
                1,
            ),
            ConsoleEvent::new(
                "evt_regroom".to_owned(),
                1,
                "factory".to_owned(),
                EventType::DispatcherNeedsRegroomObserved,
                "dispatcher".to_owned(),
                "factory:livespec-console-beads-fabro".to_owned(),
                2,
            ),
            ConsoleEvent::new(
                "evt_spec".to_owned(),
                1,
                "spec".to_owned(),
                EventType::LivespecNextSnapshotObserved,
                "livespec:next".to_owned(),
                "console:other-repo".to_owned(),
                3,
            ),
            ConsoleEvent::new(
                "evt_revise".to_owned(),
                1,
                "spec".to_owned(),
                EventType::LivespecReviseRequired,
                "livespec:next".to_owned(),
                "console:other-repo".to_owned(),
                4,
            ),
            ConsoleEvent::new(
                "evt_ready".to_owned(),
                1,
                "orchestrator".to_owned(),
                EventType::WorkItemSnapshotObserved,
                "orchestrator:list-work-items".to_owned(),
                "factory:livespec-console-beads-fabro".to_owned(),
                5,
            ),
            ConsoleEvent::new(
                "evt_drain".to_owned(),
                1,
                "console".to_owned(),
                EventType::FactoryDrainRequested,
                "console:factory-command-handler".to_owned(),
                "factory:livespec-console-beads-fabro".to_owned(),
                6,
            ),
            ConsoleEvent::new(
                "evt_done".to_owned(),
                1,
                "console".to_owned(),
                EventType::FactoryDrainCompleted,
                "console:factory-command-handler".to_owned(),
                "factory:livespec-console-beads-fabro".to_owned(),
                7,
            ),
            ConsoleEvent::new(
                "evt_failed".to_owned(),
                1,
                "console".to_owned(),
                EventType::FactoryDrainFailed,
                "console:factory-command-handler".to_owned(),
                "factory:livespec-console-beads-fabro".to_owned(),
                8,
            ),
        ]
    }

    fn assert_lane_attention_detail(model: &super::TuiScreenModel) {
        assert_eq!(
            model.detail().map(super::AttentionDetail::repo),
            Some("console")
        );
        assert_eq!(
            model.detail().map(super::AttentionDetail::work_item),
            Some("console-pending")
        );
        assert_eq!(
            model.detail().map(super::AttentionDetail::fabro_run),
            Some("evt_pending")
        );
        assert_eq!(
            model.detail().map(super::AttentionDetail::attach_command),
            Some("fabro attach evt_pending")
        );
        assert_eq!(
            model.detail().map(super::AttentionDetail::actions),
            Some([].as_slice())
        );
    }

    fn assert_lane_attention_timeline(model: &super::TuiScreenModel) {
        assert_eq!(
            model.detail().map(|detail| detail.timeline().len()),
            Some(3)
        );
        assert_eq!(
            model
                .detail()
                .and_then(|detail| detail.timeline().first())
                .map(super::TimelineEntry::event_id),
            Some("evt_blocked")
        );
        assert_eq!(
            model
                .detail()
                .and_then(|detail| detail.timeline().first())
                .map(super::TimelineEntry::source),
            Some("orchestrator")
        );
        assert_eq!(
            model
                .detail()
                .and_then(|detail| detail.timeline().first())
                .map(super::TimelineEntry::label),
            Some("Work-item snapshot")
        );
        assert_eq!(
            model
                .detail()
                .and_then(|detail| detail.timeline().get(1))
                .map(super::TimelineEntry::event_id),
            Some("evt_accept")
        );
        assert_eq!(
            model
                .detail()
                .and_then(|detail| detail.timeline().get(2))
                .map(super::TimelineEntry::event_id),
            Some("evt_pending")
        );
    }

    #[test]
    fn source_reference_helpers_derive_repo_and_fabro_run() {
        let gate = ConsoleEvent::new(
            "evt_gate".to_owned(),
            1,
            "factory".to_owned(),
            EventType::FabroHumanGateObserved,
            "fabro:run_17".to_owned(),
            "repo:livespec-console-beads-fabro".to_owned(),
            2,
        );
        let fallback =
            ConsoleEvent::fixture("evt_no_run", EventType::LivespecReviseRequired, "livespec");
        let plain_stream = ConsoleEvent::new(
            "evt_plain".to_owned(),
            1,
            "factory".to_owned(),
            EventType::LivespecReviseRequired,
            "livespec".to_owned(),
            "livespec-console-beads-fabro".to_owned(),
            1,
        );

        assert_eq!(super::repo_id(&gate), "livespec-console-beads-fabro");
        assert_eq!(
            super::repo_id(&plain_stream),
            "livespec-console-beads-fabro"
        );
        assert_eq!(super::fabro_run_id(&gate), "run_17");
        assert_eq!(super::fabro_run_id(&fallback), "evt_no_run");
    }

    #[test]
    fn tui_model_clamps_selection_to_last_attention_item() {
        let events = [
            lane_event(
                "evt_1",
                "console-1",
                Lane::PendingApproval,
                None,
                "a0",
                "pending-approval",
            ),
            lane_event(
                "evt_2",
                "console-2",
                Lane::Blocked,
                Some(LaneReason::NeedsHuman),
                "a1",
                "blocked",
            ),
        ];

        let model = build_tui_model(&events, 99);

        assert_eq!(model.selected_attention_index(), Some(1));
        assert_eq!(
            model.detail().map(super::AttentionDetail::work_item),
            Some("console-2")
        );
        assert_eq!(
            model.detail().map(super::AttentionDetail::fabro_run),
            Some("evt_2")
        );
        assert_eq!(
            model.detail().map(super::AttentionDetail::actions),
            Some([].as_slice())
        );
    }

    #[test]
    fn navigation_and_action_labels_are_stable() {
        assert_eq!(TuiView::Attention.label(), "Attention");
        assert_eq!(TuiView::Spec.label(), "Spec");
        assert_eq!(TuiView::Lanes.label(), "Lanes");
        assert_eq!(TuiView::Events.label(), "Events");
        assert_eq!(TuiView::Repos.label(), "Repos");
        assert_eq!(OperatorAction::OpenFabroAttach.label(), "Open Fabro attach");
        assert_eq!(OperatorAction::CopyFabroAttach.label(), "Copy Fabro attach");
    }

    #[test]
    fn operator_action_validation_rejects_empty_input() {
        let result = validate_operator_action("  ");

        assert_eq!(result, Err(ApplicationError::EmptyOperatorAction));
    }

    #[test]
    fn operator_action_validation_trims_valid_requester() {
        let result = validate_operator_action("  operator  ");

        assert_eq!(result, Ok("operator"));
    }

    #[test]
    fn all_event_type_labels_are_stable() {
        assert_eq!(
            EventType::WorkItemSnapshotObserved.label(),
            "Work-item snapshot"
        );
        assert_eq!(
            EventType::DispatcherNeedsRegroomObserved.label(),
            "Dispatcher needs-regroom"
        );
        assert_eq!(
            EventType::FabroHumanGateObserved.label(),
            "Fabro human gate"
        );
        assert_eq!(EventType::CommandAccepted.label(), "Command accepted");
        assert_eq!(EventType::CommandRejected.label(), "Command rejected");
        assert_eq!(
            EventType::FactoryDrainCompleted.label(),
            "Factory drain completed"
        );
        assert_eq!(
            EventType::FactoryDrainFailed.label(),
            "Factory drain failed"
        );
        assert_eq!(
            EventType::FactoryDrainNotWired.label(),
            "Factory drain not wired"
        );
        assert_eq!(
            EventType::FactoryDrainRequested.label(),
            "Factory drain requested"
        );
        assert_eq!(
            EventType::FactoryDrainStarted.label(),
            "Factory drain started"
        );
        assert_eq!(
            EventType::GithubPullRequestSnapshotObserved.label(),
            "GitHub pull request snapshot"
        );
        assert_eq!(
            EventType::LivespecNextSnapshotObserved.label(),
            "LiveSpec next snapshot"
        );
        assert_eq!(
            EventType::LivespecReviseRequired.label(),
            "LiveSpec revise required"
        );
        assert_eq!(
            EventType::SourceCompletenessFindingObserved.label(),
            "Source completeness finding"
        );
        assert_eq!(
            EventType::SourceNotObservedFindingObserved.label(),
            "Source not-observed finding"
        );
    }

    proptest! {
        #[test]
        fn operator_action_validation_accepts_every_string_with_visible_content(
            leading in "\\s*",
            value in "[[:graph:]]+",
            trailing in "\\s*",
        ) {
            let candidate = format!("{leading}{value}{trailing}");
            let result = validate_operator_action(&candidate);

            proptest::prop_assert_eq!(result, Ok(value.as_str()));
        }

        #[test]
        fn operator_action_validation_rejects_every_whitespace_only_string(
            candidate in "\\s*",
        ) {
            let result = validate_operator_action(&candidate);

            proptest::prop_assert_eq!(result, Err(ApplicationError::EmptyOperatorAction));
        }
    }

    fn factory_drain_test_command() -> CommandEnvelope {
        CommandEnvelope::new(
            "cmd_drain".to_owned(),
            CommandType::FactoryDrainRequested,
            "fleet:livespec".to_owned(),
            "fleet:livespec:factory.drain_requested:budget=1:parallel=1".to_owned(),
            "operator".to_owned(),
        )
    }

    const fn ready_factory_drain_policy() -> FactoryDrainPolicy {
        FactoryDrainPolicy::new(1)
    }

    #[derive(Default)]
    struct CompletingDrainPort {
        requests: Vec<FactoryDrainRequest>,
    }

    impl FactoryDrainPort for CompletingDrainPort {
        fn drain_ready_queue(
            &mut self,
            request: &FactoryDrainRequest,
        ) -> super::ApplicationResult<FactoryDrainPortOutcome> {
            self.requests.push(request.clone());
            Ok(FactoryDrainPortOutcome::completed(1))
        }
    }

    struct FailingDrainPort;

    impl FactoryDrainPort for FailingDrainPort {
        fn drain_ready_queue(
            &mut self,
            _request: &FactoryDrainRequest,
        ) -> super::ApplicationResult<FactoryDrainPortOutcome> {
            Ok(FactoryDrainPortOutcome::failed())
        }
    }

    struct ErrorDrainPort;

    impl FactoryDrainPort for ErrorDrainPort {
        fn drain_ready_queue(
            &mut self,
            _request: &FactoryDrainRequest,
        ) -> super::ApplicationResult<FactoryDrainPortOutcome> {
            Err(ApplicationError::FactoryDrainPortFailed)
        }
    }

    struct NotWiringDrainPort;

    impl FactoryDrainPort for NotWiringDrainPort {
        fn drain_ready_queue(
            &mut self,
            _request: &FactoryDrainRequest,
        ) -> super::ApplicationResult<FactoryDrainPortOutcome> {
            Ok(FactoryDrainPortOutcome::not_wired())
        }
    }

    struct StubDrainProbe {
        outcome: SourceProbeOutcome,
    }

    impl SourceProbe for StubDrainProbe {
        fn run_command(&self, _program: &str, _args: &[&str]) -> SourceProbeOutcome {
            self.outcome.clone()
        }

        fn read_file(&self, _path: &str) -> SourceProbeOutcome {
            self.outcome.clone()
        }
    }

    fn drain_request() -> FactoryDrainRequest {
        FactoryDrainRequest::new("fleet:livespec".to_owned(), 1, 1)
    }

    #[test]
    fn dispatcher_drain_port_completes_with_reported_count() {
        let probe = StubDrainProbe {
            outcome: SourceProbeOutcome::observed("drain: dispatched 3 items", true),
        };
        let mut port = DispatcherFactoryDrainPort::new(&probe, "dispatcher", &["drain", "--json"]);

        let outcome = port.drain_ready_queue(&drain_request());

        assert_eq!(outcome, Ok(FactoryDrainPortOutcome::completed(3)));
    }

    #[test]
    fn dispatcher_drain_port_reports_zero_when_no_count() {
        let probe = StubDrainProbe {
            outcome: SourceProbeOutcome::observed("drain: ready queue empty", true),
        };
        let mut port = DispatcherFactoryDrainPort::new(&probe, "dispatcher", &["drain"]);

        let outcome = port.drain_ready_queue(&drain_request());

        assert_eq!(outcome, Ok(FactoryDrainPortOutcome::completed(0)));
    }

    #[test]
    fn dispatcher_drain_port_fails_on_non_zero_run() {
        let probe = StubDrainProbe {
            outcome: SourceProbeOutcome::observed("drain error", false),
        };
        let mut port = DispatcherFactoryDrainPort::new(&probe, "dispatcher", &["drain"]);

        let outcome = port.drain_ready_queue(&drain_request());

        assert_eq!(outcome, Ok(FactoryDrainPortOutcome::failed()));
    }

    #[test]
    fn dispatcher_drain_port_is_not_wired_when_unavailable() {
        let probe = StubDrainProbe {
            outcome: SourceProbeOutcome::unavailable("dispatcher binary not found"),
        };
        let mut port = DispatcherFactoryDrainPort::new(&probe, "dispatcher", &["drain"]);

        let outcome = port.drain_ready_queue(&drain_request());

        assert_eq!(outcome, Ok(FactoryDrainPortOutcome::not_wired()));
    }

    #[test]
    fn stub_drain_probe_serves_both_capabilities() {
        let probe = StubDrainProbe {
            outcome: SourceProbeOutcome::unavailable("no source"),
        };

        assert_eq!(
            probe.read_file("/unused"),
            SourceProbeOutcome::unavailable("no source")
        );
    }
}
