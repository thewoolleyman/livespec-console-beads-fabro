//! Scenario 31 -- Per-item verbs are offered wherever the operator sees the
//! work-item (`SPECIFICATION/scenarios.md`), grading the `contracts.md` TUI
//! Contract clause "Per-item verb surface parity" (v047, gap-3a3bzkps).
//!
//! The clause: a needs-attention row backed by a known work-item id resolves the
//! SAME standardized record a drilled-in lane selection resolves, so it MUST
//! offer every per-item verb that record's lifecycle state admits -- per-item
//! dispatch, the move-to-status picker, the driver handoff, the human valves --
//! with the same availability predicates and the same persisted command
//! envelopes. The hosting view is NEVER an availability input. A row backed by
//! no known work-item id offers nothing and hints nothing.
//!
//! The measured failure this grades (2026-08-31 dogfooding transcript): the
//! item that needed the operator was sitting in the inbox with every verb
//! presented unavailable, and the operator had to leave the inbox, re-find the
//! same item under Lanes, and drill in before anything lit up.

use console_application::action_registry::{self, ActionSpec};
use console_application::source_adapters::{
    AcceptancePolicy, AdapterResult, AdmissionPolicy, AttentionHandoff, AttentionItemSnapshot,
    AttentionSourceRef, Lane, WorkItemSnapshot, attention_item_payload_json,
    work_item_snapshot_payload_json,
};
use console_application::{
    AttentionDetail, LaneFocus, OperatorAction, TuiInteractionState, TuiOverlay, TuiScreenModel,
    TuiView, build_tui_model_for_state,
};
use console_domain::{ConsoleEvent, EventType};
use console_tui::{TuiRuntimeEffect, TuiTerminalInput, step_tui_runtime};

const REPO: &str = "livespec-console-beads-fabro";

/// One ingested work-item snapshot for `work_item_id` resting in `lane`.
///
/// Neither `ready` nor `backlog` draws attention on its own (the lane fold
/// surfaces only the lanes that rest on a human step), so the record reaches the
/// inbox through the orchestrator's needs-attention row below -- which is
/// exactly the shape the clause is about.
fn work_item_event(event_id: &str, work_item_id: &str, lane: Lane) -> AdapterResult<ConsoleEvent> {
    let snapshot = WorkItemSnapshot::new(
        REPO,
        work_item_id,
        lane,
        None,
        "a0",
        match lane {
            Lane::Backlog => "backlog",
            _other => "ready",
        },
        AdmissionPolicy::Manual,
        AcceptancePolicy::AiThenHuman,
        1,
    )?;
    Ok(ConsoleEvent::new(
        event_id.to_owned(),
        1,
        "factory".to_owned(),
        EventType::WorkItemSnapshotObserved,
        "orchestrator".to_owned(),
        format!("{REPO}:{work_item_id}"),
        1,
    )
    .with_payload_json(work_item_snapshot_payload_json(&snapshot)))
}

/// One ingested needs-attention row whose source reference names `work_item`.
fn attention_row_event(event_id: &str, work_item: &str) -> ConsoleEvent {
    let item = AttentionItemSnapshot::new(
        &format!("impl:{work_item}"),
        "impl-ready",
        "normal",
        &format!("Ready implementation work: {work_item}"),
        AttentionSourceRef::new(REPO, Some(work_item), None),
        AttentionHandoff::new(
            "drive",
            Some(&format!("impl:{work_item}")),
            &format!("drive --action impl:{work_item}"),
        ),
    );
    attention_event(event_id, &item)
}

/// One ingested needs-attention row that names a PATH and no work-item at all.
fn pathy_attention_row_event(event_id: &str) -> ConsoleEvent {
    let item = AttentionItemSnapshot::new(
        "spec:revise:SPECIFICATION/contracts.md",
        "spec-revise",
        "normal",
        "Spec revision owed: contracts.md",
        AttentionSourceRef::new(REPO, None, Some("SPECIFICATION/contracts.md")),
        AttentionHandoff::new(
            "livespec",
            Some("revise"),
            "claude \"/livespec:revise SPECIFICATION/contracts.md\"",
        ),
    );
    attention_event(event_id, &item)
}

