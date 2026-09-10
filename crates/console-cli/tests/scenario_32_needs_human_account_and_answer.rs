//! Scenario 32 -- The needs-human account is rendered and the operator's answer
//! rides the resolve-blocked valve (`SPECIFICATION/scenarios.md`,
//! `SPECIFICATION/contracts.md`).
//!
//! The console CONSUMES orchestrator b3 here (charter D2): it renders the
//! account the orchestrator composed in the projected valve item's `summary`,
//! and it invokes the orchestrator's published action surface with the governed
//! argv. It never reads the factory, never reads the run, never scans console
//! events to fill a field the projection left out, and never writes the
//! `livespec-human-answer` ledger comment itself.

use std::cell::RefCell;

use console_application::source_adapters::{
    AcceptancePolicy, AdapterResult, AdmissionPolicy, AttentionHandoff, AttentionItemSnapshot,
    AttentionSourceRef, Lane, LaneReason, SourceProbe, SourceProbeOutcome, WorkItemComment,
    WorkItemDetail, WorkItemSnapshot, attention_item_payload_json, work_item_snapshot_payload_json,
};
use console_application::{
    AttentionDetail, DispatcherOrchestratorActionPort, OrchestratorActionOutcome,
    OrchestratorActionPort, OrchestratorActionRequest, PendingValve, TuiInteraction,
    TuiInteractionState, TuiOverlay, TuiScreenModel, build_tui_model_for_state,
    handle_work_item_resolve_blocked_command,
};
use console_domain::{CommandEnvelope, CommandType, ConsoleEvent, EventType};
use console_tui::{TuiRuntimeEffect, TuiTerminalInput, render_to_text, step_tui_runtime};

const WORK_ITEM: &str = "console-needs-human-1";
const REPO: &str = "livespec-console-beads-fabro";
const RUN_ID: &str = "01M1Y0QTYWGG5MKEBBT1XS0JNW";

/// The orchestrator's own account of the terminated run, as `needs-attention
/// --json` composes it into the valve item's `summary`: the run id, the factory
/// and its server, that the run terminated at the `needs_human` node and routed
/// the decision to this valve, why, what it reported, the preserved ref, and the
/// available actions. The console assembles none of it.
const ACCOUNT: &str = "fabro run 01M1Y0QTYWGG5MKEBBT1XS0JNW on factory hp (poweredge-xubuntu) \
                       terminated at the needs-human node and routed the decision to this valve. \
                       Why: needs-human. It reported: the gate needs an operator ruling. Tree \
                       preserved on refs/heads/needs-human/01M1Y0QTYWGG5MKEBBT1XS0JNW. Actions: \
                       resolve-blocked ready, resolve-blocked backlog, or leave blocked.";

/// A distinctive tail token of the account: present in the WHOLE detail, absent
/// from the elided row.
const ACCOUNT_TAIL: &str = "or leave blocked.";

const RESOLVE_VALVE: &str = "drive --action resolve-blocked:console-needs-human-1:ready";
const REWORK_VALVE: &str = "drive --action reject:console-needs-human-1:rework";

const ANSWER: &str = "Ship the narrow fix; the wide refactor is a separate item.";

const ANSWER_COMMENT: &str = "livespec-human-answer (operator via console, \
                              2026-09-07T14:00:00Z, resolve-blocked:console-needs-human-1:ready): \
                              Ship the narrow fix; the wide refactor is a separate item.";

/// The orchestrator's verbatim refusal of a poisoned answer -- its
/// template-opener preflight, which runs BEFORE anything is written.
const POISON_SUMMARY: &str = "the answer opens with a prompt-template marker; nothing was written and the item stays blocked";

