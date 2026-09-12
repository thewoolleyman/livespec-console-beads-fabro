//! The `active` lane's `executing` count decays when the run behind it is
//! observed to have stopped.
//!
//! # Why this gate exists
//!
//! Measured at the real TUI on 2026-09-12 (build `3296ba8`,
//! `livespec-console-beads-fabro-prgntw`): the Lanes view showed
//! `active (2); executing 2 claimed 0`, with
//! `livespec-console-beads-fabro-gqmtwa.4 [active] executing`. At that moment
//! that item's Fabro run `01M29W537ZC97M5HHE9S0P71BM` was `succeeded`, its PR
//! had MERGED, and AI acceptance had already failed it back to
//! `rework:pending`. Nothing was executing.
//!
//! The fold behind `executing` inserted `Executing` when a run was observed
//! RUNNING and then did nothing at all for the same run observed at a terminal
//! status kind, so the first observation's claim stood forever. The only
//! downgrade path was a dispatcher-journal entry carrying a terminal status,
//! and for that item the journal entry that would have corrected it never
//! arrived — the newest one predated the merge. `executing` is the console's
//! WIP signal against a `wip_cap: 1` repo, so a phantom `executing` inflates
//! apparent WIP and is indistinguishable on screen from a live run.
//!
//! # The observation that justifies the decayed state
//!
//! `FabroRunState::is_running` is the ONE classification the console draws
//! from a status kind, and it is drawn POSITIVELY: a run is executing only
//! when the source says `running`. A later observation of the SAME run at any
//! other status kind is that same reading returning false — the console
//! observed that this run is no longer reported running. Withdrawing the claim
//! that observation made is not an inference about the item's lifecycle; it is
//! the observation expiring with its own source.
//!
//! What the console must NOT do is promote the item to `finished?`. That state
//! means "a TERMINAL outcome was observed", and the only source that says so
//! explicitly is the dispatcher journal's `terminal_status` field. Fabro's
//! status vocabulary belongs to Fabro (see `FabroRunState`'s doc comment: the
//! adapter READS each observed kind rather than classifying it), so the console
//! cannot tell `succeeded` from `queued` without inventing the closed enum that
//! type exists to refuse. It therefore falls back to `claimed` — in the active
//! lane, with no run currently observed executing it — and leaves
//! finished-ness to the journal, exactly as `SPECIFICATION/contracts.md`'s
//! never-infer-state-from-an-observation clause requires.

use console_application::source_adapters::Lane;
use console_application::{TuiInteractionState, TuiOverlay, TuiView, build_tui_model_for_state};
use console_domain::{ConsoleEvent, EventType};
use console_tui::render_to_text;

/// The index of the `active` column in `Lane::all()` order, which is what the
/// Lanes overview's `with_selected_lane_index` addresses.
const ACTIVE_LANE_INDEX: usize = 3;

/// The item the dogfood pass caught: merged, then returned to rework, still
/// rendered `executing`.
const ITEM: &str = "console-merged-then-reworked";

/// A work-item snapshot observation placing `ITEM` in the `active` lane.
fn active_lane_event(event_id: &str) -> ConsoleEvent {
    let payload = format!(
        concat!(
            r#"{{"repo":"console","work_item_id":"{}","lane":"{}","lane_reason":null,"#,
            r#""rank":"a1","status":"active","detail":{{"title":"Merged, then returned to rework"}},"#,
            r#""source_version":1}}"#
        ),
        ITEM,
        Lane::Active.label()
    );
    ConsoleEvent::fixture(
        event_id,
        EventType::WorkItemSnapshotObserved,
        "orchestrator",
    )
    .with_payload_json(payload)
}

/// A Fabro run observation for `ITEM`, carrying the status kind VERBATIM as the
/// source reported it.
fn fabro_run_event(event_id: &str, run_id: &str, status_kind: &str) -> ConsoleEvent {
    let payload = format!(
        r#"{{"repo":"console","work_item_id":"{ITEM}","run_id":"{run_id}","state":"{status_kind}","source_version":1}}"#
    );
    ConsoleEvent::fixture(event_id, EventType::FabroRunObserved, "fabro").with_payload_json(payload)
}

