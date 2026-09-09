//! Per-source event counts by type, so the store's own composition is
//! visible on the Event sources roster -- livespec-console-beads-fabro-
//! mx9u.20.2.
//!
//! On 2026-09-09 the store held 9,039 events of which 1,828 -- just over
//! 20% -- were [`EventType::SourceNotObservedFindingObserved`] markers,
//! produced by a second console writer that ran for roughly eleven hours
//! (`livespec-console-beads-fabro-mx9u.23` closes that specific writer-
//! identity hole). Nothing in the TUI showed that; it was found only by
//! querying the `SQLite` store by hand. This module is the VISIBILITY half:
//! a per-source tally of positive observations against not-observed
//! markers, so a pathological source is apparent to a human before `doctor`
//! is ever run.
//!
//! [`source_event_counts`] classifies events the SAME way
//! [`crate::source_observation_tally`] already does -- a
//! [`EventType::SourceNotObservedFindingObserved`] counts against
//! `not_observed`, anything [`crate::is_positive_source_observation`] admits
//! counts against `observed`, and every other event type is not source-
//! health-bearing and is excluded -- so a source's counted total can never
//! disagree with whether that source is even a roster row at all.
//!
//! [`SharedSourceEventCounts`] is the render-thread / poller-thread handoff
//! cell, structured identically to
//! [`crate::build_identity::SharedBuildStaleness`] and
//! [`crate::source_adapters`]'s own shared cells: the composition root's
//! background source poller folds the aggregate on its own ~2s cadence (one
//! `SqliteEventStore::list_console_events` read it does not otherwise need,
//! plus a single O(n) in-memory pass over events already resident in the
//! process), never on the thread the operator's keystrokes depend on
//! (livespec-console-beads-fabro-mx9u.8, mx9u.9: a per-render or
//! per-keystroke aggregate over the whole log is exactly the regression
//! those items fixed). The render thread takes a cheap, non-blocking
//! snapshot every tick.

use std::collections::BTreeMap;

use console_domain::{ConsoleEvent, EventType};

use crate::is_positive_source_observation;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// One source's event counts, split into positive observations and
/// not-observed markers -- the minimum breakdown AC1 asks for.
pub struct SourceEventCounts {
    /// Count of events classified as a positive observation of this source
    /// ([`crate::is_positive_source_observation`]).
    pub observed: usize,
    /// Count of [`EventType::SourceNotObservedFindingObserved`] events for
    /// this source.
    pub not_observed: usize,
}

impl SourceEventCounts {
    #[must_use]
    /// The total of both counted categories.
    pub const fn total(&self) -> usize {
        self.observed + self.not_observed
    }

    #[must_use]
    /// The not-observed share as a whole percent, rounded to the nearest
    /// integer.
    ///
    /// AC3: a source with a disproportionate share of not-observed markers
    /// must be apparent WITHOUT the operator doing arithmetic -- raw counts
    /// side by side do not satisfy that on their own, so this is the number
    /// the roster actually shows. Pure integer arithmetic (round-half-up via
    /// `total / 2`), never a float cast, so this stays a `const fn` and
    /// never trips `clippy::pedantic`'s cast lints (this workspace denies
    /// `pedantic`).
    pub const fn not_observed_percent(&self) -> usize {
        let total = self.total();
        if total == 0 {
            return 0;
        }
        (self.not_observed * 100 + total / 2) / total
    }
}

#[must_use]
/// Fold `events` into a per-source [`SourceEventCounts`] map.
///
/// A pure, allocation-only function over an already-loaded `events` slice --
/// no store access here. The composition root calls this from the
/// background source poller, reusing the SAME `list_console_events` read
/// [`crate::doctor::last_successful_observed_at`]'s callers already pay for
/// on that cadence, never per render.
pub fn source_event_counts(events: &[ConsoleEvent]) -> BTreeMap<String, SourceEventCounts> {
    let mut counts: BTreeMap<String, SourceEventCounts> = BTreeMap::new();
    for event in events {
        let event_type = *event.event_type();
        if event_type == EventType::SourceNotObservedFindingObserved {
            counts
                .entry(event.source().to_owned())
                .or_default()
                .not_observed += 1;
        } else if is_positive_source_observation(event_type) {
            counts
                .entry(event.source().to_owned())
                .or_default()
                .observed += 1;
        }
    }
    counts
}