/// The account is rendered VERBATIM: elided with an indicator on an inbox row
/// that cannot hold it, and WHOLE in the drilled-in detail.
#[test]
fn the_needs_human_account_is_rendered_elided_on_the_row_and_whole_in_the_detail()
-> AdapterResult<()> {
    let events = needs_human_scene(ACCOUNT, &[])?;
    let model = attention_model(&events);

    // The model carries the projection's summary byte-for-byte: nothing here
    // composed, augmented, or re-derived any part of it.
    assert_eq!(
        model.detail().and_then(AttentionDetail::account),
        Some(ACCOUNT),
        "the detail must carry the projected summary verbatim"
    );

    // The row: at the pinned 112x28 viewport the account cannot fit, so the row
    // elides it WITH AN INDICATOR rather than stopping mid-sentence in silence.
    let narrow = rendered(&model, 112, 28);
    // Matched by the row's leading marker plus the lane word, not by the whole
    // `Blocked: needs-human` phrase: since mx9u.2 the row leads with the work
    // item's own token, so at this deliberately narrow pane the lane phrase is
    // itself one of the things that elides -- which is the property under test.
    let row = narrow
        .lines()
        .find(|line| line.contains("> ") && line.contains("Blocked"))
        .unwrap_or_default()
        .to_owned();
    assert!(
        row.contains('…'),
        "an elided row must indicate the elision: {row}"
    );
    assert!(
        !row.contains(ACCOUNT_TAIL),
        "the elided row cannot be holding the whole account: {row}"
    );

    // The detail: given a viewport whose Detail pane can hold it, the whole
    // account renders, untruncated. The viewport is wider than it used to need
    // to be because the Attention view now splits its body 50/50 rather than
    // 38/62 (livespec-console-beads-fabro-mx9u.2) -- the Detail pane is half of
    // what is left after the navigation pane, so holding a ~380-character
    // account on one line takes about 800 columns. What is under test is
    // unchanged: given room, the account renders WHOLE.
    assert!(
        rendered(&model, 820, 32).contains(ACCOUNT),
        "the drilled-in detail must render the whole summary"
    );
    Ok(())
}

/// The fail-soft case: a projection carrying only the item's title renders the
/// title and the valves, and asserts nothing about the run.
#[test]
fn a_valve_item_carrying_only_the_title_renders_the_title_and_the_valves() -> AdapterResult<()> {
    const TITLE: &str = "Wire the answer field into the resolve-blocked dialog";
    let events = needs_human_scene(TITLE, &[])?;
    let screen = rendered(&attention_model(&events), 640, 32);

    assert!(screen.contains(TITLE), "the title must render");
    assert!(
        screen.contains(RESOLVE_VALVE),
        "the resolve-blocked valve must render"
    );
    assert!(
        screen.contains(REWORK_VALVE),
        "the rework valve must render"
    );
    // Nothing about the run is asserted: the console did not reach for the
    // factory, the run, or the console event log to invent an account.
    for absent in [
        RUN_ID,
        "needs-human node",
        "refs/heads/needs-human",
        "poweredge-xubuntu",
    ] {
        assert!(
            !screen.contains(absent),
            "an account-less projection must not report {absent}"
        );
    }
    Ok(())
}

/// A confirmed resolve-blocked dialog carrying an answer persists that answer in
/// the command payload, and the governed drive argv carries
/// `resolve-blocked:<id>:ready` followed by `--answer` and the answer verbatim.
#[test]
fn a_confirmed_answer_rides_the_resolve_blocked_valve_as_the_answer_argument() -> AdapterResult<()>
{
    let events = needs_human_scene(ACCOUNT, &[])?;
    let mut state = TuiInteractionState::new(
        0,
        TuiOverlay::ValveConfirm {
            valve: PendingValve::MoveStatus {
                from: Lane::Blocked,
                to: Lane::Ready,
            },
            answer: String::new(),
        },
    );

    // The operator types the answer into the dialog, one keystroke at a time.
    for character in ANSWER.chars() {
        state = step_tui_runtime(
            &state,
            &events,
            TuiTerminalInput::Interaction(TuiInteraction::TypeChar(character)),
            "operator",
        )
        .state()
        .clone();
    }
    assert_eq!(
        state.overlay().valve_answer(),
        Some(ANSWER),
        "the dialog must hold the operator's text verbatim"
    );
    // The field renders beside the target-status choice.
    assert!(
        rendered(&build_tui_model_for_state(&events, &state), 640, 32).contains(ANSWER),
        "the dialog must echo the answer it will send"
    );

    // Confirming persists `work_item.resolve_blocked_requested` carrying both
    // the target status and the answer.
    let confirmed = step_tui_runtime(&state, &events, TuiTerminalInput::Confirm, "operator");
    let effect = confirmed.effect();
    assert_eq!(
        persisted_command(effect).map(CommandEnvelope::command_type),
        Some(&CommandType::WorkItemResolveBlockedRequested)
    );
    assert_eq!(
        persisted_command(effect).map(CommandEnvelope::aggregate_id),
        Some(WORK_ITEM)
    );
    let payload = persisted_payload(effect).unwrap_or_default().to_owned();
    let parsed: serde_json::Value = serde_json::from_str(&payload).unwrap_or_default();
    assert_eq!(parsed["target_status"], serde_json::json!("ready"));
    assert_eq!(parsed["answer"], serde_json::json!(ANSWER));

    // And the handler invokes the orchestrator's published action surface with
    // the governed argv: the action id, then `--answer`, then the answer.
    let command = persisted_command(effect)
        .cloned()
        .unwrap_or_else(resolve_blocked_command);
    let probe = ArgRecordingProbe::default();
    let mut port = DispatcherOrchestratorActionPort::new(&probe, "drive", &["--repo", "/orch"]);
    assert_eq!(
        handle_work_item_resolve_blocked_command(&command, &payload, &mut port)
            .map(|outcome| outcome.command_status().to_owned()),
        Ok("completed".to_owned())
    );
    assert_eq!(
        probe.observed(),
        vec![vec![
            "--repo".to_owned(),
            "/orch".to_owned(),
            "--action".to_owned(),
            format!("resolve-blocked:{WORK_ITEM}:ready"),
            "--answer".to_owned(),
            ANSWER.to_owned(),
            "--invoker".to_owned(),
            "operator".to_owned(),
        ]],
        "the answer must ride the governed drive argv verbatim"
    );
    Ok(())
}

