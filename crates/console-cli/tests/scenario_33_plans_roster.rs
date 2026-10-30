//! Scenario 33 -- Plans are first-class console records.
//!
//! The Plans roster consumes the same persisted work-item snapshots as Lanes.
//! It recognises only open `epic` records as plans, counts children from their
//! published `parent` field, and leaves fields the source does not publish
//! explicitly absent. This acceptance test drives the rendered console rather
//! than a renderer-private helper so the view registry, roster, status hint,
//! and Help overlay stay in lockstep.

use console_application::source_adapters::Lane;
use console_application::{
    LaneFocus, TuiInteractionState, TuiOverlay, TuiView, build_tui_model_for_state,
};
use console_domain::{ConsoleEvent, EventType};
use console_tui::render_to_text;

const REPO: &str = "livespec-console-beads-fabro";

/// A consumed `list-work-items` snapshot. The raw payload is intentional: it
/// exercises the consumer boundary for `parent` and `labels`, rather than
/// constructing a console-only plan relationship in the test.
fn work_item(
    event_id: &str,
    id: &str,
    lane: &str,
    rank: &str,
    detail: &serde_json::Value,
) -> ConsoleEvent {
    ConsoleEvent::new(
        event_id.to_owned(),
        1,
        "factory".to_owned(),
        EventType::WorkItemSnapshotObserved,
        "orchestrator".to_owned(),
        format!("repo:{REPO}"),
        1,
    )
    .with_payload_json(
        serde_json::json!({
            "repo": REPO,
            "work_item_id": id,
            "lane": lane,
            "rank": rank,
            "status": lane,
            "source_version": 1,
            "detail": detail,
        })
        .to_string(),
    )
}

fn roster_events() -> Vec<ConsoleEvent> {
    vec![
        work_item(
            "plan-a",
            "plan-a",
            "ready",
            "a0",
            &serde_json::json!({"title": "Plan A", "type": "epic"}),
        ),
        work_item(
            "plan-b",
            "plan-b",
            "blocked",
            "b0",
            &serde_json::json!({"title": "Plan B", "type": "epic"}),
        ),
        work_item(
            "closed-plan",
            "closed-plan",
            "done",
            "c0",
            &serde_json::json!({"title": "Closed plan", "type": "epic"}),
        ),
        work_item(
            "a-backlog-child",
            "a-backlog-child",
            "backlog",
            "a0",
            &serde_json::json!({"title": "Backlog child", "parent": "plan-a"}),
        ),
        work_item(
            "a-ready-child",
            "a-ready-child",
            "ready",
            "a1",
            &serde_json::json!({"title": "Ready child", "parent": "plan-a"}),
        ),
        work_item(
            "a-upstream-child",
            "a-upstream-child",
            "blocked",
            "a2",
            &serde_json::json!({
                "title": "Upstream-blocked child",
                "parent": "plan-a",
                "labels": ["upstream-dep:livespec-orchestrator-beads-fabro"],
            }),
        ),
        work_item(
            "b-child",
            "b-child",
            "active",
            "a0",
            &serde_json::json!({"title": "Active child", "parent": "plan-b"}),
        ),
        work_item(
            "unrelated",
            "unrelated",
            "blocked",
            "z0",
            &serde_json::json!({
                "title": "Unrelated upstream dependency",
                "labels": ["upstream-dep:another"],
            }),
        ),
    ]
}

#[test]
fn plans_roster_is_rank_ordered_and_leaves_unpublished_plan_facts_absent() -> Result<(), String> {
    let events = roster_events();
    // Before Plans exists this is Events, so the test fails as a real rendered
    // assertion instead of depending on an enum variant that has not compiled
    // yet. Once Plans is inserted after Lanes, index three is its canonical
    // registry position and the same test drives the new view.
    let plans = TuiView::all()[3];
    let state = TuiInteractionState::for_view(plans, 0, TuiOverlay::None);
    let model = build_tui_model_for_state(&events, &state);
    let rendered = render_to_text(&model, 220, 40).map_err(|error| format!("{error:?}"))?;

    let a = rendered
        .find("Plan A")
        .ok_or_else(|| "Plan A missing from Plans roster".to_owned())?;
    let b = rendered
        .find("Plan B")
        .ok_or_else(|| "Plan B missing from Plans roster".to_owned())?;
    assert!(a < b, "plans must retain published rank order: {rendered}");
    assert!(
        !rendered.contains("Closed plan"),
        "done plans do not belong in the open-plan roster: {rendered}"
    );
    assert!(
        rendered.contains("backlog 1")
            && rendered.contains("ready 1")
            && rendered.contains("blocked 1")
            && rendered.contains("upstream blocked 1"),
        "Plan A must show lane and upstream-dependency child counts: {rendered}"
    );
    assert!(
        rendered.contains("next action —")
            && rendered.contains("last session / age —")
            && rendered.contains("plan slug —"),
        "unpublished plan facts stay explicitly absent: {rendered}"
    );
    assert!(
        rendered.contains("type plan") && !rendered.contains("epic"),
        "the human-facing plan surface never exposes the storage type: {rendered}"
    );
    assert!(
        rendered.contains("> 4 Plans"),
        "Plans is the fourth direct view and is named in the view roster: {rendered}"
    );

    let help = build_tui_model_for_state(
        &events,
        &state.with_overlay(TuiOverlay::Help {
            focus: console_application::HelpFocus::Text,
            selected_section: 4,
            scroll: 0,
        }),
    );
    let rendered_help = render_to_text(&help, 220, 40).map_err(|error| format!("{error:?}"))?;
    assert!(
        rendered_help.contains("Plans -- the rank-ordered roster"),
        "Help must name the Plans view: {rendered_help}"
    );
    Ok(())
}

