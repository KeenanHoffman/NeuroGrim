//! BB #2a `Overlay<T>` + BB #2b `WorkingState<W>`.
//!
//! The Overlay is the broker's read-only consumer-facing projection of its
//! working state. Per BROKER-CONTRACT.md §"The Overlay contract": atomic-swap
//! updates, versioned read, no-torn-read enforcement.
//!
//! ## Implementation (Wave 1)
//!
//! BROKER-SPEC-GAPS.md gap #5 resolution: `arc-swap` crate provides the
//! atomic-swap primitive. The broker (single writer) calls `swap()` to publish
//! a new Overlay; consumers (many readers) call `load()` to get a snapshot.
//! Consumers' loaded snapshots are reference-counted and survive subsequent
//! swaps — no torn reads possible.
//!
//! ## Working state vs Overlay (per spec)
//!
//! - **`Overlay<T>`** — read-only consumer-facing. Atomic-swap publish from
//!   broker; reference-counted snapshot read from consumer.
//! - **`WorkingState<W>`** — broker-private full read/write surface. Never
//!   exposed to consumers. Holds the broker's accumulator state (loaded
//!   catalog, workflow positions, skill-filter weight cache, rate-limit
//!   counters, etc.).
//!
//! Wave 1 implements both with arc-swap-backed Overlay + simple
//! Arc<Mutex<W>>-backed WorkingState.

use std::sync::Arc;

/// Read-only consumer-facing projection with atomic-swap updates.
/// Per BB #2a Overlay primitive.
pub struct Overlay<T> {
    inner: arc_swap::ArcSwap<T>,
}

impl<T> Overlay<T> {
    /// Create a new Overlay with the given initial state.
    pub fn new(initial: T) -> Self {
        Self {
            inner: arc_swap::ArcSwap::from_pointee(initial),
        }
    }

    /// Atomically swap the current Overlay state with `new`. Existing reader
    /// snapshots are unaffected; new readers see `new`.
    pub fn swap(&self, new: T) {
        self.inner.store(Arc::new(new));
    }

    /// Read a snapshot of the current Overlay state. The snapshot survives
    /// subsequent swaps (Arc reference-counted).
    pub fn load(&self) -> Arc<T> {
        self.inner.load_full()
    }
}

/// A read guard over an Overlay snapshot. Wave 1 may extend this for
/// version-tracking + curation-budget telemetry per spec.
pub type OverlayReadGuard<T> = Arc<T>;

/// Broker-private full read/write state. Per BB #2b WorkingState.
/// Wave 1 wraps Arc<Mutex<W>> with broker-internal access only.
pub struct WorkingState<W> {
    inner: Arc<tokio::sync::Mutex<W>>,
}

impl<W> WorkingState<W> {
    pub fn new(initial: W) -> Self {
        Self {
            inner: Arc::new(tokio::sync::Mutex::new(initial)),
        }
    }

    /// Acquire a write lock on the working state. Broker-internal only;
    /// consumers must not call this.
    pub async fn lock(&self) -> tokio::sync::MutexGuard<'_, W> {
        self.inner.lock().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::sync::Arc;

    #[test]
    fn overlay_swap_publishes_new_state() {
        let o = Overlay::new(vec![1, 2, 3]);
        let snap1 = o.load();
        o.swap(vec![4, 5, 6]);
        let snap2 = o.load();
        assert_eq!(*snap1, vec![1, 2, 3]); // snap1 unaffected
        assert_eq!(*snap2, vec![4, 5, 6]); // snap2 sees new state
    }

    proptest! {
        /// Property: under arbitrary concurrent reads + writes, every read
        /// returns ONE of the published versions (no torn reads).
        #[test]
        fn overlay_no_torn_reads_under_concurrent_access(
            values in proptest::collection::vec(0i32..1000, 1..50)
        ) {
            let o = Arc::new(Overlay::new(vec![0i32]));
            let writer_handles: Vec<_> = values
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    let o = o.clone();
                    std::thread::spawn(move || {
                        o.swap(vec![v; i + 1]);
                    })
                })
                .collect();

            let reader_handles: Vec<_> = (0..20)
                .map(|_| {
                    let o = o.clone();
                    std::thread::spawn(move || {
                        for _ in 0..50 {
                            let snap = o.load();
                            // No-torn-read invariant: every element of the
                            // snapshot equals the first element (since each
                            // write publishes a vector where all elements
                            // are identical). If snap is torn (mixed from
                            // two writes), this would fail.
                            if !snap.is_empty() {
                                let first = snap[0];
                                prop_assert!(snap.iter().all(|&x| x == first),
                                    "torn read detected: {:?}", *snap);
                            }
                        }
                        Ok(())
                    })
                })
                .collect();

            for h in writer_handles {
                h.join().unwrap();
            }
            for h in reader_handles {
                h.join().unwrap().unwrap();
            }
        }
    }

    #[tokio::test]
    async fn working_state_lock_serializes_writes() {
        let ws = Arc::new(WorkingState::new(0i32));
        let tasks: Vec<_> = (0..100)
            .map(|_| {
                let ws = ws.clone();
                tokio::spawn(async move {
                    let mut guard = ws.lock().await;
                    *guard += 1;
                })
            })
            .collect();
        for t in tasks {
            t.await.unwrap();
        }
        // If the Mutex was broken, increments would race + total < 100.
        assert_eq!(*ws.lock().await, 100);
    }
}
