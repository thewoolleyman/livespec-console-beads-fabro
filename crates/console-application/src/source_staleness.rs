//! Whether the picture the console renders from currently-unavailable event
//! sources is CURRENT or STALE, and since when -- the presentation half of
//! livespec-console-beads-fabro-mx9u.17.
//!
//! `mx9u.11` fixed a systemic freeze (a source-read failure that froze the
//! WHOLE header for hours), but left the underlying CONDITION untouched: a
//! source that has been unavailable since some past moment keeps having its
//! last good snapshot rendered with no indication that it is no longer
//! current. `doctor` (livespec-console-beads-fabro-mx9u.14,
//! [`crate::doctor`]) already dates that moment per source, from the same
//! `(source, last-successful-read)` derivation
//! ([`crate::doctor::last_successful_observed_at`]) this module folds into
//! ONE header-line-worthy fact: across every currently-unavailable source
//! (the header's own [`crate::TuiProjection::unavailable_sources`] tally),
//! either the OLDEST last-successful-read (the longest-running outage, the
//! worst single number worth putting in one line), or -- when even one of
//! them has never had a successful read at all -- [`SourceStaleness::NeverObserved`],
//! never a fabricated timestamp. "Do not present unknown as known" is the
//! theme of this whole phase (see the epic,
//! livespec-console-beads-fabro-mx9u.20): a default or synthesized timestamp
//! rendered as though measured is the same defect class as the tally that
//! once branded a healthy source unavailable.
//!
//! [`source_staleness_header_segment`] renders the shared vocabulary
//! [`crate::doctor::unavailable_source_finding`] and the Help header section
//! (`console_tui::header_help_lines`) already use -- "STALE, not current" and
//! "last successful read: <...>" -- so the header rider, `doctor`, and Help
//! can never describe the same condition in different words.
//!
//! [`SharedSourceStaleness`] is the render-thread / poller-thread handoff
//! cell, structured identically to
//! [`crate::build_identity::SharedBuildStaleness`] and for the same reason:
//! computing this needs two `SQLite` reads (`list_console_events_with_observed_at`,
//! `list_checkpoints`) beyond the cheap event re-list the render thread
//! already does every tick, so it runs on the background source poller's
//! existing cadence, never on the thread the operator's keystrokes depend on
//! (livespec-console-beads-fabro-mx9u.8, mx9u.9: a per-render or per-keystroke
//! store read is exactly the regression those items fixed).

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
/// How stale the console's rendered picture is, from its currently-unavailable
/// event sources.
pub enum SourceStaleness {
    /// No event source is currently unavailable: nothing to report.
    AllObserved,
    /// At least one currently-unavailable source has NEVER had a successful
    /// read -- there is no timestamp to name, and naming one anyway would be
    /// exactly the "present unknown as known" defect this item exists to
    /// close.
    NeverObserved,
    /// Every currently-unavailable source has a known last successful read;
    /// this is the OLDEST of them (the longest-running outage), as an RFC
    /// 3339 timestamp.
    Since(String),
}

#[must_use]
/// Fold the header's unavailable-sources tally and the per-source last-success
/// map into the single [`SourceStaleness`] fact worth a header line.
///
/// `unavailable_sources` is the header's own tally; `last_success` is keyed by
/// source, from [`crate::doctor::last_successful_observed_at`]. A source
/// present in `unavailable_sources` but ABSENT from `last_success`
/// has never been read successfully -- that source alone forces
/// [`SourceStaleness::NeverObserved`] for the whole line, even when every
/// other unavailable source has a perfectly good timestamp: the unknown case
/// is the more urgent fact, and folding it into "since <the other source's
/// timestamp>" would hide it.
pub fn oldest_unavailable_since(
    unavailable_sources: &[String],
    last_success: &BTreeMap<String, String>,
) -> SourceStaleness {
    if unavailable_sources.is_empty() {
        return SourceStaleness::AllObserved;
    }
    let mut oldest: Option<&str> = None;
    for source in unavailable_sources {
        let Some(since) = last_success.get(source) else {
            return SourceStaleness::NeverObserved;
        };
        oldest = Some(oldest.map_or(since.as_str(), |current| current.min(since.as_str())));
    }
    oldest.map_or(SourceStaleness::NeverObserved, |since| {
        SourceStaleness::Since(since.to_owned())
    })
}

