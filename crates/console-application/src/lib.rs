//! Application services and projections for the operator console.
//!
//! This crate folds canonical [`console_domain::ConsoleEvent`] values into the
//! TUI screen model, source-ingestion projections, operator action outcomes,
//! and factory-drain command handling policy. It is the use-case layer: it owns
//! console decisions while persistence, terminal I/O, and host command execution
//! stay behind ports.
//!
//! ```rust,ignore
//! use console_application::{build_tui_model, TuiView};
//!
//! let events = Vec::new();
//! let model = build_tui_model(&events, 0);
//! assert_eq!(model.active_view(), TuiView::Attention);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::{BTreeMap, BTreeSet};

use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};

/// Module containing source-adapters support.
pub mod source_adapters;

use source_adapters::{
    AcceptancePolicy, AdmissionPolicy, AttentionItemSnapshot, AttentionSourceRef, Lane, LaneReason,
    SourceProbe, SourceProbeOutcome, WorkItemSnapshot, materialize_attention_items,
    work_item_snapshot_from_payload_json,
};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents attention item data used by the console.
pub struct AttentionItem {
    id: String,
    title: String,
    source: String,
    source_reference: String,
    next_action: Option<OperatorAction>,
}

impl AttentionItem {
    #[must_use]
    /// Construct a new value from its required fields.
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
    /// Return the id value.
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    /// Return the title value.
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    /// Return the source value.
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    /// Return the source reference value.
    pub fn source_reference(&self) -> &str {
        &self.source_reference
    }

    #[must_use]
    /// Return the stored value.
    pub const fn next_action(&self) -> Option<OperatorAction> {
        self.next_action
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Variants for tui view state or outcome values.
pub enum TuiView {
    /// Attention variant.
    Attention,
    /// Spec variant.
    Spec,
    /// Lanes variant.
    Lanes,
    /// Events variant.
    Events,
    /// Repos variant.
    Repos,
}

impl TuiView {
    #[must_use]
    /// Return the canonical ordered set of values.
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
    /// Return the stable display label for this value.
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
    /// Overview variant.
    Overview,
    /// Lane variant.
    Lane(Lane),
}

/// Which pane the arrow keys drive.
///
/// Either the left **Views** navigation menu or the **Content** pane (the
/// active view's list). Focus starts on the Views nav so `up`/`down` walk the
/// vertical Views menu intuitively; `Enter`/`right` dive into the Content pane,
/// and `Esc`/`left` return to the nav.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPane {
    /// The left Views navigation menu (the default focus).
    Nav,
    /// The active view's content pane (its list of items or lanes).
    Content,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Variants for operator action state or outcome values.
pub enum OperatorAction {
    /// Open fabro attach variant.
    OpenFabroAttach,
    /// Copy fabro attach variant.
    CopyFabroAttach,
}

impl OperatorAction {
    #[must_use]
    /// Return the stable display label for this value.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::OpenFabroAttach => "Open Fabro attach",
            Self::CopyFabroAttach => "Copy Fabro attach",
        }
    }
}

/// One operator human-valve or policy-edit intent staged in the valve-confirm
/// modal.
///
/// The payload valves carry the mode/policy the operator has dialed in against
/// the selected work-item; approve and accept carry no payload. The valve is
/// submitted through the shared orchestrator action port when the operator
/// confirms; a destructive reject is warned before submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingValve {
    /// The approve human valve (`pending-approval -> ready`).
    Approve,
    /// The accept human valve (the human acceptance act).
    Accept,
    /// The reject valve with its routing mode (destructive).
    Reject(RejectMode),
    /// The set-admission policy dial with its dialed-in policy.
    SetAdmission(AdmissionPolicy),
    /// The set-acceptance policy dial with its dialed-in policy.
    SetAcceptance(AcceptancePolicy),
}

impl PendingValve {
    #[must_use]
    /// The stable display label for this valve.
    pub const fn valve_label(&self) -> &'static str {
        match self {
            Self::Approve => "Approve",
            Self::Accept => "Accept",
            Self::Reject(_mode) => "Reject",
            Self::SetAdmission(_policy) => "Set admission",
            Self::SetAcceptance(_policy) => "Set acceptance",
        }
    }

    #[must_use]
    /// The dialed-in mode/policy label for a payload valve, or `None` for the
    /// payload-free approve/accept valves.
    pub const fn option_label(&self) -> Option<&'static str> {
        match self {
            Self::Approve | Self::Accept => None,
            Self::Reject(mode) => Some(mode.as_str()),
            Self::SetAdmission(policy) => Some(policy.label()),
            Self::SetAcceptance(policy) => Some(policy.label()),
        }
    }

    #[must_use]
    /// Whether this valve is destructive, so the confirm modal warns before it
    /// is submitted. Only reject is destructive.
    pub const fn is_destructive(&self) -> bool {
        matches!(self, Self::Reject(_mode))
    }

    #[must_use]
    /// This valve with its mode/policy rotated one step (forward or backward).
    /// The payload-free approve/accept valves are returned unchanged.
    pub fn cycled(self, forward: bool) -> Self {
        match self {
            Self::Approve | Self::Accept => self,
            Self::Reject(mode) => Self::Reject(rotate(RejectMode::all(), mode, forward)),
            Self::SetAdmission(policy) => {
                Self::SetAdmission(rotate(AdmissionPolicy::all(), policy, forward))
            }
            Self::SetAcceptance(policy) => {
                Self::SetAcceptance(rotate(AcceptancePolicy::all(), policy, forward))
            }
        }
    }
}

