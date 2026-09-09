//! The running console binary's build identity, and how stale it is against
//! the observed repo's current HEAD.
//!
//! `livespec-console-beads-fabro-mx9u.13`: a dogfood pass graded fixes that
//! were not in the running binary -- the maintainer's pane was five merges
//! behind and the header carried no version, sha, or build age, so nothing
//! on screen said so. This module is the fix's model layer: a
//! [`BuildIdentity`] naming the commit and moment the binary was BUILT (see
//! [`BuildIdentity`]'s own doc for why that has to be compile-time), and a
//! [`BuildStaleness`] naming how far that build now trails the repo it is
//! watching.

use crate::source_adapters::{SourceProbe, SourceProbeOutcome};

/// The short commit sha and build timestamp the running binary was compiled
/// from.
///
/// Deliberately carries NO logic for reading either value -- both are handed
/// in fully formed, sourced from `env!()`-embedded constants stamped at
/// COMPILE time by the binary crate's build script (`console-cli/build.rs`).
/// A binary is built once and then runs, copied, or is dogfooded for hours
/// afterward; if this read the working tree at STARTUP instead, a binary
/// copied to another host, or simply left running while `git pull` moved the
/// checkout out from under it, would silently start reporting a commit it
/// was never built from. Compiling the sha in is what makes it durable: it
/// is exactly as true at hour six of a dogfood pass as it was at process
/// start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildIdentity {
    sha: String,
    built_at: String,
}

impl BuildIdentity {
    #[must_use]
    /// Construct a new value from its required fields.
    pub fn new(sha: impl Into<String>, built_at: impl Into<String>) -> Self {
        Self {
            sha: sha.into(),
            built_at: built_at.into(),
        }
    }

    #[must_use]
    /// Return the short commit sha the binary was built from.
    pub fn sha(&self) -> &str {
        &self.sha
    }