#[must_use]
/// The header's stale-since rider, or `None` while every source is current.
///
/// Deliberately terse -- "one line, must never crowd the header" (maintainer
/// ruling) -- so it keeps only the word `STALE` in common with the fuller
/// wording `doctor`'s finding and the Help header section share with each
/// other (livespec-console-beads-fabro-mx9u.17 AC4 binds THOSE two, not this
/// rider, to identical phrasing). `never observed` still says plainly that no
/// timestamp exists, rather than fabricating one.
pub fn source_staleness_header_segment(staleness: &SourceStaleness) -> Option<String> {
    match staleness {
        SourceStaleness::AllObserved => None,
        SourceStaleness::NeverObserved => Some("STALE, never observed".to_owned()),
        SourceStaleness::Since(since) => Some(format!("STALE since {since}")),
    }
}

/// A thread-shared cell holding the most recently observed [`SourceStaleness`].
///
/// See the module doc for why this exists at all (the same reasoning as
/// [`crate::build_identity::SharedBuildStaleness`], reused here for a second
/// live re-probe). The background source poller computes a fresh value every
/// cycle via [`Self::set`]; the render thread takes a cheap, non-blocking
/// snapshot every idle tick via [`Self::get`].
#[derive(Clone, Debug)]
pub struct SharedSourceStaleness(std::sync::Arc<std::sync::Mutex<SourceStaleness>>);

impl SharedSourceStaleness {
    #[must_use]
    /// Construct a new cell, unset (reads as [`SourceStaleness::AllObserved`]
    /// until the first [`Self::set`]) -- silence-by-design for the brief
    /// startup window before the poller's first sweep lands, same as the
    /// `event sources: loading` tell already covers for the unavailability
    /// tally itself.
    pub fn new() -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(
            SourceStaleness::AllObserved,
        )))
    }

    /// Overwrite the shared value with a freshly observed staleness.
    ///
    /// Called from the background poller thread, once per its cadence. A
    /// poisoned lock is swallowed rather than propagated, exactly as
    /// [`crate::build_identity::SharedBuildStaleness::set`] does: this
    /// module's contract is silence on failure, never a crash.
    pub fn set(&self, staleness: SourceStaleness) {
        if let Ok(mut guard) = self.0.lock() {
            *guard = staleness;
        }
    }

    #[must_use]
    /// Read the latest shared value.
    ///
    /// A poisoned lock degrades to [`SourceStaleness::AllObserved`] -- the
    /// same silence-by-design fallback as a fresh, unset cell -- rather than
    /// a panic that would take the whole render loop down over a stale-since
    /// rider.
    pub fn get(&self) -> SourceStaleness {
        self.0
            .lock()
            .map_or(SourceStaleness::AllObserved, |guard| guard.clone())
    }
}

impl Default for SharedSourceStaleness {
    fn default() -> Self {
        Self::new()
    }
}

/// A thread-shared cell holding the per-source last-successful-read map, for
/// the Event sources roster's stale-since COLUMN
/// (livespec-console-beads-fabro-mx9u.17, pzbdbo.29's roster).
///
/// Structured identically to [`SharedSourceStaleness`] above, and for the
/// same reason -- it is the SAME poller-computed fact
/// ([`crate::doctor::last_successful_observed_at`]), just handed to the
/// render loop unsummarized so a roster row can read its OWN source's entry
/// rather than the header's single worst-case timestamp.
#[derive(Clone, Debug)]
pub struct SharedSourceLastSuccess(std::sync::Arc<std::sync::Mutex<BTreeMap<String, String>>>);

