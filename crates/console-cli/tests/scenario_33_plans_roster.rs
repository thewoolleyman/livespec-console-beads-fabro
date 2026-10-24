//! Scenario 33 -- Plans are first-class console records.
//!
//! The Plans roster consumes the same persisted work-item snapshots as Lanes.
//! It recognises only open `epic` records as plans, counts children from their
//! published `parent` field, and leaves fields the source does not publish
//! explicitly absent. This acceptance test drives the rendered console rather
//! than a renderer-private helper so the view registry, roster, status hint,
//! and Help overlay stay in lockstep.

use console_application::{TuiInteractionState, TuiOverlay, TuiView, build_tui_model_for_state};
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
