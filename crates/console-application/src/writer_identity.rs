//! The identity of the process that wrote a source availability marker.
//!
//! `livespec-console-beads-fabro-mx9u.23`: a second console process wrote to
//! the same store as a first one for roughly eleven hours, and nothing in
//! the store could name either. Dedupe on `source_event_id` proved it was
//! genuinely TWO writers with DIFFERENT behaviour (one could resolve a
//! source, the other could not) -- but which two processes, and what
//! stopped one of them, were unknowable from the store as it stood.
//!
//! This module is that gap's model layer: a [`WriterIdentity`] naming the
//! OS process, the binary, and the commit that wrote a given marker, plus
//! the JSON encode/decode for the marker's `metadata_json` column it lives
//! in. It carries NO logic for reading the running process's own pid, exe
//! path, or cwd -- that is host IO and belongs at the composition root
//! (`console-cli`'s binary, mirroring where `build_identity`'s `git`/`date`
//! shells already live), which hands a fully-formed value in here.

use std::collections::BTreeMap;

/// The identity of the process that wrote (or is writing) a source
/// availability marker: which OS process, which binary, from where, and
/// which commit it was built from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriterIdentity {
    pid: u32,
    exe_path: String,
    cwd: String,
    build_sha: String,
}

impl WriterIdentity {
    #[must_use]
    /// Construct a new value from its required fields.
    pub fn new(
        pid: u32,
        exe_path: impl Into<String>,
        cwd: impl Into<String>,
        build_sha: impl Into<String>,
    ) -> Self {
        Self {
            pid,
            exe_path: exe_path.into(),
            cwd: cwd.into(),
            build_sha: build_sha.into(),
        }
    }

    #[must_use]
    /// Return the OS process id.
    pub const fn pid(&self) -> u32 {
        self.pid
    }

    #[must_use]
    /// Return the executable path.
    pub fn exe_path(&self) -> &str {
        &self.exe_path
    }

    #[must_use]
    /// Return the current working directory.
    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    #[must_use]
    /// Return the build sha.
    pub fn build_sha(&self) -> &str {
        &self.build_sha
    }

    #[must_use]
    /// A sentinel identity for call sites that predate this instrumentation
    /// or genuinely have no process identity to stamp (most test fixtures).
    /// Never used by the real ingestion path -- see `console-cli`'s
    /// composition root for the identity built from the actual running
    /// process.
    pub fn unknown() -> Self {
        Self::new(0, "unknown", "unknown", "unknown")
    }

    #[must_use]
    /// The JSON object stamped onto an availability marker's `metadata_json`
    /// column: `{"writer_pid": ..., "writer_exe_path": ..., "writer_cwd":
    /// ..., "writer_build_sha": ...}`. `writer_`-prefixed keys so a future
    /// metadata field added for another reason never collides with these.
    pub fn to_metadata_json(&self) -> String {
        serde_json::json!({
            "writer_pid": self.pid,
            "writer_exe_path": self.exe_path,
            "writer_cwd": self.cwd,
            "writer_build_sha": self.build_sha,
        })
        .to_string()
    }

    #[must_use]
    /// Parse a writer identity back out of a stored marker's `metadata_json`.
    ///
    /// `None` for anything that is not a fully-stamped identity: malformed
    /// JSON, a bare `{}` (a marker this instrumentation never touched), or a
    /// marker missing one of the four fields (a future schema change, or a
    /// legacy row) -- doctor's multi-writer check (AC2) must never attribute
    /// a marker it cannot fully identify to some guessed writer.
    pub fn from_metadata_json(metadata_json: &str) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_str(metadata_json).ok()?;
        let object = value.as_object()?;
        let pid = object.get("writer_pid")?.as_u64()?;
        let exe_path = object.get("writer_exe_path")?.as_str()?.to_owned();
        let cwd = object.get("writer_cwd")?.as_str()?.to_owned();
        let build_sha = object.get("writer_build_sha")?.as_str()?.to_owned();
        Some(Self::new(
            u32::try_from(pid).ok()?,
            exe_path,
            cwd,
            build_sha,
        ))
    }

    #[must_use]
    /// The key AC2's multi-writer doctor finding groups markers by: `(build
    /// sha, exe path)`. Deliberately NOT `pid` -- an ordinary restart of the
    /// SAME binary (the routine case CLAUDE.md documents: rebuild and
    /// recreate the TUI pane on every session restart) mints a new pid but
    /// is not a second writer worth flagging. Two DIFFERENT builds (or the
    /// same build run from two different paths) are.
    pub fn writer_key(&self) -> (String, String) {
        (self.build_sha.clone(), self.exe_path.clone())
    }
}