impl SharedSourceLastSuccess {
    #[must_use]
    /// Construct a new cell, unset (reads as an empty map until the first
    /// [`Self::set`]) -- silence-by-design for the brief startup window
    /// before the poller's first sweep lands, same as
    /// [`SharedSourceStaleness::new`].
    pub fn new() -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(BTreeMap::new())))
    }

    /// Overwrite the shared value with a freshly observed map.
    ///
    /// Called from the background poller thread, once per its cadence. A
    /// poisoned lock is swallowed rather than propagated, exactly as
    /// [`SharedSourceStaleness::set`] does.
    pub fn set(&self, last_success: BTreeMap<String, String>) {
        if let Ok(mut guard) = self.0.lock() {
            *guard = last_success;
        }
    }

    #[must_use]
    /// Read the latest shared value.
    ///
    /// A poisoned lock degrades to an empty map -- the same silence-by-design
    /// fallback as a fresh, unset cell -- rather than a panic that would take
    /// the whole render loop down over a roster column.
    pub fn get(&self) -> BTreeMap<String, String> {
        self.0
            .lock()
            .map_or_else(|_error| BTreeMap::new(), |guard| guard.clone())
    }
}

impl Default for SharedSourceLastSuccess {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        SharedSourceLastSuccess, SharedSourceStaleness, SourceStaleness, oldest_unavailable_since,
        source_staleness_header_segment,
    };

    #[test]
    fn all_observed_when_no_source_is_unavailable() {
        assert_eq!(
            oldest_unavailable_since(&[], &BTreeMap::new()),
            SourceStaleness::AllObserved
        );
        assert_eq!(
            source_staleness_header_segment(&SourceStaleness::AllObserved),
            None
        );
    }

    #[test]
    fn never_observed_when_an_unavailable_source_has_no_last_success() {
        let unavailable = vec!["dispatcher".to_owned()];
        assert_eq!(
            oldest_unavailable_since(&unavailable, &BTreeMap::new()),
            SourceStaleness::NeverObserved
        );
    }

    #[test]
    fn never_observed_dominates_even_when_another_unavailable_source_has_a_timestamp() {
        // "unknown" outranks "known but old" -- folding a NeverObserved source
        // into the other source's timestamp would hide the more urgent fact.
        let unavailable = vec!["dispatcher".to_owned(), "livespec".to_owned()];
        let mut last_success = BTreeMap::new();
        last_success.insert("dispatcher".to_owned(), "2026-09-01T00:00:00Z".to_owned());
        assert_eq!(
            oldest_unavailable_since(&unavailable, &last_success),
            SourceStaleness::NeverObserved
        );
    }

    #[test]
    fn since_names_the_oldest_last_successful_read_across_unavailable_sources() {
        let unavailable = vec!["dispatcher".to_owned(), "livespec".to_owned()];
        let mut last_success = BTreeMap::new();
        last_success.insert("dispatcher".to_owned(), "2026-09-08T14:05:14Z".to_owned());
        last_success.insert("livespec".to_owned(), "2026-09-05T00:00:00Z".to_owned());
        assert_eq!(
            oldest_unavailable_since(&unavailable, &last_success),
            SourceStaleness::Since("2026-09-05T00:00:00Z".to_owned())
        );
    }

    #[test]
    fn since_ignores_a_last_success_for_a_source_that_is_not_currently_unavailable() {
        let unavailable = vec!["dispatcher".to_owned()];
        let mut last_success = BTreeMap::new();
        last_success.insert("dispatcher".to_owned(), "2026-09-08T14:05:14Z".to_owned());
        last_success.insert("github".to_owned(), "2020-01-01T00:00:00Z".to_owned());
        assert_eq!(
            oldest_unavailable_since(&unavailable, &last_success),
            SourceStaleness::Since("2026-09-08T14:05:14Z".to_owned())
        );
    }

    #[test]
    fn header_segment_names_stale_since_or_never_observed_tersely() {
        // Deliberately terse (one line, must never crowd the header): the
        // rider keeps only `STALE` in common with the fuller wording
        // `doctor`'s finding and Help share with each other -- see
        // `header_help_and_doctors_stale_finding_describe_the_same_condition_in_the_same_words`
        // in `console-tui` for THAT AC4 pairing. An empty fallback rather than
        // a panic on an unexpected `None`: the assertions below then fail
        // honestly on an empty segment instead.
        let never =
            source_staleness_header_segment(&SourceStaleness::NeverObserved).unwrap_or_default();
        assert_eq!(never, "STALE, never observed");

        let since = source_staleness_header_segment(&SourceStaleness::Since(
            "2026-09-08T14:05:14Z".to_owned(),
        ))
        .unwrap_or_default();
        assert_eq!(since, "STALE since 2026-09-08T14:05:14Z");
    }

    #[test]
    fn shared_cell_starts_all_observed_and_reflects_the_latest_set() {
        let shared = SharedSourceStaleness::new();
        assert_eq!(shared.get(), SourceStaleness::AllObserved);

        shared.set(SourceStaleness::NeverObserved);
        assert_eq!(shared.get(), SourceStaleness::NeverObserved);

        shared.set(SourceStaleness::Since("2026-09-08T14:05:14Z".to_owned()));
        assert_eq!(
            shared.get(),
            SourceStaleness::Since("2026-09-08T14:05:14Z".to_owned())
        );
    }

    #[test]
    fn shared_cell_default_matches_new() {
        assert_eq!(
            SharedSourceStaleness::default().get(),
            SourceStaleness::AllObserved
        );
    }

    #[test]
    #[allow(clippy::unwrap_used, clippy::panic)]
    fn shared_source_staleness_survives_a_poisoned_lock() {
        let shared = SharedSourceStaleness::new();
        let other_handle = shared.clone();
        // Poison the mutex by panicking while it is held, exactly as
        // `SharedBuildStaleness`'s own test does -- this is the ONLY way to
        // reach `set`'s and `get`'s `Err` branch honestly rather than by
        // annotation.
        let join_result = std::thread::spawn(move || {
            other_handle.set(SourceStaleness::NeverObserved);
            let _guard = other_handle.0.lock().unwrap();
            panic!("deliberately poisoning the lock for the fallback test");
        })
        .join();
        assert!(join_result.is_err());

        // `get` degrades to `AllObserved` rather than propagating the
        // poison...
        assert_eq!(shared.get(), SourceStaleness::AllObserved);
        // ...and a later `set` is silently swallowed rather than panicking
        // the caller, per the module's silence-on-failure contract.
        shared.set(SourceStaleness::Since("2026-09-08T14:05:14Z".to_owned()));
        assert_eq!(shared.get(), SourceStaleness::AllObserved);
    }

    #[test]
    fn shared_last_success_starts_empty_and_reflects_the_latest_set() {
        let shared = SharedSourceLastSuccess::new();
        assert_eq!(shared.get(), BTreeMap::new());

        let mut last_success = BTreeMap::new();
        last_success.insert("dispatcher".to_owned(), "2026-09-08T14:05:14Z".to_owned());
        shared.set(last_success.clone());
        assert_eq!(shared.get(), last_success);
    }

    #[test]
    fn shared_last_success_default_matches_new() {
        assert_eq!(SharedSourceLastSuccess::default().get(), BTreeMap::new());
    }

    #[test]
    #[allow(clippy::unwrap_used, clippy::panic)]
    fn shared_last_success_survives_a_poisoned_lock() {
        let shared = SharedSourceLastSuccess::new();
        let other_handle = shared.clone();
        let mut poisoning_map = BTreeMap::new();
        poisoning_map.insert("dispatcher".to_owned(), "2026-09-08T14:05:14Z".to_owned());
        let join_result = std::thread::spawn(move || {
            other_handle.set(poisoning_map);
            let _guard = other_handle.0.lock().unwrap();
            panic!("deliberately poisoning the lock for the fallback test");
        })
        .join();
        assert!(join_result.is_err());

        assert_eq!(shared.get(), BTreeMap::new());
        let mut later = BTreeMap::new();
        later.insert("livespec".to_owned(), "2026-09-05T00:00:00Z".to_owned());
        shared.set(later);
        assert_eq!(shared.get(), BTreeMap::new());
    }
}
