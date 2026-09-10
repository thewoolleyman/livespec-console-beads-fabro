//! The header's factory tell: WHAT happened, WHEN, and WHY
//! (livespec-console-beads-fabro-mx9u.3).
//!
//! # The defect
//!
//! Measured across the 2026-09-08 dogfood passes: `factory: dispatch item
//! failed` sat in the header of every capture from 06:56 to 12:30, across two
//! console restarts, with no time, no cause, and no key that explained it. At
//! 211 columns the status line repeated the same fact as `last command:
//! dispatch item failed — cause not reported`, so two chrome elements said one
//! thing and neither said why or when. An operator cannot tell a failure that
//! happened moments ago from one that happened before lunch, and a tell that
//! never ages is one they learn to stop reading.
//!
//! # Why the timestamp arrives from OUTSIDE the projection
//!
//! [`console_domain::ConsoleEvent`] carries no timestamp: the store exposes
//! `list_console_events_with_observed_at` as a SEPARATE read precisely so the
//! domain envelope is not widened for the one diagnostic that needs a moment
//! (see that function's own doc comment, written for
//! livespec-console-beads-fabro-mx9u.14). This module follows that precedent
//! rather than inventing a second one: the outcome is derived from those pairs
//! by a pure function, and the CLOCK is injected alongside it rather than read
//! from a hidden global — the injectability rule the coverage-region discipline
//! in CLAUDE.md states.

use console_domain::{ConsoleEvent, EventType};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::failure_cause;

/// What the header's factory tell needs to say, and the two moments it needs to
/// say WHEN.
///
/// `observed_now` travels with the outcome rather than being read where the
/// segment is rendered: the projection is clock-free, so the moment is supplied
/// by whoever already holds a clock (the background poller), the same way the
/// per-source last-successful-read map is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactoryOutcomeTell {
    activity: String,
    cause: Option<String>,
    observed_at: String,
    observed_now: String,
}

impl FactoryOutcomeTell {
    #[must_use]
    /// Construct a tell from its parts.
    pub const fn new(
        activity: String,
        cause: Option<String>,
        observed_at: String,
        observed_now: String,
    ) -> Self {
        Self {
            activity,
            cause,
            observed_at,
            observed_now,
        }
    }

    #[must_use]
    /// The activity label, as the header has always rendered it
    /// (`dispatch item failed`, `drain in flight`, ...).
    pub fn activity(&self) -> &str {
        &self.activity
    }

    #[must_use]
    /// The cause the outcome's payload recorded, when it recorded one.
    pub fn cause(&self) -> Option<&str> {
        self.cause.as_deref()
    }

    #[must_use]
    /// When the outcome was observed.
    pub fn observed_at(&self) -> &str {
        &self.observed_at
    }

    #[must_use]
    /// How long ago the outcome was observed, in the header's words, or `None`
    /// when either moment is unparseable — an unreadable timestamp yields NO
    /// age rather than a fabricated one, the same rule the stale-since rider
    /// keeps.
    pub fn age(&self) -> Option<String> {
        relative_age(&self.observed_now, &self.observed_at)
    }

    #[must_use]
    /// The whole outcome as the overlay shows it: activity, age and the FULL
    /// cause, untruncated. The header renders as much of this as it has room
    /// for; this is what Enter on the focused header opens
    /// (livespec-console-beads-fabro-mx9u.3 AC3).
    pub fn full_text(&self) -> String {
        let mut text = self.activity.clone();
        if let Some(age) = self.age() {
            text.push(' ');
            text.push_str(&age);
        }
        text.push_str("\nobserved at ");
        text.push_str(&self.observed_at);
        text.push_str("\ncause: ");
        text.push_str(self.cause.as_deref().unwrap_or(crate::OUTCOME_CAUSE_ABSENT));
        text
    }
}

/// How long ago `then` was, measured from `now`, in the header's words.
///
/// Coarse ON PURPOSE. The question the operator is asking is "is this fresh or
/// is it left over from this morning", and a tell that reads `2h ago` answers
/// it in three characters where `2h 14m 03s` would spend a dozen saying no more.
/// A moment in the FUTURE (clock skew between the writer and this process) reads
/// as `just now` rather than as a negative age, and an unparseable moment on
/// either side yields `None` rather than a guess.
#[must_use]
pub fn relative_age(now: &str, then: &str) -> Option<String> {
    let now = OffsetDateTime::parse(now, &Rfc3339).ok()?;
    let then = OffsetDateTime::parse(then, &Rfc3339).ok()?;
    let seconds = (now - then).whole_seconds();
    if seconds < 60 {
        return Some("just now".to_owned());
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return Some(format!("{minutes}m ago"));
    }
    let hours = minutes / 60;
    if hours < 24 {
        return Some(format!("{hours}h ago"));
    }
    Some(format!("{}d ago", hours / 24))
}