/// A dispatcher-journal observation of work IN PROGRESS: `kind: progress`,
/// carrying NO `terminal_status`. This is the entry the measured store actually
/// held for the phantom item (seq 10077 at 03:56:35Z, recorded before the
/// merge), and it is a second thing claiming `executing` on that item's behalf
/// — so the decay has to withdraw this claim too, not just the run's.
fn dispatcher_progress_event(event_id: &str, dispatch_id: &str) -> ConsoleEvent {
    let payload = format!(
        r#"{{"repo":"console","work_item_id":"{ITEM}","dispatch_id":"{dispatch_id}","kind":"progress","source_version":2}}"#
    );
    ConsoleEvent::fixture(
        event_id,
        EventType::DispatcherJournalProgressObserved,
        "dispatcher",
    )
    .with_payload_json(payload)
}

/// A dispatcher-journal observation carrying an explicit `terminal_status` —
/// the one source that says "finished" in so many words.
fn dispatcher_terminal_event(event_id: &str, dispatch_id: &str) -> ConsoleEvent {
    let payload = format!(
        r#"{{"repo":"console","work_item_id":"{ITEM}","dispatch_id":"{dispatch_id}","kind":"backlog-bounce","terminal_status":"completed","source_version":3}}"#
    );
    ConsoleEvent::fixture(
        event_id,
        EventType::DispatcherBacklogBounceObserved,
        "dispatcher",
    )
    .with_payload_json(payload)
}

/// The Lanes overview exactly as the operator reads it, with the `active`
/// column selected. A render error degrades to the empty string, which fails
/// every assertion below with the frame it did produce in the message.
fn lanes_overview_text(events: &[ConsoleEvent]) -> String {
    let model = build_tui_model_for_state(
        events,
        &TuiInteractionState::for_view(TuiView::Lanes, 0, TuiOverlay::None)
            .with_selected_lane_index(ACTIVE_LANE_INDEX),
    );
    render_to_text(&model, 120, 30).unwrap_or_default()
}

/// ACCEPTANCE 1 and 2: the EXACT measured sequence — a journal entry claiming
/// progress with no terminal status, the run observed running, then the SAME
/// run observed at a terminal status kind, and NO journal entry carrying a
/// terminal status ever. That is the store as it stood when the Lanes view
/// showed a merged, reworked item as `executing`. The summary must not say
/// `executing 1`.
///
/// The journal's stale `progress` claim is in the population deliberately:
/// withdrawing only the RUN's claim would leave the journal's standing and
/// reproduce the defect exactly, which is why the brief says not to simply
/// trust the journal more — the entry that would have corrected it never came.
///
/// ACCEPTANCE 3: the row label and the summary agree — the item reads
/// `claimed` and the summary counts it under `claimed`, not under `finished?`
/// (a state no observation here justifies).
#[test]
fn a_run_observed_running_then_observed_stopped_is_no_longer_executing() {
    let events = [
        active_lane_event("evt_snapshot"),
        dispatcher_progress_event("evt_journal_progress", "dispatch_1"),
        fabro_run_event("evt_run_running", "01M29W537ZC97M5HHE9S0P71BM", "running"),
        fabro_run_event(
            "evt_run_succeeded",
            "01M29W537ZC97M5HHE9S0P71BM",
            "succeeded",
        ),
    ];

    let rendered = lanes_overview_text(&events);

    assert!(
        !rendered.contains("executing 1"),
        "the stopped run still counts toward WIP:\n{rendered}"
    );
    assert!(
        rendered.contains("active (1); executing 0 claimed 1"),
        "{rendered}"
    );
    assert!(
        rendered.contains(&format!("{ITEM} [active] claimed")),
        "{rendered}"
    );
    assert!(
        !rendered.contains("finished?"),
        "no observation here reported a terminal OUTCOME:\n{rendered}"
    );
}