/// A thread-shared cell holding the most recently computed per-source
/// [`SourceEventCounts`] map.
///
/// See the module doc for why this exists at all (the same reasoning as
/// [`crate::build_identity::SharedBuildStaleness`]). The background source
/// poller computes a fresh value every cycle via [`Self::set`]; the render
/// thread takes a cheap, non-blocking snapshot every idle tick via
/// [`Self::get`].
#[derive(Clone, Debug)]
pub struct SharedSourceEventCounts(
    std::sync::Arc<std::sync::Mutex<BTreeMap<String, SourceEventCounts>>>,
);

impl SharedSourceEventCounts {
    #[must_use]
    /// Construct a new cell, unset (reads as an empty map until the first
    /// [`Self::set`]) -- silence-by-design for the brief startup window
    /// before the poller's first sweep lands. An EMPTY map here is the
    /// honest "not yet counted" state, distinct from a row's own `0`: the
    /// roster reads absence from this map as "not yet counted", never as a
    /// confident zero (the "do not present unknown as known" theme this
    /// whole phase is built on: mx9u.14, mx9u.17, mx9u.22, mx9u.24).
    pub fn new() -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(BTreeMap::new())))
    }

    /// Overwrite the shared value with a freshly computed map.
    ///
    /// Called from the background poller thread, once per its cadence. A
    /// poisoned lock is swallowed rather than propagated, exactly as
    /// [`crate::build_identity::SharedBuildStaleness::set`] does: this
    /// module's contract is silence on failure, never a crash.
    pub fn set(&self, counts: BTreeMap<String, SourceEventCounts>) {
        if let Ok(mut guard) = self.0.lock() {
            *guard = counts;
        }
    }

    #[must_use]
    /// Read the latest shared value.
    ///
    /// A poisoned lock degrades to an empty map -- the same silence-by-design
    /// fallback as a fresh, unset cell -- rather than a panic that would take
    /// the whole render loop down over a roster column.
    pub fn get(&self) -> BTreeMap<String, SourceEventCounts> {
        self.0
            .lock()
            .map_or_else(|_error| BTreeMap::new(), |guard| guard.clone())
    }
}

impl Default for SharedSourceEventCounts {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use console_domain::{ConsoleEvent, EventType};

    use super::{SharedSourceEventCounts, SourceEventCounts, source_event_counts};

    #[test]
    fn empty_events_produce_an_empty_map() {
        assert_eq!(source_event_counts(&[]), BTreeMap::new());
    }

    #[test]
    fn a_not_observed_marker_counts_against_not_observed_only() {
        let events = [ConsoleEvent::fixture(
            "evt_1",
            EventType::SourceNotObservedFindingObserved,
            "dispatcher",
        )];

        let counts = source_event_counts(&events);

        assert_eq!(
            counts.get("dispatcher"),
            Some(&SourceEventCounts {
                observed: 0,
                not_observed: 1
            })
        );
    }

    #[test]
    fn a_positive_observation_counts_against_observed_only() {
        let events = [ConsoleEvent::fixture(
            "evt_1",
            EventType::SourceObservedFindingObserved,
            "dispatcher",
        )];

        let counts = source_event_counts(&events);

        assert_eq!(
            counts.get("dispatcher"),
            Some(&SourceEventCounts {
                observed: 1,
                not_observed: 0
            })
        );
    }

    #[test]
    fn an_event_type_that_is_neither_angle_is_excluded_from_the_tally() {
        // `CommandAccepted` carries a `source` too (every `ConsoleEvent`
        // does), but it is neither a positive source observation nor a
        // not-observed marker -- the SAME exclusion
        // `crate::source_observation_tally` applies, so this source never
        // becomes a roster row in the first place and must not silently
        // gain a phantom count either.
        let events = [ConsoleEvent::fixture(
            "evt_1",
            EventType::CommandAccepted,
            "dispatcher",
        )];

        assert_eq!(source_event_counts(&events), BTreeMap::new());
    }