fn attention_event(event_id: &str, item: &AttentionItemSnapshot) -> ConsoleEvent {
    ConsoleEvent::new(
        event_id.to_owned(),
        1,
        "factory".to_owned(),
        EventType::AttentionItemAppeared,
        "needs-attention".to_owned(),
        format!("attention_item:{REPO}:{}", item.id()),
        1,
    )
    .with_payload_json(attention_item_payload_json(item))
}

/// The inbox state with the single needs-attention row selected.
const fn attention_state() -> TuiInteractionState {
    TuiInteractionState::for_view(TuiView::Attention, 0, TuiOverlay::None)
}

/// The Lanes state drilled into `lane` with its first item selected.
fn drill_state(lane: Lane) -> TuiInteractionState {
    TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
        .with_lane_focus(LaneFocus::Lane(lane))
        .with_selected_lane_item_index(0)
}

/// Every per-item verb the registry OFFERS for the model's current selection,
/// by action id. This is the one derivation both the Status-line hints and the
/// key handlers consult, evaluated here on whichever surface `model` sits on.
fn offered_verbs(model: &TuiScreenModel) -> Vec<&'static str> {
    let Some(ctx) = model.selected_action_context() else {
        return Vec::new();
    };
    action_registry::ACTION_REGISTRY
        .iter()
        .filter(|spec| is_per_item(spec) && (spec.availability)(&ctx))
        .map(|spec| spec.id)
        .collect()
}

/// The per-item Status-line key tokens offered for the model's selection.
fn hinted_keys(model: &TuiScreenModel) -> Vec<&'static str> {
    model
        .selected_action_context()
        .map(|ctx| action_registry::available_hint_tokens(&ctx))
        .unwrap_or_default()
}

/// Whether `spec` is a PER-ITEM verb: the selection-less globals and the
/// board-wide ready drain are not, and neither is claimed by this clause.
const fn is_per_item(spec: &ActionSpec) -> bool {
    !matches!(
        spec.staging,
        action_registry::ActionStaging::Global(_) | action_registry::ActionStaging::FactoryDrain
    )
}

/// The action ids the needs-attention DETAIL pane presses on the selected row.
fn detail_action_ids(model: &TuiScreenModel) -> Vec<&'static str> {
    model
        .detail()
        .map(AttentionDetail::actions)
        .unwrap_or_default()
        .iter()
        .map(|action| match action {
            OperatorAction::Registered(id) => *id,
        })
        .collect()
}

/// The menu coordinates of `action_id`, so the test drives the SAME generated
/// menu the operator does rather than a private staging shortcut.
fn menu_position_for(action_id: &str) -> Option<(usize, usize)> {
    action_registry::menu_tree()
        .iter()
        .enumerate()
        .find_map(|(top_index, _top)| {
            action_registry::menu_actions(top_index)
                .iter()
                .position(|spec| spec.id == action_id)
                .map(|action_index| (top_index, action_index))
        })
}

/// Drive `Enter` on the generated menu row for `action_id` from `state`, then
/// `Enter` again on whatever confirmation it opened, and return the effect.
fn confirm_through_the_menu(
    state: &TuiInteractionState,
    events: &[ConsoleEvent],
    action_id: &str,
) -> (TuiOverlay, TuiRuntimeEffect) {
    // `(0, 0)` is the `Work item > Hand off > Driver handoff` row, so a lookup
    // that ever missed would stage a DIFFERENT action rather than silently
    // stage nothing — the assertion on the staged overlay catches it.
    let (top, selected) = menu_position_for(action_id).unwrap_or((0, 0));
    let opened = step_tui_runtime(
        &state
            .clone()
            .with_overlay(TuiOverlay::Menu { top, selected }),
        events,
        TuiTerminalInput::Confirm,
        "operator",
    );
    let confirmed = step_tui_runtime(
        opened.state(),
        events,
        TuiTerminalInput::Confirm,
        "operator",
    );
    (opened.state().overlay().clone(), confirmed.effect().clone())
}

