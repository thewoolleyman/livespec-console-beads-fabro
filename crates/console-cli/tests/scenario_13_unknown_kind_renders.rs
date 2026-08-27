//! Scenario 13 — an attention kind the console has never seen survives
//! ingestion and renders verbatim in the Attention pane.
//!
//! Wire-consumer evidence, deliverable 1 (ddfbcx.3): Ruling R2 — the
//! attention kind set is the producer's, open and additive. The console is a
//! wire consumer and must not maintain a local enumeration that silently
//! drops kinds it predates.
//!
//! The parse-layer evidence already exists at
//! `console-application/tests/needs_attention_snapshot.rs:29` (landed by
//! leg 7 / e55mov). This scenario covers the rest of the pipeline:
//! `materialize_attention_items` → `unified_attention_entries` →
//! `build_tui_model_for_state` → `render_to_text`.

use console_application::source_adapters::{
    AttentionHandoff, AttentionItemSnapshot, AttentionSourceRef, attention_item_payload_json,
};
use console_application::{TuiInteractionState, TuiOverlay, build_tui_model_for_state};
use console_domain::{ConsoleEvent, EventType};
use console_tui::render_to_text;

#[test]
fn unknown_attention_kind_survives_ingestion_and_renders_verbatim() {
    // The kind "synthetic-future-kind-wire-consumer-evidence" appears in no
    // console code. A local kind filter on the ingestion or projection path
    // would silently drop this item, and the assertion below would fail.
    // Mandatory positive control (ddfbcx.3): this test was shown to FAIL
    // under a filter in unified_attention_entries that passed only the
    // known-kind set {"impl-ready", "human-valve", "acceptance"}. Exact
    // failure:
    //   output.as_ref().map(|rendered| rendered.contains(UNKNOWN_KIND)) == Ok(true)
    // Filter reverted; only the test ships.
    const UNKNOWN_KIND: &str = "synthetic-future-kind-wire-consumer-evidence";
    let item = AttentionItemSnapshot::new(
        "wi-wire-consumer-evidence",
        UNKNOWN_KIND,
        "high",
        UNKNOWN_KIND,
        AttentionSourceRef::new("test-repo", Some("wi-wire-consumer-evidence"), None),
        AttentionHandoff::new(UNKNOWN_KIND, None, "inspect:wi-wire-consumer-evidence"),
    );
    let event = ConsoleEvent::fixture(
        "evt-wire-consumer-evidence",
        EventType::AttentionItemAppeared,
        "needs-attention",
    )
    .with_payload_json(attention_item_payload_json(&item));
    let state = TuiInteractionState::new(0, TuiOverlay::None);
    let model = build_tui_model_for_state(&[event], &state);
    // 200 cols: the attention pane gets 38% of (200-18) ≈ 69 cols inner, more
    // than enough for a 44-char kind string that would be clipped at 112 cols
    // (where the pane is only ~33 inner cols wide).
    let output = render_to_text(&model, 200, 28);
    assert_eq!(
        output
            .as_ref()
            .map(|rendered| rendered.contains(UNKNOWN_KIND)),
        Ok(true),
        "unknown kind must appear verbatim in the rendered inbox"
    );
}
