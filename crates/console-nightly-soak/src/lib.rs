//! Nightly soak driver and finding submission through the write ingress.
//!
//! Drives the full fuzz soak and `cargo mutants` sweep, derives a stable
//! identity for every finding, renders it as the ratified ingress fingerprint,
//! and submits the finding through the single-verb tailnet write ingress —
//! never failing master.
//!
//! Per `SPECIFICATION/non-functional-requirements.md` §Quality Gate (as
//! ratified in v048), dedup is performed HOST-SIDE by the ingress: it suppresses
//! a filing when it observes an open chore already carrying the fingerprint. So
//! this client sends `create` and NOTHING else — it never reads the work-items
//! ledger, holds no work-items database credential, and cannot distinguish a
//! filing from a suppression. There is deliberately no client-side "already
//! open" answer to report, because the client has no way to know.
//!
//! The [`FindingIngress`] trait is the seam: tests inject [`RecordingIngress`];
//! the binary composition root wires the production transport.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Finding types
// ---------------------------------------------------------------------------

/// A finding's stable identity, rendered as the ingress fingerprint.
///
/// The rendered form always matches the ratified grammar
/// `^[a-z0-9]([a-z0-9-]{0,62}[a-z0-9])?$` — a DNS-label-like slug of at most
/// 64 characters. The ingress rejects anything else, so the rendering is a
/// conformance obligation, not a presentation choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint(String);

impl Fingerprint {
    /// The rendered fingerprint, sent on the `create` request and persisted on
    /// the filed work-item as the dedup key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A single nightly finding: either a fuzz crash or a surviving mutant.
#[derive(Debug, Clone)]
pub enum FindingKind {
    /// A libFuzzer crash with its reproducing input bytes.
    FuzzCrash {
        /// The fuzz target name.
        target: String,
        /// The reproducing input bytes (content of the crash artifact file).
        reproducing_input: Vec<u8>,
    },
    /// A surviving mutant not on the justified-survivor allow-list.
    SurvivingMutant {
        /// The source file that was mutated.
        source_file: String,
        /// The line number of the mutation.
        line: u32,
        /// The mutation operator name (from cargo-mutants output).
        mutation_operator: String,
    },
}

impl FindingKind {
    /// Render this finding's stable signature as the ingress fingerprint.
    ///
    /// The ratified derivation is unchanged: a fuzz crash is identified by its
    /// reproducing input, a surviving mutant by its
    /// `(source file, line, mutation-operator)` identity. What differs is the
    /// RENDERING, which must survive the ratified grammar:
    ///
    /// - Fuzz crash: `fuzz-<first 32 hex of SHA-256(reproducing input)>`.
    /// - Surviving mutant:
    ///   `mutant-<first 32 hex of SHA-256("<file>:<line>:<operator>")>`.
    ///
    /// The mutant identity is HASHED rather than sanitized in place because it
    /// is caller-supplied text the crate does not control — a source path
    /// carries slashes and dots, an operator string carries spaces and
    /// punctuation, and either can exceed the 64-character budget on its own.
    /// Hashing makes the rendering total: every identity, however hostile,
    /// renders inside the grammar. Half a SHA-256 (128 bits) leaves collision
    /// risk far below the rate at which a soak invents findings.
    #[must_use]
    pub fn fingerprint(&self) -> Fingerprint {
        match self {
            Self::FuzzCrash {
                reproducing_input, ..
            } => {
                let digest = Sha256::digest(reproducing_input);
                Fingerprint(format!("fuzz-{}", short_hex(&digest)))
            }
            Self::SurvivingMutant {
                source_file,
                line,
                mutation_operator,
            } => {
                let identity = format!("{source_file}:{line}:{mutation_operator}");
                let digest = Sha256::digest(identity.as_bytes());
                Fingerprint(format!("mutant-{}", short_hex(&digest)))
            }
        }
    }
}

/// Lowercase hex of the first 16 bytes of `bytes` — 32 characters, which keeps
/// both rendered prefixes well inside the 64-character fingerprint budget.
fn short_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .take(16)
        .fold(String::with_capacity(32), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

// ---------------------------------------------------------------------------
// Ingress seam
// ---------------------------------------------------------------------------

/// An error from the write ingress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngressError(
    /// The error message.
    pub String,
);

impl std::fmt::Display for IngressError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ingress error: {}", self.0)
    }
}