#[test]
fn a_ready_item_offers_the_same_per_item_verbs_on_the_inbox_row_and_in_the_drilled_lane()
-> AdapterResult<()> {
    let events = [
        work_item_event("evt_wi_ready", "wi-parity-ready", Lane::Ready)?,
        attention_row_event("evt_attn_ready", "wi-parity-ready"),
    ];

    let inbox = build_tui_model_for_state(&events, &attention_state());
    let drill = build_tui_model_for_state(&events, &drill_state(Lane::Ready));

    // Both surfaces resolve the SAME record, so the premise of the parity claim
    // holds before its conclusion is asserted.
    assert_eq!(inbox.selected_work_item_id(), Some("wi-parity-ready"));
    assert_eq!(drill.selected_work_item_id(), Some("wi-parity-ready"));

    let inbox_verbs = offered_verbs(&inbox);
    assert_eq!(
        inbox_verbs,
        offered_verbs(&drill),
        "the hosting view must not be an availability input"
    );
    assert!(
        inbox_verbs.contains(&"dispatch-selected-item"),
        "per-item dispatch is state-admitted at `ready`: {inbox_verbs:?}"
    );
    assert!(
        inbox_verbs.contains(&"move"),
        "the move-to-status picker is state-admitted at `ready`: {inbox_verbs:?}"
    );
    // And the Status-line hints on each surface name the same per-item keys.
    assert_eq!(hinted_keys(&inbox), hinted_keys(&drill));
    Ok(())
}

#[test]
fn confirming_per_item_dispatch_from_the_inbox_row_persists_the_item_dispatch_command()
-> AdapterResult<()> {
    let events = [
        work_item_event("evt_wi_ready", "wi-parity-dispatch", Lane::Ready)?,
        attention_row_event("evt_attn_ready", "wi-parity-dispatch"),
    ];

    let (staged, effect) =
        confirm_through_the_menu(&attention_state(), &events, "dispatch-selected-item");

    assert_eq!(
        staged,
        TuiOverlay::FactoryDispatchItemConfirm {
            work_item_id: "wi-parity-dispatch".to_owned(),
        },
        "the inbox row stages the same confirmation dialog the drilled lane does"
    );
    // The persisted envelope, read off the effect: the aggregate is the row's
    // own work-item and the command is `factory.dispatch_item_requested`.
    let persisted = match &effect {
        TuiRuntimeEffect::PersistCommand(command) => Some((
            command.aggregate_id().to_owned(),
            command.idempotency_key().to_owned(),
        )),
        _other => None,
    };
    assert_eq!(
        persisted,
        Some((
            "wi-parity-dispatch".to_owned(),
            "wi-parity-dispatch:factory.dispatch_item_requested".to_owned(),
        )),
        "confirming per-item dispatch from the inbox row must persist the item \
         dispatch command; got {effect:?}"
    );

    // The SAME dialog and the SAME envelope the drilled-in lane produces.
    let (drill_staged, drill_effect) =
        confirm_through_the_menu(&drill_state(Lane::Ready), &events, "dispatch-selected-item");
    assert_eq!(staged, drill_staged);
    assert_eq!(effect, drill_effect);
    Ok(())
}

#[test]
fn a_backlog_inbox_row_offers_move_and_the_driver_handoff_but_neither_approve_nor_accept()
-> AdapterResult<()> {
    let events = [
        work_item_event("evt_wi_backlog", "wi-parity-backlog", Lane::Backlog)?,
        attention_row_event("evt_attn_backlog", "wi-parity-backlog"),
    ];

    let inbox = build_tui_model_for_state(&events, &attention_state());
    let verbs = offered_verbs(&inbox);

    assert!(verbs.contains(&"move"), "{verbs:?}");
    assert!(verbs.contains(&"driver-handoff"), "{verbs:?}");
    assert!(
        !verbs.contains(&"approve"),
        "the vocabulary does not admit approve at `backlog`: {verbs:?}"
    );
    assert!(
        !verbs.contains(&"accept"),
        "the vocabulary does not admit accept at `backlog`: {verbs:?}"
    );

    // The pressable detail-pane roster is the same offering, not a second one.
    assert_eq!(detail_action_ids(&inbox), verbs);
    Ok(())
}

#[test]
fn an_inbox_row_with_no_work_item_id_offers_no_per_item_verb_and_hints_no_per_item_key() {
    let events = [pathy_attention_row_event("evt_attn_pathy")];

    let inbox = build_tui_model_for_state(&events, &attention_state());

    assert_eq!(inbox.attention_items().len(), 1, "the row is present");
    assert_eq!(inbox.selected_work_item_id(), None);
    assert_eq!(offered_verbs(&inbox), Vec::<&str>::new());
    assert_eq!(hinted_keys(&inbox), Vec::<&str>::new());
    assert_eq!(detail_action_ids(&inbox), Vec::<&str>::new());
    assert_eq!(inbox.footer(), action_registry::global_status_hint());
}
