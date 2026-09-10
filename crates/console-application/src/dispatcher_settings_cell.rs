//! The handoff between the session's background effective-policy read and the
//! render thread (livespec-console-beads-fabro-mx9u.29).
//!
//! # Why this exists
//!
//! Reading the effective dispatcher settings is one `drive --action config`
//! shell-out. Measured 2026-09-09 against real backing CLIs it cost ~6 seconds,
//! and the interactive composition root made it BEFORE painting its first
//! frame -- six seconds of blank terminal on a console CLAUDE.md has every
//! restarted plan session rebuild and relaunch. `pzbdbo.27` had already moved
//! the source ingest off that path for the same reason; this is the same move
//! for the one remaining pre-first-frame shell-out.
//!
//! # Why the delivery is ONE-SHOT, unlike its neighbours
//!
//! [`crate::build_identity::SharedBuildStaleness`] and
//! [`crate::source_staleness::SharedSourceStaleness`] hold a value the poller
//! RE-COMPUTES every cycle, so the render thread re-reads them every tick and
//! re-applying an unchanged value costs nothing. The effective policy is not
//! like that. It is read ONCE at launch, and afterwards the only thing that
//! re-reads it is the operator's own settings WRITE, on its own gated path
//! (`TuiLiveSession::refresh_dispatcher_settings`, fired when the write's
//! outcome event lands). A cell that kept re-delivering the launch value would
//! overwrite that fresher post-write read on the very next tick -- the row the
//! operator just edited would flip back to its old value. So this cell hands
//! its value over exactly once and then goes quiet.

use crate::DispatcherSettingsRead;

/// A thread-shared, take-once cell holding the effective-policy read the
/// session's background reader produced.
///
/// Empty until the reader answers; [`Self::take`] then yields the read exactly
/// once. See the module doc for why one-shot rather than the re-read-every-tick
/// shape its neighbouring cells use.
#[derive(Clone, Debug, Default)]
pub struct SharedDispatcherSettingsRead(
    std::sync::Arc<std::sync::Mutex<Option<DispatcherSettingsRead>>>,
);

impl SharedDispatcherSettingsRead {
    #[must_use]
    /// Construct an empty cell -- nothing to deliver until the background read
    /// answers.
    pub fn new() -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(None)))
    }

    /// Publish the read the background reader produced.
    ///
    /// Called once from that reader thread. A FAILED read
    /// ([`DispatcherSettingsRead::NotObserved`]) is published just like a
    /// successful one: the console must not sit on the not-yet-read tell
    /// forever when the answer is that the read surface is broken. A poisoned
    /// lock is swallowed rather than propagated, the same silence-on-failure
    /// contract [`crate::build_identity::SharedBuildStaleness::set`] keeps --
    /// the cost is one session rendering the not-yet-read tell, never a crash.
    pub fn set(&self, read: DispatcherSettingsRead) {
        if let Ok(mut guard) = self.0.lock() {
            *guard = Some(read);
        }
    }

    #[must_use]
    /// Take the pending read, if one has arrived since the last take.
    ///
    /// Called from the render thread once per tick: a non-blocking `Mutex`
    /// lock, never IO on that thread. `None` means "nothing new" -- either the
    /// background read has not answered yet, or its answer has already been
    /// folded in. A poisoned lock degrades to `None`, which leaves the state
    /// the render loop already holds untouched.
    pub fn take(&self) -> Option<DispatcherSettingsRead> {
        self.0.lock().ok().and_then(|mut guard| guard.take())
    }
}

#[cfg(test)]
mod tests {
    use super::SharedDispatcherSettingsRead;
    use crate::DispatcherSettingsRead;
    use crate::source_adapters::AcceptancePolicy;
    use crate::{DispatcherSettings, DispatcherSettingsRead::Observed};

    fn observed() -> DispatcherSettingsRead {
        Observed(DispatcherSettings::new(
            false,
            false,
            AcceptancePolicy::AiThenHuman,
            3,
            2,
            5,
        ))
    }

    #[test]
    fn a_cell_the_background_read_has_not_answered_yet_delivers_nothing() {
        // The first frame renders the seed the composition root passed it
        // (`NotYetRead`); the render loop must not be handed a value nobody
        // read.
        let cell = SharedDispatcherSettingsRead::new();
        assert_eq!(cell.take(), None);
        assert_eq!(cell.take(), None);
    }

    #[test]
    fn the_startup_read_is_delivered_exactly_once_so_it_cannot_clobber_a_later_reread() {
        // The property the whole take-once shape exists for: the render loop
        // takes this EVERY tick, while the operator's own settings write
        // re-reads on its own gated path. A cell that kept re-delivering its
        // startup value would flip the row the operator just edited back to
        // the old value on the very next tick.
        let cell = SharedDispatcherSettingsRead::new();
        cell.set(observed());
        assert_eq!(cell.take(), Some(observed()));
        assert_eq!(cell.take(), None);
    }

    #[test]
    fn a_failed_background_read_is_delivered_rather_than_swallowed() {
        // AC3. The console must not sit on the not-yet-read tell forever when
        // the answer is that the read surface is broken.
        let cell = SharedDispatcherSettingsRead::new();
        cell.set(DispatcherSettingsRead::NotObserved);
        assert_eq!(cell.take(), Some(DispatcherSettingsRead::NotObserved));
        assert_eq!(cell.take(), None);
    }

    #[test]
    fn a_later_read_is_delivered_after_an_earlier_one_was_taken() {
        // Take-once is per READ, not per cell: nothing about this handoff
        // forbids a second publication, and a cell that went permanently mute
        // after one take would silently drop it.
        let cell = SharedDispatcherSettingsRead::new();
        cell.set(DispatcherSettingsRead::NotObserved);
        assert_eq!(cell.take(), Some(DispatcherSettingsRead::NotObserved));
        cell.set(observed());
        assert_eq!(cell.take(), Some(observed()));
    }

    #[test]
    fn default_matches_new() {
        assert_eq!(SharedDispatcherSettingsRead::default().take(), None);
    }

    #[test]
    #[allow(clippy::unwrap_used, clippy::panic)]
    fn a_poisoned_lock_degrades_to_nothing_to_deliver_rather_than_a_panic() {
        // Silence on failure, never a crash: a poisoned lock costs this
        // session its background read (it keeps rendering the not-yet-read
        // tell) rather than taking the render loop down with it.
        let cell = SharedDispatcherSettingsRead::new();
        let other_handle = cell.clone();
        let join_result = std::thread::spawn(move || {
            other_handle.set(observed());
            let _guard = other_handle.0.lock().unwrap();
            panic!("deliberately poisoning the lock for the fallback test");
        })
        .join();
        assert!(join_result.is_err());

        assert_eq!(cell.take(), None);
        cell.set(observed());
        assert_eq!(cell.take(), None);
    }
}