/// Render a stable, human-readable label for one writer, for a doctor
/// finding: `build <sha> at <exe path>`.
#[must_use]
pub fn writer_label(build_sha: &str, exe_path: &str) -> String {
    format!("build {build_sha} at {exe_path}")
}

/// Count how many of `identities` fall under each `writer_key`, in
/// first-seen order -- used by doctor to report "N markers" per distinct
/// writer without caring about the identities' relative timing.
#[must_use]
pub fn count_by_writer_key(identities: &[WriterIdentity]) -> Vec<((String, String), usize)> {
    let mut order: Vec<(String, String)> = Vec::new();
    let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    for identity in identities {
        let key = identity.writer_key();
        if !counts.contains_key(&key) {
            order.push(key.clone());
        }
        *counts.entry(key).or_insert(0) += 1;
    }
    order
        .into_iter()
        .map(|key| {
            let count = counts[&key];
            (key, count)
        })
        .collect()
}

/// Whether THIS process currently holds its store's writer lease.
///
/// `livespec-console-beads-fabro-mx9u.23` AC3: a process that does NOT hold
/// the lease has already degraded to read-only in `refresh_sources` (no
/// availability markers, no ingest events); this is the header's half of
/// that fix, so the operator is TOLD rather than left to infer it from an
/// event log that has simply stopped moving.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriterLeaseStatus {
    /// This process holds the lease and is free to write.
    Writable,
    /// Another live process holds the lease. This process is a read-only
    /// OBSERVER of the store: it writes nothing until the other writer's
    /// lease goes stale or it releases the store itself.
    ReadOnly(WriterIdentity),
}

impl WriterLeaseStatus {
    #[must_use]
    /// Whether this status permits writing.
    pub const fn is_writable(&self) -> bool {
        matches!(self, Self::Writable)
    }
}

/// The header's read-only-observer tell.
///
/// Present ONLY while [`WriterLeaseStatus::ReadOnly`], naming the writer that
/// holds the store instead -- absent for [`WriterLeaseStatus::Writable`], the
/// unremarkable and overwhelmingly common case, which needs no chrome. This
/// is the pure text; the application layer's header assembly is responsible
/// for the `" | "` join and where in display order it lands.
#[must_use]
pub fn writer_lease_status_segment(status: &WriterLeaseStatus) -> Option<String> {
    match status {
        WriterLeaseStatus::ReadOnly(holder) => Some(format!(
            "READ-ONLY: store owned by {}",
            writer_label(holder.build_sha(), holder.exe_path())
        )),
        WriterLeaseStatus::Writable => None,
    }
}

/// A thread-shared cell holding the most recently observed
/// [`WriterLeaseStatus`].
///
/// Mirrors `console_application::build_identity::SharedBuildStaleness`
/// exactly: the background poller thread is the only place the store's
/// writer lease is actually contended for (`refresh_sources`), so it is also
/// the only place that can know the CURRENT answer; the render thread just
/// reads whatever was written last, through a `Mutex` lock around a `Clone`
/// value it releases immediately.
#[derive(Clone, Debug)]
pub struct SharedWriterLeaseStatus(std::sync::Arc<std::sync::Mutex<WriterLeaseStatus>>);

impl SharedWriterLeaseStatus {
    #[must_use]
    /// Construct a new cell, starting `Writable` -- the ordinary case, and
    /// the only sane default before the first poll has even run.
    pub fn new() -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(
            WriterLeaseStatus::Writable,
        )))
    }

    /// Overwrite the shared value with a freshly observed status. See
    /// [`crate::build_identity::SharedBuildStaleness::set`] for the poisoned-lock
    /// handling this mirrors.
    pub fn set(&self, status: WriterLeaseStatus) {
        if let Ok(mut guard) = self.0.lock() {
            *guard = status;
        }
    }

    #[must_use]
    /// Read the latest shared value. Degrades to `Writable` on a poisoned
    /// lock, matching this module's silence-on-failure contract.
    pub fn get(&self) -> WriterLeaseStatus {
        self.0
            .lock()
            .map_or(WriterLeaseStatus::Writable, |guard| guard.clone())
    }
}