/// The single-verb seam onto the tailnet beads write ingress.
///
/// The ingress accepts exactly one request —
/// `create <fingerprint> <base64url-title> [<base64url-body>]` — and performs
/// the filing, the labelling, and the open-chore suppression host-side under
/// its own pinned `bd`. No lifecycle status, rank, or priority can be
/// expressed through it, and there is no read verb: a client cannot ask
/// whether a chore already exists, which is why this trait has one method and
/// returns no filing outcome.
pub trait FindingIngress {
    /// Submit one `create` request for a finding.
    ///
    /// Returning `Ok(())` means the ingress ACCEPTED the request, not that a
    /// work-item was created — an accepted request whose fingerprint already
    /// has an open chore is suppressed host-side, indistinguishably.
    fn create(
        &self,
        fingerprint: &str,
        title: &str,
        body: Option<&str>,
    ) -> Result<(), IngressError>;
}

// ---------------------------------------------------------------------------
// Ingress request encoding
// ---------------------------------------------------------------------------

/// Maximum decoded byte length of a title on a `create` request.
///
/// The ingress request travels as one SSH remote command, so an unbounded
/// title would push an unbounded argument at the forced command. A title is a
/// one-line summary; anything past this is surplus, and the identity it
/// summarises survives in full in the body.
pub const INGRESS_TITLE_MAX_BYTES: usize = 200;

/// Maximum decoded byte length of a body on a `create` request.
///
/// Generous enough to carry every field [`FindingKind`] renders, bounded so a
/// hostile mutation-operator string cannot grow the request without limit.
pub const INGRESS_BODY_MAX_BYTES: usize = 4000;

/// The RFC 4648 §5 (URL and filename safe) alphabet.
const BASE64URL_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Render the single line a `create` request sends to the write ingress.
///
/// The ratified request shape is
/// `create <fingerprint> <base64url-title> [<base64url-body>]` — one line,
/// space-separated, with the body omitted entirely when there is none. The
/// fingerprint travels verbatim because it already matches the ratified
/// DNS-label-like grammar; the title and body are base64url (RFC 4648 §5,
/// padding stripped) because they are free text that would otherwise carry
/// spaces the forced command splits on.
///
/// Both free-text fields are made control-character-free and length-bounded
/// BEFORE encoding, so the values the ingress decodes are bounded and printable
/// rather than merely wire-safe: base64 would happily smuggle a NUL or an
/// escape sequence through to `bd create` intact.
#[must_use]
pub fn ingress_request_line(fingerprint: &str, title: &str, body: Option<&str>) -> String {
    let mut line = format!(
        "create {fingerprint} {}",
        encode_field(title, INGRESS_TITLE_MAX_BYTES, false)
    );
    if let Some(body) = body {
        line.push(' ');
        line.push_str(&encode_field(body, INGRESS_BODY_MAX_BYTES, true));
    }
    line
}

/// Sanitize, bound, and base64url-encode one free-text request field.
fn encode_field(text: &str, max_bytes: usize, keep_newlines: bool) -> String {
    let printable = control_free(text, keep_newlines);
    base64url_no_pad(bounded(&printable, max_bytes).as_bytes())
}

/// Replace every control character with a space.
///
/// A title is a single line, so it keeps none. A body keeps `\n`: the paragraph
/// structure [`chore_fields`] renders is the whole point of the body, and a
/// newline inside a base64url-encoded field cannot break the one-line request
/// shape. Everything else — NUL, ESC, DEL, the C1 range — is replaced rather
/// than dropped, so a truncation point stays a byte offset into readable text.
fn control_free(text: &str, keep_newlines: bool) -> String {
    text.chars()
        .map(|ch| {
            if ch.is_control() && !(keep_newlines && ch == '\n') {
                ' '
            } else {
                ch
            }
        })
        .collect()
}

/// Truncate to at most `max_bytes`, backing up to the nearest character
/// boundary so the result is still valid UTF-8 for the ingress to decode.
fn bounded(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.get(..end).unwrap_or("")
}

/// Base64url (RFC 4648 §5) with the `=` padding stripped, as the ingress
/// request grammar specifies.
fn base64url_no_pad(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk.first().copied().unwrap_or(0);
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        push_symbol(&mut encoded, first >> 2);
        push_symbol(&mut encoded, ((first & 0b0000_0011) << 4) | (second >> 4));
        if chunk.len() > 1 {
            push_symbol(&mut encoded, ((second & 0b0000_1111) << 2) | (third >> 6));
        }
        if chunk.len() > 2 {
            push_symbol(&mut encoded, third & 0b0011_1111);
        }
    }
    encoded
}

