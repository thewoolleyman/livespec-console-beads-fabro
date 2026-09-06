//! Ratified-fingerprint conformance for every finding this crate can emit.
//!
//! `SPECIFICATION/non-functional-requirements.md` §Quality Gate (as ratified in
//! v048) requires the nightly to derive a stable finding signature and to
//! RENDER it as a fingerprint matching `^[a-z0-9]([a-z0-9-]{0,62}[a-z0-9])?$` —
//! a DNS-label-like slug of at most 64 characters. The tailnet write ingress
//! rejects anything else, so a fingerprint that escapes the grammar is not a
//! cosmetic defect: the finding is silently never filed.
//!
//! The grammar therefore has to hold for EVERY finding the crate can emit, not
//! for the handful of well-behaved ones a soak usually yields — a mutant's
//! identity is a source path, a line, and a cargo-mutants operator string, none
//! of which the crate controls. These tests drive the adversarial end of that
//! input space: paths with spaces and non-ASCII, operator strings far longer
//! than the fingerprint budget, and line 0.

use console_nightly_soak::FindingKind;

/// The ratified grammar `^[a-z0-9]([a-z0-9-]{0,62}[a-z0-9])?$`, hand-rolled so
/// the conformance assertion carries no dependency of its own.
fn matches_ratified_fingerprint(candidate: &str) -> bool {
    const fn is_alnum(c: char) -> bool {
        c.is_ascii_digit() || c.is_ascii_lowercase()
    }
    let chars: Vec<char> = candidate.chars().collect();
    match chars.as_slice() {
        [] => false,
        [only] => is_alnum(*only),
        [first, middle @ .., last] => {
            is_alnum(*first)
                && is_alnum(*last)
                && middle.len() <= 62
                && middle.iter().copied().all(|c| is_alnum(c) || c == '-')
        }
    }
}

fn fuzz(target: &str, reproducing_input: &[u8]) -> String {
    FindingKind::FuzzCrash {
        target: target.to_owned(),
        reproducing_input: reproducing_input.to_vec(),
    }
    .fingerprint()
    .as_str()
    .to_owned()
}

fn mutant(source_file: &str, line: u32, mutation_operator: &str) -> String {
    FindingKind::SurvivingMutant {
        source_file: source_file.to_owned(),
        line,
        mutation_operator: mutation_operator.to_owned(),
    }
    .fingerprint()
    .as_str()
    .to_owned()
}

// --- the grammar holds for every emittable fingerprint ---

#[test]
fn the_matcher_rejects_the_forms_the_grammar_excludes() {
    // Guards the assertions below: a matcher that accepted everything would
    // make every conformance test in this file vacuous.
    assert!(!matches_ratified_fingerprint(""));
    assert!(!matches_ratified_fingerprint("-lead"));
    assert!(!matches_ratified_fingerprint("trail-"));
    assert!(!matches_ratified_fingerprint("has:colon"));
    assert!(!matches_ratified_fingerprint("has/slash"));
    assert!(!matches_ratified_fingerprint("has.dot"));
    assert!(!matches_ratified_fingerprint("Upper"));
    assert!(!matches_ratified_fingerprint(&"a".repeat(65)));
    assert!(matches_ratified_fingerprint("a"));
    assert!(matches_ratified_fingerprint("fuzz-0"));
    assert!(matches_ratified_fingerprint(&"a".repeat(64)));
}

#[test]
fn fuzz_crash_fingerprints_match_the_ratified_grammar() {
    let cases = [
        fuzz("event_envelope", b""),
        fuzz("event_envelope", b"\x00"),
        fuzz("adapter_normalization", b"crash input bytes"),
        fuzz("source_payload", &[0xff_u8; 4096]),
        // A target name is operator-visible text, not part of the identity —
        // assert the grammar survives one anyway.
        fuzz("target with spaces / and ünicode", b"payload"),
    ];
    for fingerprint in &cases {
        assert!(
            matches_ratified_fingerprint(fingerprint),
            "fuzz fingerprint escaped the ratified grammar: {fingerprint}"
        );
    }
}

#[test]
fn surviving_mutant_fingerprints_match_the_ratified_grammar() {
    let cases = [
        mutant("crates/console-domain/src/lib.rs", 42, "replace + with -"),
        // Line 0: cargo-mutants has emitted a zero line for a whole-file
        // mutation; the identity must still render.
        mutant("crates/console-domain/src/lib.rs", 0, "replace + with -"),
        mutant(
            "crates/console-domain/src/lib.rs",
            u32::MAX,
            "delete match arm",
        ),
        // Paths the crate does not control: spaces, non-ASCII, and dots.
        mutant(
            "crates/console tui/src/render view.rs",
            7,
            "replace with ()",
        ),
        mutant("crates/コンソール/src/lib.rs", 7, "replace ✓ with ✗"),
        mutant(
            &format!("crates/{}/src/lib.rs", "deep/".repeat(400)),
            9,
            "op",
        ),
        // An operator string far longer than the whole fingerprint budget.
        mutant(
            "crates/console-domain/src/lib.rs",
            11,
            &"replace ".repeat(1000),
        ),
        mutant("", 0, ""),
    ];
    for fingerprint in &cases {
        assert!(
            matches_ratified_fingerprint(fingerprint),
            "mutant fingerprint escaped the ratified grammar: {fingerprint}"
        );
    }
}

// --- stability: the same finding always renders the same fingerprint ---

#[test]
fn the_same_finding_renders_a_stable_fingerprint() {
    assert_eq!(
        fuzz("event_envelope", b"crash input bytes"),
        fuzz("event_envelope", b"crash input bytes")
    );
    assert_eq!(
        mutant("crates/console-domain/src/lib.rs", 42, "replace + with -"),
        mutant("crates/console-domain/src/lib.rs", 42, "replace + with -")
    );
}

#[test]
fn the_fuzz_fingerprint_ignores_everything_but_the_reproducing_input() {
    // The ratified derivation is a hash of the reproducing input, so the same
    // crash found through a different target dedups to one chore.
    assert_eq!(
        fuzz("event_envelope", b"crash input bytes"),
        fuzz("source_payload", b"crash input bytes")
    );
}

// --- distinctness: distinct findings render distinct fingerprints ---

#[test]
fn distinct_fuzz_inputs_render_distinct_fingerprints() {
    assert_ne!(fuzz("t", b"one"), fuzz("t", b"two"));
    assert_ne!(fuzz("t", b""), fuzz("t", b"\x00"));
}

#[test]
fn each_part_of_a_mutant_identity_changes_the_fingerprint() {
    let base = mutant("crates/console-domain/src/lib.rs", 42, "replace + with -");
    assert_ne!(
        base,
        mutant(
            "crates/console-application/src/lib.rs",
            42,
            "replace + with -"
        )
    );
    assert_ne!(
        base,
        mutant("crates/console-domain/src/lib.rs", 43, "replace + with -")
    );
    assert_ne!(
        base,
        mutant("crates/console-domain/src/lib.rs", 42, "replace - with +")
    );
}

#[test]
fn the_two_finding_kinds_never_collide() {
    // Distinct kinds carry distinct rendered prefixes, so a fuzz crash and a
    // mutant can never suppress one another host-side.
    assert_ne!(
        fuzz("event_envelope", b"crates/console-domain/src/lib.rs:42:op"),
        mutant("crates/console-domain/src/lib.rs", 42, "op")
    );
}
