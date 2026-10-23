//! The sources the RUNNING BUILD polls, and what the event log may be asked
//! about them.
//!
//! The event log is a HISTORY. It keeps answering for sources the build no
//! longer has, and a derivation that reads it alone cannot tell a source that
//! is DOWN from one that is RETIRED -- both present as a last finding that was
//! not-observed, and the retired one never recovers because nothing polls it.
//!
//! MEASURED 2026-09-12T19:04Z on the real store, TUI build 19cc391: the header
//! read `event sources: 3 unavailable (fabro, +2 more)` while the store
//! recorded exactly TWO not-observed sources. The third was the unqualified
//! `fabro` source the pre-z4y7kp build polled; nothing has polled that name
//! since 2026-09-11. It counted against the tally AND sorted first, so the one
//! name the header had room for was the one name no running adapter owns. An
//! operator reading "fabro" unavailable concludes the factory their work runs
//! on is down -- hp was healthy and executing -- and that reading cost real
//! time twice, most recently on 2026-09-29 when it produced a walkthrough step
//! telling the maintainer to start a local fabro that no host in this fleet
//! runs (livespec-console-beads-fabro-rhh56m defects 2 and 3).
//!
//! Which sources the console may report on is therefore a fact about the
//! RUNNING BUILD, not about the log. It is DECLARED by the composition root --
//! which is the only place that knows which adapters it resolved -- and
//! [`restricted_to_current_sources`] is the one rule that applies it, so the
//! header's tally and the Event sources roster cannot filter differently from
//! each other.
//!
//! The rule only ever REMOVES. A source the build polls but has made no
//! observation of is still absent: it is neither available nor unavailable,
//! because every state the console shows stays derived from an observation it
//! actually made.

#[must_use]
/// Keep only the `sources` the current build actually polls, in order.
///
/// `current_sources` is the composition root's declaration of its resolved
/// adapter set, as source INSTANCE names (`fabro:hp`, not `fabro`).
///
/// An EMPTY declaration means the set is not yet KNOWN -- the brief startup
/// window before the poller's first sweep names it -- and restricts nothing.
/// Reading it as "no source is current" would hide every source on the strength
/// of a fact nobody has established yet, turning a cockpit-blind screen into a
/// blank one: this item's own defect with the sign flipped. The header's
/// `event sources: loading` tell already covers that window honestly.
pub fn restricted_to_current_sources(
    sources: &[String],
    current_sources: &[String],
) -> Vec<String> {
    if current_sources.is_empty() {
        return sources.to_vec();
    }
    sources
        .iter()
        .filter(|source| current_sources.contains(source))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::restricted_to_current_sources;

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    /// The source instances a two-factory checkout's adapter set owns, in the
    /// order `live_source_adapters_with_programs` mints them. The retired
    /// unqualified `fabro` is deliberately absent.
    fn current_sources() -> Vec<String> {
        names(&[
            "orchestrator",
            "dispatcher",
            "fabro:hp",
            "fabro:vps",
            "livespec",
            "github",
            "reconcile-runs",
        ])
    }

    /// The measured case: the retired `fabro` row drops out and each factory
    /// keeps its OWN instance name. Order is preserved, because the header
    /// names the first surviving source when it can only fit one.
    #[test]
    fn a_source_no_current_adapter_owns_is_dropped_and_the_rest_keep_their_order() {
        let observed = names(&["fabro", "fabro:hp", "fabro:vps", "livespec"]);

        assert_eq!(
            restricted_to_current_sources(&observed, &current_sources()),
            names(&["fabro:hp", "fabro:vps", "livespec"])
        );
    }

    /// A FILTER, never a fill: a source the build polls but has no observation
    /// for stays absent, so no availability state is invented for it.
    #[test]
    fn a_polled_source_with_no_observation_is_not_added() {
        let observed = names(&["fabro:vps"]);

        assert_eq!(
            restricted_to_current_sources(&observed, &current_sources()),
            names(&["fabro:vps"])
        );
    }

    /// The startup window: an unknown adapter set restricts nothing.
    #[test]
    fn an_unknown_adapter_set_restricts_nothing() {
        let observed = names(&["fabro", "fabro:vps", "reconcile-runs"]);

        assert_eq!(
            restricted_to_current_sources(&observed, &[]),
            names(&["fabro", "fabro:vps", "reconcile-runs"])
        );
    }
}