/// Append the alphabet symbol for one six-bit group.
fn push_symbol(encoded: &mut String, six_bits: u8) {
    let symbol = BASE64URL_ALPHABET
        .get(usize::from(six_bits))
        .copied()
        .unwrap_or(b'A');
    encoded.push(char::from(symbol));
}

// ---------------------------------------------------------------------------
// Filing logic
// ---------------------------------------------------------------------------

/// Nightly soak filing logic: renders each finding's fingerprint and submits it
/// to the write ingress.
pub struct NightlySoakFiler<'a> {
    ingress: &'a dyn FindingIngress,
}

impl<'a> NightlySoakFiler<'a> {
    /// Create a new filer backed by the given ingress.
    #[must_use]
    pub const fn new(ingress: &'a dyn FindingIngress) -> Self {
        Self { ingress }
    }

    /// Submit one finding to the ingress, returning the fingerprint sent.
    ///
    /// The fingerprint is the whole of the client's dedup responsibility: it is
    /// stable across runs, so two runs that rediscover one finding send one
    /// key, and the ingress decides whether that key needs a new chore.
    pub fn process_finding(&self, kind: &FindingKind) -> Result<Fingerprint, IngressError> {
        let fingerprint = kind.fingerprint();
        let (title, body) = chore_fields(kind);
        self.ingress
            .create(fingerprint.as_str(), &title, Some(&body))?;
        Ok(fingerprint)
    }
}

/// The title and body of the chore a finding files.
///
/// The body carries the raw identity in full, because the fingerprint hashes it
/// away: without this, a filed chore names a finding nobody can locate.
fn chore_fields(kind: &FindingKind) -> (String, String) {
    match kind {
        FindingKind::FuzzCrash { target, .. } => (
            format!("nightly: fuzz crash in target {target}"),
            format!(
                "Nightly soak finding: fuzz crash.\n\n\
                 fuzz target: {target}\n\n\
                 The fingerprint is the SHA-256 of the reproducing input; the \
                 input itself is committed to the target's regression corpus."
            ),
        ),
        FindingKind::SurvivingMutant {
            source_file,
            line,
            mutation_operator,
        } => (
            format!("nightly: surviving mutant in {source_file}:{line} ({mutation_operator})"),
            format!(
                "Nightly soak finding: surviving mutant.\n\n\
                 source file: {source_file}\n\
                 line: {line}\n\
                 mutation operator: {mutation_operator}\n\n\
                 The fingerprint is the SHA-256 of that three-part identity."
            ),
        ),
    }
}

// ---------------------------------------------------------------------------
// Test double — available for both unit tests and any future integration use
// ---------------------------------------------------------------------------

/// One `create` request as recorded by [`RecordingIngress`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateRequest {
    /// The rendered fingerprint the request carried.
    pub fingerprint: String,
    /// The chore title.
    pub title: String,
    /// The optional chore body.
    pub body: Option<String>,
}

/// In-memory [`FindingIngress`] double that accepts every request and records
/// it for inspection.
///
/// It cannot model suppression, and deliberately does not try to: suppression
/// happens inside the real ingress, past this seam.
#[derive(Debug, Default)]
pub struct RecordingIngress {
    created: std::cell::RefCell<Vec<CreateRequest>>,
}

impl RecordingIngress {
    /// Every request submitted through this double, in submission order.
    #[must_use]
    pub fn created(&self) -> Vec<CreateRequest> {
        self.created.borrow().clone()
    }
}

impl FindingIngress for RecordingIngress {
    fn create(
        &self,
        fingerprint: &str,
        title: &str,
        body: Option<&str>,
    ) -> Result<(), IngressError> {
        self.created.borrow_mut().push(CreateRequest {
            fingerprint: fingerprint.to_owned(),
            title: title.to_owned(),
            body: body.map(str::to_owned),
        });
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Unit tests (coverage gate)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn fuzz_finding() -> FindingKind {
        FindingKind::FuzzCrash {
            target: "event_envelope".to_owned(),
            reproducing_input: b"crash input bytes".to_vec(),
        }
    }

    fn mutant_finding() -> FindingKind {
        FindingKind::SurvivingMutant {
            source_file: "crates/console-domain/src/lib.rs".to_owned(),
            line: 42,
            mutation_operator: "replace + with -".to_owned(),
        }
    }

    // Maps a submission outcome to a string so assert_eq! can check the variant
    // without uncovered match arms. Both arms are exercised across tests.
    fn variant_name(result: &Result<Fingerprint, IngressError>) -> &'static str {
        match result {
            Ok(_) => "submitted",
            Err(_) => "error",
        }
    }