    #[must_use]
    /// Return the build's wall-clock timestamp.
    pub fn built_at(&self) -> &str {
        &self.built_at
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// How far the running build's commit trails the observed repo's current HEAD.
pub enum BuildStaleness {
    /// The build commit IS the repo's current HEAD.
    Current,
    /// The repo's HEAD carries `N` commits the build does not.
    ///
    /// Covers both an ordinary fast-forward gap (more commits landed after
    /// the build) and a build commit that is no longer an ancestor of HEAD at
    /// all (history was rewritten under it) -- see
    /// [`build_staleness_from_rev_list_count`] for why one count captures
    /// both.
    Behind(u32),
    /// The comparison could not be made: no repo was observed, the build sha
    /// is unknown to it (a shallow clone, most likely), or the probe itself
    /// was unreachable. Silent by design -- an unproven staleness claim is
    /// never asserted as a definite one.
    Unknown,
}

/// The build-identity chrome text: sha and build timestamp.
///
/// Rendered in the header pane's block TITLE (see
/// `console_tui::render_header`), not as a content-line field -- measured
/// livespec-console-beads-fabro-mx9u.13 against the maintainer's own real,
/// busy 159-column header (a factory alert AND several unavailable sources
/// beside a two-digit attention count): even a sha-only short form of this
/// text had no room left in the content line once every genuinely-content
/// field had its say, and an operator checking which build is running is
/// exactly as likely to do it during a busy moment as a quiet one. A title
/// costs nothing from that budget and is always visible, so the identity
/// lives there instead; only the STALE anomaly (see
/// [`build_staleness_segment`]) stays in the content line, at
/// `TransientState`.
#[must_use]
pub fn build_identity_segment(identity: &BuildIdentity) -> String {
    format!("build {} (built {})", identity.sha(), identity.built_at())
}

/// The header's stale-build tell.
///
/// Present ONLY while [`BuildStaleness::Behind`], naming how far behind.
/// Absent for [`BuildStaleness::Current`] -- the unremarkable case needs no
/// chrome -- and for [`BuildStaleness::Unknown`], so a comparison the console
/// could not actually make is never rendered as though it had.
#[must_use]
pub fn build_staleness_segment(staleness: BuildStaleness) -> Option<String> {
    match staleness {
        BuildStaleness::Behind(commits) => Some(format!(
            "build STALE: {commits} commit{} behind",
            if commits == 1 { "" } else { "s" }
        )),
        BuildStaleness::Current | BuildStaleness::Unknown => None,
    }
}

/// Parse the outcome of `git rev-list --count <build-sha>..HEAD` into a
/// [`BuildStaleness`].
///
/// `git rev-list A..B` counts the commits reachable from `B` that are NOT
/// reachable from `A`, regardless of whether `A` is literally an ancestor of
/// `B`. That single count is exactly what a stale-build tell needs: it is the
/// same number for the ordinary case (the build's commit is an ancestor, and
/// N more commits landed after it) as for a build commit that has fallen off
/// HEAD's history entirely (a rebase or force-push under it) -- either way,
/// HEAD carries `N` commits the build does not, which is the fact the tell
/// reports.
#[must_use]
pub fn build_staleness_from_rev_list_count(outcome: &SourceProbeOutcome) -> BuildStaleness {
    let SourceProbeOutcome::Observed {
        stdout, success, ..
    } = outcome
    else {
        return BuildStaleness::Unknown;
    };
    if !success {
        return BuildStaleness::Unknown;
    }
    match stdout.trim().parse::<u32>() {
        Ok(0) => BuildStaleness::Current,
        Ok(commits) => BuildStaleness::Behind(commits),
        Err(_parse_error) => BuildStaleness::Unknown,
    }
}

/// Observe [`BuildStaleness`] for `build_sha` against `repo_path`'s current
/// HEAD, through `probe` -- the one IO seam this module ever crosses.
///
/// `livespec-console-beads-fabro-mx9u.26`: this used to run ONCE at session
/// startup, on the theory that the running build never changes mid-session so
/// nothing justifies a repeat `git` shell-out. That theory conflated the two
/// halves of "stale": the BUILD sha is indeed fixed for the process's life,
/// but the repo's HEAD is not, and staleness is a function of both. Probed
/// once, the tell measures the gap at the one moment it is guaranteed to be
/// zero -- right after the pane's own build -- and then never again, no
/// matter how far HEAD moves under a session left running for hours (which is
/// the documented, intended usage; see `CLAUDE.md`'s pane-recreation recipe
/// and the "left running between dogfood passes" directive).
///
/// So this is now called REPEATEDLY, on the caller's chosen cadence -- but
/// the ORIGINAL objection was legitimate for the cadence it was rejecting:
/// see [`SharedBuildStaleness`] for where the periodic call actually lives
/// and why that cadence, not this one, is the right place to re-probe.
#[must_use]
pub fn observe_build_staleness(
    probe: &dyn SourceProbe,
    repo_path: &str,
    build_sha: &str,
) -> BuildStaleness {
    let outcome = probe.run_command(
        "git",
        &[
            "-C",
            repo_path,
            "rev-list",
            "--count",
            &format!("{build_sha}..HEAD"),
        ],
    );
    build_staleness_from_rev_list_count(&outcome)
}

/// A thread-shared cell holding the most recently observed [`BuildStaleness`].
///
/// This is the seam that answers `livespec-console-beads-fabro-mx9u.26`'s
/// cadence question. [`observe_build_staleness`] shells out to `git`, so it
/// must never run on the render thread -- the console already learned that
/// lesson for the SOURCE polls (`livespec-console-beads-fabro-pzbdbo.25`,
/// `mx9u.8`/`mx9u.9`: a per-keystroke or per-render shell-out dropped
/// keystrokes and regressed input latency from 91ms back toward 577ms). The
/// fix there was a background poller thread with its own cadence, writing
/// into the store for the render thread to re-read; this reuses that EXACT
/// thread and EXACT cadence (the composition root's `POLLER_CADENCE`, 2
/// seconds) rather than inventing a second timer, because that cadence
/// already governs a sweep of far heavier CLI shell-outs (six backing CLIs
/// plus the orchestrator's `needs-attention` snapshot) every cycle. Measured
/// locally against this repo, `git rev-list --count <sha>..HEAD` costs
/// 10-60ms -- a small fraction of a 2-second window that already tolerates
/// slower calls than this one, and it runs on the same thread that already
/// pays that cost, never on the thread the operator's keystrokes depend on.
///
/// A per-render or per-keystroke cadence remains wrong for the same reason it
/// always was: the render thread's `event::poll` must stay responsive, and a
/// `git` shell-out is IO this process does not control the latency of (a
/// contended disk, an NFS-backed checkout, or simply a slow host would all
/// show up as dropped keystrokes). The 2-second background cadence is slow
/// enough that it costs nothing perceptible and fast enough that a session
/// left running for hours -- the case this item exists for -- learns it has
/// fallen behind within a couple of poll cycles rather than never.
///
/// The render thread never shells out to read this: it takes a `Mutex` lock
/// around a `Copy` enum and releases it immediately, on its own existing
/// idle-tick cadence (`event::poll`'s 250ms timeout already wakes it that
/// often). Starts at [`BuildStaleness::Unknown`] so a session rendered before
/// the first background probe completes shows nothing rather than a false
/// `Current` -- silence-by-design stays intact even for the brief startup
/// window (though the composition root also runs one synchronous probe
/// before the first frame, so this window is not normally observed in
/// practice).
#[derive(Clone, Debug)]
pub struct SharedBuildStaleness(std::sync::Arc<std::sync::Mutex<BuildStaleness>>);

impl SharedBuildStaleness {
    #[must_use]
    /// Construct a new cell, unset (reads as [`BuildStaleness::Unknown`] until
    /// the first [`Self::set`]).
    pub fn new() -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(
            BuildStaleness::Unknown,
        )))
    }

    /// Overwrite the shared value with a freshly observed staleness.
    ///
    /// Called from the background poller thread, once per its cadence. A
    /// poisoned lock -- unreachable here since the critical section is a
    /// single assignment that cannot panic, but not provably so to the
    /// compiler -- is swallowed rather than propagated: this module's
    /// contract is silence on failure, never a crash, and a lost write simply
    /// leaves the previous value in place for one more cycle.
    pub fn set(&self, staleness: BuildStaleness) {
        if let Ok(mut guard) = self.0.lock() {
            *guard = staleness;
        }
    }

    /// Read the latest shared value.
    ///
    /// Called from the render thread, once per idle tick. A poisoned lock
    /// degrades to [`BuildStaleness::Unknown`] -- the same silence-by-design
    /// fallback [`build_staleness_from_rev_list_count`] uses for every other
    /// unprovable case, rather than a panic that would take the whole render
    /// loop down over a stale-build tell.
    #[must_use]
    pub fn get(&self) -> BuildStaleness {
        self.0
            .lock()
            .map_or(BuildStaleness::Unknown, |guard| *guard)
    }
}

