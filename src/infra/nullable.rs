//! The write channel shared by the nullable infrastructure wrappers. A
//! wrapper emits each write it makes through an [`OutputListener`], so a
//! test can track what went out.
//!
//! Only tests can track output, so in other builds an [`OutputListener`]
//! holds nothing and emitting is free.

#[cfg(test)]
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::{fmt, marker::PhantomData};

/// The writes one tracker has seen. The tracker owns it, and the listener
/// holds only a weak reference, so a dropped tracker stops collecting.
#[cfg(test)]
type Seen<T> = Mutex<Vec<T>>;

/// The write channel of a wrapper. The wrapper calls [`emit`] at each write,
/// in the code path that real and nulled instances share. Clones share
/// their trackers.
///
/// [`emit`]: OutputListener::emit
#[derive(Clone)]
pub(crate) struct OutputListener<T> {
    #[cfg(test)]
    trackers: Arc<Mutex<Vec<Weak<Seen<T>>>>>,
    _data: PhantomData<fn() -> T>,
}

impl<T> Default for OutputListener<T> {
    fn default() -> Self {
        Self {
            #[cfg(test)]
            trackers: Arc::default(),
            _data: PhantomData,
        }
    }
}

impl<T> fmt::Debug for OutputListener<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OutputListener").finish_non_exhaustive()
    }
}

impl<T: Clone> OutputListener<T> {
    /// Record one write. `data` runs only when a live tracker listens, so a
    /// write nobody tracks builds nothing.
    pub(crate) fn emit(&self, data: impl FnOnce() -> T) {
        #[cfg(test)]
        {
            let mut trackers = self.trackers.lock().unwrap_or_else(PoisonError::into_inner);
            let live: Vec<Arc<Seen<T>>> = trackers.iter().filter_map(Weak::upgrade).collect();
            // A dropped tracker can never read again, so forget it.
            trackers.retain(|tracker| tracker.strong_count() > 0);
            drop(trackers);
            if live.is_empty() {
                return;
            }
            let data = data();
            for tracker in live {
                tracker
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(data.clone());
            }
        }
        #[cfg(not(test))]
        let _ = (self, data);
    }

    /// Start recording writes. The tracker sees only writes made after this
    /// call, and stops recording when it is dropped.
    #[cfg(test)]
    pub(crate) fn track(&self) -> OutputTracker<T> {
        let seen = Arc::new(Mutex::new(Vec::new()));
        self.trackers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Arc::downgrade(&seen));
        OutputTracker { seen }
    }
}

/// The writes a wrapper made since a test started tracking it, in order.
#[cfg(test)]
pub(crate) struct OutputTracker<T> {
    seen: Arc<Seen<T>>,
}

#[cfg(test)]
impl<T: Clone> OutputTracker<T> {
    pub(crate) fn data(&self) -> Vec<T> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[cfg(test)]
mod test {
    use super::OutputListener;

    #[test]
    fn a_tracker_sees_writes_made_after_it_starts_from_any_clone() {
        let listener = OutputListener::default();
        listener.emit(|| "before");
        let tracker = listener.track();
        listener.emit(|| "first");
        listener.clone().emit(|| "second");
        assert_eq!(tracker.data(), ["first", "second"]);
    }

    #[test]
    fn emitting_without_a_tracker_builds_nothing() {
        let listener = OutputListener::<&str>::default();
        listener.emit(|| panic!("built data nobody tracks"));
    }

    #[test]
    fn a_dropped_tracker_stops_collecting() {
        let listener = OutputListener::<&str>::default();
        drop(listener.track());
        listener.emit(|| panic!("built data for a dropped tracker"));
    }
}
