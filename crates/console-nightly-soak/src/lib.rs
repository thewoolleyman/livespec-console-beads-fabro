//! Nightly soak driver and idempotent chore filing.
//!
//! Drives the full fuzz soak and `cargo mutants` sweep, computes stable
//! finding signatures, and idempotently files chores for new findings through
//! the orchestrator capture surface — never failing master, never
//! double-filing.
//!
//! The [`LedgerPort`] trait is the seam: tests inject [`BeadsDouble`]; the
//! binary composition root wires the production beads/orchestrator impl.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Finding types
// ---------------------------------------------------------------------------

/// A stable, deterministic identity for a nightly quality finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindingSignature(String);

impl FindingSignature {
    /// The raw signature string, persisted on the filed work-item.
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
    /// Compute a stable signature for this finding.
    ///
    /// - Fuzz crash: SHA-256 hex of the reproducing input bytes.
    /// - Surviving mutant: `mutant:<source_file>:<line>:<mutation_operator>`.
    #[must_use]
    pub fn signature(&self) -> FindingSignature {
        match self {
            Self::FuzzCrash {
                reproducing_input, ..
            } => {
                let digest = Sha256::digest(reproducing_input);
                FindingSignature(format!("fuzz:{}", hex_encode(&digest)))
            }
            Self::SurvivingMutant {
                source_file,
                line,
                mutation_operator,
            } => FindingSignature(format!("mutant:{source_file}:{line}:{mutation_operator}")),
        }
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

// ---------------------------------------------------------------------------
// Ledger seam
// ---------------------------------------------------------------------------

/// A chore record returned from the ledger.
#[derive(Debug, Clone)]
pub struct LedgerChore {
    /// The chore's description text, which carries the persisted signature.
    pub description: String,
}

/// An error from ledger operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerError(
    /// The error message.
    pub String,
);

impl std::fmt::Display for LedgerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ledger error: {}", self.0)
    }
}

/// Seam for ledger access, enabling tests to inject a beads double.
pub trait LedgerPort {
    /// List all non-closed chores from the ledger.
    fn list_open_chores(&self) -> Result<Vec<LedgerChore>, LedgerError>;
    /// File a new top-of-rank chore through the orchestrator capture surface.
    fn file_chore(
        &self,
        title: &str,
        description: &str,
        signature: &str,
    ) -> Result<(), LedgerError>;
}

// ---------------------------------------------------------------------------
// Filing logic
// ---------------------------------------------------------------------------

/// Whether a finding was filed as a new chore or already had an open chore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilingResult {
    /// A new chore was filed for this finding.
    Filed {
        /// The finding's stable signature, now persisted on the chore.
        signature: FindingSignature,
    },
    /// An open chore already existed — nothing was filed (idempotent).
    AlreadyOpen {
        /// The signature that matched an existing open chore.
        signature: FindingSignature,
    },
}

/// Nightly soak filing logic: checks for an existing open chore carrying the
/// finding's stable signature and files a new top-of-rank chore only when
/// none exists.
pub struct NightlySoakFiler<'a> {
    ledger: &'a dyn LedgerPort,
}

impl<'a> NightlySoakFiler<'a> {
    /// Create a new filer backed by the given ledger.
    #[must_use]
    pub fn new(ledger: &'a dyn LedgerPort) -> Self {
        Self { ledger }
    }

    /// Process one finding: check for an existing open chore by signature and
    /// file a new top-of-rank chore only when none exists.
    pub fn process_finding(&self, kind: &FindingKind) -> Result<FilingResult, LedgerError> {
        let signature = kind.signature();
        let open_chores = self.ledger.list_open_chores()?;
        let already_open = open_chores
            .iter()
            .any(|chore| chore.description.contains(signature.as_str()));
        if already_open {
            return Ok(FilingResult::AlreadyOpen { signature });
        }
        let (title, description) = chore_fields(kind, &signature);
        self.ledger
            .file_chore(&title, &description, signature.as_str())?;
        Ok(FilingResult::Filed { signature })
    }
}

fn chore_fields(kind: &FindingKind, sig: &FindingSignature) -> (String, String) {
    let title = match kind {
        FindingKind::FuzzCrash { target, .. } => {
            format!("nightly: fuzz crash in target {target}")
        }
        FindingKind::SurvivingMutant {
            source_file,
            line,
            mutation_operator,
        } => {
            format!("nightly: surviving mutant in {source_file}:{line} ({mutation_operator})")
        }
    };
    let description = format!(
        "Nightly soak finding.\n\nnightly-soak finding signature: {}",
        sig.as_str()
    );
    (title, description)
}

// ---------------------------------------------------------------------------
// Test double — available for both unit tests and any future integration use
// ---------------------------------------------------------------------------

/// In-memory beads double implementing [`LedgerPort`].
///
/// Holds a fixed set of pre-existing open chores; records every
/// [`LedgerPort::file_chore`] call for inspection in tests.
pub struct BeadsDouble {
    open_chores: Vec<LedgerChore>,
    filed: std::cell::RefCell<Vec<(String, String, String)>>,
}

impl BeadsDouble {
    /// Create a new double with the given pre-existing open chores.
    #[must_use]
    pub const fn new(open_chores: Vec<LedgerChore>) -> Self {
        Self {
            open_chores,
            filed: std::cell::RefCell::new(Vec::new()),
        }
    }

