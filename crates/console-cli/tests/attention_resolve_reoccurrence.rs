//! `livespec-console-beads-fabro-mx9u.11` — an `attention_item.resolved`
//! event's identity must depend on WHICH occurrence is being resolved.
//!
//! Before this fix `attention_item_resolved_event` hashed only
//! `(repo, id, "resolved")`: every resolution of a given attention id computed
//! the identical identity, so the event store's `UNIQUE(source,
//! source_event_id)` constraint let only the FIRST resolution of an id EVER
//! land. An id that appeared, resolved, and then reappeared (a worktree
//! recreated at the same path, or any other toggling condition) computed the
//! same resolved identity on its second resolution, collided, and the append
//! was silently swallowed — the row then stayed open forever even though
//! `diff_needs_attention` correctly kept re-detecting its absence every poll.
//! Measured against the live ledger: 32 of 33 open `hygiene:stale-worktree:*`
//! rows already carried an earlier resolved event, having burned their one
//! possible identity on a prior toggle.
//!
//! These tests drive the real ingestion path end-to-end against a live event
//! store (the Scenario 12 precedent), proving: (1) a toggled id's second
//! resolution is a DISTINCT, successfully-persisted event; (2) the projected
//! inbox tracks open/retired correctly across repeated toggles; and (4) a
//! fixture shaped like the real stuck rows (one earlier resolved event
//! already on record) retires cleanly once the id genuinely disappears again.

use console_application::project_attention;
use console_application::source_adapters::{
    AttentionHandoff, AttentionItemSnapshot, AttentionSourceRef, NeedsAttentionReadOutcome,
    NeedsAttentionSnapshotPort,
};
use console_domain::{ConsoleEvent, EventType};
use console_eventstore::{
    AppendOutcome, AppendStatus, CommandAppend, CommandAppendOutcome, CommandStatusUpdateOutcome,
    EventAppend, EventStoreResult, SqliteEventStore, StoredCommand,
};
use livespec_console_beads_fabro::{
    ConsoleRuntimeError, FactoryCommandStore, NeedsAttentionIngest, ingest_needs_attention,
};

struct StubNeedsAttentionPort {
    snapshot: Vec<AttentionItemSnapshot>,
}

impl NeedsAttentionSnapshotPort for StubNeedsAttentionPort {
    fn read_snapshot(&self) -> NeedsAttentionReadOutcome {
        NeedsAttentionReadOutcome::Observed(self.snapshot.clone())
    }
}

const REPO: &str = "livespec-console-beads-fabro";

fn attention_item(id: &str, summary: &str) -> AttentionItemSnapshot {
    AttentionItemSnapshot::new(
        id,
        "hygiene",
        "low",
        summary,
        AttentionSourceRef::new(REPO, None, Some(id)),
        AttentionHandoff::new("reap", Some(id), &format!("reap:{id}")),
    )
}

fn ingest(
    store: &mut dyn FactoryCommandStore,
    snapshot: Vec<AttentionItemSnapshot>,
    observed_at: &str,
) -> Result<usize, ConsoleRuntimeError> {
    let port = StubNeedsAttentionPort { snapshot };
    let needs_attention = NeedsAttentionIngest::new(&port, REPO);
    ingest_needs_attention(store, &needs_attention, observed_at)
}

fn is_open(store: &SqliteEventStore, id: &str) -> Result<bool, ConsoleRuntimeError> {
    let events = store.list_console_events()?;
    let inbox = project_attention(&events);
    Ok(inbox.iter().any(|item| item.id() == id))
}

fn resolved_event_count(store: &SqliteEventStore) -> Result<usize, ConsoleRuntimeError> {
    Ok(store
        .list_console_events()?
        .iter()
        .filter(|event| event.event_type() == &EventType::AttentionItemResolved)
        .count())
}

/// AC1 + AC4: a `hygiene:stale-worktree`-shaped id appears, resolves,
/// reappears, and resolves again — BOTH resolutions must be persisted as
/// distinct events, and the row must end up retired (not stuck open).
#[test]
fn a_toggled_attention_item_persists_every_resolution() -> Result<(), ConsoleRuntimeError> {
    let mut store = SqliteEventStore::open_in_memory()?;
    let id = "hygiene:stale-worktree:/data/worktrees/example";

    // First occurrence: appears, then resolves. The very first resolution of
    // an id never collides (nothing has claimed its identity yet), so this
    // step passes even under the pre-fix formula — it is the SECOND
    // resolution below that exercises the fix.
    ingest(
        &mut store,
        vec![attention_item(id, "Stale worktree detected")],
        "t0",
    )?;
    assert!(is_open(&store, id)?, "item should be open after appearing");
    ingest(&mut store, vec![], "t1")?;
    assert!(
        !is_open(&store, id)?,
        "item should retire on first resolution"
    );
    assert_eq!(resolved_event_count(&store)?, 1);

    // The worktree is recreated at the same path and goes stale again — a
    // distinct occurrence, reflected as different summary content, the way a
    // real hygiene finding would carry a fresh detail each time it fires.
    ingest(
        &mut store,
        vec![attention_item(id, "Stale worktree detected again")],
        "t2",
    )?;
    assert!(is_open(&store, id)?, "item should reopen on reappearance");

    // The second resolution is the one that collided pre-fix: the old formula
    // hashed only (repo, id, "resolved"), identical to the first resolution's
    // identity, so this append was silently swallowed as a Duplicate and the
    // row stayed open forever.
    ingest(&mut store, vec![], "t3")?;
    assert!(
        !is_open(&store, id)?,
        "item must retire on its SECOND resolution too — this is the bug this fix closes"
    );
    assert_eq!(
        resolved_event_count(&store)?,
        2,
        "both resolutions must be durably persisted as distinct events"
    );

    Ok(())
}

