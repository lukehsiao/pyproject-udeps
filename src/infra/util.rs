//! Write-channel utility for nullable infrastructure wrappers.
//!
//! Wrappers hold an [`OutputListener`] and emit domain data at the moment of
//! each write, on the code path shared by real and nulled modes. Tests call
//! the wrapper's `track_*()` method to get an [`OutputTracker`] handle.

use std::sync::{Arc, Mutex, MutexGuard, Weak};

/// Lock, recovering from poisoning. The guarded data (plain vectors) stays
/// valid even if another thread panicked while holding the lock.
fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Debug, Default)]
pub struct OutputListener<T: Clone> {
    trackers: Mutex<Vec<Weak<Mutex<Vec<T>>>>>,
}

impl<T: Clone> OutputListener<T> {
    #[must_use]
    pub fn new() -> Self {
        OutputListener {
            trackers: Mutex::new(Vec::new()),
        }
    }

    /// Record one write. A no-op unless a tracker is subscribed.
    pub fn emit(&self, data: T) {
        let mut trackers = lock_unpoisoned(&self.trackers);
        trackers.retain(|tracker| {
            tracker.upgrade().is_some_and(|cell| {
                lock_unpoisoned(&cell).push(data.clone());
                true
            })
        });
    }

    #[must_use]
    pub fn track(&self) -> OutputTracker<T> {
        let data = Arc::new(Mutex::new(Vec::new()));
        lock_unpoisoned(&self.trackers).push(Arc::downgrade(&data));
        OutputTracker { data }
    }
}

#[derive(Debug, Clone)]
pub struct OutputTracker<T> {
    data: Arc<Mutex<Vec<T>>>,
}

impl<T: Clone> OutputTracker<T> {
    #[must_use]
    pub fn data(&self) -> Vec<T> {
        lock_unpoisoned(&self.data).clone()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn tracker_records_emitted_data() {
        let listener = OutputListener::new();
        let tracker = listener.track();
        listener.emit("one");
        listener.emit("two");
        assert_eq!(tracker.data(), vec!["one", "two"]);
    }

    #[test]
    fn emit_without_tracker_is_a_noop() {
        let listener = OutputListener::new();
        listener.emit("unobserved");
        let tracker = listener.track();
        assert_eq!(tracker.data(), Vec::<&str>::new());
    }

    #[test]
    fn dropped_trackers_stop_recording() {
        let listener = OutputListener::new();
        let tracker = listener.track();
        drop(tracker);
        listener.emit("after drop");
        let fresh = listener.track();
        assert_eq!(fresh.data(), Vec::<&str>::new());
    }
}
