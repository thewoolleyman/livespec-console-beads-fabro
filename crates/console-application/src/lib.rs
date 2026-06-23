#![forbid(unsafe_code)]

use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};

pub mod source_adapters;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttentionItem {
    id: String,
    title: String,
    source: String,
    source_reference: String,
    next_action: OperatorAction,
}

impl AttentionItem {
    #[must_use]
    pub const fn new(
        id: String,
        title: String,
        source: String,
        source_reference: String,
        next_action: OperatorAction,
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
    pub const fn next_action(&self) -> OperatorAction {
        self.next_action
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiView {
    Attention,
    Spec,
    Ready,
    Factory,
    Manual,
    Done,
    Events,
    Repos,
}

impl TuiView {
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Attention,
            Self::Spec,
            Self::Ready,
            Self::Factory,
            Self::Manual,
            Self::Done,
            Self::Events,
            Self::Repos,
        ]
    }

    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Attention => "Attention",
            Self::Spec => "Spec",
            Self::Ready => "Ready",
            Self::Factory => "Factory",
            Self::Manual => "Manual",
            Self::Done => "Done",
            Self::Events => "Events",
            Self::Repos => "Repos",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorAction {
    Acknowledge,
    Snooze,
    OpenFabroAttach,
    CopyFabroAttach,
}

impl OperatorAction {
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Acknowledge => "Acknowledge",
            Self::Snooze => "Snooze",
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
    TypeChar(char),
    Backspace,
    SelectNextAction,
    SelectPreviousAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiInteractionState {
    selected_attention_index: usize,
    overlay: TuiOverlay,
}

impl TuiInteractionState {
    #[must_use]
    pub const fn new(selected_attention_index: usize, overlay: TuiOverlay) -> Self {
        Self {
            selected_attention_index,
            overlay,
        }
    }

    #[must_use]
    pub const fn selected_attention_index(&self) -> usize {
        self.selected_attention_index
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
pub enum ApplicationError {
    EmptyOperatorAction,
    NoSelectedAttentionItem,
    NoSelectedOperatorAction,
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

#[must_use]
pub fn project_attention(events: &[ConsoleEvent]) -> Vec<AttentionItem> {
    project_attention_from_events(attention_events(events))
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
    let attention_events = attention_events_matching(events, search_query);
    let attention_items = project_attention_from_events(attention_events.clone());
    let selected_attention_index =
        selected_index(attention_items.len(), state.selected_attention_index());
    let detail = selected_attention_index
        .map(|index| build_attention_detail(attention_events[index], events));
    let overlay = normalize_overlay(state.overlay(), detail.as_ref());
    TuiScreenModel {
        active_view: TuiView::Attention,
        navigation: TuiView::all().to_vec(),
        attention_items,
        selected_attention_index,
        detail,
        overlay,
        header: format!(
            "fleet: livespec | mode: tui | attention: {}",
            attention_events.len()
        ),
        footer: "shortcuts: arrows select | enter details | / search | : command palette"
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
        TuiInteraction::SelectNext => TuiInteractionState::new(
            move_selection_down(
                model.attention_items().len(),
                state.selected_attention_index(),
            ),
            state.overlay().clone(),
        ),
        TuiInteraction::SelectPrevious => TuiInteractionState::new(
            move_selection_up(state.selected_attention_index()),
            state.overlay().clone(),
        ),
        TuiInteraction::OpenSearch => TuiInteractionState::new(
            state.selected_attention_index(),
            TuiOverlay::Search {
                query: String::new(),
            },
        ),
        TuiInteraction::OpenCommandPalette => TuiInteractionState::new(
            state.selected_attention_index(),
            TuiOverlay::CommandPalette {
                query: String::new(),
            },
        ),
        TuiInteraction::OpenCommandModal => TuiInteractionState::new(
            state.selected_attention_index(),
            TuiOverlay::CommandModal {
                selected_action_index: 0,
            },
        ),
        TuiInteraction::CloseOverlay => {
            TuiInteractionState::new(state.selected_attention_index(), TuiOverlay::None)
        }
        TuiInteraction::TypeChar(value) => TuiInteractionState::new(
            state.selected_attention_index(),
            type_overlay_char(state.overlay(), value),
        ),
        TuiInteraction::Backspace => TuiInteractionState::new(
            state.selected_attention_index(),
            backspace_overlay_query(state.overlay()),
        ),
        TuiInteraction::SelectNextAction => TuiInteractionState::new(
            state.selected_attention_index(),
            move_action_down(state.overlay(), model.detail()),
        ),
        TuiInteraction::SelectPreviousAction => TuiInteractionState::new(
            state.selected_attention_index(),
            move_action_up(state.overlay()),
        ),
    }
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
    let requested_by = validate_operator_action(requested_by)?;
    let detail = model
        .detail()
        .ok_or(ApplicationError::NoSelectedAttentionItem)?;
    let action = model
        .selected_operator_action()
        .ok_or(ApplicationError::NoSelectedOperatorAction)?;
    Ok(match action {
        OperatorAction::Acknowledge => OperatorActionOutcome::PersistCommand(attention_command(
            detail,
            CommandType::AttentionAcknowledgeRequested,
            requested_by,
        )),
        OperatorAction::Snooze => OperatorActionOutcome::PersistCommand(attention_command(
            detail,
            CommandType::AttentionSnoozeRequested,
            requested_by,
        )),
        OperatorAction::OpenFabroAttach => {
            OperatorActionOutcome::OpenAttachCommand(detail.attach_command().to_owned())
        }
        OperatorAction::CopyFabroAttach => {
            OperatorActionOutcome::CopyAttachCommand(detail.attach_command().to_owned())
        }
    })
}

fn attention_events(events: &[ConsoleEvent]) -> Vec<&ConsoleEvent> {
    events
        .iter()
        .filter(|event| event.event_type().requires_attention())
        .collect()
}

fn attention_events_matching<'a>(
    events: &'a [ConsoleEvent],
    search_query: Option<&str>,
) -> Vec<&'a ConsoleEvent> {
    attention_events(events)
        .into_iter()
        .filter(|event| attention_event_matches(event, search_query))
        .collect()
}

fn attention_event_matches(event: &ConsoleEvent, search_query: Option<&str>) -> bool {
    search_query.is_none_or(|query| {
        query.is_empty()
            || event
                .event_type()
                .label()
                .to_lowercase()
                .contains(&query.to_lowercase())
            || event
                .source()
                .to_lowercase()
                .contains(&query.to_lowercase())
            || event
                .stream_id()
                .to_lowercase()
                .contains(&query.to_lowercase())
    })
}

fn project_attention_from_events(events: Vec<&ConsoleEvent>) -> Vec<AttentionItem> {
    events
        .into_iter()
        .map(|event| {
            AttentionItem::new(
                event.event_id().to_owned(),
                event.event_type().label().to_owned(),
                event.source().to_owned(),
                event.stream_id().to_owned(),
                event.event_type().next_operator_action(),
            )
        })
        .collect()
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

fn attention_command(
    detail: &AttentionDetail,
    command_type: CommandType,
    requested_by: &str,
) -> CommandEnvelope {
    let action_name = command_type.contract_name().replace('.', "_");
    CommandEnvelope::new(
        format!("cmd_{}_{}", detail.work_item(), action_name),
        command_type,
        detail.work_item().to_owned(),
        format!("{}:{}", detail.work_item(), command_type.contract_name()),
        requested_by.to_owned(),
    )
}

fn build_attention_detail(event: &ConsoleEvent, events: &[ConsoleEvent]) -> AttentionDetail {
    let fabro_run = fabro_run_id(event);
    AttentionDetail::new(
        repo_id(event),
        event.event_id().to_owned(),
        fabro_run.clone(),
        format!("fabro attach {fabro_run}"),
        latest_timeline(events, event.stream_id(), 3),
        event.event_type().actions(),
    )
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
    let mut matching_events = events
        .iter()
        .filter(|event| event.stream_id() == selected_stream_id)
        .collect::<Vec<_>>();
    matching_events.sort_by_key(|event| event.stream_seq());
    matching_events
        .into_iter()
        .rev()
        .take(requested_count)
        .map(|event| {
            TimelineEntry::new(
                event.event_id().to_owned(),
                event.event_type().label().to_owned(),
                event.source().to_owned(),
            )
        })
        .collect()
}

trait AttentionEvent {
    fn requires_attention(&self) -> bool;
    fn label(&self) -> &'static str;
    fn next_operator_action(&self) -> OperatorAction;
    fn actions(&self) -> Vec<OperatorAction>;
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
            Self::BeadsWorkItemSnapshotObserved => "Beads work-item snapshot",
            Self::FabroHumanGateObserved => "Fabro human gate",
            Self::LivespecNextSnapshotObserved => "LiveSpec next snapshot",
            Self::LivespecReviseRequired => "LiveSpec revise required",
            Self::DispatcherNeedsRegroomObserved => "Dispatcher needs-regroom",
            Self::FactoryDrainRequested => "Factory drain requested",
            Self::SourceCompletenessFindingObserved => "Source completeness finding",
        }
    }

    fn next_operator_action(&self) -> OperatorAction {
        match self {
            Self::FabroHumanGateObserved => OperatorAction::OpenFabroAttach,
            Self::LivespecReviseRequired | Self::DispatcherNeedsRegroomObserved => {
                OperatorAction::Acknowledge
            }
            Self::BeadsWorkItemSnapshotObserved
            | Self::FactoryDrainRequested
            | Self::LivespecNextSnapshotObserved
            | Self::SourceCompletenessFindingObserved => OperatorAction::Acknowledge,
        }
    }

    fn actions(&self) -> Vec<OperatorAction> {
        match self {
            Self::FabroHumanGateObserved => vec![
                OperatorAction::Acknowledge,
                OperatorAction::Snooze,
                OperatorAction::OpenFabroAttach,
                OperatorAction::CopyFabroAttach,
            ],
            Self::LivespecReviseRequired | Self::DispatcherNeedsRegroomObserved => {
                vec![OperatorAction::Acknowledge, OperatorAction::Snooze]
            }
            Self::BeadsWorkItemSnapshotObserved
            | Self::FactoryDrainRequested
            | Self::LivespecNextSnapshotObserved
            | Self::SourceCompletenessFindingObserved => vec![OperatorAction::Acknowledge],
        }
    }
}

#[cfg(test)]
mod tests {
    use console_domain::{CommandType, ConsoleEvent, EventType};
    use proptest::proptest;

    use super::{
        ApplicationError, AttentionEvent, OperatorAction, TuiInteraction, TuiInteractionState,
        TuiOverlay, TuiView, build_tui_model, build_tui_model_for_state, project_attention,
        reduce_tui_interaction, resolve_selected_operator_action, validate_operator_action,
    };

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
        assert_eq!(projected[0].source(), "fabro");
        assert_eq!(
            projected[0].source_reference(),
            "factory:livespec-console-beads-fabro"
        );
        assert_eq!(projected[0].next_action(), OperatorAction::OpenFabroAttach);
        assert_eq!(projected[1].source(), "livespec");
    }

    #[test]
    fn tui_model_defaults_to_attention_with_required_navigation() {
        let model = build_tui_model(&[], 0);

        assert_eq!(model.active_view(), TuiView::Attention);
        assert_eq!(model.navigation(), TuiView::all());
        assert_eq!(model.attention_items(), []);
        assert_eq!(model.selected_attention_index(), None);
        assert_eq!(model.detail(), None);
        assert_eq!(model.overlay(), &TuiOverlay::None);
        assert_eq!(model.selected_operator_action(), None);
        assert_eq!(model.header(), "fleet: livespec | mode: tui | attention: 0");
        assert_eq!(
            model.footer(),
            "shortcuts: arrows select | enter details | / search | : command palette"
        );
    }

    #[test]
    fn tui_model_shows_fabro_gate_detail_and_actions() {
        let model = build_tui_model(&fabro_gate_events(), 0);

        assert_eq!(model.selected_attention_index(), Some(0));
        assert_eq!(model.attention_items().len(), 3);
        assert_fabro_gate_detail(&model);
        assert_fabro_gate_timeline(&model);
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
            Some("evt_regroom")
        );

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectPrevious);
        let model = build_tui_model_for_state(&events, &state);

        assert_eq!(state.selected_attention_index(), 0);
        assert_eq!(model.selected_attention_index(), Some(0));
        assert_fabro_gate_detail(&model);
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
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::TypeChar('r'));
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::TypeChar('e'));
        let state = reduce_tui_interaction(&state, &events, TuiInteraction::TypeChar('v'));
        let model = build_tui_model_for_state(&events, &state);

        assert!(state.overlay().is_open());
        assert_eq!(state.overlay().query(), Some("rev"));
        assert_eq!(
            model
                .attention_items()
                .iter()
                .map(super::AttentionItem::id)
                .collect::<Vec<_>>(),
            ["evt_other"]
        );
        assert_eq!(
            model.detail().map(super::AttentionDetail::work_item),
            Some("evt_other")
        );
        assert_eq!(
            model.overlay(),
            &TuiOverlay::Search {
                query: "rev".to_owned()
            }
        );

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::Backspace);
        let model = build_tui_model_for_state(&events, &state);

        assert_eq!(state.overlay().query(), Some("re"));
        assert_eq!(model.attention_items().len(), 3);
    }

    #[test]
    fn tui_search_matches_source_and_stream_reference() {
        let events = fabro_gate_events();
        let source_state = TuiInteractionState::new(
            0,
            TuiOverlay::Search {
                query: "RUN_17".to_owned(),
            },
        );
        let stream_state = TuiInteractionState::new(
            0,
            TuiOverlay::Search {
                query: "other".to_owned(),
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

        assert_eq!(state.overlay().selected_action_index(), Some(2));
        assert_eq!(
            model.selected_operator_action(),
            Some(OperatorAction::OpenFabroAttach)
        );

        let state = reduce_tui_interaction(&state, &events, TuiInteraction::SelectPreviousAction);
        let model = build_tui_model_for_state(&events, &state);

        assert_eq!(state.overlay().selected_action_index(), Some(1));
        assert_eq!(
            model.selected_operator_action(),
            Some(OperatorAction::Snooze)
        );
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
                selected_action_index: 1
            }
        );
        assert_eq!(
            model.selected_operator_action(),
            Some(OperatorAction::Snooze)
        );
    }

    #[test]
    fn selected_acknowledge_action_resolves_to_attention_command() {
        let events = fabro_gate_events();
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::CommandModal {
                selected_action_index: 0,
            },
        );
        let model = build_tui_model_for_state(&events, &state);
        let outcome = resolve_selected_operator_action(&model, "operator");

        let command = outcome
            .as_ref()
            .ok()
            .and_then(super::OperatorActionOutcome::command);
        assert_eq!(
            command.map(console_domain::CommandEnvelope::command_id),
            Some("cmd_evt_gate_attention_acknowledge_requested")
        );
        assert_eq!(
            command.map(console_domain::CommandEnvelope::command_type),
            Some(&CommandType::AttentionAcknowledgeRequested)
        );
        assert_eq!(
            command.map(console_domain::CommandEnvelope::aggregate_id),
            Some("evt_gate")
        );
        assert_eq!(
            command.map(console_domain::CommandEnvelope::idempotency_key),
            Some("evt_gate:attention.acknowledge_requested")
        );
        assert_eq!(
            command.map(console_domain::CommandEnvelope::requested_by),
            Some("operator")
        );
        assert_eq!(
            outcome
                .as_ref()
                .ok()
                .and_then(super::OperatorActionOutcome::attach_command),
            None
        );
    }

    #[test]
    fn selected_snooze_action_resolves_to_attention_command() {
        let events = fabro_gate_events();
        let state = TuiInteractionState::new(
            0,
            TuiOverlay::CommandModal {
                selected_action_index: 1,
            },
        );
        let model = build_tui_model_for_state(&events, &state);
        let outcome = resolve_selected_operator_action(&model, "operator");

        assert_eq!(
            outcome
                .as_ref()
                .ok()
                .and_then(super::OperatorActionOutcome::command)
                .map(console_domain::CommandEnvelope::command_type),
            Some(&CommandType::AttentionSnoozeRequested)
        );
    }

    #[test]
    fn selected_attach_actions_resolve_to_local_terminal_effects() {
        let events = fabro_gate_events();
        for (selected_action_index, expected) in [
            (
                2,
                super::OperatorActionOutcome::OpenAttachCommand("fabro attach run_17".to_owned()),
            ),
            (
                3,
                super::OperatorActionOutcome::CopyAttachCommand("fabro attach run_17".to_owned()),
            ),
        ] {
            let state = TuiInteractionState::new(
                0,
                TuiOverlay::CommandModal {
                    selected_action_index,
                },
            );
            let model = build_tui_model_for_state(&events, &state);

            let outcome = resolve_selected_operator_action(&model, "operator");

            assert_eq!(outcome, Ok(expected));
            assert_eq!(
                outcome
                    .as_ref()
                    .ok()
                    .and_then(super::OperatorActionOutcome::command),
                None
            );
            assert_eq!(
                outcome
                    .as_ref()
                    .ok()
                    .and_then(super::OperatorActionOutcome::attach_command),
                Some("fabro attach run_17")
            );
        }
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
                "repo:livespec-console-beads-fabro".to_owned(),
                1,
            ),
            ConsoleEvent::new(
                "evt_gate".to_owned(),
                1,
                "factory".to_owned(),
                EventType::FabroHumanGateObserved,
                "fabro:run_17".to_owned(),
                "repo:livespec-console-beads-fabro".to_owned(),
                2,
            ),
            ConsoleEvent::new(
                "evt_regroom".to_owned(),
                1,
                "factory".to_owned(),
                EventType::DispatcherNeedsRegroomObserved,
                "dispatcher".to_owned(),
                "repo:livespec-console-beads-fabro".to_owned(),
                3,
            ),
            ConsoleEvent::new(
                "evt_other".to_owned(),
                1,
                "factory".to_owned(),
                EventType::LivespecReviseRequired,
                "livespec".to_owned(),
                "repo:other".to_owned(),
                4,
            ),
        ]
    }

    fn assert_fabro_gate_detail(model: &super::TuiScreenModel) {
        assert_eq!(
            model.detail().map(super::AttentionDetail::repo),
            Some("livespec-console-beads-fabro")
        );
        assert_eq!(
            model.detail().map(super::AttentionDetail::work_item),
            Some("evt_gate")
        );
        assert_eq!(
            model.detail().map(super::AttentionDetail::fabro_run),
            Some("run_17")
        );
        assert_eq!(
            model.detail().map(super::AttentionDetail::attach_command),
            Some("fabro attach run_17")
        );
        assert_eq!(
            model.detail().map(super::AttentionDetail::actions),
            Some(
                [
                    OperatorAction::Acknowledge,
                    OperatorAction::Snooze,
                    OperatorAction::OpenFabroAttach,
                    OperatorAction::CopyFabroAttach,
                ]
                .as_slice()
            )
        );
    }

    fn assert_fabro_gate_timeline(model: &super::TuiScreenModel) {
        assert_eq!(
            model.detail().map(|detail| detail.timeline().len()),
            Some(3)
        );
        assert_eq!(
            model
                .detail()
                .and_then(|detail| detail.timeline().first())
                .map(super::TimelineEntry::event_id),
            Some("evt_regroom")
        );
        assert_eq!(
            model
                .detail()
                .and_then(|detail| detail.timeline().first())
                .map(super::TimelineEntry::source),
            Some("dispatcher")
        );
        assert_eq!(
            model
                .detail()
                .and_then(|detail| detail.timeline().first())
                .map(super::TimelineEntry::label),
            Some("Dispatcher needs-regroom")
        );
        assert_eq!(
            model
                .detail()
                .and_then(|detail| detail.timeline().get(1))
                .map(super::TimelineEntry::event_id),
            Some("evt_gate")
        );
        assert_eq!(
            model
                .detail()
                .and_then(|detail| detail.timeline().get(2))
                .map(super::TimelineEntry::event_id),
            Some("evt_old")
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
            ConsoleEvent::fixture("evt_1", EventType::FabroHumanGateObserved, "fabro"),
            ConsoleEvent::fixture("evt_2", EventType::LivespecReviseRequired, "livespec"),
        ];

        let model = build_tui_model(&events, 99);

        assert_eq!(model.selected_attention_index(), Some(1));
        assert_eq!(
            model.detail().map(super::AttentionDetail::work_item),
            Some("evt_2")
        );
        assert_eq!(
            model.detail().map(super::AttentionDetail::fabro_run),
            Some("evt_2")
        );
        assert_eq!(
            model.detail().map(super::AttentionDetail::actions),
            Some([OperatorAction::Acknowledge, OperatorAction::Snooze].as_slice())
        );
    }

    #[test]
    fn non_attention_factory_action_policy_is_stable() {
        for event_type in [
            EventType::BeadsWorkItemSnapshotObserved,
            EventType::FactoryDrainRequested,
            EventType::LivespecNextSnapshotObserved,
            EventType::SourceCompletenessFindingObserved,
        ] {
            assert!(!event_type.requires_attention());
            assert_eq!(
                event_type.next_operator_action(),
                OperatorAction::Acknowledge
            );
            assert_eq!(event_type.actions(), [OperatorAction::Acknowledge]);
        }
    }

    #[test]
    fn attention_event_policy_is_stable_for_every_attention_type() {
        for event_type in [
            EventType::FabroHumanGateObserved,
            EventType::LivespecReviseRequired,
            EventType::DispatcherNeedsRegroomObserved,
        ] {
            assert!(event_type.requires_attention());
        }
        assert_eq!(
            EventType::LivespecReviseRequired.next_operator_action(),
            OperatorAction::Acknowledge
        );
        assert_eq!(
            EventType::DispatcherNeedsRegroomObserved.next_operator_action(),
            OperatorAction::Acknowledge
        );
        assert_eq!(
            EventType::LivespecReviseRequired.actions(),
            [OperatorAction::Acknowledge, OperatorAction::Snooze]
        );
        assert_eq!(
            EventType::DispatcherNeedsRegroomObserved.actions(),
            [OperatorAction::Acknowledge, OperatorAction::Snooze]
        );
    }

    #[test]
    fn navigation_and_action_labels_are_stable() {
        assert_eq!(TuiView::Attention.label(), "Attention");
        assert_eq!(TuiView::Spec.label(), "Spec");
        assert_eq!(TuiView::Ready.label(), "Ready");
        assert_eq!(TuiView::Factory.label(), "Factory");
        assert_eq!(TuiView::Manual.label(), "Manual");
        assert_eq!(TuiView::Done.label(), "Done");
        assert_eq!(TuiView::Events.label(), "Events");
        assert_eq!(TuiView::Repos.label(), "Repos");
        assert_eq!(OperatorAction::Acknowledge.label(), "Acknowledge");
        assert_eq!(OperatorAction::Snooze.label(), "Snooze");
        assert_eq!(OperatorAction::OpenFabroAttach.label(), "Open Fabro attach");
        assert_eq!(OperatorAction::CopyFabroAttach.label(), "Copy Fabro attach");
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

    #[test]
    fn all_event_type_labels_are_stable() {
        assert_eq!(
            EventType::BeadsWorkItemSnapshotObserved.label(),
            "Beads work-item snapshot"
        );
        assert_eq!(
            EventType::DispatcherNeedsRegroomObserved.label(),
            "Dispatcher needs-regroom"
        );
        assert_eq!(
            EventType::FabroHumanGateObserved.label(),
            "Fabro human gate"
        );
        assert_eq!(
            EventType::FactoryDrainRequested.label(),
            "Factory drain requested"
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
}