/// The LATEST factory outcome in the log, paired with when it was observed.
///
/// The event scan mirrors [`crate::factory_drain_activity`] exactly — same
/// arms, same labels — because the two must never disagree about which event is
/// the current tell. The cause is read by [`crate::failure_cause`], the SAME
/// reader that backs the status line's failed-command notice, so the header and
/// the status line cannot report different causes for one failure
/// (livespec-console-beads-fabro-mx9u.3 AC2).
#[must_use]
pub fn latest_factory_outcome(
    events_with_observed_at: &[(ConsoleEvent, String)],
    observed_now: &str,
) -> Option<FactoryOutcomeTell> {
    for (event, observed_at) in events_with_observed_at.iter().rev() {
        if let Some(activity) = crate::factory_activity_label(event) {
            return Some(FactoryOutcomeTell::new(
                activity,
                failure_cause(event.payload_json()),
                observed_at.clone(),
                observed_now.to_owned(),
            ));
        }
    }
    None
}

/// Whether `event_type` is one the factory tell speaks for at all.
#[must_use]
pub const fn is_factory_outcome_event_type(event_type: EventType) -> bool {
    matches!(
        event_type,
        EventType::FactoryDrainRequested
            | EventType::FactoryDrainStarted
            | EventType::FactoryDispatchItemRequested
            | EventType::FactoryDispatchItemStarted
            | EventType::FactoryDrainCompleted
            | EventType::FactoryDrainFailed
            | EventType::FactoryDrainAwaitingHuman
            | EventType::FactoryDrainNotWired
            | EventType::FactoryDispatchItemCompleted
            | EventType::FactoryDispatchItemFailed
            | EventType::FactoryDispatchItemNotWired
    )
}

/// How many characters of a recorded CAUSE the header tell carries.
///
/// The header's fields are atomic — kept whole or dropped whole, never
/// mid-truncated — so the cause is bounded HERE, before the field exists,
/// rather than by teaching the fitter a second multi-form field (the
/// source-health degrade is the only one, and it is tuned against a protected
/// e2e guarantee). Enough to carry a real refusal's opening clause; short
/// enough that the field still fits beside the rest at a dogfood width. The
/// WHOLE cause is one keystroke away — Enter on the focused header
/// (livespec-console-beads-fabro-mx9u.3).
pub const FACTORY_TELL_CAUSE_BUDGET: usize = 48;

/// The header's factory tell: the activity, how long ago it happened, and why,
/// as much of each as the field carries (livespec-console-beads-fabro-mx9u.3).
///
/// The age and cause ride along only when the outcome the runtime supplied is
/// for the SAME activity the event log currently reports. A tell that paired
/// one event's label with another event's timestamp would be worse than the
/// bare label it replaced, so a disagreement degrades to exactly what the
/// header said before this item.
#[must_use]
pub fn factory_tell_text(
    activity: Option<&str>,
    outcome: Option<&FactoryOutcomeTell>,
) -> Option<String> {
    let activity = activity?;
    let Some(outcome) = outcome else {
        return Some(activity.to_owned());
    };
    if outcome.activity() != activity {
        return Some(activity.to_owned());
    }
    let mut text = activity.to_owned();
    if let Some(age) = outcome.age() {
        text.push(' ');
        text.push_str(&age);
    }
    if let Some(cause) = outcome.cause() {
        text.push_str(" — ");
        text.push_str(&elide_cause(cause));
    }
    Some(text)
}

/// `cause` cut to [`FACTORY_TELL_CAUSE_BUDGET`] characters with an ellipsis
/// when anything was cut, and returned whole when it already fits.
fn elide_cause(cause: &str) -> String {
    let flattened = cause.split_whitespace().collect::<Vec<_>>().join(" ");
    if flattened.chars().count() <= FACTORY_TELL_CAUSE_BUDGET {
        return flattened;
    }
    let mut elided = flattened
        .chars()
        .take(FACTORY_TELL_CAUSE_BUDGET - 1)
        .collect::<String>();
    elided.push('…');
    elided
}