/// A poisoned answer is refused by the orchestrator: the refusal surfaces
/// verbatim as the command's outcome, and no comment and no transition are
/// observed on the item.
#[test]
fn a_poisoned_answer_surfaces_the_refusal_verbatim_and_leaves_the_item_blocked() -> AdapterResult<()>
{
    let mut events = needs_human_scene(ACCOUNT, &[])?;
    let command = resolve_blocked_command();
    let payload = serde_json::json!({"target_status": "ready", "answer": ANSWER}).to_string();
    let mut port = RefusingActionPort;

    let outcome = handle_work_item_resolve_blocked_command(&command, &payload, &mut port).ok();

    assert_eq!(
        outcome
            .as_ref()
            .map(|outcome| outcome.command_status().to_owned()),
        Some("failed".to_owned())
    );
    let failure = outcome
        .as_ref()
        .and_then(|outcome| {
            outcome
                .events()
                .iter()
                .find(|event| *event.event_type() == EventType::WorkItemActionFailed)
        })
        .cloned();
    assert!(
        failure
            .as_ref()
            .is_some_and(|event| event.payload_json().contains(POISON_SUMMARY)),
        "the orchestrator's refusal must ride the command outcome: {failure:?}"
    );

    // The operator reads that refusal verbatim on the item's own surface.
    events.extend(failure);
    let model = attention_model(&events);
    let line = model
        .action_failure_for(WORK_ITEM)
        .map(console_application::ActionFailure::display_line)
        .unwrap_or_default();
    assert!(
        line.contains("poisoned-human-answer") && line.contains(POISON_SUMMARY),
        "the refusal must surface verbatim: {line}"
    );

    // No comment and no transition are observed: the item is still blocked and
    // its detail carries no answer comment.
    assert_eq!(
        model.detail().map(AttentionDetail::answer_comments),
        Some([].as_slice()),
        "a refused answer writes no comment"
    );
    assert!(
        rendered(&model, 640, 32).contains("Blocked: needs-human"),
        "a refused answer leaves the item blocked"
    );
    Ok(())
}

/// After a successful resolve, the item's detail shows the
/// `livespec-human-answer` comment as the context surface returns it.
#[test]
fn the_detail_shows_the_livespec_human_answer_comment_after_a_successful_resolve()
-> AdapterResult<()> {
    let events = needs_human_scene(ACCOUNT, &[ANSWER_COMMENT])?;
    let model = attention_model(&events);

    assert_eq!(
        model.detail().map(AttentionDetail::answer_comments),
        Some([ANSWER_COMMENT.to_owned()].as_slice()),
        "the answer comment must be carried verbatim as the surface returned it"
    );
    assert!(
        rendered(&model, 640, 32).contains(ANSWER_COMMENT),
        "the detail must show the livespec-human-answer comment"
    );
    Ok(())
}

/// The scene: a work-item resting at `blocked / needs-human` whose projected
/// valve item carries `summary` as its account, beside the rework valve the
/// projection also advertises for it. `comments` are the ledger comments the
/// context surface returned with the record.
fn needs_human_scene(summary: &str, comments: &[&str]) -> AdapterResult<Vec<ConsoleEvent>> {
    Ok(vec![
        work_item_event(comments)?,
        attention_event(
            "evt_attn_resolve",
            "needs-human:console-needs-human-1",
            summary,
            RESOLVE_VALVE,
        ),
        attention_event(
            "evt_attn_rework",
            "needs-human-rework:console-needs-human-1",
            summary,
            REWORK_VALVE,
        ),
    ])
}

