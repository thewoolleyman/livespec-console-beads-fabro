//! The not-observed marker's CAUSE discriminator is a pinned, stable hash
//! (livespec-console-beads-fabro-mx9u.20.3).
//!
//! `cause_discriminator` is a hand-written FNV-1a rather than a
//! `DefaultHasher` call for one reason its own doc comment states: the value
//! is PERSISTED into a stored marker's identity, so a discriminator that
//! moved would re-mint every stored marker's id and make one unchanged outage
//! look like a fresh one.
//!
//! Nothing pinned its OUTPUT. Every existing gate asserted only that SOME id
//! was produced, which cargo-mutants measured exactly on 2026-09-12: replacing
//! the accumulate step `hash ^= u64::from(*byte)` with `hash |= ...` and with
//! `hash &= ...` both survived the whole suite. A hash nobody pins is not a
//! stable identity, it is an arbitrary one that happens not to have moved yet.
//!
//! These gates pin it from the outside, through the only surface that exposes
//! it: the identity `not_observed_event` mints, whose last `:`-separated field
//! IS the discriminator. Two claims, and the second is the one the operator
//! feels:
//!
//! 1. GOLDEN VALUES. For three reasons measured against the real store on
//!    2026-09-09, the exact 16-hex-digit discriminator the implementation
//!    produces: FNV-1a's shape (offset basis `0xcbf2_9ce4_8422_2325`,
//!    XOR-then-multiply per byte) over the multiplier the implementation
//!    actually carries, `0x1000_0000_01b3`. That multiplier is one hex digit
//!    WIDER than the canonical FNV-1a 64-bit prime `0x100_0000_01b3`, and
//!    these values pin it as it stands rather than correcting it, deliberately:
//!    the whole reason the constant is written out is that the output is
//!    PERSISTED, so narrowing it to the textbook prime would re-mint every
//!    stored marker's id -- the exact harm the function's doc comment names.
//!    Any change to the accumulate step changes these bytes.
//! 2. DISTINCTNESS. One source's SAME message with a different `os error`
//!    number must not collapse to one identity. This is the failure the
//!    discriminator exists to prevent -- the dedupe would swallow the second,
//!    current cause and keep rendering the first, stale one -- and it is
//!    precisely what `|=` does here: saturating toward all-ones, it maps
//!    `(os error 2)` and `(os error 5)` onto the same value, while `&=`
//!    collapses both to all-zeroes.

use console_application::source_adapters::{SourceAdapterKind, SourceInstance, not_observed_event};

/// The repo every marker below is minted for. Its name carries no `:`, so the
/// discriminator stays the last `:`-separated field of the identity.
const REPO: &str = "livespec-console-beads-fabro";

/// An arbitrary but fixed transition epoch: the discriminator is a function of
/// the REASON alone, so holding the epoch still keeps these values pinned to
/// the one thing under test.
const EPOCH: u64 = 7;

/// The discriminator segment of the not-observed marker `reason` mints: the
/// last `:`-separated field of the event id
/// `evt:<source>:<repo>:not_observed:<epoch>:<cause>`.
fn discriminator_for(reason: &str) -> String {
    let source = SourceInstance::sole(SourceAdapterKind::LiveSpec);
    let minted = not_observed_event(&source, REPO, reason, EPOCH);
    minted
        .event()
        .event_id()
        .rsplit_once(':')
        .map_or_else(String::new, |(_prefix, cause)| cause.to_owned())
}

/// The three causes measured against the real store on 2026-09-09, each with
/// the discriminator its reason hashes to.
///
/// Written out rather than computed: a derivation would restate whatever the
/// production code does, which is the exact reason the mutants survived. These
/// are the bytes the stored identities must keep carrying.
const GOLDEN: [(&str, &str); 3] = [
    (
        "livespec: No such file or directory (os error 2)",
        "f845725039682e41",
    ),
    ("source command exited non-zero", "6599fd67ebf5b726"),
    ("no work-item in journal entry", "7eca3d22a8c53c53"),
];

/// The discriminator is exactly the hash the implementation computes over the
/// reason, byte for byte.
#[test]
fn the_discriminator_is_the_pinned_hash_of_its_reason() {
    for (reason, expected) in GOLDEN {
        assert_eq!(
            discriminator_for(reason),
            expected,
            "the persisted discriminator for {reason:?} moved; every stored \
             marker minted under the old value now reads as a different outage"
        );
    }
}

/// The identity the store dedupes on carries the SAME discriminator the event
/// id does, so pinning one pins what is actually persisted.
#[test]
fn the_source_event_id_carries_the_same_discriminator() {
    let source = SourceInstance::sole(SourceAdapterKind::LiveSpec);
    for (reason, expected) in GOLDEN {
        let minted = not_observed_event(&source, REPO, reason, EPOCH);
        assert_eq!(
            minted.source_event_id(),
            format!("livespec:{REPO}:not_observed:{EPOCH}:{expected}"),
            "the deduped identity for {reason:?} must end in its own cause"
        );
    }
}

/// Two different `os error` numbers from ONE source are two different causes,
/// so they must mint two different identities.
///
/// If they collapsed, the second failure would dedupe away against the first
/// and the roster would keep rendering the stale cause as current -- the
/// defect this item's re-poll action exists to make impossible.
#[test]
fn one_source_reporting_two_different_os_errors_gets_two_identities() {
    let missing = discriminator_for("livespec: No such file or directory (os error 2)");
    let denied = discriminator_for("livespec: No such file or directory (os error 5)");

    assert_ne!(
        missing, denied,
        "two distinct causes collapsed to one identity, so the second would \
         dedupe away and the operator would keep reading the first"
    );
}

/// The discriminator is a fixed-width 16-hex-digit field, which is what lets
/// the identity be split on `:` at all.
#[test]
fn the_discriminator_is_sixteen_hex_digits() {
    for (reason, _expected) in GOLDEN {
        let cause = discriminator_for(reason);
        assert_eq!(cause.len(), 16, "{reason:?} minted {cause:?}");
        assert!(
            cause.chars().all(|character| character.is_ascii_hexdigit()),
            "{reason:?} minted a non-hex discriminator: {cause:?}"
        );
    }
}