    #[test]
    fn counts_accumulate_separately_per_source() {
        let events = [
            ConsoleEvent::fixture(
                "evt_1",
                EventType::SourceNotObservedFindingObserved,
                "dispatcher",
            ),
            ConsoleEvent::fixture(
                "evt_2",
                EventType::SourceNotObservedFindingObserved,
                "dispatcher",
            ),
            ConsoleEvent::fixture(
                "evt_3",
                EventType::SourceObservedFindingObserved,
                "livespec",
            ),
        ];

        let counts = source_event_counts(&events);

        assert_eq!(
            counts.get("dispatcher"),
            Some(&SourceEventCounts {
                observed: 0,
                not_observed: 2
            })
        );
        assert_eq!(
            counts.get("livespec"),
            Some(&SourceEventCounts {
                observed: 1,
                not_observed: 0
            })
        );
    }

    #[test]
    fn total_sums_both_categories() {
        let counts = SourceEventCounts {
            observed: 3,
            not_observed: 2,
        };
        assert_eq!(counts.total(), 5);
    }

    #[test]
    fn not_observed_percent_is_zero_over_an_empty_total() {
        assert_eq!(SourceEventCounts::default().not_observed_percent(), 0);
    }

    #[test]
    fn not_observed_percent_matches_the_postmortem_ratio() {
        // The exact motivating measurement (2026-09-09): 9,039 events, 1,828
        // not-observed markers, "just over 20%" -- rounds to 20.
        let counts = SourceEventCounts {
            observed: 9_039 - 1_828,
            not_observed: 1_828,
        };
        assert_eq!(counts.not_observed_percent(), 20);
    }

    #[test]
    fn not_observed_percent_rounds_half_up() {
        // 1 of 3 is 33.33...%, rounds to 33; 2 of 3 is 66.66...%, rounds to
        // 67 -- proving the rounding direction rather than a floor/ceil that
        // would silently under- or over-state the share.
        assert_eq!(
            SourceEventCounts {
                observed: 2,
                not_observed: 1
            }
            .not_observed_percent(),
            33
        );
        assert_eq!(
            SourceEventCounts {
                observed: 1,
                not_observed: 2
            }
            .not_observed_percent(),
            67
        );
    }

    #[test]
    fn not_observed_percent_rounding_term_is_half_of_total_not_its_remainder() {
        // A `total` of 3 (used above) can't tell `total / 2` apart from
        // `total % 2` -- both equal 1 there, coincidentally. An EVEN total
        // of 6 can: `total / 2` is 3 (rounds 1/6 = 16.67% up to 17), while
        // `total % 2` would be 0 (rounds it down to 16) -- proving the
        // rounding term is the true half, not the leftover remainder.
        assert_eq!(
            SourceEventCounts {
                observed: 5,
                not_observed: 1
            }
            .not_observed_percent(),
            17
        );
    }

    #[test]
    fn shared_cell_starts_empty_and_reflects_the_latest_set() {
        let shared = SharedSourceEventCounts::new();
        assert_eq!(shared.get(), BTreeMap::new());

        let mut fresh = BTreeMap::new();
        fresh.insert(
            "dispatcher".to_owned(),
            SourceEventCounts {
                observed: 4,
                not_observed: 1,
            },
        );
        shared.set(fresh.clone());
        assert_eq!(shared.get(), fresh);
    }

    #[test]
    fn shared_cell_default_matches_new() {
        assert_eq!(SharedSourceEventCounts::default().get(), BTreeMap::new());
    }

    #[test]
    #[allow(clippy::unwrap_used, clippy::panic)]
    fn shared_cell_survives_a_poisoned_lock() {
        let shared = SharedSourceEventCounts::new();
        let other_handle = shared.clone();
        let mut poisoning_map = BTreeMap::new();
        poisoning_map.insert(
            "dispatcher".to_owned(),
            SourceEventCounts {
                observed: 1,
                not_observed: 0,
            },
        );
        let join_result = std::thread::spawn(move || {
            other_handle.set(poisoning_map);
            let _guard = other_handle.0.lock().unwrap();
            panic!("deliberately poisoning the lock for the fallback test");
        })
        .join();
        assert!(join_result.is_err());

        // `get` degrades to an empty map rather than propagating the
        // poison...
        assert_eq!(shared.get(), BTreeMap::new());
        // ...and a later `set` is silently swallowed rather than panicking
        // the caller, per the module's silence-on-failure contract.
        let mut later = BTreeMap::new();
        later.insert(
            "livespec".to_owned(),
            SourceEventCounts {
                observed: 2,
                not_observed: 0,
            },
        );
        shared.set(later);
        assert_eq!(shared.get(), BTreeMap::new());
    }
}