impl Default for SharedBuildStaleness {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BuildIdentity, BuildStaleness, SharedBuildStaleness, build_identity_segment,
        build_staleness_from_rev_list_count, build_staleness_segment, observe_build_staleness,
    };
    use crate::source_adapters::{SourceProbe, SourceProbeOutcome};

    #[test]
    fn identity_segment_names_the_sha_and_build_timestamp() {
        let identity = BuildIdentity::new("1b69345", "2026-09-08T11:16:34Z");
        assert_eq!(
            build_identity_segment(&identity),
            "build 1b69345 (built 2026-09-08T11:16:34Z)"
        );
    }

    #[test]
    fn staleness_segment_names_how_far_behind() {
        assert_eq!(
            build_staleness_segment(BuildStaleness::Behind(5)),
            Some("build STALE: 5 commits behind".to_owned())
        );
    }

    #[test]
    fn staleness_segment_uses_the_singular_for_exactly_one_commit() {
        assert_eq!(
            build_staleness_segment(BuildStaleness::Behind(1)),
            Some("build STALE: 1 commit behind".to_owned())
        );
    }

    #[test]
    fn staleness_segment_is_absent_when_current() {
        assert_eq!(build_staleness_segment(BuildStaleness::Current), None);
    }

    #[test]
    fn staleness_segment_is_absent_when_unknown() {
        assert_eq!(build_staleness_segment(BuildStaleness::Unknown), None);
    }

    #[test]
    fn rev_list_count_of_zero_is_current() {
        let outcome = SourceProbeOutcome::observed("0\n", true);
        assert_eq!(
            build_staleness_from_rev_list_count(&outcome),
            BuildStaleness::Current
        );
    }

    #[test]
    fn rev_list_count_of_five_is_behind_by_five() {
        // The dogfood postmortem's own numbers: 5 merges landed between the
        // build and origin/master.
        let outcome = SourceProbeOutcome::observed("5\n", true);
        assert_eq!(
            build_staleness_from_rev_list_count(&outcome),
            BuildStaleness::Behind(5)
        );
    }

    #[test]
    fn a_failed_probe_is_unknown_not_current() {
        let outcome = SourceProbeOutcome::unavailable("git: not found");
        assert_eq!(
            build_staleness_from_rev_list_count(&outcome),
            BuildStaleness::Unknown
        );
    }

    #[test]
    fn an_unsuccessful_command_is_unknown_even_with_stdout() {
        // A build sha unknown to a shallow clone exits non-zero with an
        // empty/garbage count; that is not proof of currency.
        let outcome = SourceProbeOutcome::observed("", false);
        assert_eq!(
            build_staleness_from_rev_list_count(&outcome),
            BuildStaleness::Unknown
        );
    }

    #[test]
    fn unparseable_stdout_is_unknown() {
        let outcome = SourceProbeOutcome::observed("fatal: bad object\n", true);
        assert_eq!(
            build_staleness_from_rev_list_count(&outcome),
            BuildStaleness::Unknown
        );
    }

    struct RecordingProbe {
        expected_program: &'static str,
        response: SourceProbeOutcome,
        seen: std::cell::RefCell<Option<(String, Vec<String>)>>,
    }

    impl SourceProbe for RecordingProbe {
        fn run_command(&self, program: &str, args: &[&str]) -> SourceProbeOutcome {
            assert_eq!(program, self.expected_program);
            *self.seen.borrow_mut() = Some((
                program.to_owned(),
                args.iter().map(|arg| (*arg).to_owned()).collect(),
            ));
            self.response.clone()
        }

        fn read_file(&self, _path: &str) -> SourceProbeOutcome {
            SourceProbeOutcome::unavailable("test probe: no file sources")
        }
    }

    #[test]
    fn observe_build_staleness_runs_git_rev_list_count_against_the_repo_and_sha() {
        let probe = RecordingProbe {
            expected_program: "git",
            response: SourceProbeOutcome::observed("5\n", true),
            seen: std::cell::RefCell::new(None),
        };
        let staleness = observe_build_staleness(&probe, "/data/projects/repo", "1b69345");
        assert_eq!(staleness, BuildStaleness::Behind(5));
        // The `SourceProbe` trait requires `read_file` too, even though this
        // seam never calls it -- exercised directly so the stub carries no
        // dead branch.
        assert_eq!(
            probe.read_file("irrelevant"),
            SourceProbeOutcome::unavailable("test probe: no file sources")
        );
        assert_eq!(
            probe.seen.into_inner(),
            Some((
                "git".to_owned(),
                vec![
                    "-C".to_owned(),
                    "/data/projects/repo".to_owned(),
                    "rev-list".to_owned(),
                    "--count".to_owned(),
                    "1b69345..HEAD".to_owned(),
                ]
            ))
        );
    }

    #[test]
    fn shared_build_staleness_starts_unknown_and_reads_back_a_write() {
        let shared = SharedBuildStaleness::new();
        assert_eq!(shared.get(), BuildStaleness::Unknown);

        shared.set(BuildStaleness::Behind(3));
        assert_eq!(shared.get(), BuildStaleness::Behind(3));

        // A later write overwrites, not accumulates -- AC3's "1 behind, then 5"
        // is exactly this: the render thread sees whatever was written last.
        shared.set(BuildStaleness::Current);
        assert_eq!(shared.get(), BuildStaleness::Current);
    }

    #[test]
    fn shared_build_staleness_default_matches_new() {
        assert_eq!(
            SharedBuildStaleness::default().get(),
            BuildStaleness::Unknown
        );
    }

    #[test]
    fn shared_build_staleness_clones_share_the_same_cell() {
        let shared = SharedBuildStaleness::new();
        let handle = shared.clone();
        handle.set(BuildStaleness::Behind(7));
        // The clone is a handle to the SAME cell (an `Arc`), not an independent
        // copy -- this is what lets the poller thread's clone and the render
        // thread's clone see each other's writes at all.
        assert_eq!(shared.get(), BuildStaleness::Behind(7));
    }

    #[test]
    #[allow(clippy::unwrap_used, clippy::panic)]
    fn shared_build_staleness_survives_a_poisoned_lock() {
        let shared = SharedBuildStaleness::new();
        let other_handle = shared.clone();
        // Poison the mutex by panicking while it is held, exactly as a future
        // caller's own panic-inducing bug would -- this is the ONLY way to
        // reach `set`'s and `get`'s `Err` branch honestly rather than by
        // annotation.
        let join_result = std::thread::spawn(move || {
            other_handle.set(BuildStaleness::Current);
            let _guard = other_handle.0.lock().unwrap();
            panic!("deliberately poisoning the lock for the fallback test");
        })
        .join();
        assert!(join_result.is_err());

        // `get` degrades to `Unknown` rather than propagating the poison...
        assert_eq!(shared.get(), BuildStaleness::Unknown);
        // ...and a later `set` is silently swallowed rather than panicking the
        // caller, per the module's silence-on-failure contract.
        shared.set(BuildStaleness::Behind(9));
        assert_eq!(shared.get(), BuildStaleness::Unknown);
    }
}
