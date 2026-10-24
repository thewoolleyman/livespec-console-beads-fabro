//! A plan page uses the public plan vocabulary, never the storage type.

use console_application::{project_plan_page, render_plan_page_html};
use console_domain::{ConsoleEvent, EventType};

#[test]
fn plan_page_never_exposes_the_epic_storage_type() {
    let event = ConsoleEvent::fixture(
        "plan-page-epic",
        EventType::WorkItemSnapshotObserved,
        "orchestrator",
    )
    .with_payload_json(
        serde_json::json!({
            "repo": "livespec-console-beads-fabro",
            "work_item_id": "plan-page",
            "lane": "ready",
            "rank": "a0",
            "status": "open",
            "source_version": 1,
            "detail": {"title": "Plans redesign", "item_type": "epic"},
        })
        .to_string(),
    );

    let page = project_plan_page(&[event], "plan-page");
    let rendered = render_plan_page_html("plan-page", &page);

    assert!(rendered.contains("<h2>Plan</h2>"), "{rendered}");
    assert!(
        !rendered.to_ascii_lowercase().contains("epic"),
        "{rendered}"
    );
}