impl Default for SharedWriterLeaseStatus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{WriterIdentity, count_by_writer_key, writer_label};

    fn identity(pid: u32, exe_path: &str, build_sha: &str) -> WriterIdentity {
        WriterIdentity::new(pid, exe_path, "/data/projects/repo", build_sha)
    }

    #[test]
    fn exposes_every_field() {
        let value = WriterIdentity::new(
            4321,
            "/opt/console/bin/console",
            "/data/projects/repo",
            "abc1234",
        );

        assert_eq!(value.pid(), 4321);
        assert_eq!(value.exe_path(), "/opt/console/bin/console");
        assert_eq!(value.cwd(), "/data/projects/repo");
        assert_eq!(value.build_sha(), "abc1234");
    }

    #[test]
    fn round_trips_through_metadata_json() {
        let original = identity(4321, "/opt/console/bin/console", "abc1234");

        let encoded = original.to_metadata_json();
        let decoded = WriterIdentity::from_metadata_json(&encoded);

        assert_eq!(decoded, Some(original));
    }

    #[test]
    fn metadata_json_names_every_field() {
        let identity = WriterIdentity::new(42, "/opt/console", "/home/op", "deadbee");

        let json = identity.to_metadata_json();

        assert!(json.contains(r#""writer_pid":42"#));
        assert!(json.contains(r#""writer_exe_path":"/opt/console""#));
        assert!(json.contains(r#""writer_cwd":"/home/op""#));
        assert!(json.contains(r#""writer_build_sha":"deadbee""#));
    }

    #[test]
    fn an_empty_metadata_object_carries_no_identity() {
        assert_eq!(WriterIdentity::from_metadata_json("{}"), None);
    }

    #[test]
    fn malformed_json_carries_no_identity() {
        assert_eq!(WriterIdentity::from_metadata_json("not json"), None);
    }

    #[test]
    fn a_metadata_object_missing_a_field_carries_no_identity() {
        let partial = r#"{"writer_pid": 1, "writer_exe_path": "/opt/console"}"#;

        assert_eq!(WriterIdentity::from_metadata_json(partial), None);
    }

    #[test]
    fn valid_json_that_is_not_an_object_carries_no_identity() {
        assert_eq!(WriterIdentity::from_metadata_json("5"), None);
        assert_eq!(WriterIdentity::from_metadata_json("[]"), None);
    }

    #[test]
    fn a_non_numeric_writer_pid_carries_no_identity() {
        let bad = r#"{"writer_pid": "not-a-number", "writer_exe_path": "/opt/console", "writer_cwd": "/repo", "writer_build_sha": "abc1234"}"#;

        assert_eq!(WriterIdentity::from_metadata_json(bad), None);
    }

    #[test]
    fn a_metadata_object_missing_only_the_exe_path_carries_no_identity() {
        let partial = r#"{"writer_pid": 1}"#;

        assert_eq!(WriterIdentity::from_metadata_json(partial), None);
    }

    #[test]
    fn a_non_string_writer_exe_path_carries_no_identity() {
        let bad = r#"{"writer_pid": 1, "writer_exe_path": 5, "writer_cwd": "/repo", "writer_build_sha": "abc1234"}"#;

        assert_eq!(WriterIdentity::from_metadata_json(bad), None);
    }

    #[test]
    fn a_non_string_writer_cwd_carries_no_identity() {
        let bad = r#"{"writer_pid": 1, "writer_exe_path": "/opt/console", "writer_cwd": 5, "writer_build_sha": "abc1234"}"#;

        assert_eq!(WriterIdentity::from_metadata_json(bad), None);
    }

    #[test]
    fn a_metadata_object_missing_only_the_build_sha_carries_no_identity() {
        let partial =
            r#"{"writer_pid": 1, "writer_exe_path": "/opt/console", "writer_cwd": "/repo"}"#;

        assert_eq!(WriterIdentity::from_metadata_json(partial), None);
    }

    #[test]
    fn a_non_string_writer_build_sha_carries_no_identity() {
        let bad = r#"{"writer_pid": 1, "writer_exe_path": "/opt/console", "writer_cwd": "/repo", "writer_build_sha": 5}"#;

        assert_eq!(WriterIdentity::from_metadata_json(bad), None);
    }

    #[test]
    fn a_writer_pid_too_large_for_u32_carries_no_identity() {
        let bad = r#"{"writer_pid": 99999999999, "writer_exe_path": "/opt/console", "writer_cwd": "/repo", "writer_build_sha": "abc1234"}"#;

        assert_eq!(WriterIdentity::from_metadata_json(bad), None);
    }

    #[test]
    fn writer_key_ignores_pid_so_an_ordinary_restart_of_the_same_build_matches() {
        let first_run = identity(111, "/opt/console", "abc1234");
        let restarted = identity(222, "/opt/console", "abc1234");

        assert_eq!(first_run.writer_key(), restarted.writer_key());
    }

    #[test]
    fn writer_key_differs_on_a_different_build_sha() {
        let a = identity(111, "/opt/console", "abc1234");
        let b = identity(111, "/opt/console", "9999999");

        assert_ne!(a.writer_key(), b.writer_key());
    }

    #[test]
    fn writer_label_names_the_sha_and_path() {
        assert_eq!(
            writer_label("abc1234", "/opt/console"),
            "build abc1234 at /opt/console"
        );
    }

    #[test]
    fn count_by_writer_key_tallies_each_distinct_writer_in_first_seen_order() {
        let identities = vec![
            identity(111, "/opt/console-a", "abc1234"),
            identity(222, "/opt/console-b", "9999999"),
            // Same build as the first row, different pid (an ordinary
            // restart) -- must fold into the SAME writer, not a third one.
            identity(333, "/opt/console-a", "abc1234"),
            identity(222, "/opt/console-b", "9999999"),
        ];

        let counts = count_by_writer_key(&identities);

        assert_eq!(
            counts,
            vec![
                (("abc1234".to_owned(), "/opt/console-a".to_owned()), 2),
                (("9999999".to_owned(), "/opt/console-b".to_owned()), 2),
            ]
        );
    }

    #[test]
    fn count_by_writer_key_of_no_identities_is_empty() {
        assert_eq!(count_by_writer_key(&[]), Vec::new());
    }

    #[test]
    fn writer_lease_status_segment_names_the_holder_when_read_only() {
        let holder = identity(1, "/opt/other-console", "9999999");
        assert_eq!(
            super::writer_lease_status_segment(&super::WriterLeaseStatus::ReadOnly(holder)),
            Some("READ-ONLY: store owned by build 9999999 at /opt/other-console".to_owned())
        );
    }

    #[test]
    fn writer_lease_status_segment_is_absent_when_writable() {
        assert_eq!(
            super::writer_lease_status_segment(&super::WriterLeaseStatus::Writable),
            None
        );
    }

    #[test]
    fn writer_lease_status_writable_is_writable() {
        assert!(super::WriterLeaseStatus::Writable.is_writable());
    }

    #[test]
    fn writer_lease_status_read_only_is_not_writable() {
        assert!(
            !super::WriterLeaseStatus::ReadOnly(identity(1, "/opt/console", "abc1234"))
                .is_writable()
        );
    }

    #[test]
    fn shared_writer_lease_status_starts_writable_and_reads_back_a_write() {
        let shared = super::SharedWriterLeaseStatus::new();
        assert_eq!(shared.get(), super::WriterLeaseStatus::Writable);

        let holder = identity(999, "/opt/other-console", "9999999");
        shared.set(super::WriterLeaseStatus::ReadOnly(holder.clone()));
        assert_eq!(shared.get(), super::WriterLeaseStatus::ReadOnly(holder));

        shared.set(super::WriterLeaseStatus::Writable);
        assert_eq!(shared.get(), super::WriterLeaseStatus::Writable);
    }

    #[test]
    fn shared_writer_lease_status_default_matches_new() {
        assert_eq!(
            super::SharedWriterLeaseStatus::default().get(),
            super::WriterLeaseStatus::Writable
        );
    }

    #[test]
    fn shared_writer_lease_status_clones_share_the_same_cell() {
        let shared = super::SharedWriterLeaseStatus::new();
        let handle = shared.clone();
        let holder = identity(5, "/opt/other", "fedcba9");
        handle.set(super::WriterLeaseStatus::ReadOnly(holder.clone()));
        assert_eq!(shared.get(), super::WriterLeaseStatus::ReadOnly(holder));
    }

    #[test]
    #[allow(clippy::unwrap_used, clippy::panic)]
    fn shared_writer_lease_status_survives_a_poisoned_lock() {
        let shared = super::SharedWriterLeaseStatus::new();
        let other_handle = shared.clone();
        let join_result = std::thread::spawn(move || {
            other_handle.set(super::WriterLeaseStatus::ReadOnly(identity(
                1,
                "/opt/console",
                "abc1234",
            )));
            let _guard = other_handle.0.lock().unwrap();
            panic!("deliberately poisoning the lock for the fallback test");
        })
        .join();
        assert!(join_result.is_err());

        assert_eq!(shared.get(), super::WriterLeaseStatus::Writable);
        shared.set(super::WriterLeaseStatus::ReadOnly(identity(
            2,
            "/opt/console-2",
            "0000000",
        )));
        assert_eq!(shared.get(), super::WriterLeaseStatus::Writable);
    }
}