    /// The number of chores filed through this double so far.
    #[must_use]
    pub fn filed_count(&self) -> usize {
        self.filed.borrow().len()
    }
}

impl LedgerPort for BeadsDouble {
    fn list_open_chores(&self) -> Result<Vec<LedgerChore>, LedgerError> {
        Ok(self.open_chores.clone())
    }

    fn file_chore(
        &self,
        title: &str,
        description: &str,
        signature: &str,
    ) -> Result<(), LedgerError> {
        self.filed.borrow_mut().push((
            title.to_owned(),
            description.to_owned(),
            signature.to_owned(),
        ));
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
            source_file: "src/lib.rs".to_owned(),
            line: 42,
            mutation_operator: "replace + with -".to_owned(),
        }
    }

    fn open_chore_for(kind: &FindingKind) -> LedgerChore {
        LedgerChore {
            description: format!(
                "nightly-soak finding signature: {}",
                kind.signature().as_str()
            ),
        }
    }

    // Maps a FilingResult to a string so assert_eq! can check the variant
    // without uncovered match arms. All three arms are exercised across tests.
    fn variant_name(r: &Result<FilingResult, LedgerError>) -> &'static str {
        match r {
            Ok(FilingResult::Filed { .. }) => "filed",
            Ok(FilingResult::AlreadyOpen { .. }) => "already_open",
            Err(_) => "error",
        }
    }

    // Error-injection doubles for covering the ? arms in process_finding.

    struct ListErrorLedger;
    impl LedgerPort for ListErrorLedger {
        fn list_open_chores(&self) -> Result<Vec<LedgerChore>, LedgerError> {
            Err(LedgerError("simulated list error".to_owned()))
        }
        fn file_chore(&self, _: &str, _: &str, _: &str) -> Result<(), LedgerError> {
            Ok(())
        }
    }

    struct FileErrorLedger;
    impl LedgerPort for FileErrorLedger {
        fn list_open_chores(&self) -> Result<Vec<LedgerChore>, LedgerError> {
            Ok(vec![])
        }
        fn file_chore(&self, _: &str, _: &str, _: &str) -> Result<(), LedgerError> {
            Err(LedgerError("simulated file error".to_owned()))
        }
    }

    // --- scenario (b): no open chore → files exactly one chore ---

    #[test]
    fn fuzz_finding_with_no_open_chore_files_exactly_one_chore() {
        let double = BeadsDouble::new(vec![]);
        let filer = NightlySoakFiler::new(&double);
        let result = filer.process_finding(&fuzz_finding());
        assert_eq!(variant_name(&result), "filed");
        assert_eq!(double.filed_count(), 1);
    }

    #[test]
    fn mutant_finding_with_no_open_chore_files_exactly_one_chore() {
        let double = BeadsDouble::new(vec![]);
        let filer = NightlySoakFiler::new(&double);
        let result = filer.process_finding(&mutant_finding());
        assert_eq!(variant_name(&result), "filed");
        assert_eq!(double.filed_count(), 1);
    }

    // --- scenario (c): open chore already exists → files nothing ---

    #[test]
    fn fuzz_finding_with_existing_open_chore_files_nothing() {
        let finding = fuzz_finding();
        let double = BeadsDouble::new(vec![open_chore_for(&finding)]);
        let filer = NightlySoakFiler::new(&double);
        let result = filer.process_finding(&finding);
        assert_eq!(variant_name(&result), "already_open");
        assert_eq!(double.filed_count(), 0);
    }

    #[test]
    fn mutant_finding_with_existing_open_chore_files_nothing() {
        let finding = mutant_finding();
        let double = BeadsDouble::new(vec![open_chore_for(&finding)]);
        let filer = NightlySoakFiler::new(&double);
        let result = filer.process_finding(&finding);
        assert_eq!(variant_name(&result), "already_open");
        assert_eq!(double.filed_count(), 0);
    }

    // --- ? error arms: list and file errors propagate ---

    #[test]
    fn list_chores_error_propagates() {
        let ledger = ListErrorLedger;
        let filer = NightlySoakFiler::new(&ledger);
        let result = filer.process_finding(&fuzz_finding());
        assert_eq!(variant_name(&result), "error");
    }

    #[test]
    fn file_chore_error_propagates() {
        let ledger = FileErrorLedger;
        let filer = NightlySoakFiler::new(&ledger);
        let result = filer.process_finding(&fuzz_finding());
        assert_eq!(variant_name(&result), "error");
    }

    // --- signature stability ---

    #[test]
    fn fuzz_signature_is_stable() {
        let finding = fuzz_finding();
        assert_eq!(finding.signature(), finding.signature());
    }

    #[test]
    fn mutant_signature_is_stable() {
        let finding = mutant_finding();
        assert_eq!(finding.signature(), finding.signature());
    }

    // --- LedgerError display coverage ---

    #[test]
    fn ledger_error_displays_message() {
        let error = LedgerError("something went wrong".to_owned());
        assert_eq!(error.to_string(), "ledger error: something went wrong");
    }

    // --- ListErrorLedger::file_chore stub coverage ---

    #[test]
    fn list_error_ledger_file_chore_stub_returns_ok() {
        assert!(ListErrorLedger.file_chore("t", "d", "s").is_ok());
    }
}