/// The text the factory-outcome overlay opens with, or `None` when there is
/// nothing to open (livespec-console-beads-fabro-mx9u.3 AC3).
///
/// `tell` is the header's own composed tell, which
/// [`factory_tell_text`] already refuses to decorate with an age or a cause
/// belonging to a different activity. Reading agreement back off it keeps ONE
/// rule for what the header and this overlay both say: an outcome the event log
/// has moved past opens nothing, rather than showing a stale cause under a
/// fresh heading.
#[must_use]
pub fn overlay_text(outcome: Option<&FactoryOutcomeTell>, tell: Option<&str>) -> Option<String> {
    let outcome = outcome?;
    if tell?.starts_with(outcome.activity()) {
        Some(outcome.full_text())
    } else {
        None
    }
}

/// A thread-shared cell holding the most recently derived factory outcome.
///
/// The same shape, and for the same reason, as
/// [`crate::source_staleness::SharedSourceLastSuccess`]: deriving this needs a
/// store read AND a clock, neither of which belongs on the render thread, so
/// the background poller computes it on its own cadence and the render loop
/// takes a cheap, non-blocking snapshot each tick. Re-set every sweep rather
/// than taken once, because the AGE it carries keeps changing even when the
/// outcome does not.
#[derive(Clone, Debug, Default)]
pub struct SharedFactoryOutcome(std::sync::Arc<std::sync::Mutex<Option<FactoryOutcomeTell>>>);