/// AC2: an attention id that toggles N times is open exactly when its latest
/// event is appeared/changed, and retired exactly when it is resolved — at
/// every step, not just the first and last.
#[test]
fn a_repeatedly_toggling_attention_item_tracks_open_and_retired_state_at_every_step()
-> Result<(), ConsoleRuntimeError> {
    let mut store = SqliteEventStore::open_in_memory()?;
    let id = "hygiene:stale-worktree:/data/worktrees/toggler";

    for cycle in 0..4u32 {
        let appear_at = format!("t{}-appear", cycle * 2);
        ingest(
            &mut store,
            vec![attention_item(
                id,
                &format!("Stale worktree, cycle {cycle}"),
            )],
            &appear_at,
        )?;
        assert!(
            is_open(&store, id)?,
            "cycle {cycle}: item must be open right after appearing"
        );

        let resolve_at = format!("t{}-resolve", cycle * 2 + 1);
        ingest(&mut store, vec![], &resolve_at)?;
        assert!(
            !is_open(&store, id)?,
            "cycle {cycle}: item must be retired right after resolving"
        );
    }

    assert_eq!(
        resolved_event_count(&store)?,
        4,
        "every one of the 4 resolutions across the toggling fixture must land"
    );
    Ok(())
}

/// AC3: a resolved-event append the store rejects as Duplicate is surfaced as
/// a failure, not silently swallowed into a plain `Ok`.
#[test]
fn a_resolved_event_the_store_rejects_as_duplicate_is_surfaced_not_swallowed() -> Result<(), String>
{
    let mut store =
        SqliteEventStore::open_in_memory().map_err(|error| format!("open failed: {error:?}"))?;
    let id = "hygiene:stale-worktree:/data/worktrees/example";
    ingest(
        &mut store,
        vec![attention_item(id, "Stale worktree detected")],
        "t0",
    )
    .map_err(|error| format!("seeding appearance failed: {error:?}"))?;

    let mut duplicating = DuplicateOnResolveStore { inner: &mut store };
    let result = ingest(&mut duplicating, vec![], "t1");

    // Asserted against the rendered `Debug` text rather than a compile-time
    // pattern match on a specific `ConsoleRuntimeError` variant, so this test
    // is a genuine runtime Red/Green: it compiles unchanged before AND after
    // the fix, and only its assertion result flips once `ingest_needs_attention`
    // stops silently folding a rejected resolved-event append into `Ok`.
    let Err(error) = result else {
        return Err(format!(
            "expected an Err when the store rejects the resolved-event append as a duplicate, got {result:?}"
        ));
    };
    let rendered = format!("{error:?}");
    if !rendered.contains("AttentionResolveDuplicate") {
        return Err(format!(
            "expected an AttentionResolveDuplicate failure, got: {rendered}"
        ));
    }
    if !rendered.contains(REPO) {
        return Err(format!("diagnostic should name the repo: {rendered}"));
    }
    Ok(())
}

/// A store decorator that always reports a resolved-event append as
/// `Duplicate`, delegating everything else to the wrapped real store. Used to
/// prove `ingest_needs_attention` surfaces a rejected resolved append as a
/// failure rather than folding it into a silent `Ok`.
struct DuplicateOnResolveStore<'a> {
    inner: &'a mut SqliteEventStore,
}

impl FactoryCommandStore for DuplicateOnResolveStore<'_> {
    fn list_commands(&self) -> EventStoreResult<Vec<StoredCommand>> {
        self.inner.list_commands()
    }

    fn list_console_events(&self) -> EventStoreResult<Vec<ConsoleEvent>> {
        self.inner.list_console_events()
    }

    fn append_command(&mut self, append: &CommandAppend) -> EventStoreResult<CommandAppendOutcome> {
        self.inner.append_command(append)
    }

    fn append_event(&mut self, append: &EventAppend) -> EventStoreResult<AppendOutcome> {
        if append.event().event_type() == &EventType::AttentionItemResolved {
            return Ok(AppendOutcome::new(0, AppendStatus::Duplicate));
        }
        self.inner.append_event(append)
    }

    fn claim_command(&mut self, command_id: &str, claimed_at: &str) -> EventStoreResult<bool> {
        self.inner.claim_command(command_id, claimed_at)
    }

    fn update_command_status(
        &mut self,
        command_id: &str,
        status: &str,
        updated_at: &str,
        result_json: Option<&str>,
        error_json: Option<&str>,
    ) -> EventStoreResult<CommandStatusUpdateOutcome> {
        self.inner
            .update_command_status(command_id, status, updated_at, result_json, error_json)
    }

    fn finalize_executing_command_status(
        &mut self,
        command_id: &str,
        status: &str,
        updated_at: &str,
        result_json: Option<&str>,
        error_json: Option<&str>,
    ) -> EventStoreResult<CommandStatusUpdateOutcome> {
        self.inner.finalize_executing_command_status(
            command_id,
            status,
            updated_at,
            result_json,
            error_json,
        )
    }

    fn fail_stale_executing_commands(
        &mut self,
        stale_before: &str,
        recovered_at: &str,
        error_json: &str,
    ) -> EventStoreResult<usize> {
        self.inner
            .fail_stale_executing_commands(stale_before, recovered_at, error_json)
    }
}
