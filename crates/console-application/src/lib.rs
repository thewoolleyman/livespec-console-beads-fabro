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
    SourceProbe, SourceProbeOutcome, WorkItemDetail, WorkItemSnapshot,
    attention_item_snapshot_from_payload_json, materialize_attention_items,
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
    /// Settings variant -- the dispatcher-settings surface.
    Settings,
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
            Self::Settings,
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
            Self::Settings => "Settings",
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
/// The cockpit body is three side-by-side panes — the left **Views** navigation
/// menu, the middle **Content** pane (the active view's list), and the right
/// **Detail** pane (the selected item's details) — above which sits the
/// **Header** pane (the top status line). `left`/`right` walk focus spatially
/// between the body panes, clamped at the ends (`right` stops on Detail, `left`
/// stops on Nav); `up`/`down` act WITHIN the focused pane — moving the Views
/// selection, the Content selection, or scrolling the Detail pane. `Tab`/`BackTab`
/// cycle focus across EVERY pane including the Header. Focus starts on the Views
/// nav so `up`/`down` walk the vertical Views menu intuitively. The `Lanes` view
/// has no Detail pane, so `right` clamps at Content there and the focus cycle
/// skips the Detail pane. While the Header holds focus, `left`/`right` scroll it
/// horizontally rather than walking the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPane {
    /// The left Views navigation menu (the default focus).
    Nav,
    /// The active view's content pane (its list of items or lanes).
    Content,
    /// The right Detail pane (the selected item's details; scrollable).
    Detail,
    /// The top Header pane (the status line; focusable and horizontally
    /// scrollable so content clipped on a narrow viewport is reachable).
    Header,
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
    /// The move-to-status valve: move the selected work-item from its current
    /// lane (`from`) to an operator-drivable target lane (`to`), cycling `to`
    /// through the targets the operator may drive from `from`. It maps onto the
    /// orchestrator's real transition actions (approve / accept / resolve-blocked)
    /// -- never an invented status jump.
    MoveStatus {
        /// The selected work-item's current lane (its source state).
        from: Lane,
        /// The dialed-in target lane, cycled among the operator-drivable targets.
        to: Lane,
    },
    /// The per-item override valve: set or clear ONE of the three overridable cap
    /// settings (`merge_on_review_cap` / `review_fix_cap` / `acceptance_rework_cap`)
    /// on the selected work-item, cycling the dialed-in value including a
    /// `clear`-to-inherit-global option. It maps onto the orchestrator's per-cap
    /// override actions -- never a console-side ledger write.
    SetOverride(DispatcherOverride),
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
            Self::MoveStatus { .. } => "Move status",
            Self::SetOverride(_dial) => "Set override",
        }
    }

    #[must_use]
    /// The dialed-in mode/policy/target label for a payload valve, or `None` for
    /// the payload-free approve/accept valves. The per-item override valve renders
    /// a dynamic value string, so it returns `None` here and is handled by
    /// [`Self::option_display`].
    pub const fn option_label(&self) -> Option<&'static str> {
        match self {
            Self::Approve | Self::Accept | Self::SetOverride(_) => None,
            Self::Reject(mode) => Some(mode.as_str()),
            Self::SetAdmission(policy) => Some(policy.label()),
            Self::SetAcceptance(policy) => Some(policy.label()),
            Self::MoveStatus { to, .. } => Some(to.label()),
        }
    }

    #[must_use]
    /// The dialed-in option as an owned display string, for every payload valve
    /// (including the per-item override, whose value is dynamic and so has no
    /// `'static` label). `None` for the payload-free approve/accept valves.
    pub fn option_display(&self) -> Option<String> {
        match self {
            Self::SetOverride(dial) => Some(dial.option_display()),
            _other => self.option_label().map(str::to_owned),
        }
    }

    #[must_use]
    /// Whether this valve is destructive, so the confirm modal warns before it
    /// is submitted. Only reject is destructive.
    pub const fn is_destructive(&self) -> bool {
        matches!(self, Self::Reject(_mode))
    }

    #[must_use]
    /// This valve with its mode/policy/target/value rotated one step (forward or
    /// backward). The payload-free approve/accept valves are returned unchanged.
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
            Self::MoveStatus { from, to } => Self::MoveStatus {
                from,
                to: rotate(status_move_targets(from), to, forward),
            },
            Self::SetOverride(dial) => Self::SetOverride(dial.cycled(forward)),
        }
    }
}

/// One of the three per-item override valves, paired with its dialed-in value.
///
/// Each maps onto the orchestrator's named per-cap override action; a `Clear`
/// value clears the per-item label back to inherit-global.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatcherOverride {
    /// The `merge_on_review_cap` boolean override.
    MergeOnReviewCap(OverrideBool),
    /// The `review_fix_cap` positive-integer override.
    ReviewFixCap(OverrideInt),
    /// The `acceptance_rework_cap` positive-integer override.
    AcceptanceReworkCap(OverrideInt),
}

/// The dialed-in value of a boolean per-item override: on, off, or cleared
/// (inherit the global default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideBool {
    /// Override the item to `true`.
    On,
    /// Override the item to `false`.
    Off,
    /// Clear the per-item override, inheriting the global default.
    Clear,
}

impl OverrideBool {
    /// This value cycled one step (forward `On -> Off -> Clear -> On`).
    #[must_use]
    const fn cycled(self, forward: bool) -> Self {
        match (self, forward) {
            (Self::On, true) | (Self::Clear, false) => Self::Off,
            (Self::Off, true) | (Self::On, false) => Self::Clear,
            (Self::Clear, true) | (Self::Off, false) => Self::On,
        }
    }
}

/// The dialed-in value of a positive-integer per-item override: a value in
/// `1..=INT_OVERRIDE_MAX`, or cleared (inherit the global default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideInt {
    /// Override the item to this positive integer.
    Value(u32),
    /// Clear the per-item override, inheriting the global default.
    Clear,
}

/// The largest per-item integer override the dial proposes; forward past it wraps
/// back to `Clear`. The console owns no cap policy -- the orchestrator is the
/// authority on legality -- so this is only the operator-facing dial range.
const INT_OVERRIDE_MAX: u32 = 9;

impl OverrideInt {
    /// This value cycled one step. Forward walks `Clear -> 1 -> 2 -> ... ->
    /// INT_OVERRIDE_MAX -> Clear`; backward reverses. Values stay positive
    /// (`>= 1`), matching the orchestrator's positive-int contract.
    #[must_use]
    const fn cycled(self, forward: bool) -> Self {
        match (self, forward) {
            (Self::Clear, true) => Self::Value(1),
            (Self::Clear, false) => Self::Value(INT_OVERRIDE_MAX),
            (Self::Value(value), true) if value >= INT_OVERRIDE_MAX => Self::Clear,
            (Self::Value(value), true) => Self::Value(value + 1),
            (Self::Value(value), false) if value <= 1 => Self::Clear,
            (Self::Value(value), false) => Self::Value(value - 1),
        }
    }
}

impl DispatcherOverride {
    /// The orchestrator `dispatcher.*` key this override targets.
    #[must_use]
    pub const fn setting_key(&self) -> &'static str {
        match self {
            Self::MergeOnReviewCap(_value) => "merge_on_review_cap",
            Self::ReviewFixCap(_value) => "review_fix_cap",
            Self::AcceptanceReworkCap(_value) => "acceptance_rework_cap",
        }
    }

    /// The orchestrator `drive` action verb this override rides.
    #[must_use]
    pub const fn action_verb(&self) -> &'static str {
        match self {
            Self::MergeOnReviewCap(_value) => "set-merge-on-review-cap",
            Self::ReviewFixCap(_value) => "set-review-fix-cap",
            Self::AcceptanceReworkCap(_value) => "set-acceptance-rework-cap",
        }
    }

    /// The dialed-in value as the action-id's trailing segment: `true`/`false`
    /// for a bool, the decimal digits for an int, and `clear` for either's clear.
    #[must_use]
    pub fn value_literal(&self) -> String {
        match self {
            Self::MergeOnReviewCap(OverrideBool::On) => "true".to_owned(),
            Self::MergeOnReviewCap(OverrideBool::Off) => "false".to_owned(),
            Self::MergeOnReviewCap(OverrideBool::Clear)
            | Self::ReviewFixCap(OverrideInt::Clear)
            | Self::AcceptanceReworkCap(OverrideInt::Clear) => "clear".to_owned(),
            Self::ReviewFixCap(OverrideInt::Value(value))
            | Self::AcceptanceReworkCap(OverrideInt::Value(value)) => value.to_string(),
        }
    }

    /// The dialed-in value as the `{ setting, value }` payload's `value` field: a
    /// JSON bool, a JSON number, or JSON `null` for a clear.
    #[must_use]
    pub fn payload_value(&self) -> serde_json::Value {
        match self {
            Self::MergeOnReviewCap(OverrideBool::On) => serde_json::Value::Bool(true),
            Self::MergeOnReviewCap(OverrideBool::Off) => serde_json::Value::Bool(false),
            Self::MergeOnReviewCap(OverrideBool::Clear)
            | Self::ReviewFixCap(OverrideInt::Clear)
            | Self::AcceptanceReworkCap(OverrideInt::Clear) => serde_json::Value::Null,
            Self::ReviewFixCap(OverrideInt::Value(value))
            | Self::AcceptanceReworkCap(OverrideInt::Value(value)) => {
                serde_json::Value::Number((*value).into())
            }
        }
    }

    /// The operator-facing `key = value` string the confirm modal renders (with
    /// `on`/`off`/`clear` for a bool and the number or `clear` for an int).
    #[must_use]
    pub fn option_display(&self) -> String {
        let value = match self {
            Self::MergeOnReviewCap(OverrideBool::On) => "on".to_owned(),
            Self::MergeOnReviewCap(OverrideBool::Off) => "off".to_owned(),
            Self::MergeOnReviewCap(OverrideBool::Clear)
            | Self::ReviewFixCap(OverrideInt::Clear)
            | Self::AcceptanceReworkCap(OverrideInt::Clear) => "clear".to_owned(),
            Self::ReviewFixCap(OverrideInt::Value(value))
            | Self::AcceptanceReworkCap(OverrideInt::Value(value)) => value.to_string(),
        };
        format!("{} = {value}", self.setting_key())
    }

    /// This override with its value cycled one step (forward or backward).
    #[must_use]
    pub const fn cycled(self, forward: bool) -> Self {
        match self {
            Self::MergeOnReviewCap(value) => Self::MergeOnReviewCap(value.cycled(forward)),
            Self::ReviewFixCap(value) => Self::ReviewFixCap(value.cycled(forward)),
            Self::AcceptanceReworkCap(value) => Self::AcceptanceReworkCap(value.cycled(forward)),
        }
    }
}