impl SharedFactoryOutcome {
    #[must_use]
    /// Construct a new cell, unset (reads as `None` until the first
    /// [`Self::set`]) -- the header then renders exactly the bare tell it
    /// rendered before this item, which is the right thing to show before the
    /// first sweep has derived anything.
    pub fn new() -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(None)))
    }

    /// Overwrite the shared value with a freshly derived outcome.
    ///
    /// A poisoned lock is swallowed rather than propagated, the same
    /// silence-on-failure contract every neighbouring cell keeps: the cost is a
    /// header tell without its age for one cycle, never a crashed poller.
    pub fn set(&self, outcome: Option<FactoryOutcomeTell>) {
        if let Ok(mut guard) = self.0.lock() {
            *guard = outcome;
        }
    }

    #[must_use]
    /// Read the latest shared value. A poisoned lock degrades to `None`, which
    /// renders the bare tell rather than a stale age.
    pub fn get(&self) -> Option<FactoryOutcomeTell> {
        self.0.lock().ok().and_then(|guard| guard.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FactoryOutcomeTell, is_factory_outcome_event_type, latest_factory_outcome, relative_age,
    };
    use console_domain::{ConsoleEvent, EventType};

    const NOW: &str = "2026-09-10T12:00:00Z";

    /// The tell a lookup that found nothing degrades to, so a failing test
    /// reports what it saw rather than panicking on an `unwrap`. Eager, so its
    /// construction is exercised on every call rather than only when a lookup
    /// unexpectedly comes back empty.
    fn blank_tell() -> FactoryOutcomeTell {
        FactoryOutcomeTell::new(String::new(), None, String::new(), String::new())
    }

    #[track_caller]
    fn check(condition: bool, context: &str) {
        assert!(condition, "{context}: condition was false");
    }

    #[test]
    #[should_panic(expected = "condition was false")]
    fn check_panics_with_context_when_condition_is_false() {
        check(false, "expected panic");
    }

    /// The fallback the lookups degrade to when they find nothing: empty in
    /// every field, so a test that unexpectedly finds no outcome reports the
    /// blank rather than panicking on an `unwrap`.
    #[test]
    fn the_blank_tell_is_empty_in_every_field() {
        let blank = blank_tell();
        assert_eq!(blank.activity(), "");
        assert_eq!(blank.cause(), None);
        assert_eq!(blank.observed_at(), "");
        assert_eq!(blank.age(), None);
    }

    fn event(event_id: &str, event_type: EventType, payload_json: &str) -> ConsoleEvent {
        ConsoleEvent::fixture(event_id, event_type, "console:factory-command-handler")
            .with_payload_json(payload_json.to_owned())
    }

    /// The age is COARSE by design: the operator is asking "is this fresh or is
    /// it left over from this morning".
    #[test]
    fn the_age_answers_fresh_or_stale_in_as_few_characters_as_it_can() {
        assert_eq!(
            relative_age(NOW, "2026-09-10T11:59:31Z"),
            Some("just now".to_owned())
        );
        assert_eq!(
            relative_age(NOW, "2026-09-10T11:45:00Z"),
            Some("15m ago".to_owned())
        );
        assert_eq!(
            relative_age(NOW, "2026-09-10T09:00:00Z"),
            Some("3h ago".to_owned())
        );
        assert_eq!(
            relative_age(NOW, "2026-09-07T12:00:00Z"),
            Some("3d ago".to_owned())
        );
        // Clock skew between the writer and this process reads as `just now`
        // rather than as a negative age.
        assert_eq!(
            relative_age(NOW, "2026-09-10T12:00:30Z"),
            Some("just now".to_owned())
        );
        // An unreadable moment on either side yields NO age rather than a
        // fabricated one -- the rule the stale-since rider already keeps.
        assert_eq!(relative_age(NOW, "not a timestamp"), None);
        assert_eq!(relative_age("not a timestamp", NOW), None);
    }

    /// AC2's contract, at the reader: the cause comes from the SAME
    /// `failure_cause` the status line uses, so the two surfaces cannot report
    /// different causes for one failure.
    #[test]
    fn the_outcome_carries_the_cause_its_payload_recorded() {
        let events = vec![(
            event(
                "evt_failed",
                EventType::FactoryDispatchItemFailed,
                r#"{"domain_error":"dispatch_refused","summary":"the item is not in the ready set"}"#,
            ),
            "2026-09-10T09:00:00Z".to_owned(),
        )];
        let outcome = latest_factory_outcome(&events, NOW);
        let outcome = outcome.unwrap_or_else(blank_tell);
        assert_eq!(outcome.activity(), "dispatch item failed");
        assert_eq!(outcome.age(), Some("3h ago".to_owned()));
        check(
            outcome
                .cause()
                .is_some_and(|cause| cause.contains("ready set")),
            &format!("cause was {:?}", outcome.cause()),
        );
        // The accessor the overlay and any future surface read the exact
        // moment through.
        assert_eq!(outcome.observed_at(), "2026-09-10T09:00:00Z");
        // The overlay's text carries the WHOLE cause and the exact moment.
        let full = outcome.full_text();
        check(
            full.contains("dispatch item failed 3h ago"),
            &format!("full text was {full}"),
        );
        check(
            full.contains("2026-09-10T09:00:00Z"),
            &format!("full text was {full}"),
        );
        check(
            full.contains("the item is not in the ready set"),
            &format!("full text was {full}"),
        );
    }

    /// A failure whose payload recorded NO cause says so, rather than leaving a
    /// bare verdict nobody can diagnose.
    #[test]
    fn a_failure_with_an_empty_payload_names_the_absence() {
        let events = vec![(
            event("evt_failed", EventType::FactoryDrainFailed, "{}"),
            "2026-09-10T11:00:00Z".to_owned(),
        )];
        let outcome = latest_factory_outcome(&events, NOW);
        let outcome = outcome.unwrap_or_else(blank_tell);
        assert_eq!(outcome.cause(), None);
        check(
            outcome.full_text().contains("cause not reported"),
            &format!("full text was {}", outcome.full_text()),
        );
    }

    /// AC1's second half, at the reader: a LATER factory outcome replaces the
    /// earlier one, and events the tell does not speak for are passed over
    /// rather than mistaken for one.
    #[test]
    fn the_latest_factory_outcome_wins_and_unrelated_events_are_passed_over() {
        let events = vec![
            (
                event("evt_failed", EventType::FactoryDispatchItemFailed, "{}"),
                "2026-09-10T09:00:00Z".to_owned(),
            ),
            (
                event("evt_done", EventType::FactoryDispatchItemCompleted, "{}"),
                "2026-09-10T11:30:00Z".to_owned(),
            ),
            (
                event(
                    "evt_other",
                    EventType::SourceNotObservedFindingObserved,
                    "{}",
                ),
                "2026-09-10T11:45:00Z".to_owned(),
            ),
        ];
        let outcome = latest_factory_outcome(&events, NOW);
        let outcome = outcome.unwrap_or_else(blank_tell);
        assert_eq!(outcome.activity(), "dispatch item completed");
        assert_eq!(outcome.age(), Some("30m ago".to_owned()));

        // A log with no factory event at all yields no tell.
        let unrelated = vec![(
            event(
                "evt_other",
                EventType::SourceNotObservedFindingObserved,
                "{}",
            ),
            "2026-09-10T11:45:00Z".to_owned(),
        )];
        assert_eq!(latest_factory_outcome(&unrelated, NOW), None);
    }

    /// Every way the overlay can decline to open, and the one way it opens
    /// (livespec-console-beads-fabro-mx9u.3 AC3).
    #[test]
    fn the_overlay_opens_only_when_the_outcome_describes_what_the_header_says() {
        let tell = FactoryOutcomeTell::new(
            "drain failed".to_owned(),
            Some("the dispatcher refused".to_owned()),
            "2026-09-10T11:00:00Z".to_owned(),
            NOW.to_owned(),
        );

        // Agreement: the overlay carries the WHOLE outcome.
        let opened = super::overlay_text(
            Some(&tell),
            Some("drain failed 1h ago — the dispatcher refused"),
        );
        check(
            opened
                .as_deref()
                .is_some_and(|text| text.contains("the dispatcher refused")),
            &format!("opened with {opened:?}"),
        );

        // The header has moved on to a different activity: nothing opens.
        assert_eq!(
            super::overlay_text(Some(&tell), Some("dispatch item completed")),
            None
        );

        // The header is saying nothing about the factory at all -- the log
        // carries no factory event -- so there is nothing to drill into, even
        // though an outcome from earlier is still in hand.
        assert_eq!(super::overlay_text(Some(&tell), None), None);

        // And with no outcome supplied at all.
        assert_eq!(super::overlay_text(None, Some("drain failed")), None);
    }

    #[test]
    fn the_shared_cell_starts_unset_and_reflects_the_latest_set() {
        let cell = super::SharedFactoryOutcome::new();
        assert_eq!(cell.get(), None);
        let tell = FactoryOutcomeTell::new(
            "drain failed".to_owned(),
            None,
            "2026-09-10T11:00:00Z".to_owned(),
            NOW.to_owned(),
        );
        cell.set(Some(tell.clone()));
        assert_eq!(cell.get(), Some(tell));
        // Re-set every sweep, INCLUDING back to nothing: the tell is retired
        // when the log no longer carries a factory outcome at all.
        cell.set(None);
        assert_eq!(cell.get(), None);
        assert_eq!(super::SharedFactoryOutcome::default().get(), None);
    }

    #[test]
    #[allow(clippy::unwrap_used, clippy::panic)]
    fn a_poisoned_cell_degrades_to_the_bare_tell_rather_than_a_stale_age() {
        let cell = super::SharedFactoryOutcome::new();
        let other = cell.clone();
        let join = std::thread::spawn(move || {
            other.set(Some(FactoryOutcomeTell::new(
                "drain failed".to_owned(),
                None,
                "2026-09-10T11:00:00Z".to_owned(),
                NOW.to_owned(),
            )));
            let _guard = other.0.lock().unwrap();
            panic!("deliberately poisoning the lock for the fallback test");
        })
        .join();
        assert!(join.is_err());
        assert_eq!(cell.get(), None);
        // A later sweep's `set` on the poisoned cell is swallowed, never a
        // crashed poller, and the header keeps rendering the bare tell.
        cell.set(Some(blank_tell()));
        assert_eq!(cell.get(), None);
    }

    /// An unreadable timestamp yields NO age in either rendering -- the header
    /// tell and the overlay both drop it rather than fabricate one -- while
    /// the activity and the cause still show.
    #[test]
    fn an_unreadable_timestamp_renders_without_an_age() {
        let tell = FactoryOutcomeTell::new(
            "drain failed".to_owned(),
            Some("the dispatcher refused".to_owned()),
            "not a timestamp".to_owned(),
            NOW.to_owned(),
        );
        assert_eq!(
            super::factory_tell_text(Some("drain failed"), Some(&tell)),
            Some("drain failed — the dispatcher refused".to_owned())
        );
        assert_eq!(
            tell.full_text(),
            "drain failed\nobserved at not a timestamp\ncause: the dispatcher refused"
        );
    }

    #[test]
    fn the_event_type_predicate_agrees_with_the_labeller() {
        assert!(is_factory_outcome_event_type(
            EventType::FactoryDispatchItemFailed
        ));
        assert!(is_factory_outcome_event_type(
            EventType::FactoryDrainStarted
        ));
        assert!(!is_factory_outcome_event_type(
            EventType::SourceNotObservedFindingObserved
        ));
    }
}