    // Error-injection double covering the ? arm in process_finding.
    struct FailingIngress;
    impl FindingIngress for FailingIngress {
        fn create(&self, _: &str, _: &str, _: Option<&str>) -> Result<(), IngressError> {
            Err(IngressError("simulated transport failure".to_owned()))
        }
    }

    // --- every finding is submitted through the single create verb ---

    #[test]
    fn a_fuzz_finding_is_submitted_as_exactly_one_create_request() {
        let ingress = RecordingIngress::default();
        let filer = NightlySoakFiler::new(&ingress);
        let finding = fuzz_finding();

        let result = filer.process_finding(&finding);

        assert_eq!(variant_name(&result), "submitted");
        let created = ingress.created();
        assert_eq!(created.len(), 1);
        assert_eq!(
            created[0],
            CreateRequest {
                fingerprint: finding.fingerprint().as_str().to_owned(),
                title: "nightly: fuzz crash in target event_envelope".to_owned(),
                body: created[0].body.clone(),
            }
        );
    }

    #[test]
    fn a_mutant_finding_is_submitted_as_exactly_one_create_request() {
        let ingress = RecordingIngress::default();
        let filer = NightlySoakFiler::new(&ingress);
        let finding = mutant_finding();

        let result = filer.process_finding(&finding);

        assert_eq!(variant_name(&result), "submitted");
        let created = ingress.created();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].fingerprint, finding.fingerprint().as_str());
        assert_eq!(
            created[0].title,
            "nightly: surviving mutant in crates/console-domain/src/lib.rs:42 (replace + with -)"
        );
    }

    #[test]
    fn rediscovering_a_finding_resubmits_the_same_fingerprint() {
        // The client cannot suppress — the ingress does — so a second run of the
        // same finding submits again, carrying the identical dedup key.
        let ingress = RecordingIngress::default();
        let filer = NightlySoakFiler::new(&ingress);

        let first = filer.process_finding(&fuzz_finding());
        let second = filer.process_finding(&fuzz_finding());

        assert_eq!(variant_name(&first), "submitted");
        assert_eq!(variant_name(&second), "submitted");
        let created = ingress.created();
        assert_eq!(created.len(), 2);
        assert_eq!(created[0].fingerprint, created[1].fingerprint);
    }

    // --- the body carries the identity the fingerprint hashes away ---

    #[test]
    fn a_fuzz_body_names_the_target() {
        let ingress = RecordingIngress::default();
        let filer = NightlySoakFiler::new(&ingress);

        let result = filer.process_finding(&fuzz_finding());

        assert_eq!(variant_name(&result), "submitted");
        let created = ingress.created();
        let body = created[0].body.clone().unwrap_or_default();
        assert!(body.contains("fuzz target: event_envelope"));
    }

    #[test]
    fn a_mutant_body_names_the_file_line_and_operator() {
        let ingress = RecordingIngress::default();
        let filer = NightlySoakFiler::new(&ingress);

        let result = filer.process_finding(&mutant_finding());

        assert_eq!(variant_name(&result), "submitted");
        let created = ingress.created();
        let body = created[0].body.clone().unwrap_or_default();
        assert!(body.contains("source file: crates/console-domain/src/lib.rs"));
        assert!(body.contains("line: 42"));
        assert!(body.contains("mutation operator: replace + with -"));
    }

    // --- ? error arm: an ingress failure propagates ---

    #[test]
    fn an_ingress_failure_propagates() {
        let ingress = FailingIngress;
        let filer = NightlySoakFiler::new(&ingress);

        let result = filer.process_finding(&fuzz_finding());

        assert_eq!(variant_name(&result), "error");
    }

    // --- fingerprint rendering ---

    #[test]
    fn a_fuzz_fingerprint_renders_the_hashed_input_under_the_fuzz_prefix() {
        let fingerprint = fuzz_finding().fingerprint();

        // SHA-256("crash input bytes"), first 16 bytes.
        assert_eq!(
            fingerprint.as_str(),
            "fuzz-c2619dea7ad540634263a2ef3c632f08"
        );
    }

    #[test]
    fn a_mutant_fingerprint_renders_the_hashed_identity_under_the_mutant_prefix() {
        let fingerprint = mutant_finding().fingerprint();

        // SHA-256("crates/console-domain/src/lib.rs:42:replace + with -"),
        // first 16 bytes.
        assert_eq!(
            fingerprint.as_str(),
            "mutant-7367e080768a2733ac64929a669ded1a"
        );
    }

    #[test]
    fn a_fingerprint_is_stable_across_calls() {
        let finding = mutant_finding();

        assert_eq!(finding.fingerprint(), finding.fingerprint());
    }

    #[test]
    fn short_hex_renders_sixteen_bytes_as_lowercase_hex() {
        assert_eq!(short_hex(&[0x00, 0x0f, 0xff]), "000fff");
        assert_eq!(short_hex(&[0xab_u8; 20]), "ab".repeat(16));
    }

    // --- the single-line create request the ingress receives ---

    #[test]
    fn a_request_line_carries_the_verb_the_fingerprint_and_both_encoded_fields() {
        let line = ingress_request_line("fuzz-abc123", "hello", Some("hi"));

        // base64url("hello") = aGVsbG8, base64url("hi") = aGk (padding stripped).
        assert_eq!(line, "create fuzz-abc123 aGVsbG8 aGk");
    }

    #[test]
    fn a_request_line_omits_the_body_field_entirely_when_there_is_none() {
        let line = ingress_request_line("fuzz-abc123", "hello", None);

        assert_eq!(line, "create fuzz-abc123 aGVsbG8");
    }

    #[test]
    fn a_request_line_has_no_field_that_could_split_the_forced_command() {
        let line = ingress_request_line(
            &fuzz_finding().fingerprint().0,
            "a title with spaces\tand a tab",
            Some("a body\nwith a newline"),
        );

        // Verb, fingerprint, title, body — and nothing the ingress could read
        // as a fifth argument.
        assert_eq!(line.split(' ').count(), 4);
        assert!(!line.contains(['\n', '\t']));
    }

    #[test]
    fn a_control_character_never_reaches_the_decoded_title_or_body() {
        assert_eq!(control_free("a\u{0}b\u{1b}c\td", false), "a b c d");
        assert_eq!(control_free("a\nb", false), "a b");
        // A body keeps its paragraph structure; everything else still goes.
        assert_eq!(control_free("a\nb\u{7f}c", true), "a\nb c");
    }

    #[test]
    fn an_oversized_field_is_bounded_before_it_is_encoded() {
        let short = "under the bound";
        assert_eq!(bounded(short, INGRESS_TITLE_MAX_BYTES), short);

        let long = "x".repeat(INGRESS_TITLE_MAX_BYTES + 50);
        assert_eq!(bounded(&long, INGRESS_TITLE_MAX_BYTES).len(), 200);

        let hostile = FindingKind::SurvivingMutant {
            source_file: "s".repeat(500),
            line: 0,
            mutation_operator: "o".repeat(500),
        };
        let (title, body) = chore_fields(&hostile);
        let line = ingress_request_line(hostile.fingerprint().as_str(), &title, Some(&body));
        // Both fields bounded and base64-expanded by 4/3, plus verb and
        // fingerprint — far short of the 1000-character identity fed in.
        assert!(line.len() < 6000);
    }

    #[test]
    fn bounding_backs_up_to_a_character_boundary_rather_than_splitting_a_char() {
        // "é" is two bytes, so a two-byte bound must not cut it in half.
        assert_eq!(bounded("aé", 2), "a");
        // Backing up past every candidate boundary yields the empty prefix.
        assert_eq!(bounded("é", 1), "");
    }

    #[test]
    fn base64url_encodes_every_chunk_remainder_and_uses_the_url_safe_alphabet() {
        assert_eq!(base64url_no_pad(b""), "");
        assert_eq!(base64url_no_pad(b"h"), "aA");
        assert_eq!(base64url_no_pad(b"hi"), "aGk");
        assert_eq!(base64url_no_pad(b"hello"), "aGVsbG8");
        // The two symbols that separate base64url from standard base64.
        assert_eq!(base64url_no_pad(&[0xfb, 0xff, 0xfe]), "-__-");
    }

    // --- IngressError display coverage ---

    #[test]
    fn an_ingress_error_displays_its_message() {
        let error = IngressError("something went wrong".to_owned());

        assert_eq!(error.to_string(), "ingress error: something went wrong");
    }
}
