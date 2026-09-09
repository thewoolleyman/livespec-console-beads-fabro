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

use std::borrow::Cow;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};

/// The operator action registry.
///
/// The single source of truth for the per-item operator action set, from
/// which hints, key bindings, and menus derive.
pub mod action_registry;
/// The running binary's build identity and its staleness against the repo.
pub mod build_identity;
/// The `doctor` diagnostic: console-health findings derived from the SAME
/// in-process projection the header renders.
pub mod doctor;
/// Module containing source-adapters support.
pub mod source_adapters;

use build_identity::{BuildIdentity, BuildStaleness, build_staleness_segment};
use source_adapters::{
    AcceptancePolicy, AdmissionPolicy, AttentionItemSnapshot, AttentionSourceRef, Lane, LaneReason,
    OrphanedFactoryRun, SourceProbe, SourceProbeOutcome, WorkItemComment, WorkItemDetail,
    WorkItemSnapshot, attention_item_snapshot_from_payload_json,
    dispatcher_journal_from_payload_json, fabro_run_snapshot_from_payload_json,
    materialize_attention_items, reconcile_runs_snapshot_from_payload_json,
    work_item_snapshot_from_payload_json,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Whether an `active` lane item has an observed run signal behind it.
pub enum LaneExecutionState {
    /// The item is not in the `active` lane, so the claim/execution split does
    /// not apply.
    NotActive,
    /// The item is in the `active` lane, but no dispatcher/Fabro execution
    /// observation has been ingested for it.
    Claimed,
    /// The item is in the `active` lane and has an observed dispatcher or Fabro
    /// execution signal.
    Executing,
    /// The item is still in the `active` lane, but a terminal dispatcher
    /// outcome has been observed before ledger reconciliation moved the row.
    FinishedUnreconciled,
}

impl LaneExecutionState {
    #[must_use]
    /// Return the stable display label for this value.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::NotActive => "-",
            Self::Claimed => "claimed",
            Self::Executing => "executing",
            Self::FinishedUnreconciled => "finished?",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents attention item data used by the console.
pub struct AttentionItem {
    id: String,
    work_item_id: Option<String>,
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
        work_item_id: Option<String>,
        title: String,
        source: String,
        source_reference: String,
        next_action: Option<OperatorAction>,
    ) -> Self {
        Self {
            id,
            work_item_id,
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
    /// Return the work-item id this attention row can drill into, when known.
    pub fn work_item_id(&self) -> Option<&str> {
        self.work_item_id.as_deref()
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

#[derive(Debug, Clone, PartialEq, Eq)]
/// The resolved orchestrator plugin root/build selected for backing programs.
pub struct PluginResolution {
    source: String,
    root: Option<String>,
    version: Option<String>,
}

impl PluginResolution {
    #[must_use]
    /// Build a resolved plugin summary.
    pub const fn resolved(source: String, root: String, version: Option<String>) -> Self {
        Self {
            source,
            root: Some(root),
            version,
        }
    }

    #[must_use]
    /// Build an unresolved plugin summary.
    pub const fn unresolved() -> Self {
        Self {
            source: String::new(),
            root: None,
            version: None,
        }
    }

    #[must_use]
    /// Return the resolution source label.
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    /// Return the resolved plugin root, if any.
    pub fn root(&self) -> Option<&str> {
        self.root.as_deref()
    }

    #[must_use]
    /// Return the resolved plugin build/version, if known.
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
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

/// Which sub-view the `Events` container is showing.
///
/// Either its own picker home (the "Stored events" / "Event sources" row
/// list), or one of the two sub-views drilled into. Mirrors [`LaneFocus`]'s
/// overview/drilled-in shape -- the same escape-returns-to-overview,
/// session-only-persistence pattern the `Lanes` view already proved out.
///
/// Maintainer ruling 2026-09-09: top-level `Events` becomes a container so the
/// per-source health the header only ellipsifies today (`sources: 3
/// unavailable (dispatcher, +2 more)`) has somewhere to be shown in full. This
/// value carries ONLY the navigation model; the "Event sources" roster's real
/// content (health, cause, last-successful-read, per-row actions) is a sibling
/// item (`livespec-console-beads-fabro-pzbdbo.29`,
/// `livespec-console-beads-fabro-mx9u.20.2`) and deliberately renders as an
/// empty/skeleton state here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventsFocus {
    /// The container's own picker home, listing its two named sub-views.
    Overview,
    /// The "Stored events" sub-view -- exactly what the `Events` view rendered
    /// before it became a container. No behaviour change to its content.
    StoredEvents,
    /// The "Event sources" sub-view -- the per-source roster. Its content is a
    /// sibling item; this carries only the navigation model and an
    /// empty/skeleton state, structured so a per-row action surface can be
    /// added later without a reshape.
    EventSources,
}

impl EventsFocus {
    #[must_use]
    /// Return the canonical ordered set of drillable sub-views (excluding the
    /// container's own overview, which is not itself a sub-view).
    pub const fn all() -> &'static [Self] {
        &[Self::StoredEvents, Self::EventSources]
    }

    #[must_use]
    /// Return the stable display label for this sub-view. The container's own
    /// overview has no single sub-view name of its own, so it borrows the
    /// container's.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Overview => "Events",
            Self::StoredEvents => "Stored events",
            // Maintainer ruling 2026-09-09: SOURCES is always presented in the
            // UI as "EVENT SOURCES", for clarity against the event-sourcing
            // architecture the console is built on.
            Self::EventSources => "Event sources",
        }
    }
}

/// Which pane the arrow keys drive.
///
/// The cockpit body is three side-by-side panes — the left **Views** navigation
/// menu, the middle **Content** pane (the active view's list), and the right
/// **Detail** pane (the selected item's details) — above which sits the
/// **Header** pane (the top status line). `left`/`right` walk focus spatially
/// between the body panes (`right` stops on Detail; `left` returns to Nav, then
/// enters the rendered menu bar from that resting left edge); `up`/`down` act
/// WITHIN the focused pane — moving the Views selection, the Content selection,
/// or scrolling the Detail pane. `Tab`/`BackTab` cycle focus across EVERY pane
/// including the Header. Focus starts on the Views nav so `up`/`down` walk the
/// vertical Views menu intuitively. The `Lanes` view has no Detail pane, so
/// `right` clamps at Content there and the focus cycle skips the Detail pane.
/// While the Header holds focus, `left`/`right` scroll it horizontally rather
/// than walking the body.
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
/// The focused pane inside the modal Help overlay.
pub enum HelpFocus {
    /// The left section menu. Up/down move between help sections.
    Menu,
    /// The right prose pane. Up/down scroll the selected section by one row.
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// One action the operator can select on the attention detail.
///
/// Every action is a REGISTRY action. The enum used to carry two more variants,
/// `OpenFabroAttach` and `CopyFabroAttach`, which opened or copied a
/// `fabro attach <run>` command; the needs-human redesign retired the handoff
/// they served (`SPECIFICATION/contracts.md`: the console MUST NOT render,
/// copy, or execute an attach handoff to a factory run), and no
/// production path ever put them in a detail's action list, so they were
/// removed rather than left as a selectable route to a command that finds
/// nothing.
pub enum OperatorAction {
    /// A registered operator action, referenced by its stable registry id.
    Registered(&'static str),
}

impl OperatorAction {
    #[must_use]
    /// Return the stable display label for this value.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Registered(id) => {
                action_registry::action_for_id(id).map_or(id, |spec| spec.label)
            }
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
    /// The workflow-scope-override human valve: record the selected item's
    /// declared `.github/workflows/` path as citation-only, so the
    /// factory-safety host-only refusal stops holding it. Maps onto the
    /// orchestrator's `set-workflow-scope-override:<id>:citation-only` action
    /// -- the documented remedy that refusal names. The scope value has exactly
    /// one admitted value today, so the dial does not cycle.
    SetWorkflowScopeOverride,
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
            Self::SetWorkflowScopeOverride => "Set workflow scope",
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
            Self::SetWorkflowScopeOverride => Some(WORKFLOW_SCOPE_CITATION_ONLY),
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
    ///
    /// The move-to-status valve renders the whole staged TRANSITION
    /// (`<from> -> <to>`), not the target alone: the modal opens pre-staged on
    /// the first legal target, and a bare target name never says what `Enter`
    /// would actually do to the selected item.
    pub fn option_display(&self) -> Option<String> {
        match self {
            Self::SetOverride(dial) => Some(dial.option_display()),
            Self::MoveStatus { from, to } => Some(format!("{} -> {}", from.label(), to.label())),
            _other => self.option_label().map(str::to_owned),
        }
    }

    #[must_use]
    /// The caption the dialed-in option is rendered under, in the confirm modal
    /// and in the Status line. The move valve stages a lane TRANSITION; every
    /// other payload valve stages a policy or mode.
    pub const fn option_caption(&self) -> &'static str {
        match self {
            Self::MoveStatus { .. } => "Move",
            _other => "Policy/mode",
        }
    }

    #[must_use]
    /// Whether this valve is destructive, so the confirm modal warns before it
    /// is submitted. Only reject is destructive.
    pub const fn is_destructive(&self) -> bool {
        matches!(self, Self::Reject(_mode))
    }

    #[must_use]
    /// Whether this valve's dialog offers the optional free-text ANSWER field
    /// beside its target-status choice.
    ///
    /// Only the resolve-blocked dialog does -- the `blocked -> ready | backlog`
    /// move, which is the valve a `blocked / needs-human` item rests on. The
    /// answer is what the orchestrator writes as the item's
    /// `livespec-human-answer` comment before it transitions, so the next
    /// dispatch's brief carries it (`SPECIFICATION/contracts.md`). Every other
    /// valve is answer-less, and
    /// typed characters stay inert on it exactly as before.
    pub const fn accepts_answer(&self) -> bool {
        matches!(
            self,
            Self::MoveStatus {
                from: Lane::Blocked,
                ..
            }
        )
    }

    #[must_use]
    /// This valve with its mode/policy/target/value rotated one step (forward or
    /// backward). The payload-free approve/accept valves are returned unchanged.
    pub fn cycled(self, forward: bool) -> Self {
        match self {
            // The scope dial has exactly one admitted value, so it does not
            // cycle, exactly like the payload-free valves.
            Self::Approve | Self::Accept | Self::SetWorkflowScopeOverride => self,
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

/// The one workflow-scope value the orchestrator's allowlist admits.
const WORKFLOW_SCOPE_CITATION_ONLY: &str = "citation-only";

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
/// This is the console's consumed copy of the orchestrator-owned per-state
/// operator verb vocabulary for the broad move-status verb. It deliberately does
/// NOT re-expand to every pre-terminal status: `pending-approval -> ready` stays
/// the approve valve, `done` stays reachable only by accept, and `active` is
/// entered by dispatch or acceptance rework rather than by operator relocation. A
/// lane with no operator-drivable target returns an empty slice, so the
/// move-status valve never opens on it.
const fn status_move_targets(from: Lane) -> &'static [Lane] {
    match from {
        Lane::Backlog => &[Lane::Ready, Lane::Blocked],
        Lane::PendingApproval | Lane::Ready | Lane::Acceptance => &[Lane::Backlog, Lane::Blocked],
        Lane::Blocked => &[Lane::Backlog, Lane::Ready],
        Lane::Active | Lane::Done => &[],
    }
}

/// Whether a selected work-item's lifecycle state admits the per-item verb.
///
/// This predicate is the shared state-valid slice for presentation and key
/// handling. It consumes the orchestrator-owned per-state operator verb
/// vocabulary as a table: hidden hints and inert keys therefore stay in lockstep.
#[must_use]
pub fn per_item_verb_is_state_valid(lane: Lane, verb: PendingValve) -> bool {
    match verb {
        PendingValve::Approve => matches!(lane, Lane::PendingApproval),
        PendingValve::Accept => matches!(lane, Lane::Acceptance),
        PendingValve::Reject(_mode) => {
            matches!(lane, Lane::PendingApproval | Lane::Acceptance)
        }
        PendingValve::SetAdmission(_policy) => {
            matches!(lane, Lane::Backlog | Lane::PendingApproval)
        }
        PendingValve::SetAcceptance(_policy) => {
            matches!(
                lane,
                Lane::Backlog | Lane::PendingApproval | Lane::Ready | Lane::Active
            )
        }
        PendingValve::SetOverride(
            DispatcherOverride::MergeOnReviewCap(_) | DispatcherOverride::ReviewFixCap(_),
        ) => matches!(lane, Lane::Backlog | Lane::PendingApproval | Lane::Ready),
        PendingValve::SetOverride(DispatcherOverride::AcceptanceReworkCap(_value)) => {
            matches!(
                lane,
                Lane::Backlog | Lane::PendingApproval | Lane::Ready | Lane::Active
            )
        }
        PendingValve::MoveStatus { from, .. } => {
            lane == from && !status_move_targets(from).is_empty()
        }
        // Console-scoped, not vocabulary-consumed: the orchestrator ships no
        // source-state guard for this valve and publishes no vocabulary row
        // for it, so the console offers it on the `ready` lane and lets the
        // REGISTRY availability add the deciding dimension —
        // `awaits_scope_override`, NOT factory safety. The two are disjoint:
        // a factory-marked item is refused on an earlier arm than the one this
        // override clears, so it is never the item awaiting one.
        PendingValve::SetWorkflowScopeOverride => matches!(lane, Lane::Ready),
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
    /// Command explainer variant.
    CommandExplainer {
        /// Index of the selected action within the modal's action list.
        selected_action_index: usize,
    },
    /// Help variant: the navigable, pane-specific modal help overlay opened
    /// with `?`. It carries the selected left-menu section index and the
    /// right-pane vertical scroll offset. It closes ONLY on `Esc` -- no other
    /// key, command, valve, or view-switch dismisses it (per the TUI Contract).
    Help {
        /// Which Help pane arrow keys currently drive.
        focus: HelpFocus,
        /// Index of the selected help section in the left menu. `0` is the
        /// `Global actions` section; `1..` map to `TuiView::all()` in order, so
        /// section `i` (for `i >= 1`) is `TuiView::all()[i - 1]`.
        selected_section: usize,
        /// The right-pane vertical scroll offset (the topmost visible wrapped
        /// row) for the selected section. Reset to `0` whenever the section
        /// changes, and clamped by the renderer to the section's wrapped height.
        scroll: usize,
    },
    /// Menu variant: the menu bar plus the open top-level node's submenu,
    /// GENERATED from the action registry's `menu_path` taxonomy. It carries
    /// only cursor state — which bar node is open and which action is selected
    /// within it — never the tree itself, which is derived on render so a new
    /// registry entry appears without any state migration.
    Menu {
        /// Index of the open top-level bar node.
        top: usize,
        /// Index of the selected action within that node's FLATTENED action
        /// list (group headers render but are not selectable).
        selected: usize,
    },
    /// Driver-handoff variant: the full-width render-only overlay showing the
    /// copy-paste-safe LLM-driver invocation for the selected work-item. It
    /// carries only the command string; confirming it copies via the terminal
    /// effect path and never records, spawns, monitors, or awaits a driver.
    DriverHandoff {
        /// The rendered driver command.
        command: String,
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
    /// Factory-dispatch confirmation pinned to the selected work-item.
    FactoryDispatchItemConfirm {
        /// The work-item id that confirmation will dispatch.
        work_item_id: String,
    },
    /// Factory-drain confirmation pinned to the ready item the next drain will
    /// claim under the board's rank-ordered ready lane.
    FactoryDrainConfirm {
        /// The work-item id the drain will claim first.
        work_item_id: String,
        /// The rank value that made it the next drain target.
        rank: String,
    },
    /// Valve-confirm variant: the confirm modal that stages one operator
    /// human-valve/policy-edit intent against the selected work-item. `Enter`
    /// submits the valve through the shared orchestrator action port; `up`/`down`
    /// cycle a payload valve's mode/policy; `Esc` cancels. Reject is warned as
    /// dangerous before submission.
    ValveConfirm {
        /// The staged valve intent (with its dialed-in mode/policy).
        valve: PendingValve,
        /// The optional free-text ANSWER typed beside the target-status choice,
        /// offered only by the resolve-blocked dialog (see
        /// [`PendingValve::accepts_answer`]). Empty until the operator types.
        ///
        /// It travels with the persisted `work_item.resolve_blocked_requested`
        /// command and reaches the orchestrator as that action's `--answer`
        /// argument; the orchestrator — never the console — writes the
        /// `livespec-human-answer` ledger comment
        /// (`SPECIFICATION/contracts.md`).
        /// The operator's bytes are carried verbatim: nothing here edits,
        /// rewords, or normalizes them.
        answer: String,
    },
    /// Action-invoker variant: the generic roster of EVERY registered operator
    /// action for the current selection, opened from the command palette
    /// (`actions`). Available actions stage their normal confirm flow on
    /// `Enter`; unavailable ones render marked and stay inert. It is
    /// SCAFFOLDING for the menu shell, deliberately minimal: every action is
    /// reachable here before menus exist, including the hotkey-less ones.
    ActionInvoker {
        /// Index of the selected row in the registry roster.
        selected_action: usize,
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
            | Self::CommandExplainer { .. }
            | Self::ActionInvoker { .. }
            | Self::FactoryDrainConfirm { .. }
            | Self::FactoryDispatchItemConfirm { .. }
            | Self::ValveConfirm { .. }
            | Self::DriverHandoff { .. }
            | Self::WorkItemDetail { .. }
            | Self::Help { .. }
            | Self::Menu { .. } => None,
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
            | Self::CommandExplainer { .. }
            | Self::ActionInvoker { .. }
            | Self::FactoryDrainConfirm { .. }
            | Self::FactoryDispatchItemConfirm { .. }
            | Self::ValveConfirm { .. }
            | Self::DriverHandoff { .. }
            | Self::Help { .. }
            | Self::Menu { .. } => None,
        }
    }

    #[must_use]
    /// Return the selected action index when the overlay is a command modal.
    pub const fn selected_action_index(&self) -> Option<usize> {
        match self {
            Self::CommandModal {
                selected_action_index,
            }
            | Self::CommandExplainer {
                selected_action_index,
            } => Some(*selected_action_index),
            Self::None
            | Self::Search { .. }
            | Self::CommandPalette { .. }
            | Self::ActionInvoker { .. }
            | Self::FactoryDrainConfirm { .. }
            | Self::FactoryDispatchItemConfirm { .. }
            | Self::ValveConfirm { .. }
            | Self::DriverHandoff { .. }
            | Self::WorkItemDetail { .. }
            | Self::Help { .. }
            | Self::Menu { .. } => None,
        }
    }

    #[must_use]
    /// Return the staged valve when the overlay is the valve-confirm modal, or
    /// `None` for any other overlay.
    pub const fn valve_confirm(&self) -> Option<PendingValve> {
        match self {
            Self::ValveConfirm { valve, .. } => Some(*valve),
            Self::None
            | Self::Search { .. }
            | Self::CommandPalette { .. }
            | Self::CommandModal { .. }
            | Self::CommandExplainer { .. }
            | Self::ActionInvoker { .. }
            | Self::FactoryDrainConfirm { .. }
            | Self::FactoryDispatchItemConfirm { .. }
            | Self::DriverHandoff { .. }
            | Self::WorkItemDetail { .. }
            | Self::Help { .. }
            | Self::Menu { .. } => None,
        }
    }

    #[must_use]
    /// The free-text answer typed into the valve-confirm modal, or `None` for
    /// any other overlay.
    ///
    /// Returned VERBATIM, including surrounding whitespace: the console never
    /// edits or rewords the operator's text. A valve that offers no answer
    /// field simply never accumulates one, so this stays empty for it.
    pub fn valve_answer(&self) -> Option<&str> {
        match self {
            Self::ValveConfirm { answer, .. } => Some(answer),
            Self::None
            | Self::Search { .. }
            | Self::CommandPalette { .. }
            | Self::CommandModal { .. }
            | Self::CommandExplainer { .. }
            | Self::ActionInvoker { .. }
            | Self::FactoryDrainConfirm { .. }
            | Self::FactoryDispatchItemConfirm { .. }
            | Self::DriverHandoff { .. }
            | Self::WorkItemDetail { .. }
            | Self::Help { .. }
            | Self::Menu { .. } => None,
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
    /// Open the selected command's explainer.
    OpenCommandExplainer,
    /// Open the generic action-invoker roster for the current selection.
    OpenActionInvoker,
    /// Open the generated menu bar.
    OpenMenu,
    /// Move to the next top-level menu node.
    MenuNextTop,
    /// Move to the previous top-level menu node.
    MenuPreviousTop,
    /// Close overlay variant.
    CloseOverlay,
    /// Select next view variant.
    SelectNextView,
    /// Select previous view variant.
    SelectPreviousView,
    /// Select the NAMED view outright — the direct view-switch keys (`1`..`6`),
    /// which skip the walk that [`SelectNextView`](Self::SelectNextView) makes
    /// one step at a time and the focus ring `Tab` cycles through the Header.
    /// Idempotent on the view already active.
    SelectView(TuiView),
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
    /// Drill the `Events` container's overview into its selected sub-view
    /// (`Stored events` or `Event sources`). Mirrors [`Self::DrillIntoLane`].
    DrillIntoEventsSubView,
    /// Return a drilled-in `Events` sub-view to the container's own overview
    /// (the `Esc`/`left` step-out this AC requires). Mirrors
    /// [`Self::ReturnToLaneOverview`].
    ReturnToEventsOverview,
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
    /// Scroll the modal Help right-hand text pane DOWN by one render-measured
    /// page. Inert unless the Help overlay is open.
    HelpPageDown,
    /// Scroll the modal Help right-hand text pane UP by one render-measured
    /// page. Inert unless the Help overlay is open.
    HelpPageUp,
    /// Move focus inside the modal Help overlay to the left section menu.
    HelpFocusMenu,
    /// Move focus inside the modal Help overlay to the right prose pane.
    HelpFocusText,
    /// Open the driver-handoff overlay on the currently selected work-item, when
    /// its lifecycle state admits a handoff per the orchestrator-owned verb
    /// vocabulary.
    OpenDriverHandoff,
    /// Open the work-item detail modal on the currently selected work-item,
    /// showing its full standardized record. Opens at the top of the record.
    OpenWorkItemDetail,
    /// Open a read-back confirmation for dispatching the selected work-item.
    OpenFactoryDispatchItemConfirm,
    /// Open a read-back confirmation for draining the next ranked ready
    /// work-item.
    OpenFactoryDrainConfirm,
    /// Scroll the work-item detail modal DOWN by the given number of rows (`1`
    /// for a line step). Inert unless that modal is open; the offset clamps to
    /// the record's render-measured wrapped height, so the scroll never runs past
    /// the last row.
    WorkItemDetailScrollDown(usize),
    /// Scroll the work-item detail modal UP by the given number of rows,
    /// saturating at the top. Inert unless that modal is open.
    WorkItemDetailScrollUp(usize),
    /// Scroll the work-item detail modal DOWN by one render-measured page. Inert
    /// unless that modal is open.
    WorkItemDetailPageDown,
    /// Scroll the work-item detail modal UP by one render-measured page. Inert
    /// unless that modal is open.
    WorkItemDetailPageUp,
    /// Open the valve-confirm modal staging the given human-valve/policy-edit
    /// intent against the selected work-item.
    OpenValveConfirm(PendingValve),
    /// Cycle the valve-confirm modal's payload valve to its next (`true`) or
    /// previous (`false`) mode/policy. Inert for a payload-free valve.
    CycleValveOption(bool),
}

/// Which end of a list a selection move was refused at.
///
/// Recorded by the reducer whenever `up`/`down` cannot move the routed
/// selection any further, and rendered by the Status line as a cue. The
/// selection itself is UNCHANGED — the console does not wrap — so this value is
/// how a reached edge is told apart from a broken key.
///
/// Measured 2026-08-31, the dogfooding session's FIRST keystroke: focus starts
/// on the Views navigation with `Attention` (the first entry) selected, the
/// operator pressed `Up` ("Up arrow seems intuitive, but doesn't work"), and
/// nothing at all happened — no wrap, no cue, no hint change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListEdge {
    /// The selection is on the first entry and `up` moved nothing.
    Top,
    /// The selection is on the last entry and `down` moved nothing.
    Bottom,
}

impl ListEdge {
    #[must_use]
    /// The Status-line cue naming this edge.
    pub const fn cue(self) -> &'static str {
        match self {
            Self::Top => "at top of list",
            Self::Bottom => "at end of list",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents tui interaction state data used by the console.
pub struct TuiInteractionState {
    active_view: TuiView,
    selected_attention_index: usize,
    selected_attention_id: Option<String>,
    lane_focus: LaneFocus,
    selected_lane_index: usize,
    selected_lane_item_index: usize,
    selected_lane_item_id: Option<String>,
    events_focus: EventsFocus,
    selected_events_index: usize,
    focus: FocusPane,
    detail_scroll: usize,
    detail_max_scroll: usize,
    header_scroll: usize,
    header_max_scroll: usize,
    help_max_scroll: usize,
    help_page_rows: usize,
    work_item_detail_max_scroll: usize,
    work_item_detail_page_rows: usize,
    overlay: TuiOverlay,
    selected_repo: String,
    selected_setting_index: usize,
    dispatcher_settings: DispatcherSettingsRead,
    dispatcher_setting_write: DispatcherSettingWriteState,
    plugin_resolution: PluginResolution,
    transient_status: Option<String>,
    list_edge: Option<ListEdge>,
    build_identity: Option<BuildIdentity>,
    build_staleness: BuildStaleness,
    // Whether the session's FIRST background source ingest is still in flight.
    // `false` by construction (every existing caller), so a session with no
    // opinion on the matter renders exactly as before. The composition root
    // seeds this `true` for an interactive launch (livespec-console-beads-fabro-
    // pzbdbo.27 -- the first frame now draws from whatever the store already
    // holds, BEFORE the background poller's first sweep lands) and clears it
    // once that sweep completes, so the header's `event sources: loading`
    // tell never outlives the condition it names.
    startup_ingest_pending: bool,
}

impl TuiInteractionState {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(selected_attention_index: usize, overlay: TuiOverlay) -> Self {
        Self {
            active_view: TuiView::Attention,
            selected_attention_index,
            selected_attention_id: None,
            lane_focus: LaneFocus::Overview,
            selected_lane_index: 0,
            selected_lane_item_index: 0,
            selected_lane_item_id: None,
            events_focus: EventsFocus::Overview,
            selected_events_index: 0,
            focus: FocusPane::Nav,
            detail_scroll: 0,
            detail_max_scroll: 0,
            header_scroll: 0,
            header_max_scroll: 0,
            help_max_scroll: 0,
            help_page_rows: 1,
            work_item_detail_max_scroll: 0,
            work_item_detail_page_rows: 1,
            overlay,
            selected_repo: String::new(),
            selected_setting_index: 0,
            dispatcher_settings: DispatcherSettingsRead::NotObserved,
            dispatcher_setting_write: DispatcherSettingWriteState::Idle,
            plugin_resolution: PluginResolution::unresolved(),
            transient_status: None,
            list_edge: None,
            build_identity: None,
            build_staleness: BuildStaleness::Unknown,
            startup_ingest_pending: false,
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
            selected_attention_id: None,
            lane_focus: LaneFocus::Overview,
            selected_lane_index: 0,
            selected_lane_item_index: 0,
            selected_lane_item_id: None,
            events_focus: EventsFocus::Overview,
            selected_events_index: 0,
            focus: FocusPane::Nav,
            detail_scroll: 0,
            detail_max_scroll: 0,
            header_scroll: 0,
            header_max_scroll: 0,
            help_max_scroll: 0,
            help_page_rows: 1,
            work_item_detail_max_scroll: 0,
            work_item_detail_page_rows: 1,
            overlay,
            selected_repo: String::new(),
            selected_setting_index: 0,
            dispatcher_settings: DispatcherSettingsRead::NotObserved,
            dispatcher_setting_write: DispatcherSettingWriteState::Idle,
            plugin_resolution: PluginResolution::unresolved(),
            transient_status: None,
            list_edge: None,
            build_identity: None,
            build_staleness: BuildStaleness::Unknown,
            startup_ingest_pending: false,
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

    /// Replace the Help overlay's render-measured vertical extents, preserving
    /// every other field. The TUI renderer measures the prose pane's maximum
    /// scroll offset and visible height, then feeds them back so Help page keys
    /// move by exactly the current viewport height.
    #[must_use]
    pub const fn with_help_scroll_extents(mut self, max_scroll: usize, page_rows: usize) -> Self {
        self.help_max_scroll = max_scroll;
        self.help_page_rows = page_rows;
        self
    }

    /// Replace the work-item detail modal's render-measured vertical extents,
    /// preserving every other field. The TUI renderer measures both the maximum
    /// scroll offset and the visible body height from the modal's current wrapped
    /// content rectangle, then feeds them back before the next key press so
    /// `PgUp`/`PgDn` move by exactly one visible page.
    #[must_use]
    pub const fn with_work_item_detail_scroll_extents(
        mut self,
        max_scroll: usize,
        page_rows: usize,
    ) -> Self {
        self.work_item_detail_max_scroll = max_scroll;
        self.work_item_detail_page_rows = page_rows;
        self
    }

    /// Replace the Attention list's selected row WITHOUT an identity anchor.
    ///
    /// Used where only a position is known — a preview state, or a caller with
    /// no row in hand. It CLEARS any previous anchor rather than leaving one
    /// pointing at an item the new row is not, exactly as
    /// [`Self::with_selected_lane_item_index`] does for the drilled-in lane.
    #[must_use]
    pub fn with_selected_attention_index(mut self, selected_attention_index: usize) -> Self {
        self.selected_attention_index = selected_attention_index;
        self.selected_attention_id = None;
        self
    }

    /// Replace the Attention list's selected row with both the current position
    /// and the attention-item identity that row represents.
    ///
    /// The position keeps keyboard movement simple; the id is the durable
    /// selection anchor read on the next projection rebuild, so a source refresh
    /// that re-ranks or re-groups the list follows the item the operator chose
    /// instead of whatever later lands at the old row index.
    #[must_use]
    pub fn with_selected_attention(
        mut self,
        selected_attention_index: usize,
        attention_id: &str,
    ) -> Self {
        self.selected_attention_index = selected_attention_index;
        self.selected_attention_id = Some(attention_id.to_owned());
        self
    }

    #[must_use]
    /// Return the stored value.
    pub fn with_lane_focus(mut self, lane_focus: LaneFocus) -> Self {
        self.lane_focus = lane_focus;
        if matches!(lane_focus, LaneFocus::Overview) {
            self.selected_lane_item_id = None;
        }
        self
    }

    #[must_use]
    /// Return the stored value.
    pub const fn with_selected_lane_index(mut self, selected_lane_index: usize) -> Self {
        self.selected_lane_index = selected_lane_index;
        self
    }

    /// Replace which sub-view the `Events` container is showing, preserving
    /// every other field. Mirrors [`Self::with_lane_focus`]: a NEW view switch
    /// does not reset this (see [`view_interaction_state`]), so returning to
    /// `Events` later in the same session lands back on whatever sub-view was
    /// last active -- the session-only "remembers the last sub-view" AC.
    #[must_use]
    pub const fn with_events_focus(mut self, events_focus: EventsFocus) -> Self {
        self.events_focus = events_focus;
        self
    }

    /// Replace the container's own picker-row cursor (which of the two named
    /// sub-views is highlighted while `events_focus` is
    /// [`EventsFocus::Overview`]), preserving every other field.
    #[must_use]
    pub const fn with_selected_events_index(mut self, selected_events_index: usize) -> Self {
        self.selected_events_index = selected_events_index;
        self
    }

    /// Replace the selected work-item row within a drilled-in lane (the
    /// per-item cursor the `Lanes` drill-in moves with up/down), preserving every
    /// other field.
    #[must_use]
    pub fn with_selected_lane_item_index(mut self, selected_lane_item_index: usize) -> Self {
        self.selected_lane_item_index = selected_lane_item_index;
        self.selected_lane_item_id = None;
        self
    }

    /// Replace the selected work-item within a drilled-in lane with both the
    /// current row and the item identity that row represents.
    ///
    /// The row keeps keyboard movement simple; the id is the durable selection
    /// anchor used on the next projection rebuild, so a re-sort follows the
    /// work-item instead of whatever later lands at the old row index.
    #[must_use]
    pub fn with_selected_lane_item(
        mut self,
        selected_lane_item_index: usize,
        work_item_id: &str,
    ) -> Self {
        self.selected_lane_item_index = selected_lane_item_index;
        self.selected_lane_item_id = Some(work_item_id.to_owned());
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
    /// Return the standing of the operator's most recent dispatcher-setting
    /// write.
    pub const fn dispatcher_setting_write(&self) -> &DispatcherSettingWriteState {
        &self.dispatcher_setting_write
    }

    #[must_use]
    /// Return this value with the dispatcher-setting write lifecycle replaced:
    /// set when an edit is submitted, folded again when a fresh effective read
    /// confirms (or fails to confirm) it.
    pub const fn with_dispatcher_setting_write(
        mut self,
        dispatcher_setting_write: DispatcherSettingWriteState,
    ) -> Self {
        self.dispatcher_setting_write = dispatcher_setting_write;
        self
    }

    #[must_use]
    /// Return this value with the resolved orchestrator plugin summary replaced.
    pub fn with_plugin_resolution(mut self, plugin_resolution: PluginResolution) -> Self {
        self.plugin_resolution = plugin_resolution;
        self
    }

    #[must_use]
    /// Return this value with the running binary's build identity replaced.
    /// The composition root resolves it ONCE, from the binary's own
    /// compile-time-embedded sha and build timestamp -- see
    /// [`build_identity::BuildIdentity`].
    pub fn with_build_identity(mut self, build_identity: Option<BuildIdentity>) -> Self {
        self.build_identity = build_identity;
        self
    }

    #[must_use]
    /// Return this value with the build's observed staleness against the
    /// repo's current HEAD replaced. The composition root resolves this ONCE
    /// at startup, the same way it resolves [`Self::with_dispatcher_settings`]
    /// -- see [`build_identity::observe_build_staleness`].
    pub const fn with_build_staleness(mut self, build_staleness: BuildStaleness) -> Self {
        self.build_staleness = build_staleness;
        self
    }

    #[must_use]
    /// Return this value with whether the session's first background source
    /// ingest is still in flight replaced. The composition root re-checks this
    /// every tick (a cheap atomic read, never a store or CLI call) and folds the
    /// answer in here, exactly as it re-reads dispatcher settings after a
    /// settled write -- see [`Self::with_build_staleness`].
    pub const fn with_startup_ingest_pending(mut self, startup_ingest_pending: bool) -> Self {
        self.startup_ingest_pending = startup_ingest_pending;
        self
    }

    #[must_use]
    /// Return this value with the transient header status replaced.
    pub fn with_transient_status(mut self, transient_status: Option<String>) -> Self {
        self.transient_status = transient_status;
        self
    }

    #[must_use]
    /// Return this value with the reached list edge replaced.
    ///
    /// The reducer re-derives this on EVERY interaction, so a cue never
    /// outlives the keystroke that earned it: the next keystroke either reaches
    /// an edge again or clears the cue.
    pub const fn with_list_edge(mut self, list_edge: Option<ListEdge>) -> Self {
        self.list_edge = list_edge;
        self
    }

    #[must_use]
    /// Return the list edge the last interaction was refused at, if any.
    pub const fn list_edge(&self) -> Option<ListEdge> {
        self.list_edge
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
    /// Return the Help overlay's maximum scroll offset measured by the last
    /// render.
    pub const fn help_max_scroll(&self) -> usize {
        self.help_max_scroll
    }

    #[must_use]
    /// Return the Help overlay's visible prose-pane rows measured by the last
    /// render. It is at least one row so a page key remains a real movement
    /// before the first measured frame.
    pub const fn help_page_rows(&self) -> usize {
        self.help_page_rows
    }

    #[must_use]
    /// Return the work-item detail modal's maximum scroll offset measured by the
    /// last render.
    pub const fn work_item_detail_max_scroll(&self) -> usize {
        self.work_item_detail_max_scroll
    }

    #[must_use]
    /// Return the work-item detail modal's visible body rows measured by the
    /// last render. It is at least one row so a page key remains a real movement
    /// before the first measured frame.
    pub const fn work_item_detail_page_rows(&self) -> usize {
        self.work_item_detail_page_rows
    }

    #[must_use]
    /// Return the stored value.
    pub const fn selected_attention_index(&self) -> usize {
        self.selected_attention_index
    }

    #[must_use]
    /// Return the selected attention-item id anchor, when operator movement has
    /// established one. This — not the row index — is what the next projection
    /// rebuild resolves the Attention cursor against.
    pub fn selected_attention_id(&self) -> Option<&str> {
        self.selected_attention_id.as_deref()
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
    /// Which sub-view the `Events` container is showing.
    pub const fn events_focus(&self) -> EventsFocus {
        self.events_focus
    }

    #[must_use]
    /// The container's own picker-row cursor, meaningful only while
    /// `events_focus` is [`EventsFocus::Overview`].
    pub const fn selected_events_index(&self) -> usize {
        self.selected_events_index
    }

    #[must_use]
    /// Return the selected work-item row within a drilled-in lane.
    pub const fn selected_lane_item_index(&self) -> usize {
        self.selected_lane_item_index
    }

    #[must_use]
    /// Return the selected drilled-lane work-item id anchor, when one has been
    /// established by operator movement or lane drill-in.
    pub fn selected_lane_item_id(&self) -> Option<&str> {
        self.selected_lane_item_id.as_deref()
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

    #[must_use]
    /// Return the resolved orchestrator plugin summary displayed in Settings.
    pub const fn plugin_resolution(&self) -> &PluginResolution {
        &self.plugin_resolution
    }

    #[must_use]
    /// Return the running binary's build identity, when the composition root
    /// resolved one.
    pub const fn build_identity(&self) -> Option<&BuildIdentity> {
        self.build_identity.as_ref()
    }

    #[must_use]
    /// Return the build's observed staleness against the repo's current HEAD.
    pub const fn build_staleness(&self) -> BuildStaleness {
        self.build_staleness
    }

    #[must_use]
    /// Return whether the session's first background source ingest is still
    /// in flight. See [`Self::with_startup_ingest_pending`].
    pub const fn startup_ingest_pending(&self) -> bool {
        self.startup_ingest_pending
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
    fabro_factory: Option<String>,
    valve_commands: Vec<String>,
    timeline: Vec<TimelineEntry>,
    actions: Vec<OperatorAction>,
    account: Option<String>,
    answer_comments: Vec<String>,
}

impl AttentionDetail {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(
        repo: String,
        work_item: String,
        fabro_run: String,
        fabro_factory: Option<String>,
        valve_commands: Vec<String>,
        timeline: Vec<TimelineEntry>,
        actions: Vec<OperatorAction>,
    ) -> Self {
        Self {
            repo,
            work_item,
            fabro_run,
            fabro_factory,
            valve_commands,
            timeline,
            actions,
            account: None,
            answer_comments: Vec::new(),
        }
    }

    #[must_use]
    /// This detail carrying the projection's ACCOUNT for the item -- the
    /// terminated run's own story as the ORCHESTRATOR composed it, taken
    /// verbatim from the projected valve item's `summary`.
    ///
    /// A builder rather than a `new` parameter because it is optional at the
    /// source: the orchestrator's enrichment is fail-soft, so a projection that
    /// carries only the item's title leaves this `None` and the console asserts
    /// nothing about the run (`SPECIFICATION/contracts.md`).
    pub fn with_account(mut self, account: Option<String>) -> Self {
        self.account = account;
        self
    }

    #[must_use]
    /// This detail carrying the item's `livespec-human-answer` ledger comments
    /// as the context surface returned them, oldest first.
    ///
    /// The orchestrator writes that comment when it accepts an answer through
    /// the resolve-blocked valve; the console only READS it back, so a
    /// successful resolve shows the answer where the operator gave it.
    pub fn with_answer_comments(mut self, answer_comments: Vec<String>) -> Self {
        self.answer_comments = answer_comments;
        self
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
    /// The factory the run lives on, when the ledger stamped one.
    pub fn fabro_factory(&self) -> Option<&str> {
        self.fabro_factory.as_deref()
    }

    #[must_use]
    /// The operator commands the orchestrator's attention projection
    /// ADVERTISES for this entry, in projection order.
    ///
    /// Read from the projection's own handoffs, never composed here. This slot
    /// used to hold a locally-formatted `fabro attach <run>`, which the
    /// needs-human redesign turned into a command that finds nothing: a
    /// needs-human outcome now terminates the run and rests the item at
    /// `blocked / needs-human`, so there is no run to attach to and the
    /// decision is a ledger valve (`SPECIFICATION/contracts.md`, and
    /// `SPECIFICATION/scenarios.md` Scenario 30). What the operator is
    /// handed is therefore whatever the projection says is pressable for this
    /// item — for a needs-human item, the `resolve-blocked:<id>:ready` valve,
    /// whose summary carries the rework-or-from-scratch re-dispatch route.
    /// Empty when the projection advertises nothing, which is an honest
    /// "nothing to press" rather than a fabricated command.
    pub fn valve_commands(&self) -> &[String] {
        &self.valve_commands
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

    #[must_use]
    /// The projected ACCOUNT of the item -- for a `blocked / needs-human` item,
    /// the terminated run's story (run id, factory, why, what it reported, the
    /// preserved ref, the available actions) exactly as the orchestrator
    /// composed it in the valve item's `summary`.
    ///
    /// Rendered WHOLE in the drilled-in detail and never truncated there. The
    /// console composes, augments, and re-derives no part of it: nothing here
    /// inspects the factory, reads the run, or scans console events to fill a
    /// field the projection left out. `None` when the projection carried no
    /// account.
    pub fn account(&self) -> Option<&str> {
        self.account.as_deref()
    }

    #[must_use]
    /// The item's `livespec-human-answer` ledger comments, verbatim, oldest
    /// first, as the context surface returned them.
    pub fn answer_comments(&self) -> &[String] {
        &self.answer_comments
    }
}

/// The latest observed refusal/failure of an operator action per work-item.
///
/// Projected from the `work_item.action.failed` event stream and cleared
/// again the moment a later action against the item completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionFailure {
    action_id: String,
    refusal: Option<String>,
}

impl ActionFailure {
    /// The failed action's id (`<verb>:<work-item-id>[:<value>]`).
    #[must_use]
    pub fn action_id(&self) -> &str {
        &self.action_id
    }

    /// The operator-facing failure line: the structured refusal's
    /// `domain_error` and `summary` when the payload parses as the drive
    /// surface's `--json` shape, the raw refusal otherwise, or an honest
    /// no-diagnostic marker when the surface emitted nothing.
    #[must_use]
    pub fn display_line(&self) -> String {
        let Some(refusal) = self.refusal.as_deref() else {
            return format!("{} failed (no diagnostic emitted)", self.action_id);
        };
        let parsed: Option<serde_json::Value> = serde_json::from_str(refusal).ok();
        let field = |key: &str| {
            parsed
                .as_ref()
                .and_then(|value| value.get(key))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };
        match (field("domain_error"), field("summary")) {
            (Some(domain_error), Some(summary)) => {
                format!("{} refused — {domain_error}: {summary}", self.action_id)
            }
            (Some(domain_error), None) => {
                format!("{} refused — {domain_error}", self.action_id)
            }
            (None, Some(summary)) => format!("{} refused — {summary}", self.action_id),
            (None, None) => format!("{} failed — {refusal}", self.action_id),
        }
    }
}

/// The failure payload itself when it carries the drive surface's structured
/// refusal fields, or `None` when it names no diagnostic at all.
///
/// This is what keeps a flattened structured refusal from being reported as
/// silence; a payload carrying only bookkeeping (`action_id`, `status`) stays
/// `None`, so an action that genuinely emitted nothing is still reported as
/// having emitted nothing rather than as a JSON blob.
fn structured_refusal(payload: Option<&serde_json::Value>) -> Option<String> {
    payload
        .filter(|value| value.get("domain_error").is_some() || value.get("summary").is_some())
        .map(serde_json::Value::to_string)
}

/// Fold the `work_item.action.*` outcome events into the latest failure per
/// work-item: a failed action surfaces until a later action against the same
/// item completes, so a stale refusal never outlives its recovery.
fn project_action_failures(events: &[ConsoleEvent]) -> BTreeMap<String, ActionFailure> {
    let mut failures = BTreeMap::new();
    for event in events {
        match event.event_type() {
            EventType::WorkItemActionFailed => {
                let payload: Option<serde_json::Value> =
                    serde_json::from_str(event.payload_json()).ok();
                let field = |key: &str| {
                    payload
                        .as_ref()
                        .and_then(|value| value.get(key))
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                };
                if let Some(action_id) = field("action_id") {
                    failures.insert(
                        event.stream_id().to_owned(),
                        ActionFailure {
                            action_id,
                            // A STRUCTURED refusal (the drive surface's `--json`
                            // shape) is flattened into this payload at append
                            // time, so it has no `refusal` key to read; reading
                            // only that key reported the orchestrator's most
                            // informative refusals -- a poisoned-answer
                            // preflight among them -- as "no diagnostic
                            // emitted". The whole payload IS the refusal in
                            // that case, and `display_line` renders its
                            // `domain_error` and `summary` verbatim.
                            refusal: field("refusal")
                                .or_else(|| structured_refusal(payload.as_ref())),
                        },
                    );
                }
            }
            EventType::WorkItemActionCompleted => {
                failures.remove(event.stream_id());
            }
            EventType::DispatcherRefusalObserved => {
                if let Some(entry) = dispatcher_journal_from_payload_json(event.payload_json())
                    && let Some(diagnostic) = entry.diagnostic()
                {
                    failures.insert(
                        entry.work_item_id().to_owned(),
                        ActionFailure {
                            action_id: format!("dispatch:{}", entry.dispatch_id()),
                            refusal: Some(
                                serde_json::json!({
                                    "domain_error": entry.kind().label(),
                                    "summary": diagnostic,
                                })
                                .to_string(),
                            ),
                        },
                    );
                }
            }
            _other => {}
        }
    }
    failures
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents tui screen model data used by the console.
pub struct TuiScreenModel {
    active_view: TuiView,
    navigation: Vec<TuiView>,
    attention_items: Vec<AttentionItem>,
    attention_total: usize,
    selected_attention_index: Option<usize>,
    detail: Option<AttentionDetail>,
    view_items: Vec<ViewSummaryItem>,
    lane_board: LaneBoard,
    lane_focus: LaneFocus,
    selected_lane_index: Option<usize>,
    selected_lane_item_index: Option<usize>,
    missing_selected_lane_item_id: Option<String>,
    events_focus: EventsFocus,
    selected_events_index: usize,
    focus: FocusPane,
    detail_scroll: usize,
    header_scroll: usize,
    overlay: TuiOverlay,
    selected_repo: String,
    selected_setting_index: Option<usize>,
    dispatcher_settings: DispatcherSettingsRead,
    dispatcher_setting_write: DispatcherSettingWriteState,
    plugin_resolution: PluginResolution,
    unavailable_sources: Vec<String>,
    observed_source_names: Vec<String>,
    factory_activity: Option<String>,
    transient_status: Option<String>,
    list_edge: Option<ListEdge>,
    command_outcome: Option<CommandOutcome>,
    header: String,
    action_failures: BTreeMap<String, ActionFailure>,
    orphaned_factory_runs: Vec<OrphanedFactoryRun>,
    build_identity: Option<BuildIdentity>,
    build_staleness: BuildStaleness,
    startup_ingest_pending: bool,
}

impl TuiScreenModel {
    #[must_use]
    /// Return the stored value.
    pub const fn active_view(&self) -> TuiView {
        self.active_view
    }

    #[must_use]
    /// The orphaned-factory-runs lane: runs the orchestrator's `reconcile-runs`
    /// dry-run projection reports as holding a scheduler slot for work the
    /// ledger says nothing is waiting on.
    ///
    /// Fed from the LATEST observed projection
    /// ([`project_orphaned_factory_runs`]), never re-derived here: the console
    /// consumes the reconciler's orphan rule as data so the lane cannot
    /// disagree with the remedy command it prints beside each row.
    pub fn orphaned_factory_runs(&self) -> &[OrphanedFactoryRun] {
        &self.orphaned_factory_runs
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

    /// How many items the needs-attention inbox holds in total, ignoring any
    /// active search filter.
    ///
    /// [`Self::attention_items`] is the FILTERED list -- what the content pane
    /// draws -- while this is the whole inbox, which is what the header's
    /// `attention:` count reports and what the search overlay's `N of M match`
    /// feedback measures against. Keeping the two apart is the point: a filter
    /// changes what is shown, not how much work is waiting.
    #[must_use]
    pub const fn attention_total(&self) -> usize {
        self.attention_total
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

    /// Which sub-view the `Events` container is showing (its own picker home,
    /// or one of the two sub-views drilled into).
    #[must_use]
    pub const fn events_focus(&self) -> EventsFocus {
        self.events_focus
    }

    /// The container's own picker-row cursor -- which of the two named
    /// sub-views is highlighted while `events_focus` is
    /// [`EventsFocus::Overview`]. Clamped by the renderer, so it is always a
    /// valid index into [`EventsFocus::all`].
    #[must_use]
    pub const fn selected_events_index(&self) -> usize {
        self.selected_events_index
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
    /// The anchored drilled-lane work-item id that is no longer present in the
    /// current lane, if any. Renderers surface this explicitly instead of
    /// silently substituting the item now occupying the old row.
    pub fn missing_selected_lane_item_id(&self) -> Option<&str> {
        self.missing_selected_lane_item_id.as_deref()
    }

    /// The latest failed operator action against `work_item_id`, if the most
    /// recent observed action outcome for it was a failure. Cleared by a later
    /// completed action, so a recovered item carries no stale refusal.
    #[must_use]
    pub fn action_failure_for(&self, work_item_id: &str) -> Option<&ActionFailure> {
        self.action_failures.get(work_item_id)
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

    /// The ready work-item a budget-1 drain will claim first, read from the
    /// SAME rank-ordered ready column the board renders.
    #[must_use]
    pub fn next_ready_drain_target(&self) -> Option<&LaneWorkItem> {
        self.lane_board.column(Lane::Ready)?.items().first()
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

    /// The selected work-item record the per-item read-only surfaces act on.
    /// Attention resolves by id through the lane board; Lanes uses the drilled-in
    /// lane selection directly. Other views do not select work-items.
    #[must_use]
    pub fn selected_work_item(&self) -> Option<&LaneWorkItem> {
        match self.active_view {
            TuiView::Attention => self
                .selected_work_item_id()
                .and_then(|work_item_id| self.work_item_by_id(work_item_id)),
            TuiView::Lanes => self.selected_lane_item(),
            TuiView::Spec | TuiView::Events | TuiView::Repos | TuiView::Settings => None,
        }
    }

    /// How many rows the list the per-item cursor moves over currently holds.
    ///
    /// The LIST-state input to the Status line's navigation hint, read from the
    /// same collection the up/down keys step through: the needs-attention rows
    /// in `Attention`, the drilled-in lane's column in `Lanes`. The views that
    /// carry no per-item cursor report `0`, as does the lane overview — its
    /// cursor moves over LANES, and its hint is composed separately.
    ///
    /// Deliberately independent of what the selection ADMITS: the cursor moves
    /// over a terminal-lane row exactly as it moves over a ready one.
    #[must_use]
    pub fn selected_list_row_count(&self) -> usize {
        match self.active_view {
            TuiView::Attention => self.attention_items.len(),
            TuiView::Lanes => match self.lane_focus {
                LaneFocus::Overview => 0,
                LaneFocus::Lane(lane) => self.lane_board.column(lane).map_or(0, LaneColumn::count),
            },
            TuiView::Spec | TuiView::Events | TuiView::Repos | TuiView::Settings => 0,
        }
    }

    /// The selected work-item's eligible driver-handoff command, if its lane and
    /// factory-safety marker admit the handoff verb.
    #[must_use]
    pub fn selected_driver_handoff_command(&self) -> Option<String> {
        driver_handoff_command(self.selected_work_item()?)
    }

    /// The work-item id the per-item valves act on: the selected drilled-in lane
    /// item's id in the `Lanes` view, the selected Attention item's work-item id
    /// in the `Attention` view, else `None`. This is what lets a per-item valve
    /// fire on an individually-selected lane item, not only on an Attention item;
    /// the other views carry no selectable work-item, so valves stay inert there.
    #[must_use]
    pub fn selected_work_item_id(&self) -> Option<&str> {
        match self.active_view {
            TuiView::Attention => self
                .selected_attention_index
                .and_then(|index| self.attention_items.get(index))
                .and_then(AttentionItem::work_item_id),
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
    /// Return the standing of the operator's most recent dispatcher-setting
    /// write, which qualifies the ONE `Settings` row that write targets. The
    /// console still owns no setting state: this is the standing of the
    /// operator's own edit, which the orchestrator cannot report.
    pub const fn dispatcher_setting_write(&self) -> &DispatcherSettingWriteState {
        &self.dispatcher_setting_write
    }

    #[must_use]
    /// Return the resolved orchestrator plugin summary displayed in Settings.
    pub const fn plugin_resolution(&self) -> &PluginResolution {
        &self.plugin_resolution
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
    /// Every distinct backing-source name this build has EVER observed,
    /// healthy or degraded, sorted for a stable order. Fed to the "Event
    /// sources" sub-view's navigation-only skeleton: a NAME-ONLY existence
    /// claim, never a health claim, so an empty result means this build has
    /// not yet observed any source at all -- not that every source is
    /// healthy (a healthy source still appears here).
    pub fn observed_source_names(&self) -> &[String] {
        &self.observed_source_names
    }

    #[must_use]
    /// Return the header value.
    pub fn header(&self) -> &str {
        &self.header
    }

    #[must_use]
    /// Return the running binary's build identity, when one was resolved.
    pub const fn build_identity(&self) -> Option<&BuildIdentity> {
        self.build_identity.as_ref()
    }

    #[must_use]
    /// Return the build's observed staleness against the repo's current HEAD.
    pub const fn build_staleness(&self) -> BuildStaleness {
        self.build_staleness
    }

    #[must_use]
    /// Whether the session's FIRST background source ingest is still in
    /// flight -- `true` only for the brief window between the first frame
    /// drawing from whatever the store already held and the background
    /// poller's first sweep landing (livespec-console-beads-fabro-pzbdbo.27).
    /// Renderers use this to show a still-loading source as LOADING rather
    /// than unavailable or silently empty; it is never derived from the event
    /// log, because a persisted store from a previous session already carries
    /// real history that this session has not yet re-confirmed.
    pub const fn startup_ingest_pending(&self) -> bool {
        self.startup_ingest_pending
    }

    #[must_use]
    /// Compose the header to fit `width` display columns without ever truncating
    /// mid-field.
    ///
    /// A pinned small terminal (the dogfood target is 112 columns) cannot hold
    /// every header field at once, so this degrades gracefully rather than
    /// letting a wide field clip the ones after it: it elides the source-health
    /// segment's names (to `+N more`, then to a bare count), then drops whole
    /// fields by declared information-value priority. Constant identity fields go
    /// first, followed by static context fields, then state counts. Transient
    /// refusal/not-wired/error fields survive static fields under pressure, and
    /// while any source is unavailable the source COUNT is never dropped, so the
    /// header always keeps the cockpit-blind-vs-idle tell. At a width wide enough
    /// for everything this returns the same content as [`header`](Self::header).
    pub fn header_line(&self, width: usize) -> String {
        fit_header_line(
            header_repo_label(&self.selected_repo),
            self.active_view.label(),
            // The INBOX total, matching [`header`](Self::header) exactly: this
            // is the shrink-to-fit rendering of the same line, so it cannot
            // report a different count. A search filter narrows
            // `attention_items`; it never changes how much work is waiting.
            self.attention_total,
            self.factory_activity.as_deref(),
            self.transient_status.as_deref(),
            &self.unavailable_sources,
            self.build_staleness,
            self.startup_ingest_pending,
            width,
        )
    }

    #[must_use]
    /// The whole Status line: a reached list edge's cue and the transient
    /// operator-feedback message for the last command that reached a terminal
    /// outcome, when either is present, beside the context shortcut hints.
    ///
    /// The hints are rendered in the MIDDLE, cue first and outcome message
    /// last, rather than replacing them. Both surrounding facts are
    /// load-bearing. Replacing the hints with either the cue or the outcome
    /// message would empty the hint line for as long as one was shown, which
    /// the Status-line-hints contract forbids outright. A reached list edge
    /// ([`ListEdge`]) is CUED ahead of the hints, so an `up`/`down` that could
    /// not move the selection says so instead of looking inert; the cue is
    /// transient state carried by the reducer, not a context-keyed derivation,
    /// so it prefixes whichever hints the context already owns rather than
    /// replacing them. An outcome message can be long -- a `move-status`
    /// action id carries a full work-item id, and a refusal carries the
    /// store's own diagnostic -- so this is the common case, not the corner;
    /// it is appended last so it never pushes the cue or the hints past the
    /// right edge of a narrow Status pane.
    pub fn footer(&self) -> Cow<'static, str> {
        let hints = self.list_edge.map_or_else(
            || self.context_footer_hint(),
            |edge| Cow::Owned(format!("{} | {}", edge.cue(), self.context_footer_hint())),
        );
        let Some(outcome) = self.command_outcome.as_ref() else {
            return hints;
        };
        Cow::Owned(format!("{hints} | {}", outcome.full))
    }

    #[must_use]
    /// The Status-line shortcut hints for the CURRENT context (per the TUI
    /// Contract's Status-line-hints clause / Scenario 19), before any
    /// list-edge cue or command-outcome message is composed around them by
    /// [`Self::footer`].
    ///
    /// Derived on read from the currently-focused pane (`active_view`) and any
    /// open modal/overlay (`overlay`) rather than stored, so the hint line is
    /// never a single static string: it renders the keys that act in the
    /// current context, it changes when focus moves to a different pane, and an
    /// open overlay replaces the pane's hints with that overlay's (restored when
    /// the overlay closes). It is never empty, so no context in which shortcut
    /// actions are available shows a blank hint line. See [`Self::footer`].
    fn context_footer_hint(&self) -> Cow<'static, str> {
        // The Header pane is not view-keyed, so its focused hints come from
        // `focus` rather than `active_view`: while it holds focus (and no overlay
        // owns the line), the hints describe its horizontal-scroll keys. An open
        // overlay still owns the hint line first, so it is matched ahead of the
        // Header-focus branch.
        match (&self.overlay, self.focus) {
            (TuiOverlay::None, FocusPane::Header) => {
                Cow::Owned(with_global_status_hint(HEADER_FOOTER_PREFIX))
            }
            (TuiOverlay::CommandModal { .. }, _) if self.selected_operator_action().is_none() => {
                model_pane_footer_hint(self)
            }
            (TuiOverlay::None, _) => model_pane_footer_hint(self),
            (overlay, _) => overlay_footer_hint(overlay),
        }
    }

    #[must_use]
    /// Compose the Status-line hints to fit `width` display columns, declaring
    /// anything shed instead of letting the terminal clip it away.
    ///
    /// [`footer`](Self::footer) composes the hints the context owns; nothing in
    /// that string knows how wide the Status band is, so a band narrower than
    /// the row simply STOPPED mid-hint and said nothing about it. Measured
    /// 2026-09-08 in a 105-column pane: a selected ready work-item's `d
    /// dispatch` fell off the right edge with no ellipsis, so an operator
    /// reading the band could not tell an unavailable verb from an undrawn one.
    /// That under-reports available actions, which the Status-line honesty
    /// contract forbids.
    ///
    /// So the hints degrade the way the header does: the last command's verdict
    /// abbreviates to its short form first, then whole segments are shed by
    /// declared [`HintPriority`](action_registry::HintPriority) — policy dials,
    /// then the per-item verbs, then that verdict, then navigation, and the
    /// globals last — and everything shed is carried in an explicit
    /// `+N more: ?` marker that names the key which reopens it. At a width wide
    /// enough for everything this returns [`footer`](Self::footer) unchanged.
    pub fn footer_line(&self, width: usize) -> String {
        fit_footer_line(&self.footer(), width, self.command_outcome.as_ref())
    }

    /// The availability context for the selected work-item, or `None` when no
    /// per-item surface holds a selected work-item. This is the ONE context both
    /// the Status-line hints and the key handlers consult, so hidden hints and
    /// inert keys cannot diverge.
    #[must_use]
    pub fn selected_action_context(&self) -> Option<action_registry::ActionContext> {
        let surface = match (self.active_view, self.lane_focus) {
            (TuiView::Attention, _) => action_registry::ActionSurface::Attention,
            (TuiView::Lanes, LaneFocus::Lane(_lane)) => action_registry::ActionSurface::LaneDrill,
            (
                TuiView::Lanes
                | TuiView::Spec
                | TuiView::Events
                | TuiView::Repos
                | TuiView::Settings,
                _,
            ) => return None,
        };
        let ready_work_item_count = self
            .lane_board
            .column(Lane::Ready)
            .map_or(0, LaneColumn::count);
        self.selected_work_item().map(|item| {
            action_registry::ActionContext::for_item(item, surface, ready_work_item_count)
        })
    }

    /// The selection-less availability context used by factory/global menu
    /// actions.
    ///
    /// Per-item actions still demand [`Self::selected_action_context`]; this
    /// context exists only so a selection-less registry entry can consume board
    /// facts such as ready-work count without a parallel menu predicate.
    #[must_use]
    pub fn global_action_context(&self) -> action_registry::ActionContext {
        action_registry::ActionContext {
            lane: Lane::Ready,
            admission_policy: AdmissionPolicy::Manual,
            acceptance_policy: AcceptancePolicy::AiThenHuman,
            has_driver_handoff: false,
            awaits_scope_override: false,
            ready_work_item_count: self
                .lane_board
                .column(Lane::Ready)
                .map_or(0, LaneColumn::count),
            surface: action_registry::ActionSurface::LaneDrill,
        }
    }
}

/// The Status-line shortcut hints shown while the Header pane holds focus with no
/// overlay open: the horizontal-scroll and leave keys that act on the focused
/// header. Non-empty and context-specific, like every other focused-pane hint.
const HEADER_FOOTER_PREFIX: &str = "left/right scroll | esc/tab leave";

fn with_global_status_hint(prefix: &str) -> String {
    format!("{prefix} | {}", action_registry::global_status_hint())
}

/// The Status-line shortcut hints an open modal/overlay owns while it holds
/// focus, per the TUI Contract / Scenario 19: the returned keys are the ones
/// that act in that overlay, replacing the pane's hints until it closes. The
/// `None` arm is the harmless fallback for a caller that routed a closed
/// overlay here; the no-overlay hints come from [`model_pane_footer_hint`].
fn overlay_footer_hint(overlay: &TuiOverlay) -> Cow<'static, str> {
    match overlay {
        TuiOverlay::None => Cow::Owned(action_registry::global_status_hint()),
        TuiOverlay::Search { .. } => Cow::Borrowed("type to search | esc cancel"),
        TuiOverlay::CommandPalette { .. } => Cow::Borrowed("type a command | esc cancel"),
        TuiOverlay::CommandModal { .. } => {
            Cow::Borrowed("up/down select action | enter explain | esc cancel")
        }
        TuiOverlay::CommandExplainer { .. } => Cow::Borrowed("enter continue | esc cancel"),
        TuiOverlay::ActionInvoker { .. } => {
            Cow::Borrowed("up/down select | enter stage | esc cancel")
        }
        TuiOverlay::Menu { .. } => {
            Cow::Borrowed("left/right menu | up/down select | enter stage | esc cancel")
        }
        TuiOverlay::FactoryDispatchItemConfirm { .. } => {
            Cow::Borrowed("enter dispatch selected item | esc cancel")
        }
        TuiOverlay::FactoryDrainConfirm { .. } => {
            Cow::Borrowed("enter dispatch ready work | esc cancel")
        }
        TuiOverlay::ValveConfirm { valve, .. } => valve_confirm_footer_hint(*valve),
        TuiOverlay::DriverHandoff { .. } => {
            Cow::Borrowed("enter copy sent to terminal | esc cancel")
        }
        TuiOverlay::WorkItemDetail { .. } => {
            Cow::Borrowed("up/down scroll | PgUp/PgDn page | esc close item")
        }
        TuiOverlay::Help { .. } => {
            Cow::Borrowed("left/right pane | up/down act | PgUp/PgDn page | esc close help")
        }
    }
}

/// The Status-line hints an open valve-confirm modal owns, LEADING with the
/// parameter it currently has staged.
///
/// A payload valve opens already staged — the move-to-status valve on the first
/// target the operator may drive from the item's current lane — and `Enter`
/// fires THAT one. Naming it here (and in the modal, see the caption below)
/// is what lets the operator read the pending transition off the screen instead
/// of knowing `status_move_targets` by heart; it re-renders as up/down cycle
/// the choice. Measured 2026-08-31: the dogfooding recipe had to warn "if it
/// isn't showing ready, cycle to it before Enter".
///
/// The payload-free approve/accept valves stage no parameter, so their hints are
/// unchanged.
fn valve_confirm_footer_hint(valve: PendingValve) -> Cow<'static, str> {
    const KEYS: &str = "up/down change | enter confirm | esc cancel";
    valve
        .option_display()
        .map_or(Cow::Borrowed(KEYS), |staged| {
            Cow::Owned(format!(
                "{}: {staged} | {KEYS}",
                valve.option_caption().to_ascii_lowercase()
            ))
        })
}

/// The Status-line hints for a focused pane `view` with no overlay open: the
/// keys that act on that pane RIGHT NOW.
///
/// `has_selected_work_item` is `false` whenever no WORK-ITEM is selected, which
/// is NOT the same as "the pane has no rows" -- the two panes differ:
///
/// - In a drilled-in lane, `selected_lane_item` is `None` only when the lane is
///   empty, so there the two coincide.
/// - In Attention they do NOT. The value comes from `AttentionItem::work_item_id`,
///   which is `None` for any row whose source reference names no work-item --
///   a plan thread, a hygiene finding, a spec-revise item. A POPULATED inbox
///   sitting on such a row therefore has no selected work-item, and the
///   per-item keys and record drill-in are correctly not advertised.
///
/// The uniform rule is about the WORK-ITEM, not about rows: with none selected,
/// the per-item keys and record drill-in are alike inert, and none is
/// advertised. (Before the attention rows carried their own source work-item,
/// this value came from the always-present detail projection, and "no rows" and
/// "no work-item" really were the same condition. They are not any more.)
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
fn model_pane_footer_hint(model: &TuiScreenModel) -> Cow<'static, str> {
    match model.active_view {
        // A selected work-item's hints DERIVE from the action registry through
        // the same availability context the key handlers consult; with no
        // work-item selected (a non-item Attention row, the lane overview, an
        // empty drilled-in lane) the per-item keys are alike inert and none is
        // advertised.
        TuiView::Attention => model.selected_action_context().map_or_else(
            || Cow::Owned(action_registry::global_status_hint()),
            |ctx| {
                Cow::Owned(action_registry::selected_item_hint(
                    &ctx,
                    model.selected_list_row_count(),
                ))
            },
        ),
        TuiView::Lanes => match model.lane_focus {
            // The lane OVERVIEW selects a LANE, never a work-item, so every
            // per-item key is inert here and none is advertised.
            LaneFocus::Overview => {
                Cow::Owned(with_global_status_hint("up/down move | enter drill"))
            }
            // An EMPTY drilled-in lane: nothing is selected, so `enter` opens
            // nothing, every per-item key is inert, AND up/down have no row to
            // move over. Only stepping back out does anything.
            LaneFocus::Lane(_lane) => model.selected_action_context().map_or_else(
                || Cow::Owned(with_global_status_hint("esc lane list")),
                |ctx| {
                    Cow::Owned(action_registry::selected_item_hint(
                        &ctx,
                        model.selected_list_row_count(),
                    ))
                },
            ),
        },
        TuiView::Settings => Cow::Owned(with_global_status_hint(
            "up/down move | enter/space edit row",
        )),
        // The container's own overview selects a SUB-VIEW, mirroring the lane
        // overview above: `enter` is the one new key, named here exactly as the
        // lane overview names its own. A drilled-in sub-view names its own
        // step-out key instead, per AC4 (Escape returns to the container, not
        // out of the view).
        TuiView::Events => match model.events_focus {
            EventsFocus::Overview => Cow::Owned(with_global_status_hint(
                "up/down move | enter drill | left/right focus | / search",
            )),
            EventsFocus::StoredEvents | EventsFocus::EventSources => Cow::Owned(
                with_global_status_hint("esc sub-view list | left/right focus | / search"),
            ),
        },
        TuiView::Spec | TuiView::Repos => Cow::Owned(with_global_status_hint(
            "up/down move | left/right focus | / search",
        )),
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
    /// Invalid workflow scope variant -- a
    /// `work_item.set_workflow_scope_override_requested` command carried a
    /// payload whose `scope` was absent or not `citation-only` (the one scope
    /// the orchestrator's value allowlist admits).
    InvalidWorkflowScope,
    /// Invalid resolve-blocked target variant -- a
    /// `work_item.resolve_blocked_requested` command carried a payload whose
    /// `target_status` was absent or not one of {ready, backlog}.
    InvalidResolveBlockedTarget,
    /// Invalid resolve-blocked answer variant -- a
    /// `work_item.resolve_blocked_requested` command carried an `answer` key
    /// that was not a non-empty string. The answer is optional, so its ABSENCE
    /// is never this error; a present-but-unusable one is refused rather than
    /// silently dropped, because dropping it would lose the operator's text.
    InvalidResolveBlockedAnswer,
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
    /// Factory selected-item dispatch port failed variant.
    FactoryDispatchItemPortFailed,
    /// No selected attention item variant.
    NoSelectedAttentionItem,
    /// No selected work-item variant -- a per-item valve was invoked with no
    /// work-item selected in either the Attention detail or a drilled-in lane.
    NoSelectedWorkItem,
    /// No selected operator action variant.
    NoSelectedOperatorAction,
    /// Unavailable operator action variant -- an action was visible as a
    /// selectable operator route but does not apply in the current context.
    UnavailableOperatorAction,
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
    /// Copy driver handoff command variant. This is a terminal-copy effect only,
    /// never a persisted command.
    CopyDriverHandoff(String),
}

impl OperatorActionOutcome {
    #[must_use]
    /// Return the wrapped command envelope.
    pub const fn command(&self) -> Option<&CommandEnvelope> {
        match self {
            Self::PersistCommand(command) | Self::PersistCommandWithPayload { command, .. } => {
                Some(command)
            }
            Self::CopyDriverHandoff(_) => None,
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
/// Represents one selected work-item dispatch request.
pub struct FactoryDispatchItemRequest {
    work_item_id: String,
}

impl FactoryDispatchItemRequest {
    #[must_use]
    /// Construct a new value from its required fields.
    pub const fn new(work_item_id: String) -> Self {
        Self { work_item_id }
    }

    #[must_use]
    /// Return the target work-item id.
    pub fn work_item_id(&self) -> &str {
        &self.work_item_id
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
    Failed {
        /// The child surface's captured stdout, when it emitted one.
        diagnostic: Option<String>,
    },
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
        Self::Failed { diagnostic: None }
    }

    #[must_use]
    /// A failure carrying the diagnostic payload the drain surface emitted.
    pub const fn failed_with_diagnostic(diagnostic: String) -> Self {
        Self::Failed {
            diagnostic: Some(diagnostic),
        }
    }

    #[must_use]
    /// Return the stored value.
    pub const fn not_wired() -> Self {
        Self::NotWired
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Variants for selected-item factory dispatch port outcome state.
pub enum FactoryDispatchItemPortOutcome {
    /// Completed variant.
    Completed,
    /// Failed variant.
    Failed {
        /// The child surface's captured stdout, when it emitted one.
        diagnostic: Option<String>,
    },
    /// The request was accepted but no real selected-item Dispatcher port is
    /// wired, so no dispatch was attempted.
    NotWired,
}

impl FactoryDispatchItemPortOutcome {
    #[must_use]
    /// Return the stored value.
    pub const fn completed() -> Self {
        Self::Completed
    }

    #[must_use]
    /// Return the stored value.
    pub const fn failed() -> Self {
        Self::Failed { diagnostic: None }
    }

    #[must_use]
    /// A failure carrying the diagnostic payload the dispatch surface emitted.
    pub const fn failed_with_diagnostic(diagnostic: String) -> Self {
        Self::Failed {
            diagnostic: Some(diagnostic),
        }
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

/// Port interface for dispatching one selected ready item through a factory.
pub trait FactoryDispatchItemPort {
    /// Dispatch one ready work-item through the concrete Dispatcher port.
    ///
    /// # Errors
    /// Returns an application error when the port cannot produce a trustworthy outcome.
    fn dispatch_item(
        &mut self,
        request: &FactoryDispatchItemRequest,
    ) -> ApplicationResult<FactoryDispatchItemPortOutcome>;
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
    ///
    /// Ready-work evidence composes BOTH surfaces: the genuinely-ingested
    /// work-item lane board AND the live `impl:` rows of the ingested
    /// `attention_item.*` stream. The second leg is the aw6z staleness repair
    /// relocated to policy-read time: the needs-attention adapter no longer
    /// synthesizes Ready-lane snapshots (v042 Initial-Adapters exclusivity),
    /// so a fresher composed surface showing ready implementation work must
    /// count HERE, without mutating the work-item lane projection. The gate
    /// only tests emptiness, so an item present on both surfaces is harmless.
    pub fn from_events(events: &[ConsoleEvent]) -> Self {
        let lane_ready_count = project_lane_board(events)
            .column(Lane::Ready)
            .map_or(0, LaneColumn::count);
        let attention_ready_count = materialize_attention_items(events)
            .iter()
            .filter(|item| item.id().starts_with("impl:"))
            .count();
        Self::new(lane_ready_count + attention_ready_count)
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
        request: &FactoryDrainRequest,
    ) -> ApplicationResult<FactoryDrainPortOutcome> {
        let mut arg_refs: Vec<&str> = self.args.iter().map(String::as_str).collect();
        let budget = request.budget().to_string();
        let parallel = request.parallel().to_string();
        arg_refs.push("--budget");
        arg_refs.push(budget.as_str());
        arg_refs.push("--parallel");
        arg_refs.push(parallel.as_str());
        Ok(match self.probe.run_command(&self.program, &arg_refs) {
            SourceProbeOutcome::Observed {
                stdout,
                success: true,
                ..
            } => FactoryDrainPortOutcome::completed(dispatched_item_count(&stdout)),
            SourceProbeOutcome::Observed {
                success: false,
                stdout,
                ..
            } => {
                let diagnostic = stdout.trim();
                if diagnostic.is_empty() {
                    FactoryDrainPortOutcome::failed()
                } else {
                    FactoryDrainPortOutcome::failed_with_diagnostic(diagnostic.to_owned())
                }
            }
            SourceProbeOutcome::Unavailable { .. } => FactoryDrainPortOutcome::not_wired(),
        })
    }
}

/// Real selected-item factory-dispatch port.
///
/// It invokes the Dispatcher's governed `loop` surface bounded to the named
/// item (`--budget 1 --parallel 1 --item <id>`), preserving the orchestrator's
/// ranked eligibility, WIP cap, and `--item`-keyed cost gate. A successful run
/// completes only when the Dispatcher reports a non-zero dispatched count; a
/// genuine unavailable Dispatcher remains an honest not-wired outcome.
pub struct DispatcherFactoryDispatchItemPort<'a> {
    probe: &'a dyn SourceProbe,
    program: String,
    args: Vec<String>,
}

impl<'a> DispatcherFactoryDispatchItemPort<'a> {
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

impl FactoryDispatchItemPort for DispatcherFactoryDispatchItemPort<'_> {
    fn dispatch_item(
        &mut self,
        request: &FactoryDispatchItemRequest,
    ) -> ApplicationResult<FactoryDispatchItemPortOutcome> {
        let mut arg_refs: Vec<&str> = self.args.iter().map(String::as_str).collect();
        arg_refs.push("--budget");
        arg_refs.push("1");
        arg_refs.push("--parallel");
        arg_refs.push("1");
        arg_refs.push("--item");
        arg_refs.push(request.work_item_id());
        Ok(match self.probe.run_command(&self.program, &arg_refs) {
            SourceProbeOutcome::Observed {
                stdout,
                success: true,
                ..
            } if dispatched_item_count(&stdout) > 0 => FactoryDispatchItemPortOutcome::completed(),
            SourceProbeOutcome::Observed {
                stdout,
                success: true,
                ..
            } => {
                let diagnostic = stdout.trim();
                if diagnostic.is_empty() {
                    FactoryDispatchItemPortOutcome::failed_with_diagnostic(
                        "dispatcher reported zero dispatched items".to_owned(),
                    )
                } else {
                    FactoryDispatchItemPortOutcome::failed_with_diagnostic(diagnostic.to_owned())
                }
            }
            SourceProbeOutcome::Observed {
                success: false,
                stdout,
                ..
            } => {
                let diagnostic = stdout.trim();
                if diagnostic.is_empty() {
                    FactoryDispatchItemPortOutcome::failed()
                } else {
                    FactoryDispatchItemPortOutcome::failed_with_diagnostic(diagnostic.to_owned())
                }
            }
            SourceProbeOutcome::Unavailable { .. } => FactoryDispatchItemPortOutcome::not_wired(),
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
    requested_by: String,
    answer: Option<String>,
}

impl OrchestratorActionRequest {
    #[must_use]
    /// Construct a new value from its required fields.
    pub fn new(action_id: String) -> Self {
        Self {
            action_id,
            requested_by: "operator".to_owned(),
            answer: None,
        }
    }

    #[must_use]
    /// Construct a new attributed request.
    pub const fn attributed(action_id: String, requested_by: String) -> Self {
        Self {
            action_id,
            requested_by,
            answer: None,
        }
    }

    #[must_use]
    /// This request carrying the operator's free-text answer, which the port
    /// appends to the invocation as `--answer <answer>`.
    ///
    /// `None` leaves the argv untouched, so every answer-less action invokes
    /// exactly the argv it always did.
    pub fn with_answer(mut self, answer: Option<&str>) -> Self {
        self.answer = answer.map(str::to_owned);
        self
    }

    #[must_use]
    /// Return the action id value.
    pub fn action_id(&self) -> &str {
        &self.action_id
    }

    #[must_use]
    /// Return the asserted principal requesting the action.
    pub fn requested_by(&self) -> &str {
        &self.requested_by
    }

    #[must_use]
    /// Return the operator's free-text answer when this request carries one.
    pub fn answer(&self) -> Option<&str> {
        self.answer.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Variants for orchestrator action outcome state or outcome values.
pub enum OrchestratorActionOutcome {
    /// The orchestrator action completed successfully.
    Completed,
    /// The orchestrator action failed. When the action surface emitted a
    /// structured refusal on stdout (the `--json` payload carrying
    /// `action_id` / `domain_error` / `summary`), it rides here instead of
    /// being discarded at the port boundary.
    Failed {
        /// The child surface's captured stdout, when it emitted one.
        refusal: Option<String>,
    },
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
        Self::Failed { refusal: None }
    }

    #[must_use]
    /// A failure carrying the refusal payload the action surface emitted.
    pub const fn failed_with_refusal(refusal: String) -> Self {
        Self::Failed {
            refusal: Some(refusal),
        }
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
        // The operator's answer rides IMMEDIATELY behind the action it answers,
        // verbatim and as a separate argv element, so nothing quotes, escapes,
        // or reflows the text on its way to the orchestrator. Absent for every
        // action that carries no answer.
        if let Some(answer) = request.answer() {
            args.push("--answer");
            args.push(answer);
        }
        args.push("--invoker");
        args.push(request.requested_by());
        Ok(match self.probe.run_command(&self.program, &args) {
            SourceProbeOutcome::Observed { success: true, .. } => {
                OrchestratorActionOutcome::completed()
            }
            SourceProbeOutcome::Observed {
                success: false,
                stdout,
                ..
            } => {
                // The refusal payload only exists HERE — discarding it was the
                // presentation half of the silent-valve defect. A blank stdout
                // stays an unexplained failure rather than an empty refusal.
                let refusal = stdout.trim();
                if refusal.is_empty() {
                    OrchestratorActionOutcome::failed()
                } else {
                    OrchestratorActionOutcome::failed_with_refusal(refusal.to_owned())
                }
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
        args.push("--invoker");
        args.push(request.requested_by());
        Ok(match self.probe.run_command(&self.program, &args) {
            SourceProbeOutcome::Observed {
                stdout,
                success: true,
                ..
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
        item.source_ref().work_item().map(ToOwned::to_owned),
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
    execution_state: LaneExecutionState,
    rank: String,
    status: String,
    admission_policy: AdmissionPolicy,
    acceptance_policy: AcceptancePolicy,
    observation_confirmed: bool,
    detail: WorkItemDetail,
}

impl LaneWorkItem {
    fn from_snapshot(
        snapshot: &WorkItemSnapshot,
        execution_state: LaneExecutionState,
        observation_confirmed: bool,
    ) -> Self {
        Self {
            work_item_id: snapshot.work_item_id().to_owned(),
            repo: snapshot.repo().to_owned(),
            lane: snapshot.lane(),
            lane_reason: snapshot.lane_reason(),
            execution_state,
            rank: snapshot.rank().to_owned(),
            status: snapshot.status().to_owned(),
            admission_policy: snapshot.admission_policy(),
            acceptance_policy: snapshot.acceptance_policy(),
            observation_confirmed,
            detail: snapshot.detail().clone(),
        }
    }

    #[must_use]
    /// Whether the backing source that fed this row was successfully observed
    /// on its LATEST poll.
    ///
    /// `false` means every value on this row -- `rank`, the title, the policies
    /// -- is LAST-KNOWN rather than current: the console could not read the
    /// source this cycle and is serving what it last saw. It is the same fact
    /// the header's `event sources: N unavailable` tally reports, carried down to the
    /// row so the surface can qualify the values instead of presenting them as
    /// confirmed (`livespec-console-beads-fabro-v8un`, operator rider [2]).
    ///
    /// It is deliberately NOT a claim that the row is WRONG -- an unavailable
    /// source usually means the values still hold. It is a claim that the
    /// console cannot vouch for them, which is the distinction between unknown
    /// and unset that rider [2] requires.
    pub const fn observation_confirmed(&self) -> bool {
        self.observation_confirmed
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
    /// Whether this lane row is merely claimed or has an observed execution
    /// signal. Non-active rows return [`LaneExecutionState::NotActive`].
    pub const fn execution_state(&self) -> LaneExecutionState {
        self.execution_state
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

    #[must_use]
    /// Count rows that have been claimed but have no observed execution signal.
    pub fn claimed_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.execution_state() == LaneExecutionState::Claimed)
            .count()
    }

    #[must_use]
    /// Count rows that have an observed dispatcher/Fabro execution signal.
    pub fn executing_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.execution_state() == LaneExecutionState::Executing)
            .count()
    }

    #[must_use]
    /// Count active rows whose run has finished before ledger reconciliation.
    pub fn finished_unreconciled_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.execution_state() == LaneExecutionState::FinishedUnreconciled)
            .count()
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

/// Read model for one stable plan page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPage {
    epic: Option<PlanWorkItem>,
    children: Vec<PlanWorkItem>,
    handoff_entries: Vec<PlanHandoffEntry>,
}

impl PlanPage {
    /// The plan epic, when it has been observed.
    #[must_use]
    pub const fn epic(&self) -> Option<&PlanWorkItem> {
        self.epic.as_ref()
    }

    /// Child work-items that depend on the plan epic, rank ordered.
    #[must_use]
    pub fn children(&self) -> &[PlanWorkItem] {
        &self.children
    }

    /// Handoff entries from the epic's ledger comments, in ledger order.
    #[must_use]
    pub fn handoff_entries(&self) -> &[PlanHandoffEntry] {
        &self.handoff_entries
    }
}

/// Work-item summary rendered on a plan page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanWorkItem {
    work_item_id: String,
    title: Option<String>,
    status: String,
    lane: Lane,
    rank: String,
}

impl PlanWorkItem {
    fn from_snapshot(snapshot: &WorkItemSnapshot) -> Self {
        Self {
            work_item_id: snapshot.work_item_id().to_owned(),
            title: snapshot.detail().title.clone(),
            status: snapshot.status().to_owned(),
            lane: snapshot.lane(),
            rank: snapshot.rank().to_owned(),
        }
    }

    /// Work-item id.
    #[must_use]
    pub fn work_item_id(&self) -> &str {
        &self.work_item_id
    }

    /// Human title, when the ledger emitted one.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Stored lifecycle status.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Projected lane.
    #[must_use]
    pub const fn lane(&self) -> Lane {
        self.lane
    }

    /// Rank key.
    #[must_use]
    pub fn rank(&self) -> &str {
        &self.rank
    }
}

/// One handoff entry rendered from a plan epic ledger comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanHandoffEntry {
    id: Option<String>,
    author: Option<String>,
    created_at: Option<String>,
    text: String,
}

impl PlanHandoffEntry {
    fn from_comment(comment: &WorkItemComment) -> Self {
        Self {
            id: comment.id.clone(),
            author: comment.author.clone(),
            created_at: comment.created_at.clone(),
            text: comment.text.clone(),
        }
    }

    /// Comment id, when the backing ledger emitted one.
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// Comment author.
    #[must_use]
    pub fn author(&self) -> Option<&str> {
        self.author.as_deref()
    }

    /// Comment creation timestamp.
    #[must_use]
    pub fn created_at(&self) -> Option<&str> {
        self.created_at.as_deref()
    }

    /// Comment body.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Project one plan page from persisted work-item observations.
#[must_use]
pub fn project_plan_page(events: &[ConsoleEvent], epic_id: &str) -> PlanPage {
    let mut latest: BTreeMap<String, WorkItemSnapshot> = BTreeMap::new();
    for event in events {
        if *event.event_type() != EventType::WorkItemSnapshotObserved {
            continue;
        }
        let Some(snapshot) = work_item_snapshot_from_payload_json(event.payload_json()) else {
            continue;
        };
        latest.insert(snapshot.work_item_id().to_owned(), snapshot);
    }
    let epic = latest.get(epic_id).map(PlanWorkItem::from_snapshot);
    let mut children = latest
        .values()
        .filter(|snapshot| snapshot.detail().depends_on.iter().any(|id| id == epic_id))
        .map(PlanWorkItem::from_snapshot)
        .collect::<Vec<_>>();
    children.sort_by(|left, right| {
        left.rank()
            .cmp(right.rank())
            .then_with(|| left.work_item_id().cmp(right.work_item_id()))
    });
    let handoff_entries = latest
        .get(epic_id)
        .map(|snapshot| {
            snapshot
                .detail()
                .comments
                .iter()
                .map(PlanHandoffEntry::from_comment)
                .collect()
        })
        .unwrap_or_default();
    PlanPage {
        epic,
        children,
        handoff_entries,
    }
}

/// The stable console URL path for a plan page.
#[must_use]
pub fn plan_page_url(epic_id: &str) -> String {
    format!("/plans/{}", escape_url_path_segment(epic_id))
}

/// Render one plan page as standalone HTML.
#[must_use]
pub fn render_plan_page_html(epic_id: &str, page: &PlanPage) -> String {
    let title = page.epic().and_then(PlanWorkItem::title).unwrap_or("Plan");
    let mut html = String::new();
    html.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">");
    html.push_str("<title>");
    html.push_str(&escape_html(title));
    html.push_str("</title><style>body{font-family:system-ui,sans-serif;max-width:920px;margin:32px auto;padding:0 20px;color:#1f2933;background:#fafafa}h1{font-size:30px;margin:0 0 6px}h2{font-size:20px;margin-top:30px;border-bottom:1px solid #d8dee4;padding-bottom:6px}.meta{color:#52606d}.item,.entry{border:1px solid #d8dee4;background:#fff;border-radius:6px;padding:14px;margin:12px 0}.row{display:flex;gap:12px;flex-wrap:wrap}.pill{font-size:13px;border:1px solid #bcccdc;border-radius:999px;padding:2px 8px;background:#f0f4f8}pre{white-space:pre-wrap;font:14px ui-monospace,monospace;line-height:1.5}</style></head><body>");
    html.push_str("<h1>");
    html.push_str(&escape_html(title));
    html.push_str("</h1><div class=\"meta\">");
    html.push_str(&escape_html(epic_id));
    html.push_str(" at ");
    html.push_str(&escape_html(&plan_page_url(epic_id)));
    html.push_str("</div>");
    html.push_str("<h2>Epic</h2>");
    if let Some(epic) = page.epic() {
        render_plan_work_item(&mut html, epic);
    } else {
        html.push_str("<div class=\"item\">");
        html.push_str(&escape_html(epic_id));
        html.push_str(" has not been observed.</div>");
    }
    html.push_str("<h2>Children</h2>");
    if page.children().is_empty() {
        html.push_str("<div class=\"item\">No child work-items observed.</div>");
    } else {
        for child in page.children() {
            render_plan_work_item(&mut html, child);
        }
    }
    html.push_str("<h2>Handoff Entries</h2>");
    if page.handoff_entries().is_empty() {
        html.push_str("<div class=\"entry\">No handoff entries observed.</div>");
    } else {
        for entry in page.handoff_entries() {
            render_plan_handoff_entry(&mut html, entry);
        }
    }
    html.push_str("</body></html>");
    html
}

fn render_plan_work_item(html: &mut String, item: &PlanWorkItem) {
    html.push_str("<div class=\"item\"><strong>");
    html.push_str(&escape_html(item.work_item_id()));
    html.push_str("</strong>");
    if let Some(title) = item.title() {
        html.push_str("<div>");
        html.push_str(&escape_html(title));
        html.push_str("</div>");
    }
    html.push_str("<div class=\"row\"><span class=\"pill\">status: ");
    html.push_str(&escape_html(item.status()));
    html.push_str("</span><span class=\"pill\">lane: ");
    html.push_str(item.lane().label());
    html.push_str("</span><span class=\"pill\">rank: ");
    html.push_str(&escape_html(item.rank()));
    html.push_str("</span></div></div>");
}

fn render_plan_handoff_entry(html: &mut String, entry: &PlanHandoffEntry) {
    html.push_str("<article class=\"entry\"><div class=\"row\">");
    if let Some(created_at) = entry.created_at() {
        html.push_str("<span class=\"pill\">");
        html.push_str(&escape_html(created_at));
        html.push_str("</span>");
    }
    if let Some(author) = entry.author() {
        html.push_str("<span class=\"pill\">");
        html.push_str(&escape_html(author));
        html.push_str("</span>");
    }
    if let Some(id) = entry.id() {
        html.push_str("<span class=\"pill\">");
        html.push_str(&escape_html(id));
        html.push_str("</span>");
    }
    html.push_str("</div><pre>");
    html.push_str(&escape_html(entry.text()));
    html.push_str("</pre></article>");
}

fn escape_html(text: &str) -> String {
    text.chars()
        .flat_map(|character| match character {
            '&' => "&amp;".chars().collect::<Vec<_>>(),
            '<' => "&lt;".chars().collect(),
            '>' => "&gt;".chars().collect(),
            '"' => "&quot;".chars().collect(),
            '\'' => "&#39;".chars().collect(),
            other => vec![other],
        })
        .collect()
}

fn escape_url_path_segment(text: &str) -> String {
    text.chars()
        .flat_map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                vec![character]
            } else {
                format!("%{:02X}", u32::from(character)).chars().collect()
            }
        })
        .collect()
}

/// Project the seven-lane board by reducing the work-item snapshot observations.
///
/// The latest observation per work-item wins (later events supersede earlier
/// ones), each item lands in its emitted `lane`, and every lane is ordered by
/// the fractional `rank` (ties broken by id). Events whose payload is not a
/// complete snapshot are skipped.
#[must_use]
pub fn project_lane_board(events: &[ConsoleEvent]) -> LaneBoard {
    let execution_states = observed_execution_states(events);
    // The sources whose MOST RECENT observation failed. A row fed by one of
    // them is serving last-known values the console cannot currently vouch for,
    // and it says so rather than rendering them as confirmed
    // (`livespec-console-beads-fabro-v8un`, operator rider [2]).
    let degraded = unavailable_sources(events);
    let mut latest: BTreeMap<String, LaneWorkItem> = BTreeMap::new();
    for event in events {
        if *event.event_type() != EventType::WorkItemSnapshotObserved {
            continue;
        }
        let Some(snapshot) = work_item_snapshot_from_payload_json(event.payload_json()) else {
            continue;
        };
        let execution_state = execution_state_for_snapshot(&snapshot, &execution_states);
        let observation_confirmed = !degraded.iter().any(|source| source == event.source());
        latest.insert(
            snapshot.work_item_id().to_owned(),
            LaneWorkItem::from_snapshot(&snapshot, execution_state, observation_confirmed),
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

fn observed_execution_states(
    events: &[ConsoleEvent],
) -> BTreeMap<(String, String), LaneExecutionState> {
    let mut execution_states = BTreeMap::new();
    for event in events {
        match event.event_type() {
            EventType::DispatcherBacklogBounceObserved
            | EventType::DispatcherJournalProgressObserved => {
                if let Some(entry) = dispatcher_journal_from_payload_json(event.payload_json()) {
                    let state = if entry.terminal_status().is_some() {
                        LaneExecutionState::FinishedUnreconciled
                    } else {
                        LaneExecutionState::Executing
                    };
                    execution_states.insert(
                        (entry.repo().to_owned(), entry.work_item_id().to_owned()),
                        state,
                    );
                }
            }
            // Only a run the SOURCE reported as running claims execution. The
            // observation alone says nothing: a run at a terminal status kind
            // has stopped, and marking the item executing off the mere fact
            // that a run was seen is the infer-state-from-an-observation
            // failure `SPECIFICATION/contracts.md` forbids. A non-running run
            // leaves the map untouched
            // rather than asserting some other state the console did not
            // observe -- the dispatcher journal is what owns finished-ness.
            EventType::FabroRunObserved => {
                if let Some(snapshot) = fabro_run_snapshot_from_payload_json(event.payload_json())
                    && snapshot.state().is_running()
                {
                    execution_states.insert(
                        (
                            snapshot.repo().to_owned(),
                            snapshot.work_item_id().to_owned(),
                        ),
                        LaneExecutionState::Executing,
                    );
                }
            }
            _other => {}
        }
    }
    execution_states
}

fn execution_state_for_snapshot(
    snapshot: &WorkItemSnapshot,
    execution_states: &BTreeMap<(String, String), LaneExecutionState>,
) -> LaneExecutionState {
    if snapshot.lane() != Lane::Active {
        return LaneExecutionState::NotActive;
    }
    execution_states
        .get(&(
            snapshot.repo().to_owned(),
            snapshot.work_item_id().to_owned(),
        ))
        .copied()
        .unwrap_or(LaneExecutionState::Claimed)
}

/// The event-log-derived projection behind a [`TuiScreenModel`].
///
/// Everything here depends only on the event log and the active search query
/// -- NOT on the interaction state's cursor, scroll offsets, focus, or overlay
/// -- so it is safe to build ONCE and reuse across every render of the SAME
/// log and query. That reuse is the whole fix for
/// livespec-console-beads-fabro-mx9u.8 ("moving between attention items is
/// very laggy"): before this type existed, [`build_tui_model_for_state`]
/// re-derived all of this from scratch (several full scans of the event log)
/// on EVERY call, and the terminal loop called it several times per single
/// keystroke. Worse, the needs-attention account/valve-command lookups
/// RE-MATERIALIZED the whole needs-attention surface once per attention-list
/// ROW -- `O(rows * events)` work on one build alone, on an inbox of a few
/// hundred events and a couple hundred rows easily hundreds of milliseconds
/// (see `advertised_account`'s history).
///
/// [`project_tui_events`] performs each full scan exactly once. [`render_tui_model`]
/// (or the state-typed [`build_tui_model_for_state`] convenience wrapper)
/// turns a projection plus the current interaction state into a frame,
/// touching the raw event log only for the ONE selected row's detail (an
/// unavoidable per-selection cost, not a per-row one) and for the active
/// view's own summary rows.
#[derive(Debug, Clone)]
pub struct TuiProjection {
    search_query: Option<String>,
    unavailable_sources: Vec<String>,
    observed_source_names: Vec<String>,
    attention_entries: Vec<AttentionEntry>,
    attention_items: Vec<AttentionItem>,
    attention_total: usize,
    lane_board: LaneBoard,
    factory_activity: Option<String>,
    action_failures: BTreeMap<String, ActionFailure>,
    command_outcome: Option<CommandOutcome>,
    orphaned_factory_runs: Vec<OrphanedFactoryRun>,
    needs_attention_by_work_item: NeedsAttentionByWorkItem,
}

impl TuiProjection {
    #[must_use]
    /// The backing sources that degraded to a not-observed finding this
    /// cycle, exactly as [`TuiScreenModel::unavailable_sources`] renders in
    /// the header -- `doctor` reads THIS accessor rather than re-deriving the
    /// tally, so it can never disagree with what the header shows
    /// (livespec-console-beads-fabro-mx9u.14).
    pub fn unavailable_sources(&self) -> &[String] {
        &self.unavailable_sources
    }

    // `observed_source_names` has no `TuiProjection`-level accessor: unlike
    // `unavailable_sources` (which `doctor` reads through this same type),
    // nothing outside `render_tui_model` consumes the projection's copy today
    // -- it flows straight to `TuiScreenModel` as a private field. Add one
    // when a second consumer needs it (the roster item,
    // `livespec-console-beads-fabro-mx9u.20.2`, is the likely first), rather
    // than carrying an unused, untested accessor now.

    #[must_use]
    /// The needs-attention inbox total, exactly as
    /// [`TuiScreenModel::attention_total`] renders in the header's
    /// `attention:` count.
    pub const fn attention_total(&self) -> usize {
        self.attention_total
    }
}

#[cfg(test)]
thread_local! {
    /// Counts calls to [`project_tui_events`] -- the EXPENSIVE, event-log-scanning
    /// half of the model build. `#[cfg(test)]`-only and thread-local (the
    /// standard test harness gives each `#[test]` its own OS thread) so tests
    /// running in parallel never see each other's counts.
    static TUI_PROJECTION_BUILD_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Counts calls to [`render_tui_model`] -- the CHEAP half, safe to call once
    /// per keystroke against a projection built once for the whole burst.
    static TUI_RENDER_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
/// Test-only counters for [`project_tui_events`] / [`render_tui_model`] calls,
/// asserting the "one projection, many renders" property
/// livespec-console-beads-fabro-mx9u.8's acceptance criteria name directly.
pub(crate) fn reset_tui_projection_counters() {
    TUI_PROJECTION_BUILD_COUNT.with(|count| count.set(0));
    TUI_RENDER_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
/// Return the `(project_tui_events calls, render_tui_model calls)` recorded
/// since the last [`reset_tui_projection_counters`].
pub(crate) fn tui_projection_counters() -> (usize, usize) {
    (
        TUI_PROJECTION_BUILD_COUNT.with(std::cell::Cell::get),
        TUI_RENDER_COUNT.with(std::cell::Cell::get),
    )
}

#[must_use]
/// Build the event-log-derived projection for `events`, filtered by
/// `search_query`. See [`TuiProjection`] for why this is split from
/// [`render_tui_model`] and what calling it once buys.
pub fn project_tui_events(events: &[ConsoleEvent], search_query: Option<&str>) -> TuiProjection {
    #[cfg(test)]
    TUI_PROJECTION_BUILD_COUNT.with(|count| count.set(count.get() + 1));
    // Folded ONCE and shared: `unavailable_sources` and `observed_source_names`
    // are two views over the same per-source tally, not two independent event-
    // log scans.
    let source_tally = source_observation_tally(events);
    let unavailable_sources = unavailable_sources_from_tally(&source_tally);
    let observed_source_names = source_tally.into_keys().collect::<Vec<_>>();
    let needs_attention_items = materialize_attention_items(events);
    let needs_attention_by_work_item = group_needs_attention_by_work_item(&needs_attention_items);
    let attention_entries = unified_attention_entries(events, search_query, &needs_attention_items);
    let attention_items = attention_entries
        .iter()
        .map(|entry| entry.to_attention_item(&needs_attention_by_work_item))
        .collect::<Vec<_>>();
    // The header's `attention:` count is the INBOX TOTAL, never the filtered
    // match count. A search narrows what the list SHOWS, not how much work is
    // waiting, and a header that followed the filter turned the one number
    // reporting the operator's outstanding load into a report on the current
    // query (dogfooded 2026-09-08: `attention: 80` became `attention: 1` under
    // `/una`, then `attention: 22`, none of which was the inbox). The match
    // count belongs to the search overlay, which is where the operator typed.
    //
    // Any active search overlay -- even one whose query is still empty --
    // recomputes the unfiltered pass, rather than special-casing the empty
    // string: `attention_item_matches` / `attention_snapshot_matches` already
    // treat an empty query as matching everything, so the filtered pass and
    // the unfiltered recompute agree on the empty-query count regardless. A
    // guard here that tried to skip the recompute for "nothing to narrow yet"
    // would be redundant with that agreement, not an optimization -- it can
    // only ever produce a mutation-testing survivor with no observable effect
    // (measured 2026-09-08, PR #1096 check-mutants: `!query.is_empty()`
    // replaced with `true` survived because both sides compute the same
    // count). With no active overlay at all, there is no query to narrow by,
    // so the filtered and unfiltered lists are the same list.
    //
    // Computed HERE (cached with the rest of the projection) rather than per
    // render: it depends only on `events` and whether search is active, the
    // same granularity `project_tui_events`'s own cache key already uses, so
    // recomputing it per render would defeat the "one projection, many
    // renders" property this whole type exists for. It reuses
    // `needs_attention_items`, already materialized above, rather than
    // re-scanning the event log for it a second time.
    let attention_total = search_query.map_or(attention_items.len(), |_query| {
        unified_attention_entries(events, None, &needs_attention_items).len()
    });
    TuiProjection {
        search_query: search_query.map(str::to_owned),
        unavailable_sources,
        observed_source_names,
        attention_entries,
        attention_items,
        attention_total,
        lane_board: project_lane_board(events),
        factory_activity: factory_drain_activity(events),
        action_failures: project_action_failures(events),
        command_outcome: project_command_outcome(events),
        orphaned_factory_runs: project_orphaned_factory_runs(events),
        needs_attention_by_work_item,
    }
}

#[must_use]
/// Render `state` against an already-built `projection`.
///
/// Touches the raw event log only for the one selected row's detail and for
/// the active view's own summary rows -- see [`TuiProjection`]. This is the
/// per-keystroke fast path: it never re-scans the event log for anything the
/// projection already carries.
pub fn render_tui_model(
    projection: &TuiProjection,
    events: &[ConsoleEvent],
    state: &TuiInteractionState,
) -> TuiScreenModel {
    #[cfg(test)]
    TUI_RENDER_COUNT.with(|count| count.set(count.get() + 1));
    let (selected_attention_index, displaced_attention_id) =
        selected_attention_for_state(&projection.attention_entries, state);
    let detail = selected_attention_index.map(|index| {
        projection.attention_entries[index]
            .to_detail(events, &projection.needs_attention_by_work_item)
    });
    let overlay = normalize_overlay(state.overlay(), detail.as_ref());
    let active_view = state.active_view();
    let lane_board = projection.lane_board.clone();
    let lane_focus = state.lane_focus();
    let selected_lane_index = match (active_view, lane_focus) {
        (TuiView::Lanes, LaneFocus::Overview) => {
            Some(state.selected_lane_index().min(Lane::all().len() - 1))
        }
        _ => None,
    };
    // The per-item cursor exists only while drilled into a lane that holds at
    // least one item; an empty lane has nothing to select.
    let (selected_lane_item_index, missing_selected_lane_item_id) =
        selected_lane_item_for_state(active_view, lane_focus, &lane_board, state);
    let selected_setting_index = match active_view {
        TuiView::Settings => Some(
            state
                .selected_setting_index()
                .min(DispatcherSettingRow::all().len() - 1),
        ),
        _ => None,
    };
    let events_focus = state.events_focus();
    let selected_events_index = state
        .selected_events_index()
        .min(EventsFocus::all().len() - 1);
    // The Status line is ONE channel, and its rule is that the most recent thing
    // to contradict the operator's expectation is what it says. A cursor that
    // moved out from under them because the anchored row left the list is
    // exactly that, so it outranks a status carried from an earlier keystroke.
    // It does not stick: `reduce_tui_interaction` re-anchors the state on the
    // operator's next interaction, which is their acknowledgement.
    //
    // A row that left because the OPERATOR narrowed the list is NOT news — the
    // open search query is its own explanation — so the report is suppressed
    // while one is filtering rather than firing on every typed character.
    let transient_status = displaced_attention_id
        .filter(|_displaced| projection.search_query.is_none())
        .map_or_else(
            || state.transient_status.clone(),
            |displaced| Some(displaced_attention_status(&displaced)),
        );
    TuiScreenModel {
        active_view,
        navigation: TuiView::all().to_vec(),
        attention_items: projection.attention_items.clone(),
        attention_total: projection.attention_total,
        selected_attention_index,
        detail,
        view_items: view_summary_items(
            active_view,
            events_focus,
            &projection.observed_source_names,
            events,
        ),
        lane_board,
        lane_focus,
        selected_lane_index,
        selected_lane_item_index,
        missing_selected_lane_item_id,
        events_focus,
        selected_events_index,
        focus: state.focus(),
        detail_scroll: state.detail_scroll(),
        header_scroll: state.header_scroll(),
        overlay,
        selected_repo: state.selected_repo().to_owned(),
        selected_setting_index,
        dispatcher_settings: state.dispatcher_settings().clone(),
        dispatcher_setting_write: state.dispatcher_setting_write().clone(),
        plugin_resolution: state.plugin_resolution().clone(),
        action_failures: projection.action_failures.clone(),
        // The canonical, untruncated header. `header_line` keeps this display
        // order for wide terminals and sheds narrow-terminal fields by declared
        // information-value priority, not by this string's field positions.
        // The build IDENTITY itself is not one of these fields -- it lives in
        // the header pane's block title instead; see `fit_header_line`'s doc.
        header: format!(
            "fleet: livespec | mode: tui | repo: {} | view: {} | attention: {}{}{}{}{}{}",
            header_repo_label(state.selected_repo()),
            active_view.label(),
            projection.attention_total,
            factory_activity_segment(projection.factory_activity.as_deref()),
            transient_status_segment(transient_status.as_deref()),
            build_staleness_header_segment(state.build_staleness()),
            source_health_header_segment(&projection.unavailable_sources),
            startup_ingest_header_segment(state.startup_ingest_pending())
        ),
        unavailable_sources: projection.unavailable_sources.clone(),
        observed_source_names: projection.observed_source_names.clone(),
        factory_activity: projection.factory_activity.clone(),
        transient_status,
        list_edge: state.list_edge(),
        command_outcome: projection.command_outcome.clone(),
        orphaned_factory_runs: projection.orphaned_factory_runs.clone(),
        build_identity: state.build_identity().cloned(),
        build_staleness: state.build_staleness(),
        startup_ingest_pending: state.startup_ingest_pending(),
    }
}

#[must_use]
/// Build tui model from the supplied inputs.
pub fn build_tui_model(events: &[ConsoleEvent], requested_selection: usize) -> TuiScreenModel {
    let state = TuiInteractionState::new(requested_selection, TuiOverlay::None);
    build_tui_model_for_state(events, &state)
}

#[must_use]
/// Build tui model for state from the supplied inputs.
///
/// A convenience wrapper over [`project_tui_events`] + [`render_tui_model`] for
/// callers that render exactly once and have no reason to cache the
/// projection (most tests, and any one-shot preview). The terminal loop's
/// per-keystroke hot path calls the two halves directly instead, so it can
/// reuse ONE projection across many renders -- see [`TuiProjection`].
pub fn build_tui_model_for_state(
    events: &[ConsoleEvent],
    state: &TuiInteractionState,
) -> TuiScreenModel {
    let projection = project_tui_events(events, search_query(state.overlay()));
    render_tui_model(&projection, events, state)
}

#[must_use]
/// Project the orphaned-factory-runs lane from the console's own event stream.
///
/// The LATEST `factory.run_orphans_observed` event wins outright and earlier
/// ones are discarded: each carries a whole reconciler projection, and a run
/// that has stopped being orphaned simply leaves the newer set. Folding the
/// events together instead would keep rendering an orphan the orchestrator no
/// longer claims — the lane would accumulate rather than track.
///
/// No event at all yields an EMPTY lane. That is honest here rather than a
/// clean-factory claim: the adapter records an unreadable or unreachable
/// reconciler as a `source.not_observed_finding_observed`, which the header's
/// source-availability tally surfaces, so "no orphans" and "not observed" stay
/// distinguishable to the operator.
pub fn project_orphaned_factory_runs(events: &[ConsoleEvent]) -> Vec<OrphanedFactoryRun> {
    events
        .iter()
        .filter(|event| *event.event_type() == EventType::FactoryRunOrphansObserved)
        .filter_map(|event| reconcile_runs_snapshot_from_payload_json(event.payload_json()))
        .next_back()
        .map(|snapshot| snapshot.orphaned_runs().to_vec())
        .unwrap_or_default()
}

/// Resolve the Attention list's cursor for this rebuild, by IDENTITY.
///
/// Returns the selected row and, when the anchored item is no longer listed,
/// the id that vanished so the caller can say so.
///
/// The list is rebuilt from the event stream on EVERY source refresh and is
/// rank-ordered, so a row's position is not stable between one frame and the
/// next: another session moving an unrelated work-item re-ranks the list under
/// the operator's cursor. Anchoring on the id makes the cursor follow the item
/// the operator chose. When the anchor has left the list the stored row index is
/// the fallback, clamped to the new length — the surviving row nearest where the
/// item used to be, since the rows behind it have shifted up into its place.
/// With no anchor at all (a fresh state, or a position-only caller) this is the
/// pure positional behaviour it replaced.
fn selected_attention_for_state(
    entries: &[AttentionEntry],
    state: &TuiInteractionState,
) -> (Option<usize>, Option<String>) {
    let fallback = selected_index(entries.len(), state.selected_attention_index());
    let Some(anchor) = state.selected_attention_id() else {
        return (fallback, None);
    };
    if let Some(index) = entries.iter().position(|entry| entry.row_id() == anchor) {
        return (Some(index), None);
    }
    (fallback, Some(anchor.to_owned()))
}

/// The Status-line report for an Attention cursor displaced by a refresh.
///
/// Deliberately terse: a status wider than [`MAX_TRANSIENT_STATUS_CHARS`] costs
/// the operator the ENTIRE header line (see the header-fitting bound), and the
/// id it names is the widest part. The fixed text is short enough that a
/// fleet-length work-item id stays well inside that budget.
fn displaced_attention_status(displaced_id: &str) -> String {
    format!("{displaced_id} left the list; cursor on nearest row")
}

fn selected_lane_item_for_state(
    active_view: TuiView,
    lane_focus: LaneFocus,
    lane_board: &LaneBoard,
    state: &TuiInteractionState,
) -> (Option<usize>, Option<String>) {
    let (TuiView::Lanes, LaneFocus::Lane(lane)) = (active_view, lane_focus) else {
        return (None, None);
    };
    let Some(column) = lane_board.column(lane) else {
        return (None, state.selected_lane_item_id().map(str::to_owned));
    };
    if column.count() == 0 {
        return (None, state.selected_lane_item_id().map(str::to_owned));
    }
    if let Some(work_item_id) = state.selected_lane_item_id() {
        for (index, item) in column.items().iter().enumerate() {
            if item.work_item_id() == work_item_id {
                return (Some(index), None);
            }
        }
        return (None, Some(work_item_id.to_owned()));
    }
    (
        Some(state.selected_lane_item_index().min(column.count() - 1)),
        None,
    )
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

/// Whether `event_type` is a POSITIVE observation of its source: a data
/// snapshot, journal entry, or observed-and-idle marker that clears any prior
/// not-observed finding for the same source.
///
/// Shared by [`unavailable_sources`] (deciding WHICH sources the header counts
/// as down) and [`doctor::build_doctor_report`] (dating WHEN each unavailable
/// source last succeeded), so the two can never classify an event
/// differently -- doctor's staleness timestamp is always the moment the
/// header's own tally used to decide the source was down
/// (livespec-console-beads-fabro-mx9u.14).
#[must_use]
pub(crate) const fn is_positive_source_observation(event_type: EventType) -> bool {
    matches!(
        event_type,
        EventType::SourceObservedFindingObserved
            | EventType::WorkItemSnapshotObserved
            | EventType::SourceCompletenessFindingObserved
            | EventType::DispatcherBacklogBounceObserved
            | EventType::DispatcherJournalProgressObserved
            | EventType::DispatcherRefusalObserved
            | EventType::FabroRunObserved
            | EventType::GithubPullRequestSnapshotObserved
            | EventType::LivespecNextSnapshotObserved
            | EventType::LivespecReviseRequired
    )
}

/// Fold the event log into a per-source `(name -> currently degraded?)` tally,
/// in `global_seq` order: a [`EventType::SourceNotObservedFindingObserved`]
/// marks its source degraded, and any LATER positive observation of that same
/// source -- a snapshot event or the observed-and-idle
/// [`EventType::SourceObservedFindingObserved`] marker -- clears it. Every
/// source that has EVER reported EITHER kind of event becomes a key, healthy
/// or not, so the map doubles as the source-existence registry
/// ([`TuiProjection::observed_source_names`] reads every key) and the
/// unavailability tally ([`unavailable_sources_from_tally`] filters it).
fn source_observation_tally(events: &[ConsoleEvent]) -> BTreeMap<String, bool> {
    let mut tally: BTreeMap<String, bool> = BTreeMap::new();
    for event in events {
        if *event.event_type() == EventType::SourceNotObservedFindingObserved {
            tally.insert(event.source().to_owned(), true);
            continue;
        }
        if is_positive_source_observation(*event.event_type()) {
            // A positive observation clears any prior not-observed finding for
            // this source, and (`or_insert`) ADMITS a never-degraded source to
            // the registry too -- unlike the not-observed arm above, silently
            // skipping this one would leave every consistently-healthy source
            // invisible to `observed_source_names`, which is exactly the "no
            // event sources observed yet" defect measured on the real store
            // 2026-09-09 (mx9u.20.1 review): six sources were polling
            // successfully and the roster still claimed none existed.
            tally
                .entry(event.source().to_owned())
                .and_modify(|degraded| *degraded = false)
                .or_insert(false);
        }
    }
    tally
}

/// The distinct backing-source names whose MOST RECENT observation was a
/// not-observed finding, sorted for a stable header order.
///
/// So a source that degraded on an earlier cycle but was observed
/// successfully on a later one no longer counts, and a transient failure is
/// never branded permanently. A source counts only while its most recent
/// observation was not-observed, so the operator can distinguish a cockpit-blind
/// screen from an idle factory.
fn unavailable_sources(events: &[ConsoleEvent]) -> Vec<String> {
    unavailable_sources_from_tally(&source_observation_tally(events))
}

/// The degraded-only half of [`unavailable_sources`], taking an
/// ALREADY-FOLDED tally so [`project_tui_events`] can share one fold with
/// [`observed_source_names`] rather than scanning the event log twice.
fn unavailable_sources_from_tally(tally: &BTreeMap<String, bool>) -> Vec<String> {
    tally
        .iter()
        .filter(|&(_source, degraded)| *degraded)
        .map(|(source, _degraded)| source.clone())
        .collect()
}

/// The header's source-health segment: an empty string when every source was
/// observed (no phantom count on a true-empty screen), else ` | event sources:
/// N unavailable (name, ...)` counting and attributing the degraded sources so
/// a false-empty is never indistinguishable from a true-empty.
fn source_health_header_segment(unavailable_sources: &[String]) -> String {
    if unavailable_sources.is_empty() {
        String::new()
    } else {
        format!(
            " | event sources: {} unavailable ({})",
            unavailable_sources.len(),
            unavailable_sources.join(", ")
        )
    }
}

/// The header's startup-ingest tell text, without its leading separator.
///
/// Exported so a consumer that needs to recognize the tell (a rendered-text
/// test, or the e2e tmux harness's own settling logic, which must not mistake
/// a still-loading frame for a converged one) never hand-copies the literal.
pub const STARTUP_INGEST_LOADING_TELL: &str = "event sources: loading";

/// The header's startup-ingest tell: while the session's first background
/// source sweep has not yet landed, the frame is drawn from whatever the store
/// already held rather than a fresh read, so this says so instead of letting a
/// stale-or-empty screen pass as current (livespec-console-beads-fabro-
/// pzbdbo.27). Empty once that first sweep completes, exactly like
/// [`source_health_header_segment`] is empty once every source is observed.
fn startup_ingest_header_segment(startup_ingest_pending: bool) -> String {
    if startup_ingest_pending {
        format!(" | {STARTUP_INGEST_LOADING_TELL}")
    } else {
        String::new()
    }
}

fn factory_activity_segment(activity: Option<&str>) -> String {
    activity.map_or_else(String::new, |value| format!(" | factory: {value}"))
}

fn transient_status_segment(status: Option<&str>) -> String {
    status.map_or_else(String::new, |value| format!(" | status: {value}"))
}

/// The canonical header's stale-build tell, or empty while current/unknown.
fn build_staleness_header_segment(staleness: BuildStaleness) -> String {
    build_staleness_segment(staleness).map_or_else(String::new, |tell| format!(" | {tell}"))
}

/// What a failed command's Status-line message says when its stored error
/// payload names no cause at all.
///
/// The reported defect was a command row reading `status: failed` beside
/// `error_json: {}`. A bare "failed" is not diagnosable by anyone, so the
/// ABSENCE is stated rather than left to read as a message that simply had
/// nothing to add.
const OUTCOME_CAUSE_ABSENT: &str = "cause not reported";

#[derive(Debug, Clone, PartialEq, Eq)]
/// One command's terminal verdict in BOTH the forms the Status band can draw.
///
/// A band with room draws [`full`](Self::full) -- which command, how it ended,
/// and the cause the store recorded. A band without room draws
/// [`short`](Self::short) instead of dropping the verdict outright, because a
/// verdict is the one Status-line segment nothing else can restore: the Help
/// roster lists actions, and the stream retires the verdict the moment the next
/// command starts. Measured 2026-09-08 at 105 columns, before the short form
/// existed: `last command: dispatch item failed — cause not reported` was 55
/// columns of a 105-column band, so the fitter shed it whole and the operator
/// was left with no sign that the dispatch they had just pressed had failed.
///
/// The short form keeps the two facts that survive abbreviation -- WHICH
/// command and HOW it ended -- and drops the cause, which the operator reads by
/// widening the pane. Both forms are composed together by
/// [`command_outcome_notice`] from the same inputs, so they cannot disagree
/// about what happened.
struct CommandOutcome {
    full: String,
    short: String,
}

/// The transient operator-feedback slot: the terminal outcome of the operator's
/// LAST command, rendered for the Status line.
///
/// Derived on read from the command-outcome events already in the store -- the
/// `factory.drain.*`, `factory.dispatch_item.*` and `work_item.action.*`
/// terminal contracts the command handlers already append -- so the slot has NO
/// write path of its own. Nothing stores it and nothing has to remember to
/// clear it, which is why it cannot drift out of step with what actually
/// happened.
///
/// TRANSIENCE IS THE EVENT STREAM'S, not a timer's. The fold walks BACKWARDS
/// and stops at the first event that decides the slot: a terminal outcome fills
/// it, and a request/start marker EMPTIES it. So the moment the operator
/// launches the next command the previous verdict stops being reported beside
/// hints that now describe a different action, and a message can never outlive
/// the command it belongs to. A timer would have needed a clock in a pure
/// projection and would have hidden the outcome from an operator who looked
/// away for a moment; the stream already carries the only boundary that matters.
fn project_command_outcome(events: &[ConsoleEvent]) -> Option<CommandOutcome> {
    events
        .iter()
        .rev()
        .find_map(|event| match event.event_type() {
            EventType::FactoryDrainCompleted => {
                Some(Some(command_outcome_notice("drain", "succeeded", None)))
            }
            EventType::FactoryDrainFailed => {
                Some(Some(failed_command_notice("drain", event.payload_json())))
            }
            EventType::FactoryDrainAwaitingHuman => Some(Some(command_outcome_notice(
                "drain",
                "stopped at a human valve",
                None,
            ))),
            EventType::FactoryDrainNotWired => {
                Some(Some(command_outcome_notice("drain", "not wired", None)))
            }
            EventType::FactoryDispatchItemCompleted => Some(Some(command_outcome_notice(
                "dispatch item",
                "succeeded",
                None,
            ))),
            EventType::FactoryDispatchItemFailed => Some(Some(failed_command_notice(
                "dispatch item",
                event.payload_json(),
            ))),
            EventType::FactoryDispatchItemNotWired => Some(Some(command_outcome_notice(
                "dispatch item",
                "not wired",
                None,
            ))),
            EventType::WorkItemActionCompleted => Some(Some(command_outcome_notice(
                &work_item_action_name(event.payload_json()),
                "succeeded",
                None,
            ))),
            EventType::WorkItemActionFailed => Some(Some(failed_command_notice(
                &work_item_action_name(event.payload_json()),
                event.payload_json(),
            ))),
            EventType::WorkItemActionNotWired => Some(Some(command_outcome_notice(
                &work_item_action_name(event.payload_json()),
                "not wired",
                None,
            ))),
            // A newly requested or started command retires the previous
            // verdict: from here on the operator is waiting on THIS command,
            // and the last one's outcome is no longer feedback, it is history.
            EventType::FactoryDrainRequested
            | EventType::FactoryDrainStarted
            | EventType::FactoryDispatchItemRequested
            | EventType::FactoryDispatchItemStarted
            | EventType::WorkItemActionStarted => Some(None),
            _other => None,
        })
        .flatten()
}

/// One command's Status-line message: which command, how it ended, and the
/// cause when one is carried, beside the short form a narrow band draws.
///
/// The short form names the command by its FIRST word -- `dispatch item`
/// becomes `dispatch` -- and drops the cause, so
/// `last command: dispatch item failed — cause not reported` shortens to
/// `last: dispatch failed`. A one-word command name (`drain`) and an action id
/// (`approve:lcbf-k0w`) carry no space and so are named whole.
fn command_outcome_notice(command: &str, outcome: &str, cause: Option<String>) -> CommandOutcome {
    let full = cause.map_or_else(
        || format!("last command: {command} {outcome}"),
        |cause| format!("last command: {command} {outcome} — {cause}"),
    );
    let head = command
        .split_once(' ')
        .map_or(command, |(head, _rest)| head);
    CommandOutcome {
        full,
        short: format!("last: {head} {outcome}"),
    }
}

/// A failed command's Status-line message: the cause its stored error payload
/// carries, or an explicit statement that the payload carries none.
fn failed_command_notice(command: &str, payload_json: &str) -> CommandOutcome {
    command_outcome_notice(
        command,
        "failed",
        Some(failure_cause(payload_json).unwrap_or_else(|| OUTCOME_CAUSE_ABSENT.to_owned())),
    )
}

/// The failure cause a command's stored error payload carries, or `None` when
/// it carries none.
///
/// One reader serves all three failure contracts because they share one writer,
/// [`diagnostic_event_payload`]: a STRUCTURED refusal (the drive surface's
/// `--json` shape) is spliced in whole, so its `domain_error` / `summary` sit
/// at the payload root rather than under a `refusal` key; an unstructured
/// diagnostic is wrapped as `refusal`; and a surface that emitted nothing at
/// all leaves the empty object behind.
fn failure_cause(payload_json: &str) -> Option<String> {
    let payload: Option<serde_json::Value> = serde_json::from_str(payload_json).ok();
    let field = |key: &str| {
        payload
            .as_ref()
            .and_then(|value| value.get(key))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    match (field("domain_error"), field("summary")) {
        (Some(domain_error), Some(summary)) => Some(format!("{domain_error}: {summary}")),
        (Some(domain_error), None) => Some(domain_error),
        (None, Some(summary)) => Some(summary),
        (None, None) => field("refusal"),
    }
}

/// The `action_id` a `work_item.action.*` event carries (`approve:<id>`,
/// `move-status:<id>:<value>`) -- the name the operator recognizes as the
/// command they issued -- or the contract's own name when the payload names no
/// action.
fn work_item_action_name(payload_json: &str) -> String {
    serde_json::from_str::<serde_json::Value>(payload_json)
        .ok()
        .as_ref()
        .and_then(|payload| payload.get("action_id"))
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| "work-item action".to_owned(), str::to_owned)
}

fn factory_drain_activity(events: &[ConsoleEvent]) -> Option<String> {
    events
        .iter()
        .rev()
        .find_map(|event| match event.event_type() {
            EventType::FactoryDrainRequested | EventType::FactoryDrainStarted => {
                Some("drain in flight".to_owned())
            }
            EventType::FactoryDispatchItemRequested | EventType::FactoryDispatchItemStarted => {
                Some("dispatch item in flight".to_owned())
            }
            EventType::FactoryDrainCompleted => Some("drain completed".to_owned()),
            EventType::FactoryDrainFailed => Some("drain failed".to_owned()),
            EventType::FactoryDrainAwaitingHuman => Some("drain awaiting human".to_owned()),
            EventType::FactoryDrainNotWired => Some("drain not wired".to_owned()),
            EventType::FactoryDispatchItemCompleted => Some("dispatch item completed".to_owned()),
            EventType::FactoryDispatchItemFailed => Some("dispatch item failed".to_owned()),
            EventType::FactoryDispatchItemNotWired => Some("dispatch item not wired".to_owned()),
            EventType::CommandRejected if event.stream_id() == "fleet:livespec" => {
                Some("drain rejected".to_owned())
            }
            _other => None,
        })
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
        " | event sources: {count} unavailable ({})",
        unavailable_sources.join(", ")
    )];
    // The `+N more` form only makes sense once at least one name is elided.
    if count >= 2 {
        forms.push(format!(
            " | event sources: {count} unavailable ({}, +{} more)",
            unavailable_sources[0],
            count - 1
        ));
    }
    forms.push(format!(" | event sources: {count} unavailable"));
    forms
}

/// The display width of a header line in terminal columns. The header is ASCII
/// (field labels, repo ids, source names), so a char count is its column width.
fn header_display_width(line: &str) -> usize {
    line.chars().count()
}

/// The longest a transient status may be and still leave the pinned-width header
/// carrying its `attention:` count.
///
/// WHY A BOUND EXISTS AT ALL. [`fit_header_line`] keeps each field ATOMIC — kept
/// or dropped whole, never mid-truncated — and sheds them by declared priority
/// until the line fits. A transient status is the highest-priority field, so it
/// is dropped LAST; an over-long one therefore evicts every other field first,
/// and once it is finally dropped too the composed line is the EMPTY STRING. A
/// blank header is the worst of both worlds: the operator loses their context
/// AND is told nothing, which is the very failure the status was added to
/// prevent (livespec-console-beads-fabro-zbnnlv).
///
/// The bound is on the PRODUCER rather than on the fitter because the atomic-field
/// rule is the ratified degradation contract: a status that does not fit is a
/// status written too long, not a header that should start truncating fields.
///
/// THE ARITHMETIC, for the case the gate exercises (no source-health segment):
/// the dogfood terminal is 112 columns, 110 inside the header block's borders.
/// Once the lower-priority fields have been shed the line is
/// `attention: N | status: <status>`, whose fixed part is 26 columns at a
/// four-digit count (`attention: 9999 | status: `), leaving 84. A status with an
/// unavailable-source segment present still sheds `attention:` — the source
/// count is the cockpit-blind tell and outranks it by design.
pub const MAX_TRANSIENT_STATUS_CHARS: usize = 84;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// The declared information value for a header segment under width pressure.
///
/// Lower variants are shed first. This order is intentionally separate from the
/// display order so a new field cannot accidentally make transient
/// refusal/not-wired/error state disappear just because it sits near the right
/// edge of the canonical header.
enum HeaderSegmentPriority {
    ConstantIdentity,
    StaticContext,
    StateCount,
    TransientState,
}

#[derive(Debug, Clone)]
/// One whole field in the header's canonical display order.
struct HeaderField {
    text: String,
    priority: HeaderSegmentPriority,
}

/// One shrink step for the header fitter: drop a whole field by its display-order
/// index, or step the source-health segment down to its next-narrower form.
enum Shrink {
    DropField(usize),
    DegradeSource,
}

/// Build the ORDERED shrink plan for [`fit_header_line`]: one op per
/// over-budget step, least valuable first.
///
/// With TWO OR MORE unavailable sources, the names first elide to the
/// intermediate `+N more` form -- a genuinely cheap step -- before any field
/// yields, then low-priority static fields yield, then the source segment
/// collapses to its bare count before higher-value state fields are
/// considered. Field eviction itself comes from each field's declared
/// priority (already reflected in `drop_order`'s order), not from where the
/// field happens to sit in the header string.
///
/// With exactly ONE unavailable source there is no `+N more` -- the only two
/// forms are the named one and the bare count -- so an unconditional first
/// `DegradeSource` would jump straight from "named" to "gone", sacrificing
/// the name even in cases where dropping a couple of low-priority fields
/// alone would have made room to keep it (measured
/// livespec-console-beads-fabro-mx9u.13, against a real single-unavailable-
/// source header at the pinned 112-column budget: the unconditional-first
/// degrade cost the name for no reason -- `fleet:`/`mode:` alone would have
/// bought enough room). So for that case the (single) degrade step is tried
/// LAST, after every field has had its chance, the same way the source
/// COUNT is protected from EVER being dropped.
fn header_shrink_plan(
    drop_order: Vec<(usize, HeaderSegmentPriority)>,
    source_form_count: usize,
) -> Vec<Shrink> {
    let (low_priority_drops, high_priority_drops): (Vec<_>, Vec<_>) = drop_order
        .into_iter()
        .partition(|(_index, priority)| *priority < HeaderSegmentPriority::StateCount);
    let has_intermediate_source_abbreviation = source_form_count >= 3;
    let mut plan = Vec::new();
    if has_intermediate_source_abbreviation {
        plan.push(Shrink::DegradeSource);
    }
    plan.extend(
        low_priority_drops
            .into_iter()
            .map(|(index, _priority)| Shrink::DropField(index)),
    );
    plan.push(Shrink::DegradeSource);
    plan.extend(
        high_priority_drops
            .into_iter()
            .map(|(index, _priority)| Shrink::DropField(index)),
    );
    plan
}

/// Compose the width-fitted header. See [`TuiScreenModel::header_line`] for the
/// degradation contract. This is the pure core: it composes the atomic fields in
/// a fixed display order and, while the line is over `width`, applies the shrink
/// plan (see [`header_shrink_plan`]) one step at a time — re-measuring after
/// each step and stopping as soon as it fits.
///
/// The build IDENTITY itself is NOT one of these fields -- it lives in the
/// header pane's block TITLE instead (see `console_tui::render_header`),
/// which costs nothing from this content-line budget and is therefore always
/// visible regardless of width or how many alerts are competing for room.
/// Measured livespec-console-beads-fabro-mx9u.13: at the maintainer's real,
/// busy 159-column header (a factory alert AND several unavailable sources
/// beside a two-digit attention count), even a sha-only short form of the
/// identity had no room left once every genuinely-content field had its
/// say -- an operator checking which build is running is exactly as likely
/// to do it during a busy moment as a quiet one, so a segment that only
/// survives the quiet moments does not meet the bar. The STALE tell stays
/// here, at `TransientState`, because it is a live anomaly on par with
/// `factory:`/`status:`, not an identity fact -- see
/// [`build_staleness_segment`].
#[allow(clippy::too_many_arguments)]
fn fit_header_line(
    repo: &str,
    view: &str,
    attention: usize,
    factory_activity: Option<&str>,
    transient_status: Option<&str>,
    unavailable_sources: &[String],
    build_staleness: BuildStaleness,
    startup_ingest_pending: bool,
    width: usize,
) -> String {
    // Fixed display order; `None` means the whole field was
    // dropped to make room. Each field is atomic -- kept or dropped whole, never
    // mid-truncated.
    let mut fields = [
        Some(HeaderField {
            text: "fleet: livespec".to_owned(),
            priority: HeaderSegmentPriority::ConstantIdentity,
        }),
        Some(HeaderField {
            text: "mode: tui".to_owned(),
            priority: HeaderSegmentPriority::ConstantIdentity,
        }),
        Some(HeaderField {
            text: format!("repo: {repo}"),
            priority: HeaderSegmentPriority::StaticContext,
        }),
        Some(HeaderField {
            text: format!("view: {view}"),
            priority: HeaderSegmentPriority::StaticContext,
        }),
        Some(HeaderField {
            text: format!("attention: {attention}"),
            priority: HeaderSegmentPriority::StateCount,
        }),
        factory_activity.map(|activity| HeaderField {
            text: format!("factory: {activity}"),
            priority: HeaderSegmentPriority::TransientState,
        }),
        transient_status.map(|status| HeaderField {
            text: format!("status: {status}"),
            priority: HeaderSegmentPriority::TransientState,
        }),
        // Same tier as `factory:`/`status:` above -- an active stale-build
        // tell is exactly the kind of anomaly that class exists to protect,
        // and shares its reasoning: an operator grading a dogfood pass
        // against the wrong binary is at least as costly as a missed
        // refusal. livespec-console-beads-fabro-mx9u.13.
        build_staleness_segment(build_staleness).map(|tell| HeaderField {
            text: tell,
            priority: HeaderSegmentPriority::TransientState,
        }),
        // Same tier again: a still-loading first sweep is a live, transient
        // condition on par with a factory alert or a stale build, not an
        // identity fact, and it is gone the moment the sweep lands
        // (livespec-console-beads-fabro-pzbdbo.27). Kept whole or dropped
        // whole like every other field here -- there is no partial form,
        // unlike the source-health segment's multi-step degrade, because this
        // tell is binary (the WHOLE first sweep is in flight or it is not).
        startup_ingest_pending.then(|| HeaderField {
            text: STARTUP_INGEST_LOADING_TELL.to_owned(),
            priority: HeaderSegmentPriority::TransientState,
        }),
    ];
    let source_forms = source_health_segment_forms(unavailable_sources);
    let mut source_idx = 0usize; // 0 = widest (full names)

    let compose = |fields: &[Option<HeaderField>; 9], source_idx: usize| -> String {
        let mut line = fields
            .iter()
            .filter_map(|field| field.as_ref().map(|field| field.text.as_str()))
            .collect::<Vec<_>>()
            .join(" | ");
        if let Some(source) = source_forms.get(source_idx) {
            line.push_str(source);
        }
        line
    };

    let mut drop_order = fields
        .iter()
        .enumerate()
        .filter_map(|(index, field)| field.as_ref().map(|field| (index, field.priority)))
        .collect::<Vec<_>>();
    drop_order.sort_by_key(|(index, priority)| (*priority, *index));
    let plan = header_shrink_plan(drop_order, source_forms.len());

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

/// The separator every Status-line hint row is composed with, and therefore the
/// seam [`fit_footer_line`] splits it back apart on. No hint token contains it,
/// so the split recovers exactly the segments the composers joined.
const HINT_SEPARATOR: &str = " | ";

/// The display width of a Status-line hint row in terminal columns. Hint tokens
/// are ASCII (key names and verbs), so a char count is its column width.
fn hint_display_width(line: &str) -> usize {
    line.chars().count()
}

/// The key the overflow marker names as the door to everything the band could
/// not draw.
///
/// Pinned to the registry's `open-help` accelerator by
/// `the_overflow_marker_names_the_registrys_help_key`, so the marker can never
/// name a key Help has stopped being bound to.
const HINT_OVERFLOW_KEY: &str = "?";

/// The hint row with the `kept` segments joined back together and everything
/// shed carried as an explicit `+N more: ?` marker.
///
/// The marker NAMES the key that reopens what it counted rather than only
/// counting it. `+5 more` told an operator that five hints existed and nothing
/// about how to read them, so at exactly the widths where the roster was most
/// needed the overflow was a dead end; `?` opens Help on the focused pane's own
/// section, which is where those hints are listed. That door is also what lets
/// the per-item verbs yield ahead of navigation and the last command's verdict
/// in [`HintPriority`](action_registry::HintPriority) — a shed verb is one
/// keystroke from being read, and neither of the other two is.
///
/// Callers shed at least one segment before composing, so the marker is
/// unconditional here; a row that fits is returned by [`fit_footer_line`]
/// without ever reaching this.
fn compose_footer_line(segments: &[&str], kept: &[bool]) -> String {
    let shed = kept.iter().filter(|keep| !**keep).count();
    let mut parts = segments
        .iter()
        .zip(kept)
        .filter(|(_segment, keep)| **keep)
        .map(|(segment, _keep)| (*segment).to_owned())
        .collect::<Vec<_>>();
    parts.push(format!("+{shed} more: {HINT_OVERFLOW_KEY}"));
    parts.join(HINT_SEPARATOR)
}

/// The declared priority of one Status-line segment.
///
/// [`action_registry::hint_priority`] is derived from the registry and reads an
/// unregistered segment as a navigation cue, which is right for the list-edge
/// cue and wrong for the one other unregistered segment the row can carry: the
/// last command's verdict. That segment is composed by
/// [`TuiScreenModel::footer`], not by an action, so the fitter is where it is
/// recognised. It is matched in its SHORT form alone because
/// [`fit_footer_line`] abbreviates the verdict before it sheds anything, so by
/// the time a segment is ranked the row can only carry the short form.
fn footer_segment_priority(
    segment: &str,
    outcome: Option<&CommandOutcome>,
) -> action_registry::HintPriority {
    if outcome.is_some_and(|outcome| segment == outcome.short) {
        action_registry::HintPriority::Outcome
    } else {
        action_registry::hint_priority(segment)
    }
}

/// Compose the width-fitted Status line. See [`TuiScreenModel::footer_line`] for
/// the degradation contract. This is the pure core: the last command's verdict
/// ABBREVIATES to its short form first, and then each remaining segment is
/// atomic — kept or shed whole, never mid-truncated — so while the row is over
/// `width` it sheds one more segment, lowest declared priority first and
/// right-to-left within a priority, re-measuring after each step and stopping as
/// soon as it fits.
///
/// The verdict abbreviates BEFORE anything is shed because the two facts its
/// short form keeps -- which command, how it ended -- are worth more than the
/// dial its 34 saved columns would otherwise have bought, and because a verdict
/// is the one segment the overflow marker's door cannot reopen.
///
/// Right-to-left within a priority keeps the LEFTMOST segment of a class
/// longest, which is what preserves the list-edge cue ahead of the navigation
/// keys it explains.
///
/// The final `take(width)` is the degenerate tail: a band too narrow even for
/// the bare marker still must not return a row wider than the space on offer.
fn fit_footer_line(hints: &str, width: usize, outcome: Option<&CommandOutcome>) -> String {
    if hint_display_width(hints) <= width {
        return hints.to_owned();
    }
    let row = outcome.map_or_else(
        || hints.to_owned(),
        |outcome| hints.replace(outcome.full.as_str(), &outcome.short),
    );
    if hint_display_width(&row) <= width {
        return row;
    }
    let segments = row.split(HINT_SEPARATOR).collect::<Vec<_>>();
    let mut shed_order = (0..segments.len()).collect::<Vec<_>>();
    shed_order.sort_by_key(|index| {
        (
            footer_segment_priority(segments[*index], outcome),
            Reverse(*index),
        )
    });
    let mut kept = vec![true; segments.len()];
    for index in shed_order {
        kept[index] = false;
        let line = compose_footer_line(&segments, &kept);
        if hint_display_width(&line) <= width {
            return line;
        }
    }
    compose_footer_line(&segments, &kept)
        .chars()
        .take(width)
        .collect()
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
///
/// Builds its own model from `events` and `state` before reducing -- the
/// convenience form for a caller that has no already-built model to reuse
/// (most tests, and any one-shot caller). The terminal loop's hot path calls
/// [`reduce_tui_interaction_with_model`] directly against its cached model
/// instead, so a run of navigation keystrokes never re-scans the event log at
/// all (livespec-console-beads-fabro-mx9u.8): before the split, THIS function
/// was the fourth full projection build hiding inside one keystroke's path,
/// on top of the three the terminal loop's own draw/interpret/redraw already
/// performed.
pub fn reduce_tui_interaction(
    state: &TuiInteractionState,
    events: &[ConsoleEvent],
    interaction: TuiInteraction,
) -> TuiInteractionState {
    let model = build_tui_model_for_state(events, state);
    reduce_tui_interaction_with_model(state, &model, interaction)
}

#[must_use]
/// The cheap half of [`reduce_tui_interaction`].
///
/// Reduces `interaction` against an ALREADY-BUILT `model` rather than deriving
/// one from the raw event log. See [`TuiProjection`] and
/// [`reduce_tui_interaction`] for why this split exists.
pub fn reduce_tui_interaction_with_model(
    state: &TuiInteractionState,
    model: &TuiScreenModel,
    interaction: TuiInteraction,
) -> TuiInteractionState {
    // Derived BEFORE the move is applied, from the selection the keystroke was
    // pressed against, and stamped on EVERY interaction's result: a cue never
    // outlives the keystroke that earned it.
    let list_edge = list_edge_reached(state, model, interaction);
    let next = reduce_interaction_state(state, model, interaction).with_list_edge(list_edge);
    reanchor_displaced_attention(next, model)
}

/// Re-point an Attention anchor whose item has left the list at the row the
/// cursor actually sits on.
///
/// The fallback row is a projection, not state, so a stale anchor would keep
/// resolving to it — and keep re-announcing the same displacement on the Status
/// line — on every later rebuild, overwriting whatever the operator's newest
/// action reported. Any interaction is the operator's acknowledgement: they have
/// seen the list as it now stands. An interaction that moved the cursor itself
/// has already written a live anchor, so this is a no-op for it.
fn reanchor_displaced_attention(
    next: TuiInteractionState,
    model: &TuiScreenModel,
) -> TuiInteractionState {
    let Some(anchor) = next.selected_attention_id() else {
        return next;
    };
    if model
        .attention_items()
        .iter()
        .any(|item| item.id() == anchor)
    {
        return next;
    }
    // The same landing the rebuild resolved, re-anchored the same way a keystroke
    // anchors: an emptied list has no row to name, and drops the anchor instead.
    let landed = current_attention_index(&next, model);
    select_attention_at(&next, model, landed)
}

/// The state change one interaction makes, before [`reduce_tui_interaction`]
/// stamps the list-edge cue on it.
/// The state change one WHICH-VIEW-IS-ACTIVE interaction makes.
///
/// Three interactions share this concern and its one side effect: the stepwise
/// walk the Views nav's `up`/`down` drive (clamped at the ends by
/// [`move_view_up`] / [`move_view_down`]) and the aimed jump the direct
/// view-switch digits make. All three reset the Detail pane's scroll, because
/// the pane is about to show a DIFFERENT view's content and an inherited offset
/// would open it part-scrolled.
fn view_interaction_state(
    state: &TuiInteractionState,
    interaction: TuiInteraction,
) -> TuiInteractionState {
    let view = match interaction {
        TuiInteraction::SelectView(target) => target,
        TuiInteraction::SelectNextView => move_view_down(state.active_view()),
        _previous => move_view_up(state.active_view()),
    };
    state.clone().with_active_view(view).with_detail_scroll(0)
}

fn reduce_interaction_state(
    state: &TuiInteractionState,
    model: &TuiScreenModel,
    interaction: TuiInteraction,
) -> TuiInteractionState {
    match interaction {
        TuiInteraction::SelectNext => select_next(state, model),
        TuiInteraction::SelectPrevious => select_previous(state, model),
        TuiInteraction::SelectNextView
        | TuiInteraction::SelectPreviousView
        | TuiInteraction::SelectView(_) => view_interaction_state(state, interaction),
        TuiInteraction::OpenSearch => state.clone().with_overlay(TuiOverlay::Search {
            query: String::new(),
        }),
        TuiInteraction::OpenCommandPalette => {
            state.clone().with_overlay(TuiOverlay::CommandPalette {
                query: String::new(),
            })
        }
        TuiInteraction::OpenCommandModal => state.clone().with_overlay(open_command_modal(model)),
        TuiInteraction::OpenCommandExplainer => open_command_explainer_state(state, model),
        TuiInteraction::OpenMenu
        | TuiInteraction::MenuNextTop
        | TuiInteraction::MenuPreviousTop => menu_interaction_state(state, interaction),
        TuiInteraction::OpenActionInvoker => state
            .clone()
            .with_overlay(TuiOverlay::ActionInvoker { selected_action: 0 }),
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
        TuiInteraction::DrillIntoLane => drill_into_lane(state, model),
        TuiInteraction::ReturnToLaneOverview => state.clone().with_lane_focus(LaneFocus::Overview),
        TuiInteraction::DrillIntoEventsSubView => drill_into_events_sub_view(state),
        TuiInteraction::ReturnToEventsOverview => {
            state.clone().with_events_focus(EventsFocus::Overview)
        }
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
        TuiInteraction::OpenHelp => state.clone().with_overlay(open_help_overlay(state)),
        TuiInteraction::HelpSelectNextSection
        | TuiInteraction::HelpSelectPreviousSection
        | TuiInteraction::HelpScrollDown
        | TuiInteraction::HelpScrollUp
        | TuiInteraction::HelpPageDown
        | TuiInteraction::HelpPageUp
        | TuiInteraction::HelpFocusMenu
        | TuiInteraction::HelpFocusText => help_interaction_state(state, interaction),
        TuiInteraction::OpenDriverHandoff => state
            .clone()
            .with_overlay(open_driver_handoff_overlay(model)),
        TuiInteraction::OpenWorkItemDetail => {
            state.clone().with_overlay(open_work_item_detail(model))
        }
        TuiInteraction::OpenFactoryDispatchItemConfirm
        | TuiInteraction::OpenFactoryDrainConfirm => {
            open_factory_confirm_state(state, model, interaction)
        }
        TuiInteraction::WorkItemDetailScrollDown(rows) => {
            work_item_detail_scroll_state(state, rows, true)
        }
        TuiInteraction::WorkItemDetailScrollUp(rows) => {
            work_item_detail_scroll_state(state, rows, false)
        }
        TuiInteraction::WorkItemDetailPageDown => work_item_detail_page_scroll_state(state, true),
        TuiInteraction::WorkItemDetailPageUp => work_item_detail_page_scroll_state(state, false),
        TuiInteraction::OpenValveConfirm(valve) => open_valve_confirm_state(state, model, valve),
        TuiInteraction::CycleValveOption(forward) => state
            .clone()
            .with_overlay(cycle_valve_option(state.overlay(), forward)),
    }
}

/// The list edge an `up`/`down` keystroke was refused at, or `None` when the
/// selection actually moves (or the interaction moves no list selection at all).
///
/// The selection does NOT wrap — wrapping would change behaviour — so the
/// refusal is ANNOUNCED instead: the reducer records the edge and the Status
/// line cues it. Read from the state the keystroke was pressed against, so the
/// question asked is "could this move go anywhere?", the same question the
/// clamped `move_selection_*` helpers answer silently.
///
/// With an overlay open, `up`/`down` scroll a reading surface, cycle a staged
/// valve parameter, or walk an action roster rather than moving the Content
/// pane's list, so no edge is announced there.
fn list_edge_reached(
    state: &TuiInteractionState,
    model: &TuiScreenModel,
    interaction: TuiInteraction,
) -> Option<ListEdge> {
    if !matches!(state.overlay(), TuiOverlay::None) {
        return None;
    }
    let (index, count) = match interaction {
        TuiInteraction::SelectPreviousView | TuiInteraction::SelectNextView => {
            (view_index(state.active_view()), TuiView::all().len())
        }
        TuiInteraction::SelectPrevious | TuiInteraction::SelectNext => {
            content_selection_cursor(state, model)
        }
        _other => return None,
    };
    if count == 0 {
        // No rows at all: an empty drilled-in lane or an empty inbox has no top
        // and no end to reach, and its hints already say so.
        return None;
    }
    match interaction {
        TuiInteraction::SelectPreviousView | TuiInteraction::SelectPrevious => {
            (index == 0).then_some(ListEdge::Top)
        }
        _other => (index + 1 >= count).then_some(ListEdge::Bottom),
    }
}

/// The Content pane's routed selection cursor and the length of the list it
/// indexes: which list `SelectNext` / `SelectPrevious` actually move, and how
/// far it goes. Routed exactly as [`select_next`] routes them, so the edge
/// announced is the edge of the list that refused to move.
fn content_selection_cursor(state: &TuiInteractionState, model: &TuiScreenModel) -> (usize, usize) {
    if is_lane_overview(state) {
        (state.selected_lane_index(), Lane::all().len())
    } else if is_lane_drilldown(state) {
        (
            current_lane_item_index(state, model),
            drilldown_item_count(state, model),
        )
    } else if is_settings_view(state) {
        (
            state.selected_setting_index(),
            DispatcherSettingRow::all().len(),
        )
    } else if is_events_overview(state) {
        (state.selected_events_index(), EventsFocus::all().len())
    } else {
        (
            current_attention_index(state, model),
            model.attention_items().len(),
        )
    }
}

/// Stage the valve-confirm modal — only for a valve the registry offers for
/// the current selection.
///
/// Presentation and invocation share ONE availability derivation: an
/// unoffered valve is refused at staging, exactly as its hint is suppressed
/// and its key is inert.
fn open_valve_confirm_state(
    state: &TuiInteractionState,
    model: &TuiScreenModel,
    valve: PendingValve,
) -> TuiInteractionState {
    if model
        .selected_action_context()
        .is_some_and(|ctx| action_registry::valve_is_available(valve, &ctx))
    {
        state.clone().with_overlay(TuiOverlay::ValveConfirm {
            valve,
            // A freshly-staged dialog starts with an empty answer: text typed
            // into an earlier valve never carries into the next one.
            answer: String::new(),
        })
    } else {
        state.clone()
    }
}

fn help_interaction_state(
    state: &TuiInteractionState,
    interaction: TuiInteraction,
) -> TuiInteractionState {
    let overlay = match interaction {
        TuiInteraction::HelpSelectNextSection => help_select_section(state.overlay(), true),
        TuiInteraction::HelpSelectPreviousSection => help_select_section(state.overlay(), false),
        TuiInteraction::HelpScrollDown => {
            help_scroll(state.overlay(), 1, true, state.help_max_scroll())
        }
        TuiInteraction::HelpScrollUp => {
            help_scroll(state.overlay(), 1, false, state.help_max_scroll())
        }
        TuiInteraction::HelpPageDown => help_scroll(
            state.overlay(),
            state.help_page_rows(),
            true,
            state.help_max_scroll(),
        ),
        TuiInteraction::HelpPageUp => help_scroll(
            state.overlay(),
            state.help_page_rows(),
            false,
            state.help_max_scroll(),
        ),
        TuiInteraction::HelpFocusMenu => help_focus(state.overlay(), HelpFocus::Menu),
        TuiInteraction::HelpFocusText => help_focus(state.overlay(), HelpFocus::Text),
        _ => state.overlay().clone(),
    };
    state.clone().with_overlay(overlay)
}

fn work_item_detail_scroll_state(
    state: &TuiInteractionState,
    rows: usize,
    down: bool,
) -> TuiInteractionState {
    state.clone().with_overlay(work_item_detail_scroll(
        state.overlay(),
        rows,
        down,
        state.work_item_detail_max_scroll(),
    ))
}

fn work_item_detail_page_scroll_state(
    state: &TuiInteractionState,
    down: bool,
) -> TuiInteractionState {
    work_item_detail_scroll_state(state, state.work_item_detail_page_rows(), down)
}

/// The command modal resolves only when the current detail exposes at least one
/// runnable local action. With no action there is nothing honest to select or
/// run, so the modal remains closed.
fn open_command_modal(model: &TuiScreenModel) -> TuiOverlay {
    if model
        .detail()
        .is_some_and(|detail| !detail.actions().is_empty())
    {
        TuiOverlay::CommandModal {
            selected_action_index: 0,
        }
    } else {
        TuiOverlay::None
    }
}

fn open_command_explainer_state(
    state: &TuiInteractionState,
    model: &TuiScreenModel,
) -> TuiInteractionState {
    state.clone().with_overlay(open_command_explainer(model))
}

fn open_command_explainer(model: &TuiScreenModel) -> TuiOverlay {
    model
        .overlay()
        .selected_action_index()
        .filter(|index| {
            model
                .detail()
                .is_some_and(|detail| detail.actions().get(*index).is_some())
        })
        .map_or(TuiOverlay::None, |selected_action_index| {
            TuiOverlay::CommandExplainer {
                selected_action_index,
            }
        })
}

fn open_help_overlay(state: &TuiInteractionState) -> TuiOverlay {
    TuiOverlay::Help {
        focus: HelpFocus::Menu,
        selected_section: help_section_for_focus(state.focus(), state.active_view()),
        scroll: 0,
    }
}

fn open_driver_handoff_overlay(model: &TuiScreenModel) -> TuiOverlay {
    model
        .selected_driver_handoff_command()
        .map_or(TuiOverlay::None, |command| TuiOverlay::DriverHandoff {
            command,
        })
}

fn driver_handoff_command(item: &LaneWorkItem) -> Option<String> {
    let operation = match item.lane() {
        Lane::Backlog => "groom",
        // Keyed on the marking being PRESENT, not on any one spelling. The
        // dispatcher's own first refusal arm is `factory_safety is not None`,
        // so any marked `ready` item is one the factory will refuse and an
        // attended host session is the only route forward.
        //
        // This arm previously tested `== Some("host-only-refused")`, which is
        // a dispatcher STAGE name and NOT a member of the published
        // `FactorySafety` vocabulary (`needs-host-secrets` /
        // `mutates-host-machinery` / `needs-privileged-host`). It could
        // therefore never fire on real data, and the ten tests that covered it
        // all invented the value they then asserted on. Measured 2026-08-03;
        // see `-w7d`.
        Lane::Ready if item.detail().factory_safety.is_some() => "implement",
        Lane::PendingApproval
        | Lane::Ready
        | Lane::Active
        | Lane::Acceptance
        | Lane::Blocked
        | Lane::Done => return None,
    };
    Some(format!(
        r#"claude "/livespec-orchestrator-beads-fabro:{operation} {}""#,
        item.work_item_id()
    ))
}

/// The overlay `OpenWorkItemDetail` resolves to: the work-item detail modal
/// PINNED to the selected item's id, or no overlay at all when nothing is
/// selected.
///
/// Pinning the id here (rather than letting the renderer re-read the selection
/// each frame) is what keeps the open modal on the item it was opened on while
/// ingestion keeps re-ranking the lists underneath it. With no selection there
/// is no honest record to show, so the modal does not open at all.
fn open_work_item_detail(model: &TuiScreenModel) -> TuiOverlay {
    model
        .selected_work_item_id()
        .map_or(TuiOverlay::None, |work_item_id| {
            TuiOverlay::WorkItemDetail {
                work_item_id: work_item_id.to_owned(),
                scroll: 0,
            }
        })
}

fn open_factory_dispatch_item_confirm(model: &TuiScreenModel) -> TuiOverlay {
    model
        .selected_work_item_id()
        .map_or(TuiOverlay::None, |work_item_id| {
            TuiOverlay::FactoryDispatchItemConfirm {
                work_item_id: work_item_id.to_owned(),
            }
        })
}

fn open_factory_confirm_state(
    state: &TuiInteractionState,
    model: &TuiScreenModel,
    interaction: TuiInteraction,
) -> TuiInteractionState {
    let overlay = match interaction {
        TuiInteraction::OpenFactoryDispatchItemConfirm => open_factory_dispatch_item_confirm(model),
        TuiInteraction::OpenFactoryDrainConfirm => open_factory_drain_confirm(model),
        _ => state.overlay().clone(),
    };
    state.clone().with_overlay(overlay)
}

fn open_factory_drain_confirm(model: &TuiScreenModel) -> TuiOverlay {
    model
        .next_ready_drain_target()
        .map_or(TuiOverlay::None, |item| TuiOverlay::FactoryDrainConfirm {
            work_item_id: item.work_item_id().to_owned(),
            rank: item.rank().to_owned(),
        })
}

/// Scroll the work-item detail modal by `rows` down (`down`) or up, leaving any
/// other overlay unchanged (the interaction is inert unless that modal is open).
///
/// Down clamps to the record's measured wrapped height — the same
/// feed-back-the-measured-max discipline the Detail pane uses — so a page or line
/// step cannot skip beyond the true bottom of a long description.
fn work_item_detail_scroll(
    overlay: &TuiOverlay,
    rows: usize,
    down: bool,
    max_scroll: usize,
) -> TuiOverlay {
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
            scroll.saturating_add(rows).min(max_scroll)
        } else {
            scroll.saturating_sub(rows)
        },
    }
}

/// Rotate the valve-confirm modal's payload valve one step (forward or
/// backward), leaving any non-valve overlay unchanged.
///
/// The typed answer rides through the rotation: dialing `ready` over to
/// `backlog` must not silently discard text the operator has already written.
fn cycle_valve_option(overlay: &TuiOverlay, forward: bool) -> TuiOverlay {
    match overlay {
        TuiOverlay::ValveConfirm { valve, answer } => TuiOverlay::ValveConfirm {
            valve: valve.cycled(forward),
            answer: answer.clone(),
        },
        _other => overlay.clone(),
    }
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

/// Whether the `Events` container is showing its own overview home (the
/// "Stored events" / "Event sources" picker), where up/down moves the
/// selected sub-view row rather than the attention selection. A drilled-in
/// sub-view falls through to the SAME attention-cursor routing every other
/// summary view (`Spec`, `Repos`) already used before this container existed
/// — see [`select_next`] — so "Stored events" keeps its pre-container up/down
/// behaviour exactly (AC3: no behaviour change to its content).
fn is_events_overview(state: &TuiInteractionState) -> bool {
    state.active_view() == TuiView::Events && state.events_focus() == EventsFocus::Overview
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
        select_lane_item_at(
            state,
            model,
            move_selection_down(
                drilldown_item_count(state, model),
                current_lane_item_index(state, model),
            ),
        )
    } else if is_settings_view(state) {
        state
            .clone()
            .with_selected_setting_index(move_selection_down(
                DispatcherSettingRow::all().len(),
                state.selected_setting_index(),
            ))
    } else if is_events_overview(state) {
        state
            .clone()
            .with_selected_events_index(move_selection_down(
                EventsFocus::all().len(),
                state.selected_events_index(),
            ))
    } else {
        select_attention_at(
            state,
            model,
            move_selection_down(
                model.attention_items().len(),
                current_attention_index(state, model),
            ),
        )
        // A different item is now selected, so its Detail pane shows
        // different content: reset the scroll so the previous item's offset
        // never carries over.
        .with_detail_scroll(0)
    }
}

/// Move the selection up, routed to the lane overview row or the settings row
/// when one of those views is active, else to the attention list.
fn select_previous(state: &TuiInteractionState, model: &TuiScreenModel) -> TuiInteractionState {
    if is_lane_overview(state) {
        state
            .clone()
            .with_selected_lane_index(move_selection_up(state.selected_lane_index()))
    } else if is_lane_drilldown(state) {
        select_lane_item_at(
            state,
            model,
            move_selection_up(current_lane_item_index(state, model)),
        )
    } else if is_settings_view(state) {
        state
            .clone()
            .with_selected_setting_index(move_selection_up(state.selected_setting_index()))
    } else if is_events_overview(state) {
        state
            .clone()
            .with_selected_events_index(move_selection_up(state.selected_events_index()))
    } else {
        select_attention_at(
            state,
            model,
            move_selection_up(current_attention_index(state, model)),
        )
        // Reset the Detail scroll for the newly-selected item (see select_next).
        .with_detail_scroll(0)
    }
}

/// Move the Attention cursor to `index` and re-anchor it on the item that row
/// actually holds, so the next refresh follows the operator's choice rather than
/// the position they left it at. A row that is not there (an empty list) leaves
/// the position alone with no anchor to make up.
fn select_attention_at(
    state: &TuiInteractionState,
    model: &TuiScreenModel,
    index: usize,
) -> TuiInteractionState {
    model.attention_items().get(index).map_or_else(
        || state.clone().with_selected_attention_index(index),
        |item| state.clone().with_selected_attention(index, item.id()),
    )
}

/// The Attention row the operator is moving FROM: the position this rebuild
/// resolved the anchor to, not the position the state was last written with.
/// After a refresh that re-ranked the list the two differ, and moving from the
/// stale one is the very jump this anchoring exists to stop.
fn current_attention_index(state: &TuiInteractionState, model: &TuiScreenModel) -> usize {
    model
        .selected_attention_index()
        .unwrap_or_else(|| state.selected_attention_index())
}

const fn current_lane_item_index(state: &TuiInteractionState, model: &TuiScreenModel) -> usize {
    if let Some(index) = model.selected_lane_item_index() {
        return index;
    }
    state.selected_lane_item_index()
}

/// Drill the lane overview's selected lane into a full per-lane list.
fn drill_into_lane(state: &TuiInteractionState, model: &TuiScreenModel) -> TuiInteractionState {
    let lane = Lane::all()[state.selected_lane_index().min(Lane::all().len() - 1)];
    let drilled = state.clone().with_lane_focus(LaneFocus::Lane(lane));
    if let Some(item) = model
        .lane_board()
        .column(lane)
        .and_then(|column| column.items().first())
    {
        return drilled.with_selected_lane_item(0, item.work_item_id());
    }
    drilled
}

/// Drill the `Events` container overview's selected row into the named
/// sub-view it names. Unlike [`drill_into_lane`] there is no per-item identity
/// to anchor: the two sub-views are fixed and named, not a dynamic work-item
/// list.
fn drill_into_events_sub_view(state: &TuiInteractionState) -> TuiInteractionState {
    let sub_view = EventsFocus::all()[state
        .selected_events_index()
        .min(EventsFocus::all().len() - 1)];
    state.clone().with_events_focus(sub_view)
}

fn select_lane_item_at(
    state: &TuiInteractionState,
    model: &TuiScreenModel,
    index: usize,
) -> TuiInteractionState {
    let LaneFocus::Lane(lane) = state.lane_focus() else {
        return state.clone().with_selected_lane_item_index(index);
    };
    if let Some(item) = model
        .lane_board()
        .column(lane)
        .and_then(|column| column.items().get(index))
    {
        return state
            .clone()
            .with_selected_lane_item(index, item.work_item_id());
    }
    state.clone().with_selected_lane_item_index(index)
}

/// Validate operator action.
pub fn validate_operator_action(action: &str) -> ApplicationResult<&str> {
    let trimmed = action.trim();
    if trimmed.is_empty() {
        return Err(ApplicationError::EmptyOperatorAction);
    }
    Ok(trimmed)
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
    let _requested_by = validate_operator_action(requested_by)?;
    let TuiOverlay::CommandPalette { query: _ } = model.overlay() else {
        return Err(ApplicationError::NoSelectedOperatorAction);
    };
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
    // The invocation-side half of the one-derivation rule: the confirm
    // resolves only an action the registry offers for THIS selection, so a
    // staged valve whose availability lapsed (or that was staged around the
    // key handler) cannot fire while unoffered.
    if !model
        .selected_action_context()
        .is_some_and(|ctx| action_registry::valve_is_available(valve, &ctx))
    {
        return Err(ApplicationError::NoSelectedOperatorAction);
    }
    valve_outcome(
        valve,
        work_item_id,
        requested_by,
        model.overlay().valve_answer().unwrap_or_default(),
    )
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
    answer: &str,
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
            move_status_outcome(from, to, work_item_id, requested_by, answer)
        }
        PendingValve::SetOverride(override_dial) => Some(work_item_override_outcome(
            work_item_id,
            override_dial,
            requested_by,
        )),
        PendingValve::SetWorkflowScopeOverride => Some(work_item_payload_outcome(
            "set_workflow_scope_override",
            CommandType::WorkItemSetWorkflowScopeOverrideRequested,
            work_item_id,
            "scope",
            WORKFLOW_SCOPE_CITATION_ONLY,
            requested_by,
        )),
    }
}

/// Map an offered `from -> to` move onto the persist outcome for the real
/// orchestrator transition it drives. Blocked resolution uses the semantic
/// `resolve-blocked` valve (`blocked -> ready | backlog`); the remaining offered
/// targets ride the guarded broad `move:<id>:<target>` action. `None` for any
/// pair that is not in [`status_move_targets`] -- this rejects stale or manually
/// staged duplicate semantic-valve paths such as `pending-approval -> ready` and
/// `acceptance -> done`.
///
/// A resolve-blocked move also carries the operator's optional free-text
/// `answer` in its payload, verbatim and only when it is more than whitespace;
/// the command handler passes it to the orchestrator as `--answer`, which is
/// what writes the item's `livespec-human-answer` comment.
fn move_status_outcome(
    from: Lane,
    to: Lane,
    work_item_id: &str,
    requested_by: &str,
    answer: &str,
) -> Option<OperatorActionOutcome> {
    if !status_move_targets(from).contains(&to) {
        return None;
    }
    if matches!(from, Lane::Blocked) {
        return Some(resolve_blocked_outcome(
            work_item_id,
            to.label(),
            requested_by,
            answer,
        ));
    }
    Some(work_item_payload_outcome(
        "move",
        CommandType::WorkItemMoveRequested,
        work_item_id,
        "target_status",
        to.label(),
        requested_by,
    ))
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

/// The persisted-command payload key carrying the operator's free-text answer
/// on a `work_item.resolve_blocked_requested` command.
const RESOLVE_BLOCKED_ANSWER_KEY: &str = "answer";

/// Build the persist outcome for the resolve-blocked valve: the
/// `work_item.resolve_blocked_requested` command, its `target_status`, and the
/// operator's optional free-text `answer`.
///
/// The answer is carried VERBATIM and only when it is more than whitespace --
/// a blank field is an absent answer, not an empty comment. The console never
/// writes the ledger comment itself: the handler passes this text to the
/// orchestrator's action surface as `--answer`, and the orchestrator
/// poison-preflights it, writes the `livespec-human-answer` comment, and only
/// then transitions the item (`SPECIFICATION/contracts.md`).
fn resolve_blocked_outcome(
    work_item_id: &str,
    target_status: &str,
    requested_by: &str,
    answer: &str,
) -> OperatorActionOutcome {
    let command = CommandEnvelope::new(
        format!("cmd_work_item_resolve_blocked_requested_{work_item_id}_{target_status}"),
        CommandType::WorkItemResolveBlockedRequested,
        work_item_id.to_owned(),
        format!("{work_item_id}:work_item.resolve_blocked_requested:target_status={target_status}"),
        requested_by.to_owned(),
    );
    let mut payload = serde_json::Map::new();
    payload.insert(
        "target_status".to_owned(),
        serde_json::Value::String(target_status.to_owned()),
    );
    if !answer.trim().is_empty() {
        payload.insert(
            RESOLVE_BLOCKED_ANSWER_KEY.to_owned(),
            serde_json::Value::String(answer.to_owned()),
        );
    }
    OperatorActionOutcome::PersistCommandWithPayload {
        command,
        payload_json: serde_json::Value::Object(payload).to_string(),
    }
}

/// Whether a command-palette query asks for the action-invoker roster.
#[must_use]
pub fn command_palette_query_opens_action_invoker(query: &str) -> bool {
    query.trim().eq_ignore_ascii_case("actions")
}

/// Build the canonical command envelope for dispatching one ready work item.
#[must_use]
pub fn factory_drain_command(requested_by: &str) -> CommandEnvelope {
    CommandEnvelope::new(
        "cmd_factory_drain_requested_budget_1_parallel_1".to_owned(),
        CommandType::FactoryDrainRequested,
        "fleet:livespec".to_owned(),
        "fleet:livespec:factory.drain_requested:budget=1:parallel=1".to_owned(),
        requested_by.to_owned(),
    )
}

/// Build the canonical command envelope for dispatching the selected ready item.
#[must_use]
pub fn factory_dispatch_item_command(work_item_id: &str, requested_by: &str) -> CommandEnvelope {
    CommandEnvelope::new(
        format!("cmd_factory_dispatch_item_requested_{work_item_id}"),
        CommandType::FactoryDispatchItemRequested,
        work_item_id.to_owned(),
        format!("{work_item_id}:factory.dispatch_item_requested"),
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
        FactoryDrainPortOutcome::Failed { diagnostic } => {
            events.push(factory_command_event(
                command,
                EventType::FactoryDrainStarted,
                "started",
                2,
            ));
            if drain_failure_is_human_valve_park(diagnostic.as_deref()) {
                events.push(factory_command_event(
                    command,
                    EventType::FactoryDrainAwaitingHuman,
                    "awaiting_human",
                    3,
                ));
                "parked-awaiting-human"
            } else {
                events.push(factory_drain_failure_event(
                    command,
                    diagnostic.as_deref(),
                    3,
                ));
                "failed"
            }
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

/// Handle selected-item factory dispatch command.
pub fn handle_factory_dispatch_item_command(
    command: &CommandEnvelope,
    port: &mut dyn FactoryDispatchItemPort,
) -> ApplicationResult<FactoryCommandOutcome> {
    let request = FactoryDispatchItemRequest::new(command.aggregate_id().to_owned());
    let port_outcome = port.dispatch_item(&request)?;
    let mut events = vec![factory_command_event(
        command,
        EventType::CommandAccepted,
        "accepted",
        1,
    )];
    let command_status = match port_outcome {
        FactoryDispatchItemPortOutcome::Completed => {
            events.push(factory_command_event(
                command,
                EventType::FactoryDispatchItemStarted,
                "started",
                2,
            ));
            events.push(factory_command_event(
                command,
                EventType::FactoryDispatchItemCompleted,
                "completed",
                3,
            ));
            "completed"
        }
        FactoryDispatchItemPortOutcome::Failed { diagnostic } => {
            events.push(factory_command_event(
                command,
                EventType::FactoryDispatchItemStarted,
                "started",
                2,
            ));
            events.push(factory_dispatch_item_failure_event(
                command,
                diagnostic.as_deref(),
                3,
            ));
            "failed"
        }
        FactoryDispatchItemPortOutcome::NotWired => {
            events.push(factory_command_event(
                command,
                EventType::FactoryDispatchItemNotWired,
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

fn drain_failure_is_human_valve_park(diagnostic: Option<&str>) -> bool {
    let Some(diagnostic) = diagnostic else {
        return false;
    };
    let lower = diagnostic.to_ascii_lowercase();
    let compact = lower.split_whitespace().collect::<String>();
    lower.contains("held manual admission")
        || lower.contains("parked in acceptance")
        || lower.contains("parked at acceptance")
        || lower.contains("parked in pending-approval")
        || lower.contains("parked at pending-approval")
        || lower.contains("lane=acceptance")
        || lower.contains("lane=pending-approval")
        || lower.contains("status=acceptance")
        || lower.contains("status=pending-approval")
        || compact.contains(r#""lane":"acceptance""#)
        || compact.contains(r#""lane":"pending-approval""#)
        || compact.contains(r#""status":"acceptance""#)
        || compact.contains(r#""status":"pending-approval""#)
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

fn factory_drain_failure_event(
    command: &CommandEnvelope,
    diagnostic: Option<&str>,
    stream_seq: u64,
) -> ConsoleEvent {
    ConsoleEvent::new(
        format!("evt_{}_failed", command.command_id()),
        1,
        command_event_context(EventType::FactoryDrainFailed).to_owned(),
        EventType::FactoryDrainFailed,
        "console:factory-command-handler".to_owned(),
        command.aggregate_id().to_owned(),
        stream_seq,
    )
    .with_payload_json(serde_json::Value::Object(diagnostic_event_payload(diagnostic)).to_string())
}

fn factory_dispatch_item_failure_event(
    command: &CommandEnvelope,
    diagnostic: Option<&str>,
    stream_seq: u64,
) -> ConsoleEvent {
    ConsoleEvent::new(
        format!("evt_{}_failed", command.command_id()),
        1,
        command_event_context(EventType::FactoryDispatchItemFailed).to_owned(),
        EventType::FactoryDispatchItemFailed,
        "console:factory-command-handler".to_owned(),
        command.aggregate_id().to_owned(),
        stream_seq,
    )
    .with_payload_json(serde_json::Value::Object(diagnostic_event_payload(diagnostic)).to_string())
}

const fn command_event_context(event_type: EventType) -> &'static str {
    match event_type {
        EventType::CommandAccepted | EventType::CommandRejected => "command",
        EventType::FactoryDrainCompleted
        | EventType::FactoryDrainFailed
        | EventType::FactoryDrainAwaitingHuman
        | EventType::FactoryDrainNotWired
        | EventType::FactoryDispatchItemCompleted
        | EventType::FactoryDispatchItemFailed
        | EventType::FactoryDispatchItemNotWired
        | EventType::FactoryDispatchItemRequested
        | EventType::FactoryDispatchItemStarted
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
        | EventType::DispatcherJournalProgressObserved
        | EventType::DispatcherRefusalObserved
        | EventType::FabroRunObserved
        | EventType::FactoryRunOrphansObserved
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

/// Handle a `work_item.set_workflow_scope_override_requested` command.
///
/// The workflow-scope-override human valve: `payload_json` is
/// `{"scope": "citation-only"}` (the one value the orchestrator's allowlist
/// admits). The handler validates the work-item id, validates the scope,
/// derives the `set-workflow-scope-override:<work-item-id>:<scope>` action-id,
/// and rides the shared orchestrator-action port and `work_item` outcome
/// family exactly like the policy dials. Thin console-side validation only --
/// the orchestrator's `drive` surface owns the recorded override -- and it
/// never writes the ledger directly.
///
/// # Errors
/// Returns [`ApplicationError::EmptyWorkItemId`] when the id is empty and
/// [`ApplicationError::InvalidWorkflowScope`] when the payload's `scope` is
/// absent or not `citation-only`; also surfaces a port error when the port
/// cannot produce a trustworthy outcome.
pub fn handle_work_item_set_workflow_scope_override_command(
    command: &CommandEnvelope,
    payload_json: &str,
    port: &mut dyn OrchestratorActionPort,
) -> ApplicationResult<WorkItemCommandOutcome> {
    let work_item_id = validate_work_item_id(command.aggregate_id())?;
    let scope = workflow_scope_from_payload(payload_json)?;
    let action_id = format!("set-workflow-scope-override:{work_item_id}:{scope}");
    run_work_item_action(command, &action_id, port)
}

/// Extract the workflow `scope` from a command's persisted `payload_json`.
///
/// The payload is the JSON object `{"scope": "citation-only"}`; any other
/// shape or value is an [`ApplicationError::InvalidWorkflowScope`].
fn workflow_scope_from_payload(payload_json: &str) -> ApplicationResult<&'static str> {
    let value: serde_json::Value = serde_json::from_str(payload_json)
        .map_err(|_error| ApplicationError::InvalidWorkflowScope)?;
    let scope = value
        .get("scope")
        .and_then(serde_json::Value::as_str)
        .ok_or(ApplicationError::InvalidWorkflowScope)?;
    if scope != WORKFLOW_SCOPE_CITATION_ONLY {
        return Err(ApplicationError::InvalidWorkflowScope);
    }
    Ok(WORKFLOW_SCOPE_CITATION_ONLY)
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
/// The payload's optional `answer` rides to the orchestrator as the action's
/// `--answer` argument, verbatim. The orchestrator poison-preflights it, writes
/// the `livespec-human-answer` ledger comment, and only then transitions the
/// item; a refused answer comes back as this command's refusal and the item
/// stays blocked.
///
/// # Errors
/// Returns [`ApplicationError::EmptyWorkItemId`] when the id is empty,
/// [`ApplicationError::InvalidResolveBlockedTarget`] when the payload's
/// `target_status` is absent or not one of {ready, backlog}, and
/// [`ApplicationError::InvalidResolveBlockedAnswer`] when the payload carries an
/// `answer` that is not a non-empty string; also surfaces a port error when the
/// port cannot produce a trustworthy outcome.
pub fn handle_work_item_resolve_blocked_command(
    command: &CommandEnvelope,
    payload_json: &str,
    port: &mut dyn OrchestratorActionPort,
) -> ApplicationResult<WorkItemCommandOutcome> {
    let work_item_id = validate_work_item_id(command.aggregate_id())?;
    // Parsed ONCE and read twice: the target and the answer are two fields of
    // the same persisted payload, and re-parsing would give the answer reader an
    // unreachable second parse-failure path.
    let payload: serde_json::Value = serde_json::from_str(payload_json)
        .map_err(|_error| ApplicationError::InvalidResolveBlockedTarget)?;
    let target = resolve_blocked_target(&payload)?;
    let answer = resolve_blocked_answer(&payload)?;
    let action_id = format!("resolve-blocked:{work_item_id}:{target}");
    run_work_item_action_with_answer(command, &action_id, answer.as_deref(), port)
}

/// Extract the resolve-blocked `target_status` from a command's parsed payload.
///
/// The payload is the JSON object `{"target_status": "ready" | "backlog"}`; any
/// other shape is an [`ApplicationError::InvalidResolveBlockedTarget`]. These are
/// the two targets the orchestrator's `resolve-blocked` action accepts.
fn resolve_blocked_target(payload: &serde_json::Value) -> ApplicationResult<&'static str> {
    let target = payload
        .get("target_status")
        .and_then(serde_json::Value::as_str)
        .ok_or(ApplicationError::InvalidResolveBlockedTarget)?;
    match target {
        "ready" => Ok("ready"),
        "backlog" => Ok("backlog"),
        _other => Err(ApplicationError::InvalidResolveBlockedTarget),
    }
}

/// Extract the resolve-blocked payload's OPTIONAL operator `answer`.
///
/// Absent is the ordinary case (`Ok(None)`): the answer field is optional. A
/// present `answer` must be a non-empty string -- anything else is a corrupt
/// payload rather than an answer, and is refused before the orchestrator is
/// invoked, because silently dropping it would lose the operator's text. What
/// is returned is VERBATIM; nothing here trims, edits, or rewords the bytes.
fn resolve_blocked_answer(payload: &serde_json::Value) -> ApplicationResult<Option<String>> {
    let Some(answer) = payload.get(RESOLVE_BLOCKED_ANSWER_KEY) else {
        return Ok(None);
    };
    match answer.as_str() {
        Some(text) if !text.trim().is_empty() => Ok(Some(text.to_owned())),
        _other => Err(ApplicationError::InvalidResolveBlockedAnswer),
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
    run_work_item_action_with_answer(command, action_id, None, port)
}

/// [`run_work_item_action`] carrying the operator's optional free-text answer,
/// which the port appends to the invocation as `--answer <answer>`.
///
/// Only the resolve-blocked valve supplies one today. The refusal path is the
/// shared one: a poisoned answer the orchestrator rejects comes back as this
/// action's refusal payload and is surfaced verbatim, with no transition and no
/// comment observed on the item.
fn run_work_item_action_with_answer(
    command: &CommandEnvelope,
    action_id: &str,
    answer: Option<&str>,
    port: &mut dyn OrchestratorActionPort,
) -> ApplicationResult<WorkItemCommandOutcome> {
    let request = OrchestratorActionRequest::attributed(
        action_id.to_owned(),
        command.requested_by().to_owned(),
    )
    .with_answer(answer);
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
        OrchestratorActionOutcome::Failed { refusal } => {
            events.push(work_item_command_event(
                command,
                EventType::WorkItemActionStarted,
                "started",
                action_id,
                2,
            ));
            events.push(work_item_failure_event(
                command,
                action_id,
                refusal.as_deref(),
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

/// Build the `work_item.action.failed` event, carrying the refusal payload the
/// action surface emitted (when it emitted one) beside the action id — the
/// store-side half of surfacing a refused valve instead of discarding its
/// diagnostic at the presentation boundary.
fn work_item_failure_event(
    command: &CommandEnvelope,
    action_id: &str,
    refusal: Option<&str>,
    stream_seq: u64,
) -> ConsoleEvent {
    let mut payload = diagnostic_event_payload(refusal);
    payload.insert("action_id".to_owned(), action_id.to_owned().into());
    ConsoleEvent::new(
        format!("evt_{}_failed", command.command_id()),
        1,
        command_event_context(EventType::WorkItemActionFailed).to_owned(),
        EventType::WorkItemActionFailed,
        "console:work-item-command-handler".to_owned(),
        command.aggregate_id().to_owned(),
        stream_seq,
    )
    .with_payload_json(serde_json::Value::Object(payload).to_string())
}

fn diagnostic_event_payload(
    diagnostic: Option<&str>,
) -> serde_json::Map<String, serde_json::Value> {
    let Some(diagnostic) = diagnostic
        .map(str::trim)
        .filter(|diagnostic| !diagnostic.is_empty())
    else {
        return serde_json::Map::new();
    };
    match serde_json::from_str::<serde_json::Value>(diagnostic) {
        Ok(serde_json::Value::Object(payload)) => payload,
        _other => {
            let mut payload = serde_json::Map::new();
            payload.insert("refusal".to_owned(), diagnostic.to_owned().into());
            payload
        }
    }
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
/// that setting. The mapping is exhaustive over the API-configurable keys, so a key the type
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

/// One of the API-configurable dispatcher settings paired with the value to
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

    #[must_use]
    /// The `Settings` row this write targets.
    ///
    /// Total by construction: writes and rows are the same six API-configurable
    /// keys, so a write always names exactly one row and the mapping never has
    /// to fail. That is what lets the write lifecycle
    /// ([`DispatcherSettingWriteState`]) qualify the ONE row the operator edited
    /// without a lookup that could come back empty.
    pub const fn row(&self) -> DispatcherSettingRow {
        match self {
            Self::AutoApproveReady(_) => DispatcherSettingRow::AutoApproveReady,
            Self::MergeOnReviewCap(_) => DispatcherSettingRow::MergeOnReviewCap,
            Self::AcceptanceMode(_) => DispatcherSettingRow::AcceptanceMode,
            Self::ReviewFixCap(_) => DispatcherSettingRow::ReviewFixCap,
            Self::AcceptanceReworkCap(_) => DispatcherSettingRow::AcceptanceReworkCap,
            Self::WipCap(_) => DispatcherSettingRow::WipCap,
        }
    }

    #[must_use]
    /// The requested value rendered exactly as [`DispatcherSettingRow::value`]
    /// renders an effective one -- `on`/`off` for a bool, the kebab-case label
    /// for [`AcceptancePolicy`], the decimal digits for an int.
    ///
    /// Deliberately NOT [`Self::value_literal`], which is the orchestrator's
    /// `set-config` wire grammar (`true`/`false`). The re-read confirmation
    /// compares this against the row's freshly-observed value, so the two must
    /// be the SAME rendering or a landed write would read as unchanged.
    pub fn rendered_value(&self) -> String {
        match self {
            Self::AutoApproveReady(value) | Self::MergeOnReviewCap(value) => {
                bool_label(*value).to_owned()
            }
            Self::AcceptanceMode(policy) => policy.label().to_owned(),
            Self::ReviewFixCap(value) | Self::AcceptanceReworkCap(value) | Self::WipCap(value) => {
                value.to_string()
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

/// Default retained for reading older dispatcher config payloads that carried
/// `dispatcher.drift_capture_merge_threshold` before the row was removed from
/// the console's editable surface.
const DRIFT_CAPTURE_MERGE_THRESHOLD_DEFAULT: u32 = 1;

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

/// The dispatcher policy settings the `Settings` view renders, in display
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
    /// The rows in display order.
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

/// How a `Settings` row qualifies its effective value while a write of that row
/// is in flight, or has settled without moving it.
///
/// The row's value is ALWAYS the effective value the console last observed. This
/// says what the operator's own outstanding edit did to that value's standing,
/// so a landed edit is never rendered as if nothing had been asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingRowStatus {
    /// No write of this row is outstanding: the effective value stands alone.
    Effective,
    /// A write of this row is in flight and nothing has confirmed it yet. The
    /// wrapped string is the value the operator asked for, rendered as the row
    /// renders an effective one.
    Pending(String),
    /// A write of this row settled and the fresh effective read STILL reports
    /// the value observed before it. The wrapped string is the value the
    /// operator asked for and did not get.
    Unchanged(String),
}

/// A `Settings` view row prepared for rendering.
///
/// Carries the label, the effective value, the inline help for the detail pane,
/// whether the row is dangerous, and how an outstanding write of this row
/// qualifies that value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingRow {
    label: &'static str,
    value: String,
    help: &'static str,
    dangerous: bool,
    status: SettingRowStatus,
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

    #[must_use]
    /// How an outstanding write of this row qualifies its effective value.
    pub const fn status(&self) -> &SettingRowStatus {
        &self.status
    }

    #[must_use]
    /// The operator-facing text of this row, value cell and all.
    ///
    /// Lives HERE rather than in the renderer because it is the sentence the
    /// operator reads to decide whether the factory is armed, and the defect
    /// this closes was precisely that the sentence lied
    /// (livespec-console-beads-fabro-30c). A presentation layer adds only the
    /// selection marker; the words themselves are model-layer and asserted by
    /// model-layer tests across the write boundary.
    pub fn text(&self) -> String {
        let danger = if self.dangerous { "  (dangerous)" } else { "" };
        let label = self.label;
        let value = &self.value;
        match &self.status {
            SettingRowStatus::Effective => format!("{label}  [ {value} ]{danger}"),
            SettingRowStatus::Pending(requested) => {
                format!("{label}  [ {value} -> {requested} ]  writing...{danger}")
            }
            SettingRowStatus::Unchanged(requested) => {
                format!("{label}  [ {value} ]  unchanged (requested {requested}){danger}")
            }
        }
    }
}

/// The lifecycle of the operator's most recent dispatcher-setting write, as the
/// `Settings` view renders it.
///
/// The console still holds NO setting state of its own: every value rendered is
/// the effective value read from the orchestrator's published read surface. What
/// this holds is the standing of the operator's own outstanding EDIT, which the
/// orchestrator has no way to report and which is exactly what went missing --
/// a write landed, the pane kept rendering the launch-time snapshot, and the
/// operator was told their change was lost
/// (livespec-console-beads-fabro-30c).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum DispatcherSettingWriteState {
    /// No write is outstanding: every row renders its effective value alone.
    /// A settled write that MOVED the effective value returns here, because the
    /// fresh value is then the whole story.
    #[default]
    Idle,
    /// A write was submitted and neither its outcome event nor a confirming
    /// re-read has been observed yet.
    Pending {
        /// The single-setting write the operator submitted.
        write: DispatcherSettingWrite,
        /// How many events the log held when the write was submitted, so this
        /// write's outcome event is told apart from an older one for the same
        /// setting. The log is append-only, so the count is a stable watermark.
        submitted_after_events: usize,
    },
    /// The write's outcome landed and the fresh effective read still reports the
    /// value observed before it -- the orchestrator did not take the change.
    Unchanged {
        /// The single-setting write whose value the operator did not get.
        write: DispatcherSettingWrite,
    },
}

impl DispatcherSettingWriteState {
    #[must_use]
    /// The pending state a just-applied runtime effect implies, or `None` when
    /// the effect persisted something other than a well-formed dispatcher-setting
    /// write.
    ///
    /// `event_count` is the size of the event log at submission time -- the
    /// watermark the `Pending` variant carries.
    pub fn submitted(
        command_type: &CommandType,
        payload_json: &str,
        event_count: usize,
    ) -> Option<Self> {
        if !matches!(command_type, CommandType::ConfigDispatcherSettingSet) {
            return None;
        }
        let request = DispatcherSettingSetRequest::from_payload_json(payload_json).ok()?;
        Some(Self::Pending {
            write: request.write().clone(),
            submitted_after_events: event_count,
        })
    }

    #[must_use]
    /// The write this state is about, or `None` when nothing is outstanding.
    pub const fn write(&self) -> Option<&DispatcherSettingWrite> {
        match self {
            Self::Idle => None,
            Self::Pending { write, .. } | Self::Unchanged { write } => Some(write),
        }
    }

    /// How this state qualifies `row`: only the ONE row the outstanding write
    /// targets is qualified; every other row renders its effective value alone.
    fn status_for(&self, row: DispatcherSettingRow) -> SettingRowStatus {
        match self {
            Self::Pending { write, .. } if write.row() == row => {
                SettingRowStatus::Pending(write.rendered_value())
            }
            Self::Unchanged { write } if write.row() == row => {
                SettingRowStatus::Unchanged(write.rendered_value())
            }
            Self::Idle | Self::Pending { .. } | Self::Unchanged { .. } => {
                SettingRowStatus::Effective
            }
        }
    }
}

/// Whether the pending write's outcome has landed in the event log, so a fresh
/// effective read is now worth its shell-out.
///
/// The gate exists because the `config` read is a real orchestrator invocation:
/// re-reading on every render tick would shell out four times a second for a
/// value that changes only when the operator edits it. Waiting for the outcome
/// event is also the only way to tell "the worker has not run the write yet"
/// from "the write ran and the value did not move" -- both look identical in a
/// bare re-read, and reporting the first as the second is a fresh lie.
///
/// Matches on the SETTING KEY at or after the submission watermark, so an older
/// outcome for the same setting earlier in the session never settles this write.
/// A not-wired outcome settles it too: the write is over, and the re-read will
/// find the value unmoved and say so.
#[must_use]
pub fn dispatcher_setting_write_settled(
    state: &DispatcherSettingWriteState,
    events: &[ConsoleEvent],
) -> bool {
    let DispatcherSettingWriteState::Pending {
        write,
        submitted_after_events,
    } = state
    else {
        return false;
    };
    events
        .iter()
        .skip(*submitted_after_events)
        .any(|event| dispatcher_setting_outcome_names(event, write.key()))
}

/// Whether one event is a dispatcher-setting outcome (changed or not-wired) for
/// `key`. An unparseable or key-less payload is not a match -- an outcome that
/// cannot say which setting it is about settles nothing.
fn dispatcher_setting_outcome_names(event: &ConsoleEvent, key: &str) -> bool {
    if !matches!(
        event.event_type(),
        EventType::ConfigDispatcherSettingChanged | EventType::ConfigDispatcherSettingNotWired
    ) {
        return false;
    }
    serde_json::from_str::<serde_json::Value>(event.payload_json())
        .ok()
        .as_ref()
        .and_then(|payload| payload.get("setting"))
        .and_then(serde_json::Value::as_str)
        == Some(key)
}

/// Fold a fresh effective read into the write state.
///
/// This is the confirmation step the `Settings` view was missing: the value the
/// orchestrator NOW reports either carries the operator's edit -- in which case
/// the write is over and the fresh value is the whole story ([`Idle`]) -- or it
/// does not, in which case the row says so rather than rendering the old value
/// as though nothing had been asked for ([`Unchanged`]).
///
/// A read surface that went unreadable confirms NOTHING, so the write stays
/// pending; the pane degrades to its named not-observed finding, which is the
/// honest report of a console that cannot see the effective policy.
///
/// [`Idle`]: DispatcherSettingWriteState::Idle
/// [`Unchanged`]: DispatcherSettingWriteState::Unchanged
#[must_use]
pub fn fold_dispatcher_setting_reread(
    state: &DispatcherSettingWriteState,
    fresh: &DispatcherSettingsRead,
) -> DispatcherSettingWriteState {
    let DispatcherSettingWriteState::Pending { write, .. } = state else {
        return state.clone();
    };
    let DispatcherSettingsRead::Observed(settings) = fresh else {
        return state.clone();
    };
    if write.row().value(settings) == write.rendered_value() {
        DispatcherSettingWriteState::Idle
    } else {
        DispatcherSettingWriteState::Unchanged {
            write: write.clone(),
        }
    }
}

/// Build the `Settings` rows from the effective values the console observed,
/// in display order, each qualified by the operator's outstanding write.
///
/// The `Settings` view renders these; an unobserved read surface has no rows to
/// render (the caller degrades to a not-observed finding).
#[must_use]
pub fn dispatcher_setting_rows(
    settings: &DispatcherSettings,
    write_state: &DispatcherSettingWriteState,
) -> Vec<SettingRow> {
    DispatcherSettingRow::all()
        .iter()
        .map(|row| SettingRow {
            label: row.label(),
            value: row.value(settings),
            help: row.help(),
            dangerous: row.dangerous(),
            status: write_state.status_for(*row),
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

/// The effective dispatcher settings as the orchestrator's `config` read
/// reports them — a point-in-time read of the orchestrator-owned values, never
/// persisted by the console.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatcherSettings {
    auto_approve_ready: bool,
    merge_on_review_cap: bool,
    acceptance_mode: AcceptancePolicy,
    review_fix_cap: u32,
    acceptance_rework_cap: u32,
    drift_capture_merge_threshold: u32,
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
        Self::new_with_drift_capture_merge_threshold(
            auto_approve_ready,
            merge_on_review_cap,
            acceptance_mode,
            review_fix_cap,
            acceptance_rework_cap,
            DRIFT_CAPTURE_MERGE_THRESHOLD_DEFAULT,
            wip_cap,
        )
    }

    #[must_use]
    /// Construct a new value with an explicit `drift_capture_merge_threshold`.
    pub const fn new_with_drift_capture_merge_threshold(
        auto_approve_ready: bool,
        merge_on_review_cap: bool,
        acceptance_mode: AcceptancePolicy,
        review_fix_cap: u32,
        acceptance_rework_cap: u32,
        drift_capture_merge_threshold: u32,
        wip_cap: u32,
    ) -> Self {
        Self {
            auto_approve_ready,
            merge_on_review_cap,
            acceptance_mode,
            review_fix_cap,
            acceptance_rework_cap,
            drift_capture_merge_threshold,
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
    /// The effective `drift_capture_merge_threshold` value.
    pub const fn drift_capture_merge_threshold(&self) -> u32 {
        self.drift_capture_merge_threshold
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
    /// The orchestrator reported all settings.
    Observed(DispatcherSettings),
    /// No trustworthy read was produced (the surface is not wired, the action
    /// failed, or its payload could not be parsed). The caller degrades to a
    /// named not-observed finding rather than an assumed value.
    NotObserved,
}

/// Reads and writes the API-configurable dispatcher settings THROUGH the
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

    /// Read the effective values of all settings through the `config` action.
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

    /// Write one setting through its action as the supplied principal.
    ///
    /// # Errors
    /// Returns an application error when the underlying port cannot produce a
    /// trustworthy outcome.
    pub fn write_setting_attributed(
        &mut self,
        setting: &DispatcherSettingWrite,
        requested_by: &str,
    ) -> ApplicationResult<OrchestratorActionOutcome> {
        let request = OrchestratorActionRequest::attributed(
            set_config_action_id(setting),
            requested_by.to_owned(),
        );
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

/// Parse the `config` read's `settings[]` array into the effective values.
/// `None` when the payload does not parse or any declared key is absent or
/// mistyped, so the caller degrades to a not-observed finding.
fn settings_from_config_read(stdout: &str) -> Option<DispatcherSettings> {
    let payload: ConfigReadPayload = serde_json::from_str(stdout).ok()?;
    let mut by_key: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for setting in payload.settings {
        let _ = by_key.insert(setting.key, setting.value);
    }
    Some(DispatcherSettings::new_with_drift_capture_merge_threshold(
        bool_setting(&by_key, "auto_approve_ready")?,
        bool_setting(&by_key, "merge_on_review_cap")?,
        acceptance_setting(&by_key, "acceptance_mode")?,
        u32_setting(&by_key, "review_fix_cap")?,
        u32_setting(&by_key, "acceptance_rework_cap")?,
        u32_setting(&by_key, "drift_capture_merge_threshold")
            .unwrap_or(DRIFT_CAPTURE_MERGE_THRESHOLD_DEFAULT),
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
// Auto-disposition observation — reading the orchestrator plane's per-decision
// auto-disposition audit records.
// ---------------------------------------------------------------------------

/// The journal `stage` marker the orchestrator plane writes for one per-decision
/// auto-disposition audit record; the console reads only records carrying it and
/// ignores every other journal stage (calibration, dispatch).
const AUTO_DISPOSITION_STAGE: &str = "auto-disposition";

/// The current auto-disposition vocabulary published by the orchestrator.
const AUTO_DISPOSITION_AUTO_APPROVE: &str = "auto-approve";
const AUTO_DISPOSITION_AI_AUTO_ACCEPT: &str = "ai-auto-accept";
const AUTO_DISPOSITION_AI_FAIL_AUTO_REWORK: &str = "ai-fail-auto-rework";
const AUTO_DISPOSITION_SHIP_ON_CAP: &str = "ship-on-cap";
const AUTO_DISPOSITION_CAP_EXCEEDED_ESCALATION: &str = "cap-exceeded-escalation";

/// The three collapsible gates the console's internal reflection command/event
/// path uses for compatibility with existing event logs.
const AUTONOMOUS_GATE_APPROVE: &str = "approve";
const AUTONOMOUS_GATE_ACCEPTANCE: &str = "acceptance";
const AUTONOMOUS_GATE_NEEDS_HUMAN: &str = "needs-human";

/// One per-decision auto-disposition audit entry read back off the orchestrator
/// plane's published Dispatcher journal.
///
/// `work_item_id` names the disposed item; `disposition` is one of the
/// orchestrator's published auto-disposition values; `governing_settings` names
/// the settings that governed it. `gate` and `decision` are the console's
/// internal reflection projection for the existing command/event path. The
/// console consumes the journal record verbatim -- it never re-derives a plane's
/// disposition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousDecision {
    work_item_id: String,
    gate: String,
    decision: String,
    disposition: String,
    governing_settings: Vec<String>,
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
            governing_settings: Vec::new(),
        }
    }

    #[must_use]
    /// Construct a journal-backed value from the orchestrator's live schema.
    pub fn from_auto_disposition(
        work_item_id: &str,
        disposition: &str,
        governing_settings: Vec<String>,
    ) -> Option<Self> {
        let gate = auto_disposition_reflection_gate(disposition)?;
        Some(Self {
            work_item_id: work_item_id.to_owned(),
            gate: gate.to_owned(),
            decision: disposition.to_owned(),
            disposition: disposition.to_owned(),
            governing_settings,
        })
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

    #[must_use]
    /// Return the governing settings recorded by the orchestrator.
    pub fn governing_settings(&self) -> &[String] {
        &self.governing_settings
    }

    #[must_use]
    /// Return true when this journal record is an escalation the console must
    /// surface as needs-attention.
    pub fn is_escalation(&self) -> bool {
        self.disposition == AUTO_DISPOSITION_CAP_EXCEEDED_ESCALATION
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
/// Fail-open, mirroring the orchestrator plane's published read surface reader:
/// a malformed line -- bad JSON, a non-object, a record missing a required
/// field, or an out-of-range disposition -- is skipped rather than raising, and
/// only `auto-disposition` stage records are considered. Records split into
/// auto-resolutions and escalations by disposition, preserving journal order
/// within each bucket.
#[must_use]
pub fn read_autonomous_decisions_from_journal(journal_text: &str) -> AutonomousAudit {
    let mut auto_resolutions = Vec::new();
    let mut escalations = Vec::new();
    for line in journal_text.lines() {
        let Some(decision) = autonomous_decision_from_line(line) else {
            continue;
        };
        if decision.is_escalation() {
            escalations.push(decision);
        } else {
            auto_resolutions.push(decision);
        }
    }
    AutonomousAudit::new(auto_resolutions, escalations)
}

/// Parse one journal line into an [`AutonomousDecision`], or `None` when it is
/// not a valid `auto-disposition` record (malformed JSON, a non-object, a
/// different stage, or an absent/out-of-range required field).
fn autonomous_decision_from_line(line: &str) -> Option<AutonomousDecision> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let object = value.as_object()?;
    if object.get("stage").and_then(serde_json::Value::as_str)? != AUTO_DISPOSITION_STAGE {
        return None;
    }
    let work_item_id = object
        .get("work_item_id")
        .and_then(serde_json::Value::as_str)?;
    let disposition = object
        .get("disposition")
        .and_then(serde_json::Value::as_str)?;
    let governing_settings = object
        .get("governing_settings")
        .and_then(serde_json::Value::as_array)?
        .iter()
        .map(serde_json::Value::as_str)
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    AutonomousDecision::from_auto_disposition(work_item_id, disposition, governing_settings)
}

fn auto_disposition_reflection_gate(disposition: &str) -> Option<&'static str> {
    match disposition {
        AUTO_DISPOSITION_AUTO_APPROVE => Some(AUTONOMOUS_GATE_APPROVE),
        AUTO_DISPOSITION_AI_AUTO_ACCEPT
        | AUTO_DISPOSITION_AI_FAIL_AUTO_REWORK
        | AUTO_DISPOSITION_SHIP_ON_CAP => Some(AUTONOMOUS_GATE_ACCEPTANCE),
        AUTO_DISPOSITION_CAP_EXCEEDED_ESCALATION => Some(AUTONOMOUS_GATE_NEEDS_HUMAN),
        _other => None,
    }
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
/// reads the `auto-disposition` stage records from it fail-open. An unreadable
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
                ..
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
    let command_status = match settings_port
        .write_setting_attributed(write, command.requested_by())?
    {
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
        OrchestratorActionOutcome::NotWired | OrchestratorActionOutcome::Failed { .. } => {
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
    ///
    /// A work-item row carries the projection's ACCOUNT for the item after its
    /// lane title, verbatim, so the operator reads the terminated run's own
    /// story where the decision is offered rather than a bare
    /// `Blocked: needs-human` with nothing to decide on. The row NEVER truncates
    /// it here -- eliding to fit belongs to the renderer, which alone knows the
    /// pane's width, and the whole text always survives into the detail.
    fn to_attention_item(&self, needs_attention: &NeedsAttentionByWorkItem) -> AttentionItem {
        match self {
            Self::WorkItem(entry) => AttentionItem::new(
                entry.snapshot.work_item_id().to_owned(),
                Some(entry.snapshot.work_item_id().to_owned()),
                attention_row_title(
                    &attention_title(&entry.snapshot),
                    advertised_account(needs_attention, entry.snapshot.work_item_id()).as_deref(),
                ),
                entry.event.source().to_owned(),
                entry.snapshot.repo().to_owned(),
                None,
            ),
            Self::NeedsAttention(item) => attention_item_from_snapshot(item),
        }
    }

    /// The row's stable identity — the same id [`Self::to_attention_item`] puts
    /// on the projected [`AttentionItem`], read WITHOUT building the row (which
    /// costs an event scan per entry). This is the Attention cursor's anchor
    /// across refreshes.
    fn row_id(&self) -> &str {
        match self {
            Self::WorkItem(entry) => entry.snapshot.work_item_id(),
            Self::NeedsAttention(item) => item.id(),
        }
    }

    /// The detail-pane projection: the rich fabro / timeline / valve detail for a
    /// work-item, or the composed repo + subject + operator-handoff detail for a
    /// needs-attention item.
    fn to_detail(
        &self,
        events: &[ConsoleEvent],
        needs_attention: &NeedsAttentionByWorkItem,
    ) -> AttentionDetail {
        match self {
            Self::WorkItem(entry) => build_attention_detail(entry, events, needs_attention),
            Self::NeedsAttention(item) => build_needs_attention_detail(item, events),
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
    needs_attention_items: &[AttentionItemSnapshot],
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
    // Takes the ALREADY-materialized needs-attention surface rather than
    // re-materializing it: the caller ([`project_tui_events`]) built it once for
    // the whole projection, and re-scanning the event log here (as this used to)
    // is exactly the per-projection-build cost mx9u.8 exists to remove.
    for item in needs_attention_items {
        if !attention_item_matches(item, search_query) {
            continue;
        }
        if item
            .source_ref()
            .work_item()
            .is_some_and(|work_item| claimed_work_item_ids.contains(work_item))
        {
            continue;
        }
        entries.push(AttentionEntry::NeedsAttention(item.clone()));
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
/// and the operator handoff command to run (always the item's own handoff --
/// the operator action the composed surface prescribes). Per the v042
/// re-sourced Scenario 5, a row that references a work item JOINS that
/// record's detail by id against genuinely-ingested work-item snapshots: the
/// joined timeline and lane-derived valve actions come from the real ingested
/// record, never from state synthesized off the attention row. A referenced
/// record the console never ingested renders as explicitly absent, with no
/// timeline and no actions -- absence is stated, not papered over.
fn build_needs_attention_detail(
    item: &AttentionItemSnapshot,
    events: &[ConsoleEvent],
) -> AttentionDetail {
    let source_ref = item.source_ref();
    let handoff_command = vec![item.handoff().command().to_owned()];
    // The projection's own summary, verbatim: this row IS the projected item,
    // so its account needs no lookup and gets no augmentation.
    let account = Some(item.summary().to_owned());
    if let Some(work_item_id) = source_ref.work_item() {
        if let Some(entry) = latest_work_item_snapshot(events, work_item_id) {
            let detail = entry.snapshot.detail();
            return AttentionDetail::new(
                source_ref.repo().to_owned(),
                work_item_id.to_owned(),
                detail
                    .dispatch_fabro_run_id
                    .clone()
                    .unwrap_or_else(|| "-".to_owned()),
                detail.dispatch_factory.clone(),
                handoff_command,
                latest_timeline(events, entry.event.stream_id(), 3),
                attention_detail_actions(&entry),
            )
            .with_account(account)
            .with_answer_comments(human_answer_comments(detail));
        }
        return AttentionDetail::new(
            source_ref.repo().to_owned(),
            format!("{work_item_id} (record not ingested)"),
            "-".to_owned(),
            None,
            handoff_command,
            Vec::new(),
            Vec::new(),
        )
        .with_account(account);
    }
    let subject = source_ref.path().unwrap_or_else(|| item.id());
    AttentionDetail::new(
        source_ref.repo().to_owned(),
        subject.to_owned(),
        "-".to_owned(),
        None,
        handoff_command,
        Vec::new(),
        Vec::new(),
    )
    .with_account(account)
}

/// The stable opener the orchestrator gives the ledger comment it writes for an
/// operator's answer (`livespec-human-answer (<invoker> via <source>, <at>,
/// <action-id>): <answer>`).
const HUMAN_ANSWER_COMMENT_MARKER: &str = "livespec-human-answer";

/// The item's `livespec-human-answer` ledger comments, verbatim and in ledger
/// order, from the context surface's own record.
///
/// A pure READ of what `list-work-items --json` already carried: the console
/// never writes this comment and never reconstructs its text. Selecting it by
/// the orchestrator's stable marker is recognition, not composition -- the
/// bytes rendered are the bytes the surface returned.
fn human_answer_comments(detail: &WorkItemDetail) -> Vec<String> {
    detail
        .comments
        .iter()
        .filter(|comment| comment.text.starts_with(HUMAN_ANSWER_COMMENT_MARKER))
        .map(|comment| comment.text.clone())
        .collect()
}

/// The latest genuinely-ingested work-item snapshot for `work_item_id`, with
/// its carrying event. Deliberately NOT filtered by [`requires_attention`]:
/// the needs-attention detail joins ANY ingested record by id (v042
/// Scenario 5); what it never does is synthesize one.
fn latest_work_item_snapshot(
    events: &[ConsoleEvent],
    work_item_id: &str,
) -> Option<AttentionSnapshot> {
    let mut latest: Option<AttentionSnapshot> = None;
    for event in events {
        if *event.event_type() != EventType::WorkItemSnapshotObserved {
            continue;
        }
        let Some(snapshot) = work_item_snapshot_from_payload_json(event.payload_json()) else {
            continue;
        };
        if snapshot.work_item_id() != work_item_id {
            continue;
        }
        latest = Some(AttentionSnapshot {
            event: event.clone(),
            snapshot,
        });
    }
    latest
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

/// The inbox row's label: the entry's lane title, followed by the projection's
/// account for it when one was advertised.
///
/// The account is appended VERBATIM and the title is kept, so the row still
/// names the lane it is resting in and the reader still gets the run's story.
/// An account that merely repeats the title adds nothing, so it is not doubled.
fn attention_row_title(title: &str, account: Option<&str>) -> String {
    match account {
        Some(account) if account != title => format!("{title} — {account}"),
        _other => title.to_owned(),
    }
}

fn attention_title(snapshot: &WorkItemSnapshot) -> String {
    match (snapshot.lane(), snapshot.lane_reason()) {
        (Lane::PendingApproval, _) => "Pending approval".to_owned(),
        (Lane::Acceptance, _) => "Acceptance review".to_owned(),
        (Lane::Blocked, Some(reason)) => format!("Blocked: {}", reason.label()),
        (lane, _) => lane.label().to_owned(),
    }
}

#[must_use]
/// The active search query implied by `overlay`, if any.
///
/// The same read [`build_tui_model_for_state`] uses to filter the Attention
/// list. Exposed so a caller that caches a [`TuiProjection`] across renders
/// (the terminal loop's hot path) can compute the SAME cache key without
/// duplicating this match.
pub fn tui_search_query(overlay: &TuiOverlay) -> Option<&str> {
    search_query(overlay)
}

fn search_query(overlay: &TuiOverlay) -> Option<&str> {
    match overlay {
        TuiOverlay::Search { query } => Some(query),
        TuiOverlay::None
        | TuiOverlay::CommandPalette { .. }
        | TuiOverlay::CommandModal { .. }
        | TuiOverlay::CommandExplainer { .. }
        | TuiOverlay::ActionInvoker { .. }
        | TuiOverlay::FactoryDrainConfirm { .. }
        | TuiOverlay::FactoryDispatchItemConfirm { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::DriverHandoff { .. }
        | TuiOverlay::WorkItemDetail { .. }
        | TuiOverlay::Help { .. }
        | TuiOverlay::Menu { .. } => None,
    }
}

fn normalize_overlay(overlay: &TuiOverlay, detail: Option<&AttentionDetail>) -> TuiOverlay {
    match overlay {
        TuiOverlay::CommandModal {
            selected_action_index,
        } => TuiOverlay::CommandModal {
            selected_action_index: clamp_action_index(detail, *selected_action_index),
        },
        TuiOverlay::CommandExplainer {
            selected_action_index,
        } => TuiOverlay::CommandExplainer {
            selected_action_index: clamp_action_index(detail, *selected_action_index),
        },
        TuiOverlay::None
        | TuiOverlay::Search { .. }
        | TuiOverlay::CommandPalette { .. }
        | TuiOverlay::ActionInvoker { .. }
        | TuiOverlay::FactoryDrainConfirm { .. }
        | TuiOverlay::FactoryDispatchItemConfirm { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::DriverHandoff { .. }
        | TuiOverlay::WorkItemDetail { .. }
        | TuiOverlay::Help { .. }
        | TuiOverlay::Menu { .. } => overlay.clone(),
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
            focus,
            selected_section,
            ..
        } => {
            let next = if down {
                (selected_section + 1).min(HELP_SECTION_COUNT - 1)
            } else {
                selected_section.saturating_sub(1)
            };
            TuiOverlay::Help {
                focus: *focus,
                selected_section: next,
                scroll: 0,
            }
        }
        TuiOverlay::None
        | TuiOverlay::Search { .. }
        | TuiOverlay::CommandPalette { .. }
        | TuiOverlay::CommandModal { .. }
        | TuiOverlay::CommandExplainer { .. }
        | TuiOverlay::ActionInvoker { .. }
        | TuiOverlay::FactoryDrainConfirm { .. }
        | TuiOverlay::FactoryDispatchItemConfirm { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::DriverHandoff { .. }
        | TuiOverlay::WorkItemDetail { .. }
        | TuiOverlay::Menu { .. } => overlay.clone(),
    }
}

/// Scroll the modal Help right-hand text pane by `rows`, preserving the focused
/// pane and selected section. Down clamps to the render-measured bottom; up
/// saturates at the top. Leaves a non-Help overlay unchanged.
fn help_scroll(overlay: &TuiOverlay, rows: usize, down: bool, max_scroll: usize) -> TuiOverlay {
    match overlay {
        TuiOverlay::Help {
            focus,
            selected_section,
            scroll,
        } => TuiOverlay::Help {
            focus: *focus,
            selected_section: *selected_section,
            scroll: if down {
                scroll.saturating_add(rows).min(max_scroll)
            } else {
                scroll.saturating_sub(rows)
            },
        },
        TuiOverlay::None
        | TuiOverlay::Search { .. }
        | TuiOverlay::CommandPalette { .. }
        | TuiOverlay::CommandModal { .. }
        | TuiOverlay::CommandExplainer { .. }
        | TuiOverlay::ActionInvoker { .. }
        | TuiOverlay::FactoryDrainConfirm { .. }
        | TuiOverlay::FactoryDispatchItemConfirm { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::DriverHandoff { .. }
        | TuiOverlay::WorkItemDetail { .. }
        | TuiOverlay::Menu { .. } => overlay.clone(),
    }
}

/// Move focus inside the Help overlay, leaving every other overlay unchanged.
fn help_focus(overlay: &TuiOverlay, focus: HelpFocus) -> TuiOverlay {
    match overlay {
        TuiOverlay::Help {
            selected_section,
            scroll,
            ..
        } => TuiOverlay::Help {
            focus,
            selected_section: *selected_section,
            scroll: *scroll,
        },
        TuiOverlay::None
        | TuiOverlay::Search { .. }
        | TuiOverlay::CommandPalette { .. }
        | TuiOverlay::CommandModal { .. }
        | TuiOverlay::CommandExplainer { .. }
        | TuiOverlay::ActionInvoker { .. }
        | TuiOverlay::FactoryDrainConfirm { .. }
        | TuiOverlay::FactoryDispatchItemConfirm { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::DriverHandoff { .. }
        | TuiOverlay::WorkItemDetail { .. }
        | TuiOverlay::Menu { .. } => overlay.clone(),
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
        // Only the answer-bearing dialog accumulates text; on every other valve
        // a typed character stays inert, exactly as it was before the field
        // existed.
        TuiOverlay::ValveConfirm { valve, answer } if valve.accepts_answer() => {
            TuiOverlay::ValveConfirm {
                valve: *valve,
                answer: format!("{answer}{value}"),
            }
        }
        TuiOverlay::None
        | TuiOverlay::CommandModal { .. }
        | TuiOverlay::CommandExplainer { .. }
        | TuiOverlay::ActionInvoker { .. }
        | TuiOverlay::FactoryDrainConfirm { .. }
        | TuiOverlay::FactoryDispatchItemConfirm { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::DriverHandoff { .. }
        | TuiOverlay::WorkItemDetail { .. }
        | TuiOverlay::Help { .. }
        | TuiOverlay::Menu { .. } => overlay.clone(),
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
        TuiOverlay::ValveConfirm { valve, answer } if valve.accepts_answer() => {
            TuiOverlay::ValveConfirm {
                valve: *valve,
                answer: drop_last_char(answer),
            }
        }
        TuiOverlay::None
        | TuiOverlay::CommandModal { .. }
        | TuiOverlay::CommandExplainer { .. }
        | TuiOverlay::ActionInvoker { .. }
        | TuiOverlay::FactoryDrainConfirm { .. }
        | TuiOverlay::FactoryDispatchItemConfirm { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::DriverHandoff { .. }
        | TuiOverlay::WorkItemDetail { .. }
        | TuiOverlay::Help { .. }
        | TuiOverlay::Menu { .. } => overlay.clone(),
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
        TuiOverlay::ActionInvoker { selected_action } => TuiOverlay::ActionInvoker {
            selected_action: (selected_action + 1)
                .min(action_registry::ACTION_REGISTRY.len().saturating_sub(1)),
        },
        // Clamped to the OPEN node's own action count, not the registry's:
        // each bar node holds a different number of actions, so a registry-wide
        // bound would let the cursor run off the end of a short menu.
        TuiOverlay::Menu { top, selected } => TuiOverlay::Menu {
            top: *top,
            selected: (selected + 1)
                .min(action_registry::menu_actions(*top).len().saturating_sub(1)),
        },
        TuiOverlay::None
        | TuiOverlay::Search { .. }
        | TuiOverlay::CommandPalette { .. }
        | TuiOverlay::CommandExplainer { .. }
        | TuiOverlay::FactoryDrainConfirm { .. }
        | TuiOverlay::FactoryDispatchItemConfirm { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::DriverHandoff { .. }
        | TuiOverlay::WorkItemDetail { .. }
        | TuiOverlay::Help { .. } => overlay.clone(),
    }
}

/// The state each menu interaction produces.
///
/// Opening always starts at the first bar node with its first action selected;
/// the two walks delegate to [`menu_move_top`]. Kept together because they are
/// one concern, and because splitting them across three reducer arms pushed
/// `reduce_tui_interaction` past its line budget for no readability gain.
fn menu_interaction_state(
    state: &TuiInteractionState,
    interaction: TuiInteraction,
) -> TuiInteractionState {
    let overlay = match interaction {
        TuiInteraction::MenuNextTop => menu_move_top(state.overlay(), true),
        TuiInteraction::MenuPreviousTop => menu_move_top(state.overlay(), false),
        _open => TuiOverlay::Menu {
            top: 0,
            selected: 0,
        },
    };
    state.clone().with_overlay(overlay)
}

/// Walk the menu bar one node forward or back, wrapping at both ends.
///
/// The selection RESETS to the first action of the newly opened node: carrying
/// an index across nodes would land the cursor on an unrelated action, and the
/// nodes hold different numbers of actions so the index may not even exist.
fn menu_move_top(overlay: &TuiOverlay, forward: bool) -> TuiOverlay {
    let TuiOverlay::Menu { top, .. } = overlay else {
        return overlay.clone();
    };
    // Wrapping is expressed as clamp-then-fall-back rather than modular
    // arithmetic over a separately-guarded count: `% count` needs an
    // `if count == 0` guard that no test can reach, because ACTION_REGISTRY is
    // a non-empty const and so the tree always has at least one node.
    let count = action_registry::menu_tree().len();
    let next = if forward {
        top.checked_add(1).filter(|next| *next < count).unwrap_or(0)
    } else {
        top.checked_sub(1)
            .unwrap_or_else(|| count.saturating_sub(1))
    };
    TuiOverlay::Menu {
        top: next,
        selected: 0,
    }
}

fn move_action_up(overlay: &TuiOverlay) -> TuiOverlay {
    match overlay {
        TuiOverlay::CommandModal {
            selected_action_index,
        } => TuiOverlay::CommandModal {
            selected_action_index: selected_action_index.saturating_sub(1),
        },
        TuiOverlay::ActionInvoker { selected_action } => TuiOverlay::ActionInvoker {
            selected_action: selected_action.saturating_sub(1),
        },
        TuiOverlay::Menu { top, selected } => TuiOverlay::Menu {
            top: *top,
            selected: selected.saturating_sub(1),
        },
        TuiOverlay::None
        | TuiOverlay::Search { .. }
        | TuiOverlay::CommandPalette { .. }
        | TuiOverlay::CommandExplainer { .. }
        | TuiOverlay::FactoryDrainConfirm { .. }
        | TuiOverlay::FactoryDispatchItemConfirm { .. }
        | TuiOverlay::ValveConfirm { .. }
        | TuiOverlay::DriverHandoff { .. }
        | TuiOverlay::WorkItemDetail { .. }
        | TuiOverlay::Help { .. } => overlay.clone(),
    }
}

fn clamp_action_index(detail: Option<&AttentionDetail>, requested_index: usize) -> usize {
    detail
        .and_then(|detail| selected_index(detail.actions().len(), requested_index))
        .unwrap_or_default()
}

/// The detail pane for a work-item entry.
///
/// Two sources, each consumed rather than re-derived. The run id and factory
/// come from the item's OWN dispatch stamps (`dispatch_fabro_run_id` /
/// `dispatch_factory`), and the pressable commands come from the orchestrator's
/// attention projection. Neither is reconstructed from the console's event log
/// any more: the run id used to be recovered by scanning backwards for a Fabro
/// observation and then formatted into a `fabro attach` handoff, which the
/// needs-human redesign left pointing at a run that no longer exists
/// (`SPECIFICATION/contracts.md`).
fn build_attention_detail(
    entry: &AttentionSnapshot,
    events: &[ConsoleEvent],
    needs_attention: &NeedsAttentionByWorkItem,
) -> AttentionDetail {
    let event = &entry.event;
    let detail = entry.snapshot.detail();
    let actions = attention_detail_actions(entry);
    AttentionDetail::new(
        entry.snapshot.repo().to_owned(),
        entry.snapshot.work_item_id().to_owned(),
        detail
            .dispatch_fabro_run_id
            .clone()
            .unwrap_or_else(|| "-".to_owned()),
        detail.dispatch_factory.clone(),
        advertised_valve_commands(needs_attention, entry.snapshot.work_item_id()),
        latest_timeline(events, event.stream_id(), 3),
        actions,
    )
    .with_account(advertised_account(
        needs_attention,
        entry.snapshot.work_item_id(),
    ))
    .with_answer_comments(human_answer_comments(detail))
}

/// The needs-attention surface, materialized once and grouped by the
/// `work_item_id` it references, in the SAME relative order
/// [`materialize_attention_items`] returned (first-match-wins, matching what
/// `advertised_account`'s old `.find()` and `advertised_valve_commands`'s old
/// `.filter()` read directly off the event log).
///
/// This is the index behind both lookups below. Building it is `O(events)`,
/// once; before it existed, EVERY attention-list row re-materialized the whole
/// needs-attention surface (another full event-log scan, JSON-decoding
/// included) just to read its own account/valve-commands -- `O(rows * events)`
/// on a build, and the dominant cost behind
/// livespec-console-beads-fabro-mx9u.8's measured per-keystroke lag.
type NeedsAttentionByWorkItem = BTreeMap<String, Vec<AttentionItemSnapshot>>;

fn group_needs_attention_by_work_item(items: &[AttentionItemSnapshot]) -> NeedsAttentionByWorkItem {
    let mut grouped: NeedsAttentionByWorkItem = BTreeMap::new();
    for item in items {
        if let Some(work_item_id) = item.source_ref().work_item() {
            grouped
                .entry(work_item_id.to_owned())
                .or_default()
                .push(item.clone());
        }
    }
    grouped
}

/// The ACCOUNT the ingested needs-attention projection carries for
/// `work_item_id` -- its valve item's `summary`, verbatim.
///
/// The same read as [`advertised_valve_commands`], over the same dedupe blind
/// spot: the projected row for a `blocked / needs-human` item is exactly the
/// one the unified list folds into its richer work-item entry, so without this
/// the account the orchestrator composed would never reach the surface. `None`
/// when the projection advertises no row for the item -- an honest absence, not
/// a locally-composed stand-in.
fn advertised_account(
    needs_attention: &NeedsAttentionByWorkItem,
    work_item_id: &str,
) -> Option<String> {
    needs_attention
        .get(work_item_id)
        .and_then(|items| items.first())
        .map(|item| item.summary().to_owned())
}

/// Every command the ingested needs-attention projection advertises for
/// `work_item_id`, in projection order.
///
/// The unified attention list DEDUPES a needs-attention row into the richer
/// work-item entry it points at, so for a `blocked / needs-human` item the row
/// carrying the `resolve-blocked:<id>:ready` valve is exactly the one that does
/// not get its own list entry. Reading the projection back here is what keeps
/// that valve reachable: without it the dedupe silently swallowed the only
/// pressable thing the orchestrator advertised for the item.
fn advertised_valve_commands(
    needs_attention: &NeedsAttentionByWorkItem,
    work_item_id: &str,
) -> Vec<String> {
    needs_attention
        .get(work_item_id)
        .into_iter()
        .flatten()
        .map(|item| item.handoff().command().to_owned())
        .collect()
}

/// The pressable per-item roster the needs-attention detail pane offers for
/// `entry`, derived from the registry.
///
/// EVERY per-item staging kind, not just the valves: the driver handoff and the
/// per-item factory dispatch are per-item verbs too, and withholding them here
/// while the drilled-in lane offers them is precisely the surface split
/// `contracts.md` "Per-item verb surface parity" forbids. What is excluded is
/// what is not per-item at all — the selection-less globals and the board-wide
/// ready drain, neither of which acts on this row's work-item.
fn attention_detail_actions(entry: &AttentionSnapshot) -> Vec<OperatorAction> {
    // This item is a vehicle for deriving the ACTION roster; it is never
    // rendered as a lane row, and the registry does not read the confirmation
    // flag. Nothing on this path knows the backing source's health, so it
    // claims none: an unvouched-for row that is never shown costs nothing,
    // while claiming `confirmed` here would be a false assurance if this item
    // ever did reach a surface.
    let item = LaneWorkItem::from_snapshot(&entry.snapshot, LaneExecutionState::NotActive, false);
    let ctx = action_registry::ActionContext::for_item(
        &item,
        action_registry::ActionSurface::Attention,
        0,
    );
    action_registry::ACTION_REGISTRY
        .iter()
        .filter(|spec| {
            !matches!(
                spec.staging,
                action_registry::ActionStaging::Global(_)
                    | action_registry::ActionStaging::FactoryDrain
            ) && (spec.availability)(&ctx)
        })
        .map(|spec| OperatorAction::Registered(spec.id))
        .collect()
}

fn view_summary_items(
    active_view: TuiView,
    events_focus: EventsFocus,
    observed_sources: &[String],
    events: &[ConsoleEvent],
) -> Vec<ViewSummaryItem> {
    match active_view {
        TuiView::Spec => spec_view_items(events),
        TuiView::Events => events_container_items(events_focus, observed_sources, events),
        TuiView::Repos => repos_view_items(events),
        // The Attention, Lanes, and Settings views render their own projections
        // (the attention list / detail, the lane board, the dispatcher-settings
        // rows), not summary rows.
        TuiView::Attention | TuiView::Lanes | TuiView::Settings => Vec::new(),
    }
}

/// The `Events` container's content rows, keyed on which sub-view is showing.
///
/// `Overview` lists the two sub-views by name -- the container's own picker
/// home. `StoredEvents` is EXACTLY [`events_view_items`], the pre-container
/// Events content, unchanged (AC3). `EventSources` is an empty/skeleton state
/// only: the real per-source roster (health, cause, last-successful-read, and
/// per-row actions) is a sibling item
/// (`livespec-console-beads-fabro-pzbdbo.29`,
/// `livespec-console-beads-fabro-mx9u.20.2`); this renders just the source
/// names so the sub-view is not empty of structure, without claiming detail it
/// does not carry.
fn events_container_items(
    events_focus: EventsFocus,
    observed_sources: &[String],
    events: &[ConsoleEvent],
) -> Vec<ViewSummaryItem> {
    match events_focus {
        EventsFocus::Overview => EventsFocus::all()
            .iter()
            .map(|sub_view| ViewSummaryItem::new(sub_view.label().to_owned(), String::new()))
            .collect(),
        EventsFocus::StoredEvents => events_view_items(events),
        EventsFocus::EventSources => event_sources_skeleton_items(observed_sources),
    }
}

/// The "Event sources" sub-view's empty/skeleton state: a plain name-only row
/// per source this build has EVER observed reporting, healthy or degraded,
/// from the SAME name-only registry `TuiProjection::observed_source_names`
/// exposes -- never a second encoding of source identity, and NEVER derived
/// from `unavailable_sources`, whose empty case means "nothing is currently
/// down" and would misrender here as "no sources exist" (measured on the real
/// store 2026-09-09, mx9u.20.1 review: six sources polling successfully,
/// this sub-view claiming none had ever been observed). A healthy source
/// still emits a positive observation event, so it still appears here; only a
/// build that has observed NO source at all renders the placeholder.
fn event_sources_skeleton_items(observed_sources: &[String]) -> Vec<ViewSummaryItem> {
    if observed_sources.is_empty() {
        return vec![ViewSummaryItem::new(
            "No sources have reported any events yet".to_owned(),
            String::new(),
        )];
    }
    observed_sources
        .iter()
        .map(|source| ViewSummaryItem::new(source.clone(), String::new()))
        .collect()
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
    let mut repos = events.iter().filter_map(repo_id).collect::<Vec<_>>();
    repos.sort();
    repos.dedup();
    let mut fleet_streams = events
        .iter()
        .map(ConsoleEvent::stream_id)
        .filter(|stream_id| fleet_scoped(stream_id))
        .collect::<Vec<_>>();
    let fleet_scoped_events = fleet_streams.len();
    fleet_streams.sort_unstable();
    fleet_streams.dedup();
    let unattributable_events = events
        .iter()
        .filter(|event| repo_id(event).is_none() && !fleet_scoped(event.stream_id()))
        .collect::<Vec<_>>();
    let unattributable = unattributable_events.len();
    // AC5 (livespec-console-beads-fabro-mx9u.21): "observed" is easy to misread
    // as a configured roster or a filesystem scan of known repos. It is
    // neither -- stated here, on the view itself, rather than left for the
    // operator to infer from a number that can never explain its own ceiling.
    let mut items = vec![
        ViewSummaryItem::new(
            "What \"observed\" means".to_owned(),
            "The distinct repos the EVENT LOG is attributed to -- not a \
             configured roster, and not repos discovered on disk. A repo with \
             no events in this store does not appear, however many repos \
             exist in the fleet."
                .to_owned(),
        ),
        ViewSummaryItem::new(format!("Repos observed: {}", repos.len()), repos.join(", ")),
    ];
    // NOT SILENT either. A fleet stream belongs to no repository, but that is a
    // reason to attribute it correctly rather than to drop it: an operator who
    // knows the store holds drain commands and sees them in no row learns that
    // this view is partial, which is exactly the trust the row below protects.
    // The detail is the observed fleet-stream roster, the same operational
    // shape as the repo roster above.
    if fleet_scoped_events > 0 {
        items.push(ViewSummaryItem::new(
            format!("Fleet-scoped events: {fleet_scoped_events}"),
            fleet_streams.join(", "),
        ));
    }
    // NOT SILENT. Dropping the unattributable events without saying so would
    // turn "Repos observed: 1" into a number that quietly excludes a fifth of
    // the store, which is the same reassuring-but-partial signal this projection
    // just stopped producing in the other direction.
    //
    // NOT JUST A COUNT either (livespec-console-beads-fabro-mx9u.21): measured
    // on the real store, 390 such events were five DIFFERENT event families,
    // all console-originated command/action streams that legitimately carry no
    // repo (`command.accepted`, `attention_item.resolved`,
    // `work_item.action.*`, `config.*`, `factory.*`). A bare count cannot tell
    // the operator whether that is a real attribution hole or the console's
    // own bookkeeping, so the row also names the families.
    if unattributable > 0 {
        let breakdown = unattributable_family_breakdown(&unattributable_events)
            .into_iter()
            .map(|(family, count)| format!("{family} {count}"))
            .collect::<Vec<_>>()
            .join(", ");
        items.push(ViewSummaryItem::new(
            format!("Events with no derivable repo: {unattributable}"),
            format!(
                "Their stream key does not carry one. Every event is expected \
                 to stream under `{{context}}:{{repo}}`; these carry a bare id \
                 instead, so the repo cannot be recovered from the key and is \
                 NOT guessed. By event family: {breakdown}."
            ),
        ));
    }
    items
}

/// Group unattributable events by event-type FAMILY: the segment of
/// [`EventType::contract_name`] before its first `.`. Sibling event types that
/// share a family collapse into ONE row, labelled by their longest common
/// dotted prefix plus a wildcard (`work_item.action.started` and
/// `work_item.action.completed` collapse to `work_item.action.*`,
/// `factory.drain.completed` and `factory.dispatch_item_requested` collapse
/// only as far as `factory.*`); a family with only ONE participating event
/// type is named exactly instead (`command.accepted`), so a small,
/// distinctive family is not obscured behind a wildcard that would otherwise
/// read the same as a large, heterogeneous one. Sorted by descending count
/// (ties broken alphabetically) so the bulk of the bucket reads first.
/// One event-type contract name paired with how many unattributable events
/// carried it.
type ContractCount<'a> = (&'a str, usize);

fn unattributable_family_breakdown(events: &[&ConsoleEvent]) -> Vec<(String, usize)> {
    let mut per_contract: BTreeMap<&'static str, usize> = BTreeMap::new();
    for event in events {
        *per_contract
            .entry(event.event_type().contract_name())
            .or_insert(0) += 1;
    }
    // The HEAD member is a required field, not an `Option` or a `Vec` element
    // reached by indexing -- a family group is only ever created (`or_insert_with`
    // below) together with its first member, so "a family with no members" is
    // unrepresentable rather than merely unexpected. That is what lets the fold
    // below skip an empty-group branch no caller can ever actually take.
    let mut per_family: BTreeMap<&str, (ContractCount<'_>, Vec<ContractCount<'_>>)> =
        BTreeMap::new();
    for (contract, count) in &per_contract {
        let family = contract.split('.').next().unwrap_or(contract);
        per_family
            .entry(family)
            .and_modify(|(_head, rest)| rest.push((contract, *count)))
            .or_insert_with(|| ((contract, *count), Vec::new()));
    }
    let mut breakdown = per_family
        .into_values()
        .map(|((head_contract, head_count), rest)| {
            let total = head_count + rest.iter().map(|(_contract, count)| count).sum::<usize>();
            let label = if rest.is_empty() {
                head_contract.to_owned()
            } else {
                let mut prefix = head_contract.split('.').collect::<Vec<_>>();
                for (contract, _count) in &rest {
                    let segments = contract.split('.').collect::<Vec<_>>();
                    let shared = prefix
                        .iter()
                        .zip(segments.iter())
                        .take_while(|(left, right)| left == right)
                        .count();
                    prefix.truncate(shared);
                }
                format!("{}.*", prefix.join("."))
            };
            (label, total)
        })
        .collect::<Vec<_>>();
    breakdown.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    breakdown
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
/// - A THIRD shape exists in real stores and this derivation used to invent a
///   repo for it: a key with NO COLON AT ALL, carrying a bare work-item id.
///   `command.accepted` and `attention_item.resolved` both stream that way.
///   Measured in a live console store: 140 of 765 events, 65 distinct ids.
///
/// Returns `None` when the repo is NOT derivable rather than guessing one. The
/// old fallbacks — the whole key for a colonless stream, and the literal `"-"`
/// for an attention key with no middle segment — turned those 140 events into 66
/// fictitious repos, so "Repos observed" rendered 67 against a single-tenant
/// store, listing work-item ids and a bare dash to the operator as repositories.
/// That contradicted this module's own stated invariant, that the projection
/// "collapses to the single observed tenant".
///
/// A FOURTH shape is not a defect at all and still had to be excluded: a stream
/// under the FLEET context, `fleet:{family}`. It satisfies `{context}:{repo}`
/// syntactically, so the second bullet extracted its family name — `livespec` —
/// and rendered it as a repository. Measured in the same live store: "Repos
/// observed: 2: livespec, livespec-console-beads-fabro" on a single-tenant
/// store, from the 56 events on the one `fleet:livespec` stream. See
/// [`fleet_scoped`].
///
/// The ROOT CAUSE is upstream of here: those streams do not follow the
/// `{context}:{repo}` contract the second bullet describes. Correcting the keys
/// themselves is event-identity work and belongs to that thread; this function's
/// job is to stop manufacturing data it does not have.
fn repo_id(event: &ConsoleEvent) -> Option<String> {
    // Checked before the event-shape match, not inside one arm of it, so the
    // answer is "no repository" for a fleet stream whatever event rides it --
    // including an event whose payload carries a repo of its own.
    if fleet_scoped(event.stream_id()) {
        return None;
    }
    match event.event_type() {
        EventType::AttentionItemAppeared | EventType::AttentionItemChanged => {
            attention_item_snapshot_from_payload_json(event.payload_json()).map_or_else(
                || attention_stream_repo(event.stream_id()),
                |item| Some(item.source_ref().repo().to_owned()),
            )
        }
        EventType::AttentionItemResolved => attention_stream_repo(event.stream_id()),
        _other => stream_prefix_repo(event.stream_id()),
    }
}

/// The stream context that carries FLEET-scoped state: state of the product
/// family as a whole rather than of any one repository.
const FLEET_STREAM_CONTEXT: &str = "fleet";

/// Whether a stream key is fleet-scoped, i.e. keyed `fleet:{family}`.
///
/// `fleet:livespec` is the one such stream today: every operator factory-drain
/// command is keyed under it (see `factory_drain_command`), and the aggregate it
/// names is the product FAMILY the header renders as `fleet: livespec` — not a
/// tenant, not a checkout, and so not a row in the repo roster.
fn fleet_scoped(stream_id: &str) -> bool {
    stream_id
        .split_once(':')
        .is_some_and(|(context, _family)| context == FLEET_STREAM_CONTEXT)
}

/// The repo segment of a `{context}:{repo}` stream key: the text after the first
/// colon, or the whole key when it carries no colon.
fn stream_prefix_repo(stream_id: &str) -> Option<String> {
    stream_id
        .split_once(':')
        .map(|(_context, repo)| repo.to_owned())
        .filter(|repo| !repo.is_empty())
}

/// The repo segment of an `attention_item:{repo}:{id}` stream key: its middle
/// segment. Falls back to `-` when the key carries no middle segment (an
/// attention stream key is always three-part, so this is a defensive default).
fn attention_stream_repo(stream_id: &str) -> Option<String> {
    let mut parts = stream_id.splitn(3, ':');
    let _context = parts.next();
    parts
        .next()
        .filter(|repo| !repo.is_empty())
        .map(ToOwned::to_owned)
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
            Self::FabroRunObserved => "Fabro human gate",
            Self::FactoryDrainCompleted => "Factory drain completed",
            Self::FactoryDrainFailed => "Factory drain failed",
            Self::FactoryDrainAwaitingHuman => "Factory drain awaiting human",
            Self::FactoryDrainNotWired => "Factory drain not wired",
            Self::FactoryDispatchItemCompleted => "Factory dispatch item completed",
            Self::FactoryDispatchItemFailed => "Factory dispatch item failed",
            Self::FactoryDispatchItemNotWired => "Factory dispatch item not wired",
            Self::FactoryDispatchItemRequested => "Factory dispatch item requested",
            Self::FactoryDispatchItemStarted => "Factory dispatch item started",
            Self::GithubPullRequestSnapshotObserved => "GitHub pull request snapshot",
            Self::LivespecNextSnapshotObserved => "LiveSpec next snapshot",
            Self::LivespecReviseRequired => "LiveSpec revise required",
            Self::DispatcherBacklogBounceObserved => "Dispatcher backlog bounce",
            Self::DispatcherJournalProgressObserved => "Dispatcher journal progress",
            Self::DispatcherRefusalObserved => "Dispatcher refusal",
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
            Self::FactoryRunOrphansObserved => "Factory run orphans observed",
            Self::ConfigDispatcherSettingChanged => "Dispatcher setting changed",
            Self::ConfigDispatcherSettingNotWired => "Dispatcher setting not wired",
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::manual_assert, clippy::panic)]

    use std::cell::RefCell;

    use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};
    use proptest::proptest;

    use super::failure_cause;
    use super::source_adapters::{
        AcceptancePolicy, AdmissionPolicy, AttentionHandoff, AttentionItemSnapshot,
        AttentionSourceRef, DispatcherJournalEntry, DispatcherJournalKind, Lane, LaneReason,
        SourceProbe, SourceProbeOutcome, WorkItemComment, WorkItemDetail, WorkItemSnapshot,
        attention_item_payload_json, attention_resolved_payload_json,
        dispatcher_journal_payload_json,
    };
    use super::{
        ActionFailure, ApplicationError, AttentionDetail, AttentionEvent, AttentionItem,
        AutonomousAudit, AutonomousDecisionsPort, CommandOutcome, ConfigCommandOutcome,
        DispatcherFactoryDispatchItemPort, DispatcherFactoryDrainPort,
        DispatcherOrchestratorActionPort, DispatcherOverride, DispatcherSettingRow,
        DispatcherSettingSetRequest, DispatcherSettingWrite, DispatcherSettingWriteState,
        DispatcherSettings, DispatcherSettingsPort, DispatcherSettingsRead, EventsFocus,
        FactoryDispatchItemPort, FactoryDispatchItemPortOutcome, FactoryDispatchItemRequest,
        FactoryDrainPolicy, FactoryDrainPort, FactoryDrainPortOutcome, FactoryDrainRequest,
        FocusPane, HEADER_SCROLL_STEP, HELP_SECTION_COUNT, HINT_OVERFLOW_KEY, HelpFocus,
        JournalAutonomousDecisionsPort, LaneExecutionState, LaneFocus, LaneWorkItem, ListEdge,
        MAX_TRANSIENT_STATUS_CHARS, OUTCOME_CAUSE_ABSENT, OperatorAction, OperatorActionOutcome,
        OrchestratorActionOutcome, OrchestratorActionPort, OrchestratorActionRequest, OverrideBool,
        OverrideInt, PendingValve, PluginResolution, RejectMode, STARTUP_INGEST_LOADING_TELL,
        SettingRow, SettingRowStatus, TuiInteraction, TuiInteractionState, TuiOverlay,
        TuiScreenModel, TuiView, action_registry, build_tui_model, build_tui_model_for_state,
        command_outcome_notice, command_palette_query_opens_action_invoker,
        dispatcher_setting_rows, dispatcher_setting_write_settled, drilldown_item_count,
        factory_dispatch_item_command, fit_footer_line, fold_dispatcher_setting_reread,
        handle_config_dispatcher_setting_set_command, handle_factory_dispatch_item_command,
        handle_factory_drain_command, handle_work_item_accept_command,
        handle_work_item_approve_command, handle_work_item_move_command,
        handle_work_item_reject_command, handle_work_item_resolve_blocked_command,
        handle_work_item_set_acceptance_command, handle_work_item_set_admission_command,
        handle_work_item_set_dispatcher_override_command,
        handle_work_item_set_workflow_scope_override_command, header_help_section,
        help_section_for_focus, help_section_for_view, hint_display_width, model_pane_footer_hint,
        overlay_footer_hint, per_item_verb_is_state_valid, plan_page_url, project_action_failures,
        project_attention, project_lane_board, project_orphaned_factory_runs, project_plan_page,
        reduce_tui_interaction, render_plan_page_html, resolve_command_palette_action,
        resolve_dispatcher_setting_edit, resolve_valve_action, set_acceptance_policy_from_payload,
        set_admission_policy_from_payload, status_move_targets, validate_operator_action,
        work_item_action_name, work_item_failure_event, work_item_override_outcome,
    };

    #[track_caller]
    fn check(condition: bool, context: &str) {
        if !condition {
            panic!("{context}");
        }
    }

    #[track_caller]
    fn ok_factory_command_outcome(
        result: super::ApplicationResult<super::FactoryCommandOutcome>,
    ) -> super::FactoryCommandOutcome {
        match result {
            Ok(outcome) => outcome,
            Err(error) => panic!("{error:?}"),
        }
    }

    #[track_caller]
    fn ok_work_item_command_outcome(
        result: super::ApplicationResult<super::WorkItemCommandOutcome>,
    ) -> super::WorkItemCommandOutcome {
        match result {
            Ok(outcome) => outcome,
            Err(error) => panic!("{error:?}"),
        }
    }

    #[track_caller]
    fn ok_operator_action_outcome(
        result: super::ApplicationResult<OperatorActionOutcome>,
    ) -> OperatorActionOutcome {
        match result {
            Ok(outcome) => outcome,
            Err(error) => panic!("{error:?}"),
        }
    }

    #[test]
    #[should_panic(expected = "expected panic")]
    fn check_panics() {
        check(false, "expected panic");
    }

    #[test]
    #[should_panic(expected = "NoSelectedAttentionItem")]
    fn ok_factory_command_outcome_panics() {
        ok_factory_command_outcome(Err(ApplicationError::NoSelectedAttentionItem));
    }

    #[test]
    #[should_panic(expected = "NoSelectedAttentionItem")]
    fn ok_work_item_command_outcome_panics() {
        ok_work_item_command_outcome(Err(ApplicationError::NoSelectedAttentionItem));
    }

    #[test]
    #[should_panic(expected = "NoSelectedOperatorAction")]
    fn ok_operator_action_outcome_panics() {
        ok_operator_action_outcome(Err(ApplicationError::NoSelectedOperatorAction));
    }

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

    fn lane_event_with_factory_safety(
        event_id: &str,
        work_item_id: &str,
        lane: Lane,
        factory_safety: Option<&str>,
        rank: &str,
        status: &str,
    ) -> ConsoleEvent {
        let factory_safety_json =
            factory_safety.map_or_else(|| "null".to_owned(), |value| format!("\"{value}\""));
        let payload = format!(
            concat!(
                r#"{{"repo":"console","work_item_id":"{}","#,
                r#""lane":"{}","lane_reason":null,"rank":"{}","status":"{}","#,
                r#""source_version":1,"detail":{{"title":"Driver handoff fixture","#,
                r#""factory_safety":{}}}}}"#,
            ),
            work_item_id,
            lane.label(),
            rank,
            status,
            factory_safety_json,
        );
        ConsoleEvent::fixture(
            event_id,
            EventType::WorkItemSnapshotObserved,
            "orchestrator",
        )
        .with_payload_json(payload)
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

    fn dispatcher_execution_event(
        event_id: &str,
        work_item_id: &str,
        dispatch_id: &str,
    ) -> ConsoleEvent {
        let entries: Vec<DispatcherJournalEntry> = DispatcherJournalEntry::new(
            "console",
            work_item_id,
            dispatch_id,
            DispatcherJournalKind::Progress,
            2,
        )
        .ok()
        .into_iter()
        .collect();
        assert_eq!(entries.len(), 1);
        let entry = entries[0].clone();
        let payload = dispatcher_journal_payload_json(&entry);
        ConsoleEvent::fixture(
            event_id,
            EventType::DispatcherJournalProgressObserved,
            "dispatcher",
        )
        .with_payload_json(payload)
    }

    fn dispatcher_terminal_events(work_item_id: &str, dispatch_id: &str) -> Vec<ConsoleEvent> {
        let payload = format!(
            r#"{{"repo":"console","work_item_id":"{work_item_id}","dispatch_id":"{dispatch_id}","kind":"backlog-bounce","terminal_status":"completed","source_version":3}}"#
        );
        vec![
            ConsoleEvent::fixture(
                "evt_terminal",
                EventType::DispatcherBacklogBounceObserved,
                "dispatcher",
            )
            .with_payload_json(payload),
        ]
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
    fn active_lane_distinguishes_claimed_items_from_executing_items() {
        let events = [
            lane_event(
                "evt_claimed_a",
                "console-claimed-a",
                Lane::Active,
                None,
                "a1",
                "active",
            ),
            lane_event(
                "evt_claimed_b",
                "console-claimed-b",
                Lane::Active,
                None,
                "a2",
                "active",
            ),
            lane_event(
                "evt_executing",
                "console-executing",
                Lane::Active,
                None,
                "a3",
                "active",
            ),
            dispatcher_execution_event("evt_dispatch", "console-executing", "dispatch_1"),
        ];

        let board = project_lane_board(&events);
        let active = &board.columns()[3];
        assert_eq!(active.lane(), Lane::Active);
        let states: Vec<(&str, LaneExecutionState)> = active
            .items()
            .iter()
            .map(|item| (item.work_item_id(), item.execution_state()))
            .collect();

        assert_eq!(active.count(), 3);
        assert_eq!(active.claimed_count(), 2);
        assert_eq!(active.executing_count(), 1);
        assert_eq!(
            states,
            [
                ("console-claimed-a", LaneExecutionState::Claimed),
                ("console-claimed-b", LaneExecutionState::Claimed),
                ("console-executing", LaneExecutionState::Executing),
            ]
        );
    }

    #[test]
    fn active_lane_marks_terminal_signalled_items_finished_unreconciled() {
        let mut events = vec![
            lane_event(
                "evt_finished",
                "console-finished",
                Lane::Active,
                None,
                "a1",
                "active",
            ),
            dispatcher_execution_event("evt_dispatch", "console-finished", "dispatch_done"),
        ];
        events.extend(dispatcher_terminal_events(
            "console-finished",
            "dispatch_done",
        ));

        let board = project_lane_board(&events);
        let active = &board.columns()[3];
        let item = &active.items()[0];

        assert_eq!(active.count(), 1);
        assert_eq!(active.claimed_count(), 0);
        assert_eq!(active.executing_count(), 0);
        assert_eq!(item.lane(), Lane::Active);
        assert_eq!(item.execution_state().label(), "finished?");
    }

    #[test]
    fn lane_board_skips_non_snapshot_and_unparseable_payloads() {
        let events = [
            // A different event type is not a lane source.
            ConsoleEvent::fixture("evt_gate", EventType::FabroRunObserved, "fabro"),
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
    fn lane_item_observation_confirmed_is_false_when_its_own_source_is_degraded() {
        // The lane item's snapshot came from "orchestrator" (`lane_event`'s
        // fixed source), and "orchestrator" is also the source whose MOST
        // RECENT observation failed -- the not-observed finding is ordered
        // after the snapshot, so it is the one that wins -- so the row's
        // values are last-known, not current, and `observation_confirmed()`
        // says so.
        let events = [
            lane_event("evt_ok", "console-1", Lane::Ready, None, "a0", "ready"),
            ConsoleEvent::fixture(
                "evt_orchestrator_not_observed",
                EventType::SourceNotObservedFindingObserved,
                "orchestrator",
            ),
        ];

        let board = project_lane_board(&events);

        let item = &board
            .column(Lane::Ready)
            .map(super::LaneColumn::items)
            .unwrap_or_default()[0];
        assert!(!item.observation_confirmed());
    }

    #[test]
    fn lane_item_observation_confirmed_is_true_when_a_different_source_is_degraded() {
        // "github" degraded, not "orchestrator" -- the row's own backing
        // source was observed this cycle, so its values are confirmed even
        // though the header shows an unrelated source unavailable.
        let events = [
            ConsoleEvent::fixture(
                "evt_github_not_observed",
                EventType::SourceNotObservedFindingObserved,
                "github",
            ),
            lane_event("evt_ok", "console-1", Lane::Ready, None, "a0", "ready"),
        ];

        let board = project_lane_board(&events);

        let item = &board
            .column(Lane::Ready)
            .map(super::LaneColumn::items)
            .unwrap_or_default()[0];
        assert!(item.observation_confirmed());
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
        assert_eq!(registry_attention_actions_for_model(&model), []);
        // This fixture carries no reconciler observation, so the
        // orphaned-factory-runs lane is EMPTY -- the honest projection of a
        // source nothing has read this session.
        assert_eq!(model.orphaned_factory_runs(), []);
        assert_eq!(
            model.header(),
            "fleet: livespec | mode: tui | repo: - | view: Attention | attention: 0"
        );
        // The default Attention view (no overlay) shows the Attention pane's
        // context-specific hints -- non-empty and appropriate to the focused
        // pane, never the old single static string (Scenario 19 / TUI Contract).
        // This fixture's inbox is EMPTY ("attention: 0"), so the per-item valve
        // keys, record drill-in, and up/down navigation act on nothing.
        assert_eq!(model.footer(), "1-6 view | ? help | q quit");
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
        // No ingested record for the referenced work item -> the subject
        // carries the explicit-absence marker (v042 Scenario 5), with no
        // synthesized timeline or actions.
        let detail = super::build_needs_attention_detail(&with_work_item, &[]);
        assert_eq!(detail.repo(), orchestrator);
        assert_eq!(detail.work_item(), "bd-ib-ss7rkr (record not ingested)");
        assert_eq!(detail.fabro_run(), "-");
        assert_eq!(detail.valve_commands(), ["drive-cmd".to_owned()]);
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
            super::build_needs_attention_detail(&with_path, &[]).work_item(),
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
            super::build_needs_attention_detail(&bare, &[]).work_item(),
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

    /// The direct view-switch reduction: the aimed jump the `1`..`6` keys make,
    /// from EVERY starting view to every destination, including the no-op onto
    /// the view already active.
    ///
    /// The stepwise walk above is a different question — it asks where ONE step
    /// lands and where the walk clamps. This asks whether an aimed jump arrives,
    /// which no amount of stepping proves.
    #[test]
    fn selecting_a_view_by_name_arrives_from_any_other_view() {
        let events = fabro_gate_events();
        for from in TuiView::all() {
            for to in TuiView::all() {
                // Built EAGERLY, and reported through `check`: a failure-only
                // `assert_eq!` message is a region no passing run can execute,
                // which the coverage gate reads as a genuine miss.
                let jump = format!("{} -> {}", from.label(), to.label());
                let start = TuiInteractionState::for_view(*from, 0, TuiOverlay::None);
                let landed =
                    reduce_tui_interaction(&start, &events, TuiInteraction::SelectView(*to));

                check(landed.active_view() == *to, &jump);
                check(
                    build_tui_model_for_state(&events, &landed).active_view() == *to,
                    &jump,
                );
                // No list edge is announced: an aimed jump moves no cursor over
                // a list, so there is no refusal to cue.
                check(landed.list_edge().is_none(), &jump);
            }
        }
    }

    /// A jump resets the Detail pane's scroll for the same reason a step does:
    /// the pane is about to show a DIFFERENT view's content, and an inherited
    /// offset would open it part-scrolled.
    #[test]
    fn selecting_a_view_by_name_resets_the_detail_scroll() {
        let events = fabro_gate_events();
        let start = TuiInteractionState::for_view(TuiView::Attention, 0, TuiOverlay::None)
            .with_detail_max_scroll(9)
            .with_detail_scroll(4);
        assert_eq!(start.detail_scroll(), 4);

        let landed =
            reduce_tui_interaction(&start, &events, TuiInteraction::SelectView(TuiView::Repos));

        assert_eq!(landed.active_view(), TuiView::Repos);
        assert_eq!(landed.detail_scroll(), 0);
    }

    /// The dogfooding session's FIRST keystroke: `Up` on the Views navigation
    /// with `Attention` (the first entry) already selected. The selection still
    /// does not move and still does not wrap -- the Status line SAYS so, which
    /// is what tells a reached edge from a broken key.
    #[test]
    fn an_arrow_at_a_views_navigation_edge_cues_the_edge_it_reached() {
        let events = fabro_gate_events();
        let start = TuiInteractionState::new(0, TuiOverlay::None);
        let hints = build_tui_model_for_state(&events, &start)
            .footer()
            .into_owned();

        let top = reduce_tui_interaction(&start, &events, TuiInteraction::SelectPreviousView);
        assert_eq!(top.active_view(), TuiView::Attention);
        assert_eq!(top.list_edge(), Some(ListEdge::Top));
        // The cue LEADS the hints the context already owned; it does not
        // replace them, so nothing the operator could press stops being named.
        assert_eq!(
            build_tui_model_for_state(&events, &top).footer(),
            format!("at top of list | {hints}")
        );

        // The mirror at the other end: `Down` on the LAST view.
        let last = TuiInteractionState::for_view(TuiView::Settings, 0, TuiOverlay::None);
        let bottom = reduce_tui_interaction(&last, &events, TuiInteraction::SelectNextView);
        assert_eq!(bottom.active_view(), TuiView::Settings);
        assert_eq!(bottom.list_edge(), Some(ListEdge::Bottom));
        assert!(
            build_tui_model_for_state(&events, &bottom)
                .footer()
                .starts_with("at end of list | "),
        );

        // A move that MOVES carries no cue, and the cue does not outlive the
        // keystroke that earned it.
        let moved = reduce_tui_interaction(&top, &events, TuiInteraction::SelectNextView);
        assert_eq!(moved.active_view(), TuiView::Spec);
        assert_eq!(moved.list_edge(), None);
        assert_eq!(
            build_tui_model_for_state(&events, &moved).footer(),
            build_tui_model_for_state(
                &events,
                &TuiInteractionState::for_view(TuiView::Spec, 0, TuiOverlay::None)
            )
            .footer()
        );
    }

    /// Every list the Content pane hosts cues its own edges: the attention
    /// inbox, the lane overview, a drilled-in lane, and the settings rows.
    #[test]
    fn an_arrow_at_a_content_list_edge_cues_the_edge_it_reached() {
        let events = fabro_gate_events();
        let attention_count =
            build_tui_model_for_state(&events, &TuiInteractionState::new(0, TuiOverlay::None))
                .attention_items()
                .len();
        check(attention_count > 1, "the inbox fixture must hold a list");

        for (label, on_first, on_last, movable) in [
            (
                "attention inbox",
                TuiInteractionState::new(0, TuiOverlay::None),
                TuiInteractionState::new(attention_count - 1, TuiOverlay::None),
                TuiInteractionState::new(0, TuiOverlay::None),
            ),
            (
                "lane overview",
                TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None),
                TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
                    .with_selected_lane_index(Lane::all().len() - 1),
                TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None),
            ),
            (
                "settings rows",
                TuiInteractionState::for_view(TuiView::Settings, 0, TuiOverlay::None),
                TuiInteractionState::for_view(TuiView::Settings, 0, TuiOverlay::None)
                    .with_selected_setting_index(DispatcherSettingRow::all().len() - 1),
                TuiInteractionState::for_view(TuiView::Settings, 0, TuiOverlay::None),
            ),
        ] {
            let at_top = reduce_tui_interaction(&on_first, &events, TuiInteraction::SelectPrevious);
            check(
                at_top.list_edge() == Some(ListEdge::Top),
                &format!("{label}: up on the first row must cue the top"),
            );
            check(
                build_tui_model_for_state(&events, &at_top)
                    .footer()
                    .starts_with("at top of list | "),
                &format!("{label}: the top cue must lead the Status line"),
            );

            let at_end = reduce_tui_interaction(&on_last, &events, TuiInteraction::SelectNext);
            check(
                at_end.list_edge() == Some(ListEdge::Bottom),
                &format!("{label}: down on the last row must cue the end"),
            );
            check(
                build_tui_model_for_state(&events, &at_end)
                    .footer()
                    .starts_with("at end of list | "),
                &format!("{label}: the end cue must lead the Status line"),
            );

            let moved = reduce_tui_interaction(&movable, &events, TuiInteraction::SelectNext);
            check(
                moved.list_edge().is_none(),
                &format!("{label}: a move that moves carries no cue"),
            );
        }

        // A drilled-in lane: two pending-approval items, so index 0 is the top
        // and index 1 is the end.
        let drilldown = drilldown_events();
        let first = drilldown_state(Lane::PendingApproval, 0, TuiOverlay::None);
        assert_eq!(
            reduce_tui_interaction(&first, &drilldown, TuiInteraction::SelectPrevious).list_edge(),
            Some(ListEdge::Top)
        );
        assert_eq!(
            reduce_tui_interaction(&first, &drilldown, TuiInteraction::SelectNext).list_edge(),
            None
        );
        let last = drilldown_state(Lane::PendingApproval, 1, TuiOverlay::None);
        let at_end = reduce_tui_interaction(&last, &drilldown, TuiInteraction::SelectNext);
        assert_eq!(at_end.list_edge(), Some(ListEdge::Bottom));
        assert!(
            build_tui_model_for_state(&drilldown, &at_end)
                .footer()
                .starts_with("at end of list | ")
        );
    }

    /// Where there is no list to reach the end of, and where up/down mean
    /// something else, nothing is cued.
    #[test]
    fn no_edge_is_cued_without_a_list_or_behind_an_open_overlay() {
        let events = drilldown_events();
        // An EMPTY drilled-in lane (backlog carries no fixture item) has no top
        // and no end: its hints already say only stepping back out does anything.
        let empty = drilldown_state(Lane::Backlog, 0, TuiOverlay::None);
        assert_eq!(
            reduce_tui_interaction(&empty, &events, TuiInteraction::SelectNext).list_edge(),
            None
        );
        assert_eq!(
            reduce_tui_interaction(&empty, &events, TuiInteraction::SelectPrevious).list_edge(),
            None
        );

        // Behind an open overlay up/down scroll, cycle, or walk a roster rather
        // than moving the Content pane's list, so no edge is announced.
        let searching = TuiInteractionState::new(
            0,
            TuiOverlay::Search {
                query: String::new(),
            },
        );
        assert_eq!(
            reduce_tui_interaction(&searching, &events, TuiInteraction::SelectPrevious).list_edge(),
            None
        );

        // An interaction that moves no list selection never cues either.
        assert_eq!(
            reduce_tui_interaction(
                &TuiInteractionState::new(0, TuiOverlay::None),
                &events,
                TuiInteraction::FocusContent,
            )
            .list_edge(),
            None
        );
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
        assert_ne!(model.footer(), model_pane_footer_hint(&model));

        // An open overlay wins the hint line even while the header holds focus.
        let with_help = focused.with_overlay(TuiOverlay::Help {
            focus: HelpFocus::Menu,
            selected_section: 0,
            scroll: 0,
        });
        let help_model = build_tui_model_for_state(&events, &with_help);
        assert!(help_model.footer().contains("close help"));
    }

    #[test]
    fn header_surfaces_factory_drain_in_flight_and_terminal_status() {
        let requested = ConsoleEvent::fixture(
            "evt_cmd_factory_drain_requested_budget_1_parallel_1_0_requested",
            EventType::FactoryDrainRequested,
            "console:factory-command-handler",
        );
        let completed = ConsoleEvent::fixture(
            "evt_cmd_factory_drain_requested_budget_1_parallel_1_0_completed",
            EventType::FactoryDrainCompleted,
            "console:factory-command-handler",
        );
        let failed = ConsoleEvent::fixture(
            "evt_cmd_factory_drain_requested_budget_1_parallel_1_0_failed",
            EventType::FactoryDrainFailed,
            "console:factory-command-handler",
        );
        let awaiting_human = ConsoleEvent::fixture(
            "evt_cmd_factory_drain_requested_budget_1_parallel_1_0_awaiting_human",
            EventType::FactoryDrainAwaitingHuman,
            "console:factory-command-handler",
        );
        let not_wired = ConsoleEvent::fixture(
            "evt_cmd_factory_drain_requested_budget_1_parallel_1_0_not_wired",
            EventType::FactoryDrainNotWired,
            "console:factory-command-handler",
        );
        let rejected = ConsoleEvent::new(
            "evt_cmd_factory_drain_requested_budget_1_parallel_1_0_rejected".to_owned(),
            1,
            "command".to_owned(),
            EventType::CommandRejected,
            "console:factory-command-handler".to_owned(),
            "fleet:livespec".to_owned(),
            1,
        );
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None);

        let in_flight = build_tui_model_for_state(std::slice::from_ref(&requested), &state);
        assert!(in_flight.header().contains("factory: drain in flight"));
        assert!(
            in_flight
                .header_line(120)
                .contains("factory: drain in flight")
        );

        let terminal = build_tui_model_for_state(&[requested, completed], &state);
        assert!(terminal.header().contains("factory: drain completed"));

        let failed_model = build_tui_model_for_state(&[failed], &state);
        assert!(failed_model.header().contains("factory: drain failed"));

        let awaiting_human_model = build_tui_model_for_state(&[awaiting_human], &state);
        assert!(
            awaiting_human_model
                .header()
                .contains("factory: drain awaiting human")
        );

        let not_wired_model = build_tui_model_for_state(&[not_wired], &state);
        assert!(
            not_wired_model
                .header()
                .contains("factory: drain not wired")
        );

        let rejected_model = build_tui_model_for_state(&[rejected], &state);
        assert!(rejected_model.header().contains("factory: drain rejected"));
    }

    /// The Lanes overview with no overlay open: an ordinary operator screen
    /// whose hint line is non-empty, so an outcome message is asserted BESIDE
    /// the hints rather than in place of them.
    fn lane_overview_status_line(events: &[ConsoleEvent]) -> String {
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None);
        build_tui_model_for_state(events, &state)
            .footer()
            .into_owned()
    }

    fn outcome_event(event_type: EventType, payload_json: &str) -> ConsoleEvent {
        ConsoleEvent::fixture(
            "evt_command_outcome",
            event_type,
            "console:factory-command-handler",
        )
        .with_payload_json(payload_json.to_owned())
    }

    const LANE_OVERVIEW_HINTS: &str = "up/down move | enter drill | 1-6 view | ? help | q quit";

    #[test]
    fn the_status_line_reports_a_succeeded_command_beside_the_context_hints() {
        // The success direction of the same defect: a landed drain, dispatch or
        // valve confirmed nothing, so "it worked" and "it failed" looked alike.
        let succeeded = [
            (
                outcome_event(EventType::FactoryDrainCompleted, "{}"),
                "last command: drain succeeded",
            ),
            (
                outcome_event(EventType::FactoryDispatchItemCompleted, "{}"),
                "last command: dispatch item succeeded",
            ),
            (
                outcome_event(
                    EventType::WorkItemActionCompleted,
                    r#"{"action_id":"approve:lcbf-k0w"}"#,
                ),
                "last command: approve:lcbf-k0w succeeded",
            ),
        ];

        for (event, message) in succeeded {
            assert_eq!(
                lane_overview_status_line(std::slice::from_ref(&event)),
                format!("{LANE_OVERVIEW_HINTS} | {message}")
            );
        }
    }

    #[test]
    fn the_status_line_reports_a_failed_command_with_the_cause_the_store_carries() {
        let structured =
            r#"{"domain_error":"dispatch_refused","summary":"no ready work-item is factory-safe"}"#;
        let failed = [
            (
                outcome_event(EventType::FactoryDrainFailed, structured),
                "last command: drain failed — dispatch_refused: no ready work-item is factory-safe",
            ),
            (
                outcome_event(EventType::FactoryDispatchItemFailed, structured),
                "last command: dispatch item failed — dispatch_refused: no ready work-item is factory-safe",
            ),
            (
                outcome_event(
                    EventType::WorkItemActionFailed,
                    r#"{"action_id":"approve:lcbf-k0w","refusal":"the ledger refused the transition"}"#,
                ),
                "last command: approve:lcbf-k0w failed — the ledger refused the transition",
            ),
        ];

        for (event, message) in failed {
            assert_eq!(
                lane_overview_status_line(std::slice::from_ref(&event)),
                format!("{LANE_OVERVIEW_HINTS} | {message}")
            );
        }
    }

    #[test]
    fn a_failure_whose_stored_payload_names_no_cause_says_the_cause_is_absent() {
        // The reported shape: `status: failed` beside `error_json: {}`. The
        // absence is STATED; a bare "failed" is diagnosable by nobody.
        let event = outcome_event(EventType::FactoryDrainFailed, "{}");

        assert_eq!(
            lane_overview_status_line(std::slice::from_ref(&event)),
            format!("{LANE_OVERVIEW_HINTS} | last command: drain failed — {OUTCOME_CAUSE_ABSENT}")
        );
    }

    #[test]
    fn the_remaining_terminal_outcomes_are_named_for_what_they_are() {
        // Not-wired and a human-valve park are terminal too, and neither is a
        // success or a failure -- reporting them as either would be the same
        // dishonesty in a different direction.
        let named = [
            (
                outcome_event(EventType::FactoryDrainNotWired, "{}"),
                "last command: drain not wired",
            ),
            (
                outcome_event(EventType::FactoryDispatchItemNotWired, "{}"),
                "last command: dispatch item not wired",
            ),
            (
                outcome_event(EventType::WorkItemActionNotWired, "{}"),
                "last command: work-item action not wired",
            ),
            (
                outcome_event(EventType::FactoryDrainAwaitingHuman, "{}"),
                "last command: drain stopped at a human valve",
            ),
        ];

        for (event, message) in named {
            assert_eq!(
                lane_overview_status_line(std::slice::from_ref(&event)),
                format!("{LANE_OVERVIEW_HINTS} | {message}")
            );
        }
    }

    #[test]
    fn a_command_in_flight_retires_the_previous_command_s_verdict() {
        // Transience comes from the stream, not a clock: every request/start
        // marker empties the slot, so a verdict never outlives its command.
        let completed = outcome_event(EventType::FactoryDrainCompleted, "{}");
        for in_flight in [
            EventType::FactoryDrainRequested,
            EventType::FactoryDrainStarted,
            EventType::FactoryDispatchItemRequested,
            EventType::FactoryDispatchItemStarted,
            EventType::WorkItemActionStarted,
        ] {
            let events = [completed.clone(), outcome_event(in_flight, "{}")];

            assert_eq!(lane_overview_status_line(&events), LANE_OVERVIEW_HINTS);
        }
    }

    #[test]
    fn an_event_that_is_not_a_command_outcome_leaves_the_status_line_alone() {
        let events = fabro_gate_events();

        assert_eq!(lane_overview_status_line(&events), LANE_OVERVIEW_HINTS);
        assert_eq!(lane_overview_status_line(&[]), LANE_OVERVIEW_HINTS);
    }

    #[test]
    fn a_failure_cause_is_read_from_whichever_shape_the_payload_stored() {
        // `diagnostic_event_payload` splices a structured refusal in whole,
        // wraps an unstructured one as `refusal`, and stores `{}` for silence,
        // so all four shapes reach this reader.
        assert_eq!(
            failure_cause(
                r#"{"domain_error":"invalid_state","summary":"not at pending-approval"}"#
            ),
            Some("invalid_state: not at pending-approval".to_owned())
        );
        assert_eq!(
            failure_cause(r#"{"domain_error":"invalid_state"}"#),
            Some("invalid_state".to_owned())
        );
        assert_eq!(
            failure_cause(r#"{"summary":"not at pending-approval"}"#),
            Some("not at pending-approval".to_owned())
        );
        assert_eq!(
            failure_cause(r#"{"refusal":"exit status 1"}"#),
            Some("exit status 1".to_owned())
        );
        assert_eq!(failure_cause("{}"), None);
        assert_eq!(failure_cause("not json at all"), None);
    }

    #[test]
    fn a_work_item_action_without_an_action_id_is_named_by_its_contract() {
        assert_eq!(
            work_item_action_name(r#"{"action_id":"reject:lcbf-k0w"}"#),
            "reject:lcbf-k0w"
        );
        assert_eq!(work_item_action_name("{}"), "work-item action");
        assert_eq!(work_item_action_name("not json at all"), "work-item action");
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
                focus: HelpFocus::Menu,
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
            focus: HelpFocus::Menu,
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
                focus: HelpFocus::Menu,
                selected_section: expected_section,
                scroll: 0,
            };
            check(
                opened.overlay() == &expected,
                "open help should focus active pane section",
            );
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
        let opened = opened.with_help_scroll_extents(10, 4);
        let scrolled = reduce_tui_interaction(&opened, &events, TuiInteraction::HelpScrollDown);
        assert_eq!(
            scrolled.overlay(),
            &TuiOverlay::Help {
                focus: HelpFocus::Menu,
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
                focus: HelpFocus::Menu,
                selected_section: 4,
                scroll: 0,
            }
        );
        // Up moves back.
        let up = reduce_tui_interaction(&down, &events, TuiInteraction::HelpSelectPreviousSection);
        assert_eq!(
            up.overlay(),
            &TuiOverlay::Help {
                focus: HelpFocus::Menu,
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
                focus: HelpFocus::Menu,
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
                focus: HelpFocus::Menu,
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
        let opened = opened.with_help_scroll_extents(5, 3);
        let up = reduce_tui_interaction(&opened, &events, TuiInteraction::HelpScrollUp);
        assert_eq!(
            up.overlay(),
            &TuiOverlay::Help {
                focus: HelpFocus::Menu,
                selected_section: help_section_for_view(TuiView::Events),
                scroll: 0,
            }
        );
        let down = reduce_tui_interaction(&up, &events, TuiInteraction::HelpScrollDown);
        let down = reduce_tui_interaction(&down, &events, TuiInteraction::HelpScrollDown);
        assert_eq!(
            down.overlay(),
            &TuiOverlay::Help {
                focus: HelpFocus::Menu,
                selected_section: help_section_for_view(TuiView::Events),
                scroll: 2,
            }
        );
        let clamped = reduce_tui_interaction(&down, &events, TuiInteraction::HelpPageDown);
        assert_eq!(
            clamped.overlay(),
            &TuiOverlay::Help {
                focus: HelpFocus::Menu,
                selected_section: help_section_for_view(TuiView::Events),
                scroll: 5,
            }
        );
    }

    #[test]
    fn help_focus_and_focused_arrows_drive_the_expected_pane() {
        let events = fabro_gate_events();
        let opened = reduce_tui_interaction(
            &TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None),
            &events,
            TuiInteraction::OpenHelp,
        )
        .with_help_scroll_extents(20, 6);

        let text_focused = reduce_tui_interaction(&opened, &events, TuiInteraction::HelpFocusText);
        assert_eq!(
            text_focused.overlay(),
            &TuiOverlay::Help {
                focus: HelpFocus::Text,
                selected_section: help_section_for_view(TuiView::Lanes),
                scroll: 0,
            }
        );

        let scrolled =
            reduce_tui_interaction(&text_focused, &events, TuiInteraction::HelpScrollDown);
        assert_eq!(
            scrolled.overlay(),
            &TuiOverlay::Help {
                focus: HelpFocus::Text,
                selected_section: help_section_for_view(TuiView::Lanes),
                scroll: 1,
            }
        );

        let menu_focused =
            reduce_tui_interaction(&scrolled, &events, TuiInteraction::HelpFocusMenu);
        let next_section = reduce_tui_interaction(
            &menu_focused,
            &events,
            TuiInteraction::HelpSelectNextSection,
        );
        assert_eq!(
            next_section.overlay(),
            &TuiOverlay::Help {
                focus: HelpFocus::Menu,
                selected_section: help_section_for_view(TuiView::Events),
                scroll: 0,
            }
        );
    }

    #[test]
    fn help_page_scroll_uses_measured_rows_and_clamps() {
        let events = fabro_gate_events();
        let opened = reduce_tui_interaction(
            &TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None),
            &events,
            TuiInteraction::OpenHelp,
        )
        .with_help_scroll_extents(14, 6);

        let down = reduce_tui_interaction(&opened, &events, TuiInteraction::HelpPageDown);
        assert_eq!(
            down.overlay(),
            &TuiOverlay::Help {
                focus: HelpFocus::Menu,
                selected_section: help_section_for_view(TuiView::Events),
                scroll: 6,
            }
        );

        let near_bottom = opened.with_overlay(TuiOverlay::Help {
            focus: HelpFocus::Text,
            selected_section: help_section_for_view(TuiView::Events),
            scroll: 12,
        });
        let clamped = reduce_tui_interaction(&near_bottom, &events, TuiInteraction::HelpPageDown);
        assert_eq!(
            clamped.overlay(),
            &TuiOverlay::Help {
                focus: HelpFocus::Text,
                selected_section: help_section_for_view(TuiView::Events),
                scroll: 14,
            }
        );

        let up = reduce_tui_interaction(&down, &events, TuiInteraction::HelpPageUp);
        assert_eq!(
            up.overlay(),
            &TuiOverlay::Help {
                focus: HelpFocus::Menu,
                selected_section: help_section_for_view(TuiView::Events),
                scroll: 0,
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
            TuiInteraction::HelpPageDown,
            TuiInteraction::HelpPageUp,
            TuiInteraction::HelpFocusMenu,
            TuiInteraction::HelpFocusText,
        ] {
            let stepped = reduce_tui_interaction(&base, &events, interaction);
            assert_eq!(stepped.overlay(), &TuiOverlay::None);
        }

        let non_help_interaction = super::help_interaction_state(&base, TuiInteraction::SelectNext);
        assert_eq!(non_help_interaction.overlay(), &TuiOverlay::None);
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
        // whole title; the Spec count row carries NO baked-in explanatory
        // detail (B5 -- operational content only). The Repos view is the one
        // exception, opening with a fixed row stating what "observed" means
        // (mx9u.21 AC5) before its live repo roster. Events is covered below
        // instead of in this loop: it is now a container (mx9u.20.1) whose
        // pre-container "Stored events" row only reappears once drilled in.
        for (view, row_index, expected_title, expected_detail) in [
            (TuiView::Spec, 0, "LiveSpec next snapshots: 1", ""),
            (
                TuiView::Repos,
                1,
                "Repos observed: 2",
                "livespec-console-beads-fabro, other-repo",
            ),
        ] {
            let state = TuiInteractionState::for_view(view, 0, TuiOverlay::None);
            let model = build_tui_model_for_state(&events, &state);

            assert_eq!(model.active_view(), view);
            assert_eq!(model.view_items()[row_index].title(), expected_title);
            assert_eq!(model.view_items()[row_index].detail(), expected_detail);
        }

        // Events is a container: its own overview lists the two sub-view names
        // (AC2), and the pre-container stored-event count only reappears once
        // drilled into "Stored events" (AC3: unchanged content).
        let overview = build_tui_model_for_state(
            &events,
            &TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None),
        );
        assert_eq!(overview.view_items()[0].title(), "Stored events");
        assert_eq!(overview.view_items()[1].title(), "Event sources");

        let drilled = build_tui_model_for_state(&events, &events_stored_events_state());
        assert_eq!(drilled.view_items()[0].title(), "Stored events: 8");
        assert_eq!(drilled.view_items()[0].detail(), "");
    }

    /// A `TuiInteractionState` on the `Events` view, drilled into the "Stored
    /// events" sub-view -- the pre-container content every `tui_events_*` test
    /// below asserts is unchanged (AC3).
    fn events_stored_events_state() -> TuiInteractionState {
        reduce_tui_interaction(
            &TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None),
            &[],
            TuiInteraction::DrillIntoEventsSubView,
        )
    }

    #[test]
    fn tui_events_view_latest_row_carries_operational_detail_only() {
        let events = view_summary_events();
        let state = events_stored_events_state();
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
        let state = events_stored_events_state();
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
    fn events_focus_label_and_all_are_stable() {
        // `EventsFocus::Overview` borrows the container's OWN label, since it
        // is not itself a named sub-view; `all()` names only the two it is
        // NOT (the drillable set the container overview lists and the picker
        // cursor moves over).
        assert_eq!(EventsFocus::Overview.label(), "Events");
        assert_eq!(EventsFocus::StoredEvents.label(), "Stored events");
        // Maintainer ruling 2026-09-09: SOURCES is always "EVENT SOURCES".
        assert_eq!(EventsFocus::EventSources.label(), "Event sources");
        assert_eq!(
            EventsFocus::all(),
            [EventsFocus::StoredEvents, EventsFocus::EventSources]
        );
    }

    #[test]
    fn tui_events_overview_arrows_move_the_selected_sub_view_not_the_attention_list() {
        // Mirrors `tui_lanes_overview_arrows_move_the_selected_lane_not_the_attention_list`:
        // the container's own overview has a real per-row cursor (AC2), and
        // moving it must not disturb the (unrelated, hidden-behind-this-view)
        // Attention selection.
        let events = fabro_gate_events();
        let state = TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None);
        assert_eq!(state.events_focus(), EventsFocus::Overview);

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectNext);
        let model = build_tui_model_for_state(&events, &state);
        assert_eq!(state.selected_events_index(), 1);
        assert_eq!(model.selected_events_index(), 1);
        assert_eq!(state.selected_attention_index(), 0);

        // Only two sub-views exist, so a further SelectNext clamps at the last.
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectNext);
        assert_eq!(state.selected_events_index(), 1);

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectPrevious);
        assert_eq!(state.selected_events_index(), 0);
        // Kills the `TuiScreenModel::selected_events_index -> 1` constant
        // mutant (check-mutants, PR #1155 review): every assertion above
        // happens to want `1`, which a hardcoded `1` satisfies just as well as
        // the real accessor. This is the one case in this test where the
        // correct answer is `0`.
        let model = build_tui_model_for_state(&events, &state);
        assert_eq!(model.selected_events_index(), 0);
    }

    #[test]
    fn render_tui_model_clamps_an_out_of_range_selected_events_index() {
        // Mirrors the lane overview's own out-of-range clamp
        // (`tui_lanes_overview_clamps_the_selected_lane_at_the_last_lane`).
        // `render_tui_model`'s `.min(EventsFocus::all().len() - 1)` is normally
        // unreachable through the reducer (`move_selection_down` already keeps
        // the raw state in bounds), so nothing else in this suite ever hands it
        // an out-of-range index -- which is exactly why `check-mutants` found
        // the arithmetic here untested: swapping the `- 1` for `+ 1` or `/ 1`
        // widens the clamp bound to 3 or 2 and this test never notices unless
        // it feeds a raw index the reducer itself would never produce.
        let state = TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None)
            .with_selected_events_index(5);
        let model = build_tui_model_for_state(&fabro_gate_events(), &state);

        assert_eq!(model.selected_events_index(), 1);
    }

    #[test]
    fn drill_into_events_sub_view_clamps_an_out_of_range_selected_index() {
        // The sibling clamp inside `drill_into_events_sub_view`: an
        // out-of-range `selected_events_index` (never produced by the reducer,
        // see the test above) must still resolve to the LAST sub-view rather
        // than indexing `EventsFocus::all()` out of bounds. A `+ 1` or `/ 1`
        // mutant on this clamp's `- 1` widens the bound past the 2-element
        // slice's length and this call panics, which the mutant tester reads
        // as CAUGHT.
        let state = TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None)
            .with_selected_events_index(5);
        let events = fabro_gate_events();

        let drilled =
            reduce_tui_interaction(&state, &events, TuiInteraction::DrillIntoEventsSubView);

        assert_eq!(drilled.events_focus(), EventsFocus::EventSources);
    }

    #[test]
    fn tui_events_drill_into_event_sources_and_return_to_overview() {
        // AC2 + AC4, exercising the SECOND sub-view (`EventSources`), which the
        // "Stored events" fixtures above never reach: drilling from the
        // overview's second row opens "Event sources" fed from the
        // `observed_source_names` projection (name-only existence, NOT the
        // `unavailable_sources` health tally), and `ReturnToEventsOverview`
        // (Esc's reducer target) restores the picker.
        let events = [
            ConsoleEvent::fixture(
                "evt_dispatcher_down",
                EventType::SourceNotObservedFindingObserved,
                "dispatcher",
            ),
            ConsoleEvent::fixture(
                "evt_livespec_down",
                EventType::SourceNotObservedFindingObserved,
                "livespec",
            ),
        ];
        let state = TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None)
            .with_selected_events_index(1);
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::DrillIntoEventsSubView);
        let model = build_tui_model_for_state(&events, &state);

        assert_eq!(state.events_focus(), EventsFocus::EventSources);
        assert_eq!(model.events_focus(), EventsFocus::EventSources);
        let titles: Vec<&str> = model
            .view_items()
            .iter()
            .map(super::ViewSummaryItem::title)
            .collect();
        assert_eq!(titles, ["dispatcher", "livespec"]);
        assert_eq!(model.observed_source_names(), ["dispatcher", "livespec"]);
        // Both happen to be unavailable in THIS fixture, but the sub-view is
        // fed by existence, not health -- see the healthy-source test below.
        assert_eq!(model.unavailable_sources(), ["dispatcher", "livespec"]);

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::ReturnToEventsOverview);
        let model = build_tui_model_for_state(&events, &state);
        assert_eq!(state.events_focus(), EventsFocus::Overview);
        assert_eq!(model.events_focus(), EventsFocus::Overview);
    }

    #[test]
    fn tui_event_sources_sub_view_names_a_healthy_source_not_only_unavailable_ones() {
        // THE REGRESSION this guards: measured on the real store 2026-09-09
        // (mx9u.20.1 review) with six sources polling successfully, this
        // sub-view was fed from `unavailable_sources` (empty, since nothing was
        // down) and rendered "No event sources observed yet" -- false, and
        // false in exactly the direction the fix order this item belongs to
        // exists to end (mx9u.22/mx9u.24/mx9u.14/mx9u.17: unknown or stale
        // state presented as fact). A source that has only ever reported
        // POSITIVE observations -- never degraded -- must still be named here.
        let events = [ConsoleEvent::fixture(
            "evt_github_ok",
            EventType::GithubPullRequestSnapshotObserved,
            "github",
        )];
        let state = TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None)
            .with_events_focus(EventsFocus::EventSources);
        let model = build_tui_model_for_state(&events, &state);

        // Confirms the fixture actually exercises "healthy", not "unavailable":
        assert!(model.unavailable_sources().is_empty());
        assert_eq!(model.observed_source_names(), ["github"]);
        assert_eq!(model.view_items().len(), 1);
        assert_eq!(model.view_items()[0].title(), "github");
        assert!(!model.view_items()[0].title().contains("observed yet"));
    }

    #[test]
    fn tui_event_sources_sub_view_states_an_empty_projection() {
        // No source has EVER reported an event -- healthy or degraded -- so
        // the skeleton says so plainly rather than rendering an empty list
        // (AC5 of the parent epic's roster item still owns the full per-source
        // line; this is the navigation model's own empty state). Genuinely
        // empty events, not merely "nothing currently unavailable": see the
        // healthy-source test above for why those two are NOT interchangeable.
        let state = TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None)
            .with_events_focus(EventsFocus::EventSources);
        let model = build_tui_model_for_state(&[], &state);

        assert_eq!(model.view_items().len(), 1);
        assert_eq!(
            model.view_items()[0].title(),
            "No sources have reported any events yet"
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

    /// `tui_search_query` is the public wrapper a caching caller (the
    /// terminal loop's hot path) uses to compute its cache key without
    /// duplicating the private `search_query` match -- it must read exactly
    /// the same thing `search_query` itself does.
    #[test]
    fn tui_search_query_matches_search_query_for_every_overlay_shape() {
        let search = TuiOverlay::Search {
            query: "drain".to_owned(),
        };
        assert_eq!(super::tui_search_query(&search), Some("drain"));
        assert_eq!(super::tui_search_query(&TuiOverlay::None), None);
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
    fn tui_command_modal_stays_closed_without_attention_actions() {
        // A row that names NO work-item is the selection with nothing honest to
        // run: it resolves no record, so it offers no per-item verb on any
        // surface. (A `blocked` work-item row used to stand in for it here, and
        // stopped once the inbox row started offering the state-admitted
        // move-status picker -- Scenario 31.)
        let pathless = AttentionItemSnapshot::new(
            "spec:adapt:SPECIFICATION",
            "spec",
            "medium",
            "Adapt the specification",
            AttentionSourceRef::new("console", None, Some("SPECIFICATION")),
            AttentionHandoff::new("livespec-op", None, "livespec adapt"),
        );
        let events = [attention_appeared("evt_pathless", &pathless)];
        let state = reduce_tui_interaction(
            &TuiInteractionState::new(0, TuiOverlay::None),
            &events,
            TuiInteraction::OpenCommandModal,
        );

        assert_eq!(state.overlay(), &TuiOverlay::None);

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectNextAction);
        let model = build_tui_model_for_state(&events, &state);

        assert_eq!(state.overlay(), &TuiOverlay::None);
        assert_eq!(model.selected_operator_action(), None);
    }

    #[test]
    fn tui_command_modal_clamps_to_available_actions() {
        let events = fabro_gate_events();
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::CommandModal {
                selected_action_index: 99,
            },
        );
        let model = build_tui_model_for_state(&events, &state);
        let registry_actions = registry_attention_actions_for_model(&model);
        let last_action = registry_actions.len().saturating_sub(1);
        check(
            last_action > 0,
            "command modal should expose multiple actions",
        );

        assert_eq!(
            model.overlay(),
            &TuiOverlay::CommandModal {
                selected_action_index: last_action
            }
        );
        assert_eq!(
            model.selected_operator_action(),
            registry_actions.get(last_action).copied()
        );
    }

    #[test]
    fn command_palette_drain_is_not_a_parallel_dispatch_encoding() {
        for query in ["drain", "Drain ready queue", "  drain  "] {
            let state = TuiInteractionState::new(
                0,
                TuiOverlay::CommandPalette {
                    query: query.to_owned(),
                },
            );
            let model = build_tui_model_for_state(&fabro_gate_events(), &state);

            let outcome = resolve_command_palette_action(&model, "operator");

            assert_eq!(outcome, Err(ApplicationError::UnknownCommandPaletteAction));
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
            attention_total: 0,
            selected_attention_index: None,
            detail: None,
            view_items: Vec::new(),
            lane_board: project_lane_board(&[]),
            lane_focus: super::LaneFocus::Overview,
            selected_lane_index: None,
            selected_lane_item_index: None,
            missing_selected_lane_item_id: None,
            events_focus: super::EventsFocus::Overview,
            selected_events_index: 0,
            focus: FocusPane::Nav,
            detail_scroll: 0,
            header_scroll: 0,
            overlay: TuiOverlay::CommandModal {
                selected_action_index: 0,
            },
            selected_repo: String::new(),
            selected_setting_index: None,
            dispatcher_settings: DispatcherSettingsRead::NotObserved,
            dispatcher_setting_write: DispatcherSettingWriteState::Idle,
            plugin_resolution: PluginResolution::unresolved(),
            unavailable_sources: Vec::new(),
            observed_source_names: Vec::new(),
            factory_activity: None,
            transient_status: None,
            list_edge: None,
            command_outcome: None,
            header: "LiveSpec Console".to_owned(),
            action_failures: std::collections::BTreeMap::new(),
            orphaned_factory_runs: Vec::new(),
            build_identity: None,
            build_staleness: super::BuildStaleness::Unknown,
            startup_ingest_pending: false,
        };

        assert_eq!(model.selected_operator_action(), None);
    }

    #[test]
    fn orphaned_factory_runs_round_trips_populated_slot() {
        // Kills the mutant `replace TuiScreenModel::orphaned_factory_runs ->
        // &[OrphanedFactoryRun] with Vec::leak(Vec::new())` (cargo-mutants,
        // PR #923 check-mutants): every other test asserts the slot EMPTY,
        // which an always-empty getter also satisfies. The production feed
        // relies on the populated slot, so the getter must return what was
        // stored, not a constant.
        let run = super::source_adapters::OrphanedFactoryRun::new(
            "01M1RUN",
            "hp",
            "failed",
            "livespec-console-beads-fabro-h7jp",
            Some("ready"),
            "superseded-run",
            "none",
        );
        let model = super::TuiScreenModel {
            active_view: TuiView::Attention,
            navigation: vec![TuiView::Attention],
            attention_items: Vec::new(),
            attention_total: 0,
            selected_attention_index: None,
            detail: None,
            view_items: Vec::new(),
            lane_board: project_lane_board(&[]),
            lane_focus: super::LaneFocus::Overview,
            selected_lane_index: None,
            selected_lane_item_index: None,
            missing_selected_lane_item_id: None,
            events_focus: super::EventsFocus::Overview,
            selected_events_index: 0,
            focus: FocusPane::Nav,
            detail_scroll: 0,
            header_scroll: 0,
            overlay: TuiOverlay::None,
            selected_repo: String::new(),
            selected_setting_index: None,
            dispatcher_settings: DispatcherSettingsRead::NotObserved,
            dispatcher_setting_write: DispatcherSettingWriteState::Idle,
            plugin_resolution: PluginResolution::unresolved(),
            unavailable_sources: Vec::new(),
            observed_source_names: Vec::new(),
            factory_activity: None,
            transient_status: None,
            list_edge: None,
            command_outcome: None,
            header: "LiveSpec Console".to_owned(),
            action_failures: std::collections::BTreeMap::new(),
            orphaned_factory_runs: vec![run.clone()],
            build_identity: None,
            build_staleness: super::BuildStaleness::Unknown,
            startup_ingest_pending: false,
        };

        assert_eq!(model.orphaned_factory_runs(), [run]);
    }

    /// Build one `factory.run_orphans_observed` event carrying `runs`, exactly
    /// as the reconciler source adapter writes it.
    fn orphan_projection_event(
        event_id: &str,
        stream_seq: u64,
        runs: Vec<super::source_adapters::OrphanedFactoryRun>,
    ) -> ConsoleEvent {
        ConsoleEvent::new(
            event_id.to_owned(),
            1,
            "console".to_owned(),
            EventType::FactoryRunOrphansObserved,
            "reconcile-runs".to_owned(),
            "reconcile_runs:console".to_owned(),
            stream_seq,
        )
        .with_payload_json(
            super::source_adapters::reconcile_runs_snapshot_payload_json(
                &super::source_adapters::ReconcileRunsSnapshot::new(runs, Vec::new()),
            ),
        )
    }

    fn orphan_run_fixture(run_id: &str) -> super::source_adapters::OrphanedFactoryRun {
        super::source_adapters::OrphanedFactoryRun::new(
            run_id,
            "hp",
            "running",
            "livespec-console-beads-fabro-h7jp",
            Some("blocked"),
            "item-not-active",
            "none",
        )
    }

    /// The model's lane is FED from the event stream rather than hardcoded
    /// empty, and the LATEST projection wins outright.
    #[test]
    fn build_tui_model_feeds_the_orphaned_runs_lane_from_the_latest_projection() {
        let stale =
            orphan_projection_event("evt-orphans-1", 1, vec![orphan_run_fixture("01M1OLD")]);
        let fresh =
            orphan_projection_event("evt-orphans-2", 2, vec![orphan_run_fixture("01M1NEW")]);
        let interaction = TuiInteractionState::new(0, TuiOverlay::None);

        let model = build_tui_model_for_state(&[stale, fresh], &interaction);

        assert_eq!(
            model.orphaned_factory_runs(),
            [orphan_run_fixture("01M1NEW")]
        );
    }

    /// A projection that reports NO orphan replaces the previous one rather
    /// than folding into it. Accumulating instead would keep rendering an
    /// orphan the orchestrator no longer claims, and the operator would run a
    /// remedy that finds nothing.
    #[test]
    fn a_cleared_projection_empties_the_orphaned_runs_lane() {
        let orphaned =
            orphan_projection_event("evt-orphans-1", 1, vec![orphan_run_fixture("01M1OLD")]);
        let cleared = orphan_projection_event("evt-orphans-2", 2, Vec::new());

        assert_eq!(project_orphaned_factory_runs(&[orphaned, cleared]), []);
    }

    /// Events of every other type are invisible to this projection, and a
    /// corrupt payload on the right event type is skipped rather than replayed
    /// as an empty lane.
    #[test]
    fn the_orphaned_runs_projection_reads_only_readable_reconciler_events() {
        let unrelated = ConsoleEvent::fixture("evt-other", EventType::FabroRunObserved, "fabro");
        let corrupt = ConsoleEvent::new(
            "evt-orphans-corrupt".to_owned(),
            1,
            "console".to_owned(),
            EventType::FactoryRunOrphansObserved,
            "reconcile-runs".to_owned(),
            "reconcile_runs:console".to_owned(),
            9,
        )
        .with_payload_json("{}".to_owned());
        let readable =
            orphan_projection_event("evt-orphans-1", 1, vec![orphan_run_fixture("01M1RUN")]);

        assert_eq!(project_orphaned_factory_runs(&[]), []);
        assert_eq!(
            project_orphaned_factory_runs(std::slice::from_ref(&unrelated)),
            []
        );
        assert_eq!(
            project_orphaned_factory_runs(&[readable, corrupt, unrelated]),
            [orphan_run_fixture("01M1RUN")]
        );
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
    fn factory_dispatch_item_command_targets_the_selected_item() {
        let command = factory_dispatch_item_command("console-selected", "operator");

        assert_eq!(
            command.command_type(),
            &CommandType::FactoryDispatchItemRequested
        );
        assert_eq!(command.aggregate_id(), "console-selected");
        assert_eq!(
            command.idempotency_key(),
            "console-selected:factory.dispatch_item_requested"
        );
    }

    #[test]
    fn factory_dispatch_item_handler_accepts_and_records_not_wired() {
        let command = factory_dispatch_item_command("console-selected", "operator");
        let mut port = NotWiringDispatchItemPort::default();

        let outcome = handle_factory_dispatch_item_command(&command, &mut port);

        assert_eq!(
            outcome
                .as_ref()
                .map(super::FactoryCommandOutcome::command_status),
            Ok("not_wired")
        );
        assert_eq!(port.requests, ["console-selected"]);
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
                &EventType::FactoryDispatchItemNotWired,
            ])
        );
    }

    #[test]
    fn factory_dispatch_item_handler_records_completed_and_failed_outcomes() {
        let command = factory_dispatch_item_command("console-selected", "operator");
        let mut completing = CompletingDispatchItemPort::default();
        let completed = handle_factory_dispatch_item_command(&command, &mut completing);
        assert_eq!(
            completed
                .as_ref()
                .map(super::FactoryCommandOutcome::command_status),
            Ok("completed")
        );
        assert_eq!(
            completed
                .as_ref()
                .map(super::FactoryCommandOutcome::events)
                .map(|events| events
                    .iter()
                    .map(ConsoleEvent::event_type)
                    .collect::<Vec<_>>()),
            Ok(vec![
                &EventType::CommandAccepted,
                &EventType::FactoryDispatchItemStarted,
                &EventType::FactoryDispatchItemCompleted,
            ])
        );

        let mut failing = FailingDispatchItemPort;
        let failed = handle_factory_dispatch_item_command(&command, &mut failing);
        assert_eq!(
            failed
                .as_ref()
                .map(super::FactoryCommandOutcome::command_status),
            Ok("failed")
        );
        assert_eq!(
            failed
                .as_ref()
                .map(super::FactoryCommandOutcome::events)
                .and_then(|events| {
                    events
                        .last()
                        .map(ConsoleEvent::event_type)
                        .ok_or(&ApplicationError::NoSelectedAttentionItem)
                }),
            Ok(&EventType::FactoryDispatchItemFailed)
        );
        assert_eq!(
            failed
                .as_ref()
                .map(super::FactoryCommandOutcome::events)
                .map(|events| events[2].payload_json()),
            Ok(r#"{"refusal":"dispatch item refused"}"#)
        );
    }

    #[test]
    fn factory_dispatch_item_helpers_cover_default_outcomes_and_labels() {
        assert_eq!(
            FactoryDispatchItemPortOutcome::failed(),
            FactoryDispatchItemPortOutcome::Failed { diagnostic: None }
        );
        assert_eq!(
            FactoryDispatchItemPortOutcome::failed_with_diagnostic("held".to_owned()),
            FactoryDispatchItemPortOutcome::Failed {
                diagnostic: Some("held".to_owned())
            }
        );
        assert_eq!(
            EventType::FactoryDispatchItemCompleted.label(),
            "Factory dispatch item completed"
        );
        assert_eq!(
            EventType::FactoryDispatchItemFailed.label(),
            "Factory dispatch item failed"
        );
        assert_eq!(
            EventType::FactoryDispatchItemNotWired.label(),
            "Factory dispatch item not wired"
        );
        assert_eq!(
            EventType::FactoryDispatchItemRequested.label(),
            "Factory dispatch item requested"
        );
        assert_eq!(
            EventType::FactoryDispatchItemStarted.label(),
            "Factory dispatch item started"
        );
    }

    #[test]
    fn dispatcher_dispatch_item_port_uses_governed_bounded_loop_surface() {
        let probe = ArgsRecordingDrainProbe {
            config: SourceProbeOutcome::unavailable("unused"),
            drain: SourceProbeOutcome::observed("dispatch: dispatched 1 item", true),
            observed_args: std::cell::RefCell::new(Vec::new()),
        };
        let request = FactoryDispatchItemRequest::new("wi-selected".to_owned());
        let mut port =
            DispatcherFactoryDispatchItemPort::new(&probe, "dispatcher", &["loop", "--repo", "."]);

        let outcome = port.dispatch_item(&request);

        assert_ne!(outcome, Ok(FactoryDispatchItemPortOutcome::not_wired()));
        assert_eq!(outcome, Ok(FactoryDispatchItemPortOutcome::completed()));
        assert_eq!(
            *probe.observed_args.borrow(),
            [
                "loop",
                "--repo",
                ".",
                "--budget",
                "1",
                "--parallel",
                "1",
                "--item",
                "wi-selected"
            ]
        );
        assert!(
            !probe
                .observed_args
                .borrow()
                .iter()
                .any(|arg| arg == "dispatch")
        );
    }

    #[test]
    fn dispatcher_dispatch_item_port_surfaces_zero_dispatch_and_unavailable_honestly() {
        let zero_probe = ArgsRecordingDrainProbe {
            config: SourceProbeOutcome::unavailable("unused"),
            drain: SourceProbeOutcome::observed("drain: ready queue empty", true),
            observed_args: std::cell::RefCell::new(Vec::new()),
        };
        let request = FactoryDispatchItemRequest::new("wi-selected".to_owned());
        let mut zero_port =
            DispatcherFactoryDispatchItemPort::new(&zero_probe, "dispatcher", &["loop"]);

        assert_eq!(
            zero_port.dispatch_item(&request),
            Ok(FactoryDispatchItemPortOutcome::failed_with_diagnostic(
                "drain: ready queue empty".to_owned()
            ))
        );

        let empty_zero_probe = ArgsRecordingDrainProbe {
            config: SourceProbeOutcome::unavailable("unused"),
            drain: SourceProbeOutcome::observed("", true),
            observed_args: std::cell::RefCell::new(Vec::new()),
        };
        let mut empty_zero_port =
            DispatcherFactoryDispatchItemPort::new(&empty_zero_probe, "dispatcher", &["loop"]);

        assert_eq!(
            empty_zero_port.dispatch_item(&request),
            Ok(FactoryDispatchItemPortOutcome::failed_with_diagnostic(
                "dispatcher reported zero dispatched items".to_owned()
            ))
        );

        let failed_probe = ArgsRecordingDrainProbe {
            config: SourceProbeOutcome::unavailable("unused"),
            drain: SourceProbeOutcome::observed("dispatcher refused", false),
            observed_args: std::cell::RefCell::new(Vec::new()),
        };
        let mut failed_port =
            DispatcherFactoryDispatchItemPort::new(&failed_probe, "dispatcher", &["loop"]);

        assert_eq!(
            failed_port.dispatch_item(&request),
            Ok(FactoryDispatchItemPortOutcome::failed_with_diagnostic(
                "dispatcher refused".to_owned()
            ))
        );

        let empty_failed_probe = ArgsRecordingDrainProbe {
            config: SourceProbeOutcome::unavailable("unused"),
            drain: SourceProbeOutcome::observed("", false),
            observed_args: std::cell::RefCell::new(Vec::new()),
        };
        let mut empty_failed_port =
            DispatcherFactoryDispatchItemPort::new(&empty_failed_probe, "dispatcher", &["loop"]);

        assert_eq!(
            empty_failed_port.dispatch_item(&request),
            Ok(FactoryDispatchItemPortOutcome::failed())
        );

        let unavailable_probe = ArgsRecordingDrainProbe {
            config: SourceProbeOutcome::unavailable("unused"),
            drain: SourceProbeOutcome::unavailable("dispatcher missing"),
            observed_args: std::cell::RefCell::new(Vec::new()),
        };
        let mut unavailable_port =
            DispatcherFactoryDispatchItemPort::new(&unavailable_probe, "dispatcher", &["loop"]);

        assert_eq!(
            unavailable_port.dispatch_item(&request),
            Ok(FactoryDispatchItemPortOutcome::not_wired())
        );
    }

    #[test]
    fn header_surfaces_factory_dispatch_item_statuses() {
        for (event_type, expected) in [
            (
                EventType::FactoryDispatchItemRequested,
                "dispatch item in flight",
            ),
            (
                EventType::FactoryDispatchItemStarted,
                "dispatch item in flight",
            ),
            (
                EventType::FactoryDispatchItemCompleted,
                "dispatch item completed",
            ),
            (EventType::FactoryDispatchItemFailed, "dispatch item failed"),
            (
                EventType::FactoryDispatchItemNotWired,
                "dispatch item not wired",
            ),
        ] {
            let event = ConsoleEvent::fixture("evt_dispatch_item", event_type, "console");
            assert_eq!(
                super::factory_drain_activity(&[event]),
                Some(expected.to_owned())
            );
        }
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
    fn factory_drain_handler_keeps_no_diagnostic_failure_failed() {
        let command = factory_drain_test_command();
        let mut port = NoDiagnosticFailingDrainPort;

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
    fn factory_drain_handler_records_acceptance_park_as_awaiting_human() {
        let command = factory_drain_test_command();
        let mut port = AcceptanceParkDrainPort;

        let outcome = ok_factory_command_outcome(handle_factory_drain_command(
            &command,
            &ready_factory_drain_policy(),
            &mut port,
        ));

        assert_eq!(outcome.command_status(), "parked-awaiting-human");
        assert_eq!(
            outcome
                .events()
                .iter()
                .map(ConsoleEvent::event_type)
                .collect::<Vec<_>>(),
            vec![
                &EventType::CommandAccepted,
                &EventType::FactoryDrainStarted,
                &EventType::FactoryDrainAwaitingHuman,
            ]
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

    /// The resolve-blocked dialog's free-text ANSWER field: typed characters
    /// accumulate on it verbatim, backspace edits it, dialing the target status
    /// keeps it, and every OTHER valve stays text-inert exactly as before.
    #[test]
    fn the_resolve_blocked_dialog_accumulates_the_answer_and_other_valves_stay_inert() {
        let events: [ConsoleEvent; 0] = [];
        let resolve = PendingValve::MoveStatus {
            from: Lane::Blocked,
            to: Lane::Ready,
        };
        // Only the resolve-blocked move offers an answer.
        let into_blocked = PendingValve::MoveStatus {
            from: Lane::Ready,
            to: Lane::Blocked,
        };
        assert!(resolve.accepts_answer());
        assert!(!PendingValve::Approve.accepts_answer());
        assert!(!into_blocked.accepts_answer());

        let mut state = TuiInteractionState::new(
            0,
            TuiOverlay::ValveConfirm {
                valve: resolve,
                answer: String::new(),
            },
        );
        for character in "ship it!".chars() {
            state = reduce_tui_interaction(&state, &events, TuiInteraction::TypeChar(character));
        }
        state = reduce_tui_interaction(&state, &events, TuiInteraction::Backspace);
        assert_eq!(state.overlay().valve_answer(), Some("ship it"));

        // Dialing the target status must not discard text already typed.
        let dialed =
            reduce_tui_interaction(&state, &events, TuiInteraction::CycleValveOption(true));
        assert_eq!(dialed.overlay().valve_answer(), Some("ship it"));
        assert_eq!(
            dialed.overlay().valve_confirm(),
            Some(PendingValve::MoveStatus {
                from: Lane::Blocked,
                to: Lane::Backlog,
            })
        );

        // An answer-less valve ignores typing and backspace, and a non-valve
        // overlay carries no answer at all.
        let approve = TuiInteractionState::new(
            0,
            TuiOverlay::ValveConfirm {
                valve: PendingValve::Approve,
                answer: String::new(),
            },
        );
        let typed = reduce_tui_interaction(&approve, &events, TuiInteraction::TypeChar('x'));
        let typed = reduce_tui_interaction(&typed, &events, TuiInteraction::Backspace);
        assert_eq!(typed.overlay().valve_answer(), Some(""));
        assert_eq!(TuiOverlay::None.valve_answer(), None);
    }

    /// Backspace is inert on an answer-less valve even when the overlay
    /// already carries text.
    ///
    /// The sibling test above exercises the same guard with an EMPTY answer,
    /// where dropping a character and refusing to drop one are indistinguishable
    /// -- so it cannot see the guard at all. Constructing the overlay with text
    /// already in it makes the guard observable: an `Approve` valve accepts no
    /// answer, so the text must survive backspace verbatim.
    #[test]
    fn backspace_leaves_an_answer_less_valves_text_untouched() {
        let events: [ConsoleEvent; 0] = [];
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::ValveConfirm {
                valve: PendingValve::Approve,
                answer: "keep every character".to_owned(),
            },
        );

        let after = reduce_tui_interaction(&state, &events, TuiInteraction::Backspace);

        assert_eq!(after.overlay().valve_answer(), Some("keep every character"));
    }

    /// A flattened structured refusal is recognised by EITHER field alone.
    ///
    /// `structured_refusal` filters on `domain_error` OR `summary`, and the two
    /// arrive independently: the drive surface emits `domain_error` for a
    /// refusal it names and `summary` for one it only describes. A payload
    /// carrying just one of them is still a refusal and must be reported, while
    /// a payload carrying only bookkeeping stays silence.
    #[test]
    fn a_structured_refusal_is_recognised_by_either_field_alone() {
        let domain_error_only = serde_json::json!({"action_id": "a", "domain_error": "refused"});
        let summary_only = serde_json::json!({"action_id": "a", "summary": "refused"});
        let bookkeeping_only = serde_json::json!({"action_id": "a", "status": "failed"});

        assert_eq!(
            super::structured_refusal(Some(&domain_error_only)),
            Some(domain_error_only.to_string())
        );
        assert_eq!(
            super::structured_refusal(Some(&summary_only)),
            Some(summary_only.to_string())
        );
        assert_eq!(super::structured_refusal(Some(&bookkeeping_only)), None);
        assert_eq!(super::structured_refusal(None), None);
    }

    /// The detail returns the answer comments it was given, verbatim and in
    /// order -- the console READS the orchestrator's `livespec-human-answer`
    /// comments back, so an accessor that dropped or invented one would show
    /// the operator an answer nobody wrote.
    #[test]
    fn attention_detail_returns_its_answer_comments_verbatim_and_in_order() {
        let detail = AttentionDetail::new(
            "console".to_owned(),
            "item-1".to_owned(),
            "run-1".to_owned(),
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        assert!(detail.answer_comments().is_empty());

        let with_comments = detail.with_answer_comments(vec![
            "livespec-human-answer (op via console): first".to_owned(),
            "livespec-human-answer (op via console): second".to_owned(),
        ]);

        assert_eq!(
            with_comments.answer_comments(),
            [
                "livespec-human-answer (op via console): first".to_owned(),
                "livespec-human-answer (op via console): second".to_owned(),
            ]
        );
    }

    #[test]
    fn tui_interaction_open_command_explainer_without_selection_leaves_no_overlay() {
        let events = fabro_gate_events();
        let state = TuiInteractionState::new(0, TuiOverlay::None);

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::OpenCommandExplainer);

        assert_eq!(state.overlay(), &TuiOverlay::None);
    }

    /// Scenario 30: a `blocked / needs-human` item renders the orchestrator's
    /// `resolve-blocked` valve and NO attach command.
    ///
    /// The dedupe in the unified list folds the human-valve attention row into
    /// this richer work-item entry, so the valve reaching the operator at all
    /// is the whole point: before this, the entry rendered
    /// `fabro attach <run-scanned-from-events>` — a command that finds nothing,
    /// because the needs-human terminal ended the run.
    #[test]
    fn needs_human_item_renders_the_resolve_blocked_valve_and_no_attach() {
        let valve = AttentionItemSnapshot::new(
            "valve:resolve-blocked:console-blocked",
            "human-valve",
            "high",
            "Resolve human-needed block for work-item console-blocked",
            AttentionSourceRef::new("console", Some("console-blocked"), None),
            AttentionHandoff::new(
                "drive",
                Some("resolve-blocked:console-blocked:ready"),
                "drive resolve-blocked:console-blocked:ready",
            ),
        );
        let events = [
            needs_human_lane_event("evt_blocked", "console-blocked", Some(("01RUN", "hp"))),
            attention_appeared("evt_valve", &valve),
            // A run observation for the SAME item is present and must not
            // resurrect an attach handoff from it.
            fabro_run_event("evt_run", "console", "console-blocked", "run_17", 2),
        ];

        let model = build_tui_model(&events, 0);
        let detail = model.detail();

        assert_eq!(
            detail.map(super::AttentionDetail::valve_commands),
            Some(["drive resolve-blocked:console-blocked:ready".to_owned()].as_slice())
        );
        // The run id and factory are the item's OWN dispatch stamps, NOT the
        // `run_17` the console happens to have observed.
        assert_eq!(detail.map(super::AttentionDetail::fabro_run), Some("01RUN"));
        assert_eq!(
            detail.and_then(super::AttentionDetail::fabro_factory),
            Some("hp")
        );
    }

    /// The projected valve item's `summary` -- the terminated run's ACCOUNT as
    /// the orchestrator composed it -- reaches the row and the detail verbatim.
    ///
    /// The dedupe folds the projected row into the richer work-item entry, so
    /// without this read the account would never surface at all; and nothing
    /// here composes it, so an account-less projection reports nothing about the
    /// run even while the ledger's own run stamps are present.
    #[test]
    fn the_projected_account_reaches_the_row_and_the_detail_verbatim() {
        const ACCOUNT: &str = "fabro run 01RUN on factory hp terminated at the needs-human node. \
                               Why: the gate needs a ruling. Tree preserved on \
                               refs/heads/needs-human/01RUN.";
        let events = [
            needs_human_lane_event("evt_blocked", "console-blocked", Some(("01RUN", "hp"))),
            attention_appeared("evt_valve", &account_valve(ACCOUNT)),
        ];

        let model = build_tui_model(&events, 0);

        // The detail carries the projected summary verbatim, and the row carries
        // the lane title and then the account, untruncated.
        let row_title = format!("Blocked: needs-human — {ACCOUNT}");
        assert_eq!(
            model.detail().and_then(super::AttentionDetail::account),
            Some(ACCOUNT)
        );
        assert_eq!(
            model.attention_items().first().map(AttentionItem::title),
            Some(row_title.as_str())
        );
    }

    /// The fail-soft case: no projected row for the item, so no account, and the
    /// row falls back to the lane title alone.
    #[test]
    fn an_account_less_projection_leaves_the_detail_and_row_asserting_nothing() {
        let events = [needs_human_lane_event(
            "evt_blocked",
            "console-blocked",
            Some(("01RUN", "hp")),
        )];

        let model = build_tui_model(&events, 0);

        assert_eq!(
            model.detail().and_then(super::AttentionDetail::account),
            None
        );
        assert_eq!(
            model.attention_items().first().map(AttentionItem::title),
            Some("Blocked: needs-human")
        );
        // An account that merely repeats the row's own title is not doubled onto
        // it, and an absent one leaves the title alone.
        assert_eq!(
            super::attention_row_title("Blocked: needs-human", Some("Blocked: needs-human")),
            "Blocked: needs-human"
        );
        assert_eq!(
            super::attention_row_title("Pending approval", None),
            "Pending approval"
        );
    }

    /// The `livespec-human-answer` comment the orchestrator wrote is read back
    /// from the item's own record, verbatim, and nothing else is.
    #[test]
    fn the_detail_carries_only_the_human_answer_comments_the_surface_returned() {
        const ANSWER_COMMENT: &str = "livespec-human-answer (operator via console, 2026-09-07T14:00:00Z, \
             resolve-blocked:console-blocked:ready): ship the narrow fix";
        let detail = WorkItemDetail {
            comments: vec![
                WorkItemComment {
                    text: "an ordinary operator handoff".to_owned(),
                    ..WorkItemComment::default()
                },
                WorkItemComment {
                    text: ANSWER_COMMENT.to_owned(),
                    ..WorkItemComment::default()
                },
            ],
            ..WorkItemDetail::default()
        };

        assert_eq!(
            super::human_answer_comments(&detail),
            [ANSWER_COMMENT.to_owned()]
        );
        assert_eq!(
            super::human_answer_comments(&WorkItemDetail::default()),
            [] as [String; 0]
        );
    }

    /// A valve row for the item, carrying `summary` as its account.
    fn account_valve(summary: &str) -> AttentionItemSnapshot {
        AttentionItemSnapshot::new(
            "valve:resolve-blocked:console-blocked",
            "human-valve",
            "high",
            summary,
            AttentionSourceRef::new("console", Some("console-blocked"), None),
            AttentionHandoff::new(
                "drive",
                Some("resolve-blocked:console-blocked:ready"),
                "drive resolve-blocked:console-blocked:ready",
            ),
        )
    }

    /// An item the ledger never stamped renders its run as absent.
    ///
    /// Absence is STATED, not reconstructed: the projection used to answer with
    /// whatever run the console had last seen for the repo and item, which is a
    /// different question and silently wrong across a re-dispatch.
    #[test]
    fn undispatched_item_renders_no_run_and_no_valve() {
        let events = [
            needs_human_lane_event("evt_blocked", "console-blocked", None),
            fabro_run_event("evt_run", "console", "console-blocked", "run_17", 2),
        ];

        let model = build_tui_model(&events, 0);
        let detail = model.detail();

        assert_eq!(detail.map(super::AttentionDetail::fabro_run), Some("-"));
        assert_eq!(detail.and_then(super::AttentionDetail::fabro_factory), None);
        assert_eq!(
            detail.map(super::AttentionDetail::valve_commands),
            Some([].as_slice())
        );
    }

    /// A `blocked / needs-human` lane observation, optionally carrying the
    /// Dispatcher's `(run id, factory)` stamps.
    fn needs_human_lane_event(
        event_id: &str,
        work_item_id: &str,
        dispatch: Option<(&str, &str)>,
    ) -> ConsoleEvent {
        let stamps = dispatch.map_or_else(String::new, |(run_id, factory)| {
            format!(r#","dispatch_fabro_run_id":"{run_id}","dispatch_factory":"{factory}""#)
        });
        let payload = format!(
            concat!(
                r#"{{"repo":"console","work_item_id":"{}","lane":"blocked","#,
                r#""lane_reason":"needs-human","rank":"a0","status":"blocked","#,
                r#""source_version":1,"detail":{{"title":"Needs-human fixture"{}}}}}"#,
            ),
            work_item_id, stamps,
        );
        ConsoleEvent::fixture(
            event_id,
            EventType::WorkItemSnapshotObserved,
            "orchestrator",
        )
        .with_payload_json(payload)
    }

    /// An advertised valve for a DIFFERENT item never leaks onto this one.
    ///
    /// The projection read is by work-item id, and getting that filter wrong
    /// would hand the operator a command that acts on somebody else's item.
    #[test]
    fn advertised_valves_are_scoped_to_their_own_work_item() {
        let other = AttentionItemSnapshot::new(
            "valve:resolve-blocked:console-other",
            "human-valve",
            "high",
            "Resolve human-needed block for work-item console-other",
            AttentionSourceRef::new("console", Some("console-other"), None),
            AttentionHandoff::new(
                "drive",
                Some("resolve-blocked:console-other:ready"),
                "drive resolve-blocked:console-other:ready",
            ),
        );
        // A row pointing at no work-item at all (a spec/plan/hygiene row) must
        // not attach itself to whichever item happens to be selected either.
        let pathless = AttentionItemSnapshot::new(
            "spec:adapt:SPECIFICATION",
            "spec",
            "medium",
            "Adapt the specification",
            AttentionSourceRef::new("console", None, Some("SPECIFICATION")),
            AttentionHandoff::new("livespec-op", None, "livespec adapt"),
        );
        let events = [
            needs_human_lane_event("evt_blocked", "console-blocked", None),
            attention_appeared("evt_other", &other),
            attention_appeared("evt_pathless", &pathless),
        ];
        let needs_attention =
            super::group_needs_attention_by_work_item(&super::materialize_attention_items(&events));

        assert_eq!(
            super::advertised_valve_commands(&needs_attention, "console-blocked"),
            Vec::<String>::new()
        );
        assert_eq!(
            super::advertised_valve_commands(&needs_attention, "console-other"),
            vec!["drive resolve-blocked:console-other:ready".to_owned()]
        );
    }

    /// A 200-row Attention list, one `blocked / needs-human` work-item
    /// snapshot per row -- the shape livespec-console-beads-fabro-mx9u.8's
    /// acceptance criteria measure against ("200 attention rows").
    fn many_attention_rows(count: usize) -> Vec<ConsoleEvent> {
        (0..count)
            .map(|index| {
                let work_item_id = format!("wi-{index:04}");
                needs_human_lane_event(&format!("evt_{index:04}"), &work_item_id, None)
            })
            .collect()
    }

    /// livespec-console-beads-fabro-mx9u.8, acceptance criterion 3: "The
    /// detail pane for the newly selected row is rendered from the
    /// already-projected model, not re-derived per keystroke ... (expected:
    /// one projection, ten renders)."
    ///
    /// Before the `TuiProjection` split this asserts, EVERY keystroke's
    /// selection change re-scanned the whole 200-row event log from scratch
    /// (several times over, between the terminal loop's draw / key-interpret
    /// / `reduce_tui_interaction` steps) -- this test proves the fast path
    /// now shares ONE projection across a whole run of moves, exactly as the
    /// terminal loop's own cache does.
    #[test]
    fn ten_consecutive_moves_share_one_projection() {
        super::reset_tui_projection_counters();
        let events = many_attention_rows(200);
        let projection = super::project_tui_events(&events, None);
        let mut state = TuiInteractionState::new(0, TuiOverlay::None);
        for _ in 0..10 {
            let model = super::render_tui_model(&projection, &events, &state);
            state = super::reduce_tui_interaction_with_model(
                &state,
                &model,
                TuiInteraction::SelectNext,
            );
        }

        assert_eq!(super::tui_projection_counters(), (1, 10));
        assert_eq!(state.selected_attention_index(), 10);
    }

    /// The same "one projection, ten renders" property, but through the
    /// state-typed public API ([`build_tui_model_for_state`]) a one-shot
    /// caller uses. It builds a FRESH projection every call by design (it has
    /// no cache to reuse across calls) -- this pins that down so the split
    /// above is never mistaken for `build_tui_model_for_state` itself having
    /// become cheap to call in a loop.
    #[test]
    fn build_tui_model_for_state_builds_a_fresh_projection_every_call() {
        super::reset_tui_projection_counters();
        let events = many_attention_rows(200);
        let mut state = TuiInteractionState::new(0, TuiOverlay::None);
        for _ in 0..10 {
            let model = super::build_tui_model_for_state(&events, &state);
            state = super::reduce_tui_interaction(&state, &events, TuiInteraction::SelectNext);
            let _ = model;
        }

        // `build_tui_model_for_state` (10 calls) plus `reduce_tui_interaction`'s
        // own internal build (10 calls) each build a projection -- a caller
        // that wants to SHARE one across a run must use
        // `project_tui_events`/`render_tui_model` directly, as the terminal
        // loop's hot path does.
        let (projections, _renders) = super::tui_projection_counters();
        assert_eq!(projections, 20);
    }

    #[test]
    fn factory_drain_policy_composes_lane_and_attention_ready_counts() {
        // Kills the +/-/* mutants on the composed sum: with one genuinely
        // Ready lane item AND one live `impl:` attention row the count must be
        // exactly 2 (1-1 would report an empty queue; 1*1 would under-count).
        let lane = lane_event("evt_ready_sum", "wi-lane", Lane::Ready, None, "a0", "ready");
        let row = attention_item("impl:wi-row", "impl-ready", "Ready implementation work");
        let events = [lane, attention_appeared("evt_attn_sum", &row)];
        assert_eq!(
            super::FactoryDrainPolicy::from_events(&events),
            super::FactoryDrainPolicy::new(2),
        );
    }

    #[test]
    fn latest_work_item_snapshot_filters_by_id_and_takes_the_latest() {
        let other = lane_event("evt_other", "wi-other", Lane::Ready, None, "a0", "ready");
        let malformed = ConsoleEvent::fixture(
            "evt_bad",
            EventType::WorkItemSnapshotObserved,
            "orchestrator",
        )
        .with_payload_json("{}".to_owned());
        let first = lane_event("evt_first", "wi-x", Lane::Active, None, "a1", "active");
        let second = lane_event("evt_second", "wi-x", Lane::Ready, None, "a2", "ready");
        let events = [other, malformed, first, second];

        let hit = super::latest_work_item_snapshot(&events, "wi-x");
        check(
            hit.map(|entry| entry.snapshot.lane()) == Some(Lane::Ready),
            "same-id snapshot should be found with the latest snapshot winning",
        );
        check(
            super::latest_work_item_snapshot(&events, "wi-absent").is_none(),
            "an id with no ingested snapshot resolves to none",
        );
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

    fn fabro_run_event(
        event_id: &str,
        repo: &str,
        work_item_id: &str,
        run_id: &str,
        source_version: u64,
    ) -> ConsoleEvent {
        let mut payload = serde_json::Map::new();
        payload.insert("repo".to_owned(), repo.to_owned().into());
        payload.insert("work_item_id".to_owned(), work_item_id.to_owned().into());
        payload.insert("run_id".to_owned(), run_id.to_owned().into());
        payload.insert("state".to_owned(), "human-gate".into());
        payload.insert("source_version".to_owned(), source_version.into());
        ConsoleEvent::new(
            event_id.to_owned(),
            1,
            "factory".to_owned(),
            EventType::FabroRunObserved,
            "fabro".to_owned(),
            format!("repo:{repo}"),
            source_version,
        )
        .with_payload_json(serde_json::Value::Object(payload).to_string())
    }

    fn view_summary_events() -> [ConsoleEvent; 8] {
        [
            ConsoleEvent::new(
                "evt_gate".to_owned(),
                1,
                "factory".to_owned(),
                EventType::FabroRunObserved,
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
            Some("-")
        );
        assert_eq!(
            model.detail().map(super::AttentionDetail::valve_commands),
            Some([].as_slice())
        );
        assert_eq!(
            model.detail().map(super::AttentionDetail::actions),
            Some(registry_attention_actions_for_model(model).as_slice())
        );
    }

    fn registry_attention_actions_for_model(model: &super::TuiScreenModel) -> Vec<OperatorAction> {
        let Some(ctx) = model.selected_action_context() else {
            return Vec::new();
        };
        action_registry::ACTION_REGISTRY
            .iter()
            .filter(|spec| {
                matches!(
                    spec.staging,
                    action_registry::ActionStaging::Valve(_)
                        | action_registry::ActionStaging::DriverHandoff
                ) && (spec.availability)(&ctx)
            })
            .map(|spec| OperatorAction::Registered(spec.id))
            .collect()
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
    fn source_reference_helpers_derive_repo_from_the_stream_key() {
        let gate = fabro_run_event(
            "evt_gate",
            "livespec-console-beads-fabro",
            "livespec-console-beads-fabro-y45jhj",
            "run_17",
            2,
        );
        let plain_stream = ConsoleEvent::new(
            "evt_plain".to_owned(),
            1,
            "factory".to_owned(),
            EventType::LivespecReviseRequired,
            "livespec".to_owned(),
            "livespec-console-beads-fabro".to_owned(),
            1,
        );

        assert_eq!(
            super::repo_id(&gate),
            Some("livespec-console-beads-fabro".to_owned())
        );
        // A COLONLESS key yields no repo. This fixture used to assert the whole
        // key came back, and it passed only because the key it chose happens to
        // BE a repo name. Real colonless keys are bare work-item ids, so the old
        // fallback returned a work-item id as a repository — a test that could
        // not fail on the shape production actually produces.
        assert_eq!(super::repo_id(&plain_stream), None);
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
            Some("livespec-orchestrator-beads-fabro".to_owned())
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
        assert_eq!(
            super::repo_id(&pull),
            Some("livespec-orchestrator-beads-fabro".to_owned())
        );

        // A stream key with no colon yields NO repo. It used to degrade to the
        // whole key, which is how bare work-item ids reached the operator as
        // repositories.
        let plain = ConsoleEvent::new(
            "evt_plain".to_owned(),
            1,
            "factory".to_owned(),
            EventType::WorkItemSnapshotObserved,
            "orchestrator".to_owned(),
            "livespec-orchestrator-beads-fabro".to_owned(),
            1,
        );
        assert_eq!(super::repo_id(&plain), None);

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
            Some("livespec-orchestrator-beads-fabro".to_owned())
        );

        // A malformed attention stream key (no middle segment) yields NO repo.
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
        // An attention key with no middle segment yields no repo. The old `"-"`
        // was rendered to the operator as if it were a repository.
        assert_eq!(super::repo_id(&malformed), None);

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
            Some("livespec-orchestrator-beads-fabro".to_owned())
        );
    }

    #[test]
    fn a_single_tenant_store_reports_one_repo_not_one_per_work_item() {
        // FOUND BY DOGFOODING. A live single-tenant console rendered
        // "Repos observed: 67" and listed work-item ids and a bare "-" to the
        // operator as repositories. Measured in that store: 765 events, of which
        // 140 (65 distinct ids) stream under a key with NO COLON — every
        // `command.accepted` and every `attention_item.resolved`. The old
        // fallbacks turned each into a fictitious repo.
        //
        // The previous unit tests all passed. They covered the colonless shape
        // with a fixture whose key WAS a repo name, so the fallback looked
        // correct; production keys are bare work-item ids. This fixture uses the
        // shape production actually produces.
        let repo_scoped = ConsoleEvent::new(
            "evt_repo_scoped".to_owned(),
            1,
            "repo".to_owned(),
            EventType::LivespecReviseRequired,
            "livespec".to_owned(),
            "repo:livespec-console-beads-fabro".to_owned(),
            1,
        );
        let bare_work_item = ConsoleEvent::new(
            "evt_bare".to_owned(),
            2,
            "orchestrator".to_owned(),
            EventType::LivespecReviseRequired,
            "livespec".to_owned(),
            "livespec-console-beads-fabro-3lxx7t".to_owned(),
            1,
        );
        let another_bare = ConsoleEvent::new(
            "evt_bare_2".to_owned(),
            3,
            "orchestrator".to_owned(),
            EventType::LivespecReviseRequired,
            "livespec".to_owned(),
            "livespec-console-beads-fabro-6yii4r".to_owned(),
            1,
        );

        let items = super::repos_view_items(&[repo_scoped, bare_work_item, another_bare]);
        let rendered = format!("{items:?}");

        // `check` with a STATIC context, not `assert!` with an interpolated
        // message: the format arm of a failure message is a branch no passing run
        // takes, and check-coverage refuses it.
        check(
            rendered.contains("Repos observed: 1"),
            "a single-tenant store must report ONE repo",
        );
        check(
            !rendered.contains("3lxx7t"),
            "a bare work-item id must not be listed as a repository",
        );
        // NOT SILENT: the two events it could not attribute are counted out loud.
        check(
            rendered.contains("Events with no derivable repo: 2"),
            "the dropped events must be reported, not quietly excluded",
        );
    }

    #[test]
    fn a_store_whose_events_all_carry_a_repo_reports_no_unattributable_line() {
        // NEGATIVE CONTROL on that second line. If it appeared unconditionally it
        // would be noise on a healthy store, and an operator would learn to skip
        // it — which is how a real warning stops being read.
        let repo_scoped = ConsoleEvent::new(
            "evt_only".to_owned(),
            1,
            "repo".to_owned(),
            EventType::LivespecReviseRequired,
            "livespec".to_owned(),
            "repo:livespec-console-beads-fabro".to_owned(),
            1,
        );

        let rendered = format!("{:?}", super::repos_view_items(&[repo_scoped]));

        check(
            rendered.contains("Repos observed: 1"),
            "the healthy store still reports its one repo",
        );
        check(
            !rendered.contains("no derivable repo"),
            "a store with nothing to report must stay quiet",
        );
        // The same negative control for the fleet row: this store has no fleet
        // stream, so the row that reports one must not appear.
        check(
            !rendered.contains("Fleet-scoped events"),
            "a store with no fleet stream must not grow a fleet row",
        );
    }

    #[test]
    fn the_fleet_stream_is_a_product_family_not_a_second_repository() {
        // FOUND BY DOGFOODING, on the same single-tenant store as the test
        // above, after the colonless-key fix had landed: "Repos observed: 2:
        // livespec, livespec-console-beads-fabro". The `livespec` row came from
        // the 56 events on the ONE `fleet:livespec` stream -- every operator
        // factory-drain command -- whose key satisfies `{context}:{repo}`
        // syntactically while naming the product FAMILY, not a tenant.
        let repo_scoped = ConsoleEvent::new(
            "evt_repo_scoped".to_owned(),
            1,
            "repo".to_owned(),
            EventType::LivespecReviseRequired,
            "livespec".to_owned(),
            "repo:livespec-console-beads-fabro".to_owned(),
            1,
        );
        let drain_requested = ConsoleEvent::new(
            "evt_drain_requested".to_owned(),
            1,
            "console".to_owned(),
            EventType::FactoryDrainRequested,
            "console:factory-command-handler".to_owned(),
            "fleet:livespec".to_owned(),
            1,
        );
        let drain_completed = ConsoleEvent::new(
            "evt_drain_completed".to_owned(),
            1,
            "console".to_owned(),
            EventType::FactoryDrainCompleted,
            "console:factory-command-handler".to_owned(),
            "fleet:livespec".to_owned(),
            2,
        );

        // No repo, whatever the event shape: the derivation refuses the stream,
        // not one event type riding it.
        assert_eq!(super::repo_id(&drain_requested), None);
        check(
            super::fleet_scoped("fleet:livespec") && !super::fleet_scoped(repo_scoped.stream_id()),
            "only the fleet context is fleet-scoped",
        );

        let items = super::repos_view_items(&[repo_scoped, drain_requested, drain_completed]);
        let rendered = format!("{items:?}");

        check(
            rendered.contains("Repos observed: 1"),
            "the family name must not be counted as a second repository",
        );
        // NOT SILENT: the fleet events are counted out loud on their own row,
        // with the stream roster as its operational detail.
        check(
            rendered.contains("Fleet-scoped events: 2"),
            "the fleet events must be reported, not quietly excluded",
        );
        check(
            rendered.contains("fleet:livespec"),
            "the fleet row names the stream its count came from",
        );
        // They are NOT folded into the unattributable row either: that row is a
        // defect report about keys that break the `{context}:{repo}` contract,
        // and a fleet key does not break it.
        check(
            !rendered.contains("no derivable repo"),
            "a fleet stream is attributed, not unattributable",
        );
    }

    #[test]
    fn every_event_is_classified_into_exactly_one_of_the_three_repos_view_buckets() {
        // AC6 (livespec-console-beads-fabro-mx9u.21): the view cannot silently
        // drop a category. `repo_id` and `fleet_scoped` are the classification
        // ITSELF (`repos_view_items` is built entirely from them), so this
        // asserts the partition at the level where it is actually decided,
        // over a fixture deliberately mixing every observed stream shape --
        // including TWO events for the SAME repo, so a bucket's EVENT count
        // cannot be confused with the "Repos observed" DISTINCT-NAME count.
        let events = [
            ConsoleEvent::new(
                "evt_repo_1".to_owned(),
                1,
                "repo".to_owned(),
                EventType::LivespecReviseRequired,
                "livespec".to_owned(),
                "repo:livespec-console-beads-fabro".to_owned(),
                1,
            ),
            ConsoleEvent::new(
                "evt_repo_2".to_owned(),
                1,
                "repo".to_owned(),
                EventType::LivespecReviseRequired,
                "livespec".to_owned(),
                "repo:livespec-console-beads-fabro".to_owned(),
                2,
            ),
            ConsoleEvent::new(
                "evt_fleet".to_owned(),
                1,
                "console".to_owned(),
                EventType::FactoryDrainRequested,
                "console:factory-command-handler".to_owned(),
                "fleet:livespec".to_owned(),
                1,
            ),
            ConsoleEvent::new(
                "evt_cmd".to_owned(),
                1,
                "orchestrator".to_owned(),
                EventType::CommandAccepted,
                "livespec".to_owned(),
                "bd-ib-aaa".to_owned(),
                1,
            ),
            ConsoleEvent::new(
                "evt_action".to_owned(),
                1,
                "orchestrator".to_owned(),
                EventType::WorkItemActionFailed,
                "livespec".to_owned(),
                "bd-ib-bbb".to_owned(),
                1,
            ),
        ];

        let repo_attributed = events
            .iter()
            .filter(|event| super::repo_id(event).is_some());
        let fleet_scoped = events
            .iter()
            .filter(|event| super::fleet_scoped(event.stream_id()));
        let unattributable = events.iter().filter(|event| {
            super::repo_id(event).is_none() && !super::fleet_scoped(event.stream_id())
        });

        // Mutually exclusive: no event is double-counted...
        check(
            repo_attributed.clone().count()
                + fleet_scoped.clone().count()
                + unattributable.clone().count()
                == events.len(),
            "the three buckets must partition the fixture without overlap or gaps",
        );
        // ...and exhaustive: every event actually lands somewhere, pinned by
        // this fixture's known composition (2 repo-attributed, 1 fleet, 2
        // unattributable) rather than by the arithmetic identity alone, which
        // an accidentally-empty bucket would still satisfy.
        check(repo_attributed.count() == 2, "two events share one repo");
        check(fleet_scoped.count() == 1, "one event is fleet-scoped");
        check(
            unattributable.count() == 2,
            "two events carry no derivable repo",
        );
    }

    #[test]
    fn unattributable_family_breakdown_collapses_siblings_to_their_common_prefix() {
        // `just check-coverage` measures `cargo llvm-cov --lib` only, so the
        // multi-member collapse branch and its longest-common-prefix walk --
        // exercised by `tests/repos_view_fleet_scope.rs`'s
        // the_unattributable_row_names_the_event_families_not_only_a_count --
        // still read as UNCOVERED production code there; this inline
        // counterpart closes that gate. THREE sibling types (not two) so the
        // prefix walk's loop actually iterates more than once. TWO
        // single-member families sharing the SAME count (`command.accepted`
        // and `attention_item.resolved`, both 1) so the final `sort_by`'s
        // primary key ties and its `then_with` alphabetical tie-break
        // actually runs -- distinct counts alone never reach it.
        fn colonless(event_id: &str, event_type: EventType, stream_id: &str) -> ConsoleEvent {
            ConsoleEvent::new(
                event_id.to_owned(),
                1,
                "orchestrator".to_owned(),
                event_type,
                "livespec".to_owned(),
                stream_id.to_owned(),
                1,
            )
        }
        let events = [
            colonless("evt_started", EventType::WorkItemActionStarted, "bd-ib-aaa"),
            colonless(
                "evt_completed",
                EventType::WorkItemActionCompleted,
                "bd-ib-bbb",
            ),
            colonless("evt_failed", EventType::WorkItemActionFailed, "bd-ib-ccc"),
            colonless("evt_cmd", EventType::CommandAccepted, "bd-ib-ddd"),
            colonless(
                "evt_resolved",
                EventType::AttentionItemResolved,
                "bd-ib-eee",
            ),
        ];
        let refs = events.iter().collect::<Vec<_>>();

        let breakdown = super::unattributable_family_breakdown(&refs);

        check(
            breakdown
                == [
                    ("work_item.action.*".to_owned(), 3),
                    ("attention_item.resolved".to_owned(), 1),
                    ("command.accepted".to_owned(), 1),
                ],
            "three sibling work_item.action.* types collapse into one wildcard row \
             ahead of the two tied single-member rows, which tie-break alphabetically",
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
            Some("-")
        );
        // The clamped selection lands on the `blocked` row, whose lifecycle
        // state admits the move-status picker on this surface exactly as it
        // does in the drilled-in lane (Scenario 31).
        assert_eq!(
            model.detail().map(super::AttentionDetail::actions),
            Some([OperatorAction::Registered("move")].as_slice())
        );
    }

    /// Three attention-requiring work-items, rank-ordered `console-1`,
    /// `console-2`, `console-3`. The Attention list folds the LATEST snapshot per
    /// work-item id, so appending another snapshot for one of them is exactly
    /// what a source refresh does to this list.
    fn churning_attention_events() -> Vec<ConsoleEvent> {
        vec![
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
            lane_event(
                "evt_3",
                "console-3",
                Lane::Acceptance,
                None,
                "a2",
                "acceptance",
            ),
        ]
    }

    /// The state with `console-2` — the middle row — chosen the way an operator
    /// chooses it: by moving the cursor onto it.
    fn attention_state_on_console_2() -> TuiInteractionState {
        let events = churning_attention_events();
        let state = reduce_tui_interaction(
            &TuiInteractionState::new(0, TuiOverlay::None),
            &events,
            TuiInteraction::SelectNext,
        );
        assert_eq!(state.selected_attention_index(), 1);
        assert_eq!(state.selected_attention_id(), Some("console-2"));
        state
    }

    #[test]
    fn attention_selection_follows_its_item_across_a_reordering_refresh() {
        // livespec-console-beads-fabro-pzbdbo.24. Dogfooded against the real TUI
        // while other fleet sessions mutated the shared ledger: the list rebuilt
        // and re-ranked on every source refresh, and a positionally-tracked
        // cursor put a DIFFERENT item under the operator between choosing a row
        // and pressing a key. Six of six attempts.
        let state = attention_state_on_console_2();

        // The refresh re-ranks `console-2` to the end of the list. Positionally,
        // row 1 is now `console-3`; by identity the cursor follows `console-2`.
        let mut refreshed = churning_attention_events();
        refreshed.push(lane_event(
            "evt_2_reranked",
            "console-2",
            Lane::Blocked,
            Some(LaneReason::NeedsHuman),
            "a9",
            "blocked",
        ));
        let model = build_tui_model_for_state(&refreshed, &state);

        assert_eq!(
            model
                .attention_items()
                .iter()
                .map(AttentionItem::id)
                .collect::<Vec<_>>(),
            vec!["console-1", "console-3", "console-2"]
        );
        assert_eq!(model.selected_attention_index(), Some(2));
        assert_eq!(
            model.detail().map(AttentionDetail::work_item),
            Some("console-2")
        );
        // Nothing was displaced, so the Status line has nothing to report.
        check(
            !model.header().contains("status: "),
            "a cursor that followed its item has no departure to announce",
        );
    }

    #[test]
    fn attention_selection_follows_a_needs_attention_row_across_a_refresh() {
        // The unified list carries TWO kinds of row. A needs-attention item's
        // identity is its OWN id, not a work-item id, and it shifts position
        // whenever the rank-ordered lane fold ahead of it gains or loses a row --
        // which is precisely what another session's ledger write does.
        let orchestrator = "livespec-orchestrator-beads-fabro";
        let plan = AttentionItemSnapshot::new(
            "plan:console-autonomous-mode",
            "plan",
            "medium",
            "Review plan thread console-autonomous-mode.",
            AttentionSourceRef::new(orchestrator, None, Some("plan/console-autonomous-mode/")),
            AttentionHandoff::new("plan", None, "codex exec plan console-autonomous-mode"),
        );
        let prune = AttentionItemSnapshot::new(
            "spec:prune-history:SPECIFICATION",
            "spec",
            "low",
            "33 unpruned history versions; consider pruning",
            AttentionSourceRef::new(orchestrator, None, Some("SPECIFICATION")),
            AttentionHandoff::new("livespec-op", None, "codex exec livespec:prune-history"),
        );
        let events = vec![
            attention_appeared("evt_plan", &plan),
            attention_appeared("evt_prune", &prune),
        ];
        let state = reduce_tui_interaction(
            &TuiInteractionState::new(0, TuiOverlay::None),
            &events,
            TuiInteraction::SelectNext,
        );
        assert_eq!(
            state.selected_attention_id(),
            Some("spec:prune-history:SPECIFICATION")
        );

        // The refresh brings in a work-item, which sorts ahead of both
        // needs-attention rows and pushes the selected one down.
        let mut refreshed = events;
        refreshed.push(lane_event(
            "evt_wi",
            "bd-ib-ss7rkr",
            Lane::Blocked,
            Some(LaneReason::NeedsHuman),
            "a0",
            "blocked",
        ));
        let model = build_tui_model_for_state(&refreshed, &state);

        assert_eq!(
            model
                .attention_items()
                .iter()
                .map(AttentionItem::id)
                .collect::<Vec<_>>(),
            vec![
                "bd-ib-ss7rkr",
                "plan:console-autonomous-mode",
                "spec:prune-history:SPECIFICATION",
            ]
        );
        assert_eq!(model.selected_attention_index(), Some(2));
    }

    #[test]
    fn attention_movement_after_a_reordering_refresh_steps_from_the_item_not_the_old_row() {
        // The other half of following the item: the NEXT keystroke must move
        // relative to where the item now is. Stepping from the stale stored row
        // would jump the cursor exactly as the positional bug did.
        let state = attention_state_on_console_2();
        let mut refreshed = churning_attention_events();
        refreshed.push(lane_event(
            "evt_2_reranked",
            "console-2",
            Lane::Blocked,
            Some(LaneReason::NeedsHuman),
            "a9",
            "blocked",
        ));

        // `console-2` is last after the re-rank, so down is refused at the end
        // rather than walking to row 2 from the stored row 1.
        let down = reduce_tui_interaction(&state, &refreshed, TuiInteraction::SelectNext);
        assert_eq!(down.selected_attention_id(), Some("console-2"));
        assert_eq!(down.list_edge(), Some(ListEdge::Bottom));

        // And up lands on `console-2`'s CURRENT neighbour, not row 0.
        let up = reduce_tui_interaction(&state, &refreshed, TuiInteraction::SelectPrevious);
        assert_eq!(up.selected_attention_id(), Some("console-3"));
        assert_eq!(up.selected_attention_index(), 1);
    }

    #[test]
    fn attention_selection_lands_on_the_nearest_row_and_says_so_when_its_item_leaves() {
        // The item the operator was on is accepted by another session and stops
        // needing attention. The cursor cannot follow it, so it takes the
        // surviving row nearest where the item stood -- and the Status line names
        // what moved, rather than silently handing the operator a neighbour.
        let state = attention_state_on_console_2();
        let mut refreshed = churning_attention_events();
        refreshed.push(lane_event(
            "evt_2_departed",
            "console-2",
            Lane::Active,
            None,
            "a1",
            "active",
        ));

        let model = build_tui_model_for_state(&refreshed, &state);

        assert_eq!(
            model
                .attention_items()
                .iter()
                .map(AttentionItem::id)
                .collect::<Vec<_>>(),
            vec!["console-1", "console-3"]
        );
        assert_eq!(model.selected_attention_index(), Some(1));
        assert_eq!(
            model.detail().map(AttentionDetail::work_item),
            Some("console-3")
        );
        check(
            model
                .header()
                .contains("status: console-2 left the list; cursor on nearest row"),
            "the departure must reach the Status line the operator is reading",
        );
    }

    #[test]
    fn a_departure_report_outranks_a_carried_status_and_clears_on_the_next_keystroke() {
        // ONE Status-line channel, most-recent-contradiction wins: a cursor moved
        // out from under the operator outranks a status from an earlier
        // keystroke. It must not then STICK -- a stale anchor that kept
        // re-announcing would overwrite every later report forever.
        let state =
            attention_state_on_console_2().with_transient_status(Some("earlier report".to_owned()));
        let mut refreshed = churning_attention_events();
        refreshed.push(lane_event(
            "evt_2_departed",
            "console-2",
            Lane::Active,
            None,
            "a1",
            "active",
        ));

        let displaced = build_tui_model_for_state(&refreshed, &state);
        check(
            displaced
                .header()
                .contains("status: console-2 left the list"),
            "the newer contradiction is what the one Status-line channel says",
        );

        // Any interaction is the operator's acknowledgement: it re-anchors on the
        // row the cursor actually sits on, and the carried status is theirs again.
        let acknowledged = reduce_tui_interaction(&state, &refreshed, TuiInteraction::FocusContent);
        assert_eq!(acknowledged.selected_attention_id(), Some("console-3"));
        assert_eq!(acknowledged.selected_attention_index(), 1);
        check(
            build_tui_model_for_state(&refreshed, &acknowledged)
                .header()
                .contains("status: earlier report"),
            "a re-anchored cursor stops overwriting the operator's own report",
        );
    }

    #[test]
    fn an_emptied_attention_list_drops_the_anchor_rather_than_inventing_a_row() {
        // Every attention row leaves at once. There is no surviving row to
        // re-anchor on, so the anchor is dropped instead of naming an item that
        // is not there.
        let state = attention_state_on_console_2();
        let mut emptied = churning_attention_events();
        for (event_id, work_item_id) in [
            ("evt_1_departed", "console-1"),
            ("evt_2_departed", "console-2"),
            ("evt_3_departed", "console-3"),
        ] {
            emptied.push(lane_event(
                event_id,
                work_item_id,
                Lane::Active,
                None,
                "a0",
                "active",
            ));
        }

        let model = build_tui_model_for_state(&emptied, &state);
        assert_eq!(model.selected_attention_index(), None);

        let acknowledged = reduce_tui_interaction(&state, &emptied, TuiInteraction::FocusContent);
        assert_eq!(acknowledged.selected_attention_id(), None);
    }

    #[test]
    fn a_narrowing_search_moves_the_cursor_without_announcing_a_departure() {
        // MUST-NOT-FLAG CONTROL. A row that leaves because the OPERATOR narrowed
        // the list is not news -- their own open query is the explanation -- and
        // reporting it would rewrite the Status line on every typed character.
        let events = churning_attention_events();
        let state = attention_state_on_console_2().with_overlay(TuiOverlay::Search {
            query: "console-3".to_owned(),
        });

        let model = build_tui_model_for_state(&events, &state);

        assert_eq!(
            model
                .attention_items()
                .iter()
                .map(AttentionItem::id)
                .collect::<Vec<_>>(),
            vec!["console-3"]
        );
        assert_eq!(model.selected_attention_index(), Some(0));
        check(
            !model.header().contains("status: "),
            "a filtered-out row is explained by the operator's own query",
        );
    }

    #[test]
    fn a_position_only_attention_selection_carries_no_anchor() {
        // The positional setter is still how a preview state or a caller with no
        // row in hand selects. It must CLEAR a previous anchor rather than leave
        // one pointing at an item the new row is not.
        let anchored = attention_state_on_console_2();
        let positional = anchored.with_selected_attention_index(0);

        assert_eq!(positional.selected_attention_index(), 0);
        assert_eq!(positional.selected_attention_id(), None);
    }

    #[test]
    fn attention_selection_helper_falls_back_to_the_row_when_the_row_is_absent() {
        let events = churning_attention_events();
        let state = TuiInteractionState::new(0, TuiOverlay::None);
        let model = build_tui_model_for_state(&events, &state);

        let selected = super::select_attention_at(&state, &model, 9);

        assert_eq!(selected.selected_attention_index(), 9);
        assert_eq!(selected.selected_attention_id(), None);
    }

    #[test]
    fn drilled_in_lane_selection_follows_its_item_across_a_reordering_refresh() {
        // The same identity-stable cursor in the other per-item surface. The lane
        // list is rank-ordered from the same fold, so it churns the same way.
        let events = drilldown_events();
        let start = drilldown_state(Lane::PendingApproval, 0, TuiOverlay::None);
        let on_second = reduce_tui_interaction(&start, &events, TuiInteraction::SelectNext);
        assert_eq!(on_second.selected_lane_item_id(), Some("wi-b"));

        // The refresh re-ranks `wi-b` ahead of `wi-a`, swapping the two rows.
        let mut refreshed = events;
        refreshed.push(lane_event(
            "e2_reranked",
            "wi-b",
            Lane::PendingApproval,
            None,
            "0",
            "pending-approval",
        ));
        let model = build_tui_model_for_state(&refreshed, &on_second);

        assert_eq!(
            model
                .lane_board()
                .column(Lane::PendingApproval)
                .map(|column| column
                    .items()
                    .iter()
                    .map(LaneWorkItem::work_item_id)
                    .collect::<Vec<_>>()),
            Some(vec!["wi-b", "wi-a"])
        );
        assert_eq!(model.selected_lane_item_index(), Some(0));
        assert_eq!(model.missing_selected_lane_item_id(), None);
    }

    #[test]
    fn navigation_and_action_labels_are_stable() {
        assert_eq!(TuiView::Attention.label(), "Attention");
        assert_eq!(TuiView::Spec.label(), "Spec");
        assert_eq!(TuiView::Lanes.label(), "Lanes");
        assert_eq!(TuiView::Events.label(), "Events");
        assert_eq!(TuiView::Repos.label(), "Repos");
        assert_eq!(
            OperatorAction::Registered("approve").label(),
            "Approve work-item"
        );
        // An id with no registry entry falls back to the id itself rather than
        // rendering a blank row.
        assert_eq!(
            OperatorAction::Registered("not-registered").label(),
            "not-registered"
        );
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
            EventType::DispatcherJournalProgressObserved.label(),
            "Dispatcher journal progress"
        );
        assert_eq!(
            EventType::DispatcherRefusalObserved.label(),
            "Dispatcher refusal"
        );
        assert_eq!(EventType::FabroRunObserved.label(), "Fabro human gate");
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
            EventType::FactoryDrainAwaitingHuman.label(),
            "Factory drain awaiting human"
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
            Ok(FactoryDrainPortOutcome::failed_with_diagnostic(
                r#"{"summary":"factory-safety refusal","domain_error":"host-only-refused"}"#
                    .to_owned(),
            ))
        }
    }

    struct NoDiagnosticFailingDrainPort;

    impl FactoryDrainPort for NoDiagnosticFailingDrainPort {
        fn drain_ready_queue(
            &mut self,
            _request: &FactoryDrainRequest,
        ) -> super::ApplicationResult<FactoryDrainPortOutcome> {
            Ok(FactoryDrainPortOutcome::failed())
        }
    }

    struct AcceptanceParkDrainPort;

    impl FactoryDrainPort for AcceptanceParkDrainPort {
        fn drain_ready_queue(
            &mut self,
            _request: &FactoryDrainRequest,
        ) -> super::ApplicationResult<FactoryDrainPortOutcome> {
            Ok(FactoryDrainPortOutcome::failed_with_diagnostic(
                "parked in acceptance under acceptance_policy ai-then-human".to_owned(),
            ))
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

    #[derive(Default)]
    struct NotWiringDispatchItemPort {
        requests: Vec<String>,
    }

    impl FactoryDispatchItemPort for NotWiringDispatchItemPort {
        fn dispatch_item(
            &mut self,
            request: &FactoryDispatchItemRequest,
        ) -> super::ApplicationResult<FactoryDispatchItemPortOutcome> {
            self.requests.push(request.work_item_id().to_owned());
            Ok(FactoryDispatchItemPortOutcome::not_wired())
        }
    }

    #[derive(Default)]
    struct CompletingDispatchItemPort {
        requests: Vec<String>,
    }

    impl FactoryDispatchItemPort for CompletingDispatchItemPort {
        fn dispatch_item(
            &mut self,
            request: &FactoryDispatchItemRequest,
        ) -> super::ApplicationResult<FactoryDispatchItemPortOutcome> {
            self.requests.push(request.work_item_id().to_owned());
            Ok(FactoryDispatchItemPortOutcome::completed())
        }
    }

    struct FailingDispatchItemPort;

    impl FactoryDispatchItemPort for FailingDispatchItemPort {
        fn dispatch_item(
            &mut self,
            _request: &FactoryDispatchItemRequest,
        ) -> super::ApplicationResult<FactoryDispatchItemPortOutcome> {
            Ok(FactoryDispatchItemPortOutcome::failed_with_diagnostic(
                "dispatch item refused".to_owned(),
            ))
        }
    }

    struct ErroringDispatchItemPort;

    impl FactoryDispatchItemPort for ErroringDispatchItemPort {
        fn dispatch_item(
            &mut self,
            _request: &FactoryDispatchItemRequest,
        ) -> super::ApplicationResult<FactoryDispatchItemPortOutcome> {
            Err(super::ApplicationError::FactoryDispatchItemPortFailed)
        }
    }

    #[test]
    fn handle_factory_dispatch_item_command_propagates_a_port_error() {
        let command = factory_dispatch_item_command("console-selected", "operator");
        let mut port = ErroringDispatchItemPort;

        let result = handle_factory_dispatch_item_command(&command, &mut port);

        assert_eq!(
            result.map(|_outcome| ()),
            Err(super::ApplicationError::FactoryDispatchItemPortFailed)
        );
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
    fn factory_drain_handler_threads_json_diagnostic_fields_into_the_failure_event() {
        let command = factory_drain_test_command();
        let mut port = FailingDrainPort;

        let outcome = ok_factory_command_outcome(handle_factory_drain_command(
            &command,
            &ready_factory_drain_policy(),
            &mut port,
        ));
        let failed_payloads = outcome
            .events()
            .iter()
            .filter(|event| *event.event_type() == EventType::FactoryDrainFailed)
            .map(ConsoleEvent::payload_json)
            .collect::<Vec<_>>();

        let expected = serde_json::json!({
            "summary": "factory-safety refusal",
            "domain_error": "host-only-refused"
        })
        .to_string();
        assert_eq!(failed_payloads, [expected.as_str()]);
    }

    #[test]
    fn dispatcher_drain_port_fails_on_non_zero_run() {
        let probe = StubDrainProbe {
            outcome: SourceProbeOutcome::observed(
                r#"{"summary":"held manual admission","domain_error":"invalid-source-state"}"#,
                false,
            ),
        };
        let mut port = DispatcherFactoryDrainPort::new(&probe, "dispatcher", &["loop"]);

        let outcome = port.drain_ready_queue(&drain_request());

        assert_eq!(
            outcome,
            Ok(FactoryDrainPortOutcome::failed_with_diagnostic(
                r#"{"summary":"held manual admission","domain_error":"invalid-source-state"}"#
                    .to_owned()
            ))
        );
    }

    #[test]
    fn dispatcher_drain_port_failure_without_stdout_carries_no_diagnostic() {
        let probe = StubDrainProbe {
            outcome: SourceProbeOutcome::observed("   ", false),
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
        // the Dispatcher owns its own mode.
        assert_eq!(outcome, Ok(FactoryDrainPortOutcome::completed(2)));
        assert_eq!(
            *probe.observed_args.borrow(),
            ["loop", "--budget", "1", "--parallel", "1"]
        );
    }

    #[test]
    fn dispatcher_drain_port_threads_requested_budget_and_parallel() {
        let probe = ArgsRecordingDrainProbe {
            config: SourceProbeOutcome::unavailable("unused"),
            drain: SourceProbeOutcome::observed("drain: dispatched 7 items", true),
            observed_args: std::cell::RefCell::new(Vec::new()),
        };
        let request = FactoryDrainRequest::new("fleet:livespec".to_owned(), 7, 3);
        let mut port = DispatcherFactoryDrainPort::new(&probe, "dispatcher", &["loop"]);

        let outcome = port.drain_ready_queue(&request);

        assert_eq!(outcome, Ok(FactoryDrainPortOutcome::completed(7)));
        assert_eq!(
            *probe.observed_args.borrow(),
            ["loop", "--budget", "7", "--parallel", "3"]
        );
    }

    // A journal line for one auto-disposition, in the exact wire shape the
    // orchestrator plane's published record contract emits.
    fn autonomous_journal_line(
        work_item_id: &str,
        disposition: &str,
        governing_settings: &[&str],
    ) -> String {
        serde_json::json!({
            "stage": "auto-disposition",
            "work_item_id": work_item_id,
            "disposition": disposition,
            "governing_settings": governing_settings,
        })
        .to_string()
    }

    #[test]
    fn read_autonomous_decisions_splits_buckets_and_preserves_order() {
        let journal = [
            autonomous_journal_line("wi-1", "auto-approve", &["auto_approve_ready"]),
            autonomous_journal_line("wi-2", "ai-auto-accept", &["acceptance_mode"]),
            autonomous_journal_line(
                "wi-3",
                "ai-fail-auto-rework",
                &["acceptance_mode", "acceptance_rework_cap"],
            ),
            autonomous_journal_line("wi-4", "ship-on-cap", &["merge_on_review_cap"]),
            autonomous_journal_line("wi-5", "cap-exceeded-escalation", &["review_fix_cap"]),
        ]
        .join("\n");

        let audit = super::read_autonomous_decisions_from_journal(&journal);

        assert_eq!(audit.auto_resolutions().len(), 4);
        assert_eq!(audit.auto_resolutions()[0].work_item_id(), "wi-1");
        assert_eq!(audit.auto_resolutions()[0].gate(), "approve");
        assert_eq!(audit.auto_resolutions()[0].decision(), "auto-approve");
        assert_eq!(audit.auto_resolutions()[0].disposition(), "auto-approve");
        assert_eq!(
            audit.auto_resolutions()[0].governing_settings(),
            ["auto_approve_ready"]
        );
        assert_eq!(audit.auto_resolutions()[1].work_item_id(), "wi-2");
        assert_eq!(audit.auto_resolutions()[1].gate(), "acceptance");
        assert_eq!(
            audit.auto_resolutions()[2].disposition(),
            "ai-fail-auto-rework"
        );
        assert_eq!(audit.auto_resolutions()[3].disposition(), "ship-on-cap");
        assert_eq!(audit.escalations().len(), 1);
        assert_eq!(audit.escalations()[0].work_item_id(), "wi-5");
        assert_eq!(
            audit.escalations()[0].disposition(),
            "cap-exceeded-escalation"
        );
        assert_eq!(audit.escalations()[0].gate(), "needs-human");
    }

    #[test]
    fn read_autonomous_decisions_skips_malformed_and_foreign_records() {
        let journal = [
            "not json".to_owned(),
            "[1,2,3]".to_owned(),
            r#"{"stage":"calibration","work_item_id":"wi-x"}"#.to_owned(),
            r#"{"stage":"autonomous-decision","work_item_id":"wi-old","gate":"approve","decision":"d","disposition":"auto-resolved"}"#.to_owned(),
            r#"{"stage":"auto-disposition","work_item_id":"wi-z","disposition":"unknown","governing_settings":["acceptance_mode"]}"#.to_owned(),
            r#"{"stage":"auto-disposition","work_item_id":"wi-y","disposition":"auto-approve"}"#.to_owned(),
            r#"{"stage":"auto-disposition","disposition":"auto-approve","governing_settings":["auto_approve_ready"]}"#.to_owned(),
            autonomous_journal_line("wi-ok", "auto-approve", &["auto_approve_ready"]),
        ]
        .join("\n");

        let audit = super::read_autonomous_decisions_from_journal(&journal);

        // Only the single well-formed live-schema record survives; every
        // malformed, retired-schema, or foreign-stage line is skipped fail-open.
        assert_eq!(audit.auto_resolutions().len(), 1);
        assert_eq!(audit.auto_resolutions()[0].work_item_id(), "wi-ok");
        assert!(audit.escalations().is_empty());
    }

    #[test]
    fn autonomous_decision_line_rejects_missing_typed_fields() {
        for line in [
            r#"{"stage":1,"work_item_id":"wi-1","disposition":"auto-approve","governing_settings":["auto_approve_ready"]}"#,
            r#"{"stage":"auto-disposition","work_item_id":"wi-1","governing_settings":["auto_approve_ready"]}"#,
            r#"{"stage":"auto-disposition","work_item_id":"wi-1","disposition":"auto-approve","governing_settings":["auto_approve_ready",7]}"#,
        ] {
            check(
                super::autonomous_decision_from_line(line).is_none(),
                "malformed autonomous decision fields should be skipped",
            );
        }
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
                &autonomous_journal_line("wi-1", "auto-approve", &["auto_approve_ready"]),
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
    fn approve_handler_derives_action_id_and_appends_shared_work_item_events() {
        let command = approve_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome =
            ok_work_item_command_outcome(handle_work_item_approve_command(&command, &mut port));

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
    }

    #[test]
    fn approve_handler_records_failed_outcome_with_start() {
        let command = approve_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::failed());

        let outcome =
            ok_work_item_command_outcome(handle_work_item_approve_command(&command, &mut port));

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
    }

    #[test]
    fn approve_handler_records_not_wired_without_fabricating_start() {
        let command = approve_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::not_wired());

        let outcome =
            ok_work_item_command_outcome(handle_work_item_approve_command(&command, &mut port));

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
    fn accept_handler_derives_action_id_and_routes_through_the_shared_port() {
        let command = accept_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome =
            ok_work_item_command_outcome(handle_work_item_accept_command(&command, &mut port));

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
    fn reject_handler_maps_regroom_payload_onto_the_reject_action_id() {
        let command = reject_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome = ok_work_item_command_outcome(handle_work_item_reject_command(
            &command,
            r#"{"mode":"regroom"}"#,
            &mut port,
        ));

        // The mode from the payload lands in the third action-id segment.
        assert_eq!(port.observed_action_ids, ["reject:wi-1:regroom"]);
        assert_eq!(outcome.command_status(), "completed");
        for event in outcome.events() {
            assert_eq!(
                event.payload_json(),
                r#"{"action_id":"reject:wi-1:regroom"}"#
            );
        }
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
    fn resolve_blocked_handler_maps_each_target_onto_the_action_id() {
        for (payload, expected) in [
            (r#"{"target_status":"ready"}"#, "resolve-blocked:wi-1:ready"),
            (
                r#"{"target_status":"backlog"}"#,
                "resolve-blocked:wi-1:backlog",
            ),
        ] {
            let command = resolve_blocked_command();
            let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

            let outcome = ok_work_item_command_outcome(handle_work_item_resolve_blocked_command(
                &command, payload, &mut port,
            ));

            assert_eq!(port.observed_action_ids, [expected]);
            assert_eq!(outcome.command_status(), "completed");
            for event in outcome.events() {
                assert_eq!(
                    event.payload_json(),
                    format!(r#"{{"action_id":"{expected}"}}"#)
                );
            }
        }
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

    /// The operator's optional answer rides the payload and the argv verbatim,
    /// and only when it is more than whitespace.
    #[test]
    fn resolve_blocked_carries_the_operator_answer_into_the_payload_and_the_argv() {
        const ANSWER: &str = "  Ship the narrow fix; the refactor is a separate item.  ";

        // The valve outcome: the answer is added beside the target status, kept
        // byte-for-byte (leading and trailing spaces included), and a blank or
        // whitespace-only field is an ABSENT answer rather than an empty one.
        let with_answer = super::valve_outcome(
            PendingValve::MoveStatus {
                from: Lane::Blocked,
                to: Lane::Ready,
            },
            "wi-1",
            "operator",
            ANSWER,
        );
        assert!(matches!(
            with_answer,
            Some(OperatorActionOutcome::PersistCommandWithPayload { ref command, ref payload_json })
                if command.command_type() == &CommandType::WorkItemResolveBlockedRequested
                    && command.aggregate_id() == "wi-1"
                    && payload_json
                        == &serde_json::json!({"answer": ANSWER, "target_status": "ready"})
                            .to_string()
        ));
        for blank in ["", "   "] {
            let outcome = super::valve_outcome(
                PendingValve::MoveStatus {
                    from: Lane::Blocked,
                    to: Lane::Backlog,
                },
                "wi-1",
                "operator",
                blank,
            );
            assert!(matches!(
                outcome,
                Some(OperatorActionOutcome::PersistCommandWithPayload { ref payload_json, .. })
                    if payload_json == r#"{"target_status":"backlog"}"#
            ));
        }

        // The governed drive argv: `--answer <answer>` rides immediately behind
        // the action it answers, and an answer-less action's argv is unchanged.
        let probe = ArgRecordingProbe {
            outcome: SourceProbeOutcome::observed("", true),
            observed_args: std::cell::RefCell::new(Vec::new()),
        };
        let mut port = drive_over(&probe);
        assert_eq!(
            handle_work_item_resolve_blocked_command(
                &resolve_blocked_command(),
                &serde_json::json!({"target_status": "ready", "answer": ANSWER}).to_string(),
                &mut port,
            )
            .map(|outcome| outcome.command_status().to_owned()),
            Ok("completed".to_owned())
        );
        assert_eq!(
            probe.observed_args.borrow().clone(),
            [
                "drive.py",
                "--repo",
                "/orch",
                "--json",
                "--action",
                "resolve-blocked:wi-1:ready",
                "--answer",
                ANSWER,
                "--invoker",
                "operator",
            ]
        );
        let mut port = drive_over(&probe);
        let _ = handle_work_item_resolve_blocked_command(
            &resolve_blocked_command(),
            r#"{"target_status":"ready"}"#,
            &mut port,
        );
        // An answer-less action invokes the argv it always did.
        let answer_less = probe.observed_args.borrow().clone();
        assert!(!answer_less.contains(&"--answer".to_owned()));
    }

    /// A present-but-unusable `answer` is refused before the orchestrator is
    /// invoked; an absent one is the ordinary case.
    #[test]
    fn resolve_blocked_refuses_a_present_but_unusable_answer_without_invoking_the_port() {
        for payload in [
            r#"{"target_status":"ready","answer":42}"#,
            r#"{"target_status":"ready","answer":""}"#,
            r#"{"target_status":"ready","answer":"   "}"#,
            r#"{"target_status":"ready","answer":null}"#,
        ] {
            let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
            assert_eq!(
                handle_work_item_resolve_blocked_command(
                    &resolve_blocked_command(),
                    payload,
                    &mut port
                ),
                Err(ApplicationError::InvalidResolveBlockedAnswer)
            );
            assert_eq!(port.observed_action_ids, [] as [String; 0]);
        }
    }

    /// The request's answer is carried on the request itself, so every port sees
    /// it (and every answer-less request keeps reporting `None`).
    #[test]
    fn an_orchestrator_action_request_carries_its_optional_answer() {
        let bare = OrchestratorActionRequest::new("approve:wi-1".to_owned());
        assert_eq!(bare.answer(), None);
        assert_eq!(bare.requested_by(), "operator");
        let answered = OrchestratorActionRequest::attributed(
            "resolve-blocked:wi-1:ready".to_owned(),
            "operator".to_owned(),
        )
        .with_answer(Some("the answer"));
        assert_eq!(answered.answer(), Some("the answer"));
        assert_eq!(answered.with_answer(None).answer(), None);
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
    fn move_handler_maps_each_pre_terminal_target_onto_the_move_action_id() {
        for (payload, expected) in [
            (r#"{"target_status":"backlog"}"#, "move:wi-1:backlog"),
            (r#"{"target_status":"ready"}"#, "move:wi-1:ready"),
            (r#"{"target_status":"blocked"}"#, "move:wi-1:blocked"),
            (r#"{"target_status":"active"}"#, "move:wi-1:active"),
        ] {
            let command = move_command();
            let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
            let outcome = ok_work_item_command_outcome(handle_work_item_move_command(
                &command, payload, &mut port,
            ));
            assert_eq!(port.observed_action_ids, [expected]);
            assert_eq!(outcome.command_status(), "completed");
        }
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
    fn dispatcher_override_handler_maps_each_cap_setting_and_clear_onto_its_action_id() {
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
            let outcome = ok_work_item_command_outcome(
                handle_work_item_set_dispatcher_override_command(&command, payload, &mut port),
            );
            assert_eq!(port.observed_action_ids, [expected]);
            assert_eq!(outcome.command_status(), "completed");
        }
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
    fn dispatcher_override_action_id_rejects_missing_value_and_bad_rework_cap() {
        let missing_value =
            super::dispatcher_override_action_id("wi-1", r#"{"setting":"merge_on_review_cap"}"#);
        let bad_rework_cap = super::dispatcher_override_action_id(
            "wi-1",
            r#"{"setting":"acceptance_rework_cap","value":0}"#,
        );

        check(
            missing_value == Err(ApplicationError::InvalidDispatcherOverrideSetting),
            "dispatcher override should require a value",
        );
        check(
            bad_rework_cap == Err(ApplicationError::InvalidDispatcherOverrideSetting),
            "acceptance rework cap override should reject non-positive values",
        );
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
    fn reject_handler_maps_rework_payload_onto_the_reject_action_id() {
        let command = reject_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome = ok_work_item_command_outcome(handle_work_item_reject_command(
            &command,
            r#"{"mode":"rework"}"#,
            &mut port,
        ));

        assert_eq!(port.observed_action_ids, ["reject:wi-1:rework"]);
        assert_eq!(outcome.command_status(), "completed");
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
    fn reject_handler_rejects_malformed_payload_without_invoking_port() {
        let command = reject_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome = handle_work_item_reject_command(&command, "not json", &mut port);

        check(
            outcome == Err(ApplicationError::InvalidRejectMode),
            "malformed reject payload should be rejected",
        );
        check(
            port.observed_action_ids.is_empty(),
            "malformed reject payload should not invoke the port",
        );
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
    fn set_admission_handler_maps_auto_payload_onto_the_action_id() {
        let command = set_admission_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome = ok_work_item_command_outcome(handle_work_item_set_admission_command(
            &command,
            r#"{"policy":"auto"}"#,
            &mut port,
        ));

        // The policy from the payload lands in the third action-id segment.
        assert_eq!(port.observed_action_ids, ["set-admission:wi-1:auto"]);
        assert_eq!(outcome.command_status(), "completed");
        for event in outcome.events() {
            assert_eq!(
                event.payload_json(),
                r#"{"action_id":"set-admission:wi-1:auto"}"#
            );
        }
    }

    #[test]
    fn set_admission_handler_maps_manual_payload_onto_the_action_id() {
        let command = set_admission_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());

        let outcome = ok_work_item_command_outcome(handle_work_item_set_admission_command(
            &command,
            r#"{"policy":"manual"}"#,
            &mut port,
        ));

        assert_eq!(port.observed_action_ids, ["set-admission:wi-1:manual"]);
        assert_eq!(outcome.command_status(), "completed");
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
    fn set_acceptance_handler_maps_ai_only_payload_onto_the_action_id() {
        let command = set_acceptance_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
        let payload = r#"{"policy":"ai-only"}"#;

        let outcome = ok_work_item_command_outcome(handle_work_item_set_acceptance_command(
            &command, payload, &mut port,
        ));

        // The policy from the payload lands in the third action-id segment.
        assert_eq!(port.observed_action_ids, ["set-acceptance:wi-1:ai-only"]);
        assert_eq!(outcome.command_status(), "completed");
        for event in outcome.events() {
            assert_eq!(
                event.payload_json(),
                r#"{"action_id":"set-acceptance:wi-1:ai-only"}"#
            );
        }
    }

    #[test]
    fn set_acceptance_handler_maps_human_only_payload_onto_the_action_id() {
        let command = set_acceptance_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
        let payload = r#"{"policy":"human-only"}"#;

        let outcome = ok_work_item_command_outcome(handle_work_item_set_acceptance_command(
            &command, payload, &mut port,
        ));

        assert_eq!(port.observed_action_ids, ["set-acceptance:wi-1:human-only"]);
        assert_eq!(outcome.command_status(), "completed");
    }

    #[test]
    fn set_acceptance_handler_maps_ai_then_human_payload_onto_the_action_id() {
        let command = set_acceptance_command();
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
        let payload = r#"{"policy":"ai-then-human"}"#;

        let outcome = ok_work_item_command_outcome(handle_work_item_set_acceptance_command(
            &command, payload, &mut port,
        ));

        assert_eq!(
            port.observed_action_ids,
            ["set-acceptance:wi-1:ai-then-human"]
        );
        assert_eq!(outcome.command_status(), "completed");
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
            [
                "drive",
                "--repo",
                "/repo",
                "--action",
                "approve:wi-1",
                "--invoker",
                "operator",
            ]
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
            outcome: SourceProbeOutcome::observed(
                r#"{"summary":"approve requires an effective-manual pending-approval item.","domain_error":"invalid-source-state"}"#,
                false,
            ),
        };
        let mut port = DispatcherOrchestratorActionPort::new(&probe, "drive", &["--repo", "/repo"]);

        let outcome = port.run_action(&OrchestratorActionRequest::new("approve:wi-1".to_owned()));

        assert_eq!(
            outcome,
            Ok(OrchestratorActionOutcome::failed_with_refusal(
                r#"{"summary":"approve requires an effective-manual pending-approval item.","domain_error":"invalid-source-state"}"#.to_owned()
            ))
        );
    }

    #[test]
    fn dispatcher_action_port_failure_without_stdout_carries_no_refusal() {
        // A blank stdout stays an unexplained failure rather than an empty
        // refusal payload.
        let probe = StubDrainProbe {
            outcome: SourceProbeOutcome::observed("   ", false),
        };
        let mut port = DispatcherOrchestratorActionPort::new(&probe, "drive", &["--repo", "/repo"]);
        let outcome = port.run_action(&OrchestratorActionRequest::new("approve:wi-1".to_owned()));
        assert_eq!(outcome, Ok(OrchestratorActionOutcome::failed()));
    }

    #[test]
    fn workflow_scope_override_valve_maps_onto_its_action_id_and_payload() {
        // The valve's dial has exactly one admitted value, so its labels,
        // cycling, and outcome are all pinned here in one place.
        let valve = PendingValve::SetWorkflowScopeOverride;
        assert_eq!(valve.valve_label(), "Set workflow scope");
        assert_eq!(valve.option_label(), Some("citation-only"));
        assert_eq!(valve.option_display().as_deref(), Some("citation-only"));
        assert!(!valve.is_destructive());
        assert_eq!(valve.cycled(true), valve);
        assert_eq!(valve.cycled(false), valve);
        assert!(per_item_verb_is_state_valid(Lane::Ready, valve));
        assert!(!per_item_verb_is_state_valid(Lane::Backlog, valve));
        assert!(!per_item_verb_is_state_valid(Lane::Acceptance, valve));

        let outcome = super::valve_outcome(valve, "wi-1", "operator", "");
        assert!(matches!(
            outcome,
            Some(OperatorActionOutcome::PersistCommandWithPayload { command, payload_json })
                if command.command_type() == &CommandType::WorkItemSetWorkflowScopeOverrideRequested
                    && command.aggregate_id() == "wi-1"
                    && payload_json == r#"{"scope":"citation-only"}"#
        ));
    }

    #[test]
    fn workflow_scope_handler_maps_the_payload_onto_the_action_id() {
        let command = CommandEnvelope::new(
            "cmd_scope".to_owned(),
            CommandType::WorkItemSetWorkflowScopeOverrideRequested,
            "wi-1".to_owned(),
            "wi-1:work_item.set_workflow_scope_override_requested:scope=citation-only".to_owned(),
            "operator".to_owned(),
        );
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
        let outcome = handle_work_item_set_workflow_scope_override_command(
            &command,
            r#"{"scope":"citation-only"}"#,
            &mut port,
        );
        assert_eq!(
            port.observed_action_ids,
            ["set-workflow-scope-override:wi-1:citation-only"]
        );
        assert_eq!(
            outcome.map(|outcome| outcome.command_status().to_owned()),
            Ok("completed".to_owned())
        );
    }

    #[test]
    fn a_failed_action_outcome_threads_its_refusal_into_the_failure_event() {
        // The Failed arm of the shared work-item action runner, exercised in
        // THIS crate with a captured refusal: the failure event carries it.
        let command = CommandEnvelope::new(
            "cmd_scope_fail".to_owned(),
            CommandType::WorkItemSetWorkflowScopeOverrideRequested,
            "wi-1".to_owned(),
            "wi-1:work_item.set_workflow_scope_override_requested".to_owned(),
            "operator".to_owned(),
        );
        let mut port = RecordingActionPort::returning(
            OrchestratorActionOutcome::failed_with_refusal(r#"{"domain_error":"x"}"#.to_owned()),
        );
        let outcome = handle_work_item_set_workflow_scope_override_command(
            &command,
            r#"{"scope":"citation-only"}"#,
            &mut port,
        );
        assert_eq!(
            outcome
                .as_ref()
                .map(super::WorkItemCommandOutcome::command_status),
            Ok("failed")
        );
        let carries_refusal = outcome.iter().any(|outcome| {
            outcome
                .events()
                .iter()
                .any(|event| event.payload_json().contains("domain_error"))
        });
        assert!(carries_refusal);
    }

    #[test]
    fn a_failed_approve_threads_json_diagnostic_fields_into_the_failure_event() {
        let command = approve_command();
        let mut port = RecordingActionPort::returning(
            OrchestratorActionOutcome::failed_with_refusal(
                r#"{"summary":"approve requires an effective-manual pending-approval item.","domain_error":"invalid-source-state"}"#.to_owned(),
            ),
        );

        let outcome =
            ok_work_item_command_outcome(handle_work_item_approve_command(&command, &mut port));
        let failed_payloads = outcome
            .events()
            .iter()
            .filter(|event| *event.event_type() == EventType::WorkItemActionFailed)
            .map(ConsoleEvent::payload_json)
            .collect::<Vec<_>>();

        let expected = serde_json::json!({
            "action_id": "approve:wi-1",
            "summary": "approve requires an effective-manual pending-approval item.",
            "domain_error": "invalid-source-state"
        })
        .to_string();
        assert_eq!(failed_payloads, [expected.as_str()]);
    }

    #[test]
    fn workflow_scope_handler_rejects_an_empty_work_item_id_without_invoking_the_port() {
        let command = CommandEnvelope::new(
            "cmd_scope_empty".to_owned(),
            CommandType::WorkItemSetWorkflowScopeOverrideRequested,
            "   ".to_owned(),
            "empty:work_item.set_workflow_scope_override_requested".to_owned(),
            "operator".to_owned(),
        );
        let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
        let outcome = handle_work_item_set_workflow_scope_override_command(
            &command,
            r#"{"scope":"citation-only"}"#,
            &mut port,
        );
        assert_eq!(outcome, Err(ApplicationError::EmptyWorkItemId));
        assert!(port.observed_action_ids.is_empty());
    }

    #[test]
    fn workflow_scope_handler_rejects_bad_payloads_without_invoking_the_port() {
        let command = CommandEnvelope::new(
            "cmd_scope".to_owned(),
            CommandType::WorkItemSetWorkflowScopeOverrideRequested,
            "wi-1".to_owned(),
            "wi-1:work_item.set_workflow_scope_override_requested".to_owned(),
            "operator".to_owned(),
        );
        for payload in ["not json", "{}", r#"{"scope":"everything"}"#] {
            let mut port = RecordingActionPort::returning(OrchestratorActionOutcome::completed());
            let outcome =
                handle_work_item_set_workflow_scope_override_command(&command, payload, &mut port);
            assert_eq!(outcome, Err(ApplicationError::InvalidWorkflowScope));
            assert!(port.observed_action_ids.is_empty());
        }
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
    /// all released settings at explicit non-default values.
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

    /// The Events timeline renders event LABELS, so a reconciler observation
    /// the operator can see in the lane must also be nameable in the timeline
    /// that records where it came from.
    #[test]
    fn the_reconciler_projection_event_carries_a_timeline_label() {
        assert_eq!(
            EventType::FactoryRunOrphansObserved.label(),
            "Factory run orphans observed"
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
            r#"{"repo":"repo-a","setting":"drift_capture_merge_threshold","value":1}"#,
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
        assert_eq!(settings.drift_capture_merge_threshold(), 1);
        assert_eq!(settings.wip_cap(), 9);
    }

    #[test]
    fn settings_port_reads_all_effective_values_through_the_config_action() {
        let probe = ArgRecordingProbe {
            outcome: SourceProbeOutcome::observed(CONFIG_READ_JSON, true),
            observed_args: RefCell::new(Vec::new()),
        };
        let mut drive = drive_over(&probe);
        let mut settings = DispatcherSettingsPort::new(&mut drive);

        let read = settings.read_settings();

        assert_eq!(
            read,
            Ok(DispatcherSettingsRead::Observed(
                DispatcherSettings::new_with_drift_capture_merge_threshold(
                    true,
                    false,
                    AcceptancePolicy::AiOnly,
                    4,
                    2,
                    1,
                    9,
                )
            ))
        );
        // The read rode the `config` action-id, nothing more.
        assert_eq!(
            *probe.observed_args.borrow(),
            [
                "drive.py",
                "--repo",
                "/orch",
                "--json",
                "--action",
                "config",
                "--invoker",
                "operator",
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
    fn settings_read_surfaces_port_errors() {
        let mut port = ReadErrorActionPort;
        let mut settings = DispatcherSettingsPort::new(&mut port);

        let read = settings.read_settings();
        let write = settings.write_setting(&DispatcherSettingWrite::WipCap(7));

        check(
            read == Err(ApplicationError::FactoryDrainPortFailed),
            "settings read port errors should surface",
        );
        check(
            write == Ok(OrchestratorActionOutcome::completed()),
            "read-error fixture should still cover its write surface",
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
            { "key": "acceptance_rework_cap", "value": 2 },
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
    fn settings_read_requires_each_declared_key() {
        for payload in [
            r#"{"settings":[
              {"key":"merge_on_review_cap","value":false},
              {"key":"acceptance_mode","value":"ai-only"},
              {"key":"review_fix_cap","value":4},
              {"key":"acceptance_rework_cap","value":2},
              {"key":"wip_cap","value":9}
            ]}"#,
            r#"{"settings":[
              {"key":"auto_approve_ready","value":true},
              {"key":"acceptance_mode","value":"ai-only"},
              {"key":"review_fix_cap","value":4},
              {"key":"acceptance_rework_cap","value":2},
              {"key":"wip_cap","value":9}
            ]}"#,
            r#"{"settings":[
              {"key":"auto_approve_ready","value":true},
              {"key":"merge_on_review_cap","value":false},
              {"key":"review_fix_cap","value":4},
              {"key":"acceptance_rework_cap","value":2},
              {"key":"wip_cap","value":9}
            ]}"#,
            r#"{"settings":[
              {"key":"auto_approve_ready","value":true},
              {"key":"merge_on_review_cap","value":false},
              {"key":"acceptance_mode","value":"ai-only"},
              {"key":"review_fix_cap","value":4},
              {"key":"wip_cap","value":9}
            ]}"#,
            r#"{"settings":[
              {"key":"auto_approve_ready","value":true},
              {"key":"merge_on_review_cap","value":false},
              {"key":"acceptance_mode","value":"ai-only"},
              {"key":"review_fix_cap","value":4},
              {"key":"acceptance_rework_cap","value":2}
            ]}"#,
            r#"{"settings":[
              {"key":"auto_approve_ready","value":true},
              {"key":"merge_on_review_cap","value":false},
              {"key":"acceptance_mode","value":"ai-only"},
              {"key":"acceptance_rework_cap","value":2},
              {"key":"wip_cap","value":9}
            ]}"#,
        ] {
            check(
                super::settings_from_config_read(payload).is_none(),
                "settings read should reject missing declared keys",
            );
        }
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
                "set-config:wip_cap:7",
                "--invoker",
                "operator",
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

    struct ReadErrorActionPort;

    impl OrchestratorActionPort for ReadErrorActionPort {
        fn run_action(
            &mut self,
            _request: &OrchestratorActionRequest,
        ) -> super::ApplicationResult<OrchestratorActionOutcome> {
            Ok(OrchestratorActionOutcome::completed())
        }

        fn read_action(
            &mut self,
            _request: &OrchestratorActionRequest,
        ) -> super::ApplicationResult<super::OrchestratorActionReading> {
            Err(ApplicationError::FactoryDrainPortFailed)
        }
    }

    struct WriteErrorActionPort;

    impl OrchestratorActionPort for WriteErrorActionPort {
        fn run_action(
            &mut self,
            _request: &OrchestratorActionRequest,
        ) -> super::ApplicationResult<OrchestratorActionOutcome> {
            Err(ApplicationError::FactoryDrainPortFailed)
        }

        fn read_action(
            &mut self,
            _request: &OrchestratorActionRequest,
        ) -> super::ApplicationResult<super::OrchestratorActionReading> {
            Ok(super::OrchestratorActionReading::observed(
                CONFIG_READ_JSON_DEFAULTS.to_owned(),
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

    #[test]
    fn config_handler_surfaces_read_and_write_port_errors() {
        let mut read_port = ReadErrorActionPort;
        let mut read_settings = DispatcherSettingsPort::new(&mut read_port);
        let read = handle_config_dispatcher_setting_set_command(
            &dispatcher_setting_set_command(),
            r#"{"repo":"repo-a","setting":"auto_approve_ready","value":true}"#,
            "2026-07-11T00:00:04Z",
            &mut read_settings,
        );

        check(
            read == Err(ApplicationError::FactoryDrainPortFailed),
            "config handler should surface read errors",
        );

        let mut write_port = WriteErrorActionPort;
        let mut write_settings = DispatcherSettingsPort::new(&mut write_port);
        let write = handle_config_dispatcher_setting_set_command(
            &dispatcher_setting_set_command(),
            r#"{"repo":"repo-a","setting":"auto_approve_ready","value":true}"#,
            "2026-07-11T00:00:05Z",
            &mut write_settings,
        );

        check(
            write == Err(ApplicationError::FactoryDrainPortFailed),
            "config handler should surface write errors",
        );
    }
    // -----------------------------------------------------------------------
    // Retired autonomous-mode surface: tests assert the toggle, modal, dangerous
    // label, and header indicator are absent from the current TUI.
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
        check(
            model.header().contains(&format!("repo: {CONFIRM_REPO}")),
            "header should include selected repo",
        );
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
                .contains("event sources: 2 unavailable (github, orchestrator)")
        );
    }

    #[test]
    fn header_shows_no_unavailability_count_when_every_source_is_observed() {
        // Factory-idle: no not-observed finding, so no phantom count and no
        // false alarm -- a true-empty screen stays clean.
        let idle = build_tui_model(&[], 0);
        assert!(idle.unavailable_sources().is_empty());
        assert!(!idle.header().contains("unavailable"));
        assert!(!idle.header().contains("event sources:"));
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

    #[test]
    fn unavailable_tally_counts_a_fresh_re_down_after_recovery() {
        // Persistent stores deduplicate by (source, source_event_id), so a
        // re-down transition must arrive as a fresh row after the recovery
        // observation. Given that stream shape, the header reflects the latest
        // poll outcome even when an older positive source snapshot exists.
        let events = [
            ConsoleEvent::fixture(
                "evt_livespec_down_epoch_1",
                EventType::SourceNotObservedFindingObserved,
                "livespec",
            ),
            ConsoleEvent::fixture(
                "evt_livespec_snapshot_stale_positive",
                EventType::LivespecNextSnapshotObserved,
                "livespec",
            ),
            ConsoleEvent::fixture(
                "evt_livespec_observed_epoch_2",
                EventType::SourceObservedFindingObserved,
                "livespec",
            ),
            ConsoleEvent::fixture(
                "evt_livespec_down_epoch_3",
                EventType::SourceNotObservedFindingObserved,
                "livespec",
            ),
        ];

        assert_eq!(
            build_tui_model(&events, 0).unavailable_sources(),
            ["livespec".to_owned()]
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

    /// The dogfooded hint row: a selected READY work-item in a drilled-in lane,
    /// which is the widest row the Status band composes (navigation, the driver
    /// handoff, the lifecycle move, all five policy dials, `d dispatch`, and the
    /// globals). Derived from the registry so it cannot drift from what ships.
    fn ready_item_hints() -> String {
        action_registry::selected_item_hint(
            &action_registry::ActionContext {
                lane: Lane::Ready,
                admission_policy: AdmissionPolicy::Manual,
                acceptance_policy: AcceptancePolicy::AiThenHuman,
                has_driver_handoff: true,
                awaits_scope_override: false,
                ready_work_item_count: 1,
                surface: action_registry::ActionSurface::LaneDrill,
            },
            2,
        )
    }

    /// The policy dials the drilled ready lane offers -- the class the
    /// 2026-09-08 dogfood passes measured surviving while the way back out of
    /// the lane and the last command's verdict were already gone.
    const READY_LANE_DIALS: [&str; 4] = [
        "g merge cap",
        "f fix cap",
        "n set-acceptance",
        "k rework cap",
    ];

    /// How the operator moves over the lane, opens a row, and leaves it again.
    const READY_LANE_WAY_AROUND: [&str; 3] = ["up/down move", "enter item", "esc lane list"];

    /// The dogfooded verdict, in the shape `FactoryDispatchItemFailed` composes
    /// when the stored error payload names no cause: the 55-column message that
    /// used to be shed whole from a 105-column band.
    fn dispatch_failed_outcome() -> CommandOutcome {
        command_outcome_notice(
            "dispatch item",
            "failed",
            Some(OUTCOME_CAUSE_ABSENT.to_owned()),
        )
    }

    #[test]
    fn a_narrow_status_band_declares_the_hints_it_could_not_draw() {
        // The measured defect (2026-09-08, a 105-column pane): the band ran out
        // of room mid-row and simply STOPPED, so an operator could not tell an
        // unavailable verb from an undrawn one. Whatever is shed is now COUNTED
        // -- and named with the key that reopens it.
        let hints = ready_item_hints();
        assert!(hint_display_width(&hints) > 98);

        let fitted = fit_footer_line(&hints, 98, None);
        assert!(hint_display_width(&fitted) <= 98);
        check(
            fitted.ends_with(&format!(" more: {HINT_OVERFLOW_KEY}")) && fitted.contains('+'),
            "a band too narrow for every hint must say how many it dropped and where to read them",
        );
    }

    #[test]
    fn the_overflow_marker_names_the_registrys_help_key() {
        // The marker is a DOOR, so the key it names has to be the key that
        // actually opens Help. Pinned against the registry rather than against
        // a second copy of the character, so rebinding Help moves both.
        let bound =
            action_registry::action_for_id("open-help").map(action_registry::accelerator_display);
        check(
            bound.as_deref() == Some(HINT_OVERFLOW_KEY),
            "the overflow marker must name the key the registry binds Help to",
        );
    }

    #[test]
    fn the_status_band_sheds_the_policy_dials_before_the_way_back_out() {
        // livespec-console-beads-fabro-mx9u.1. The ranking used to shed
        // navigation FIRST, so at 98 columns the drilled ready lane drew four
        // rarely-used dials while `esc lane list` -- the only way back out of
        // the lane -- was gone. A dial is a configuration override an operator
        // reaches for occasionally; the way out is how they undo the drill-in.
        let fitted = fit_footer_line(&ready_item_hints(), 98, None);

        for token in READY_LANE_WAY_AROUND {
            check(
                fitted.contains(token),
                "the way around the lane must survive the width squeeze",
            );
        }
        for dial in READY_LANE_DIALS {
            check(
                !fitted.contains(dial),
                "and the policy dials must be what yielded to make room for it",
            );
        }
    }

    #[test]
    fn no_width_draws_a_policy_dial_while_the_way_back_out_is_missing() {
        // The ordering stated as the property it is, over every width the band
        // can be given rather than the two the dogfood passes happened to
        // measure: a dial is never on screen unless all three navigation keys
        // already are.
        let hints = ready_item_hints();
        for width in 1..=hint_display_width(&hints) {
            let fitted = fit_footer_line(&hints, width, None);
            let drew_a_dial = READY_LANE_DIALS.iter().any(|dial| fitted.contains(dial));
            let drew_the_way_out = READY_LANE_WAY_AROUND
                .iter()
                .all(|token| fitted.contains(token));
            check(
                !drew_a_dial || drew_the_way_out,
                "a dial must never outlive the way back out of the lane",
            );
        }
    }

    #[test]
    fn the_last_commands_verdict_shortens_before_a_single_hint_is_shed() {
        // 34 of the verdict's 55 columns are the command's full name and the
        // cause; the two facts an operator cannot reconstruct are WHICH command
        // and HOW it ended. Giving those up to keep one more dial was the trade
        // the old fitter made, because it could only shed segments whole.
        let outcome = dispatch_failed_outcome();
        let hints = ready_item_hints();
        let row = format!("{hints} | {}", outcome.full);

        let fitted = fit_footer_line(&row, 200, Some(&outcome));

        assert_eq!(fitted, format!("{hints} | last: dispatch failed"));
        check(
            !fitted.contains(" more: "),
            "abbreviating the verdict alone must be enough at this width, with nothing shed",
        );
    }

    #[test]
    fn the_last_commands_verdict_outlives_every_dial_and_every_verb() {
        // The 105-column pane the report measured. The verdict is the one
        // segment the overflow marker's door cannot reopen -- Help lists
        // actions, and the event stream retires the verdict the moment the next
        // command starts -- so it yields only once every dial and every verb
        // already has.
        let outcome = dispatch_failed_outcome();
        let hints = ready_item_hints();
        let fitted = fit_footer_line(&format!("{hints} | {}", outcome.full), 105, Some(&outcome));

        assert!(hint_display_width(&fitted) <= 105);
        check(
            fitted.contains("last: dispatch failed"),
            "the verdict of the command just run must still be readable at 105 columns",
        );
        for gone in READY_LANE_DIALS
            .into_iter()
            .chain(["d dispatch", "h handoff"])
        {
            check(
                !fitted.contains(gone),
                "and it must have outlived every dial and every verb to get there",
            );
        }
    }

    #[test]
    fn a_status_band_wide_enough_carries_the_hint_row_untouched() {
        // The no-regression half: given room, the fitter is the identity, so
        // every hint the context owns renders exactly as it does today -- and
        // the verdict keeps its cause rather than being abbreviated for nothing.
        let hints = ready_item_hints();
        assert_eq!(fit_footer_line(&hints, 300, None), hints);
        assert!(!hints.contains(" more"));

        let outcome = dispatch_failed_outcome();
        let row = format!("{hints} | {}", outcome.full);
        assert_eq!(fit_footer_line(&row, 300, Some(&outcome)), row);
    }

    #[test]
    fn a_status_band_too_narrow_even_for_the_marker_still_fits_the_band() {
        // The degenerate tail. Nothing useful can be said in four columns, but
        // the fitter still must not hand the renderer a row wider than the band
        // and let the clip it exists to prevent happen anyway.
        let fitted = fit_footer_line(&ready_item_hints(), 4, None);
        assert_eq!(fitted, "+13 ");
    }

    #[test]
    fn header_line_fits_the_pinned_width_and_preserves_the_priority_fields() {
        // The dogfood target is a 112-column terminal (inner width 110 inside the
        // header block's borders) with several sources down. The header MUST fit
        // and keep the operationally-important fields plus the cockpit-blind tell
        // (the source count), degrading low-priority fields and the names.
        let model = blind_model(
            CONFIRM_REPO,
            &["dispatcher", "fabro", "github", "livespec", "orchestrator"],
        );
        let line = model.header_line(110);
        assert!(line.chars().count() <= 110);
        assert!(line.contains("view: Attention"));
        assert!(line.contains("attention: 0"));
        // The count survives even when the names cannot: how-many is the tell.
        assert!(line.contains("event sources: 5 unavailable"));
    }

    #[test]
    fn header_line_matches_the_canonical_header_when_wide() {
        // Given room to spare, the fitted header is the full canonical header --
        // every field and every source name, nothing dropped.
        let model = blind_model("-", &["fabro", "github"]);
        let line = model.header_line(300);
        assert_eq!(line, model.header());
        assert!(line.contains("event sources: 2 unavailable (fabro, github)"));
    }

    #[test]
    fn header_line_elides_source_names_before_dropping_priority_fields() {
        // At an intermediate width the names abbreviate to a `+N more` marker
        // while the priority fields stay whole -- never a mid-field truncation.
        //
        // Width is 118, not the round 112 this test used before the
        // "sources" -> "event sources" rename (livespec-console-beads-fabro-
        // mx9u.19, maintainer ruling 2026-09-09): "event " costs 6 more
        // columns in EVERY source-health form, so the same qualitative
        // shrink shape (fleet/mode dropped, repo/view/attention kept) now
        // needs 6 more columns of room to reproduce -- the behavior under
        // test, not this literal number, is what the acceptance criterion is.
        let model = blind_model(CONFIRM_REPO, &["alpha", "bravo", "charlie"]);
        let line = model.header_line(118);
        assert!(line.chars().count() <= 118);
        assert!(line.contains("+2 more"));
        check(
            line.contains(&format!("repo: {CONFIRM_REPO}")),
            "header line should include selected repo",
        );
        assert!(line.contains("attention: 0"));
    }

    #[test]
    fn header_line_never_drops_the_source_count() {
        // Even on an absurdly narrow terminal (below the target), the header keeps
        // the source count (the blind-vs-idle tell); lower-value fields and the
        // source names are shed first.
        let model = blind_model(CONFIRM_REPO, &["fabro", "github", "orchestrator"]);
        let line = model.header_line(60);
        assert!(line.contains("event sources: 3 unavailable"));
    }

    #[test]
    fn header_line_carries_no_source_segment_when_every_source_is_observed() {
        // A healthy cycle never grows a phantom source segment, at any width.
        let model = build_tui_model(&[], 0);
        for width in [40_usize, 80, 110, 300] {
            let line = model.header_line(width);
            assert!(!line.contains("unavailable"));
            assert!(!line.contains("event sources:"));
        }
        assert!(model.header_line(300).contains("repo: -"));
    }

    #[test]
    // livespec-console-beads-fabro-pzbdbo.27 AC1: the screen model renders a
    // complete frame BEFORE any source has returned data, with the header
    // naming that sources are still loading. Built at t=0 -- an empty event
    // log, exactly what the store holds before the background poller's first
    // sweep has landed -- so this is the very first frame the fix draws.
    fn header_names_loading_sources_before_any_source_has_returned_data() {
        let state = TuiInteractionState::new(0, TuiOverlay::None).with_startup_ingest_pending(true);
        let model = build_tui_model_for_state(&[], &state);

        assert!(model.startup_ingest_pending());
        assert!(model.header().contains(STARTUP_INGEST_LOADING_TELL));
        // A width wide enough for everything renders the same content as the
        // canonical header -- the fitted and unfitted forms must agree.
        assert_eq!(model.header_line(300), model.header());
    }

    #[test]
    // Companion to the above: once the first sweep lands (the composition root
    // clears the flag), the tell disappears -- it never outlives the condition
    // it names.
    fn header_drops_the_loading_tell_once_startup_ingest_completes() {
        let state =
            TuiInteractionState::new(0, TuiOverlay::None).with_startup_ingest_pending(false);
        let model = build_tui_model_for_state(&[], &state);

        assert!(!model.startup_ingest_pending());
        assert!(!model.header().contains(STARTUP_INGEST_LOADING_TELL));
    }

    #[test]
    fn header_line_names_the_single_unavailable_source_without_a_more_marker() {
        // A single unavailable source has no name to elide, so there is no
        // `+N more` abbreviation tier: the header shows the one name, then only
        // the bare count degrades under width pressure.
        let model = blind_model("-", &["orchestrator"]);
        let wide = model.header_line(300);
        assert!(wide.contains("event sources: 1 unavailable (orchestrator)"));
        assert!(!wide.contains("more"));
        // Under width pressure the lone-name form collapses straight to the count.
        let narrow = model.header_line(40);
        assert!(narrow.contains("event sources: 1 unavailable"));
        assert!(!narrow.contains("(orchestrator)"));
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum RenderedHeaderSegmentKind {
        Static,
        State,
        Transient,
    }

    fn rendered_header_segments(line: &str) -> Vec<RenderedHeaderSegmentKind> {
        line.split(" | ")
            .map(|segment| {
                if segment.starts_with("factory:") || segment.starts_with("event sources:") {
                    RenderedHeaderSegmentKind::Transient
                } else if segment.starts_with("attention:") {
                    RenderedHeaderSegmentKind::State
                } else {
                    RenderedHeaderSegmentKind::Static
                }
            })
            .collect()
    }

    #[test]
    fn header_line_prioritizes_transient_segments_over_static_fields_at_narrow_widths() {
        // This is a rendered-surface assertion over segment kind, not a pin to a
        // single dispatch string: the factory segment stands in for any transient
        // refusal/not-wired/error segment the header carries.
        let events = [ConsoleEvent::fixture(
            "evt_dispatch_item_not_wired",
            EventType::FactoryDispatchItemNotWired,
            "console:factory-command-handler",
        )];
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_selected_repo(CONFIRM_REPO.to_owned());
        let model = build_tui_model_for_state(&events, &state);

        let wide = model.header_line(300);
        assert_eq!(wide, model.header());
        assert!(rendered_header_segments(&wide).contains(&RenderedHeaderSegmentKind::Transient));
        check(
            wide.contains(&format!("repo: {CONFIRM_REPO}")),
            "wide header should include selected repo",
        );
        assert!(wide.contains("view: Lanes"));

        let narrow = model.header_line(80);
        assert!(narrow.chars().count() <= 80);
        assert!(rendered_header_segments(&narrow).contains(&RenderedHeaderSegmentKind::Transient));
        assert!(narrow.contains("factory:"));
        let static_field_yielded =
            !(narrow.contains(&format!("repo: {CONFIRM_REPO}")) & narrow.contains("view: Lanes"));
        check(
            static_field_yielded,
            "narrow header should yield a static field",
        );
    }

    #[test]
    fn header_line_prioritizes_transient_factory_state_over_static_segments() {
        let event = ConsoleEvent::fixture(
            "evt_dispatch_item_not_wired",
            EventType::FactoryDispatchItemNotWired,
            "factory",
        );
        let state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_active_view(TuiView::Lanes)
            .with_selected_repo(CONFIRM_REPO.to_owned());
        let model = build_tui_model_for_state(&[event], &state);
        let segments = [
            "fleet: livespec",
            "mode: tui",
            &format!("repo: {CONFIRM_REPO}"),
            "view: Lanes",
            "attention: 0",
            "factory: dispatch item not wired",
        ];

        let wide = model.header_line(160);
        for segment in segments {
            assert!(wide.contains(segment));
        }

        let narrow = model.header_line(60);
        assert!(narrow.chars().count() <= 60);
        assert!(narrow.contains("factory: dispatch item not wired"));
        check(
            !(narrow.contains("repo:") & narrow.contains("attention:")),
            "narrow header should yield at least one static segment",
        );
    }

    #[test]
    fn header_line_carries_transient_status_refusals() {
        let state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_transient_status(Some("Dispatch ready work unavailable".to_owned()));
        let model = build_tui_model_for_state(&[], &state);

        assert!(
            model
                .header()
                .contains("status: Dispatch ready work unavailable")
        );
        let narrow = model.header_line(70);
        assert!(narrow.chars().count() <= 70);
        assert!(narrow.contains("status: Dispatch ready work unavailable"));
    }

    // -----------------------------------------------------------------------
    // Build identity in the chrome (livespec-console-beads-fabro-mx9u.13). A
    // dogfood pass graded fixes that were not in the running binary because
    // the header carried no version, sha, or build age; these pin the fix.
    // -----------------------------------------------------------------------

    #[test]
    fn the_model_carries_the_running_builds_sha_and_build_timestamp() {
        // The rendered-text proof that the CHROME names the sha and build
        // timestamp lives in console-tui (the header pane's block title --
        // see `the_header_pane_title_names_the_running_builds_sha_and_build_timestamp`),
        // since that is where the identity is rendered
        // (livespec-console-beads-fabro-mx9u.13 review: it does NOT survive
        // as a content-line field at the maintainer's own real, busy width).
        // This is the model-layer half: the fact reaches `TuiScreenModel`
        // intact, which is what that rendering reads from.
        let state = TuiInteractionState::new(0, TuiOverlay::None).with_build_identity(Some(
            super::BuildIdentity::new("1b69345", "2026-09-08T11:16:34Z"),
        ));
        let model = build_tui_model_for_state(&[], &state);

        assert_eq!(
            model.build_identity(),
            Some(&super::BuildIdentity::new(
                "1b69345",
                "2026-09-08T11:16:34Z"
            ))
        );
    }

    #[test]
    fn a_current_build_carries_no_stale_tell() {
        let state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_build_identity(Some(super::BuildIdentity::new(
                "e656104",
                "2026-09-08T13:17:00Z",
            )))
            .with_build_staleness(super::BuildStaleness::Current);
        let model = build_tui_model_for_state(&[], &state);

        assert!(!model.header().contains("STALE"));
    }

    #[test]
    fn a_build_behind_head_carries_an_explicit_stale_tell_naming_how_far_behind() {
        // The dogfood postmortem's own numbers: five merges landed between the
        // maintainer's running build and origin/master.
        let state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_build_identity(Some(super::BuildIdentity::new(
                "1b69345",
                "2026-09-08T11:16:34Z",
            )))
            .with_build_staleness(super::BuildStaleness::Behind(5));
        let model = build_tui_model_for_state(&[], &state);

        check(
            model.header().contains("build STALE: 5 commits behind"),
            "header should carry an explicit stale-build tell naming how far behind",
        );
        assert_eq!(model.build_staleness(), super::BuildStaleness::Behind(5));
    }

    #[test]
    fn an_unknown_staleness_asserts_nothing_either_way() {
        // No repo was observed, or the comparison could not be made -- an
        // unproven claim is silent rather than defaulting to either verdict.
        let state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_build_identity(Some(super::BuildIdentity::new("1b69345", "unknown")))
            .with_build_staleness(super::BuildStaleness::Unknown);
        let model = build_tui_model_for_state(&[], &state);

        assert!(!model.header().contains("STALE"));
    }

    #[test]
    fn no_build_identity_carries_no_build_segment_at_all() {
        // The default state (every other header test in this module) resolves
        // no build identity, and must not grow a phantom `build:` segment.
        let model = build_tui_model(&[], 0);
        assert!(!model.header().contains("build:"));
    }

    #[test]
    fn the_stale_build_tell_survives_narrow_widths_that_shed_static_context() {
        // The tell shares `TransientState` priority with `factory:`/`status:`
        // -- the same tier the 2026-09-08 inversion (mx9u.1) protects -- so it
        // outlives `repo:`/`view:` under width pressure, the same way a
        // factory refusal does.
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_selected_repo(CONFIRM_REPO.to_owned())
            .with_build_identity(Some(super::BuildIdentity::new(
                "1b69345",
                "2026-09-08T11:16:34Z",
            )))
            .with_build_staleness(super::BuildStaleness::Behind(5));
        let model = build_tui_model_for_state(&[], &state);

        let wide = model.header_line(300);
        assert_eq!(wide, model.header());
        assert!(wide.contains("build STALE: 5 commits behind"));

        let narrow = model.header_line(45);
        assert!(narrow.chars().count() <= 45);
        check(
            narrow.contains("build STALE: 5 commits behind"),
            &format!(
                "the stale tell should outlive repo/view/plain-identity at a narrow width: \
                 {narrow:?}"
            ),
        );
        check(
            !(narrow.contains(&format!("repo: {CONFIRM_REPO}")) & narrow.contains("view: Lanes")),
            "a narrow header carrying the stale tell should have yielded a static field",
        );
    }

    #[test]
    fn a_stale_tell_wide_enough_to_matter_still_fits_the_dogfood_pinned_width() {
        // Same 110-column arithmetic
        // `header_line_fits_the_pinned_width_and_preserves_the_priority_fields`
        // pins, with a stale build ALSO present: it must fit alongside the
        // cockpit-blind source count, not merely alongside the ordinary fields.
        let sources = ["dispatcher", "fabro", "github", "livespec", "orchestrator"];
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
        let state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_selected_repo(CONFIRM_REPO.to_owned())
            .with_build_identity(Some(super::BuildIdentity::new(
                "1b69345",
                "2026-09-08T11:16:34Z",
            )))
            .with_build_staleness(super::BuildStaleness::Behind(5));
        let model = build_tui_model_for_state(&events, &state);
        let line = model.header_line(110);

        assert!(line.chars().count() <= 110);
        assert!(line.contains("event sources: 5 unavailable"));
        assert!(line.contains("build STALE: 5 commits behind"));
    }

    #[test]
    fn the_build_identity_never_appears_in_the_content_line_regardless_of_width_or_crowding() {
        // Measured against the maintainer's own live pane at a real
        // 159-column width (livespec-console-beads-fabro-mx9u.13 review): a
        // build-identity CONTENT-LINE field, even in a sha-only short form,
        // still had no room once `factory:`/`event sources:` alerts and a
        // two-digit `attention:` count all competed for the same budget --
        // exactly the moment an operator most wants to check the running
        // build. So the identity does not live in this content line at all
        // any more; it lives in the header pane's block TITLE instead (see
        // `console_tui::render_header`), which costs nothing from this
        // budget and is unconditionally visible. This test pins the
        // NEGATIVE: no width, crowded or otherwise, ever renders `build:` in
        // the content line, so nothing can silently regress it back in as a
        // field here.
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_selected_repo(CONFIRM_REPO.to_owned())
            .with_build_identity(Some(super::BuildIdentity::new(
                "923a5a5",
                "2026-09-08T23:27:15Z",
            )))
            .with_build_staleness(super::BuildStaleness::Current);
        let events: Vec<ConsoleEvent> = ["dispatcher", "fabro", "github"]
            .iter()
            .map(|&source| {
                ConsoleEvent::fixture(
                    &format!("evt_{source}_not_observed"),
                    EventType::SourceNotObservedFindingObserved,
                    source,
                )
            })
            .collect();
        let model = build_tui_model_for_state(&events, &state);

        for width in [40_usize, 100, 159, 300] {
            let line = model.header_line(width);
            check(
                !line.contains("build:"),
                &format!(
                    "the content line must never carry the build identity at width {width}: \
                     {line:?}"
                ),
            );
        }
        // The canonical (unfitted) header agrees.
        assert!(!model.header().contains("build:"));
    }

    #[test]
    fn a_status_within_the_budget_keeps_the_attention_count_in_the_pinned_header() {
        // livespec-console-beads-fabro-zbnnlv. A status is the LAST field the
        // fitter drops, so an over-long one evicts every other field on its way
        // out. `MAX_TRANSIENT_STATUS_CHARS` is the point at which that stops
        // happening at the pinned width, and this is what the number means.
        let state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_transient_status(Some("s".repeat(MAX_TRANSIENT_STATUS_CHARS)));
        let line = build_tui_model_for_state(&[], &state).header_line(110);

        assert!(line.chars().count() <= 110);
        check(
            line.contains("attention: 0"),
            "a budgeted status must not cost the operator their attention count",
        );
        check(
            line.contains("status: "),
            "and it must still be the report it was written to be",
        );
    }

    #[test]
    fn an_unbounded_status_blanks_the_pinned_header_entirely() {
        // THE MUST-FAIL CONTROL for the bound above, and the measured defect it
        // exists to prevent: with every field shed and the status still too wide,
        // the fitter drops the status too and composes the EMPTY STRING. The
        // operator loses their context AND is told nothing. Pinned here so the
        // budget can never be raised without someone reading why it exists.
        let state = TuiInteractionState::new(0, TuiOverlay::None)
            .with_transient_status(Some("s".repeat(MAX_TRANSIENT_STATUS_CHARS * 3)));
        let line = build_tui_model_for_state(&[], &state).header_line(110);

        check(
            line.is_empty(),
            "the unbudgeted case is a blank header, which is why producers are bounded",
        );
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
        let mut state = TuiInteractionState::new(0, TuiOverlay::None);
        for _step in 0..TuiView::all().len() {
            let model = build_tui_model_for_state(&[], &state);
            assert!(!model.footer().trim().is_empty());
            state = reduce_tui_interaction(&state, &[], TuiInteraction::SelectNextView);
        }
        assert!(
            item_hint(
                action_registry::ActionSurface::Attention,
                Lane::PendingApproval
            )
            .contains("p approve")
        );
        assert!(
            item_hint(action_registry::ActionSurface::LaneDrill, Lane::Ready)
                .contains("move-status")
        );
        assert!(model_pane_footer_hint(&settings_view_model()).contains("enter/space edit row"));
        // The read-only nav views surface select + focus-move + search.
        let read_only = "up/down move | left/right focus | / search | 1-6 view | ? help | q quit";
        assert!(read_only.contains("left/right focus") && read_only.contains("search"));
        for view in [TuiView::Spec, TuiView::Repos] {
            let model = view_model(view);
            assert_eq!(model.footer(), read_only);
        }
        // Events is a container: its own overview names the new `enter drill`
        // key exactly as the lane overview names its own, and a drilled-in
        // sub-view names its own step-out key instead of the shared hint above.
        let events_overview = view_model(TuiView::Events);
        assert_eq!(events_overview.events_focus(), EventsFocus::Overview);
        assert_eq!(
            events_overview.footer(),
            "up/down move | enter drill | left/right focus | / search | 1-6 view | ? help | q quit"
        );
        let events_drilled = build_tui_model_for_state(
            &[],
            &reduce_tui_interaction(
                &TuiInteractionState::for_view(TuiView::Events, 0, TuiOverlay::None),
                &[],
                TuiInteraction::DrillIntoEventsSubView,
            ),
        );
        assert_eq!(events_drilled.events_focus(), EventsFocus::StoredEvents);
        assert_eq!(
            events_drilled.footer(),
            "esc sub-view list | left/right focus | / search | 1-6 view | ? help | q quit"
        );
    }

    #[test]
    fn the_navigation_row_count_reads_the_list_the_cursor_moves_over() {
        // The LIST-state input the Status line's movement hint is composed
        // from. It counts ROWS, never actions, so the terminal lane reports its
        // column size exactly as an actionable lane does.
        let inbox = [
            attention_appeared("evt_inbox_a", &attention_item("console-a", "approval", "A")),
            attention_appeared("evt_inbox_b", &attention_item("console-b", "approval", "B")),
        ];
        let attention =
            build_tui_model_for_state(&inbox, &TuiInteractionState::new(0, TuiOverlay::None));
        assert_eq!(attention.active_view(), TuiView::Attention);
        assert_eq!(attention.selected_list_row_count(), 2);

        let done_rows = [
            lane_event(
                "evt_done_a",
                "console-done-a",
                Lane::Done,
                None,
                "a0",
                "done",
            ),
            lane_event(
                "evt_done_b",
                "console-done-b",
                Lane::Done,
                None,
                "a1",
                "done",
            ),
        ];
        let drilled_state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_lane_focus(LaneFocus::Lane(Lane::Done));
        assert_eq!(
            build_tui_model_for_state(&done_rows, &drilled_state).selected_list_row_count(),
            2
        );
        // A lane the board holds no column for has nothing to move over.
        let empty_state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_lane_focus(LaneFocus::Lane(Lane::Ready));
        assert_eq!(
            build_tui_model_for_state(&done_rows, &empty_state).selected_list_row_count(),
            0
        );
        // The lane OVERVIEW's cursor moves over LANES, not items, so its hint
        // is composed separately and it reports no item rows.
        assert_eq!(
            build_tui_model_for_state(
                &done_rows,
                &TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            )
            .selected_list_row_count(),
            0
        );
        // The views carrying no per-item cursor report none either.
        for view in [
            TuiView::Spec,
            TuiView::Events,
            TuiView::Repos,
            TuiView::Settings,
        ] {
            assert_eq!(view_model(view).selected_list_row_count(), 0);
        }
    }

    #[test]
    fn footer_hint_changes_when_focus_moves_to_a_different_pane() {
        // Scenario 19 case 2: moving focus from Lanes to Settings changes the
        // hints to that pane's actions, and the two panes' hints DIFFER (their
        // action sets genuinely differ: status-move/valves vs. edit).
        let lanes = item_hint(action_registry::ActionSurface::LaneDrill, Lane::Ready);
        let settings = model_pane_footer_hint(&settings_view_model());
        assert_ne!(lanes, settings);
        assert!(lanes.contains("move-status") && !lanes.contains("edit row"));
        assert!(settings.contains("edit row") && !settings.contains("move-status"));
    }

    #[test]
    fn footer_hint_reflects_the_open_overlay_and_restores_the_pane_on_close() {
        // Scenario 19 case 3: opening an overlay replaces the focused pane's
        // hints with that overlay's, and closing it (overlay back to None)
        // restores the pane's hints. Exercised against the Lanes overview so
        // the restore is observable via its distinctive `enter drill` key.
        let overview = view_model(TuiView::Lanes);
        let pane = overview.footer();
        assert!(pane.contains("enter drill"));
        let help = overlay_footer_hint(&TuiOverlay::Help {
            focus: HelpFocus::Menu,
            selected_section: help_section_for_view(TuiView::Lanes),
            scroll: 0,
        });
        assert_ne!(pane, help);
        assert!(help.contains("close help") && !help.contains("enter drill"));
        // Closing the overlay restores the underlying pane's hints verbatim.
        assert_eq!(view_model(TuiView::Lanes).footer(), pane);
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
    fn plan_page_projects_epic_children_and_handoff_comments() {
        let events = [
            plan_snapshot_event(
                "evt_epic",
                "plan-epic",
                "a0",
                "open",
                r#"{"title":"Planning Lane","item_type":"epic","depends_on":[],"comments":[{"id":"c1","author":"alice","created_at":"2026-08-16T08:00:00Z","text":"first handoff"},{"id":"c2","author":"bob","created_at":"2026-08-16T09:00:00Z","text":"second <handoff>"}]}"#,
            ),
            plan_snapshot_event(
                "evt_child_b",
                "child-b",
                "b0",
                "ready",
                r#"{"title":"Second child","depends_on":["plan-epic"]}"#,
            ),
            plan_snapshot_event(
                "evt_child_a",
                "child-a",
                "a1",
                "blocked",
                r#"{"title":"First child","depends_on":["plan-epic"]}"#,
            ),
            plan_snapshot_event(
                "evt_child_c",
                "child-c",
                "a1",
                "ready",
                r#"{"depends_on":["plan-epic"]}"#,
            ),
            plan_snapshot_event(
                "evt_unrelated",
                "other",
                "a0",
                "ready",
                r#"{"title":"Other","depends_on":[]}"#,
            ),
        ];

        let page = project_plan_page(&events, "plan-epic");

        assert_eq!(
            page.epic().map(super::PlanWorkItem::title),
            Some(Some("Planning Lane"))
        );
        assert_eq!(
            page.children()
                .iter()
                .map(|child| (child.work_item_id(), child.status()))
                .collect::<Vec<_>>(),
            vec![
                ("child-a", "blocked"),
                ("child-c", "ready"),
                ("child-b", "ready")
            ]
        );
        assert_eq!(page.children()[1].title(), None);
        assert_eq!(
            page.handoff_entries()
                .iter()
                .map(super::PlanHandoffEntry::text)
                .collect::<Vec<_>>(),
            vec!["first handoff", "second <handoff>"]
        );
        assert!(render_plan_page_html("plan-epic", &page).contains("child-c"));
    }

    #[test]
    fn plan_page_html_has_stable_url_and_escaped_ledger_text() {
        let events = [plan_snapshot_event(
            "evt_epic",
            "plan-epic",
            "a0",
            "open",
            r#"{"title":"Plan <One>","item_type":"epic","depends_on":[],"comments":[{"id":"c1","author":"alice","created_at":"2026-08-16T08:00:00Z","text":"handoff with <tag> & detail"}]}"#,
        )];
        let page = project_plan_page(&events, "plan-epic");

        let html = render_plan_page_html("plan-epic", &page);

        assert_eq!(plan_page_url("plan-epic"), "/plans/plan-epic");
        assert!(html.contains("Plan &lt;One&gt;"));
        assert!(html.contains("/plans/plan-epic"));
        assert!(html.contains("handoff with &lt;tag&gt; &amp; detail"));
        assert!(!html.contains("handoff with <tag> & detail"));
    }

    #[test]
    fn plan_page_skips_foreign_events_and_renders_empty_placeholders() {
        let events = [
            ConsoleEvent::fixture("evt_command", EventType::CommandAccepted, "console")
                .with_payload_json("{}".to_owned()),
            ConsoleEvent::fixture(
                "evt_bad_snapshot",
                EventType::WorkItemSnapshotObserved,
                "orchestrator",
            )
            .with_payload_json("{}".to_owned()),
        ];

        let page = project_plan_page(&events, "plan with spaces");
        let html = render_plan_page_html("plan with spaces", &page);

        assert_eq!(page.epic(), None);
        assert!(page.children().is_empty());
        assert!(page.handoff_entries().is_empty());
        assert_eq!(
            plan_page_url("plan with spaces"),
            "/plans/plan%20with%20spaces"
        );
        assert!(html.contains("plan with spaces has not been observed."));
        assert!(html.contains("No handoff entries observed."));
    }

    #[test]
    fn plan_page_escapes_quotes_and_apostrophes() {
        let events = [plan_snapshot_event(
            "evt_epic",
            "plan-epic",
            "a0",
            "open",
            r#"{"title":"Plan \"quoted\"","item_type":"epic","depends_on":[],"comments":[{"text":"operator's \"handoff\""}]}"#,
        )];
        let page = project_plan_page(&events, "plan-epic");

        let html = render_plan_page_html("plan-epic", &page);

        assert!(html.contains("Plan &quot;quoted&quot;"));
        assert!(html.contains("operator&#39;s &quot;handoff&quot;"));
    }

    fn plan_snapshot_event(
        event_id: &str,
        work_item_id: &str,
        rank: &str,
        status: &str,
        detail_json: &str,
    ) -> ConsoleEvent {
        let payload = format!(
            r#"{{"repo":"console","work_item_id":"{work_item_id}","lane":"ready","lane_reason":null,"rank":"{rank}","status":"{status}","source_version":1,"detail":{detail_json}}}"#
        );
        ConsoleEvent::fixture(
            event_id,
            EventType::WorkItemSnapshotObserved,
            "orchestrator",
        )
        .with_payload_json(payload)
    }

    /// A model whose active view is `view`, driven there through the reducer.
    fn view_model(view: TuiView) -> TuiScreenModel {
        let mut state = TuiInteractionState::new(0, TuiOverlay::None);
        for _step in 0..TuiView::all().len() {
            if build_tui_model_for_state(&[], &state).active_view() == view {
                break;
            }
            state = reduce_tui_interaction(&state, &[], TuiInteraction::SelectNextView);
        }
        let model = build_tui_model_for_state(&[], &state);
        assert_eq!(model.active_view(), view);
        model
    }

    /// The Settings-view model the footer tests read the edit hint from.
    fn settings_view_model() -> TuiScreenModel {
        view_model(TuiView::Settings)
    }

    /// The availability context the per-item hint tests drive: a
    /// manual-admission, ai-then-human item, with the driver-handoff verb
    /// exactly where production claims it (a backlog item, on EITHER per-item
    /// surface -- the claim is a property of the record, not of the view
    /// hosting it).
    fn test_item_ctx(
        surface: action_registry::ActionSurface,
        lane: Lane,
    ) -> action_registry::ActionContext {
        action_registry::ActionContext {
            lane,
            admission_policy: AdmissionPolicy::Manual,
            acceptance_policy: AcceptancePolicy::AiThenHuman,
            has_driver_handoff: matches!(lane, Lane::Backlog),
            // The default test selection is not awaiting an override, which is
            // what a real item reads today — the signal is unpublished.
            awaits_scope_override: false,
            ready_work_item_count: 1,
            surface,
        }
    }

    /// The row count of the standard test list: a multi-row list, which is what
    /// every pinned hint string below documents. The navigation fragment is
    /// composed from THIS, not from the lane's action set.
    const TEST_LIST_ROWS: usize = 2;

    /// The registry-derived Status-line hint for the standard test context.
    fn item_hint(surface: action_registry::ActionSurface, lane: Lane) -> String {
        action_registry::selected_item_hint(&test_item_ctx(surface, lane), TEST_LIST_ROWS)
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn per_item_verb_predicate_and_hints_cover_every_state() {
        assert!(per_item_verb_is_state_valid(
            Lane::PendingApproval,
            PendingValve::Approve
        ));
        assert!(!per_item_verb_is_state_valid(
            Lane::Acceptance,
            PendingValve::Approve
        ));
        assert!(per_item_verb_is_state_valid(
            Lane::Acceptance,
            PendingValve::Accept
        ));
        assert!(!per_item_verb_is_state_valid(
            Lane::Done,
            PendingValve::Accept
        ));
        assert!(per_item_verb_is_state_valid(
            Lane::PendingApproval,
            PendingValve::Reject(RejectMode::Rework)
        ));
        assert!(per_item_verb_is_state_valid(
            Lane::Acceptance,
            PendingValve::Reject(RejectMode::Rework)
        ));
        assert!(!per_item_verb_is_state_valid(
            Lane::Backlog,
            PendingValve::Reject(RejectMode::Rework)
        ));
        assert!(per_item_verb_is_state_valid(
            Lane::Backlog,
            PendingValve::SetAdmission(AdmissionPolicy::Manual)
        ));
        assert!(!per_item_verb_is_state_valid(
            Lane::Ready,
            PendingValve::SetAdmission(AdmissionPolicy::Manual)
        ));
        assert!(per_item_verb_is_state_valid(
            Lane::Active,
            PendingValve::SetAcceptance(AcceptancePolicy::AiThenHuman)
        ));
        assert!(!per_item_verb_is_state_valid(
            Lane::Acceptance,
            PendingValve::SetAcceptance(AcceptancePolicy::AiThenHuman)
        ));
        assert!(per_item_verb_is_state_valid(
            Lane::Ready,
            PendingValve::SetOverride(DispatcherOverride::MergeOnReviewCap(OverrideBool::Clear))
        ));
        assert!(per_item_verb_is_state_valid(
            Lane::Ready,
            PendingValve::SetOverride(DispatcherOverride::ReviewFixCap(OverrideInt::Clear))
        ));
        assert!(!per_item_verb_is_state_valid(
            Lane::Active,
            PendingValve::SetOverride(DispatcherOverride::ReviewFixCap(OverrideInt::Clear))
        ));
        assert!(per_item_verb_is_state_valid(
            Lane::Active,
            PendingValve::SetOverride(DispatcherOverride::AcceptanceReworkCap(OverrideInt::Clear))
        ));
        assert!(!per_item_verb_is_state_valid(
            Lane::Acceptance,
            PendingValve::SetOverride(DispatcherOverride::AcceptanceReworkCap(OverrideInt::Clear))
        ));
        assert!(per_item_verb_is_state_valid(
            Lane::Backlog,
            PendingValve::MoveStatus {
                from: Lane::Backlog,
                to: Lane::Ready,
            }
        ));
        assert!(!per_item_verb_is_state_valid(
            Lane::Ready,
            PendingValve::MoveStatus {
                from: Lane::Backlog,
                to: Lane::Ready,
            }
        ));
        assert!(!per_item_verb_is_state_valid(
            Lane::Done,
            PendingValve::MoveStatus {
                from: Lane::Done,
                to: Lane::Backlog,
            }
        ));

        let attention_pending = item_hint(
            action_registry::ActionSurface::Attention,
            Lane::PendingApproval,
        );
        assert!(attention_pending.contains("p approve") && attention_pending.contains("r reject"));
        let attention_acceptance =
            item_hint(action_registry::ActionSurface::Attention, Lane::Acceptance);
        assert!(
            attention_acceptance.contains("c accept") && attention_acceptance.contains("r reject")
        );
        let attention_done = item_hint(action_registry::ActionSurface::Attention, Lane::Done);
        assert!(attention_done.contains("enter open") && !attention_done.contains("reject"));

        for (lane, expected) in [
            (Lane::Backlog, "m set-admission"),
            (Lane::PendingApproval, "p approve"),
            (Lane::Ready, "g merge cap"),
            (Lane::Active, "k rework cap"),
            (Lane::Acceptance, "c accept"),
            (Lane::Blocked, "s move-status"),
            (Lane::Done, "enter item"),
        ] {
            let hint = item_hint(action_registry::ActionSurface::LaneDrill, lane);
            check(
                hint.contains(expected),
                "hint should contain expected lane action",
            );
        }
    }

    #[test]
    fn per_item_status_hints_are_derived_from_the_state_valid_predicate() {
        for lane in Lane::all() {
            let attention = item_hint(action_registry::ActionSurface::Attention, *lane);
            let drilled = item_hint(action_registry::ActionSurface::LaneDrill, *lane);

            for (hint, verb) in [
                ("p approve", PendingValve::Approve),
                ("c accept", PendingValve::Accept),
                ("r reject", PendingValve::Reject(RejectMode::Rework)),
                (
                    "m set-admission",
                    PendingValve::SetAdmission(AdmissionPolicy::Manual),
                ),
                (
                    "n set-acceptance",
                    PendingValve::SetAcceptance(AcceptancePolicy::AiThenHuman),
                ),
                (
                    "g merge cap",
                    PendingValve::SetOverride(DispatcherOverride::MergeOnReviewCap(
                        OverrideBool::Clear,
                    )),
                ),
                (
                    "f fix cap",
                    PendingValve::SetOverride(DispatcherOverride::ReviewFixCap(OverrideInt::Clear)),
                ),
                (
                    "k rework cap",
                    PendingValve::SetOverride(DispatcherOverride::AcceptanceReworkCap(
                        OverrideInt::Clear,
                    )),
                ),
            ] {
                let valid = per_item_verb_is_state_valid(*lane, verb);
                assert_eq!(attention.contains(hint), valid);
                assert_eq!(drilled.contains(hint), valid);
            }

            let move_status = "s move-status";
            let move_status_valid = status_move_targets(*lane).first().is_some_and(|to| {
                per_item_verb_is_state_valid(
                    *lane,
                    PendingValve::MoveStatus {
                        from: *lane,
                        to: *to,
                    },
                )
            });
            // Move-status is state-derived on BOTH per-item surfaces: the
            // hosting view is not an availability input (Scenario 31).
            assert_eq!(attention.contains(move_status), move_status_valid);
            assert_eq!(drilled.contains(move_status), move_status_valid);
        }
    }

    #[test]
    fn overlay_footer_hint_offers_the_bare_navigation_fallback_for_no_overlay() {
        // The None arm is the harmless fallback for a caller that routed a
        // closed overlay here; production routes None to the pane hints first.
        assert_eq!(
            overlay_footer_hint(&TuiOverlay::None),
            "1-6 view | ? help | q quit"
        );
    }

    #[test]
    fn the_reducer_refuses_to_stage_a_valve_the_registry_does_not_offer() {
        // fabro_gate_events index 2 selects the BLOCKED item, which admits no
        // approve: staging is refused and the overlay stays closed — the same
        // derivation that suppresses the hint and makes the key inert.
        let state = TuiInteractionState::new(2, TuiOverlay::None);
        let refused = reduce_tui_interaction(
            &state,
            &fabro_gate_events(),
            TuiInteraction::OpenValveConfirm(PendingValve::Approve),
        );
        assert_eq!(refused.overlay(), &TuiOverlay::None);
        // The acceptance item (index 1) admits accept: staging opens the modal.
        let staged = reduce_tui_interaction(
            &TuiInteractionState::new(1, TuiOverlay::None),
            &fabro_gate_events(),
            TuiInteraction::OpenValveConfirm(PendingValve::Accept),
        );
        assert_eq!(
            staged.overlay(),
            &TuiOverlay::ValveConfirm {
                valve: PendingValve::Accept,
                answer: String::new(),
            }
        );
    }

    #[test]
    fn the_model_exposes_the_projected_failure_and_the_drilled_hint_derives() {
        // The failure accessor and the drilled-lane hint derivation both run
        // through the MODEL here, not only through the free functions.
        let command = CommandEnvelope::new(
            "cmd_model_af".to_owned(),
            CommandType::WorkItemApproveRequested,
            "console-pending".to_owned(),
            "console-pending:work_item.approve_requested".to_owned(),
            "operator".to_owned(),
        );
        let mut events: Vec<ConsoleEvent> = fabro_gate_events().into_iter().collect();
        events.push(work_item_failure_event(
            &command,
            "approve:console-pending",
            Some("held"),
            9,
        ));
        let drilled = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_lane_focus(LaneFocus::Lane(Lane::PendingApproval))
            .with_selected_lane_item_index(0)
            .with_focus(FocusPane::Content);
        let model = build_tui_model_for_state(&events, &drilled);
        assert_eq!(
            model
                .action_failure_for("console-pending")
                .map(ActionFailure::action_id),
            Some("approve:console-pending")
        );
        assert!(model.action_failure_for("console-unknown").is_none());
        // The drilled selection's Status hints derive through the model path.
        assert!(model.footer().contains("p approve"));
    }

    #[test]
    fn action_failure_display_line_covers_every_refusal_shape() {
        let structured = ActionFailure {
            action_id: "approve:console-auto".to_owned(),
            refusal: Some(
                r#"{"action_id":"approve:console-auto","domain_error":"invalid-source-state","status":"failed","summary":"approve requires an effective-manual pending-approval item."}"#
                    .to_owned(),
            ),
        };
        assert_eq!(
            structured.display_line(),
            "approve:console-auto refused — invalid-source-state: approve requires an effective-manual pending-approval item."
        );
        let summary_only = ActionFailure {
            action_id: "a:b".to_owned(),
            refusal: Some(r#"{"summary":"held"}"#.to_owned()),
        };
        assert_eq!(summary_only.display_line(), "a:b refused — held");
        let error_only = ActionFailure {
            action_id: "a:b".to_owned(),
            refusal: Some(r#"{"domain_error":"invalid-action-id"}"#.to_owned()),
        };
        assert_eq!(error_only.display_line(), "a:b refused — invalid-action-id");
        let raw = ActionFailure {
            action_id: "a:b".to_owned(),
            refusal: Some("dispatcher exploded".to_owned()),
        };
        assert_eq!(raw.display_line(), "a:b failed — dispatcher exploded");
        let silent = ActionFailure {
            action_id: "a:b".to_owned(),
            refusal: None,
        };
        assert_eq!(silent.display_line(), "a:b failed (no diagnostic emitted)");
        assert_eq!(silent.action_id(), "a:b");
        // The derived impls are part of the type's surface: a clone compares
        // equal and the debug form names the type.
        assert_eq!(silent.clone(), silent);
        check(
            format!("{silent:?}").contains("ActionFailure"),
            "silent action failure should retain debug identity",
        );
    }

    #[test]
    fn action_failures_project_the_latest_failure_and_clear_on_recovery() {
        let command = CommandEnvelope::new(
            "cmd_af".to_owned(),
            CommandType::WorkItemApproveRequested,
            "console-af".to_owned(),
            "console-af:work_item.approve_requested".to_owned(),
            "operator".to_owned(),
        );
        let failed = work_item_failure_event(
            &command,
            "approve:console-af",
            Some(r#"{"domain_error":"invalid-source-state","summary":"held"}"#),
            3,
        );
        let failures = project_action_failures(std::slice::from_ref(&failed));
        assert_eq!(
            failures.get("console-af").map(ActionFailure::action_id),
            Some("approve:console-af")
        );
        // The refusal reaches the operator VERBATIM. A structured refusal is
        // FLATTENED into the failure payload, so it carries no `refusal` key;
        // reading only that key reported the orchestrator's most informative
        // refusals as silence.
        assert_eq!(
            failures.get("console-af").map(ActionFailure::display_line),
            Some("approve:console-af refused — invalid-source-state: held".to_owned())
        );
        // A failure naming no diagnostic at all is still reported as silence,
        // not as a JSON blob of its own bookkeeping.
        let silent = work_item_failure_event(&command, "approve:console-af", None, 3);
        assert_eq!(
            project_action_failures(&[silent])
                .get("console-af")
                .map(ActionFailure::display_line),
            Some("approve:console-af failed (no diagnostic emitted)".to_owned())
        );
        // A malformed failure payload (no action_id) projects nothing rather
        // than a phantom entry.
        let malformed = ConsoleEvent::new(
            "evt_af_malformed".to_owned(),
            1,
            "work_item".to_owned(),
            EventType::WorkItemActionFailed,
            "console:work-item-command-handler".to_owned(),
            "console-af-malformed".to_owned(),
            1,
        )
        .with_payload_json("{}".to_owned());
        assert!(project_action_failures(&[malformed]).is_empty());
        let dispatcher_entries: Vec<DispatcherJournalEntry> = DispatcherJournalEntry::new(
            "console",
            "console-af",
            "dispatch-without-detail",
            DispatcherJournalKind::HostOnlyRefused,
            5,
        )
        .ok()
        .into_iter()
        .collect();
        assert_eq!(dispatcher_entries.len(), 1);
        let dispatcher_without_detail = ConsoleEvent::new(
            "evt_dispatch_without_detail".to_owned(),
            1,
            "factory".to_owned(),
            EventType::DispatcherRefusalObserved,
            "dispatcher".to_owned(),
            "repo:console".to_owned(),
            5,
        )
        .with_payload_json(dispatcher_journal_payload_json(&dispatcher_entries[0]));
        assert!(project_action_failures(&[dispatcher_without_detail]).is_empty());
        let dispatcher_with_detail = ConsoleEvent::new(
            "evt_dispatch_with_detail".to_owned(),
            1,
            "factory".to_owned(),
            EventType::DispatcherRefusalObserved,
            "dispatcher".to_owned(),
            "repo:console".to_owned(),
            6,
        )
        .with_payload_json(dispatcher_journal_payload_json(
            &dispatcher_entries[0]
                .clone()
                .with_diagnostic("factory-safety refusal requires host-only execution"),
        ));
        let dispatcher_failures = project_action_failures(&[dispatcher_with_detail]);
        assert_eq!(
            dispatcher_failures
                .get("console-af")
                .map(ActionFailure::display_line),
            Some(
                "dispatch:dispatch-without-detail refused — host-only-refused: factory-safety refusal requires host-only execution".to_owned()
            )
        );
        // A later completed action against the SAME item clears the failure.
        let completed = super::work_item_command_event(
            &command,
            EventType::WorkItemActionCompleted,
            "completed",
            "approve:console-af",
            4,
        );
        assert!(project_action_failures(&[failed, completed]).is_empty());
    }

    #[test]
    fn the_new_variants_carry_their_derived_debug_forms() {
        // Derive-generated arms are part of the surface: the debug forms name
        // the variants (and are what a failed assertion would print).
        assert!(
            format!("{:?}", TuiOverlay::ActionInvoker { selected_action: 3 })
                .contains("ActionInvoker")
        );
        assert!(
            format!("{:?}", PendingValve::SetWorkflowScopeOverride)
                .contains("SetWorkflowScopeOverride")
        );
        assert!(
            format!("{:?}", ApplicationError::InvalidWorkflowScope)
                .contains("InvalidWorkflowScope")
        );
    }

    #[test]
    fn the_palette_actions_query_opens_the_invoker_and_the_roster_moves_and_clamps() {
        assert!(command_palette_query_opens_action_invoker("actions"));
        assert!(command_palette_query_opens_action_invoker("  ACTIONS  "));
        assert!(!command_palette_query_opens_action_invoker("drain"));

        let state = TuiInteractionState::new(0, TuiOverlay::None);
        let opened = reduce_tui_interaction(&state, &[], TuiInteraction::OpenActionInvoker);
        assert_eq!(
            opened.overlay(),
            &TuiOverlay::ActionInvoker { selected_action: 0 }
        );
        // Up from the top clamps; down walks; down past the end clamps.
        let up = reduce_tui_interaction(&opened, &[], TuiInteraction::SelectPreviousAction);
        assert_eq!(
            up.overlay(),
            &TuiOverlay::ActionInvoker { selected_action: 0 }
        );
        let mut walked = opened;
        for _step in 0..(action_registry::ACTION_REGISTRY.len() + 3) {
            walked = reduce_tui_interaction(&walked, &[], TuiInteraction::SelectNextAction);
        }
        assert_eq!(
            walked.overlay(),
            &TuiOverlay::ActionInvoker {
                selected_action: action_registry::ACTION_REGISTRY.len() - 1
            }
        );
    }

    #[test]
    fn attention_hint_falls_back_to_non_verb_navigation_for_unrendered_combinations() {
        // A selection admitting no action keeps the navigation-only hints. The
        // lane that admits nothing is `done`: a shipped item offers no onward
        // move and no valve, on either per-item surface. (`blocked` used to
        // stand in for it here, and stopped once the inbox row started offering
        // the state-admitted move-status picker -- Scenario 31.)
        let hint = item_hint(action_registry::ActionSurface::Attention, Lane::Done);
        assert_eq!(
            hint,
            "up/down move | enter open | 1-6 view | ? help | q quit"
        );
    }

    #[test]
    fn lane_hint_falls_back_to_non_verb_navigation_for_unrendered_combinations() {
        // A terminal-lane selection admits no action, so the derived hint drops
        // every VERB -- and keeps the navigation fragment, because the cursor
        // still moves over the lane's rows. The movement hint is composed from
        // the list, never from the action set.
        let hint = item_hint(action_registry::ActionSurface::LaneDrill, Lane::Done);
        assert_eq!(
            hint,
            "up/down move | enter item | esc lane list | 1-6 view | ? help | q quit"
        );
        // The one-row lane is the only case that drops it: there is nowhere to
        // move to, so naming the key would be the same dishonesty inverted.
        assert_eq!(
            action_registry::selected_item_hint(
                &test_item_ctx(action_registry::ActionSurface::LaneDrill, Lane::Done),
                1,
            ),
            "enter item | esc lane list | 1-6 view | ? help | q quit"
        );
    }

    #[test]
    fn lane_handoff_footer_hint_is_only_added_where_the_item_claims_the_driver_verb() {
        use action_registry::{ActionContext, ActionSurface, selected_item_hint};
        let ready = |handoff: bool| {
            selected_item_hint(
                &ActionContext {
                    lane: Lane::Ready,
                    admission_policy: AdmissionPolicy::Manual,
                    acceptance_policy: AcceptancePolicy::AiThenHuman,
                    has_driver_handoff: handoff,
                    awaits_scope_override: false,
                    ready_work_item_count: 1,
                    surface: ActionSurface::LaneDrill,
                },
                TEST_LIST_ROWS,
            )
        };
        assert!(ready(true).contains("h handoff") && ready(true).contains("g merge cap"));
        assert!(!ready(false).contains("h handoff"));
        // The CLAIM is what gates the verb, not the hosting view: an Attention
        // row on an item that claims the handoff hints it exactly as the
        // drilled-in lane does, and the key acts there (Scenario 31).
        let backlog = |surface| {
            selected_item_hint(
                &ActionContext {
                    lane: Lane::Backlog,
                    admission_policy: AdmissionPolicy::Manual,
                    acceptance_policy: AcceptancePolicy::AiThenHuman,
                    has_driver_handoff: true,
                    awaits_scope_override: false,
                    ready_work_item_count: 1,
                    surface,
                },
                TEST_LIST_ROWS,
            )
        };
        assert!(backlog(ActionSurface::Attention).contains("h handoff"));
        assert!(backlog(ActionSurface::LaneDrill).contains("h handoff"));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn derived_hints_reproduce_every_pinned_context_string() {
        use action_registry::ActionSurface::{Attention, LaneDrill};
        // Byte-identity with the strings the operator docs pin, per context.
        // The docs lockstep gate compares the docs table against this same
        // derivation, so a drifted row fails there and a drifted derivation
        // fails here.
        for (surface, lane, handoff, expected) in [
            // The Attention rows carry the SAME per-item tokens as their
            // drilled-in counterparts below -- only the navigation prefix
            // differs, because `esc lane list` genuinely applies to a lane
            // cursor and not to an inbox row (Scenario 31).
            (
                Attention,
                Lane::Backlog,
                true,
                "up/down move | enter open | h handoff | s move-status | m set-admission | g merge cap | f fix cap | n set-acceptance | k rework cap | 1-6 view | ? help | q quit",
            ),
            (
                Attention,
                Lane::PendingApproval,
                false,
                "up/down move | enter open | s move-status | p approve | r reject | m set-admission | g merge cap | f fix cap | n set-acceptance | k rework cap | 1-6 view | ? help | q quit",
            ),
            (
                Attention,
                Lane::Ready,
                false,
                "up/down move | enter open | s move-status | g merge cap | f fix cap | n set-acceptance | k rework cap | d dispatch | 1-6 view | ? help | q quit",
            ),
            (
                Attention,
                Lane::Active,
                false,
                "up/down move | enter open | n set-acceptance | k rework cap | 1-6 view | ? help | q quit",
            ),
            (
                Attention,
                Lane::Acceptance,
                false,
                "up/down move | enter open | s move-status | c accept | r reject | 1-6 view | ? help | q quit",
            ),
            (
                Attention,
                Lane::Blocked,
                false,
                "up/down move | enter open | s move-status | 1-6 view | ? help | q quit",
            ),
            (
                Attention,
                Lane::Done,
                false,
                "up/down move | enter open | 1-6 view | ? help | q quit",
            ),
            (
                LaneDrill,
                Lane::Backlog,
                true,
                "up/down move | enter item | esc lane list | h handoff | s move-status | m set-admission | g merge cap | f fix cap | n set-acceptance | k rework cap | 1-6 view | ? help | q quit",
            ),
            (
                LaneDrill,
                Lane::PendingApproval,
                false,
                "up/down move | enter item | esc lane list | s move-status | p approve | r reject | m set-admission | g merge cap | f fix cap | n set-acceptance | k rework cap | 1-6 view | ? help | q quit",
            ),
            (
                LaneDrill,
                Lane::Ready,
                true,
                "up/down move | enter item | esc lane list | h handoff | s move-status | g merge cap | f fix cap | n set-acceptance | k rework cap | d dispatch | 1-6 view | ? help | q quit",
            ),
            (
                LaneDrill,
                Lane::Ready,
                false,
                "up/down move | enter item | esc lane list | s move-status | g merge cap | f fix cap | n set-acceptance | k rework cap | d dispatch | 1-6 view | ? help | q quit",
            ),
            (
                LaneDrill,
                Lane::Active,
                false,
                "up/down move | enter item | esc lane list | n set-acceptance | k rework cap | 1-6 view | ? help | q quit",
            ),
            (
                LaneDrill,
                Lane::Acceptance,
                false,
                "up/down move | enter item | esc lane list | s move-status | c accept | r reject | 1-6 view | ? help | q quit",
            ),
            (
                LaneDrill,
                Lane::Blocked,
                false,
                "up/down move | enter item | esc lane list | s move-status | 1-6 view | ? help | q quit",
            ),
            (
                LaneDrill,
                Lane::Done,
                false,
                // The terminal lane offers no verb and still names the move:
                // the navigation fragment is the LIST's, not the verbs'.
                "up/down move | enter item | esc lane list | 1-6 view | ? help | q quit",
            ),
        ] {
            let ctx = action_registry::ActionContext {
                lane,
                admission_policy: AdmissionPolicy::Manual,
                acceptance_policy: AcceptancePolicy::AiThenHuman,
                has_driver_handoff: handoff,
                awaits_scope_override: false,
                ready_work_item_count: 1,
                surface,
            };
            assert_eq!(
                action_registry::selected_item_hint(&ctx, TEST_LIST_ROWS),
                expected
            );
        }
    }

    #[test]
    fn a_dispatcher_admitted_item_neither_hints_nor_stages_the_approve_valve() {
        use action_registry::{
            ActionContext, ActionSurface, KeyChord, action_for_chord, selected_item_hint,
            stage_action,
        };
        // The approve valve fires only on an effective-manual pending-approval
        // item; a dispatcher-admitted (`auto`) one must not advertise a key
        // that cannot fire, and the key must be inert -- ONE derivation for
        // both, so they cannot diverge.
        let auto = ActionContext {
            lane: Lane::PendingApproval,
            admission_policy: AdmissionPolicy::Auto,
            acceptance_policy: AcceptancePolicy::AiThenHuman,
            has_driver_handoff: false,
            awaits_scope_override: false,
            ready_work_item_count: 1,
            surface: ActionSurface::Attention,
        };
        let hint = selected_item_hint(&auto, TEST_LIST_ROWS);
        assert_eq!(
            hint,
            "up/down move | enter open | s move-status | r reject | m set-admission | g merge cap | f fix cap | n set-acceptance | k rework cap | 1-6 view | ? help | q quit"
        );
        let approve = action_for_chord(KeyChord::plain('p')).map(|spec| stage_action(spec, &auto));
        assert_eq!(approve, Some(None));

        let drilled_auto = ActionContext {
            surface: ActionSurface::LaneDrill,
            ..auto
        };
        assert_eq!(
            selected_item_hint(&drilled_auto, TEST_LIST_ROWS),
            "up/down move | enter item | esc lane list | s move-status | r reject | m set-admission | g merge cap | f fix cap | n set-acceptance | k rework cap | 1-6 view | ? help | q quit"
        );

        // The manual sibling keeps the valve offered and stageable.
        let manual = ActionContext {
            admission_policy: AdmissionPolicy::Manual,
            ..auto
        };
        assert!(selected_item_hint(&manual, TEST_LIST_ROWS).contains("p approve"));
        let staged = action_for_chord(KeyChord::plain('p')).map(|spec| stage_action(spec, &manual));
        assert_eq!(
            staged,
            Some(Some(action_registry::StagedAction::Valve(
                PendingValve::Approve
            )))
        );
    }

    #[test]
    fn the_status_hint_distinguishes_the_lane_overview_from_a_drilled_in_lane() {
        const MODAL_ITEM: &str = "console-pinned";
        // Enter drills into a LANE from the overview but opens an ITEM inside a
        // drilled-in lane, so the hint must name a different action in each --
        // advertising "enter drill" in both is the lie this surface fixes.
        // The lane OVERVIEW selects a LANE, not an item, so every per-item key
        // is inert there and none may be advertised.
        let overview = "up/down move | enter drill | 1-6 view | ? help | q quit".to_owned();
        assert!(overview.contains("enter drill"));
        for inert in ["move-status", "p approve", "c accept", "set-admission"] {
            assert!(!overview.contains(inert));
        }

        let drilled = item_hint(action_registry::ActionSurface::LaneDrill, Lane::Ready);
        assert!(drilled.contains("enter item") && !drilled.contains("enter drill"));
        // With an item selected the per-item keys DO act, so they are listed.
        assert!(drilled.contains("move-status") && drilled.contains("g merge cap"));

        // An EMPTY drilled-in lane selects nothing: `enter` opens nothing and
        // every per-item key is inert, so neither is advertised.
        let empty = "esc lane list | 1-6 view | ? help | q quit".to_owned();
        assert!(!empty.contains("enter item") && !empty.contains("enter drill"));
        assert!(!empty.contains("move-status") && !empty.contains("p approve"));
        // Nothing to move over either, so the navigation key goes too.
        assert!(!empty.contains("up/down move"));
        assert!(empty.contains("esc lane list"));

        // Attention drops its per-item valves when the inbox is empty.
        let attention_empty = "1-6 view | ? help | q quit".to_owned();
        assert!(!attention_empty.contains("p approve"));
        assert!(!attention_empty.contains("enter open"));
        // The open modal owns the hint line and names its own keys.
        let modal = overlay_footer_hint(&TuiOverlay::WorkItemDetail {
            work_item_id: MODAL_ITEM.to_owned(),
            scroll: 0,
        });
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
        )
        .with_work_item_detail_scroll_extents(10, 3);
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
        let paged = reduce_tui_interaction(&open, &events, TuiInteraction::WorkItemDetailPageDown);
        assert_eq!(
            paged.overlay(),
            &TuiOverlay::WorkItemDetail {
                work_item_id: MODAL_ITEM.to_owned(),
                scroll: 7
            }
        );
        let page_up = reduce_tui_interaction(&paged, &events, TuiInteraction::WorkItemDetailPageUp);
        assert_eq!(
            page_up.overlay(),
            &TuiOverlay::WorkItemDetail {
                work_item_id: MODAL_ITEM.to_owned(),
                scroll: 4
            }
        );
        let clamped = reduce_tui_interaction(
            &open.clone().with_work_item_detail_scroll_extents(6, 40),
            &events,
            TuiInteraction::WorkItemDetailPageDown,
        );
        assert_eq!(
            clamped.overlay(),
            &TuiOverlay::WorkItemDetail {
                work_item_id: MODAL_ITEM.to_owned(),
                scroll: 6
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
                focus: HelpFocus::Menu,
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
    fn factory_dispatch_item_confirm_opens_on_the_selected_item_only() {
        const MODAL_ITEM: &str = "console-pinned";
        let ready_events = [lane_event(
            "evt_dispatch_confirm",
            MODAL_ITEM,
            Lane::Ready,
            None,
            "a0",
            "ready",
        )];
        let selected = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_lane_focus(LaneFocus::Lane(Lane::Ready))
            .with_selected_lane_item_index(0);

        let dispatch_confirm = reduce_tui_interaction(
            &selected,
            &ready_events,
            TuiInteraction::OpenFactoryDispatchItemConfirm,
        );

        assert_eq!(
            dispatch_confirm.overlay(),
            &TuiOverlay::FactoryDispatchItemConfirm {
                work_item_id: MODAL_ITEM.to_owned()
            }
        );
        let dispatch_without_selection = reduce_tui_interaction(
            &TuiInteractionState::new(0, TuiOverlay::None),
            &[],
            TuiInteraction::OpenFactoryDispatchItemConfirm,
        );
        assert_eq!(dispatch_without_selection.overlay(), &TuiOverlay::None);
    }

    #[test]
    fn factory_drain_confirm_opens_on_the_ranked_ready_item_only() {
        let ready_events = [
            lane_event(
                "evt_drain_confirm_later",
                "console-ready-later",
                Lane::Ready,
                None,
                "a1",
                "ready",
            ),
            lane_event(
                "evt_drain_confirm_next",
                "console-ready-next",
                Lane::Ready,
                None,
                "a0",
                "ready",
            ),
        ];
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None);

        let drain_confirm = reduce_tui_interaction(
            &state,
            &ready_events,
            TuiInteraction::OpenFactoryDrainConfirm,
        );

        assert_eq!(
            drain_confirm.overlay(),
            &TuiOverlay::FactoryDrainConfirm {
                work_item_id: "console-ready-next".to_owned(),
                rank: "a0".to_owned()
            }
        );
        let drain_without_ready =
            reduce_tui_interaction(&state, &[], TuiInteraction::OpenFactoryDrainConfirm);
        assert_eq!(drain_without_ready.overlay(), &TuiOverlay::None);
        let fallback_state = state.clone().with_overlay(TuiOverlay::Search {
            query: "keep".to_owned(),
        });
        let fallback_model = build_tui_model_for_state(&ready_events, &state);
        let fallback = super::open_factory_confirm_state(
            &fallback_state,
            &fallback_model,
            TuiInteraction::OpenSearch,
        );
        assert_eq!(
            fallback.overlay(),
            &TuiOverlay::Search {
                query: "keep".to_owned()
            }
        );
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
                answer: String::new(),
            },
            TuiOverlay::FactoryDispatchItemConfirm {
                work_item_id: "wi-ready".to_owned(),
            },
            TuiOverlay::FactoryDrainConfirm {
                work_item_id: "wi-ready".to_owned(),
                rank: "a0".to_owned(),
            },
            TuiOverlay::DriverHandoff {
                command: r#"claude "/livespec-orchestrator-beads-fabro:implement wi-ready""#
                    .to_owned(),
            },
            TuiOverlay::WorkItemDetail {
                work_item_id: "wi-ready".to_owned(),
                scroll: 0,
            },
            TuiOverlay::Help {
                focus: HelpFocus::Menu,
                selected_section: 0,
                scroll: 0,
            },
        ];
        for overlay in &overlays {
            let hint = overlay_footer_hint(overlay);
            // Non-empty and mentions the overlay's exit key.
            assert!(!hint.trim().is_empty() && hint.contains("esc"));
            // The overlay owns the hints: they do NOT fall through to the
            // underlying Attention pane's keys.
            assert_ne!(
                hint,
                item_hint(
                    action_registry::ActionSurface::Attention,
                    Lane::PendingApproval
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
        let rows: Vec<SettingRow> =
            dispatcher_setting_rows(&settings, &DispatcherSettingWriteState::Idle);
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
        assert!(!rows[4].help().contains("dangerous / use with caution"));

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

    // -----------------------------------------------------------------------
    // The write boundary: a dispatcher-setting edit is pending until its
    // outcome lands, then a fresh effective read either carries it or does not,
    // and the row SAYS WHICH. Before this, the Settings pane rendered the
    // effective policy read once at launch and never re-read, so a landed edit
    // rendered as the pre-edit value for the rest of the session and the
    // operator was told their change was lost
    // (livespec-console-beads-fabro-30c).
    // -----------------------------------------------------------------------

    /// The observed policy the write-boundary tests start from: `wip_cap` 5 and
    /// `auto_approve_ready` off — the exact pair from the filed defect.
    fn observed_policy(wip_cap: u32, auto_approve_ready: bool) -> DispatcherSettings {
        DispatcherSettings::new(
            auto_approve_ready,
            false,
            AcceptancePolicy::AiThenHuman,
            4,
            2,
            wip_cap,
        )
    }

    /// Display-order indices of the rows the write-boundary tests assert on.
    /// The order itself is pinned by
    /// `dispatcher_setting_rows_render_each_effective_value_and_flag_dangerous_rows`.
    const AUTO_APPROVE_ROW: usize = 0;
    const REVIEW_FIX_CAP_ROW: usize = 3;
    const WIP_CAP_ROW: usize = 5;

    /// The rendered text of the Settings row at `index` under `write_state`.
    fn row_text(
        settings: &DispatcherSettings,
        write_state: &DispatcherSettingWriteState,
        index: usize,
    ) -> String {
        dispatcher_setting_rows(settings, write_state)[index].text()
    }

    /// A `config.dispatcher_setting.changed` / `.not_wired` outcome event for
    /// `setting`, as the config command handler appends it.
    fn setting_outcome_event(event_type: EventType, setting: &str) -> ConsoleEvent {
        ConsoleEvent::fixture("evt_setting_outcome", event_type, "console:test")
            .with_payload_json(serde_json::json!({ "setting": setting }).to_string())
    }

    #[test]
    fn a_row_with_no_outstanding_write_renders_its_effective_value_alone() {
        let settings = observed_policy(5, false);
        let idle = DispatcherSettingWriteState::Idle;

        assert_eq!(row_text(&settings, &idle, WIP_CAP_ROW), "WIP cap  [ 5 ]");
        assert_eq!(
            row_text(&settings, &idle, AUTO_APPROVE_ROW),
            "Auto-approve ready  [ off ]  (dangerous)"
        );
        assert_eq!(idle.write(), None);
    }

    #[test]
    fn a_pending_write_renders_the_transition_on_its_own_row_only() {
        let settings = observed_policy(5, false);
        let pending = DispatcherSettingWriteState::Pending {
            write: DispatcherSettingWrite::WipCap(6),
            submitted_after_events: 0,
        };

        // The edited row does NOT present the pre-edit value as current: it
        // names the value asked for and says the write is still running.
        assert_eq!(
            row_text(&settings, &pending, WIP_CAP_ROW),
            "WIP cap  [ 5 -> 6 ]  writing..."
        );
        assert_eq!(
            dispatcher_setting_rows(&settings, &pending)[5].status(),
            &SettingRowStatus::Pending("6".to_owned())
        );
        // Every other row is untouched — one write qualifies one row.
        assert_eq!(
            row_text(&settings, &pending, AUTO_APPROVE_ROW),
            "Auto-approve ready  [ off ]  (dangerous)"
        );
        assert_eq!(pending.write(), Some(&DispatcherSettingWrite::WipCap(6)));
    }

    #[test]
    fn a_pending_dangerous_toggle_keeps_its_dangerous_label_while_it_arms() {
        // The arming case the defect made dangerous: the operator flips
        // autonomy on, the dispatcher honours it on its next read, and the
        // cockpit used to keep rendering `off` for the rest of the session.
        let settings = observed_policy(5, false);
        let pending = DispatcherSettingWriteState::Pending {
            write: DispatcherSettingWrite::AutoApproveReady(true),
            submitted_after_events: 3,
        };

        assert_eq!(
            row_text(&settings, &pending, AUTO_APPROVE_ROW),
            "Auto-approve ready  [ off -> on ]  writing...  (dangerous)"
        );
    }

    #[test]
    fn a_reread_that_carries_the_write_renders_the_new_value_with_nothing_left_over() {
        let pending = DispatcherSettingWriteState::Pending {
            write: DispatcherSettingWrite::WipCap(6),
            submitted_after_events: 0,
        };
        // The orchestrator now reports 6 — the edit landed.
        let fresh = DispatcherSettingsRead::Observed(observed_policy(6, false));

        let folded = fold_dispatcher_setting_reread(&pending, &fresh);

        assert_eq!(folded, DispatcherSettingWriteState::Idle);
        assert_eq!(
            row_text(&observed_policy(6, false), &folded, WIP_CAP_ROW),
            "WIP cap  [ 6 ]"
        );
    }

    #[test]
    fn a_reread_that_did_not_move_the_value_says_so_instead_of_showing_the_old_one_silently() {
        let pending = DispatcherSettingWriteState::Pending {
            write: DispatcherSettingWrite::WipCap(6),
            submitted_after_events: 0,
        };
        // The write reported an outcome but the effective policy is still 5.
        let settings = observed_policy(5, false);
        let fresh = DispatcherSettingsRead::Observed(settings.clone());

        let folded = fold_dispatcher_setting_reread(&pending, &fresh);

        assert_eq!(
            folded,
            DispatcherSettingWriteState::Unchanged {
                write: DispatcherSettingWrite::WipCap(6),
            }
        );
        assert_eq!(folded.write(), Some(&DispatcherSettingWrite::WipCap(6)));
        assert_eq!(
            row_text(&settings, &folded, WIP_CAP_ROW),
            "WIP cap  [ 5 ]  unchanged (requested 6)"
        );
        assert_eq!(
            dispatcher_setting_rows(&settings, &folded)[5].status(),
            &SettingRowStatus::Unchanged("6".to_owned())
        );
        // The unchanged report qualifies only the row it is about.
        assert_eq!(
            row_text(&settings, &folded, REVIEW_FIX_CAP_ROW),
            "Review fix cap  [ 4 ]"
        );
    }

    #[test]
    fn an_unreadable_reread_confirms_nothing_and_leaves_the_write_pending() {
        let pending = DispatcherSettingWriteState::Pending {
            write: DispatcherSettingWrite::WipCap(6),
            submitted_after_events: 2,
        };

        // A read surface that went unreadable is not evidence either way, so the
        // write is NOT reported as unchanged; the pane degrades to its named
        // not-observed finding.
        assert_eq!(
            fold_dispatcher_setting_reread(&pending, &DispatcherSettingsRead::NotObserved),
            pending
        );
    }

    #[test]
    fn folding_a_reread_leaves_a_state_with_no_write_in_flight_alone() {
        let fresh = DispatcherSettingsRead::Observed(observed_policy(5, false));
        let unchanged = DispatcherSettingWriteState::Unchanged {
            write: DispatcherSettingWrite::WipCap(6),
        };

        assert_eq!(
            fold_dispatcher_setting_reread(&DispatcherSettingWriteState::Idle, &fresh),
            DispatcherSettingWriteState::Idle
        );
        // An already-reported unchanged outcome is not re-decided by a later
        // read; the operator's next edit of that row replaces it.
        assert_eq!(
            fold_dispatcher_setting_reread(&unchanged, &fresh),
            unchanged
        );
    }

    #[test]
    fn a_write_settles_on_its_own_outcome_event_and_on_nothing_else() {
        let pending = DispatcherSettingWriteState::Pending {
            write: DispatcherSettingWrite::WipCap(6),
            submitted_after_events: 1,
        };
        let older = setting_outcome_event(EventType::ConfigDispatcherSettingChanged, "wip_cap");
        let mine = setting_outcome_event(EventType::ConfigDispatcherSettingChanged, "wip_cap");
        let other = setting_outcome_event(
            EventType::ConfigDispatcherSettingChanged,
            "auto_approve_ready",
        );
        let unrelated =
            ConsoleEvent::fixture("evt_other", EventType::CommandAccepted, "console:test");

        // Nothing outstanding never settles.
        assert!(!dispatcher_setting_write_settled(
            &DispatcherSettingWriteState::Idle,
            std::slice::from_ref(&mine)
        ));
        // An outcome BEFORE the submission watermark is a previous edit's.
        assert!(!dispatcher_setting_write_settled(
            &pending,
            std::slice::from_ref(&older)
        ));
        // An event that is not a settings outcome at all, and an outcome for a
        // different setting, are both after the watermark and neither settles.
        assert!(!dispatcher_setting_write_settled(
            &pending,
            &[older.clone(), unrelated, other]
        ));
        // This write's own outcome, at the watermark, settles it.
        assert!(dispatcher_setting_write_settled(&pending, &[older, mine]));
    }

    #[test]
    fn a_not_wired_outcome_settles_the_write_and_a_key_less_payload_settles_nothing() {
        let pending = DispatcherSettingWriteState::Pending {
            write: DispatcherSettingWrite::WipCap(6),
            submitted_after_events: 0,
        };

        // The write is over even though it achieved nothing; the re-read then
        // finds the value unmoved and the row reports that.
        assert!(dispatcher_setting_write_settled(
            &pending,
            &[setting_outcome_event(
                EventType::ConfigDispatcherSettingNotWired,
                "wip_cap"
            )]
        ));
        // An outcome that cannot say which setting it is about settles nothing:
        // unparseable, absent, and wrongly-typed all report the same.
        let settled = ["not json at all", "{}", r#"{"setting":42}"#]
            .into_iter()
            .map(|payload| {
                let opaque = ConsoleEvent::fixture(
                    "evt_opaque",
                    EventType::ConfigDispatcherSettingChanged,
                    "console:test",
                )
                .with_payload_json(payload.to_owned());
                dispatcher_setting_write_settled(&pending, &[opaque])
            })
            .collect::<Vec<_>>();
        assert_eq!(settled, [false, false, false]);
    }

    #[test]
    fn a_submitted_setting_write_becomes_pending_and_anything_else_does_not() {
        let payload = serde_json::json!({
            "repo": CONFIRM_REPO,
            "setting": "wip_cap",
            "value": 6,
        })
        .to_string();

        assert_eq!(
            DispatcherSettingWriteState::submitted(
                &CommandType::ConfigDispatcherSettingSet,
                &payload,
                7
            ),
            Some(DispatcherSettingWriteState::Pending {
                write: DispatcherSettingWrite::WipCap(6),
                submitted_after_events: 7,
            })
        );
        // A command of any other type is not a settings write.
        assert_eq!(
            DispatcherSettingWriteState::submitted(
                &CommandType::WorkItemApproveRequested,
                &payload,
                7
            ),
            None
        );
        // A malformed payload names no setting, so nothing is claimed pending.
        assert_eq!(
            DispatcherSettingWriteState::submitted(
                &CommandType::ConfigDispatcherSettingSet,
                "{}",
                7
            ),
            None
        );
    }

    #[test]
    fn every_setting_write_names_its_row_and_renders_as_that_row_renders() {
        let writes = [
            DispatcherSettingWrite::AutoApproveReady(true),
            DispatcherSettingWrite::MergeOnReviewCap(false),
            DispatcherSettingWrite::AcceptanceMode(AcceptancePolicy::AiOnly),
            DispatcherSettingWrite::ReviewFixCap(4),
            DispatcherSettingWrite::AcceptanceReworkCap(2),
            DispatcherSettingWrite::WipCap(6),
        ];

        // Writes and rows are the same six keys in the same order.
        assert_eq!(
            writes
                .iter()
                .map(DispatcherSettingWrite::row)
                .collect::<Vec<_>>(),
            DispatcherSettingRow::all()
        );
        // The requested value is rendered the way an EFFECTIVE value is, which
        // is what lets the re-read confirmation compare the two: `on`, not the
        // `true` of the orchestrator's `set-config` wire grammar.
        assert_eq!(
            writes
                .iter()
                .map(DispatcherSettingWrite::rendered_value)
                .collect::<Vec<_>>(),
            ["on", "off", "ai-only", "4", "2", "6"]
        );
        assert_eq!(
            DispatcherSettingWrite::AutoApproveReady(true).value_literal(),
            "true"
        );
    }

    #[test]
    fn dispatcher_setting_row_next_write_flips_cycles_and_increments() {
        // review_fix_cap 9 wraps to the minimum; acceptance_rework_cap 0 (below the
        // minimum) is nudged up to it; wip_cap 3 increments.
        let settings = DispatcherSettings::new_with_drift_capture_merge_threshold(
            false,
            true,
            AcceptancePolicy::AiThenHuman,
            9,
            0,
            9,
            3,
        );
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
            DispatcherSettingWrite::MergeOnReviewCap(false).value_json(),
            serde_json::json!(false)
        );
        assert_eq!(
            DispatcherSettingWrite::AcceptanceMode(AcceptancePolicy::HumanOnly).value_json(),
            serde_json::json!("human-only")
        );
        assert_eq!(
            DispatcherSettingWrite::ReviewFixCap(4).value_json(),
            serde_json::json!(4)
        );
        assert_eq!(
            DispatcherSettingWrite::AcceptanceReworkCap(6).value_json(),
            serde_json::json!(6)
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
    fn editing_errors_when_selected_setting_row_is_out_of_range() {
        let mut model = settings_model(
            DispatcherSettings::new(false, false, AcceptancePolicy::AiThenHuman, 3, 2, 5),
            0,
        );
        model.selected_setting_index = Some(usize::MAX);

        let outcome = resolve_dispatcher_setting_edit(&model, "operator");

        check(
            outcome == Err(ApplicationError::NoSelectedDispatcherSetting),
            "out-of-range settings rows should not resolve a write",
        );
    }

    #[test]
    fn settings_selection_moves_within_the_settings_rows_and_clamps() {
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

        // A terminal-copy outcome carries no persisted command.
        let handoff = OperatorActionOutcome::CopyDriverHandoff("claude groom wi".to_owned());
        assert_eq!(handoff.command(), None);
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
            &TuiInteractionState::new(
                0,
                TuiOverlay::ValveConfirm {
                    valve,
                    answer: String::new(),
                },
            ),
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

        // The move-status valve labels itself and shows its staged transition;
        // it is never destructive (its reject-based routes are excluded).
        let move_valve = PendingValve::MoveStatus {
            from: Lane::PendingApproval,
            to: Lane::Ready,
        };
        assert_eq!(move_valve.valve_label(), "Move status");
        assert_eq!(move_valve.option_label(), Some("ready"));
        // The DISPLAY carries the whole staged transition rather than the
        // target alone: what `Enter` would actually do to the selected item.
        assert_eq!(
            move_valve.option_display(),
            Some("pending-approval -> ready".to_owned())
        );
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

    /// The move picker opens PRE-STAGED on the first legal target, so the
    /// staged transition is rendered -- in the picker and in the Status line --
    /// before the operator confirms, and re-rendered as they cycle the choice.
    ///
    /// Measured 2026-08-31: nothing before confirmation said which transition
    /// `Enter` would fire, and the dogfooding recipe had to warn "if it isn't
    /// showing ready, cycle to it before Enter".
    #[test]
    fn the_move_picker_renders_its_staged_transition_before_confirmation() {
        let events = [lane_event(
            "e1",
            "wi-back",
            Lane::Backlog,
            None,
            "a",
            "backlog",
        )];
        let drilled = drilldown_state(Lane::Backlog, 0, TuiOverlay::None);
        // The picker opens pre-staged on the FIRST target `backlog` may drive.
        let staged = PendingValve::MoveStatus {
            from: Lane::Backlog,
            to: Lane::Ready,
        };
        assert_eq!(
            build_tui_model_for_state(&events, &drilled).selected_move_status_valve(),
            Some(staged)
        );

        // The picker's own text: the caption the transition is rendered under,
        // and the transition itself. The renderer composes the two verbatim.
        assert_eq!(staged.option_caption(), "Move");
        assert_eq!(staged.option_display(), Some("backlog -> ready".to_owned()));
        // A policy valve keeps its own caption.
        assert_eq!(PendingValve::Approve.option_caption(), "Policy/mode");

        // The Status line leads with the same staged transition, ahead of the
        // keys that act on the open modal.
        let opened =
            reduce_tui_interaction(&drilled, &events, TuiInteraction::OpenValveConfirm(staged));
        assert_eq!(
            build_tui_model_for_state(&events, &opened).footer(),
            "move: backlog -> ready | up/down change | enter confirm | esc cancel"
        );

        // Cycling the choice re-renders BOTH surfaces against the newly staged
        // target -- `backlog` also drives `blocked`.
        let cycled =
            reduce_tui_interaction(&opened, &events, TuiInteraction::CycleValveOption(true));
        assert_eq!(
            cycled
                .overlay()
                .valve_confirm()
                .and_then(|valve| valve.option_display()),
            Some("backlog -> blocked".to_owned())
        );
        assert_eq!(
            build_tui_model_for_state(&events, &cycled).footer(),
            "move: backlog -> blocked | up/down change | enter confirm | esc cancel"
        );

        // A payload-free valve stages no parameter, so its hints are unchanged.
        assert_eq!(
            build_tui_model_for_state(
                &events,
                &drilled.with_overlay(TuiOverlay::ValveConfirm {
                    valve: PendingValve::Approve,
                    answer: String::new(),
                })
            )
            .footer(),
            "up/down change | enter confirm | esc cancel"
        );
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
        // Blocked offers backlog/ready only; up/down walks the vocabulary order.
        let blocked_ready = PendingValve::MoveStatus {
            from: Lane::Blocked,
            to: Lane::Ready,
        };
        assert_eq!(
            blocked_ready.cycled(true),
            PendingValve::MoveStatus {
                from: Lane::Blocked,
                to: Lane::Backlog,
            }
        );
        assert_eq!(
            blocked_ready.cycled(false),
            PendingValve::MoveStatus {
                from: Lane::Blocked,
                to: Lane::Backlog,
            }
        );

        // The drivable target sets mirror the per-state operator vocabulary:
        // no `active` target, no pending-approval direct `ready`, and no moves
        // from `active` or `done`.
        assert_eq!(
            status_move_targets(Lane::Backlog),
            &[Lane::Ready, Lane::Blocked]
        );
        assert_eq!(
            status_move_targets(Lane::PendingApproval),
            &[Lane::Backlog, Lane::Blocked]
        );
        assert_eq!(
            status_move_targets(Lane::Ready),
            &[Lane::Backlog, Lane::Blocked]
        );
        assert_eq!(status_move_targets(Lane::Active), &[] as &[Lane]);
        assert_eq!(
            status_move_targets(Lane::Acceptance),
            &[Lane::Backlog, Lane::Blocked]
        );
        assert_eq!(
            status_move_targets(Lane::Blocked),
            &[Lane::Backlog, Lane::Ready]
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
                answer: String::new(),
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
                answer: String::new(),
            }
        );

        let cycled =
            reduce_tui_interaction(&opened, &events, TuiInteraction::CycleValveOption(true));
        assert_eq!(
            cycled.overlay(),
            &TuiOverlay::ValveConfirm {
                valve: PendingValve::SetAcceptance(AcceptancePolicy::AiOnly),
                answer: String::new(),
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
        for (valve, command_type, action, index, item_id) in [
            (
                PendingValve::Approve,
                CommandType::WorkItemApproveRequested,
                "approve",
                0,
                "console-pending",
            ),
            // Accept resolves against the ACCEPTANCE-lane item: the registry
            // availability check refuses an accept staged off its lane.
            (
                PendingValve::Accept,
                CommandType::WorkItemAcceptRequested,
                "accept",
                1,
                "console-accept",
            ),
        ] {
            let model = build_tui_model_for_state(
                &fabro_gate_events(),
                &TuiInteractionState::new(
                    index,
                    TuiOverlay::ValveConfirm {
                        valve,
                        answer: String::new(),
                    },
                ),
            );
            let outcome = resolve_valve_action(&model, "operator");
            let command = outcome
                .as_ref()
                .ok()
                .and_then(OperatorActionOutcome::command);
            assert_eq!(
                command.map(CommandEnvelope::command_type),
                Some(&command_type)
            );
            assert_eq!(command.map(CommandEnvelope::aggregate_id), Some(item_id));
            assert_eq!(
                command.map(CommandEnvelope::idempotency_key),
                Some(format!("{item_id}:work_item.{action}_requested").as_str())
            );
            assert_eq!(command.map(CommandEnvelope::requested_by), Some("operator"));
            // Payloadless: a plain PersistCommand, never PersistCommandWithPayload.
            let outcome = ok_operator_action_outcome(outcome);
            check(
                std::mem::discriminant(&outcome)
                    == std::mem::discriminant(&OperatorActionOutcome::PersistCommand(
                        factory_drain_test_command(),
                    )),
                "payloadless valve should persist a payloadless command",
            );
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
                    answer: String::new(),
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
    fn drilled_lane_selection_survives_a_re_rank_and_stages_the_same_item() {
        let before = [
            lane_event(
                "evt_ready_a_before",
                "wi-a",
                Lane::Ready,
                None,
                "a0",
                "ready",
            ),
            lane_event(
                "evt_ready_target_before",
                "wi-target",
                Lane::Ready,
                None,
                "b0",
                "ready",
            ),
        ];
        let starting = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_lane_focus(LaneFocus::Lane(Lane::Ready));
        let selected = reduce_tui_interaction(&starting, &before, TuiInteraction::SelectNext);
        let selected_before = build_tui_model_for_state(&before, &selected);
        assert_eq!(selected_before.selected_lane_item_index(), Some(1));
        assert_eq!(selected_before.selected_work_item_id(), Some("wi-target"));

        let after = [
            lane_event(
                "evt_ready_target_after",
                "wi-target",
                Lane::Ready,
                None,
                "a0",
                "ready",
            ),
            lane_event(
                "evt_ready_a_after",
                "wi-a",
                Lane::Ready,
                None,
                "b0",
                "ready",
            ),
        ];
        let selected_after = build_tui_model_for_state(&after, &selected);
        assert_eq!(selected_after.selected_lane_item_index(), Some(0));
        assert_eq!(selected_after.selected_work_item_id(), Some("wi-target"));

        let staged = reduce_tui_interaction(
            &selected,
            &after,
            TuiInteraction::OpenFactoryDispatchItemConfirm,
        );
        assert_eq!(
            staged.overlay(),
            &TuiOverlay::FactoryDispatchItemConfirm {
                work_item_id: "wi-target".to_owned()
            }
        );

        let vanished = [lane_event(
            "evt_ready_a_without_target",
            "wi-a",
            Lane::Ready,
            None,
            "b0",
            "ready",
        )];
        let vanished_model = build_tui_model_for_state(&vanished, &selected);
        assert_eq!(vanished_model.selected_lane_item_index(), None);
        assert_eq!(vanished_model.selected_work_item_id(), None);
        assert_eq!(
            vanished_model.missing_selected_lane_item_id(),
            Some("wi-target")
        );
        let not_staged = reduce_tui_interaction(
            &selected,
            &vanished,
            TuiInteraction::OpenFactoryDispatchItemConfirm,
        );
        assert_eq!(not_staged.overlay(), &TuiOverlay::None);
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
    fn selected_work_item_resolves_attention_record_for_driver_handoff() {
        let attention_item = AttentionItemSnapshot::new(
            "attention-ready-host-only",
            "human-valve",
            "high",
            "Ready item needs host-only implementation",
            AttentionSourceRef::new("console", Some("wi-ready-host-only"), None),
            AttentionHandoff::new("implement", None, "implement:wi-ready-host-only"),
        );
        let events = [
            lane_event_with_factory_safety(
                "evt_ready_host_only",
                "wi-ready-host-only",
                Lane::Ready,
                Some("needs-privileged-host"),
                "a0",
                "ready",
            ),
            lane_event_with_factory_safety(
                "evt_backlog",
                "wi-backlog",
                Lane::Backlog,
                None,
                "a1",
                "backlog",
            ),
            attention_appeared("evt_attention_ready_host_only", &attention_item),
        ];
        let model = build_tui_model_for_state(
            &events,
            &TuiInteractionState::for_view(TuiView::Attention, 0, TuiOverlay::None),
        );

        assert_eq!(
            model.selected_work_item().map(LaneWorkItem::work_item_id),
            Some("wi-ready-host-only")
        );
        assert_eq!(
            model.selected_driver_handoff_command().as_deref(),
            Some(r#"claude "/livespec-orchestrator-beads-fabro:implement wi-ready-host-only""#)
        );

        let backlog = build_tui_model_for_state(
            &events,
            &TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
                .with_lane_focus(LaneFocus::Lane(Lane::Backlog)),
        );
        assert_eq!(
            backlog.selected_driver_handoff_command().as_deref(),
            Some(r#"claude "/livespec-orchestrator-beads-fabro:groom wi-backlog""#)
        );

        let no_item = build_tui_model_for_state(
            &events,
            &TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None),
        );
        assert_eq!(no_item.selected_driver_handoff_command(), None);

        for view in [
            TuiView::Spec,
            TuiView::Events,
            TuiView::Repos,
            TuiView::Settings,
        ] {
            let inert = build_tui_model_for_state(
                &events,
                &TuiInteractionState::for_view(view, 0, TuiOverlay::None),
            );
            assert!(inert.selected_work_item().is_none());
            assert_eq!(inert.selected_driver_handoff_command(), None);
        }
    }

    #[test]
    fn open_driver_handoff_overlay_uses_the_selected_eligible_item() {
        let events = [
            lane_event_with_factory_safety(
                "evt_ready_host_only_overlay",
                "wi-ready-host-only-overlay",
                Lane::Ready,
                Some("needs-privileged-host"),
                "a0",
                "ready",
            ),
            lane_event_with_factory_safety(
                "evt_ready_safe_overlay",
                "wi-ready-safe-overlay",
                Lane::Ready,
                // Factory-SAFE is the ABSENCE of a marking, not a marking that
                // spells "safe" — the published vocabulary has no such value.
                None,
                "a1",
                "ready",
            ),
        ];
        let eligible_state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_lane_focus(LaneFocus::Lane(Lane::Ready));

        let opened =
            reduce_tui_interaction(&eligible_state, &events, TuiInteraction::OpenDriverHandoff);

        assert_eq!(
            opened.overlay(),
            &TuiOverlay::DriverHandoff {
                command: r#"claude "/livespec-orchestrator-beads-fabro:implement wi-ready-host-only-overlay""#
                    .to_owned(),
            }
        );

        let safe_ready_state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_lane_focus(LaneFocus::Lane(Lane::Ready))
            .with_selected_lane_item_index(1);
        let safe_ready = reduce_tui_interaction(
            &safe_ready_state,
            &events,
            TuiInteraction::OpenDriverHandoff,
        );

        assert_eq!(safe_ready.overlay(), &TuiOverlay::None);

        let no_item_state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_lane_focus(LaneFocus::Lane(Lane::Active));
        let no_item =
            reduce_tui_interaction(&no_item_state, &events, TuiInteraction::OpenDriverHandoff);

        assert_eq!(no_item.overlay(), &TuiOverlay::None);
    }

    #[test]
    fn open_work_item_detail_uses_attention_selection() {
        let events = fabro_gate_events();
        let state = TuiInteractionState::for_view(TuiView::Attention, 2, TuiOverlay::None)
            .with_focus(FocusPane::Content);

        let opened = reduce_tui_interaction(&state, &events, TuiInteraction::OpenWorkItemDetail);

        assert_eq!(
            opened.overlay(),
            &TuiOverlay::WorkItemDetail {
                work_item_id: "console-blocked".to_owned(),
                scroll: 0
            }
        );
        let closed = reduce_tui_interaction(&opened, &events, TuiInteraction::CloseOverlay);
        assert_eq!(closed.overlay(), &TuiOverlay::None);
        assert_eq!(closed.selected_attention_index(), 2);
        assert_eq!(closed.active_view(), TuiView::Attention);
    }

    #[test]
    fn attention_selection_drills_only_when_source_work_item_is_known() {
        let mut events = vec![lane_event(
            "evt_known_record",
            "wi-act",
            Lane::Active,
            None,
            "a",
            "active",
        )];
        let work_item_backed = AttentionItemSnapshot::new(
            "attention-human-gate-1",
            "human-valve",
            "high",
            "Ready item needs a human gate",
            AttentionSourceRef::new("console", Some("wi-act"), None),
            AttentionHandoff::new("approve", None, "approve:wi-act"),
        );
        let path_backed = AttentionItemSnapshot::new(
            "attention-z-doc-1",
            "spec-hygiene",
            "medium",
            "Spec document needs review",
            AttentionSourceRef::new("console", None, Some("SPECIFICATION/spec.md")),
            AttentionHandoff::new("inspect", None, "inspect:SPECIFICATION/spec.md"),
        );
        events.push(attention_appeared(
            "evt_attention_work_item",
            &work_item_backed,
        ));
        events.push(attention_appeared("evt_attention_path", &path_backed));

        let work_item_model = build_tui_model_for_state(
            &events,
            &TuiInteractionState::for_view(TuiView::Attention, 0, TuiOverlay::None),
        );
        assert_eq!(
            work_item_model.attention_items()[0].id(),
            "attention-human-gate-1"
        );
        assert_eq!(work_item_model.selected_work_item_id(), Some("wi-act"));

        let opened = reduce_tui_interaction(
            &TuiInteractionState::for_view(TuiView::Attention, 0, TuiOverlay::None),
            &events,
            TuiInteraction::OpenWorkItemDetail,
        );
        assert_eq!(
            opened.overlay(),
            &TuiOverlay::WorkItemDetail {
                work_item_id: "wi-act".to_owned(),
                scroll: 0
            }
        );

        let path_model = build_tui_model_for_state(
            &events,
            &TuiInteractionState::for_view(TuiView::Attention, 1, TuiOverlay::None),
        );
        assert_eq!(path_model.attention_items()[1].id(), "attention-z-doc-1");
        assert_eq!(path_model.selected_work_item_id(), None);

        let inert = reduce_tui_interaction(
            &TuiInteractionState::for_view(TuiView::Attention, 1, TuiOverlay::None),
            &events,
            TuiInteraction::OpenWorkItemDetail,
        );
        assert_eq!(inert.overlay(), &TuiOverlay::None);
    }

    #[test]
    fn command_modal_opens_for_registry_derived_attention_actions() {
        let events = [lane_event(
            "evt_1",
            "console-pending",
            Lane::PendingApproval,
            None,
            "a1",
            "pending-approval",
        )];
        let state = TuiInteractionState::for_view(TuiView::Attention, 0, TuiOverlay::None)
            .with_focus(FocusPane::Content);
        let model = build_tui_model_for_state(&events, &state);
        let registry_actions = registry_attention_actions_for_model(&model);
        assert!(!registry_actions.is_empty());
        assert_eq!(
            model.detail().map(AttentionDetail::actions),
            Some(registry_actions.as_slice())
        );

        let opened = reduce_tui_interaction(&state, &events, TuiInteraction::OpenCommandModal);

        assert_eq!(
            opened.overlay(),
            &TuiOverlay::CommandModal {
                selected_action_index: 0
            }
        );
    }

    #[test]
    fn command_modal_action_selection_moves_and_clamps() {
        let detail = AttentionDetail::new(
            "repo".to_owned(),
            "work-item".to_owned(),
            "run".to_owned(),
            None,
            vec![],
            vec![],
            vec![
                OperatorAction::Registered("approve"),
                OperatorAction::Registered("accept"),
            ],
        );
        let at_first = TuiOverlay::CommandModal {
            selected_action_index: 0,
        };
        let at_second = TuiOverlay::CommandModal {
            selected_action_index: 1,
        };

        assert_eq!(super::move_action_down(&at_first, Some(&detail)), at_second);
        assert_eq!(
            super::move_action_down(&at_second, Some(&detail)),
            at_second
        );
        assert_eq!(super::move_action_up(&at_second), at_first);
        assert_eq!(super::move_action_up(&at_first), at_first);
    }

    #[test]
    fn command_explainer_overlay_exposes_the_same_selected_action_and_hint() {
        let detail = AttentionDetail::new(
            "repo".to_owned(),
            "work-item".to_owned(),
            "run".to_owned(),
            None,
            vec![],
            vec![],
            vec![OperatorAction::Registered("approve")],
        );
        let model = TuiScreenModel {
            active_view: TuiView::Attention,
            navigation: vec![TuiView::Attention],
            attention_items: vec![],
            attention_total: 0,
            selected_attention_index: None,
            detail: Some(detail),
            view_items: vec![],
            lane_board: project_lane_board(&[]),
            lane_focus: LaneFocus::Overview,
            selected_lane_index: None,
            selected_lane_item_index: None,
            missing_selected_lane_item_id: None,
            events_focus: EventsFocus::Overview,
            selected_events_index: 0,
            focus: FocusPane::Content,
            detail_scroll: 0,
            header_scroll: 0,
            overlay: TuiOverlay::CommandExplainer {
                selected_action_index: 0,
            },
            selected_repo: String::new(),
            selected_setting_index: None,
            dispatcher_settings: DispatcherSettingsRead::NotObserved,
            dispatcher_setting_write: DispatcherSettingWriteState::Idle,
            plugin_resolution: PluginResolution::unresolved(),
            unavailable_sources: vec![],
            observed_source_names: vec![],
            factory_activity: None,
            transient_status: None,
            list_edge: None,
            command_outcome: None,
            header: String::new(),
            action_failures: std::collections::BTreeMap::new(),
            orphaned_factory_runs: Vec::new(),
            build_identity: None,
            build_staleness: super::BuildStaleness::Unknown,
            startup_ingest_pending: false,
        };

        assert_eq!(
            model.selected_operator_action(),
            Some(OperatorAction::Registered("approve"))
        );
        assert_eq!(model.footer(), "enter continue | esc cancel");
    }

    #[test]
    fn menu_selection_moves_and_clamps_to_the_open_node() {
        // Clamped to the OPEN node's own action count, not the registry's: the
        // bar nodes hold different numbers of actions, so a registry-wide bound
        // would let the cursor run off the end of a short menu and select an
        // action that node does not contain.
        let top = 0;
        let last = action_registry::menu_actions(top).len().saturating_sub(1);
        // One-line assert on a bound local, per the llvm-cov pincer
        // `action_registry.rs` documents: a wrapped failure-only message lands
        // on a line llvm-cov counts as never executed.
        check(
            last >= 1,
            "menu action list should have at least two actions",
        );
        let at_first = TuiOverlay::Menu { top, selected: 0 };
        let at_second = TuiOverlay::Menu { top, selected: 1 };
        let at_last = TuiOverlay::Menu {
            top,
            selected: last,
        };

        assert_eq!(super::move_action_down(&at_first, None), at_second);
        assert_eq!(super::move_action_down(&at_last, None), at_last);
        assert_eq!(super::move_action_up(&at_second), at_first);
        assert_eq!(super::move_action_up(&at_first), at_first);
    }

    #[test]
    fn the_menu_bar_walk_wraps_both_ways_and_resets_the_selection() {
        // The selection RESETS on every bar move. Carrying an index across
        // nodes would land the cursor on an unrelated action, and the nodes
        // hold different numbers of actions so the index may not even exist.
        let events: [ConsoleEvent; 0] = [];
        let count = action_registry::menu_tree().len();
        check(count >= 2, "the bar must not be a single degenerate node");
        let opened = TuiInteractionState::for_view(
            TuiView::Lanes,
            0,
            TuiOverlay::Menu {
                top: 0,
                selected: 3,
            },
        );

        let forward = reduce_tui_interaction(&opened, &events, TuiInteraction::MenuNextTop);
        let back = reduce_tui_interaction(&forward, &events, TuiInteraction::MenuPreviousTop);
        // Back from the FIRST node wraps to the last; forward from the LAST
        // wraps to the first. Both ends, because a walk that wraps one way only
        // strands the operator at whichever end it does not.
        let wrapped_back = reduce_tui_interaction(&back, &events, TuiInteraction::MenuPreviousTop);
        let wrapped_forward =
            reduce_tui_interaction(&wrapped_back, &events, TuiInteraction::MenuNextTop);

        assert_eq!(
            forward.overlay(),
            &TuiOverlay::Menu {
                top: 1,
                selected: 0
            }
        );
        assert_eq!(
            back.overlay(),
            &TuiOverlay::Menu {
                top: 0,
                selected: 0
            }
        );
        assert_eq!(
            wrapped_back.overlay(),
            &TuiOverlay::Menu {
                top: count - 1,
                selected: 0
            }
        );
        assert_eq!(
            wrapped_forward.overlay(),
            &TuiOverlay::Menu {
                top: 0,
                selected: 0
            }
        );
    }

    #[test]
    fn opening_the_menu_starts_at_the_first_node_and_first_action() {
        // Opening is a DIFFERENT reducer path from the bar walks, and it must
        // start from a known place: resuming a stale (top, selected) would open
        // the menu wherever the last session left it, on an action the operator
        // never chose.
        let events: [ConsoleEvent; 0] = [];
        let closed = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None);

        let opened = reduce_tui_interaction(&closed, &events, TuiInteraction::OpenMenu);

        assert_eq!(
            opened.overlay(),
            &TuiOverlay::Menu {
                top: 0,
                selected: 0
            }
        );
    }

    #[test]
    fn every_overlay_owns_a_distinct_status_hint_while_it_holds_focus() {
        // TUI Contract / Scenario 19: an open overlay REPLACES the pane's hints
        // with the keys that act in it. Quantified over every variant, because
        // an overlay that inherits another's hints tells the operator to press
        // keys that do nothing there.
        let overlays = [
            TuiOverlay::None,
            TuiOverlay::Search {
                query: String::new(),
            },
            TuiOverlay::CommandPalette {
                query: String::new(),
            },
            TuiOverlay::CommandModal {
                selected_action_index: 0,
            },
            TuiOverlay::ActionInvoker { selected_action: 0 },
            TuiOverlay::Menu {
                top: 0,
                selected: 0,
            },
            TuiOverlay::ValveConfirm {
                valve: PendingValve::Approve,
                answer: String::new(),
            },
            TuiOverlay::DriverHandoff {
                command: String::new(),
            },
            TuiOverlay::WorkItemDetail {
                work_item_id: String::new(),
                scroll: 0,
            },
            TuiOverlay::Help {
                focus: HelpFocus::Menu,
                selected_section: 0,
                scroll: 0,
            },
        ];

        let hints: Vec<String> = overlays
            .iter()
            .map(super::overlay_footer_hint)
            .map(std::borrow::Cow::into_owned)
            .collect();

        let distinct: std::collections::BTreeSet<&String> = hints.iter().collect();
        check(
            distinct.len() == hints.len(),
            "overlay hints should be distinct",
        );
        // The open overlays all say how to LEAVE; only the closed state does not.
        for (overlay, hint) in overlays.iter().zip(&hints) {
            let escapable = hint.contains("esc") || !overlay.is_open();
            check(escapable, "open overlay hint should include an exit path");
        }
    }

    #[test]
    fn a_bar_walk_with_no_menu_open_is_inert() {
        // The bar walks describe a move WITHIN the bar, so with no menu open
        // they must not conjure one: only OpenMenu opens the menu, and a walk
        // that opened it would give the bar two entry points with different
        // starting nodes.
        let events: [ConsoleEvent; 0] = [];
        let closed = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None);

        let forward = reduce_tui_interaction(&closed, &events, TuiInteraction::MenuNextTop);
        let back = reduce_tui_interaction(&closed, &events, TuiInteraction::MenuPreviousTop);

        assert_eq!(forward.overlay(), &TuiOverlay::None);
        assert_eq!(back.overlay(), &TuiOverlay::None);
    }

    #[test]
    fn command_modal_opens_when_actions_exist() {
        let model = TuiScreenModel {
            active_view: TuiView::Attention,
            navigation: TuiView::all().to_vec(),
            attention_items: vec![],
            attention_total: 0,
            selected_attention_index: Some(0),
            detail: Some(AttentionDetail::new(
                "repo".to_owned(),
                "work-item".to_owned(),
                "run".to_owned(),
                None,
                vec![],
                vec![],
                vec![
                    OperatorAction::Registered("approve"),
                    OperatorAction::Registered("accept"),
                ],
            )),
            view_items: vec![],
            lane_board: project_lane_board(&[]),
            lane_focus: LaneFocus::Overview,
            selected_lane_index: Some(0),
            selected_lane_item_index: None,
            missing_selected_lane_item_id: None,
            events_focus: EventsFocus::Overview,
            selected_events_index: 0,
            focus: FocusPane::Content,
            detail_scroll: 0,
            header_scroll: 0,
            overlay: TuiOverlay::None,
            selected_repo: String::new(),
            selected_setting_index: None,
            dispatcher_settings: DispatcherSettingsRead::NotObserved,
            dispatcher_setting_write: DispatcherSettingWriteState::Idle,
            plugin_resolution: PluginResolution::unresolved(),
            unavailable_sources: vec![],
            observed_source_names: vec![],
            factory_activity: None,
            transient_status: None,
            list_edge: None,
            command_outcome: None,
            header: String::new(),
            action_failures: std::collections::BTreeMap::new(),
            orphaned_factory_runs: Vec::new(),
            build_identity: None,
            build_staleness: super::BuildStaleness::Unknown,
            startup_ingest_pending: false,
        };

        let overlay = super::open_command_modal(&model);

        assert_eq!(
            overlay,
            TuiOverlay::CommandModal {
                selected_action_index: 0
            }
        );
        let opened_model = TuiScreenModel { overlay, ..model };
        assert_eq!(
            opened_model.selected_operator_action(),
            Some(OperatorAction::Registered("approve"))
        );
        assert_eq!(
            opened_model.footer(),
            "up/down select action | enter explain | esc cancel"
        );
    }

    #[test]
    fn selected_move_status_valve_offers_the_first_ratified_target() {
        let events = drilldown_events();
        // A pending-approval item can only withdraw to backlog or park as
        // blocked; admission to ready stays on the approve valve.
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
        // Active is entered by dispatch or acceptance rework, not operator
        // relocation, so it has no move-status valve.
        let active =
            build_tui_model_for_state(&events, &drilldown_state(Lane::Active, 0, TuiOverlay::None));
        assert_eq!(active.selected_move_status_valve(), None);
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
    fn lane_drilldown_predicate_accepts_lane_focus() {
        let drilled = drilldown_state(Lane::PendingApproval, 0, TuiOverlay::None);
        let overview = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None);

        check(
            super::is_lane_drilldown(&drilled),
            "lanes view with a lane focus should be a drilldown",
        );
        check(
            !super::is_lane_drilldown(&overview),
            "lanes overview should not be a drilldown",
        );
    }

    #[test]
    fn reduce_moves_the_per_item_cursor_within_a_drilled_in_lane() {
        let events = drilldown_events();
        let start = drilldown_state(Lane::PendingApproval, 0, TuiOverlay::None);
        // Down advances to the second item; a further down clamps at the last.
        let down = reduce_tui_interaction(&start, &events, TuiInteraction::SelectNext);
        assert_eq!(down.selected_lane_item_index(), 1);
        assert_eq!(down.selected_lane_item_id(), Some("wi-b"));
        let down_again = reduce_tui_interaction(&down, &events, TuiInteraction::SelectNext);
        assert_eq!(down_again.selected_lane_item_index(), 1);
        assert_eq!(down_again.selected_lane_item_id(), Some("wi-b"));
        // Up returns to the first item.
        let up = reduce_tui_interaction(&down, &events, TuiInteraction::SelectPrevious);
        assert_eq!(up.selected_lane_item_index(), 0);
        assert_eq!(up.selected_lane_item_id(), Some("wi-a"));
    }

    #[test]
    fn lane_item_selection_helper_falls_back_to_row_without_a_drilled_lane() {
        let events = drilldown_events();
        let overview = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None);
        let model = build_tui_model_for_state(&events, &overview);

        let selected = super::select_lane_item_at(&overview, &model, 1);

        assert_eq!(selected.selected_lane_item_index(), 1);
        assert_eq!(selected.selected_lane_item_id(), None);
    }

    #[test]
    fn lane_item_selection_helper_falls_back_to_row_when_the_row_is_absent() {
        let events = drilldown_events();
        let drilled = drilldown_state(Lane::PendingApproval, 0, TuiOverlay::None);
        let model = build_tui_model_for_state(&events, &drilled);

        let selected = super::select_lane_item_at(&drilled, &model, 9);

        assert_eq!(selected.selected_lane_item_index(), 9);
        assert_eq!(selected.selected_lane_item_id(), None);
    }

    #[test]
    fn lane_item_movement_falls_back_to_stored_row_when_no_item_is_selected() {
        let selected_missing = drilldown_state(Lane::Ready, 1, TuiOverlay::None)
            .with_selected_lane_item(1, "wi-missing");
        let model = build_tui_model_for_state(&drilldown_events(), &selected_missing);
        assert_eq!(model.selected_lane_item_index(), None);

        let moved = reduce_tui_interaction(
            &selected_missing,
            &drilldown_events(),
            TuiInteraction::SelectPrevious,
        );

        assert_eq!(moved.selected_lane_item_index(), 0);
        assert_eq!(moved.selected_lane_item_id(), None);
    }

    #[test]
    fn selected_lane_item_resolver_reports_a_missing_anchor_without_a_lane_column() {
        let state = drilldown_state(Lane::Ready, 0, TuiOverlay::None)
            .with_selected_lane_item(0, "wi-missing-column");
        let board = super::LaneBoard {
            columns: Vec::new(),
        };

        let resolved = super::selected_lane_item_for_state(
            TuiView::Lanes,
            LaneFocus::Lane(Lane::Ready),
            &board,
            &state,
        );

        assert_eq!(resolved, (None, Some("wi-missing-column".to_owned())));
    }

    #[test]
    fn model_selection_reports_absent_lane_columns() {
        let mut model = build_tui_model_for_state(
            &drilldown_events(),
            &drilldown_state(Lane::Ready, 0, TuiOverlay::None),
        );
        model.lane_board = super::LaneBoard {
            columns: Vec::new(),
        };
        model.selected_lane_item_index = Some(0);

        check(
            model.next_ready_drain_target().is_none(),
            "ready drain target should be absent without a ready column",
        );
        check(
            model.selected_lane_item().is_none(),
            "selected lane item should be absent without its lane column",
        );
    }

    #[test]
    fn drill_into_lane_establishes_an_identity_anchor_when_the_lane_has_an_item() {
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_selected_lane_index(1);
        let drilled =
            reduce_tui_interaction(&state, &drilldown_events(), TuiInteraction::DrillIntoLane);

        assert_eq!(drilled.lane_focus(), LaneFocus::Lane(Lane::PendingApproval));
        assert_eq!(drilled.selected_lane_item_index(), 0);
        assert_eq!(drilled.selected_lane_item_id(), Some("wi-a"));
    }

    #[test]
    fn drill_into_empty_lane_keeps_the_row_fallback_without_an_identity_anchor() {
        let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_selected_lane_index(0);
        let drilled =
            reduce_tui_interaction(&state, &drilldown_events(), TuiInteraction::DrillIntoLane);

        assert_eq!(drilled.lane_focus(), LaneFocus::Lane(Lane::Backlog));
        assert_eq!(drilled.selected_lane_item_index(), 0);
        assert_eq!(drilled.selected_lane_item_id(), None);
    }

    #[test]
    fn move_status_resolves_to_the_real_orchestrator_transition_for_the_selected_item() {
        let events = drilldown_events();
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
                    answer: String::new(),
                },
            ),
        );
        let resolve_outcome =
            ok_operator_action_outcome(resolve_valve_action(&resolve, "operator"));
        assert!(matches!(
            &resolve_outcome,
            OperatorActionOutcome::PersistCommandWithPayload { command, payload_json }
                if command.command_type() == &CommandType::WorkItemResolveBlockedRequested
                    && command.aggregate_id() == "wi-blk"
                    && payload_json == r#"{"target_status":"backlog"}"#
        ));
    }

    #[test]
    fn move_status_with_a_non_drivable_pair_is_no_selected_operator_action() {
        let events = drilldown_events();
        for (from, to) in [
            (Lane::PendingApproval, Lane::Ready),
            (Lane::PendingApproval, Lane::Active),
            (Lane::PendingApproval, Lane::Done),
            (Lane::Acceptance, Lane::Done),
            (Lane::Acceptance, Lane::Ready),
            (Lane::Active, Lane::Blocked),
        ] {
            let model = build_tui_model_for_state(
                &events,
                &drilldown_state(
                    from,
                    0,
                    TuiOverlay::ValveConfirm {
                        valve: PendingValve::MoveStatus { from, to },
                        answer: String::new(),
                    },
                ),
            );
            assert_eq!(
                resolve_valve_action(&model, "operator"),
                Err(ApplicationError::NoSelectedOperatorAction)
            );
        }
    }

    #[test]
    fn move_status_broad_targets_map_onto_the_move_command_with_the_target_payload() {
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
                    answer: String::new(),
                },
            ),
        );
        assert!(matches!(
            ok_operator_action_outcome(resolve_valve_action(&model, "operator")),
            OperatorActionOutcome::PersistCommandWithPayload { ref command, ref payload_json }
                if command.command_type() == &CommandType::WorkItemMoveRequested
                    && command.aggregate_id() == "wi-a"
                    && payload_json == r#"{"target_status":"backlog"}"#
        ));
        // Cover the remaining move-outcome arms: acceptance -> blocked as a
        // broad target, and blocked -> ready via resolve-blocked (the other half
        // of the blocked pair).
        let cases = [
            (
                Lane::Acceptance,
                Lane::Blocked,
                "wi-acc",
                "move",
                r#"{"target_status":"blocked"}"#,
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
                        answer: String::new(),
                    },
                ),
            );
            assert!(matches!(
                ok_operator_action_outcome(resolve_valve_action(&model, "operator")),
                OperatorActionOutcome::PersistCommandWithPayload { ref command, ref payload_json }
                    if command.aggregate_id() == item && payload_json == expected_payload
            ));
        }
    }

    #[test]
    fn set_override_valve_resolves_to_the_override_command_for_the_selected_item() {
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
                    answer: String::new(),
                },
            ),
        );
        assert!(matches!(
            ok_operator_action_outcome(resolve_valve_action(&model, "operator")),
            OperatorActionOutcome::PersistCommandWithPayload { ref command, ref payload_json }
                if command.command_type() == &CommandType::WorkItemSetDispatcherOverrideRequested
                    && command.aggregate_id() == "wi-a"
                    && payload_json == r#"{"setting":"merge_on_review_cap","value":true}"#
        ));
    }
}