/// The operator-drivable target lanes an item may be moved to from `from`, each
/// mapping to a real orchestrator action ([`move_status_outcome`]).
///
/// Every pre-terminal pipeline status is reachable via the guarded broad
/// `move:<id>:<target>` action -- `backlog`, `ready`, `active`, `blocked` (minus
/// `from` itself) -- with a SEMANTIC valve preferred where one exists:
/// `pending-approval -> ready` uses approve, and `blocked -> ready | backlog`
/// uses resolve-blocked. `done` is reachable ONLY from `acceptance` via accept
/// (the ship-guard: no broad move ever targets `done`), and a `done` item offers
/// no onward move at all (the picker never surfaces un-shipping). The corrective
/// `acceptance -> active | backlog` reject routes are NOT offered here -- reject
/// stays on its own `r` valve, since it reverts a merged change and is warned as
/// destructive. A lane with no operator-drivable target returns an empty slice,
/// so the move-status valve never opens on it.
const fn status_move_targets(from: Lane) -> &'static [Lane] {
    match from {
        Lane::Backlog => &[Lane::Ready, Lane::Active, Lane::Blocked],
        Lane::PendingApproval => &[Lane::Backlog, Lane::Ready, Lane::Active, Lane::Blocked],
        Lane::Ready => &[Lane::Backlog, Lane::Active, Lane::Blocked],
        Lane::Active => &[Lane::Backlog, Lane::Ready, Lane::Blocked],
        Lane::Acceptance => &[
            Lane::Backlog,
            Lane::Ready,
            Lane::Active,
            Lane::Blocked,
            Lane::Done,
        ],
        Lane::Blocked => &[Lane::Backlog, Lane::Ready, Lane::Active],
        Lane::Done => &[],
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
    /// Help variant: the navigable, pane-specific modal help overlay opened
    /// with `?`. It carries the selected left-menu section index and the
    /// right-pane vertical scroll offset. It closes ONLY on `Esc` -- no other
    /// key, command, valve, or view-switch dismisses it (per the TUI Contract).
    Help {
        /// Index of the selected help section in the left menu. `0` is the
        /// `Global actions` section; `1..` map to `TuiView::all()` in order, so
        /// section `i` (for `i >= 1`) is `TuiView::all()[i - 1]`.
        selected_section: usize,
        /// The right-pane vertical scroll offset (the topmost visible wrapped
        /// row) for the selected section. Reset to `0` whenever the section
        /// changes, and clamped by the renderer to the section's wrapped height.
        scroll: usize,
    },
    /// Work-item detail variant: the near-full-screen modal showing the FULL
    /// standardized record of the selected work-item — its title, description,
    /// and the rest of the descriptive shape the lane row has no room for.
    ///
    /// It carries only its scroll offset, never the item itself: like
    /// [`ValveConfirm`](Self::ValveConfirm), the renderer reads the target from
    /// the SAME selection `Enter` opened it on, so the modal can never drift
    /// onto a different work-item than the one the operator picked.
    WorkItemDetail {
        /// The work-item the modal was opened on, PINNED at open time.
        ///
        /// The modal resolves its record by this id, never by the lane
        /// selection index: ingestion keeps appending while the modal is open,
        /// and a re-ranked or newly-inserted sibling would otherwise slide a
        /// DIFFERENT work-item under the same index and silently swap the
        /// record the operator is reading.
        work_item_id: String,
        /// Vertical scroll offset (the topmost visible wrapped row), clamped by
        /// the renderer so a long description scrolls without running past its
        /// last row.
        scroll: usize,
    },
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
            | Self::ValveConfirm { .. }
            | Self::WorkItemDetail { .. }
            | Self::Help { .. } => None,
        }
    }

    #[must_use]
    /// Return the scroll offset when the overlay is the work-item detail modal,
    /// or `None` for any other overlay.
    pub const fn work_item_detail_scroll(&self) -> Option<usize> {
        match self {
            Self::WorkItemDetail { scroll, .. } => Some(*scroll),
            Self::None
            | Self::Search { .. }
            | Self::CommandPalette { .. }
            | Self::CommandModal { .. }
            | Self::ValveConfirm { .. }
            | Self::Help { .. } => None,
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
            | Self::ValveConfirm { .. }
            | Self::WorkItemDetail { .. }
            | Self::Help { .. } => None,
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
            | Self::WorkItemDetail { .. }
            | Self::Help { .. } => None,
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
    /// Move focus from the Views nav to the Content pane (the `Enter`/`right`
    /// dive-in from the nav).
    FocusContent,
    /// Move focus from the Content pane back to the Views nav (the `Esc`/`left`
    /// step-out from the content list).
    FocusNav,
    /// Move focus from the Content pane to the right Detail pane (the `right`
    /// step-in from the content list, on a view that has a Detail pane).
    FocusDetail,
    /// Cycle focus to the NEXT pane in the ring (the `Tab` binding), wrapping
    /// Nav -> Content -> Detail -> Header -> Nav. The ring skips the Detail pane
    /// on a view that has none (`Lanes`), so it reads Nav -> Content -> Header there.
    FocusNextPane,
    /// Cycle focus to the PREVIOUS pane in the ring (the `BackTab`/`Shift-Tab`
    /// binding), the reverse of [`FocusNextPane`](Self::FocusNextPane).
    FocusPreviousPane,
    /// Scroll the focused Header pane one step to the RIGHT (the `right` key while
    /// the Header pane holds focus), revealing header content clipped off the
    /// right edge. Clamped to the render-measured maximum so it stops at the true
    /// right edge; inert once the whole header already fits.
    ScrollHeaderRight,
    /// Scroll the focused Header pane one step to the LEFT (the `left` key while
    /// the Header pane holds focus), back toward its left-justified default.
    /// Saturates at the left edge (offset `0`).
    ScrollHeaderLeft,
    /// Scroll the focused Detail pane's content down one line (the `down` key
    /// while the Detail pane holds focus), revealing content clipped below.
    ScrollDetailDown,
    /// Scroll the focused Detail pane's content up one line (the `up` key while
    /// the Detail pane holds focus).
    ScrollDetailUp,
    /// Open the navigable, pane-specific modal Help overlay (the `?` binding),
    /// auto-focused to the section for the currently active pane/view.
    OpenHelp,
    /// Move the modal Help left-menu selection to the NEXT section (down),
    /// clamped at the last section, resetting the right-pane scroll. Inert
    /// unless the Help overlay is open.
    HelpSelectNextSection,
    /// Move the modal Help left-menu selection to the PREVIOUS section (up),
    /// clamped at the first section, resetting the right-pane scroll. Inert
    /// unless the Help overlay is open.
    HelpSelectPreviousSection,
    /// Scroll the modal Help right-hand text pane DOWN one row. Inert unless the
    /// Help overlay is open; the renderer clamps the offset to the section
    /// height, so the scroll never runs past the last wrapped row.
    HelpScrollDown,
    /// Scroll the modal Help right-hand text pane UP one row. Inert unless the
    /// Help overlay is open.
    HelpScrollUp,
    /// Open the work-item detail modal on the currently selected work-item,
    /// showing its full standardized record. Opens at the top of the record.
    OpenWorkItemDetail,
    /// Scroll the work-item detail modal DOWN by the given number of rows (`1`
    /// for a line step, a page height for `PgDn`). Inert unless that modal is
    /// open; the renderer clamps the offset to the record's wrapped height, so
    /// the scroll never runs past the last row.
    WorkItemDetailScrollDown(usize),
    /// Scroll the work-item detail modal UP by the given number of rows,
    /// saturating at the top. Inert unless that modal is open.
    WorkItemDetailScrollUp(usize),
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
    selected_lane_item_index: usize,
    focus: FocusPane,
    detail_scroll: usize,
    detail_max_scroll: usize,
    header_scroll: usize,
    header_max_scroll: usize,
    overlay: TuiOverlay,
    selected_repo: String,
    selected_setting_index: usize,
    dispatcher_settings: DispatcherSettingsRead,
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
            selected_lane_item_index: 0,
            focus: FocusPane::Nav,
            detail_scroll: 0,
            detail_max_scroll: 0,
            header_scroll: 0,
            header_max_scroll: 0,
            overlay,
            selected_repo: String::new(),
            selected_setting_index: 0,
            dispatcher_settings: DispatcherSettingsRead::NotObserved,
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
            selected_lane_item_index: 0,
            focus: FocusPane::Nav,
            detail_scroll: 0,
            detail_max_scroll: 0,
            header_scroll: 0,
            header_max_scroll: 0,
            overlay,
            selected_repo: String::new(),
            selected_setting_index: 0,
            dispatcher_settings: DispatcherSettingsRead::NotObserved,
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
    ///
    /// This is the single seam every focus change flows through, so it also
    /// resets the Header pane's horizontal scroll to its left-justified default
    /// whenever focus moves to a NON-Header pane (blur). That keeps the
    /// snap-back-on-blur invariant centralized: a focus change back to the
    /// Header always starts at offset `0`, and blur never leaves the header
    /// stuck mid-scroll (per Scenario 20 / the TUI Contract).
    #[must_use]
    pub const fn with_focus(mut self, focus: FocusPane) -> Self {
        self.focus = focus;
        if !matches!(focus, FocusPane::Header) {
            self.header_scroll = 0;
        }
        self
    }

    /// Replace the Detail pane's scroll offset (the topmost visible detail line),
    /// preserving every other field. Reset to `0` whenever the selection or view
    /// changes so a scroll never carries onto a different item's details.
    #[must_use]
    pub const fn with_detail_scroll(mut self, detail_scroll: usize) -> Self {
        self.detail_scroll = detail_scroll;
        self
    }

    /// Replace the Detail pane's maximum scroll offset — the largest topmost-row
    /// offset at which the pane's LAST wrapped row is still visible — preserving
    /// every other field. The renderer measures it from the wrapped line count at
    /// the pane's inner width (`Paragraph::line_count`, the same count that sizes
    /// the scrollbar) and the interactive loop feeds it back each frame, so the
    /// scroll-down clamp reaches the true wrapped bottom rather than a
    /// width-agnostic logical line count that under-counts wrapped rows.
    #[must_use]
    pub const fn with_detail_max_scroll(mut self, detail_max_scroll: usize) -> Self {
        self.detail_max_scroll = detail_max_scroll;
        self
    }

    /// Replace the Header pane's horizontal scroll offset (the leftmost visible
    /// header column while the Header pane holds focus), preserving every other
    /// field. Reset to `0` on blur (see [`with_focus`](Self::with_focus)).
    #[must_use]
    pub const fn with_header_scroll(mut self, header_scroll: usize) -> Self {
        self.header_scroll = header_scroll;
        self
    }

    /// Replace the Header pane's maximum horizontal scroll offset — the largest
    /// leftmost-column offset at which the header line's last column is still
    /// visible — preserving every other field. The renderer measures it from the
    /// full header width minus the pane's inner width and the interactive loop
    /// feeds it back each frame, mirroring `with_detail_max_scroll`, so the
    /// scroll-right clamp reaches the true right edge at the current viewport
    /// width rather than a width-agnostic guess.
    #[must_use]
    pub const fn with_header_max_scroll(mut self, header_max_scroll: usize) -> Self {
        self.header_max_scroll = header_max_scroll;
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

    /// Replace the selected work-item row within a drilled-in lane (the
    /// per-item cursor the `Lanes` drill-in moves with up/down), preserving every
    /// other field.
    #[must_use]
    pub const fn with_selected_lane_item_index(mut self, selected_lane_item_index: usize) -> Self {
        self.selected_lane_item_index = selected_lane_item_index;
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
    /// sets the repo the operator's writes target -- the Settings-view setting
    /// edits and the valve confirmations.
    pub fn with_selected_repo(mut self, selected_repo: String) -> Self {
        self.selected_repo = selected_repo;
        self
    }

    #[must_use]
    /// Return this value with the selected setting row (the Settings view's
    /// content selection) replaced.
    pub const fn with_selected_setting_index(mut self, selected_setting_index: usize) -> Self {
        self.selected_setting_index = selected_setting_index;
        self
    }

    #[must_use]
    /// Return this value with the observed dispatcher settings replaced. The
    /// composition root reads them once from the orchestrator's published read
    /// surface; the console holds no setting state of its own and only renders
    /// what it observed (an unreadable surface stays `NotObserved`).
    pub const fn with_dispatcher_settings(
        mut self,
        dispatcher_settings: DispatcherSettingsRead,
    ) -> Self {
        self.dispatcher_settings = dispatcher_settings;
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
    /// Return the Detail pane's scroll offset (the topmost visible detail line).
    pub const fn detail_scroll(&self) -> usize {
        self.detail_scroll
    }

    #[must_use]
    /// Return the Detail pane's maximum scroll offset as measured by the last
    /// render (see `with_detail_max_scroll`). The scroll-down reducer clamps to
    /// this so the scroll range and the scrollbar are derived from the SAME
    /// wrapped line count.
    pub const fn detail_max_scroll(&self) -> usize {
        self.detail_max_scroll
    }

    #[must_use]
    /// Return the Header pane's horizontal scroll offset (the leftmost visible
    /// header column while the Header pane holds focus).
    pub const fn header_scroll(&self) -> usize {
        self.header_scroll
    }

    #[must_use]
    /// Return the Header pane's maximum horizontal scroll offset as measured by
    /// the last render (see [`with_header_max_scroll`](Self::with_header_max_scroll)).
    /// The scroll-right reducer clamps to this so the scroll range agrees with the
    /// header content actually clipped at the current viewport width.
    pub const fn header_max_scroll(&self) -> usize {
        self.header_max_scroll
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
    /// Return the selected work-item row within a drilled-in lane.
    pub const fn selected_lane_item_index(&self) -> usize {
        self.selected_lane_item_index
    }

    #[must_use]
    /// Return the stored value.
    pub const fn overlay(&self) -> &TuiOverlay {
        &self.overlay
    }

    #[must_use]
    /// Return the selected repo whose dispatcher settings the TUI presents.
    pub fn selected_repo(&self) -> &str {
        &self.selected_repo
    }

    #[must_use]
    /// Return the selected setting row (the Settings view's content selection).
    pub const fn selected_setting_index(&self) -> usize {
        self.selected_setting_index
    }

    #[must_use]
    /// Return the dispatcher settings the console observed for the selected repo.
    pub const fn dispatcher_settings(&self) -> &DispatcherSettingsRead {
        &self.dispatcher_settings
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
    selected_lane_item_index: Option<usize>,
    focus: FocusPane,
    detail_scroll: usize,
    header_scroll: usize,
    overlay: TuiOverlay,
    selected_repo: String,
    selected_setting_index: Option<usize>,
    dispatcher_settings: DispatcherSettingsRead,
    unavailable_sources: Vec<String>,
    header: String,
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

    /// The selected work-item row within a drilled-in lane, present only while
    /// the `Lanes` view is drilled into a lane that holds at least one item;
    /// `None` otherwise. This is the per-item cursor the operator moves with
    /// up/down to select an individual work-item.
    #[must_use]
    pub const fn selected_lane_item_index(&self) -> Option<usize> {
        self.selected_lane_item_index
    }

    #[must_use]
    /// The board's work-item with this id, in whatever lane currently holds it.
    ///
    /// Resolves by IDENTITY rather than by lane + selection index, so a surface
    /// that stays open across a source refresh (the work-item detail modal)
    /// keeps showing the item it was opened on even when ingestion re-ranks the
    /// lane, inserts a sibling above it, or moves the item to another lane.
    /// Returns `None` once the item leaves the board entirely, which the caller
    /// MUST surface rather than silently substituting a neighbour.
    pub fn work_item_by_id(&self, work_item_id: &str) -> Option<&LaneWorkItem> {
        self.lane_board
            .columns()
            .iter()
            .flat_map(LaneColumn::items)
            .find(|item| item.work_item_id() == work_item_id)
    }

    /// The selected work-item within a drilled-in lane, or `None` when the
    /// `Lanes` view is not drilled into a non-empty lane.
    #[must_use]
    pub fn selected_lane_item(&self) -> Option<&LaneWorkItem> {
        let LaneFocus::Lane(lane) = self.lane_focus else {
            return None;
        };
        let index = self.selected_lane_item_index?;
        self.lane_board.column(lane)?.items().get(index)
    }

    /// The work-item id the per-item valves act on: the selected drilled-in lane
    /// item's id in the `Lanes` view, the selected Attention item's work-item id
    /// in the `Attention` view, else `None`. This is what lets a per-item valve
    /// fire on an individually-selected lane item, not only on an Attention item;
    /// the other views carry no selectable work-item, so valves stay inert there.
    #[must_use]
    pub fn selected_work_item_id(&self) -> Option<&str> {
        match self.active_view {
            TuiView::Attention => self.detail.as_ref().map(AttentionDetail::work_item),
            TuiView::Lanes => self.selected_lane_item().map(LaneWorkItem::work_item_id),
            TuiView::Spec | TuiView::Events | TuiView::Repos | TuiView::Settings => None,
        }
    }

    /// The move-status valve the operator may open on the selected drilled-in
    /// lane item, staged at the first operator-drivable target for the item's
    /// current lane, or `None` when no lane item is selected or its lane has no
    /// operator-drivable target (so the move-status valve never opens on it).
    #[must_use]
    pub fn selected_move_status_valve(&self) -> Option<PendingValve> {
        let item = self.selected_lane_item()?;
        let to = status_move_targets(item.lane()).first().copied()?;
        Some(PendingValve::MoveStatus {
            from: item.lane(),
            to,
        })
    }

    /// Which pane the arrow keys currently drive (the Views nav, the Content
    /// pane, or the Detail pane). Renderers use it to mark the focused pane.
    #[must_use]
    pub const fn focus(&self) -> FocusPane {
        self.focus
    }

    /// The Detail pane's scroll offset (the topmost visible detail line). The
    /// renderer clamps it to what actually fits so an overscroll from a shrunk
    /// detail is harmless.
    #[must_use]
    pub const fn detail_scroll(&self) -> usize {
        self.detail_scroll
    }

    #[must_use]
    /// The Header pane's horizontal scroll offset (the leftmost visible header
    /// column) for the renderer to pan the focused header by. `0` (left-justified)
    /// whenever the Header pane is not focused.
    pub const fn header_scroll(&self) -> usize {
        self.header_scroll
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
    /// Return the selected repo whose dispatcher settings this model presents.
    pub fn selected_repo(&self) -> &str {
        &self.selected_repo
    }

    #[must_use]
    /// Return the dispatcher settings the console observed for the selected repo,
    /// rendered by the `Settings` view. The console holds no setting state of its
    /// own; an unreadable read surface stays `NotObserved`.
    pub const fn dispatcher_settings(&self) -> &DispatcherSettingsRead {
        &self.dispatcher_settings
    }

    #[must_use]
    /// Return the selected setting row in the `Settings` view, present only while
    /// that view is active; `None` otherwise.
    pub const fn selected_setting_index(&self) -> Option<usize> {
        self.selected_setting_index
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
    /// nav pane — then the `attention` count). The `repo` field is never dropped,
    /// and — while any source is unavailable — the source COUNT is never dropped,
    /// so the header always keeps the cockpit-blind-vs-idle tell. At a width wide
    /// enough for everything this returns the same content as [`header`](Self::header).
    pub fn header_line(&self, width: usize) -> String {
        fit_header_line(
            header_repo_label(&self.selected_repo),
            self.active_view.label(),
            self.attention_items.len(),
            &self.unavailable_sources,
            width,
        )
    }

    #[must_use]
    /// The Status-line shortcut hints for the CURRENT context (per the TUI
    /// Contract's Status-line-hints clause / Scenario 19).
    ///
    /// Derived on read from the currently-focused pane (`active_view`) and any
    /// open modal/overlay (`overlay`) rather than stored, so the hint line is
    /// never a single static string: it renders the keys that act in the
    /// current context, it changes when focus moves to a different pane, and an
    /// open overlay replaces the pane's hints with that overlay's (restored when
    /// the overlay closes). It is never empty, so no context in which shortcut
    /// actions are available shows a blank hint line. See [`footer_hint`].
    pub fn footer(&self) -> &str {
        // The Header pane is not view-keyed, so its focused hints come from
        // `focus` rather than `active_view`: while it holds focus (and no overlay
        // owns the line), the hints describe its horizontal-scroll keys. An open
        // overlay still owns the hint line first, so it is matched ahead of the
        // Header-focus branch.
        match (&self.overlay, self.focus) {
            (TuiOverlay::None, FocusPane::Header) => HEADER_FOOTER_HINT,
            (overlay, _) => footer_hint(
                self.active_view,
                self.lane_focus,
                self.selected_work_item_id().is_some(),
                overlay,
            ),
        }
    }
}

/// The Status-line shortcut hints shown while the Header pane holds focus with no
/// overlay open: the horizontal-scroll and leave keys that act on the focused
/// header. Non-empty and context-specific, like every other focused-pane hint.
const HEADER_FOOTER_HINT: &str = "left/right scroll | esc/tab leave | ? help | q quit";

/// The context-specific Status-line shortcut hints for a focused pane
/// (`active_view`) and any open `overlay`, per the TUI Contract / Scenario 19.
///
/// An open modal/overlay owns the hints while it holds focus, so it is matched
/// FIRST: the returned keys are the ones that act in that overlay, replacing the
/// pane's hints until the overlay closes. With no overlay open the hints reflect
/// the focused pane's own available actions, so switching focus to a different
/// pane changes the hints. Every arm returns a non-empty, context-appropriate
/// string, so no context with available actions renders a blank line. The
/// specific strings and bindings are an implementation detail; they stay in
/// lock-step with the key handler and the modal Help sections.
const fn footer_hint(
    active_view: TuiView,
    lane_focus: LaneFocus,
    has_selected_work_item: bool,
    overlay: &TuiOverlay,
) -> &'static str {
    match overlay {
        TuiOverlay::None => pane_footer_hint(active_view, lane_focus, has_selected_work_item),
        TuiOverlay::Search { .. } => "type to search | esc cancel",
        TuiOverlay::CommandPalette { .. } => "type a drain command | esc cancel",
        TuiOverlay::CommandModal { .. } => "up/down select action | enter run | esc cancel",
        TuiOverlay::ValveConfirm { .. } => "up/down change | enter confirm | esc cancel",
        TuiOverlay::WorkItemDetail { .. } => "up/down scroll | PgUp/PgDn page | esc close item",
        TuiOverlay::Help { .. } => "up/down section | PgUp/PgDn scroll | esc close help",
    }
}

/// The Status-line hints for a focused pane `view` with no overlay open: the
/// keys that act on that pane. The read-only nav views (Spec, Events, Repos)
/// share one hint set because their available actions are identical (select +
/// move focus + search); the actionable panes (Attention, Lanes) surface their
/// human-valve/status-move keys, and Settings surfaces its edit key.
///
/// `Lanes` is keyed on `lane_focus` because `Enter` does two DIFFERENT things
/// there: on the lane overview it drills into a lane, and inside a drilled-in
/// lane it opens the selected work-item's detail modal. Advertising the same
/// `enter drill` in both places is precisely the lie this hint used to tell —
/// the hint MUST name the action `Enter` actually performs in the current
/// context, or the Status line is worse than blank.
/// The Status-line hints for a focused pane `view` with no overlay open: the
/// keys that act on that pane RIGHT NOW.
///
/// Keyed on `lane_focus` and `has_selected_work_item`, not on the view alone,
/// because both change which keys actually do anything. `Enter` means "drill
/// into a lane" on the lane overview but "open the selected item's record"
/// inside a drilled-in lane; and every per-item key (the valves, the policy
/// dials, the status move) acts only on a SELECTED work-item, so all of them are
/// inert on the lane overview -- which selects a lane, not an item -- and in an
/// empty drilled-in lane. Listing them there would advertise keys that do
/// nothing, which is the dishonesty the Status-line contract forbids.
///
/// The read-only nav views (Spec, Events, Repos) share one hint set because
/// their available actions are identical (select + move focus + search), and
/// Settings surfaces its edit key.
const fn pane_footer_hint(
    view: TuiView,
    lane_focus: LaneFocus,
    has_selected_work_item: bool,
) -> &'static str {
    match view {
        TuiView::Attention => {
            if has_selected_work_item {
                "up/down move | enter open | p/c/r approve/accept/reject | \
                 m/n set-admission/acceptance | ? help | q quit"
            } else {
                "up/down move | enter open | ? help | q quit"
            }
        }
        TuiView::Lanes => match lane_focus {
            // The lane OVERVIEW selects a LANE, never a work-item, so every
            // per-item key is inert here and none is advertised.
            LaneFocus::Overview => "up/down move | enter drill | ? help | q quit",
            LaneFocus::Lane(_lane) if has_selected_work_item => {
                "up/down move | enter item | esc lane list | s move-status | \
                 p/c/r approve/accept/reject | m/n set-admission/acceptance | ? help | q quit"
            }
            // An EMPTY drilled-in lane: nothing is selected, so `enter` opens
            // nothing and every per-item key is inert.
            LaneFocus::Lane(_lane) => "up/down move | esc lane list | ? help | q quit",
        },
        TuiView::Settings => "up/down move | enter/space edit row | ? help | q quit",
        TuiView::Spec | TuiView::Events | TuiView::Repos => {
            "up/down move | left/right focus | / search | ? help | q quit"
        }
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
    /// Invalid resolve-blocked target variant -- a
    /// `work_item.resolve_blocked_requested` command carried a payload whose
    /// `target_status` was absent or not one of {ready, backlog}.
    InvalidResolveBlockedTarget,
    /// Invalid move target variant -- a `work_item.move_requested` command carried
    /// a payload whose `target_status` was absent or not one of the pre-terminal
    /// pipeline statuses {backlog, ready, blocked, active}.
    InvalidMoveTarget,
    /// Invalid dispatcher-override variant -- a
    /// `work_item.set_dispatcher_override_requested` command named a setting that
    /// admits no per-item override (`wip_cap`) or is served by a policy dial
    /// (`auto_approve_ready` / `acceptance_mode`), named an unknown setting, or
    /// carried a `value` of the wrong type (or a non-positive int) for its cap.
    InvalidDispatcherOverrideSetting,
    /// Invalid dispatcher-setting payload variant -- a
    /// `config.dispatcher_setting_set` command carried a payload that was
    /// malformed, missing a required `repo` / `setting` / `value` field, named an
    /// unknown setting, or carried a value of the wrong type for that setting.
    InvalidDispatcherSettingPayload,
    /// Dispatcher settings not observed variant -- an edit was attempted on the
    /// Settings view while the orchestrator's read surface had not produced a
    /// trustworthy read, so there is no effective value to edit.
    DispatcherSettingsNotObserved,
    /// No selected dispatcher setting variant -- an edit was attempted with no
    /// Settings row selected.
    NoSelectedDispatcherSetting,
    /// Factory drain port failed variant.
    FactoryDrainPortFailed,
    /// No selected attention item variant.
    NoSelectedAttentionItem,
    /// No selected work-item variant -- a per-item valve was invoked with no
    /// work-item selected in either the Attention detail or a drilled-in lane.
    NoSelectedWorkItem,
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
    /// payload-bearing commands whose object the handler reads back -- the
    /// `config.dispatcher_setting_set` write (`{ repo, setting, value }`) and the
    /// payload-bearing work-item valves (`{ mode }` / `{ policy }`) -- since the
    /// payload-less `PersistCommand` path persists an empty `{}` object those
    /// handlers would reject.
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
/// The drain NEVER passes a `--mode` flag to the Dispatcher `loop` subcommand:
/// the Dispatcher owns its own mode, read from the orchestrator's own policy
/// settings, not forwarded on the launcher argv. Every drain therefore builds
/// the SAME argv.
pub struct DispatcherFactoryDrainPort<'a> {
    probe: &'a dyn SourceProbe,
    program: String,
    args: Vec<String>,
}

/// Ready-candidate consideration cap for one operator-initiated drain pass.
const OPERATOR_DRAIN_BUDGET: u32 = 50;

impl<'a> DispatcherFactoryDrainPort<'a> {
    #[must_use]
    /// Construct a new value from its required fields.
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
        let mut arg_refs: Vec<&str> = self.args.iter().map(String::as_str).collect();
        let budget = OPERATOR_DRAIN_BUDGET.to_string();
        arg_refs.push("--budget");
        arg_refs.push(budget.as_str());
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

/// The captured result of a READING orchestrator action.
///
/// For example the `config` read: the honest outcome plus the action's stdout,
/// so the caller can parse the JSON the orchestrator emitted. A write action
/// reports its outcome through [`OrchestratorActionOutcome`] alone and discards
/// stdout; a read needs the payload, hence this richer result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestratorActionReading {
    outcome: OrchestratorActionOutcome,
    stdout: String,
}

impl OrchestratorActionReading {
    #[must_use]
    /// A completed read carrying the action's captured stdout.
    pub const fn observed(stdout: String) -> Self {
        Self {
            outcome: OrchestratorActionOutcome::completed(),
            stdout,
        }
    }

    #[must_use]
    /// A non-zero read: the action ran but reported failure, so its stdout is
    /// not trustworthy and is discarded.
    pub const fn failed() -> Self {
        Self {
            outcome: OrchestratorActionOutcome::failed(),
            stdout: String::new(),
        }
    }

    #[must_use]
    /// A not-wired read: no real action surface is reachable.
    pub const fn not_wired() -> Self {
        Self {
            outcome: OrchestratorActionOutcome::not_wired(),
            stdout: String::new(),
        }
    }

    #[must_use]
    /// Return the honest outcome of the read.
    pub const fn outcome(&self) -> &OrchestratorActionOutcome {
        &self.outcome
    }

    #[must_use]
    /// Return the captured stdout (empty unless the read completed).
    pub fn stdout(&self) -> &str {
        &self.stdout
    }
}

/// Port interface for the orchestrator's published `drive` action surface,
/// supplied by an outer layer.
///
/// The single surface every `work_item.*` valve/policy command AND every
/// dispatcher-settings read/write rides: the console issues an action-id
/// through it and never writes the ledger or the orchestrator's `.livespec.jsonc`
/// directly.
pub trait OrchestratorActionPort {
    /// Run one orchestrator action-id and return its honest outcome.
    ///
    /// # Errors
    /// Returns an application error when the port cannot produce a trustworthy outcome.
    fn run_action(
        &mut self,
        request: &OrchestratorActionRequest,
    ) -> ApplicationResult<OrchestratorActionOutcome>;

    /// Run one READING orchestrator action-id and capture its stdout.
    ///
    /// The default is an honest not-wired reading, so a port that carries no
    /// real read capability never fabricates a payload. The host-backed
    /// [`DispatcherOrchestratorActionPort`] overrides this to capture the
    /// action's real stdout (for example the `config` read's settings JSON).
    ///
    /// # Errors
    /// Returns an application error when the port cannot produce a trustworthy outcome.
    fn read_action(
        &mut self,
        request: &OrchestratorActionRequest,
    ) -> ApplicationResult<OrchestratorActionReading> {
        let _ = request;
        Ok(OrchestratorActionReading::not_wired())
    }
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

    fn read_action(
        &mut self,
        request: &OrchestratorActionRequest,
    ) -> ApplicationResult<OrchestratorActionReading> {
        let mut args: Vec<&str> = self.base_args.iter().map(String::as_str).collect();
        args.push("--action");
        args.push(request.action_id());
        Ok(match self.probe.run_command(&self.program, &args) {
            SourceProbeOutcome::Observed {
                stdout,
                success: true,
            } => OrchestratorActionReading::observed(stdout),
            SourceProbeOutcome::Observed { success: false, .. } => {
                OrchestratorActionReading::failed()
            }
            SourceProbeOutcome::Unavailable { .. } => OrchestratorActionReading::not_wired(),
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
/// work-item lane (contracts.md; scenarios.md Scenario 12).
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
    admission_policy: AdmissionPolicy,
    acceptance_policy: AcceptancePolicy,
    detail: WorkItemDetail,
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
            admission_policy: snapshot.admission_policy(),
            acceptance_policy: snapshot.acceptance_policy(),
            detail: snapshot.detail().clone(),
        }
    }

    #[must_use]
    /// The item's admission policy, as the orchestrator emitted it.
    pub const fn admission_policy(&self) -> AdmissionPolicy {
        self.admission_policy
    }

    #[must_use]
    /// The item's acceptance policy, as the orchestrator emitted it.
    pub const fn acceptance_policy(&self) -> AcceptancePolicy {
        self.acceptance_policy
    }

    #[must_use]
    /// The descriptive half of this item's standardized record — what the
    /// work-item detail modal renders.
    pub const fn detail(&self) -> &WorkItemDetail {
        &self.detail
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
    let attention_entries = unified_attention_entries(events, search_query);
    let attention_items = attention_entries
        .iter()
        .map(AttentionEntry::to_attention_item)
        .collect::<Vec<_>>();
    let attention_count = attention_items.len();
    let selected_attention_index =
        selected_index(attention_items.len(), state.selected_attention_index());
    let detail = selected_attention_index.map(|index| attention_entries[index].to_detail(events));
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
    // The per-item cursor exists only while drilled into a lane that holds at
    // least one item; an empty lane has nothing to select.
    let selected_lane_item_index = match (active_view, lane_focus) {
        (TuiView::Lanes, LaneFocus::Lane(lane)) => lane_board
            .column(lane)
            .map(LaneColumn::count)
            .filter(|count| *count > 0)
            .map(|count| state.selected_lane_item_index().min(count - 1)),
        _ => None,
    };
    let selected_setting_index = match active_view {
        TuiView::Settings => Some(
            state
                .selected_setting_index()
                .min(DispatcherSettingRow::all().len() - 1),
        ),
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
        selected_lane_item_index,
        focus: state.focus(),
        detail_scroll: state.detail_scroll(),
        header_scroll: state.header_scroll(),
        overlay,
        selected_repo: state.selected_repo().to_owned(),
        selected_setting_index,
        dispatcher_settings: state.dispatcher_settings().clone(),
        // The canonical, untruncated header. The source-health segment sits LAST
        // (after attention) so that when a narrow terminal cannot hold every
        // field, `header_line` degrades from the right — dropping the low-value
        // constants and eliding source names — while the operationally-important
        // repo / view / attention fields survive. See `header_line`.
        header: format!(
            "fleet: livespec | mode: tui | repo: {} | view: {} | attention: {}{}",
            header_repo_label(state.selected_repo()),
            active_view.label(),
            attention_count,
            source_health_header_segment(&unavailable_sources)
        ),
        unavailable_sources,
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

/// The distinct backing-source names whose MOST RECENT observation was a
/// not-observed finding, sorted for a stable header order.
///
/// The tally reflects the LATEST poll outcome per source, not any historical
/// failure: folding the event log in `global_seq` order, a
/// [`EventType::SourceNotObservedFindingObserved`] marks its source unavailable,
/// and any LATER positive observation of that same source -- a snapshot event or
/// the observed-and-idle [`EventType::SourceObservedFindingObserved`] marker --
/// clears it. So a source that degraded on an earlier cycle but was observed
/// successfully on a later one no longer counts, and a transient failure is
/// never branded permanently. A source counts only while its most recent
/// observation was not-observed, so the operator can distinguish a cockpit-blind
/// screen from an idle factory.
fn unavailable_sources(events: &[ConsoleEvent]) -> Vec<String> {
    let mut unavailable: BTreeMap<String, bool> = BTreeMap::new();
    for event in events {
        match event.event_type() {
            EventType::SourceNotObservedFindingObserved => {
                unavailable.insert(event.source().to_owned(), true);
            }
            // A positive observation from a backing source clears any prior
            // not-observed finding for it. `and_modify` (never `insert`) keeps a
            // never-degraded source out of the map entirely, so only genuinely
            // degraded-then-recovered sources are tracked and cleared.
            EventType::SourceObservedFindingObserved
            | EventType::WorkItemSnapshotObserved
            | EventType::SourceCompletenessFindingObserved
            | EventType::DispatcherBacklogBounceObserved
            | EventType::FabroHumanGateObserved
            | EventType::GithubPullRequestSnapshotObserved
            | EventType::LivespecNextSnapshotObserved
            | EventType::LivespecReviseRequired => {
                unavailable
                    .entry(event.source().to_owned())
                    .and_modify(|degraded| *degraded = false);
            }
            _other => {}
        }
    }
    unavailable
        .into_iter()
        .filter_map(|(source, degraded)| degraded.then_some(source))
        .collect()
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
/// re-measuring after each step and stopping as soon as it fits. `repo` is never
/// dropped.
fn fit_header_line(
    repo: &str,
    view: &str,
    attention: usize,
    unavailable_sources: &[String],
    width: usize,
) -> String {
    // Fixed display order; `Some` = present, `None` = dropped to make room. Each
    // field is atomic — kept or dropped whole, never mid-truncated.
    let mut fields: [Option<String>; 5] = [
        Some("fleet: livespec".to_owned()),      // 0 — constant identity
        Some("mode: tui".to_owned()),            // 1 — constant
        Some(format!("repo: {repo}")),           // 2 — never dropped
        Some(format!("view: {view}")),           // 3
        Some(format!("attention: {attention}")), // 4
    ];
    let source_forms = source_health_segment_forms(unavailable_sources);
    let mut source_idx = 0usize; // 0 = widest (full names)

    let compose = |fields: &[Option<String>; 5], source_idx: usize| -> String {
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
        Shrink::DropField(3),  // view (already shown, highlighted, in the nav pane)
        Shrink::DropField(4),  // attention count
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

/// Columns the focused Header pane pans per `left`/`right` press. Larger than a
/// single column so a modestly-overflowing header is traversed end-to-end in a
/// few presses (a one-line status header has no need for column-fine control);
/// the render-measured clamp still stops exactly at the true right edge, and the
/// left step saturates at the left-justified default. The specific step is an
/// implementation detail per the TUI Contract in contracts.md.
const HEADER_SCROLL_STEP: usize = 8;

/// The pane the focus ring lands on AFTER `current` when cycling forward (the
/// `Tab` binding). The ring is Nav -> Content -> Detail -> Header -> Nav, but a
/// view with no Detail pane (`Lanes`) skips it, so the ring is Nav -> Content ->
/// Header -> Nav there.
const fn next_focus_pane(current: FocusPane, active_view: TuiView) -> FocusPane {
    match current {
        FocusPane::Nav => FocusPane::Content,
        FocusPane::Content => {
            if view_has_detail_pane(active_view) {
                FocusPane::Detail
            } else {
                FocusPane::Header
            }
        }
        FocusPane::Detail => FocusPane::Header,
        FocusPane::Header => FocusPane::Nav,
    }
}

/// The pane the focus ring lands on BEFORE `current` when cycling backward (the
/// `BackTab`/`Shift-Tab` binding) — the reverse of [`next_focus_pane`].
const fn previous_focus_pane(current: FocusPane, active_view: TuiView) -> FocusPane {
    match current {
        FocusPane::Nav => FocusPane::Header,
        FocusPane::Content => FocusPane::Nav,
        FocusPane::Detail => FocusPane::Content,
        FocusPane::Header => {
            if view_has_detail_pane(active_view) {
                FocusPane::Detail
            } else {
                FocusPane::Content
            }
        }
    }
}

/// Whether `active_view` renders a right-hand Detail pane (every view except
/// `Lanes`, which spans the full body width beside the nav). The focus ring and
/// the spatial `right` walk both clamp against this so neither lands on a Detail
/// pane the view does not draw.
const fn view_has_detail_pane(active_view: TuiView) -> bool {
    !matches!(active_view, TuiView::Lanes)
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
            .with_active_view(move_view_down(state.active_view()))
            .with_detail_scroll(0),
        TuiInteraction::SelectPreviousView => state
            .clone()
            .with_active_view(move_view_up(state.active_view()))
            .with_detail_scroll(0),
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
        TuiInteraction::FocusContent => state.clone().with_focus(FocusPane::Content),
        TuiInteraction::FocusNav => state.clone().with_focus(FocusPane::Nav),
        TuiInteraction::FocusDetail => state.clone().with_focus(FocusPane::Detail),
        TuiInteraction::FocusNextPane => state
            .clone()
            .with_focus(next_focus_pane(state.focus(), state.active_view())),
        TuiInteraction::FocusPreviousPane => state
            .clone()
            .with_focus(previous_focus_pane(state.focus(), state.active_view())),
        TuiInteraction::ScrollHeaderRight => {
            // Clamp to the render-measured maximum (the full header width minus
            // the pane's inner width), fed back each frame exactly like the Detail
            // pane's vertical clamp, so the right edge reached is the true clip
            // point at the current viewport width.
            state.clone().with_header_scroll(
                (state.header_scroll() + HEADER_SCROLL_STEP).min(state.header_max_scroll()),
            )
        }
        TuiInteraction::ScrollHeaderLeft => state
            .clone()
            .with_header_scroll(state.header_scroll().saturating_sub(HEADER_SCROLL_STEP)),
        TuiInteraction::ScrollDetailDown => {
            // Clamp to the render-measured wrapped max scroll (the largest offset
            // that keeps the pane's last wrapped row visible), NOT a width-agnostic
            // logical line count. The renderer measures it via `Paragraph::line_count`
            // — the SAME count that sizes the scrollbar — and the interactive loop
            // feeds it back into the state, so the scroll range and the scrollbar
            // agree and the true bottom of a wrapping detail is reachable (Finding G).
            state
                .clone()
                .with_detail_scroll((state.detail_scroll() + 1).min(state.detail_max_scroll()))
        }
        TuiInteraction::ScrollDetailUp => state
            .clone()
            .with_detail_scroll(state.detail_scroll().saturating_sub(1)),
        TuiInteraction::OpenHelp => state.clone().with_overlay(TuiOverlay::Help {
            selected_section: help_section_for_focus(state.focus(), state.active_view()),
            scroll: 0,
        }),
        TuiInteraction::HelpSelectNextSection => state
            .clone()
            .with_overlay(help_select_section(state.overlay(), true)),
        TuiInteraction::HelpSelectPreviousSection => state
            .clone()
            .with_overlay(help_select_section(state.overlay(), false)),
        TuiInteraction::HelpScrollDown => state
            .clone()
            .with_overlay(help_scroll(state.overlay(), true)),
        TuiInteraction::HelpScrollUp => state
            .clone()
            .with_overlay(help_scroll(state.overlay(), false)),
        TuiInteraction::OpenWorkItemDetail => {
            state.clone().with_overlay(open_work_item_detail(&model))
        }
        TuiInteraction::WorkItemDetailScrollDown(rows) => state
            .clone()
            .with_overlay(work_item_detail_scroll(state.overlay(), rows, true)),
        TuiInteraction::WorkItemDetailScrollUp(rows) => state
            .clone()
            .with_overlay(work_item_detail_scroll(state.overlay(), rows, false)),
        TuiInteraction::OpenValveConfirm(valve) => state
            .clone()
            .with_overlay(TuiOverlay::ValveConfirm { valve }),
        TuiInteraction::CycleValveOption(forward) => state
            .clone()
            .with_overlay(cycle_valve_option(state.overlay(), forward)),
    }
}

/// The overlay `OpenWorkItemDetail` resolves to: the work-item detail modal
/// PINNED to the selected item's id, or no overlay at all when nothing is
/// selected.
///
/// Pinning the id here (rather than letting the renderer re-read the lane
/// selection each frame) is what keeps the open modal on the item it was opened
/// on while ingestion keeps re-ranking the lane underneath it. With no selection
/// there is no honest record to show, so the modal does not open at all.
fn open_work_item_detail(model: &TuiScreenModel) -> TuiOverlay {
    model
        .selected_lane_item()
        .map_or(TuiOverlay::None, |item| TuiOverlay::WorkItemDetail {
            work_item_id: item.work_item_id().to_owned(),
            scroll: 0,
        })
}

/// Scroll the work-item detail modal by `rows` down (`down`) or up, leaving any
/// other overlay unchanged (the interaction is inert unless that modal is open).
///
/// Down saturates here and is clamped by the renderer against the record's
/// measured wrapped height — the same feed-back-the-measured-max discipline the
/// Detail pane and the Help modal use — so the true bottom of a long
/// description is reachable and no further.
fn work_item_detail_scroll(overlay: &TuiOverlay, rows: usize, down: bool) -> TuiOverlay {
    let TuiOverlay::WorkItemDetail {
        work_item_id,
        scroll,
    } = overlay
    else {
        return overlay.clone();
    };
    TuiOverlay::WorkItemDetail {
        // The pinned id rides through every scroll step: scrolling must never
        // re-resolve WHICH work-item is on screen.
        work_item_id: work_item_id.clone(),
        scroll: if down {
            scroll.saturating_add(rows)
        } else {
            scroll.saturating_sub(rows)
        },
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

/// Whether the `Lanes` view is drilled into a single lane, where up/down moves
/// the per-item cursor within that lane's list rather than the attention
/// selection.
fn is_lane_drilldown(state: &TuiInteractionState) -> bool {
    state.active_view() == TuiView::Lanes && matches!(state.lane_focus(), LaneFocus::Lane(_lane))
}

/// The number of work-items in the currently drilled-in lane, or `0` when the
/// `Lanes` view is not drilled into a lane. Used to bound the per-item cursor.
fn drilldown_item_count(state: &TuiInteractionState, model: &TuiScreenModel) -> usize {
    let LaneFocus::Lane(lane) = state.lane_focus() else {
        return 0;
    };
    model.lane_board().column(lane).map_or(0, LaneColumn::count)
}

/// Whether the `Settings` view is active, where up/down moves the selected
/// setting row rather than the attention selection.
fn is_settings_view(state: &TuiInteractionState) -> bool {
    state.active_view() == TuiView::Settings
}

/// Move the selection down, routed to the lane overview row or the settings row
/// when one of those views is active, else to the attention list.
fn select_next(state: &TuiInteractionState, model: &TuiScreenModel) -> TuiInteractionState {
    if is_lane_overview(state) {
        state.clone().with_selected_lane_index(move_selection_down(
            Lane::all().len(),
            state.selected_lane_index(),
        ))
    } else if is_lane_drilldown(state) {
        state
            .clone()
            .with_selected_lane_item_index(move_selection_down(
                drilldown_item_count(state, model),
                state.selected_lane_item_index(),
            ))
    } else if is_settings_view(state) {
        state
            .clone()
            .with_selected_setting_index(move_selection_down(
                DispatcherSettingRow::all().len(),
                state.selected_setting_index(),
            ))
    } else {
        state
            .clone()
            .with_selected_attention_index(move_selection_down(
                model.attention_items().len(),
                state.selected_attention_index(),
            ))
            // A different item is now selected, so its Detail pane shows
            // different content: reset the scroll so the previous item's offset
            // never carries over.
            .with_detail_scroll(0)
    }
}

/// Move the selection up, routed to the lane overview row or the settings row
/// when one of those views is active, else to the attention list.
fn select_previous(state: &TuiInteractionState) -> TuiInteractionState {
    if is_lane_overview(state) {
        state
            .clone()
            .with_selected_lane_index(move_selection_up(state.selected_lane_index()))
    } else if is_lane_drilldown(state) {
        state
            .clone()
            .with_selected_lane_item_index(move_selection_up(state.selected_lane_item_index()))
    } else if is_settings_view(state) {
        state
            .clone()
            .with_selected_setting_index(move_selection_up(state.selected_setting_index()))
    } else {
        state
            .clone()
            .with_selected_attention_index(move_selection_up(state.selected_attention_index()))
            // Reset the Detail scroll for the newly-selected item (see select_next).
            .with_detail_scroll(0)
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

/// Resolve the edit of the selected `Settings` row into a single per-setting
/// write.
///
/// Editing a dispatcher setting is an ORDINARY recorded write: it produces a
/// `config.dispatcher_setting_set` command for the one setting under the cursor,
/// carrying the NEXT value (a flipped bool, a cycled enum, or an incremented
/// int) computed from the effective value the console observed. There is NO
/// type-to-confirm modal or any other arming ceremony -- enabling a dangerous
/// setting rides the exact same path as any other operator command.
///
/// # Errors
/// Returns [`ApplicationError::EmptyOperatorAction`] when `requested_by` is
/// blank, [`ApplicationError::DispatcherSettingsNotObserved`] when no
/// trustworthy read produced the effective values, and
/// [`ApplicationError::NoSelectedDispatcherSetting`] when no Settings row is
/// selected.
pub fn resolve_dispatcher_setting_edit(
    model: &TuiScreenModel,
    requested_by: &str,
) -> ApplicationResult<OperatorActionOutcome> {
    validate_operator_action(requested_by)?;
    let DispatcherSettingsRead::Observed(settings) = model.dispatcher_settings() else {
        return Err(ApplicationError::DispatcherSettingsNotObserved);
    };
    let index = model
        .selected_setting_index()
        .ok_or(ApplicationError::NoSelectedDispatcherSetting)?;
    let row = DispatcherSettingRow::all()
        .get(index)
        .ok_or(ApplicationError::NoSelectedDispatcherSetting)?;
    let write = row.next_write(settings);
    Ok(dispatcher_setting_set_outcome(
        model.selected_repo(),
        &write,
        requested_by,
    ))
}

/// Build the `config.dispatcher_setting_set` persist outcome for `repo`,
/// carrying the `{ repo, setting, value }` payload the Configuration context
/// reads back. This is the one and only console command that changes a global
/// default, and it changes exactly one setting.
fn dispatcher_setting_set_outcome(
    repo: &str,
    write: &DispatcherSettingWrite,
    requested_by: &str,
) -> OperatorActionOutcome {
    let key = write.key();
    let value_literal = write.value_literal();
    let command = CommandEnvelope::new(
        format!("cmd_config_dispatcher_setting_set_{repo}_{key}_{value_literal}"),
        CommandType::ConfigDispatcherSettingSet,
        repo.to_owned(),
        format!("{repo}:config.dispatcher_setting_set:{key}={value_literal}"),
        requested_by.to_owned(),
    );
    let payload_json = serde_json::json!({
        "repo": repo,
        "setting": key,
        "value": write.value_json(),
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
/// the valve-confirm modal (or a move-status valve stages a pair that is not an
/// operator-drivable transition), and [`ApplicationError::NoSelectedWorkItem`]
/// when no work-item is selected in either the Attention detail or a drilled-in
/// lane. The selected item's work-item id is carried verbatim as the command
/// aggregate; the orchestrator's `drive` surface (and the downstream
/// `work_item.*` handler) is the authority on its legality.
pub fn resolve_valve_action(
    model: &TuiScreenModel,
    requested_by: &str,
) -> ApplicationResult<OperatorActionOutcome> {
    let requested_by = validate_operator_action(requested_by)?;
    let valve = model
        .overlay()
        .valve_confirm()
        .ok_or(ApplicationError::NoSelectedOperatorAction)?;
    let work_item_id = model
        .selected_work_item_id()
        .ok_or(ApplicationError::NoSelectedWorkItem)?;
    valve_outcome(valve, work_item_id, requested_by)
        .ok_or(ApplicationError::NoSelectedOperatorAction)
}

/// Build the persist outcome for one staged valve against `work_item_id`, or
/// `None` when a move-status valve stages a pair that is not an
/// operator-drivable transition (the payload-carrying valves and the plain
/// human valves are always `Some`).
fn valve_outcome(
    valve: PendingValve,
    work_item_id: &str,
    requested_by: &str,
) -> Option<OperatorActionOutcome> {
    match valve {
        PendingValve::Approve => Some(OperatorActionOutcome::PersistCommand(work_item_command(
            "approve",
            CommandType::WorkItemApproveRequested,
            work_item_id,
            requested_by,
        ))),
        PendingValve::Accept => Some(OperatorActionOutcome::PersistCommand(work_item_command(
            "accept",
            CommandType::WorkItemAcceptRequested,
            work_item_id,
            requested_by,
        ))),
        PendingValve::Reject(mode) => Some(work_item_payload_outcome(
            "reject",
            CommandType::WorkItemRejectRequested,
            work_item_id,
            "mode",
            mode.as_str(),
            requested_by,
        )),
        PendingValve::SetAdmission(policy) => Some(work_item_payload_outcome(
            "set_admission",
            CommandType::WorkItemSetAdmissionRequested,
            work_item_id,
            "policy",
            policy.label(),
            requested_by,
        )),
        PendingValve::SetAcceptance(policy) => Some(work_item_payload_outcome(
            "set_acceptance",
            CommandType::WorkItemSetAcceptanceRequested,
            work_item_id,
            "policy",
            policy.label(),
            requested_by,
        )),
        PendingValve::MoveStatus { from, to } => {
            move_status_outcome(from, to, work_item_id, requested_by)
        }
        PendingValve::SetOverride(override_dial) => Some(work_item_override_outcome(
            work_item_id,
            override_dial,
            requested_by,
        )),
    }
}

/// Map a `from -> to` move onto the persist outcome for the real orchestrator
/// transition it drives. A SEMANTIC valve wins where one exists: `approve`
/// (`pending-approval -> ready`), `accept` (`acceptance -> done`), and
/// `resolve-blocked` (`blocked -> ready | backlog`). Every other pre-terminal
/// target (`backlog`/`ready`/`active`/`blocked`) rides the guarded broad
/// `move:<id>:<target>` action. `None` for any pair that is not an
/// operator-drivable transition -- the move-status valve only ever stages a pair
/// produced by [`status_move_targets`], so this never rejects a valve the
/// operator could actually open.
fn move_status_outcome(
    from: Lane,
    to: Lane,
    work_item_id: &str,
    requested_by: &str,
) -> Option<OperatorActionOutcome> {
    match (from, to) {
        (Lane::PendingApproval, Lane::Ready) => {
            Some(OperatorActionOutcome::PersistCommand(work_item_command(
                "approve",
                CommandType::WorkItemApproveRequested,
                work_item_id,
                requested_by,
            )))
        }
        (Lane::Acceptance, Lane::Done) => {
            Some(OperatorActionOutcome::PersistCommand(work_item_command(
                "accept",
                CommandType::WorkItemAcceptRequested,
                work_item_id,
                requested_by,
            )))
        }
        (Lane::Blocked, Lane::Ready | Lane::Backlog) => Some(work_item_payload_outcome(
            "resolve_blocked",
            CommandType::WorkItemResolveBlockedRequested,
            work_item_id,
            "target_status",
            to.label(),
            requested_by,
        )),
        (_from, Lane::Backlog | Lane::Ready | Lane::Active | Lane::Blocked) => {
            Some(work_item_payload_outcome(
                "move",
                CommandType::WorkItemMoveRequested,
                work_item_id,
                "target_status",
                to.label(),
                requested_by,
            ))
        }
        _other => None,
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
        | EventType::FactoryDrainStarted => "factory",
        EventType::WorkItemActionStarted
        | EventType::WorkItemActionCompleted
        | EventType::WorkItemActionFailed
        | EventType::WorkItemActionNotWired => "work_item",
        EventType::ConfigDispatcherSettingChanged | EventType::ConfigDispatcherSettingNotWired => {
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
        | EventType::SourceObservedFindingObserved
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

/// Handle a `work_item.resolve_blocked_requested` command.
///
/// Resolve-blocked moves a `blocked` work-item on to `ready` or `backlog`:
/// `payload_json` is `{"target_status": "ready" | "backlog"}`. The handler
/// validates the work-item id, parses and validates the target, derives the
/// `resolve-blocked:<work-item-id>:<target>` action-id, and rides the shared
/// orchestrator-action port and `work_item` outcome family exactly like the
/// other valves. Thin console-side validation only -- the orchestrator's `drive`
/// surface owns state-legality (it refuses a non-`blocked` item) -- and it never
/// writes the ledger directly.
///
/// # Errors
/// Returns [`ApplicationError::EmptyWorkItemId`] when the id is empty and
/// [`ApplicationError::InvalidResolveBlockedTarget`] when the payload's
/// `target_status` is absent or not one of {ready, backlog}; also surfaces a port
/// error when the port cannot produce a trustworthy outcome.
pub fn handle_work_item_resolve_blocked_command(
    command: &CommandEnvelope,
    payload_json: &str,
    port: &mut dyn OrchestratorActionPort,
) -> ApplicationResult<WorkItemCommandOutcome> {
    let work_item_id = validate_work_item_id(command.aggregate_id())?;
    let target = resolve_blocked_target_from_payload(payload_json)?;
    let action_id = format!("resolve-blocked:{work_item_id}:{target}");
    run_work_item_action(command, &action_id, port)
}

/// Extract the resolve-blocked `target_status` from a command's persisted
/// `payload_json`.
///
/// The payload is the JSON object `{"target_status": "ready" | "backlog"}`; any
/// other shape is an [`ApplicationError::InvalidResolveBlockedTarget`]. These are
/// the two targets the orchestrator's `resolve-blocked` action accepts.
fn resolve_blocked_target_from_payload(payload_json: &str) -> ApplicationResult<&'static str> {
    let value: serde_json::Value = serde_json::from_str(payload_json)
        .map_err(|_error| ApplicationError::InvalidResolveBlockedTarget)?;
    let target = value
        .get("target_status")
        .and_then(serde_json::Value::as_str)
        .ok_or(ApplicationError::InvalidResolveBlockedTarget)?;
    match target {
        "ready" => Ok("ready"),
        "backlog" => Ok("backlog"),
        _other => Err(ApplicationError::InvalidResolveBlockedTarget),
    }
}

/// Handle a `work_item.move_requested` command.
///
/// Move relocates a work-item to a pre-terminal pipeline status: `payload_json`
/// is `{"target_status": "backlog" | "ready" | "blocked" | "active"}`. The
/// handler validates the work-item id, parses and validates the target, derives
/// the guarded `move:<work-item-id>:<target>` action-id, and rides the shared
/// orchestrator-action port and `work_item` outcome family. Thin console-side
/// validation only -- the orchestrator's `drive` surface owns state-legality (it
/// refuses `done`/`acceptance`/`pending-approval` targets, the ship-guard) -- and
/// it never writes the ledger directly.
///
/// # Errors
/// Returns [`ApplicationError::EmptyWorkItemId`] when the id is empty and
/// [`ApplicationError::InvalidMoveTarget`] when the payload's `target_status` is
/// absent or not a pre-terminal pipeline status; also surfaces a port error when
/// the port cannot produce a trustworthy outcome.
pub fn handle_work_item_move_command(
    command: &CommandEnvelope,
    payload_json: &str,
    port: &mut dyn OrchestratorActionPort,
) -> ApplicationResult<WorkItemCommandOutcome> {
    let work_item_id = validate_work_item_id(command.aggregate_id())?;
    let target = move_target_from_payload(payload_json)?;
    let action_id = format!("move:{work_item_id}:{target}");
    run_work_item_action(command, &action_id, port)
}

/// Extract the move `target_status` from a command's persisted `payload_json`.
///
/// The payload is `{"target_status": "backlog" | "ready" | "blocked" |
/// "active"}` -- the four pre-terminal pipeline statuses the orchestrator's
/// guarded `move` action accepts. Any other shape (including a `done` /
/// `acceptance` / `pending-approval` target the ship-guard forbids) is an
/// [`ApplicationError::InvalidMoveTarget`].
fn move_target_from_payload(payload_json: &str) -> ApplicationResult<&'static str> {
    let value: serde_json::Value =
        serde_json::from_str(payload_json).map_err(|_error| ApplicationError::InvalidMoveTarget)?;
    let target = value
        .get("target_status")
        .and_then(serde_json::Value::as_str)
        .ok_or(ApplicationError::InvalidMoveTarget)?;
    match target {
        "backlog" => Ok("backlog"),
        "ready" => Ok("ready"),
        "blocked" => Ok("blocked"),
        "active" => Ok("active"),
        _other => Err(ApplicationError::InvalidMoveTarget),
    }
}

/// Handle a `work_item.set_dispatcher_override_requested` command.
///
/// Per-item override sets or clears ONE of the three overridable cap settings:
/// `payload_json` is `{"setting": "<key>", "value": <json>}` where `value` is a
/// bool for `merge_on_review_cap`, a positive int for `review_fix_cap` /
/// `acceptance_rework_cap`, or `null` to clear the override back to
/// inherit-global. The handler validates the work-item id, maps the setting onto
/// its orchestrator action verb, serializes the value (`clear` for a null), and
/// rides the shared orchestrator-action port and `work_item` outcome family. It
/// rejects `wip_cap` (no per-item override) and `auto_approve_ready` /
/// `acceptance_mode` (served by the admission / acceptance policy dials), so each
/// overridable setting has exactly one console command.
///
/// # Errors
/// Returns [`ApplicationError::EmptyWorkItemId`] when the id is empty and
/// [`ApplicationError::InvalidDispatcherOverrideSetting`] when `setting` is
/// absent/unknown/not overridable by this command or `value` is the wrong type
/// (or a non-positive int) for its cap; also surfaces a port error.
pub fn handle_work_item_set_dispatcher_override_command(
    command: &CommandEnvelope,
    payload_json: &str,
    port: &mut dyn OrchestratorActionPort,
) -> ApplicationResult<WorkItemCommandOutcome> {
    let work_item_id = validate_work_item_id(command.aggregate_id())?;
    let action_id = dispatcher_override_action_id(work_item_id, payload_json)?;
    run_work_item_action(command, &action_id, port)
}

/// Derive the `set-<cap>:<work-item-id>:<value>` action-id from a per-item
/// override command's `{ setting, value }` payload, mapping each of the three cap
/// settings onto its orchestrator verb and rejecting any other setting.
fn dispatcher_override_action_id(
    work_item_id: &str,
    payload_json: &str,
) -> ApplicationResult<String> {
    let value: serde_json::Value = serde_json::from_str(payload_json)
        .map_err(|_error| ApplicationError::InvalidDispatcherOverrideSetting)?;
    let setting = value
        .get("setting")
        .and_then(serde_json::Value::as_str)
        .ok_or(ApplicationError::InvalidDispatcherOverrideSetting)?;
    let raw_value = value
        .get("value")
        .ok_or(ApplicationError::InvalidDispatcherOverrideSetting)?;
    let (verb, literal) = match setting {
        "merge_on_review_cap" => ("set-merge-on-review-cap", bool_override_literal(raw_value)?),
        "review_fix_cap" => ("set-review-fix-cap", int_override_literal(raw_value)?),
        "acceptance_rework_cap" => (
            "set-acceptance-rework-cap",
            int_override_literal(raw_value)?,
        ),
        _other => return Err(ApplicationError::InvalidDispatcherOverrideSetting),
    };
    Ok(format!("{verb}:{work_item_id}:{literal}"))
}

/// The action-id value segment for a boolean cap override: `true`/`false`, or
/// `clear` for a JSON null (clear-to-inherit). Any other JSON type is invalid.
fn bool_override_literal(value: &serde_json::Value) -> ApplicationResult<String> {
    if value.is_null() {
        return Ok("clear".to_owned());
    }
    value
        .as_bool()
        .map(|flag| flag.to_string())
        .ok_or(ApplicationError::InvalidDispatcherOverrideSetting)
}

/// The action-id value segment for an integer cap override: the positive decimal
/// value, or `clear` for a JSON null. Zero and non-integers are invalid (the
/// orchestrator's cap contract is a positive int).
fn int_override_literal(value: &serde_json::Value) -> ApplicationResult<String> {
    if value.is_null() {
        return Ok("clear".to_owned());
    }
    let number = u32_from_json(value).ok_or(ApplicationError::InvalidDispatcherOverrideSetting)?;
    if number == 0 {
        return Err(ApplicationError::InvalidDispatcherOverrideSetting);
    }
    Ok(number.to_string())
}

/// Build the persist-with-payload outcome for a per-item override valve: a
/// `work_item.set_dispatcher_override_requested` command carrying
/// `{ "setting": "<key>", "value": <json> }`, where the value is a bool, a
/// number, or `null` for clear-to-inherit.
fn work_item_override_outcome(
    work_item_id: &str,
    override_dial: DispatcherOverride,
    requested_by: &str,
) -> OperatorActionOutcome {
    let key = override_dial.setting_key();
    let value_literal = override_dial.value_literal();
    let command = CommandEnvelope::new(
        format!(
            "cmd_work_item_set_dispatcher_override_requested_{work_item_id}_{key}_{value_literal}"
        ),
        CommandType::WorkItemSetDispatcherOverrideRequested,
        work_item_id.to_owned(),
        format!("{work_item_id}:work_item.set_dispatcher_override_requested:{key}={value_literal}"),
        requested_by.to_owned(),
    );
    let mut payload = serde_json::Map::new();
    let _ = payload.insert(
        "setting".to_owned(),
        serde_json::Value::String(key.to_owned()),
    );
    let _ = payload.insert("value".to_owned(), override_dial.payload_value());
    OperatorActionOutcome::PersistCommandWithPayload {
        command,
        payload_json: serde_json::Value::Object(payload).to_string(),
    }
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
// Configuration context — dispatcher-settings read/write through the API.
// ---------------------------------------------------------------------------

/// The parsed `{ repo, setting, value }` payload of a
/// `config.dispatcher_setting_set` command: the target repo plus the single
/// typed [`DispatcherSettingWrite`] it changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatcherSettingSetRequest {
    repo: String,
    write: DispatcherSettingWrite,
}

impl DispatcherSettingSetRequest {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(repo: String, write: DispatcherSettingWrite) -> Self {
        Self { repo, write }
    }

    #[must_use]
    /// Return the target repo id.
    pub fn repo(&self) -> &str {
        &self.repo
    }

    #[must_use]
    /// Return the single setting write this command effects.
    pub const fn write(&self) -> &DispatcherSettingWrite {
        &self.write
    }

    /// Parse the `{ repo, setting, value }` payload from a command's persisted
    /// `payload_json`.
    ///
    /// # Errors
    /// Returns [`ApplicationError::InvalidDispatcherSettingPayload`] when the JSON
    /// is malformed, a required field is absent, `repo` is empty, `setting` names
    /// no known key, or `value` is the wrong type for that setting.
    pub fn from_payload_json(payload_json: &str) -> ApplicationResult<Self> {
        let value: serde_json::Value = serde_json::from_str(payload_json)
            .map_err(|_error| ApplicationError::InvalidDispatcherSettingPayload)?;
        let repo = value
            .get("repo")
            .and_then(serde_json::Value::as_str)
            .ok_or(ApplicationError::InvalidDispatcherSettingPayload)?;
        if repo.trim().is_empty() {
            return Err(ApplicationError::InvalidDispatcherSettingPayload);
        }
        let setting = value
            .get("setting")
            .and_then(serde_json::Value::as_str)
            .ok_or(ApplicationError::InvalidDispatcherSettingPayload)?;
        let setting_value = value
            .get("value")
            .ok_or(ApplicationError::InvalidDispatcherSettingPayload)?;
        let write = dispatcher_setting_write_from_key_value(setting, setting_value)
            .ok_or(ApplicationError::InvalidDispatcherSettingPayload)?;
        Ok(Self::new(repo.to_owned(), write))
    }
}

/// Build the typed [`DispatcherSettingWrite`] for one `{ setting, value }` pair,
/// or `None` when `setting` names no known key or `value` is the wrong type for
/// that setting. The mapping is exhaustive over the six keys, so a key the type
/// system knows is handled here too.
fn dispatcher_setting_write_from_key_value(
    setting: &str,
    value: &serde_json::Value,
) -> Option<DispatcherSettingWrite> {
    match setting {
        "auto_approve_ready" => value
            .as_bool()
            .map(DispatcherSettingWrite::AutoApproveReady),
        "merge_on_review_cap" => value
            .as_bool()
            .map(DispatcherSettingWrite::MergeOnReviewCap),
        "acceptance_mode" => value
            .as_str()
            .and_then(acceptance_policy_from_label)
            .map(DispatcherSettingWrite::AcceptanceMode),
        "review_fix_cap" => u32_from_json(value).map(DispatcherSettingWrite::ReviewFixCap),
        "acceptance_rework_cap" => {
            u32_from_json(value).map(DispatcherSettingWrite::AcceptanceReworkCap)
        }
        "wip_cap" => u32_from_json(value).map(DispatcherSettingWrite::WipCap),
        _unknown => None,
    }
}

/// Parse an [`AcceptancePolicy`] from its kebab-case label, or `None`.
fn acceptance_policy_from_label(label: &str) -> Option<AcceptancePolicy> {
    AcceptancePolicy::all()
        .iter()
        .copied()
        .find(|policy| policy.label() == label)
}

/// Read a JSON number as a `u32`, or `None` when it is not a non-negative
/// integer in range.
fn u32_from_json(value: &serde_json::Value) -> Option<u32> {
    value.as_u64().and_then(|number| u32::try_from(number).ok())
}

/// One of the six API-configurable dispatcher settings paired with the value to
/// write.
///
/// The console commands each setting THROUGH the orchestrator's published
/// `set-config:<key>:<value>` action and holds no setting state of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatcherSettingWrite {
    /// `auto_approve_ready` (bool): auto-approve ready work-items.
    AutoApproveReady(bool),
    /// `merge_on_review_cap` (bool): merge once the review cap is reached.
    MergeOnReviewCap(bool),
    /// `acceptance_mode` (enum): the acceptance policy, reusing [`AcceptancePolicy`].
    AcceptanceMode(AcceptancePolicy),
    /// `review_fix_cap` (int): the review-fix attempt ceiling.
    ReviewFixCap(u32),
    /// `acceptance_rework_cap` (int): the acceptance-rework attempt ceiling.
    AcceptanceReworkCap(u32),
    /// `wip_cap` (int): the per-repo concurrency ceiling (no per-item override).
    WipCap(u32),
}

impl DispatcherSettingWrite {
    #[must_use]
    /// The orchestrator `dispatcher.*` key this write targets.
    pub const fn key(&self) -> &'static str {
        match self {
            Self::AutoApproveReady(_) => "auto_approve_ready",
            Self::MergeOnReviewCap(_) => "merge_on_review_cap",
            Self::AcceptanceMode(_) => "acceptance_mode",
            Self::ReviewFixCap(_) => "review_fix_cap",
            Self::AcceptanceReworkCap(_) => "acceptance_rework_cap",
            Self::WipCap(_) => "wip_cap",
        }
    }

    #[must_use]
    /// The value serialized as the orchestrator's `set-config` grammar expects:
    /// `true`/`false` for a bool, the kebab-case label for [`AcceptancePolicy`],
    /// and the decimal digits for an int.
    pub fn value_literal(&self) -> String {
        match self {
            Self::AutoApproveReady(value) | Self::MergeOnReviewCap(value) => value.to_string(),
            Self::AcceptanceMode(policy) => policy.label().to_owned(),
            Self::ReviewFixCap(value) | Self::AcceptanceReworkCap(value) | Self::WipCap(value) => {
                value.to_string()
            }
        }
    }

    #[must_use]
    /// The value as typed JSON for the `config.dispatcher_setting_set` payload's
    /// `value` field: a JSON bool for a bool, a JSON string (the kebab-case
    /// label) for [`AcceptancePolicy`], and a JSON number for an int.
    pub fn value_json(&self) -> serde_json::Value {
        match self {
            Self::AutoApproveReady(value) | Self::MergeOnReviewCap(value) => {
                serde_json::Value::Bool(*value)
            }
            Self::AcceptanceMode(policy) => serde_json::Value::String(policy.label().to_owned()),
            Self::ReviewFixCap(value) | Self::AcceptanceReworkCap(value) | Self::WipCap(value) => {
                serde_json::Value::Number((*value).into())
            }
        }
    }
}

/// The largest value the console proposes when cycling an integer setting row;
/// `Enter`/`Space` increments by one and wraps back to [`INT_SETTING_MIN`] past
/// this ceiling. The console owns no policy semantics -- the orchestrator is the
/// authority on a value's legality -- so this is only the operator-facing dial
/// range, never a persisted bound.
const INT_SETTING_MAX: u32 = 9;

/// The smallest value the integer-setting dial proposes; the caps are per-run
/// ceilings for which zero is never a useful operator proposal, so the dial
/// wraps to one rather than zero.
const INT_SETTING_MIN: u32 = 1;

/// One step of the integer-setting dial: increment by one, wrapping from
/// [`INT_SETTING_MAX`] back to [`INT_SETTING_MIN`] (an observed value below the
/// minimum, including zero, is nudged up to the minimum).
const fn cycled_int_setting(value: u32) -> u32 {
    if value >= INT_SETTING_MAX || value < INT_SETTING_MIN {
        INT_SETTING_MIN
    } else {
        value + 1
    }
}

/// The six dispatcher policy settings the `Settings` view renders, in display
/// order.
///
/// This is the single source of truth binding each row's label, inline
/// help, dangerous-ness, rendered value, and per-edit write, so the surface and
/// its edits can never drift from each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatcherSettingRow {
    /// The `auto_approve_ready` bool row.
    AutoApproveReady,
    /// The `merge_on_review_cap` bool row.
    MergeOnReviewCap,
    /// The `acceptance_mode` enum row.
    AcceptanceMode,
    /// The `review_fix_cap` int row.
    ReviewFixCap,
    /// The `acceptance_rework_cap` int row.
    AcceptanceReworkCap,
    /// The `wip_cap` int row.
    WipCap,
}

impl DispatcherSettingRow {
    #[must_use]
    /// The six rows in display order.
    pub const fn all() -> &'static [Self] {
        &[
            Self::AutoApproveReady,
            Self::MergeOnReviewCap,
            Self::AcceptanceMode,
            Self::ReviewFixCap,
            Self::AcceptanceReworkCap,
            Self::WipCap,
        ]
    }

    #[must_use]
    /// The operator-facing row label.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::AutoApproveReady => "Auto-approve ready",
            Self::MergeOnReviewCap => "Merge on review cap",
            Self::AcceptanceMode => "Acceptance mode",
            Self::ReviewFixCap => "Review fix cap",
            Self::AcceptanceReworkCap => "Acceptance rework cap",
            Self::WipCap => "WIP cap",
        }
    }

    #[must_use]
    /// The row's inline help. A dangerous row's help carries the
    /// "dangerous / use with caution" label (see [`Self::dangerous`]).
    pub const fn help(&self) -> &'static str {
        match self {
            Self::AutoApproveReady => {
                "dangerous / use with caution -- when on, the factory auto-approves a ready \
                 work-item with no human in the loop. Enter/Space toggles."
            }
            Self::MergeOnReviewCap => {
                "dangerous / use with caution -- when on, the factory merges a change once the \
                 review-fix cap is reached with no human sign-off. Enter/Space toggles."
            }
            Self::AcceptanceMode => {
                "dangerous / use with caution when ai-only -- how a work-item is accepted: \
                 ai-then-human, ai-only (AI auto-accepts, no human), or human-only. \
                 Enter/Space cycles."
            }
            Self::ReviewFixCap => {
                "the review-fix attempt ceiling before the factory escalates to a human. \
                 Enter/Space increments (wraps)."
            }
            Self::AcceptanceReworkCap => {
                "the acceptance-rework attempt ceiling before the factory escalates to a human. \
                 Enter/Space increments (wraps)."
            }
            Self::WipCap => {
                "the per-repo concurrency ceiling (no per-item override). \
                 Enter/Space increments (wraps)."
            }
        }
    }

    #[must_use]
    /// Whether a non-default value of this setting lets the factory act without a
    /// human, so every UI surface labels it "dangerous / use with caution".
    pub const fn dangerous(&self) -> bool {
        matches!(
            self,
            Self::AutoApproveReady | Self::MergeOnReviewCap | Self::AcceptanceMode
        )
    }

    #[must_use]
    /// The orchestrator `dispatcher.*` key this row surfaces -- the
    /// API-configurable key name the settings-completeness check matches against
    /// the orchestrator's published config-manifest. It is the same key the
    /// row's [`DispatcherSettingWrite`] carries.
    pub const fn orchestrator_key(&self) -> &'static str {
        match self {
            Self::AutoApproveReady => "auto_approve_ready",
            Self::MergeOnReviewCap => "merge_on_review_cap",
            Self::AcceptanceMode => "acceptance_mode",
            Self::ReviewFixCap => "review_fix_cap",
            Self::AcceptanceReworkCap => "acceptance_rework_cap",
            Self::WipCap => "wip_cap",
        }
    }

    #[must_use]
    /// The effective value of this row, rendered as the operator sees it.
    pub fn value(&self, settings: &DispatcherSettings) -> String {
        match self {
            Self::AutoApproveReady => bool_label(settings.auto_approve_ready()).to_owned(),
            Self::MergeOnReviewCap => bool_label(settings.merge_on_review_cap()).to_owned(),
            Self::AcceptanceMode => settings.acceptance_mode().label().to_owned(),
            Self::ReviewFixCap => settings.review_fix_cap().to_string(),
            Self::AcceptanceReworkCap => settings.acceptance_rework_cap().to_string(),
            Self::WipCap => settings.wip_cap().to_string(),
        }
    }

    #[must_use]
    /// The single-setting write an edit of this row submits: a flipped bool, the
    /// enum cycled one step, or the int incremented (wrapping).
    pub fn next_write(&self, settings: &DispatcherSettings) -> DispatcherSettingWrite {
        match self {
            Self::AutoApproveReady => {
                DispatcherSettingWrite::AutoApproveReady(!settings.auto_approve_ready())
            }
            Self::MergeOnReviewCap => {
                DispatcherSettingWrite::MergeOnReviewCap(!settings.merge_on_review_cap())
            }
            Self::AcceptanceMode => DispatcherSettingWrite::AcceptanceMode(rotate(
                AcceptancePolicy::all(),
                settings.acceptance_mode(),
                true,
            )),
            Self::ReviewFixCap => {
                DispatcherSettingWrite::ReviewFixCap(cycled_int_setting(settings.review_fix_cap()))
            }
            Self::AcceptanceReworkCap => DispatcherSettingWrite::AcceptanceReworkCap(
                cycled_int_setting(settings.acceptance_rework_cap()),
            ),
            Self::WipCap => DispatcherSettingWrite::WipCap(cycled_int_setting(settings.wip_cap())),
        }
    }
}

/// A `Settings` view row prepared for rendering: the label, the effective value,
/// the inline help for the detail pane, and whether the row is dangerous.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingRow {
    label: &'static str,
    value: String,
    help: &'static str,
    dangerous: bool,
}

impl SettingRow {
    #[must_use]
    /// The operator-facing row label.
    pub const fn label(&self) -> &'static str {
        self.label
    }

    #[must_use]
    /// The effective value the console observed for this row.
    pub fn value(&self) -> &str {
        &self.value
    }

    #[must_use]
    /// The row's inline help for the detail pane.
    pub const fn help(&self) -> &'static str {
        self.help
    }

    #[must_use]
    /// Whether the row is dangerous (labelled "dangerous / use with caution").
    pub const fn dangerous(&self) -> bool {
        self.dangerous
    }
}

/// Build the six `Settings` rows from the effective values the console observed,
/// in display order.
///
/// The `Settings` view renders these; an unobserved read surface has no rows to
/// render (the caller degrades to a not-observed finding).
#[must_use]
pub fn dispatcher_setting_rows(settings: &DispatcherSettings) -> Vec<SettingRow> {
    DispatcherSettingRow::all()
        .iter()
        .map(|row| SettingRow {
            label: row.label(),
            value: row.value(settings),
            help: row.help(),
            dangerous: row.dangerous(),
        })
        .collect()
}

/// The operator-facing on/off label for a bool setting value.
const fn bool_label(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

/// The `config` read action-id: the orchestrator reports every effective
/// dispatcher setting and whether it is explicitly set or defaulted.
const CONFIG_READ_ACTION_ID: &str = "config";

/// Build the `set-config:<key>:<value>` write action-id for one setting — the
/// per-setting write grammar the orchestrator's `drive` surface publishes.
fn set_config_action_id(setting: &DispatcherSettingWrite) -> String {
    format!("set-config:{}:{}", setting.key(), setting.value_literal())
}

/// The six effective dispatcher settings as the orchestrator's `config` read
/// reports them — a point-in-time read of the orchestrator-owned values, never
/// persisted by the console.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatcherSettings {
    auto_approve_ready: bool,
    merge_on_review_cap: bool,
    acceptance_mode: AcceptancePolicy,
    review_fix_cap: u32,
    acceptance_rework_cap: u32,
    wip_cap: u32,
}

impl DispatcherSettings {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(
        auto_approve_ready: bool,
        merge_on_review_cap: bool,
        acceptance_mode: AcceptancePolicy,
        review_fix_cap: u32,
        acceptance_rework_cap: u32,
        wip_cap: u32,
    ) -> Self {
        Self {
            auto_approve_ready,
            merge_on_review_cap,
            acceptance_mode,
            review_fix_cap,
            acceptance_rework_cap,
            wip_cap,
        }
    }

    #[must_use]
    /// The effective `auto_approve_ready` value.
    pub const fn auto_approve_ready(&self) -> bool {
        self.auto_approve_ready
    }

    #[must_use]
    /// The effective `merge_on_review_cap` value.
    pub const fn merge_on_review_cap(&self) -> bool {
        self.merge_on_review_cap
    }

    #[must_use]
    /// The effective `acceptance_mode` value.
    pub const fn acceptance_mode(&self) -> AcceptancePolicy {
        self.acceptance_mode
    }

    #[must_use]
    /// The effective `review_fix_cap` value.
    pub const fn review_fix_cap(&self) -> u32 {
        self.review_fix_cap
    }

    #[must_use]
    /// The effective `acceptance_rework_cap` value.
    pub const fn acceptance_rework_cap(&self) -> u32 {
        self.acceptance_rework_cap
    }

    #[must_use]
    /// The effective `wip_cap` value.
    pub const fn wip_cap(&self) -> u32 {
        self.wip_cap
    }
}

/// The honest outcome of reading the effective dispatcher settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatcherSettingsRead {
    /// The orchestrator reported all six settings.
    Observed(DispatcherSettings),
    /// No trustworthy read was produced (the surface is not wired, the action
    /// failed, or its payload could not be parsed). The caller degrades to a
    /// named not-observed finding rather than an assumed value.
    NotObserved,
}

/// Reads and writes the six API-configurable dispatcher settings THROUGH the
/// orchestrator's published `drive` config actions, riding the shared
/// [`OrchestratorActionPort`].
///
/// It shells no subprocess of its own and holds no setting state: every read
/// goes through the `config` action and every write through a
/// `set-config:<key>:<value>` action, so the console never writes the
/// orchestrator's `.livespec.jsonc` directly. The orchestrator owns the single
/// persistent record of each setting.
pub struct DispatcherSettingsPort<'a> {
    action_port: &'a mut dyn OrchestratorActionPort,
}

impl<'a> DispatcherSettingsPort<'a> {
    #[must_use]
    /// Construct a settings port over the shared orchestrator-action port.
    pub fn new(action_port: &'a mut dyn OrchestratorActionPort) -> Self {
        Self { action_port }
    }

    /// Read the effective values of all six settings through the `config` action.
    ///
    /// # Errors
    /// Returns an application error when the underlying port cannot produce a
    /// trustworthy outcome.
    pub fn read_settings(&mut self) -> ApplicationResult<DispatcherSettingsRead> {
        let request = OrchestratorActionRequest::new(CONFIG_READ_ACTION_ID.to_owned());
        let reading = self.action_port.read_action(&request)?;
        if *reading.outcome() != OrchestratorActionOutcome::Completed {
            return Ok(DispatcherSettingsRead::NotObserved);
        }
        Ok(settings_from_config_read(reading.stdout()).map_or(
            DispatcherSettingsRead::NotObserved,
            DispatcherSettingsRead::Observed,
        ))
    }

    /// Write one setting through its `set-config:<key>:<value>` action and return
    /// the honest outcome.
    ///
    /// # Errors
    /// Returns an application error when the underlying port cannot produce a
    /// trustworthy outcome.
    pub fn write_setting(
        &mut self,
        setting: &DispatcherSettingWrite,
    ) -> ApplicationResult<OrchestratorActionOutcome> {
        let request = OrchestratorActionRequest::new(set_config_action_id(setting));
        self.action_port.run_action(&request)
    }
}

/// The `config` read payload shape the orchestrator emits under `--json`: a
/// `settings[]` array of one `{ key, value }` per effective setting.
#[derive(serde::Deserialize)]
struct ConfigReadPayload {
    settings: Vec<ConfigReadSetting>,
}

#[derive(serde::Deserialize)]
struct ConfigReadSetting {
    key: String,
    value: serde_json::Value,
}

/// Parse the `config` read's `settings[]` array into the six effective values.
/// `None` when the payload does not parse or any of the six keys is absent or
/// mistyped, so the caller degrades to a not-observed finding.
fn settings_from_config_read(stdout: &str) -> Option<DispatcherSettings> {
    let payload: ConfigReadPayload = serde_json::from_str(stdout).ok()?;
    let mut by_key: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for setting in payload.settings {
        let _ = by_key.insert(setting.key, setting.value);
    }
    Some(DispatcherSettings::new(
        bool_setting(&by_key, "auto_approve_ready")?,
        bool_setting(&by_key, "merge_on_review_cap")?,
        acceptance_setting(&by_key, "acceptance_mode")?,
        u32_setting(&by_key, "review_fix_cap")?,
        u32_setting(&by_key, "acceptance_rework_cap")?,
        u32_setting(&by_key, "wip_cap")?,
    ))
}

fn bool_setting(by_key: &BTreeMap<String, serde_json::Value>, key: &str) -> Option<bool> {
    by_key.get(key).and_then(serde_json::Value::as_bool)
}

fn u32_setting(by_key: &BTreeMap<String, serde_json::Value>, key: &str) -> Option<u32> {
    let raw = by_key.get(key).and_then(serde_json::Value::as_u64)?;
    u32::try_from(raw).ok()
}

fn acceptance_setting(
    by_key: &BTreeMap<String, serde_json::Value>,
    key: &str,
) -> Option<AcceptancePolicy> {
    let value = by_key.get(key)?;
    serde_json::from_value::<AcceptancePolicy>(value.clone()).ok()
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

/// Handle a `config.dispatcher_setting_set` command.
///
/// The Configuration context's per-setting write. It parses the
/// `{ repo, setting, value }` payload into a single typed
/// [`DispatcherSettingWrite`], reads the effective value the setting had before
/// the write (for the audit fact), then effects the change through the
/// orchestrator's published `set-config` command surface (via
/// [`DispatcherSettingsPort`]) rather than the orchestrator's `.livespec.jsonc`
/// directly. On a completed write it appends the durable
/// `config.dispatcher_setting.changed` audit event carrying
/// `{ repo, setting, previous, new, actor, occurred_at }`. A not-wired or failed
/// write surfaces `config.dispatcher_setting.not_wired` and NO changed event,
/// never a fabricated success. There is NO arming ceremony: enabling a dangerous
/// setting rides this exact path like any other write.
///
/// # Errors
/// Returns [`ApplicationError::InvalidDispatcherSettingPayload`] when the payload
/// is malformed, and surfaces a port error when the port cannot produce a
/// trustworthy outcome.
pub fn handle_config_dispatcher_setting_set_command(
    command: &CommandEnvelope,
    payload_json: &str,
    occurred_at: &str,
    settings_port: &mut DispatcherSettingsPort<'_>,
) -> ApplicationResult<ConfigCommandOutcome> {
    let request = DispatcherSettingSetRequest::from_payload_json(payload_json)?;
    let write = request.write();
    // The effective value before the write, for the audit fact. An unreadable
    // read surface yields a null `previous` rather than a fabricated value.
    let previous_value = match settings_port.read_settings()? {
        DispatcherSettingsRead::Observed(settings) => previous_setting_value_json(&settings, write),
        DispatcherSettingsRead::NotObserved => serde_json::Value::Null,
    };
    let mut events = vec![config_command_event(
        command,
        EventType::CommandAccepted,
        "accepted",
        1,
        "{}",
    )];
    let command_status = match settings_port.write_setting(write)? {
        OrchestratorActionOutcome::Completed => {
            events.push(config_command_event(
                command,
                EventType::ConfigDispatcherSettingChanged,
                "changed",
                2,
                &serde_json::json!({
                    "repo": request.repo(),
                    "setting": write.key(),
                    "previous": previous_value,
                    "new": write.value_json(),
                    "actor": command.requested_by(),
                    "occurred_at": occurred_at,
                })
                .to_string(),
            ));
            "completed"
        }
        OrchestratorActionOutcome::NotWired | OrchestratorActionOutcome::Failed => {
            // The settings write did not land (no real surface, or the action
            // failed). Emit the honest not-wired outcome and NO changed event
            // rather than fabricating success.
            events.push(config_command_event(
                command,
                EventType::ConfigDispatcherSettingNotWired,
                "not_wired",
                2,
                &serde_json::json!({ "repo": request.repo(), "setting": write.key() }).to_string(),
            ));
            "not_wired"
        }
    };
    Ok(ConfigCommandOutcome::new(command_status.to_owned(), events))
}

/// The effective value one setting had BEFORE a write, as typed JSON for the
/// `config.dispatcher_setting.changed` audit fact's `previous` field. Reads the
/// field the `write` targets from the pre-write [`DispatcherSettings`].
fn previous_setting_value_json(
    settings: &DispatcherSettings,
    write: &DispatcherSettingWrite,
) -> serde_json::Value {
    match write {
        DispatcherSettingWrite::AutoApproveReady(_) => {
            serde_json::Value::Bool(settings.auto_approve_ready())
        }
        DispatcherSettingWrite::MergeOnReviewCap(_) => {
            serde_json::Value::Bool(settings.merge_on_review_cap())
        }
        DispatcherSettingWrite::AcceptanceMode(_) => {
            serde_json::Value::String(settings.acceptance_mode().label().to_owned())
        }
        DispatcherSettingWrite::ReviewFixCap(_) => {
            serde_json::Value::Number(settings.review_fix_cap().into())
        }
        DispatcherSettingWrite::AcceptanceReworkCap(_) => {
            serde_json::Value::Number(settings.acceptance_rework_cap().into())
        }
        DispatcherSettingWrite::WipCap(_) => serde_json::Value::Number(settings.wip_cap().into()),
    }
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

/// One entry in the unified Attention view.
///
/// Either a valve-actionable work-item lane snapshot (the console's own lane fold
/// over `work_item.*` observations) or a product needs-attention item ingested
/// from the orchestrator's `needs-attention` surface. Projecting BOTH into one
/// list is the spec's needs-attention inbox (`scenarios.md` Scenario 1: "the
/// needs-attention view lists all three items from the `attention_item` stream"):
/// the operator sees every human-owned action -- the lane valves AND the spec /
/// plan / hygiene / human-valve needs-attention items -- in one place, each
/// attributed to its true `source_ref.repo`.
#[derive(Debug, Clone)]
enum AttentionEntry {
    WorkItem(AttentionSnapshot),
    NeedsAttention(AttentionItemSnapshot),
}

impl AttentionEntry {
    /// The list-row projection: the entry rendered as an [`AttentionItem`].
    fn to_attention_item(&self) -> AttentionItem {
        match self {
            Self::WorkItem(entry) => AttentionItem::new(
                entry.snapshot.work_item_id().to_owned(),
                attention_title(&entry.snapshot),
                entry.event.source().to_owned(),
                entry.snapshot.repo().to_owned(),
                None,
            ),
            Self::NeedsAttention(item) => attention_item_from_snapshot(item),
        }
    }

    /// The detail-pane projection: the rich fabro / timeline / valve detail for a
    /// work-item, or the composed repo + subject + operator-handoff detail for a
    /// needs-attention item.
    fn to_detail(&self, events: &[ConsoleEvent]) -> AttentionDetail {
        match self {
            Self::WorkItem(entry) => build_attention_detail(entry, events),
            Self::NeedsAttention(item) => build_needs_attention_detail(item),
        }
    }
}

/// The unified Attention list: valve-actionable work-item lane snapshots first
/// (rank-ordered), then the ingested needs-attention items (id-ordered),
/// de-duplicated so a work-item that surfaces in BOTH the lane fold and the
/// needs-attention surface (for example a `blocked` / `needs-human` item that is
/// also a human-valve needs-attention item) appears once -- as its richer
/// work-item entry, which preserves the existing fabro-attach / timeline / valve
/// detail. Both kinds are filtered by the active search query.
fn unified_attention_entries(
    events: &[ConsoleEvent],
    search_query: Option<&str>,
) -> Vec<AttentionEntry> {
    let work_items = attention_snapshots_matching(events, search_query);
    let claimed_work_item_ids: BTreeSet<&str> = work_items
        .iter()
        .map(|entry| entry.snapshot.work_item_id())
        .collect();
    let mut entries: Vec<AttentionEntry> = work_items
        .iter()
        .cloned()
        .map(AttentionEntry::WorkItem)
        .collect();
    for item in materialize_attention_items(events) {
        if !attention_item_matches(&item, search_query) {
            continue;
        }
        if item
            .source_ref()
            .work_item()
            .is_some_and(|work_item| claimed_work_item_ids.contains(work_item))
        {
            continue;
        }
        entries.push(AttentionEntry::NeedsAttention(item));
    }
    entries
}

/// Whether a needs-attention item matches the active search query, mirroring the
/// work-item matcher over the fields the item carries.
fn attention_item_matches(item: &AttentionItemSnapshot, search_query: Option<&str>) -> bool {
    search_query.is_none_or(|query| {
        if query.is_empty() {
            return true;
        }
        let needle = query.to_lowercase();
        let source_ref = item.source_ref();
        item.summary().to_lowercase().contains(&needle)
            || item.id().to_lowercase().contains(&needle)
            || item.kind().to_lowercase().contains(&needle)
            || source_ref.repo().to_lowercase().contains(&needle)
            || source_ref
                .work_item()
                .is_some_and(|value| value.to_lowercase().contains(&needle))
            || source_ref
                .path()
                .is_some_and(|value| value.to_lowercase().contains(&needle))
    })
}

/// The detail pane for an ingested needs-attention item: its true source repo,
/// the subject it points at (its work-item, else its path, else its stable id),
/// and the operator handoff command to run. It carries no fabro run and no lane
/// valve actions -- those belong only to lane work-item entries -- so the
/// fabro-run slot is a `-` placeholder and the actions / timeline are empty.
fn build_needs_attention_detail(item: &AttentionItemSnapshot) -> AttentionDetail {
    let source_ref = item.source_ref();
    let subject = source_ref
        .work_item()
        .or_else(|| source_ref.path())
        .unwrap_or_else(|| item.id());
    AttentionDetail::new(
        source_ref.repo().to_owned(),
        subject.to_owned(),
        "-".to_owned(),
        item.handoff().command().to_owned(),
        Vec::new(),
        Vec::new(),
    )
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
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::WorkItemDetail { .. }
        | TuiOverlay::Help { .. } => None,
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
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::WorkItemDetail { .. }
        | TuiOverlay::Help { .. } => overlay.clone(),
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

/// The number of sections the modal Help menu carries.
///
/// One `Global actions` section, one section per focusable view pane
/// (`TuiView`), plus a final section for the top/header pane -- the count the
/// menu enumerates and the navigation clamp bounds against.
pub const HELP_SECTION_COUNT: usize = 1 + TuiView::all().len() + 1;

/// The Help-menu section index for the top/header pane: the LAST section.
///
/// It comes after `Global actions` and every view pane. Pressing `?` while the
/// Header pane holds focus auto-focuses Help here (per the TUI Contract: one
/// section per focusable pane, `?` opens auto-focused to THAT pane's section).
#[must_use]
pub const fn header_help_section() -> usize {
    HELP_SECTION_COUNT - 1
}

/// The Help-menu section index that pane/view `view` auto-focuses.
///
/// Section `0` is `Global actions`; each view occupies section
/// `view_index + 1`, so the section order mirrors the nav (per the TUI
/// Contract: one section per focusable pane, `?` opens auto-focused to THAT
/// pane's section).
#[must_use]
pub fn help_section_for_view(view: TuiView) -> usize {
    view_index(view) + 1
}

/// The Help-menu section index that the currently focused pane auto-focuses.
///
/// It is the top/header pane's own section when the Header holds focus,
/// otherwise the active view's section. Threads focus through `OpenHelp` so `?`
/// opens on the focused pane's section even when that pane is the header (which
/// is not view-keyed).
#[must_use]
pub fn help_section_for_focus(focus: FocusPane, active_view: TuiView) -> usize {
    match focus {
        FocusPane::Header => header_help_section(),
        FocusPane::Nav | FocusPane::Content | FocusPane::Detail => {
            help_section_for_view(active_view)
        }
    }
}

/// Move the modal Help menu selection one section forward (`down`) or backward
/// (`up`), clamped to the valid section range, resetting the right-pane scroll
/// so a newly selected section always starts at its top. Leaves a non-Help
/// overlay unchanged (the interaction is inert unless Help is open).
fn help_select_section(overlay: &TuiOverlay, down: bool) -> TuiOverlay {
    match overlay {
        TuiOverlay::Help {
            selected_section, ..
        } => {
            let next = if down {
                (selected_section + 1).min(HELP_SECTION_COUNT - 1)
            } else {
                selected_section.saturating_sub(1)
            };
            TuiOverlay::Help {
                selected_section: next,
                scroll: 0,
            }
        }
        TuiOverlay::None
        | TuiOverlay::Search { .. }
        | TuiOverlay::CommandPalette { .. }
        | TuiOverlay::CommandModal { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::WorkItemDetail { .. } => overlay.clone(),
    }
}

/// Scroll the modal Help right-hand text pane one row down (`down`) or up,
/// preserving the selected section. Down saturates (the renderer clamps the
/// offset to the section's wrapped height); up saturates at the top. Leaves a
/// non-Help overlay unchanged.
fn help_scroll(overlay: &TuiOverlay, down: bool) -> TuiOverlay {
    match overlay {
        TuiOverlay::Help {
            selected_section,
            scroll,
        } => TuiOverlay::Help {
            selected_section: *selected_section,
            scroll: if down {
                scroll.saturating_add(1)
            } else {
                scroll.saturating_sub(1)
            },
        },
        TuiOverlay::None
        | TuiOverlay::Search { .. }
        | TuiOverlay::CommandPalette { .. }
        | TuiOverlay::CommandModal { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::WorkItemDetail { .. } => overlay.clone(),
    }
}

fn type_overlay_char(overlay: &TuiOverlay, value: char) -> TuiOverlay {
    match overlay {
        TuiOverlay::Search { query } => TuiOverlay::Search {
            query: format!("{query}{value}"),
        },
        TuiOverlay::CommandPalette { query } => TuiOverlay::CommandPalette {
            query: format!("{query}{value}"),
        },
        TuiOverlay::None
        | TuiOverlay::CommandModal { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::WorkItemDetail { .. }
        | TuiOverlay::Help { .. } => overlay.clone(),
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
        TuiOverlay::None
        | TuiOverlay::CommandModal { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::WorkItemDetail { .. }
        | TuiOverlay::Help { .. } => overlay.clone(),
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
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::WorkItemDetail { .. }
        | TuiOverlay::Help { .. } => overlay.clone(),
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
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::WorkItemDetail { .. }
        | TuiOverlay::Help { .. } => overlay.clone(),
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
        // The Attention, Lanes, and Settings views render their own projections
        // (the attention list / detail, the lane board, the dispatcher-settings
        // rows), not summary rows.
        TuiView::Attention | TuiView::Lanes | TuiView::Settings => Vec::new(),
    }
}

fn spec_view_items(events: &[ConsoleEvent]) -> Vec<ViewSummaryItem> {
    // Operational counts only: each row's live count is its whole content, with
    // no baked-in explanatory detail (B5 -- pane bodies carry operational
    // content only; any explanation lives in the user documentation).
    vec![
        ViewSummaryItem::new(
            format!(
                "LiveSpec next snapshots: {}",
                count_events(events, EventType::LivespecNextSnapshotObserved)
            ),
            String::new(),
        ),
        ViewSummaryItem::new(
            format!(
                "Revise required: {}",
                count_events(events, EventType::LivespecReviseRequired)
            ),
            String::new(),
        ),
    ]
}

fn events_view_items(events: &[ConsoleEvent]) -> Vec<ViewSummaryItem> {
    let latest = events
        .last()
        .map_or_else(|| "none".to_owned(), latest_event_summary);
    vec![
        // The stored-event count is the whole operational content of this row;
        // the latest-event row below carries the live latest-event summary.
        // Neither carries baked-in explanatory prose (B5).
        ViewSummaryItem::new(format!("Stored events: {}", events.len()), String::new()),
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

/// The repo each event belongs to, for the "Repos observed" projection.
///
/// The derivation is event-shape aware because two stream-key shapes coexist:
///
/// - A needs-attention `attention_item:{repo}:{id}` stream embeds a
///   colon-bearing item id (`valve:set-admission:bd-ib-ss7rkr`,
///   `spec:prune-history:SPECIFICATION`, `hygiene:stale-branch:refs/heads/...`),
///   so the repo can NOT be recovered by splitting the stream key. For an
///   `appeared` / `changed` event the true repo is read from the item's own
///   `source_ref.repo` in the payload — correct for every persisted row
///   regardless of the repo the stream was keyed under. A `resolved` event
///   carries only the id, so it falls back to the stream key's middle segment.
/// - Every other event streams under `{context}:{repo}` (`repo:{repo}`,
///   `factory:{repo}`, ...); the repo is the segment AFTER the first colon.
fn repo_id(event: &ConsoleEvent) -> String {
    match event.event_type() {
        EventType::AttentionItemAppeared | EventType::AttentionItemChanged => {
            attention_item_snapshot_from_payload_json(event.payload_json()).map_or_else(
                || attention_stream_repo(event.stream_id()),
                |item| item.source_ref().repo().to_owned(),
            )
        }
        EventType::AttentionItemResolved => attention_stream_repo(event.stream_id()),
        _other => stream_prefix_repo(event.stream_id()),
    }
}

/// The repo segment of a `{context}:{repo}` stream key: the text after the first
/// colon, or the whole key when it carries no colon.
fn stream_prefix_repo(stream_id: &str) -> String {
    stream_id
        .split_once(':')
        .map_or_else(|| stream_id.to_owned(), |(_context, repo)| repo.to_owned())
}

/// The repo segment of an `attention_item:{repo}:{id}` stream key: its middle
/// segment. Falls back to `-` when the key carries no middle segment (an
/// attention stream key is always three-part, so this is a defensive default).
fn attention_stream_repo(stream_id: &str) -> String {
    let mut parts = stream_id.splitn(3, ':');
    let _context = parts.next();
    parts
        .next()
        .filter(|repo| !repo.is_empty())
        .map_or_else(|| "-".to_owned(), ToOwned::to_owned)
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
            Self::SourceObservedFindingObserved => "Source observed (idle)",
            Self::AttentionItemAppeared => "Attention item appeared",
            Self::AttentionItemChanged => "Attention item changed",
            Self::AttentionItemResolved => "Attention item resolved",
            Self::ConfigDispatcherSettingChanged => "Dispatcher setting changed",
            Self::ConfigDispatcherSettingNotWired => "Dispatcher setting not wired",
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
        AutonomousDecisionsPort, ConfigCommandOutcome, DispatcherFactoryDrainPort,
        DispatcherOrchestratorActionPort, DispatcherOverride, DispatcherSettingRow,
        DispatcherSettingSetRequest, DispatcherSettingWrite, DispatcherSettings,
        DispatcherSettingsPort, DispatcherSettingsRead, FactoryDrainPolicy, FactoryDrainPort,
        FactoryDrainPortOutcome, FactoryDrainRequest, FocusPane, HEADER_SCROLL_STEP,
        HELP_SECTION_COUNT, JournalAutonomousDecisionsPort, LaneFocus, LaneWorkItem,
        OperatorAction, OperatorActionOutcome, OrchestratorActionOutcome, OrchestratorActionPort,
        OrchestratorActionRequest, OverrideBool, OverrideInt, PendingValve, RejectMode, SettingRow,
        TuiInteraction, TuiInteractionState, TuiOverlay, TuiScreenModel, TuiView, build_tui_model,
        build_tui_model_for_state, dispatcher_setting_rows, drilldown_item_count, footer_hint,
        handle_config_dispatcher_setting_set_command, handle_factory_drain_command,
        handle_work_item_accept_command, handle_work_item_approve_command,
        handle_work_item_move_command, handle_work_item_reject_command,
        handle_work_item_resolve_blocked_command, handle_work_item_set_acceptance_command,
        handle_work_item_set_admission_command, handle_work_item_set_dispatcher_override_command,
        header_help_section, help_section_for_focus, help_section_for_view, project_attention,
        project_lane_board, reduce_tui_interaction, resolve_command_palette_action,
        resolve_dispatcher_setting_edit, resolve_selected_operator_action, resolve_valve_action,
        set_acceptance_policy_from_payload, set_admission_policy_from_payload, status_move_targets,
        validate_operator_action, work_item_override_outcome,
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
            "fleet: livespec | mode: tui | repo: - | view: Attention | attention: 0"
        );
        // The default Attention view (no overlay) shows the Attention pane's
        // context-specific hints -- non-empty and appropriate to the focused
        // pane, never the old single static string (Scenario 19 / TUI Contract).
        // This fixture's inbox is EMPTY ("attention: 0"), so the per-item valve
        // keys act on nothing and are correctly absent from the hint line.
        assert_eq!(
            model.footer(),
            "up/down move | enter open | ? help | q quit"
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
    fn unified_attention_view_merges_needs_attention_items_and_dedups() {
        // A blocked / needs-human work-item the lane fold surfaces as a
        // valve-actionable item, the SAME work-item ALSO carried as a human-valve
        // needs-attention item, and two needs-attention-only items (a spec
        // prune-history and a plan review) that carry the TRUE orchestrator repo.
        let orchestrator = "livespec-orchestrator-beads-fabro";
        let valve = AttentionItemSnapshot::new(
            "valve:set-admission:bd-ib-ss7rkr",
            "human-valve",
            "high",
            "Resolve human-needed block for work-item bd-ib-ss7rkr",
            AttentionSourceRef::new(orchestrator, Some("bd-ib-ss7rkr"), None),
            AttentionHandoff::new(
                "drive",
                Some("set-admission:bd-ib-ss7rkr:manual"),
                "drive-cmd",
            ),
        );
        let prune = AttentionItemSnapshot::new(
            "spec:prune-history:SPECIFICATION",
            "spec",
            "low",
            "33 unpruned history versions; consider pruning",
            AttentionSourceRef::new(orchestrator, None, Some("SPECIFICATION")),
            AttentionHandoff::new("livespec-op", None, "codex exec livespec:prune-history"),
        );
        let plan = AttentionItemSnapshot::new(
            "plan:console-autonomous-mode",
            "plan",
            "medium",
            "Review plan thread console-autonomous-mode.",
            AttentionSourceRef::new(orchestrator, None, Some("plan/console-autonomous-mode/")),
            AttentionHandoff::new("plan", None, "codex exec plan console-autonomous-mode"),
        );
        let events = [
            lane_event(
                "evt_wi",
                "bd-ib-ss7rkr",
                Lane::Blocked,
                Some(LaneReason::NeedsHuman),
                "a0",
                "blocked",
            ),
            attention_appeared("evt_valve", &valve),
            attention_appeared("evt_prune", &prune),
            attention_appeared("evt_plan", &plan),
        ];

        let model = build_tui_model(&events, 0);

        // The valve needs-attention item de-duplicates against the lane work-item
        // (same id), so the unified list is: the richer work-item entry first, then
        // the two needs-attention-only items (id-ordered).
        let ids: Vec<&str> = model
            .attention_items()
            .iter()
            .map(AttentionItem::id)
            .collect();
        assert_eq!(
            ids,
            [
                "bd-ib-ss7rkr",
                "plan:console-autonomous-mode",
                "spec:prune-history:SPECIFICATION",
            ]
        );
        // The header attention count reflects the unified list.
        assert!(model.header_line(300).contains("attention: 3"));

        // The needs-attention items carry their TRUE orchestrator repo in the
        // composed source reference, never the console's own name.
        let plan_item = &model.attention_items()[1];
        assert_eq!(
            plan_item.title(),
            "Review plan thread console-autonomous-mode."
        );
        assert_eq!(
            plan_item.source_reference(),
            "livespec-orchestrator-beads-fabro:plan/console-autonomous-mode/"
        );

        // The work-item entry preserves its existing lane-derived detail.
        assert_eq!(
            model.detail().map(super::AttentionDetail::work_item),
            Some("bd-ib-ss7rkr")
        );

        // Selecting a needs-attention item projects its composed detail (repo +
        // path subject + operator command), not a lane detail.
        let plan_model = build_tui_model(&events, 1);
        assert_eq!(
            plan_model.detail().map(super::AttentionDetail::repo),
            Some("livespec-orchestrator-beads-fabro")
        );
        assert_eq!(
            plan_model.detail().map(super::AttentionDetail::work_item),
            Some("plan/console-autonomous-mode/")
        );
    }

    #[test]
    fn unified_attention_view_search_filters_both_kinds() {
        let orchestrator = "livespec-orchestrator-beads-fabro";
        let prune = AttentionItemSnapshot::new(
            "spec:prune-history:SPECIFICATION",
            "spec",
            "low",
            "33 unpruned history versions; consider pruning",
            AttentionSourceRef::new(orchestrator, None, Some("SPECIFICATION")),
            AttentionHandoff::new("livespec-op", None, "codex exec livespec:prune-history"),
        );
        let plan = AttentionItemSnapshot::new(
            "plan:console-autonomous-mode",
            "plan",
            "medium",
            "Review plan thread console-autonomous-mode.",
            AttentionSourceRef::new(orchestrator, None, Some("plan/console-autonomous-mode/")),
            AttentionHandoff::new("plan", None, "codex exec plan console-autonomous-mode"),
        );
        let events = [
            lane_event(
                "evt_wi",
                "console-blocked",
                Lane::Blocked,
                Some(LaneReason::NeedsHuman),
                "a0",
                "blocked",
            ),
            attention_appeared("evt_prune", &prune),
            attention_appeared("evt_plan", &plan),
        ];

        // "prune" matches only the spec item; the work-item and the plan item are
        // filtered out of the unified list.
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::Search {
                query: "prune".to_owned(),
            },
        );
        let model = build_tui_model_for_state(&events, &state);
        let ids: Vec<&str> = model
            .attention_items()
            .iter()
            .map(AttentionItem::id)
            .collect();
        assert_eq!(ids, ["spec:prune-history:SPECIFICATION"]);
    }

    #[test]
    fn build_needs_attention_detail_composes_subject_repo_and_command() {
        let orchestrator = "livespec-orchestrator-beads-fabro";
        // work_item present -> the subject is the work-item id.
        let with_work_item = AttentionItemSnapshot::new(
            "valve:set-admission:bd-ib-ss7rkr",
            "human-valve",
            "high",
            "Resolve block",
            AttentionSourceRef::new(orchestrator, Some("bd-ib-ss7rkr"), None),
            AttentionHandoff::new(
                "drive",
                Some("set-admission:bd-ib-ss7rkr:manual"),
                "drive-cmd",
            ),
        );
        let detail = super::build_needs_attention_detail(&with_work_item);
        assert_eq!(detail.repo(), orchestrator);
        assert_eq!(detail.work_item(), "bd-ib-ss7rkr");
        assert_eq!(detail.fabro_run(), "-");
        assert_eq!(detail.attach_command(), "drive-cmd");
        assert!(detail.timeline().is_empty());
        assert!(detail.actions().is_empty());

        // no work_item but a path -> the subject is the path.
        let with_path = AttentionItemSnapshot::new(
            "spec:prune-history:SPECIFICATION",
            "spec",
            "low",
            "prune",
            AttentionSourceRef::new(orchestrator, None, Some("SPECIFICATION")),
            AttentionHandoff::new("livespec-op", None, "prune-cmd"),
        );
        assert_eq!(
            super::build_needs_attention_detail(&with_path).work_item(),
            "SPECIFICATION"
        );

        // neither work_item nor path -> the subject falls back to the stable id.
        let bare = AttentionItemSnapshot::new(
            "hygiene:stale-branch:refs/heads/x",
            "hygiene",
            "low",
            "bare",
            AttentionSourceRef::new(orchestrator, None, None),
            AttentionHandoff::new("shell", None, "shell-cmd"),
        );
        assert_eq!(
            super::build_needs_attention_detail(&bare).work_item(),
            "hygiene:stale-branch:refs/heads/x"
        );
    }

    #[test]
    fn attention_item_matches_covers_query_branches() {
        let item = AttentionItemSnapshot::new(
            "plan:console-autonomous-mode",
            "plan",
            "medium",
            "Review plan thread console-autonomous-mode.",
            AttentionSourceRef::new(
                "livespec-orchestrator-beads-fabro",
                Some("bd-ib-ss7rkr"),
                Some("plan/console-autonomous-mode/"),
            ),
            AttentionHandoff::new("plan", None, "cmd"),
        );

        // No active search, and an empty query, both match.
        assert!(super::attention_item_matches(&item, None));
        assert!(super::attention_item_matches(&item, Some("")));
        // Each carried field can decide a match.
        assert!(super::attention_item_matches(
            &item,
            Some("review plan thread")
        ));
        assert!(super::attention_item_matches(&item, Some("bd-ib-ss7rkr")));
        assert!(super::attention_item_matches(&item, Some("orchestrator")));
        // No field contains the needle -> every arm is evaluated to false.
        assert!(!super::attention_item_matches(&item, Some("zzz-nomatch")));
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

        // SelectNextView clamps at the last view (Settings, now the sixth).
        let state = TuiInteractionState::for_view(TuiView::Settings, 0, TuiOverlay::None);
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectNextView);

        assert_eq!(state.active_view(), TuiView::Settings);
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

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::FocusDetail);
        assert_eq!(state.focus(), FocusPane::Detail);
        assert_eq!(
            build_tui_model_for_state(&events, &state).focus(),
            FocusPane::Detail
        );
    }

    #[test]
    fn tab_cycles_focus_through_every_pane_including_the_header() {
        // Scenario 20 case 1: `Tab` cycles focus around the whole pane ring —
        // Nav -> Content -> Detail -> Header -> Nav — so the top/header pane is
        // focusable like any other pane. `BackTab` walks the ring in reverse.
        let events = fabro_gate_events();
        let ring = [
            FocusPane::Nav,
            FocusPane::Content,
            FocusPane::Detail,
            FocusPane::Header,
        ];
        // Forward: Tab three times from Nav lands on the Header, then wraps to Nav.
        let mut state = TuiInteractionState::new(0, TuiOverlay::None);
        for expected in ring.iter().skip(1).chain(std::iter::once(&ring[0])) {
            state = reduce_tui_interaction(&state, &events, TuiInteraction::FocusNextPane);
            assert_eq!(state.focus(), *expected);
            assert_eq!(
                build_tui_model_for_state(&events, &state).focus(),
                *expected
            );
        }
        // Backward: BackTab from Nav walks the ring in reverse (Nav -> Header -> ...).
        let mut back = TuiInteractionState::new(0, TuiOverlay::None);
        for expected in ring
            .iter()
            .rev()
            .chain(std::iter::once(&ring[ring.len() - 1]))
        {
            back = reduce_tui_interaction(&back, &events, TuiInteraction::FocusPreviousPane);
            assert_eq!(back.focus(), *expected);
        }
    }

    #[test]
    fn tab_focus_ring_skips_the_detail_pane_on_a_view_without_one() {
        // The `Lanes` view draws no Detail pane, so the focus ring skips it:
        // Nav -> Content -> Header -> Nav (and the reverse).
        let events = fabro_gate_events();
        let mut state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None);
        for expected in [
            FocusPane::Content,
            FocusPane::Header,
            FocusPane::Nav,
            FocusPane::Content,
        ] {
            state = reduce_tui_interaction(&state, &events, TuiInteraction::FocusNextPane);
            assert_eq!(state.focus(), expected);
        }
        // Reverse from Nav: Nav -> Header -> Content -> Nav, still skipping Detail.
        let mut back = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None);
        for expected in [FocusPane::Header, FocusPane::Content, FocusPane::Nav] {
            back = reduce_tui_interaction(&back, &events, TuiInteraction::FocusPreviousPane);
            assert_eq!(back.focus(), expected);
        }
    }

    #[test]
    fn header_scroll_clamps_right_to_the_measured_max_and_saturates_left() {
        // Scenario 20 case 2: the render measures the header's overflow and the
        // loop feeds it back; ScrollHeaderRight advances by the fixed step and
        // clamps at that measured maximum, ScrollHeaderLeft saturates at the left
        // edge — so the clipped right-hand content is reachable and the pane can
        // return to its left-justified default.
        let events = fabro_gate_events();
        let max = 20usize;
        let state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_focus(FocusPane::Header)
            .with_header_max_scroll(max);

        // One Right press advances by exactly the fixed step.
        let one = reduce_tui_interaction(&state, &events, TuiInteraction::ScrollHeaderRight);
        assert_eq!(one.header_scroll(), HEADER_SCROLL_STEP);
        assert_eq!(
            build_tui_model_for_state(&events, &one).header_scroll(),
            HEADER_SCROLL_STEP
        );

        // Pressing Right past the end clamps at the render-measured max.
        let presses = max / HEADER_SCROLL_STEP + 3;
        let mut scrolled = state;
        for _ in 0..presses {
            scrolled =
                reduce_tui_interaction(&scrolled, &events, TuiInteraction::ScrollHeaderRight);
        }
        assert_eq!(scrolled.header_scroll(), max);
        assert_eq!(scrolled.header_max_scroll(), max);

        // Pressing Left past the start saturates at the left edge (offset 0).
        let mut unscrolled = scrolled;
        for _ in 0..presses {
            unscrolled =
                reduce_tui_interaction(&unscrolled, &events, TuiInteraction::ScrollHeaderLeft);
        }
        assert_eq!(unscrolled.header_scroll(), 0);
    }

    #[test]
    fn blur_resets_the_header_scroll_but_focusing_the_header_preserves_it() {
        // Scenario 20 case 3: `with_focus` is the single seam that snaps the
        // header back to its left-justified default on blur — a focus change to
        // ANY non-header pane zeroes the scroll — while a move that keeps the
        // header focused leaves the offset untouched.
        let scrolled = TuiInteractionState::new(0, TuiOverlay::None)
            .with_focus(FocusPane::Header)
            .with_header_scroll(12);
        assert_eq!(scrolled.header_scroll(), 12);
        for pane in [FocusPane::Nav, FocusPane::Content, FocusPane::Detail] {
            assert_eq!(scrolled.clone().with_focus(pane).header_scroll(), 0);
        }
        // A Header -> Header move keeps the offset (the reset guards on NON-header).
        assert_eq!(scrolled.with_focus(FocusPane::Header).header_scroll(), 12);
    }

    #[test]
    fn footer_shows_the_header_scroll_hints_while_the_header_is_focused() {
        // Scenario 19 seam extended for the header: a focused header (no overlay)
        // shows its own horizontal-scroll hints, distinct from any view pane's;
        // an open overlay still owns the hint line ahead of the header.
        let events = fabro_gate_events();
        let focused = TuiInteractionState::new(0, TuiOverlay::None).with_focus(FocusPane::Header);
        let model = build_tui_model_for_state(&events, &focused);
        assert!(model.footer().contains("scroll") && model.footer().contains("leave"));
        assert_ne!(
            model.footer(),
            footer_hint(
                model.active_view(),
                model.lane_focus(),
                model.selected_work_item_id().is_some(),
                &TuiOverlay::None,
            )
        );

        // An open overlay wins the hint line even while the header holds focus.
        let with_help = focused.with_overlay(TuiOverlay::Help {
            selected_section: 0,
            scroll: 0,
        });
        let help_model = build_tui_model_for_state(&events, &with_help);
        assert!(help_model.footer().contains("close help"));
    }

    #[test]
    fn open_help_from_the_focused_header_opens_the_header_section() {
        // Scenario 20 / B4 consistency: `?` while the header is focused opens Help
        // auto-focused to the header section, which is the LAST section (after
        // Global actions and every view pane).
        let events = fabro_gate_events();
        let state = TuiInteractionState::new(0, TuiOverlay::None).with_focus(FocusPane::Header);
        let opened = reduce_tui_interaction(&state, &events, TuiInteraction::OpenHelp);
        assert_eq!(
            opened.overlay(),
            &TuiOverlay::Help {
                selected_section: header_help_section(),
                scroll: 0,
            }
        );
        assert_eq!(header_help_section(), HELP_SECTION_COUNT - 1);
        // help_section_for_focus routes the header to its section and every other
        // pane to the active view's section.
        assert_eq!(
            help_section_for_focus(FocusPane::Header, TuiView::Attention),
            header_help_section()
        );
        assert_eq!(
            help_section_for_focus(FocusPane::Content, TuiView::Settings),
            help_section_for_view(TuiView::Settings)
        );
    }

    #[test]
    fn tui_interaction_scrolls_the_detail_pane_and_clamps() {
        let events = fabro_gate_events();
        // The renderer measures the Detail pane's wrapped max scroll and the loop
        // feeds it into the state; ScrollDetailDown clamps to exactly that offset
        // (the same wrapped count that sizes the scrollbar), so the true bottom of
        // a wrapping detail is reachable.
        let max = 5;
        let state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_focus(FocusPane::Detail)
            .with_detail_max_scroll(max);

        // Scrolling down past the end clamps the offset at the render-measured max,
        // and the model reflects the clamped offset.
        let mut scrolled = state;
        for _ in 0..(max + 3) {
            scrolled = reduce_tui_interaction(&scrolled, &events, TuiInteraction::ScrollDetailDown);
        }
        assert_eq!(scrolled.detail_scroll(), max);
        assert_eq!(
            build_tui_model_for_state(&events, &scrolled).detail_scroll(),
            max
        );

        // Scrolling up past the top saturates the offset at zero.
        let mut unscrolled = scrolled;
        for _ in 0..(max + 3) {
            unscrolled =
                reduce_tui_interaction(&unscrolled, &events, TuiInteraction::ScrollDetailUp);
        }
        assert_eq!(unscrolled.detail_scroll(), 0);
    }

    #[test]
    fn tui_interaction_resets_detail_scroll_when_selection_or_view_changes() {
        let events = fabro_gate_events();

        // Moving the content selection down resets the scroll to the top.
        let next = reduce_tui_interaction(
            &TuiInteractionState::new(0, TuiOverlay::None).with_detail_scroll(2),
            &events,
            TuiInteraction::SelectNext,
        );
        assert_eq!(next.selected_attention_index(), 1);
        assert_eq!(next.detail_scroll(), 0);

        // Moving up resets it too.
        let prev = reduce_tui_interaction(
            &TuiInteractionState::new(1, TuiOverlay::None).with_detail_scroll(2),
            &events,
            TuiInteraction::SelectPrevious,
        );
        assert_eq!(prev.selected_attention_index(), 0);
        assert_eq!(prev.detail_scroll(), 0);

        // Switching the active view (next then previous) resets it.
        let next_view = reduce_tui_interaction(
            &TuiInteractionState::new(0, TuiOverlay::None).with_detail_scroll(2),
            &events,
            TuiInteraction::SelectNextView,
        );
        assert_eq!(next_view.active_view(), TuiView::Spec);
        assert_eq!(next_view.detail_scroll(), 0);

        let prev_view = reduce_tui_interaction(
            &TuiInteractionState::for_view(TuiView::Spec, 0, TuiOverlay::None)
                .with_detail_scroll(2),
            &events,
            TuiInteraction::SelectPreviousView,
        );
        assert_eq!(prev_view.active_view(), TuiView::Attention);
        assert_eq!(prev_view.detail_scroll(), 0);
    }

    #[test]
    fn tui_interaction_opens_and_closes_the_help_overlay() {
        let events = fabro_gate_events();
        let state = TuiInteractionState::new(0, TuiOverlay::None);

        // OpenHelp auto-focuses the section for the active view. The default view
        // is Attention (view index 0), so its section is 1 (`Global actions` is 0).
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::OpenHelp);
        let model = build_tui_model_for_state(&events, &state);
        let expected = TuiOverlay::Help {
            selected_section: help_section_for_view(TuiView::Attention),
            scroll: 0,
        };
        assert_eq!(state.overlay(), &expected);
        assert_eq!(model.overlay(), &expected);
        assert_eq!(help_section_for_view(TuiView::Attention), 1);
        assert!(model.overlay().is_open());

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::CloseOverlay);
        assert_eq!(state.overlay(), &TuiOverlay::None);
    }

    #[test]
    fn open_help_auto_focuses_the_active_pane_section() {
        // `?` from the Settings pane opens auto-focused to the Settings section;
        // from Lanes, the Lanes section. Section order mirrors the nav; Settings
        // is the last VIEW section (the top/header pane owns the final section
        // after it, so this is `HELP_SECTION_COUNT - 2`, not `- 1`).
        let events = fabro_gate_events();
        for (view, expected_section) in [
            (TuiView::Attention, 1),
            (TuiView::Lanes, 3),
            (TuiView::Settings, help_section_for_view(TuiView::Settings)),
        ] {
            let state = TuiInteractionState::for_view(view, 0, TuiOverlay::None);
            let opened = reduce_tui_interaction(&state, &events, TuiInteraction::OpenHelp);
            let expected = TuiOverlay::Help {
                selected_section: expected_section,
                scroll: 0,
            };
            assert_eq!(opened.overlay(), &expected, "auto-focus from {view:?}");
        }
    }

    #[test]
    fn help_menu_navigation_changes_section_and_resets_scroll() {
        // Up/Down navigate the left menu, clamped at both ends; each move resets
        // the right-pane scroll so a new section starts at its top.
        let events = fabro_gate_events();
        // Open on Lanes (section 3), scroll the right pane down, then navigate.
        let opened = reduce_tui_interaction(
            &TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None),
            &events,
            TuiInteraction::OpenHelp,
        );
        let scrolled = reduce_tui_interaction(&opened, &events, TuiInteraction::HelpScrollDown);
        assert_eq!(
            scrolled.overlay(),
            &TuiOverlay::Help {
                selected_section: 3,
                scroll: 1,
            }
        );
        // Down moves to the next section AND resets the scroll to the top.
        let down =
            reduce_tui_interaction(&scrolled, &events, TuiInteraction::HelpSelectNextSection);
        assert_eq!(
            down.overlay(),
            &TuiOverlay::Help {
                selected_section: 4,
                scroll: 0,
            }
        );
        // Up moves back.
        let up = reduce_tui_interaction(&down, &events, TuiInteraction::HelpSelectPreviousSection);
        assert_eq!(
            up.overlay(),
            &TuiOverlay::Help {
                selected_section: 3,
                scroll: 0,
            }
        );
        // Down clamps at the last section.
        let mut clamped = up;
        for _step in 0..HELP_SECTION_COUNT + 2 {
            clamped =
                reduce_tui_interaction(&clamped, &events, TuiInteraction::HelpSelectNextSection);
        }
        assert_eq!(
            clamped.overlay(),
            &TuiOverlay::Help {
                selected_section: HELP_SECTION_COUNT - 1,
                scroll: 0,
            }
        );
        // Up clamps at the first section.
        let mut floored = clamped;
        for _step in 0..HELP_SECTION_COUNT + 2 {
            floored = reduce_tui_interaction(
                &floored,
                &events,
                TuiInteraction::HelpSelectPreviousSection,
            );
        }
        assert_eq!(
            floored.overlay(),
            &TuiOverlay::Help {
                selected_section: 0,
                scroll: 0,
            }
        );
    }

    #[test]
    fn help_scroll_saturates_at_the_top_and_leaves_section_untouched() {
        // HelpScrollUp at the top stays at 0; the selected section never moves.
        let events = fabro_gate_events();
        let opened = reduce_tui_interaction(
            &TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None),
            &events,
            TuiInteraction::OpenHelp,
        );
        let up = reduce_tui_interaction(&opened, &events, TuiInteraction::HelpScrollUp);
        assert_eq!(
            up.overlay(),
            &TuiOverlay::Help {
                selected_section: help_section_for_view(TuiView::Events),
                scroll: 0,
            }
        );
        let down = reduce_tui_interaction(&up, &events, TuiInteraction::HelpScrollDown);
        let down = reduce_tui_interaction(&down, &events, TuiInteraction::HelpScrollDown);
        assert_eq!(
            down.overlay(),
            &TuiOverlay::Help {
                selected_section: help_section_for_view(TuiView::Events),
                scroll: 2,
            }
        );
    }

    #[test]
    fn help_navigation_and_scroll_are_inert_without_the_help_overlay() {
        // The Help-specific interactions never mutate a non-Help overlay.
        let events = fabro_gate_events();
        let base = TuiInteractionState::new(0, TuiOverlay::None);
        for interaction in [
            TuiInteraction::HelpSelectNextSection,
            TuiInteraction::HelpSelectPreviousSection,
            TuiInteraction::HelpScrollDown,
            TuiInteraction::HelpScrollUp,
        ] {
            let stepped = reduce_tui_interaction(&base, &events, interaction);
            assert_eq!(stepped.overlay(), &TuiOverlay::None);
        }
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

        // Each non-attention view's lead row carries its operational count as the
        // whole title; the Spec and Events count rows carry NO baked-in
        // explanatory detail (B5 -- operational content only), while the Repos
        // row's detail is the live repo roster (operational, retained).
        for (view, expected_title, expected_detail) in [
            (TuiView::Spec, "LiveSpec next snapshots: 1", ""),
            (TuiView::Events, "Stored events: 8", ""),
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
    fn tui_events_view_latest_row_carries_operational_detail_only() {
        let events = view_summary_events();
        let state = TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None);
        let model = build_tui_model_for_state(&events, &state);

        // The Events view's second row is the live latest-event summary: an
        // operational detail (source event label / source / stream), never
        // baked-in explanatory prose.
        let latest = &model.view_items()[1];
        assert_eq!(latest.title(), "Latest event");
        assert!(!latest.detail().is_empty());
        assert!(!latest.detail().contains("canonical source"));
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
            selected_lane_item_index: None,
            focus: FocusPane::Nav,
            detail_scroll: 0,
            header_scroll: 0,
            overlay: TuiOverlay::CommandModal {
                selected_action_index: 0,
            },
            selected_repo: String::new(),
            selected_setting_index: None,
            dispatcher_settings: DispatcherSettingsRead::NotObserved,
            unavailable_sources: Vec::new(),
            header: "LiveSpec Console".to_owned(),
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
            selected_lane_item_index: None,
            focus: FocusPane::Nav,
            detail_scroll: 0,
            header_scroll: 0,
            overlay: TuiOverlay::CommandModal {
                selected_action_index: 0,
            },
            selected_repo: String::new(),
            selected_setting_index: None,
            dispatcher_settings: DispatcherSettingsRead::NotObserved,
            unavailable_sources: Vec::new(),
            header: String::new(),
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
    fn repo_id_reads_attention_item_repo_from_payload_not_stream_tail() {
        // The persisted needs-attention stream key embeds a colon-bearing item id
        // (`attention_item:{repo}:{id}` with `{id}` = `valve:set-admission:...`),
        // so the repo cannot be recovered from the stream tail. `repo_id` MUST
        // read the true repo from the item's own `source_ref.repo` in the
        // payload, even when the stream was keyed under a different repo.
        let item = AttentionItemSnapshot::new(
            "valve:set-admission:bd-ib-ss7rkr",
            "human-valve",
            "high",
            "Resolve human-needed block for work-item bd-ib-ss7rkr",
            AttentionSourceRef::new(
                "livespec-orchestrator-beads-fabro",
                Some("bd-ib-ss7rkr"),
                None,
            ),
            AttentionHandoff::new(
                "drive",
                Some("set-admission:bd-ib-ss7rkr:manual"),
                "drive ...",
            ),
        );
        let appeared = ConsoleEvent::new(
            "evt_attn_appeared".to_owned(),
            1,
            "needs-attention".to_owned(),
            EventType::AttentionItemAppeared,
            "needs-attention".to_owned(),
            "attention_item:livespec-console-beads-fabro:valve:set-admission:bd-ib-ss7rkr"
                .to_owned(),
            1,
        )
        .with_payload_json(attention_item_payload_json(&item));

        assert_eq!(
            super::repo_id(&appeared),
            "livespec-orchestrator-beads-fabro"
        );
    }

    #[test]
    fn repo_id_falls_back_across_stream_shapes() {
        // A `repo:{repo}` (or any `{context}:{repo}`) stream: the repo is the
        // segment after the FIRST colon, not the last.
        let pull = ConsoleEvent::new(
            "evt_pull".to_owned(),
            1,
            "factory".to_owned(),
            EventType::WorkItemSnapshotObserved,
            "orchestrator".to_owned(),
            "repo:livespec-orchestrator-beads-fabro".to_owned(),
            1,
        );
        assert_eq!(super::repo_id(&pull), "livespec-orchestrator-beads-fabro");

        // A stream key with no colon degrades to the whole key.
        let plain = ConsoleEvent::new(
            "evt_plain".to_owned(),
            1,
            "factory".to_owned(),
            EventType::WorkItemSnapshotObserved,
            "orchestrator".to_owned(),
            "livespec-orchestrator-beads-fabro".to_owned(),
            1,
        );
        assert_eq!(super::repo_id(&plain), "livespec-orchestrator-beads-fabro");

        // A `resolved` event carries only an id in its payload, so its repo comes
        // from the middle segment of the `attention_item:{repo}:{id}` stream key.
        let resolved = ConsoleEvent::new(
            "evt_resolved".to_owned(),
            1,
            "needs-attention".to_owned(),
            EventType::AttentionItemResolved,
            "needs-attention".to_owned(),
            "attention_item:livespec-orchestrator-beads-fabro:plan:console-autonomous-mode"
                .to_owned(),
            1,
        )
        .with_payload_json(attention_resolved_payload_json(
            "plan:console-autonomous-mode",
        ));
        assert_eq!(
            super::repo_id(&resolved),
            "livespec-orchestrator-beads-fabro"
        );

        // A malformed attention stream key (no middle segment) degrades to `-`.
        let malformed = ConsoleEvent::new(
            "evt_malformed".to_owned(),
            1,
            "needs-attention".to_owned(),
            EventType::AttentionItemResolved,
            "needs-attention".to_owned(),
            "attention_item".to_owned(),
            1,
        )
        .with_payload_json(attention_resolved_payload_json("x"));
        assert_eq!(super::repo_id(&malformed), "-");

        // An `appeared` event whose payload is not a complete item degrades to the
        // stream key's middle segment.
        let corrupt = ConsoleEvent::new(
            "evt_corrupt".to_owned(),
            1,
            "needs-attention".to_owned(),
            EventType::AttentionItemAppeared,
            "needs-attention".to_owned(),
            "attention_item:livespec-orchestrator-beads-fabro:spec:prune-history:SPECIFICATION"
                .to_owned(),
            1,
        )
        .with_payload_json("{}".to_owned());
        assert_eq!(
            super::repo_id(&corrupt),
            "livespec-orchestrator-beads-fabro"
        );
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
            EventType::SourceObservedFindingObserved.label(),
            "Source observed (idle)"
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
        let mut port = DispatcherFactoryDrainPort::new(&probe, "dispatcher", &["loop", "--json"]);

        let outcome = port.drain_ready_queue(&drain_request());

        assert_eq!(outcome, Ok(FactoryDrainPortOutcome::completed(3)));
    }

    #[test]
    fn dispatcher_drain_port_reports_zero_when_no_count() {
        let probe = StubDrainProbe {
            outcome: SourceProbeOutcome::observed("drain: ready queue empty", true),
        };
        let mut port = DispatcherFactoryDrainPort::new(&probe, "dispatcher", &["loop"]);

        let outcome = port.drain_ready_queue(&drain_request());

        assert_eq!(outcome, Ok(FactoryDrainPortOutcome::completed(0)));
    }

    #[test]
    fn dispatcher_drain_port_fails_on_non_zero_run() {
        let probe = StubDrainProbe {
            outcome: SourceProbeOutcome::observed("drain error", false),
        };
        let mut port = DispatcherFactoryDrainPort::new(&probe, "dispatcher", &["loop"]);

        let outcome = port.drain_ready_queue(&drain_request());

        assert_eq!(outcome, Ok(FactoryDrainPortOutcome::failed()));
    }

    #[test]
    fn dispatcher_drain_port_is_not_wired_when_unavailable() {
        let probe = StubDrainProbe {
            outcome: SourceProbeOutcome::unavailable("dispatcher binary not found"),
        };
        let mut port = DispatcherFactoryDrainPort::new(&probe, "dispatcher", &["loop"]);

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

    /// Probe for the drain-argv tests: `read_file` serves the configured
    /// `.livespec.jsonc` text; `run_command` records the drain args it was
    /// invoked with, so a test can assert exactly which flags ride the drain.
    struct ArgsRecordingDrainProbe {
        config: SourceProbeOutcome,
        drain: SourceProbeOutcome,
        observed_args: std::cell::RefCell<Vec<String>>,
    }

    impl SourceProbe for ArgsRecordingDrainProbe {
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

    #[test]
    fn dispatcher_drain_port_never_passes_a_mode_flag() {
        let probe = ArgsRecordingDrainProbe {
            // The strongest fixture for the invariant: the persistent
            // autonomous-mode permission key is ENABLED.
            config: SourceProbeOutcome::observed(AUTONOMOUS_ENABLED_CONFIG, true),
            drain: SourceProbeOutcome::observed("drain: dispatched 2 items", true),
            observed_args: std::cell::RefCell::new(Vec::new()),
        };
        assert_eq!(
            probe.read_file("cfg.jsonc"),
            SourceProbeOutcome::observed(AUTONOMOUS_ENABLED_CONFIG, true)
        );
        let mut port = DispatcherFactoryDrainPort::new(&probe, "dispatcher", &["loop"]);

        let outcome = port.drain_ready_queue(&drain_request());

        // Even with the permission armed, the drain passes NO `--mode` flag:
        // the Dispatcher owns its own mode. Every drain builds the same argv.
        assert_eq!(outcome, Ok(FactoryDrainPortOutcome::completed(2)));
        assert_eq!(*probe.observed_args.borrow(), ["loop", "--budget", "50"]);
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

    fn resolve_blocked_command() -> CommandEnvelope {
        CommandEnvelope::new(
            "cmd_resolve_blocked".to_owned(),
            CommandType::WorkItemResolveBlockedRequested,
            "wi-1".to_owned(),
            "wi-1:work_item.resolve_blocked_requested".to_owned(),
            "operator".to_owned(),
        )
    }

    #[test]
    fn resolve_blocked_handler_maps_each_target_onto_the_action_id() -> super::ApplicationResult<()>
    {
        for (payload, expected) in [
            (r#"{"target_status":"ready"}"#, "resolve-blocked:wi-1:ready"),
            (
                r#"{"target_status":"backlog"}"#,
                "resolve-blocked:wi-1:backlog",
            ),
        ] {
            let command = resolve_blocked_command();
            let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

            let outcome = handle_work_item_resolve_blocked_command(&command, payload, &mut port)?;

            assert_eq!(port.observed_action_ids, [expected]);
            assert_eq!(outcome.command_status(), "completed");
            for event in outcome.events() {
                assert_eq!(
                    event.payload_json(),
                    format!(r#"{{"action_id":"{expected}"}}"#)
                );
            }
        }
        Ok(())
    }

    #[test]
    fn resolve_blocked_handler_rejects_bad_targets_and_empty_ids_without_invoking_port() {
        // An absent, malformed, or out-of-range target is refused before the port.
        for payload in [
            r#"{"target_status":"active"}"#,
            r#"{"target_status":42}"#,
            "{}",
            "not json",
        ] {
            let command = resolve_blocked_command();
            let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
            assert_eq!(
                handle_work_item_resolve_blocked_command(&command, payload, &mut port),
                Err(ApplicationError::InvalidResolveBlockedTarget)
            );
            assert_eq!(port.observed_action_ids, [] as [String; 0]);
        }
        // An empty work-item id is refused before parsing the payload.
        let blank = CommandEnvelope::new(
            "cmd_resolve_blocked".to_owned(),
            CommandType::WorkItemResolveBlockedRequested,
            "   ".to_owned(),
            "blank:work_item.resolve_blocked_requested".to_owned(),
            "operator".to_owned(),
        );
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
        assert_eq!(
            handle_work_item_resolve_blocked_command(
                &blank,
                r#"{"target_status":"ready"}"#,
                &mut port
            ),
            Err(ApplicationError::EmptyWorkItemId)
        );
        assert_eq!(port.observed_action_ids, [] as [String; 0]);
    }

    fn move_command() -> CommandEnvelope {
        CommandEnvelope::new(
            "cmd_move".to_owned(),
            CommandType::WorkItemMoveRequested,
            "wi-1".to_owned(),
            "wi-1:work_item.move_requested".to_owned(),
            "operator".to_owned(),
        )
    }

    #[test]
    fn move_handler_maps_each_pre_terminal_target_onto_the_move_action_id()
    -> super::ApplicationResult<()> {
        for (payload, expected) in [
            (r#"{"target_status":"backlog"}"#, "move:wi-1:backlog"),
            (r#"{"target_status":"ready"}"#, "move:wi-1:ready"),
            (r#"{"target_status":"blocked"}"#, "move:wi-1:blocked"),
            (r#"{"target_status":"active"}"#, "move:wi-1:active"),
        ] {
            let command = move_command();
            let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
            let outcome = handle_work_item_move_command(&command, payload, &mut port)?;
            assert_eq!(port.observed_action_ids, [expected]);
            assert_eq!(outcome.command_status(), "completed");
        }
        Ok(())
    }

    #[test]
    fn move_handler_rejects_ship_guarded_and_malformed_targets_and_empty_ids() {
        // `done`/`acceptance`/`pending-approval` are the ship-guarded targets the
        // orchestrator refuses; a malformed or absent target is likewise refused,
        // all before the port is invoked.
        for payload in [
            r#"{"target_status":"done"}"#,
            r#"{"target_status":"acceptance"}"#,
            r#"{"target_status":"pending-approval"}"#,
            r#"{"target_status":42}"#,
            "{}",
            "not json",
        ] {
            let command = move_command();
            let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
            assert_eq!(
                handle_work_item_move_command(&command, payload, &mut port),
                Err(ApplicationError::InvalidMoveTarget)
            );
            assert_eq!(port.observed_action_ids, [] as [String; 0]);
        }
        let blank = CommandEnvelope::new(
            "cmd_move".to_owned(),
            CommandType::WorkItemMoveRequested,
            "   ".to_owned(),
            "blank:work_item.move_requested".to_owned(),
            "operator".to_owned(),
        );
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
        assert_eq!(
            handle_work_item_move_command(&blank, r#"{"target_status":"ready"}"#, &mut port),
            Err(ApplicationError::EmptyWorkItemId)
        );
        assert_eq!(port.observed_action_ids, [] as [String; 0]);
    }

    fn override_command() -> CommandEnvelope {
        CommandEnvelope::new(
            "cmd_override".to_owned(),
            CommandType::WorkItemSetDispatcherOverrideRequested,
            "wi-1".to_owned(),
            "wi-1:work_item.set_dispatcher_override_requested".to_owned(),
            "operator".to_owned(),
        )
    }

    #[test]
    fn dispatcher_override_handler_maps_each_cap_setting_and_clear_onto_its_action_id()
    -> super::ApplicationResult<()> {
        for (payload, expected) in [
            (
                r#"{"setting":"merge_on_review_cap","value":true}"#,
                "set-merge-on-review-cap:wi-1:true",
            ),
            (
                r#"{"setting":"merge_on_review_cap","value":false}"#,
                "set-merge-on-review-cap:wi-1:false",
            ),
            (
                r#"{"setting":"merge_on_review_cap","value":null}"#,
                "set-merge-on-review-cap:wi-1:clear",
            ),
            (
                r#"{"setting":"review_fix_cap","value":3}"#,
                "set-review-fix-cap:wi-1:3",
            ),
            (
                r#"{"setting":"review_fix_cap","value":null}"#,
                "set-review-fix-cap:wi-1:clear",
            ),
            (
                r#"{"setting":"acceptance_rework_cap","value":2}"#,
                "set-acceptance-rework-cap:wi-1:2",
            ),
            (
                r#"{"setting":"acceptance_rework_cap","value":null}"#,
                "set-acceptance-rework-cap:wi-1:clear",
            ),
        ] {
            let command = override_command();
            let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
            let outcome =
                handle_work_item_set_dispatcher_override_command(&command, payload, &mut port)?;
            assert_eq!(port.observed_action_ids, [expected]);
            assert_eq!(outcome.command_status(), "completed");
        }
        Ok(())
    }

    #[test]
    fn dispatcher_override_handler_rejects_non_overridable_settings_bad_values_and_empty_ids() {
        // `wip_cap` admits no per-item override; `auto_approve_ready` /
        // `acceptance_mode` are served by the policy dials; an unknown setting, a
        // wrong-typed value, and a non-positive int are all refused before the port.
        for payload in [
            r#"{"setting":"wip_cap","value":5}"#,
            r#"{"setting":"auto_approve_ready","value":true}"#,
            r#"{"setting":"acceptance_mode","value":"ai-only"}"#,
            r#"{"setting":"nonsense","value":1}"#,
            r#"{"setting":"merge_on_review_cap","value":3}"#,
            r#"{"setting":"review_fix_cap","value":true}"#,
            r#"{"setting":"review_fix_cap","value":0}"#,
            r#"{"value":1}"#,
            "not json",
        ] {
            let command = override_command();
            let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
            assert_eq!(
                handle_work_item_set_dispatcher_override_command(&command, payload, &mut port),
                Err(ApplicationError::InvalidDispatcherOverrideSetting)
            );
            assert_eq!(port.observed_action_ids, [] as [String; 0]);
        }
        let blank = CommandEnvelope::new(
            "cmd_override".to_owned(),
            CommandType::WorkItemSetDispatcherOverrideRequested,
            "   ".to_owned(),
            "blank:work_item.set_dispatcher_override_requested".to_owned(),
            "operator".to_owned(),
        );
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
        assert_eq!(
            handle_work_item_set_dispatcher_override_command(
                &blank,
                r#"{"setting":"review_fix_cap","value":3}"#,
                &mut port
            ),
            Err(ApplicationError::EmptyWorkItemId)
        );
        assert_eq!(port.observed_action_ids, [] as [String; 0]);
    }

    #[test]
    fn dispatcher_override_valve_outcome_carries_the_setting_and_value_payload() {
        // The valve builds a persist-with-payload outcome the handler reads back.
        let outcome = work_item_override_outcome(
            "wi-1",
            DispatcherOverride::AcceptanceReworkCap(OverrideInt::Value(4)),
            "operator",
        );
        assert!(matches!(
            &outcome,
            OperatorActionOutcome::PersistCommandWithPayload { command, payload_json }
                if command.command_type() == &CommandType::WorkItemSetDispatcherOverrideRequested
                    && payload_json
                        == r#"{"setting":"acceptance_rework_cap","value":4}"#
        ));
        // A cleared override serializes its value as JSON null.
        let cleared = work_item_override_outcome(
            "wi-1",
            DispatcherOverride::MergeOnReviewCap(OverrideBool::Clear),
            "operator",
        );
        assert!(matches!(
            &cleared,
            OperatorActionOutcome::PersistCommandWithPayload { payload_json, .. }
                if payload_json == r#"{"setting":"merge_on_review_cap","value":null}"#
        ));
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
    // Configuration context — dispatcher-settings read/write through the API.
    // -----------------------------------------------------------------------

    fn dispatcher_setting_set_command() -> CommandEnvelope {
        CommandEnvelope::new(
            "cmd_config_dispatcher_setting_set".to_owned(),
            CommandType::ConfigDispatcherSettingSet,
            "livespec-console-beads-fabro".to_owned(),
            "livespec-console-beads-fabro:config.dispatcher_setting_set".to_owned(),
            "operator".to_owned(),
        )
    }

    fn event_types(outcome: &ConfigCommandOutcome) -> Vec<EventType> {
        outcome
            .events()
            .iter()
            .map(|event| *event.event_type())
            .collect()
    }

    /// A `config` read payload as the orchestrator emits it under `--json`, with
    /// all six settings at explicit non-default values.
    const CONFIG_READ_JSON: &str = r#"{
      "action_id": "config",
      "kind": "config-read",
      "status": "green",
      "settings": [
        { "key": "auto_approve_ready", "value": true, "source": "explicit" },
        { "key": "merge_on_review_cap", "value": false, "source": "default" },
        { "key": "acceptance_mode", "value": "ai-only", "source": "explicit" },
        { "key": "review_fix_cap", "value": 4, "source": "explicit" },
        { "key": "acceptance_rework_cap", "value": 2, "source": "default" },
        { "key": "wip_cap", "value": 9, "source": "explicit" }
      ],
      "summary": "Read effective dispatcher settings."
    }"#;

    /// A `config` read at the default values, for asserting a change's `previous`
    /// field (here `auto_approve_ready` is off, so enabling it records a
    /// `false -> true` change).
    const CONFIG_READ_JSON_DEFAULTS: &str = r#"{
      "settings": [
        { "key": "auto_approve_ready", "value": false, "source": "default" },
        { "key": "merge_on_review_cap", "value": false, "source": "default" },
        { "key": "acceptance_mode", "value": "ai-then-human", "source": "default" },
        { "key": "review_fix_cap", "value": 3, "source": "default" },
        { "key": "acceptance_rework_cap", "value": 2, "source": "default" },
        { "key": "wip_cap", "value": 5, "source": "default" }
      ]
    }"#;

    #[test]
    fn dispatcher_setting_event_labels_are_present() {
        assert_eq!(
            EventType::ConfigDispatcherSettingChanged.label(),
            "Dispatcher setting changed"
        );
        assert_eq!(
            EventType::ConfigDispatcherSettingNotWired.label(),
            "Dispatcher setting not wired"
        );
    }

    #[test]
    fn dispatcher_setting_set_request_exposes_its_fields() {
        let request = DispatcherSettingSetRequest::new(
            "repo-a".to_owned(),
            DispatcherSettingWrite::AutoApproveReady(true),
        );
        assert_eq!(request.repo(), "repo-a");
        assert_eq!(
            request.write(),
            &DispatcherSettingWrite::AutoApproveReady(true)
        );
    }

    #[test]
    fn dispatcher_setting_set_request_parses_each_setting_type() {
        assert_eq!(
            DispatcherSettingSetRequest::from_payload_json(
                r#"{"repo":"repo-a","setting":"auto_approve_ready","value":true}"#
            ),
            Ok(DispatcherSettingSetRequest::new(
                "repo-a".to_owned(),
                DispatcherSettingWrite::AutoApproveReady(true)
            ))
        );
        assert_eq!(
            DispatcherSettingSetRequest::from_payload_json(
                r#"{"repo":"repo-a","setting":"merge_on_review_cap","value":true}"#
            ),
            Ok(DispatcherSettingSetRequest::new(
                "repo-a".to_owned(),
                DispatcherSettingWrite::MergeOnReviewCap(true)
            ))
        );
        assert_eq!(
            DispatcherSettingSetRequest::from_payload_json(
                r#"{"repo":"repo-a","setting":"acceptance_mode","value":"ai-only"}"#
            ),
            Ok(DispatcherSettingSetRequest::new(
                "repo-a".to_owned(),
                DispatcherSettingWrite::AcceptanceMode(AcceptancePolicy::AiOnly)
            ))
        );
        assert_eq!(
            DispatcherSettingSetRequest::from_payload_json(
                r#"{"repo":"repo-a","setting":"acceptance_rework_cap","value":2}"#
            ),
            Ok(DispatcherSettingSetRequest::new(
                "repo-a".to_owned(),
                DispatcherSettingWrite::AcceptanceReworkCap(2)
            ))
        );
        assert_eq!(
            DispatcherSettingSetRequest::from_payload_json(
                r#"{"repo":"repo-a","setting":"wip_cap","value":5}"#
            ),
            Ok(DispatcherSettingSetRequest::new(
                "repo-a".to_owned(),
                DispatcherSettingWrite::WipCap(5)
            ))
        );
    }

    #[test]
    fn dispatcher_setting_set_request_rejects_malformed_unknown_or_mistyped_payloads() {
        for payload in [
            "not json",
            r#"{"setting":"wip_cap","value":5}"#,
            r#"{"repo":"  ","setting":"wip_cap","value":5}"#,
            r#"{"repo":"repo-a","value":5}"#,
            r#"{"repo":"repo-a","setting":"wip_cap"}"#,
            r#"{"repo":"repo-a","setting":"unknown_key","value":5}"#,
            r#"{"repo":"repo-a","setting":"auto_approve_ready","value":5}"#,
            r#"{"repo":"repo-a","setting":"wip_cap","value":"five"}"#,
            r#"{"repo":"repo-a","setting":"wip_cap","value":-1}"#,
            r#"{"repo":"repo-a","setting":"acceptance_mode","value":"bogus"}"#,
        ] {
            assert_eq!(
                DispatcherSettingSetRequest::from_payload_json(payload),
                Err(ApplicationError::InvalidDispatcherSettingPayload)
            );
        }
    }

    // ---- The dispatcher-settings port: read + write through the API. ----

    /// Build a settings port over the real `DispatcherOrchestratorActionPort`
    /// wired to `probe`, targeting a fixed orchestrator repo with `--json`.
    fn drive_over(probe: &ArgRecordingProbe) -> DispatcherOrchestratorActionPort<'_> {
        DispatcherOrchestratorActionPort::new(probe, "drive.py", &["--repo", "/orch", "--json"])
    }

    #[test]
    fn dispatcher_settings_exposes_each_effective_value() {
        let settings = DispatcherSettings::new(true, false, AcceptancePolicy::HumanOnly, 4, 2, 9);
        assert!(settings.auto_approve_ready());
        assert!(!settings.merge_on_review_cap());
        assert_eq!(settings.acceptance_mode(), AcceptancePolicy::HumanOnly);
        assert_eq!(settings.review_fix_cap(), 4);
        assert_eq!(settings.acceptance_rework_cap(), 2);
        assert_eq!(settings.wip_cap(), 9);
    }

    #[test]
    fn settings_port_reads_all_six_effective_values_through_the_config_action() {
        let probe = ArgRecordingProbe {
            outcome: SourceProbeOutcome::observed(CONFIG_READ_JSON, true),
            observed_args: RefCell::new(Vec::new()),
        };
        let mut drive = drive_over(&probe);
        let mut settings = DispatcherSettingsPort::new(&mut drive);

        let read = settings.read_settings();

        assert_eq!(
            read,
            Ok(DispatcherSettingsRead::Observed(DispatcherSettings::new(
                true,
                false,
                AcceptancePolicy::AiOnly,
                4,
                2,
                9,
            )))
        );
        // The read rode the `config` action-id, nothing more.
        assert_eq!(
            *probe.observed_args.borrow(),
            [
                "drive.py", "--repo", "/orch", "--json", "--action", "config"
            ]
        );
    }

    #[test]
    fn settings_read_defaults_to_not_observed_without_a_read_surface() {
        // A port that does not override `read_action` uses the trait default --
        // an honest not-wired reading -- so the settings read is not-observed.
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
        let mut settings = DispatcherSettingsPort::new(&mut port);

        assert_eq!(
            settings.read_settings(),
            Ok(DispatcherSettingsRead::NotObserved)
        );
    }

    #[test]
    fn settings_read_is_not_observed_when_the_action_is_not_wired() {
        let probe = ArgRecordingProbe {
            outcome: SourceProbeOutcome::unavailable("drive.py not found"),
            observed_args: RefCell::new(Vec::new()),
        };
        let mut drive = drive_over(&probe);
        let mut settings = DispatcherSettingsPort::new(&mut drive);

        assert_eq!(
            settings.read_settings(),
            Ok(DispatcherSettingsRead::NotObserved)
        );
    }

    #[test]
    fn settings_read_is_not_observed_when_the_action_reports_failure() {
        let probe = ArgRecordingProbe {
            outcome: SourceProbeOutcome::observed("boom", false),
            observed_args: RefCell::new(Vec::new()),
        };
        let mut drive = drive_over(&probe);
        let mut settings = DispatcherSettingsPort::new(&mut drive);

        assert_eq!(
            settings.read_settings(),
            Ok(DispatcherSettingsRead::NotObserved)
        );
    }

    #[test]
    fn settings_read_is_not_observed_when_the_payload_is_unparseable() {
        let probe = ArgRecordingProbe {
            outcome: SourceProbeOutcome::observed("not json", true),
            observed_args: RefCell::new(Vec::new()),
        };
        let mut drive = drive_over(&probe);
        let mut settings = DispatcherSettingsPort::new(&mut drive);

        assert_eq!(
            settings.read_settings(),
            Ok(DispatcherSettingsRead::NotObserved)
        );
    }

    #[test]
    fn settings_read_is_not_observed_when_a_declared_key_is_absent_or_mistyped() {
        // Missing `wip_cap`, and `review_fix_cap` is a string rather than an int:
        // an untrustworthy read degrades to not-observed rather than an assumed
        // value.
        let partial = r#"{
          "settings": [
            { "key": "auto_approve_ready", "value": true },
            { "key": "merge_on_review_cap", "value": false },
            { "key": "acceptance_mode", "value": "ai-only" },
            { "key": "review_fix_cap", "value": "three" },
            { "key": "acceptance_rework_cap", "value": 2 }
          ]
        }"#;
        let probe = ArgRecordingProbe {
            outcome: SourceProbeOutcome::observed(partial, true),
            observed_args: RefCell::new(Vec::new()),
        };
        let mut drive = drive_over(&probe);
        let mut settings = DispatcherSettingsPort::new(&mut drive);

        assert_eq!(
            settings.read_settings(),
            Ok(DispatcherSettingsRead::NotObserved)
        );
    }

    #[test]
    fn settings_write_builds_the_set_config_action_id_for_each_setting() {
        let cases = [
            (
                DispatcherSettingWrite::AutoApproveReady(true),
                "set-config:auto_approve_ready:true",
            ),
            (
                DispatcherSettingWrite::MergeOnReviewCap(false),
                "set-config:merge_on_review_cap:false",
            ),
            (
                DispatcherSettingWrite::AcceptanceMode(AcceptancePolicy::HumanOnly),
                "set-config:acceptance_mode:human-only",
            ),
            (
                DispatcherSettingWrite::ReviewFixCap(4),
                "set-config:review_fix_cap:4",
            ),
            (
                DispatcherSettingWrite::AcceptanceReworkCap(2),
                "set-config:acceptance_rework_cap:2",
            ),
            (DispatcherSettingWrite::WipCap(5), "set-config:wip_cap:5"),
        ];
        for (write, expected_action_id) in cases {
            let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
            let mut settings = DispatcherSettingsPort::new(&mut port);

            let outcome = settings.write_setting(&write);

            assert_eq!(outcome, Ok(OrchestratorActionOutcome::completed()));
            assert_eq!(port.observed_action_ids, [expected_action_id]);
        }
    }

    #[test]
    fn settings_write_issues_the_orchestrator_action_through_the_read_only_probe() {
        // The port's `SourceProbe` is READ-ONLY by construction (it exposes no
        // write capability), so a setting write can only ride `run_command` --
        // the console writes `.livespec.jsonc` (or any file) NOWHERE itself.
        let probe = ArgRecordingProbe {
            outcome: SourceProbeOutcome::observed("{}", true),
            observed_args: RefCell::new(Vec::new()),
        };
        let mut drive = drive_over(&probe);
        let mut settings = DispatcherSettingsPort::new(&mut drive);

        let outcome = settings.write_setting(&DispatcherSettingWrite::WipCap(7));

        assert_eq!(outcome, Ok(OrchestratorActionOutcome::completed()));
        assert_eq!(
            *probe.observed_args.borrow(),
            [
                "drive.py",
                "--repo",
                "/orch",
                "--json",
                "--action",
                "set-config:wip_cap:7"
            ]
        );
    }

    // ---- The `config.dispatcher_setting_set` handler. ----

    /// A mock action port whose `config` read returns a fixed observed settings
    /// payload and whose writes return a fixed outcome, recording every WRITE
    /// action-id. The read rides `read_action` (which the default port leaves
    /// not-wired), so this exercises the handler's observed-previous path.
    struct ObservedReadRecordingPort {
        read_stdout: String,
        write_outcome: OrchestratorActionOutcome,
        observed_action_ids: Vec<String>,
    }

    impl OrchestratorActionPort for ObservedReadRecordingPort {
        fn run_action(
            &mut self,
            request: &OrchestratorActionRequest,
        ) -> super::ApplicationResult<OrchestratorActionOutcome> {
            self.observed_action_ids
                .push(request.action_id().to_owned());
            Ok(self.write_outcome.clone())
        }

        fn read_action(
            &mut self,
            _request: &OrchestratorActionRequest,
        ) -> super::ApplicationResult<super::OrchestratorActionReading> {
            Ok(super::OrchestratorActionReading::observed(
                self.read_stdout.clone(),
            ))
        }
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

    /// The parsed payload of the config outcome's second event (the changed /
    /// not-wired outcome event at index 1), or `Null` when absent.
    fn audit_payload(
        outcome: &super::ApplicationResult<ConfigCommandOutcome>,
    ) -> serde_json::Value {
        outcome
            .as_ref()
            .ok()
            .and_then(|handled| handled.events().get(1))
            .map(|event| serde_json::from_str(event.payload_json()).unwrap_or_default())
            .unwrap_or_default()
    }

    #[test]
    fn previous_setting_value_json_reads_each_targeted_field() {
        let settings = DispatcherSettings::new(true, false, AcceptancePolicy::HumanOnly, 4, 2, 9);
        assert_eq!(
            super::previous_setting_value_json(
                &settings,
                &DispatcherSettingWrite::AutoApproveReady(false)
            ),
            serde_json::json!(true)
        );
        assert_eq!(
            super::previous_setting_value_json(
                &settings,
                &DispatcherSettingWrite::MergeOnReviewCap(true)
            ),
            serde_json::json!(false)
        );
        assert_eq!(
            super::previous_setting_value_json(
                &settings,
                &DispatcherSettingWrite::AcceptanceMode(AcceptancePolicy::AiOnly)
            ),
            serde_json::json!("human-only")
        );
        assert_eq!(
            super::previous_setting_value_json(&settings, &DispatcherSettingWrite::ReviewFixCap(5)),
            serde_json::json!(4)
        );
        assert_eq!(
            super::previous_setting_value_json(
                &settings,
                &DispatcherSettingWrite::AcceptanceReworkCap(5)
            ),
            serde_json::json!(2)
        );
        assert_eq!(
            super::previous_setting_value_json(&settings, &DispatcherSettingWrite::WipCap(5)),
            serde_json::json!(9)
        );
    }

    #[test]
    fn config_handler_writes_a_setting_and_audits_the_change_with_the_previous_value() {
        // The read reports auto_approve_ready currently false, so the changed
        // event's `previous` is the observed value and `new` is the write's value.
        let mut port = ObservedReadRecordingPort {
            read_stdout: CONFIG_READ_JSON_DEFAULTS.to_owned(),
            write_outcome: OrchestratorActionOutcome::completed(),
            observed_action_ids: Vec::new(),
        };
        let mut settings = DispatcherSettingsPort::new(&mut port);
        let outcome = handle_config_dispatcher_setting_set_command(
            &dispatcher_setting_set_command(),
            r#"{"repo":"repo-a","setting":"auto_approve_ready","value":true}"#,
            "2026-07-11T00:00:00Z",
            &mut settings,
        );

        assert_eq!(
            outcome.as_ref().map(ConfigCommandOutcome::command_status),
            Ok("completed")
        );
        // Two events, both in the configuration context: the acceptance and the
        // durable change audit -- no arming ceremony, no factory event.
        assert_eq!(
            outcome.as_ref().map(event_types),
            Ok(vec![
                EventType::CommandAccepted,
                EventType::ConfigDispatcherSettingChanged,
            ])
        );
        assert_eq!(event_contexts(&outcome), ["command", "configuration"]);
        let payload = audit_payload(&outcome);
        assert_eq!(payload["repo"], "repo-a");
        assert_eq!(payload["setting"], "auto_approve_ready");
        assert_eq!(payload["previous"], serde_json::json!(false));
        assert_eq!(payload["new"], serde_json::json!(true));
        assert_eq!(payload["actor"], "operator");
        assert_eq!(payload["occurred_at"], "2026-07-11T00:00:00Z");
        // The change was effected through the orchestrator's `set-config` action.
        assert_eq!(
            port.observed_action_ids,
            ["set-config:auto_approve_ready:true"]
        );
    }

    #[test]
    fn config_handler_records_a_null_previous_when_the_read_surface_is_not_observed() {
        // The default RecordingActionPort leaves `read_action` not-wired, so the
        // handler records `previous: null` rather than fabricating a value.
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
        let mut settings = DispatcherSettingsPort::new(&mut port);
        let outcome = handle_config_dispatcher_setting_set_command(
            &dispatcher_setting_set_command(),
            r#"{"repo":"repo-a","setting":"wip_cap","value":5}"#,
            "2026-07-11T00:00:01Z",
            &mut settings,
        );

        assert_eq!(
            outcome.as_ref().map(ConfigCommandOutcome::command_status),
            Ok("completed")
        );
        let payload = audit_payload(&outcome);
        assert_eq!(payload["setting"], "wip_cap");
        assert_eq!(payload["previous"], serde_json::Value::Null);
        assert_eq!(payload["new"], serde_json::json!(5));
        assert_eq!(port.observed_action_ids, ["set-config:wip_cap:5"]);
    }

    #[test]
    fn config_handler_surfaces_not_wired_without_a_changed_event() {
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::not_wired());
        let mut settings = DispatcherSettingsPort::new(&mut port);
        let outcome = handle_config_dispatcher_setting_set_command(
            &dispatcher_setting_set_command(),
            r#"{"repo":"repo-a","setting":"auto_approve_ready","value":true}"#,
            "2026-07-11T00:00:02Z",
            &mut settings,
        );

        assert_eq!(
            outcome.as_ref().map(ConfigCommandOutcome::command_status),
            Ok("not_wired")
        );
        // The honest not-wired outcome, and NO changed event.
        assert_eq!(
            outcome.as_ref().map(event_types),
            Ok(vec![
                EventType::CommandAccepted,
                EventType::ConfigDispatcherSettingNotWired,
            ])
        );
        assert_eq!(event_contexts(&outcome), ["command", "configuration"]);
        assert_eq!(audit_payload(&outcome)["setting"], "auto_approve_ready");
    }

    #[test]
    fn config_handler_surfaces_not_wired_when_the_action_fails() {
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::failed());
        let mut settings = DispatcherSettingsPort::new(&mut port);
        let outcome = handle_config_dispatcher_setting_set_command(
            &dispatcher_setting_set_command(),
            r#"{"repo":"repo-a","setting":"review_fix_cap","value":3}"#,
            "2026-07-11T00:00:02Z",
            &mut settings,
        );

        assert_eq!(
            outcome.as_ref().map(ConfigCommandOutcome::command_status),
            Ok("not_wired")
        );
        assert_eq!(
            outcome.as_ref().map(event_types),
            Ok(vec![
                EventType::CommandAccepted,
                EventType::ConfigDispatcherSettingNotWired,
            ])
        );
    }

    #[test]
    fn config_handler_rejects_a_malformed_payload() {
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
        let mut settings = DispatcherSettingsPort::new(&mut port);
        let outcome = handle_config_dispatcher_setting_set_command(
            &dispatcher_setting_set_command(),
            "not json",
            "2026-07-11T00:00:03Z",
            &mut settings,
        );

        assert_eq!(
            outcome,
            Err(ApplicationError::InvalidDispatcherSettingPayload)
        );
        assert!(port.observed_action_ids.is_empty());
    }
    // -----------------------------------------------------------------------
    // TUI autonomous-mode surface (C3 slice 2): toggle, type-to-confirm modal,
    // dangerous label, and header indicator for the selected repo.
    // -----------------------------------------------------------------------

    const CONFIRM_REPO: &str = "livespec-console-beads-fabro";

    /// A model over the given overlay whose selected repo is `selected_repo`,
    /// built with no events (no attention items).
    fn repo_model(overlay: TuiOverlay, selected_repo: &str) -> TuiScreenModel {
        let state =
            TuiInteractionState::new(0, overlay).with_selected_repo(selected_repo.to_owned());
        build_tui_model_for_state(&[], &state)
    }

    #[test]
    fn header_reflects_the_selected_repo_and_carries_no_autonomous_segment() {
        let model = repo_model(TuiOverlay::None, CONFIRM_REPO);
        assert_eq!(model.selected_repo(), CONFIRM_REPO);
        assert!(model.header().contains(&format!("repo: {CONFIRM_REPO}")));
        // The retired arming surface left no `autonomous:` header segment.
        assert!(!model.header().contains("autonomous:"));
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

    #[test]
    fn unavailable_tally_clears_a_source_recovered_on_a_later_observation() {
        // A source that degraded to not-observed on an earlier cycle clears from
        // the tally when a LATER cycle observes it -- whether it recovers to an
        // observed-and-idle marker or to a data snapshot. The tally reflects the
        // LATEST poll outcome per source, so a transient failure is never
        // branded permanently.
        let recovered_to_idle = [
            ConsoleEvent::fixture(
                "evt_orch_not_observed",
                EventType::SourceNotObservedFindingObserved,
                "orchestrator",
            ),
            ConsoleEvent::fixture(
                "evt_orch_observed_idle",
                EventType::SourceObservedFindingObserved,
                "orchestrator",
            ),
        ];
        assert!(
            build_tui_model(&recovered_to_idle, 0)
                .unavailable_sources()
                .is_empty()
        );

        let recovered_to_data = [
            ConsoleEvent::fixture(
                "evt_orch_not_observed",
                EventType::SourceNotObservedFindingObserved,
                "orchestrator",
            ),
            ConsoleEvent::fixture(
                "evt_orch_snapshot",
                EventType::WorkItemSnapshotObserved,
                "orchestrator",
            ),
        ];
        assert!(
            build_tui_model(&recovered_to_data, 0)
                .unavailable_sources()
                .is_empty()
        );
    }

    #[test]
    fn unavailable_tally_reflects_the_latest_outcome_per_source() {
        // One source recovers, another degrades AFTER a prior observation, and a
        // third re-degrades after recovering: the tally is exactly the sources
        // whose MOST RECENT observation was not-observed, in sorted order.
        let events = [
            // github: observed then degraded -> unavailable.
            ConsoleEvent::fixture(
                "evt_github_idle",
                EventType::SourceObservedFindingObserved,
                "github",
            ),
            ConsoleEvent::fixture(
                "evt_github_not_observed",
                EventType::SourceNotObservedFindingObserved,
                "github",
            ),
            // orchestrator: degraded then recovered -> cleared.
            ConsoleEvent::fixture(
                "evt_orch_not_observed",
                EventType::SourceNotObservedFindingObserved,
                "orchestrator",
            ),
            ConsoleEvent::fixture(
                "evt_orch_idle",
                EventType::SourceObservedFindingObserved,
                "orchestrator",
            ),
            // fabro: never degraded -> never in the tally.
            ConsoleEvent::fixture(
                "evt_fabro_idle",
                EventType::SourceObservedFindingObserved,
                "fabro",
            ),
        ];
        assert_eq!(
            build_tui_model(&events, 0).unavailable_sources(),
            ["github".to_owned()]
        );
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
        let line = model.header_line(112);
        assert!(line.chars().count() <= 112);
        assert!(line.contains("+2 more"));
        assert!(line.contains(&format!("repo: {CONFIRM_REPO}")));
        assert!(line.contains("attention: 0"));
    }

    #[test]
    fn header_line_never_drops_the_source_count_or_repo() {
        // Even on an absurdly narrow terminal (below the target), the header keeps
        // the source count (the blind-vs-idle tell) and the repo field; only
        // lower-value fields and the source names are shed.
        let model = blind_model(CONFIRM_REPO, &["fabro", "github", "orchestrator"]);
        let line = model.header_line(60);
        assert!(line.contains("sources: 3 unavailable"));
        assert!(line.contains(&format!("repo: {CONFIRM_REPO}")));
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
    fn footer_presents_the_settings_edit_shortcut() {
        // The Settings pane's Status-line hints surface its edit key. Built as a
        // real model (Settings is the last view, reached by clamping
        // SelectNextView) so this exercises the `footer()` accessor end to end,
        // not just the free `footer_hint`.
        let mut state = TuiInteractionState::new(0, TuiOverlay::None);
        for _step in 0..TuiView::all().len() {
            state = reduce_tui_interaction(&state, &[], TuiInteraction::SelectNextView);
        }
        let model = build_tui_model_for_state(&[], &state);
        assert_eq!(model.active_view(), TuiView::Settings);
        assert!(model.footer().contains("enter/space edit row"));
    }

    #[test]
    fn footer_hint_is_non_empty_and_context_specific_for_every_focused_pane() {
        // Scenario 19 case 1 + case 4: every focused pane renders a non-empty,
        // context-appropriate hint line -- never a blank one where actions are
        // available -- and the actionable panes surface their distinct keys.
        for view in TuiView::all() {
            assert!(
                !footer_hint(*view, LaneFocus::Overview, true, &TuiOverlay::None)
                    .trim()
                    .is_empty()
            );
        }
        assert!(
            footer_hint(
                TuiView::Attention,
                LaneFocus::Overview,
                true,
                &TuiOverlay::None
            )
            .contains("approve/accept/reject")
        );
        assert!(
            footer_hint(
                TuiView::Lanes,
                LaneFocus::Lane(Lane::Ready),
                true,
                &TuiOverlay::None
            )
            .contains("move-status")
        );
        assert!(
            footer_hint(
                TuiView::Settings,
                LaneFocus::Overview,
                true,
                &TuiOverlay::None
            )
            .contains("enter/space edit row")
        );
        // The read-only nav views surface select + focus-move + search.
        for view in [TuiView::Spec, TuiView::Events, TuiView::Repos] {
            let hint = footer_hint(view, LaneFocus::Overview, true, &TuiOverlay::None);
            assert!(hint.contains("left/right focus") && hint.contains("search"));
        }
    }

    #[test]
    fn footer_hint_changes_when_focus_moves_to_a_different_pane() {
        // Scenario 19 case 2: moving focus from Lanes to Settings changes the
        // hints to that pane's actions, and the two panes' hints DIFFER (their
        // action sets genuinely differ: status-move/valves vs. edit).
        let lanes = footer_hint(
            TuiView::Lanes,
            LaneFocus::Lane(Lane::Ready),
            true,
            &TuiOverlay::None,
        );
        let settings = footer_hint(
            TuiView::Settings,
            LaneFocus::Overview,
            true,
            &TuiOverlay::None,
        );
        assert_ne!(lanes, settings);
        assert!(lanes.contains("move-status") && !lanes.contains("edit row"));
        assert!(settings.contains("edit row") && !settings.contains("move-status"));
    }

    #[test]
    fn footer_hint_reflects_the_open_overlay_and_restores_the_pane_on_close() {
        // Scenario 19 case 3: opening an overlay replaces the focused pane's
        // hints with that overlay's, and closing it (overlay back to None)
        // restores the pane's hints. Exercised against the Lanes pane so the
        // restore is observable via its distinctive `move-status` key.
        let pane = footer_hint(TuiView::Lanes, LaneFocus::Overview, true, &TuiOverlay::None);
        let help = footer_hint(
            TuiView::Lanes,
            LaneFocus::Overview,
            true,
            &TuiOverlay::Help {
                selected_section: help_section_for_view(TuiView::Lanes),
                scroll: 0,
            },
        );
        assert_ne!(pane, help);
        assert!(help.contains("close help") && !help.contains("move-status"));
        // Closing the overlay restores the underlying pane's hints verbatim.
        assert_eq!(
            footer_hint(TuiView::Lanes, LaneFocus::Overview, true, &TuiOverlay::None),
            pane
        );
    }

    #[test]
    fn the_lane_board_carries_each_item_standardized_record() {
        // The board is where the detail modal reads an item's record from, so
        // the descriptive half must survive projection alongside the lifecycle
        // half -- not be dropped between the snapshot and the lane column.
        let payload = concat!(
            r#"{"repo":"console","work_item_id":"console-rec","lane":"ready","#,
            r#""lane_reason":null,"rank":"a1","status":"ready","source_version":1,"#,
            r#""detail":{"title":"A readable title","description":"body text","#,
            r#""item_type":"bug","depends_on":["console-dep"]}}"#,
        );
        let events = [ConsoleEvent::fixture(
            "evt_rec",
            EventType::WorkItemSnapshotObserved,
            "orchestrator",
        )
        .with_payload_json(payload.to_owned())];
        let board = project_lane_board(&events);
        let items = board
            .column(Lane::Ready)
            .map(super::LaneColumn::items)
            .unwrap_or_default();
        assert_eq!(items.len(), 1);
        let detail = items[0].detail();
        assert_eq!(detail.title.as_deref(), Some("A readable title"));
        assert_eq!(detail.description.as_deref(), Some("body text"));
        assert_eq!(detail.item_type.as_deref(), Some("bug"));
        assert_eq!(detail.depends_on, vec!["console-dep".to_owned()]);
    }

    #[test]
    fn the_status_hint_distinguishes_the_lane_overview_from_a_drilled_in_lane() {
        const MODAL_ITEM: &str = "console-pinned";
        // Enter drills into a LANE from the overview but opens an ITEM inside a
        // drilled-in lane, so the hint must name a different action in each --
        // advertising "enter drill" in both is the lie this surface fixes.
        // The lane OVERVIEW selects a LANE, not an item, so every per-item key
        // is inert there and none may be advertised.
        let overview = footer_hint(
            TuiView::Lanes,
            LaneFocus::Overview,
            false,
            &TuiOverlay::None,
        );
        assert!(overview.contains("enter drill"));
        for inert in ["move-status", "approve/accept/reject", "set-admission"] {
            assert!(!overview.contains(inert));
        }

        let drilled = footer_hint(
            TuiView::Lanes,
            LaneFocus::Lane(Lane::Ready),
            true,
            &TuiOverlay::None,
        );
        assert!(drilled.contains("enter item") && !drilled.contains("enter drill"));
        // With an item selected the per-item keys DO act, so they are listed.
        assert!(drilled.contains("move-status") && drilled.contains("approve/accept/reject"));

        // An EMPTY drilled-in lane selects nothing: `enter` opens nothing and
        // every per-item key is inert, so neither is advertised.
        let empty = footer_hint(
            TuiView::Lanes,
            LaneFocus::Lane(Lane::Ready),
            false,
            &TuiOverlay::None,
        );
        assert!(!empty.contains("enter item") && !empty.contains("enter drill"));
        assert!(!empty.contains("move-status") && !empty.contains("approve/accept/reject"));
        assert!(empty.contains("esc lane list"));

        // Attention drops its per-item valves when the inbox is empty.
        let attention_empty = footer_hint(
            TuiView::Attention,
            LaneFocus::Overview,
            false,
            &TuiOverlay::None,
        );
        assert!(!attention_empty.contains("approve/accept/reject"));
        assert!(attention_empty.contains("enter open"));
        // The open modal owns the hint line and names its own keys.
        let modal = footer_hint(
            TuiView::Lanes,
            LaneFocus::Lane(Lane::Ready),
            true,
            &TuiOverlay::WorkItemDetail {
                work_item_id: MODAL_ITEM.to_owned(),
                scroll: 0,
            },
        );
        assert!(modal.contains("esc close item") && !modal.contains("enter drill"));
    }

    #[test]
    fn the_open_item_modal_leaves_every_unrelated_interaction_inert() {
        // Any id: this test drives the overlay reducer directly, with no board
        // behind it, so the pinned id only has to ride through unchanged.
        const MODAL_ITEM: &str = "console-pinned";
        // The work-item detail modal is a READ-ONLY reading surface: every
        // interaction that belongs to some OTHER overlay (text entry, command
        // action selection, Help navigation) must pass over it without mutating
        // it. Its own scroll interactions are covered alongside the modal.
        let events: [ConsoleEvent; 0] = [];
        let open = TuiInteractionState::new(
            0,
            TuiOverlay::WorkItemDetail {
                work_item_id: MODAL_ITEM.to_owned(),
                scroll: 4,
            },
        );
        let modal = TuiOverlay::WorkItemDetail {
            work_item_id: MODAL_ITEM.to_owned(),
            scroll: 4,
        };

        for interaction in [
            TuiInteraction::TypeChar('x'),
            TuiInteraction::Backspace,
            TuiInteraction::SelectNextAction,
            TuiInteraction::SelectPreviousAction,
            TuiInteraction::HelpSelectNextSection,
            TuiInteraction::HelpSelectPreviousSection,
            TuiInteraction::HelpScrollDown,
            TuiInteraction::HelpScrollUp,
            TuiInteraction::CycleValveOption(true),
        ] {
            let after = reduce_tui_interaction(&open, &events, interaction);
            // Unchanged: the item modal owns none of these interactions.
            assert_eq!(after.overlay(), &modal);
        }

        // Its OWN interactions do move it: down accumulates, up saturates at the
        // top, and both are inert against any other overlay.
        let down =
            reduce_tui_interaction(&open, &events, TuiInteraction::WorkItemDetailScrollDown(3));
        assert_eq!(
            down.overlay(),
            &TuiOverlay::WorkItemDetail {
                work_item_id: MODAL_ITEM.to_owned(),
                scroll: 7
            }
        );
        let up = reduce_tui_interaction(&down, &events, TuiInteraction::WorkItemDetailScrollUp(99));
        assert_eq!(
            up.overlay(),
            &TuiOverlay::WorkItemDetail {
                work_item_id: MODAL_ITEM.to_owned(),
                scroll: 0
            }
        );
        // Opening with NO work-item selected opens nothing: the modal exists to
        // show one item's record, so there is no honest thing to show without
        // one, and a blank modal would read as a broken screen.
        let opened_fresh =
            reduce_tui_interaction(&open, &events, TuiInteraction::OpenWorkItemDetail);
        assert_eq!(opened_fresh.overlay(), &TuiOverlay::None);

        let elsewhere = TuiInteractionState::new(0, TuiOverlay::None);
        for interaction in [
            TuiInteraction::WorkItemDetailScrollDown(1),
            TuiInteraction::WorkItemDetailScrollUp(1),
        ] {
            let after = reduce_tui_interaction(&elsewhere, &events, interaction);
            assert_eq!(after.overlay(), &TuiOverlay::None);
        }

        // The overlay accessors that belong to other overlays report nothing for
        // it, and its own scroll accessor reports the offset.
        assert_eq!(TuiOverlay::None.work_item_detail_scroll(), None);
        assert_eq!(
            TuiOverlay::Help {
                selected_section: 0,
                scroll: 0
            }
            .work_item_detail_scroll(),
            None
        );
        assert_eq!(modal.query(), None);
        assert_eq!(modal.selected_action_index(), None);
        assert_eq!(modal.valve_confirm(), None);
        assert_eq!(modal.work_item_detail_scroll(), Some(4));
        assert!(modal.is_open());
    }

    #[test]
    fn footer_hint_covers_every_overlay_with_its_own_non_empty_hints() {
        // Every overlay owns the hint line while open (matched before the pane),
        // so each renders its own non-empty, overlay-appropriate keys regardless
        // of the underlying view -- no overlay context shows a blank hint line.
        let overlays = [
            TuiOverlay::Search {
                query: "gate".to_owned(),
            },
            TuiOverlay::CommandPalette {
                query: String::new(),
            },
            TuiOverlay::CommandModal {
                selected_action_index: 0,
            },
            TuiOverlay::ValveConfirm {
                valve: PendingValve::Approve,
            },
            TuiOverlay::Help {
                selected_section: 0,
                scroll: 0,
            },
        ];
        for overlay in &overlays {
            let hint = footer_hint(TuiView::Attention, LaneFocus::Overview, true, overlay);
            // Non-empty and mentions the overlay's exit key.
            assert!(!hint.trim().is_empty() && hint.contains("esc"));
            // The overlay owns the hints: they do NOT fall through to the
            // underlying Attention pane's keys.
            assert_ne!(
                hint,
                footer_hint(
                    TuiView::Attention,
                    LaneFocus::Overview,
                    true,
                    &TuiOverlay::None
                )
            );
        }
    }

    #[test]
    fn interaction_state_carries_selected_repo_and_settings_through_the_reducer() {
        let settings = DispatcherSettings::new(true, false, AcceptancePolicy::AiOnly, 4, 2, 5);
        let state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_selected_repo(CONFIRM_REPO.to_owned())
            .with_dispatcher_settings(DispatcherSettingsRead::Observed(settings.clone()));
        assert_eq!(state.selected_repo(), CONFIRM_REPO);
        assert_eq!(
            state.dispatcher_settings(),
            &DispatcherSettingsRead::Observed(settings.clone())
        );

        // A view-navigation interaction must preserve the ambient repo + settings.
        let next = reduce_tui_interaction(&state, &[], TuiInteraction::SelectNextView);
        assert_eq!(next.selected_repo(), CONFIRM_REPO);
        assert_eq!(
            next.dispatcher_settings(),
            &DispatcherSettingsRead::Observed(settings)
        );
    }

    // -----------------------------------------------------------------------
    // The Settings surface: six views, the six dispatcher-setting rows, their
    // per-edit writes, and the ordinary recorded edit with no arming ceremony.
    // -----------------------------------------------------------------------

    /// A Settings-view model whose observed dispatcher settings are `settings`,
    /// with row `selected` under the cursor.
    fn settings_model(settings: DispatcherSettings, selected: usize) -> TuiScreenModel {
        let state = TuiInteractionState::for_view(TuiView::Settings, 0, TuiOverlay::None)
            .with_selected_repo(CONFIRM_REPO.to_owned())
            .with_selected_setting_index(selected)
            .with_dispatcher_settings(DispatcherSettingsRead::Observed(settings));
        build_tui_model_for_state(&[], &state)
    }

    #[test]
    fn tui_has_six_views_including_settings() {
        let views = TuiView::all();
        assert_eq!(views.len(), 6);
        assert_eq!(
            views,
            [
                TuiView::Attention,
                TuiView::Spec,
                TuiView::Lanes,
                TuiView::Events,
                TuiView::Repos,
                TuiView::Settings,
            ]
        );
        assert_eq!(TuiView::Settings.label(), "Settings");
    }

    #[test]
    fn dispatcher_setting_rows_render_each_effective_value_and_flag_dangerous_rows() {
        let settings = DispatcherSettings::new(true, false, AcceptancePolicy::AiOnly, 4, 2, 5);
        let rows: Vec<SettingRow> = dispatcher_setting_rows(&settings);
        assert_eq!(rows.len(), 6);

        let rendered: Vec<(&str, &str, bool)> = rows
            .iter()
            .map(|row| (row.label(), row.value(), row.dangerous()))
            .collect();
        assert_eq!(
            rendered,
            [
                ("Auto-approve ready", "on", true),
                ("Merge on review cap", "off", true),
                ("Acceptance mode", "ai-only", true),
                ("Review fix cap", "4", false),
                ("Acceptance rework cap", "2", false),
                ("WIP cap", "5", false),
            ]
        );
        // A dangerous row's help carries the required "dangerous / use with
        // caution" label; a cap row's does not.
        assert!(rows[0].help().contains("dangerous / use with caution"));
        assert!(!rows[5].help().contains("dangerous / use with caution"));

        // Each row surfaces its orchestrator `dispatcher.*` key, in display order,
        // for the settings-completeness check to match against the manifest.
        let keys: Vec<&str> = DispatcherSettingRow::all()
            .iter()
            .map(DispatcherSettingRow::orchestrator_key)
            .collect();
        assert_eq!(
            keys,
            [
                "auto_approve_ready",
                "merge_on_review_cap",
                "acceptance_mode",
                "review_fix_cap",
                "acceptance_rework_cap",
                "wip_cap",
            ]
        );
    }

    #[test]
    fn dispatcher_setting_row_next_write_flips_cycles_and_increments() {
        // review_fix_cap 9 wraps to the minimum; acceptance_rework_cap 0 (below the
        // minimum) is nudged up to it; wip_cap 3 increments.
        let settings = DispatcherSettings::new(false, true, AcceptancePolicy::AiThenHuman, 9, 0, 3);
        let writes: Vec<DispatcherSettingWrite> = DispatcherSettingRow::all()
            .iter()
            .map(|row| row.next_write(&settings))
            .collect();
        assert_eq!(
            writes,
            [
                DispatcherSettingWrite::AutoApproveReady(true),
                DispatcherSettingWrite::MergeOnReviewCap(false),
                DispatcherSettingWrite::AcceptanceMode(AcceptancePolicy::AiOnly),
                DispatcherSettingWrite::ReviewFixCap(1),
                DispatcherSettingWrite::AcceptanceReworkCap(1),
                DispatcherSettingWrite::WipCap(4),
            ]
        );
    }

    #[test]
    fn dispatcher_setting_write_value_json_is_typed() {
        assert_eq!(
            DispatcherSettingWrite::AutoApproveReady(true).value_json(),
            serde_json::json!(true)
        );
        assert_eq!(
            DispatcherSettingWrite::AcceptanceMode(AcceptancePolicy::HumanOnly).value_json(),
            serde_json::json!("human-only")
        );
        assert_eq!(
            DispatcherSettingWrite::WipCap(7).value_json(),
            serde_json::json!(7)
        );
    }

    #[test]
    fn editing_a_dangerous_setting_is_an_ordinary_recorded_write_with_no_ceremony() {
        // The Auto-approve ready row is dangerous, yet editing it submits an
        // ordinary `config.dispatcher_setting_set` command carrying that one
        // setting -- NO type-the-repo-name confirmation and NO other arming
        // ceremony (Scenario 9 / criterion 6).
        let settings =
            DispatcherSettings::new(false, false, AcceptancePolicy::AiThenHuman, 3, 2, 5);
        let model = settings_model(settings, 0);
        let outcome = resolve_dispatcher_setting_edit(&model, "operator");
        assert!(matches!(
            &outcome,
            Ok(OperatorActionOutcome::PersistCommandWithPayload { command, payload_json })
                if command.command_type() == &CommandType::ConfigDispatcherSettingSet
                    && command.aggregate_id() == CONFIRM_REPO
                    && payload_json.contains(r#""repo":"livespec-console-beads-fabro""#)
                    && payload_json.contains(r#""setting":"auto_approve_ready""#)
                    && payload_json.contains(r#""value":true"#)
        ));
    }

    #[test]
    fn editing_an_int_row_submits_the_incremented_value() {
        let settings =
            DispatcherSettings::new(false, false, AcceptancePolicy::AiThenHuman, 3, 2, 5);
        let model = settings_model(settings, 5); // WIP cap row
        let outcome = resolve_dispatcher_setting_edit(&model, "operator");
        assert!(matches!(
            &outcome,
            Ok(OperatorActionOutcome::PersistCommandWithPayload { payload_json, .. })
                if payload_json.contains(r#""setting":"wip_cap""#)
                    && payload_json.contains(r#""value":6"#)
        ));
    }

    #[test]
    fn editing_errors_when_the_settings_are_not_observed() {
        let state = TuiInteractionState::for_view(TuiView::Settings, 0, TuiOverlay::None)
            .with_selected_repo(CONFIRM_REPO.to_owned());
        let model = build_tui_model_for_state(&[], &state);
        assert_eq!(
            resolve_dispatcher_setting_edit(&model, "operator"),
            Err(ApplicationError::DispatcherSettingsNotObserved)
        );
    }

    #[test]
    fn editing_errors_without_a_selected_setting_row() {
        // Observed settings but no Settings row selected (a non-Settings view
        // leaves `selected_setting_index` unset) is the defensive no-selection
        // path.
        let settings =
            DispatcherSettings::new(false, false, AcceptancePolicy::AiThenHuman, 3, 2, 5);
        let state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_selected_repo(CONFIRM_REPO.to_owned())
            .with_dispatcher_settings(DispatcherSettingsRead::Observed(settings));
        let model = build_tui_model_for_state(&[], &state);
        assert_eq!(model.selected_setting_index(), None);
        assert_eq!(
            resolve_dispatcher_setting_edit(&model, "operator"),
            Err(ApplicationError::NoSelectedDispatcherSetting)
        );
    }

    #[test]
    fn editing_errors_on_a_blank_operator() {
        let settings =
            DispatcherSettings::new(false, false, AcceptancePolicy::AiThenHuman, 3, 2, 5);
        let model = settings_model(settings, 0);
        assert_eq!(
            resolve_dispatcher_setting_edit(&model, "   "),
            Err(ApplicationError::EmptyOperatorAction)
        );
    }

    #[test]
    fn settings_selection_moves_within_the_six_rows_and_clamps() {
        let settings =
            DispatcherSettings::new(false, false, AcceptancePolicy::AiThenHuman, 3, 2, 5);
        let state = TuiInteractionState::for_view(TuiView::Settings, 0, TuiOverlay::None)
            .with_dispatcher_settings(DispatcherSettingsRead::Observed(settings))
            .with_focus(FocusPane::Content);

        let down = reduce_tui_interaction(&state, &[], TuiInteraction::SelectNext);
        assert_eq!(down.selected_setting_index(), 1);

        // Stepping down past the last row clamps at the sixth (index 5).
        let mut walked = state.clone();
        for _ in 0..10 {
            walked = reduce_tui_interaction(&walked, &[], TuiInteraction::SelectNext);
        }
        assert_eq!(walked.selected_setting_index(), 5);

        // Stepping up from the top row stays at the first.
        let up = reduce_tui_interaction(&state, &[], TuiInteraction::SelectPrevious);
        assert_eq!(up.selected_setting_index(), 0);
    }

    #[test]
    fn persist_with_payload_outcome_exposes_command_and_no_attach() {
        let outcome = OperatorActionOutcome::PersistCommandWithPayload {
            command: CommandEnvelope::new(
                "cmd".to_owned(),
                CommandType::ConfigDispatcherSettingSet,
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

        // The move-status valve labels itself and shows its target lane; it is
        // never destructive (its reject-based routes are excluded).
        let move_valve = PendingValve::MoveStatus {
            from: Lane::PendingApproval,
            to: Lane::Ready,
        };
        assert_eq!(move_valve.valve_label(), "Move status");
        assert_eq!(move_valve.option_label(), Some("ready"));
        assert_eq!(move_valve.option_display(), Some("ready".to_owned()));
        assert!(!move_valve.is_destructive());

        // The per-item override valve labels itself, carries no `'static`
        // option_label (its value is dynamic), renders its value via
        // option_display, and is never destructive.
        let override_valve =
            PendingValve::SetOverride(DispatcherOverride::ReviewFixCap(OverrideInt::Value(3)));
        assert_eq!(override_valve.valve_label(), "Set override");
        assert_eq!(override_valve.option_label(), None);
        assert_eq!(
            override_valve.option_display(),
            Some("review_fix_cap = 3".to_owned())
        );
        assert!(!override_valve.is_destructive());
    }

    #[test]
    fn dispatcher_override_maps_each_setting_onto_its_verb_literal_payload_and_display() {
        // merge_on_review_cap is a bool: on/off/clear.
        let merge_on = DispatcherOverride::MergeOnReviewCap(OverrideBool::On);
        assert_eq!(merge_on.setting_key(), "merge_on_review_cap");
        assert_eq!(merge_on.action_verb(), "set-merge-on-review-cap");
        assert_eq!(merge_on.value_literal(), "true");
        assert_eq!(merge_on.payload_value(), serde_json::Value::Bool(true));
        assert_eq!(merge_on.option_display(), "merge_on_review_cap = on");
        let merge_off = DispatcherOverride::MergeOnReviewCap(OverrideBool::Off);
        assert_eq!(merge_off.value_literal(), "false");
        assert_eq!(merge_off.payload_value(), serde_json::Value::Bool(false));
        assert_eq!(merge_off.option_display(), "merge_on_review_cap = off");
        let merge_clear = DispatcherOverride::MergeOnReviewCap(OverrideBool::Clear);
        assert_eq!(merge_clear.value_literal(), "clear");
        assert_eq!(merge_clear.payload_value(), serde_json::Value::Null);
        assert_eq!(merge_clear.option_display(), "merge_on_review_cap = clear");

        // review_fix_cap / acceptance_rework_cap are positive ints or clear.
        let review = DispatcherOverride::ReviewFixCap(OverrideInt::Value(5));
        assert_eq!(review.setting_key(), "review_fix_cap");
        assert_eq!(review.action_verb(), "set-review-fix-cap");
        assert_eq!(review.value_literal(), "5");
        assert_eq!(review.payload_value(), serde_json::Value::Number(5.into()));
        assert_eq!(review.option_display(), "review_fix_cap = 5");
        let rework = DispatcherOverride::AcceptanceReworkCap(OverrideInt::Clear);
        assert_eq!(rework.setting_key(), "acceptance_rework_cap");
        assert_eq!(rework.action_verb(), "set-acceptance-rework-cap");
        assert_eq!(rework.value_literal(), "clear");
        assert_eq!(rework.payload_value(), serde_json::Value::Null);
        assert_eq!(rework.option_display(), "acceptance_rework_cap = clear");

        // Exercise the remaining or-pattern arms: acceptance_rework_cap with a
        // value, and review_fix_cap cleared, so every setting x value combination
        // of the literal / payload / display mappings is covered.
        let rework_value = DispatcherOverride::AcceptanceReworkCap(OverrideInt::Value(6));
        assert_eq!(rework_value.value_literal(), "6");
        assert_eq!(
            rework_value.payload_value(),
            serde_json::Value::Number(6.into())
        );
        assert_eq!(rework_value.option_display(), "acceptance_rework_cap = 6");
        let review_clear = DispatcherOverride::ReviewFixCap(OverrideInt::Clear);
        assert_eq!(review_clear.value_literal(), "clear");
        assert_eq!(review_clear.payload_value(), serde_json::Value::Null);
        assert_eq!(review_clear.option_display(), "review_fix_cap = clear");
    }

    #[test]
    fn dispatcher_override_cycles_bool_and_positive_int_values_including_clear() {
        // The bool dial walks on -> off -> clear -> on (forward), reverse back.
        let on = DispatcherOverride::MergeOnReviewCap(OverrideBool::On);
        assert_eq!(
            on.cycled(true),
            DispatcherOverride::MergeOnReviewCap(OverrideBool::Off)
        );
        assert_eq!(
            on.cycled(true).cycled(true),
            DispatcherOverride::MergeOnReviewCap(OverrideBool::Clear)
        );
        assert_eq!(
            on.cycled(true).cycled(true).cycled(true),
            DispatcherOverride::MergeOnReviewCap(OverrideBool::On)
        );
        assert_eq!(
            on.cycled(false),
            DispatcherOverride::MergeOnReviewCap(OverrideBool::Clear)
        );

        // The int dial walks clear -> 1 -> 2 -> ... -> 9 -> clear (forward),
        // reverse back, and never proposes a non-positive value.
        let clear = DispatcherOverride::ReviewFixCap(OverrideInt::Clear);
        assert_eq!(
            clear.cycled(true),
            DispatcherOverride::ReviewFixCap(OverrideInt::Value(1))
        );
        assert_eq!(
            clear.cycled(false),
            DispatcherOverride::ReviewFixCap(OverrideInt::Value(9))
        );
        assert_eq!(
            DispatcherOverride::ReviewFixCap(OverrideInt::Value(9)).cycled(true),
            DispatcherOverride::ReviewFixCap(OverrideInt::Clear)
        );
        assert_eq!(
            DispatcherOverride::AcceptanceReworkCap(OverrideInt::Value(1)).cycled(false),
            DispatcherOverride::AcceptanceReworkCap(OverrideInt::Clear)
        );
        assert_eq!(
            DispatcherOverride::AcceptanceReworkCap(OverrideInt::Value(4)).cycled(true),
            DispatcherOverride::AcceptanceReworkCap(OverrideInt::Value(5))
        );
        assert_eq!(
            DispatcherOverride::ReviewFixCap(OverrideInt::Value(5)).cycled(false),
            DispatcherOverride::ReviewFixCap(OverrideInt::Value(4))
        );
        // The override valve delegates cycling to its dial.
        assert_eq!(
            PendingValve::SetOverride(clear).cycled(true),
            PendingValve::SetOverride(DispatcherOverride::ReviewFixCap(OverrideInt::Value(1)))
        );
    }

    #[test]
    fn move_status_valve_cycles_targets_and_status_move_targets_are_the_pre_terminal_set() {
        // Blocked offers backlog/ready/active; up/down walks the ordered set.
        let blocked_ready = PendingValve::MoveStatus {
            from: Lane::Blocked,
            to: Lane::Ready,
        };
        assert_eq!(
            blocked_ready.cycled(true),
            PendingValve::MoveStatus {
                from: Lane::Blocked,
                to: Lane::Active,
            }
        );
        assert_eq!(
            blocked_ready.cycled(false),
            PendingValve::MoveStatus {
                from: Lane::Blocked,
                to: Lane::Backlog,
            }
        );

        // The drivable target sets are the pre-terminal pipeline statuses (minus
        // the item's own lane), plus `done` only from acceptance, and nothing at
        // all from a shipped `done` item.
        assert_eq!(
            status_move_targets(Lane::Backlog),
            &[Lane::Ready, Lane::Active, Lane::Blocked]
        );
        assert_eq!(
            status_move_targets(Lane::PendingApproval),
            &[Lane::Backlog, Lane::Ready, Lane::Active, Lane::Blocked]
        );
        assert_eq!(
            status_move_targets(Lane::Ready),
            &[Lane::Backlog, Lane::Active, Lane::Blocked]
        );
        assert_eq!(
            status_move_targets(Lane::Active),
            &[Lane::Backlog, Lane::Ready, Lane::Blocked]
        );
        assert_eq!(
            status_move_targets(Lane::Acceptance),
            &[
                Lane::Backlog,
                Lane::Ready,
                Lane::Active,
                Lane::Blocked,
                Lane::Done
            ]
        );
        assert_eq!(
            status_move_targets(Lane::Blocked),
            &[Lane::Backlog, Lane::Ready, Lane::Active]
        );
        assert_eq!(status_move_targets(Lane::Done), &[] as &[Lane]);
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
        // No work-item is selected (empty inbox, Attention view).
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
            Err(ApplicationError::NoSelectedWorkItem)
        );
    }

    // -----------------------------------------------------------------------
    // Per-item selection in a drilled-in lane, and the move-to-status valve
    // that transitions the individually-selected item through the orchestrator's
    // real transition actions (W7).
    // -----------------------------------------------------------------------

    /// Two pending-approval work-items plus one item per other tested lane, so a
    /// drilled-in lane holds a selectable list.
    fn drilldown_events() -> Vec<ConsoleEvent> {
        vec![
            lane_event(
                "e1",
                "wi-a",
                Lane::PendingApproval,
                None,
                "a",
                "pending-approval",
            ),
            lane_event(
                "e2",
                "wi-b",
                Lane::PendingApproval,
                None,
                "b",
                "pending-approval",
            ),
            lane_event("e3", "wi-acc", Lane::Acceptance, None, "a", "acceptance"),
            lane_event(
                "e4",
                "wi-blk",
                Lane::Blocked,
                Some(LaneReason::NeedsHuman),
                "a",
                "blocked",
            ),
            lane_event("e5", "wi-act", Lane::Active, None, "a", "active"),
            lane_event("e6", "wi-done", Lane::Done, None, "a", "done"),
        ]
    }

    fn drilldown_state(lane: Lane, item_index: usize, overlay: TuiOverlay) -> TuiInteractionState {
        TuiInteractionState::for_view(TuiView::Lanes, 0, overlay)
            .with_lane_focus(LaneFocus::Lane(lane))
            .with_selected_lane_item_index(item_index)
    }

    #[test]
    fn drilled_in_lane_selects_an_individual_work_item_and_clamps_the_cursor() {
        let events = drilldown_events();
        // Second pending-approval item selected.
        let model = build_tui_model_for_state(
            &events,
            &drilldown_state(Lane::PendingApproval, 1, TuiOverlay::None),
        );
        assert_eq!(model.selected_lane_item_index(), Some(1));
        assert_eq!(
            model.selected_lane_item().map(LaneWorkItem::work_item_id),
            Some("wi-b")
        );
        assert_eq!(model.selected_work_item_id(), Some("wi-b"));

        // An out-of-range cursor clamps to the last item.
        let clamped = build_tui_model_for_state(
            &events,
            &drilldown_state(Lane::PendingApproval, 9, TuiOverlay::None),
        );
        assert_eq!(clamped.selected_lane_item_index(), Some(1));

        // An empty lane has no selectable item (backlog carries no fixture item).
        let empty = build_tui_model_for_state(
            &events,
            &drilldown_state(Lane::Backlog, 0, TuiOverlay::None),
        );
        assert_eq!(empty.selected_lane_item_index(), None);
        assert_eq!(empty.selected_lane_item(), None);
        assert_eq!(empty.selected_work_item_id(), None);

        // The lane overview (not drilled in) carries no per-item cursor.
        let overview = build_tui_model_for_state(
            &events,
            &TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None),
        );
        assert_eq!(overview.selected_lane_item_index(), None);
        assert_eq!(overview.selected_work_item_id(), None);
    }

    #[test]
    fn selected_work_item_id_is_view_scoped() {
        let events = fabro_gate_events();
        // Attention view -> the selected attention item's work-item.
        let attention = build_tui_model_for_state(
            &events,
            &TuiInteractionState::for_view(TuiView::Attention, 0, TuiOverlay::None),
        );
        assert!(attention.selected_work_item_id().is_some());
        // A view with no selectable work-item is inert.
        for view in [
            TuiView::Spec,
            TuiView::Events,
            TuiView::Repos,
            TuiView::Settings,
        ] {
            let model = build_tui_model_for_state(
                &events,
                &TuiInteractionState::for_view(view, 0, TuiOverlay::None),
            );
            assert_eq!(model.selected_work_item_id(), None);
        }
    }

    #[test]
    fn selected_move_status_valve_offers_the_first_pre_terminal_target() {
        let events = drilldown_events();
        // A pending-approval item can move to any pre-terminal status; the valve
        // opens staged at the first target (backlog), and up/down cycles on.
        let pending = build_tui_model_for_state(
            &events,
            &drilldown_state(Lane::PendingApproval, 0, TuiOverlay::None),
        );
        assert_eq!(
            pending.selected_move_status_valve(),
            Some(PendingValve::MoveStatus {
                from: Lane::PendingApproval,
                to: Lane::Backlog,
            })
        );
        // An active item now also has pre-terminal move targets (it did not
        // before the broad move landed).
        let active =
            build_tui_model_for_state(&events, &drilldown_state(Lane::Active, 0, TuiOverlay::None));
        assert_eq!(
            active.selected_move_status_valve(),
            Some(PendingValve::MoveStatus {
                from: Lane::Active,
                to: Lane::Backlog,
            })
        );
        // A shipped `done` item offers no onward move (the picker never un-ships).
        let done =
            build_tui_model_for_state(&events, &drilldown_state(Lane::Done, 0, TuiOverlay::None));
        assert_eq!(done.selected_move_status_valve(), None);
        // No lane item selected (overview) -> no valve.
        let overview = build_tui_model_for_state(
            &events,
            &TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None),
        );
        assert_eq!(overview.selected_move_status_valve(), None);
    }

    #[test]
    fn drilldown_item_count_is_zero_off_a_drilled_in_lane_and_the_lane_size_within_it() {
        let events = drilldown_events();
        // Off a drill-in (the lane overview), the cursor bound is zero.
        let overview = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None);
        let overview_model = build_tui_model_for_state(&events, &overview);
        assert_eq!(drilldown_item_count(&overview, &overview_model), 0);
        // Drilled into the pending-approval lane, it is that lane's item count.
        let drilled = drilldown_state(Lane::PendingApproval, 0, TuiOverlay::None);
        let drilled_model = build_tui_model_for_state(&events, &drilled);
        assert_eq!(drilldown_item_count(&drilled, &drilled_model), 2);
    }

    #[test]
    fn reduce_moves_the_per_item_cursor_within_a_drilled_in_lane() {
        let events = drilldown_events();
        let start = drilldown_state(Lane::PendingApproval, 0, TuiOverlay::None);
        // Down advances to the second item; a further down clamps at the last.
        let down = reduce_tui_interaction(&start, &events, TuiInteraction::SelectNext);
        assert_eq!(down.selected_lane_item_index(), 1);
        let down_again = reduce_tui_interaction(&down, &events, TuiInteraction::SelectNext);
        assert_eq!(down_again.selected_lane_item_index(), 1);
        // Up returns to the first item.
        let up = reduce_tui_interaction(&down, &events, TuiInteraction::SelectPrevious);
        assert_eq!(up.selected_lane_item_index(), 0);
    }

    #[test]
    fn move_status_resolves_to_the_real_orchestrator_transition_for_the_selected_item()
    -> super::ApplicationResult<()> {
        let events = drilldown_events();
        // pending-approval -> ready maps onto the approve command (W7 proof).
        let approve = build_tui_model_for_state(
            &events,
            &drilldown_state(
                Lane::PendingApproval,
                0,
                TuiOverlay::ValveConfirm {
                    valve: PendingValve::MoveStatus {
                        from: Lane::PendingApproval,
                        to: Lane::Ready,
                    },
                },
            ),
        );
        let outcome = resolve_valve_action(&approve, "operator")?;
        let command = outcome.command();
        assert_eq!(
            command.map(CommandEnvelope::command_type),
            Some(&CommandType::WorkItemApproveRequested)
        );
        assert_eq!(command.map(CommandEnvelope::aggregate_id), Some("wi-a"));

        // acceptance -> done maps onto the accept command.
        let accept = build_tui_model_for_state(
            &events,
            &drilldown_state(
                Lane::Acceptance,
                0,
                TuiOverlay::ValveConfirm {
                    valve: PendingValve::MoveStatus {
                        from: Lane::Acceptance,
                        to: Lane::Done,
                    },
                },
            ),
        );
        assert_eq!(
            resolve_valve_action(&accept, "operator")?
                .command()
                .map(CommandEnvelope::command_type),
            Some(&CommandType::WorkItemAcceptRequested)
        );

        // blocked -> backlog maps onto resolve-blocked with the target payload.
        let resolve = build_tui_model_for_state(
            &events,
            &drilldown_state(
                Lane::Blocked,
                0,
                TuiOverlay::ValveConfirm {
                    valve: PendingValve::MoveStatus {
                        from: Lane::Blocked,
                        to: Lane::Backlog,
                    },
                },
            ),
        );
        let resolve_outcome = resolve_valve_action(&resolve, "operator")?;
        assert!(matches!(
            &resolve_outcome,
            OperatorActionOutcome::PersistCommandWithPayload { command, payload_json }
                if command.command_type() == &CommandType::WorkItemResolveBlockedRequested
                    && command.aggregate_id() == "wi-blk"
                    && payload_json == r#"{"target_status":"backlog"}"#
        ));
        Ok(())
    }

    #[test]
    fn move_status_with_a_non_drivable_pair_is_no_selected_operator_action() {
        let events = drilldown_events();
        // A staged pair the operator could never open (pending-approval -> done)
        // has no real transition, so it resolves to a no-op error rather than a
        // fabricated command.
        let model = build_tui_model_for_state(
            &events,
            &drilldown_state(
                Lane::PendingApproval,
                0,
                TuiOverlay::ValveConfirm {
                    valve: PendingValve::MoveStatus {
                        from: Lane::PendingApproval,
                        to: Lane::Done,
                    },
                },
            ),
        );
        assert_eq!(
            resolve_valve_action(&model, "operator"),
            Err(ApplicationError::NoSelectedOperatorAction)
        );
    }

    #[test]
    fn move_status_broad_targets_map_onto_the_move_command_with_the_target_payload()
    -> super::ApplicationResult<()> {
        let events = drilldown_events();
        // pending-approval -> backlog is a broad pre-terminal move (no semantic
        // valve for that pair), so it rides the guarded move command with the
        // target payload rather than approve/accept/resolve-blocked.
        let model = build_tui_model_for_state(
            &events,
            &drilldown_state(
                Lane::PendingApproval,
                0,
                TuiOverlay::ValveConfirm {
                    valve: PendingValve::MoveStatus {
                        from: Lane::PendingApproval,
                        to: Lane::Backlog,
                    },
                },
            ),
        );
        assert!(matches!(
            resolve_valve_action(&model, "operator")?,
            OperatorActionOutcome::PersistCommandWithPayload { ref command, ref payload_json }
                if command.command_type() == &CommandType::WorkItemMoveRequested
                    && command.aggregate_id() == "wi-a"
                    && payload_json == r#"{"target_status":"backlog"}"#
        ));
        // active -> blocked is likewise a broad move.
        let active = build_tui_model_for_state(
            &events,
            &drilldown_state(
                Lane::Active,
                0,
                TuiOverlay::ValveConfirm {
                    valve: PendingValve::MoveStatus {
                        from: Lane::Active,
                        to: Lane::Blocked,
                    },
                },
            ),
        );
        assert!(matches!(
            resolve_valve_action(&active, "operator")?,
            OperatorActionOutcome::PersistCommandWithPayload { ref payload_json, .. }
                if payload_json == r#"{"target_status":"blocked"}"#
        ));

        // Cover the remaining move-outcome arms: the `ready` and `active` broad
        // targets, and blocked -> ready via resolve-blocked (the other half of the
        // blocked pair).
        let cases = [
            (
                Lane::Active,
                Lane::Ready,
                "wi-act",
                "move",
                r#"{"target_status":"ready"}"#,
            ),
            (
                Lane::PendingApproval,
                Lane::Active,
                "wi-a",
                "move",
                r#"{"target_status":"active"}"#,
            ),
            (
                Lane::Blocked,
                Lane::Ready,
                "wi-blk",
                "resolve_blocked",
                r#"{"target_status":"ready"}"#,
            ),
        ];
        for (from, to, item, _kind, expected_payload) in cases {
            let model = build_tui_model_for_state(
                &events,
                &drilldown_state(
                    from,
                    0,
                    TuiOverlay::ValveConfirm {
                        valve: PendingValve::MoveStatus { from, to },
                    },
                ),
            );
            assert!(matches!(
                resolve_valve_action(&model, "operator")?,
                OperatorActionOutcome::PersistCommandWithPayload { ref command, ref payload_json }
                    if command.aggregate_id() == item && payload_json == expected_payload
            ));
        }
        Ok(())
    }

    #[test]
    fn set_override_valve_resolves_to_the_override_command_for_the_selected_item()
    -> super::ApplicationResult<()> {
        // A staged per-item override valve resolves, through the shared valve
        // path, into the set-dispatcher-override command for the selected item.
        let events = drilldown_events();
        let model = build_tui_model_for_state(
            &events,
            &drilldown_state(
                Lane::PendingApproval,
                0,
                TuiOverlay::ValveConfirm {
                    valve: PendingValve::SetOverride(DispatcherOverride::MergeOnReviewCap(
                        OverrideBool::On,
                    )),
                },
            ),
        );
        assert!(matches!(
            resolve_valve_action(&model, "operator")?,
            OperatorActionOutcome::PersistCommandWithPayload { ref command, ref payload_json }
                if command.command_type() == &CommandType::WorkItemSetDispatcherOverrideRequested
                    && command.aggregate_id() == "wi-a"
                    && payload_json == r#"{"setting":"merge_on_review_cap","value":true}"#
        ));
        Ok(())
    }
}