/// The same decay with the journal silent throughout: a run observed running
/// and then observed stopped withdraws its own claim on its own, without
/// needing any dispatcher entry to arrive and say so.
#[test]
fn a_stopped_run_withdraws_its_claim_with_no_journal_entry_at_all() {
    let events = [
        active_lane_event("evt_snapshot"),
        fabro_run_event("evt_run_running", "01M29W537ZC97M5HHE9S0P71BM", "running"),
        fabro_run_event(
            "evt_run_succeeded",
            "01M29W537ZC97M5HHE9S0P71BM",
            "succeeded",
        ),
    ];

    let rendered = lanes_overview_text(&events);

    assert!(
        rendered.contains("active (1); executing 0 claimed 1"),
        "{rendered}"
    );
    assert!(
        rendered.contains(&format!("{ITEM} [active] claimed")),
        "{rendered}"
    );
}

/// The decay is not a blanket zero: a run whose LATEST observation says
/// `running` still claims execution, which is the positive reading the console
/// has always drawn.
#[test]
fn a_run_still_observed_running_is_still_executing() {
    let events = [
        active_lane_event("evt_snapshot"),
        fabro_run_event("evt_run_running", "01M29W537ZC97M5HHE9S0P71BM", "running"),
        fabro_run_event("evt_run_still", "01M29W537ZC97M5HHE9S0P71BM", "running"),
    ];

    let rendered = lanes_overview_text(&events);

    assert!(
        rendered.contains("active (1); executing 1 claimed 0"),
        "{rendered}"
    );
    assert!(
        rendered.contains(&format!("{ITEM} [active] executing")),
        "{rendered}"
    );
}

/// The claim expires with its OWN run, not with whichever run the poll happened
/// to report last. A rework item carries a finished run and a live one at once,
/// and `fabro ps --json` does not promise an order within a cycle — reading the
/// item-level claim off the last event seen would drop a genuinely live run.
#[test]
fn a_stopped_run_does_not_withdraw_a_sibling_run_still_observed_running() {
    let events = [
        active_lane_event("evt_snapshot"),
        fabro_run_event("evt_first_running", "01M29W537ZC97M5HHE9S0P71BM", "running"),
        fabro_run_event(
            "evt_second_running",
            "01M2ASK49HCSCK4N3DNB5KXGAF",
            "running",
        ),
        fabro_run_event(
            "evt_first_succeeded",
            "01M29W537ZC97M5HHE9S0P71BM",
            "succeeded",
        ),
    ];

    let rendered = lanes_overview_text(&events);

    assert!(
        rendered.contains("active (1); executing 1 claimed 0"),
        "the live sibling run's claim was withdrawn by the finished one:\n{rendered}"
    );
}

/// ACCEPTANCE 4: the never-infer contract is preserved in BOTH directions. A
/// run observed at a non-running status kind withdraws the execution claim, but
/// it never overwrites `finished?` — that state is owned by the dispatcher
/// journal's explicit `terminal_status`, which IS an observed terminal outcome.
#[test]
fn a_stopped_run_does_not_overwrite_an_observed_terminal_journal_outcome() {
    let events = [
        active_lane_event("evt_snapshot"),
        fabro_run_event("evt_run_running", "01M29W537ZC97M5HHE9S0P71BM", "running"),
        dispatcher_terminal_event("evt_journal_terminal", "dispatch_done"),
        fabro_run_event(
            "evt_run_succeeded",
            "01M29W537ZC97M5HHE9S0P71BM",
            "succeeded",
        ),
    ];

    let rendered = lanes_overview_text(&events);

    assert!(
        rendered.contains("active (1); executing 0 claimed 0 finished? 1"),
        "{rendered}"
    );
    assert!(
        rendered.contains(&format!("{ITEM} [active] finished?")),
        "{rendered}"
    );
}