/// Rotate one step through `options` from `current` (forward or backward),
/// wrapping at the ends. `current` is always one of `options`, so the fallback
/// index is never taken.
fn rotate<T: Copy + PartialEq>(options: &[T], current: T, forward: bool) -> T {
    let index = options
        .iter()
        .position(|option| *option == current)
        .unwrap_or(0);
    let len = options.len();
    let next = if forward {
        (index + 1) % len
    } else {
        (index + len - 1) % len
    };
    options[next]
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Variants for tui overlay state or outcome values.
pub enum TuiOverlay {
    /// None variant.
    None,
    /// Search variant.
    Search {
        /// Current search query text entered by the operator.
        query: String,
    },
    /// Command palette variant.
    CommandPalette {
        /// Current command-palette filter text entered by the operator.
        query: String,
    },
    /// Command modal variant.
    CommandModal {
        /// Index of the currently selected action within the modal's action list.
        selected_action_index: usize,
    },
    /// Autonomous-mode type-to-confirm variant: the dangerous enable modal that
    /// gates a `config.autonomous_mode_set` submit until the operator types the
    /// confirmation phrase (the selected repo's id). Enabling full autonomous
    /// mode is dangerous, so it is never submitted straight from the toggle.
    AutonomousModeConfirm {
        /// The confirmation text the operator has typed so far.
        typed: String,
    },
    /// Help variant: a read-only keybinding reference opened with `?` and
    /// dismissed with `Esc` or `?`. It carries no state and submits nothing.
    Help,
    /// Valve-confirm variant: the confirm modal that stages one operator
    /// human-valve/policy-edit intent against the selected work-item. `Enter`
    /// submits the valve through the shared orchestrator action port; `up`/`down`
    /// cycle a payload valve's mode/policy; `Esc` cancels. Reject is warned as
    /// dangerous before submission.
    ValveConfirm {
        /// The staged valve intent (with its dialed-in mode/policy).
        valve: PendingValve,
    },
}

impl TuiOverlay {
    #[must_use]
    /// Return whether an overlay is currently open.
    pub const fn is_open(&self) -> bool {
        !matches!(self, Self::None)
    }

    #[must_use]
    /// Return the query value.
    pub fn query(&self) -> Option<&str> {
        match self {
            Self::Search { query } | Self::CommandPalette { query } => Some(query),
            Self::None
            | Self::CommandModal { .. }
            | Self::AutonomousModeConfirm { .. }
            | Self::ValveConfirm { .. }
            | Self::Help => None,
        }
    }

    #[must_use]
    /// Return the selected action index when the overlay is a command modal.
    pub const fn selected_action_index(&self) -> Option<usize> {
        match self {
            Self::CommandModal {
                selected_action_index,
            } => Some(*selected_action_index),
            Self::None
            | Self::Search { .. }
            | Self::CommandPalette { .. }
            | Self::AutonomousModeConfirm { .. }
            | Self::ValveConfirm { .. }
            | Self::Help => None,
        }
    }

    #[must_use]
    /// Return the confirmation text typed into the autonomous-mode confirm modal,
    /// or `None` for any other overlay.
    pub fn autonomous_confirm_typed(&self) -> Option<&str> {
        match self {
            Self::AutonomousModeConfirm { typed } => Some(typed),
            Self::None
            | Self::Search { .. }
            | Self::CommandPalette { .. }
            | Self::CommandModal { .. }
            | Self::ValveConfirm { .. }
            | Self::Help => None,
        }
    }

    #[must_use]
    /// Return the staged valve when the overlay is the valve-confirm modal, or
    /// `None` for any other overlay.
    pub const fn valve_confirm(&self) -> Option<PendingValve> {
        match self {
            Self::ValveConfirm { valve } => Some(*valve),
            Self::None
            | Self::Search { .. }
            | Self::CommandPalette { .. }
            | Self::CommandModal { .. }
            | Self::AutonomousModeConfirm { .. }
            | Self::Help => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Variants for tui interaction state or outcome values.
pub enum TuiInteraction {
    /// Select next variant.
    SelectNext,
    /// Select previous variant.
    SelectPrevious,
    /// Open search variant.
    OpenSearch,
    /// Open command palette variant.
    OpenCommandPalette,
    /// Open command modal variant.
    OpenCommandModal,
    /// Close overlay variant.
    CloseOverlay,
    /// Select next view variant.
    SelectNextView,
    /// Select previous view variant.
    SelectPreviousView,
    /// Type char variant.
    TypeChar(char),
    /// Backspace variant.
    Backspace,
    /// Select next action variant.
    SelectNextAction,
    /// Select previous action variant.
    SelectPreviousAction,
    /// Drill into lane variant.
    DrillIntoLane,
    /// Return to lane overview variant.
    ReturnToLaneOverview,
    /// Open the autonomous-mode type-to-confirm modal (the dangerous enable
    /// path for the selected repo).
    OpenAutonomousModeConfirm,
    /// Move focus from the Views nav to the Content pane (the `Enter`/`right`
    /// dive-in from the nav).
    FocusContent,
    /// Move focus from the Content pane back to the Views nav (the `Esc`/`left`
    /// step-out from the content list).
    FocusNav,
    /// Open the read-only Help overlay (the `?` keybinding reference).
    OpenHelp,
    /// Open the valve-confirm modal staging the given human-valve/policy-edit
    /// intent against the selected work-item.
    OpenValveConfirm(PendingValve),
    /// Cycle the valve-confirm modal's payload valve to its next (`true`) or
    /// previous (`false`) mode/policy. Inert for a payload-free valve.
    CycleValveOption(bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents tui interaction state data used by the console.
pub struct TuiInteractionState {
    active_view: TuiView,
    selected_attention_index: usize,
    lane_focus: LaneFocus,
    selected_lane_index: usize,
    focus: FocusPane,
    overlay: TuiOverlay,
    selected_repo: String,
    autonomous_mode_enabled: bool,
}

impl TuiInteractionState {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(selected_attention_index: usize, overlay: TuiOverlay) -> Self {
        Self {
            active_view: TuiView::Attention,
            selected_attention_index,
            lane_focus: LaneFocus::Overview,
            selected_lane_index: 0,
            focus: FocusPane::Nav,
            overlay,
            selected_repo: String::new(),
            autonomous_mode_enabled: false,
        }
    }

    #[must_use]
    /// Return the stored value.
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
            focus: FocusPane::Nav,
            overlay,
            selected_repo: String::new(),
            autonomous_mode_enabled: false,
        }
    }

    /// Replace the active view, preserving every other field. Used by the
    /// interaction reducer to keep state changes single-field and readable.
    #[must_use]
    pub const fn with_active_view(mut self, active_view: TuiView) -> Self {
        self.active_view = active_view;
        self
    }

    /// Replace which pane the arrow keys drive, preserving every other field.
    #[must_use]
    pub const fn with_focus(mut self, focus: FocusPane) -> Self {
        self.focus = focus;
        self
    }

    #[must_use]
    /// Return the stored value.
    pub const fn with_selected_attention_index(mut self, selected_attention_index: usize) -> Self {
        self.selected_attention_index = selected_attention_index;
        self
    }

    #[must_use]
    /// Return the stored value.
    pub const fn with_lane_focus(mut self, lane_focus: LaneFocus) -> Self {
        self.lane_focus = lane_focus;
        self
    }

    #[must_use]
    /// Return the stored value.
    pub const fn with_selected_lane_index(mut self, selected_lane_index: usize) -> Self {
        self.selected_lane_index = selected_lane_index;
        self
    }

    #[must_use]
    /// Return this value with its overlay replaced.
    pub fn with_overlay(mut self, overlay: TuiOverlay) -> Self {
        self.overlay = overlay;
        self
    }

    #[must_use]
    /// Return this value with the selected repo replaced. The composition root
    /// sets the repo whose autonomous-mode toggle and header indicator the TUI
    /// presents.
    pub fn with_selected_repo(mut self, selected_repo: String) -> Self {
        self.selected_repo = selected_repo;
        self
    }

    #[must_use]
    /// Return this value with the selected repo's derived autonomous-mode flag
    /// replaced. The composition root derives it from the repo's `.livespec.jsonc`
    /// (an absent key is disabled) and the TUI only reflects it.
    pub const fn with_autonomous_mode_enabled(mut self, autonomous_mode_enabled: bool) -> Self {
        self.autonomous_mode_enabled = autonomous_mode_enabled;
        self
    }

    #[must_use]
    /// Return the stored value.
    pub const fn active_view(&self) -> TuiView {
        self.active_view
    }

    #[must_use]
    /// Return which pane the arrow keys currently drive.
    pub const fn focus(&self) -> FocusPane {
        self.focus
    }

    #[must_use]
    /// Return the stored value.
    pub const fn selected_attention_index(&self) -> usize {
        self.selected_attention_index
    }

    #[must_use]
    /// Return the stored value.
    pub const fn lane_focus(&self) -> LaneFocus {
        self.lane_focus
    }

    #[must_use]
    /// Return the stored value.
    pub const fn selected_lane_index(&self) -> usize {
        self.selected_lane_index
    }

    #[must_use]
    /// Return the stored value.
    pub const fn overlay(&self) -> &TuiOverlay {
        &self.overlay
    }

    #[must_use]
    /// Return the selected repo whose autonomous-mode toggle and header
    /// indicator the TUI presents.
    pub fn selected_repo(&self) -> &str {
        &self.selected_repo
    }

    #[must_use]
    /// Return the selected repo's derived autonomous-mode flag.
    pub const fn autonomous_mode_enabled(&self) -> bool {
        self.autonomous_mode_enabled
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents timeline entry data used by the console.
pub struct TimelineEntry {
    event_id: String,
    label: String,
    source: String,
}

impl TimelineEntry {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(event_id: String, label: String, source: String) -> Self {
        Self {
            event_id,
            label,
            source,
        }
    }

    #[must_use]
    /// Return the event id value.
    pub fn event_id(&self) -> &str {
        &self.event_id
    }

    #[must_use]
    /// Return the label value.
    pub fn label(&self) -> &str {
        &self.label
    }

    #[must_use]
    /// Return the source value.
    pub fn source(&self) -> &str {
        &self.source
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents attention detail data used by the console.
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
    /// Construct a new value from its required fields.
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
    /// Return the repo value.
    pub fn repo(&self) -> &str {
        &self.repo
    }

    #[must_use]
    /// Return the work item value.
    pub fn work_item(&self) -> &str {
        &self.work_item
    }

    #[must_use]
    /// Return the fabro run value.
    pub fn fabro_run(&self) -> &str {
        &self.fabro_run
    }

    #[must_use]
    /// Return the attach command value.
    pub fn attach_command(&self) -> &str {
        &self.attach_command
    }

    #[must_use]
    /// Return the timeline value.
    pub fn timeline(&self) -> &[TimelineEntry] {
        &self.timeline
    }

    #[must_use]
    /// Return the actions value.
    pub fn actions(&self) -> &[OperatorAction] {
        &self.actions
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents tui screen model data used by the console.
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
    focus: FocusPane,
    overlay: TuiOverlay,
    selected_repo: String,
    autonomous_mode_enabled: bool,
    unavailable_sources: Vec<String>,
    header: String,
    footer: String,
}

impl TuiScreenModel {
    #[must_use]
    /// Return the stored value.
    pub const fn active_view(&self) -> TuiView {
        self.active_view
    }

    #[must_use]
    /// Return the navigation value.
    pub fn navigation(&self) -> &[TuiView] {
        &self.navigation
    }

    #[must_use]
    /// Return the attention items value.
    pub fn attention_items(&self) -> &[AttentionItem] {
        &self.attention_items
    }

    #[must_use]
    /// Return the stored value.
    pub const fn selected_attention_index(&self) -> Option<usize> {
        self.selected_attention_index
    }

    #[must_use]
    /// Return the stored value.
    pub const fn detail(&self) -> Option<&AttentionDetail> {
        self.detail.as_ref()
    }

    #[must_use]
    /// Return the view items value.
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

    /// Which pane the arrow keys currently drive (the Views nav or the Content
    /// pane). Renderers use it to mark the focused pane.
    #[must_use]
    pub const fn focus(&self) -> FocusPane {
        self.focus
    }

    #[must_use]
    /// Return the stored value.
    pub const fn overlay(&self) -> &TuiOverlay {
        &self.overlay
    }

    #[must_use]
    /// Return the selected operator action.
    pub fn selected_operator_action(&self) -> Option<OperatorAction> {
        let action_index = self.overlay.selected_action_index()?;
        self.detail()?.actions().get(action_index).copied()
    }

    #[must_use]
    /// Return the selected repo whose autonomous-mode toggle and header
    /// indicator this model presents.
    pub fn selected_repo(&self) -> &str {
        &self.selected_repo
    }

    #[must_use]
    /// Return whether autonomous mode is active for the selected repo, derived
    /// from that repo's `.livespec.jsonc` (an absent key is disabled).
    pub const fn autonomous_mode_enabled(&self) -> bool {
        self.autonomous_mode_enabled
    }

    #[must_use]
    /// The backing sources that degraded to a not-observed finding this cycle,
    /// as distinct source names sorted for a stable order. These are counted and
    /// named in the header so a cockpit-blind screen (sources unreachable) is
    /// distinguishable from an idle factory (nothing actionable); empty when
    /// every source was observed.
    pub fn unavailable_sources(&self) -> &[String] {
        &self.unavailable_sources
    }

    #[must_use]
    /// Return the header value.
    pub fn header(&self) -> &str {
        &self.header
    }

    #[must_use]
    /// Compose the header to fit `width` display columns without ever truncating
    /// mid-field.
    ///
    /// A pinned small terminal (the dogfood target is 112 columns) cannot hold
    /// every header field at once, so this degrades gracefully rather than
    /// letting a wide field clip the ones after it: it elides the source-health
    /// segment's names (to `+N more`, then to a bare count) and drops the
    /// low-value constant fields (`mode: tui`, then `fleet: livespec`), before it
    /// ever drops a lower-value field (`view` — already shown highlighted in the
    /// nav pane — then the `attention` count). The `repo` and `autonomous` fields
    /// are never dropped, and — while any
    /// source is unavailable — the source COUNT is never dropped, so the header
    /// always keeps the cockpit-blind-vs-idle tell. At a width wide enough for
    /// everything this returns the same content as [`header`](Self::header).
    pub fn header_line(&self, width: usize) -> String {
        fit_header_line(
            header_repo_label(&self.selected_repo),
            autonomous_mode_header_label(self.autonomous_mode_enabled),
            self.active_view.label(),
            self.attention_items.len(),
            &self.unavailable_sources,
            width,
        )
    }

    #[must_use]
    /// Return the footer value.
    pub fn footer(&self) -> &str {
        &self.footer
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents view summary item data used by the console.
pub struct ViewSummaryItem {
    title: String,
    detail: String,
}

impl ViewSummaryItem {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(title: String, detail: String) -> Self {
        Self { title, detail }
    }

    #[must_use]
    /// Return the title value.
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    /// Return the detail value.
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Variants for application error state or outcome values.
pub enum ApplicationError {
    /// Empty operator action variant.
    EmptyOperatorAction,
    /// Empty work-item id variant -- the work-item a `work_item.*` command
    /// targets carried no non-whitespace id.
    EmptyWorkItemId,
    /// Invalid reject mode variant -- a `work_item.reject_requested` command
    /// carried a payload whose `mode` was absent or not one of {rework,
    /// regroom}.
    InvalidRejectMode,
    /// Invalid admission policy variant -- a `work_item.set_admission_requested`
    /// command carried a payload whose `policy` was absent or not one of {auto,
    /// manual}.
    InvalidAdmissionPolicy,
    /// Invalid acceptance policy variant -- a
    /// `work_item.set_acceptance_requested` command carried a payload whose
    /// `policy` was absent or not one of {ai-only, human-only, ai-then-human}.
    InvalidAcceptancePolicy,
    /// Invalid autonomous-mode payload variant -- a `config.autonomous_mode_set`
    /// command carried a payload that was malformed, missing a required
    /// `repo` / `enabled` / `confirmed` field, or carried an empty `repo`.
    InvalidAutonomousModePayload,
    /// Autonomous-mode confirmation mismatch variant -- the operator confirmed
    /// the dangerous enable modal without typing the required confirmation
    /// phrase (the selected repo's id), so the enable is not submitted.
    AutonomousModeConfirmationMismatch,
    /// Factory drain port failed variant.
    FactoryDrainPortFailed,
    /// No selected attention item variant.
    NoSelectedAttentionItem,
    /// No selected operator action variant.
    NoSelectedOperatorAction,
    /// Unknown command palette action variant.
    UnknownCommandPaletteAction,
}

/// Type alias for application result values.
pub type ApplicationResult<T> = Result<T, ApplicationError>;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Variants for operator action outcome state or outcome values.
pub enum OperatorActionOutcome {
    /// Persist command variant.
    PersistCommand(CommandEnvelope),
    /// Persist a command carrying an operator-supplied JSON payload. Used by the
    /// autonomous-mode arming command, whose `{ repo, enabled, confirmed }`
    /// payload the Configuration context reads back (the payload-less
    /// `PersistCommand` path persists an empty `{}` object, which that handler
    /// would reject).
    PersistCommandWithPayload {
        /// The command envelope to persist.
        command: CommandEnvelope,
        /// The command's `{ ... }` payload JSON.
        payload_json: String,
    },
    /// Open attach command variant.
    OpenAttachCommand(String),
    /// Copy attach command variant.
    CopyAttachCommand(String),
}

impl OperatorActionOutcome {
    #[must_use]
    /// Return the wrapped command envelope.
    pub const fn command(&self) -> Option<&CommandEnvelope> {
        match self {
            Self::PersistCommand(command) | Self::PersistCommandWithPayload { command, .. } => {
                Some(command)
            }
            Self::OpenAttachCommand(_) | Self::CopyAttachCommand(_) => None,
        }
    }

    #[must_use]
    /// Return the attach command value.
    pub fn attach_command(&self) -> Option<&str> {
        match self {
            Self::OpenAttachCommand(command) | Self::CopyAttachCommand(command) => Some(command),
            Self::PersistCommand(_) | Self::PersistCommandWithPayload { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents factory drain request data used by the console.
pub struct FactoryDrainRequest {
    aggregate_id: String,
    budget: u16,
    parallel: u16,
}

impl FactoryDrainRequest {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(aggregate_id: String, budget: u16, parallel: u16) -> Self {
        Self {
            aggregate_id,
            budget,
            parallel,
        }
    }

    #[must_use]
    /// Return the aggregate id value.
    pub fn aggregate_id(&self) -> &str {
        &self.aggregate_id
    }

    #[must_use]
    /// Return the stored value.
    pub const fn budget(&self) -> u16 {
        self.budget
    }

    #[must_use]
    /// Return the stored value.
    pub const fn parallel(&self) -> u16 {
        self.parallel
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Variants for factory drain port outcome state or outcome values.
pub enum FactoryDrainPortOutcome {
    /// Completed variant.
    Completed {
        /// Number of work-items the drain dispatched.
        dispatched_items: u16,
    },
    /// Failed variant.
    Failed,
    /// The drain was requested but no real Dispatcher port is wired, so no
    /// drain was attempted. Reported honestly instead of fabricating success.
    NotWired,
}

impl FactoryDrainPortOutcome {
    #[must_use]
    /// Return the stored value.
    pub const fn completed(dispatched_items: u16) -> Self {
        Self::Completed { dispatched_items }
    }

    #[must_use]
    /// Return the stored value.
    pub const fn failed() -> Self {
        Self::Failed
    }

    #[must_use]
    /// Return the stored value.
    pub const fn not_wired() -> Self {
        Self::NotWired
    }
}

/// Port interface for factory drain port behavior supplied by an outer layer.
pub trait FactoryDrainPort {
    /// Drain ready work from the factory through the concrete Dispatcher port.
    ///
    /// # Errors
    /// Returns an application error when the port cannot produce a trustworthy outcome.
    fn drain_ready_queue(
        &mut self,
        request: &FactoryDrainRequest,
    ) -> ApplicationResult<FactoryDrainPortOutcome>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Represents factory drain policy data used by the console.
pub struct FactoryDrainPolicy {
    ready_work_item_count: usize,
}

impl FactoryDrainPolicy {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(ready_work_item_count: usize) -> Self {
        Self {
            ready_work_item_count,
        }
    }

    #[must_use]
    /// Build this value from events input.
    pub fn from_events(events: &[ConsoleEvent]) -> Self {
        let ready_work_item_count = project_lane_board(events)
            .column(Lane::Ready)
            .map_or(0, LaneColumn::count);
        Self::new(ready_work_item_count)
    }

    #[must_use]
    /// Return the stored value.
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
///
/// This is the console's autonomous-mode LAUNCHER: on each drain it reads the
/// orchestrator's single persistent permission key from the consumer's
/// `.livespec.jsonc` ([`read_autonomous_mode_from_jsonc`]) and, WHILE that key
/// is enabled, passes `--mode autonomous` to the Dispatcher `loop` subcommand
/// for that run. The armed mode is never inferred and never persists in the
/// port -- it is re-derived from the key each run, so revoking the permission
/// immediately stops arming subsequent runs.
pub struct DispatcherFactoryDrainPort<'a> {
    probe: &'a dyn SourceProbe,
    program: String,
    args: Vec<String>,
    livespec_jsonc_path: String,
}

impl<'a> DispatcherFactoryDrainPort<'a> {
    #[must_use]
    /// Construct a new value from its required fields.
    ///
    /// `livespec_jsonc_path` is the consumer project's `.livespec.jsonc`; the
    /// port reads the orchestrator autonomous-mode permission key from it each
    /// run to decide whether to arm the drain with `--mode autonomous`.
    pub fn new(
        probe: &'a dyn SourceProbe,
        program: &str,
        args: &[&str],
        livespec_jsonc_path: &str,
    ) -> Self {
        Self {
            probe,
            program: program.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            livespec_jsonc_path: livespec_jsonc_path.to_owned(),
        }
    }

    /// Whether the orchestrator autonomous-mode permission key is enabled in the
    /// consumer's `.livespec.jsonc` right now.
    ///
    /// Re-read each drain (the armed mode is per-run and never persisted in the
    /// port). An unreadable or absent config fails soft to disabled, matching
    /// the autonomous-mode default-disabled contract.
    fn autonomous_mode_enabled(&self) -> bool {
        match self.probe.read_file(&self.livespec_jsonc_path) {
            SourceProbeOutcome::Observed {
                stdout,
                success: true,
            } => read_autonomous_mode_from_jsonc(&stdout),
            SourceProbeOutcome::Observed { success: false, .. }
            | SourceProbeOutcome::Unavailable { .. } => false,
        }
    }
}

impl FactoryDrainPort for DispatcherFactoryDrainPort<'_> {
    fn drain_ready_queue(
        &mut self,
        _request: &FactoryDrainRequest,
    ) -> ApplicationResult<FactoryDrainPortOutcome> {
        let mut arg_refs: Vec<&str> = self.args.iter().map(String::as_str).collect();
        // The armed mode rides the Dispatcher `loop` per run only while the
        // permission key is enabled; it is never inferred and never persisted.
        if self.autonomous_mode_enabled() {
            arg_refs.push("--mode");
            arg_refs.push("autonomous");
        }
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

#[derive(Debug, Clone, PartialEq, Eq)]
/// One request to run a single orchestrator `drive` action through the port.
///
/// Carries the resolved action-id the console derived from a `work_item.*`
/// command (for example `approve:<work-item-id>`); the shared port is
/// action-id-keyed so every valve/policy command rides the same surface.
pub struct OrchestratorActionRequest {
    action_id: String,
}

impl OrchestratorActionRequest {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(action_id: String) -> Self {
        Self { action_id }
    }

    #[must_use]
    /// Return the action id value.
    pub fn action_id(&self) -> &str {
        &self.action_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Variants for orchestrator action outcome state or outcome values.
pub enum OrchestratorActionOutcome {
    /// The orchestrator action completed successfully.
    Completed,
    /// The orchestrator action failed.
    Failed,
    /// The action was requested but no real orchestrator action surface is
    /// wired, so nothing was attempted. Reported honestly instead of
    /// fabricating success.
    NotWired,
}

impl OrchestratorActionOutcome {
    #[must_use]
    /// Return the stored value.
    pub const fn completed() -> Self {
        Self::Completed
    }

    #[must_use]
    /// Return the stored value.
    pub const fn failed() -> Self {
        Self::Failed
    }

    #[must_use]
    /// Return the stored value.
    pub const fn not_wired() -> Self {
        Self::NotWired
    }
}

/// Port interface for the orchestrator's published `drive` action surface,
/// supplied by an outer layer.
///
/// The single surface every `work_item.*` valve/policy command rides: the
/// console issues an action-id through it and never writes the ledger directly.
pub trait OrchestratorActionPort {
    /// Run one orchestrator action-id and return its honest outcome.
    ///
    /// # Errors
    /// Returns an application error when the port cannot produce a trustworthy outcome.
    fn run_action(
        &mut self,
        request: &OrchestratorActionRequest,
    ) -> ApplicationResult<OrchestratorActionOutcome>;
}

/// Real orchestrator-action port that invokes the orchestrator's published
/// `drive` entry point through a [`SourceProbe`].
///
/// It shells `drive --repo <path> --action <action-id>` and reflects the
/// actual outcome rather than fabricating success: a successful run completes,
/// a non-zero run fails, and an unavailable `drive` binary yields a not-wired
/// outcome. The host-backed probe is supplied by the binary, so the live valve
/// never claims an action that did not happen.
pub struct DispatcherOrchestratorActionPort<'a> {
    probe: &'a dyn SourceProbe,
    program: String,
    base_args: Vec<String>,
}

impl<'a> DispatcherOrchestratorActionPort<'a> {
    #[must_use]
    /// Construct a new value from its required fields.
    ///
    /// `base_args` are the leading arguments (for example `--repo <path>`); the
    /// port appends `--action <action-id>` for each request.
    pub fn new(probe: &'a dyn SourceProbe, program: &str, base_args: &[&str]) -> Self {
        Self {
            probe,
            program: program.to_owned(),
            base_args: base_args.iter().map(|arg| (*arg).to_owned()).collect(),
        }
    }
}

impl OrchestratorActionPort for DispatcherOrchestratorActionPort<'_> {
    fn run_action(
        &mut self,
        request: &OrchestratorActionRequest,
    ) -> ApplicationResult<OrchestratorActionOutcome> {
        let mut args: Vec<&str> = self.base_args.iter().map(String::as_str).collect();
        args.push("--action");
        args.push(request.action_id());
        Ok(match self.probe.run_command(&self.program, &args) {
            SourceProbeOutcome::Observed { success: true, .. } => {
                OrchestratorActionOutcome::completed()
            }
            SourceProbeOutcome::Observed { success: false, .. } => {
                OrchestratorActionOutcome::failed()
            }
            SourceProbeOutcome::Unavailable { .. } => OrchestratorActionOutcome::not_wired(),
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
/// Represents factory command outcome data used by the console.
pub struct FactoryCommandOutcome {
    command_status: String,
    events: Vec<ConsoleEvent>,
}

impl FactoryCommandOutcome {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(command_status: String, events: Vec<ConsoleEvent>) -> Self {
        Self {
            command_status,
            events,
        }
    }

    #[must_use]
    /// Return the command status value.
    pub fn command_status(&self) -> &str {
        &self.command_status
    }

    #[must_use]
    /// Return the events value.
    pub fn events(&self) -> &[ConsoleEvent] {
        &self.events
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents a work-item command-handling outcome: the resolved command status
/// and the shared `work_item` outcome events it appended.
pub struct WorkItemCommandOutcome {
    command_status: String,
    events: Vec<ConsoleEvent>,
}

impl WorkItemCommandOutcome {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(command_status: String, events: Vec<ConsoleEvent>) -> Self {
        Self {
            command_status,
            events,
        }
    }

    #[must_use]
    /// Return the command status value.
    pub fn command_status(&self) -> &str {
        &self.command_status
    }

    #[must_use]
    /// Return the events value.
    pub fn events(&self) -> &[ConsoleEvent] {
        &self.events
    }
}

/// Project the needs-attention inbox by folding the `attention_item.*` stream.
///
/// `appeared` / `changed` upsert an item by its stable `id`, `resolved` removes
/// it; each surviving item is then rendered, ordered by `id`. Re-sourced (v016 /
/// CN1) from the diffed `attention_item.*` stream instead of re-deriving
/// attention from work-item lane snapshots: the inbox is now the product
/// needs-attention surface the console ingests and diffs at ingest, not a single
/// work-item lane (contracts.md §"Initial Adapters"; scenarios.md Scenario 12).
#[must_use]
pub fn project_attention(events: &[ConsoleEvent]) -> Vec<AttentionItem> {
    materialize_attention_items(events)
        .iter()
        .map(attention_item_from_snapshot)
        .collect()
}

/// Render one ingested attention item into the projection entry the inbox
/// carries: its stable id, its summary as the title, its kind as the source,
/// and its composed source reference.
fn attention_item_from_snapshot(item: &AttentionItemSnapshot) -> AttentionItem {
    let source_reference = attention_source_reference(item.source_ref());
    AttentionItem::new(
        item.id().to_owned(),
        item.summary().to_owned(),
        item.kind().to_owned(),
        source_reference,
        None,
    )
}

/// Render an attention item's source reference: the repo, narrowed to a specific
/// work-item or filesystem path when the composed snapshot carries one.
fn attention_source_reference(source_ref: &AttentionSourceRef) -> String {
    match (source_ref.work_item(), source_ref.path()) {
        (Some(work_item), _) => format!("{}:{work_item}", source_ref.repo()),
        (None, Some(path)) => format!("{}:{path}", source_ref.repo()),
        (None, None) => source_ref.repo().to_owned(),
    }
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
    /// Return the work item id value.
    pub fn work_item_id(&self) -> &str {
        &self.work_item_id
    }

    #[must_use]
    /// Return the repo value.
    pub fn repo(&self) -> &str {
        &self.repo
    }

    #[must_use]
    /// Return the stored value.
    pub const fn lane(&self) -> Lane {
        self.lane
    }

    #[must_use]
    /// Return the stored value.
    pub const fn lane_reason(&self) -> Option<LaneReason> {
        self.lane_reason
    }

    #[must_use]
    /// Return the rank value.
    pub fn rank(&self) -> &str {
        &self.rank
    }

    #[must_use]
    /// Return the status value.
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
    /// Return the stored value.
    pub const fn lane(&self) -> Lane {
        self.lane
    }

    #[must_use]
    /// Return the items value.
    pub fn items(&self) -> &[LaneWorkItem] {
        &self.items
    }

    #[must_use]
    /// Return the stored value.
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
    /// Return the columns value.
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
/// Build tui model from the supplied inputs.
pub fn build_tui_model(events: &[ConsoleEvent], requested_selection: usize) -> TuiScreenModel {
    let state = TuiInteractionState::new(requested_selection, TuiOverlay::None);
    build_tui_model_for_state(events, &state)
}

#[must_use]
/// Build tui model for state from the supplied inputs.
pub fn build_tui_model_for_state(
    events: &[ConsoleEvent],
    state: &TuiInteractionState,
) -> TuiScreenModel {
    let search_query = search_query(state.overlay());
    let unavailable_sources = unavailable_sources(events);
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
        focus: state.focus(),
        overlay,
        selected_repo: state.selected_repo().to_owned(),
        autonomous_mode_enabled: state.autonomous_mode_enabled(),
        // The canonical, untruncated header. The source-health segment sits LAST
        // (after attention) so that when a narrow terminal cannot hold every
        // field, `header_line` degrades from the right — dropping the low-value
        // constants and eliding source names — while the operationally-important
        // repo / autonomous / view / attention fields survive. See `header_line`.
        header: format!(
            "fleet: livespec | mode: tui | repo: {} | autonomous: {} | view: {} | attention: {}{}",
            header_repo_label(state.selected_repo()),
            autonomous_mode_header_label(state.autonomous_mode_enabled()),
            active_view.label(),
            attention_snapshots.len(),
            source_health_header_segment(&unavailable_sources)
        ),
        unavailable_sources,
        footer: "shortcuts: up/down move focused pane (views | content) | enter dive in | esc back | left/right prev/next view | / search | : drain | a autonomous-mode (dangerous / use with caution) | valves: p approve · c accept · r reject · m set-admission · n set-acceptance | ? help | q quit"
            .to_owned(),
    }
}

/// The header's repo segment: the selected repo id, or a `-` placeholder when
/// no repo is selected (for example a preview model built with the default
/// interaction state).
fn header_repo_label(selected_repo: &str) -> &str {
    if selected_repo.trim().is_empty() {
        "-"
    } else {
        selected_repo
    }
}

/// The header's autonomous-mode segment: `on` when autonomous mode is active for
/// the selected repo, else `off`.
const fn autonomous_mode_header_label(enabled: bool) -> &'static str {
    if enabled { "on" } else { "off" }
}

/// The distinct backing-source names that degraded to a not-observed finding in
/// the current event set, sorted for a stable header order. A source appears
/// here when its adapter emitted a [`EventType::SourceNotObservedFindingObserved`]
/// event this cycle instead of an observed snapshot, so the operator can see how
/// many and which sources are unreachable rather than mistaking a cockpit-blind
/// screen for an idle factory.
fn unavailable_sources(events: &[ConsoleEvent]) -> Vec<String> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    for event in events {
        if *event.event_type() == EventType::SourceNotObservedFindingObserved {
            names.insert(event.source().to_owned());
        }
    }
    names.into_iter().collect()
}

/// The header's source-health segment: an empty string when every source was
/// observed (no phantom count on a true-empty screen), else ` | sources: N
/// unavailable (name, ...)` counting and attributing the degraded sources so a
/// false-empty is never indistinguishable from a true-empty.
fn source_health_header_segment(unavailable_sources: &[String]) -> String {
    if unavailable_sources.is_empty() {
        String::new()
    } else {
        format!(
            " | sources: {} unavailable ({})",
            unavailable_sources.len(),
            unavailable_sources.join(", ")
        )
    }
}

/// The source-health segment's degradation forms, widest first, for the header
/// fitter: full names, then the first name plus a `+N more` overflow marker,
/// then a bare count. Each is a whole, never-mid-truncated string carrying its
/// own leading ` | `; empty when every source was observed. The bare-count form
/// is always present while any source is unavailable, so the fitter can always
/// keep the cockpit-blind-vs-idle tell (how many sources are down) even when the
/// names cannot fit.
fn source_health_segment_forms(unavailable_sources: &[String]) -> Vec<String> {
    if unavailable_sources.is_empty() {
        return Vec::new();
    }
    let count = unavailable_sources.len();
    let mut forms = vec![format!(
        " | sources: {count} unavailable ({})",
        unavailable_sources.join(", ")
    )];
    // The `+N more` form only makes sense once at least one name is elided.
    if count >= 2 {
        forms.push(format!(
            " | sources: {count} unavailable ({}, +{} more)",
            unavailable_sources[0],
            count - 1
        ));
    }
    forms.push(format!(" | sources: {count} unavailable"));
    forms
}

/// The display width of a header line in terminal columns. The header is ASCII
/// (field labels, repo ids, source names), so a char count is its column width.
fn header_display_width(line: &str) -> usize {
    line.chars().count()
}

/// One shrink step for the header fitter: drop the field at the given index, or
/// step the source-health segment down to its next-narrower form.
enum Shrink {
    DropField(usize),
    DegradeSource,
}

/// Compose the width-fitted header. See [`TuiScreenModel::header_line`] for the
/// degradation contract. This is the pure core: it composes the atomic fields in
/// a fixed display order and, while the line is over `width`, applies the shrink
/// plan one step at a time — eliding source names, then dropping the constant
/// `mode`/`fleet` fields, then the lower-value `view`/`attention` fields —
/// re-measuring after each step and stopping as soon as it fits. `repo` and
/// `autonomous` are never dropped.
fn fit_header_line(
    repo: &str,
    autonomous: &str,
    view: &str,
    attention: usize,
    unavailable_sources: &[String],
    width: usize,
) -> String {
    // Fixed display order; `Some` = present, `None` = dropped to make room. Each
    // field is atomic — kept or dropped whole, never mid-truncated.
    let mut fields: [Option<String>; 6] = [
        Some("fleet: livespec".to_owned()),        // 0 — constant identity
        Some("mode: tui".to_owned()),              // 1 — constant
        Some(format!("repo: {repo}")),             // 2 — never dropped
        Some(format!("autonomous: {autonomous}")), // 3 — never dropped
        Some(format!("view: {view}")),             // 4
        Some(format!("attention: {attention}")),   // 5
    ];
    let source_forms = source_health_segment_forms(unavailable_sources);
    let mut source_idx = 0usize; // 0 = widest (full names)

    let compose = |fields: &[Option<String>; 6], source_idx: usize| -> String {
        let mut line = fields
            .iter()
            .filter_map(|field| field.as_deref())
            .collect::<Vec<_>>()
            .join(" | ");
        if let Some(source) = source_forms.get(source_idx) {
            line.push_str(source);
        }
        line
    };

    // One shrink op per over-budget step, least valuable first. The constant
    // fields are dropped before the source names are elided; the source COUNT
    // outlives `view`/`attention` because those drops come last. `view` goes
    // before `attention` because the active view is already shown, highlighted,
    // in the nav pane, whereas the attention count appears nowhere else.
    let plan = [
        Shrink::DropField(1),  // mode: tui
        Shrink::DropField(0),  // fleet: livespec
        Shrink::DegradeSource, // full names -> +N more
        Shrink::DegradeSource, // +N more -> count only
        Shrink::DropField(4),  // view (already shown, highlighted, in the nav pane)
        Shrink::DropField(5),  // attention count
    ];

    let mut line = compose(&fields, source_idx);
    for op in &plan {
        if header_display_width(&line) <= width {
            break;
        }
        match *op {
            Shrink::DropField(index) => fields[index] = None,
            Shrink::DegradeSource => {
                if source_idx + 1 < source_forms.len() {
                    source_idx += 1;
                }
            }
        }
        line = compose(&fields, source_idx);
    }
    line
}

#[must_use]
/// Return the reduce tui interaction value.
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
        TuiInteraction::OpenAutonomousModeConfirm => {
            state
                .clone()
                .with_overlay(TuiOverlay::AutonomousModeConfirm {
                    typed: String::new(),
                })
        }
        TuiInteraction::FocusContent => state.clone().with_focus(FocusPane::Content),
        TuiInteraction::FocusNav => state.clone().with_focus(FocusPane::Nav),
        TuiInteraction::OpenHelp => state.clone().with_overlay(TuiOverlay::Help),
        TuiInteraction::OpenValveConfirm(valve) => state
            .clone()
            .with_overlay(TuiOverlay::ValveConfirm { valve }),
        TuiInteraction::CycleValveOption(forward) => state
            .clone()
            .with_overlay(cycle_valve_option(state.overlay(), forward)),
    }
}

/// Rotate the valve-confirm modal's payload valve one step (forward or
/// backward), leaving any non-valve overlay unchanged.
fn cycle_valve_option(overlay: &TuiOverlay, forward: bool) -> TuiOverlay {
    overlay.valve_confirm().map_or_else(
        || overlay.clone(),
        |valve| TuiOverlay::ValveConfirm {
            valve: valve.cycled(forward),
        },
    )
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

/// Validate operator action.
pub fn validate_operator_action(action: &str) -> ApplicationResult<&str> {
    let trimmed = action.trim();
    if trimmed.is_empty() {
        return Err(ApplicationError::EmptyOperatorAction);
    }
    Ok(trimmed)
}

/// Resolve selected operator action.
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

/// Whether the operator's typed confirmation phrase matches the required phrase.
///
/// The required phrase to enable autonomous mode for `repo` is the repo's own
/// id, so the operator must type the exact repo they are arming. An empty repo
/// can never match.
#[must_use]
pub fn autonomous_mode_confirmation_matches(typed: &str, repo: &str) -> bool {
    !repo.trim().is_empty() && typed.trim() == repo.trim()
}

/// Resolve the autonomous-mode ENABLE submit from the type-to-confirm modal.
///
/// Enabling full autonomous mode is dangerous, so the submit is gated: it is
/// produced only when the overlay is the autonomous-mode confirm modal AND the
/// operator typed the confirmation phrase (the selected repo's id). The
/// resulting command carries `{ repo, enabled: true, confirmed: true }`.
///
/// # Errors
/// Returns [`ApplicationError::EmptyOperatorAction`] when `requested_by` is
/// blank, [`ApplicationError::NoSelectedOperatorAction`] when the overlay is not
/// the confirm modal, and [`ApplicationError::AutonomousModeConfirmationMismatch`]
/// when the typed phrase does not match -- in which case the enable is not
/// submitted.
pub fn resolve_autonomous_mode_enable(
    model: &TuiScreenModel,
    requested_by: &str,
) -> ApplicationResult<OperatorActionOutcome> {
    validate_operator_action(requested_by)?;
    let typed = model
        .overlay()
        .autonomous_confirm_typed()
        .ok_or(ApplicationError::NoSelectedOperatorAction)?;
    let repo = model.selected_repo();
    if !autonomous_mode_confirmation_matches(typed, repo) {
        return Err(ApplicationError::AutonomousModeConfirmationMismatch);
    }
    Ok(autonomous_mode_set_outcome(repo, true, true, requested_by))
}

/// Resolve the autonomous-mode DISABLE submit for the selected repo.
///
/// Disabling requires no confirmation, so it is produced directly (no modal).
/// The resulting command carries `{ repo, enabled: false, confirmed: false }`.
///
/// # Errors
/// Returns [`ApplicationError::EmptyOperatorAction`] when `requested_by` is
/// blank and [`ApplicationError::InvalidAutonomousModePayload`] when no repo is
/// selected.
pub fn resolve_autonomous_mode_disable(
    model: &TuiScreenModel,
    requested_by: &str,
) -> ApplicationResult<OperatorActionOutcome> {
    validate_operator_action(requested_by)?;
    let repo = model.selected_repo();
    if repo.trim().is_empty() {
        return Err(ApplicationError::InvalidAutonomousModePayload);
    }
    Ok(autonomous_mode_set_outcome(
        repo,
        false,
        false,
        requested_by,
    ))
}

/// Build the `config.autonomous_mode_set` persist outcome for `repo`, carrying
/// the `{ repo, enabled, confirmed }` payload the Configuration context reads.
fn autonomous_mode_set_outcome(
    repo: &str,
    enabled: bool,
    confirmed: bool,
    requested_by: &str,
) -> OperatorActionOutcome {
    let command = CommandEnvelope::new(
        format!("cmd_autonomous_mode_set_{repo}_{enabled}"),
        CommandType::ConfigAutonomousModeSet,
        repo.to_owned(),
        format!("{repo}:config.autonomous_mode_set:enabled={enabled}"),
        requested_by.to_owned(),
    );
    let payload_json = serde_json::json!({
        "repo": repo,
        "enabled": enabled,
        "confirmed": confirmed,
    })
    .to_string();
    OperatorActionOutcome::PersistCommandWithPayload {
        command,
        payload_json,
    }
}

/// Resolve command palette action.
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

/// Resolve the valve submit from the valve-confirm modal.
///
/// The modal stages one human-valve/policy-edit intent ([`PendingValve`])
/// against the selected work-item; this reads the staged valve and the selected
/// attention item's work-item id, and produces the persist outcome for the
/// matching `work_item.*` command. Approve and accept persist a payload-less
/// command; reject, set-admission, and set-acceptance persist the `{"mode": ...}`
/// / `{"policy": ...}` payload their handlers parse. The console never writes the
/// ledger directly -- the persisted command rides the shared
/// [`OrchestratorActionPort`] `drive` surface.
///
/// # Errors
/// Returns [`ApplicationError::EmptyOperatorAction`] when `requested_by` is
/// blank, [`ApplicationError::NoSelectedOperatorAction`] when the overlay is not
/// the valve-confirm modal, and [`ApplicationError::NoSelectedAttentionItem`]
/// when nothing is selected. The selected item's work-item id is carried
/// verbatim as the command aggregate; the orchestrator's `drive` surface (and
/// the downstream `work_item.*` handler) is the authority on its legality.
pub fn resolve_valve_action(
    model: &TuiScreenModel,
    requested_by: &str,
) -> ApplicationResult<OperatorActionOutcome> {
    let requested_by = validate_operator_action(requested_by)?;
    let valve = model
        .overlay()
        .valve_confirm()
        .ok_or(ApplicationError::NoSelectedOperatorAction)?;
    let detail = model
        .detail()
        .ok_or(ApplicationError::NoSelectedAttentionItem)?;
    Ok(valve_outcome(valve, detail.work_item(), requested_by))
}

/// Build the persist outcome for one staged valve against `work_item_id`.
fn valve_outcome(
    valve: PendingValve,
    work_item_id: &str,
    requested_by: &str,
) -> OperatorActionOutcome {
    match valve {
        PendingValve::Approve => OperatorActionOutcome::PersistCommand(work_item_command(
            "approve",
            CommandType::WorkItemApproveRequested,
            work_item_id,
            requested_by,
        )),
        PendingValve::Accept => OperatorActionOutcome::PersistCommand(work_item_command(
            "accept",
            CommandType::WorkItemAcceptRequested,
            work_item_id,
            requested_by,
        )),
        PendingValve::Reject(mode) => work_item_payload_outcome(
            "reject",
            CommandType::WorkItemRejectRequested,
            work_item_id,
            "mode",
            mode.as_str(),
            requested_by,
        ),
        PendingValve::SetAdmission(policy) => work_item_payload_outcome(
            "set_admission",
            CommandType::WorkItemSetAdmissionRequested,
            work_item_id,
            "policy",
            policy.label(),
            requested_by,
        ),
        PendingValve::SetAcceptance(policy) => work_item_payload_outcome(
            "set_acceptance",
            CommandType::WorkItemSetAcceptanceRequested,
            work_item_id,
            "policy",
            policy.label(),
            requested_by,
        ),
    }
}

/// Build a payload-less `work_item.<action>_requested` command envelope keyed by
/// the target work-item id (the aggregate the orchestrator's `drive` surface
/// acts on).
fn work_item_command(
    action: &str,
    command_type: CommandType,
    work_item_id: &str,
    requested_by: &str,
) -> CommandEnvelope {
    CommandEnvelope::new(
        format!("cmd_work_item_{action}_requested_{work_item_id}"),
        command_type,
        work_item_id.to_owned(),
        format!("{work_item_id}:work_item.{action}_requested"),
        requested_by.to_owned(),
    )
}

/// Build the persist-with-payload outcome for a payload-bearing valve: the
/// `work_item.<action>_requested` command plus its single-key `{ "<key>":
/// "<value>" }` payload (the `mode` / `policy` the handler parses).
fn work_item_payload_outcome(
    action: &str,
    command_type: CommandType,
    work_item_id: &str,
    key: &str,
    value: &str,
    requested_by: &str,
) -> OperatorActionOutcome {
    let command = CommandEnvelope::new(
        format!("cmd_work_item_{action}_requested_{work_item_id}_{value}"),
        command_type,
        work_item_id.to_owned(),
        format!("{work_item_id}:work_item.{action}_requested:{key}={value}"),
        requested_by.to_owned(),
    );
    let mut payload = serde_json::Map::new();
    payload.insert(key.to_owned(), serde_json::Value::String(value.to_owned()));
    OperatorActionOutcome::PersistCommandWithPayload {
        command,
        payload_json: serde_json::Value::Object(payload).to_string(),
    }
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

/// Handle factory drain command.
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
        command_event_context(event_type).to_owned(),
        event_type,
        "console:factory-command-handler".to_owned(),
        command.aggregate_id().to_owned(),
        stream_seq,
    )
}

const fn command_event_context(event_type: EventType) -> &'static str {
    match event_type {
        EventType::CommandAccepted | EventType::CommandRejected => "command",
        EventType::FactoryDrainCompleted
        | EventType::FactoryDrainFailed
        | EventType::FactoryDrainNotWired
        | EventType::FactoryDrainRequested
        | EventType::FactoryDrainStarted
        | EventType::FactoryAutonomousModeEnableRequested
        | EventType::FactoryAutonomousModeDisableRequested
        | EventType::FactoryAutonomousModeNotWired => "factory",
        EventType::WorkItemActionStarted
        | EventType::WorkItemActionCompleted
        | EventType::WorkItemActionFailed
        | EventType::WorkItemActionNotWired => "work_item",
        EventType::ConfigAutonomousModeEnabled | EventType::ConfigAutonomousModeDisabled => {
            "configuration"
        }
        EventType::WorkItemSnapshotObserved
        | EventType::DispatcherBacklogBounceObserved
        | EventType::FabroHumanGateObserved
        | EventType::GithubPullRequestSnapshotObserved
        | EventType::LivespecNextSnapshotObserved
        | EventType::LivespecReviseRequired
        | EventType::SourceCompletenessFindingObserved
        | EventType::SourceNotObservedFindingObserved
        | EventType::AttentionItemAppeared
        | EventType::AttentionItemChanged
        | EventType::AttentionItemResolved => "source",
    }
}

/// Validate the work-item id a `work_item.*` command targets.
///
/// Thin console-side validation: the id must carry non-whitespace text. The
/// orchestrator's `drive` surface is the authority on state-legality, so the
/// console does not pre-check the item's lane -- it issues the command and
/// observes the lane change on a subsequent poll.
fn validate_work_item_id(value: &str) -> ApplicationResult<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ApplicationError::EmptyWorkItemId);
    }
    Ok(trimmed)
}

/// The mode a `work_item.reject_requested` command carries in its payload,
/// selecting where the orchestrator routes the rejected item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectMode {
    /// Send the item back for rework.
    Rework,
    /// Send the item back to be regroomed.
    Regroom,
}

impl RejectMode {
    #[must_use]
    /// The canonical ordered set of reject modes (rework, then regroom).
    pub const fn all() -> &'static [Self] {
        &[Self::Rework, Self::Regroom]
    }

    #[must_use]
    /// The action-id segment for this mode (`rework` or `regroom`).
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Rework => "rework",
            Self::Regroom => "regroom",
        }
    }

    /// Parse a mode string, rejecting any value outside {rework, regroom}.
    ///
    /// # Errors
    /// Returns [`ApplicationError::InvalidRejectMode`] when `value` is not one
    /// of the two valid modes.
    pub fn parse(value: &str) -> ApplicationResult<Self> {
        match value {
            "rework" => Ok(Self::Rework),
            "regroom" => Ok(Self::Regroom),
            _other => Err(ApplicationError::InvalidRejectMode),
        }
    }
}

/// Handle a `work_item.approve_requested` command.
///
/// Approve is the human approval act (`pending-approval -> ready`). The handler
/// validates the work-item id, derives the `approve:<work-item-id>` action-id,
/// runs it through the shared orchestrator-action port, and appends the shared
/// `work_item` outcome events. It never writes the ledger directly and never
/// fabricates the lane transition -- the orchestrator owns that.
///
/// # Errors
/// Returns an application error when the work-item id is empty or the port
/// cannot produce a trustworthy outcome.
pub fn handle_work_item_approve_command(
    command: &CommandEnvelope,
    port: &mut dyn OrchestratorActionPort,
) -> ApplicationResult<WorkItemCommandOutcome> {
    let work_item_id = validate_work_item_id(command.aggregate_id())?;
    let action_id = format!("approve:{work_item_id}");
    run_work_item_action(command, &action_id, port)
}

/// Handle a `work_item.accept_requested` command.
///
/// Accept is the human acceptance act. The handler validates the work-item id,
/// derives the `accept:<work-item-id>` action-id (no payload), and rides the
/// same shared orchestrator-action port and `work_item` outcome family as
/// approve. It never writes the ledger directly.
///
/// # Errors
/// Returns an application error when the work-item id is empty or the port
/// cannot produce a trustworthy outcome.
pub fn handle_work_item_accept_command(
    command: &CommandEnvelope,
    port: &mut dyn OrchestratorActionPort,
) -> ApplicationResult<WorkItemCommandOutcome> {
    let work_item_id = validate_work_item_id(command.aggregate_id())?;
    let action_id = format!("accept:{work_item_id}");
    run_work_item_action(command, &action_id, port)
}

/// Handle a `work_item.reject_requested` command.
///
/// Reject is the first work-item command carrying a payload beyond the
/// aggregate id: `payload_json` is `{"mode": "rework" | "regroom"}`. The handler
/// validates the work-item id, parses and validates the mode enum, derives the
/// `reject:<work-item-id>:<mode>` action-id, and rides the shared
/// orchestrator-action port and `work_item` outcome family. Thin console-side
/// validation only -- the orchestrator's `drive` surface owns state-legality --
/// and it never writes the ledger directly.
///
/// # Errors
/// Returns [`ApplicationError::EmptyWorkItemId`] when the id is empty and
/// [`ApplicationError::InvalidRejectMode`] when the payload's `mode` is absent
/// or invalid; also surfaces a port error when the port cannot produce a
/// trustworthy outcome.
pub fn handle_work_item_reject_command(
    command: &CommandEnvelope,
    payload_json: &str,
    port: &mut dyn OrchestratorActionPort,
) -> ApplicationResult<WorkItemCommandOutcome> {
    let work_item_id = validate_work_item_id(command.aggregate_id())?;
    let mode = reject_mode_from_payload(payload_json)?;
    let action_id = format!("reject:{work_item_id}:{}", mode.as_str());
    run_work_item_action(command, &action_id, port)
}

/// Extract the reject `mode` from a command's persisted `payload_json`.
///
/// The payload is the JSON object `{"mode": "rework" | "regroom"}`; any other
/// shape is an [`ApplicationError::InvalidRejectMode`].
fn reject_mode_from_payload(payload_json: &str) -> ApplicationResult<RejectMode> {
    let value: serde_json::Value =
        serde_json::from_str(payload_json).map_err(|_error| ApplicationError::InvalidRejectMode)?;
    let mode = value
        .get("mode")
        .and_then(serde_json::Value::as_str)
        .ok_or(ApplicationError::InvalidRejectMode)?;
    RejectMode::parse(mode)
}

/// Handle a `work_item.set_admission_requested` command.
///
/// Set-admission is the admission policy dial: `payload_json` is
/// `{"policy": "auto" | "manual"}`. The handler validates the work-item id,
/// parses and validates the policy enum, derives the
/// `set-admission:<work-item-id>:<policy>` action-id, and rides the shared
/// orchestrator-action port and `work_item` outcome family exactly like the
/// reject command. A policy edit never moves the item between lifecycle states:
/// the console only issues the command and emits the `work_item.action.*`
/// events, observing any effect on a subsequent poll. Thin console-side
/// validation only -- the orchestrator's `drive` surface owns state-legality --
/// and it never writes the ledger directly.
///
/// # Errors
/// Returns [`ApplicationError::EmptyWorkItemId`] when the id is empty and
/// [`ApplicationError::InvalidAdmissionPolicy`] when the payload's `policy` is
/// absent or invalid; also surfaces a port error when the port cannot produce a
/// trustworthy outcome.
pub fn handle_work_item_set_admission_command(
    command: &CommandEnvelope,
    payload_json: &str,
    port: &mut dyn OrchestratorActionPort,
) -> ApplicationResult<WorkItemCommandOutcome> {
    let work_item_id = validate_work_item_id(command.aggregate_id())?;
    let policy = set_admission_policy_from_payload(payload_json)?;
    let action_id = format!("set-admission:{work_item_id}:{}", policy.label());
    run_work_item_action(command, &action_id, port)
}

/// Extract the admission `policy` from a command's persisted `payload_json`.
///
/// The payload is the JSON object `{"policy": "auto" | "manual"}`; the value is
/// deserialized through the read-side [`AdmissionPolicy`] enum (kebab-case), so
/// the command dial and the snapshot dial share one source of truth. Any other
/// shape is an [`ApplicationError::InvalidAdmissionPolicy`].
fn set_admission_policy_from_payload(payload_json: &str) -> ApplicationResult<AdmissionPolicy> {
    let value: serde_json::Value = serde_json::from_str(payload_json)
        .map_err(|_error| ApplicationError::InvalidAdmissionPolicy)?;
    let policy = value
        .get("policy")
        .ok_or(ApplicationError::InvalidAdmissionPolicy)?;
    serde_json::from_value(policy.clone())
        .map_err(|_error| ApplicationError::InvalidAdmissionPolicy)
}

/// Handle a `work_item.set_acceptance_requested` command.
///
/// Set-acceptance is the acceptance policy dial: `payload_json` is
/// `{"policy": "ai-only" | "human-only" | "ai-then-human"}`. The handler
/// validates the work-item id, parses and validates the policy enum, derives the
/// `set-acceptance:<work-item-id>:<policy>` action-id, and rides the shared
/// orchestrator-action port and `work_item` outcome family exactly like the
/// set-admission command. A policy edit never moves the item between lifecycle
/// states: the console only issues the command and emits the `work_item.action.*`
/// events, observing any effect on a subsequent poll. Thin console-side
/// validation only -- the orchestrator's `drive` surface owns state-legality --
/// and it never writes the ledger directly.
///
/// # Errors
/// Returns [`ApplicationError::EmptyWorkItemId`] when the id is empty and
/// [`ApplicationError::InvalidAcceptancePolicy`] when the payload's `policy` is
/// absent or invalid; also surfaces a port error when the port cannot produce a
/// trustworthy outcome.
pub fn handle_work_item_set_acceptance_command(
    command: &CommandEnvelope,
    payload_json: &str,
    port: &mut dyn OrchestratorActionPort,
) -> ApplicationResult<WorkItemCommandOutcome> {
    let work_item_id = validate_work_item_id(command.aggregate_id())?;
    let policy = set_acceptance_policy_from_payload(payload_json)?;
    let action_id = format!("set-acceptance:{work_item_id}:{}", policy.label());
    run_work_item_action(command, &action_id, port)
}

/// Extract the acceptance `policy` from a command's persisted `payload_json`.
///
/// The payload is the JSON object
/// `{"policy": "ai-only" | "human-only" | "ai-then-human"}`; the value is
/// deserialized through the read-side [`AcceptancePolicy`] enum (kebab-case), so
/// the command dial and the snapshot dial share one source of truth. Any other
/// shape is an [`ApplicationError::InvalidAcceptancePolicy`].
fn set_acceptance_policy_from_payload(payload_json: &str) -> ApplicationResult<AcceptancePolicy> {
    let value: serde_json::Value = serde_json::from_str(payload_json)
        .map_err(|_error| ApplicationError::InvalidAcceptancePolicy)?;
    let policy = value
        .get("policy")
        .ok_or(ApplicationError::InvalidAcceptancePolicy)?;
    serde_json::from_value(policy.clone())
        .map_err(|_error| ApplicationError::InvalidAcceptancePolicy)
}

/// Run one resolved work-item action-id through the port and emit the shared
/// `work_item` outcome events keyed by that action-id. Shared by every
/// `work_item.*` command handler.
fn run_work_item_action(
    command: &CommandEnvelope,
    action_id: &str,
    port: &mut dyn OrchestratorActionPort,
) -> ApplicationResult<WorkItemCommandOutcome> {
    let request = OrchestratorActionRequest::new(action_id.to_owned());
    let port_outcome = port.run_action(&request)?;
    let mut events = vec![work_item_command_event(
        command,
        EventType::CommandAccepted,
        "accepted",
        action_id,
        1,
    )];
    let command_status = match port_outcome {
        OrchestratorActionOutcome::Completed => {
            events.push(work_item_command_event(
                command,
                EventType::WorkItemActionStarted,
                "started",
                action_id,
                2,
            ));
            events.push(work_item_command_event(
                command,
                EventType::WorkItemActionCompleted,
                "completed",
                action_id,
                3,
            ));
            "completed"
        }
        OrchestratorActionOutcome::Failed => {
            events.push(work_item_command_event(
                command,
                EventType::WorkItemActionStarted,
                "started",
                action_id,
                2,
            ));
            events.push(work_item_command_event(
                command,
                EventType::WorkItemActionFailed,
                "failed",
                action_id,
                3,
            ));
            "failed"
        }
        OrchestratorActionOutcome::NotWired => {
            // No real action surface is wired, so the action never started.
            // Emit an honest not-wired outcome rather than a fabricated one.
            events.push(work_item_command_event(
                command,
                EventType::WorkItemActionNotWired,
                "not_wired",
                action_id,
                2,
            ));
            "not_wired"
        }
    };
    Ok(WorkItemCommandOutcome::new(
        command_status.to_owned(),
        events,
    ))
}

/// Build one shared `work_item` outcome event, carrying the `action_id` in its
/// payload so the family is keyed by action-id across every `work_item.*`
/// command.
fn work_item_command_event(
    command: &CommandEnvelope,
    event_type: EventType,
    suffix: &str,
    action_id: &str,
    stream_seq: u64,
) -> ConsoleEvent {
    ConsoleEvent::new(
        format!("evt_{}_{}", command.command_id(), suffix),
        1,
        command_event_context(event_type).to_owned(),
        event_type,
        "console:work-item-command-handler".to_owned(),
        command.aggregate_id().to_owned(),
        stream_seq,
    )
    .with_payload_json(
        serde_json::json!({
            "action_id": action_id,
        })
        .to_string(),
    )
}

// ---------------------------------------------------------------------------
// Configuration context — full autonomous mode arming.
// ---------------------------------------------------------------------------

/// The nested key path of the orchestrator's autonomous-mode permission inside a
/// consumer project's `.livespec.jsonc`:
/// `livespec-orchestrator-beads-fabro.dispatcher.autonomous_mode`.
const ORCHESTRATOR_CONFIG_KEY: &str = "livespec-orchestrator-beads-fabro";
const DISPATCHER_CONFIG_KEY: &str = "dispatcher";
const AUTONOMOUS_MODE_CONFIG_KEY: &str = "autonomous_mode";

/// The parsed `{ repo, enabled, confirmed }` payload of a
/// `config.autonomous_mode_set` command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousModeSetRequest {
    repo: String,
    enabled: bool,
    confirmed: bool,
}

impl AutonomousModeSetRequest {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(repo: String, enabled: bool, confirmed: bool) -> Self {
        Self {
            repo,
            enabled,
            confirmed,
        }
    }

    #[must_use]
    /// Return the target repo id.
    pub fn repo(&self) -> &str {
        &self.repo
    }

    #[must_use]
    /// Whether the command requests autonomous mode enabled.
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    /// Whether the operator explicitly confirmed the change.
    pub const fn confirmed(&self) -> bool {
        self.confirmed
    }

    /// Parse the `{ repo, enabled, confirmed }` payload from a command's
    /// persisted `payload_json`.
    ///
    /// # Errors
    /// Returns [`ApplicationError::InvalidAutonomousModePayload`] when the JSON
    /// is malformed, a required field is absent or the wrong type, or `repo` is
    /// empty.
    pub fn from_payload_json(payload_json: &str) -> ApplicationResult<Self> {
        let value: serde_json::Value = serde_json::from_str(payload_json)
            .map_err(|_error| ApplicationError::InvalidAutonomousModePayload)?;
        let repo = value
            .get("repo")
            .and_then(serde_json::Value::as_str)
            .ok_or(ApplicationError::InvalidAutonomousModePayload)?;
        if repo.trim().is_empty() {
            return Err(ApplicationError::InvalidAutonomousModePayload);
        }
        let enabled = value
            .get("enabled")
            .and_then(serde_json::Value::as_bool)
            .ok_or(ApplicationError::InvalidAutonomousModePayload)?;
        let confirmed = value
            .get("confirmed")
            .and_then(serde_json::Value::as_bool)
            .ok_or(ApplicationError::InvalidAutonomousModePayload)?;
        Ok(Self::new(repo.to_owned(), enabled, confirmed))
    }
}

/// One request to arm/disarm the orchestrator's autonomous-mode permission for a
/// repo, passed to the arming port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousModeArmingRequest {
    repo: String,
    enabled: bool,
}

impl AutonomousModeArmingRequest {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(repo: String, enabled: bool) -> Self {
        Self { repo, enabled }
    }

    #[must_use]
    /// Return the target repo id.
    pub fn repo(&self) -> &str {
        &self.repo
    }

    #[must_use]
    /// Whether the permission should be armed (`true`) or disarmed (`false`).
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
}

/// The honest outcome of arming the orchestrator's autonomous-mode permission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutonomousModeArmingOutcome {
    /// The permission key was actually written in the consumer's config.
    Armed,
    /// No real arming surface is wired (or the config could not be read or
    /// written), so the key was not written. Reported honestly instead of
    /// fabricating success.
    NotWired,
}

impl AutonomousModeArmingOutcome {
    #[must_use]
    /// Return the armed value.
    pub const fn armed() -> Self {
        Self::Armed
    }

    #[must_use]
    /// Return the not-wired value.
    pub const fn not_wired() -> Self {
        Self::NotWired
    }
}

/// Port interface for arming the orchestrator's autonomous-mode permission,
/// supplied by an outer layer.
///
/// The console sets the orchestrator plane's single persistent permission --
/// the `livespec-orchestrator-beads-fabro.dispatcher.autonomous_mode` key in
/// the consumer's `.livespec.jsonc` -- through this port, and reflects the
/// honest outcome rather than fabricating success.
pub trait AutonomousModeArmingPort {
    /// Arm or disarm the permission and return the honest outcome.
    ///
    /// # Errors
    /// Returns an application error when the port cannot produce a trustworthy
    /// outcome.
    fn arm(
        &mut self,
        request: &AutonomousModeArmingRequest,
    ) -> ApplicationResult<AutonomousModeArmingOutcome>;
}

/// Real arming port that writes the orchestrator permission key directly into a
/// consumer project's `.livespec.jsonc`, through a [`SourceProbe`].
///
/// It reads the config, edits the single boolean key in place (preserving the
/// file's comments and layout), and writes it back, reflecting the actual
/// outcome: a genuine write yields [`AutonomousModeArmingOutcome::Armed`], while
/// an unreadable/unwritable/simulated config yields
/// [`AutonomousModeArmingOutcome::NotWired`]. The host-backed probe is supplied
/// by the binary, so the live arming never claims a write that did not happen.
pub struct LivespecJsoncArmingPort<'a> {
    probe: &'a dyn SourceProbe,
    livespec_jsonc_path: String,
}

impl<'a> LivespecJsoncArmingPort<'a> {
    #[must_use]
    /// Construct a new value from its required fields.
    ///
    /// `livespec_jsonc_path` is the path to the consumer project's
    /// `.livespec.jsonc` this port arms.
    pub fn new(probe: &'a dyn SourceProbe, livespec_jsonc_path: &str) -> Self {
        Self {
            probe,
            livespec_jsonc_path: livespec_jsonc_path.to_owned(),
        }
    }
}

impl AutonomousModeArmingPort for LivespecJsoncArmingPort<'_> {
    fn arm(
        &mut self,
        request: &AutonomousModeArmingRequest,
    ) -> ApplicationResult<AutonomousModeArmingOutcome> {
        let SourceProbeOutcome::Observed {
            stdout: original,
            success: true,
        } = self.probe.read_file(&self.livespec_jsonc_path)
        else {
            return Ok(AutonomousModeArmingOutcome::not_wired());
        };
        let Some(updated) = set_autonomous_mode_in_jsonc(&original, request.enabled()) else {
            return Ok(AutonomousModeArmingOutcome::not_wired());
        };
        Ok(
            match self.probe.write_file(&self.livespec_jsonc_path, &updated) {
                SourceProbeOutcome::Observed { success: true, .. } => {
                    AutonomousModeArmingOutcome::armed()
                }
                SourceProbeOutcome::Observed { success: false, .. }
                | SourceProbeOutcome::Unavailable { .. } => {
                    AutonomousModeArmingOutcome::not_wired()
                }
            },
        )
    }
}

/// Derive the current per-repo autonomous mode from a `.livespec.jsonc`.
///
/// Reads the orchestrator permission key out of the document. An absent key --
/// or an unparseable document -- is treated as disabled (fail-soft), per the
/// autonomous-mode default-disabled contract.
#[must_use]
pub fn read_autonomous_mode_from_jsonc(text: &str) -> bool {
    let stripped = strip_jsonc_comments(text);
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&stripped) else {
        return false;
    };
    value
        .get(ORCHESTRATOR_CONFIG_KEY)
        .and_then(|orchestrator| orchestrator.get(DISPATCHER_CONFIG_KEY))
        .and_then(|dispatcher| dispatcher.get(AUTONOMOUS_MODE_CONFIG_KEY))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

/// Set the orchestrator autonomous-mode permission key to `enabled` in a
/// `.livespec.jsonc` document, preserving the rest of the file (comments and
/// layout) by editing only the single key in place.
///
/// Returns `None` when the document is not a JSON object (so the key cannot be
/// located or inserted); the arming port maps that to a not-wired outcome rather
/// than crashing. Handles four shapes: the key already present (value replaced),
/// the `dispatcher` object present without the key, the orchestrator object
/// present without a `dispatcher`, and no orchestrator object at all.
#[must_use]
pub fn set_autonomous_mode_in_jsonc(text: &str, enabled: bool) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let literal = if enabled { "true" } else { "false" };
    if find_member_object_brace(&chars, DISPATCHER_CONFIG_KEY).is_some()
        && find_member_value_start(&chars, AUTONOMOUS_MODE_CONFIG_KEY).is_some()
    {
        return replace_member_value(&chars, AUTONOMOUS_MODE_CONFIG_KEY, literal);
    }
    if let Some(brace) = find_member_object_brace(&chars, DISPATCHER_CONFIG_KEY) {
        let member = format!("\"{AUTONOMOUS_MODE_CONFIG_KEY}\": {literal}");
        return Some(insert_member_after_brace(&chars, brace, &member));
    }
    if let Some(brace) = find_member_object_brace(&chars, ORCHESTRATOR_CONFIG_KEY) {
        let member = format!(
            "\"{DISPATCHER_CONFIG_KEY}\": {{ \"{AUTONOMOUS_MODE_CONFIG_KEY}\": {literal} }}"
        );
        return Some(insert_member_after_brace(&chars, brace, &member));
    }
    let top = find_top_level_brace(&chars)?;
    let member = format!(
        "\"{ORCHESTRATOR_CONFIG_KEY}\": {{ \"{DISPATCHER_CONFIG_KEY}\": {{ \"{AUTONOMOUS_MODE_CONFIG_KEY}\": {literal} }} }}"
    );
    Some(insert_member_after_brace(&chars, top, &member))
}

/// Strip `//` line and `/* */` block comments from a JSONC document, leaving
/// string literals (including any comment-like sequences inside them) intact, so
/// the result parses as strict JSON.
fn strip_jsonc_comments(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let length = chars.len();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < length {
        let current = chars[index];
        if current == '"' {
            out.push(current);
            index += 1;
            while index < length {
                let inner = chars[index];
                out.push(inner);
                index += 1;
                if inner == '\\' {
                    if index < length {
                        out.push(chars[index]);
                        index += 1;
                    }
                } else if inner == '"' {
                    break;
                }
            }
        } else if current == '/' && index + 1 < length && chars[index + 1] == '/' {
            index += 2;
            while index < length && chars[index] != '\n' {
                index += 1;
            }
        } else if current == '/' && index + 1 < length && chars[index + 1] == '*' {
            index += 2;
            while index + 1 < length && !(chars[index] == '*' && chars[index + 1] == '/') {
                index += 1;
            }
            index = if index + 1 < length {
                index + 2
            } else {
                length
            };
        } else {
            out.push(current);
            index += 1;
        }
    }
    out
}

/// The char index just past the closing quote of a string literal whose opening
/// quote is at `start`.
fn skip_string(chars: &[char], start: usize) -> usize {
    let length = chars.len();
    let mut index = start + 1;
    while index < length {
        let current = chars[index];
        index += 1;
        if current == '\\' {
            index += 1;
        } else if current == '"' {
            break;
        }
    }
    index
}

/// The char index of the next character that is neither whitespace nor part of a
/// `//` or `/* */` comment, starting at `start`.
fn skip_ws_and_comments(chars: &[char], start: usize) -> usize {
    let length = chars.len();
    let mut index = start;
    while index < length {
        let current = chars[index];
        if current.is_whitespace() {
            index += 1;
        } else if current == '/' && index + 1 < length && chars[index + 1] == '/' {
            index += 2;
            while index < length && chars[index] != '\n' {
                index += 1;
            }
        } else if current == '/' && index + 1 < length && chars[index + 1] == '*' {
            index += 2;
            while index + 1 < length && !(chars[index] == '*' && chars[index + 1] == '/') {
                index += 1;
            }
            index = if index + 1 < length {
                index + 2
            } else {
                length
            };
        } else {
            break;
        }
    }
    index
}

/// The char index just past the `:` that follows a member key `"key"` (a quoted
/// string equal to `key` followed, after whitespace and comments, by a `:`),
/// scanning `chars` while skipping strings and comments. `None` when no such
/// member key is present.
fn find_member_colon_end(chars: &[char], key: &str) -> Option<usize> {
    let length = chars.len();
    let mut index = 0;
    while index < length {
        let current = chars[index];
        if current == '"' {
            let end = skip_string(chars, index);
            let content: String = chars
                .get(index + 1..end.saturating_sub(1))?
                .iter()
                .collect();
            let after = skip_ws_and_comments(chars, end);
            if content == key && chars.get(after) == Some(&':') {
                return Some(after + 1);
            }
            index = end;
        } else if current == '/'
            && index + 1 < length
            && (chars[index + 1] == '/' || chars[index + 1] == '*')
        {
            index = skip_ws_and_comments(chars, index);
        } else {
            index += 1;
        }
    }
    None
}

/// The char index just past the `{` that opens the object value of member `key`.
/// `None` when `key` is absent or its value is not an object.
fn find_member_object_brace(chars: &[char], key: &str) -> Option<usize> {
    let after_colon = find_member_colon_end(chars, key)?;
    let brace = skip_ws_and_comments(chars, after_colon);
    if chars.get(brace) != Some(&'{') {
        return None;
    }
    Some(brace + 1)
}

/// The char index of the first character of member `key`'s scalar-or-string
/// value. `None` when `key` is absent.
fn find_member_value_start(chars: &[char], key: &str) -> Option<usize> {
    let after_colon = find_member_colon_end(chars, key)?;
    Some(skip_ws_and_comments(chars, after_colon))
}

/// The char index just past the first top-level `{`, skipping any leading
/// whitespace and comments. `None` when the document does not open an object.
fn find_top_level_brace(chars: &[char]) -> Option<usize> {
    let brace = skip_ws_and_comments(chars, 0);
    if chars.get(brace) != Some(&'{') {
        return None;
    }
    Some(brace + 1)
}

/// Replace member `key`'s scalar-or-string value with `literal`, preserving the
/// rest of `chars`. `None` when `key`'s value cannot be located.
fn replace_member_value(chars: &[char], key: &str, literal: &str) -> Option<String> {
    let value_start = find_member_value_start(chars, key)?;
    let value_end = if chars.get(value_start) == Some(&'"') {
        skip_string(chars, value_start)
    } else {
        let mut index = value_start;
        while index < chars.len()
            && (chars[index].is_alphanumeric() || chars[index] == '-' || chars[index] == '.')
        {
            index += 1;
        }
        index
    };
    let mut out: String = chars.get(..value_start)?.iter().collect();
    out.push_str(literal);
    out.extend(chars.get(value_end..)?.iter());
    Some(out)
}

/// Insert `member` as the first member of the object whose opening `{` is
/// immediately before `after_brace`, adding a separating comma when the object
/// already has members.
fn insert_member_after_brace(chars: &[char], after_brace: usize, member: &str) -> String {
    let next = skip_ws_and_comments(chars, after_brace);
    let object_is_empty = chars.get(next) == Some(&'}');
    let mut out: String = chars.iter().take(after_brace).collect();
    out.push_str("\n    ");
    out.push_str(member);
    if !object_is_empty {
        out.push(',');
    }
    out.extend(chars.iter().skip(after_brace));
    out
}

/// Represents a configuration command-handling outcome.
///
/// Carries the resolved command status and the events it appended (`command`
/// acceptance/rejection, the `factory` arming outcome, and the `configuration`
/// audit fact).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigCommandOutcome {
    command_status: String,
    events: Vec<ConsoleEvent>,
}

impl ConfigCommandOutcome {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(command_status: String, events: Vec<ConsoleEvent>) -> Self {
        Self {
            command_status,
            events,
        }
    }

    #[must_use]
    /// Return the command status value.
    pub fn command_status(&self) -> &str {
        &self.command_status
    }

    #[must_use]
    /// Return the events value.
    pub fn events(&self) -> &[ConsoleEvent] {
        &self.events
    }
}

// ---------------------------------------------------------------------------
// Full autonomous mode — observing the orchestrator plane's auto-resolutions.
// ---------------------------------------------------------------------------

/// The journal `stage` marker the orchestrator plane writes for one per-decision
/// autonomous-mode audit record; the console reads only records carrying it and
/// ignores every other journal stage (arming, calibration, dispatch).
const AUTONOMOUS_DECISION_STAGE: &str = "autonomous-decision";

/// The `auto-resolved` disposition: the plane's engine resolved the decision.
const AUTONOMOUS_DISPOSITION_AUTO_RESOLVED: &str = "auto-resolved";
/// The `escalated` disposition: the plane left the decision truly-unresolvable.
const AUTONOMOUS_DISPOSITION_ESCALATED: &str = "escalated";

/// The three collapsible gates a decision can carry, exactly as the plane's
/// published record contract enumerates them.
const AUTONOMOUS_GATE_APPROVE: &str = "approve";
const AUTONOMOUS_GATE_ACCEPTANCE: &str = "acceptance";
const AUTONOMOUS_GATE_NEEDS_HUMAN: &str = "needs-human";

/// One per-decision autonomous-mode audit entry read back off the orchestrator
/// plane's published Dispatcher journal.
///
/// `work_item_id` names the disposed item; `gate` is the collapsed gate
/// (`approve` / `acceptance` / `needs-human`); `decision` is what the plane's
/// engine decided; `disposition` is `auto-resolved` or `escalated`. The console
/// consumes this record verbatim -- it never re-derives a plane's decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousDecision {
    work_item_id: String,
    gate: String,
    decision: String,
    disposition: String,
}

impl AutonomousDecision {
    #[must_use]
    /// Construct a new value from its required fields.
    pub fn new(work_item_id: &str, gate: &str, decision: &str, disposition: &str) -> Self {
        Self {
            work_item_id: work_item_id.to_owned(),
            gate: gate.to_owned(),
            decision: decision.to_owned(),
            disposition: disposition.to_owned(),
        }
    }

    #[must_use]
    /// Return the work item id value.
    pub fn work_item_id(&self) -> &str {
        &self.work_item_id
    }

    #[must_use]
    /// Return the gate value.
    pub fn gate(&self) -> &str {
        &self.gate
    }

    #[must_use]
    /// Return the decision value.
    pub fn decision(&self) -> &str {
        &self.decision
    }

    #[must_use]
    /// Return the disposition value.
    pub fn disposition(&self) -> &str {
        &self.disposition
    }
}

/// The published read view of the autonomous per-decision journal the console
/// observes.
///
/// Every auto-resolution and every truly-unresolvable escalation the run
/// journaled, split by disposition and preserving journal order within each
/// bucket. Mirrors the orchestrator plane's published read surface.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AutonomousAudit {
    auto_resolutions: Vec<AutonomousDecision>,
    escalations: Vec<AutonomousDecision>,
}

impl AutonomousAudit {
    #[must_use]
    /// Construct a new value from its two disposition buckets.
    pub const fn new(
        auto_resolutions: Vec<AutonomousDecision>,
        escalations: Vec<AutonomousDecision>,
    ) -> Self {
        Self {
            auto_resolutions,
            escalations,
        }
    }

    #[must_use]
    /// The decisions the plane's engine auto-resolved.
    pub fn auto_resolutions(&self) -> &[AutonomousDecision] {
        &self.auto_resolutions
    }

    #[must_use]
    /// The decisions the plane escalated as truly-unresolvable.
    pub fn escalations(&self) -> &[AutonomousDecision] {
        &self.escalations
    }
}

/// Read the published autonomous per-decision audit view from a Dispatcher
/// journal document (its JSONL text).
///
/// Fail-open, mirroring the orchestrator plane's published `read_autonomous_decisions`
/// reader: a malformed line -- bad JSON, a non-object, a record missing a
/// required field, or an out-of-range gate/disposition -- is skipped rather than
/// raising, and only `autonomous-decision` stage records are considered. Records
/// split into auto-resolutions and escalations by disposition, preserving
/// journal order within each bucket.
#[must_use]
pub fn read_autonomous_decisions_from_journal(journal_text: &str) -> AutonomousAudit {
    let mut auto_resolutions = Vec::new();
    let mut escalations = Vec::new();
    for line in journal_text.lines() {
        let Some(decision) = autonomous_decision_from_line(line) else {
            continue;
        };
        if decision.disposition() == AUTONOMOUS_DISPOSITION_ESCALATED {
            escalations.push(decision);
        } else {
            auto_resolutions.push(decision);
        }
    }
    AutonomousAudit::new(auto_resolutions, escalations)
}

/// Parse one journal line into an [`AutonomousDecision`], or `None` when it is
/// not a valid `autonomous-decision` record (malformed JSON, a non-object, a
/// different stage, or an absent/out-of-range required field).
fn autonomous_decision_from_line(line: &str) -> Option<AutonomousDecision> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let object = value.as_object()?;
    if object.get("stage").and_then(serde_json::Value::as_str)? != AUTONOMOUS_DECISION_STAGE {
        return None;
    }
    let work_item_id = object
        .get("work_item_id")
        .and_then(serde_json::Value::as_str)?;
    let gate = object.get("gate").and_then(serde_json::Value::as_str)?;
    let decision = object.get("decision").and_then(serde_json::Value::as_str)?;
    let disposition = object
        .get("disposition")
        .and_then(serde_json::Value::as_str)?;
    let gate_known = gate == AUTONOMOUS_GATE_APPROVE
        || gate == AUTONOMOUS_GATE_ACCEPTANCE
        || gate == AUTONOMOUS_GATE_NEEDS_HUMAN;
    let disposition_known = disposition == AUTONOMOUS_DISPOSITION_AUTO_RESOLVED
        || disposition == AUTONOMOUS_DISPOSITION_ESCALATED;
    if !gate_known || !disposition_known {
        return None;
    }
    Some(AutonomousDecision::new(
        work_item_id,
        gate,
        decision,
        disposition,
    ))
}

/// The needs-attention item id the console resolves to reflect an auto-resolution
/// of `work_item_id` on `gate`.
///
/// The orchestrator plane keys a work-item's human-gate needs-attention item as
/// `valve:<verb>:<work-item-id>`; the reflection resolves that same id so the
/// item leaves the inbox. The gate-to-verb map is the console's consumer half of
/// that contract: `approve` -> `approve`, `acceptance` -> `accept`, `needs-human`
/// -> `set-admission`. An unknown gate yields `None` (no item to reflect).
#[must_use]
pub fn autonomous_reflection_attention_id(work_item_id: &str, gate: &str) -> Option<String> {
    let verb = match gate {
        AUTONOMOUS_GATE_APPROVE => "approve",
        AUTONOMOUS_GATE_ACCEPTANCE => "accept",
        AUTONOMOUS_GATE_NEEDS_HUMAN => "set-admission",
        _other => return None,
    };
    Some(format!("valve:{verb}:{work_item_id}"))
}

/// Port interface for reading the orchestrator plane's published per-decision
/// autonomous-mode audit, supplied by an outer layer.
///
/// The console observes each auto-resolution and each truly-unresolvable
/// escalation through this port and reflects them; it never re-derives a plane's
/// decision. Reads are fail-open: an unavailable audit surface yields an empty
/// audit rather than an error.
pub trait AutonomousDecisionsPort {
    /// Read the current published autonomous per-decision audit view.
    fn read_autonomous_decisions(&self) -> AutonomousAudit;
}

/// Real autonomous-decisions port that reads the orchestrator plane's published
/// Dispatcher journal file through a [`SourceProbe`].
///
/// The journal is the plane's PUBLISHED per-decision audit surface; the console
/// reads the `autonomous-decision` stage records from it fail-open. An unreadable
/// or absent journal yields an empty audit, never a fabricated decision.
pub struct JournalAutonomousDecisionsPort<'a> {
    probe: &'a dyn SourceProbe,
    journal_path: String,
}

impl<'a> JournalAutonomousDecisionsPort<'a> {
    #[must_use]
    /// Construct a new value from its required fields.
    ///
    /// `journal_path` is the Dispatcher journal the orchestrator plane appends
    /// its per-decision audit records to.
    pub fn new(probe: &'a dyn SourceProbe, journal_path: &str) -> Self {
        Self {
            probe,
            journal_path: journal_path.to_owned(),
        }
    }
}

impl AutonomousDecisionsPort for JournalAutonomousDecisionsPort<'_> {
    fn read_autonomous_decisions(&self) -> AutonomousAudit {
        match self.probe.read_file(&self.journal_path) {
            SourceProbeOutcome::Observed {
                stdout,
                success: true,
            } => read_autonomous_decisions_from_journal(&stdout),
            SourceProbeOutcome::Observed { success: false, .. }
            | SourceProbeOutcome::Unavailable { .. } => AutonomousAudit::default(),
        }
    }
}

/// Handle a `config.autonomous_mode_set` command.
///
/// The Configuration context's arming command. It parses the
/// `{ repo, enabled, confirmed }` payload and guards a dangerous enable: an
/// enable without `confirmed` true is REJECTED with no effect -- no arming port
/// call, no key write, and no audit event, only a `command.rejected` event. On
/// acceptance it issues the orchestrator's arming command through the arming
/// port (the plane's published command surface) and, when the key is actually
/// written, appends the matching `config.autonomous_mode.enabled` /
/// `config.autonomous_mode.disabled` audit event carrying
/// `{ repo, actor, occurred_at }`. A not-wired arming surface surfaces
/// `factory.autonomous_mode.not_wired` and no audit event, never a fabricated
/// success.
///
/// # Errors
/// Returns [`ApplicationError::InvalidAutonomousModePayload`] when the payload
/// is malformed, and surfaces a port error when the port cannot produce a
/// trustworthy outcome.
pub fn handle_config_autonomous_mode_set_command(
    command: &CommandEnvelope,
    payload_json: &str,
    occurred_at: &str,
    port: &mut dyn AutonomousModeArmingPort,
) -> ApplicationResult<ConfigCommandOutcome> {
    let request = AutonomousModeSetRequest::from_payload_json(payload_json)?;
    if request.enabled() && !request.confirmed() {
        // Dangerous-enable guard: reject with no effect (no port call, no key
        // write, no audit event) -- only the rejection is recorded.
        let reason = "autonomous mode enable requires explicit confirmation";
        return Ok(ConfigCommandOutcome::new(
            "rejected".to_owned(),
            vec![config_command_event(
                command,
                EventType::CommandRejected,
                "rejected",
                1,
                &serde_json::json!({ "reason": reason, "repo": request.repo() }).to_string(),
            )],
        ));
    }
    let mut events = vec![config_command_event(
        command,
        EventType::CommandAccepted,
        "accepted",
        1,
        "{}",
    )];
    let arming = AutonomousModeArmingRequest::new(request.repo().to_owned(), request.enabled());
    let command_status = match port.arm(&arming)? {
        AutonomousModeArmingOutcome::Armed => {
            let (factory_event, audit_event) = if request.enabled() {
                (
                    EventType::FactoryAutonomousModeEnableRequested,
                    EventType::ConfigAutonomousModeEnabled,
                )
            } else {
                (
                    EventType::FactoryAutonomousModeDisableRequested,
                    EventType::ConfigAutonomousModeDisabled,
                )
            };
            events.push(config_command_event(
                command,
                factory_event,
                "arming_requested",
                2,
                &serde_json::json!({ "repo": request.repo() }).to_string(),
            ));
            events.push(config_command_event(
                command,
                audit_event,
                "audited",
                3,
                &serde_json::json!({
                    "repo": request.repo(),
                    "actor": command.requested_by(),
                    "occurred_at": occurred_at,
                })
                .to_string(),
            ));
            "completed"
        }
        AutonomousModeArmingOutcome::NotWired => {
            // The arming surface is not wired, so the key was not written. Emit
            // the honest not-wired outcome and NO audit event.
            events.push(config_command_event(
                command,
                EventType::FactoryAutonomousModeNotWired,
                "not_wired",
                2,
                &serde_json::json!({ "repo": request.repo() }).to_string(),
            ));
            "not_wired"
        }
    };
    Ok(ConfigCommandOutcome::new(command_status.to_owned(), events))
}

/// Build one Configuration-context command event, carrying `payload_json`, from
/// the command and its resolved event type.
fn config_command_event(
    command: &CommandEnvelope,
    event_type: EventType,
    suffix: &str,
    stream_seq: u64,
    payload_json: &str,
) -> ConsoleEvent {
    ConsoleEvent::new(
        format!("evt_{}_{}", command.command_id(), suffix),
        1,
        command_event_context(event_type).to_owned(),
        event_type,
        "console:config-command-handler".to_owned(),
        command.aggregate_id().to_owned(),
        stream_seq,
    )
    .with_payload_json(payload_json.to_owned())
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

/// Whether a work-item lane snapshot rests on a human step and so must surface
/// in the attention list. The lane and its policy dials are emitted by the
/// orchestrator and consumed verbatim (the console never re-derives a lane).
///
/// A `manual`-admission `pending-approval` item awaits a human approval; a
/// `blocked`/`needs-human` item awaits a human unblock; and an `acceptance`-lane
/// item awaits a human acceptance whenever its policy carries a human leg --
/// `ai-then-human` (human confirms after the AI pass) or `human-only` (a human
/// must accept). `ai-only` acceptance carries no human step and stays unflagged
/// (and by the orchestrator's lane authority an `ai-only` item auto-completes to
/// `done` rather than resting in `acceptance`).
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
            | (
                Lane::Acceptance,
                _,
                _,
                AcceptancePolicy::AiThenHuman | AcceptancePolicy::HumanOnly
            )
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
        TuiOverlay::None
        | TuiOverlay::CommandPalette { .. }
        | TuiOverlay::CommandModal { .. }
        | TuiOverlay::AutonomousModeConfirm { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::Help => None,
    }
}

fn normalize_overlay(overlay: &TuiOverlay, detail: Option<&AttentionDetail>) -> TuiOverlay {
    match overlay {
        TuiOverlay::CommandModal {
            selected_action_index,
        } => TuiOverlay::CommandModal {
            selected_action_index: clamp_action_index(detail, *selected_action_index),
        },
        TuiOverlay::None
        | TuiOverlay::Search { .. }
        | TuiOverlay::CommandPalette { .. }
        | TuiOverlay::AutonomousModeConfirm { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::Help => overlay.clone(),
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
        TuiOverlay::AutonomousModeConfirm { typed } => TuiOverlay::AutonomousModeConfirm {
            typed: format!("{typed}{value}"),
        },
        TuiOverlay::None
        | TuiOverlay::CommandModal { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::Help => overlay.clone(),
    }
}

fn backspace_overlay_query(overlay: &TuiOverlay) -> TuiOverlay {
    match overlay {
        TuiOverlay::Search { query } => TuiOverlay::Search {
            query: drop_last_char(query),
        },
        TuiOverlay::CommandPalette { query } => TuiOverlay::CommandPalette {
            query: drop_last_char(query),
        },
        TuiOverlay::AutonomousModeConfirm { typed } => TuiOverlay::AutonomousModeConfirm {
            typed: drop_last_char(typed),
        },
        TuiOverlay::None
        | TuiOverlay::CommandModal { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::Help => overlay.clone(),
    }
}

/// Return `text` with its final character removed, or an empty string when it is
/// already empty. Shared by the overlays whose text the operator edits.
fn drop_last_char(text: &str) -> String {
    text.char_indices()
        .next_back()
        .map_or_else(String::new, |(index, _value)| text[..index].to_owned())
}

fn move_action_down(overlay: &TuiOverlay, detail: Option<&AttentionDetail>) -> TuiOverlay {
    match overlay {
        TuiOverlay::CommandModal {
            selected_action_index,
        } => TuiOverlay::CommandModal {
            selected_action_index: clamp_action_index(detail, selected_action_index + 1),
        },
        TuiOverlay::None
        | TuiOverlay::Search { .. }
        | TuiOverlay::CommandPalette { .. }
        | TuiOverlay::AutonomousModeConfirm { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::Help => overlay.clone(),
    }
}

fn move_action_up(overlay: &TuiOverlay) -> TuiOverlay {
    match overlay {
        TuiOverlay::CommandModal {
            selected_action_index,
        } => TuiOverlay::CommandModal {
            selected_action_index: selected_action_index.saturating_sub(1),
        },
        TuiOverlay::None
        | TuiOverlay::Search { .. }
        | TuiOverlay::CommandPalette { .. }
        | TuiOverlay::AutonomousModeConfirm { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::Help => overlay.clone(),
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
            Self::DispatcherBacklogBounceObserved => "Dispatcher backlog bounce",
            Self::FactoryDrainRequested => "Factory drain requested",
            Self::FactoryDrainStarted => "Factory drain started",
            Self::WorkItemActionStarted => "Work-item action started",
            Self::WorkItemActionCompleted => "Work-item action completed",
            Self::WorkItemActionFailed => "Work-item action failed",
            Self::WorkItemActionNotWired => "Work-item action not wired",
            Self::SourceCompletenessFindingObserved => "Source completeness finding",
            Self::SourceNotObservedFindingObserved => "Source not-observed finding",
            Self::AttentionItemAppeared => "Attention item appeared",
            Self::AttentionItemChanged => "Attention item changed",
            Self::AttentionItemResolved => "Attention item resolved",
            Self::ConfigAutonomousModeEnabled => "Autonomous mode enabled",
            Self::ConfigAutonomousModeDisabled => "Autonomous mode disabled",
            Self::FactoryAutonomousModeEnableRequested => "Autonomous mode enable requested",
            Self::FactoryAutonomousModeDisableRequested => "Autonomous mode disable requested",
            Self::FactoryAutonomousModeNotWired => "Autonomous mode arming not wired",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};
    use proptest::proptest;

    use super::source_adapters::{
        AcceptancePolicy, AdmissionPolicy, AttentionHandoff, AttentionItemSnapshot,
        AttentionSourceRef, Lane, LaneReason, SourceProbe, SourceProbeOutcome, WorkItemSnapshot,
        attention_item_payload_json, attention_resolved_payload_json,
    };
    use super::{
        ApplicationError, AttentionDetail, AttentionEvent, AttentionItem, AutonomousAudit,
        AutonomousDecisionsPort, AutonomousModeArmingOutcome, AutonomousModeArmingPort,
        AutonomousModeArmingRequest, AutonomousModeSetRequest, ConfigCommandOutcome,
        DispatcherFactoryDrainPort, DispatcherOrchestratorActionPort, FactoryDrainPolicy,
        FactoryDrainPort, FactoryDrainPortOutcome, FactoryDrainRequest, FocusPane,
        JournalAutonomousDecisionsPort, LaneFocus, LivespecJsoncArmingPort, OperatorAction,
        OperatorActionOutcome, OrchestratorActionOutcome, OrchestratorActionPort,
        OrchestratorActionRequest, PendingValve, RejectMode, TuiInteraction, TuiInteractionState,
        TuiOverlay, TuiScreenModel, TuiView, autonomous_mode_confirmation_matches, build_tui_model,
        build_tui_model_for_state, handle_config_autonomous_mode_set_command,
        handle_factory_drain_command, handle_work_item_accept_command,
        handle_work_item_approve_command, handle_work_item_reject_command,
        handle_work_item_set_acceptance_command, handle_work_item_set_admission_command,
        project_attention, project_lane_board, read_autonomous_mode_from_jsonc,
        reduce_tui_interaction, resolve_autonomous_mode_disable, resolve_autonomous_mode_enable,
        resolve_command_palette_action, resolve_selected_operator_action, resolve_valve_action,
        set_acceptance_policy_from_payload, set_admission_policy_from_payload,
        set_autonomous_mode_in_jsonc, validate_operator_action,
    };

    #[test]
    fn attention_projection_folds_the_attention_item_stream_ordered_by_id() {
        // Re-sourced (v016 / CN1): the inbox is the diffed `attention_item.*`
        // stream, not re-derived from work-item lanes. Non-attention events and
        // work-item snapshots are ignored by this projection.
        let events = [
            attention_appeared(
                "evt_accept",
                &attention_item("wi-accept", "acceptance", "Acceptance review"),
            ),
            attention_appeared(
                "evt_blocked",
                &attention_item("wi-blocked", "human-valve", "Blocked: needs-human"),
            ),
            attention_appeared(
                "evt_pending",
                &attention_item("wi-pending", "human-valve", "Pending approval"),
            ),
            lane_event(
                "evt_ready",
                "console-ready",
                Lane::Ready,
                None,
                "a0",
                "ready",
            ),
            ConsoleEvent::fixture("evt_revise", EventType::LivespecReviseRequired, "livespec"),
        ];

        let projected = project_attention(&events);

        assert_eq!(projected.len(), 3);
        assert_eq!(projected[0].id(), "wi-accept");
        assert_eq!(projected[0].title(), "Acceptance review");
        assert_eq!(projected[0].source(), "acceptance");
        assert_eq!(projected[0].source_reference(), "console:wi-accept");
        assert_eq!(projected[0].next_action(), None);
        assert_eq!(projected[1].id(), "wi-blocked");
        assert_eq!(projected[1].title(), "Blocked: needs-human");
        assert_eq!(projected[2].id(), "wi-pending");
        assert_eq!(projected[2].title(), "Pending approval");
    }

    #[test]
    fn attention_projection_applies_changed_and_resolved_events() {
        let events = [
            attention_appeared(
                "evt_a1",
                &attention_item("wi-a", "human-valve", "old summary"),
            ),
            attention_appeared(
                "evt_b1",
                &attention_item("wi-b", "human-valve", "b summary"),
            ),
            attention_changed(
                "evt_a2",
                &attention_item("wi-a", "human-valve", "new summary"),
            ),
            attention_resolved("evt_b2", "wi-b"),
        ];

        let projected = project_attention(&events);

        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].id(), "wi-a");
        assert_eq!(projected[0].title(), "new summary");
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
    fn attention_projection_renders_source_reference_variants_and_resolves_empty() {
        // A resolved id with no prior appeared leaves the inbox empty, and
        // work-item lane snapshots never enter the inbox.
        assert_eq!(
            project_attention(&[
                attention_resolved("evt_r", "wi-missing"),
                lane_event("evt_new", "console-1", Lane::Ready, None, "a0", "ready"),
            ]),
            []
        );

        // source_reference narrows to a path when there is no work-item, and is
        // the bare repo when the item carries neither.
        let path_item = AttentionItemSnapshot::new(
            "wi-path",
            "hygiene",
            "high",
            "Hygiene finding",
            AttentionSourceRef::new("console", None, Some("SPECIFICATION/spec.md")),
            AttentionHandoff::new("fix", None, "fix-it"),
        );
        let repo_item = AttentionItemSnapshot::new(
            "wi-repo",
            "internal",
            "low",
            "Internal note",
            AttentionSourceRef::new("console", None, None),
            AttentionHandoff::new("noop", None, "noop"),
        );

        let projected = project_attention(&[
            attention_appeared("evt_path", &path_item),
            attention_appeared("evt_repo", &repo_item),
        ]);

        assert_eq!(projected[0].id(), "wi-path");
        assert_eq!(
            projected[0].source_reference(),
            "console:SPECIFICATION/spec.md"
        );
        assert_eq!(projected[1].id(), "wi-repo");
        assert_eq!(projected[1].source_reference(), "console");
    }

    #[test]
    fn tui_attention_list_orders_same_rank_items_by_work_item_id() {
        // The TUI's own lane-derived attention list (Scenario 5, retained) still
        // orders same-rank items by work-item id.
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

        let model = build_tui_model(&events, 0);
        let ids: Vec<&str> = model
            .attention_items()
            .iter()
            .map(AttentionItem::id)
            .collect();

        assert_eq!(ids, ["console-a", "console-b"]);
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
            // A `human-only` acceptance item -- the case that most needs a human
            // -- rests in the acceptance lane (the orchestrator's lane authority
            // parks status `acceptance` verbatim) and MUST surface (fold of d6o).
            (
                Lane::Acceptance,
                None,
                AdmissionPolicy::Auto,
                AcceptancePolicy::HumanOnly,
                true,
            ),
            // An `ai-only` acceptance item has no human step and stays unflagged.
            (
                Lane::Acceptance,
                None,
                AdmissionPolicy::Auto,
                AcceptancePolicy::AiOnly,
                false,
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

    // Build attention-item fixtures and the `attention_item.*` events the
    // re-sourced projection folds, writing the canonical `payload_json` directly
    // so the projection exercises the real deserialization path.
    fn attention_item(id: &str, kind: &str, summary: &str) -> AttentionItemSnapshot {
        AttentionItemSnapshot::new(
            id,
            kind,
            "high",
            summary,
            AttentionSourceRef::new("console", Some(id), None),
            AttentionHandoff::new("approve", None, &format!("approve:{id}")),
        )
    }

    fn attention_appeared(event_id: &str, item: &AttentionItemSnapshot) -> ConsoleEvent {
        ConsoleEvent::fixture(
            event_id,
            EventType::AttentionItemAppeared,
            "needs-attention",
        )
        .with_payload_json(attention_item_payload_json(item))
    }

    fn attention_changed(event_id: &str, item: &AttentionItemSnapshot) -> ConsoleEvent {
        ConsoleEvent::fixture(event_id, EventType::AttentionItemChanged, "needs-attention")
            .with_payload_json(attention_item_payload_json(item))
    }

    fn attention_resolved(event_id: &str, id: &str) -> ConsoleEvent {
        ConsoleEvent::fixture(
            event_id,
            EventType::AttentionItemResolved,
            "needs-attention",
        )
        .with_payload_json(attention_resolved_payload_json(id))
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
        // Focus starts on the Views nav so up/down walk the vertical Views menu.
        assert_eq!(model.focus(), FocusPane::Nav);
        assert_eq!(model.overlay(), &TuiOverlay::None);
        assert_eq!(model.selected_operator_action(), None);
        assert_eq!(
            model.header(),
            "fleet: livespec | mode: tui | repo: - | autonomous: off | view: Attention | attention: 0"
        );
        assert_eq!(
            model.footer(),
            "shortcuts: up/down move focused pane (views | content) | enter dive in | esc back | left/right prev/next view | / search | : drain | a autonomous-mode (dangerous / use with caution) | valves: p approve · c accept · r reject · m set-admission · n set-acceptance | ? help | q quit"
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
    fn tui_interaction_moves_focus_between_the_nav_and_content_panes() {
        let events = fabro_gate_events();
        let state = TuiInteractionState::new(0, TuiOverlay::None);
        // Focus starts on the Views nav.
        assert_eq!(state.focus(), FocusPane::Nav);

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::FocusContent);
        let model = build_tui_model_for_state(&events, &state);
        assert_eq!(state.focus(), FocusPane::Content);
        assert_eq!(model.focus(), FocusPane::Content);

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::FocusNav);
        assert_eq!(state.focus(), FocusPane::Nav);
    }

    #[test]
    fn tui_interaction_opens_and_closes_the_help_overlay() {
        let events = fabro_gate_events();
        let state = TuiInteractionState::new(0, TuiOverlay::None);

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::OpenHelp);
        let model = build_tui_model_for_state(&events, &state);
        assert_eq!(state.overlay(), &TuiOverlay::Help);
        assert_eq!(model.overlay(), &TuiOverlay::Help);
        assert!(model.overlay().is_open());

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::CloseOverlay);
        assert_eq!(state.overlay(), &TuiOverlay::None);
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
            focus: FocusPane::Nav,
            overlay: TuiOverlay::CommandModal {
                selected_action_index: 0,
            },
            selected_repo: String::new(),
            autonomous_mode_enabled: false,
            unavailable_sources: Vec::new(),
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
    fn command_event_context_falls_back_to_source_context() {
        assert_eq!(
            super::command_event_context(EventType::SourceCompletenessFindingObserved),
            "source"
        );
    }

    #[test]
    fn command_event_context_maps_work_item_action_events_to_work_item() {
        assert_eq!(
            super::command_event_context(EventType::WorkItemActionStarted),
            "work_item"
        );
        assert_eq!(
            super::command_event_context(EventType::WorkItemActionCompleted),
            "work_item"
        );
        assert_eq!(
            super::command_event_context(EventType::WorkItemActionFailed),
            "work_item"
        );
        assert_eq!(
            super::command_event_context(EventType::WorkItemActionNotWired),
            "work_item"
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
            focus: FocusPane::Nav,
            overlay: TuiOverlay::CommandModal {
                selected_action_index: 0,
            },
            selected_repo: String::new(),
            autonomous_mode_enabled: false,
            unavailable_sources: Vec::new(),
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
                "evt_backlog_bounce".to_owned(),
                1,
                "factory".to_owned(),
                EventType::DispatcherBacklogBounceObserved,
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
            EventType::DispatcherBacklogBounceObserved.label(),
            "Dispatcher backlog bounce"
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
            EventType::WorkItemActionStarted.label(),
            "Work-item action started"
        );
        assert_eq!(
            EventType::WorkItemActionCompleted.label(),
            "Work-item action completed"
        );
        assert_eq!(
            EventType::WorkItemActionFailed.label(),
            "Work-item action failed"
        );
        assert_eq!(
            EventType::WorkItemActionNotWired.label(),
            "Work-item action not wired"
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
        assert_eq!(
            EventType::AttentionItemAppeared.label(),
            "Attention item appeared"
        );
        assert_eq!(
            EventType::AttentionItemChanged.label(),
            "Attention item changed"
        );
        assert_eq!(
            EventType::AttentionItemResolved.label(),
            "Attention item resolved"
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
        let mut port = DispatcherFactoryDrainPort::new(
            &probe,
            "dispatcher",
            &["drain", "--json"],
            "cfg.jsonc",
        );

        let outcome = port.drain_ready_queue(&drain_request());

        assert_eq!(outcome, Ok(FactoryDrainPortOutcome::completed(3)));
    }

    #[test]
    fn dispatcher_drain_port_reports_zero_when_no_count() {
        let probe = StubDrainProbe {
            outcome: SourceProbeOutcome::observed("drain: ready queue empty", true),
        };
        let mut port =
            DispatcherFactoryDrainPort::new(&probe, "dispatcher", &["drain"], "cfg.jsonc");

        let outcome = port.drain_ready_queue(&drain_request());

        assert_eq!(outcome, Ok(FactoryDrainPortOutcome::completed(0)));
    }

    #[test]
    fn dispatcher_drain_port_fails_on_non_zero_run() {
        let probe = StubDrainProbe {
            outcome: SourceProbeOutcome::observed("drain error", false),
        };
        let mut port =
            DispatcherFactoryDrainPort::new(&probe, "dispatcher", &["drain"], "cfg.jsonc");

        let outcome = port.drain_ready_queue(&drain_request());

        assert_eq!(outcome, Ok(FactoryDrainPortOutcome::failed()));
    }

    #[test]
    fn dispatcher_drain_port_is_not_wired_when_unavailable() {
        let probe = StubDrainProbe {
            outcome: SourceProbeOutcome::unavailable("dispatcher binary not found"),
        };
        let mut port =
            DispatcherFactoryDrainPort::new(&probe, "dispatcher", &["drain"], "cfg.jsonc");

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

    /// Probe for the autonomous-mode launcher tests: `read_file` returns the
    /// configured `.livespec.jsonc` text; `run_command` records the drain args
    /// it was invoked with so a test can assert `--mode autonomous` rides (or
    /// does not ride) the drain.
    struct LauncherDrainProbe {
        config: SourceProbeOutcome,
        drain: SourceProbeOutcome,
        observed_args: std::cell::RefCell<Vec<String>>,
    }

    impl SourceProbe for LauncherDrainProbe {
        fn run_command(&self, _program: &str, args: &[&str]) -> SourceProbeOutcome {
            *self.observed_args.borrow_mut() = args.iter().map(|arg| (*arg).to_owned()).collect();
            self.drain.clone()
        }

        fn read_file(&self, _path: &str) -> SourceProbeOutcome {
            self.config.clone()
        }
    }

    const AUTONOMOUS_ENABLED_CONFIG: &str =
        r#"{"livespec-orchestrator-beads-fabro":{"dispatcher":{"autonomous_mode":true}}}"#;
    const AUTONOMOUS_DISABLED_CONFIG: &str =
        r#"{"livespec-orchestrator-beads-fabro":{"dispatcher":{"autonomous_mode":false}}}"#;

    #[test]
    fn dispatcher_drain_port_arms_autonomous_when_permission_enabled() {
        let probe = LauncherDrainProbe {
            config: SourceProbeOutcome::observed(AUTONOMOUS_ENABLED_CONFIG, true),
            drain: SourceProbeOutcome::observed("drain: dispatched 2 items", true),
            observed_args: std::cell::RefCell::new(Vec::new()),
        };
        let mut port =
            DispatcherFactoryDrainPort::new(&probe, "dispatcher", &["drain"], "cfg.jsonc");

        let outcome = port.drain_ready_queue(&drain_request());

        // The armed mode rides the `loop`/drain for this run.
        assert_eq!(outcome, Ok(FactoryDrainPortOutcome::completed(2)));
        assert_eq!(
            *probe.observed_args.borrow(),
            ["drain", "--mode", "autonomous"]
        );
    }

    #[test]
    fn dispatcher_drain_port_does_not_arm_when_permission_disabled() {
        let probe = LauncherDrainProbe {
            config: SourceProbeOutcome::observed(AUTONOMOUS_DISABLED_CONFIG, true),
            drain: SourceProbeOutcome::observed("drain: dispatched 1 items", true),
            observed_args: std::cell::RefCell::new(Vec::new()),
        };
        let mut port =
            DispatcherFactoryDrainPort::new(&probe, "dispatcher", &["drain"], "cfg.jsonc");

        let outcome = port.drain_ready_queue(&drain_request());

        // A disabled permission never arms the drain.
        assert_eq!(outcome, Ok(FactoryDrainPortOutcome::completed(1)));
        assert_eq!(*probe.observed_args.borrow(), ["drain"]);
    }

    #[test]
    fn dispatcher_drain_port_does_not_arm_when_config_unreadable() {
        let probe = LauncherDrainProbe {
            config: SourceProbeOutcome::unavailable("no .livespec.jsonc"),
            drain: SourceProbeOutcome::observed("drain: dispatched 0 items", true),
            observed_args: std::cell::RefCell::new(Vec::new()),
        };
        let mut port =
            DispatcherFactoryDrainPort::new(&probe, "dispatcher", &["drain"], "cfg.jsonc");

        let outcome = port.drain_ready_queue(&drain_request());

        // An unreadable config fails soft to disabled -- no arming.
        assert_eq!(outcome, Ok(FactoryDrainPortOutcome::completed(0)));
        assert_eq!(*probe.observed_args.borrow(), ["drain"]);
    }

    // A journal line for one auto-resolved / escalated decision, in the exact
    // wire shape the orchestrator plane's published record contract emits.
    fn autonomous_journal_line(
        work_item_id: &str,
        gate: &str,
        decision: &str,
        disposition: &str,
    ) -> String {
        format!(
            r#"{{"stage":"autonomous-decision","work_item_id":"{work_item_id}","gate":"{gate}","decision":"{decision}","disposition":"{disposition}"}}"#
        )
    }

    #[test]
    fn read_autonomous_decisions_splits_buckets_and_preserves_order() {
        let journal = [
            autonomous_journal_line("wi-1", "approve", "auto-approve", "auto-resolved"),
            autonomous_journal_line("wi-2", "acceptance", "ai-accept", "auto-resolved"),
            autonomous_journal_line("wi-3", "needs-human", "escalate", "escalated"),
        ]
        .join("\n");

        let audit = super::read_autonomous_decisions_from_journal(&journal);

        assert_eq!(audit.auto_resolutions().len(), 2);
        assert_eq!(audit.auto_resolutions()[0].work_item_id(), "wi-1");
        assert_eq!(audit.auto_resolutions()[0].gate(), "approve");
        assert_eq!(audit.auto_resolutions()[0].decision(), "auto-approve");
        assert_eq!(audit.auto_resolutions()[0].disposition(), "auto-resolved");
        assert_eq!(audit.auto_resolutions()[1].work_item_id(), "wi-2");
        assert_eq!(audit.escalations().len(), 1);
        assert_eq!(audit.escalations()[0].work_item_id(), "wi-3");
        assert_eq!(audit.escalations()[0].disposition(), "escalated");
    }

    #[test]
    fn read_autonomous_decisions_skips_malformed_and_foreign_records() {
        let journal = [
            "not json".to_owned(),
            "[1,2,3]".to_owned(),
            r#"{"stage":"calibration","work_item_id":"wi-x"}"#.to_owned(),
            r#"{"stage":"autonomous-decision","work_item_id":"wi-y","gate":"bogus","decision":"d","disposition":"auto-resolved"}"#.to_owned(),
            r#"{"stage":"autonomous-decision","work_item_id":"wi-z","gate":"approve","decision":"d","disposition":"unknown"}"#.to_owned(),
            r#"{"stage":"autonomous-decision","gate":"approve","decision":"d","disposition":"auto-resolved"}"#.to_owned(),
            autonomous_journal_line("wi-ok", "approve", "auto-approve", "auto-resolved"),
        ]
        .join("\n");

        let audit = super::read_autonomous_decisions_from_journal(&journal);

        // Only the single well-formed record survives; every malformed or
        // foreign-stage line is skipped fail-open.
        assert_eq!(audit.auto_resolutions().len(), 1);
        assert_eq!(audit.auto_resolutions()[0].work_item_id(), "wi-ok");
        assert!(audit.escalations().is_empty());
    }

    #[test]
    fn read_autonomous_decisions_empty_journal_is_empty_audit() {
        let audit = super::read_autonomous_decisions_from_journal("");

        assert_eq!(audit, super::AutonomousAudit::default());
    }

    #[test]
    fn autonomous_reflection_attention_id_maps_each_gate_to_its_valve() {
        assert_eq!(
            super::autonomous_reflection_attention_id("wi-1", "approve").as_deref(),
            Some("valve:approve:wi-1")
        );
        assert_eq!(
            super::autonomous_reflection_attention_id("wi-1", "acceptance").as_deref(),
            Some("valve:accept:wi-1")
        );
        assert_eq!(
            super::autonomous_reflection_attention_id("wi-1", "needs-human").as_deref(),
            Some("valve:set-admission:wi-1")
        );
        // An unknown gate has no reflectable needs-attention item.
        assert_eq!(
            super::autonomous_reflection_attention_id("wi-1", "mystery"),
            None
        );
    }

    #[test]
    fn journal_autonomous_decisions_port_reads_and_fails_open() {
        let observed = StubDrainProbe {
            outcome: SourceProbeOutcome::observed(
                &autonomous_journal_line("wi-1", "approve", "auto-approve", "auto-resolved"),
                true,
            ),
        };
        let port = JournalAutonomousDecisionsPort::new(&observed, "journal.jsonl");
        assert_eq!(port.read_autonomous_decisions().auto_resolutions().len(), 1);

        // A non-zero read and an unavailable journal both fail open to empty.
        let failed = StubDrainProbe {
            outcome: SourceProbeOutcome::observed("partial", false),
        };
        assert_eq!(
            JournalAutonomousDecisionsPort::new(&failed, "journal.jsonl")
                .read_autonomous_decisions(),
            AutonomousAudit::default()
        );
        let missing = StubDrainProbe {
            outcome: SourceProbeOutcome::unavailable("no journal"),
        };
        assert_eq!(
            JournalAutonomousDecisionsPort::new(&missing, "journal.jsonl")
                .read_autonomous_decisions(),
            AutonomousAudit::default()
        );
    }

    fn approve_command() -> CommandEnvelope {
        CommandEnvelope::new(
            "cmd_approve".to_owned(),
            CommandType::WorkItemApproveRequested,
            "wi-1".to_owned(),
            "wi-1:work_item.approve_requested".to_owned(),
            "operator".to_owned(),
        )
    }

    struct RecordingActionPort {
        outcome: OrchestratorActionOutcome,
        observed_action_ids: Vec<String>,
    }

    impl RecordingActionPort {
        fn returning(outcome: OrchestratorActionOutcome) -> Self {
            Self {
                outcome,
                observed_action_ids: Vec::new(),
            }
        }
    }

    impl OrchestratorActionPort for RecordingActionPort {
        fn run_action(
            &mut self,
            request: &OrchestratorActionRequest,
        ) -> super::ApplicationResult<OrchestratorActionOutcome> {
            self.observed_action_ids
                .push(request.action_id().to_owned());
            Ok(self.outcome.clone())
        }
    }

    struct ArgRecordingProbe {
        outcome: SourceProbeOutcome,
        observed_args: std::cell::RefCell<Vec<String>>,
    }

    impl SourceProbe for ArgRecordingProbe {
        fn run_command(&self, program: &str, args: &[&str]) -> SourceProbeOutcome {
            let mut recorded = vec![program.to_owned()];
            recorded.extend(args.iter().map(|arg| (*arg).to_owned()));
            *self.observed_args.borrow_mut() = recorded;
            self.outcome.clone()
        }

        fn read_file(&self, _path: &str) -> SourceProbeOutcome {
            self.outcome.clone()
        }
    }

    #[test]
    fn approve_handler_derives_action_id_and_appends_shared_work_item_events()
    -> super::ApplicationResult<()> {
        let command = approve_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome = handle_work_item_approve_command(&command, &mut port)?;

        // The console routes only through the port with `approve:<work-item-id>`.
        assert_eq!(port.observed_action_ids, ["approve:wi-1"]);
        assert_eq!(outcome.command_status(), "completed");
        assert_eq!(
            outcome
                .events()
                .iter()
                .map(ConsoleEvent::event_type)
                .collect::<Vec<_>>(),
            [
                &EventType::CommandAccepted,
                &EventType::WorkItemActionStarted,
                &EventType::WorkItemActionCompleted,
            ]
        );
        // Every outcome event is keyed by the action-id in its payload and
        // sourced by the work-item command handler.
        for (position, event) in outcome.events().iter().enumerate() {
            assert_eq!(event.payload_json(), r#"{"action_id":"approve:wi-1"}"#);
            assert_eq!(event.source(), "console:work-item-command-handler");
            assert_eq!(event.stream_seq(), position as u64 + 1);
        }
        assert_eq!(outcome.events()[0].context(), "command");
        assert_eq!(outcome.events()[2].context(), "work_item");
        Ok(())
    }

    #[test]
    fn approve_handler_records_failed_outcome_with_start() -> super::ApplicationResult<()> {
        let command = approve_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::failed());

        let outcome = handle_work_item_approve_command(&command, &mut port)?;

        assert_eq!(outcome.command_status(), "failed");
        assert_eq!(
            outcome
                .events()
                .iter()
                .map(ConsoleEvent::event_type)
                .collect::<Vec<_>>(),
            [
                &EventType::CommandAccepted,
                &EventType::WorkItemActionStarted,
                &EventType::WorkItemActionFailed,
            ]
        );
        Ok(())
    }

    #[test]
    fn approve_handler_records_not_wired_without_fabricating_start() -> super::ApplicationResult<()>
    {
        let command = approve_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::not_wired());

        let outcome = handle_work_item_approve_command(&command, &mut port)?;

        // An honest not-wired action never started, so no start event.
        assert_eq!(outcome.command_status(), "not_wired");
        assert_eq!(
            outcome
                .events()
                .iter()
                .map(ConsoleEvent::event_type)
                .collect::<Vec<_>>(),
            [
                &EventType::CommandAccepted,
                &EventType::WorkItemActionNotWired
            ]
        );
        Ok(())
    }

    #[test]
    fn approve_handler_rejects_empty_work_item_id_without_invoking_port() {
        let command = CommandEnvelope::new(
            "cmd_approve".to_owned(),
            CommandType::WorkItemApproveRequested,
            "   ".to_owned(),
            "blank:work_item.approve_requested".to_owned(),
            "operator".to_owned(),
        );
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome = handle_work_item_approve_command(&command, &mut port);

        assert_eq!(outcome, Err(ApplicationError::EmptyWorkItemId));
        assert_eq!(port.observed_action_ids, [] as [String; 0]);
    }

    fn accept_command() -> CommandEnvelope {
        CommandEnvelope::new(
            "cmd_accept".to_owned(),
            CommandType::WorkItemAcceptRequested,
            "wi-1".to_owned(),
            "wi-1:work_item.accept_requested".to_owned(),
            "operator".to_owned(),
        )
    }

    fn reject_command() -> CommandEnvelope {
        CommandEnvelope::new(
            "cmd_reject".to_owned(),
            CommandType::WorkItemRejectRequested,
            "wi-1".to_owned(),
            "wi-1:work_item.reject_requested".to_owned(),
            "operator".to_owned(),
        )
    }

    #[test]
    fn accept_handler_derives_action_id_and_routes_through_the_shared_port()
    -> super::ApplicationResult<()> {
        let command = accept_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome = handle_work_item_accept_command(&command, &mut port)?;

        // Accept carries no payload -- the action-id is just `accept:<id>`, and
        // it rides the shared `work_item` outcome family exactly like approve.
        assert_eq!(port.observed_action_ids, ["accept:wi-1"]);
        assert_eq!(outcome.command_status(), "completed");
        assert_eq!(
            outcome
                .events()
                .iter()
                .map(ConsoleEvent::event_type)
                .collect::<Vec<_>>(),
            [
                &EventType::CommandAccepted,
                &EventType::WorkItemActionStarted,
                &EventType::WorkItemActionCompleted,
            ]
        );
        for event in outcome.events() {
            assert_eq!(event.payload_json(), r#"{"action_id":"accept:wi-1"}"#);
            assert_eq!(event.source(), "console:work-item-command-handler");
        }
        Ok(())
    }

    #[test]
    fn accept_handler_rejects_empty_work_item_id_without_invoking_port() {
        let command = CommandEnvelope::new(
            "cmd_accept".to_owned(),
            CommandType::WorkItemAcceptRequested,
            "   ".to_owned(),
            "blank:work_item.accept_requested".to_owned(),
            "operator".to_owned(),
        );
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome = handle_work_item_accept_command(&command, &mut port);

        assert_eq!(outcome, Err(ApplicationError::EmptyWorkItemId));
        assert_eq!(port.observed_action_ids, [] as [String; 0]);
    }

    #[test]
    fn reject_handler_maps_regroom_payload_onto_the_reject_action_id()
    -> super::ApplicationResult<()> {
        let command = reject_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome =
            handle_work_item_reject_command(&command, r#"{"mode":"regroom"}"#, &mut port)?;

        // The mode from the payload lands in the third action-id segment.
        assert_eq!(port.observed_action_ids, ["reject:wi-1:regroom"]);
        assert_eq!(outcome.command_status(), "completed");
        for event in outcome.events() {
            assert_eq!(
                event.payload_json(),
                r#"{"action_id":"reject:wi-1:regroom"}"#
            );
        }
        Ok(())
    }

    #[test]
    fn reject_handler_maps_rework_payload_onto_the_reject_action_id() -> super::ApplicationResult<()>
    {
        let command = reject_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome = handle_work_item_reject_command(&command, r#"{"mode":"rework"}"#, &mut port)?;

        assert_eq!(port.observed_action_ids, ["reject:wi-1:rework"]);
        assert_eq!(outcome.command_status(), "completed");
        Ok(())
    }

    #[test]
    fn reject_handler_rejects_invalid_mode_without_invoking_port() {
        let command = reject_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome = handle_work_item_reject_command(&command, r#"{"mode":"bogus"}"#, &mut port);

        assert_eq!(outcome, Err(ApplicationError::InvalidRejectMode));
        assert_eq!(port.observed_action_ids, [] as [String; 0]);
    }

    #[test]
    fn reject_handler_rejects_missing_mode_without_invoking_port() {
        let command = reject_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome = handle_work_item_reject_command(&command, "{}", &mut port);

        assert_eq!(outcome, Err(ApplicationError::InvalidRejectMode));
        assert_eq!(port.observed_action_ids, [] as [String; 0]);
    }

    #[test]
    fn reject_handler_rejects_empty_work_item_id_without_invoking_port() {
        let command = CommandEnvelope::new(
            "cmd_reject".to_owned(),
            CommandType::WorkItemRejectRequested,
            "   ".to_owned(),
            "blank:work_item.reject_requested".to_owned(),
            "operator".to_owned(),
        );
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome = handle_work_item_reject_command(&command, r#"{"mode":"regroom"}"#, &mut port);

        assert_eq!(outcome, Err(ApplicationError::EmptyWorkItemId));
        assert_eq!(port.observed_action_ids, [] as [String; 0]);
    }

    #[test]
    fn reject_mode_parses_valid_values_and_rejects_others() {
        assert_eq!(RejectMode::parse("rework"), Ok(RejectMode::Rework));
        assert_eq!(RejectMode::parse("regroom"), Ok(RejectMode::Regroom));
        assert_eq!(RejectMode::Rework.as_str(), "rework");
        assert_eq!(RejectMode::Regroom.as_str(), "regroom");
        assert_eq!(
            RejectMode::parse("nonsense"),
            Err(ApplicationError::InvalidRejectMode)
        );
    }

    fn set_admission_command() -> CommandEnvelope {
        CommandEnvelope::new(
            "cmd_set_admission".to_owned(),
            CommandType::WorkItemSetAdmissionRequested,
            "wi-1".to_owned(),
            "wi-1:work_item.set_admission_requested".to_owned(),
            "operator".to_owned(),
        )
    }

    #[test]
    fn set_admission_handler_maps_auto_payload_onto_the_action_id() -> super::ApplicationResult<()>
    {
        let command = set_admission_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome =
            handle_work_item_set_admission_command(&command, r#"{"policy":"auto"}"#, &mut port)?;

        // The policy from the payload lands in the third action-id segment.
        assert_eq!(port.observed_action_ids, ["set-admission:wi-1:auto"]);
        assert_eq!(outcome.command_status(), "completed");
        for event in outcome.events() {
            assert_eq!(
                event.payload_json(),
                r#"{"action_id":"set-admission:wi-1:auto"}"#
            );
        }
        Ok(())
    }

    #[test]
    fn set_admission_handler_maps_manual_payload_onto_the_action_id() -> super::ApplicationResult<()>
    {
        let command = set_admission_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome =
            handle_work_item_set_admission_command(&command, r#"{"policy":"manual"}"#, &mut port)?;

        assert_eq!(port.observed_action_ids, ["set-admission:wi-1:manual"]);
        assert_eq!(outcome.command_status(), "completed");
        Ok(())
    }

    #[test]
    fn set_admission_handler_rejects_invalid_policy_without_invoking_port() {
        let command = set_admission_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome =
            handle_work_item_set_admission_command(&command, r#"{"policy":"bogus"}"#, &mut port);

        assert_eq!(outcome, Err(ApplicationError::InvalidAdmissionPolicy));
        assert_eq!(port.observed_action_ids, [] as [String; 0]);
    }

    #[test]
    fn set_admission_handler_rejects_missing_policy_without_invoking_port() {
        let command = set_admission_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome = handle_work_item_set_admission_command(&command, "{}", &mut port);

        assert_eq!(outcome, Err(ApplicationError::InvalidAdmissionPolicy));
        assert_eq!(port.observed_action_ids, [] as [String; 0]);
    }

    #[test]
    fn set_admission_handler_rejects_empty_work_item_id_without_invoking_port() {
        let command = CommandEnvelope::new(
            "cmd_set_admission".to_owned(),
            CommandType::WorkItemSetAdmissionRequested,
            "   ".to_owned(),
            "blank:work_item.set_admission_requested".to_owned(),
            "operator".to_owned(),
        );
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome =
            handle_work_item_set_admission_command(&command, r#"{"policy":"auto"}"#, &mut port);

        assert_eq!(outcome, Err(ApplicationError::EmptyWorkItemId));
        assert_eq!(port.observed_action_ids, [] as [String; 0]);
    }

    #[test]
    fn set_admission_policy_from_payload_parses_valid_values_and_rejects_others() {
        assert_eq!(
            set_admission_policy_from_payload(r#"{"policy":"auto"}"#),
            Ok(AdmissionPolicy::Auto)
        );
        assert_eq!(
            set_admission_policy_from_payload(r#"{"policy":"manual"}"#),
            Ok(AdmissionPolicy::Manual)
        );
        assert_eq!(
            set_admission_policy_from_payload(r#"{"policy":"bogus"}"#),
            Err(ApplicationError::InvalidAdmissionPolicy)
        );
        assert_eq!(
            set_admission_policy_from_payload("{}"),
            Err(ApplicationError::InvalidAdmissionPolicy)
        );
        assert_eq!(
            set_admission_policy_from_payload("not json"),
            Err(ApplicationError::InvalidAdmissionPolicy)
        );
    }

    fn set_acceptance_command() -> CommandEnvelope {
        CommandEnvelope::new(
            "cmd_set_acceptance".to_owned(),
            CommandType::WorkItemSetAcceptanceRequested,
            "wi-1".to_owned(),
            "wi-1:work_item.set_acceptance_requested".to_owned(),
            "operator".to_owned(),
        )
    }

    #[test]
    fn set_acceptance_handler_maps_ai_only_payload_onto_the_action_id()
    -> super::ApplicationResult<()> {
        let command = set_acceptance_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
        let payload = r#"{"policy":"ai-only"}"#;

        let outcome = handle_work_item_set_acceptance_command(&command, payload, &mut port)?;

        // The policy from the payload lands in the third action-id segment.
        assert_eq!(port.observed_action_ids, ["set-acceptance:wi-1:ai-only"]);
        assert_eq!(outcome.command_status(), "completed");
        for event in outcome.events() {
            assert_eq!(
                event.payload_json(),
                r#"{"action_id":"set-acceptance:wi-1:ai-only"}"#
            );
        }
        Ok(())
    }

    #[test]
    fn set_acceptance_handler_maps_human_only_payload_onto_the_action_id()
    -> super::ApplicationResult<()> {
        let command = set_acceptance_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
        let payload = r#"{"policy":"human-only"}"#;

        let outcome = handle_work_item_set_acceptance_command(&command, payload, &mut port)?;

        assert_eq!(port.observed_action_ids, ["set-acceptance:wi-1:human-only"]);
        assert_eq!(outcome.command_status(), "completed");
        Ok(())
    }

    #[test]
    fn set_acceptance_handler_maps_ai_then_human_payload_onto_the_action_id()
    -> super::ApplicationResult<()> {
        let command = set_acceptance_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
        let payload = r#"{"policy":"ai-then-human"}"#;

        let outcome = handle_work_item_set_acceptance_command(&command, payload, &mut port)?;

        assert_eq!(
            port.observed_action_ids,
            ["set-acceptance:wi-1:ai-then-human"]
        );
        assert_eq!(outcome.command_status(), "completed");
        Ok(())
    }

    #[test]
    fn set_acceptance_handler_rejects_invalid_policy_without_invoking_port() {
        let command = set_acceptance_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome =
            handle_work_item_set_acceptance_command(&command, r#"{"policy":"bogus"}"#, &mut port);

        assert_eq!(outcome, Err(ApplicationError::InvalidAcceptancePolicy));
        assert_eq!(port.observed_action_ids, [] as [String; 0]);
    }

    #[test]
    fn set_acceptance_handler_rejects_missing_policy_without_invoking_port() {
        let command = set_acceptance_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome = handle_work_item_set_acceptance_command(&command, "{}", &mut port);

        assert_eq!(outcome, Err(ApplicationError::InvalidAcceptancePolicy));
        assert_eq!(port.observed_action_ids, [] as [String; 0]);
    }

    #[test]
    fn set_acceptance_handler_rejects_empty_work_item_id_without_invoking_port() {
        let command = CommandEnvelope::new(
            "cmd_set_acceptance".to_owned(),
            CommandType::WorkItemSetAcceptanceRequested,
            "   ".to_owned(),
            "blank:work_item.set_acceptance_requested".to_owned(),
            "operator".to_owned(),
        );
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome =
            handle_work_item_set_acceptance_command(&command, r#"{"policy":"ai-only"}"#, &mut port);

        assert_eq!(outcome, Err(ApplicationError::EmptyWorkItemId));
        assert_eq!(port.observed_action_ids, [] as [String; 0]);
    }

    #[test]
    fn set_acceptance_policy_from_payload_parses_valid_values_and_rejects_others() {
        assert_eq!(
            set_acceptance_policy_from_payload(r#"{"policy":"ai-only"}"#),
            Ok(AcceptancePolicy::AiOnly)
        );
        assert_eq!(
            set_acceptance_policy_from_payload(r#"{"policy":"human-only"}"#),
            Ok(AcceptancePolicy::HumanOnly)
        );
        assert_eq!(
            set_acceptance_policy_from_payload(r#"{"policy":"ai-then-human"}"#),
            Ok(AcceptancePolicy::AiThenHuman)
        );
        assert_eq!(
            set_acceptance_policy_from_payload(r#"{"policy":"bogus"}"#),
            Err(ApplicationError::InvalidAcceptancePolicy)
        );
        assert_eq!(
            set_acceptance_policy_from_payload("{}"),
            Err(ApplicationError::InvalidAcceptancePolicy)
        );
        assert_eq!(
            set_acceptance_policy_from_payload("not json"),
            Err(ApplicationError::InvalidAcceptancePolicy)
        );
    }

    #[test]
    fn dispatcher_action_port_shells_drive_with_action_and_completes() {
        let probe = ArgRecordingProbe {
            outcome: SourceProbeOutcome::observed("approved", true),
            observed_args: std::cell::RefCell::new(Vec::new()),
        };
        let mut port = DispatcherOrchestratorActionPort::new(&probe, "drive", &["--repo", "/repo"]);

        let outcome = port.run_action(&OrchestratorActionRequest::new("approve:wi-1".to_owned()));

        assert_eq!(outcome, Ok(OrchestratorActionOutcome::completed()));
        assert_eq!(
            probe
                .observed_args
                .borrow()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["drive", "--repo", "/repo", "--action", "approve:wi-1"]
        );
        // The action port never reads files; the probe's file capability still
        // honours the honest-observation contract.
        assert_eq!(
            probe.read_file("/unused"),
            SourceProbeOutcome::observed("approved", true)
        );
    }

    #[test]
    fn dispatcher_action_port_fails_on_non_zero_run() {
        let probe = StubDrainProbe {
            outcome: SourceProbeOutcome::observed("approve error", false),
        };
        let mut port = DispatcherOrchestratorActionPort::new(&probe, "drive", &["--repo", "/repo"]);

        let outcome = port.run_action(&OrchestratorActionRequest::new("approve:wi-1".to_owned()));

        assert_eq!(outcome, Ok(OrchestratorActionOutcome::failed()));
    }

    #[test]
    fn dispatcher_action_port_is_not_wired_when_unavailable() {
        let probe = StubDrainProbe {
            outcome: SourceProbeOutcome::unavailable("drive binary not found"),
        };
        let mut port = DispatcherOrchestratorActionPort::new(&probe, "drive", &["--repo", "/repo"]);

        let outcome = port.run_action(&OrchestratorActionRequest::new("approve:wi-1".to_owned()));

        assert_eq!(outcome, Ok(OrchestratorActionOutcome::not_wired()));
    }

    // -----------------------------------------------------------------------
    // Configuration context — full autonomous mode arming.
    // -----------------------------------------------------------------------

    /// A `.livespec.jsonc` fixture with the orchestrator object present but no
    /// `dispatcher` block, mirroring the console's own committed config.
    const CONFIG_WITHOUT_DISPATCHER: &str = r#"{
  "template": "livespec-with-diagrams",
  // a comment mentioning // and /* not a real comment */ inside prose
  "livespec-orchestrator-beads-fabro": {
    "format": "beads",
    "connection": { "tenant": "livespec-console-beads-fabro" }
  }
}"#;

    fn autonomous_mode_set_command() -> CommandEnvelope {
        CommandEnvelope::new(
            "cmd_autonomous".to_owned(),
            CommandType::ConfigAutonomousModeSet,
            "livespec-console-beads-fabro".to_owned(),
            "livespec-console-beads-fabro:config.autonomous_mode_set".to_owned(),
            "operator".to_owned(),
        )
    }

    /// Arming port double recording the requests it receives and returning a
    /// scripted outcome, so handler tests are decoupled from file I/O.
    struct RecordingArmingPort {
        outcome: AutonomousModeArmingOutcome,
        requests: Vec<AutonomousModeArmingRequest>,
    }

    impl RecordingArmingPort {
        fn new(outcome: AutonomousModeArmingOutcome) -> Self {
            Self {
                outcome,
                requests: Vec::new(),
            }
        }
    }

    impl AutonomousModeArmingPort for RecordingArmingPort {
        fn arm(
            &mut self,
            request: &AutonomousModeArmingRequest,
        ) -> super::ApplicationResult<AutonomousModeArmingOutcome> {
            self.requests.push(request.clone());
            Ok(self.outcome.clone())
        }
    }

    /// `SourceProbe` double for the real arming port: it returns scripted
    /// read/write outcomes and records the bytes handed to `write_file`.
    struct ConfigFileProbe {
        read_outcome: SourceProbeOutcome,
        write_outcome: SourceProbeOutcome,
        writes: RefCell<Vec<(String, String)>>,
    }

    impl SourceProbe for ConfigFileProbe {
        fn run_command(&self, _program: &str, _args: &[&str]) -> SourceProbeOutcome {
            SourceProbeOutcome::unavailable("run_command unused by arming port")
        }

        fn read_file(&self, _path: &str) -> SourceProbeOutcome {
            self.read_outcome.clone()
        }

        fn write_file(&self, path: &str, contents: &str) -> SourceProbeOutcome {
            self.writes
                .borrow_mut()
                .push((path.to_owned(), contents.to_owned()));
            self.write_outcome.clone()
        }
    }

    /// Read-only `SourceProbe` double inheriting the trait's default
    /// `write_file` (an honest not-wired outcome), exercising that default body.
    struct ReadOnlyConfigProbe {
        read_outcome: SourceProbeOutcome,
    }

    impl SourceProbe for ReadOnlyConfigProbe {
        fn run_command(&self, _program: &str, _args: &[&str]) -> SourceProbeOutcome {
            SourceProbeOutcome::unavailable("run_command unused by arming port")
        }

        fn read_file(&self, _path: &str) -> SourceProbeOutcome {
            self.read_outcome.clone()
        }
    }

    fn event_types(outcome: &ConfigCommandOutcome) -> Vec<EventType> {
        outcome
            .events()
            .iter()
            .map(|event| *event.event_type())
            .collect()
    }

    #[test]
    fn autonomous_mode_event_labels_are_present() {
        assert_eq!(
            EventType::ConfigAutonomousModeEnabled.label(),
            "Autonomous mode enabled"
        );
        assert_eq!(
            EventType::ConfigAutonomousModeDisabled.label(),
            "Autonomous mode disabled"
        );
        assert_eq!(
            EventType::FactoryAutonomousModeEnableRequested.label(),
            "Autonomous mode enable requested"
        );
        assert_eq!(
            EventType::FactoryAutonomousModeDisableRequested.label(),
            "Autonomous mode disable requested"
        );
        assert_eq!(
            EventType::FactoryAutonomousModeNotWired.label(),
            "Autonomous mode arming not wired"
        );
    }

    #[test]
    fn autonomous_mode_set_request_exposes_its_fields() {
        let request = AutonomousModeSetRequest::new("repo-a".to_owned(), true, false);
        assert_eq!(request.repo(), "repo-a");
        assert!(request.enabled());
        assert!(!request.confirmed());
    }

    #[test]
    fn autonomous_mode_set_request_parses_a_valid_payload() {
        assert_eq!(
            AutonomousModeSetRequest::from_payload_json(
                r#"{"repo":"repo-a","enabled":true,"confirmed":true}"#
            ),
            Ok(AutonomousModeSetRequest::new(
                "repo-a".to_owned(),
                true,
                true
            ))
        );
    }

    #[test]
    fn autonomous_mode_set_request_rejects_malformed_or_incomplete_payloads() {
        for payload in [
            "not json",
            r#"{"enabled":true,"confirmed":true}"#,
            r#"{"repo":"  ","enabled":true,"confirmed":true}"#,
            r#"{"repo":"repo-a","confirmed":true}"#,
            r#"{"repo":"repo-a","enabled":true}"#,
            r#"{"repo":"repo-a","enabled":"yes","confirmed":true}"#,
        ] {
            assert_eq!(
                AutonomousModeSetRequest::from_payload_json(payload),
                Err(ApplicationError::InvalidAutonomousModePayload)
            );
        }
    }

    #[test]
    fn read_autonomous_mode_defaults_to_disabled_when_key_is_absent() {
        assert!(!read_autonomous_mode_from_jsonc(CONFIG_WITHOUT_DISPATCHER));
        assert!(!read_autonomous_mode_from_jsonc("{}"));
        assert!(!read_autonomous_mode_from_jsonc(
            r#"{"livespec-orchestrator-beads-fabro":{"dispatcher":{}}}"#
        ));
        // A non-boolean value is treated as disabled.
        assert!(!read_autonomous_mode_from_jsonc(
            r#"{"livespec-orchestrator-beads-fabro":{"dispatcher":{"autonomous_mode":"yes"}}}"#
        ));
        // An unparseable document is fail-soft to disabled.
        assert!(!read_autonomous_mode_from_jsonc("{ not json"));
    }

    #[test]
    fn read_autonomous_mode_reads_the_written_key() {
        assert!(read_autonomous_mode_from_jsonc(
            r#"{"livespec-orchestrator-beads-fabro":{"dispatcher":{"autonomous_mode":true}}}"#
        ));
        assert!(!read_autonomous_mode_from_jsonc(
            r#"{"livespec-orchestrator-beads-fabro":{"dispatcher":{"autonomous_mode":false}}}"#
        ));
    }

    /// Whether `set_autonomous_mode_in_jsonc` produced an edit that reads back as
    /// `enabled`.
    fn set_then_read(text: &str, enabled: bool) -> Option<bool> {
        set_autonomous_mode_in_jsonc(text, enabled)
            .as_deref()
            .map(read_autonomous_mode_from_jsonc)
    }

    #[test]
    fn set_autonomous_mode_inserts_dispatcher_into_the_orchestrator_object() {
        let updated = set_autonomous_mode_in_jsonc(CONFIG_WITHOUT_DISPATCHER, true);
        // The comment and the other members survive; the new key reads back true.
        assert!(
            updated
                .as_deref()
                .is_some_and(|u| u.contains("not a real comment"))
        );
        assert!(
            updated
                .as_deref()
                .is_some_and(|u| u.contains("\"format\": \"beads\""))
        );
        assert_eq!(
            updated.as_deref().map(read_autonomous_mode_from_jsonc),
            Some(true)
        );
    }

    #[test]
    fn set_autonomous_mode_replaces_an_existing_boolean_value() {
        let original =
            r#"{"livespec-orchestrator-beads-fabro":{"dispatcher":{"autonomous_mode":false}}}"#;
        assert_eq!(set_then_read(original, true), Some(true));
        let enabled =
            r#"{"livespec-orchestrator-beads-fabro":{"dispatcher":{"autonomous_mode":true}}}"#;
        assert_eq!(set_then_read(enabled, false), Some(false));
    }

    #[test]
    fn set_autonomous_mode_replaces_a_string_shaped_value() {
        // A non-boolean value token is still replaced wholesale by the literal.
        let original =
            r#"{"livespec-orchestrator-beads-fabro":{"dispatcher":{"autonomous_mode":"off"}}}"#;
        assert_eq!(set_then_read(original, true), Some(true));
    }

    #[test]
    fn set_autonomous_mode_inserts_the_key_into_an_existing_dispatcher() {
        let original = r#"{"livespec-orchestrator-beads-fabro":{"dispatcher":{"wip_cap":3}}}"#;
        let updated = set_autonomous_mode_in_jsonc(original, true);
        assert!(
            updated
                .as_deref()
                .is_some_and(|u| u.contains("\"wip_cap\":3"))
        );
        assert_eq!(
            updated.as_deref().map(read_autonomous_mode_from_jsonc),
            Some(true)
        );
    }

    #[test]
    fn set_autonomous_mode_inserts_into_an_empty_dispatcher_without_a_trailing_comma() {
        let original = r#"{"livespec-orchestrator-beads-fabro":{"dispatcher":{}}}"#;
        assert_eq!(set_then_read(original, true), Some(true));
    }

    #[test]
    fn set_autonomous_mode_creates_the_whole_block_when_orchestrator_is_absent() {
        let original = r#"{"template": "livespec-with-diagrams"}"#;
        let updated = set_autonomous_mode_in_jsonc(original, true);
        assert!(
            updated
                .as_deref()
                .is_some_and(|u| u.contains("\"template\": \"livespec-with-diagrams\""))
        );
        assert_eq!(
            updated.as_deref().map(read_autonomous_mode_from_jsonc),
            Some(true)
        );
    }

    #[test]
    fn set_autonomous_mode_creates_the_block_in_an_empty_object() {
        assert_eq!(set_then_read("{}", true), Some(true));
    }

    #[test]
    fn set_autonomous_mode_returns_none_for_a_non_object_document() {
        assert_eq!(set_autonomous_mode_in_jsonc("[1, 2, 3]", true), None);
        assert_eq!(set_autonomous_mode_in_jsonc("   \"a string\"", true), None);
    }

    #[test]
    fn read_autonomous_mode_handles_block_comments_and_escaped_strings() {
        let config = r#"{
  /* a block comment with "quotes" and // slashes and a * star */
  "note": "value with \"escaped\" quotes and a \\ backslash and // not a comment",
  "livespec-orchestrator-beads-fabro": { "dispatcher": { "autonomous_mode": true } }
}"#;
        assert!(read_autonomous_mode_from_jsonc(config));
    }

    #[test]
    fn set_autonomous_mode_scans_past_block_comments_and_escaped_strings() {
        let config = r#"{
  /* block comment before the target */
  "note": "a \"tricky\" value",
  "livespec-orchestrator-beads-fabro": {
    "dispatcher": { "autonomous_mode": false }
  }
}"#;
        assert_eq!(set_then_read(config, true), Some(true));
    }

    #[test]
    fn jsonc_helpers_consume_unterminated_block_comments_to_end_of_input() {
        // strip_jsonc_comments consumes an unterminated block comment to EOF, so
        // the document fails to parse and reads as disabled.
        assert!(!read_autonomous_mode_from_jsonc("/* unterminated"));
        // The scanner's skip likewise consumes an unterminated block comment to
        // EOF and finds no object to edit.
        assert_eq!(set_autonomous_mode_in_jsonc("/* unterminated", true), None);
    }

    #[test]
    fn set_autonomous_mode_falls_through_when_a_member_is_not_an_object() {
        // A `dispatcher` whose value is not an object cannot be opened; the edit
        // still produces output via a higher-level fallback.
        let dispatcher_scalar = r#"{"livespec-orchestrator-beads-fabro":{"dispatcher":5}}"#;
        assert!(set_autonomous_mode_in_jsonc(dispatcher_scalar, true).is_some());
        // An orchestrator key whose value is not an object falls through to the
        // top-level block insertion.
        let orchestrator_scalar = r#"{"livespec-orchestrator-beads-fabro":5}"#;
        assert!(set_autonomous_mode_in_jsonc(orchestrator_scalar, true).is_some());
    }

    #[test]
    fn arming_port_arms_when_config_is_readable_and_writable() {
        let probe = ConfigFileProbe {
            read_outcome: SourceProbeOutcome::observed(CONFIG_WITHOUT_DISPATCHER, true),
            write_outcome: SourceProbeOutcome::observed("", true),
            writes: RefCell::new(Vec::new()),
        };
        let mut port = LivespecJsoncArmingPort::new(&probe, "/repo/.livespec.jsonc");

        let outcome = port.arm(&AutonomousModeArmingRequest::new("repo-a".to_owned(), true));

        assert_eq!(outcome, Ok(AutonomousModeArmingOutcome::armed()));
        let writes = probe.writes.borrow();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].0, "/repo/.livespec.jsonc");
        // The port wrote the EDITED content, so the key reads back enabled.
        assert!(read_autonomous_mode_from_jsonc(&writes[0].1));
        // The arming port never runs commands.
        assert!(matches!(
            probe.run_command("unused", &[]),
            SourceProbeOutcome::Unavailable { .. }
        ));
    }

    #[test]
    fn arming_port_is_not_wired_when_the_config_cannot_be_read() {
        let probe = ConfigFileProbe {
            read_outcome: SourceProbeOutcome::unavailable("no such file"),
            write_outcome: SourceProbeOutcome::observed("", true),
            writes: RefCell::new(Vec::new()),
        };
        let mut port = LivespecJsoncArmingPort::new(&probe, "/missing/.livespec.jsonc");

        let outcome = port.arm(&AutonomousModeArmingRequest::new("repo-a".to_owned(), true));

        assert_eq!(outcome, Ok(AutonomousModeArmingOutcome::not_wired()));
        assert!(probe.writes.borrow().is_empty());
    }

    #[test]
    fn arming_port_is_not_wired_when_the_read_reports_failure() {
        let probe = ConfigFileProbe {
            read_outcome: SourceProbeOutcome::observed("partial", false),
            write_outcome: SourceProbeOutcome::observed("", true),
            writes: RefCell::new(Vec::new()),
        };
        let mut port = LivespecJsoncArmingPort::new(&probe, "/repo/.livespec.jsonc");

        let outcome = port.arm(&AutonomousModeArmingRequest::new(
            "repo-a".to_owned(),
            false,
        ));

        assert_eq!(outcome, Ok(AutonomousModeArmingOutcome::not_wired()));
    }

    #[test]
    fn arming_port_is_not_wired_when_the_config_is_malformed() {
        let probe = ConfigFileProbe {
            read_outcome: SourceProbeOutcome::observed("[not an object]", true),
            write_outcome: SourceProbeOutcome::observed("", true),
            writes: RefCell::new(Vec::new()),
        };
        let mut port = LivespecJsoncArmingPort::new(&probe, "/repo/.livespec.jsonc");

        let outcome = port.arm(&AutonomousModeArmingRequest::new("repo-a".to_owned(), true));

        assert_eq!(outcome, Ok(AutonomousModeArmingOutcome::not_wired()));
        assert!(probe.writes.borrow().is_empty());
    }

    #[test]
    fn arming_port_is_not_wired_when_the_write_fails() {
        let probe = ConfigFileProbe {
            read_outcome: SourceProbeOutcome::observed(CONFIG_WITHOUT_DISPATCHER, true),
            write_outcome: SourceProbeOutcome::observed("disk error", false),
            writes: RefCell::new(Vec::new()),
        };
        let mut port = LivespecJsoncArmingPort::new(&probe, "/repo/.livespec.jsonc");

        let outcome = port.arm(&AutonomousModeArmingRequest::new("repo-a".to_owned(), true));

        assert_eq!(outcome, Ok(AutonomousModeArmingOutcome::not_wired()));
    }

    #[test]
    fn arming_port_is_not_wired_when_no_write_capability_is_present() {
        // The read-only probe inherits the trait's default write_file (an honest
        // not-wired outcome), so the arming surface is not wired.
        let probe = ReadOnlyConfigProbe {
            read_outcome: SourceProbeOutcome::observed(CONFIG_WITHOUT_DISPATCHER, true),
        };
        // The read-only double never runs commands either.
        assert!(matches!(
            probe.run_command("unused", &[]),
            SourceProbeOutcome::Unavailable { .. }
        ));
        let mut port = LivespecJsoncArmingPort::new(&probe, "/repo/.livespec.jsonc");

        let outcome = port.arm(&AutonomousModeArmingRequest::new("repo-a".to_owned(), true));

        assert_eq!(outcome, Ok(AutonomousModeArmingOutcome::not_wired()));
    }

    /// The event contexts of a handled config outcome, for assertion without
    /// extracting the outcome out of its `Result`.
    fn event_contexts(outcome: &super::ApplicationResult<ConfigCommandOutcome>) -> Vec<String> {
        outcome
            .as_ref()
            .map(|handled| {
                handled
                    .events()
                    .iter()
                    .map(|event| event.context().to_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The parsed payload of the config outcome's audit event (index 2), or
    /// `Null` when absent.
    fn audit_payload(
        outcome: &super::ApplicationResult<ConfigCommandOutcome>,
    ) -> serde_json::Value {
        outcome
            .as_ref()
            .ok()
            .and_then(|handled| handled.events().get(2))
            .map(|event| serde_json::from_str(event.payload_json()).unwrap_or_default())
            .unwrap_or_default()
    }

    #[test]
    fn config_handler_rejects_an_unconfirmed_enable_with_no_effect() {
        let mut port = RecordingArmingPort::new(AutonomousModeArmingOutcome::armed());
        let outcome = handle_config_autonomous_mode_set_command(
            &autonomous_mode_set_command(),
            r#"{"repo":"repo-a","enabled":true,"confirmed":false}"#,
            "2026-07-11T00:00:00Z",
            &mut port,
        );

        assert_eq!(
            outcome.as_ref().map(ConfigCommandOutcome::command_status),
            Ok("rejected")
        );
        // Only the rejection is recorded -- no factory arming and no audit.
        assert_eq!(
            outcome.as_ref().map(event_types),
            Ok(vec![EventType::CommandRejected])
        );
        assert_eq!(event_contexts(&outcome), ["command"]);
        // The arming port was NEVER called: no key write.
        assert!(port.requests.is_empty());
    }

    #[test]
    fn config_handler_arms_and_audits_a_confirmed_enable() {
        let mut port = RecordingArmingPort::new(AutonomousModeArmingOutcome::armed());
        let outcome = handle_config_autonomous_mode_set_command(
            &autonomous_mode_set_command(),
            r#"{"repo":"repo-a","enabled":true,"confirmed":true}"#,
            "2026-07-11T00:00:00Z",
            &mut port,
        );

        assert_eq!(
            outcome.as_ref().map(ConfigCommandOutcome::command_status),
            Ok("completed")
        );
        assert_eq!(
            outcome.as_ref().map(event_types),
            Ok(vec![
                EventType::CommandAccepted,
                EventType::FactoryAutonomousModeEnableRequested,
                EventType::ConfigAutonomousModeEnabled,
            ])
        );
        // The factory arming command was issued through the port, enabled.
        assert_eq!(port.requests.len(), 1);
        assert_eq!(port.requests[0].repo(), "repo-a");
        assert!(port.requests[0].enabled());
        // The factory event is in the factory context; the audit event is in
        // the configuration context.
        assert_eq!(
            event_contexts(&outcome),
            ["command", "factory", "configuration"]
        );
        // The audit event carries { repo, actor, occurred_at }.
        let payload = audit_payload(&outcome);
        assert_eq!(payload["repo"], "repo-a");
        assert_eq!(payload["actor"], "operator");
        assert_eq!(payload["occurred_at"], "2026-07-11T00:00:00Z");
    }

    #[test]
    fn config_handler_arms_and_audits_a_disable_without_requiring_confirmation() {
        let mut port = RecordingArmingPort::new(AutonomousModeArmingOutcome::armed());
        let outcome = handle_config_autonomous_mode_set_command(
            &autonomous_mode_set_command(),
            r#"{"repo":"repo-a","enabled":false,"confirmed":false}"#,
            "2026-07-11T00:00:01Z",
            &mut port,
        );

        assert_eq!(
            outcome.as_ref().map(ConfigCommandOutcome::command_status),
            Ok("completed")
        );
        assert_eq!(
            outcome.as_ref().map(event_types),
            Ok(vec![
                EventType::CommandAccepted,
                EventType::FactoryAutonomousModeDisableRequested,
                EventType::ConfigAutonomousModeDisabled,
            ])
        );
        assert_eq!(audit_payload(&outcome)["repo"], "repo-a");
        assert_eq!(port.requests.len(), 1);
        assert!(!port.requests[0].enabled());
    }

    #[test]
    fn config_handler_surfaces_not_wired_without_an_audit_event() {
        let mut port = RecordingArmingPort::new(AutonomousModeArmingOutcome::not_wired());
        let outcome = handle_config_autonomous_mode_set_command(
            &autonomous_mode_set_command(),
            r#"{"repo":"repo-a","enabled":true,"confirmed":true}"#,
            "2026-07-11T00:00:02Z",
            &mut port,
        );

        assert_eq!(
            outcome.as_ref().map(ConfigCommandOutcome::command_status),
            Ok("not_wired")
        );
        // The honest not-wired outcome, and NO audit event.
        assert_eq!(
            outcome.as_ref().map(event_types),
            Ok(vec![
                EventType::CommandAccepted,
                EventType::FactoryAutonomousModeNotWired,
            ])
        );
        assert_eq!(event_contexts(&outcome), ["command", "factory"]);
    }

    #[test]
    fn config_handler_rejects_a_malformed_payload() {
        let mut port = RecordingArmingPort::new(AutonomousModeArmingOutcome::armed());
        let outcome = handle_config_autonomous_mode_set_command(
            &autonomous_mode_set_command(),
            "not json",
            "2026-07-11T00:00:03Z",
            &mut port,
        );

        assert_eq!(outcome, Err(ApplicationError::InvalidAutonomousModePayload));
        assert!(port.requests.is_empty());
    }

    // -----------------------------------------------------------------------
    // TUI autonomous-mode surface (C3 slice 2): toggle, type-to-confirm modal,
    // dangerous label, and header indicator for the selected repo.
    // -----------------------------------------------------------------------

    const CONFIRM_REPO: &str = "livespec-console-beads-fabro";

    /// A model over the given overlay whose selected repo carries the given
    /// derived autonomous mode, built with no events (no attention items).
    fn autonomous_model(
        overlay: TuiOverlay,
        selected_repo: &str,
        autonomous_mode_enabled: bool,
    ) -> TuiScreenModel {
        let state = TuiInteractionState::new(0, overlay)
            .with_selected_repo(selected_repo.to_owned())
            .with_autonomous_mode_enabled(autonomous_mode_enabled);
        build_tui_model_for_state(&[], &state)
    }

    #[test]
    fn header_reflects_the_selected_repo_and_its_autonomous_mode() {
        let on = autonomous_model(TuiOverlay::None, CONFIRM_REPO, true);
        assert_eq!(on.selected_repo(), CONFIRM_REPO);
        assert!(on.autonomous_mode_enabled());
        assert!(on.header().contains(&format!("repo: {CONFIRM_REPO}")));
        assert!(on.header().contains("autonomous: on"));

        let off = autonomous_model(TuiOverlay::None, CONFIRM_REPO, false);
        assert!(!off.autonomous_mode_enabled());
        assert!(off.header().contains("autonomous: off"));
    }

    #[test]
    fn header_counts_and_names_sources_that_degraded_to_not_observed() {
        // Cockpit-blind: two sources emitted a not-observed finding this cycle.
        // The model counts and names them (sorted) so the header can surface a
        // source-unavailability indicator instead of a silently-empty view.
        let blind_events = [
            ConsoleEvent::fixture(
                "evt_orchestrator_not_observed",
                EventType::SourceNotObservedFindingObserved,
                "orchestrator",
            ),
            ConsoleEvent::fixture(
                "evt_github_not_observed",
                EventType::SourceNotObservedFindingObserved,
                "github",
            ),
        ];
        let blind = build_tui_model(&blind_events, 0);
        assert_eq!(
            blind.unavailable_sources(),
            ["github".to_owned(), "orchestrator".to_owned()]
        );
        assert!(
            blind
                .header()
                .contains("sources: 2 unavailable (github, orchestrator)")
        );
    }

    #[test]
    fn header_shows_no_unavailability_count_when_every_source_is_observed() {
        // Factory-idle: no not-observed finding, so no phantom count and no
        // false alarm -- a true-empty screen stays clean.
        let idle = build_tui_model(&[], 0);
        assert!(idle.unavailable_sources().is_empty());
        assert!(!idle.header().contains("unavailable"));
        assert!(!idle.header().contains("sources:"));
    }

    /// A model whose selected repo is `repo` and whose header reports each name
    /// in `sources` as a not-observed (unavailable) backing source this cycle.
    fn blind_model(repo: &str, sources: &[&str]) -> TuiScreenModel {
        let events: Vec<ConsoleEvent> = sources
            .iter()
            .map(|&source| {
                ConsoleEvent::fixture(
                    &format!("evt_{source}_not_observed"),
                    EventType::SourceNotObservedFindingObserved,
                    source,
                )
            })
            .collect();
        let state =
            TuiInteractionState::new(0, TuiOverlay::None).with_selected_repo(repo.to_owned());
        build_tui_model_for_state(&events, &state)
    }

    #[test]
    fn header_line_fits_the_pinned_width_and_preserves_the_priority_fields() {
        // The dogfood target is a 112-column terminal (inner width 110 inside the
        // header block's borders) with several sources down. The header MUST fit
        // and keep the operationally-important fields plus the cockpit-blind tell
        // (the source count), degrading only the constant fields and the names.
        let model = blind_model(
            CONFIRM_REPO,
            &["dispatcher", "fabro", "github", "livespec", "orchestrator"],
        );
        let line = model.header_line(110);
        assert!(line.chars().count() <= 110);
        assert!(line.contains(&format!("repo: {CONFIRM_REPO}")));
        assert!(line.contains("autonomous: off"));
        assert!(line.contains("view: Attention"));
        assert!(line.contains("attention: 0"));
        // The count survives even when the names cannot: how-many is the tell.
        assert!(line.contains("sources: 5 unavailable"));
    }

    #[test]
    fn header_line_matches_the_canonical_header_when_wide() {
        // Given room to spare, the fitted header is the full canonical header --
        // every field and every source name, nothing dropped.
        let model = blind_model("-", &["fabro", "github"]);
        let line = model.header_line(300);
        assert_eq!(line, model.header());
        assert!(line.contains("sources: 2 unavailable (fabro, github)"));
    }

    #[test]
    fn header_line_elides_source_names_before_dropping_priority_fields() {
        // At an intermediate width the names abbreviate to a `+N more` marker
        // while the priority fields stay whole -- never a mid-field truncation.
        let model = blind_model(CONFIRM_REPO, &["alpha", "bravo", "charlie"]);
        let line = model.header_line(130);
        assert!(line.chars().count() <= 130);
        assert!(line.contains("+2 more"));
        assert!(line.contains(&format!("repo: {CONFIRM_REPO}")));
        assert!(line.contains("attention: 0"));
    }

    #[test]
    fn header_line_never_drops_the_source_count_repo_or_autonomous() {
        // Even on an absurdly narrow terminal (below the target), the header keeps
        // the source count (the blind-vs-idle tell) and the repo / autonomous
        // fields; only lower-value fields and the source names are shed.
        let model = blind_model(CONFIRM_REPO, &["fabro", "github", "orchestrator"]);
        let line = model.header_line(60);
        assert!(line.contains("sources: 3 unavailable"));
        assert!(line.contains(&format!("repo: {CONFIRM_REPO}")));
        assert!(line.contains("autonomous: off"));
    }

    #[test]
    fn header_line_carries_no_source_segment_when_every_source_is_observed() {
        // A healthy cycle never grows a phantom source segment, at any width.
        let model = build_tui_model(&[], 0);
        for width in [40_usize, 80, 110, 300] {
            let line = model.header_line(width);
            assert!(!line.contains("unavailable"));
            assert!(!line.contains("sources:"));
        }
        assert!(model.header_line(300).contains("repo: -"));
        assert!(model.header_line(300).contains("autonomous: off"));
    }

    #[test]
    fn header_line_names_the_single_unavailable_source_without_a_more_marker() {
        // A single unavailable source has no name to elide, so there is no
        // `+N more` abbreviation tier: the header shows the one name, then only
        // the bare count degrades under width pressure.
        let model = blind_model("-", &["orchestrator"]);
        let wide = model.header_line(300);
        assert!(wide.contains("sources: 1 unavailable (orchestrator)"));
        assert!(!wide.contains("more"));
        // Under width pressure the lone-name form collapses straight to the count.
        let narrow = model.header_line(40);
        assert!(narrow.contains("sources: 1 unavailable"));
        assert!(!narrow.contains("(orchestrator)"));
    }

    #[test]
    fn footer_presents_the_dangerous_autonomous_toggle_shortcut() {
        let model = autonomous_model(TuiOverlay::None, CONFIRM_REPO, false);
        assert!(
            model
                .footer()
                .contains("a autonomous-mode (dangerous / use with caution)")
        );
    }

    #[test]
    fn interaction_state_carries_selected_repo_and_mode_through_the_reducer() {
        let state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_selected_repo(CONFIRM_REPO.to_owned())
            .with_autonomous_mode_enabled(true);
        assert_eq!(state.selected_repo(), CONFIRM_REPO);
        assert!(state.autonomous_mode_enabled());

        // A view-navigation interaction must preserve the ambient repo + mode.
        let next = reduce_tui_interaction(&state, &[], TuiInteraction::SelectNextView);
        assert_eq!(next.selected_repo(), CONFIRM_REPO);
        assert!(next.autonomous_mode_enabled());
    }

    #[test]
    fn autonomous_confirm_overlay_exposes_only_its_typed_text() {
        let confirm = TuiOverlay::AutonomousModeConfirm {
            typed: "abc".to_owned(),
        };
        assert_eq!(confirm.autonomous_confirm_typed(), Some("abc"));
        assert_eq!(confirm.query(), None);
        assert_eq!(confirm.selected_action_index(), None);
        assert!(confirm.is_open());
        // Other overlays carry no confirm text.
        assert_eq!(TuiOverlay::None.autonomous_confirm_typed(), None);
    }

    #[test]
    fn reducer_opens_the_autonomous_mode_confirm_modal_empty() {
        let state = TuiInteractionState::new(0, TuiOverlay::None);
        let opened = reduce_tui_interaction(&state, &[], TuiInteraction::OpenAutonomousModeConfirm);
        assert_eq!(
            opened.overlay(),
            &TuiOverlay::AutonomousModeConfirm {
                typed: String::new(),
            }
        );
    }

    #[test]
    fn autonomous_confirm_modal_accepts_typed_characters_and_backspace() {
        let empty = TuiInteractionState::new(
            0,
            TuiOverlay::AutonomousModeConfirm {
                typed: String::new(),
            },
        );
        let typed_a = reduce_tui_interaction(&empty, &[], TuiInteraction::TypeChar('a'));
        assert_eq!(
            typed_a.overlay(),
            &TuiOverlay::AutonomousModeConfirm {
                typed: "a".to_owned(),
            }
        );
        let typed_ab = reduce_tui_interaction(&typed_a, &[], TuiInteraction::TypeChar('b'));
        let backspaced = reduce_tui_interaction(&typed_ab, &[], TuiInteraction::Backspace);
        assert_eq!(
            backspaced.overlay(),
            &TuiOverlay::AutonomousModeConfirm {
                typed: "a".to_owned(),
            }
        );
        // Backspacing an already-empty confirm buffer stays empty.
        let still_empty = reduce_tui_interaction(&empty, &[], TuiInteraction::Backspace);
        assert_eq!(
            still_empty.overlay(),
            &TuiOverlay::AutonomousModeConfirm {
                typed: String::new(),
            }
        );
    }

    #[test]
    fn autonomous_confirm_overlay_is_inert_for_action_and_search_helpers() {
        // The confirm modal is normalized/searched/action-navigated as a no-op:
        // build a model over it (search_query + normalize arms) and move the
        // action selection up/down (move_action arms) -- the overlay is
        // preserved unchanged.
        let confirm = TuiOverlay::AutonomousModeConfirm {
            typed: "x".to_owned(),
        };
        let model = autonomous_model(confirm.clone(), CONFIRM_REPO, false);
        assert_eq!(model.overlay(), &confirm);

        let state = TuiInteractionState::new(0, confirm.clone());
        let down = reduce_tui_interaction(&state, &[], TuiInteraction::SelectNextAction);
        assert_eq!(down.overlay(), &confirm);
        let up = reduce_tui_interaction(&state, &[], TuiInteraction::SelectPreviousAction);
        assert_eq!(up.overlay(), &confirm);
    }

    #[test]
    fn autonomous_mode_confirmation_matches_requires_the_exact_repo() {
        assert!(autonomous_mode_confirmation_matches(
            CONFIRM_REPO,
            CONFIRM_REPO
        ));
        assert!(autonomous_mode_confirmation_matches(
            &format!("  {CONFIRM_REPO}  "),
            CONFIRM_REPO
        ));
        assert!(!autonomous_mode_confirmation_matches("nope", CONFIRM_REPO));
        assert!(!autonomous_mode_confirmation_matches("", ""));
    }

    #[test]
    fn enabling_submits_a_confirmed_command_only_when_the_typed_phrase_matches() {
        let overlay = TuiOverlay::AutonomousModeConfirm {
            typed: CONFIRM_REPO.to_owned(),
        };
        let model = autonomous_model(overlay, CONFIRM_REPO, false);
        let outcome = resolve_autonomous_mode_enable(&model, "operator");
        assert!(matches!(
            &outcome,
            Ok(OperatorActionOutcome::PersistCommandWithPayload { command, payload_json })
                if command.command_type() == &CommandType::ConfigAutonomousModeSet
                    && command.aggregate_id() == CONFIRM_REPO
                    && payload_json.contains(r#""repo":"livespec-console-beads-fabro""#)
                    && payload_json.contains(r#""enabled":true"#)
                    && payload_json.contains(r#""confirmed":true"#)
        ));
    }

    #[test]
    fn enabling_rejects_a_mismatched_confirmation_without_submitting() {
        let overlay = TuiOverlay::AutonomousModeConfirm {
            typed: "wrong-repo".to_owned(),
        };
        let model = autonomous_model(overlay, CONFIRM_REPO, false);
        assert_eq!(
            resolve_autonomous_mode_enable(&model, "operator"),
            Err(ApplicationError::AutonomousModeConfirmationMismatch)
        );
    }

    #[test]
    fn enabling_requires_the_confirm_overlay_to_be_open() {
        let model = autonomous_model(TuiOverlay::None, CONFIRM_REPO, false);
        assert_eq!(
            resolve_autonomous_mode_enable(&model, "operator"),
            Err(ApplicationError::NoSelectedOperatorAction)
        );
    }

    #[test]
    fn disabling_submits_an_unconfirmed_command_with_no_modal() {
        let model = autonomous_model(TuiOverlay::None, CONFIRM_REPO, true);
        let outcome = resolve_autonomous_mode_disable(&model, "operator");
        assert!(matches!(
            &outcome,
            Ok(OperatorActionOutcome::PersistCommandWithPayload { command, payload_json })
                if command.aggregate_id() == CONFIRM_REPO
                    && payload_json.contains(r#""enabled":false"#)
                    && payload_json.contains(r#""confirmed":false"#)
        ));
    }

    #[test]
    fn disabling_requires_a_selected_repo() {
        let model = autonomous_model(TuiOverlay::None, "", true);
        assert_eq!(
            resolve_autonomous_mode_disable(&model, "operator"),
            Err(ApplicationError::InvalidAutonomousModePayload)
        );
    }

    #[test]
    fn persist_with_payload_outcome_exposes_command_and_no_attach() {
        let outcome = OperatorActionOutcome::PersistCommandWithPayload {
            command: CommandEnvelope::new(
                "cmd".to_owned(),
                CommandType::ConfigAutonomousModeSet,
                CONFIRM_REPO.to_owned(),
                "key".to_owned(),
                "operator".to_owned(),
            ),
            payload_json: "{}".to_owned(),
        };
        assert!(outcome.command().is_some());
        assert_eq!(outcome.attach_command(), None);
    }

    // -----------------------------------------------------------------------
    // Operator valve keys (S4b): the five human-valve/policy-edit commands
    // staged in the valve-confirm modal against the selected work-item, each
    // riding the shared orchestrator action port (Scenario 11).
    // -----------------------------------------------------------------------

    /// A model over the fabro-gate events with the given valve staged in the
    /// valve-confirm modal against the selected (index 0 -> `console-pending`)
    /// work-item.
    fn valve_model(valve: PendingValve) -> TuiScreenModel {
        build_tui_model_for_state(
            &fabro_gate_events(),
            &TuiInteractionState::new(0, TuiOverlay::ValveConfirm { valve }),
        )
    }

    #[test]
    fn pending_valve_labels_options_and_destructiveness() {
        assert_eq!(PendingValve::Approve.valve_label(), "Approve");
        assert_eq!(PendingValve::Accept.valve_label(), "Accept");
        assert_eq!(
            PendingValve::Reject(RejectMode::Rework).valve_label(),
            "Reject"
        );
        assert_eq!(
            PendingValve::SetAdmission(AdmissionPolicy::Manual).valve_label(),
            "Set admission"
        );
        assert_eq!(
            PendingValve::SetAcceptance(AcceptancePolicy::AiThenHuman).valve_label(),
            "Set acceptance"
        );

        assert_eq!(PendingValve::Approve.option_label(), None);
        assert_eq!(PendingValve::Accept.option_label(), None);
        assert_eq!(
            PendingValve::Reject(RejectMode::Regroom).option_label(),
            Some("regroom")
        );
        assert_eq!(
            PendingValve::SetAdmission(AdmissionPolicy::Auto).option_label(),
            Some("auto")
        );
        assert_eq!(
            PendingValve::SetAcceptance(AcceptancePolicy::HumanOnly).option_label(),
            Some("human-only")
        );

        assert!(PendingValve::Reject(RejectMode::Rework).is_destructive());
        assert!(!PendingValve::Approve.is_destructive());
        assert!(!PendingValve::SetAdmission(AdmissionPolicy::Auto).is_destructive());
    }

    #[test]
    fn pending_valve_cycles_payload_options_and_leaves_payloadless_valves() {
        // Approve/accept carry no payload, so cycling is a no-op both ways.
        assert_eq!(PendingValve::Approve.cycled(true), PendingValve::Approve);
        assert_eq!(PendingValve::Accept.cycled(false), PendingValve::Accept);

        // Reject wraps rework <-> regroom (two states, so either direction flips).
        assert_eq!(
            PendingValve::Reject(RejectMode::Rework).cycled(true),
            PendingValve::Reject(RejectMode::Regroom)
        );
        assert_eq!(
            PendingValve::Reject(RejectMode::Regroom).cycled(false),
            PendingValve::Reject(RejectMode::Rework)
        );

        // Admission wraps manual <-> auto.
        assert_eq!(
            PendingValve::SetAdmission(AdmissionPolicy::Manual).cycled(true),
            PendingValve::SetAdmission(AdmissionPolicy::Auto)
        );
        assert_eq!(
            PendingValve::SetAdmission(AdmissionPolicy::Auto).cycled(false),
            PendingValve::SetAdmission(AdmissionPolicy::Manual)
        );

        // Acceptance has three states; forward and backward wrap differently.
        assert_eq!(
            PendingValve::SetAcceptance(AcceptancePolicy::AiThenHuman).cycled(true),
            PendingValve::SetAcceptance(AcceptancePolicy::AiOnly)
        );
        assert_eq!(
            PendingValve::SetAcceptance(AcceptancePolicy::AiThenHuman).cycled(false),
            PendingValve::SetAcceptance(AcceptancePolicy::HumanOnly)
        );
    }

    #[test]
    fn valve_confirm_accessor_returns_the_staged_valve_or_none() {
        assert_eq!(
            TuiOverlay::ValveConfirm {
                valve: PendingValve::Approve,
            }
            .valve_confirm(),
            Some(PendingValve::Approve)
        );
        assert_eq!(TuiOverlay::None.valve_confirm(), None);
    }

    #[test]
    fn reduce_opens_and_cycles_the_valve_confirm_overlay() {
        let events = fabro_gate_events();
        let opened = reduce_tui_interaction(
            &TuiInteractionState::new(0, TuiOverlay::None),
            &events,
            TuiInteraction::OpenValveConfirm(PendingValve::SetAcceptance(
                AcceptancePolicy::AiThenHuman,
            )),
        );
        assert_eq!(
            opened.overlay(),
            &TuiOverlay::ValveConfirm {
                valve: PendingValve::SetAcceptance(AcceptancePolicy::AiThenHuman),
            }
        );

        let cycled =
            reduce_tui_interaction(&opened, &events, TuiInteraction::CycleValveOption(true));
        assert_eq!(
            cycled.overlay(),
            &TuiOverlay::ValveConfirm {
                valve: PendingValve::SetAcceptance(AcceptancePolicy::AiOnly),
            }
        );

        // Cycling with no valve-confirm overlay open leaves the overlay unchanged.
        let noop = reduce_tui_interaction(
            &TuiInteractionState::new(0, TuiOverlay::None),
            &events,
            TuiInteraction::CycleValveOption(true),
        );
        assert_eq!(noop.overlay(), &TuiOverlay::None);
    }

    #[test]
    fn resolve_valve_action_persists_payloadless_approve_and_accept() {
        for (valve, command_type, action) in [
            (
                PendingValve::Approve,
                CommandType::WorkItemApproveRequested,
                "approve",
            ),
            (
                PendingValve::Accept,
                CommandType::WorkItemAcceptRequested,
                "accept",
            ),
        ] {
            let model = valve_model(valve);
            let outcome = resolve_valve_action(&model, "operator");
            let command = outcome
                .as_ref()
                .ok()
                .and_then(OperatorActionOutcome::command);
            assert_eq!(
                command.map(CommandEnvelope::command_type),
                Some(&command_type)
            );
            assert_eq!(
                command.map(CommandEnvelope::aggregate_id),
                Some("console-pending")
            );
            assert_eq!(
                command.map(CommandEnvelope::idempotency_key),
                Some(format!("console-pending:work_item.{action}_requested").as_str())
            );
            assert_eq!(command.map(CommandEnvelope::requested_by), Some("operator"));
            // Payloadless: a plain PersistCommand, never PersistCommandWithPayload.
            assert!(matches!(
                outcome,
                Ok(OperatorActionOutcome::PersistCommand(_))
            ));
        }
    }

    #[test]
    fn resolve_valve_action_reject_persists_the_mode_payload() {
        let outcome = resolve_valve_action(
            &valve_model(PendingValve::Reject(RejectMode::Regroom)),
            "operator",
        );
        assert!(matches!(
            &outcome,
            Ok(OperatorActionOutcome::PersistCommandWithPayload { command, payload_json })
                if command.command_type() == &CommandType::WorkItemRejectRequested
                    && command.aggregate_id() == "console-pending"
                    && command.idempotency_key()
                        == "console-pending:work_item.reject_requested:mode=regroom"
                    && payload_json == r#"{"mode":"regroom"}"#
        ));
    }

    #[test]
    fn resolve_valve_action_set_admission_persists_the_policy_payload() {
        let outcome = resolve_valve_action(
            &valve_model(PendingValve::SetAdmission(AdmissionPolicy::Auto)),
            "operator",
        );
        assert!(matches!(
            &outcome,
            Ok(OperatorActionOutcome::PersistCommandWithPayload { command, payload_json })
                if command.command_type() == &CommandType::WorkItemSetAdmissionRequested
                    && command.aggregate_id() == "console-pending"
                    && command.idempotency_key()
                        == "console-pending:work_item.set_admission_requested:policy=auto"
                    && payload_json == r#"{"policy":"auto"}"#
        ));
    }

    #[test]
    fn resolve_valve_action_set_acceptance_persists_the_policy_payload() {
        let outcome = resolve_valve_action(
            &valve_model(PendingValve::SetAcceptance(AcceptancePolicy::HumanOnly)),
            "operator",
        );
        assert!(matches!(
            &outcome,
            Ok(OperatorActionOutcome::PersistCommandWithPayload { command, payload_json })
                if command.command_type() == &CommandType::WorkItemSetAcceptanceRequested
                    && command.aggregate_id() == "console-pending"
                    && command.idempotency_key()
                        == "console-pending:work_item.set_acceptance_requested:policy=human-only"
                    && payload_json == r#"{"policy":"human-only"}"#
        ));
    }

    #[test]
    fn resolve_valve_action_surfaces_its_error_paths() {
        // Blank requester.
        assert_eq!(
            resolve_valve_action(&valve_model(PendingValve::Approve), " "),
            Err(ApplicationError::EmptyOperatorAction)
        );
        // The overlay is not the valve-confirm modal.
        assert_eq!(
            resolve_valve_action(&build_tui_model(&fabro_gate_events(), 0), "operator"),
            Err(ApplicationError::NoSelectedOperatorAction)
        );
        // No attention item is selected (empty inbox).
        let empty = build_tui_model_for_state(
            &[],
            &TuiInteractionState::new(
                0,
                TuiOverlay::ValveConfirm {
                    valve: PendingValve::Approve,
                },
            ),
        );
        assert_eq!(
            resolve_valve_action(&empty, "operator"),
            Err(ApplicationError::NoSelectedAttentionItem)
        );
    }
}
