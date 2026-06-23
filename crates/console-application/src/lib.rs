#![forbid(unsafe_code)]

use console_domain::{ConsoleEvent, EventType};

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
    header: String,
    footer: String,
}

impl TuiScreenModel {
    #[must_use]
    pub const fn new(
        active_view: TuiView,
        navigation: Vec<TuiView>,
        attention_items: Vec<AttentionItem>,
        selected_attention_index: Option<usize>,
        detail: Option<AttentionDetail>,
        header: String,
        footer: String,
    ) -> Self {
        Self {
            active_view,
            navigation,
            attention_items,
            selected_attention_index,
            detail,
            header,
            footer,
        }
    }

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
}

pub type ApplicationResult<T> = Result<T, ApplicationError>;

#[must_use]
pub fn project_attention(events: &[ConsoleEvent]) -> Vec<AttentionItem> {
    events
        .iter()
        .filter(|event| event.event_type().requires_attention())
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

#[must_use]
pub fn build_tui_model(events: &[ConsoleEvent], requested_selection: usize) -> TuiScreenModel {
    let attention_events = events
        .iter()
        .filter(|event| event.event_type().requires_attention())
        .collect::<Vec<_>>();
    let attention_items = project_attention(events);
    let selected_attention_index = selected_index(attention_items.len(), requested_selection);
    let detail = selected_attention_index
        .map(|index| build_attention_detail(attention_events[index], events));
    TuiScreenModel::new(
        TuiView::Attention,
        TuiView::all().to_vec(),
        attention_items,
        selected_attention_index,
        detail,
        format!(
            "fleet: livespec | mode: tui | attention: {}",
            attention_events.len()
        ),
        "shortcuts: arrows select | enter details | / search | : command palette".to_owned(),
    )
}

pub fn validate_operator_action(action: &str) -> ApplicationResult<&str> {
    let trimmed = action.trim();
    if trimmed.is_empty() {
        return Err(ApplicationError::EmptyOperatorAction);
    }
    Ok(trimmed)
}

fn selected_index(item_count: usize, requested_selection: usize) -> Option<usize> {
    (item_count > 0).then(|| requested_selection.min(item_count - 1))
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
    use console_domain::{ConsoleEvent, EventType};
    use proptest::proptest;

    use super::{
        ApplicationError, AttentionEvent, OperatorAction, TuiView, build_tui_model,
        project_attention, validate_operator_action,
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