/// The drilled-in ready lane rendered wide enough that no row truncates.
fn ready_lane_rows(events: &[ConsoleEvent]) -> Result<Vec<String>, String> {
    let state = TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
        .with_lane_focus(LaneFocus::Lane(Lane::Ready))
        .with_selected_lane_item_index(0);
    let model = build_tui_model_for_state(events, &state);
    let rendered = render_to_text(&model, 220, 40).map_err(|error| format!("{error:?}"))?;
    Ok(rendered.lines().map(str::to_owned).collect())
}

fn row_for<'a>(rows: &'a [String], id: &str) -> Result<&'a str, String> {
    rows.iter()
        .map(String::as_str)
        .find(|row| row.contains(id))
        .ok_or_else(|| format!("no lane row names {id}: {rows:?}"))
}

#[test]
fn a_planned_item_names_its_parent_id_and_a_plan_less_item_has_a_blank_plan_column()
-> Result<(), String> {
    let events = vec![
        work_item(
            "plan-p",
            "plan-p",
            "backlog",
            "a0",
            &serde_json::json!({"title": "The parent", "type": "epic"}),
        ),
        work_item(
            "planned",
            "planned-task",
            "ready",
            "a0",
            &serde_json::json!({"title": "Planned task", "type": "task", "parent": "plan-p"}),
        ),
        work_item(
            "loose",
            "loose-bug",
            "ready",
            "a1",
            &serde_json::json!({"title": "Loose bug", "type": "bug"}),
        ),
    ];
    let rows = ready_lane_rows(&events)?;

    let planned = row_for(&rows, "planned-task")?;
    assert!(
        planned.contains("plan plan-p"),
        "a planned row names its parent exactly as the record names it: {planned}"
    );

    let loose = row_for(&rows, "loose-bug")?;
    assert!(
        loose.contains("loose-bug  rank a1  [ready]  Loose bug  repo"),
        "a plan-less row carries the same fields as before, with a blank plan column: {loose}"
    );
    assert!(
        !loose.contains("plan"),
        "a plan-less row carries no flag, marker, or finding about the missing plan: {loose}"
    );
    Ok(())
}

#[test]
fn a_planned_item_names_the_slug_its_parent_record_publishes() -> Result<(), String> {
    let events = vec![
        work_item(
            "plan-p",
            "plan-p",
            "backlog",
            "a0",
            &serde_json::json!({
                "title": "The parent",
                "type": "epic",
                "plan_slug": "alpha-topic"
            }),
        ),
        work_item(
            "planned",
            "planned-task",
            "ready",
            "a0",
            &serde_json::json!({"title": "Planned task", "type": "task", "parent": "plan-p"}),
        ),
    ];
    let rows = ready_lane_rows(&events)?;

    let planned = row_for(&rows, "planned-task")?;
    assert!(
        planned.contains("plan alpha-topic"),
        "a row names its parent plan by the slug the consumed records publish: {planned}"
    );
    assert!(
        !planned.contains("plan-p"),
        "the slug replaces the parent id in the plan column: {planned}"
    );
    Ok(())
}

/// The full record surface of one work-item, as the detail modal renders it.
fn record_surface(events: &[ConsoleEvent], work_item_id: &str) -> Result<String, String> {
    let state = TuiInteractionState::for_view(
        TuiView::Lanes,
        0,
        TuiOverlay::WorkItemDetail {
            work_item_id: work_item_id.to_owned(),
            scroll: 0,
        },
    )
    .with_lane_focus(LaneFocus::Lane(Lane::Ready))
    .with_selected_lane_item_index(0);
    let model = build_tui_model_for_state(events, &state);
    render_to_text(&model, 160, 60).map_err(|error| format!("{error:?}"))
}

fn field_line<'a>(surface: &'a str, label: &str) -> Result<&'a str, String> {
    surface
        .lines()
        .find(|line| line.contains(label))
        .ok_or_else(|| format!("the record surface has no {label} field: {surface}"))
}

#[test]
fn the_full_record_surface_lists_parent_and_renders_a_missing_one_as_absent() -> Result<(), String>
{
    let events = vec![
        work_item(
            "planned",
            "planned-task",
            "ready",
            "a0",
            &serde_json::json!({"title": "Planned task", "type": "task", "parent": "plan-p"}),
        ),
        work_item(
            "loose",
            "loose-bug",
            "ready",
            "a1",
            &serde_json::json!({"title": "Loose bug", "type": "bug"}),
        ),
    ];

    let planned = record_surface(&events, "planned-task")?;
    let line = field_line(&planned, "parent")?;
    assert!(
        line.contains("plan-p"),
        "a planned record lists its parent exactly as the record names it: {line}"
    );

    let loose = record_surface(&events, "loose-bug")?;
    let line = field_line(&loose, "parent")?;
    assert!(
        line.contains('—'),
        "a record naming no parent renders the field as explicitly absent: {line}"
    );
    Ok(())
}