fn work_item_event(comments: &[&str]) -> AdapterResult<ConsoleEvent> {
    let detail = WorkItemDetail {
        title: Some("Wire the answer field into the resolve-blocked dialog".to_owned()),
        comments: comments
            .iter()
            .map(|text| WorkItemComment {
                text: (*text).to_owned(),
                ..WorkItemComment::default()
            })
            .collect(),
        ..WorkItemDetail::default()
    };
    let snapshot = WorkItemSnapshot::new(
        REPO,
        WORK_ITEM,
        Lane::Blocked,
        Some(LaneReason::NeedsHuman),
        "a0",
        "blocked",
        AdmissionPolicy::Manual,
        AcceptancePolicy::AiThenHuman,
        1,
    )?
    .with_detail(detail);
    Ok(ConsoleEvent::fixture(
        "evt_wi_blocked",
        EventType::WorkItemSnapshotObserved,
        "beads",
    )
    .with_payload_json(work_item_snapshot_payload_json(&snapshot)))
}

fn attention_event(event_id: &str, id: &str, summary: &str, command: &str) -> ConsoleEvent {
    let item = AttentionItemSnapshot::new(
        id,
        "human-valve",
        "high",
        summary,
        AttentionSourceRef::new(REPO, Some(WORK_ITEM), None),
        AttentionHandoff::new("human-valve", None, command),
    );
    ConsoleEvent::fixture(
        event_id,
        EventType::AttentionItemAppeared,
        "needs-attention",
    )
    .with_payload_json(attention_item_payload_json(&item))
}

fn attention_model(events: &[ConsoleEvent]) -> TuiScreenModel {
    build_tui_model_for_state(events, &TuiInteractionState::new(0, TuiOverlay::None))
}

/// The rendered screen at `width`x`height`, or the empty string when the
/// viewport is unrenderable (which every assertion below then fails on).
fn rendered(model: &TuiScreenModel, width: u16, height: u16) -> String {
    render_to_text(model, width, height).unwrap_or_default()
}

const fn persisted_command(effect: &TuiRuntimeEffect) -> Option<&CommandEnvelope> {
    match effect {
        TuiRuntimeEffect::PersistCommand(command)
        | TuiRuntimeEffect::PersistCommandWithPayload { command, .. } => Some(command),
        _other => None,
    }
}

fn persisted_payload(effect: &TuiRuntimeEffect) -> Option<&str> {
    match effect {
        TuiRuntimeEffect::PersistCommandWithPayload { payload_json, .. } => Some(payload_json),
        _other => None,
    }
}

fn resolve_blocked_command() -> CommandEnvelope {
    CommandEnvelope::new(
        "cmd_resolve_blocked".to_owned(),
        CommandType::WorkItemResolveBlockedRequested,
        WORK_ITEM.to_owned(),
        format!("{WORK_ITEM}:work_item.resolve_blocked_requested"),
        "operator".to_owned(),
    )
}

/// Records the exact argv the console hands the orchestrator's `drive` surface.
#[derive(Default)]
struct ArgRecordingProbe {
    calls: RefCell<Vec<Vec<String>>>,
}

impl ArgRecordingProbe {
    fn observed(&self) -> Vec<Vec<String>> {
        self.calls.borrow().clone()
    }
}

impl SourceProbe for ArgRecordingProbe {
    fn run_command(&self, _program: &str, args: &[&str]) -> SourceProbeOutcome {
        self.calls
            .borrow_mut()
            .push(args.iter().map(|arg| (*arg).to_owned()).collect());
        SourceProbeOutcome::observed("", true)
    }

    fn read_file(&self, _path: &str) -> SourceProbeOutcome {
        SourceProbeOutcome::Unavailable {
            reason: "the argv probe reads no files".to_owned(),
        }
    }
}

/// Stands in for the orchestrator refusing a poisoned answer at its
/// template-opener preflight, with nothing written.
struct RefusingActionPort;

impl OrchestratorActionPort for RefusingActionPort {
    fn run_action(
        &mut self,
        _request: &OrchestratorActionRequest,
    ) -> console_application::ApplicationResult<OrchestratorActionOutcome> {
        Ok(OrchestratorActionOutcome::failed_with_refusal(
            serde_json::json!({
                "action_id": format!("resolve-blocked:{WORK_ITEM}:ready"),
                "domain_error": "poisoned-human-answer",
                "summary": POISON_SUMMARY,
            })
            .to_string(),
        ))
    }
}
