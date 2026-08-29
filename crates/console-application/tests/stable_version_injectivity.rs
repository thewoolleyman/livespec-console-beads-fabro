use console_application::source_adapters::{
    AttentionHandoff, AttentionItemSnapshot, AttentionSourceRef, diff_needs_attention,
};

fn make_item(id: &str, summary: &str, source_repo: &str) -> AttentionItemSnapshot {
    AttentionItemSnapshot::new(
        id,
        "human-valve",
        "high",
        summary,
        AttentionSourceRef::new(source_repo, Some(id), None),
        AttentionHandoff::new("approve", Some("approve"), &format!("approve:{id}")),
    )
}

fn appeared_seq(repo: &str, item: AttentionItemSnapshot) -> u64 {
    let events = diff_needs_attention(repo, &[], &[item]);
    assert_eq!(events.len(), 1, "expected exactly one appeared event");
    events[0].event().stream_seq()
}

fn resolved_seq(repo: &str, item: AttentionItemSnapshot) -> u64 {
    let events = diff_needs_attention(repo, &[item], &[]);
    assert_eq!(events.len(), 1, "expected exactly one resolved event");
    events[0].event().stream_seq()
}

// Criteria 1 and 2b: parts 4 (summary) and 5 (source_ref.repo) in
// attention_item_version are adjacent; a trailing 0x1f in summary must NOT
// collide with a leading 0x1f in source_ref.repo.
// On master these two states share a version, so a real edit between them
// never reaches the operator.
#[test]
fn separator_shift_between_summary_and_source_repo_is_detected() {
    let trailing = appeared_seq("repo", make_item("wi-1", "Approve\x1f", "console"));
    let leading = appeared_seq("repo", make_item("wi-1", "Approve", "\x1fconsole"));
    assert_ne!(
        trailing, leading,
        "summary trailing 0x1f must not collide with source_repo leading 0x1f"
    );
}

// Criterion 2c: parts 0 (repo) and 1 (item.id()) in attention_item_resolved_event
// are adjacent; a trailing 0x1f in repo must NOT collide with a leading one in
// item.id().
#[test]
fn separator_shift_between_repo_and_id_in_resolved_event_is_detected() {
    let trailing = resolved_seq("a\x1f", make_item("b", "summary", "any-repo"));
    let leading = resolved_seq("a", make_item("\x1fb", "summary", "any-repo"));
    assert_ne!(
        trailing, leading,
        "repo trailing 0x1f must not collide with item id leading 0x1f in resolved event"
    );
}

// Criterion 3: identical inputs must always yield the same version (idempotence).
// Polling correctness depends on this; a fix that made every poll emit a fresh
// id would trade one bug for a worse one.
#[test]
fn identical_attention_item_yields_identical_version() {
    let item = make_item("wi-stable", "Pending approval", "console");
    let first = appeared_seq("repo", item.clone());
    let second = appeared_seq("repo", item);
    assert_eq!(
        first, second,
        "same input must always yield the same version"
    );
}

// Criterion 4: every version is non-zero with the low bit forced and fits a
// signed i64 (63 bits), so it round-trips through the event store's signed
// stream_seq column without overflow.
#[test]
fn attention_item_version_is_nonzero_odd_and_fits_signed_i64() {
    let version = appeared_seq("repo", make_item("wi-range", "summary", "source-repo"));
    assert_ne!(version, 0, "version must be non-zero");
    assert_eq!(
        version & 1,
        1,
        "version must have the low bit set (stable_version forces hash | 1)"
    );
    assert!(
        i64::try_from(version).is_ok(),
        "version must fit a signed i64 (source_stream_seq masks to 63 bits); got {version}"
    );
}
